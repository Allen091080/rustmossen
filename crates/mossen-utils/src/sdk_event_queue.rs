use std::collections::VecDeque;
use std::sync::Mutex;

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Task started event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStartedEvent {
    pub task_id: String,
    pub tool_use_id: Option<String>,
    pub description: String,
    pub task_type: Option<String>,
    pub workflow_name: Option<String>,
    pub prompt: Option<String>,
}

/// Usage info in task progress
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskProgressUsage {
    pub total_tokens: u64,
    pub tool_uses: u64,
    pub duration_ms: u64,
}

/// Workflow progress item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkWorkflowProgress {
    #[serde(rename = "type")]
    pub progress_type: String,
    pub index: usize,
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Task progress event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskProgressEvent {
    pub task_id: String,
    pub tool_use_id: Option<String>,
    pub description: String,
    pub usage: TaskProgressUsage,
    pub last_tool_name: Option<String>,
    pub summary: Option<String>,
    pub workflow_progress: Option<Vec<SdkWorkflowProgress>>,
}

/// Task terminal status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskTerminalStatus {
    Completed,
    Failed,
    Stopped,
}

/// Task notification event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskNotificationSdkEvent {
    pub task_id: String,
    pub tool_use_id: Option<String>,
    pub status: TaskTerminalStatus,
    pub output_file: String,
    pub summary: String,
    pub usage: Option<TaskProgressUsage>,
}

/// Session state changed event
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionStateValue {
    Idle,
    Running,
    RequiresAction,
}

/// Session state changed event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStateChangedEvent {
    pub state: SessionStateValue,
}

/// SDK event types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "subtype")]
pub enum SdkEvent {
    #[serde(rename = "task_started")]
    TaskStarted(TaskStartedEvent),
    #[serde(rename = "task_progress")]
    TaskProgress(TaskProgressEvent),
    #[serde(rename = "task_notification")]
    TaskNotification(TaskNotificationSdkEvent),
    #[serde(rename = "session_state_changed")]
    SessionStateChanged(SessionStateChangedEvent),
}

/// SDK event with metadata for draining
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkEventWithMeta {
    #[serde(flatten)]
    pub event: SdkEvent,
    pub uuid: String,
    pub session_id: String,
}

const MAX_QUEUE_SIZE: usize = 1000;

static QUEUE: Lazy<Mutex<VecDeque<SdkEvent>>> = Lazy::new(|| Mutex::new(VecDeque::new()));

/// Enqueue an SDK event. Events are only consumed in headless/streaming mode.
pub fn enqueue_sdk_event(event: SdkEvent, is_non_interactive: bool) {
    if !is_non_interactive {
        return;
    }
    let mut queue = QUEUE.lock().unwrap();
    if queue.len() >= MAX_QUEUE_SIZE {
        queue.pop_front();
    }
    queue.push_back(event);
}

/// Drain all SDK events, attaching UUID and session_id to each.
pub fn drain_sdk_events(session_id: &str) -> Vec<SdkEventWithMeta> {
    let mut queue = QUEUE.lock().unwrap();
    if queue.is_empty() {
        return Vec::new();
    }
    let events: Vec<SdkEvent> = queue.drain(..).collect();
    events
        .into_iter()
        .map(|e| SdkEventWithMeta {
            event: e,
            uuid: Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
        })
        .collect()
}

/// Emit a task_notification SDK event for a task reaching a terminal state.
pub fn emit_task_terminated_sdk(
    task_id: &str,
    status: TaskTerminalStatus,
    tool_use_id: Option<String>,
    summary: Option<String>,
    output_file: Option<String>,
    usage: Option<TaskProgressUsage>,
    is_non_interactive: bool,
) {
    enqueue_sdk_event(
        SdkEvent::TaskNotification(TaskNotificationSdkEvent {
            task_id: task_id.to_string(),
            tool_use_id,
            status,
            output_file: output_file.unwrap_or_default(),
            summary: summary.unwrap_or_default(),
            usage,
        }),
        is_non_interactive,
    );
}
