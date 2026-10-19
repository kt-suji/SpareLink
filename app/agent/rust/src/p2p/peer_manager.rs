use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use futures_util::{SinkExt, StreamExt};
use tokio::sync::RwLock;
use tokio::time::sleep;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

use crate::discovery::{DiscoveredPeer, PeerDiscoveryProvider};
use crate::models::{
    LocalAgentTelemetryMessage, SurplusCapacity, SurplusEvaluationStatus, TailscaleTelemetry,
};
use super::protocol::{P2PMessage, PeerNodeState};

/// Manages peer discovery endpoints, P2P handshake heartbeats, and topology state.
pub struct PeerManager {
    local_id: String,
    discovery: Arc<dyn PeerDiscoveryProvider>,
    peers: Arc<RwLock<HashMap<String, PeerNodeState>>>,
    discovered_endpoints: Arc<RwLock<Vec<DiscoveredPeer>>>,
    local_telemetry: Arc<RwLock<Option<LocalAgentTelemetryMessage>>>,
}

impl PeerManager {
    pub fn new(
        local_id: String,
        discovery: Box<dyn PeerDiscoveryProvider>,
        local_telemetry: Arc<RwLock<Option<LocalAgentTelemetryMessage>>>,
    ) -> Self {
        Self {
            local_id,
            discovery: Arc::from(discovery),
            peers: Arc::new(RwLock::new(HashMap::new())),
            discovered_endpoints: Arc::new(RwLock::new(Vec::new())),
            local_telemetry,
        }
    }

    /// 現在把握している全ピアノードの一覧を取得する
    pub async fn get_all_peers(&self) -> Vec<PeerNodeState> {
        let lock = self.peers.read().await;
        lock.values().cloned().collect()
    }

    /// Tailscaleの稼働ステータスを取得する
    pub async fn get_tailscale_status(&self) -> Option<TailscaleTelemetry> {
        self.discovery.get_tailscale_status().await
    }

    /// 外部（着信WebSocket）からPingを受信した際、ピア状態を更新してPongメッセージを生成する
    pub async fn handle_incoming_ping(
        &self,
        sender_id: String,
        sender_name: String,
        telemetry: LocalAgentTelemetryMessage,
    ) -> Option<P2PMessage> {
        {
            let mut lock = self.peers.write().await;
            let entry = lock.entry(sender_id.clone()).or_insert_with(|| PeerNodeState {
                id: sender_id.clone(),
                name: sender_name.clone(),
                address: sender_id.clone(),
                os: Some(telemetry.os.clone()),
                status: "online".to_string(),
                latency_ms: None,
                last_seen_secs_ago: Some(0),
                cpu_cores: Some(telemetry.cpu_cores),
                cpu_percent: Some(telemetry.cpu_percent),
                memory_percent: Some(telemetry.memory_percent),
                memory_total_gb: Some(telemetry.memory_total_gb),
                memory_available_gb: Some(telemetry.memory_available_gb),
                cpu_benchmark_score: telemetry.cpu_benchmark_score,
                surplus: telemetry.surplus.clone(),
                transport: self.discovery.transport_kind().to_string(),
            });

            entry.status = "online".to_string();
            entry.last_seen_secs_ago = Some(0);
            entry.cpu_percent = Some(telemetry.cpu_percent);
            entry.memory_percent = Some(telemetry.memory_percent);
            entry.memory_available_gb = Some(telemetry.memory_available_gb);
            entry.surplus = telemetry.surplus.clone();
        }

        // 自身の最新テレメトリを載せてPongを返信
        let my_telem = {
            let t_lock = self.local_telemetry.read().await;
            t_lock.clone()
        }?;

        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        Some(P2PMessage::Pong {
            sender_id: self.local_id.clone(),
            sender_name: my_telem.pc_name.clone(),
            timestamp_ms: now_ms,
            telemetry: my_telem,
        })
    }

    /// Starts background tasks for peer discovery and P2P heartbeat synchronization.
    pub fn start_background_tasks(self: &Arc<Self>, scan_interval_secs: u64, probe_interval_secs: u64) {
        let scan_interval = Duration::from_secs(scan_interval_secs.max(2));
        let probe_interval = Duration::from_secs(probe_interval_secs.max(1));

        let manager_discovery = Arc::clone(self);
        // Task 1: Periodic discovery scan
        tokio::spawn(async move {
            loop {
                match manager_discovery.discovery.discover_peers().await {
                    Ok(peers) => {
                        let mut end_lock = manager_discovery.discovered_endpoints.write().await;
                        *end_lock = peers.clone();

                        let mut peer_lock = manager_discovery.peers.write().await;
                        for p in peers {
                            peer_lock.entry(p.id.clone()).or_insert_with(|| PeerNodeState {
                                id: p.id.clone(),
                                name: p.name.clone(),
                                address: p.ip.clone(),
                                os: p.os.clone(),
                                status: "offline".to_string(),
                                latency_ms: None,
                                last_seen_secs_ago: None,
                                cpu_cores: None,
                                cpu_percent: None,
                                memory_percent: None,
                                memory_total_gb: None,
                                memory_available_gb: None,
                                cpu_benchmark_score: None,
                                surplus: SurplusCapacity {
                                    status: SurplusEvaluationStatus::InsufficientData,
                                    confidence: None,
                                    allocatable_cpu_score: None,
                                    allocatable_memory_gb: None,
                                    algorithm_version: None,
                                    status_note: "Peer agent awaiting connection".to_string(),
                                },
                                transport: p.transport.to_string(),
                            });
                        }
                    }
                    Err(e) => {
                        eprintln!("[SpareLink P2P] Discovery scan error: {}", e);
                    }
                }
                sleep(scan_interval).await;
            }
        });

        let manager_ping = Arc::clone(self);
        // Task 2: Periodic heartbeat ping to discovered peers
        tokio::spawn(async move {
            sleep(Duration::from_millis(1500)).await;

            loop {
                let my_telem = {
                    let lock = manager_ping.local_telemetry.read().await;
                    lock.clone()
                };

                if let Some(telemetry) = my_telem {
                    let targets = {
                        let end_lock = manager_ping.discovered_endpoints.read().await;
                        end_lock.clone()
                    };

                    for target in targets {
                        let m_clone = Arc::clone(&manager_ping);
                        let telem_clone = telemetry.clone();

                        tokio::spawn(async move {
                            m_clone.probe_peer(&target, telem_clone).await;
                        });
                    }
                }

                sleep(probe_interval).await;
            }
        });
    }

    /// 特定のピアに対してWebSocket Pingを試行し、応答テレメトリと遅延を記録する
    async fn probe_peer(&self, peer: &DiscoveredPeer, local_telemetry: LocalAgentTelemetryMessage) {
        let ws_url = peer.ws_url();
        let start = Instant::now();

        let ping_msg = P2PMessage::Ping {
            sender_id: self.local_id.clone(),
            sender_name: local_telemetry.pc_name.clone(),
            timestamp_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
            telemetry: local_telemetry,
        };

        // 1.5秒タイムアウトで接続試行
        let probe_result = tokio::time::timeout(Duration::from_millis(1500), async {
            let (mut ws_stream, _) = connect_async(&ws_url).await?;
            let ping_json = serde_json::to_string(&ping_msg)?;
            ws_stream.send(Message::Text(ping_json.into())).await?;

            if let Some(Ok(Message::Text(reply))) = ws_stream.next().await {
                let msg: P2PMessage = serde_json::from_str(&reply)?;
                return Ok::<P2PMessage, Box<dyn std::error::Error + Send + Sync>>(msg);
            }

            Err::<P2PMessage, Box<dyn std::error::Error + Send + Sync>>("応答を受信できませんでした".into())
        })
        .await;

        let elapsed_ms = start.elapsed().as_millis() as u64;

        let mut lock = self.peers.write().await;
        if let Some(entry) = lock.get_mut(&peer.id) {
            match probe_result {
                Ok(Ok(P2PMessage::Pong { telemetry, .. })) => {
                    // P2P疎通成功！ 相手の最新実測値と余剰資源を反映
                    entry.status = "online".to_string();
                    entry.latency_ms = Some(elapsed_ms);
                    entry.last_seen_secs_ago = Some(0);
                    entry.cpu_cores = Some(telemetry.cpu_cores);
                    entry.cpu_percent = Some(telemetry.cpu_percent);
                    entry.memory_percent = Some(telemetry.memory_percent);
                    entry.memory_total_gb = Some(telemetry.memory_total_gb);
                    entry.memory_available_gb = Some(telemetry.memory_available_gb);
                    entry.cpu_benchmark_score = telemetry.cpu_benchmark_score;
                    entry.surplus = telemetry.surplus;
                }
                _ => {
                    // 相手先PCでSpareLink Agentが未稼働、またはネットワーク不通
                    entry.status = "offline".to_string();
                    entry.latency_ms = None;
                }
            }
        }
    }
}
