//! Product-level render events for the terminal UI.
//!
//! Agent protocol messages are noisy and backend-shaped. This layer extracts
//! the user-visible rendering intent: stage, refresh policy, and whether the
//! event appends history or updates an active panel.

use crate::state::UiStage;
use mossen_agent::types::{ContentDelta, SdkMessage, StreamEventData};
use mossen_types::ContentBlock;
use serde_json::Value;
use std::collections::BTreeSet;

pub const STREAM_THROTTLE_MS: u64 = 33;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderEvent {
    pub kind: RenderEventKind,
    pub scope: RenderEventScope,
    pub turn_id: Option<String>,
    pub stage: UiStage,
    pub refresh: RenderRefreshPolicy,
    pub history: RenderHistoryPolicy,
}

impl RenderEvent {
    pub fn new(kind: RenderEventKind, scope: RenderEventScope, stage: UiStage) -> Self {
        let (refresh, history) = default_policies(&kind);
        Self {
            kind,
            scope,
            turn_id: None,
            stage,
            refresh,
            history,
        }
    }

    pub fn with_turn_id(mut self, turn_id: impl Into<String>) -> Self {
        let turn_id = turn_id.into();
        if !turn_id.trim().is_empty() {
            self.turn_id = Some(turn_id);
        }
        self
    }

    pub fn approval_requested(tool_name: impl Into<String>) -> Self {
        Self::new(
            RenderEventKind::ApprovalRequested {
                tool_name: tool_name.into(),
            },
            RenderEventScope::Main,
            UiStage::WaitingApproval,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderEventKind {
    TurnStarted,
    StreamStarted,
    TextDelta {
        bytes: usize,
    },
    ThinkingDelta {
        bytes: usize,
    },
    ToolInputDelta {
        bytes: usize,
    },
    CommandStarted {
        tool_id: Option<String>,
        command: Option<String>,
        cwd: Option<String>,
    },
    CommandOutput {
        tool_id: Option<String>,
        stream: String,
        bytes: usize,
        preview_lines: usize,
        hidden_lines: usize,
        total_lines: Option<usize>,
        full_log_available: bool,
    },
    CommandFinished {
        tool_id: Option<String>,
        exit_code: Option<i64>,
        duration_ms: Option<u64>,
    },
    BackgroundTaskUpdated {
        tool_id: Option<String>,
        task_id: String,
        task_type: String,
        status: String,
        command: Option<String>,
        preview_lines: usize,
        hidden_lines: usize,
        exit_code: Option<i64>,
    },
    ToolRequested {
        tool_name: String,
        tool_id: Option<String>,
    },
    ToolCompleted {
        tool_name: String,
        tool_id: Option<String>,
    },
    PlanUpdated {
        tool_id: Option<String>,
        step_count: usize,
        completed_count: usize,
        active_count: usize,
        pending_count: usize,
        blocked_count: usize,
        active_step: Option<String>,
    },
    FileChangeSummary {
        tool_id: Option<String>,
        file_count: usize,
        additions: usize,
        deletions: usize,
    },
    DiffAvailable {
        tool_id: Option<String>,
        file_count: usize,
        additions: usize,
        deletions: usize,
    },
    ApprovalRequested {
        tool_name: String,
    },
    ErrorRaised {
        source: String,
        summary: String,
    },
    ApiRetry {
        attempt: u32,
        max_retries: u32,
        retry_in_ms: u64,
    },
    CompactBoundary {
        before_token_count: u64,
        after_token_count: u64,
    },
    CompactRequestStatus {
        request_id: String,
        status: String,
        dry_run: bool,
        before_token_count: Option<u64>,
        after_token_count: Option<u64>,
        message_count_before: Option<u64>,
        message_count_after: Option<u64>,
        compacted_message_count: Option<u64>,
        reason: Option<String>,
    },
    ConversationCleared {
        message_count_before: u64,
        message_count_after: u64,
    },
    ClearRequestStatus {
        request_id: String,
        status: String,
        dry_run: bool,
        message_count_before: Option<u64>,
        message_count_after: Option<u64>,
        reason: Option<String>,
    },
    SlashCommandResult {
        request_id: String,
        command: String,
        status: String,
        summary: String,
        error: Option<String>,
    },
    TurnFinished {
        terminal: String,
    },
    FinalSummaryRecorded {
        terminal: String,
        success: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderEventScope {
    Main,
    Task(String),
}

impl RenderEventScope {
    pub fn from_task_id(task_id: Option<&str>) -> Self {
        match task_id {
            Some(task_id) => Self::Task(task_id.to_string()),
            None => Self::Main,
        }
    }

    pub fn is_main(&self) -> bool {
        matches!(self, Self::Main)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderRefreshPolicy {
    Immediate,
    Throttled { min_interval_ms: u64 },
    Passive,
}

impl RenderRefreshPolicy {
    pub fn is_immediate(self) -> bool {
        matches!(self, Self::Immediate)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderHistoryPolicy {
    Append,
    UpdateActive,
    FreezeHistory,
}

pub fn render_events_for_sdk_message(message: &SdkMessage) -> Vec<RenderEvent> {
    let scope = RenderEventScope::from_task_id(message.task_id());
    match message {
        SdkMessage::SystemInit { .. } => vec![RenderEvent::new(
            RenderEventKind::TurnStarted,
            scope,
            UiStage::Thinking,
        )],
        SdkMessage::User { .. } => Vec::new(),
        SdkMessage::Assistant { message, .. } => {
            let mut events = Vec::new();
            for block in &message.content {
                match block {
                    ContentBlock::Text(text) if !text.text.is_empty() => {
                        events.push(RenderEvent::new(
                            RenderEventKind::TextDelta {
                                bytes: text.text.len(),
                            },
                            scope.clone(),
                            UiStage::Thinking,
                        ));
                    }
                    ContentBlock::ToolUse(tool) => {
                        if is_command_tool_name(&tool.name) {
                            events.push(RenderEvent::new(
                                RenderEventKind::CommandStarted {
                                    tool_id: Some(tool.id.clone()),
                                    command: command_value_field(&tool.input, "command"),
                                    cwd: command_value_field(&tool.input, "cwd"),
                                },
                                scope.clone(),
                                UiStage::RunningCommand,
                            ));
                        } else {
                            events.push(RenderEvent::new(
                                RenderEventKind::ToolRequested {
                                    tool_name: tool.name.clone(),
                                    tool_id: Some(tool.id.clone()),
                                },
                                scope.clone(),
                                UiStage::from_tool_name(&tool.name),
                            ));
                        }
                    }
                    _ => {}
                }
            }
            events
        }
        SdkMessage::StreamEvent { event, .. } => render_events_for_stream_event(event, scope),
        SdkMessage::Result { terminal, .. } => {
            let stage = UiStage::from_terminal(terminal);
            let mut events = vec![RenderEvent::new(
                RenderEventKind::TurnFinished {
                    terminal: terminal.clone(),
                },
                scope.clone(),
                stage,
            )];
            events.push(RenderEvent::new(
                RenderEventKind::FinalSummaryRecorded {
                    terminal: terminal.clone(),
                    success: stage == UiStage::Done,
                },
                scope.clone(),
                stage,
            ));
            if stage == UiStage::Failed {
                events.push(RenderEvent::new(
                    RenderEventKind::ErrorRaised {
                        source: "terminal".to_string(),
                        summary: first_nonempty_line(terminal),
                    },
                    scope,
                    stage,
                ));
            }
            events
        }
        SdkMessage::ToolUseSummary {
            tool_name,
            tool_use_id,
            summary,
            full_content,
            ..
        } => {
            let mut events = if is_command_tool_name(tool_name) {
                let mut events = command_output_events(
                    tool_use_id.clone(),
                    summary,
                    full_content.as_deref(),
                    scope.clone(),
                );
                events.push(command_finished_event(
                    tool_use_id.clone(),
                    summary,
                    full_content.as_deref(),
                    scope.clone(),
                ));
                events
            } else {
                let mut events = vec![RenderEvent::new(
                    RenderEventKind::ToolCompleted {
                        tool_name: tool_name.clone(),
                        tool_id: tool_use_id.clone(),
                    },
                    scope.clone(),
                    UiStage::ReviewingResult,
                )];
                if is_plan_tool_name(tool_name) {
                    if let Some(event) = plan_updated_event(
                        tool_use_id.clone(),
                        summary,
                        full_content.as_deref(),
                        scope.clone(),
                    ) {
                        events.push(event);
                    }
                }
                if is_file_change_tool_name(tool_name) {
                    if let Some(event) = file_change_summary_event(
                        tool_use_id.clone(),
                        summary,
                        full_content.as_deref(),
                        scope.clone(),
                    ) {
                        events.push(event);
                    }
                    if let Some(event) = diff_available_event(
                        tool_use_id.clone(),
                        summary,
                        full_content.as_deref(),
                        scope.clone(),
                    ) {
                        events.push(event);
                    }
                }
                events
            };
            if let Some(event) = background_task_updated_event(
                tool_use_id.clone(),
                tool_name,
                summary,
                full_content.as_deref(),
                scope,
            ) {
                events.push(event);
            }
            events
        }
        SdkMessage::CompactBoundary {
            before_token_count,
            after_token_count,
            ..
        } => vec![RenderEvent::new(
            RenderEventKind::CompactBoundary {
                before_token_count: *before_token_count,
                after_token_count: *after_token_count,
            },
            scope,
            UiStage::ReviewingResult,
        )],
        SdkMessage::CompactRequestStatus {
            request_id,
            status,
            dry_run,
            before_token_count,
            after_token_count,
            message_count_before,
            message_count_after,
            compacted_message_count,
            reason,
            ..
        } => vec![RenderEvent::new(
            RenderEventKind::CompactRequestStatus {
                request_id: request_id.clone(),
                status: status.as_str().to_string(),
                dry_run: *dry_run,
                before_token_count: *before_token_count,
                after_token_count: *after_token_count,
                message_count_before: *message_count_before,
                message_count_after: *message_count_after,
                compacted_message_count: *compacted_message_count,
                reason: reason.clone(),
            },
            scope,
            UiStage::ReviewingResult,
        )],
        SdkMessage::ConversationCleared {
            message_count_before,
            message_count_after,
            ..
        } => vec![RenderEvent::new(
            RenderEventKind::ConversationCleared {
                message_count_before: *message_count_before,
                message_count_after: *message_count_after,
            },
            scope,
            UiStage::ReviewingResult,
        )],
        SdkMessage::ClearRequestStatus {
            request_id,
            status,
            dry_run,
            message_count_before,
            message_count_after,
            reason,
            ..
        } => vec![RenderEvent::new(
            RenderEventKind::ClearRequestStatus {
                request_id: request_id.clone(),
                status: status.as_str().to_string(),
                dry_run: *dry_run,
                message_count_before: *message_count_before,
                message_count_after: *message_count_after,
                reason: reason.clone(),
            },
            scope,
            UiStage::ReviewingResult,
        )],
        SdkMessage::ApiRetry {
            attempt,
            max_retries,
            retry_in_ms,
            ..
        } => vec![RenderEvent::new(
            RenderEventKind::ApiRetry {
                attempt: *attempt,
                max_retries: *max_retries,
                retry_in_ms: *retry_in_ms,
            },
            scope,
            UiStage::Retrying,
        )],
    }
}

fn render_events_for_stream_event(
    event: &StreamEventData,
    scope: RenderEventScope,
) -> Vec<RenderEvent> {
    match event {
        StreamEventData::MessageStart => vec![RenderEvent::new(
            RenderEventKind::StreamStarted,
            scope,
            UiStage::Thinking,
        )],
        StreamEventData::ContentBlockDelta { delta, .. } => match delta {
            ContentDelta::TextDelta { text } => vec![RenderEvent::new(
                RenderEventKind::TextDelta { bytes: text.len() },
                scope,
                UiStage::Thinking,
            )],
            ContentDelta::ThinkingDelta { thinking } => vec![RenderEvent::new(
                RenderEventKind::ThinkingDelta {
                    bytes: thinking.len(),
                },
                scope,
                UiStage::Thinking,
            )],
            ContentDelta::InputJsonDelta { partial_json } => vec![RenderEvent::new(
                RenderEventKind::ToolInputDelta {
                    bytes: partial_json.len(),
                },
                scope,
                UiStage::Thinking,
            )],
        },
        _ => Vec::new(),
    }
}

fn command_output_events(
    tool_id: Option<String>,
    summary: &str,
    full_content: Option<&str>,
    scope: RenderEventScope,
) -> Vec<RenderEvent> {
    let preview = parse_json_value(summary);
    let full = full_content.and_then(parse_json_value);
    let preview_object = preview.as_ref().and_then(Value::as_object);
    let full_object = full.as_ref().and_then(Value::as_object);
    ["stdout", "stderr"]
        .into_iter()
        .filter_map(|stream| {
            let summary = command_output_summary(stream, preview_object, full_object)?;
            Some(RenderEvent::new(
                RenderEventKind::CommandOutput {
                    tool_id: tool_id.clone(),
                    stream: stream.to_string(),
                    bytes: summary.bytes,
                    preview_lines: summary.preview_lines,
                    hidden_lines: summary.hidden_lines,
                    total_lines: summary.total_lines,
                    full_log_available: summary.full_log_available,
                },
                scope.clone(),
                UiStage::RunningCommand,
            ))
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CommandOutputEventSummary {
    bytes: usize,
    preview_lines: usize,
    hidden_lines: usize,
    total_lines: Option<usize>,
    full_log_available: bool,
}

fn command_output_summary(
    stream: &str,
    preview: Option<&serde_json::Map<String, Value>>,
    full: Option<&serde_json::Map<String, Value>>,
) -> Option<CommandOutputEventSummary> {
    let preview_text = preview
        .and_then(|object| object.get(stream))
        .and_then(value_as_display_text)
        .unwrap_or_default();
    let full_text = full
        .and_then(|object| object.get(stream))
        .and_then(value_as_display_text);
    let output_text = full_text.as_deref().unwrap_or(preview_text.as_str());
    if output_text.is_empty() {
        return None;
    }

    let preview_lines = count_visible_lines(&preview_text);
    let full_lines = full_text.as_deref().map(count_visible_lines);
    let hidden_key = format!("{stream}_hidden_lines");
    let preview_hidden = preview
        .and_then(|object| object.get(&hidden_key))
        .and_then(value_as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or_default();
    let hidden_lines = full_lines
        .map(|total| total.saturating_sub(preview_lines))
        .unwrap_or(preview_hidden)
        .max(preview_hidden);
    let total_lines = full_lines.filter(|total| *total > 0).or_else(|| {
        (preview_lines > 0 || hidden_lines > 0)
            .then_some(preview_lines.saturating_add(hidden_lines))
    });

    Some(CommandOutputEventSummary {
        bytes: output_text.len(),
        preview_lines,
        hidden_lines,
        total_lines,
        full_log_available: full_text
            .as_deref()
            .is_some_and(|text| !text.trim().is_empty()),
    })
}

fn command_finished_event(
    tool_id: Option<String>,
    summary: &str,
    full_content: Option<&str>,
    scope: RenderEventScope,
) -> RenderEvent {
    let payload = payload_value(summary, full_content);
    let object = payload.as_ref().and_then(Value::as_object);
    RenderEvent::new(
        RenderEventKind::CommandFinished {
            tool_id,
            exit_code: object
                .and_then(|object| object.get("exit_code").or_else(|| object.get("exitCode")))
                .and_then(value_as_i64),
            duration_ms: object
                .and_then(|object| {
                    object
                        .get("duration_ms")
                        .or_else(|| object.get("durationMs"))
                })
                .and_then(value_as_u64),
        },
        scope,
        UiStage::ReviewingResult,
    )
}

fn background_task_updated_event(
    tool_id: Option<String>,
    tool_name: &str,
    summary: &str,
    full_content: Option<&str>,
    scope: RenderEventScope,
) -> Option<RenderEvent> {
    let payload = payload_value(summary, full_content)?;
    let object = payload.as_object()?;

    if is_command_tool_name(tool_name) {
        let task_id = object.get("backgroundTaskId").and_then(Value::as_str)?;
        return Some(RenderEvent::new(
            RenderEventKind::BackgroundTaskUpdated {
                tool_id,
                task_id: task_id.to_string(),
                task_type: "background_shell".to_string(),
                status: "started".to_string(),
                command: None,
                preview_lines: object
                    .get("stdout")
                    .and_then(value_as_display_text)
                    .map(|text| count_visible_lines(&text))
                    .unwrap_or_default(),
                hidden_lines: 0,
                exit_code: None,
            },
            scope,
            UiStage::RunningCommand,
        ));
    }

    if tool_name == "TaskOutput" {
        let task = object.get("task").and_then(Value::as_object)?;
        if task.get("task_type").and_then(Value::as_str) != Some("background_shell") {
            return None;
        }
        let output = task.get("output").and_then(value_as_display_text);
        let preview_lines = output
            .as_deref()
            .map(count_visible_lines)
            .unwrap_or_default();
        return Some(RenderEvent::new(
            RenderEventKind::BackgroundTaskUpdated {
                tool_id,
                task_id: task.get("task_id")?.as_str()?.to_string(),
                task_type: "background_shell".to_string(),
                status: task
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("updated")
                    .to_string(),
                command: task
                    .get("description")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                preview_lines,
                hidden_lines: 0,
                exit_code: task.get("exit_code").and_then(value_as_i64),
            },
            scope,
            UiStage::ReviewingResult,
        ));
    }

    if tool_name == "TaskStop" {
        let task_id = object.get("task_id").and_then(Value::as_str)?;
        return Some(RenderEvent::new(
            RenderEventKind::BackgroundTaskUpdated {
                tool_id,
                task_id: task_id.to_string(),
                task_type: object
                    .get("task_type")
                    .and_then(Value::as_str)
                    .unwrap_or("background_shell")
                    .to_string(),
                status: "cancelled".to_string(),
                command: object
                    .get("command")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                preview_lines: 0,
                hidden_lines: 0,
                exit_code: None,
            },
            scope,
            UiStage::ReviewingResult,
        ));
    }

    None
}

fn plan_updated_event(
    tool_id: Option<String>,
    summary: &str,
    full_content: Option<&str>,
    scope: RenderEventScope,
) -> Option<RenderEvent> {
    plan_summary(summary, full_content).map(|summary| {
        RenderEvent::new(
            RenderEventKind::PlanUpdated {
                tool_id,
                step_count: summary.step_count,
                completed_count: summary.completed_count,
                active_count: summary.active_count,
                pending_count: summary.pending_count,
                blocked_count: summary.blocked_count,
                active_step: summary.active_step,
            },
            scope,
            UiStage::Planning,
        )
    })
}

fn file_change_summary_event(
    tool_id: Option<String>,
    summary: &str,
    full_content: Option<&str>,
    scope: RenderEventScope,
) -> Option<RenderEvent> {
    file_change_summary(summary, full_content).map(|summary| {
        RenderEvent::new(
            RenderEventKind::FileChangeSummary {
                tool_id,
                file_count: summary.file_count,
                additions: summary.additions,
                deletions: summary.deletions,
            },
            scope,
            UiStage::EditingFiles,
        )
    })
}

fn diff_available_event(
    tool_id: Option<String>,
    summary: &str,
    full_content: Option<&str>,
    scope: RenderEventScope,
) -> Option<RenderEvent> {
    let payload = payload_value(summary, full_content)?;
    if !has_diff_payload(&payload) {
        return None;
    }
    let summary = file_change_summary_from_value(&payload).unwrap_or_default();
    Some(RenderEvent::new(
        RenderEventKind::DiffAvailable {
            tool_id,
            file_count: summary.file_count,
            additions: summary.additions,
            deletions: summary.deletions,
        },
        scope,
        UiStage::ReviewingResult,
    ))
}

fn is_command_tool_name(tool_name: &str) -> bool {
    matches!(
        tool_name.to_ascii_lowercase().as_str(),
        "bash" | "powershell"
    )
}

fn is_plan_tool_name(tool_name: &str) -> bool {
    matches!(
        tool_name.to_ascii_lowercase().as_str(),
        "todowrite" | "tasknotepad"
    )
}

fn is_file_change_tool_name(tool_name: &str) -> bool {
    matches!(
        tool_name.to_ascii_lowercase().as_str(),
        "write" | "edit" | "multiedit" | "notebookedit"
    )
}

fn command_value_field(value: &Value, key: &str) -> Option<String> {
    value
        .as_object()
        .and_then(|object| object.get(key))
        .and_then(value_as_display_text)
        .filter(|value| !value.trim().is_empty())
}

fn payload_value(summary: &str, full_content: Option<&str>) -> Option<Value> {
    full_content
        .and_then(parse_json_value)
        .or_else(|| parse_json_value(summary))
}

fn parse_json_value(text: &str) -> Option<Value> {
    serde_json::from_str::<Value>(text).ok()
}

fn value_as_display_text(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

fn value_as_i64(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_str().and_then(|text| text.trim().parse().ok()))
}

fn value_as_u64(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|text| text.trim().parse().ok()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PlanEventSummary {
    step_count: usize,
    completed_count: usize,
    active_count: usize,
    pending_count: usize,
    blocked_count: usize,
    active_step: Option<String>,
}

fn plan_summary(summary: &str, full_content: Option<&str>) -> Option<PlanEventSummary> {
    if let Some(payload) = payload_value(summary, full_content) {
        if let Some(todos) = todo_items_from_value(&payload) {
            let counts = plan_status_counts(todos);
            return Some(PlanEventSummary {
                step_count: todos.len(),
                completed_count: counts.completed,
                active_count: counts.active,
                pending_count: counts.pending,
                blocked_count: counts.blocked,
                active_step: active_step_from_todos(todos),
            });
        }
    }

    let lines: Vec<&str> = full_content
        .unwrap_or(summary)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if lines.is_empty() {
        None
    } else {
        Some(PlanEventSummary {
            step_count: lines.len(),
            completed_count: 0,
            active_count: 0,
            pending_count: lines.len(),
            blocked_count: 0,
            active_step: None,
        })
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct PlanStatusCounts {
    completed: usize,
    active: usize,
    pending: usize,
    blocked: usize,
}

fn plan_status_counts(todos: &[Value]) -> PlanStatusCounts {
    let mut counts = PlanStatusCounts::default();
    for todo in todos {
        match todo_status_group(todo_status(todo)) {
            PlanStatusGroup::Completed => counts.completed = counts.completed.saturating_add(1),
            PlanStatusGroup::Active => counts.active = counts.active.saturating_add(1),
            PlanStatusGroup::Pending => counts.pending = counts.pending.saturating_add(1),
            PlanStatusGroup::Blocked => counts.blocked = counts.blocked.saturating_add(1),
        }
    }
    counts
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlanStatusGroup {
    Completed,
    Active,
    Pending,
    Blocked,
}

fn todo_status_group(status: Option<&str>) -> PlanStatusGroup {
    match status
        .unwrap_or("pending")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "completed" | "complete" | "done" | "success" | "succeeded" => PlanStatusGroup::Completed,
        "in_progress" | "active" | "running" | "started" | "working" => PlanStatusGroup::Active,
        "blocked" | "failed" | "error" | "cancelled" | "canceled" => PlanStatusGroup::Blocked,
        _ => PlanStatusGroup::Pending,
    }
}

fn todo_items_from_value(value: &Value) -> Option<&Vec<Value>> {
    if let Some(items) = value.as_array() {
        return Some(items);
    }
    let object = value.as_object()?;
    ["new_todos", "todos", "tasks", "steps"]
        .iter()
        .find_map(|key| object.get(*key).and_then(Value::as_array))
}

fn active_step_from_todos(todos: &[Value]) -> Option<String> {
    todos
        .iter()
        .find(|todo| {
            todo_status(todo)
                .map(|status| {
                    matches!(
                        status.to_ascii_lowercase().as_str(),
                        "in_progress" | "active" | "running"
                    )
                })
                .unwrap_or(false)
        })
        .and_then(todo_content)
}

fn todo_status(todo: &Value) -> Option<&str> {
    let object = todo.as_object()?;
    object
        .get("status")
        .or_else(|| object.get("state"))
        .and_then(Value::as_str)
}

fn todo_content(todo: &Value) -> Option<String> {
    let object = todo.as_object()?;
    ["content", "title", "task", "text"]
        .iter()
        .find_map(|key| object.get(*key).and_then(value_as_display_text))
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct FileChangeEventSummary {
    file_count: usize,
    additions: usize,
    deletions: usize,
}

fn file_change_summary(
    summary: &str,
    full_content: Option<&str>,
) -> Option<FileChangeEventSummary> {
    if let Some(payload) = payload_value(summary, full_content) {
        if let Some(summary) = file_change_summary_from_value(&payload) {
            return Some(summary);
        }
    }
    file_change_summary_from_text(full_content.unwrap_or(summary))
}

fn file_change_summary_from_value(value: &Value) -> Option<FileChangeEventSummary> {
    if object_has_error(value) {
        return None;
    }

    let mut paths = BTreeSet::new();
    collect_file_paths(value, &mut paths);

    let mut summary = FileChangeEventSummary {
        file_count: paths.len(),
        ..Default::default()
    };
    collect_file_change_counts(value, &mut summary);

    if summary.file_count > 0 || summary.additions > 0 || summary.deletions > 0 {
        Some(summary)
    } else {
        None
    }
}

fn file_change_summary_from_text(text: &str) -> Option<FileChangeEventSummary> {
    let trimmed = text.trim();
    if trimmed.is_empty() || looks_like_file_error(trimmed) {
        return None;
    }

    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("wrote ") && lower.contains(" lines to ") {
        return Some(FileChangeEventSummary {
            file_count: 1,
            additions: first_ascii_number(trimmed).unwrap_or(0),
            deletions: 0,
        });
    }

    if lower.starts_with("edited ") {
        let changed = first_ascii_number(trimmed).unwrap_or(0);
        return Some(FileChangeEventSummary {
            file_count: 1,
            additions: changed,
            deletions: 0,
        });
    }

    if lower.starts_with("file created successfully at:")
        || lower.contains(" has been updated successfully")
        || lower.contains(" updated successfully")
    {
        return Some(FileChangeEventSummary {
            file_count: 1,
            additions: 0,
            deletions: 0,
        });
    }

    None
}

fn object_has_error(value: &Value) -> bool {
    match value {
        Value::Object(object) => {
            object.get("error").is_some() || object.values().any(|child| object_has_error(child))
        }
        Value::Array(items) => items.iter().any(object_has_error),
        _ => false,
    }
}

fn collect_file_paths(value: &Value, paths: &mut BTreeSet<String>) {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                if is_file_path_key(key) {
                    if let Some(path) = value
                        .as_str()
                        .map(str::trim)
                        .filter(|path| !path.is_empty())
                    {
                        paths.insert(path.to_string());
                    }
                }
                collect_file_paths(value, paths);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_file_paths(item, paths);
            }
        }
        _ => {}
    }
}

fn is_file_path_key(key: &str) -> bool {
    matches!(
        key,
        "file_path" | "filePath" | "path" | "notebook_path" | "notebookPath"
    )
}

fn collect_file_change_counts(value: &Value, summary: &mut FileChangeEventSummary) {
    match value {
        Value::Object(object) => {
            summary.additions +=
                object_usize_field(object, &["additions", "added", "lines_added", "linesAdded"])
                    .unwrap_or(0);
            summary.deletions += object_usize_field(
                object,
                &["deletions", "deleted", "lines_removed", "linesRemoved"],
            )
            .unwrap_or(0);

            if let Some(new_text) = object_string_field(object, &["new_string", "newString"]) {
                summary.additions += count_visible_lines(new_text);
            }
            if let Some(old_text) = object_string_field(object, &["old_string", "oldString"]) {
                summary.deletions += count_visible_lines(old_text);
            }
            if object
                .keys()
                .any(|key| matches!(key.as_str(), "file_path" | "filePath"))
            {
                if let Some(content) = object_string_field(object, &["content"]) {
                    summary.additions += count_visible_lines(content);
                }
            }

            for value in object.values() {
                collect_file_change_counts(value, summary);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_file_change_counts(item, summary);
            }
        }
        _ => {}
    }
}

fn object_usize_field(object: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<usize> {
    keys.iter()
        .find_map(|key| object.get(*key).and_then(value_as_usize))
}

fn value_as_usize(value: &Value) -> Option<usize> {
    value_as_u64(value).and_then(|value| usize::try_from(value).ok())
}

fn object_string_field<'a>(
    object: &'a serde_json::Map<String, Value>,
    keys: &[&str],
) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| object.get(*key).and_then(Value::as_str))
}

fn has_diff_payload(value: &Value) -> bool {
    match value {
        Value::Object(object) => {
            (object_string_field(object, &["old_string", "oldString"]).is_some()
                && object_string_field(object, &["new_string", "newString"]).is_some())
                || object_string_field(object, &["diff", "patch"])
                    .map(|text| !text.trim().is_empty())
                    .unwrap_or(false)
                || object.values().any(has_diff_payload)
        }
        Value::Array(items) => items.iter().any(has_diff_payload),
        _ => false,
    }
}

fn looks_like_file_error(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.starts_with("error:")
        || lower.contains("does not exist")
        || lower.contains("not found")
        || lower.contains("no changes to make")
        || lower.contains("too large")
        || lower.contains("cannot create")
        || lower.contains("stub in this build")
}

fn first_ascii_number(text: &str) -> Option<usize> {
    let start = text.find(|ch: char| ch.is_ascii_digit())?;
    let digits: String = text[start..]
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

fn count_visible_lines(content: &str) -> usize {
    if content.is_empty() {
        return 0;
    }
    let count = content.split('\n').count();
    if content.ends_with('\n') {
        count.saturating_sub(1)
    } else {
        count
    }
}

fn first_nonempty_line(text: &str) -> String {
    const MAX_SUMMARY_CHARS: usize = 180;
    let line = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or(text.trim());
    line.chars().take(MAX_SUMMARY_CHARS).collect()
}

fn default_policies(kind: &RenderEventKind) -> (RenderRefreshPolicy, RenderHistoryPolicy) {
    match kind {
        RenderEventKind::TextDelta { .. }
        | RenderEventKind::ThinkingDelta { .. }
        | RenderEventKind::ToolInputDelta { .. }
        | RenderEventKind::CommandOutput { .. } => (
            RenderRefreshPolicy::Throttled {
                min_interval_ms: STREAM_THROTTLE_MS,
            },
            RenderHistoryPolicy::UpdateActive,
        ),
        RenderEventKind::CommandStarted { .. }
        | RenderEventKind::CommandFinished { .. }
        | RenderEventKind::BackgroundTaskUpdated { .. }
        | RenderEventKind::ToolRequested { .. }
        | RenderEventKind::ToolCompleted { .. }
        | RenderEventKind::PlanUpdated { .. }
        | RenderEventKind::FileChangeSummary { .. }
        | RenderEventKind::DiffAvailable { .. }
        | RenderEventKind::ApprovalRequested { .. }
        | RenderEventKind::ErrorRaised { .. }
        | RenderEventKind::ApiRetry { .. }
        | RenderEventKind::CompactBoundary { .. }
        | RenderEventKind::CompactRequestStatus { .. }
        | RenderEventKind::ConversationCleared { .. }
        | RenderEventKind::ClearRequestStatus { .. }
        | RenderEventKind::SlashCommandResult { .. }
        | RenderEventKind::TurnFinished { .. }
        | RenderEventKind::FinalSummaryRecorded { .. } => (
            RenderRefreshPolicy::Immediate,
            RenderHistoryPolicy::FreezeHistory,
        ),
        RenderEventKind::TurnStarted | RenderEventKind::StreamStarted => (
            RenderRefreshPolicy::Immediate,
            RenderHistoryPolicy::UpdateActive,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mossen_agent::types::StreamEventData;
    use mossen_types::{AssistantMessage, ContentBlock, Role, ToolUseBlock};
    use std::collections::HashMap;

    #[test]
    fn maps_tool_request_to_product_stage_and_immediate_refresh() {
        let message = SdkMessage::Assistant {
            message: AssistantMessage {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolUse(ToolUseBlock {
                    id: "toolu-1".to_string(),
                    name: "Bash".to_string(),
                    input: serde_json::json!({"command": "cargo test"}),
                })],
                uuid: None,
                model: None,
                stop_reason: Some("tool_use".to_string()),
                extra: HashMap::new(),
            },
            usage: None,
            task_id: None,
        };

        let events = render_events_for_sdk_message(&message);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].stage, UiStage::RunningCommand);
        assert_eq!(events[0].scope, RenderEventScope::Main);
        assert_eq!(events[0].refresh, RenderRefreshPolicy::Immediate);
        assert_eq!(events[0].history, RenderHistoryPolicy::FreezeHistory);
        assert_eq!(
            events[0].kind,
            RenderEventKind::CommandStarted {
                tool_id: Some("toolu-1".to_string()),
                command: Some("cargo test".to_string()),
                cwd: None
            }
        );
    }

    #[test]
    fn throttles_streaming_text_updates() {
        let message = SdkMessage::StreamEvent {
            event: StreamEventData::ContentBlockDelta {
                index: 0,
                delta: ContentDelta::TextDelta {
                    text: "hello".to_string(),
                },
            },
            task_id: None,
        };

        let events = render_events_for_sdk_message(&message);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].stage, UiStage::Thinking);
        assert_eq!(
            events[0].refresh,
            RenderRefreshPolicy::Throttled {
                min_interval_ms: STREAM_THROTTLE_MS,
            }
        );
        assert_eq!(events[0].history, RenderHistoryPolicy::UpdateActive);
    }

    #[test]
    fn preserves_task_scope_without_promoting_to_main() {
        let message = SdkMessage::ToolUseSummary {
            tool_name: "Read".to_string(),
            tool_use_id: Some("toolu-read".to_string()),
            summary: "{}".to_string(),
            full_content: None,
            task_id: Some("agent-1".to_string()),
        };

        let events = render_events_for_sdk_message(&message);

        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].scope,
            RenderEventScope::Task("agent-1".to_string())
        );
        assert!(!events[0].scope.is_main());
    }

    #[test]
    fn maps_command_summary_to_command_finish_event() {
        let message = SdkMessage::ToolUseSummary {
            tool_name: "Bash".to_string(),
            tool_use_id: Some("toolu-bash".to_string()),
            summary: serde_json::json!({
                "exit_code": 1,
                "duration_ms": 250
            })
            .to_string(),
            full_content: None,
            task_id: None,
        };

        let events = render_events_for_sdk_message(&message);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].stage, UiStage::ReviewingResult);
        assert_eq!(events[0].refresh, RenderRefreshPolicy::Immediate);
        assert_eq!(events[0].history, RenderHistoryPolicy::FreezeHistory);
        assert_eq!(
            events[0].kind,
            RenderEventKind::CommandFinished {
                tool_id: Some("toolu-bash".to_string()),
                exit_code: Some(1),
                duration_ms: Some(250)
            }
        );
    }

    #[test]
    fn maps_command_streams_to_output_events_before_finish() {
        let message = SdkMessage::ToolUseSummary {
            tool_name: "Bash".to_string(),
            tool_use_id: Some("toolu-bash".to_string()),
            summary: serde_json::json!({
                "stdout": "preview\n",
                "stderr": "warn\n",
                "exit_code": 0,
                "duration_ms": 42
            })
            .to_string(),
            full_content: Some(
                serde_json::json!({
                    "stdout": "preview\nfull line\n",
                    "stderr": "warn\n",
                    "exit_code": 0,
                    "duration_ms": 42
                })
                .to_string(),
            ),
            task_id: None,
        };

        let events = render_events_for_sdk_message(&message);

        assert_eq!(events.len(), 3);
        assert_eq!(events[0].stage, UiStage::RunningCommand);
        assert_eq!(
            events[0].refresh,
            RenderRefreshPolicy::Throttled {
                min_interval_ms: STREAM_THROTTLE_MS,
            }
        );
        assert_eq!(events[0].history, RenderHistoryPolicy::UpdateActive);
        assert_eq!(
            events[0].kind,
            RenderEventKind::CommandOutput {
                tool_id: Some("toolu-bash".to_string()),
                stream: "stdout".to_string(),
                bytes: "preview\nfull line\n".len(),
                preview_lines: 1,
                hidden_lines: 1,
                total_lines: Some(2),
                full_log_available: true,
            }
        );
        assert_eq!(
            events[1].kind,
            RenderEventKind::CommandOutput {
                tool_id: Some("toolu-bash".to_string()),
                stream: "stderr".to_string(),
                bytes: "warn\n".len(),
                preview_lines: 1,
                hidden_lines: 0,
                total_lines: Some(1),
                full_log_available: true,
            }
        );
        assert_eq!(
            events[2].kind,
            RenderEventKind::CommandFinished {
                tool_id: Some("toolu-bash".to_string()),
                exit_code: Some(0),
                duration_ms: Some(42)
            }
        );
    }

    #[test]
    fn maps_background_bash_summary_to_task_update_event() {
        let message = SdkMessage::ToolUseSummary {
            tool_name: "Bash".to_string(),
            tool_use_id: Some("toolu-bash".to_string()),
            summary: serde_json::json!({
                "stdout": "Command started in background task: shell-task-1",
                "backgroundTaskId": "shell-task-1",
                "timed_out": false
            })
            .to_string(),
            full_content: None,
            task_id: None,
        };

        let events = render_events_for_sdk_message(&message);

        assert!(events.iter().any(|event| {
            event.kind
                == RenderEventKind::BackgroundTaskUpdated {
                    tool_id: Some("toolu-bash".to_string()),
                    task_id: "shell-task-1".to_string(),
                    task_type: "background_shell".to_string(),
                    status: "started".to_string(),
                    command: None,
                    preview_lines: 1,
                    hidden_lines: 0,
                    exit_code: None,
                }
        }));
    }

    #[test]
    fn maps_task_output_background_shell_to_task_update_event() {
        let message = SdkMessage::ToolUseSummary {
            tool_name: "TaskOutput".to_string(),
            tool_use_id: Some("toolu-task-output".to_string()),
            summary: serde_json::json!({
                "retrieval_status": "ready",
                "task": {
                    "task_id": "shell-task-2",
                    "task_type": "background_shell",
                    "status": "completed",
                    "description": "printf output",
                    "output": "one\ntwo\n",
                    "exit_code": 0
                }
            })
            .to_string(),
            full_content: None,
            task_id: None,
        };

        let events = render_events_for_sdk_message(&message);

        assert!(events.iter().any(|event| {
            event.kind
                == RenderEventKind::BackgroundTaskUpdated {
                    tool_id: Some("toolu-task-output".to_string()),
                    task_id: "shell-task-2".to_string(),
                    task_type: "background_shell".to_string(),
                    status: "completed".to_string(),
                    command: Some("printf output".to_string()),
                    preview_lines: 2,
                    hidden_lines: 0,
                    exit_code: Some(0),
                }
        }));
    }

    #[test]
    fn maps_todowrite_summary_to_plan_event() {
        let message = SdkMessage::ToolUseSummary {
            tool_name: "TodoWrite".to_string(),
            tool_use_id: Some("toolu-plan".to_string()),
            summary: serde_json::json!({
                "old_todos": [],
                "new_todos": [
                    {"id": "1", "content": "Read render docs", "status": "completed"},
                    {"id": "2", "content": "Implement event model", "status": "in_progress"},
                    {"id": "3", "content": "Verify snapshots", "status": "pending"}
                ]
            })
            .to_string(),
            full_content: None,
            task_id: None,
        };

        let events = render_events_for_sdk_message(&message);

        assert_eq!(events.len(), 2);
        assert_eq!(events[1].stage, UiStage::Planning);
        assert_eq!(events[1].refresh, RenderRefreshPolicy::Immediate);
        assert_eq!(events[1].history, RenderHistoryPolicy::FreezeHistory);
        assert_eq!(
            events[1].kind,
            RenderEventKind::PlanUpdated {
                tool_id: Some("toolu-plan".to_string()),
                step_count: 3,
                completed_count: 1,
                active_count: 1,
                pending_count: 1,
                blocked_count: 0,
                active_step: Some("Implement event model".to_string())
            }
        );
    }

    #[test]
    fn maps_write_text_summary_to_file_change_event() {
        let message = SdkMessage::ToolUseSummary {
            tool_name: "Write".to_string(),
            tool_use_id: Some("toolu-write".to_string()),
            summary: "Wrote 12 lines to /tmp/app.rs".to_string(),
            full_content: None,
            task_id: None,
        };

        let events = render_events_for_sdk_message(&message);

        assert_eq!(events.len(), 2);
        assert_eq!(events[1].stage, UiStage::EditingFiles);
        assert_eq!(
            events[1].kind,
            RenderEventKind::FileChangeSummary {
                tool_id: Some("toolu-write".to_string()),
                file_count: 1,
                additions: 12,
                deletions: 0
            }
        );
    }

    #[test]
    fn maps_edit_summary_to_file_change_and_diff_events() {
        let message = SdkMessage::ToolUseSummary {
            tool_name: "Edit".to_string(),
            tool_use_id: Some("toolu-edit".to_string()),
            summary: serde_json::json!({
                "file_path": "/tmp/app.rs",
                "old_string": "old\nline\n",
                "new_string": "new\nline\nextra\n",
                "replace_all": false
            })
            .to_string(),
            full_content: None,
            task_id: None,
        };

        let events = render_events_for_sdk_message(&message);

        assert_eq!(events.len(), 3);
        assert_eq!(
            events[1].kind,
            RenderEventKind::FileChangeSummary {
                tool_id: Some("toolu-edit".to_string()),
                file_count: 1,
                additions: 3,
                deletions: 2
            }
        );
        assert_eq!(events[2].stage, UiStage::ReviewingResult);
        assert_eq!(
            events[2].kind,
            RenderEventKind::DiffAvailable {
                tool_id: Some("toolu-edit".to_string()),
                file_count: 1,
                additions: 3,
                deletions: 2
            }
        );
    }

    #[test]
    fn terminal_text_maps_to_finished_stage() {
        let message = SdkMessage::Result {
            terminal: "ModelError: prevented".to_string(),
            cost_usd: None,
            duration_ms: None,
            usage: None,
            task_id: None,
        };

        let events = render_events_for_sdk_message(&message);

        assert_eq!(events[0].stage, UiStage::Failed);
        assert_eq!(events[0].history, RenderHistoryPolicy::FreezeHistory);
    }

    #[test]
    fn terminal_result_records_final_summary_and_error_event() {
        let message = SdkMessage::Result {
            terminal: "ModelError: prevented".to_string(),
            cost_usd: None,
            duration_ms: None,
            usage: None,
            task_id: None,
        };

        let events = render_events_for_sdk_message(&message);

        assert_eq!(events.len(), 3);
        assert_eq!(
            events[1].kind,
            RenderEventKind::FinalSummaryRecorded {
                terminal: "ModelError: prevented".to_string(),
                success: false
            }
        );
        assert_eq!(
            events[2].kind,
            RenderEventKind::ErrorRaised {
                source: "terminal".to_string(),
                summary: "ModelError: prevented".to_string()
            }
        );
    }

    #[test]
    fn compact_request_status_maps_to_visible_render_event() {
        let message = SdkMessage::CompactRequestStatus {
            request_id: "compact-1".to_string(),
            status: mossen_agent::types::CompactRequestStatus::DryRun,
            dry_run: true,
            before_token_count: Some(128),
            after_token_count: None,
            message_count_before: Some(3),
            message_count_after: Some(3),
            compacted_message_count: Some(0),
            reason: Some("dry run only".to_string()),
            task_id: None,
        };

        let events = render_events_for_sdk_message(&message);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].stage, UiStage::ReviewingResult);
        assert_eq!(events[0].refresh, RenderRefreshPolicy::Immediate);
        assert_eq!(events[0].history, RenderHistoryPolicy::FreezeHistory);
        assert_eq!(
            events[0].kind,
            RenderEventKind::CompactRequestStatus {
                request_id: "compact-1".to_string(),
                status: "dry_run".to_string(),
                dry_run: true,
                before_token_count: Some(128),
                after_token_count: None,
                message_count_before: Some(3),
                message_count_after: Some(3),
                compacted_message_count: Some(0),
                reason: Some("dry run only".to_string())
            }
        );
    }

    #[test]
    fn clear_request_status_maps_to_visible_render_event() {
        let message = SdkMessage::ClearRequestStatus {
            request_id: "clear-1".to_string(),
            status: mossen_agent::types::ClearRequestStatus::DryRun,
            dry_run: true,
            message_count_before: Some(2),
            message_count_after: Some(2),
            reason: Some("dry run only".to_string()),
            task_id: None,
        };

        let events = render_events_for_sdk_message(&message);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].stage, UiStage::ReviewingResult);
        assert_eq!(events[0].refresh, RenderRefreshPolicy::Immediate);
        assert_eq!(events[0].history, RenderHistoryPolicy::FreezeHistory);
        assert_eq!(
            events[0].kind,
            RenderEventKind::ClearRequestStatus {
                request_id: "clear-1".to_string(),
                status: "dry_run".to_string(),
                dry_run: true,
                message_count_before: Some(2),
                message_count_after: Some(2),
                reason: Some("dry run only".to_string())
            }
        );
    }

    #[test]
    fn slash_command_result_is_immediate_freeze_history() {
        let event = RenderEvent::new(
            RenderEventKind::SlashCommandResult {
                request_id: "slash-help-1".to_string(),
                command: "help".to_string(),
                status: "completed".to_string(),
                summary: "/help completed: 12 commands".to_string(),
                error: None,
            },
            RenderEventScope::Main,
            UiStage::ReviewingResult,
        );

        assert_eq!(event.refresh, RenderRefreshPolicy::Immediate);
        assert_eq!(event.history, RenderHistoryPolicy::FreezeHistory);
        assert_eq!(event.stage, UiStage::ReviewingResult);
    }
}
