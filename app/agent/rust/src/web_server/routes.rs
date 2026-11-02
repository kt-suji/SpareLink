use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::sync::RwLock;

use crate::config::AppConfig;
use crate::job_runner::JobRunner;
use crate::p2p::PeerManager;
use super::assets::resolve_asset;

pub async fn handle_connection(
    mut stream: TcpStream,
    buf: &[u8],
    shared_config: Arc<RwLock<AppConfig>>,
    job_runner: Arc<JobRunner>,
    peer_manager: Arc<PeerManager>,
) {
    let req_str = String::from_utf8_lossy(buf);
    let first_line = req_str.lines().next().unwrap_or("");
    let mut parts = first_line.split_whitespace();
    let method = parts.next().unwrap_or("GET");
    let raw_path = parts.next().unwrap_or("/");
    let path = raw_path.split('?').next().unwrap_or("/");

    // 1. REST API: 設定の取得・更新 (/api/config)
    if path == "/api/config" {
        if method == "OPTIONS" {
            let resp = "HTTP/1.1 204 No Content\r\n\
                        Access-Control-Allow-Origin: *\r\n\
                        Access-Control-Allow-Methods: GET, POST, PUT, OPTIONS\r\n\
                        Access-Control-Allow-Headers: Content-Type\r\n\
                        Connection: close\r\n\r\n";
            let _ = stream.write_all(resp.as_bytes()).await;
            let _ = stream.flush().await;
            return;
        } else if method == "GET" {
            let cfg = shared_config.read().await;
            let json = serde_json::to_string_pretty(&*cfg).unwrap_or_else(|_| "{}".to_string());
            let header = format!(
                "HTTP/1.1 200 OK\r\n\
                 Content-Type: application/json; charset=utf-8\r\n\
                 Content-Length: {}\r\n\
                 Access-Control-Allow-Origin: *\r\n\
                 Cache-Control: no-cache\r\n\
                 Connection: close\r\n\r\n",
                json.len()
            );
            let _ = stream.write_all(header.as_bytes()).await;
            let _ = stream.write_all(json.as_bytes()).await;
            let _ = stream.flush().await;
            return;
        } else if method == "POST" || method == "PUT" {
            let body_str = if let Some(idx) = req_str.find("\r\n\r\n") {
                &req_str[idx + 4..]
            } else if let Some(idx) = req_str.find("\n\n") {
                &req_str[idx + 2..]
            } else {
                ""
            };

            match serde_json::from_str::<AppConfig>(body_str.trim()) {
                Ok(new_cfg) => {
                    {
                        let mut lock = shared_config.write().await;
                        *lock = new_cfg.clone();
                    }
                    let save_res = new_cfg.save_to_file();
                    let msg = match save_res {
                        Ok(p) => format!("設定を保存し、{} へ反映しました", p.display()),
                        Err(e) => format!("設定を適用しましたが、ファイル保存に失敗しました: {}", e),
                    };

                    let resp_json = serde_json::json!({
                        "status": "ok",
                        "message": msg,
                        "config": new_cfg
                    }).to_string();

                    let header = format!(
                        "HTTP/1.1 200 OK\r\n\
                         Content-Type: application/json; charset=utf-8\r\n\
                         Content-Length: {}\r\n\
                         Access-Control-Allow-Origin: *\r\n\
                         Connection: close\r\n\r\n",
                        resp_json.len()
                    );
                    let _ = stream.write_all(header.as_bytes()).await;
                    let _ = stream.write_all(resp_json.as_bytes()).await;
                    let _ = stream.flush().await;
                    return;
                }
                Err(e) => {
                    let err_json = serde_json::json!({
                        "status": "error",
                        "message": format!("設定JSONの解析に失敗しました: {}", e)
                    }).to_string();
                    let header = format!(
                        "HTTP/1.1 400 Bad Request\r\n\
                         Content-Type: application/json; charset=utf-8\r\n\
                         Content-Length: {}\r\n\
                         Access-Control-Allow-Origin: *\r\n\
                         Connection: close\r\n\r\n",
                        err_json.len()
                    );
                    let _ = stream.write_all(header.as_bytes()).await;
                    let _ = stream.write_all(err_json.as_bytes()).await;
                    let _ = stream.flush().await;
                    return;
                }
            }
        }
    }

    // 2. REST API: 分散ジョブ一覧取得・投入 (/api/jobs, /api/jobs/submit)
    if path == "/api/jobs" || path == "/api/jobs/submit" {
        if method == "OPTIONS" {
            let resp = "HTTP/1.1 204 No Content\r\n\
                        Access-Control-Allow-Origin: *\r\n\
                        Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
                        Access-Control-Allow-Headers: Content-Type\r\n\
                        Connection: close\r\n\r\n";
            let _ = stream.write_all(resp.as_bytes()).await;
            let _ = stream.flush().await;
            return;
        } else if method == "GET" && path == "/api/jobs" {
            let jobs = job_runner.get_all_jobs().await;
            let json = serde_json::to_string(&jobs).unwrap_or_else(|_| "[]".to_string());
            let header = format!(
                "HTTP/1.1 200 OK\r\n\
                 Content-Type: application/json; charset=utf-8\r\n\
                 Content-Length: {}\r\n\
                 Access-Control-Allow-Origin: *\r\n\
                 Cache-Control: no-cache\r\n\
                 Connection: close\r\n\r\n",
                json.len()
            );
            let _ = stream.write_all(header.as_bytes()).await;
            let _ = stream.write_all(json.as_bytes()).await;
            let _ = stream.flush().await;
            return;
        } else if (method == "POST" || method == "PUT") && (path == "/api/jobs" || path == "/api/jobs/submit") {
            let body_str = if let Some(idx) = req_str.find("\r\n\r\n") {
                &req_str[idx + 4..]
            } else if let Some(idx) = req_str.find("\n\n") {
                &req_str[idx + 2..]
            } else {
                ""
            };

            match serde_json::from_str::<crate::models::ComputeJob>(body_str.trim()) {
                Ok(job) => {
                    match job_runner.submit_job(job).await {
                        Ok(submitted) => {
                            let resp_json = serde_json::to_string(&submitted).unwrap_or_default();
                            let header = format!(
                                "HTTP/1.1 200 OK\r\n\
                                 Content-Type: application/json; charset=utf-8\r\n\
                                 Content-Length: {}\r\n\
                                 Access-Control-Allow-Origin: *\r\n\
                                 Connection: close\r\n\r\n",
                                resp_json.len()
                            );
                            let _ = stream.write_all(header.as_bytes()).await;
                            let _ = stream.write_all(resp_json.as_bytes()).await;
                            let _ = stream.flush().await;
                            return;
                        }
                        Err(e) => {
                            let err_json = serde_json::json!({
                                "status": "error",
                                "message": format!("ジョブ投入エラー: {}", e)
                            }).to_string();
                            let header = format!(
                                "HTTP/1.1 500 Internal Server Error\r\n\
                                 Content-Type: application/json; charset=utf-8\r\n\
                                 Content-Length: {}\r\n\
                                 Access-Control-Allow-Origin: *\r\n\
                                 Connection: close\r\n\r\n",
                                err_json.len()
                            );
                            let _ = stream.write_all(header.as_bytes()).await;
                            let _ = stream.write_all(err_json.as_bytes()).await;
                            let _ = stream.flush().await;
                            return;
                        }
                    }
                }
                Err(e) => {
                    let err_json = serde_json::json!({
                        "status": "error",
                        "message": format!("不正なジョブJSONフォーマット: {}", e)
                    }).to_string();
                    let header = format!(
                        "HTTP/1.1 400 Bad Request\r\n\
                         Content-Type: application/json; charset=utf-8\r\n\
                         Content-Length: {}\r\n\
                         Access-Control-Allow-Origin: *\r\n\
                         Connection: close\r\n\r\n",
                        err_json.len()
                    );
                    let _ = stream.write_all(header.as_bytes()).await;
                    let _ = stream.write_all(err_json.as_bytes()).await;
                    let _ = stream.flush().await;
                    return;
                }
            }
        }
    }

    // 3. REST API: アップデート関連
    if method == "OPTIONS" && (path == "/api/binary" || path == "/api/cluster/update" || path == "/api/self/update") {
        let resp = "HTTP/1.1 204 No Content\r\n\
                    Access-Control-Allow-Origin: *\r\n\
                    Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
                    Access-Control-Allow-Headers: Content-Type\r\n\
                    Connection: close\r\n\r\n";
        let _ = stream.write_all(resp.as_bytes()).await;
        let _ = stream.flush().await;
        return;
    }

    if method == "GET" && (path == "/api/binary" || path == "/dist/sparelink-agent.exe") {
        let binary_paths = [
            std::env::current_exe().unwrap_or_default(),
            PathBuf::from("dist-agent/sparelink-agent.exe"),
            PathBuf::from("G:\\マイドライブ\\SpareLink\\dist-agent\\sparelink-agent.exe"),
            PathBuf::from("G:\\My Drive\\SpareLink\\dist-agent\\sparelink-agent.exe"),
        ];

        let mut found_bytes = None;
        for bp in &binary_paths {
            if bp.exists() {
                if let Ok(bytes) = std::fs::read(bp) {
                    if bytes.len() > 1_000_000 {
                        found_bytes = Some(bytes);
                        break;
                    }
                }
            }
        }

        if let Some(bytes) = found_bytes {
            let header = format!(
                "HTTP/1.1 200 OK\r\n\
                 Content-Type: application/octet-stream\r\n\
                 Content-Disposition: attachment; filename=\"sparelink-agent.exe\"\r\n\
                 Content-Length: {}\r\n\
                 Access-Control-Allow-Origin: *\r\n\
                 Connection: close\r\n\r\n",
                bytes.len()
            );
            let _ = stream.write_all(header.as_bytes()).await;
            let _ = stream.write_all(&bytes).await;
            let _ = stream.flush().await;
            return;
        } else {
            let err_resp = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
            let _ = stream.write_all(err_resp.as_bytes()).await;
            let _ = stream.flush().await;
            return;
        }
    } else if (method == "POST" || method == "PUT") && path == "/api/cluster/update" {
        let current_version = env!("CARGO_PKG_VERSION");
        let updated_nodes = crate::updater::trigger_cluster_update(
            &peer_manager,
            current_version,
            None,
        ).await;

        let resp_json = serde_json::json!({
            "status": "success",
            "message": format!("{} 台のノードへ最新版 (v{}) の自動更新シグナルを送信しました", updated_nodes.len(), current_version),
            "updatedNodes": updated_nodes,
        }).to_string();

        let header = format!(
            "HTTP/1.1 200 OK\r\n\
             Content-Type: application/json; charset=utf-8\r\n\
             Content-Length: {}\r\n\
             Access-Control-Allow-Origin: *\r\n\
             Connection: close\r\n\r\n",
            resp_json.len()
        );
        let _ = stream.write_all(header.as_bytes()).await;
        let _ = stream.write_all(resp_json.as_bytes()).await;
        let _ = stream.flush().await;
        return;
    } else if (method == "POST" || method == "PUT") && path == "/api/self/update" {
        tokio::spawn(async {
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            let _ = crate::updater::perform_self_update(None).await;
        });

        let resp_json = serde_json::json!({
            "status": "success",
            "message": "自ノードの再起動・アップデートを開始しました",
        }).to_string();

        let header = format!(
            "HTTP/1.1 200 OK\r\n\
             Content-Type: application/json; charset=utf-8\r\n\
             Content-Length: {}\r\n\
             Access-Control-Allow-Origin: *\r\n\
             Connection: close\r\n\r\n",
            resp_json.len()
        );
        let _ = stream.write_all(header.as_bytes()).await;
        let _ = stream.write_all(resp_json.as_bytes()).await;
        let _ = stream.flush().await;
        return;
    }

    // 4. 静的フロントエンド配信
    let (content_type, body) = resolve_asset(path);

    let header = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n",
        content_type,
        body.len()
    );

    let _ = stream.write_all(header.as_bytes()).await;
    let _ = stream.write_all(&body).await;
    let _ = stream.flush().await;
}
