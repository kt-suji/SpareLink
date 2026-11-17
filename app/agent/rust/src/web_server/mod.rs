pub mod assets;
pub mod file_store;
pub mod routes;

use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;

use crate::config::AppConfig;
use crate::job_runner::JobRunner;
use crate::p2p::PeerManager;
use file_store::HttpRequestHead;

const MAX_HEADER_BYTES: usize = 16 * 1024;
const MAX_JSON_BODY_BYTES: usize = 4 * 1024 * 1024;

/// 他のノードから到達可能な自ノードの IP (Tailscale IP 優先、なければ LAN IP)
pub async fn resolve_self_ip(peer_manager: &PeerManager) -> String {
    if let Some(ts) = peer_manager.get_tailscale_status().await
        && let Some(ip) = ts.self_ip
        && !ip.is_empty()
    {
        return ip;
    }
    if let Ok(sock) = std::net::UdpSocket::bind("0.0.0.0:0")
        && (sock.connect("100.100.100.100:80").is_ok() || sock.connect("8.8.8.8:80").is_ok())
        && let Ok(addr) = sock.local_addr()
    {
        return addr.ip().to_string();
    }
    "127.0.0.1".to_string()
}

pub(super) async fn write_http_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
    extra_headers: &str,
) {
    let header = format!(
        "HTTP/1.1 {status}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {}\r\n\
         Access-Control-Allow-Origin: *\r\n\
         {extra_headers}\
         Connection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(header.as_bytes()).await;
    if !body.is_empty() {
        let _ = stream.write_all(body).await;
    }
    let _ = stream.flush().await;
}

pub(super) async fn write_json_response(
    stream: &mut TcpStream,
    status: &str,
    body: &[u8],
    no_cache: bool,
) {
    let extra = if no_cache { "Cache-Control: no-cache\r\n" } else { "" };
    write_http_response(stream, status, "application/json; charset=utf-8", body, extra).await;
}

pub(super) async fn write_cors_preflight(stream: &mut TcpStream, methods: &str) {
    let resp = format!(
        "HTTP/1.1 204 No Content\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Access-Control-Allow-Methods: {methods}\r\n\
         Access-Control-Allow-Headers: Content-Type\r\n\
         Connection: close\r\n\r\n"
    );
    let _ = stream.write_all(resp.as_bytes()).await;
    let _ = stream.flush().await;
}

fn find_header_end(buf: &[u8]) -> Option<(usize, usize)> {
    if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
        return Some((pos, 4));
    }
    if let Some(pos) = buf.windows(2).position(|w| w == b"\n\n") {
        return Some((pos, 2));
    }
    None
}

async fn read_http_head(stream: &mut TcpStream) -> Option<(Vec<u8>, HttpRequestHead)> {
    let mut raw = Vec::with_capacity(4096);
    let mut chunk = [0u8; 4096];
    let (hdr_end, sep_len) = loop {
        let n = stream.read(&mut chunk).await.ok()?;
        if n == 0 {
            return None;
        }
        raw.extend_from_slice(&chunk[..n]);
        if let Some(found) = find_header_end(&raw) {
            break found;
        }
        if raw.len() >= MAX_HEADER_BYTES {
            return None;
        }
    };

    let head_str = String::from_utf8_lossy(&raw[..hdr_end]);
    let first_line = head_str.lines().next().unwrap_or("");
    let mut parts = first_line.split_whitespace();
    let method = parts.next().unwrap_or("GET").to_uppercase();
    let raw_path = parts.next().unwrap_or("/");
    let path = raw_path.split('?').next().unwrap_or("/").to_string();

    let mut content_length: Option<u64> = None;
    let mut expect_continue = false;
    for line in head_str.lines().skip(1) {
        if let Some((k, v)) = line.split_once(':') {
            let key = k.trim().to_ascii_lowercase();
            let val = v.trim();
            if key == "content-length" {
                content_length = val.parse::<u64>().ok();
            } else if key == "expect" && val.eq_ignore_ascii_case("100-continue") {
                expect_continue = true;
            }
        }
    }

    let body_offset = hdr_end + sep_len;
    let remainder = raw[body_offset..].to_vec();
    let head = HttpRequestHead {
        method,
        path,
        content_length,
        expect_continue,
        body_offset,
        remainder,
    };
    Some((raw, head))
}

/// 専用のSpareLink Webダッシュボード＆REST APIサーバー (ポート 5180)
pub async fn run_dashboard_server(
    port: u16,
    shared_config: Arc<RwLock<AppConfig>>,
    job_runner: Arc<JobRunner>,
    peer_manager: Arc<PeerManager>,
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
        let peer_manager_clone = peer_manager.clone();
        tokio::spawn(async move {
            let Some((mut full_req, head)) = read_http_head(&mut stream).await else {
                return;
            };

            // 大容量ファイルステージングはメモリに溜めず直接ディスクへストリーミング
            if head.path.starts_with("/api/files/") {
                let self_ip = resolve_self_ip(&peer_manager_clone).await;
                file_store::handle_file_request(stream, head, &self_ip, port).await;
                return;
            }

            // JSON 等の通常リクエストが複数パケットに分かれた場合は Content-Length 分を読み切る
            if let Some(cl) = head.content_length {
                let target_body = (cl as usize).min(MAX_JSON_BODY_BYTES);
                let mut have = head.remainder.len();
                let mut chunk = [0u8; 4096];
                while have < target_body {
                    let to_read = (target_body - have).min(chunk.len());
                    match stream.read(&mut chunk[..to_read]).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            full_req.extend_from_slice(&chunk[..n]);
                            have += n;
                        }
                    }
                }
            }

            routes::handle_connection(
                stream,
                &head,
                &full_req[head.body_offset..],
                port,
                config_clone,
                job_runner_clone,
                peer_manager_clone,
            )
            .await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::assets::*;

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
