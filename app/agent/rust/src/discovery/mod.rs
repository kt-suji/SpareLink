pub mod tailscale;
pub mod wireguard;

use serde::{Deserialize, Serialize};

/** 通信トランスポート（アンダーレイ・VPN）の種別 */
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportKind {
    /// Tailscale (デフォルト: Tailnetによるゼロコンフィグ自動検出)
    Tailscale,
    /// WireGuard (静的設定または設定ファイルベースのVPN)
    Wireguard,
    /// 静的IPリスト (テスト・検証用)
    Static,
    /// LANブロードキャスト / マルチキャスト
    Lan,
}

impl std::fmt::Display for TransportKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportKind::Tailscale => write!(f, "Tailscale"),
            TransportKind::Wireguard => write!(f, "WireGuard"),
            TransportKind::Static => write!(f, "Static"),
            TransportKind::Lan => write!(f, "LAN"),
        }
    }
}

/** 発見されたピアのエンドポイント情報 */
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredPeer {
    /// ノード識別子 (ホスト名または公開鍵ハッシュ)
    pub id: String,
    /// 表示用PC名
    pub name: String,
    /// 接続先IPアドレス（ポート除く、例: "100.126.20.110" や "10.0.0.2"）
    pub ip: String,
    /// SpareLink Agentの待受ポート (通常 3001)
    pub port: u16,
    /// OS種別
    pub os: Option<String>,
    /// 使用トランスポート
    pub transport: TransportKind,
    /// トランスポート層でのオンライン・到達可能フラグ
    pub is_online: bool,
}

impl DiscoveredPeer {
    /// 接続用のアドレス文字列 ("IP:Port") を生成する
    #[allow(dead_code)]
    pub fn socket_address(&self) -> String {
        format!("{}:{}", self.ip, self.port)
    }

    /// WebSocket接続URL ("ws://IP:Port") を生成する
    pub fn ws_url(&self) -> String {
        format!("ws://{}:{}", self.ip, self.port)
    }
}

use std::future::Future;
use std::pin::Pin;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/**
 * ピア探索プロバイダーの共通インターフェース (抽象化トレイト)
 *
 * 【設計方針】
 * Tailscale、WireGuard、ローカルLAN、設定ファイルなど、将来どのような通信方式に
 * 移行・追加されても、P2Pコアエンジンや余剰計算資源評価ロジックを変更することなく
 * このトレイトの実装を差し替えるだけで動作可能にする。
 */
pub trait PeerDiscoveryProvider: Send + Sync {
    /// プロバイダー名
    fn name(&self) -> &'static str;

    /// 使用するトランスポート種別
    fn transport_kind(&self) -> TransportKind;

    /// 接続候補となるピアノード一覧を非同期で探索・取得する
    fn discover_peers<'a>(
        &'a self,
    ) -> BoxFuture<'a, Result<Vec<DiscoveredPeer>, Box<dyn std::error::Error + Send + Sync>>>;

    /// Tailscaleの稼働ステータスおよびオンボーディング情報を取得する (プロバイダーがTailscaleの場合)
    fn get_tailscale_status<'a>(
        &'a self,
    ) -> BoxFuture<'a, Option<crate::models::TailscaleTelemetry>> {
        Box::pin(async { None })
    }
}

/// Configuration for peer discovery provider initialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryConfig {
    pub transport: TransportKind,
    pub default_p2p_port: u16,
    pub wireguard_config_path: Option<String>,
    pub static_peers: Vec<String>,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            transport: TransportKind::Tailscale,
            default_p2p_port: 3001,
            wireguard_config_path: None,
            static_peers: Vec::new(),
        }
    }
}

/// Factory creating a peer discovery provider based on transport configuration.
pub fn create_discovery_provider(
    config: &DiscoveryConfig,
) -> Box<dyn PeerDiscoveryProvider> {
    match config.transport {
        TransportKind::Tailscale => {
            Box::new(tailscale::TailscaleDiscoveryProvider::new(config.default_p2p_port))
        }
        TransportKind::Wireguard | TransportKind::Static => {
            Box::new(wireguard::WireguardDiscoveryProvider::new(
                config.default_p2p_port,
                config.wireguard_config_path.clone(),
                config.static_peers.clone(),
            ))
        }
        TransportKind::Lan => {
            Box::new(wireguard::WireguardDiscoveryProvider::new(
                config.default_p2p_port,
                None,
                config.static_peers.clone(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tailscale_discovery_runs() {
        let provider = tailscale::TailscaleDiscoveryProvider::new(3001);
        assert_eq!(provider.transport_kind(), TransportKind::Tailscale);
        let peers_res = provider.discover_peers().await;
        assert!(peers_res.is_ok(), "Tailscale探索はエラーなく完了すべき");
        let peers = peers_res.unwrap();
        println!("Tailscaleで発見されたピア数: {}", peers.len());
        for p in &peers {
            println!(" - {} ({}) @ {}", p.name, p.os.as_deref().unwrap_or("?"), p.ws_url());
        }
    }

    #[tokio::test]
    async fn test_wireguard_env_discovery() {
        unsafe {
            std::env::set_var("SPARELINK_PEERS", "10.0.0.2:3001,10.0.0.3:3002");
        }
        let provider = wireguard::WireguardDiscoveryProvider::new(3001, None, Vec::new());
        assert_eq!(provider.transport_kind(), TransportKind::Wireguard);
        let peers = provider.discover_peers().await.expect("WireGuard discovery succeeded");
        assert_eq!(peers.len(), 2);
        assert_eq!(peers[0].ip, "10.0.0.2");
        assert_eq!(peers[0].port, 3001);
        assert_eq!(peers[1].ip, "10.0.0.3");
        assert_eq!(peers[1].port, 3002);
    }

    #[test]
    fn test_factory_switching() {
        let ts_cfg = DiscoveryConfig {
            transport: TransportKind::Tailscale,
            default_p2p_port: 3001,
            wireguard_config_path: None,
            static_peers: Vec::new(),
        };
        let ts_provider = create_discovery_provider(&ts_cfg);
        assert_eq!(ts_provider.transport_kind(), TransportKind::Tailscale);

        let wg_cfg = DiscoveryConfig {
            transport: TransportKind::Wireguard,
            default_p2p_port: 3001,
            wireguard_config_path: None,
            static_peers: Vec::new(),
        };
        let wg_provider = create_discovery_provider(&wg_cfg);
        assert_eq!(wg_provider.transport_kind(), TransportKind::Wireguard);
    }
}
