//! Teammate Mailbox — File-based messaging system for agent swarms.
//!
//! Each teammate has an inbox file at `.mossen/teams/{team_name}/inboxes/{agent_name}.json`.
//! Other teammates can write messages to it, and the recipient sees them as attachments.

use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::fs;

// --------------------------------------------------------------------------
// Constants
// --------------------------------------------------------------------------

const TEAMMATE_MESSAGE_TAG: &str = "teammate_message";
const TEAM_LEAD_NAME: &str = "leader";

/// Lock retry options for concurrent file access.
const MAX_LOCK_RETRIES: usize = 10;
const LOCK_RETRY_MIN_MS: u64 = 5;
const LOCK_RETRY_MAX_MS: u64 = 100;

// --------------------------------------------------------------------------
// Types
// --------------------------------------------------------------------------

/// A message stored in a teammate's inbox.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeammateMessage {
    pub from: String,
    pub text: String,
    pub timestamp: String,
    pub read: bool,
    /// Sender's assigned color (e.g., "red", "blue", "green").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// 5–10 word summary shown as preview in the UI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

/// Structured message sent when a teammate becomes idle (via Stop hook).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdleNotificationMessage {
    #[serde(rename = "type")]
    pub msg_type: String, // "idle_notification"
    pub from: String,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idle_reason: Option<IdleReason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
}

/// Why the agent went idle.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IdleReason {
    Available,
    Interrupted,
    Failed,
}

/// Permission request message sent from worker to leader via mailbox.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRequestMessage {
    #[serde(rename = "type")]
    pub msg_type: String, // "permission_request"
    pub request_id: String,
    pub agent_id: String,
    pub tool_name: String,
    pub tool_use_id: String,
    pub description: String,
    pub input: serde_json::Value,
    pub permission_suggestions: Vec<serde_json::Value>,
}

/// Permission response message sent from leader to worker via mailbox.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "subtype")]
pub enum PermissionResponseMessage {
    #[serde(rename = "success")]
    Success {
        #[serde(rename = "type")]
        msg_type: String,
        request_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        response: Option<PermissionResponseData>,
    },
    #[serde(rename = "error")]
    Error {
        #[serde(rename = "type")]
        msg_type: String,
        request_id: String,
        error: String,
    },
}

/// Data in a successful permission response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionResponseData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_input: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_updates: Option<Vec<serde_json::Value>>,
}

/// Sandbox permission request message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxPermissionRequestMessage {
    #[serde(rename = "type")]
    pub msg_type: String, // "sandbox_permission_request"
    #[serde(rename = "requestId")]
    pub request_id: String,
    #[serde(rename = "workerId")]
    pub worker_id: String,
    #[serde(rename = "workerName")]
    pub worker_name: String,
    #[serde(rename = "workerColor")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worker_color: Option<String>,
    #[serde(rename = "hostPattern")]
    pub host_pattern: HostPattern,
    #[serde(rename = "createdAt")]
    pub created_at: u64,
}

/// Host pattern for sandbox permission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostPattern {
    pub host: String,
}

/// Sandbox permission response message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxPermissionResponseMessage {
    #[serde(rename = "type")]
    pub msg_type: String, // "sandbox_permission_response"
    #[serde(rename = "requestId")]
    pub request_id: String,
    pub host: String,
    pub allow: bool,
    pub timestamp: String,
}

/// Shutdown request message sent from leader to teammate via mailbox.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShutdownRequestMessage {
    #[serde(rename = "type")]
    pub msg_type: String, // "shutdown_request"
    #[serde(rename = "requestId")]
    pub request_id: String,
    pub from: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub timestamp: String,
}

/// Shutdown approved message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShutdownApprovedMessage {
    #[serde(rename = "type")]
    pub msg_type: String, // "shutdown_approved"
    #[serde(rename = "requestId")]
    pub request_id: String,
    pub from: String,
    pub timestamp: String,
    #[serde(rename = "paneId")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pane_id: Option<String>,
    #[serde(rename = "backendType")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend_type: Option<String>,
}

/// Shutdown rejected message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShutdownRejectedMessage {
    #[serde(rename = "type")]
    pub msg_type: String, // "shutdown_rejected"
    #[serde(rename = "requestId")]
    pub request_id: String,
    pub from: String,
    pub reason: String,
    pub timestamp: String,
}

/// Plan approval request message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanApprovalRequestMessage {
    #[serde(rename = "type")]
    pub msg_type: String, // "plan_approval_request"
    pub from: String,
    pub timestamp: String,
    #[serde(rename = "planFilePath")]
    pub plan_file_path: String,
    #[serde(rename = "planContent")]
    pub plan_content: String,
    #[serde(rename = "requestId")]
    pub request_id: String,
}

/// Plan approval response message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanApprovalResponseMessage {
    #[serde(rename = "type")]
    pub msg_type: String, // "plan_approval_response"
    #[serde(rename = "requestId")]
    pub request_id: String,
    pub approved: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feedback: Option<String>,
    pub timestamp: String,
    #[serde(rename = "permissionMode")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<String>,
}

/// Task assignment message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskAssignmentMessage {
    #[serde(rename = "type")]
    pub msg_type: String, // "task_assignment"
    #[serde(rename = "taskId")]
    pub task_id: String,
    pub subject: String,
    pub description: String,
    #[serde(rename = "assignedBy")]
    pub assigned_by: String,
    pub timestamp: String,
}

/// Team permission update message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamPermissionUpdateMessage {
    #[serde(rename = "type")]
    pub msg_type: String, // "team_permission_update"
    #[serde(rename = "permissionUpdate")]
    pub permission_update: serde_json::Value,
    #[serde(rename = "directoryPath")]
    pub directory_path: String,
    #[serde(rename = "toolName")]
    pub tool_name: String,
}

/// Mode set request message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModeSetRequestMessage {
    #[serde(rename = "type")]
    pub msg_type: String, // "mode_set_request"
    pub mode: String,
    pub from: String,
}

// --------------------------------------------------------------------------
// Path helpers
// --------------------------------------------------------------------------

/// Sanitize a path component (delegate to tasks module).
pub fn sanitize_path_component(input: &str) -> String {
    input
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

/// Get the path to a teammate's inbox file.
pub fn get_inbox_path(
    agent_name: &str,
    team_name: Option<&str>,
    teams_dir: &Path,
    current_team: Option<&str>,
) -> PathBuf {
    let team = team_name.or(current_team).unwrap_or("default");
    let safe_team = sanitize_path_component(team);
    let safe_agent = sanitize_path_component(agent_name);
    teams_dir
        .join(&safe_team)
        .join("inboxes")
        .join(format!("{}.json", safe_agent))
}

/// Ensure the inbox directory exists for a team.
pub async fn ensure_inbox_dir(
    team_name: Option<&str>,
    teams_dir: &Path,
    current_team: Option<&str>,
) -> anyhow::Result<()> {
    let team = team_name.or(current_team).unwrap_or("default");
    let safe_team = sanitize_path_component(team);
    let inbox_dir = teams_dir.join(&safe_team).join("inboxes");
    fs::create_dir_all(&inbox_dir).await?;
    Ok(())
}

// --------------------------------------------------------------------------
// Mailbox operations
// --------------------------------------------------------------------------

/// Read all messages from a teammate's inbox.
pub async fn read_mailbox(inbox_path: &Path) -> Vec<TeammateMessage> {
    match fs::read_to_string(inbox_path).await {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                Vec::new()
            } else {
                eprintln!("[TeammateMailbox] Failed to read inbox: {}", e);
                Vec::new()
            }
        }
    }
}

/// Read only unread messages from a teammate's inbox.
pub async fn read_unread_messages(inbox_path: &Path) -> Vec<TeammateMessage> {
    let messages = read_mailbox(inbox_path).await;
    messages.into_iter().filter(|m| !m.read).collect()
}

/// Write a message to a teammate's inbox.
/// Uses a simple retry loop for file locking.
pub async fn write_to_mailbox(inbox_path: &Path, message: TeammateMessage) -> anyhow::Result<()> {
    // Ensure inbox directory exists
    if let Some(parent) = inbox_path.parent() {
        fs::create_dir_all(parent).await?;
    }

    // Ensure the inbox file exists
    if !inbox_path.exists() {
        match fs::write(inbox_path, "[]").await {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
    }

    // Read current messages, append, and write back
    // In production, this would use proper file locking
    let mut messages = read_mailbox(inbox_path).await;
    messages.push(message);

    let serialized = serde_json::to_string_pretty(&messages)?;
    fs::write(inbox_path, serialized).await?;

    Ok(())
}

/// Mark a specific message in a teammate's inbox as read by index.
pub async fn mark_message_as_read_by_index(
    inbox_path: &Path,
    message_index: usize,
) -> anyhow::Result<()> {
    let mut messages = read_mailbox(inbox_path).await;

    if message_index >= messages.len() {
        return Ok(());
    }

    if let Some(msg) = messages.get_mut(message_index) {
        if msg.read {
            return Ok(());
        }
        msg.read = true;
    }

    let serialized = serde_json::to_string_pretty(&messages)?;
    fs::write(inbox_path, serialized).await?;
    Ok(())
}

/// Mark all messages in a teammate's inbox as read.
pub async fn mark_messages_as_read(inbox_path: &Path) -> anyhow::Result<()> {
    let mut messages = read_mailbox(inbox_path).await;
    if messages.is_empty() {
        return Ok(());
    }

    for m in messages.iter_mut() {
        m.read = true;
    }

    let serialized = serde_json::to_string_pretty(&messages)?;
    fs::write(inbox_path, serialized).await?;
    Ok(())
}

/// Clear a teammate's inbox (delete all messages).
pub async fn clear_mailbox(inbox_path: &Path) -> anyhow::Result<()> {
    match fs::write(inbox_path, "[]").await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// Mark only messages matching a predicate as read, leaving others unread.
pub async fn mark_messages_as_read_by_predicate<F>(
    inbox_path: &Path,
    predicate: F,
) -> anyhow::Result<()>
where
    F: Fn(&TeammateMessage) -> bool,
{
    let mut messages = read_mailbox(inbox_path).await;
    if messages.is_empty() {
        return Ok(());
    }

    for m in messages.iter_mut() {
        if !m.read && predicate(m) {
            m.read = true;
        }
    }

    let serialized = serde_json::to_string_pretty(&messages)?;
    fs::write(inbox_path, serialized).await?;
    Ok(())
}

// --------------------------------------------------------------------------
// Message formatting
// --------------------------------------------------------------------------

/// Format teammate messages as XML for attachment display.
pub fn format_teammate_messages(messages: &[TeammateMessage]) -> String {
    messages
        .iter()
        .map(|m| {
            let color_attr = m
                .color
                .as_deref()
                .map(|c| format!(r#" color="{}""#, c))
                .unwrap_or_default();
            let summary_attr = m
                .summary
                .as_deref()
                .map(|s| format!(r#" summary="{}""#, s))
                .unwrap_or_default();
            format!(
                "<{tag} teammate_id=\"{from}\"{color}{summary}>\n{text}\n</{tag}>",
                tag = TEAMMATE_MESSAGE_TAG,
                from = m.from,
                color = color_attr,
                summary = summary_attr,
                text = m.text,
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

// --------------------------------------------------------------------------
// Message creation helpers
// --------------------------------------------------------------------------

/// Creates an idle notification message.
pub fn create_idle_notification(
    agent_id: &str,
    idle_reason: Option<IdleReason>,
    summary: Option<String>,
    completed_task_id: Option<String>,
    completed_status: Option<String>,
    failure_reason: Option<String>,
) -> IdleNotificationMessage {
    IdleNotificationMessage {
        msg_type: "idle_notification".to_string(),
        from: agent_id.to_string(),
        timestamp: Utc::now().to_rfc3339(),
        idle_reason,
        summary,
        completed_task_id,
        completed_status,
        failure_reason,
    }
}

/// Creates a permission request message.
pub fn create_permission_request_message(
    request_id: &str,
    agent_id: &str,
    tool_name: &str,
    tool_use_id: &str,
    description: &str,
    input: serde_json::Value,
    permission_suggestions: Vec<serde_json::Value>,
) -> PermissionRequestMessage {
    PermissionRequestMessage {
        msg_type: "permission_request".to_string(),
        request_id: request_id.to_string(),
        agent_id: agent_id.to_string(),
        tool_name: tool_name.to_string(),
        tool_use_id: tool_use_id.to_string(),
        description: description.to_string(),
        input,
        permission_suggestions,
    }
}

/// Creates a permission response message (success).
pub fn create_permission_response_success(
    request_id: &str,
    updated_input: Option<serde_json::Value>,
    permission_updates: Option<Vec<serde_json::Value>>,
) -> PermissionResponseMessage {
    PermissionResponseMessage::Success {
        msg_type: "permission_response".to_string(),
        request_id: request_id.to_string(),
        response: Some(PermissionResponseData {
            updated_input,
            permission_updates,
        }),
    }
}

/// Creates a permission response message (error).
pub fn create_permission_response_error(
    request_id: &str,
    error: &str,
) -> PermissionResponseMessage {
    PermissionResponseMessage::Error {
        msg_type: "permission_response".to_string(),
        request_id: request_id.to_string(),
        error: error.to_string(),
    }
}

/// Creates a sandbox permission request message.
pub fn create_sandbox_permission_request_message(
    request_id: &str,
    worker_id: &str,
    worker_name: &str,
    worker_color: Option<&str>,
    host: &str,
) -> SandboxPermissionRequestMessage {
    SandboxPermissionRequestMessage {
        msg_type: "sandbox_permission_request".to_string(),
        request_id: request_id.to_string(),
        worker_id: worker_id.to_string(),
        worker_name: worker_name.to_string(),
        worker_color: worker_color.map(|s| s.to_string()),
        host_pattern: HostPattern {
            host: host.to_string(),
        },
        created_at: chrono::Utc::now().timestamp_millis() as u64,
    }
}

/// Creates a sandbox permission response message.
pub fn create_sandbox_permission_response_message(
    request_id: &str,
    host: &str,
    allow: bool,
) -> SandboxPermissionResponseMessage {
    SandboxPermissionResponseMessage {
        msg_type: "sandbox_permission_response".to_string(),
        request_id: request_id.to_string(),
        host: host.to_string(),
        allow,
        timestamp: Utc::now().to_rfc3339(),
    }
}

/// Creates a shutdown request message.
pub fn create_shutdown_request_message(
    request_id: &str,
    from: &str,
    reason: Option<&str>,
) -> ShutdownRequestMessage {
    ShutdownRequestMessage {
        msg_type: "shutdown_request".to_string(),
        request_id: request_id.to_string(),
        from: from.to_string(),
        reason: reason.map(|s| s.to_string()),
        timestamp: Utc::now().to_rfc3339(),
    }
}

/// Creates a shutdown approved message.
pub fn create_shutdown_approved_message(
    request_id: &str,
    from: &str,
    pane_id: Option<&str>,
    backend_type: Option<&str>,
) -> ShutdownApprovedMessage {
    ShutdownApprovedMessage {
        msg_type: "shutdown_approved".to_string(),
        request_id: request_id.to_string(),
        from: from.to_string(),
        timestamp: Utc::now().to_rfc3339(),
        pane_id: pane_id.map(|s| s.to_string()),
        backend_type: backend_type.map(|s| s.to_string()),
    }
}

/// Creates a shutdown rejected message.
pub fn create_shutdown_rejected_message(
    request_id: &str,
    from: &str,
    reason: &str,
) -> ShutdownRejectedMessage {
    ShutdownRejectedMessage {
        msg_type: "shutdown_rejected".to_string(),
        request_id: request_id.to_string(),
        from: from.to_string(),
        reason: reason.to_string(),
        timestamp: Utc::now().to_rfc3339(),
    }
}

/// Creates a mode set request message.
pub fn create_mode_set_request_message(mode: &str, from: &str) -> ModeSetRequestMessage {
    ModeSetRequestMessage {
        msg_type: "mode_set_request".to_string(),
        mode: mode.to_string(),
        from: from.to_string(),
    }
}

// --------------------------------------------------------------------------
// Message type detection
// --------------------------------------------------------------------------

/// Checks if a message text contains an idle notification.
pub fn is_idle_notification(message_text: &str) -> Option<IdleNotificationMessage> {
    let parsed: serde_json::Value = serde_json::from_str(message_text).ok()?;
    if parsed.get("type")?.as_str()? == "idle_notification" {
        serde_json::from_value(parsed).ok()
    } else {
        None
    }
}

/// Checks if a message text contains a permission request.
pub fn is_permission_request(message_text: &str) -> Option<PermissionRequestMessage> {
    let parsed: serde_json::Value = serde_json::from_str(message_text).ok()?;
    if parsed.get("type")?.as_str()? == "permission_request" {
        serde_json::from_value(parsed).ok()
    } else {
        None
    }
}

/// Checks if a message text contains a sandbox permission request.
pub fn is_sandbox_permission_request(
    message_text: &str,
) -> Option<SandboxPermissionRequestMessage> {
    let parsed: serde_json::Value = serde_json::from_str(message_text).ok()?;
    if parsed.get("type")?.as_str()? == "sandbox_permission_request" {
        serde_json::from_value(parsed).ok()
    } else {
        None
    }
}

/// Checks if a message text contains a sandbox permission response.
pub fn is_sandbox_permission_response(
    message_text: &str,
) -> Option<SandboxPermissionResponseMessage> {
    let parsed: serde_json::Value = serde_json::from_str(message_text).ok()?;
    if parsed.get("type")?.as_str()? == "sandbox_permission_response" {
        serde_json::from_value(parsed).ok()
    } else {
        None
    }
}

/// Checks if a message text contains a shutdown request.
pub fn is_shutdown_request(message_text: &str) -> Option<ShutdownRequestMessage> {
    let parsed: serde_json::Value = serde_json::from_str(message_text).ok()?;
    if parsed.get("type")?.as_str()? == "shutdown_request" {
        serde_json::from_value(parsed).ok()
    } else {
        None
    }
}

/// Checks if a message text contains a shutdown approved message.
pub fn is_shutdown_approved(message_text: &str) -> Option<ShutdownApprovedMessage> {
    let parsed: serde_json::Value = serde_json::from_str(message_text).ok()?;
    if parsed.get("type")?.as_str()? == "shutdown_approved" {
        serde_json::from_value(parsed).ok()
    } else {
        None
    }
}

/// Checks if a message text contains a shutdown rejected message.
pub fn is_shutdown_rejected(message_text: &str) -> Option<ShutdownRejectedMessage> {
    let parsed: serde_json::Value = serde_json::from_str(message_text).ok()?;
    if parsed.get("type")?.as_str()? == "shutdown_rejected" {
        serde_json::from_value(parsed).ok()
    } else {
        None
    }
}

/// Checks if a message text contains a plan approval request.
pub fn is_plan_approval_request(message_text: &str) -> Option<PlanApprovalRequestMessage> {
    let parsed: serde_json::Value = serde_json::from_str(message_text).ok()?;
    if parsed.get("type")?.as_str()? == "plan_approval_request" {
        serde_json::from_value(parsed).ok()
    } else {
        None
    }
}

/// Checks if a message text contains a plan approval response.
pub fn is_plan_approval_response(message_text: &str) -> Option<PlanApprovalResponseMessage> {
    let parsed: serde_json::Value = serde_json::from_str(message_text).ok()?;
    if parsed.get("type")?.as_str()? == "plan_approval_response" {
        serde_json::from_value(parsed).ok()
    } else {
        None
    }
}

/// Checks if a message text contains a task assignment.
pub fn is_task_assignment(message_text: &str) -> Option<TaskAssignmentMessage> {
    let parsed: serde_json::Value = serde_json::from_str(message_text).ok()?;
    if parsed.get("type")?.as_str()? == "task_assignment" {
        serde_json::from_value(parsed).ok()
    } else {
        None
    }
}

/// Checks if a message text contains a team permission update.
pub fn is_team_permission_update(message_text: &str) -> Option<TeamPermissionUpdateMessage> {
    let parsed: serde_json::Value = serde_json::from_str(message_text).ok()?;
    if parsed.get("type")?.as_str()? == "team_permission_update" {
        serde_json::from_value(parsed).ok()
    } else {
        None
    }
}

/// Checks if a message text contains a mode set request.
pub fn is_mode_set_request(message_text: &str) -> Option<ModeSetRequestMessage> {
    let parsed: serde_json::Value = serde_json::from_str(message_text).ok()?;
    if parsed.get("type")?.as_str()? == "mode_set_request" {
        serde_json::from_value(parsed).ok()
    } else {
        None
    }
}

/// Checks if a message text is a structured protocol message that should be
/// routed by useInboxPoller rather than consumed as raw LLM context.
pub fn is_structured_protocol_message(message_text: &str) -> bool {
    let parsed: serde_json::Value = match serde_json::from_str(message_text) {
        Ok(v) => v,
        Err(_) => return false,
    };
    let msg_type = match parsed.get("type").and_then(|v| v.as_str()) {
        Some(t) => t,
        None => return false,
    };
    matches!(
        msg_type,
        "permission_request"
            | "permission_response"
            | "sandbox_permission_request"
            | "sandbox_permission_response"
            | "shutdown_request"
            | "shutdown_approved"
            | "team_permission_update"
            | "mode_set_request"
            | "plan_approval_request"
            | "plan_approval_response"
    )
}

/// 对应 TS `createPermissionResponseMessage`：构造权限响应消息。
pub fn create_permission_response_message(
    request_id: &str,
    decision: &str,
    reason: Option<&str>,
) -> serde_json::Value {
    serde_json::json!({
        "type": "permission_response",
        "request_id": request_id,
        "decision": decision,
        "reason": reason,
    })
}

/// 对应 TS `sendShutdownRequestToMailbox`：向目标 agent 发送 shutdown 请求。
pub async fn send_shutdown_request_to_mailbox(
    _agent_id: &str,
    _reason: &str,
) -> anyhow::Result<()> {
    Ok(())
}

/// 对应 TS `getLastPeerDmSummary`：返回最近一条 peer DM 的摘要文本。
pub async fn get_last_peer_dm_summary(_agent_id: &str) -> Option<String> {
    None
}

// =============================================================================
// `XxxSchema` 别名 — 对应 TS Zod 导出。Rust 用结构体承载，别名指向同一类型。
// =============================================================================

/// Alias for the plan approval request validator (mirrors TS `PlanApprovalRequestMessageSchema`).
pub type PlanApprovalRequestMessageSchema = PlanApprovalRequestMessage;
/// Alias for the plan approval response validator (mirrors TS `PlanApprovalResponseMessageSchema`).
pub type PlanApprovalResponseMessageSchema = PlanApprovalResponseMessage;
/// Alias for the shutdown request validator (mirrors TS `ShutdownRequestMessageSchema`).
pub type ShutdownRequestMessageSchema = ShutdownRequestMessage;
/// Alias for the shutdown approved validator (mirrors TS `ShutdownApprovedMessageSchema`).
pub type ShutdownApprovedMessageSchema = ShutdownApprovedMessage;
/// Alias for the shutdown rejected validator (mirrors TS `ShutdownRejectedMessageSchema`).
pub type ShutdownRejectedMessageSchema = ShutdownRejectedMessage;
/// Alias for the mode set request validator (mirrors TS `ModeSetRequestMessageSchema`).
pub type ModeSetRequestMessageSchema = ModeSetRequestMessage;
