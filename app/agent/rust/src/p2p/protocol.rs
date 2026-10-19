use serde::{Deserialize, Serialize};
use crate::models::LocalAgentTelemetryMessage;

/**
 * P2Pノード間で交換されるメッセージ定義
 */
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum P2PMessage {
    /// 疎通確認・最新テレメトリの送信
    Ping {
        sender_id: String,
        sender_name: String,
        timestamp_ms: u64,
        telemetry: LocalAgentTelemetryMessage,
    },
    /// Pingに対する応答・自PCテレメトリの返信
    Pong {
        sender_id: String,
        sender_name: String,
        timestamp_ms: u64,
        telemetry: LocalAgentTelemetryMessage,
    },
}

pub use crate::models::PeerNodeState;
