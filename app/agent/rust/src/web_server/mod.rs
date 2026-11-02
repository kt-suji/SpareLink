pub mod assets;
pub mod routes;

use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;
use tokio::sync::RwLock;

use crate::config::AppConfig;
use crate::job_runner::JobRunner;
use crate::p2p::PeerManager;

/// 専用のSpareLink WebダッシュボードHTTP配信サーバー (ポート 5180)
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
            let mut buf = [0u8; 8192];
            let n = match stream.read(&mut buf).await {
                Ok(n) if n > 0 => n,
                _ => return,
            };

            routes::handle_connection(
                stream,
                &buf[..n],
                config_clone,
                job_runner_clone,
                peer_manager_clone,
            ).await;
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
