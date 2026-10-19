use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::RwLock;

use crate::models::{TailscaleState, TailscaleTelemetry};
use super::{BoxFuture, DiscoveredPeer, PeerDiscoveryProvider, TransportKind};

static BROWSER_OPENED_INSTALL: AtomicBool = AtomicBool::new(false);
static BROWSER_OPENED_LOGIN: AtomicBool = AtomicBool::new(false);

/// デフォルトブラウザで指定URLを開く（Windows/macOS/Linux対応）
pub fn open_browser(url: &str) {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg(url)
            .spawn();
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        let _ = std::process::Command::new("xdg-open")
            .arg(url)
            .spawn();
    }
}

/// TailscaleバイナリをPATHおよび各OSの標準インストールディレクトリから探索する
pub fn find_tailscale_binary() -> Option<PathBuf> {
    // 1. 環境変数PATHの確認
    if let Ok(output) = std::process::Command::new("tailscale").arg("version").output() {
        if output.status.success() {
            return Some(PathBuf::from("tailscale"));
        }
    }

    // 2. Windows標準インストールパス
    #[cfg(target_os = "windows")]
    {
        let standard_paths = [
            r"C:\Program Files\Tailscale\tailscale.exe",
            r"C:\Program Files (x86)\Tailscale\tailscale.exe",
            r"C:\Program Files\Tailscale IPN\tailscale.exe",
        ];
        for p in &standard_paths {
            let path = PathBuf::from(p);
            if path.exists() {
                return Some(path);
            }
        }

        if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
            let path = PathBuf::from(local_app_data).join("Tailscale").join("tailscale.exe");
            if path.exists() {
                return Some(path);
            }
        }
    }

    // 3. Unix標準パス
    #[cfg(not(target_os = "windows"))]
    {
        let unix_paths = [
            "/usr/bin/tailscale",
            "/usr/local/bin/tailscale",
            "/snap/bin/tailscale",
            "/Applications/Tailscale.app/Contents/MacOS/Tailscale",
        ];
        for p in &unix_paths {
            let path = PathBuf::from(p);
            if path.exists() {
                return Some(path);
            }
        }
    }

    None
}

fn print_install_guidance(url: &str) {
    println!("\n┌──────────────────────────────────────────────────────────────────┐");
    println!("│ 【Tailscaleのセットアップが必要です】                           │");
    println!("│ SpareLinkのP2P自動探索・計算資源共有には Tailscale が必要です。  │");
    println!("│ 導入手順:                                                        │");
    println!("│   1. 公式インストーラーを実行（ブラウザで自動表示します）       │");
    println!("│   2. Google/Microsoft/GitHub等のアカウントでログイン             │");
    println!("│   3. 接続したい他PCでも同じTailscaleアカウントでログイン         │");
    println!("│ ダウンロードURL: {:<47} │", url);
    println!("└──────────────────────────────────────────────────────────────────┘\n");
    println!("[SpareLink] ブラウザで Tailscale 公式ダウンロードページを開きました。");
    println!("[SpareLink] インストールとログインが完了すると、自動的に認識されます。\n");
}

fn print_login_guidance(url: &str) {
    println!("\n┌──────────────────────────────────────────────────────────────────┐");
    println!("│ 【Tailscaleのログイン認証が必要です】                             │");
    println!("│ Tailscaleが未ログイン状態です。                                  │");
    println!("│ ブラウザで認証ログインを行ってください。                         │");
    println!("│ 認証URL: {:<55} │", url);
    println!("└──────────────────────────────────────────────────────────────────┘\n");
    println!("[SpareLink] ブラウザで Tailscale ログイン認証ページを開きました。");
    println!("[SpareLink] ログインが完了すると、自動的にP2P同期が開始されます。\n");
}

/// Tailscaleの稼働ステータスとJSONレスポンスを調査する
pub async fn inspect_tailscale_status() -> (TailscaleTelemetry, Option<serde_json::Value>) {
    let binary = match find_tailscale_binary() {
        Some(bin) => bin,
        None => {
            let download_url = "https://tailscale.com/download".to_string();
            if !BROWSER_OPENED_INSTALL.swap(true, Ordering::SeqCst) {
                print_install_guidance(&download_url);
                open_browser(&download_url);
            }
            return (
                TailscaleTelemetry {
                    state: TailscaleState::NotInstalled,
                    self_ip: None,
                    self_hostname: None,
                    auth_url: Some(download_url),
                    message: "Tailscaleが未インストールです。公式サイトから導入してください。".to_string(),
                },
                None,
            );
        }
    };

    let output = match Command::new(&binary)
        .arg("status")
        .arg("--json")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
    {
        Ok(out) => out,
        Err(e) => {
            return (
                TailscaleTelemetry {
                    state: TailscaleState::Error,
                    self_ip: None,
                    self_hostname: None,
                    auth_url: None,
                    message: format!("Tailscaleの実行に失敗しました: {}", e),
                },
                None,
            );
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return (
            TailscaleTelemetry {
                state: TailscaleState::Stopped,
                self_ip: None,
                self_hostname: None,
                auth_url: None,
                message: if stderr.is_empty() {
                    "Tailscaleサービスが停止しています。".to_string()
                } else {
                    format!("Tailscale停止中: {}", stderr.trim())
                },
            },
            None,
        );
    }

    let json_str = String::from_utf8_lossy(&output.stdout);
    let root: serde_json::Value = match serde_json::from_str(&json_str) {
        Ok(v) => v,
        Err(e) => {
            return (
                TailscaleTelemetry {
                    state: TailscaleState::Error,
                    self_ip: None,
                    self_hostname: None,
                    auth_url: None,
                    message: format!("Tailscale応答の解析に失敗しました: {}", e),
                },
                None,
            );
        }
    };

    let backend_state = root
        .get("BackendState")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let auth_url = root
        .get("AuthURL")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let self_hostname = root
        .get("Self")
        .and_then(|s| s.get("HostName"))
        .and_then(|h| h.as_str())
        .map(|s| s.to_string());

    let self_ip = root
        .get("Self")
        .and_then(|s| s.get("TailscaleIPs"))
        .and_then(|ips| ips.as_array())
        .and_then(|arr| {
            arr.iter().find_map(|ip| {
                let s = ip.as_str()?;
                if s.contains('.') {
                    Some(s.to_string())
                } else {
                    None
                }
            })
        });

    match backend_state {
        "Running" => {
            BROWSER_OPENED_LOGIN.store(false, Ordering::SeqCst);
            (
                TailscaleTelemetry {
                    state: TailscaleState::Running,
                    self_ip,
                    self_hostname,
                    auth_url: None,
                    message: "Tailscale接続中".to_string(),
                },
                Some(root),
            )
        }
        "NeedsLogin" | "NeedsMachineAuth" => {
            let target_url = auth_url.clone().unwrap_or_else(|| "https://login.tailscale.com".to_string());
            if !BROWSER_OPENED_LOGIN.swap(true, Ordering::SeqCst) {
                print_login_guidance(&target_url);
                open_browser(&target_url);
            }
            (
                TailscaleTelemetry {
                    state: TailscaleState::NeedsLogin,
                    self_ip,
                    self_hostname,
                    auth_url: Some(target_url),
                    message: "Tailscaleのログイン認証が必要です。".to_string(),
                },
                Some(root),
            )
        }
        "Stopped" => {
            (
                TailscaleTelemetry {
                    state: TailscaleState::Stopped,
                    self_ip,
                    self_hostname,
                    auth_url: None,
                    message: "Tailscaleが停止しています。起動してください。".to_string(),
                },
                Some(root),
            )
        }
        other => {
            (
                TailscaleTelemetry {
                    state: TailscaleState::Stopped,
                    self_ip,
                    self_hostname,
                    auth_url,
                    message: format!("Tailscale状態: {}", other),
                },
                Some(root),
            )
        }
    }
}

/**
 * Tailscaleを用いたピア自動探索プロバイダー
 *
 * 【動作仕様】
 * - `tailscale status --json` を実行し、自ノードおよびTailnet内ピアを監視・探索。
 * - 未インストール・未ログイン時は自動的にユーザーをガイド。
 */
pub struct TailscaleDiscoveryProvider {
    pub default_p2p_port: u16,
    latest_telemetry: Arc<RwLock<Option<TailscaleTelemetry>>>,
}

impl TailscaleDiscoveryProvider {
    pub fn new(default_p2p_port: u16) -> Self {
        Self {
            default_p2p_port,
            latest_telemetry: Arc::new(RwLock::new(None)),
        }
    }
}

impl PeerDiscoveryProvider for TailscaleDiscoveryProvider {
    fn name(&self) -> &'static str {
        "Tailscale CLI Discovery"
    }

    fn transport_kind(&self) -> TransportKind {
        TransportKind::Tailscale
    }

    fn get_tailscale_status<'a>(
        &'a self,
    ) -> BoxFuture<'a, Option<TailscaleTelemetry>> {
        Box::pin(async move {
            {
                let lock = self.latest_telemetry.read().await;
                if let Some(ref telem) = *lock {
                    return Some(telem.clone());
                }
            }
            let (telem, _) = inspect_tailscale_status().await;
            let mut lock = self.latest_telemetry.write().await;
            *lock = Some(telem.clone());
            Some(telem)
        })
    }

    fn discover_peers<'a>(
        &'a self,
    ) -> BoxFuture<'a, Result<Vec<DiscoveredPeer>, Box<dyn std::error::Error + Send + Sync>>> {
        Box::pin(async move {
            let (telem, root_opt) = inspect_tailscale_status().await;
            {
                let mut lock = self.latest_telemetry.write().await;
                *lock = Some(telem.clone());
            }

            if telem.state != TailscaleState::Running {
                return Ok(Vec::new());
            }

            let root = match root_opt {
                Some(r) => r,
                None => return Ok(Vec::new()),
            };

            let mut discovered = Vec::new();

            // Self (自ノード) のホスト名を取得（除外用）
            let self_hostname = root
                .get("Self")
                .and_then(|s| s.get("HostName"))
                .and_then(|h| h.as_str())
                .unwrap_or("")
                .to_string();

            // "Peer" オブジェクト内の各ノードを解析
            if let Some(peers) = root.get("Peer").and_then(|p| p.as_object()) {
                for (_key, peer_val) in peers {
                    let hostname = peer_val
                        .get("HostName")
                        .and_then(|h| h.as_str())
                        .unwrap_or("")
                        .to_string();

                    // 自ノードは除外
                    if hostname.is_empty() || hostname == self_hostname {
                        continue;
                    }

                    let os = peer_val
                        .get("OS")
                        .and_then(|o| o.as_str())
                        .map(|s| s.to_string());

                    let online = peer_val
                        .get("Online")
                        .and_then(|o| o.as_bool())
                        .unwrap_or(false);

                    // TailscaleIPs 配列から最初のIPv4アドレス (100.x.y.z) を取得
                    let ip_opt = peer_val
                        .get("TailscaleIPs")
                        .and_then(|ips| ips.as_array())
                        .and_then(|arr| {
                            arr.iter().find_map(|ip| {
                                let s = ip.as_str()?;
                                if s.contains('.') {
                                    Some(s.to_string())
                                } else {
                                    None
                                }
                            })
                        });

                    if let Some(ip) = ip_opt {
                        discovered.push(DiscoveredPeer {
                            id: format!("tailscale:{}", hostname.to_lowercase()),
                            name: hostname,
                            ip,
                            port: self.default_p2p_port,
                            os,
                            transport: TransportKind::Tailscale,
                            is_online: online,
                        });
                    }
                }
            }

            Ok(discovered)
        })
    }
}
