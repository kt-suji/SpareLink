use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const EMBEDDED_INDEX_HTML: &[u8] = include_bytes!("../../../front/dist/index.html");
const EMBEDDED_FAVICON_SVG: &[u8] = include_bytes!("../../../front/dist/favicon.svg");
const EMBEDDED_ICONS_SVG: &[u8] = include_bytes!("../../../front/dist/icons.svg");
const EMBEDDED_JS: &[u8] = include_bytes!("../../../front/dist/assets/index-BcaUYHQG.js");
const EMBEDDED_CSS: &[u8] = include_bytes!("../../../front/dist/assets/index-D1mEjTrr.css");

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

/// 専用のSpareLink WebダッシュボードHTTP配信サーバー (ポート 5180)
pub async fn run_dashboard_server(port: u16) {
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
        tokio::spawn(async move {
            let mut buf = [0u8; 2048];
            let n = match stream.read(&mut buf).await {
                Ok(n) if n > 0 => n,
                _ => return,
            };

            let req_str = String::from_utf8_lossy(&buf[..n]);
            let first_line = req_str.lines().next().unwrap_or("");
            let mut parts = first_line.split_whitespace();
            let _method = parts.next().unwrap_or("GET");
            let path = parts.next().unwrap_or("/");

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
