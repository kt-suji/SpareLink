use std::path::PathBuf;
use serde::{Deserialize, Serialize};

use super::{DiscoveredPeer, PeerDiscoveryProvider, TransportKind};

/** WireGuard / 静的設定ファイルで定義されるピア構造 */
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireguardPeerConfig {
    pub name: String,
    pub ip: String,
    pub port: Option<u16>,
    pub os: Option<String>,
}

/// WireGuard and static endpoint discovery provider.
pub struct WireguardDiscoveryProvider {
    pub default_p2p_port: u16,
    pub config_path: Option<PathBuf>,
    pub static_peers: Vec<String>,
}

impl WireguardDiscoveryProvider {
    pub fn new(default_p2p_port: u16, config_path: Option<String>, static_peers: Vec<String>) -> Self {
        let path = config_path
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("wireguard_peers.json"));
        Self {
            default_p2p_port,
            config_path: Some(path),
            static_peers,
        }
    }

    fn parse_peer_string(&self, idx: usize, entry: &str) -> Option<DiscoveredPeer> {
        let entry = entry.trim();
        if entry.is_empty() {
            return None;
        }
        let parts: Vec<&str> = entry.split(':').collect();
        let ip = parts[0].to_string();
        let port = if parts.len() > 1 {
            parts[1].parse::<u16>().unwrap_or(self.default_p2p_port)
        } else {
            self.default_p2p_port
        };

        let name = format!("peer-{}", idx + 1);
        Some(DiscoveredPeer {
            id: format!("node:{}", ip.replace('.', "-")),
            name,
            ip,
            port,
            os: Some("unknown".to_string()),
            transport: TransportKind::Wireguard,
            is_online: true,
        })
    }

    /// Reads peer addresses from SPARELINK_PEERS environment variable.
    fn discover_from_env(&self) -> Vec<DiscoveredPeer> {
        let mut peers = Vec::new();
        if let Ok(env_val) = std::env::var("SPARELINK_PEERS") {
            for (idx, entry) in env_val.split(',').enumerate() {
                if let Some(peer) = self.parse_peer_string(idx, entry) {
                    peers.push(peer);
                }
            }
        }
        peers
    }

    fn discover_from_static_config(&self) -> Vec<DiscoveredPeer> {
        self.static_peers
            .iter()
            .enumerate()
            .filter_map(|(idx, entry)| self.parse_peer_string(idx, entry))
            .collect()
    }
}

impl PeerDiscoveryProvider for WireguardDiscoveryProvider {
    fn name(&self) -> &'static str {
        "WireGuard / Static Config Discovery"
    }

    fn transport_kind(&self) -> TransportKind {
        TransportKind::Wireguard
    }

    fn discover_peers<'a>(
        &'a self,
    ) -> super::BoxFuture<'a, Result<Vec<DiscoveredPeer>, Box<dyn std::error::Error + Send + Sync>>> {
        Box::pin(async move {
            // 1. Static peer endpoints from configuration
            let from_static = self.discover_from_static_config();
            if !from_static.is_empty() {
                return Ok(from_static);
            }

            // 2. Environment variable overrides
            let from_env = self.discover_from_env();
            if !from_env.is_empty() {
                return Ok(from_env);
            }

            // 2. 設定ファイルが存在すれば読み込み
            if let Some(ref path) = self.config_path {
                if path.exists() {
                    let content = std::fs::read_to_string(path)?;
                    let list: Vec<WireguardPeerConfig> = serde_json::from_str(&content)?;

                    let peers = list
                        .into_iter()
                        .map(|c| DiscoveredPeer {
                            id: format!("wireguard:{}", c.name.to_lowercase()),
                            name: c.name,
                            ip: c.ip,
                            port: c.port.unwrap_or(self.default_p2p_port),
                            os: c.os,
                            transport: TransportKind::Wireguard,
                            is_online: true,
                        })
                        .collect();

                    return Ok(peers);
                }
            }

            // 3. 設定がない場合は空のリストを返却（エラーで異常終了はさせない）
            Ok(Vec::new())
        })
    }
}
