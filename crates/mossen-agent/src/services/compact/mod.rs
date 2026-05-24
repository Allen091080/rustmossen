//! Compact service — conversation compaction, microcompaction, and session memory compaction.

pub mod api_microcompact;
pub mod auto_compact;
pub mod compact;
pub mod compact_warning_hook;
pub mod compact_warning_state;
pub mod grouping;
pub mod micro_compact;
pub mod pending_compact_request;
pub mod post_compact_cleanup;
pub mod prompt;
pub mod session_memory_compact;
pub mod time_based_mc_config;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Internal Message representation used by the compact module.
/// Maps the TS transcript message shape used throughout compaction logic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    #[serde(rename = "type", default)]
    pub msg_type: String,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub text_content: Option<String>,
    #[serde(default)]
    pub content_blocks: Option<Vec<Value>>,
    #[serde(default)]
    pub message_id: Option<String>,
    #[serde(default)]
    pub compact_metadata: Option<Value>,
    #[serde(default)]
    pub is_meta: Option<bool>,
    #[serde(default)]
    pub uuid: Option<String>,
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, Value>,
}
