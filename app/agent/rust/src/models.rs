use std::collections::HashMap;
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peer_latencies: Option<HashMap<String, u64>>,
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

/** 分散タスクの実行ステータス */
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/** 分散計算ジョブモデル */
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputeJob {
    pub id: String,
    pub name: String,
    pub command: String,
    #[serde(alias = "jobType")]
    pub job_type: String, // "command" | "python"
    #[serde(alias = "targetNodeId")]
    pub target_node_id: String, // "auto" または特定のノードID
    #[serde(default, alias = "assignedNodeId")]
    pub assigned_node_id: Option<String>,
    #[serde(default, alias = "assignedNodeName")]
    pub assigned_node_name: Option<String>,
    #[serde(default, alias = "requesterNodeId")]
    pub requester_node_id: String,
    #[serde(default, alias = "requesterNodeName")]
    pub requester_node_name: String,
    #[serde(default = "default_pending_status")]
    pub status: JobStatus,
    #[serde(default, alias = "submittedAt")]
    pub submitted_at: String,
    #[serde(default, alias = "startedAt")]
    pub started_at: Option<String>,
    #[serde(default, alias = "completedAt")]
    pub completed_at: Option<String>,
    #[serde(default, alias = "exitCode")]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub stdout: String,
    #[serde(default)]
    pub stderr: String,
}

fn default_pending_status() -> JobStatus {
    JobStatus::Pending
}
