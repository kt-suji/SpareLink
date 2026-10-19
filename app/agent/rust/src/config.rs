use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub agent: AgentConfig,
    pub discovery: DiscoverySettings,
    pub evaluator: EvaluatorSettings,
    pub benchmark: BenchmarkSettings,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            agent: AgentConfig::default(),
            discovery: DiscoverySettings::default(),
            evaluator: EvaluatorSettings::default(),
            benchmark: BenchmarkSettings::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentConfig {
    /// Node identification name. Empty defaults to host machine name.
    pub node_name: String,
    /// Listen address for local clients and P2P communication.
    pub host: String,
    /// WebSocket listen port.
    pub port: u16,
    /// Telemetry broadcast interval in milliseconds.
    pub broadcast_interval_ms: u64,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            node_name: String::new(),
            host: "0.0.0.0".to_string(),
            port: 3001,
            broadcast_interval_ms: 2000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DiscoverySettings {
    /// Transport mechanism: "tailscale" | "wireguard" | "static" | "lan"
    pub transport: String,
    /// Path to custom peer definition file.
    pub config_path: Option<String>,
    /// Background discovery peer scan interval in seconds.
    pub scan_interval_secs: u64,
    /// P2P ping probe interval in seconds.
    pub probe_interval_secs: u64,
    /// Static peer endpoint list ("IP:Port" or "IP").
    pub static_peers: Vec<String>,
}

impl Default for DiscoverySettings {
    fn default() -> Self {
        Self {
            transport: "tailscale".to_string(),
            config_path: None,
            scan_interval_secs: 10,
            probe_interval_secs: 4,
            static_peers: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EvaluatorSettings {
    /// CPU usage percentage threshold for zero surplus allocation.
    pub high_load_threshold: f64,
    /// Safety CPU headroom percentage reserved for host.
    pub safety_headroom: f64,
    /// Minimum physical RAM reserved for host OS in gigabytes.
    pub min_memory_margin_gb: f64,
}

impl Default for EvaluatorSettings {
    fn default() -> Self {
        Self {
            high_load_threshold: 80.0,
            safety_headroom: 20.0,
            min_memory_margin_gb: 4.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BenchmarkSettings {
    /// Automatically run CPU micro-benchmark on startup.
    pub enabled: bool,
    /// Number of iterations per core (default 20,000,000).
    pub iterations_per_core: usize,
    /// Manual score override (0 = auto-measure).
    pub manual_score: u32,
}

impl Default for BenchmarkSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            iterations_per_core: 20_000_000,
            manual_score: 0,
        }
    }
}

impl AppConfig {
    /// Loads configuration from sparelink.toml, falling back to defaults.
    /// Environment variables (SPARELINK_*) take precedence.
    pub fn load() -> Self {
        let mut config = Self::find_and_parse_config().unwrap_or_default();
        config.apply_env_overrides();
        config
    }

    fn find_and_parse_config() -> Option<Self> {
        let candidate_paths = vec![
            env::var("SPARELINK_CONFIG").ok().map(PathBuf::from),
            Some(PathBuf::from("sparelink.toml")),
            Some(PathBuf::from("../sparelink.toml")),
            Some(PathBuf::from("../../sparelink.toml")),
            env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.join("sparelink.toml"))),
        ];

        for path in candidate_paths.into_iter().flatten() {
            if path.is_file() {
                if let Ok(content) = fs::read_to_string(&path) {
                    match toml::from_str::<AppConfig>(&content) {
                        Ok(parsed) => {
                            println!("[SpareLink] Loaded config from {}", path.display());
                            return Some(parsed);
                        }
                        Err(e) => {
                            eprintln!("[SpareLink] Warning: Failed to parse {}: {}", path.display(), e);
                        }
                    }
                }
            }
        }

        None
    }

    fn apply_env_overrides(&mut self) {
        if let Ok(port_str) = env::var("SPARELINK_PORT") {
            if let Ok(p) = port_str.parse::<u16>() {
                self.agent.port = p;
            }
        }
        if let Ok(transport) = env::var("SPARELINK_TRANSPORT") {
            self.discovery.transport = transport;
        }
        if let Ok(config_path) = env::var("SPARELINK_CONFIG_PATH") {
            self.discovery.config_path = Some(config_path);
        }
        if let Ok(node_name) = env::var("SPARELINK_NODE_NAME") {
            self.agent.node_name = node_name;
        }
    }

    /// Generates a default sparelink.toml template string.
    #[allow(dead_code)]
    pub fn default_template() -> &'static str {
        r#"# SpareLink 統合設定ファイル (sparelink.toml)
# ドキュメント: https://github.com/SpareLink/sparelink

[agent]
# ノード表示名（空文字の場合はホスト名を自動検出）
node_name = ""
# 待受ホストIPアドレス
host = "0.0.0.0"
# 待受ポート番号
port = 3001
# ローカル実測値の配信間隔 (ミリ秒)
broadcast_interval_ms = 2000

[discovery]
# ピア探索方式 ("tailscale" | "wireguard" | "static" | "lan")
transport = "tailscale"
# 外部ピア一覧JSONファイルのパス（wireguard/staticモード用、省略可）
# config_path = "wireguard_peers.json"
# ピアの自動スキャン間隔 (秒)
scan_interval_secs = 10
# P2Pハートビート（応答遅延・余剰資源プローブ）の間隔 (秒)
probe_interval_secs = 4
# 固定ピアの指定（例: ["100.64.0.2:3001", "192.168.1.50:3001"]）
static_peers = []

[evaluator]
# 余剰割当を0ptとして安全停止するCPU使用率閾値 (%)
high_load_threshold = 80.0
# ホストPCの操作快適性を保つためのCPU安全マージン (%)
safety_headroom = 20.0
# OS安定稼働のために常時確保する空き物理メモリ (GB)
min_memory_margin_gb = 4.0

[benchmark]
# 起動時にCPUベンチマークを自動実行するかどうか
enabled = true
# 1コアあたりの演算反復回数（デフォルト: 20000000）
iterations_per_core = 20000000
# スコア手動固定値（0 で自動計測）
manual_score = 0
"#
    }
}
