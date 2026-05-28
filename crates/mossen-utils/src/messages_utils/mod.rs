//! Messages utilities — translated from utils/messages/mappers.ts and utils/messages/systemInit.ts

use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

// --- Types ---

/// SDK message type discriminator
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SdkMessageType {
    Assistant,
    User,
    System,
}

/// SDK message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkMessage {
    #[serde(rename = "type")]
    pub msg_type: SdkMessageType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtype: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compact_metadata: Option<SdkCompactMetadata>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

/// Internal message type
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Message {
    #[serde(rename = "assistant")]
    Assistant(AssistantMessage),
    #[serde(rename = "user")]
    User(UserMessage),
    #[serde(rename = "system")]
    System(SystemMessage),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantMessage {
    pub message: Value,
    pub uuid: String,
    pub request_id: Option<String>,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMessage {
    pub message: Value,
    pub uuid: String,
    pub timestamp: String,
    #[serde(default)]
    pub is_meta: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_visible_in_transcript_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_use_result: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMessage {
    pub content: String,
    pub level: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtype: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compact_metadata: Option<CompactMetadata>,
    pub uuid: String,
    pub timestamp: String,
}

/// Compact metadata for conversation compaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactMetadata {
    pub trigger: String,
    pub pre_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preserved_segment: Option<PreservedSegment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreservedSegment {
    pub head_uuid: String,
    pub anchor_uuid: String,
    pub tail_uuid: String,
}

/// SDK compact metadata (wire format)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkCompactMetadata {
    pub trigger: String,
    pub pre_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preserved_segment: Option<SdkPreservedSegment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkPreservedSegment {
    pub head_uuid: String,
    pub anchor_uuid: String,
    pub tail_uuid: String,
}

/// SDK rate limit info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkRateLimitInfo {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limit_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub utilization: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overage_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overage_resets_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overage_disabled_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_using_overage: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub surpassed_threshold: Option<f64>,
}

/// Hosted limits (internal representation)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostedLimits {
    pub status: String,
    pub resets_at: Option<String>,
    pub rate_limit_type: Option<String>,
    pub utilization: Option<f64>,
    pub overage_status: Option<String>,
    pub overage_resets_at: Option<String>,
    pub overage_disabled_reason: Option<String>,
    pub is_using_overage: Option<bool>,
    pub surpassed_threshold: Option<f64>,
}

// --- Conversion Functions ---

/// Convert SDK compact metadata to internal format
pub fn from_sdk_compact_metadata(meta: &SdkCompactMetadata) -> CompactMetadata {
    let preserved_segment = meta.preserved_segment.as_ref().map(|seg| PreservedSegment {
        head_uuid: seg.head_uuid.clone(),
        anchor_uuid: seg.anchor_uuid.clone(),
        tail_uuid: seg.tail_uuid.clone(),
    });
    CompactMetadata {
        trigger: meta.trigger.clone(),
        pre_tokens: meta.pre_tokens,
        preserved_segment,
    }
}

/// Convert internal compact metadata to SDK format
pub fn to_sdk_compact_metadata(meta: &CompactMetadata) -> SdkCompactMetadata {
    let preserved_segment = meta
        .preserved_segment
        .as_ref()
        .map(|seg| SdkPreservedSegment {
            head_uuid: seg.head_uuid.clone(),
            anchor_uuid: seg.anchor_uuid.clone(),
            tail_uuid: seg.tail_uuid.clone(),
        });
    SdkCompactMetadata {
        trigger: meta.trigger.clone(),
        pre_tokens: meta.pre_tokens,
        preserved_segment,
    }
}

/// Convert internal messages to SDK messages
pub fn to_sdk_messages(messages: &[Message], session_id: &str) -> Vec<SdkMessage> {
    let mut result = Vec::new();
    for message in messages {
        match message {
            Message::Assistant(msg) => {
                let mut extra = serde_json::Map::new();
                extra.insert("parent_tool_use_id".to_string(), Value::Null);
                if let Some(err) = &msg.error {
                    extra.insert("error".to_string(), err.clone());
                }
                result.push(SdkMessage {
                    msg_type: SdkMessageType::Assistant,
                    message: Some(msg.message.clone()),
                    subtype: None,
                    uuid: Some(msg.uuid.clone()),
                    session_id: Some(session_id.to_string()),
                    timestamp: None,
                    compact_metadata: None,
                    extra,
                });
            }
            Message::User(msg) => {
                let mut extra = serde_json::Map::new();
                extra.insert("parent_tool_use_id".to_string(), Value::Null);
                let is_synthetic =
                    msg.is_meta || msg.is_visible_in_transcript_only.unwrap_or(false);
                extra.insert("isSynthetic".to_string(), Value::Bool(is_synthetic));
                if let Some(tool_result) = &msg.tool_use_result {
                    extra.insert("tool_use_result".to_string(), tool_result.clone());
                }
                result.push(SdkMessage {
                    msg_type: SdkMessageType::User,
                    message: Some(msg.message.clone()),
                    subtype: None,
                    uuid: Some(msg.uuid.clone()),
                    session_id: Some(session_id.to_string()),
                    timestamp: Some(msg.timestamp.clone()),
                    compact_metadata: None,
                    extra,
                });
            }
            Message::System(msg) => {
                if msg.subtype.as_deref() == Some("compact_boundary") {
                    if let Some(meta) = &msg.compact_metadata {
                        result.push(SdkMessage {
                            msg_type: SdkMessageType::System,
                            message: None,
                            subtype: Some("compact_boundary".to_string()),
                            uuid: Some(msg.uuid.clone()),
                            session_id: Some(session_id.to_string()),
                            timestamp: None,
                            compact_metadata: Some(to_sdk_compact_metadata(meta)),
                            extra: serde_json::Map::new(),
                        });
                    }
                } else if msg.subtype.as_deref() == Some("local_command")
                    && (msg.content.contains("<local-command-stdout>")
                        || msg.content.contains("<local-command-stderr>"))
                {
                    let sdk_msg = local_command_output_to_sdk_assistant_message(
                        &msg.content,
                        &msg.uuid,
                        session_id,
                    );
                    result.push(sdk_msg);
                }
            }
        }
    }
    result
}

/// Convert SDK messages to internal messages
pub fn to_internal_messages(messages: &[SdkMessage]) -> Vec<Message> {
    let mut result = Vec::new();
    for message in messages {
        match message.msg_type {
            SdkMessageType::Assistant => {
                result.push(Message::Assistant(AssistantMessage {
                    message: message.message.clone().unwrap_or(Value::Null),
                    uuid: message
                        .uuid
                        .clone()
                        .unwrap_or_else(|| Uuid::new_v4().to_string()),
                    request_id: None,
                    timestamp: chrono::Utc::now().to_rfc3339(),
                    error: None,
                }));
            }
            SdkMessageType::User => {
                let is_synthetic = message
                    .extra
                    .get("isSynthetic")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                result.push(Message::User(UserMessage {
                    message: message.message.clone().unwrap_or(Value::Null),
                    uuid: message
                        .uuid
                        .clone()
                        .unwrap_or_else(|| Uuid::new_v4().to_string()),
                    timestamp: message
                        .timestamp
                        .clone()
                        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
                    is_meta: is_synthetic,
                    is_visible_in_transcript_only: None,
                    tool_use_result: None,
                }));
            }
            SdkMessageType::System => {
                if message.subtype.as_deref() == Some("compact_boundary") {
                    if let Some(meta) = &message.compact_metadata {
                        result.push(Message::System(SystemMessage {
                            content: "Conversation compacted".to_string(),
                            level: "info".to_string(),
                            subtype: Some("compact_boundary".to_string()),
                            compact_metadata: Some(from_sdk_compact_metadata(meta)),
                            uuid: message
                                .uuid
                                .clone()
                                .unwrap_or_else(|| Uuid::new_v4().to_string()),
                            timestamp: chrono::Utc::now().to_rfc3339(),
                        }));
                    }
                }
            }
        }
    }
    result
}

static STDOUT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"<local-command-stdout>([\s\S]*?)</local-command-stdout>").unwrap());

static STDERR_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"<local-command-stderr>([\s\S]*?)</local-command-stderr>").unwrap());

/// Converts local command output to a well-formed SDKAssistantMessage
/// Strips ANSI then unwraps the XML wrapper tags.
pub fn local_command_output_to_sdk_assistant_message(
    raw_content: &str,
    uuid: &str,
    session_id: &str,
) -> SdkMessage {
    // Strip ANSI escape codes
    let clean = strip_ansi_escapes::strip_str(raw_content);
    let clean = STDOUT_RE.replace_all(&clean, "$1");
    let clean = STDERR_RE.replace_all(&clean, "$1");
    let clean_content = clean.trim().to_string();

    let message = serde_json::json!({
        "content": clean_content,
        "model": "synthetic",
        "role": "assistant",
        "stop_reason": "end_turn",
    });

    let mut extra = serde_json::Map::new();
    extra.insert("parent_tool_use_id".to_string(), Value::Null);

    SdkMessage {
        msg_type: SdkMessageType::Assistant,
        message: Some(message),
        subtype: None,
        uuid: Some(uuid.to_string()),
        session_id: Some(session_id.to_string()),
        timestamp: None,
        compact_metadata: None,
        extra,
    }
}

/// Maps internal HostedLimits to the SDK-facing SDKRateLimitInfo type
pub fn to_sdk_rate_limit_info(limits: Option<&HostedLimits>) -> Option<SdkRateLimitInfo> {
    let limits = limits?;
    Some(SdkRateLimitInfo {
        status: limits.status.clone(),
        resets_at: limits.resets_at.clone(),
        rate_limit_type: limits.rate_limit_type.clone(),
        utilization: limits.utilization,
        overage_status: limits.overage_status.clone(),
        overage_resets_at: limits.overage_resets_at.clone(),
        overage_disabled_reason: limits.overage_disabled_reason.clone(),
        is_using_overage: limits.is_using_overage,
        surpassed_threshold: limits.surpassed_threshold,
    })
}

// --- System Init ---

/// SDK compat tool name: maps new Agent tool name back to legacy Task name
pub fn sdk_compat_tool_name(name: &str) -> &str {
    const AGENT_TOOL_NAME: &str = "Agent";
    const LEGACY_AGENT_TOOL_NAME: &str = "Task";
    if name == AGENT_TOOL_NAME {
        LEGACY_AGENT_TOOL_NAME
    } else {
        name
    }
}

/// Permission mode for SDK init
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionMode {
    Default,
    Plan,
    AutoEdit,
    FullAuto,
    BypassPermissions,
}

/// Inputs for building the system init message
pub struct SystemInitInputs {
    pub tools: Vec<String>,
    pub mcp_clients: Vec<McpClientInfo>,
    pub model: String,
    pub permission_mode: PermissionMode,
    pub commands: Vec<CommandInfo>,
    pub agents: Vec<String>,
    pub skills: Vec<CommandInfo>,
    pub plugins: Vec<PluginInfo>,
    pub fast_mode: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct McpClientInfo {
    pub name: String,
    pub client_type: String,
}

#[derive(Debug, Clone)]
pub struct CommandInfo {
    pub name: String,
    pub user_invocable: bool,
}

#[derive(Debug, Clone)]
pub struct PluginInfo {
    pub name: String,
    pub path: String,
    pub source: String,
}

/// Build the `system/init` SDKMessage — the first message on the SDK stream
pub fn build_system_init_message(
    inputs: &SystemInitInputs,
    session_id: &str,
    cwd: &str,
    version: &str,
) -> SdkMessage {
    let tools: Vec<Value> = inputs
        .tools
        .iter()
        .map(|t| Value::String(sdk_compat_tool_name(t).to_string()))
        .collect();

    let mcp_servers: Vec<Value> = inputs
        .mcp_clients
        .iter()
        .map(|c| {
            serde_json::json!({
                "name": c.name,
                "status": c.client_type,
            })
        })
        .collect();

    let slash_commands: Vec<Value> = inputs
        .commands
        .iter()
        .filter(|c| c.user_invocable)
        .map(|c| Value::String(c.name.clone()))
        .collect();

    let skills: Vec<Value> = inputs
        .skills
        .iter()
        .filter(|s| s.user_invocable)
        .map(|s| Value::String(s.name.clone()))
        .collect();

    let plugins: Vec<Value> = inputs
        .plugins
        .iter()
        .map(|p| {
            serde_json::json!({
                "name": p.name,
                "path": p.path,
                "source": p.source,
            })
        })
        .collect();

    let agents: Vec<Value> = inputs
        .agents
        .iter()
        .map(|a| Value::String(a.clone()))
        .collect();

    let mut extra = serde_json::Map::new();
    extra.insert("cwd".to_string(), Value::String(cwd.to_string()));
    extra.insert("tools".to_string(), Value::Array(tools));
    extra.insert("mcp_servers".to_string(), Value::Array(mcp_servers));
    extra.insert("model".to_string(), Value::String(inputs.model.clone()));
    extra.insert(
        "permissionMode".to_string(),
        serde_json::to_value(&inputs.permission_mode).unwrap_or(Value::Null),
    );
    extra.insert("slash_commands".to_string(), Value::Array(slash_commands));
    extra.insert(
        "mossen_code_version".to_string(),
        Value::String(version.to_string()),
    );
    extra.insert("agents".to_string(), Value::Array(agents));
    extra.insert("skills".to_string(), Value::Array(skills));
    extra.insert("plugins".to_string(), Value::Array(plugins));

    SdkMessage {
        msg_type: SdkMessageType::System,
        message: None,
        subtype: Some("init".to_string()),
        uuid: Some(Uuid::new_v4().to_string()),
        session_id: Some(session_id.to_string()),
        timestamp: None,
        compact_metadata: None,
        extra,
    }
}
