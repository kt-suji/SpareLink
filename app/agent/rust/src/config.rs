use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;

/// 専用 Web ダッシュボード＆ファイルステージング HTTP ポート
pub const DASHBOARD_PORT: u16 = 5180;

/// ダッシュボードおよびローカル API のベース URL
pub fn dashboard_url() -> String {
    format!("http://localhost:{}", DASHBOARD_PORT)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub agent: AgentConfig,
    pub discovery: DiscoverySettings,
    pub evaluator: EvaluatorSettings,
    pub benchmark: BenchmarkSettings,
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

    /// Finds candidate paths for sparelink.toml.
    pub fn find_config_file_path() -> Option<PathBuf> {
        let candidate_paths = vec![
            env::var("SPARELINK_CONFIG").ok().map(PathBuf::from),
            Some(PathBuf::from("sparelink.toml")),
            Some(PathBuf::from("../sparelink.toml")),
            Some(PathBuf::from("../../sparelink.toml")),
            env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.join("sparelink.toml"))),
        ];

        candidate_paths.into_iter().flatten().find(|p| p.is_file())
    }

    fn find_and_parse_config() -> Option<Self> {
        if let Some(path) = Self::find_config_file_path()
            && let Ok(content) = fs::read_to_string(&path)
        {
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
        None
    }

    /// Saves the current configuration to sparelink.toml.
    pub fn save_to_file(&self) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
        let path = Self::find_config_file_path().unwrap_or_else(|| PathBuf::from("sparelink.toml"));
        let toml_str = toml::to_string_pretty(self)?;
        fs::write(&path, toml_str)?;
        println!("[SpareLink] Saved configuration to {}", path.display());
        Ok(path)
    }

    fn apply_env_overrides(&mut self) {
        if let Ok(port_str) = env::var("SPARELINK_PORT")
            && let Ok(p) = port_str.parse::<u16>()
        {
            self.agent.port = p;
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
}
