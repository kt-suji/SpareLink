mod bench;
mod config;
mod discovery;
mod evaluator;
mod gui;
mod job_runner;
mod models;
mod native_probe;
mod p2p;
mod updater;
mod web_server;
mod ws_server;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{broadcast, RwLock};

use config::{AppConfig, EvaluatorSettings};
use discovery::{DiscoveryConfig, TransportKind};
use job_runner::JobRunner;
use models::{LocalAgentTelemetryMessage, ProbeMetrics};
use p2p::PeerManager;

#[derive(Clone)]
struct TelemetryHub {
    benchmark_score: u32,
    config: Arc<RwLock<AppConfig>>,
    peer_manager: Arc<PeerManager>,
    local_telemetry: Arc<RwLock<Option<LocalAgentTelemetryMessage>>>,
    latest_metrics: Arc<RwLock<Option<String>>>,
    tx: broadcast::Sender<String>,
    sharing_paused: Arc<AtomicBool>,
}

impl TelemetryHub {
    async fn process_and_broadcast(
        &self,
        metrics: ProbeMetrics,
        evaluator_cfg: &EvaluatorSettings,
    ) {
        let ram_factor =
            evaluator::calculate_ram_capacity_factor(metrics.cpu_cores, metrics.memory_total_gb);
        let composite_benchmark_score = (self.benchmark_score as f64 * ram_factor).round() as u32;

        let mut surplus = evaluator::evaluate_surplus_capacity_with_config(
            &metrics,
            Some(self.benchmark_score),
            evaluator_cfg,
        );

        let is_paused = self.sharing_paused.load(Ordering::SeqCst);
        if is_paused {
            surplus.allocatable_cpu_score = Some(0);
            surplus.allocatable_memory_gb = Some(0.0);
            surplus.status_note = "Paused by user (一時停止中)".to_string();
        }

        let known_peers = self.peer_manager.get_all_peers().await;
        let tailscale = self.peer_manager.get_tailscale_status().await;
        let ram_per_core = metrics.memory_total_gb / (metrics.cpu_cores as f64).max(1.0);

        println!(
            "[{}] CPU: {:>4.1}% | MEM: {:>4.1}% ({:.1}/{:.1}GB) | Score: {} pt ({:.2}x, {:.1}GB/c) | Surplus: {:>5} pt{} | Peers: {} online",
            metrics.pc_name,
            metrics.cpu_percent,
            metrics.memory_percent,
            (metrics.memory_total_gb - metrics.memory_available_gb).max(0.0),
            metrics.memory_total_gb,
            composite_benchmark_score,
            ram_factor,
            ram_per_core,
            surplus.allocatable_cpu_score.unwrap_or(0),
            if is_paused { " [PAUSED]" } else { "" },
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
            cpu_benchmark_score: Some(composite_benchmark_score),
            surplus,
            peers: Some(known_peers),
            tailscale,
        };

        *self.local_telemetry.write().await = Some(telemetry.clone());

        if let Ok(telemetry_json) = serde_json::to_string(&telemetry) {
            *self.latest_metrics.write().await = Some(telemetry_json.clone());
            let _ = self.tx.send(telemetry_json);
        }
    }
}

async fn run_agent_backend(
    config: Arc<RwLock<AppConfig>>,
    sharing_paused: Arc<AtomicBool>,
    local_telemetry_holder: Arc<RwLock<Option<LocalAgentTelemetryMessage>>>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("[SpareLink] Initializing agent backend daemon...");

    let initial_config = config.read().await.clone();

    // 1. Benchmark evaluation
    let benchmark_score = if initial_config.benchmark.manual_score > 0 {
        println!(
            "[SpareLink] Manual benchmark override: {} pt",
            initial_config.benchmark.manual_score
        );
        initial_config.benchmark.manual_score
    } else if initial_config.benchmark.enabled {
        let detected_cores = bench::logical_cpu_cores();
        println!(
            "[SpareLink] Running benchmark across {} logical cores...",
            detected_cores
        );
        tokio::task::spawn_blocking(move || bench::run_quick_cpu_benchmark(detected_cores))
            .await
            .unwrap_or(10_000)
    } else {
        10_000
    };

    println!("[SpareLink] Benchmark score: {} pt", benchmark_score);

    // 2. Transport & Peer Discovery Provider initialization
    let transport_kind = match initial_config.discovery.transport.to_lowercase().as_str() {
        "wireguard" | "wg" => TransportKind::Wireguard,
        "static" => TransportKind::Static,
        "lan" => TransportKind::Lan,
        _ => TransportKind::Tailscale,
    };

    let discovery_config = DiscoveryConfig {
        transport: transport_kind,
        default_p2p_port: initial_config.agent.port,
        wireguard_config_path: initial_config.discovery.config_path.clone(),
        static_peers: initial_config.discovery.static_peers.clone(),
    };

    let discovery_provider = discovery::create_discovery_provider(&discovery_config);
    println!(
        "[SpareLink] Discovery provider: {} ({})",
        discovery_provider.name(),
        discovery_provider.transport_kind()
    );

    let local_id = if !initial_config.agent.node_name.is_empty() {
        initial_config.agent.node_name.clone()
    } else {
        std::env::var("COMPUTERNAME")
            .or_else(|_| std::env::var("HOSTNAME"))
            .unwrap_or_else(|_| "node".to_string())
            .to_lowercase()
    };

    let peer_manager = Arc::new(PeerManager::new(
        local_id.clone(),
        discovery_provider,
        local_telemetry_holder.clone(),
    ));

    let my_pc_name = initial_config.agent.node_name.clone();
    let job_runner = Arc::new(JobRunner::new(
        local_id.clone(),
        if my_pc_name.is_empty() {
            local_id
        } else {
            my_pc_name
        },
        peer_manager.clone(),
        config.clone(),
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
        initial_config.discovery.scan_interval_secs,
        initial_config.discovery.probe_interval_secs,
    );

    let latest_metrics = Arc::new(RwLock::new(None::<String>));
    let (tx, _rx) = broadcast::channel::<String>(32);

    // 3. Metrics collection probe spawn
    let hub = TelemetryHub {
        benchmark_score,
        config: config.clone(),
        peer_manager: peer_manager.clone(),
        local_telemetry: local_telemetry_holder,
        latest_metrics: latest_metrics.clone(),
        tx: tx.clone(),
        sharing_paused,
    };
    spawn_metrics_probe(hub);

    // 4. WebSocket & P2P server listener
    let ws_addr = format!("{}:{}", initial_config.agent.host, initial_config.agent.port);
    let listener = tokio::net::TcpListener::bind(&ws_addr).await?;
    println!("[SpareLink] WebSocket API & P2P listener active on ws://{}", ws_addr);

    // 5. Dedicated Web Dashboard HTTP Server (Port 5180)
    let config_for_web = config.clone();
    let job_runner_for_web = job_runner.clone();
    let peer_manager_for_web = peer_manager.clone();
    tokio::spawn(async move {
        web_server::run_dashboard_server(
            config::DASHBOARD_PORT,
            config_for_web,
            job_runner_for_web,
            peer_manager_for_web,
        )
        .await;
    });

    ws_server::serve(listener, peer_manager, job_runner, latest_metrics, tx).await;
    Ok(())
}

#[cfg(target_os = "windows")]
fn spawn_metrics_probe(hub: TelemetryHub) {
    spawn_native_probe_task(hub);
}

#[cfg(not(target_os = "windows"))]
fn spawn_metrics_probe(hub: TelemetryHub) {
    use std::path::PathBuf;
    use std::process::Stdio;
    use tokio::io::{AsyncBufReadExt, BufReader};
    use tokio::process::Command;

    let script_path = PathBuf::from("../python/main.py");
    if !script_path.exists() {
        spawn_native_probe_task(hub);
        return;
    }

    let python_path = [
        PathBuf::from("../../.venv/bin/python"),
        PathBuf::from("../../../.venv/bin/python"),
        PathBuf::from(".venv/bin/python"),
    ]
    .into_iter()
    .find(|p| p.exists())
    .unwrap_or_else(|| PathBuf::from("python3"));

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
                            let evaluator_cfg = hub.config.read().await.evaluator.clone();
                            hub.process_and_broadcast(metrics, &evaluator_cfg).await;
                        }
                    }
                });
            }
        }
        Err(e) => {
            eprintln!("[SpareLink] Python probe launch error ({}). Falling back to native probe.", e);
            spawn_native_probe_task(hub);
        }
    }
}

fn spawn_native_probe_task(hub: TelemetryHub) {
    tokio::spawn(async move {
        let mut probe = native_probe::NativeWindowsProbe::new();
        loop {
            let (evaluator_cfg, broadcast_interval_ms) = {
                let lock = hub.config.read().await;
                (lock.evaluator.clone(), lock.agent.broadcast_interval_ms)
            };
            let interval = Duration::from_millis(broadcast_interval_ms.max(500));
            tokio::time::sleep(interval).await;
            let metrics = probe.collect_metrics();
            hub.process_and_broadcast(metrics, &evaluator_cfg).await;
        }
    });
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    updater::cleanup_old_binaries();
    if let Ok(true) = updater::ensure_local_execution_if_needed() {
        return Ok(());
    }

    let config = Arc::new(RwLock::new(AppConfig::load()));
    let headless = std::env::args().any(|a| a == "--headless" || a == "--no-gui");

    let sharing_paused = Arc::new(AtomicBool::new(false));
    let local_telemetry_holder = Arc::new(RwLock::new(None::<LocalAgentTelemetryMessage>));

    if headless {
        println!("[SpareLink] Running in headless mode (no GUI).");
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;
        rt.block_on(run_agent_backend(config, sharing_paused, local_telemetry_holder))?;
    } else {
        println!("[SpareLink] Launching native monitor GUI widget...");
        let backend_config = config.clone();
        let backend_paused = sharing_paused.clone();
        let backend_telemetry = local_telemetry_holder.clone();

        std::thread::spawn(move || {
            let rt = match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    eprintln!("[SpareLink] Failed to build Tokio runtime: {}", e);
                    return;
                }
            };

            if let Err(e) = rt.block_on(run_agent_backend(backend_config, backend_paused, backend_telemetry)) {
                eprintln!("[SpareLink] Agent backend error: {}", e);
            }
        });

        let initial_pos = gui::get_bottom_right_position(360.0, 470.0);
        let options = eframe::NativeOptions {
            viewport: eframe::egui::ViewportBuilder::default()
                .with_inner_size([360.0, 470.0])
                .with_min_inner_size([330.0, 380.0])
                .with_position(initial_pos)
                .with_title("SpareLink Monitor")
                .with_always_on_top()
                .with_decorations(false)
                .with_resizable(true),
            ..Default::default()
        };

        if let Err(e) = eframe::run_native(
            "SpareLink Monitor",
            options,
            Box::new(move |cc| {
                gui::SpareLinkMonitorApp::configure_theme(&cc.egui_ctx, true);
                Ok(Box::new(gui::SpareLinkMonitorApp::new(
                    local_telemetry_holder,
                    sharing_paused,
                )))
            }),
        ) {
            eprintln!("[SpareLink] GUI runtime error: {}", e);
        }

        std::process::exit(0);
    }

    Ok(())
}
