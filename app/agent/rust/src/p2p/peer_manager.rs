use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use futures_util::{SinkExt, StreamExt};
use tokio::sync::RwLock;
use tokio::time::sleep;
use tokio_tungstenite::client_async;
use tokio_tungstenite::tungstenite::Message;

use crate::discovery::{DiscoveredPeer, PeerDiscoveryProvider};
use crate::models::{
    LocalAgentTelemetryMessage, SurplusCapacity, SurplusEvaluationStatus, TailscaleTelemetry,
};
use super::protocol::{P2PMessage, PeerNodeState};

/// 受信したPingの送信元情報と既存ピアリストを照合し、同一ノードのキーを特定する
fn find_matching_peer_key(
    peers: &HashMap<String, PeerNodeState>,
    sender_id: &str,
    sender_name: &str,
) -> Option<String> {
    let s_id_lower = sender_id.to_lowercase();
    let s_name_lower = sender_name.to_lowercase();
    let ts_key = format!("tailscale:{}", s_id_lower);
    let ts_name_key = format!("tailscale:{}", s_name_lower);

    if peers.contains_key(sender_id) {
        return Some(sender_id.to_string());
    }
    if peers.contains_key(&ts_key) {
        return Some(ts_key);
    }
    if peers.contains_key(&ts_name_key) {
        return Some(ts_name_key);
    }

    for (k, v) in peers {
        let k_clean = k.strip_prefix("tailscale:").unwrap_or(k).to_lowercase();
        let id_clean = v.id.strip_prefix("tailscale:").unwrap_or(&v.id).to_lowercase();
        let name_clean = v.name.to_lowercase();

        if k_clean == s_id_lower
            || k_clean == s_name_lower
            || id_clean == s_id_lower
            || id_clean == s_name_lower
            || name_clean == s_id_lower
            || name_clean == s_name_lower
        {
            return Some(k.clone());
        }
    }

    None
}

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
            let target_key = find_matching_peer_key(&lock, &sender_id, &sender_name)
                .unwrap_or_else(|| {
                    if sender_id.starts_with("tailscale:") {
                        sender_id.clone()
                    } else {
                        format!("tailscale:{}", sender_id.to_lowercase())
                    }
                });

            let entry = lock.entry(target_key.clone()).or_insert_with(|| PeerNodeState {
                id: target_key.clone(),
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
            entry.name = sender_name.clone();
            entry.last_seen_secs_ago = Some(0);
            entry.cpu_cores = Some(telemetry.cpu_cores);
            entry.memory_total_gb = Some(telemetry.memory_total_gb);
            entry.cpu_benchmark_score = telemetry.cpu_benchmark_score;
            entry.cpu_percent = Some(telemetry.cpu_percent);
            entry.memory_percent = Some(telemetry.memory_percent);
            entry.memory_available_gb = Some(telemetry.memory_available_gb);
            entry.surplus = telemetry.surplus.clone();
            if entry.latency_ms.is_none() {
                entry.latency_ms = Some(1);
            }
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
                    // ピアの前回疎通からの経過秒数を更新（15秒以上疎通が途絶えたらオフライン化）
                    {
                        let mut p_lock = manager_ping.peers.write().await;
                        for p in p_lock.values_mut() {
                            if let Some(ref mut sec) = p.last_seen_secs_ago {
                                *sec = sec.saturating_add(probe_interval_secs);
                                if *sec > 15 {
                                    p.status = "offline".to_string();
                                    p.latency_ms = None;
                                }
                            }
                        }
                    }

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

    /// 特定のピアに対してWebSocket Pingを試行し、応答テレメトリと正確なPing遅延(ms)を記録する
    async fn probe_peer(&self, peer: &DiscoveredPeer, local_telemetry: LocalAgentTelemetryMessage) {
        let ws_url = peer.ws_url();
        let sock_addr = peer.socket_address();

        let ping_msg = P2PMessage::Ping {
            sender_id: self.local_id.clone(),
            sender_name: local_telemetry.pc_name.clone(),
            timestamp_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
            telemetry: local_telemetry,
        };

        // 1.5秒タイムアウトで接続およびPing/Pong計測
        let probe_result = tokio::time::timeout(Duration::from_millis(1500), async {
            // 1. 純粋なTCPハンドシェイク遅延 (SYN -> SYN-ACKのネットワーク往復RTT)
            let tcp_start = Instant::now();
            let tcp_stream = tokio::net::TcpStream::connect(&sock_addr).await?;
            let tcp_rtt_ms = tcp_start.elapsed().as_millis() as u64;

            // 2. Nagleアルゴリズム無効化 (WindowsのDelayed ACK 40ms待機を回避)
            let _ = tcp_stream.set_nodelay(true);

            // 3. WebSocketプロトコルアップグレード
            let (mut ws_stream, _) = client_async(&ws_url, tcp_stream).await?;

            // 4. Ping送信からPong受信までの往復RTT計測
            let ping_json = serde_json::to_string(&ping_msg)?;
            let ping_start = Instant::now();
            ws_stream.send(Message::Text(ping_json.into())).await?;

            while let Some(Ok(Message::Text(reply))) = ws_stream.next().await {
                if let Ok(msg) = serde_json::from_str::<P2PMessage>(&reply) {
                    if let P2PMessage::Pong { telemetry, .. } = msg {
                        let app_rtt_ms = ping_start.elapsed().as_millis() as u64;
                        let latency = app_rtt_ms.min(tcp_rtt_ms);
                        return Ok::<(LocalAgentTelemetryMessage, u64), Box<dyn std::error::Error + Send + Sync>>((telemetry, latency));
                    }
                } else if let Ok(telem) = serde_json::from_str::<LocalAgentTelemetryMessage>(&reply) {
                    let app_rtt_ms = ping_start.elapsed().as_millis() as u64;
                    let latency = app_rtt_ms.min(tcp_rtt_ms);
                    return Ok::<(LocalAgentTelemetryMessage, u64), Box<dyn std::error::Error + Send + Sync>>((telem, latency));
                }
            }

            Err::<(LocalAgentTelemetryMessage, u64), Box<dyn std::error::Error + Send + Sync>>("応答を受信できませんでした".into())
        })
        .await;

        let mut lock = self.peers.write().await;
        let target_key = find_matching_peer_key(&lock, &peer.id, &peer.name)
            .unwrap_or_else(|| peer.id.clone());

        if let Some(entry) = lock.get_mut(&target_key) {
            match probe_result {
                Ok(Ok((telemetry, latency_ms))) => {
                    // P2P疎通成功！ 相手の最新実測値と実測Ping遅延(ms)を反映
                    entry.status = "online".to_string();
                    entry.latency_ms = Some(latency_ms);
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
                    // 相手先PCへの外向きProbeが失敗した場合でも、直近でPingを受信している場合はオンラインを維持
                    let is_recently_seen = entry.last_seen_secs_ago.map(|s| s < 15).unwrap_or(false);
                    if is_recently_seen {
                        if entry.latency_ms.is_none() {
                            entry.latency_ms = Some(0);
                        }
                    } else {
                        entry.status = "offline".to_string();
                        entry.latency_ms = None;
                    }
                }
            }
        }
    }
}
