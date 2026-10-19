mod bench;
mod config;
mod discovery;
mod evaluator;
mod models;
mod native_probe;
mod p2p;

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::{broadcast, RwLock};
use tokio_tungstenite::tungstenite::Message;

use config::{AppConfig, EvaluatorSettings};
use discovery::{DiscoveryConfig, TransportKind};
use models::{LocalAgentTelemetryMessage, ProbeMetrics};
use p2p::{P2PMessage, PeerManager};

fn find_python_path() -> PathBuf {
    let candidates = [
        PathBuf::from("../../.venv/Scripts/python.exe"),
        PathBuf::from("../../../.venv/Scripts/python.exe"),
        PathBuf::from(".venv/Scripts/python.exe"),
    ];

    for candidate in &candidates {
        if candidate.exists() {
            return candidate.clone();
        }
    }

    PathBuf::from("python")
}

async fn process_and_broadcast(
    metrics: ProbeMetrics,
    benchmark_score: u32,
    evaluator_cfg: &EvaluatorSettings,
    peer_manager: &Arc<PeerManager>,
    local_telemetry_holder: &Arc<RwLock<Option<LocalAgentTelemetryMessage>>>,
    latest_metrics: &Arc<RwLock<Option<String>>>,
    tx: &broadcast::Sender<String>,
) {
    let surplus = evaluator::evaluate_surplus_capacity_with_config(
        &metrics,
        Some(benchmark_score),
        evaluator_cfg,
    );
    let known_peers = peer_manager.get_all_peers().await;
    let tailscale = peer_manager.get_tailscale_status().await;

    println!(
        "[{}] CPU: {:>4.1}% | MEM: {:>4.1}% ({:.1}/{:.1}GB) | Surplus CPU: {:>5} pt | Surplus MEM: {:>4.1}GB | Peers: {} online",
        metrics.pc_name,
        metrics.cpu_percent,
        metrics.memory_percent,
        (metrics.memory_total_gb - metrics.memory_available_gb).max(0.0),
        metrics.memory_total_gb,
        surplus.allocatable_cpu_score.unwrap_or(0),
        surplus.allocatable_memory_gb.unwrap_or(0.0),
        known_peers.iter().filter(|p| p.status == "online").count(),
    );

    let telemetry = LocalAgentTelemetryMessage {
        pc_name: metrics.pc_name,
        os: metrics.os,
        cpu_cores: metrics.cpu_cores,
        cpu_percent: metrics.cpu_percent,
        memory_percent: metrics.memory_percent,
        memory_total_gb: metrics.memory_total_gb,
        memory_available_gb: metrics.memory_available_gb,
        cpu_benchmark_score: Some(benchmark_score),
        surplus,
        peers: Some(known_peers),
        tailscale,
    };

    {
        let mut t_lock = local_telemetry_holder.write().await;
        *t_lock = Some(telemetry.clone());
    }

    if let Ok(telemetry_json) = serde_json::to_string(&telemetry) {
        {
            let mut lock = latest_metrics.write().await;
            *lock = Some(telemetry_json.clone());
        }
        let _ = tx.send(telemetry_json);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = AppConfig::load();

    println!("[SpareLink] Initializing agent daemon...");

    // 1. Benchmark evaluation
    let benchmark_score = if config.benchmark.manual_score > 0 {
        println!("[SpareLink] Manual benchmark override: {} pt", config.benchmark.manual_score);
        config.benchmark.manual_score
    } else if config.benchmark.enabled {
        let detected_cores = std::thread::available_parallelism()
            .map(|n| n.get() as u32)
            .unwrap_or(4);
        println!(
            "[SpareLink] Running benchmark across {} logical cores...",
            detected_cores
        );
        tokio::task::spawn_blocking(move || {
            bench::run_quick_cpu_benchmark(detected_cores)
        })
        .await
        .unwrap_or(10_000)
    } else {
        10_000
    };

    println!("[SpareLink] Benchmark score: {} pt", benchmark_score);

    // 2. Transport & Peer Discovery Provider initialization
    let transport_kind = match config.discovery.transport.to_lowercase().as_str() {
        "wireguard" | "wg" => TransportKind::Wireguard,
        "static" => TransportKind::Static,
        "lan" => TransportKind::Lan,
        _ => TransportKind::Tailscale,
    };

    let discovery_config = DiscoveryConfig {
        transport: transport_kind,
        default_p2p_port: config.agent.port,
        wireguard_config_path: config.discovery.config_path.clone(),
        static_peers: config.discovery.static_peers.clone(),
    };

    let discovery_provider = discovery::create_discovery_provider(&discovery_config);
    println!(
        "[SpareLink] Discovery provider: {} ({})",
        discovery_provider.name(),
        discovery_provider.transport_kind()
    );

    let local_id = if !config.agent.node_name.is_empty() {
        config.agent.node_name.clone()
    } else {
        std::env::var("COMPUTERNAME")
            .or_else(|_| std::env::var("HOSTNAME"))
            .unwrap_or_else(|_| "node".to_string())
            .to_lowercase()
    };

    let local_telemetry_holder = Arc::new(RwLock::new(None::<LocalAgentTelemetryMessage>));

    let peer_manager = Arc::new(PeerManager::new(
        local_id,
        discovery_provider,
        local_telemetry_holder.clone(),
    ));

    // Tailscale等の初期ステータス確認およびオンボーディング案内
    if let Some(ts) = peer_manager.get_tailscale_status().await {
        match ts.state {
            models::TailscaleState::Running => {
                println!(
                    "[SpareLink] Tailscale接続完了 (ノード名: {}, IP: {})",
                    ts.self_hostname.as_deref().unwrap_or("不明"),
                    ts.self_ip.as_deref().unwrap_or("未割当")
                );
            }
            models::TailscaleState::NeedsLogin => {
                println!("[SpareLink] Tailscale未ログイン: ブラウザで認証ログイン画面を開きました。");
            }
            models::TailscaleState::NotInstalled => {
                println!("[SpareLink] Tailscale未検出: ブラウザで公式インストーラー案内を開きました。");
            }
            models::TailscaleState::Stopped => {
                println!("[SpareLink] Tailscaleサービスが停止しています。接続を有効化してください。");
            }
            models::TailscaleState::Error => {
                println!("[SpareLink] Tailscale状態取得エラー: {}", ts.message);
            }
        }
    }

    peer_manager.start_background_tasks(
        config.discovery.scan_interval_secs,
        config.discovery.probe_interval_secs,
    );

    let latest_metrics = Arc::new(RwLock::new(None::<String>));
    let (tx, _rx) = broadcast::channel::<String>(32);

    let python_path = find_python_path();
    let script_path = PathBuf::from("../python/main.py");
    let has_python_script = script_path.exists();

    let tx_probe = tx.clone();
    let latest_metrics_probe = latest_metrics.clone();
    let peer_manager_probe = peer_manager.clone();
    let local_telemetry_probe = local_telemetry_holder.clone();
    let evaluator_cfg = config.evaluator.clone();
    let broadcast_interval_ms = config.agent.broadcast_interval_ms;

    // 3. Metrics collection probe spawn
    if has_python_script {
        let evaluator_cfg_py = evaluator_cfg.clone();
        match Command::new(&python_path)
            .arg(&script_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
        {
            Ok(mut child) => {
                if let Some(stdout) = child.stdout.take() {
                    tokio::spawn(async move {
                        let mut reader = BufReader::new(stdout).lines();
                        while let Ok(Some(line)) = reader.next_line().await {
                            if let Ok(metrics) = serde_json::from_str::<ProbeMetrics>(&line) {
                                process_and_broadcast(
                                    metrics,
                                    benchmark_score,
                                    &evaluator_cfg_py,
                                    &peer_manager_probe,
                                    &local_telemetry_probe,
                                    &latest_metrics_probe,
                                    &tx_probe,
                                )
                                .await;
                            }
                        }
                    });
                }
            }
            Err(e) => {
                eprintln!("[SpareLink] Python probe launch error ({}). Falling back to Windows native probe.", e);
                spawn_native_probe_task(
                    benchmark_score,
                    evaluator_cfg,
                    broadcast_interval_ms,
                    peer_manager_probe,
                    local_telemetry_probe,
                    latest_metrics_probe,
                    tx_probe,
                );
            }
        }
    } else {
        spawn_native_probe_task(
            benchmark_score,
            evaluator_cfg,
            broadcast_interval_ms,
            peer_manager_probe,
            local_telemetry_probe,
            latest_metrics_probe,
            tx_probe,
        );
    }

    // 4. WebSocket & P2P server listener
    let ws_addr = format!("{}:{}", config.agent.host, config.agent.port);
    let listener = tokio::net::TcpListener::bind(&ws_addr).await?;
    println!("[SpareLink] WebSocket API & P2P listener active on ws://{}", ws_addr);

    while let Ok((stream, peer_addr)) = listener.accept().await {
        let mut rx = tx.subscribe();
        let latest_metrics = latest_metrics.clone();
        let peer_manager_ws = peer_manager.clone();

        tokio::spawn(async move {
            match tokio_tungstenite::accept_async(stream).await {
                Ok(mut ws_stream) => {
                    {
                        let lock = latest_metrics.read().await;
                        if let Some(ref json) = *lock {
                            let _ = ws_stream.send(Message::Text(json.clone().into())).await;
                        }
                    }

                    loop {
                        tokio::select! {
                            msg_opt = ws_stream.next() => {
                                match msg_opt {
                                    Some(Ok(Message::Text(text))) => {
                                        if let Ok(p2p_msg) = serde_json::from_str::<P2PMessage>(&text) {
                                            if let P2PMessage::Ping { sender_id, sender_name, telemetry, .. } = p2p_msg {
                                                if let Some(pong) = peer_manager_ws.handle_incoming_ping(sender_id, sender_name, telemetry).await {
                                                    if let Ok(pong_json) = serde_json::to_string(&pong) {
                                                        let _ = ws_stream.send(Message::Text(pong_json.into())).await;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    Some(Ok(Message::Close(_))) | None => break,
                                    _ => {}
                                }
                            }
                            telemetry_res = rx.recv() => {
                                match telemetry_res {
                                    Ok(json) => {
                                        if ws_stream.send(Message::Text(json.into())).await.is_err() {
                                            break;
                                        }
                                    }
                                    Err(_) => break,
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("[SpareLink] WebSocket handshake error from {}: {}", peer_addr, e);
                }
            }
        });
    }

    Ok(())
}

fn spawn_native_probe_task(
    benchmark_score: u32,
    evaluator_cfg: EvaluatorSettings,
    broadcast_interval_ms: u64,
    peer_manager: Arc<PeerManager>,
    local_telemetry_holder: Arc<RwLock<Option<LocalAgentTelemetryMessage>>>,
    latest_metrics: Arc<RwLock<Option<String>>>,
    tx: broadcast::Sender<String>,
) {
    let interval = Duration::from_millis(broadcast_interval_ms.max(500));
    tokio::spawn(async move {
        let mut probe = native_probe::NativeWindowsProbe::new();
        loop {
            tokio::time::sleep(interval).await;
            let metrics = probe.collect_metrics();
            process_and_broadcast(
                metrics,
                benchmark_score,
                &evaluator_cfg,
                &peer_manager,
                &local_telemetry_holder,
                &latest_metrics,
                &tx,
            )
            .await;
        }
    });
}