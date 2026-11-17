use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::sync::RwLock;

use super::assets::resolve_asset;
use super::file_store::HttpRequestHead;
use crate::config::AppConfig;
use crate::job_runner::JobRunner;
use crate::p2p::PeerManager;

pub async fn handle_connection(
    mut stream: TcpStream,
    head: &HttpRequestHead,
    body: &[u8],
    port: u16,
    shared_config: Arc<RwLock<AppConfig>>,
    job_runner: Arc<JobRunner>,
    peer_manager: Arc<PeerManager>,
) {
    let method = head.method.as_str();
    let path = head.path.as_str();

    if method == "OPTIONS" {
        match path {
            "/api/config" => {
                super::write_cors_preflight(&mut stream, "GET, POST, PUT, OPTIONS").await;
                return;
            }
            "/api/jobs" | "/api/jobs/submit" | "/api/binary" | "/api/cluster/update"
            | "/api/self/update" => {
                super::write_cors_preflight(&mut stream, "GET, POST, OPTIONS").await;
                return;
            }
            _ => {}
        }
    }

    // 0. REST API: 自ノード情報 (/api/self) — 拡張機能から Tailscale IP やポートを取得
    if path == "/api/self" && method == "GET" {
        let self_ip = super::resolve_self_ip(&peer_manager).await;
        let ws_port = shared_config.read().await.agent.port;
        let resp_json = serde_json::json!({
            "self_ip": self_ip,
            "http_port": port,
            "ws_port": ws_port
        })
        .to_string();
        super::write_json_response(&mut stream, "200 OK", resp_json.as_bytes(), true).await;
        return;
    }

    // 1. REST API: 設定の取得・更新 (/api/config)
    if path == "/api/config" {
        if method == "GET" {
            let cfg = shared_config.read().await;
            let json = serde_json::to_string_pretty(&*cfg).unwrap_or_else(|_| "{}".to_string());
            super::write_json_response(&mut stream, "200 OK", json.as_bytes(), true).await;
            return;
        } else if method == "POST" || method == "PUT" {
            let body_str = String::from_utf8_lossy(body);
            match serde_json::from_str::<AppConfig>(body_str.trim()) {
                Ok(new_cfg) => {
                    *shared_config.write().await = new_cfg.clone();
                    let msg = match new_cfg.save_to_file() {
                        Ok(p) => format!("設定を保存し、{} へ反映しました", p.display()),
                        Err(e) => format!("設定を適用しましたが、ファイル保存に失敗しました: {}", e),
                    };
                    let resp_json = serde_json::json!({
                        "status": "ok",
                        "message": msg,
                        "config": new_cfg
                    })
                    .to_string();
                    super::write_json_response(&mut stream, "200 OK", resp_json.as_bytes(), false).await;
                }
                Err(e) => {
                    let err_json = serde_json::json!({
                        "status": "error",
                        "message": format!("設定JSONの解析に失敗しました: {}", e)
                    })
                    .to_string();
                    super::write_json_response(&mut stream, "400 Bad Request", err_json.as_bytes(), false)
                        .await;
                }
            }
            return;
        }
    }

    // 2. REST API: 分散ジョブ一覧取得・投入 (/api/jobs, /api/jobs/submit)
    if path == "/api/jobs" && method == "GET" {
        let jobs = job_runner.get_all_jobs().await;
        let json = serde_json::to_string(&jobs).unwrap_or_else(|_| "[]".to_string());
        super::write_json_response(&mut stream, "200 OK", json.as_bytes(), true).await;
        return;
    }
    if (path == "/api/jobs" || path == "/api/jobs/submit") && (method == "POST" || method == "PUT") {
        let body_str = String::from_utf8_lossy(body);
        match serde_json::from_str::<crate::models::ComputeJob>(body_str.trim()) {
            Ok(job) => match job_runner.submit_job(job).await {
                Ok(submitted) => {
                    let resp_json = serde_json::to_string(&submitted).unwrap_or_default();
                    super::write_json_response(&mut stream, "200 OK", resp_json.as_bytes(), false).await;
                }
                Err(e) => {
                    let err_json = serde_json::json!({
                        "status": "error",
                        "message": format!("ジョブ投入エラー: {}", e)
                    })
                    .to_string();
                    super::write_json_response(
                        &mut stream,
                        "500 Internal Server Error",
                        err_json.as_bytes(),
                        false,
                    )
                    .await;
                }
            },
            Err(e) => {
                let err_json = serde_json::json!({
                    "status": "error",
                    "message": format!("不正なジョブJSONフォーマット: {}", e)
                })
                .to_string();
                super::write_json_response(&mut stream, "400 Bad Request", err_json.as_bytes(), false).await;
            }
        }
        return;
    }

    // 3. REST API: アップデート関連
    if method == "GET" && (path == "/api/binary" || path == "/dist/sparelink-agent.exe") {
        let found_bytes = crate::updater::dist_binary_candidates(true)
            .into_iter()
            .find_map(|bp| {
                bp.exists()
                    .then(|| std::fs::read(&bp).ok())
                    .flatten()
                    .filter(|bytes| bytes.len() > 1_000_000)
            });

        if let Some(bytes) = found_bytes {
            super::write_http_response(
                &mut stream,
                "200 OK",
                "application/octet-stream",
                &bytes,
                "Content-Disposition: attachment; filename=\"sparelink-agent.exe\"\r\n",
            )
            .await;
        } else {
            let err_resp = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
            let _ = stream.write_all(err_resp.as_bytes()).await;
            let _ = stream.flush().await;
        }
        return;
    }

    if (method == "POST" || method == "PUT") && path == "/api/cluster/update" {
        let current_version = env!("CARGO_PKG_VERSION");
        let updated_nodes =
            crate::updater::trigger_cluster_update(&peer_manager, current_version, None).await;
        let resp_json = serde_json::json!({
            "status": "success",
            "message": format!("{} 台のノードへ最新版 (v{}) の自動更新シグナルを送信しました", updated_nodes.len(), current_version),
            "updatedNodes": updated_nodes,
        })
        .to_string();
        super::write_json_response(&mut stream, "200 OK", resp_json.as_bytes(), false).await;
        return;
    }

    if (method == "POST" || method == "PUT") && path == "/api/self/update" {
        tokio::spawn(async {
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            let _ = crate::updater::perform_self_update(None).await;
        });
        let resp_json = serde_json::json!({
            "status": "success",
            "message": "自ノードの再起動・アップデートを開始しました",
        })
        .to_string();
        super::write_json_response(&mut stream, "200 OK", resp_json.as_bytes(), false).await;
        return;
    }

    // 4. 未知の /api/* は SPA へフォールバックせず 404 JSON を返す
    if path.starts_with("/api/") {
        let err_json = br#"{"status":"error","message":"Unknown API endpoint"}"#;
        super::write_json_response(&mut stream, "404 Not Found", err_json, false).await;
        return;
    }

    // 5. 静的フロントエンド配信
    let (content_type, body) = resolve_asset(path);
    super::write_http_response(&mut stream, "200 OK", content_type, &body, "").await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::{DiscoveryConfig, TransportKind, create_discovery_provider};
    use tokio::io::AsyncReadExt;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn test_api_self_and_unknown_404_over_tcp() {
        let cfg = Arc::new(RwLock::new(AppConfig::default()));
        let telemetry = Arc::new(RwLock::new(None));
        let provider = create_discovery_provider(&DiscoveryConfig {
            transport: TransportKind::Static,
            ..Default::default()
        });
        let pm = Arc::new(PeerManager::new("test-node".to_string(), provider, telemetry.clone()));
        let jr = Arc::new(JobRunner::new(
            "test-node".to_string(),
            "test-node".to_string(),
            pm.clone(),
            cfg.clone(),
            telemetry,
        ));

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let server = tokio::spawn(async move {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let (full_req, head) = super::super::read_http_head(&mut stream).await.unwrap();
                handle_connection(
                    stream,
                    &head,
                    &full_req[head.body_offset..],
                    port,
                    cfg.clone(),
                    jr.clone(),
                    pm.clone(),
                )
                .await;
            }
        });

        let mut client1 = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        client1
            .write_all(b"GET /api/self HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
            .await
            .unwrap();
        let mut resp1 = String::new();
        client1.read_to_string(&mut resp1).await.unwrap();
        assert!(resp1.contains("200 OK"), "unexpected /api/self response: {resp1}");
        assert!(resp1.contains(&format!("\"http_port\":{port}")));

        let mut client2 = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        client2
            .write_all(b"GET /api/unknown_endpoint HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
            .await
            .unwrap();
        let mut resp2 = String::new();
        client2.read_to_string(&mut resp2).await.unwrap();
        assert!(resp2.contains("404 Not Found"), "unexpected 404 response: {resp2}");
        assert!(resp2.contains("Unknown API endpoint"));

        server.await.unwrap();
    }
}
