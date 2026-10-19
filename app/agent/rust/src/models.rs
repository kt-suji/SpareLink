use serde::{Deserialize, Serialize};

/** Python監視プローブから標準出力経由で受け取る実測システム情報 */
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeMetrics {
    pub pc_name: String,
    pub os: String,
    pub cpu_cores: u32,
    pub cpu_percent: f64,
    pub memory_percent: f64,
    pub memory_total_gb: f64,
    pub memory_available_gb: f64,
}

/** 余剰計算資源の評価ステータス */
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurplusEvaluationStatus {
    Evaluated,
    Evaluating,
    InsufficientData,
    Unsupported,
}

/** 余剰計算資源の評価結果 */
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurplusCapacity {
    pub status: SurplusEvaluationStatus,
    pub confidence: Option<f64>,
    pub allocatable_cpu_score: Option<u32>,
    pub allocatable_memory_gb: Option<f64>,
    pub algorithm_version: Option<String>,
    pub status_note: String,
}

/** 各ピアノードの動的追跡状態 */
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerNodeState {
    pub id: String,
    pub name: String,
    pub address: String,
    pub os: Option<String>,
    pub status: String, // "online" | "offline"
    pub latency_ms: Option<u64>,
    pub last_seen_secs_ago: Option<u64>,
    pub cpu_cores: Option<u32>,
    pub cpu_percent: Option<f64>,
    pub memory_percent: Option<f64>,
    pub memory_total_gb: Option<f64>,
    pub memory_available_gb: Option<f64>,
    pub cpu_benchmark_score: Option<u32>,
    pub surplus: SurplusCapacity,
    pub transport: String,
}

/** Tailscale の稼働ステータス */
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TailscaleState {
    Running,
    NeedsLogin,
    Stopped,
    NotInstalled,
    Error,
}

/** Tailscale 連携状態テレメトリ */
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TailscaleTelemetry {
    pub state: TailscaleState,
    pub self_ip: Option<String>,
    pub self_hostname: Option<String>,
    pub auth_url: Option<String>,
    pub message: String,
}

/** フロントエンドへWebSocket配信する自PCの統合ステータスメッセージ */
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalAgentTelemetryMessage {
    pub pc_name: String,
    pub os: String,
    pub cpu_cores: u32,
    pub cpu_percent: f64,
    pub memory_percent: f64,
    pub memory_total_gb: f64,
    pub memory_available_gb: f64,
    pub cpu_benchmark_score: Option<u32>,
    pub surplus: SurplusCapacity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peers: Option<Vec<PeerNodeState>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tailscale: Option<TailscaleTelemetry>,
}
