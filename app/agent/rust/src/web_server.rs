use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const EMBEDDED_INDEX_HTML: &[u8] = include_bytes!("../../../front/dist/index.html");
const EMBEDDED_FAVICON_SVG: &[u8] = include_bytes!("../../../front/dist/favicon.svg");
const EMBEDDED_ICONS_SVG: &[u8] = include_bytes!("../../../front/dist/icons.svg");
const EMBEDDED_JS: &[u8] = include_bytes!("../../../front/dist/assets/index-D-p3Ryuc.js");
const EMBEDDED_CSS: &[u8] = include_bytes!("../../../front/dist/assets/index-LHwY0Eqm.css");

fn get_content_type(path: &str) -> &'static str {
    if path.ends_with(".html") || path == "/" {
        "text/html; charset=utf-8"
    } else if path.ends_with(".js") {
        "application/javascript; charset=utf-8"
    } else if path.ends_with(".css") {
        "text/css; charset=utf-8"
    } else if path.ends_with(".svg") {
        "image/svg+xml"
    } else if path.ends_with(".png") {
        "image/png"
    } else if path.ends_with(".json") {
        "application/json"
    } else {
        "application/octet-stream"
    }
}

fn try_read_file_from_disk(rel_path: &str) -> Option<Vec<u8>> {
    let clean_path = rel_path.trim_start_matches('/');
    let candidates = [
        PathBuf::from("app/front/dist").join(clean_path),
        PathBuf::from("../front/dist").join(clean_path),
        PathBuf::from("../../front/dist").join(clean_path),
        PathBuf::from("dist").join(clean_path),
    ];

    for path in &candidates {
        if path.is_file() {
            if let Ok(data) = std::fs::read(path) {
                return Some(data);
            }
        }
    }
    None
}

fn resolve_asset(req_path: &str) -> (&'static str, Vec<u8>) {
    let raw_path = req_path.split('?').next().unwrap_or("/");
    let path = if raw_path.is_empty() || raw_path == "/" {
        "/index.html"
    } else {
        raw_path
    };

    // 1. ディスクからの最新読み込みを試行
    if let Some(disk_data) = try_read_file_from_disk(path) {
        return (get_content_type(path), disk_data);
    }

    // 2. 内蔵アセットからの読み込み（単一バイナリ配布用フォールバック）
    match path {
        "/favicon.svg" => ("image/svg+xml", EMBEDDED_FAVICON_SVG.to_vec()),
        "/icons.svg" => ("image/svg+xml", EMBEDDED_ICONS_SVG.to_vec()),
        p if p.ends_with(".js") => ("application/javascript; charset=utf-8", EMBEDDED_JS.to_vec()),
        p if p.ends_with(".css") => ("text/css; charset=utf-8", EMBEDDED_CSS.to_vec()),
        _ => ("text/html; charset=utf-8", EMBEDDED_INDEX_HTML.to_vec()),
    }
}

use std::sync::Arc;
use tokio::sync::RwLock;
use crate::config::AppConfig;
use crate::job_runner::JobRunner;

/// 専用のSpareLink WebダッシュボードHTTP配信サーバー (ポート 5180)
pub async fn run_dashboard_server(
    port: u16,
    shared_config: Arc<RwLock<AppConfig>>,
    job_runner: Arc<JobRunner>,
) {
    let addr = format!("0.0.0.0:{}", port);
    let listener = match TcpListener::bind(&addr).await {
        Ok(l) => {
            println!(
                "[SpareLink] Dedicated Web Dashboard server active on http://localhost:{}",
                port
            );
            l
        }
        Err(e) => {
            println!(
                "[SpareLink] Dashboard port {} is already in use ({}). Delegating web traffic to running server.",
                port, e
            );
            return;
        }
    };

    while let Ok((mut stream, _)) = listener.accept().await {
        let config_clone = shared_config.clone();
        let job_runner_clone = job_runner.clone();
        tokio::spawn(async move {
            let mut buf = [0u8; 8192];
            let n = match stream.read(&mut buf).await {
                Ok(n) if n > 0 => n,
                _ => return,
            };

            let req_str = String::from_utf8_lossy(&buf[..n]);
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
                    let cfg = config_clone.read().await;
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
                            // メモリ上の設定オブジェクトを即時更新
                            {
                                let mut lock = config_clone.write().await;
                                *lock = new_cfg.clone();
                            }
                            // sparelink.toml へ永続化
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
                    let jobs = job_runner_clone.get_all_jobs().await;
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
                            match job_runner_clone.submit_job(job).await {
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

            // 3. 静的フロントエンド配信
            let (content_type, body) = resolve_asset(path);

            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n",
                content_type,
                body.len()
            );

            let _ = stream.write_all(header.as_bytes()).await;
            let _ = stream.write_all(&body).await;
            let _ = stream.flush().await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedded_assets_present() {
        assert!(!EMBEDDED_INDEX_HTML.is_empty());
        assert!(!EMBEDDED_JS.is_empty());
        assert!(!EMBEDDED_CSS.is_empty());
    }

    #[test]
    fn test_resolve_root_returns_html() {
        let (ct, data) = resolve_asset("/");
        assert_eq!(ct, "text/html; charset=utf-8");
        assert!(!data.is_empty());
    }
}
