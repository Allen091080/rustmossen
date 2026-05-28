//! Public types for SendMessageTool — Rust mirror of TS exports.

use serde::{Deserialize, Serialize};

/// `SendMessageTool.ts` `Input`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SendMessageInput {
    pub action: String,             // "send" | "broadcast" | "request" | "respond"
    pub target: Option<String>,     // teammate name
    pub message: serde_json::Value, // string | StructuredMessage
    pub request_id: Option<String>,
}

/// `SendMessageTool.ts` `MessageRouting`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MessageRouting {
    pub sender: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sender_color: Option<String>,
    pub target: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

/// `SendMessageTool.ts` `MessageOutput`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MessageOutput {
    pub success: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub routing: Option<MessageRouting>,
}

/// `SendMessageTool.ts` `BroadcastOutput`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BroadcastOutput {
    pub success: bool,
    pub message: String,
    pub recipients: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub routing: Option<MessageRouting>,
}

/// `SendMessageTool.ts` `RequestOutput`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RequestOutput {
    pub success: bool,
    pub message: String,
    pub request_id: String,
    pub target: String,
}

/// `SendMessageTool.ts` `ResponseOutput`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResponseOutput {
    pub success: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

/// `SendMessageTool.ts` `SendMessageToolOutput`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SendMessageToolOutput {
    Message(MessageOutput),
    Broadcast(BroadcastOutput),
    Request(RequestOutput),
    Response(ResponseOutput),
}

/// `SendMessageTool.ts` `SendMessageTool` — value-shape marker that ports
/// of the TS const can reference.
#[derive(Debug, Clone, Default)]
pub struct SendMessageTool {
    pub name: String,
}

impl SendMessageTool {
    pub const TOOL_NAME: &'static str = "SendMessage";
    pub fn new() -> Self {
        Self {
            name: Self::TOOL_NAME.to_string(),
        }
    }
}
