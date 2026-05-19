//! Mossen Agent SDK permission mode type.
//! Translated from `services/api/mossenAgentSdk.ts` (9 lines).

use serde::{Deserialize, Serialize};

/// Mossen-owned mirror for the public agent SDK permission mode type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MossenAgentPermissionMode {
    Default,
    AcceptEdits,
    BypassPermissions,
    Plan,
    DontAsk,
    Auto,
}

impl MossenAgentPermissionMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::AcceptEdits => "acceptEdits",
            Self::BypassPermissions => "bypassPermissions",
            Self::Plan => "plan",
            Self::DontAsk => "dontAsk",
            Self::Auto => "auto",
        }
    }
}

impl std::fmt::Display for MossenAgentPermissionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// TS `type MossenStream` — opaque streaming-response handle returned by the
/// SDK; the Rust port models it as an async-stream of JSON values.
pub type MossenStream =
    std::pin::Pin<Box<dyn futures::Stream<Item = serde_json::Value> + Send>>;
