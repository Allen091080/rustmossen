//! Stream-json render event bridge.
//!
//! The raw SDK message stream is backend-shaped. This bridge reuses the TUI's
//! semantic render event extractor and emits stable NDJSON events that a
//! Codex-CLI-like terminal renderer can consume without scraping assistant text.

use mossen_agent::types::{ApiUsage, ContentDelta, SdkMessage, StreamEventData};
use mossen_tui::render_events::{
    render_events_for_sdk_message, RenderEvent, RenderEventKind, RenderEventScope,
    RenderHistoryPolicy, RenderRefreshPolicy, STREAM_THROTTLE_MS,
};
use mossen_tui::state::UiStage;
use mossen_types::ContentBlock;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::stream_json_terminal_renderer::{
    stream_json_terminal_patch_safe_line, StreamJsonTerminalDrawScheduler,
    StreamJsonTerminalPatchRenderer, STREAM_JSON_RENDER_PATCH_MAX_LINE_CELLS,
};
use mossen_utils::context::terminal_context_window_tokens;

pub const STREAM_JSON_RENDER_EVENT_SCHEMA_VERSION: u32 = 2;
pub const STREAM_JSON_RENDER_SNAPSHOT_SCHEMA_VERSION: u32 = 1;
pub const STREAM_JSON_RENDER_FRAME_SCHEMA_VERSION: u32 = 1;
pub const STREAM_JSON_RENDER_EVENT_TYPE: &str = "render_event";
pub const STREAM_JSON_RENDER_SNAPSHOT_TYPE: &str = "render_snapshot";
pub const STREAM_JSON_RENDER_FRAME_TYPE: &str = "render_frame";
pub const STREAM_JSON_RENDER_EVENT_THROTTLE_MS: u64 = STREAM_THROTTLE_MS;
const STREAM_JSON_RENDER_VISIBLE_TEXT_TAIL_BYTES: usize = 4096;
const STREAM_JSON_RENDER_VISIBLE_TEXT_PREVIEW_LINES: usize = 4;
const STREAM_JSON_RENDER_ASSISTANT_ACTIVITY_VISIBLE_LINES: usize =
    STREAM_JSON_RENDER_VISIBLE_TEXT_PREVIEW_LINES;
const STREAM_JSON_RENDER_TRANSCRIPT_MAX_BYTES: usize = 64 * 1024;
const STREAM_JSON_RENDER_TRANSCRIPT_MAX_LINES: usize = 80;
const STREAM_JSON_RENDER_COMMAND_PREVIEW_MAX_LINES: usize = 3;
const STREAM_JSON_RENDER_COMMAND_EXPANDED_PREVIEW_MAX_LINES: usize = 12;
const STREAM_JSON_RENDER_COMMAND_HISTORY_MAX_ITEMS: usize = 6;
const STREAM_JSON_RENDER_BACKGROUND_TASK_MAX_ITEMS: usize = 5;
const STREAM_JSON_RENDER_BACKGROUND_TASK_EXPANDED_MAX_ITEMS: usize = 10;
const STREAM_JSON_RENDER_DIFF_FILE_PREVIEW_MAX_LINES: usize = 4;
const STREAM_JSON_RENDER_DIFF_FILE_EXPANDED_PREVIEW_MAX_LINES: usize = 12;
const STREAM_JSON_RENDER_DIFF_HUNK_PREVIEW_MAX_LINES: usize = 4;
const STREAM_JSON_RENDER_DIFF_HUNK_EXPANDED_PREVIEW_MAX_LINES: usize = 12;
const STREAM_JSON_RENDER_DIFF_SECTION_PREVIEW_MAX_FILES: usize = 1;
const STREAM_JSON_RENDER_DIFF_SECTION_EXPANDED_MAX_FILES: usize = 4;
const STREAM_JSON_RENDER_DIFF_SECTION_HUNK_PREVIEW_MAX_LINES: usize = 4;
const STREAM_JSON_TERMINAL_APPROVAL_ACTION_COUNT: usize = 4;
const STREAM_JSON_RENDER_APPROVAL_PREVIEW_MAX_LINES: usize = 5;
const STREAM_JSON_RENDER_ERROR_DETAIL_PREVIEW_MAX_LINES: usize = 3;
const STREAM_JSON_RENDER_ERROR_DETAIL_EXPANDED_PREVIEW_MAX_LINES: usize = 10;
const STREAM_JSON_RENDER_SLASH_RESULT_PREVIEW_MAX_LINES: usize = 8;
const STREAM_JSON_RENDER_SLASH_RESULT_EVENT_TOP_START_ROW: usize = 1;
const STREAM_JSON_RENDER_FOOTER_VISIBLE_HINT_MAX: usize = 6;
const STREAM_JSON_TERMINAL_PERMISSION_MODE_ENV: &str = "MOSSEN_PERMISSION_MODE";
const STREAM_JSON_TERMINAL_STATUS_LINE_FULL_MAX_CHARS: usize = 140;
const STREAM_JSON_TERMINAL_STATUS_LINE_COMPACT_MAX_CHARS: usize = 96;
const STREAM_JSON_TERMINAL_STATUS_MODEL_MAX_CHARS: usize = 28;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamJsonTerminalWidgetControl {
    ToggleCommandExpansion,
    ToggleBackgroundTaskExpansion,
    ToggleFileChangeExpansion,
    ToggleDiffExpansion,
    ToggleErrorExpansion,
    FocusNextApprovalAction,
    FocusPreviousApprovalAction,
    ActivateFocusedApprovalAction,
    ActivateApprovalActionByKey(char),
}

#[derive(Debug, Clone)]
pub struct StreamJsonRenderEventEmitter {
    next_event_sequence: u64,
    next_source_message_sequence: u64,
    state: StreamJsonRenderStreamState,
    previous_frame_fingerprint: Option<StreamJsonRenderFrameFingerprint>,
    terminal_patch_renderer: StreamJsonTerminalPatchRenderer,
    terminal_draw_scheduler: StreamJsonTerminalDrawScheduler,
}

impl StreamJsonRenderEventEmitter {
    pub fn new() -> Self {
        Self {
            next_event_sequence: 1,
            next_source_message_sequence: 1,
            state: StreamJsonRenderStreamState::new(),
            previous_frame_fingerprint: None,
            terminal_patch_renderer: StreamJsonTerminalPatchRenderer::new(),
            terminal_draw_scheduler: StreamJsonTerminalDrawScheduler::new(),
        }
    }

    pub fn emit_for_sdk_message(&mut self, message: &SdkMessage) -> Vec<Value> {
        let mut values = Vec::new();
        self.apply_sdk_message_render_events(message, Some(&mut values));
        values
    }

    pub fn seed_terminal_session_model(&mut self, model: &str) -> bool {
        self.state.seed_terminal_session_model(model)
    }

    fn apply_sdk_message_render_events(
        &mut self,
        message: &SdkMessage,
        mut output: Option<&mut Vec<Value>>,
    ) -> bool {
        let source_message_sequence = self.next_source_message_sequence;
        self.next_source_message_sequence = self.next_source_message_sequence.saturating_add(1);
        let source_message_type = sdk_message_type_key(message);
        let emitted_at_ms = unix_timestamp_millis();
        self.state
            .apply_session_metadata_from_sdk_message(message, emitted_at_ms);
        let mut events = render_events_for_sdk_message(message);
        events.extend(terminal_supplemental_render_events_for_sdk_message(message));
        if let SdkMessage::Result { terminal, .. } = message {
            if !self.state.should_emit_final_summary_for_result(terminal) {
                events.retain(|event| {
                    !matches!(event.kind, RenderEventKind::FinalSummaryRecorded { .. })
                });
            }
        }
        let mut emitted_any = false;

        for (event_index_in_source, event) in events.into_iter().enumerate() {
            let event_sequence = self.next_event_sequence;
            self.next_event_sequence = self.next_event_sequence.saturating_add(1);
            let mut value = stream_json_render_event_value(
                &event,
                RenderEventStreamMetadata {
                    event_sequence,
                    source_message_sequence,
                    source_message_type,
                    event_index_in_source,
                    emitted_at_ms,
                },
            );
            terminal_enrich_tool_summary_event_value(&mut value, message);
            self.state.apply_render_event_value(&value);
            if let Some(values) = output.as_mut() {
                values.push(value);
            }
            emitted_any = true;
        }

        emitted_any
    }

    pub fn emit_stream_items_for_sdk_message(&mut self, message: &SdkMessage) -> Vec<Value> {
        let mut values = self.emit_for_sdk_message(message);
        if !values.is_empty() {
            self.state.apply_visible_content_from_sdk_message(message);
            values.extend(self.emit_current_terminal_render_items());
        }
        values
    }

    pub fn emit_terminal_draw_plan_items_for_sdk_message(
        &mut self,
        message: &SdkMessage,
    ) -> Vec<Value> {
        if self.apply_sdk_message_render_events(message, None) {
            self.state.apply_visible_content_from_sdk_message(message);
            self.emit_current_terminal_draw_plan_items()
        } else {
            Vec::new()
        }
    }

    pub fn emit_terminal_permission_request_items(
        &mut self,
        tool_name: &str,
        input: &Value,
    ) -> Vec<Value> {
        let event = RenderEvent::approval_requested(tool_name.to_string());
        let mut values = self.emit_for_render_event(
            event,
            "terminal_permission_request",
            unix_timestamp_millis(),
        );
        self.state
            .mark_terminal_permission_request_context(tool_name, input);
        if !values.is_empty() {
            values.extend(self.emit_current_terminal_render_items());
        }
        values
    }

    pub fn emit_terminal_permission_request_draw_plan_items(
        &mut self,
        tool_name: &str,
        input: &Value,
    ) -> Vec<Value> {
        let event = RenderEvent::approval_requested(tool_name.to_string());
        self.apply_single_render_event_value(
            event,
            "terminal_permission_request",
            unix_timestamp_millis(),
        );
        self.state
            .mark_terminal_permission_request_context(tool_name, input);
        self.emit_current_terminal_draw_plan_items()
    }

    pub fn emit_terminal_widget_control_items(
        &mut self,
        control: StreamJsonTerminalWidgetControl,
    ) -> Vec<Value> {
        let changed = match control {
            StreamJsonTerminalWidgetControl::ToggleCommandExpansion => {
                self.state.toggle_command_widget_expanded()
            }
            StreamJsonTerminalWidgetControl::ToggleBackgroundTaskExpansion => {
                self.state.toggle_background_task_panel_expanded()
            }
            StreamJsonTerminalWidgetControl::ToggleFileChangeExpansion => {
                self.state.toggle_file_change_widget_expanded()
            }
            StreamJsonTerminalWidgetControl::ToggleDiffExpansion => {
                self.state.toggle_diff_widget_expanded()
            }
            StreamJsonTerminalWidgetControl::ToggleErrorExpansion => {
                self.state.toggle_error_widget_expanded()
            }
            StreamJsonTerminalWidgetControl::FocusNextApprovalAction => {
                self.state.focus_next_approval_action()
            }
            StreamJsonTerminalWidgetControl::FocusPreviousApprovalAction => {
                self.state.focus_previous_approval_action()
            }
            StreamJsonTerminalWidgetControl::ActivateFocusedApprovalAction => {
                self.state.activate_focused_approval_action()
            }
            StreamJsonTerminalWidgetControl::ActivateApprovalActionByKey(key) => {
                self.state.activate_approval_action_by_key(key)
            }
        };
        if changed {
            self.emit_current_terminal_render_items()
        } else {
            Vec::new()
        }
    }

    pub fn emit_terminal_widget_control_draw_plan_items(
        &mut self,
        control: StreamJsonTerminalWidgetControl,
    ) -> Vec<Value> {
        let changed = match control {
            StreamJsonTerminalWidgetControl::ToggleCommandExpansion => {
                self.state.toggle_command_widget_expanded()
            }
            StreamJsonTerminalWidgetControl::ToggleBackgroundTaskExpansion => {
                self.state.toggle_background_task_panel_expanded()
            }
            StreamJsonTerminalWidgetControl::ToggleFileChangeExpansion => {
                self.state.toggle_file_change_widget_expanded()
            }
            StreamJsonTerminalWidgetControl::ToggleDiffExpansion => {
                self.state.toggle_diff_widget_expanded()
            }
            StreamJsonTerminalWidgetControl::ToggleErrorExpansion => {
                self.state.toggle_error_widget_expanded()
            }
            StreamJsonTerminalWidgetControl::FocusNextApprovalAction => {
                self.state.focus_next_approval_action()
            }
            StreamJsonTerminalWidgetControl::FocusPreviousApprovalAction => {
                self.state.focus_previous_approval_action()
            }
            StreamJsonTerminalWidgetControl::ActivateFocusedApprovalAction => {
                self.state.activate_focused_approval_action()
            }
            StreamJsonTerminalWidgetControl::ActivateApprovalActionByKey(key) => {
                self.state.activate_approval_action_by_key(key)
            }
        };
        if changed {
            self.emit_current_terminal_forced_draw_plan_items("widget_control")
        } else {
            Vec::new()
        }
    }

    pub fn pending_terminal_approval_action_id(&self) -> Option<String> {
        self.state
            .approval_action_intent
            .as_ref()
            .and_then(|intent| intent.get("actionId").or_else(|| intent.get("action")))
            .and_then(Value::as_str)
            .map(str::to_string)
    }

    pub fn emit_terminal_approval_bridge_status_items(
        &mut self,
        bridge_status: &str,
        submitted: bool,
        requires_decision_bridge: bool,
    ) -> Vec<Value> {
        if self.state.mark_approval_action_bridge_status(
            bridge_status,
            submitted,
            requires_decision_bridge,
        ) {
            self.emit_current_terminal_render_items()
        } else {
            Vec::new()
        }
    }

    pub fn emit_terminal_approval_bridge_status_draw_plan_items(
        &mut self,
        bridge_status: &str,
        submitted: bool,
        requires_decision_bridge: bool,
    ) -> Vec<Value> {
        if self.state.mark_approval_action_bridge_status(
            bridge_status,
            submitted,
            requires_decision_bridge,
        ) {
            self.emit_current_terminal_forced_draw_plan_items("approval_bridge_status")
        } else {
            Vec::new()
        }
    }

    pub fn emit_terminal_approval_edit_command_items(
        &mut self,
        bridge_status: &str,
        command: Option<&str>,
        editing: bool,
    ) -> Vec<Value> {
        if self
            .state
            .mark_approval_edit_command_status(bridge_status, command, editing)
        {
            self.emit_current_terminal_render_items()
        } else {
            Vec::new()
        }
    }

    pub fn emit_terminal_approval_edit_command_draw_plan_items(
        &mut self,
        bridge_status: &str,
        command: Option<&str>,
        editing: bool,
    ) -> Vec<Value> {
        if self
            .state
            .mark_approval_edit_command_status(bridge_status, command, editing)
        {
            self.emit_current_terminal_forced_draw_plan_items("approval_edit_command")
        } else {
            Vec::new()
        }
    }

    pub fn emit_terminal_resize_draw_plan_items(&mut self) -> Vec<Value> {
        self.emit_current_terminal_forced_draw_plan_items("viewport_resize")
    }

    pub fn emit_terminal_status_heartbeat_draw_plan_items(&mut self) -> Vec<Value> {
        self.emit_terminal_status_heartbeat_draw_plan_items_at(unix_timestamp_millis())
    }

    fn emit_terminal_status_heartbeat_draw_plan_items_at(
        &mut self,
        emitted_at_ms: u64,
    ) -> Vec<Value> {
        if self.state.mark_terminal_status_heartbeat(emitted_at_ms) {
            self.emit_current_terminal_draw_plan_items()
        } else {
            Vec::new()
        }
    }

    pub fn emit_slash_command_result_items(
        &mut self,
        request_id: &str,
        response: &Value,
        error: Option<&str>,
    ) -> Vec<Value> {
        let command = response
            .get("command")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        let status = response
            .get("status")
            .and_then(Value::as_str)
            .or_else(|| error.map(|_| "error"))
            .unwrap_or("completed")
            .to_string();
        let summary = slash_command_result_summary(&command, &status, response, error);
        let event = RenderEvent::new(
            RenderEventKind::SlashCommandResult {
                request_id: request_id.to_string(),
                command: command.clone(),
                status: status.clone(),
                summary: summary.clone(),
                error: error.map(terminal_clean_line),
            },
            RenderEventScope::Main,
            UiStage::ReviewingResult,
        );
        let source_message_sequence = self.next_source_message_sequence;
        self.next_source_message_sequence = self.next_source_message_sequence.saturating_add(1);
        let event_sequence = self.next_event_sequence;
        self.next_event_sequence = self.next_event_sequence.saturating_add(1);
        let mut value = stream_json_render_event_value(
            &event,
            RenderEventStreamMetadata {
                event_sequence,
                source_message_sequence,
                source_message_type: "slash_command_result",
                event_index_in_source: 0,
                emitted_at_ms: unix_timestamp_millis(),
            },
        );
        terminal_enrich_slash_result_event_value(&mut value, response, error);
        self.state.apply_render_event_value(&value);
        let mut values = vec![value];
        self.state
            .mark_slash_command_result_response(request_id, response, &summary, error);
        if !values.is_empty() {
            values.extend(self.emit_current_terminal_render_items());
        }
        values
    }

    fn emit_current_terminal_render_items(&mut self) -> Vec<Value> {
        let mut values = Vec::with_capacity(4);
        values.push(self.state.snapshot_value());
        let (frame, fingerprint) = self
            .state
            .terminal_frame_value_with_previous(self.previous_frame_fingerprint.as_ref());
        let terminal_patch = self.terminal_patch_renderer.render_frame_value(&frame);
        let terminal_draw_plan = self
            .terminal_draw_scheduler
            .render_patch_value(&terminal_patch);
        self.previous_frame_fingerprint = Some(fingerprint);
        values.push(frame);
        values.push(terminal_patch);
        values.push(terminal_draw_plan);
        values
    }

    fn emit_current_terminal_draw_plan_items(&mut self) -> Vec<Value> {
        let (frame, fingerprint) = self
            .state
            .terminal_frame_value_with_previous(self.previous_frame_fingerprint.as_ref());
        let terminal_patch = self.terminal_patch_renderer.render_frame_value(&frame);
        let terminal_draw_plan = self
            .terminal_draw_scheduler
            .render_patch_value(&terminal_patch);
        self.previous_frame_fingerprint = Some(fingerprint);
        vec![terminal_draw_plan]
    }

    fn emit_current_terminal_forced_draw_plan_items(&mut self, reason: &str) -> Vec<Value> {
        let (frame, fingerprint) = self
            .state
            .terminal_frame_value_with_previous(self.previous_frame_fingerprint.as_ref());
        let terminal_patch = self
            .terminal_patch_renderer
            .render_frame_value_forced(&frame, reason);
        let terminal_draw_plan = self
            .terminal_draw_scheduler
            .render_patch_value(&terminal_patch);
        self.previous_frame_fingerprint = Some(fingerprint);
        vec![terminal_draw_plan]
    }

    pub fn snapshot_value(&self) -> Value {
        self.state.snapshot_value()
    }

    pub fn terminal_frame_value(&self) -> Value {
        self.state.terminal_frame_value()
    }

    fn emit_for_render_event(
        &mut self,
        event: RenderEvent,
        source_message_type: &'static str,
        emitted_at_ms: u64,
    ) -> Vec<Value> {
        vec![self.apply_single_render_event_value(event, source_message_type, emitted_at_ms)]
    }

    fn apply_single_render_event_value(
        &mut self,
        event: RenderEvent,
        source_message_type: &'static str,
        emitted_at_ms: u64,
    ) -> Value {
        let source_message_sequence = self.next_source_message_sequence;
        self.next_source_message_sequence = self.next_source_message_sequence.saturating_add(1);
        let event_sequence = self.next_event_sequence;
        self.next_event_sequence = self.next_event_sequence.saturating_add(1);
        let value = stream_json_render_event_value(
            &event,
            RenderEventStreamMetadata {
                event_sequence,
                source_message_sequence,
                source_message_type,
                event_index_in_source: 0,
                emitted_at_ms,
            },
        );
        self.state.apply_render_event_value(&value);
        value
    }
}

impl Default for StreamJsonRenderEventEmitter {
    fn default() -> Self {
        Self::new()
    }
}

pub fn stream_json_render_events_for_sdk_message(message: &SdkMessage) -> Vec<Value> {
    StreamJsonRenderEventEmitter::new().emit_for_sdk_message(message)
}

fn terminal_supplemental_render_events_for_sdk_message(message: &SdkMessage) -> Vec<RenderEvent> {
    let SdkMessage::ToolUseSummary {
        tool_name,
        tool_use_id,
        summary,
        full_content,
        ..
    } = message
    else {
        return Vec::new();
    };

    if tool_name != "TaskOutput" {
        return Vec::new();
    }

    let Some(payload) = terminal_tool_summary_payload(summary, full_content.as_deref()) else {
        return Vec::new();
    };
    let Some(task) = payload.get("task").and_then(Value::as_object) else {
        return Vec::new();
    };
    if task.get("task_type").and_then(Value::as_str) != Some("background_shell") {
        return Vec::new();
    }

    let task_id = task
        .get("task_id")
        .and_then(Value::as_str)
        .unwrap_or("background-task")
        .to_string();
    let output = task
        .get("output")
        .and_then(terminal_value_as_display_text)
        .unwrap_or_default();
    let line_count = terminal_visible_line_count(&output);
    let preview_lines = line_count.min(STREAM_JSON_RENDER_COMMAND_PREVIEW_MAX_LINES);
    let hidden_lines = line_count.saturating_sub(preview_lines);
    let exit_code = task.get("exit_code").and_then(Value::as_i64);
    let scope = RenderEventScope::Task(task_id);

    let mut events = Vec::new();
    if !output.trim().is_empty() {
        events.push(RenderEvent::new(
            RenderEventKind::CommandOutput {
                tool_id: tool_use_id.clone(),
                stream: "output".to_string(),
                bytes: output.len(),
                preview_lines,
                hidden_lines,
                total_lines: Some(line_count),
                full_log_available: true,
            },
            scope.clone(),
            UiStage::RunningCommand,
        ));
    }
    events.push(RenderEvent::new(
        RenderEventKind::CommandFinished {
            tool_id: tool_use_id.clone(),
            exit_code,
            duration_ms: None,
        },
        scope,
        UiStage::ReviewingResult,
    ));
    events
}

fn terminal_enrich_tool_summary_event_value(value: &mut Value, message: &SdkMessage) {
    let SdkMessage::ToolUseSummary {
        tool_name,
        summary,
        full_content,
        ..
    } = message
    else {
        return;
    };

    let is_command_output = value
        .get("kind")
        .and_then(Value::as_str)
        .is_some_and(|kind| kind == "command_output");
    let is_command_finished = value
        .get("kind")
        .and_then(Value::as_str)
        .is_some_and(|kind| kind == "command_finished");
    if !is_command_output && !is_command_finished {
        return;
    }

    let Some(payload) = value.get_mut("payload").and_then(Value::as_object_mut) else {
        return;
    };
    let Some(summary_payload) = terminal_tool_summary_payload(summary, full_content.as_deref())
    else {
        return;
    };

    if let Some(background_task_id) = summary_payload
        .get("backgroundTaskId")
        .and_then(Value::as_str)
        .map(str::to_string)
    {
        payload.insert("backgroundTaskId".to_string(), json!(background_task_id));
        payload.insert("taskType".to_string(), json!("background_shell"));
        payload.insert("taskStatus".to_string(), json!("started"));
    }

    if tool_name == "TaskOutput" {
        if let Some(task) = summary_payload.get("task").and_then(Value::as_object) {
            terminal_enrich_task_output_payload(payload, task);
        }
    }

    if is_command_output {
        let stream = payload
            .get("stream")
            .and_then(Value::as_str)
            .unwrap_or("output");
        if !payload.contains_key("previewLineItems") {
            if let Some(text) = terminal_summary_preview_text(&summary_payload, tool_name, stream) {
                let lines = terminal_text_head_preview_lines(
                    &text,
                    STREAM_JSON_RENDER_COMMAND_EXPANDED_PREVIEW_MAX_LINES,
                );
                if !lines.is_empty() {
                    payload.insert("previewLineItems".to_string(), json!(lines));
                }
            }
        }
    }
}

fn terminal_enrich_task_output_payload(
    payload: &mut Map<String, Value>,
    task: &Map<String, Value>,
) {
    for (source, target) in [
        ("task_id", "taskId"),
        ("task_type", "taskType"),
        ("status", "taskStatus"),
        ("description", "command"),
    ] {
        if let Some(value) = task.get(source) {
            payload.insert(target.to_string(), value.clone());
        }
    }
    if let Some(exit_code) = task.get("exit_code") {
        payload
            .entry("exitCode".to_string())
            .or_insert_with(|| exit_code.clone());
    }
}

fn terminal_summary_preview_text(payload: &Value, tool_name: &str, stream: &str) -> Option<String> {
    if tool_name == "TaskOutput" {
        return payload
            .get("task")
            .and_then(|task| task.get("output"))
            .and_then(terminal_value_as_display_text)
            .filter(|text| !text.trim().is_empty());
    }

    payload
        .get(stream)
        .or_else(|| {
            if stream == "output" {
                payload.get("stdout")
            } else {
                None
            }
        })
        .and_then(terminal_value_as_display_text)
        .filter(|text| !text.trim().is_empty())
}

fn terminal_tool_summary_payload(summary: &str, full_content: Option<&str>) -> Option<Value> {
    full_content
        .and_then(|text| serde_json::from_str::<Value>(text).ok())
        .or_else(|| serde_json::from_str::<Value>(summary).ok())
}

fn terminal_value_as_display_text(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

fn terminal_visible_line_count(text: &str) -> usize {
    let count = text.lines().filter(|line| !line.trim().is_empty()).count();
    if count == 0 && !text.trim().is_empty() {
        1
    } else {
        count
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StreamJsonRenderStreamState {
    last_sequence: u64,
    applied_count: u64,
    ignored_stale_count: u64,
    current_stage: String,
    current_scope: Value,
    current_activity: Option<Value>,
    current_plan_widget: Option<Value>,
    current_command_widget: Option<Value>,
    terminal_command_history: Vec<Value>,
    current_background_tasks: BTreeMap<String, Value>,
    current_background_tasks_expanded: bool,
    current_file_change_widget: Option<Value>,
    current_diff_widget: Option<Value>,
    current_error_widget: Option<Value>,
    current_final_summary_widget: Option<Value>,
    current_slash_result_widget: Option<Value>,
    last_refresh_policy: String,
    pending_throttled_render: bool,
    needs_immediate_render: bool,
    assistant_text_tail: String,
    assistant_text_transcript: String,
    assistant_text_transcript_omitted_bytes: usize,
    append_count: u64,
    update_active_count: u64,
    freeze_history_count: u64,
    last_history_policy: String,
    terminal_finished: bool,
    terminal_success: Option<bool>,
    terminal_reason: Option<String>,
    turn_started_at_ms: Option<u64>,
    last_emitted_at_ms: Option<u64>,
    current_model: Option<String>,
    status_input_tokens: u64,
    status_output_tokens: u64,
    status_cache_read_input_tokens: u64,
    status_cache_creation_input_tokens: u64,
    status_compact_after_tokens: Option<u64>,
    status_thinking_bytes: u64,
    approval_action_focus_index: usize,
    approval_action_intent: Option<Value>,
    approval_action_intent_sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StreamJsonRenderFrameFingerprint {
    frame_hash: String,
    region_hashes: Vec<(String, String)>,
    regions: Vec<StreamJsonRenderRegionFingerprint>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StreamJsonRenderRegionFingerprint {
    id: String,
    role: String,
    anchor: String,
    placement: String,
    region_hash: String,
    line_count: usize,
}

impl StreamJsonRenderFrameFingerprint {
    fn region_hash(&self, region_id: &str) -> Option<&str> {
        self.region_hashes
            .iter()
            .find_map(|(id, hash)| (id == region_id).then_some(hash.as_str()))
    }
}

impl StreamJsonRenderStreamState {
    pub fn new() -> Self {
        Self {
            last_sequence: 0,
            applied_count: 0,
            ignored_stale_count: 0,
            current_stage: "idle".to_string(),
            current_scope: json!({ "kind": "main" }),
            current_activity: None,
            current_plan_widget: None,
            current_command_widget: None,
            terminal_command_history: Vec::new(),
            current_background_tasks: BTreeMap::new(),
            current_background_tasks_expanded: false,
            current_file_change_widget: None,
            current_diff_widget: None,
            current_error_widget: None,
            current_final_summary_widget: None,
            current_slash_result_widget: None,
            last_refresh_policy: "passive".to_string(),
            pending_throttled_render: false,
            needs_immediate_render: false,
            assistant_text_tail: String::new(),
            assistant_text_transcript: String::new(),
            assistant_text_transcript_omitted_bytes: 0,
            append_count: 0,
            update_active_count: 0,
            freeze_history_count: 0,
            last_history_policy: "freeze_history".to_string(),
            terminal_finished: false,
            terminal_success: None,
            terminal_reason: None,
            turn_started_at_ms: None,
            last_emitted_at_ms: None,
            current_model: None,
            status_input_tokens: 0,
            status_output_tokens: 0,
            status_cache_read_input_tokens: 0,
            status_cache_creation_input_tokens: 0,
            status_compact_after_tokens: None,
            status_thinking_bytes: 0,
            approval_action_focus_index: 0,
            approval_action_intent: None,
            approval_action_intent_sequence: 0,
        }
    }

    fn apply_session_metadata_from_sdk_message(
        &mut self,
        message: &SdkMessage,
        emitted_at_ms: u64,
    ) {
        self.last_emitted_at_ms = Some(emitted_at_ms);
        if self.turn_started_at_ms.is_none() {
            self.turn_started_at_ms = Some(emitted_at_ms);
        }

        match message {
            SdkMessage::SystemInit { model, .. } => {
                self.current_model = Some(terminal_status_label(
                    model,
                    STREAM_JSON_TERMINAL_STATUS_MODEL_MAX_CHARS,
                ));
            }
            SdkMessage::Assistant { message, usage, .. } => {
                if let Some(model) = message.model.as_deref() {
                    self.current_model = Some(terminal_status_label(
                        model,
                        STREAM_JSON_TERMINAL_STATUS_MODEL_MAX_CHARS,
                    ));
                }
                if let Some(usage) = usage.as_ref() {
                    self.apply_status_usage(usage);
                }
            }
            SdkMessage::StreamEvent {
                event:
                    StreamEventData::MessageDelta {
                        usage: Some(usage), ..
                    },
                ..
            }
            | SdkMessage::Result {
                usage: Some(usage), ..
            } => {
                self.apply_status_usage(usage);
            }
            SdkMessage::CompactBoundary {
                after_token_count, ..
            } => {
                self.status_compact_after_tokens = Some(*after_token_count);
            }
            SdkMessage::CompactRequestStatus {
                after_token_count: Some(after_token_count),
                ..
            } => {
                self.status_compact_after_tokens = Some(*after_token_count);
            }
            SdkMessage::ConversationCleared { .. } => {
                self.status_input_tokens = 0;
                self.status_output_tokens = 0;
                self.status_cache_read_input_tokens = 0;
                self.status_cache_creation_input_tokens = 0;
                self.status_compact_after_tokens = None;
            }
            _ => {}
        }
    }

    fn seed_terminal_session_model(&mut self, model: &str) -> bool {
        if self.current_model.is_some() {
            return false;
        }
        let model = model.trim();
        if model.is_empty() {
            return false;
        }
        let model = terminal_status_label(model, STREAM_JSON_TERMINAL_STATUS_MODEL_MAX_CHARS);
        if self.current_model.as_deref() == Some(model.as_str()) {
            return false;
        }
        self.current_model = Some(model);
        true
    }

    fn apply_status_usage(&mut self, usage: &ApiUsage) {
        self.status_input_tokens = usage.input_tokens;
        self.status_output_tokens = usage.output_tokens;
        self.status_cache_read_input_tokens = usage.cache_read_input_tokens.unwrap_or(0);
        self.status_cache_creation_input_tokens = usage.cache_creation_input_tokens.unwrap_or(0);
        self.status_compact_after_tokens = None;
    }

    fn mark_terminal_status_heartbeat(&mut self, emitted_at_ms: u64) -> bool {
        if self.terminal_finished {
            return false;
        }

        let previous_status_bar = terminal_status_bar_value(
            self,
            terminal_stage_label(&self.current_stage),
            &terminal_scope_label(&self.current_scope),
        );
        let previous_activity = self.current_activity.clone();
        let previous_stage = self.current_stage.clone();

        if self.turn_started_at_ms.is_none() {
            self.turn_started_at_ms = Some(emitted_at_ms);
        }
        self.last_emitted_at_ms = Some(
            self.last_emitted_at_ms
                .map(|last| last.max(emitted_at_ms))
                .unwrap_or(emitted_at_ms),
        );

        if self.current_stage == "idle" {
            self.current_stage = "thinking".to_string();
        }
        if self.current_activity.is_none() {
            self.current_activity = Some(json!({
                "kind": "status_heartbeat",
                "summary": "waiting for model stream",
            }));
        }

        self.last_refresh_policy = "throttled".to_string();
        self.last_history_policy = "update_active".to_string();
        self.pending_throttled_render = true;

        let next_status_bar = terminal_status_bar_value(
            self,
            terminal_stage_label(&self.current_stage),
            &terminal_scope_label(&self.current_scope),
        );
        previous_stage != self.current_stage
            || previous_activity != self.current_activity
            || previous_status_bar != next_status_bar
    }

    pub fn apply_render_event_value(&mut self, value: &Value) -> bool {
        if value.get("type").and_then(Value::as_str) != Some(STREAM_JSON_RENDER_EVENT_TYPE) {
            return false;
        }

        let sequence = value.get("sequence").and_then(Value::as_u64).unwrap_or(0);
        if sequence == 0 || sequence <= self.last_sequence {
            self.ignored_stale_count = self.ignored_stale_count.saturating_add(1);
            return false;
        }

        self.last_sequence = sequence;
        self.applied_count = self.applied_count.saturating_add(1);
        if let Some(stage) = value.get("stage").and_then(Value::as_str) {
            self.current_stage = stage.to_string();
        }
        if let Some(scope) = value.get("scope") {
            self.current_scope = scope.clone();
        }

        let kind = value
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let payload = value.get("payload").unwrap_or(&Value::Null);
        if let Some(emitted_at_ms) = value.get("emittedAtMs").and_then(Value::as_u64) {
            self.last_emitted_at_ms = Some(emitted_at_ms);
            if self.turn_started_at_ms.is_none() || kind == "turn_started" {
                self.turn_started_at_ms = Some(emitted_at_ms);
            }
        }
        if let Some(activity) = render_snapshot_activity(kind, payload) {
            if !(terminal_approval_active(self.current_activity.as_ref())
                && !terminal_approval_active(Some(&activity)))
            {
                self.current_activity = Some(activity);
            }
        } else if !terminal_status_heartbeat_activity(self.current_activity.as_ref())
            && !terminal_approval_active(self.current_activity.as_ref())
        {
            self.current_activity = None;
        }
        if kind == "thinking_delta" {
            self.status_thinking_bytes = self
                .status_thinking_bytes
                .saturating_add(payload.get("bytes").and_then(Value::as_u64).unwrap_or(0));
        }
        if kind == "approval_requested" {
            self.approval_action_focus_index = 0;
            self.approval_action_intent = None;
            self.approval_action_intent_sequence = 0;
        } else if !terminal_approval_active(self.current_activity.as_ref()) {
            self.approval_action_intent = None;
        }
        self.apply_terminal_widget_event(kind, payload);

        if let Some(policy) = value
            .get("refresh")
            .and_then(|refresh| refresh.get("policy"))
            .and_then(Value::as_str)
        {
            self.last_refresh_policy = policy.to_string();
            match policy {
                "immediate" => self.needs_immediate_render = true,
                "throttled" => self.pending_throttled_render = true,
                _ => {}
            }
        }

        if let Some(history) = value.get("history").and_then(Value::as_str) {
            self.last_history_policy = history.to_string();
            match history {
                "append" => self.append_count = self.append_count.saturating_add(1),
                "update_active" => {
                    self.update_active_count = self.update_active_count.saturating_add(1)
                }
                "freeze_history" => {
                    self.freeze_history_count = self.freeze_history_count.saturating_add(1)
                }
                _ => {}
            }
        }

        match kind {
            "turn_finished" => {
                self.terminal_finished = true;
                self.terminal_reason = payload
                    .get("terminal")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                self.terminal_success = self
                    .terminal_reason
                    .as_deref()
                    .and_then(terminal_success_from_terminal);
            }
            "final_summary_recorded" => {
                self.terminal_finished = true;
                self.terminal_success = payload.get("success").and_then(Value::as_bool);
                self.terminal_reason = payload
                    .get("terminal")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .or_else(|| self.terminal_reason.clone());
            }
            _ => {}
        }

        true
    }

    fn apply_terminal_widget_event(&mut self, kind: &str, payload: &Value) {
        if terminal_event_retires_slash_result(kind, payload) {
            self.retire_slash_result_widget_for_lifecycle();
        }

        match kind {
            "command_started" => {
                self.current_command_widget = Some(terminal_command_started_widget(payload));
            }
            "command_output" => {
                self.current_command_widget = Some(terminal_command_output_widget(
                    self.current_command_widget.take(),
                    payload,
                ));
            }
            "command_finished" => {
                let command_widget =
                    terminal_command_finished_widget(self.current_command_widget.take(), payload);
                self.record_terminal_command_history(&command_widget);
                self.current_command_widget = Some(command_widget);
            }
            "background_task_updated" => {
                self.upsert_background_task_widget(payload);
            }
            "plan_updated" => {
                self.current_plan_widget = Some(terminal_plan_widget(payload));
            }
            "file_change_summary" => {
                self.current_file_change_widget =
                    Some(terminal_file_change_widget("file_change_summary", payload));
            }
            "diff_available" => {
                self.current_diff_widget =
                    Some(terminal_file_change_widget("diff_available", payload));
            }
            "error_raised" => {
                self.current_error_widget = Some(terminal_error_widget(payload));
            }
            "api_retry" => {
                self.current_error_widget = Some(terminal_error_retry_widget(
                    self.current_error_widget.take(),
                    payload,
                ));
            }
            "final_summary_recorded" => {
                self.current_final_summary_widget = Some(terminal_final_summary_widget(
                    payload,
                    self.current_command_widget.as_ref(),
                    &self.terminal_command_history,
                    self.current_file_change_widget.as_ref(),
                    self.current_diff_widget.as_ref(),
                    self.current_error_widget.as_ref(),
                ));
            }
            "slash_command_result" => {
                self.current_slash_result_widget =
                    Some(terminal_slash_result_widget_from_payload(payload));
            }
            _ => {}
        }
    }

    fn upsert_background_task_widget(&mut self, payload: &Value) {
        let Some(task_id) = payload.get("taskId").and_then(Value::as_str) else {
            return;
        };
        let mut task = self
            .current_background_tasks
            .get(task_id)
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        task.insert("taskId".to_string(), json!(task_id));
        task.insert(
            "taskType".to_string(),
            payload
                .get("taskType")
                .cloned()
                .unwrap_or_else(|| json!("background_shell")),
        );
        task.insert(
            "taskStatus".to_string(),
            payload
                .get("taskStatus")
                .cloned()
                .unwrap_or_else(|| json!("updated")),
        );
        if let Some(tool_id) = payload.get("toolId") {
            task.insert("toolId".to_string(), tool_id.clone());
        }
        if let Some(command) = payload.get("command").filter(|value| !value.is_null()) {
            task.insert("command".to_string(), command.clone());
        }
        if let Some(exit_code) = payload.get("exitCode") {
            task.insert("exitCode".to_string(), exit_code.clone());
        }
        if let Some(preview_lines) = payload.get("previewLines") {
            task.insert("previewLines".to_string(), preview_lines.clone());
        }
        if let Some(hidden_lines) = payload.get("hiddenLines") {
            task.insert("hiddenLines".to_string(), hidden_lines.clone());
        }
        task.insert("updatedSequence".to_string(), json!(self.last_sequence));
        task.insert(
            "summary".to_string(),
            json!(terminal_background_task_summary(&task)),
        );
        self.current_background_tasks
            .insert(task_id.to_string(), Value::Object(task));
    }

    fn record_terminal_command_history(&mut self, command: &Value) {
        self.terminal_command_history
            .push(terminal_command_history_item(command));
        if self.terminal_command_history.len() > STREAM_JSON_RENDER_COMMAND_HISTORY_MAX_ITEMS {
            let excess = self
                .terminal_command_history
                .len()
                .saturating_sub(STREAM_JSON_RENDER_COMMAND_HISTORY_MAX_ITEMS);
            self.terminal_command_history.drain(..excess);
        }
    }

    fn should_emit_final_summary_for_result(&self, terminal: &str) -> bool {
        let success = UiStage::from_terminal(terminal) == UiStage::Done;
        !success || self.has_terminal_work_activity()
    }

    fn has_terminal_work_activity(&self) -> bool {
        self.current_command_widget.is_some()
            || !self.terminal_command_history.is_empty()
            || self.current_plan_widget.is_some()
            || !self.current_background_tasks.is_empty()
            || self.current_file_change_widget.is_some()
            || self.current_diff_widget.is_some()
            || self.current_error_widget.is_some()
            || self.current_slash_result_widget.is_some()
    }

    fn apply_visible_content_from_sdk_message(&mut self, message: &SdkMessage) {
        let mut saw_visible_text = false;
        let mut appended_text = false;
        for text in assistant_visible_text_segments(message) {
            saw_visible_text = true;
            appended_text |= self.append_assistant_text(text);
        }

        if saw_visible_text || appended_text {
            let preview_lines = terminal_text_preview_lines(
                &self.assistant_text_tail,
                STREAM_JSON_RENDER_VISIBLE_TEXT_PREVIEW_LINES,
            );
            if !preview_lines.is_empty() {
                if let Some(Value::Object(activity)) = self.current_activity.as_mut() {
                    activity.insert("previewLines".to_string(), json!(preview_lines));
                }
            }
        }
    }

    pub fn toggle_command_widget_expanded(&mut self) -> bool {
        if terminal_toggle_widget_expanded(self.current_command_widget.as_mut()) {
            self.mark_terminal_widget_control_dirty();
            true
        } else {
            false
        }
    }

    pub fn toggle_background_task_panel_expanded(&mut self) -> bool {
        if self.current_background_tasks.is_empty() {
            return false;
        }

        self.current_background_tasks_expanded = !self.current_background_tasks_expanded;
        self.mark_terminal_widget_control_dirty();
        true
    }

    pub fn toggle_file_change_widget_expanded(&mut self) -> bool {
        if terminal_toggle_widget_expanded(self.current_file_change_widget.as_mut()) {
            self.mark_terminal_widget_control_dirty();
            true
        } else {
            false
        }
    }

    pub fn toggle_diff_widget_expanded(&mut self) -> bool {
        if terminal_toggle_widget_expanded(self.current_diff_widget.as_mut()) {
            self.mark_terminal_widget_control_dirty();
            true
        } else {
            false
        }
    }

    pub fn toggle_error_widget_expanded(&mut self) -> bool {
        if terminal_toggle_widget_expanded(self.current_error_widget.as_mut()) {
            self.mark_terminal_widget_control_dirty();
            true
        } else {
            false
        }
    }

    pub fn focus_next_approval_action(&mut self) -> bool {
        if !terminal_approval_active(self.current_activity.as_ref()) {
            return false;
        }

        self.approval_action_focus_index =
            (self.approval_action_focus_index + 1) % STREAM_JSON_TERMINAL_APPROVAL_ACTION_COUNT;
        self.mark_terminal_widget_control_dirty();
        true
    }

    pub fn focus_previous_approval_action(&mut self) -> bool {
        if !terminal_approval_active(self.current_activity.as_ref()) {
            return false;
        }

        self.approval_action_focus_index = (self
            .approval_action_focus_index
            .saturating_add(STREAM_JSON_TERMINAL_APPROVAL_ACTION_COUNT - 1))
            % STREAM_JSON_TERMINAL_APPROVAL_ACTION_COUNT;
        self.mark_terminal_widget_control_dirty();
        true
    }

    pub fn activate_focused_approval_action(&mut self) -> bool {
        if !terminal_approval_active(self.current_activity.as_ref()) {
            return false;
        }

        self.record_approval_action_intent(self.approval_action_focus_index, "focused")
    }

    pub fn activate_approval_action_by_key(&mut self, key: char) -> bool {
        if !terminal_approval_active(self.current_activity.as_ref()) {
            return false;
        }

        let normalized = key.to_ascii_lowercase().to_string();
        let Some(index) = terminal_approval_action_specs()
            .iter()
            .position(|(_, _, shortcut)| *shortcut == normalized.as_str())
        else {
            return false;
        };

        self.approval_action_focus_index = index;
        self.record_approval_action_intent(index, "shortcut")
    }

    fn record_approval_action_intent(&mut self, focus_index: usize, source: &str) -> bool {
        let selected_index = focus_index % STREAM_JSON_TERMINAL_APPROVAL_ACTION_COUNT;
        let (action_id, label, key) = terminal_approval_action_specs()[selected_index];
        self.approval_action_intent_sequence =
            self.approval_action_intent_sequence.saturating_add(1);
        self.approval_action_intent = Some(json!({
            "available": true,
            "sequence": self.approval_action_intent_sequence,
            "action": action_id,
            "actionId": action_id,
            "label": label,
            "key": key,
            "source": source,
            "submitted": false,
            "renderOnly": true,
            "requiresDecisionBridge": true,
        }));
        self.mark_terminal_widget_control_dirty();
        true
    }

    fn mark_terminal_permission_request_context(&mut self, tool_name: &str, input: &Value) -> bool {
        if !terminal_approval_active(self.current_activity.as_ref()) {
            return false;
        }
        let preview_lines = terminal_permission_preview_lines(tool_name, input);
        let Some(Value::Object(activity)) = self.current_activity.as_mut() else {
            return false;
        };

        activity.insert(
            "inputPreview".to_string(),
            terminal_permission_preview_value(tool_name, input, &preview_lines),
        );
        activity.insert("inputRedacted".to_string(), Value::Bool(true));
        activity.insert(
            "inputPreviewLines".to_string(),
            Value::Array(
                preview_lines
                    .into_iter()
                    .map(Value::String)
                    .collect::<Vec<_>>(),
            ),
        );
        self.mark_terminal_widget_control_dirty();
        true
    }

    fn mark_approval_action_bridge_status(
        &mut self,
        bridge_status: &str,
        submitted: bool,
        requires_decision_bridge: bool,
    ) -> bool {
        if !terminal_approval_active(self.current_activity.as_ref()) {
            return false;
        }
        let submitted_activity = {
            let Some(Value::Object(intent)) = self.approval_action_intent.as_mut() else {
                return false;
            };

            let action = intent
                .get("action")
                .and_then(Value::as_str)
                .unwrap_or("approval_action")
                .to_string();
            let label = intent
                .get("label")
                .and_then(Value::as_str)
                .unwrap_or("Approval action")
                .to_string();
            intent.insert("submitted".to_string(), Value::Bool(submitted));
            intent.insert("renderOnly".to_string(), Value::Bool(false));
            intent.insert(
                "requiresDecisionBridge".to_string(),
                Value::Bool(requires_decision_bridge),
            );
            intent.insert(
                "bridgeStatus".to_string(),
                Value::String(bridge_status.to_string()),
            );

            if submitted && !requires_decision_bridge {
                Some(json!({
                    "kind": "approval_submitted",
                    "summary": format!("approval submitted: {label}"),
                    "approvalAction": action,
                    "bridgeStatus": bridge_status,
                    "submitted": true,
                    "blocking": false,
                    "independentRegion": false,
                }))
            } else {
                None
            }
        };

        if let Some(activity) = submitted_activity {
            self.current_activity = Some(activity);
        }
        self.mark_terminal_widget_control_dirty();
        true
    }

    fn mark_approval_edit_command_status(
        &mut self,
        bridge_status: &str,
        command: Option<&str>,
        editing: bool,
    ) -> bool {
        if !terminal_approval_active(self.current_activity.as_ref()) {
            return false;
        }
        let Some(Value::Object(intent)) = self.approval_action_intent.as_mut() else {
            return false;
        };

        intent.insert(
            "bridgeStatus".to_string(),
            Value::String(bridge_status.to_string()),
        );
        intent.insert("renderOnly".to_string(), Value::Bool(false));
        intent.insert("requiresDecisionBridge".to_string(), Value::Bool(true));
        intent.insert(
            "editMode".to_string(),
            json!({
                "active": editing,
                "commandPreview": command.map(terminal_clean_line),
                "empty": command.map(str::trim).map(str::is_empty).unwrap_or(false),
                "submitKey": "Enter",
                "cancelKey": "Esc",
            }),
        );
        self.mark_terminal_widget_control_dirty();
        true
    }

    fn mark_terminal_widget_control_dirty(&mut self) {
        self.needs_immediate_render = true;
        self.pending_throttled_render = false;
        self.last_refresh_policy = "immediate".to_string();
    }

    fn mark_slash_command_result_response(
        &mut self,
        request_id: &str,
        response: &Value,
        summary: &str,
        error: Option<&str>,
    ) {
        self.current_slash_result_widget = Some(terminal_slash_result_widget(
            request_id, response, summary, error,
        ));
        self.mark_terminal_widget_control_dirty();
    }

    fn retire_slash_result_widget_for_lifecycle(&mut self) {
        if self.current_slash_result_widget.take().is_some() {
            self.mark_terminal_widget_control_dirty();
        }
    }

    fn append_assistant_text(&mut self, text: &str) -> bool {
        let Some(text) = self.assistant_text_segment_not_already_recorded(text) else {
            return false;
        };
        self.append_assistant_text_tail(text);
        self.append_assistant_text_transcript(text);
        true
    }

    fn assistant_text_segment_not_already_recorded<'a>(&self, text: &'a str) -> Option<&'a str> {
        if text.is_empty() {
            return None;
        }
        let transcript = self.assistant_text_transcript.as_str();
        if transcript.is_empty() {
            return Some(text);
        }
        if transcript.ends_with(text) || text.ends_with(transcript) {
            return None;
        }
        if let Some(suffix) = text.strip_prefix(transcript) {
            return (!suffix.is_empty()).then_some(suffix);
        }
        Some(text)
    }

    fn append_assistant_text_tail(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.assistant_text_tail.push_str(text);
        if self.assistant_text_tail.len() <= STREAM_JSON_RENDER_VISIBLE_TEXT_TAIL_BYTES {
            return;
        }

        let excess = self
            .assistant_text_tail
            .len()
            .saturating_sub(STREAM_JSON_RENDER_VISIBLE_TEXT_TAIL_BYTES);
        let cut = self
            .assistant_text_tail
            .char_indices()
            .find_map(|(idx, _)| (idx >= excess).then_some(idx))
            .unwrap_or(self.assistant_text_tail.len());
        self.assistant_text_tail.drain(..cut);
    }

    fn append_assistant_text_transcript(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.assistant_text_transcript.push_str(text);
        if self.assistant_text_transcript.len() <= STREAM_JSON_RENDER_TRANSCRIPT_MAX_BYTES {
            return;
        }

        let excess = self
            .assistant_text_transcript
            .len()
            .saturating_sub(STREAM_JSON_RENDER_TRANSCRIPT_MAX_BYTES);
        let cut = self
            .assistant_text_transcript
            .char_indices()
            .find_map(|(idx, _)| (idx >= excess).then_some(idx))
            .unwrap_or(self.assistant_text_transcript.len());
        self.assistant_text_transcript.drain(..cut);
        self.assistant_text_transcript_omitted_bytes = self
            .assistant_text_transcript_omitted_bytes
            .saturating_add(cut);
    }

    pub fn snapshot_value(&self) -> Value {
        let status_bar = terminal_status_bar_value(
            self,
            terminal_stage_label(&self.current_stage),
            &terminal_scope_label(&self.current_scope),
        );
        json!({
            "type": STREAM_JSON_RENDER_SNAPSHOT_TYPE,
            "subtype": STREAM_JSON_RENDER_SNAPSHOT_TYPE,
            "schemaVersion": STREAM_JSON_RENDER_SNAPSHOT_SCHEMA_VERSION,
            "eventSchemaVersion": STREAM_JSON_RENDER_EVENT_SCHEMA_VERSION,
            "lastSequence": self.last_sequence,
            "appliedCount": self.applied_count,
            "ignoredStaleCount": self.ignored_stale_count,
            "stage": self.current_stage,
            "scope": self.current_scope.clone(),
            "activity": self.current_activity.clone(),
            "refresh": {
                "lastPolicy": self.last_refresh_policy,
                "needsImmediateRender": self.needs_immediate_render,
                "pendingThrottledRender": self.pending_throttled_render,
                "throttleMs": STREAM_JSON_RENDER_EVENT_THROTTLE_MS,
            },
            "history": {
                "lastPolicy": self.last_history_policy,
                "appendCount": self.append_count,
                "updateActiveCount": self.update_active_count,
                "freezeHistoryCount": self.freeze_history_count,
                "scrollStable": true,
                "preserveScrollOnUpdateActive": true,
            },
            "terminal": {
                "finished": self.terminal_finished,
                "success": self.terminal_success,
                "reason": self.terminal_reason,
                "statusBar": status_bar,
                "command": {
                    "available": self.current_command_widget.is_some(),
                    "summaryOnly": self
                        .current_command_widget
                        .as_ref()
                        .is_some_and(|widget| !terminal_widget_expanded(widget)),
                    "expanded": self
                        .current_command_widget
                        .as_ref()
                        .is_some_and(terminal_widget_expanded),
                    "widget": self.current_command_widget.clone(),
                    "history": self.terminal_command_history.clone(),
                    "historyCount": self.terminal_command_history.len(),
                    "historyRetainedMax": STREAM_JSON_RENDER_COMMAND_HISTORY_MAX_ITEMS,
                },
                "plan": {
                    "available": self.current_plan_widget.is_some(),
                    "independentRegion": self.current_plan_widget.is_some(),
                    "widget": self.current_plan_widget.clone(),
                },
                "backgroundTasks": {
                    "available": !self.current_background_tasks.is_empty(),
                    "count": self.current_background_tasks.len(),
                    "maxItems": STREAM_JSON_RENDER_BACKGROUND_TASK_MAX_ITEMS,
                    "expandedMaxItems": STREAM_JSON_RENDER_BACKGROUND_TASK_EXPANDED_MAX_ITEMS,
                    "collapsedByDefault": !self.current_background_tasks.is_empty(),
                    "summaryOnly": !self.current_background_tasks.is_empty() && !self.current_background_tasks_expanded,
                    "expanded": self.current_background_tasks_expanded,
                    "items": terminal_background_task_items_value(&self.current_background_tasks),
                    "expandedItems": terminal_background_task_items_value_with_limit(
                        &self.current_background_tasks,
                        STREAM_JSON_RENDER_BACKGROUND_TASK_EXPANDED_MAX_ITEMS
                    ),
                },
                "fileChanges": {
                    "available": self.current_file_change_widget.is_some(),
                    "independentRegion": self.current_file_change_widget.is_some(),
                    "collapsedByDefault": self.current_file_change_widget.is_some(),
                    "summaryOnly": self
                        .current_file_change_widget
                        .as_ref()
                        .is_some_and(|widget| !terminal_widget_expanded(widget)),
                    "expanded": self
                        .current_file_change_widget
                        .as_ref()
                        .is_some_and(terminal_widget_expanded),
                    "widget": self.current_file_change_widget.clone(),
                },
                "diff": {
                    "available": self.current_diff_widget.is_some(),
                    "collapsedByDefault": self.current_diff_widget.is_some(),
                    "expanded": self
                        .current_diff_widget
                        .as_ref()
                        .is_some_and(terminal_widget_expanded),
                    "widget": self.current_diff_widget.clone(),
                },
                "error": {
                    "available": self.current_error_widget.is_some(),
                    "layered": self.current_error_widget.is_some(),
                    "collapsedByDefault": self.current_error_widget.is_some(),
                    "summaryOnly": self
                        .current_error_widget
                        .as_ref()
                        .is_some_and(|widget| !terminal_widget_expanded(widget)),
                    "expanded": self
                        .current_error_widget
                        .as_ref()
                        .is_some_and(terminal_widget_expanded),
                    "detailsAvailable": self.current_error_widget
                        .as_ref()
                        .and_then(|widget| widget.get("detailsAvailable"))
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    "widget": self.current_error_widget.clone(),
                },
                "finalSummary": {
                    "available": self.current_final_summary_widget.is_some(),
                    "independentRegion": self.current_final_summary_widget.is_some(),
                    "widget": self.current_final_summary_widget.clone(),
                },
                "slashResult": {
                    "available": self.current_slash_result_widget.is_some(),
                    "independentRegion": self.current_slash_result_widget.is_some(),
                    "boundedPreview": self.current_slash_result_widget.is_some(),
                    "rawResponseIncluded": false,
                    "redacted": true,
                    "widget": self.current_slash_result_widget.clone(),
                },
                "approval": {
                    "blocking": terminal_approval_active(self.current_activity.as_ref()),
                    "independentRegion": terminal_approval_active(self.current_activity.as_ref()),
                    "toolName": terminal_approval_tool_name(self.current_activity.as_ref()),
                    "inputPreview": terminal_approval_input_preview_value(self.current_activity.as_ref()),
                    "actionModel": terminal_approval_action_model_value(
                        self.current_activity.as_ref(),
                        self.approval_action_focus_index,
                        self.approval_action_intent.as_ref(),
                    ),
                },
                "interaction": terminal_interaction_value(self),
                "transcript": {
                    "available": !self.assistant_text_transcript.trim().is_empty(),
                    "bytes": self.assistant_text_transcript.len(),
                    "omittedBytes": self.assistant_text_transcript_omitted_bytes,
                    "maxBytes": STREAM_JSON_RENDER_TRANSCRIPT_MAX_BYTES,
                },
            },
        })
    }

    pub fn terminal_frame_value(&self) -> Value {
        self.terminal_frame_value_with_previous(None).0
    }

    fn terminal_frame_value_with_previous(
        &self,
        previous: Option<&StreamJsonRenderFrameFingerprint>,
    ) -> (Value, StreamJsonRenderFrameFingerprint) {
        let scope_label = terminal_scope_label(&self.current_scope);
        let stage_label = terminal_stage_label(&self.current_stage);
        let status_bar = terminal_status_bar_value(self, stage_label, &scope_label);
        let status_line = status_bar
            .get("line")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| terminal_clean_line(&format!("{stage_label} | {scope_label}")));
        let status_line_variants = terminal_status_line_variants(&status_bar);
        let approval_lines = terminal_approval_lines(
            self.current_activity.as_ref(),
            self.approval_action_focus_index,
            self.approval_action_intent.as_ref(),
        );
        let approval_active = approval_lines.is_some();
        let plan_lines = terminal_plan_lines(self.current_plan_widget.as_ref());
        let plan_active = plan_lines.is_some();
        let plan_activity_active = self
            .current_activity
            .as_ref()
            .and_then(|activity| activity.get("kind"))
            .and_then(Value::as_str)
            == Some("plan_updated");
        let command_lines = terminal_command_lines(self.current_command_widget.as_ref());
        let command_active = command_lines.is_some();
        let background_task_lines = terminal_background_task_lines(
            &self.current_background_tasks,
            self.current_background_tasks_expanded,
        );
        let background_task_active = background_task_lines.is_some();
        let file_change_lines =
            terminal_file_change_lines(self.current_file_change_widget.as_ref());
        let file_change_active = file_change_lines.is_some();
        let diff_lines = terminal_diff_lines(self.current_diff_widget.as_ref());
        let diff_active = diff_lines.is_some();
        let error_lines = terminal_error_lines(self.current_error_widget.as_ref());
        let error_active = error_lines.is_some();
        let final_summary_lines =
            terminal_final_summary_lines(self.current_final_summary_widget.as_ref());
        let final_summary_active = final_summary_lines.is_some();
        let slash_result_lines =
            terminal_slash_result_lines(self.current_slash_result_widget.as_ref());
        let slash_result_active = slash_result_lines.is_some();
        let slash_result_activity_active = terminal_slash_result_activity_active(
            self.current_activity.as_ref(),
            slash_result_active,
        );
        let active_lines = if approval_active
            || error_active
            || final_summary_active
            || plan_activity_active
            || slash_result_activity_active
        {
            Vec::new()
        } else {
            terminal_activity_lines(self.current_activity.as_ref())
        };
        let footer_line_variants = terminal_footer_line_variants(self);
        let footer_line = footer_line_variants
            .get("compact")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| terminal_footer_line(self));
        let draw_mode = terminal_draw_mode(&self.last_refresh_policy);
        let active_update_mode = terminal_active_update_mode(&self.last_history_policy);
        let transcript_lines = terminal_transcript_lines(self);
        let commits_scrollback = !transcript_lines.is_empty();
        let mut status_region = terminal_frame_region(
            "status",
            "status",
            "top",
            "replace",
            vec![status_line.clone()],
        );
        terminal_frame_region_attach_line_variants(
            &mut status_region,
            status_line_variants,
            "status_bar_width_profile",
        );
        let mut footer_region =
            terminal_frame_region("footer", "footer", "bottom", "replace", vec![footer_line]);
        terminal_frame_region_attach_line_variants(
            &mut footer_region,
            footer_line_variants,
            "footer_hint_width_profile",
        );

        let mut regions = vec![
            status_region,
            terminal_frame_region(
                "active",
                "activity",
                "bottom",
                active_update_mode,
                active_lines,
            ),
            footer_region,
        ];
        if let Some(error_lines) = error_lines {
            regions.push(terminal_frame_region(
                "error",
                "error",
                "top",
                terminal_error_update_mode(self.current_error_widget.as_ref()),
                error_lines,
            ));
        }
        if let Some(final_summary_lines) = final_summary_lines {
            regions.push(terminal_frame_region(
                "final_summary",
                "final_summary",
                "top",
                "replace_final_summary",
                final_summary_lines,
            ));
        }
        if let Some(slash_result_lines) = slash_result_lines {
            regions.push(terminal_frame_region(
                "slash_result",
                "slash_result",
                "top",
                "replace_slash_result",
                slash_result_lines,
            ));
        }
        if let Some(plan_lines) = plan_lines {
            regions.push(terminal_frame_region(
                "plan",
                "plan",
                "top",
                "replace_plan",
                plan_lines,
            ));
        }
        if let Some(command_lines) = command_lines {
            regions.push(terminal_frame_region(
                "command",
                "command",
                "top",
                terminal_command_update_mode(self.current_command_widget.as_ref()),
                command_lines,
            ));
        }
        if let Some(background_task_lines) = background_task_lines {
            regions.push(terminal_frame_region(
                "background_tasks",
                "background_tasks",
                "top",
                terminal_background_task_update_mode(self.current_background_tasks_expanded),
                background_task_lines,
            ));
        }
        if let Some(file_change_lines) = file_change_lines {
            regions.push(terminal_frame_region(
                "file_changes",
                "file_changes",
                "top",
                terminal_file_change_update_mode(self.current_file_change_widget.as_ref()),
                file_change_lines,
            ));
        }
        if let Some(diff_lines) = diff_lines {
            regions.push(terminal_frame_region(
                "diff",
                "diff",
                "top",
                terminal_diff_update_mode(self.current_diff_widget.as_ref()),
                diff_lines,
            ));
        }
        if let Some(approval_lines) = approval_lines {
            regions.push(terminal_frame_region(
                "approval",
                "approval",
                "bottom",
                "replace_blocking",
                approval_lines,
            ));
        }
        if commits_scrollback {
            regions.push(terminal_frame_region(
                "transcript",
                "transcript",
                "scrollback",
                "append_scrollback",
                transcript_lines,
            ));
        }
        let frame_hash = stable_render_frame_hash(&regions);
        let region_fingerprints = render_frame_region_fingerprints(&regions);
        let fingerprint = StreamJsonRenderFrameFingerprint {
            frame_hash: frame_hash.clone(),
            region_hashes: render_frame_region_hashes(&region_fingerprints),
            regions: region_fingerprints,
        };
        let first_frame = previous.is_none();
        let previous_frame_hash = previous
            .map(|fingerprint| fingerprint.frame_hash.clone())
            .unwrap_or_default();
        let (changed_region_ids, unchanged_region_ids, removed_region_ids, retired_regions) =
            render_frame_region_delta(&regions, previous);
        let dirty = (self.needs_immediate_render
            || self.pending_throttled_render
            || self.terminal_finished)
            && !changed_region_ids.is_empty();
        let region_hashes = render_frame_region_hash_object(&fingerprint);
        let mut frame = json!({
            "type": STREAM_JSON_RENDER_FRAME_TYPE,
            "subtype": STREAM_JSON_RENDER_FRAME_TYPE,
            "schemaVersion": STREAM_JSON_RENDER_FRAME_SCHEMA_VERSION,
            "eventSchemaVersion": STREAM_JSON_RENDER_EVENT_SCHEMA_VERSION,
            "snapshotSchemaVersion": STREAM_JSON_RENDER_SNAPSHOT_SCHEMA_VERSION,
            "sequence": self.last_sequence,
            "frameId": format!("render-frame-{:08}", self.last_sequence),
            "frameHash": frame_hash,
            "stage": self.current_stage,
            "scope": self.current_scope.clone(),
            "status": {
                "label": stage_label,
                "line": status_line,
                "bar": status_bar.clone(),
                "blocking": approval_active,
                "terminal": self.terminal_finished,
            },
            "regions": regions,
            "draw": {
                "dirty": dirty,
                "mode": draw_mode,
                "preferredStrategy": "patch_regions",
                "replaceWholeScreen": false,
                "skipIfFrameHashUnchanged": true,
                "changedRegionIds": changed_region_ids,
                "unchangedRegionIds": unchanged_region_ids,
                "activeRegionId": "active",
                "statusRegionId": "status",
                "footerRegionId": "footer",
                "approvalRegionId": if approval_active { "approval" } else { "" },
                "planRegionId": if plan_active { "plan" } else { "" },
                "commandRegionId": if command_active { "command" } else { "" },
                "backgroundTaskRegionId": if background_task_active { "background_tasks" } else { "" },
                "fileChangeRegionId": if file_change_active { "file_changes" } else { "" },
                "diffRegionId": if diff_active { "diff" } else { "" },
                "errorRegionId": if error_active { "error" } else { "" },
                "finalSummaryRegionId": if final_summary_active { "final_summary" } else { "" },
                "slashResultRegionId": if slash_result_active { "slash_result" } else { "" },
                "blockingRegionIds": if approval_active { json!(["approval"]) } else { json!([]) },
                "scrollbackRegionId": if commits_scrollback { "transcript" } else { "" },
                "removedRegionIds": removed_region_ids,
            },
            "changes": {
                "firstFrame": first_frame,
                "previousFrameHash": previous_frame_hash,
                "currentFrameHash": frame_hash,
                "regionHashes": region_hashes,
                "changedRegionIds": changed_region_ids,
                "unchangedRegionIds": unchanged_region_ids,
                "removedRegionIds": removed_region_ids,
                "retiredRegions": retired_regions,
                "skipDrawWhenUnchanged": true,
            },
            "refresh": {
                "lastPolicy": self.last_refresh_policy,
                "needsImmediateRender": self.needs_immediate_render,
                "pendingThrottledRender": self.pending_throttled_render,
                "throttleMs": STREAM_JSON_RENDER_EVENT_THROTTLE_MS,
            },
            "scroll": {
                "stable": true,
                "historyPolicy": self.last_history_policy,
                "preserveOnActiveUpdate": true,
                "stickyFollowsTail": self.last_history_policy != "freeze_history",
                "commitToScrollback": commits_scrollback,
                "appendOnce": commits_scrollback,
            },
            "terminal": {
                "finished": self.terminal_finished,
                "success": self.terminal_success,
                "reason": self.terminal_reason,
                "statusBar": status_bar,
                "command": {
                    "available": command_active,
                    "summaryOnly": self
                        .current_command_widget
                        .as_ref()
                        .is_some_and(|widget| !terminal_widget_expanded(widget)),
                    "expanded": self
                        .current_command_widget
                        .as_ref()
                        .is_some_and(terminal_widget_expanded),
                    "widget": self.current_command_widget.clone(),
                    "history": self.terminal_command_history.clone(),
                    "historyCount": self.terminal_command_history.len(),
                    "historyRetainedMax": STREAM_JSON_RENDER_COMMAND_HISTORY_MAX_ITEMS,
                },
                "plan": {
                    "available": plan_active,
                    "independentRegion": plan_active,
                    "widget": self.current_plan_widget.clone(),
                },
                "backgroundTasks": {
                    "available": background_task_active,
                    "count": self.current_background_tasks.len(),
                    "maxItems": STREAM_JSON_RENDER_BACKGROUND_TASK_MAX_ITEMS,
                    "expandedMaxItems": STREAM_JSON_RENDER_BACKGROUND_TASK_EXPANDED_MAX_ITEMS,
                    "collapsedByDefault": background_task_active,
                    "summaryOnly": background_task_active && !self.current_background_tasks_expanded,
                    "expanded": self.current_background_tasks_expanded,
                    "items": terminal_background_task_items_value(&self.current_background_tasks),
                    "expandedItems": terminal_background_task_items_value_with_limit(
                        &self.current_background_tasks,
                        STREAM_JSON_RENDER_BACKGROUND_TASK_EXPANDED_MAX_ITEMS
                    ),
                },
                "fileChanges": {
                    "available": file_change_active,
                    "independentRegion": file_change_active,
                    "collapsedByDefault": file_change_active,
                    "summaryOnly": self
                        .current_file_change_widget
                        .as_ref()
                        .is_some_and(|widget| !terminal_widget_expanded(widget)),
                    "expanded": self
                        .current_file_change_widget
                        .as_ref()
                        .is_some_and(terminal_widget_expanded),
                    "widget": self.current_file_change_widget.clone(),
                },
                "diff": {
                    "available": diff_active,
                    "collapsedByDefault": diff_active,
                    "expanded": self
                        .current_diff_widget
                        .as_ref()
                        .is_some_and(terminal_widget_expanded),
                    "widget": self.current_diff_widget.clone(),
                },
                "error": {
                    "available": error_active,
                    "layered": error_active,
                    "collapsedByDefault": error_active,
                    "summaryOnly": self
                        .current_error_widget
                        .as_ref()
                        .is_some_and(|widget| !terminal_widget_expanded(widget)),
                    "expanded": self
                        .current_error_widget
                        .as_ref()
                        .is_some_and(terminal_widget_expanded),
                    "detailsAvailable": self.current_error_widget
                        .as_ref()
                        .and_then(|widget| widget.get("detailsAvailable"))
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    "widget": self.current_error_widget.clone(),
                },
                "finalSummary": {
                    "available": final_summary_active,
                    "independentRegion": final_summary_active,
                    "widget": self.current_final_summary_widget.clone(),
                },
                "slashResult": {
                    "available": slash_result_active,
                    "independentRegion": slash_result_active,
                    "boundedPreview": slash_result_active,
                    "rawResponseIncluded": false,
                    "redacted": true,
                    "widget": self.current_slash_result_widget.clone(),
                },
                "approval": {
                    "blocking": approval_active,
                    "independentRegion": approval_active,
                    "toolName": terminal_approval_tool_name(self.current_activity.as_ref()),
                    "inputPreview": terminal_approval_input_preview_value(self.current_activity.as_ref()),
                    "actionModel": terminal_approval_action_model_value(
                        self.current_activity.as_ref(),
                        self.approval_action_focus_index,
                        self.approval_action_intent.as_ref(),
                    ),
                },
                "interaction": terminal_interaction_value(self),
                "transcript": {
                    "committedToScrollback": commits_scrollback,
                    "bytes": self.assistant_text_transcript.len(),
                    "omittedBytes": self.assistant_text_transcript_omitted_bytes,
                    "maxBytes": STREAM_JSON_RENDER_TRANSCRIPT_MAX_BYTES,
                },
            },
        });
        if let Value::Object(map) = &mut frame {
            map.insert(
                "frameHash".to_string(),
                Value::String(fingerprint.frame_hash.clone()),
            );
        }
        (frame, fingerprint)
    }
}

impl Default for StreamJsonRenderStreamState {
    fn default() -> Self {
        Self::new()
    }
}

fn render_snapshot_activity(kind: &str, payload: &Value) -> Option<Value> {
    match kind {
        "text_delta" => Some(json!({
            "kind": "assistant_message",
            "summary": format_bytes_summary("assistant text", payload),
            "bytes": payload.get("bytes").and_then(Value::as_u64),
        })),
        "thinking_delta" => Some(json!({
            "kind": "thinking",
            "summary": format_bytes_summary("thinking", payload),
            "bytes": payload.get("bytes").and_then(Value::as_u64),
        })),
        "tool_input_delta" => Some(json!({
            "kind": "tool_input",
            "summary": format_bytes_summary("tool input", payload),
            "bytes": payload.get("bytes").and_then(Value::as_u64),
        })),
        "command_started" => Some(json!({
            "kind": "command_started",
            "summary": payload
                .get("command")
                .and_then(Value::as_str)
                .map(|command| format!("command started: {command}"))
                .unwrap_or_else(|| "command started".to_string()),
            "toolId": payload.get("toolId").cloned().unwrap_or(Value::Null),
        })),
        "command_output" => {
            let stream = payload
                .get("stream")
                .and_then(Value::as_str)
                .unwrap_or("output");
            let preview_lines = payload
                .get("previewLines")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let hidden_lines = payload
                .get("hiddenLines")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            Some(json!({
                "kind": "command_output",
                "summary": format!("{stream}: {preview_lines} shown, {hidden_lines} hidden"),
                "stream": stream,
                "bytes": payload.get("bytes").and_then(Value::as_u64),
                "toolId": payload.get("toolId").cloned().unwrap_or(Value::Null),
            }))
        }
        "command_finished" => Some(json!({
            "kind": "command_finished",
            "summary": format!(
                "command finished: {}",
                payload
                    .get("exitCode")
                    .and_then(Value::as_i64)
                    .map(|code| format!("exit {code}"))
                    .unwrap_or_else(|| "exit unknown".to_string())
            ),
            "toolId": payload.get("toolId").cloned().unwrap_or(Value::Null),
        })),
        "background_task_updated" => Some(json!({
            "kind": "background_task_updated",
            "summary": payload
                .get("taskId")
                .and_then(Value::as_str)
                .map(|task_id| {
                    let status = payload
                        .get("taskStatus")
                        .and_then(Value::as_str)
                        .unwrap_or("updated");
                    format!("background task {status}: {task_id}")
                })
                .unwrap_or_else(|| "background task updated".to_string()),
            "taskId": payload.get("taskId").cloned().unwrap_or(Value::Null),
            "taskStatus": payload.get("taskStatus").cloned().unwrap_or(Value::Null),
        })),
        "tool_requested" | "tool_completed" => Some(json!({
            "kind": kind,
            "summary": payload
                .get("toolName")
                .and_then(Value::as_str)
                .map(|tool_name| format!("{kind}: {tool_name}"))
                .unwrap_or_else(|| kind.to_string()),
            "toolName": payload.get("toolName").cloned().unwrap_or(Value::Null),
            "toolId": payload.get("toolId").cloned().unwrap_or(Value::Null),
        })),
        "plan_updated" => Some(json!({
            "kind": "plan_updated",
            "summary": format!(
                "plan updated: {} step(s), {} completed",
                payload.get("stepCount").and_then(Value::as_u64).unwrap_or(0),
                payload
                    .get("completedCount")
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
            ),
            "activeStep": payload.get("activeStep").cloned().unwrap_or(Value::Null),
        })),
        "file_change_summary" | "diff_available" => Some(json!({
            "kind": kind,
            "summary": format!(
                "{}: {} file(s), +{} -{}",
                kind,
                payload.get("fileCount").and_then(Value::as_u64).unwrap_or(0),
                payload.get("additions").and_then(Value::as_u64).unwrap_or(0),
                payload.get("deletions").and_then(Value::as_u64).unwrap_or(0)
            ),
        })),
        "approval_requested" => Some(json!({
            "kind": "approval_requested",
            "summary": payload
                .get("toolName")
                .and_then(Value::as_str)
                .map(|tool_name| format!("approval requested: {tool_name}"))
                .unwrap_or_else(|| "approval requested".to_string()),
            "toolName": payload.get("toolName").cloned().unwrap_or(Value::Null),
            "blocking": true,
            "decisions": ["approve_once", "reject", "edit_command", "approve_for_session"],
            "independentRegion": true,
        })),
        "error_raised" => Some(json!({
            "kind": "error",
            "summary": payload
                .get("summary")
                .and_then(Value::as_str)
                .unwrap_or("error"),
            "source": payload.get("source").cloned().unwrap_or(Value::Null),
        })),
        "api_retry" => Some(json!({
            "kind": "api_retry",
            "summary": format!(
                "api retry {}/{}",
                payload.get("attempt").and_then(Value::as_u64).unwrap_or(0),
                payload.get("maxRetries").and_then(Value::as_u64).unwrap_or(0)
            ),
        })),
        "compact_boundary" => Some(json!({
            "kind": "compact_boundary",
            "summary": format!(
                "compact boundary: {} -> {} tokens",
                payload
                    .get("beforeTokenCount")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                payload
                    .get("afterTokenCount")
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
            ),
        })),
        "compact_request_status" => {
            let status = payload
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("updated");
            let request_id = payload
                .get("requestId")
                .and_then(Value::as_str)
                .unwrap_or("-");
            Some(json!({
                "kind": "compact_request_status",
                "summary": format!("compact request {status}: {request_id}"),
                "requestId": payload.get("requestId").cloned().unwrap_or(Value::Null),
                "status": payload.get("status").cloned().unwrap_or(Value::Null),
                "dryRun": payload.get("dryRun").cloned().unwrap_or(Value::Null),
                "reason": payload.get("reason").cloned().unwrap_or(Value::Null),
            }))
        }
        "conversation_cleared" => Some(json!({
            "kind": "conversation_cleared",
            "summary": format!(
                "conversation cleared: {} -> {} messages",
                payload
                    .get("messageCountBefore")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                payload
                    .get("messageCountAfter")
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
            ),
        })),
        "clear_request_status" => {
            let status = payload
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("updated");
            let request_id = payload
                .get("requestId")
                .and_then(Value::as_str)
                .unwrap_or("-");
            Some(json!({
                "kind": "clear_request_status",
                "summary": format!("clear request {status}: {request_id}"),
                "requestId": payload.get("requestId").cloned().unwrap_or(Value::Null),
                "status": payload.get("status").cloned().unwrap_or(Value::Null),
                "dryRun": payload.get("dryRun").cloned().unwrap_or(Value::Null),
                "reason": payload.get("reason").cloned().unwrap_or(Value::Null),
            }))
        }
        "slash_command_result" => Some(json!({
            "kind": "slash_command_result",
            "summary": payload
                .get("summary")
                .and_then(Value::as_str)
                .unwrap_or("slash command result"),
            "requestId": payload.get("requestId").cloned().unwrap_or(Value::Null),
            "command": payload.get("command").cloned().unwrap_or(Value::Null),
            "status": payload.get("status").cloned().unwrap_or(Value::Null),
            "error": payload.get("error").cloned().unwrap_or(Value::Null),
        })),
        "turn_finished" => Some(json!({
            "kind": "turn_finished",
            "summary": payload
                .get("terminal")
                .and_then(Value::as_str)
                .map(|terminal| format!("turn finished: {terminal}"))
                .unwrap_or_else(|| "turn finished".to_string()),
        })),
        "final_summary_recorded" => Some(json!({
            "kind": "final_summary",
            "summary": if payload
                .get("success")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                "final summary recorded: success".to_string()
            } else {
                payload
                    .get("terminal")
                    .and_then(Value::as_str)
                    .map(|terminal| format!("final summary recorded: {terminal}"))
                    .unwrap_or_else(|| "final summary recorded".to_string())
            },
            "success": payload.get("success").and_then(Value::as_bool),
        })),
        _ => None,
    }
}

fn format_bytes_summary(label: &str, payload: &Value) -> String {
    let bytes = payload.get("bytes").and_then(Value::as_u64).unwrap_or(0);
    format!("{label}: {bytes} bytes")
}

fn assistant_visible_text_segments(message: &SdkMessage) -> Vec<&str> {
    match message {
        SdkMessage::Assistant { message, .. } => message
            .content
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text(text) if !text.text.is_empty() => Some(text.text.as_str()),
                _ => None,
            })
            .collect(),
        SdkMessage::StreamEvent {
            event:
                StreamEventData::ContentBlockDelta {
                    delta: ContentDelta::TextDelta { text },
                    ..
                },
            ..
        } if !text.is_empty() => vec![text.as_str()],
        _ => Vec::new(),
    }
}

fn terminal_text_preview_lines(text: &str, max_lines: usize) -> Vec<String> {
    if max_lines == 0 {
        return Vec::new();
    }
    let mut lines = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(terminal_clean_line)
        .collect::<Vec<_>>();
    if lines.is_empty() && !text.trim().is_empty() {
        lines.push(terminal_clean_line(text));
    }
    if lines.len() > max_lines {
        lines.split_off(lines.len() - max_lines)
    } else {
        lines
    }
}

fn terminal_text_head_preview_lines(text: &str, max_lines: usize) -> Vec<String> {
    if max_lines == 0 {
        return Vec::new();
    }
    let mut lines = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(terminal_clean_line)
        .take(max_lines)
        .collect::<Vec<_>>();
    if lines.is_empty() && !text.trim().is_empty() {
        lines.push(terminal_clean_line(text));
    }
    lines
}

fn terminal_diff_text_preview_lines(text: &str, max_lines: usize) -> Vec<String> {
    if max_lines == 0 {
        return Vec::new();
    }
    let all_lines = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(terminal_clean_line)
        .collect::<Vec<_>>();
    if all_lines.is_empty() && !text.trim().is_empty() {
        return vec![terminal_clean_line(text)];
    }
    let start = all_lines
        .iter()
        .position(|line| line.starts_with("@@"))
        .unwrap_or(0);
    all_lines.into_iter().skip(start).take(max_lines).collect()
}

fn terminal_preview_lines_from_value(value: &Value, max_lines: usize) -> Vec<String> {
    let mut lines = Vec::new();
    terminal_append_preview_lines_from_value(value, max_lines, &mut lines);
    lines
}

fn terminal_append_preview_lines_from_value(
    value: &Value,
    max_lines: usize,
    lines: &mut Vec<String>,
) {
    if lines.len() >= max_lines {
        return;
    }
    match value {
        Value::String(text) => {
            let remaining = max_lines.saturating_sub(lines.len());
            lines.extend(terminal_text_preview_lines(text, remaining));
        }
        Value::Array(items) => {
            for item in items {
                terminal_append_preview_lines_from_value(item, max_lines, lines);
                if lines.len() >= max_lines {
                    break;
                }
            }
        }
        Value::Object(object) => {
            for key in [
                "lines", "preview", "text", "content", "output", "stdout", "stderr",
            ] {
                if let Some(value) = object.get(key) {
                    terminal_append_preview_lines_from_value(value, max_lines, lines);
                    if lines.len() >= max_lines {
                        break;
                    }
                }
            }
        }
        _ => {}
    }
}

fn terminal_command_preview_line_items(
    payload: &Value,
    stream: &str,
    max_lines: usize,
) -> Vec<String> {
    for key in [
        "previewLineItems",
        "previewLines",
        "outputPreviewLines",
        "linePreview",
        "outputLines",
    ] {
        if let Some(value) = payload.get(key) {
            let lines = terminal_preview_lines_from_value(value, max_lines);
            if !lines.is_empty() {
                return lines;
            }
        }
    }

    for key in [
        format!("{stream}PreviewLines"),
        format!("{stream}Preview"),
        stream.to_string(),
        "output".to_string(),
        "text".to_string(),
        "content".to_string(),
    ] {
        if let Some(value) = payload.get(&key) {
            let lines = terminal_preview_lines_from_value(value, max_lines);
            if !lines.is_empty() {
                return lines;
            }
        }
    }

    Vec::new()
}

fn terminal_command_output_line_count(
    payload: &Value,
    preview_line_count: u64,
    hidden_lines: u64,
    fallback_preview_items: usize,
) -> u64 {
    let payload_line_count = preview_line_count.saturating_add(hidden_lines);
    if payload_line_count > 0 {
        return payload_line_count;
    }

    payload
        .get("totalLines")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| u64::try_from(fallback_preview_items).unwrap_or(0))
}

fn terminal_command_tail_line_items(
    map: &Map<String, Value>,
    key: &str,
    stream: &str,
    incoming: &[String],
    limit: usize,
) -> Vec<String> {
    let previous_stream = map
        .get("outputTailStream")
        .and_then(Value::as_str)
        .unwrap_or(stream);
    let mut lines = if previous_stream == stream {
        terminal_string_array_field(map, key)
    } else {
        Vec::new()
    };

    lines.extend(incoming.iter().map(|line| terminal_clean_line(line)));
    if lines.len() > limit {
        lines.split_off(lines.len() - limit)
    } else {
        lines
    }
}

fn terminal_string_array_field(map: &Map<String, Value>, key: &str) -> Vec<String> {
    map.get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(terminal_clean_line)
        .collect()
}

fn terminal_widget_expanded(widget: &Value) -> bool {
    widget
        .get("expanded")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn terminal_toggle_widget_expanded(widget: Option<&mut Value>) -> bool {
    let Some(Value::Object(map)) = widget else {
        return false;
    };
    let next_expanded = !map
        .get("expanded")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    map.insert("expanded".to_string(), json!(next_expanded));
    map.insert(
        "expansionState".to_string(),
        json!(if next_expanded {
            "expanded"
        } else {
            "collapsed"
        }),
    );
    map.insert("summaryOnly".to_string(), json!(!next_expanded));
    true
}

fn terminal_value_as_u64(value: &Value) -> Option<u64> {
    match value {
        Value::Number(number) => number.as_u64(),
        Value::String(text) => text.trim().parse::<u64>().ok(),
        _ => None,
    }
}

fn terminal_object_u64_field(object: &Map<String, Value>, keys: &[&str]) -> Option<u64> {
    keys.iter()
        .find_map(|key| object.get(*key).and_then(terminal_value_as_u64))
}

fn terminal_object_string_field<'a>(
    object: &'a Map<String, Value>,
    keys: &[&str],
) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| object.get(*key).and_then(Value::as_str))
}

fn terminal_diff_file_preview_lines(payload: &Value, max_files: usize) -> Vec<String> {
    for key in ["files", "fileChanges", "changedFiles", "changes"] {
        if let Some(Value::Array(items)) = payload.get(key) {
            let lines = items
                .iter()
                .filter_map(terminal_file_preview_line)
                .take(max_files)
                .collect::<Vec<_>>();
            if !lines.is_empty() {
                return lines;
            }
        }
    }

    terminal_file_preview_line(payload)
        .into_iter()
        .take(max_files)
        .collect()
}

fn terminal_file_preview_line(value: &Value) -> Option<String> {
    match value {
        Value::String(path) => {
            let path = terminal_clean_line(path);
            (!path.is_empty()).then_some(path)
        }
        Value::Object(object) => {
            let path = terminal_object_string_field(
                object,
                &[
                    "path",
                    "filePath",
                    "file_path",
                    "notebookPath",
                    "notebook_path",
                ],
            )?;
            let path = terminal_clean_line(path);
            if path.is_empty() {
                return None;
            }
            let status = terminal_object_string_field(
                object,
                &["status", "kind", "changeType", "change_type", "type"],
            )
            .map(terminal_file_status_label)
            .unwrap_or_else(|| "M".to_string());
            let additions = terminal_object_u64_field(
                object,
                &["additions", "added", "lines_added", "linesAdded"],
            )
            .unwrap_or(0);
            let deletions = terminal_object_u64_field(
                object,
                &["deletions", "deleted", "lines_removed", "linesRemoved"],
            )
            .unwrap_or(0);
            let mut line = format!("{status} {path}");
            if additions > 0 || deletions > 0 {
                line.push_str(&format!(" +{additions} -{deletions}"));
            }
            Some(terminal_clean_line(&line))
        }
        _ => None,
    }
}

fn terminal_file_status_label(status: &str) -> String {
    match status.trim().to_ascii_lowercase().as_str() {
        "modified" | "modify" | "updated" | "update" | "edited" | "edit" | "changed" => {
            "M".to_string()
        }
        "added" | "add" | "created" | "create" | "new" => "A".to_string(),
        "deleted" | "delete" | "removed" | "remove" => "D".to_string(),
        "renamed" | "rename" | "moved" | "move" => "R".to_string(),
        other if other.is_empty() => "M".to_string(),
        other => terminal_clean_line(other),
    }
}

#[derive(Debug, Clone)]
struct TerminalUnifiedDiffFileSection {
    path: String,
    additions: u64,
    deletions: u64,
    preview_lines: Vec<String>,
}

impl TerminalUnifiedDiffFileSection {
    fn new(path: String) -> Self {
        Self {
            path: if path.trim().is_empty() {
                "diff".to_string()
            } else {
                terminal_clean_line(&path)
            },
            additions: 0,
            deletions: 0,
            preview_lines: Vec::new(),
        }
    }

    fn push_preview_line(&mut self, line: &str, max_lines: usize) {
        if self.preview_lines.len() < max_lines {
            self.preview_lines.push(terminal_clean_line(line));
        }
    }

    fn value(self) -> Value {
        let summary = format!("{} +{} -{}", self.path, self.additions, self.deletions);
        json!({
            "path": self.path,
            "additions": self.additions,
            "deletions": self.deletions,
            "summary": summary,
            "previewLines": self.preview_lines,
        })
    }
}

fn terminal_diff_file_section_values(
    payload: &Value,
    max_files: usize,
    max_hunk_lines_per_file: usize,
) -> Vec<Value> {
    let mut sections = Vec::new();
    terminal_append_diff_file_sections(payload, max_files, max_hunk_lines_per_file, &mut sections);
    sections
}

fn terminal_append_diff_file_sections(
    value: &Value,
    max_files: usize,
    max_hunk_lines_per_file: usize,
    sections: &mut Vec<Value>,
) {
    if sections.len() >= max_files {
        return;
    }

    match value {
        Value::String(text) => {
            let remaining = max_files.saturating_sub(sections.len());
            sections.extend(
                terminal_parse_unified_diff_file_sections(text, remaining, max_hunk_lines_per_file)
                    .into_iter()
                    .map(TerminalUnifiedDiffFileSection::value),
            );
        }
        Value::Array(items) => {
            for item in items {
                terminal_append_diff_file_sections(
                    item,
                    max_files,
                    max_hunk_lines_per_file,
                    sections,
                );
                if sections.len() >= max_files {
                    break;
                }
            }
        }
        Value::Object(object) => {
            for key in ["diffText", "unifiedDiff", "diff", "patch"] {
                if let Some(value) = object.get(key) {
                    terminal_append_diff_file_sections(
                        value,
                        max_files,
                        max_hunk_lines_per_file,
                        sections,
                    );
                    if sections.len() >= max_files {
                        return;
                    }
                }
            }
            for key in ["hunks", "diffs", "patches"] {
                if let Some(value) = object.get(key) {
                    terminal_append_diff_file_sections(
                        value,
                        max_files,
                        max_hunk_lines_per_file,
                        sections,
                    );
                    if sections.len() >= max_files {
                        return;
                    }
                }
            }
        }
        _ => {}
    }
}

fn terminal_parse_unified_diff_file_sections(
    text: &str,
    max_files: usize,
    max_hunk_lines_per_file: usize,
) -> Vec<TerminalUnifiedDiffFileSection> {
    if max_files == 0 {
        return Vec::new();
    }

    let mut sections = Vec::new();
    let mut current: Option<TerminalUnifiedDiffFileSection> = None;
    let mut current_seen_hunk = false;

    for raw_line in text.lines() {
        let line = terminal_clean_line(raw_line);
        if line.trim().is_empty() {
            continue;
        }

        if let Some(path) = terminal_unified_diff_git_header_path(&line) {
            if let Some(section) = current.take() {
                sections.push(section);
                if sections.len() >= max_files {
                    break;
                }
            }
            current = Some(TerminalUnifiedDiffFileSection::new(path));
            current_seen_hunk = false;
            continue;
        }

        if let Some(path) = terminal_unified_diff_marker_path(&line, "+++ ") {
            let section =
                current.get_or_insert_with(|| TerminalUnifiedDiffFileSection::new(path.clone()));
            if path != "/dev/null" {
                section.path = terminal_clean_line(&path);
            }
            continue;
        }

        if terminal_unified_diff_marker_path(&line, "--- ").is_some() {
            current.get_or_insert_with(|| TerminalUnifiedDiffFileSection::new("diff".to_string()));
            continue;
        }

        if line.starts_with("@@") {
            let section =
                current.get_or_insert_with(|| TerminalUnifiedDiffFileSection::new("diff".into()));
            current_seen_hunk = true;
            section.push_preview_line(&line, max_hunk_lines_per_file);
            continue;
        }

        if line.starts_with('+') && !line.starts_with("+++") {
            let section =
                current.get_or_insert_with(|| TerminalUnifiedDiffFileSection::new("diff".into()));
            section.additions = section.additions.saturating_add(1);
            if current_seen_hunk {
                section.push_preview_line(&line, max_hunk_lines_per_file);
            }
            continue;
        }

        if line.starts_with('-') && !line.starts_with("---") {
            let section =
                current.get_or_insert_with(|| TerminalUnifiedDiffFileSection::new("diff".into()));
            section.deletions = section.deletions.saturating_add(1);
            if current_seen_hunk {
                section.push_preview_line(&line, max_hunk_lines_per_file);
            }
            continue;
        }

        if line.starts_with(' ') && current_seen_hunk {
            if let Some(section) = current.as_mut() {
                section.push_preview_line(&line, max_hunk_lines_per_file);
            }
        }
    }

    if let Some(section) = current {
        if sections.len() < max_files {
            sections.push(section);
        }
    }

    sections
}

fn terminal_unified_diff_git_header_path(line: &str) -> Option<String> {
    let rest = line.strip_prefix("diff --git ")?;
    let (_, path) = rest.rsplit_once(" b/")?;
    Some(terminal_clean_line(path))
}

fn terminal_unified_diff_marker_path(line: &str, marker: &str) -> Option<String> {
    let path = line.strip_prefix(marker)?.trim();
    if path.is_empty() {
        return None;
    }
    let path = path
        .split('\t')
        .next()
        .unwrap_or(path)
        .split("  ")
        .next()
        .unwrap_or(path);
    Some(terminal_clean_line(
        path.strip_prefix("a/")
            .or_else(|| path.strip_prefix("b/"))
            .unwrap_or(path),
    ))
}

fn terminal_diff_preview_lines(payload: &Value, max_lines: usize) -> Vec<String> {
    let mut lines = Vec::new();
    terminal_append_diff_preview_lines(payload, max_lines, &mut lines);
    lines
}

fn terminal_append_diff_preview_lines(value: &Value, max_lines: usize, lines: &mut Vec<String>) {
    if lines.len() >= max_lines {
        return;
    }

    match value {
        Value::String(text) => {
            let remaining = max_lines.saturating_sub(lines.len());
            lines.extend(terminal_diff_text_preview_lines(text, remaining));
        }
        Value::Array(items) => {
            for item in items {
                terminal_append_diff_preview_lines(item, max_lines, lines);
                if lines.len() >= max_lines {
                    break;
                }
            }
        }
        Value::Object(object) => {
            for key in ["diffText", "unifiedDiff", "diff", "patch"] {
                if let Some(Value::String(text)) = object.get(key) {
                    let remaining = max_lines.saturating_sub(lines.len());
                    lines.extend(terminal_diff_text_preview_lines(text, remaining));
                    if lines.len() >= max_lines {
                        return;
                    }
                }
            }
            for key in ["hunks", "diffs", "patches"] {
                if let Some(value) = object.get(key) {
                    terminal_append_diff_preview_lines(value, max_lines, lines);
                    if lines.len() >= max_lines {
                        return;
                    }
                }
            }
        }
        _ => {}
    }
}

fn terminal_command_started_widget(payload: &Value) -> Value {
    json!({
        "kind": "command",
        "phase": "running",
        "toolId": payload.get("toolId").cloned().unwrap_or(Value::Null),
        "command": payload.get("command").cloned().unwrap_or(Value::Null),
        "cwd": payload.get("cwd").cloned().unwrap_or(Value::Null),
        "summary": payload
            .get("command")
            .and_then(Value::as_str)
            .map(|command| format!("command started: {command}"))
            .unwrap_or_else(|| "command started".to_string()),
        "expanded": false,
        "expansionState": "collapsed",
        "collapsedByDefault": true,
        "summaryOnly": true,
    })
}

fn terminal_command_output_widget(previous: Option<Value>, payload: &Value) -> Value {
    let mut map = match previous {
        Some(Value::Object(map)) => map,
        _ => Map::new(),
    };
    let stream = payload
        .get("stream")
        .and_then(Value::as_str)
        .unwrap_or("output");
    let preview_line_items = terminal_command_preview_line_items(
        payload,
        stream,
        STREAM_JSON_RENDER_COMMAND_PREVIEW_MAX_LINES,
    );
    let expanded_preview_line_items = terminal_command_preview_line_items(
        payload,
        stream,
        STREAM_JSON_RENDER_COMMAND_EXPANDED_PREVIEW_MAX_LINES,
    );
    let preview_lines = payload
        .get("previewLines")
        .and_then(Value::as_u64)
        .or_else(|| u64::try_from(preview_line_items.len()).ok())
        .unwrap_or(0);
    let hidden_lines = payload
        .get("hiddenLines")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let output_chunk_count = map
        .get("outputChunkCount")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .saturating_add(1);
    let chunk_line_count = terminal_command_output_line_count(
        payload,
        preview_lines,
        hidden_lines,
        expanded_preview_line_items.len(),
    );
    let observed_output_lines = map
        .get("observedOutputLines")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .saturating_add(chunk_line_count);
    let output_tail_line_items = terminal_command_tail_line_items(
        &map,
        "outputTailLineItems",
        stream,
        &preview_line_items,
        STREAM_JSON_RENDER_COMMAND_PREVIEW_MAX_LINES,
    );
    let expanded_output_tail_line_items = terminal_command_tail_line_items(
        &map,
        "expandedOutputTailLineItems",
        stream,
        &expanded_preview_line_items,
        STREAM_JSON_RENDER_COMMAND_EXPANDED_PREVIEW_MAX_LINES,
    );
    let retained_output_line_count = u64::try_from(output_tail_line_items.len()).unwrap_or(0);
    let retained_expanded_output_line_count =
        u64::try_from(expanded_output_tail_line_items.len()).unwrap_or(0);
    let dropped_output_line_count =
        observed_output_lines.saturating_sub(retained_output_line_count);
    let dropped_expanded_output_line_count =
        observed_output_lines.saturating_sub(retained_expanded_output_line_count);
    let output_tail_stream_changed = map
        .get("outputTailStream")
        .and_then(Value::as_str)
        .is_some_and(|previous_stream| previous_stream != stream);
    let expanded = map
        .get("expanded")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    map.insert("kind".to_string(), json!("command"));
    map.insert("phase".to_string(), json!("output"));
    if let Some(tool_id) = payload.get("toolId") {
        map.insert("toolId".to_string(), tool_id.clone());
    }
    terminal_copy_command_task_fields(&mut map, payload);
    map.insert("stream".to_string(), json!(stream));
    map.insert(
        "bytes".to_string(),
        payload.get("bytes").cloned().unwrap_or(Value::Null),
    );
    map.insert("previewLines".to_string(), json!(preview_lines));
    map.insert("previewLineCount".to_string(), json!(preview_lines));
    map.insert("previewLineItems".to_string(), json!(preview_line_items));
    map.insert(
        "expandedPreviewLineItems".to_string(),
        json!(expanded_preview_line_items),
    );
    map.insert("outputChunkCount".to_string(), json!(output_chunk_count));
    map.insert(
        "observedOutputLines".to_string(),
        json!(observed_output_lines),
    );
    map.insert("outputTailStream".to_string(), json!(stream));
    map.insert(
        "outputTailStreamChanged".to_string(),
        json!(output_tail_stream_changed),
    );
    map.insert(
        "outputTailLineItems".to_string(),
        json!(output_tail_line_items),
    );
    map.insert(
        "expandedOutputTailLineItems".to_string(),
        json!(expanded_output_tail_line_items),
    );
    map.insert(
        "retainedOutputLineCount".to_string(),
        json!(retained_output_line_count),
    );
    map.insert(
        "retainedExpandedOutputLineCount".to_string(),
        json!(retained_expanded_output_line_count),
    );
    map.insert(
        "droppedOutputLineCount".to_string(),
        json!(dropped_output_line_count),
    );
    map.insert(
        "droppedExpandedOutputLineCount".to_string(),
        json!(dropped_expanded_output_line_count),
    );
    map.insert(
        "outputTailLineLimit".to_string(),
        json!(STREAM_JSON_RENDER_COMMAND_PREVIEW_MAX_LINES),
    );
    map.insert(
        "expandedOutputTailLineLimit".to_string(),
        json!(STREAM_JSON_RENDER_COMMAND_EXPANDED_PREVIEW_MAX_LINES),
    );
    map.insert("outputTailPreviewAvailable".to_string(), json!(true));
    map.insert(
        "previewLineLimit".to_string(),
        json!(STREAM_JSON_RENDER_COMMAND_PREVIEW_MAX_LINES),
    );
    map.insert(
        "expandedPreviewLineLimit".to_string(),
        json!(STREAM_JSON_RENDER_COMMAND_EXPANDED_PREVIEW_MAX_LINES),
    );
    map.insert("hiddenLines".to_string(), json!(hidden_lines));
    map.insert(
        "totalLines".to_string(),
        payload.get("totalLines").cloned().unwrap_or(Value::Null),
    );
    map.insert(
        "fullLogAvailable".to_string(),
        payload
            .get("fullLogAvailable")
            .cloned()
            .unwrap_or(Value::Bool(false)),
    );
    let summary = terminal_command_task_summary(&map)
        .unwrap_or_else(|| format!("{stream}: {preview_lines} shown, {hidden_lines} hidden"));
    map.insert("summary".to_string(), json!(summary));
    map.insert("expanded".to_string(), json!(expanded));
    map.insert("collapsedByDefault".to_string(), json!(true));
    map.insert(
        "expansionState".to_string(),
        json!(if expanded { "expanded" } else { "collapsed" }),
    );
    map.insert("summaryOnly".to_string(), json!(!expanded));
    Value::Object(map)
}

fn terminal_command_finished_widget(previous: Option<Value>, payload: &Value) -> Value {
    let mut map = match previous {
        Some(Value::Object(map)) => map,
        _ => Map::new(),
    };
    map.insert("kind".to_string(), json!("command"));
    map.insert("phase".to_string(), json!("finished"));
    if let Some(tool_id) = payload.get("toolId") {
        map.insert("toolId".to_string(), tool_id.clone());
    }
    terminal_copy_command_task_fields(&mut map, payload);
    map.insert(
        "exitCode".to_string(),
        payload.get("exitCode").cloned().unwrap_or(Value::Null),
    );
    map.insert(
        "durationMs".to_string(),
        payload.get("durationMs").cloned().unwrap_or(Value::Null),
    );
    let summary = terminal_command_task_summary(&map).unwrap_or_else(|| {
        format!(
            "command finished: {}",
            payload
                .get("exitCode")
                .and_then(Value::as_i64)
                .map(|code| format!("exit {code}"))
                .unwrap_or_else(|| "exit unknown".to_string())
        )
    });
    map.insert("summary".to_string(), json!(summary));
    let expanded = map
        .get("expanded")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    map.insert("expanded".to_string(), json!(expanded));
    map.insert("collapsedByDefault".to_string(), json!(true));
    map.insert(
        "expansionState".to_string(),
        json!(if expanded { "expanded" } else { "collapsed" }),
    );
    map.insert("summaryOnly".to_string(), json!(!expanded));
    Value::Object(map)
}

fn terminal_copy_command_task_fields(map: &mut Map<String, Value>, payload: &Value) {
    for key in [
        "backgroundTaskId",
        "taskId",
        "taskType",
        "taskStatus",
        "command",
    ] {
        if let Some(value) = payload.get(key) {
            map.insert(key.to_string(), value.clone());
        }
    }
}

fn terminal_command_task_summary(map: &Map<String, Value>) -> Option<String> {
    if let Some(task_id) = map.get("backgroundTaskId").and_then(Value::as_str) {
        let status = map
            .get("taskStatus")
            .and_then(Value::as_str)
            .unwrap_or("started");
        return Some(format!("background task {status}: {task_id}"));
    }
    if let Some(task_id) = map.get("taskId").and_then(Value::as_str) {
        let status = map
            .get("taskStatus")
            .and_then(Value::as_str)
            .unwrap_or("updated");
        return Some(format!("background task {status}: {task_id}"));
    }
    None
}

fn terminal_command_history_item(command: &Value) -> Value {
    let command_text = command
        .get("command")
        .and_then(Value::as_str)
        .map(terminal_clean_line)
        .filter(|value| !value.trim().is_empty());
    let cwd = command
        .get("cwd")
        .and_then(Value::as_str)
        .map(terminal_clean_line)
        .filter(|value| !value.trim().is_empty());
    let exit_code = command.get("exitCode").and_then(Value::as_i64);
    let status = match exit_code {
        Some(0) => "passed",
        Some(_) => "failed",
        None => "unknown",
    };
    let exit_label = exit_code
        .map(|code| format!("exit {code}"))
        .unwrap_or_else(|| "exit unknown".to_string());
    let summary = command_text
        .as_ref()
        .map(|text| format!("{} -> {}", terminal_status_label(text, 72), exit_label))
        .or_else(|| {
            command
                .get("summary")
                .and_then(Value::as_str)
                .map(terminal_clean_line)
        })
        .unwrap_or_else(|| format!("command -> {exit_label}"));

    json!({
        "kind": "command_history_item",
        "command": command_text,
        "cwd": cwd,
        "exitCode": exit_code,
        "durationMs": command.get("durationMs").cloned().unwrap_or(Value::Null),
        "status": status,
        "summary": terminal_clean_line(&summary),
    })
}

fn terminal_command_history_summary(command_history: &[Value]) -> Value {
    let total = u64::try_from(command_history.len()).unwrap_or(0);
    let passed = command_history
        .iter()
        .filter(|item| item.get("status").and_then(Value::as_str) == Some("passed"))
        .count();
    let failed = command_history
        .iter()
        .filter(|item| item.get("status").and_then(Value::as_str) == Some("failed"))
        .count();
    let unknown = command_history
        .iter()
        .filter(|item| item.get("status").and_then(Value::as_str) == Some("unknown"))
        .count();
    let passed = u64::try_from(passed).unwrap_or(0);
    let failed = u64::try_from(failed).unwrap_or(0);
    let unknown = u64::try_from(unknown).unwrap_or(0);
    let status = if total == 0 {
        "not_recorded"
    } else if failed > 0 {
        "failed"
    } else if unknown > 0 {
        "inconclusive"
    } else {
        "passed"
    };
    let summary = if total == 0 {
        "not recorded".to_string()
    } else if unknown > 0 {
        format!("{total} command(s), {passed} passed, {failed} failed, {unknown} unknown")
    } else {
        format!("{total} command(s), {passed} passed, {failed} failed")
    };

    json!({
        "status": status,
        "summary": summary,
        "totalCommands": total,
        "passedCommands": passed,
        "failedCommands": failed,
        "unknownCommands": unknown,
        "retainedMax": STREAM_JSON_RENDER_COMMAND_HISTORY_MAX_ITEMS,
        "items": command_history,
    })
}

fn terminal_final_summary_residual_risk(
    success: bool,
    error: Option<&Value>,
    verification_summary: &Value,
) -> Value {
    let mut items = Vec::new();
    if let Some(summary) = error
        .and_then(|error| error.get("summary"))
        .and_then(Value::as_str)
        .map(terminal_clean_line)
        .filter(|summary| !summary.trim().is_empty())
    {
        items.push(summary);
    }

    let failed_commands = verification_summary
        .get("failedCommands")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if failed_commands > 0
        && !items
            .iter()
            .any(|item| item.contains("command") || item.contains("test"))
    {
        items.push(format!("{failed_commands} command(s) failed"));
    }

    let verification_status = verification_summary
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("not_recorded");
    if success && verification_status == "not_recorded" {
        items.push("no command verification recorded".to_string());
    }

    let status = if items.is_empty() {
        "none_recorded"
    } else {
        "recorded"
    };
    let summary = items
        .first()
        .cloned()
        .unwrap_or_else(|| "none recorded".to_string());

    json!({
        "status": status,
        "summary": summary,
        "items": items,
    })
}

fn terminal_background_task_summary(task: &Map<String, Value>) -> String {
    let task_id = task
        .get("taskId")
        .and_then(Value::as_str)
        .unwrap_or("background-task");
    let status = task
        .get("taskStatus")
        .and_then(Value::as_str)
        .unwrap_or("updated");
    format!("background task {status}: {task_id}")
}

fn terminal_background_task_items(tasks: &BTreeMap<String, Value>) -> Vec<Value> {
    terminal_background_task_items_with_limit(tasks, STREAM_JSON_RENDER_BACKGROUND_TASK_MAX_ITEMS)
}

fn terminal_background_task_items_with_limit(
    tasks: &BTreeMap<String, Value>,
    max_items: usize,
) -> Vec<Value> {
    let mut items = tasks.values().cloned().collect::<Vec<_>>();
    items.sort_by(|left, right| {
        let left_sequence = left
            .get("updatedSequence")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let right_sequence = right
            .get("updatedSequence")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        right_sequence.cmp(&left_sequence).then_with(|| {
            right
                .get("taskId")
                .and_then(Value::as_str)
                .unwrap_or("")
                .cmp(left.get("taskId").and_then(Value::as_str).unwrap_or(""))
        })
    });
    items.truncate(max_items);
    items
}

fn terminal_background_task_items_value(tasks: &BTreeMap<String, Value>) -> Value {
    Value::Array(terminal_background_task_items(tasks))
}

fn terminal_background_task_items_value_with_limit(
    tasks: &BTreeMap<String, Value>,
    max_items: usize,
) -> Value {
    Value::Array(terminal_background_task_items_with_limit(tasks, max_items))
}

fn terminal_file_change_widget(kind: &str, payload: &Value) -> Value {
    let file_count = payload
        .get("fileCount")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let additions = payload
        .get("additions")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let deletions = payload
        .get("deletions")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let file_preview_lines =
        terminal_diff_file_preview_lines(payload, STREAM_JSON_RENDER_DIFF_FILE_PREVIEW_MAX_LINES);
    let expanded_file_preview_lines = terminal_diff_file_preview_lines(
        payload,
        STREAM_JSON_RENDER_DIFF_FILE_EXPANDED_PREVIEW_MAX_LINES,
    );
    let preview_file_count = u64::try_from(file_preview_lines.len()).unwrap_or(0);
    let expanded_preview_file_count = u64::try_from(expanded_file_preview_lines.len()).unwrap_or(0);
    let omitted_file_count = if preview_file_count > 0 {
        file_count.saturating_sub(preview_file_count)
    } else {
        0
    };
    let expanded_omitted_file_count = if expanded_preview_file_count > 0 {
        file_count.saturating_sub(expanded_preview_file_count)
    } else {
        0
    };
    let diff_preview_lines =
        terminal_diff_preview_lines(payload, STREAM_JSON_RENDER_DIFF_HUNK_PREVIEW_MAX_LINES);
    let expanded_diff_preview_lines = terminal_diff_preview_lines(
        payload,
        STREAM_JSON_RENDER_DIFF_HUNK_EXPANDED_PREVIEW_MAX_LINES,
    );
    let diff_preview_available = !diff_preview_lines.is_empty();
    let diff_file_sections = terminal_diff_file_section_values(
        payload,
        STREAM_JSON_RENDER_DIFF_SECTION_PREVIEW_MAX_FILES,
        STREAM_JSON_RENDER_DIFF_SECTION_HUNK_PREVIEW_MAX_LINES,
    );
    let expanded_diff_file_sections = terminal_diff_file_section_values(
        payload,
        STREAM_JSON_RENDER_DIFF_SECTION_EXPANDED_MAX_FILES,
        STREAM_JSON_RENDER_DIFF_HUNK_EXPANDED_PREVIEW_MAX_LINES,
    );
    let diff_file_section_count = u64::try_from(diff_file_sections.len()).unwrap_or(0);
    let expanded_diff_file_section_count =
        u64::try_from(expanded_diff_file_sections.len()).unwrap_or(0);
    let diff_file_section_preview_available = !diff_file_sections.is_empty();
    json!({
        "kind": if kind == "diff_available" { "diff" } else { "file_change_summary" },
        "phase": if kind == "diff_available" { "diff_available" } else { "summary" },
        "toolId": payload.get("toolId").cloned().unwrap_or(Value::Null),
        "fileCount": file_count,
        "additions": additions,
        "deletions": deletions,
        "expanded": false,
        "expansionState": "collapsed",
        "collapsedByDefault": true,
        "filePreviewLines": file_preview_lines,
        "expandedFilePreviewLines": expanded_file_preview_lines,
        "previewFileCount": preview_file_count,
        "expandedPreviewFileCount": expanded_preview_file_count,
        "omittedFileCount": omitted_file_count,
        "expandedOmittedFileCount": expanded_omitted_file_count,
        "diffPreviewLines": diff_preview_lines,
        "expandedDiffPreviewLines": expanded_diff_preview_lines,
        "diffPreviewAvailable": diff_preview_available,
        "diffFileSections": diff_file_sections,
        "expandedDiffFileSections": expanded_diff_file_sections,
        "diffFileSectionCount": diff_file_section_count,
        "expandedDiffFileSectionCount": expanded_diff_file_section_count,
        "diffFileSectionPreviewAvailable": diff_file_section_preview_available,
        "filePreviewLimit": STREAM_JSON_RENDER_DIFF_FILE_PREVIEW_MAX_LINES,
        "expandedFilePreviewLimit": STREAM_JSON_RENDER_DIFF_FILE_EXPANDED_PREVIEW_MAX_LINES,
        "diffPreviewLimit": STREAM_JSON_RENDER_DIFF_HUNK_PREVIEW_MAX_LINES,
        "expandedDiffPreviewLimit": STREAM_JSON_RENDER_DIFF_HUNK_EXPANDED_PREVIEW_MAX_LINES,
        "diffSectionPreviewFileLimit": STREAM_JSON_RENDER_DIFF_SECTION_PREVIEW_MAX_FILES,
        "expandedDiffSectionFileLimit": STREAM_JSON_RENDER_DIFF_SECTION_EXPANDED_MAX_FILES,
        "diffSectionHunkPreviewLimit": STREAM_JSON_RENDER_DIFF_SECTION_HUNK_PREVIEW_MAX_LINES,
        "expandedDiffSectionHunkPreviewLimit": STREAM_JSON_RENDER_DIFF_HUNK_EXPANDED_PREVIEW_MAX_LINES,
        "summary": format!("{file_count} file(s), +{additions} -{deletions}"),
    })
}

fn terminal_error_widget(payload: &Value) -> Value {
    let source = payload
        .get("source")
        .and_then(Value::as_str)
        .unwrap_or("runtime");
    let summary = payload
        .get("summary")
        .and_then(Value::as_str)
        .unwrap_or("error");
    let details = terminal_error_detail_text(payload, summary);
    let detail_preview_lines = terminal_text_head_preview_lines(
        &details,
        STREAM_JSON_RENDER_ERROR_DETAIL_PREVIEW_MAX_LINES,
    );
    let expanded_detail_preview_lines = terminal_text_head_preview_lines(
        &details,
        STREAM_JSON_RENDER_ERROR_DETAIL_EXPANDED_PREVIEW_MAX_LINES,
    );
    json!({
        "kind": "error",
        "phase": "raised",
        "source": source,
        "title": "Error",
        "summary": terminal_clean_line(summary),
        "keyDetail": detail_preview_lines
            .first()
            .cloned()
            .unwrap_or_else(|| terminal_clean_line(summary)),
        "details": terminal_clean_line(&details),
        "detailsAvailable": !details.trim().is_empty(),
        "expanded": false,
        "expansionState": "collapsed",
        "collapsedByDefault": true,
        "summaryOnly": true,
        "detailPreviewLines": detail_preview_lines,
        "expandedDetailPreviewLines": expanded_detail_preview_lines,
        "detailPreviewLimit": STREAM_JSON_RENDER_ERROR_DETAIL_PREVIEW_MAX_LINES,
        "expandedDetailPreviewLimit": STREAM_JSON_RENDER_ERROR_DETAIL_EXPANDED_PREVIEW_MAX_LINES,
        "retrying": false,
        "layered": true,
    })
}

fn terminal_error_detail_text(payload: &Value, fallback: &str) -> String {
    payload
        .get("details")
        .or_else(|| payload.get("detail"))
        .or_else(|| payload.get("error"))
        .and_then(terminal_value_as_display_text)
        .filter(|text| !text.trim().is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

fn terminal_error_retry_widget(previous: Option<Value>, payload: &Value) -> Value {
    let mut map = match previous {
        Some(Value::Object(map)) => map,
        _ => Map::new(),
    };
    let attempt = payload.get("attempt").and_then(Value::as_u64).unwrap_or(0);
    let max_retries = payload
        .get("maxRetries")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let retry_in_ms = payload
        .get("retryInMs")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    map.insert("kind".to_string(), json!("error"));
    map.insert("phase".to_string(), json!("retrying"));
    map.insert("title".to_string(), json!("Retrying request"));
    map.insert(
        "summary".to_string(),
        json!(format!("retrying API request {attempt}/{max_retries}")),
    );
    map.insert("attempt".to_string(), json!(attempt));
    map.insert("maxRetries".to_string(), json!(max_retries));
    map.insert("retryInMs".to_string(), json!(retry_in_ms));
    let retry_detail =
        format!("retrying API request {attempt}/{max_retries}; next retry in {retry_in_ms}ms");
    let detail_preview_lines = terminal_text_head_preview_lines(
        &retry_detail,
        STREAM_JSON_RENDER_ERROR_DETAIL_PREVIEW_MAX_LINES,
    );
    let expanded_detail_preview_lines = terminal_text_head_preview_lines(
        &retry_detail,
        STREAM_JSON_RENDER_ERROR_DETAIL_EXPANDED_PREVIEW_MAX_LINES,
    );
    let expanded = map
        .get("expanded")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    map.insert("expanded".to_string(), json!(expanded));
    map.insert(
        "expansionState".to_string(),
        json!(if expanded { "expanded" } else { "collapsed" }),
    );
    map.insert("collapsedByDefault".to_string(), json!(true));
    map.insert("summaryOnly".to_string(), json!(!expanded));
    map.insert("details".to_string(), json!(retry_detail));
    map.insert(
        "detailPreviewLines".to_string(),
        json!(detail_preview_lines),
    );
    map.insert(
        "expandedDetailPreviewLines".to_string(),
        json!(expanded_detail_preview_lines),
    );
    map.insert(
        "detailPreviewLimit".to_string(),
        json!(STREAM_JSON_RENDER_ERROR_DETAIL_PREVIEW_MAX_LINES),
    );
    map.insert(
        "expandedDetailPreviewLimit".to_string(),
        json!(STREAM_JSON_RENDER_ERROR_DETAIL_EXPANDED_PREVIEW_MAX_LINES),
    );
    map.insert("detailsAvailable".to_string(), json!(true));
    map.insert("retrying".to_string(), json!(true));
    map.insert("layered".to_string(), json!(true));
    Value::Object(map)
}

fn terminal_final_summary_widget(
    payload: &Value,
    command: Option<&Value>,
    command_history: &[Value],
    file_change: Option<&Value>,
    diff: Option<&Value>,
    error: Option<&Value>,
) -> Value {
    let terminal = payload
        .get("terminal")
        .and_then(Value::as_str)
        .unwrap_or("finished");
    let success = payload
        .get("success")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let result = if success { "success" } else { "failed" };
    let command_history_value = Value::Array(command_history.to_vec());
    let verification_summary = terminal_command_history_summary(command_history);
    let residual_risk_summary =
        terminal_final_summary_residual_risk(success, error, &verification_summary);
    json!({
        "kind": "final_summary",
        "phase": "recorded",
        "terminal": terminal,
        "success": success,
        "result": result,
        "summary": format!("final summary: {result}"),
        "commandSummary": command.cloned().unwrap_or(Value::Null),
        "commandHistory": command_history_value,
        "commandHistoryCount": command_history.len(),
        "commandHistoryRetainedMax": STREAM_JSON_RENDER_COMMAND_HISTORY_MAX_ITEMS,
        "verificationSummary": verification_summary,
        "residualRiskSummary": residual_risk_summary,
        "fileChangeSummary": file_change.cloned().unwrap_or(Value::Null),
        "diffSummary": diff.cloned().unwrap_or(Value::Null),
        "errorSummary": error.cloned().unwrap_or(Value::Null),
        "independentRegion": true,
    })
}

fn terminal_plan_widget(payload: &Value) -> Value {
    let step_count = payload
        .get("stepCount")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let completed_count = payload
        .get("completedCount")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let active_count = payload
        .get("activeCount")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let pending_count = payload
        .get("pendingCount")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let blocked_count = payload
        .get("blockedCount")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let active_step = payload
        .get("activeStep")
        .and_then(Value::as_str)
        .map(terminal_clean_line)
        .filter(|step| !step.is_empty());
    json!({
        "kind": "plan",
        "phase": "updated",
        "toolId": payload.get("toolId").cloned().unwrap_or(Value::Null),
        "stepCount": step_count,
        "completedCount": completed_count,
        "activeCount": active_count,
        "pendingCount": pending_count,
        "blockedCount": blocked_count,
        "activeStep": active_step,
        "summary": format!(
            "plan: {step_count} step(s), {completed_count} done, {active_count} active, {pending_count} pending, {blocked_count} blocked"
        ),
        "independentRegion": true,
        "bounded": true,
    })
}

fn terminal_plan_lines(plan: Option<&Value>) -> Option<Vec<String>> {
    let plan = plan?;
    let mut lines = Vec::new();
    lines.push("plan".to_string());
    if let Some(summary) = plan.get("summary").and_then(Value::as_str) {
        lines.push(terminal_clean_line(summary));
    }
    if let Some(active_step) = plan.get("activeStep").and_then(Value::as_str) {
        if !active_step.trim().is_empty() {
            lines.push(terminal_clean_line(&format!("active: {active_step}")));
        }
    }
    let completed = plan
        .get("completedCount")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let active = plan.get("activeCount").and_then(Value::as_u64).unwrap_or(0);
    let pending = plan
        .get("pendingCount")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let blocked = plan
        .get("blockedCount")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    lines.push(format!(
        "progress: done {completed} | active {active} | pending {pending} | blocked {blocked}"
    ));
    lines.truncate(5);
    Some(lines)
}

fn terminal_background_task_lines(
    tasks: &BTreeMap<String, Value>,
    expanded: bool,
) -> Option<Vec<String>> {
    if tasks.is_empty() {
        return None;
    }

    let mut lines = vec![if expanded {
        "background task details".to_string()
    } else {
        "background tasks".to_string()
    }];
    let max_items = if expanded {
        STREAM_JSON_RENDER_BACKGROUND_TASK_EXPANDED_MAX_ITEMS
    } else {
        STREAM_JSON_RENDER_BACKGROUND_TASK_MAX_ITEMS
    };
    for task in terminal_background_task_items_with_limit(tasks, max_items) {
        let task_id = task
            .get("taskId")
            .and_then(Value::as_str)
            .unwrap_or("background-task");
        let status = task
            .get("taskStatus")
            .and_then(Value::as_str)
            .unwrap_or("updated");
        let mut parts = vec![format!("{status}: {task_id}")];
        if let Some(command) = task.get("command").and_then(Value::as_str) {
            parts.push(format!("cmd: {command}"));
        }
        let preview_lines = task
            .get("previewLines")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let hidden_lines = task.get("hiddenLines").and_then(Value::as_u64).unwrap_or(0);
        if preview_lines > 0 || hidden_lines > 0 {
            parts.push(format!(
                "output: {preview_lines} lines, {hidden_lines} hidden"
            ));
        }
        if let Some(exit_code) = task.get("exitCode").and_then(Value::as_i64) {
            parts.push(format!("exit: {exit_code}"));
        }
        if expanded {
            lines.push(terminal_clean_line(&format!("{status}: {task_id}")));
            if let Some(command) = task.get("command").and_then(Value::as_str) {
                lines.push(terminal_clean_line(&format!("cmd: {command}")));
            }
            if preview_lines > 0 || hidden_lines > 0 {
                lines.push(format!(
                    "output: {preview_lines} lines, {hidden_lines} hidden"
                ));
            }
            if let Some(exit_code) = task.get("exitCode").and_then(Value::as_i64) {
                lines.push(format!("exit: {exit_code}"));
            }
        } else {
            lines.push(terminal_clean_line(&parts.join(" | ")));
        }
    }
    lines.truncate(if expanded {
        32
    } else {
        STREAM_JSON_RENDER_BACKGROUND_TASK_MAX_ITEMS + 1
    });
    Some(lines)
}

fn terminal_background_task_update_mode(expanded: bool) -> &'static str {
    if expanded {
        "replace_expanded_summary"
    } else {
        "replace_summary"
    }
}

fn terminal_file_change_lines(file_change: Option<&Value>) -> Option<Vec<String>> {
    let file_change = file_change?;
    let mut lines = Vec::new();
    let expanded = terminal_widget_expanded(file_change);
    lines.push(if expanded {
        "file change details".to_string()
    } else {
        "file changes".to_string()
    });
    if let Some(summary) = file_change.get("summary").and_then(Value::as_str) {
        lines.push(terminal_clean_line(summary));
    }
    let file_preview_key = if expanded {
        "expandedFilePreviewLines"
    } else {
        "filePreviewLines"
    };
    if let Some(file_preview_lines) = file_change.get(file_preview_key).and_then(Value::as_array) {
        for preview in file_preview_lines.iter().filter_map(Value::as_str) {
            lines.push(terminal_clean_line(preview));
        }
    }
    let omitted_key = if expanded {
        "expandedOmittedFileCount"
    } else {
        "omittedFileCount"
    };
    if let Some(omitted) = file_change.get(omitted_key).and_then(Value::as_u64) {
        if omitted > 0 {
            lines.push(format!("files: {omitted} more hidden"));
        }
    }
    if expanded {
        lines.push("files: expanded preview".to_string());
    } else if file_change
        .get("collapsedByDefault")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        lines.push("files: collapsed".to_string());
    }
    lines.truncate(if expanded { 18 } else { 8 });
    Some(lines)
}

fn terminal_command_lines(command: Option<&Value>) -> Option<Vec<String>> {
    let command = command?;
    let mut lines = Vec::new();
    let expanded = terminal_widget_expanded(command);
    lines.push(if expanded {
        "command details".to_string()
    } else {
        "command summary".to_string()
    });
    if let Some(value) = command.get("command").and_then(Value::as_str) {
        lines.push(terminal_clean_line(&format!("cmd: {value}")));
    }
    if let Some(value) = command.get("cwd").and_then(Value::as_str) {
        lines.push(terminal_clean_line(&format!("cwd: {value}")));
    }
    if let Some(value) = command
        .get("backgroundTaskId")
        .or_else(|| command.get("taskId"))
        .and_then(Value::as_str)
    {
        lines.push(terminal_clean_line(&format!("task: {value}")));
    }
    if let Some(value) = command.get("taskStatus").and_then(Value::as_str) {
        lines.push(terminal_clean_line(&format!("status: {value}")));
    }
    if let Some(summary) = command.get("summary").and_then(Value::as_str) {
        lines.push(terminal_clean_line(summary));
    }
    let preview_line_count = command
        .get("previewLineCount")
        .or_else(|| command.get("previewLines"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let hidden_lines = command
        .get("hiddenLines")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let output_chunk_count = command
        .get("outputChunkCount")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let observed_output_lines = command
        .get("observedOutputLines")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if output_chunk_count > 1 && observed_output_lines > 0 {
        let retained_lines = if expanded {
            command
                .get("retainedExpandedOutputLineCount")
                .and_then(Value::as_u64)
        } else {
            command
                .get("retainedOutputLineCount")
                .and_then(Value::as_u64)
        }
        .unwrap_or(0);
        let hidden_tail_lines = observed_output_lines.saturating_sub(retained_lines);
        lines.push(format!(
            "output: {retained_lines} tail, {hidden_tail_lines} hidden, {output_chunk_count} chunks"
        ));
    } else if preview_line_count > 0 || hidden_lines > 0 {
        lines.push(format!(
            "output: {preview_line_count} shown, {hidden_lines} hidden"
        ));
    }
    if command
        .get("fullLogAvailable")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        lines.push("full log: available".to_string());
    }
    if let Some(exit_code) = command.get("exitCode").and_then(Value::as_i64) {
        lines.push(format!("exit: {exit_code}"));
    }
    if let Some(duration_ms) = command.get("durationMs").and_then(Value::as_u64) {
        lines.push(format!("duration: {duration_ms}ms"));
    }
    let stream = command
        .get("stream")
        .and_then(Value::as_str)
        .unwrap_or("output");
    let tail_stream = command
        .get("outputTailStream")
        .and_then(Value::as_str)
        .unwrap_or(stream);
    let preview_key = if expanded {
        if output_chunk_count > 1 {
            "expandedOutputTailLineItems"
        } else {
            "expandedPreviewLineItems"
        }
    } else if output_chunk_count > 1 {
        "outputTailLineItems"
    } else {
        "previewLineItems"
    };
    if let Some(preview_items) = command.get(preview_key).and_then(Value::as_array) {
        for preview in preview_items.iter().filter_map(Value::as_str) {
            let preview_stream = if output_chunk_count > 1 {
                tail_stream
            } else {
                stream
            };
            lines.push(terminal_clean_line(&format!("{preview_stream}: {preview}")));
        }
    }
    lines.truncate(if expanded { 18 } else { 10 });
    Some(lines)
}

fn terminal_diff_lines(diff: Option<&Value>) -> Option<Vec<String>> {
    let diff = diff?;
    let mut lines = Vec::new();
    let expanded = terminal_widget_expanded(diff);
    let title = if diff.get("kind").and_then(Value::as_str) == Some("diff") {
        if expanded {
            "diff details"
        } else {
            "diff summary"
        }
    } else {
        "file changes"
    };
    lines.push(title.to_string());
    if let Some(summary) = diff.get("summary").and_then(Value::as_str) {
        lines.push(terminal_clean_line(summary));
    }
    let file_preview_key = if expanded {
        "expandedFilePreviewLines"
    } else {
        "filePreviewLines"
    };
    if let Some(file_preview_lines) = diff.get(file_preview_key).and_then(Value::as_array) {
        for preview in file_preview_lines.iter().filter_map(Value::as_str) {
            lines.push(terminal_clean_line(preview));
        }
    }
    let omitted_key = if expanded {
        "expandedOmittedFileCount"
    } else {
        "omittedFileCount"
    };
    if let Some(omitted) = diff.get(omitted_key).and_then(Value::as_u64) {
        if omitted > 0 {
            lines.push(format!("files: {omitted} more hidden"));
        }
    }
    if expanded {
        lines.push("diff: expanded preview".to_string());
    } else if diff
        .get("collapsedByDefault")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        lines.push("diff: collapsed".to_string());
    }
    let section_key = if expanded {
        "expandedDiffFileSections"
    } else {
        "diffFileSections"
    };
    let section_preview_lines = diff
        .get(section_key)
        .and_then(Value::as_array)
        .filter(|sections| !sections.is_empty());
    if expanded {
        if let Some(sections) = section_preview_lines {
            lines.push("diff files:".to_string());
            for section in sections {
                if let Some(summary) = section.get("summary").and_then(Value::as_str) {
                    lines.push(terminal_clean_line(&format!("file: {summary}")));
                }
                if let Some(preview_lines) = section.get("previewLines").and_then(Value::as_array) {
                    for preview in preview_lines.iter().filter_map(Value::as_str) {
                        lines.push(terminal_clean_line(preview));
                    }
                }
            }
            lines.truncate(if expanded { 28 } else { 12 });
            return Some(lines);
        }
    }
    let diff_preview_key = if expanded {
        "expandedDiffPreviewLines"
    } else {
        "diffPreviewLines"
    };
    if let Some(diff_preview_lines) = diff.get(diff_preview_key).and_then(Value::as_array) {
        if !diff_preview_lines.is_empty() {
            lines.push("diff preview:".to_string());
            for preview in diff_preview_lines.iter().filter_map(Value::as_str) {
                lines.push(terminal_clean_line(preview));
            }
        }
    }
    lines.truncate(if expanded { 28 } else { 12 });
    Some(lines)
}

fn terminal_command_update_mode(command: Option<&Value>) -> &'static str {
    if command.is_some_and(terminal_widget_expanded) {
        "replace_expanded_preview"
    } else {
        "replace_summary"
    }
}

fn terminal_file_change_update_mode(file_change: Option<&Value>) -> &'static str {
    if file_change.is_some_and(terminal_widget_expanded) {
        "replace_expanded_file_summary"
    } else {
        "replace_file_summary"
    }
}

fn terminal_diff_update_mode(diff: Option<&Value>) -> &'static str {
    if diff.is_some_and(terminal_widget_expanded) {
        "replace_expanded_preview"
    } else {
        "replace_collapsed"
    }
}

fn terminal_error_update_mode(error: Option<&Value>) -> &'static str {
    if error.is_some_and(terminal_widget_expanded) {
        "replace_error_details"
    } else {
        "replace_layered"
    }
}

fn terminal_error_lines(error: Option<&Value>) -> Option<Vec<String>> {
    let error = error?;
    let mut lines = Vec::new();
    let expanded = terminal_widget_expanded(error);
    lines.push(if expanded {
        "error details".to_string()
    } else {
        "error".to_string()
    });
    if let Some(title) = error.get("title").and_then(Value::as_str) {
        lines.push(terminal_clean_line(title));
    }
    if let Some(summary) = error.get("summary").and_then(Value::as_str) {
        lines.push(terminal_clean_line(&format!("summary: {summary}")));
    }
    if let Some(key_detail) = error.get("keyDetail").and_then(Value::as_str) {
        lines.push(terminal_clean_line(&format!("key detail: {key_detail}")));
    }
    if error
        .get("retrying")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        let retry_in_ms = error.get("retryInMs").and_then(Value::as_u64).unwrap_or(0);
        lines.push(format!("retrying: true, next in {retry_in_ms}ms"));
    }
    let detail_preview_key = if expanded {
        "expandedDetailPreviewLines"
    } else {
        "detailPreviewLines"
    };
    if let Some(detail_preview_lines) = error.get(detail_preview_key).and_then(Value::as_array) {
        if !detail_preview_lines.is_empty() {
            lines.push(if expanded {
                "details:".to_string()
            } else {
                "details: available".to_string()
            });
            if expanded {
                for preview in detail_preview_lines.iter().filter_map(Value::as_str) {
                    lines.push(terminal_clean_line(preview));
                }
            }
        }
    } else if error
        .get("detailsAvailable")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        lines.push(if expanded {
            "details: unavailable".to_string()
        } else {
            "details: available".to_string()
        });
    }
    lines.truncate(if expanded { 16 } else { 6 });
    Some(lines)
}

fn terminal_final_summary_lines(final_summary: Option<&Value>) -> Option<Vec<String>> {
    let final_summary = final_summary?;
    let mut lines = Vec::new();
    lines.push("final summary".to_string());
    if let Some(result) = final_summary.get("result").and_then(Value::as_str) {
        lines.push(terminal_clean_line(&format!("result: {result}")));
    }
    if let Some(terminal) = final_summary.get("terminal").and_then(Value::as_str) {
        lines.push(terminal_clean_line(&format!("reason: {terminal}")));
    }
    if let Some(summary) = final_summary
        .get("fileChangeSummary")
        .and_then(|file_change| file_change.get("summary"))
        .and_then(Value::as_str)
    {
        lines.push(terminal_clean_line(&format!("files: {summary}")));
    }
    if let Some(summary) = final_summary
        .get("diffSummary")
        .and_then(|diff| diff.get("summary"))
        .and_then(Value::as_str)
    {
        lines.push(terminal_clean_line(&format!("diff: {summary}")));
    }
    if let Some(summary) = final_summary
        .get("commandSummary")
        .and_then(|command| command.get("summary"))
        .and_then(Value::as_str)
    {
        lines.push(terminal_clean_line(&format!("command: {summary}")));
    }
    if let Some(summary) = final_summary
        .get("verificationSummary")
        .and_then(|verification| verification.get("summary"))
        .and_then(Value::as_str)
        .filter(|summary| !summary.trim().is_empty())
    {
        lines.push(terminal_clean_line(&format!("verification: {summary}")));
    }
    if let Some(command_history) = final_summary
        .get("commandHistory")
        .and_then(Value::as_array)
        .filter(|history| !history.is_empty())
    {
        lines.push(format!("commands: {} recorded", command_history.len()));
        for command in command_history.iter().take(3) {
            if let Some(summary) = command.get("summary").and_then(Value::as_str) {
                lines.push(terminal_clean_line(&format!("cmd: {summary}")));
            }
        }
    }
    if let Some(summary) = final_summary
        .get("residualRiskSummary")
        .and_then(|risk| risk.get("summary"))
        .and_then(Value::as_str)
        .filter(|summary| !summary.trim().is_empty() && *summary != "none recorded")
    {
        lines.push(terminal_clean_line(&format!("risk: {summary}")));
    }
    lines.truncate(12);
    Some(lines)
}

fn terminal_slash_result_widget_from_payload(payload: &Value) -> Value {
    let request_id = payload
        .get("requestId")
        .and_then(Value::as_str)
        .unwrap_or("slash-command");
    let command = payload
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let status = payload
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("completed");
    let summary = payload
        .get("summary")
        .and_then(Value::as_str)
        .unwrap_or("slash command result");
    let error = payload.get("error").and_then(Value::as_str);

    let mut widget =
        terminal_slash_result_widget_from_parts(request_id, command, status, summary, None, error);
    terminal_copy_slash_result_preview_payload(payload, &mut widget);
    terminal_copy_slash_result_region_contract_payload(payload, &mut widget);
    terminal_copy_slash_result_region_render_payload(payload, &mut widget);
    terminal_copy_slash_result_region_patch_payload(payload, &mut widget);
    terminal_attach_slash_result_region_render(&mut widget);
    terminal_attach_slash_result_region_patch(&mut widget);
    widget
}

fn terminal_slash_result_widget(
    request_id: &str,
    response: &Value,
    summary: &str,
    error: Option<&str>,
) -> Value {
    let command = response
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let status = response
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or_else(|| {
            if error.is_some() {
                "error"
            } else {
                "completed"
            }
        });

    terminal_slash_result_widget_from_parts(
        request_id,
        command,
        status,
        summary,
        Some(response),
        error,
    )
}

fn terminal_slash_result_widget_from_parts(
    request_id: &str,
    command: &str,
    status: &str,
    summary: &str,
    response: Option<&Value>,
    error: Option<&str>,
) -> Value {
    let all_lines = terminal_slash_result_preview_lines(command, response, error);
    let total_line_count = all_lines.len();
    let preview_lines = all_lines
        .iter()
        .take(STREAM_JSON_RENDER_SLASH_RESULT_PREVIEW_MAX_LINES)
        .cloned()
        .collect::<Vec<_>>();
    let preview_line_count = preview_lines.len();
    let omitted_line_count = total_line_count.saturating_sub(preview_lines.len());

    let mut widget = json!({
        "kind": "slash_command_result",
        "requestId": request_id,
        "command": terminal_clean_line(command),
        "status": terminal_clean_line(status),
        "summary": terminal_clean_line(summary),
        "error": error.map(terminal_clean_line),
        "previewLines": preview_lines,
        "previewLineCount": preview_line_count,
        "totalLineCount": total_line_count,
        "omittedLineCount": omitted_line_count,
        "previewLimit": STREAM_JSON_RENDER_SLASH_RESULT_PREVIEW_MAX_LINES,
        "independentRegion": true,
        "bounded": true,
        "redacted": true,
        "rawResponseIncluded": false,
        "terminalRegion": terminal_slash_result_region_contract_value(),
    });
    terminal_attach_slash_result_region_render(&mut widget);
    terminal_attach_slash_result_region_patch(&mut widget);
    widget
}

fn terminal_enrich_region_contract_event_value(value: &mut Value) {
    let kind = value
        .get("kind")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_default();
    let Some(payload) = value.get_mut("payload").and_then(Value::as_object_mut) else {
        return;
    };

    if kind == "slash_command_result" {
        payload.insert(
            "terminalRegion".to_string(),
            terminal_slash_result_region_contract_value(),
        );
    }

    if terminal_slash_result_lifecycle_retire_kind(&kind) {
        payload.insert("retireRegionIds".to_string(), json!(["slash_result"]));
        payload.insert(
            "terminalRetireRegions".to_string(),
            json!([terminal_slash_result_retire_region_contract_value(&kind)]),
        );
        payload.insert(
            "terminalRegionPatch".to_string(),
            terminal_slash_result_retire_region_patch_value(&kind),
        );
    }
}

fn terminal_slash_result_region_contract_value() -> Value {
    json!({
        "id": "slash_result",
        "regionId": "slash_result",
        "role": "slash_result",
        "anchor": "top",
        "placement": "top",
        "updateMode": "replace_slash_result",
        "drawRegionField": "slashResultRegionId",
        "independent": true,
        "independentRegion": true,
        "bounded": true,
        "boundedPreview": true,
        "redacted": true,
        "rawResponseIncluded": false,
        "activeDuplicateSuppression": true,
        "retireSignalField": "terminalRetireRegions",
        "retireUpdateMode": "clear_retired",
        "retireOnKinds": [
            "turn_started",
            "compact_boundary",
            "compact_request_status",
            "conversation_cleared",
            "clear_request_status",
        ],
    })
}

fn terminal_slash_result_retire_region_contract_value(kind: &str) -> Value {
    json!({
        "id": "slash_result",
        "regionId": "slash_result",
        "role": "slash_result",
        "anchor": "top",
        "placement": "top",
        "updateMode": "clear_retired",
        "reason": "slash_result_lifecycle_boundary",
        "retiredByEventKind": kind,
        "requiresExplicitClear": true,
    })
}

fn terminal_slash_result_patch_safety_value() -> Value {
    json!({
        "patchSafeLines": true,
        "maxLineCells": STREAM_JSON_RENDER_PATCH_MAX_LINE_CELLS,
        "terminalControlSequencesStripped": true,
        "inlineControlsNormalized": true,
        "boundedLineCells": true,
    })
}

fn terminal_slash_result_patch_top_stack_layout_value(
    line_count: usize,
    max_line_count: u64,
) -> Value {
    json!({
        "schemaVersion": 1,
        "regionId": "slash_result",
        "anchor": "top",
        "layoutMode": "dynamic_top_stack",
        "topStartRow": STREAM_JSON_RENDER_SLASH_RESULT_EVENT_TOP_START_ROW,
        "topLineCount": line_count,
        "maxLineCount": max_line_count,
        "rowExpression": format!("top+{}", STREAM_JSON_RENDER_SLASH_RESULT_EVENT_TOP_START_ROW),
        "precedingRegionIds": ["status"],
        "requiredPrecedingRegionIds": ["status"],
        "statusRegionRows": STREAM_JSON_RENDER_SLASH_RESULT_EVENT_TOP_START_ROW,
        "startRowSource": "event_patch_status_baseline",
        "framePatchFallback": true,
        "conflictPolicy": "prefer_frame_patch_layout",
        "preventsStatusOverwrite": true,
    })
}

fn terminal_slash_result_retire_top_stack_layout_value() -> Value {
    json!({
        "schemaVersion": 1,
        "regionId": "slash_result",
        "anchor": "top",
        "layoutMode": "previous_client_layout",
        "requiresPreviousLayout": true,
        "conflictPolicy": "skip_if_region_absent",
    })
}

fn terminal_slash_result_replace_scroll_value() -> Value {
    json!({
        "stable": true,
        "preserveOnActiveUpdate": true,
        "preserveDuringManualScroll": true,
        "manualScrollPolicy": "hold_noncritical_top_region_update",
        "manualScrollBypass": false,
        "historyPolicy": "update_top_region",
        "commitToScrollback": false,
        "appendOnce": false,
    })
}

fn terminal_slash_result_retire_scroll_value() -> Value {
    json!({
        "stable": true,
        "preserveOnActiveUpdate": true,
        "preserveDuringManualScroll": false,
        "manualScrollPolicy": "bypass_for_lifecycle_clear",
        "manualScrollBypass": true,
        "historyPolicy": "clear_retired_region",
        "commitToScrollback": false,
        "appendOnce": false,
    })
}

fn terminal_slash_result_retire_region_patch_value(kind: &str) -> Value {
    let operation = json!({
        "op": "clear_region",
        "regionId": "slash_result",
        "role": "slash_result",
        "anchor": "top",
        "placement": "top",
        "updateMode": "clear_retired",
        "lineCount": 0,
        "sourceLineCount": 0,
        "safeLineCount": 0,
        "maxLineCells": STREAM_JSON_RENDER_PATCH_MAX_LINE_CELLS,
        "maxLineWidthCells": 0,
        "lineWidthCells": [],
        "truncated": false,
        "controlCharsStripped": false,
        "patchSafeLines": true,
        "previousLineCountSource": "client_region_state",
        "previousRegionHashSource": "client_region_state",
        "requiresPreviousLayout": true,
        "skipIfRegionAbsent": true,
        "lines": [],
    });
    json!({
        "schemaVersion": 1,
        "strategy": "anchored_region_patch",
        "preferredStrategy": "patch_regions",
        "regionId": "slash_result",
        "drawRegionField": "slashResultRegionId",
        "replaceWholeScreen": false,
        "dynamicTopStack": true,
        "requiresFrameTopStackLayout": true,
        "eventPatchDrawPlanCompatible": false,
        "topStackLayout": terminal_slash_result_retire_top_stack_layout_value(),
        "requiresPreviousRegionLayout": true,
        "sequenceGuard": {
            "field": "sequence",
            "dropIfNotIncreasing": true,
        },
        "dropWhenSuperseded": true,
        "idempotencyKey": format!("slash_result:clear:{kind}"),
        "skipIfRegionAbsent": true,
        "targetPreviousRegionHashField": "client_region_state.slash_result.regionHash",
        "retire": true,
        "retiredByEventKind": kind,
        "bounded": true,
        "redacted": true,
        "rawResponseIncluded": false,
        "maxLineCells": STREAM_JSON_RENDER_PATCH_MAX_LINE_CELLS,
        "maxLineWidthCells": 0,
        "lineWidthCells": [],
        "sourceLineCount": 0,
        "safeLineCount": 0,
        "truncated": false,
        "controlCharsStripped": false,
        "patchSafeLines": true,
        "safety": terminal_slash_result_patch_safety_value(),
        "operationCount": 1,
        "operations": [operation],
        "cursor": {
            "preservePrompt": true,
            "restoreAfterDraw": true,
        },
        "scroll": terminal_slash_result_retire_scroll_value(),
        "flush": {
            "policy": "immediate",
            "shouldFlush": true,
            "coalesceSafe": false,
        },
    })
}

fn terminal_slash_result_lifecycle_retire_kind(kind: &str) -> bool {
    matches!(
        kind,
        "turn_started"
            | "compact_boundary"
            | "compact_request_status"
            | "conversation_cleared"
            | "clear_request_status"
    )
}

fn terminal_event_retires_slash_result(kind: &str, payload: &Value) -> bool {
    terminal_slash_result_lifecycle_retire_kind(kind)
        || terminal_payload_retires_region(payload, "slash_result")
}

fn terminal_payload_retires_region(payload: &Value, region_id: &str) -> bool {
    payload
        .get("retireRegionIds")
        .and_then(Value::as_array)
        .is_some_and(|regions| {
            regions
                .iter()
                .any(|region| region.as_str() == Some(region_id))
        })
        || payload
            .get("terminalRetireRegions")
            .and_then(Value::as_array)
            .is_some_and(|regions| {
                regions.iter().any(|region| {
                    region.get("id").and_then(Value::as_str) == Some(region_id)
                        || region.get("regionId").and_then(Value::as_str) == Some(region_id)
                })
            })
}

fn terminal_enrich_slash_result_event_value(
    value: &mut Value,
    response: &Value,
    error: Option<&str>,
) {
    let Some(payload) = value.get_mut("payload").and_then(Value::as_object_mut) else {
        return;
    };
    let request_id = payload
        .get("requestId")
        .and_then(Value::as_str)
        .unwrap_or("slash-command");
    let summary = payload
        .get("summary")
        .and_then(Value::as_str)
        .unwrap_or("slash command result");
    let widget = terminal_slash_result_widget(request_id, response, summary, error);
    terminal_copy_slash_result_preview_fields(&widget, payload);
    terminal_copy_slash_result_region_fields(&widget, payload);
    terminal_attach_event_sequence_to_region_payloads(value);
}

fn terminal_copy_slash_result_preview_payload(payload: &Value, widget: &mut Value) {
    let Some(widget_object) = widget.as_object_mut() else {
        return;
    };
    terminal_copy_slash_result_preview_fields(payload, widget_object);
}

fn terminal_copy_slash_result_preview_fields(source: &Value, target: &mut Map<String, Value>) {
    for key in [
        "previewLines",
        "previewLineCount",
        "totalLineCount",
        "omittedLineCount",
        "previewLimit",
        "independentRegion",
        "bounded",
        "redacted",
        "rawResponseIncluded",
    ] {
        if let Some(value) = source.get(key) {
            target.insert(key.to_string(), value.clone());
        }
    }
}

fn terminal_copy_slash_result_region_contract_payload(payload: &Value, widget: &mut Value) {
    let Some(widget_object) = widget.as_object_mut() else {
        return;
    };
    if let Some(region) = payload.get("terminalRegion") {
        widget_object.insert("terminalRegion".to_string(), region.clone());
    }
}

fn terminal_copy_slash_result_region_render_payload(payload: &Value, widget: &mut Value) {
    let Some(widget_object) = widget.as_object_mut() else {
        return;
    };
    if let Some(region_render) = payload.get("terminalRegionRender") {
        widget_object.insert("terminalRegionRender".to_string(), region_render.clone());
    }
}

fn terminal_copy_slash_result_region_patch_payload(payload: &Value, widget: &mut Value) {
    let Some(widget_object) = widget.as_object_mut() else {
        return;
    };
    if let Some(region_patch) = payload.get("terminalRegionPatch") {
        widget_object.insert("terminalRegionPatch".to_string(), region_patch.clone());
    }
}

fn terminal_copy_slash_result_region_fields(source: &Value, target: &mut Map<String, Value>) {
    for key in [
        "terminalRegion",
        "terminalRegionRender",
        "terminalRegionPatch",
    ] {
        if let Some(value) = source.get(key) {
            target.insert(key.to_string(), value.clone());
        }
    }
}

fn terminal_attach_event_sequence_to_region_payloads(value: &mut Value) {
    let source_event_sequence = value.get("sequence").and_then(Value::as_u64).unwrap_or(0);
    let source_event_kind = value
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let Some(payload) = value.get_mut("payload").and_then(Value::as_object_mut) else {
        return;
    };
    for key in ["terminalRegionRender", "terminalRegionPatch"] {
        if let Some(region_payload) = payload.get_mut(key).and_then(Value::as_object_mut) {
            region_payload.insert(
                "sourceEventSequence".to_string(),
                json!(source_event_sequence),
            );
            region_payload.insert(
                "sourceEventKind".to_string(),
                Value::String(source_event_kind.clone()),
            );
            region_payload.insert(
                "eventSequenceGuard".to_string(),
                json!({
                    "field": "sequence",
                    "dropIfNotIncreasing": true,
                }),
            );
        }
    }
    if let Some(region_patch) = payload
        .get_mut("terminalRegionPatch")
        .and_then(Value::as_object_mut)
    {
        region_patch
            .entry("sequence".to_string())
            .or_insert_with(|| json!(source_event_sequence));
        region_patch.insert("dropWhenSuperseded".to_string(), Value::Bool(true));
        if let Some(operations) = region_patch
            .get_mut("operations")
            .and_then(Value::as_array_mut)
        {
            for operation in operations {
                if let Some(operation) = operation.as_object_mut() {
                    operation.insert(
                        "sourceEventSequence".to_string(),
                        json!(source_event_sequence),
                    );
                }
            }
        }
    }
}

fn terminal_slash_result_region_hash(lines: &[String]) -> String {
    terminal_frame_region(
        "slash_result",
        "slash_result",
        "top",
        "replace_slash_result",
        lines.to_vec(),
    )
    .get("regionHash")
    .and_then(Value::as_str)
    .unwrap_or_default()
    .to_string()
}

fn terminal_slash_result_patch_safe_lines(
    lines: &[Value],
) -> (Vec<String>, Vec<usize>, usize, bool, bool) {
    let mut safe_lines = Vec::new();
    let mut line_width_cells = Vec::new();
    let mut max_line_width_cells = 0usize;
    let mut any_truncated = false;
    let mut any_stripped = false;

    for line in lines.iter().filter_map(Value::as_str) {
        let (safe, width_cells, truncated, stripped) =
            stream_json_terminal_patch_safe_line(line, STREAM_JSON_RENDER_PATCH_MAX_LINE_CELLS);
        max_line_width_cells = max_line_width_cells.max(width_cells);
        any_truncated |= truncated;
        any_stripped |= stripped;
        line_width_cells.push(width_cells);
        safe_lines.push(safe);
    }

    (
        safe_lines,
        line_width_cells,
        max_line_width_cells,
        any_truncated,
        any_stripped,
    )
}

fn terminal_attach_slash_result_region_render(widget: &mut Value) {
    let Some(lines) = terminal_slash_result_lines(Some(widget)) else {
        return;
    };
    let line_count = lines.len();
    let max_line_count = STREAM_JSON_RENDER_SLASH_RESULT_PREVIEW_MAX_LINES.saturating_add(4);
    let region_hash = terminal_slash_result_region_hash(&lines);
    let idempotency_key = format!("slash_result:{region_hash}");
    let region_render = json!({
        "id": "slash_result",
        "regionId": "slash_result",
        "role": "slash_result",
        "anchor": "top",
        "placement": "top",
        "sticky": "top",
        "updateMode": "replace_slash_result",
        "drawRegionField": "slashResultRegionId",
        "regionHash": region_hash,
        "idempotencyKey": idempotency_key,
        "skipIfRegionHashUnchanged": true,
        "dedupeField": "regionHash",
        "lines": lines,
        "lineCount": line_count,
        "maxLineCount": max_line_count,
        "bounded": true,
        "redacted": true,
        "rawResponseIncluded": false,
    });
    if let Some(widget_object) = widget.as_object_mut() {
        widget_object.insert("terminalRegionRender".to_string(), region_render);
    }
}

fn terminal_attach_slash_result_region_patch(widget: &mut Value) {
    let Some(region_render) = widget.get("terminalRegionRender") else {
        return;
    };
    let lines = region_render
        .get("lines")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let (safe_lines, line_width_cells, max_line_width_cells, any_truncated, any_stripped) =
        terminal_slash_result_patch_safe_lines(&lines);
    let source_line_count = lines.len();
    let line_count = safe_lines.len();
    let max_line_count = region_render
        .get("maxLineCount")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| {
            STREAM_JSON_RENDER_SLASH_RESULT_PREVIEW_MAX_LINES.saturating_add(4) as u64
        });
    let region_hash = region_render
        .get("regionHash")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let idempotency_key = format!("slash_result:{region_hash}");
    let top_stack_layout =
        terminal_slash_result_patch_top_stack_layout_value(line_count, max_line_count);
    let operation = json!({
        "op": "replace_region",
        "regionId": "slash_result",
        "role": "slash_result",
        "anchor": "top",
        "placement": "top",
        "updateMode": "replace_slash_result",
        "regionHash": region_hash.clone(),
        "skipIfRegionHashUnchanged": true,
        "lineCount": line_count,
        "topStartRow": STREAM_JSON_RENDER_SLASH_RESULT_EVENT_TOP_START_ROW,
        "topLineCount": line_count,
        "layoutMode": "dynamic_top_stack",
        "topStackLayout": top_stack_layout.clone(),
        "sourceLineCount": source_line_count,
        "safeLineCount": line_count,
        "maxLineCount": max_line_count,
        "maxLineCells": STREAM_JSON_RENDER_PATCH_MAX_LINE_CELLS,
        "maxLineWidthCells": max_line_width_cells,
        "lineWidthCells": line_width_cells.clone(),
        "truncated": any_truncated,
        "controlCharsStripped": any_stripped,
        "patchSafeLines": true,
        "sourceLinesField": "terminalRegionRender.lines",
        "bounded": true,
        "redacted": true,
        "rawResponseIncluded": false,
        "lines": safe_lines,
    });
    let region_patch = json!({
        "schemaVersion": 1,
        "strategy": "anchored_region_patch",
        "preferredStrategy": "patch_regions",
        "regionId": "slash_result",
        "drawRegionField": "slashResultRegionId",
        "regionHash": region_hash.clone(),
        "idempotencyKey": idempotency_key,
        "skipIfRegionHashUnchanged": true,
        "dedupeField": "regionHash",
        "sourceRegionHashField": "terminalRegionRender.regionHash",
        "replaceWholeScreen": false,
        "dynamicTopStack": true,
        "requiresFrameTopStackLayout": true,
        "eventPatchDrawPlanCompatible": true,
        "topStackLayout": top_stack_layout,
        "sequenceGuard": {
            "field": "sequence",
            "dropIfNotIncreasing": true,
        },
        "dropWhenSuperseded": true,
        "sourceRegionRenderField": "terminalRegionRender",
        "bounded": true,
        "redacted": true,
        "rawResponseIncluded": false,
        "maxLineCells": STREAM_JSON_RENDER_PATCH_MAX_LINE_CELLS,
        "maxLineWidthCells": max_line_width_cells,
        "lineWidthCells": line_width_cells,
        "sourceLineCount": source_line_count,
        "safeLineCount": line_count,
        "truncated": any_truncated,
        "controlCharsStripped": any_stripped,
        "patchSafeLines": true,
        "safety": terminal_slash_result_patch_safety_value(),
        "operationCount": 1,
        "operations": [operation],
        "cursor": {
            "preservePrompt": true,
            "restoreAfterDraw": true,
        },
        "scroll": terminal_slash_result_replace_scroll_value(),
        "flush": {
            "policy": "immediate",
            "shouldFlush": true,
            "coalesceSafe": false,
        },
    });
    if let Some(widget_object) = widget.as_object_mut() {
        widget_object.insert("terminalRegionPatch".to_string(), region_patch);
    }
}

fn terminal_slash_result_preview_lines(
    command: &str,
    response: Option<&Value>,
    error: Option<&str>,
) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(error) = error {
        lines.push(terminal_clean_line(&format!("error: {error}")));
    }

    let Some(response) = response else {
        return lines;
    };

    match command {
        "help" => terminal_append_slash_help_preview_lines(response, &mut lines),
        "capabilities" => {
            if let Some(capabilities) = response.get("capabilities").and_then(Value::as_array) {
                lines.push(format!("capabilities: {}", capabilities.len()));
            }
            terminal_append_slash_object_preview_lines(response, &mut lines);
        }
        "permissions" => {
            if let Some(permissions) = response.get("permissions") {
                terminal_append_slash_object_preview_lines(permissions, &mut lines);
            }
        }
        _ => {
            if let Some(details) = response.get(command) {
                terminal_append_slash_object_preview_lines(details, &mut lines);
            } else {
                terminal_append_slash_object_preview_lines(response, &mut lines);
            }
        }
    }

    lines
        .into_iter()
        .map(|line| terminal_clean_line(&line))
        .filter(|line| !line.is_empty())
        .collect()
}

fn terminal_append_slash_help_preview_lines(response: &Value, lines: &mut Vec<String>) {
    let Some(commands) = response.get("commands").and_then(Value::as_array) else {
        return;
    };
    lines.push(format!("commands: {}", commands.len()));
    for entry in commands {
        if let Some(line) = terminal_slash_help_command_line(entry) {
            lines.push(line);
        }
    }
}

fn terminal_slash_help_command_line(entry: &Value) -> Option<String> {
    let name = entry.get("name").and_then(Value::as_str)?;
    let supported = entry
        .get("supported")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let title = entry
        .get("title")
        .or_else(|| entry.get("summary"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let status = if supported { "ready" } else { "known" };
    let line = if title.trim().is_empty() {
        format!("/{name} - {status}")
    } else {
        format!("/{name} - {status} - {title}")
    };
    Some(terminal_clean_line(&line))
}

fn terminal_append_slash_object_preview_lines(value: &Value, lines: &mut Vec<String>) {
    let Some(object) = value.as_object() else {
        if !value.is_null() {
            lines.push(format!("value: {}", terminal_preview_value(value)));
        }
        return;
    };

    for key in [
        "action",
        "mode_label",
        "mode",
        "status",
        "active",
        "pending",
        "dry_run",
        "requires_confirm",
        "run_requires_confirm",
        "execution_stage",
        "source",
        "next",
    ] {
        terminal_append_slash_object_field(object, key, lines);
    }

    if let Some(rule_counts) = object.get("rule_counts").and_then(Value::as_object) {
        let allow = rule_counts
            .get("allow")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let deny = rule_counts.get("deny").and_then(Value::as_u64).unwrap_or(0);
        let ask = rule_counts.get("ask").and_then(Value::as_u64).unwrap_or(0);
        lines.push(format!("rules: allow {allow}, deny {deny}, ask {ask}"));
    }
    if let Some(options) = object
        .get("mode_options")
        .or_else(|| object.get("available_modes"))
        .and_then(Value::as_array)
    {
        lines.push(format!("options: {}", options.len()));
    }

    let mut keys = object.keys().map(String::as_str).collect::<Vec<_>>();
    keys.sort_unstable();
    for key in keys {
        if lines.len() >= STREAM_JSON_RENDER_SLASH_RESULT_PREVIEW_MAX_LINES.saturating_mul(2) {
            break;
        }
        if terminal_slash_result_priority_key(key) || !terminal_slash_result_safe_key(key) {
            continue;
        }
        terminal_append_slash_object_field(object, key, lines);
    }
}

fn terminal_append_slash_object_field(
    object: &Map<String, Value>,
    key: &str,
    lines: &mut Vec<String>,
) {
    if !terminal_slash_result_safe_key(key) {
        return;
    }
    let Some(value) = object.get(key) else {
        return;
    };
    let Some(preview) = terminal_slash_result_value_preview(value) else {
        return;
    };
    lines.push(terminal_clean_line(&format!("{key}: {preview}")));
}

fn terminal_slash_result_priority_key(key: &str) -> bool {
    matches!(
        key,
        "action"
            | "mode_label"
            | "mode"
            | "status"
            | "active"
            | "pending"
            | "dry_run"
            | "requires_confirm"
            | "run_requires_confirm"
            | "execution_stage"
            | "source"
            | "next"
            | "rule_counts"
            | "mode_options"
            | "available_modes"
    )
}

fn terminal_slash_result_safe_key(key: &str) -> bool {
    let normalized = key
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    ![
        "secret",
        "token",
        "apikey",
        "password",
        "credential",
        "authorization",
        "custominstruction",
        "raw",
    ]
    .iter()
    .any(|blocked| normalized.contains(blocked))
}

fn terminal_slash_result_value_preview(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::Bool(value) => Some(value.to_string()),
        Value::Number(value) => Some(value.to_string()),
        Value::String(value) => {
            let line = terminal_clean_line(value);
            (!line.is_empty()).then_some(line)
        }
        Value::Array(values) => Some(format!("{} item(s)", values.len())),
        Value::Object(object) => Some(format!("object[{}]", object.len())),
    }
}

fn terminal_slash_result_lines(slash_result: Option<&Value>) -> Option<Vec<String>> {
    let slash_result = slash_result?;
    let mut lines = Vec::new();
    let command = slash_result
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let status = slash_result
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("completed");
    lines.push(terminal_clean_line(&format!("slash result: /{command}")));
    lines.push(terminal_clean_line(&format!("status: {status}")));
    if let Some(summary) = slash_result.get("summary").and_then(Value::as_str) {
        lines.push(terminal_clean_line(summary));
    }
    if let Some(error) = slash_result.get("error").and_then(Value::as_str) {
        lines.push(terminal_clean_line(&format!("error: {error}")));
    }
    if let Some(preview_lines) = slash_result.get("previewLines").and_then(Value::as_array) {
        for preview in preview_lines.iter().filter_map(Value::as_str) {
            lines.push(terminal_clean_line(preview));
        }
    }
    if let Some(omitted) = slash_result
        .get("omittedLineCount")
        .and_then(Value::as_u64)
        .filter(|omitted| *omitted > 0)
    {
        lines.push(format!("... {omitted} more line(s) hidden"));
    }
    lines.truncate(STREAM_JSON_RENDER_SLASH_RESULT_PREVIEW_MAX_LINES.saturating_add(4));
    Some(lines)
}

fn terminal_slash_result_activity_active(
    activity: Option<&Value>,
    slash_result_active: bool,
) -> bool {
    slash_result_active
        && activity
            .and_then(|activity| activity.get("kind"))
            .and_then(Value::as_str)
            == Some("slash_command_result")
}

fn terminal_transcript_lines(state: &StreamJsonRenderStreamState) -> Vec<String> {
    if !state.terminal_finished || state.assistant_text_transcript.trim().is_empty() {
        return Vec::new();
    }

    let mut body_lines = state
        .assistant_text_transcript
        .lines()
        .map(terminal_clean_line)
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>();
    if body_lines.is_empty() {
        body_lines.push(terminal_clean_line(&state.assistant_text_transcript));
    }

    let omitted_lines = body_lines
        .len()
        .saturating_sub(STREAM_JSON_RENDER_TRANSCRIPT_MAX_LINES);
    if omitted_lines > 0 {
        body_lines = body_lines.split_off(omitted_lines);
    }

    let mut lines = Vec::with_capacity(body_lines.len().saturating_add(4));
    lines.push("assistant transcript".to_string());
    if state.assistant_text_transcript_omitted_bytes > 0 {
        lines.push(format!(
            "... {} earlier transcript byte(s) omitted",
            state.assistant_text_transcript_omitted_bytes
        ));
    }
    if omitted_lines > 0 {
        lines.push(format!(
            "... {omitted_lines} earlier transcript line(s) omitted"
        ));
    }
    lines.extend(body_lines);
    lines.push(terminal_footer_line(state));
    lines
}

fn terminal_stage_label(stage: &str) -> &'static str {
    match stage {
        "idle" => "Idle",
        "thinking" => "Thinking",
        "planning" => "Planning",
        "reading_repo" => "Reading repo",
        "editing_files" => "Editing files",
        "waiting_approval" => "Waiting approval",
        "running_command" => "Running command",
        "reviewing_result" => "Reviewing result",
        "retrying" => "Retrying",
        "done" => "Done",
        "failed" => "Failed",
        "cancelled" => "Cancelled",
        _ => "Working",
    }
}

fn terminal_success_from_terminal(terminal: &str) -> Option<bool> {
    match UiStage::from_terminal(terminal) {
        UiStage::Done => Some(true),
        UiStage::Failed | UiStage::Cancelled => Some(false),
        _ => None,
    }
}

fn terminal_scope_label(scope: &Value) -> String {
    match scope.get("kind").and_then(Value::as_str) {
        Some("task") => scope
            .get("taskId")
            .and_then(Value::as_str)
            .map(|task_id| format!("task {task_id}"))
            .unwrap_or_else(|| "task".to_string()),
        _ => "main".to_string(),
    }
}

fn terminal_status_bar_value(
    state: &StreamJsonRenderStreamState,
    stage_label: &str,
    scope_label: &str,
) -> Value {
    let model = state.current_model.as_deref().unwrap_or("unknown");
    let (mode, mode_label, mode_short, mode_source) = terminal_status_permission_mode();
    let reasoning = terminal_status_reasoning_label(state);
    let reasoning_short = terminal_status_reasoning_short_label(state);
    let elapsed_ms = terminal_status_elapsed_ms(state);
    let elapsed = terminal_status_elapsed_label(elapsed_ms);
    let context_tokens = terminal_status_context_tokens(state);
    let context_window_tokens = terminal_status_context_window_tokens(model);
    let context = terminal_status_context_label(context_tokens, context_window_tokens);
    let model_short = terminal_status_label(model, 18);

    let full_line = terminal_clean_line(&format!(
        "{stage_label} {elapsed} | {scope_label} | model:{model} | mode:{mode_label} | reasoning:{reasoning} | ctx:{context}"
    ));
    let compact_line = terminal_clean_line(&format!(
        "{stage_label} {elapsed} | {scope_label} | {model_short} | mode:{mode_short} | r:{reasoning_short} | ctx:{context}"
    ));
    let minimal_line = terminal_clean_line(&format!(
        "{stage_label} {elapsed} | {scope_label} | mode:{mode_short} | ctx:{context}"
    ));
    let line = if full_line.chars().count() <= STREAM_JSON_TERMINAL_STATUS_LINE_FULL_MAX_CHARS {
        full_line.clone()
    } else if compact_line.chars().count() <= STREAM_JSON_TERMINAL_STATUS_LINE_COMPACT_MAX_CHARS {
        compact_line.clone()
    } else {
        terminal_status_label(
            &minimal_line,
            STREAM_JSON_TERMINAL_STATUS_LINE_COMPACT_MAX_CHARS,
        )
    };

    json!({
        "line": line,
        "fullLine": full_line,
        "compactLine": compact_line,
        "minimalLine": minimal_line,
        "stage": stage_label,
        "scope": scope_label,
        "elapsedMs": elapsed_ms,
        "elapsed": elapsed,
        "model": model,
        "mode": {
            "value": mode,
            "label": mode_label,
            "short": mode_short,
            "source": mode_source,
        },
        "reasoning": {
            "state": reasoning,
            "short": reasoning_short,
            "thinkingBytes": state.status_thinking_bytes,
        },
        "context": {
            "tokens": context_tokens,
            "windowTokens": context_window_tokens,
            "label": context,
            "inputTokens": state.status_input_tokens,
            "outputTokens": state.status_output_tokens,
            "cacheReadInputTokens": state.status_cache_read_input_tokens,
            "cacheCreationInputTokens": state.status_cache_creation_input_tokens,
            "compactAfterTokens": state.status_compact_after_tokens,
        },
        "widthVariants": {
            "fullMaxChars": STREAM_JSON_TERMINAL_STATUS_LINE_FULL_MAX_CHARS,
            "compactMaxChars": STREAM_JSON_TERMINAL_STATUS_LINE_COMPACT_MAX_CHARS,
            "full": full_line,
            "compact": compact_line,
            "minimal": minimal_line,
        },
    })
}

fn terminal_status_elapsed_ms(state: &StreamJsonRenderStreamState) -> u64 {
    match (state.turn_started_at_ms, state.last_emitted_at_ms) {
        (Some(started), Some(current)) if current >= started => current - started,
        _ => 0,
    }
}

fn terminal_status_elapsed_label(elapsed_ms: u64) -> String {
    let seconds = elapsed_ms / 1000;
    if seconds < 60 {
        format!("{seconds}s")
    } else {
        format!("{}m{}s", seconds / 60, seconds % 60)
    }
}

fn terminal_status_reasoning_label(state: &StreamJsonRenderStreamState) -> &'static str {
    if state
        .current_activity
        .as_ref()
        .and_then(|activity| activity.get("kind"))
        .and_then(Value::as_str)
        == Some("thinking")
        || state.current_stage == "thinking"
    {
        "active"
    } else if state.status_thinking_bytes > 0 {
        "seen"
    } else {
        "idle"
    }
}

fn terminal_status_reasoning_short_label(state: &StreamJsonRenderStreamState) -> &'static str {
    match terminal_status_reasoning_label(state) {
        "active" => "on",
        "seen" => "seen",
        _ => "idle",
    }
}

fn terminal_status_context_tokens(state: &StreamJsonRenderStreamState) -> Option<u64> {
    state.status_compact_after_tokens.or_else(|| {
        let tokens = state
            .status_input_tokens
            .saturating_add(state.status_output_tokens)
            .saturating_add(state.status_cache_read_input_tokens)
            .saturating_add(state.status_cache_creation_input_tokens);
        (tokens > 0).then_some(tokens)
    })
}

fn terminal_status_context_window_tokens(model: &str) -> Option<u64> {
    terminal_context_window_tokens(model)
}

fn terminal_status_context_label(tokens: Option<u64>, window_tokens: Option<u64>) -> String {
    let used = tokens
        .map(terminal_status_token_label)
        .unwrap_or_else(|| "?".to_string());
    match window_tokens {
        Some(window) => format!("{used}/{}", terminal_status_token_label(window)),
        None => used,
    }
}

fn terminal_status_token_label(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        if tokens % 1_000_000 == 0 {
            format!("{}m", tokens / 1_000_000)
        } else {
            format!("{:.1}m", tokens as f64 / 1_000_000.0)
        }
    } else if tokens >= 1_000 {
        if tokens % 1_000 == 0 {
            format!("{}k", tokens / 1_000)
        } else {
            format!("{:.1}k", tokens as f64 / 1_000.0)
        }
    } else {
        tokens.to_string()
    }
}

fn terminal_status_permission_mode() -> (&'static str, &'static str, &'static str, &'static str) {
    let raw_mode = std::env::var(STREAM_JSON_TERMINAL_PERMISSION_MODE_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let source = if raw_mode.is_some() { "env" } else { "default" };
    let key = raw_mode
        .as_deref()
        .map(terminal_status_permission_mode_key)
        .unwrap_or_else(|| "default".to_string());
    let (value, label, short) = match key.as_str() {
        "plan" => ("plan", "Plan", "plan"),
        "acceptedits" => ("acceptEdits", "Accept Edits", "edit"),
        "bypasspermissions" | "bypass" | "fullauto" => ("bypassPermissions", "Full Auto", "full"),
        "dontask" | "dontprompt" | "neverask" => ("dontAsk", "Don't Ask", "ask"),
        "auto" => ("auto", "Auto", "auto"),
        "yolo" => ("yolo", "Yolo", "yolo"),
        _ => ("default", "Supervised", "sup"),
    };
    (value, label, short, source)
}

fn terminal_status_permission_mode_key(raw: &str) -> String {
    raw.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn terminal_status_label(value: &str, max_chars: usize) -> String {
    let line = terminal_clean_line(value);
    if max_chars == 0 || line.chars().count() <= max_chars {
        return line;
    }
    if max_chars <= 3 {
        return line.chars().take(max_chars).collect();
    }
    let prefix: String = line.chars().take(max_chars.saturating_sub(3)).collect();
    format!("{prefix}...")
}

fn terminal_status_line_variants(status_bar: &Value) -> Value {
    let variants = status_bar.get("widthVariants");
    let fallback = status_bar
        .get("line")
        .and_then(Value::as_str)
        .unwrap_or("Working | main");
    json!({
        "full": variants
            .and_then(|value| value.get("full"))
            .and_then(Value::as_str)
            .map(terminal_clean_line)
            .unwrap_or_else(|| terminal_clean_line(fallback)),
        "compact": variants
            .and_then(|value| value.get("compact"))
            .and_then(Value::as_str)
            .map(terminal_clean_line)
            .unwrap_or_else(|| terminal_clean_line(fallback)),
        "minimal": variants
            .and_then(|value| value.get("minimal"))
            .and_then(Value::as_str)
            .map(terminal_clean_line)
            .unwrap_or_else(|| terminal_clean_line(fallback)),
    })
}

fn slash_command_result_summary(
    command: &str,
    status: &str,
    response: &Value,
    error: Option<&str>,
) -> String {
    if let Some(error) = error {
        return terminal_status_label(&format!("/{command} error: {error}"), 160);
    }

    let mut parts = vec![format!("/{command} {status}")];
    if let Some(details) = response.get(command).and_then(Value::as_object) {
        if let Some(action) = details.get("action").and_then(Value::as_str) {
            parts.push(format!("action {action}"));
        }
        if let Some(next) = details.get("next").and_then(Value::as_str) {
            parts.push(next.to_string());
        } else if let Some(active) = details.get("active").and_then(Value::as_bool) {
            parts.push(if active { "active" } else { "inactive" }.to_string());
        } else if let Some(pending) = details.get("pending").and_then(Value::as_bool) {
            parts.push(if pending { "pending" } else { "not pending" }.to_string());
        }
    } else if command == "help" {
        if let Some(count) = response
            .get("commands")
            .and_then(Value::as_array)
            .map(Vec::len)
        {
            parts.push(format!("{count} commands"));
        }
    } else if command == "capabilities" {
        if let Some(count) = response
            .get("capabilities")
            .and_then(Value::as_array)
            .map(Vec::len)
        {
            parts.push(format!("{count} capabilities"));
        }
    }

    terminal_status_label(&parts.join(": "), 160)
}

fn terminal_approval_active(activity: Option<&Value>) -> bool {
    activity
        .and_then(|activity| activity.get("kind"))
        .and_then(Value::as_str)
        == Some("approval_requested")
}

fn terminal_status_heartbeat_activity(activity: Option<&Value>) -> bool {
    activity
        .and_then(|activity| activity.get("kind"))
        .and_then(Value::as_str)
        == Some("status_heartbeat")
}

fn terminal_approval_lines(
    activity: Option<&Value>,
    focus_index: usize,
    action_intent: Option<&Value>,
) -> Option<Vec<String>> {
    let activity = activity?;
    if !terminal_approval_active(Some(activity)) {
        return None;
    }

    let mut lines = vec!["approval required".to_string()];
    if let Some(tool_name) = activity.get("toolName").and_then(Value::as_str) {
        lines.push(terminal_clean_line(&format!("tool: {tool_name}")));
    }
    if let Some(summary) = activity.get("summary").and_then(Value::as_str) {
        lines.push(terminal_clean_line(summary));
    }
    if let Some(preview_lines) = activity.get("inputPreviewLines").and_then(Value::as_array) {
        for preview in preview_lines.iter().filter_map(Value::as_str) {
            lines.push(terminal_clean_line(preview));
        }
    }
    lines.push(terminal_clean_line(&format!(
        "actions: {}",
        terminal_approval_action_line(focus_index)
    )));
    lines.push("select: Enter or y/n/e/a".to_string());
    lines.push("navigate: Tab/Shift-Tab or Left/Right".to_string());
    if let Some(intent) = action_intent {
        let label = intent
            .get("label")
            .and_then(Value::as_str)
            .unwrap_or("action");
        let status = intent
            .get("bridgeStatus")
            .and_then(Value::as_str)
            .unwrap_or("decision bridge pending");
        lines.push(terminal_clean_line(&format!(
            "selected: {label} ({})",
            terminal_approval_bridge_status_label(status)
        )));
        if let Some(edit_mode) = intent.get("editMode").and_then(Value::as_object) {
            if edit_mode
                .get("active")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                let command = edit_mode
                    .get("commandPreview")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                lines.push(terminal_clean_line(&format!("edit command: {command}")));
                lines.push("edit: type command, Enter submits, Esc cancels".to_string());
            }
        }
    }
    lines.push("blocking: waiting for user permission".to_string());
    Some(lines)
}

fn terminal_approval_bridge_status_label(status: &str) -> &'static str {
    match status {
        "submitted" => "submitted",
        "editing" => "editing command",
        "cancelled" => "edit cancelled",
        "interrupted" => "turn interrupted",
        "empty_command" => "empty command",
        "no_edit_command" => "no edit in progress",
        "unsupported" => "not wired",
        "no_pending_permission" => "no pending request",
        "send_failed" => "send failed",
        _ => "decision bridge pending",
    }
}

fn terminal_approval_tool_name(activity: Option<&Value>) -> Value {
    activity
        .and_then(|activity| terminal_approval_active(Some(activity)).then_some(activity))
        .and_then(|activity| activity.get("toolName").cloned())
        .unwrap_or(Value::Null)
}

fn terminal_approval_input_preview_value(activity: Option<&Value>) -> Value {
    activity
        .and_then(|activity| terminal_approval_active(Some(activity)).then_some(activity))
        .and_then(|activity| activity.get("inputPreview").cloned())
        .unwrap_or_else(|| json!({ "available": false }))
}

fn terminal_approval_action_specs() -> [(&'static str, &'static str, &'static str); 4] {
    [
        ("approve_once", "Approve once", "y"),
        ("reject", "Reject", "n"),
        ("edit_command", "Edit command", "e"),
        ("approve_for_session", "Always allow", "a"),
    ]
}

fn terminal_approval_action_line(focus_index: usize) -> String {
    terminal_approval_action_specs()
        .iter()
        .enumerate()
        .map(|(index, (_, label, _))| {
            if index == focus_index % STREAM_JSON_TERMINAL_APPROVAL_ACTION_COUNT {
                format!("[>{label}<]")
            } else {
                format!("[{label}]")
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn terminal_approval_focused_action_label(focus_index: usize) -> &'static str {
    terminal_approval_action_specs()[focus_index % STREAM_JSON_TERMINAL_APPROVAL_ACTION_COUNT].1
}

fn terminal_approval_focused_action_id(focus_index: usize) -> &'static str {
    terminal_approval_action_specs()[focus_index % STREAM_JSON_TERMINAL_APPROVAL_ACTION_COUNT].0
}

fn terminal_approval_action_model_value(
    activity: Option<&Value>,
    focus_index: usize,
    action_intent: Option<&Value>,
) -> Value {
    if !terminal_approval_active(activity) {
        return json!({
            "available": false,
            "actions": [],
            "pendingIntent": {
                "available": false,
            },
        });
    }

    let selected_index = focus_index % STREAM_JSON_TERMINAL_APPROVAL_ACTION_COUNT;
    let actions = terminal_approval_action_specs()
        .iter()
        .enumerate()
        .map(|(index, (id, label, key))| {
            json!({
                "id": id,
                "label": label,
                "key": key,
                "focused": index == selected_index,
                "primary": *id == "approve_once",
                "destructive": *id == "reject",
                "requiresEdit": *id == "edit_command",
                "sessionScoped": *id == "approve_for_session",
            })
        })
        .collect::<Vec<_>>();

    json!({
        "available": true,
        "focusNavigation": true,
        "focusKeys": ["Tab", "Shift+Tab", "Left", "Right"],
        "activation": true,
        "enterSelectsFocused": true,
        "activationKeys": ["Enter", "y", "n", "e", "a"],
        "focusedIndex": selected_index,
        "focusedAction": terminal_approval_focused_action_id(selected_index),
        "focusedLabel": terminal_approval_focused_action_label(selected_index),
        "actions": actions,
        "pendingIntent": action_intent
            .cloned()
            .unwrap_or_else(|| json!({ "available": false })),
    })
}

fn terminal_interaction_value(state: &StreamJsonRenderStreamState) -> Value {
    let hints = terminal_interaction_hints(state);
    let footer_hints = terminal_footer_visible_hints(&hints);
    let footer_hint_overflow_count = terminal_footer_hint_overflow_count(&hints);
    json!({
        "available": !state.terminal_finished,
        "visibleInFooter": !state.terminal_finished,
        "manualScrollControls": !state.terminal_finished,
        "manualScrollKeys": ["PgUp", "Home", "Up", "PageDown", "End", "Down", "Ctrl+L"],
        "commandToggleAvailable": state.current_command_widget.is_some(),
        "commandToggleKey": if state.current_command_widget.is_some() { Value::String("o".to_string()) } else { Value::Null },
        "commandExpanded": state
            .current_command_widget
            .as_ref()
            .is_some_and(terminal_widget_expanded),
        "backgroundTaskToggleAvailable": !state.current_background_tasks.is_empty(),
        "backgroundTaskToggleKey": if state.current_background_tasks.is_empty() { Value::Null } else { Value::String("b".to_string()) },
        "backgroundTaskExpanded": state.current_background_tasks_expanded,
        "fileChangeToggleAvailable": state.current_file_change_widget.is_some(),
        "fileChangeToggleKey": if state.current_file_change_widget.is_some() { Value::String("f".to_string()) } else { Value::Null },
        "fileChangeExpanded": state
            .current_file_change_widget
            .as_ref()
            .is_some_and(terminal_widget_expanded),
        "diffToggleAvailable": state.current_diff_widget.is_some(),
        "diffToggleKey": if state.current_diff_widget.is_some() { Value::String("d".to_string()) } else { Value::Null },
        "diffExpanded": state
            .current_diff_widget
            .as_ref()
            .is_some_and(terminal_widget_expanded),
        "errorToggleAvailable": state.current_error_widget.is_some(),
        "errorToggleKey": if state.current_error_widget.is_some() { Value::String("x".to_string()) } else { Value::Null },
        "errorExpanded": state
            .current_error_widget
            .as_ref()
            .is_some_and(terminal_widget_expanded),
        "approvalDecisionHints": terminal_approval_active(state.current_activity.as_ref()),
        "approvalActivationAvailable": terminal_approval_active(state.current_activity.as_ref()),
        "approvalActivationKeys": ["Enter", "y", "n", "e", "a"],
        "approvalActionModel": terminal_approval_action_model_value(
            state.current_activity.as_ref(),
            state.approval_action_focus_index,
            state.approval_action_intent.as_ref(),
        ),
        "footerHintMax": STREAM_JSON_RENDER_FOOTER_VISIBLE_HINT_MAX,
        "footerHintsBounded": true,
        "footerHints": footer_hints,
        "footerHintOverflowCount": footer_hint_overflow_count,
        "fullHints": hints.clone(),
        "hints": hints,
    })
}

fn terminal_interaction_hints(state: &StreamJsonRenderStreamState) -> Vec<String> {
    if state.terminal_finished {
        return Vec::new();
    }

    let mut hints = Vec::new();
    if terminal_approval_active(state.current_activity.as_ref()) {
        hints.push("approval pending".to_string());
        hints.push("Enter select".to_string());
        hints.push("y/n/e/a shortcuts".to_string());
        hints.push(format!(
            "Tab action: {}",
            terminal_approval_focused_action_label(state.approval_action_focus_index)
        ));
    }
    if let Some(widget) = state.current_command_widget.as_ref() {
        if terminal_widget_expanded(widget) {
            hints.push("o collapse cmd".to_string());
        } else {
            hints.push("o expand cmd".to_string());
        }
    }
    if !state.current_background_tasks.is_empty() {
        if state.current_background_tasks_expanded {
            hints.push("b collapse bg".to_string());
        } else {
            hints.push("b expand bg".to_string());
        }
    }
    if let Some(widget) = state.current_file_change_widget.as_ref() {
        if terminal_widget_expanded(widget) {
            hints.push("f collapse files".to_string());
        } else {
            hints.push("f expand files".to_string());
        }
    }
    if let Some(widget) = state.current_diff_widget.as_ref() {
        if terminal_widget_expanded(widget) {
            hints.push("d collapse diff".to_string());
        } else {
            hints.push("d expand diff".to_string());
        }
    }
    if let Some(widget) = state.current_error_widget.as_ref() {
        if terminal_widget_expanded(widget) {
            hints.push("x collapse error".to_string());
        } else {
            hints.push("x expand error".to_string());
        }
    }
    hints.push("PgUp hold".to_string());
    hints.push("PgDn/End live".to_string());
    hints.push("Ctrl+L live".to_string());
    hints
}

fn terminal_footer_visible_hints(hints: &[String]) -> Vec<String> {
    if hints.len() <= STREAM_JSON_RENDER_FOOTER_VISIBLE_HINT_MAX {
        return hints.to_vec();
    }

    let visible_count = STREAM_JSON_RENDER_FOOTER_VISIBLE_HINT_MAX.saturating_sub(1);
    let mut visible = hints
        .iter()
        .take(visible_count)
        .cloned()
        .collect::<Vec<_>>();
    let omitted_count = hints.len().saturating_sub(visible_count);
    visible.push(format!("+{omitted_count} more"));
    visible
}

fn terminal_footer_hint_overflow_count(hints: &[String]) -> usize {
    if hints.len() <= STREAM_JSON_RENDER_FOOTER_VISIBLE_HINT_MAX {
        0
    } else {
        hints
            .len()
            .saturating_sub(STREAM_JSON_RENDER_FOOTER_VISIBLE_HINT_MAX.saturating_sub(1))
    }
}

fn terminal_activity_lines(activity: Option<&Value>) -> Vec<String> {
    let Some(activity) = activity else {
        return vec!["No active render activity".to_string()];
    };
    if activity.get("kind").and_then(Value::as_str) == Some("assistant_message") {
        let mut lines = terminal_activity_preview_lines(activity);
        if lines.is_empty() {
            if let Some(summary) = activity.get("summary").and_then(Value::as_str) {
                lines.push(terminal_clean_line(summary));
            }
        }
        if lines.is_empty() {
            lines.push("assistant message".to_string());
        }
        lines.truncate(STREAM_JSON_RENDER_ASSISTANT_ACTIVITY_VISIBLE_LINES);
        while lines.len() < STREAM_JSON_RENDER_ASSISTANT_ACTIVITY_VISIBLE_LINES {
            lines.push(String::new());
        }
        return lines;
    }

    let mut lines = Vec::new();
    if let Some(summary) = activity.get("summary").and_then(Value::as_str) {
        lines.push(terminal_clean_line(summary));
    }
    for key in ["activeStep", "toolName", "toolId", "stream"] {
        if let Some(value) = activity.get(key).and_then(Value::as_str) {
            lines.push(terminal_clean_line(&format!("{key}: {value}")));
        }
    }
    lines.extend(terminal_activity_preview_lines(activity));
    if let Some(bytes) = activity.get("bytes").and_then(Value::as_u64) {
        lines.push(format!("bytes: {bytes}"));
    }
    if let Some(success) = activity.get("success").and_then(Value::as_bool) {
        lines.push(format!("success: {success}"));
    }
    if lines.is_empty() {
        lines.push(
            activity
                .get("kind")
                .and_then(Value::as_str)
                .map(terminal_clean_line)
                .unwrap_or_else(|| "activity".to_string()),
        );
    }
    lines.truncate(6);
    lines
}

fn terminal_activity_preview_lines(activity: &Value) -> Vec<String> {
    activity
        .get("previewLines")
        .and_then(Value::as_array)
        .map(|preview_lines| {
            preview_lines
                .iter()
                .filter_map(Value::as_str)
                .map(terminal_clean_line)
                .filter(|line| !line.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn terminal_footer_line(state: &StreamJsonRenderStreamState) -> String {
    let status = terminal_footer_status_line(state);

    let hints = terminal_interaction_hints(state);
    let footer_hints = terminal_footer_visible_hints(&hints);
    if hints.is_empty() {
        terminal_clean_line(&status)
    } else {
        terminal_clean_line(&format!("{status} | keys: {}", footer_hints.join(" | ")))
    }
}

fn terminal_footer_status_line(state: &StreamJsonRenderStreamState) -> String {
    if state.terminal_finished {
        let outcome = match state.terminal_success {
            Some(true) => "success",
            Some(false) => "failed",
            None => "finished",
        };
        format!(
            "turn {outcome} | reason: {}",
            state.terminal_reason.as_deref().unwrap_or("unknown")
        )
    } else if state.pending_throttled_render {
        format!(
            "streaming | coalescing updates every {}ms",
            STREAM_JSON_RENDER_EVENT_THROTTLE_MS
        )
    } else if state.needs_immediate_render {
        "ready to patch terminal regions".to_string()
    } else {
        "stable".to_string()
    }
}

fn terminal_footer_line_variants(state: &StreamJsonRenderStreamState) -> Value {
    let status = terminal_footer_status_line(state);
    let hints = terminal_interaction_hints(state);
    let compact_hints = terminal_footer_visible_hints(&hints);
    let full = if hints.is_empty() {
        terminal_clean_line(&status)
    } else {
        terminal_clean_line(&format!("{status} | keys: {}", hints.join(" | ")))
    };
    let compact = if compact_hints.is_empty() {
        terminal_clean_line(&status)
    } else {
        terminal_clean_line(&format!("{status} | keys: {}", compact_hints.join(" | ")))
    };
    json!({
        "full": full,
        "compact": compact,
        "minimal": terminal_clean_line(&status),
    })
}

fn terminal_draw_mode(refresh_policy: &str) -> &'static str {
    match refresh_policy {
        "immediate" => "patch_now",
        "throttled" => "coalesce_then_patch",
        _ => "idle",
    }
}

fn terminal_active_update_mode(history_policy: &str) -> &'static str {
    match history_policy {
        "append" => "append_history",
        "update_active" => "replace_active",
        "freeze_history" => "keep_history",
        _ => "replace_active",
    }
}

fn terminal_frame_region(
    id: &str,
    role: &str,
    sticky: &str,
    update_mode: &str,
    lines: Vec<String>,
) -> Value {
    let mut region = json!({
        "id": id,
        "role": role,
        "anchor": sticky,
        "placement": sticky,
        "sticky": sticky,
        "updateMode": update_mode,
        "lines": lines,
    });
    let region_hash = stable_render_region_hash(&region);
    if let Value::Object(map) = &mut region {
        map.insert("regionHash".to_string(), Value::String(region_hash));
    }
    region
}

fn terminal_frame_region_attach_line_variants(region: &mut Value, variants: Value, source: &str) {
    if let Value::Object(map) = region {
        map.insert("lineVariants".to_string(), variants);
        map.insert(
            "lineVariantPolicy".to_string(),
            Value::String("choose_shortest_fitting_variant".to_string()),
        );
        map.insert(
            "lineVariantSource".to_string(),
            Value::String(source.to_string()),
        );
        map.insert("viewportSelectableLines".to_string(), Value::Bool(true));
    }
    let region_hash = stable_render_region_hash(region);
    if let Value::Object(map) = region {
        map.insert("regionHash".to_string(), Value::String(region_hash));
    }
}

fn render_frame_region_fingerprints(regions: &[Value]) -> Vec<StreamJsonRenderRegionFingerprint> {
    regions
        .iter()
        .filter_map(|region| {
            let id = region.get("id").and_then(Value::as_str)?.to_string();
            let region_hash = region
                .get("regionHash")
                .and_then(Value::as_str)?
                .to_string();
            let line_count = region
                .get("lines")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or(0);
            Some(StreamJsonRenderRegionFingerprint {
                id,
                role: region
                    .get("role")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                anchor: region
                    .get("anchor")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                placement: region
                    .get("placement")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                region_hash,
                line_count,
            })
        })
        .collect()
}

fn render_frame_region_hashes(
    regions: &[StreamJsonRenderRegionFingerprint],
) -> Vec<(String, String)> {
    regions
        .iter()
        .map(|region| (region.id.clone(), region.region_hash.clone()))
        .collect()
}

fn render_frame_region_hash_object(fingerprint: &StreamJsonRenderFrameFingerprint) -> Value {
    let mut map = Map::new();
    for (id, hash) in &fingerprint.region_hashes {
        map.insert(id.clone(), Value::String(hash.clone()));
    }
    Value::Object(map)
}

fn render_frame_region_delta(
    regions: &[Value],
    previous: Option<&StreamJsonRenderFrameFingerprint>,
) -> (Vec<String>, Vec<String>, Vec<String>, Vec<Value>) {
    let mut changed = Vec::new();
    let mut unchanged = Vec::new();
    let mut current_ids = Vec::new();
    for region in regions {
        let Some(id) = region.get("id").and_then(Value::as_str) else {
            continue;
        };
        current_ids.push(id.to_string());
        let Some(region_hash) = region.get("regionHash").and_then(Value::as_str) else {
            continue;
        };
        if previous
            .and_then(|fingerprint| fingerprint.region_hash(id))
            .is_some_and(|previous_hash| previous_hash == region_hash)
        {
            unchanged.push(id.to_string());
        } else {
            changed.push(id.to_string());
        }
    }
    let retired_regions = previous
        .map(|fingerprint| {
            fingerprint
                .regions
                .iter()
                .filter(|region| !current_ids.iter().any(|id| id == &region.id))
                .map(|region| {
                    json!({
                        "id": region.id.clone(),
                        "role": region.role.clone(),
                        "anchor": region.anchor.clone(),
                        "placement": region.placement.clone(),
                        "regionHash": region.region_hash.clone(),
                        "previousLineCount": region.line_count,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let removed_region_ids = retired_regions
        .iter()
        .filter_map(|region| region.get("id").and_then(Value::as_str))
        .map(str::to_string)
        .collect::<Vec<_>>();
    changed.extend(removed_region_ids.iter().cloned());
    (changed, unchanged, removed_region_ids, retired_regions)
}

fn terminal_permission_preview_value(tool_name: &str, input: &Value, lines: &[String]) -> Value {
    json!({
        "available": !lines.is_empty(),
        "toolName": tool_name,
        "lines": lines,
        "lineCount": lines.len(),
        "maxLines": STREAM_JSON_RENDER_APPROVAL_PREVIEW_MAX_LINES,
        "redacted": true,
        "bounded": true,
    })
}

fn terminal_permission_preview_lines(tool_name: &str, input: &Value) -> Vec<String> {
    let mut lines = match tool_name {
        "Bash" | "Execute" => terminal_bash_permission_preview_lines(input),
        "Read" => terminal_path_permission_preview_lines(input, "file_path", "file"),
        "Write" => terminal_write_permission_preview_lines(input),
        "Edit" | "MultiEdit" => terminal_edit_permission_preview_lines(input),
        "Glob" => terminal_path_permission_preview_lines(input, "pattern", "pattern"),
        "Grep" => terminal_grep_permission_preview_lines(input),
        _ => terminal_generic_permission_preview_lines(input),
    };
    if lines.is_empty() {
        lines = terminal_generic_permission_preview_lines(input);
    }
    lines.truncate(STREAM_JSON_RENDER_APPROVAL_PREVIEW_MAX_LINES);
    lines
        .into_iter()
        .map(|line| terminal_clean_line(&line))
        .filter(|line| !line.is_empty())
        .collect()
}

fn terminal_bash_permission_preview_lines(input: &Value) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(command) = input.get("command").and_then(Value::as_str) {
        lines.push(format!("command: {command}"));
    }
    if let Some(cwd) = input.get("cwd").and_then(Value::as_str) {
        lines.push(format!("cwd: {cwd}"));
    }
    if let Some(description) = input.get("description").and_then(Value::as_str) {
        lines.push(format!("description: {description}"));
    }
    if let Some(timeout_ms) = input
        .get("timeout_ms")
        .or_else(|| input.get("timeout"))
        .and_then(Value::as_u64)
    {
        lines.push(format!("timeout: {timeout_ms}ms"));
    }
    lines
}

fn terminal_path_permission_preview_lines(input: &Value, key: &str, label: &str) -> Vec<String> {
    input
        .get(key)
        .and_then(Value::as_str)
        .map(|value| vec![format!("{label}: {value}")])
        .unwrap_or_default()
}

fn terminal_write_permission_preview_lines(input: &Value) -> Vec<String> {
    let mut lines = terminal_path_permission_preview_lines(input, "file_path", "file");
    if let Some(content) = input.get("content").and_then(Value::as_str) {
        lines.push(format!("content: {} chars", content.chars().count()));
    }
    lines
}

fn terminal_edit_permission_preview_lines(input: &Value) -> Vec<String> {
    let mut lines = terminal_path_permission_preview_lines(input, "file_path", "file");
    if let Some(old_string) = input.get("old_string").and_then(Value::as_str) {
        lines.push(format!("old: {old_string}"));
    }
    if let Some(new_string) = input.get("new_string").and_then(Value::as_str) {
        lines.push(format!("new: {new_string}"));
    }
    if let Some(edits) = input.get("edits").and_then(Value::as_array) {
        lines.push(format!("edits: {}", edits.len()));
    }
    lines
}

fn terminal_grep_permission_preview_lines(input: &Value) -> Vec<String> {
    let mut lines = terminal_path_permission_preview_lines(input, "pattern", "pattern");
    if let Some(path) = input.get("path").and_then(Value::as_str) {
        lines.push(format!("path: {path}"));
    }
    if let Some(glob) = input.get("glob").and_then(Value::as_str) {
        lines.push(format!("glob: {glob}"));
    }
    lines
}

fn terminal_generic_permission_preview_lines(input: &Value) -> Vec<String> {
    let Some(object) = input.as_object() else {
        return vec![format!("input: {}", terminal_preview_value(input))];
    };
    let mut keys = object.keys().collect::<Vec<_>>();
    keys.sort();
    keys.into_iter()
        .take(STREAM_JSON_RENDER_APPROVAL_PREVIEW_MAX_LINES)
        .filter_map(|key| {
            let value = object.get(key)?;
            Some(format!("{key}: {}", terminal_preview_value(value)))
        })
        .collect()
}

fn terminal_preview_value(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => value.clone(),
        Value::Array(values) => format!("array[{}]", values.len()),
        Value::Object(object) => {
            let mut keys = object.keys().map(String::as_str).collect::<Vec<_>>();
            keys.sort();
            let preview = keys.into_iter().take(3).collect::<Vec<_>>().join(", ");
            if preview.is_empty() {
                "object{}".to_string()
            } else {
                format!("object{{{preview}}}")
            }
        }
    }
}

fn terminal_clean_line(value: &str) -> String {
    let mut line = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch == '\n' || ch == '\r' || ch == '\t' {
            line.push(' ');
        } else if !ch.is_control() {
            line.push(ch);
        }
    }
    let line = line.trim().to_string();
    if line.chars().count() <= 240 {
        return line;
    }
    let prefix: String = line.chars().take(237).collect();
    format!("{prefix}...")
}

fn stable_render_region_hash(region: &Value) -> String {
    let mut hash_input = region.clone();
    if let Value::Object(map) = &mut hash_input {
        map.remove("regionHash");
    }
    stable_json_hash(&hash_input)
}

fn stable_render_frame_hash(regions: &[Value]) -> String {
    let region_hashes = regions
        .iter()
        .filter_map(|region| {
            Some(json!({
                "id": region.get("id")?.clone(),
                "regionHash": region.get("regionHash")?.clone(),
            }))
        })
        .collect::<Vec<_>>();
    stable_json_hash(&json!({ "regions": region_hashes }))
}

fn stable_json_hash(value: &Value) -> String {
    let serialized = serde_json::to_string(value).unwrap_or_default();
    let mut hash = 0xcbf29ce484222325u64;
    for byte in serialized.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

#[derive(Debug, Clone, Copy)]
struct RenderEventStreamMetadata {
    event_sequence: u64,
    source_message_sequence: u64,
    source_message_type: &'static str,
    event_index_in_source: usize,
    emitted_at_ms: u64,
}

fn stream_json_render_event_value(
    event: &RenderEvent,
    metadata: RenderEventStreamMetadata,
) -> Value {
    let mut value = Map::new();
    value.insert(
        "type".to_string(),
        Value::String(STREAM_JSON_RENDER_EVENT_TYPE.to_string()),
    );
    value.insert(
        "subtype".to_string(),
        Value::String(STREAM_JSON_RENDER_EVENT_TYPE.to_string()),
    );
    value.insert(
        "schemaVersion".to_string(),
        Value::Number(STREAM_JSON_RENDER_EVENT_SCHEMA_VERSION.into()),
    );
    value.insert(
        "sequence".to_string(),
        Value::Number(metadata.event_sequence.into()),
    );
    value.insert("stream".to_string(), render_event_stream_value(metadata));
    value.insert(
        "emittedAtMs".to_string(),
        Value::Number(metadata.emitted_at_ms.into()),
    );
    value.insert(
        "kind".to_string(),
        Value::String(render_event_kind_key(&event.kind).to_string()),
    );
    value.insert("scope".to_string(), render_event_scope_value(&event.scope));
    value.insert(
        "stage".to_string(),
        Value::String(ui_stage_key(event.stage).to_string()),
    );
    value.insert(
        "refresh".to_string(),
        render_refresh_policy_value(event.refresh),
    );
    value.insert(
        "history".to_string(),
        Value::String(render_history_policy_key(event.history).to_string()),
    );
    value.insert("payload".to_string(), render_event_payload(&event.kind));
    if let Some(turn_id) = event.turn_id.as_ref() {
        value.insert("turnId".to_string(), Value::String(turn_id.clone()));
    }
    let mut value = Value::Object(value);
    terminal_enrich_region_contract_event_value(&mut value);
    terminal_attach_event_sequence_to_region_payloads(&mut value);
    value
}

fn render_event_stream_value(metadata: RenderEventStreamMetadata) -> Value {
    json!({
        "eventSequence": metadata.event_sequence,
        "sourceMessageSequence": metadata.source_message_sequence,
        "sourceMessageType": metadata.source_message_type,
        "eventIndexInSource": metadata.event_index_in_source,
    })
}

fn sdk_message_type_key(message: &SdkMessage) -> &'static str {
    match message {
        SdkMessage::SystemInit { .. } => "system_init",
        SdkMessage::User { .. } => "user",
        SdkMessage::Assistant { .. } => "assistant",
        SdkMessage::StreamEvent { .. } => "stream_event",
        SdkMessage::Result { .. } => "result",
        SdkMessage::ToolUseSummary { .. } => "tool_use_summary",
        SdkMessage::CompactBoundary { .. } => "compact_boundary",
        SdkMessage::CompactRequestStatus { .. } => "compact_request_status",
        SdkMessage::ConversationCleared { .. } => "conversation_cleared",
        SdkMessage::ClearRequestStatus { .. } => "clear_request_status",
        SdkMessage::ApiRetry { .. } => "api_retry",
    }
}

fn unix_timestamp_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .unwrap_or_default()
}

fn render_event_scope_value(scope: &RenderEventScope) -> Value {
    match scope {
        RenderEventScope::Main => json!({ "kind": "main" }),
        RenderEventScope::Task(task_id) => json!({
            "kind": "task",
            "taskId": task_id,
        }),
    }
}

fn render_refresh_policy_value(policy: RenderRefreshPolicy) -> Value {
    match policy {
        RenderRefreshPolicy::Immediate => json!({ "policy": "immediate" }),
        RenderRefreshPolicy::Throttled { min_interval_ms } => json!({
            "policy": "throttled",
            "minIntervalMs": min_interval_ms,
        }),
        RenderRefreshPolicy::Passive => json!({ "policy": "passive" }),
    }
}

fn render_history_policy_key(policy: RenderHistoryPolicy) -> &'static str {
    match policy {
        RenderHistoryPolicy::Append => "append",
        RenderHistoryPolicy::UpdateActive => "update_active",
        RenderHistoryPolicy::FreezeHistory => "freeze_history",
    }
}

fn ui_stage_key(stage: UiStage) -> &'static str {
    match stage {
        UiStage::Idle => "idle",
        UiStage::Thinking => "thinking",
        UiStage::Planning => "planning",
        UiStage::ReadingRepo => "reading_repo",
        UiStage::EditingFiles => "editing_files",
        UiStage::WaitingApproval => "waiting_approval",
        UiStage::RunningCommand => "running_command",
        UiStage::ReviewingResult => "reviewing_result",
        UiStage::Retrying => "retrying",
        UiStage::Done => "done",
        UiStage::Failed => "failed",
        UiStage::Cancelled => "cancelled",
    }
}

fn render_event_kind_key(kind: &RenderEventKind) -> &'static str {
    match kind {
        RenderEventKind::TurnStarted => "turn_started",
        RenderEventKind::StreamStarted => "stream_started",
        RenderEventKind::TextDelta { .. } => "text_delta",
        RenderEventKind::ThinkingDelta { .. } => "thinking_delta",
        RenderEventKind::ToolInputDelta { .. } => "tool_input_delta",
        RenderEventKind::CommandStarted { .. } => "command_started",
        RenderEventKind::CommandOutput { .. } => "command_output",
        RenderEventKind::CommandFinished { .. } => "command_finished",
        RenderEventKind::BackgroundTaskUpdated { .. } => "background_task_updated",
        RenderEventKind::ToolRequested { .. } => "tool_requested",
        RenderEventKind::ToolCompleted { .. } => "tool_completed",
        RenderEventKind::PlanUpdated { .. } => "plan_updated",
        RenderEventKind::FileChangeSummary { .. } => "file_change_summary",
        RenderEventKind::DiffAvailable { .. } => "diff_available",
        RenderEventKind::ApprovalRequested { .. } => "approval_requested",
        RenderEventKind::ErrorRaised { .. } => "error_raised",
        RenderEventKind::ApiRetry { .. } => "api_retry",
        RenderEventKind::CompactBoundary { .. } => "compact_boundary",
        RenderEventKind::CompactRequestStatus { .. } => "compact_request_status",
        RenderEventKind::ConversationCleared { .. } => "conversation_cleared",
        RenderEventKind::ClearRequestStatus { .. } => "clear_request_status",
        RenderEventKind::SlashCommandResult { .. } => "slash_command_result",
        RenderEventKind::TurnFinished { .. } => "turn_finished",
        RenderEventKind::FinalSummaryRecorded { .. } => "final_summary_recorded",
    }
}

fn render_event_payload(kind: &RenderEventKind) -> Value {
    match kind {
        RenderEventKind::TurnStarted | RenderEventKind::StreamStarted => json!({}),
        RenderEventKind::TextDelta { bytes }
        | RenderEventKind::ThinkingDelta { bytes }
        | RenderEventKind::ToolInputDelta { bytes } => json!({ "bytes": bytes }),
        RenderEventKind::CommandStarted {
            tool_id,
            command,
            cwd,
        } => json!({
            "toolId": tool_id,
            "command": command,
            "cwd": cwd,
        }),
        RenderEventKind::CommandOutput {
            tool_id,
            stream,
            bytes,
            preview_lines,
            hidden_lines,
            total_lines,
            full_log_available,
        } => json!({
            "toolId": tool_id,
            "stream": stream,
            "bytes": bytes,
            "previewLines": preview_lines,
            "hiddenLines": hidden_lines,
            "totalLines": total_lines,
            "fullLogAvailable": full_log_available,
        }),
        RenderEventKind::CommandFinished {
            tool_id,
            exit_code,
            duration_ms,
        } => json!({
            "toolId": tool_id,
            "exitCode": exit_code,
            "durationMs": duration_ms,
        }),
        RenderEventKind::BackgroundTaskUpdated {
            tool_id,
            task_id,
            task_type,
            status,
            command,
            preview_lines,
            hidden_lines,
            exit_code,
        } => json!({
            "toolId": tool_id,
            "taskId": task_id,
            "taskType": task_type,
            "taskStatus": status,
            "command": command,
            "previewLines": preview_lines,
            "hiddenLines": hidden_lines,
            "exitCode": exit_code,
        }),
        RenderEventKind::ToolRequested { tool_name, tool_id }
        | RenderEventKind::ToolCompleted { tool_name, tool_id } => json!({
            "toolName": tool_name,
            "toolId": tool_id,
        }),
        RenderEventKind::PlanUpdated {
            tool_id,
            step_count,
            completed_count,
            active_count,
            pending_count,
            blocked_count,
            active_step,
        } => json!({
            "toolId": tool_id,
            "stepCount": step_count,
            "completedCount": completed_count,
            "activeCount": active_count,
            "pendingCount": pending_count,
            "blockedCount": blocked_count,
            "activeStep": active_step,
        }),
        RenderEventKind::FileChangeSummary {
            tool_id,
            file_count,
            additions,
            deletions,
        }
        | RenderEventKind::DiffAvailable {
            tool_id,
            file_count,
            additions,
            deletions,
        } => json!({
            "toolId": tool_id,
            "fileCount": file_count,
            "additions": additions,
            "deletions": deletions,
        }),
        RenderEventKind::ApprovalRequested { tool_name } => json!({
            "toolName": tool_name,
        }),
        RenderEventKind::ErrorRaised { source, summary } => json!({
            "source": source,
            "summary": summary,
        }),
        RenderEventKind::ApiRetry {
            attempt,
            max_retries,
            retry_in_ms,
        } => json!({
            "attempt": attempt,
            "maxRetries": max_retries,
            "retryInMs": retry_in_ms,
        }),
        RenderEventKind::CompactBoundary {
            before_token_count,
            after_token_count,
        } => json!({
            "beforeTokenCount": before_token_count,
            "afterTokenCount": after_token_count,
        }),
        RenderEventKind::CompactRequestStatus {
            request_id,
            status,
            dry_run,
            before_token_count,
            after_token_count,
            message_count_before,
            message_count_after,
            compacted_message_count,
            reason,
        } => json!({
            "requestId": request_id,
            "status": status,
            "dryRun": dry_run,
            "beforeTokenCount": before_token_count,
            "afterTokenCount": after_token_count,
            "messageCountBefore": message_count_before,
            "messageCountAfter": message_count_after,
            "compactedMessageCount": compacted_message_count,
            "reason": reason,
        }),
        RenderEventKind::ConversationCleared {
            message_count_before,
            message_count_after,
        } => json!({
            "messageCountBefore": message_count_before,
            "messageCountAfter": message_count_after,
        }),
        RenderEventKind::ClearRequestStatus {
            request_id,
            status,
            dry_run,
            message_count_before,
            message_count_after,
            reason,
        } => json!({
            "requestId": request_id,
            "status": status,
            "dryRun": dry_run,
            "messageCountBefore": message_count_before,
            "messageCountAfter": message_count_after,
            "reason": reason,
        }),
        RenderEventKind::SlashCommandResult {
            request_id,
            command,
            status,
            summary,
            error,
        } => json!({
            "requestId": request_id,
            "command": command,
            "status": status,
            "summary": summary,
            "error": error,
        }),
        RenderEventKind::TurnFinished { terminal } => json!({
            "terminal": terminal,
        }),
        RenderEventKind::FinalSummaryRecorded { terminal, success } => json!({
            "terminal": terminal,
            "success": success,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stream_json_terminal_renderer::{
        StreamJsonTerminalDrawScheduler, StreamJsonTerminalPatchRenderer,
        STREAM_JSON_RENDER_DRAW_PLAN_SCHEMA_VERSION, STREAM_JSON_RENDER_DRAW_PLAN_TYPE,
        STREAM_JSON_RENDER_PATCH_SCHEMA_VERSION, STREAM_JSON_RENDER_PATCH_TYPE,
    };
    use mossen_agent::types::{ApiUsage, ContentDelta, SdkMessage, StreamEventData};
    use mossen_types::{AssistantMessage, Role, TextBlock, ToolUseBlock};

    fn terminal_status_env_lock() -> std::sync::MutexGuard<'static, ()> {
        crate::test_support::env_lock()
    }

    #[test]
    fn serializes_stream_text_delta_as_throttled_render_event() {
        let message = SdkMessage::StreamEvent {
            event: StreamEventData::ContentBlockDelta {
                index: 0,
                delta: ContentDelta::TextDelta {
                    text: "hello".to_string(),
                },
            },
            task_id: None,
        };

        let events = stream_json_render_events_for_sdk_message(&message);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["type"], STREAM_JSON_RENDER_EVENT_TYPE);
        assert_eq!(events[0]["schemaVersion"], 2);
        assert_eq!(events[0]["sequence"], 1);
        assert_eq!(events[0]["stream"]["eventSequence"], 1);
        assert_eq!(events[0]["stream"]["sourceMessageSequence"], 1);
        assert_eq!(events[0]["stream"]["sourceMessageType"], "stream_event");
        assert_eq!(events[0]["stream"]["eventIndexInSource"], 0);
        assert!(events[0]["emittedAtMs"].is_u64());
        assert_eq!(events[0]["kind"], "text_delta");
        assert_eq!(events[0]["stage"], "thinking");
        assert_eq!(events[0]["refresh"]["policy"], "throttled");
        assert_eq!(
            events[0]["refresh"]["minIntervalMs"],
            STREAM_JSON_RENDER_EVENT_THROTTLE_MS
        );
        assert_eq!(events[0]["history"], "update_active");
        assert_eq!(events[0]["payload"]["bytes"], 5);
    }

    #[test]
    fn serializes_compact_request_status_as_immediate_render_event() {
        let message = SdkMessage::CompactRequestStatus {
            request_id: "compact-status-1".to_string(),
            status: mossen_agent::types::CompactRequestStatus::Skipped,
            dry_run: false,
            before_token_count: Some(77),
            after_token_count: None,
            message_count_before: Some(1),
            message_count_after: Some(1),
            compacted_message_count: Some(0),
            reason: Some("not enough messages to compact".to_string()),
            task_id: None,
        };

        let events = stream_json_render_events_for_sdk_message(&message);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["type"], STREAM_JSON_RENDER_EVENT_TYPE);
        assert_eq!(
            events[0]["stream"]["sourceMessageType"],
            "compact_request_status"
        );
        assert_eq!(events[0]["kind"], "compact_request_status");
        assert_eq!(events[0]["stage"], "reviewing_result");
        assert_eq!(events[0]["refresh"]["policy"], "immediate");
        assert_eq!(events[0]["history"], "freeze_history");
        assert_eq!(events[0]["payload"]["requestId"], "compact-status-1");
        assert_eq!(events[0]["payload"]["status"], "skipped");
        assert_eq!(events[0]["payload"]["dryRun"], false);
        assert_eq!(events[0]["payload"]["beforeTokenCount"], 77);
        assert_eq!(events[0]["payload"]["afterTokenCount"], Value::Null);
        assert_eq!(events[0]["payload"]["messageCountBefore"], 1);
        assert_eq!(events[0]["payload"]["messageCountAfter"], 1);
        assert_eq!(events[0]["payload"]["compactedMessageCount"], 0);
        assert_eq!(
            events[0]["payload"]["reason"],
            "not enough messages to compact"
        );
    }

    #[test]
    fn serializes_clear_request_status_as_immediate_render_event() {
        let message = SdkMessage::ClearRequestStatus {
            request_id: "clear-status-1".to_string(),
            status: mossen_agent::types::ClearRequestStatus::DryRun,
            dry_run: true,
            message_count_before: Some(2),
            message_count_after: Some(2),
            reason: Some("dry run only".to_string()),
            task_id: None,
        };

        let events = stream_json_render_events_for_sdk_message(&message);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["type"], STREAM_JSON_RENDER_EVENT_TYPE);
        assert_eq!(
            events[0]["stream"]["sourceMessageType"],
            "clear_request_status"
        );
        assert_eq!(events[0]["kind"], "clear_request_status");
        assert_eq!(events[0]["stage"], "reviewing_result");
        assert_eq!(events[0]["refresh"]["policy"], "immediate");
        assert_eq!(events[0]["history"], "freeze_history");
        assert_eq!(events[0]["payload"]["requestId"], "clear-status-1");
        assert_eq!(events[0]["payload"]["status"], "dry_run");
        assert_eq!(events[0]["payload"]["dryRun"], true);
        assert_eq!(events[0]["payload"]["messageCountBefore"], 2);
        assert_eq!(events[0]["payload"]["messageCountAfter"], 2);
        assert_eq!(events[0]["payload"]["reason"], "dry run only");
    }

    #[test]
    fn emits_slash_command_result_as_terminal_render_items() {
        let mut emitter = StreamJsonRenderEventEmitter::new();
        let response = json!({
            "subtype": "slash_command_result",
            "command": "compact",
            "status": "queued",
            "compact": {
                "action": "plan",
                "pending": true,
                "next": "use /compact run --confirm to apply a real compaction"
            }
        });

        let items = emitter.emit_slash_command_result_items("slash-compact-1", &response, None);

        assert_eq!(items[0]["type"], STREAM_JSON_RENDER_EVENT_TYPE);
        assert_eq!(items[0]["kind"], "slash_command_result");
        assert_eq!(items[0]["stage"], "reviewing_result");
        assert_eq!(items[0]["refresh"]["policy"], "immediate");
        assert_eq!(items[0]["history"], "freeze_history");
        assert_eq!(items[0]["payload"]["requestId"], "slash-compact-1");
        assert_eq!(items[0]["payload"]["command"], "compact");
        assert_eq!(items[0]["payload"]["status"], "queued");
        assert!(items[0]["payload"]["summary"]
            .as_str()
            .unwrap()
            .contains("/compact queued"));
        assert!(items.iter().any(|item| {
            item.get("type").and_then(Value::as_str) == Some(STREAM_JSON_RENDER_FRAME_TYPE)
                && item["regions"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|region| {
                        region["id"] == "slash_result"
                            && region["lines"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .filter_map(Value::as_str)
                                .any(|line| line.contains("/compact queued"))
                    })
        }));
        assert!(items.iter().any(|item| {
            item.get("type").and_then(Value::as_str) == Some(STREAM_JSON_RENDER_SNAPSHOT_TYPE)
                && item["terminal"]["slashResult"]["available"] == true
                && item["terminal"]["slashResult"]["rawResponseIncluded"] == false
                && item["terminal"]["slashResult"]["widget"]["previewLines"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .any(|line| line.contains("use /compact run --confirm"))
        }));
        assert!(items.iter().any(|item| {
            item.get("type").and_then(Value::as_str) == Some(STREAM_JSON_RENDER_FRAME_TYPE)
                && item["draw"]["slashResultRegionId"] == "slash_result"
                && item["regions"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|region| region["id"] == "active")
                    .and_then(|region| region["lines"].as_array())
                    .is_some_and(Vec::is_empty)
        }));
        assert!(items.iter().any(|item| {
            item.get("type").and_then(Value::as_str) == Some(STREAM_JSON_RENDER_DRAW_PLAN_TYPE)
        }));
    }

    #[test]
    fn slash_command_result_terminal_region_renders_bounded_help_catalog() {
        let mut emitter = StreamJsonRenderEventEmitter::new();
        let response = json!({
            "subtype": "slash_command_result",
            "command": "help",
            "status": "completed",
            "commands": [
                { "name": "help", "title": "Show commands", "supported": true },
                { "name": "permissions", "title": "Choose approval mode", "supported": true },
                { "name": "compact", "title": "Compact conversation", "supported": true },
                { "name": "clear", "title": "Clear conversation", "supported": true },
                { "name": "mcp", "title": "Inspect MCP servers", "supported": true },
                { "name": "config", "title": "Inspect configuration", "supported": true },
                { "name": "doctor", "title": "Run diagnostics", "supported": true },
                { "name": "login", "title": "Auth handoff", "supported": true },
                { "name": "logout", "title": "Logout handoff", "supported": true },
                { "name": "experimental", "title": "Known command", "supported": false }
            ]
        });

        let items = emitter.emit_slash_command_result_items("slash-help-1", &response, None);
        let frame = items
            .iter()
            .find(|item| {
                item.get("type").and_then(Value::as_str) == Some(STREAM_JSON_RENDER_FRAME_TYPE)
            })
            .expect("render frame");
        let slash_region = frame["regions"]
            .as_array()
            .expect("regions")
            .iter()
            .find(|region| region["id"] == "slash_result")
            .expect("slash result region");
        let lines = slash_region["lines"].as_array().expect("slash lines");

        assert_eq!(frame["draw"]["slashResultRegionId"], "slash_result");
        assert_eq!(slash_region["anchor"], "top");
        assert_eq!(slash_region["updateMode"], "replace_slash_result");
        assert!(lines
            .iter()
            .filter_map(Value::as_str)
            .any(|line| line == "commands: 10"));
        assert!(lines
            .iter()
            .filter_map(Value::as_str)
            .any(|line| line.contains("/permissions - ready")));
        assert!(lines
            .iter()
            .filter_map(Value::as_str)
            .any(|line| line.contains("more line(s) hidden")));
        assert!(
            lines.len() <= STREAM_JSON_RENDER_SLASH_RESULT_PREVIEW_MAX_LINES + 4,
            "slash result region must be bounded"
        );
    }

    #[test]
    fn slash_command_result_retires_before_clear_lifecycle_status() {
        let mut emitter = StreamJsonRenderEventEmitter::new();
        let response = json!({
            "subtype": "slash_command_result",
            "command": "clear",
            "status": "queued",
            "clear": {
                "action": "run",
                "pending": true,
                "next": "waiting for the next safe point"
            }
        });

        let initial_items =
            emitter.emit_slash_command_result_items("slash-clear-1", &response, None);
        assert!(initial_items.iter().any(|item| {
            item.get("type").and_then(Value::as_str) == Some(STREAM_JSON_RENDER_FRAME_TYPE)
                && item["draw"]["slashResultRegionId"] == "slash_result"
        }));

        let lifecycle_items =
            emitter.emit_stream_items_for_sdk_message(&SdkMessage::ClearRequestStatus {
                request_id: "clear-status-1".to_string(),
                status: mossen_agent::types::ClearRequestStatus::Completed,
                dry_run: false,
                message_count_before: Some(2),
                message_count_after: Some(0),
                reason: None,
                task_id: None,
            });
        let frame = lifecycle_items
            .iter()
            .find(|item| {
                item.get("type").and_then(Value::as_str) == Some(STREAM_JSON_RENDER_FRAME_TYPE)
            })
            .expect("render frame");
        let active_region = frame["regions"]
            .as_array()
            .expect("regions")
            .iter()
            .find(|region| region["id"] == "active")
            .expect("active region");

        assert_eq!(frame["draw"]["slashResultRegionId"], "");
        assert_eq!(frame["terminal"]["slashResult"]["available"], false);
        assert!(frame["changes"]["removedRegionIds"]
            .as_array()
            .expect("removed regions")
            .iter()
            .any(|region| region == "slash_result"));
        assert!(frame["changes"]["retiredRegions"]
            .as_array()
            .expect("retired regions")
            .iter()
            .any(|region| region["id"] == "slash_result"));
        assert!(active_region["lines"]
            .as_array()
            .expect("active lines")
            .iter()
            .filter_map(Value::as_str)
            .any(|line| line.contains("clear request completed: clear-status-1")));
    }

    #[test]
    fn slash_command_result_event_payload_carries_bounded_redacted_preview() {
        let mut emitter = StreamJsonRenderEventEmitter::new();
        let response = json!({
            "subtype": "slash_command_result",
            "command": "permissions",
            "status": "completed",
            "permissions": {
                "mode": "on-request",
                "source": "session",
                "rule_counts": { "allow": 2, "deny": 1, "ask": 3 },
                "mode_options": ["on-request", "on-failure", "never"],
                "api_token": "secret-token-value",
                "custom_instructions": "do not render",
                "raw_response": { "authorization": "Bearer secret" }
            }
        });

        let items = emitter.emit_slash_command_result_items("slash-permissions-1", &response, None);
        let event = items
            .iter()
            .find(|item| {
                item.get("type").and_then(Value::as_str) == Some(STREAM_JSON_RENDER_EVENT_TYPE)
            })
            .expect("slash result event");
        let payload = &event["payload"];
        let payload_text = payload.to_string();

        assert_eq!(
            payload["previewLimit"],
            STREAM_JSON_RENDER_SLASH_RESULT_PREVIEW_MAX_LINES
        );
        assert_eq!(payload["bounded"], true);
        assert_eq!(payload["redacted"], true);
        assert_eq!(payload["rawResponseIncluded"], false);
        assert!(payload["previewLines"]
            .as_array()
            .expect("preview lines")
            .iter()
            .filter_map(Value::as_str)
            .any(|line| line == "rules: allow 2, deny 1, ask 3"));
        assert!(
            payload["previewLines"]
                .as_array()
                .expect("preview lines")
                .len()
                <= STREAM_JSON_RENDER_SLASH_RESULT_PREVIEW_MAX_LINES
        );
        assert!(!payload_text.contains("secret-token-value"));
        assert!(!payload_text.contains("api_token"));
        assert!(!payload_text.contains("custom_instructions"));
        assert!(!payload_text.contains("raw_response"));
        assert!(!payload_text.contains("Bearer secret"));
    }

    #[test]
    fn slash_command_result_event_only_reducer_renders_preview_region() {
        let mut emitter = StreamJsonRenderEventEmitter::new();
        let response = json!({
            "subtype": "slash_command_result",
            "command": "permissions",
            "status": "completed",
            "permissions": {
                "mode": "on-request",
                "source": "session",
                "rule_counts": { "allow": 1, "deny": 0, "ask": 2 },
                "mode_options": ["on-request", "on-failure"]
            }
        });
        let event = emitter
            .emit_slash_command_result_items("slash-permissions-2", &response, None)
            .into_iter()
            .find(|item| {
                item.get("type").and_then(Value::as_str) == Some(STREAM_JSON_RENDER_EVENT_TYPE)
            })
            .expect("slash result event");

        let mut state = StreamJsonRenderStreamState::new();
        assert!(state.apply_render_event_value(&event));
        let frame = state.terminal_frame_value();
        let slash_region = frame["regions"]
            .as_array()
            .expect("regions")
            .iter()
            .find(|region| region["id"] == "slash_result")
            .expect("slash result region");
        let active_region = frame["regions"]
            .as_array()
            .expect("regions")
            .iter()
            .find(|region| region["id"] == "active")
            .expect("active region");

        assert_eq!(frame["draw"]["slashResultRegionId"], "slash_result");
        assert!(slash_region["lines"]
            .as_array()
            .expect("slash lines")
            .iter()
            .filter_map(Value::as_str)
            .any(|line| line == "rules: allow 1, deny 0, ask 2"));
        assert!(active_region["lines"]
            .as_array()
            .expect("active lines")
            .is_empty());
        assert_eq!(
            frame["terminal"]["slashResult"]["widget"]["rawResponseIncluded"],
            false
        );
    }

    #[test]
    fn slash_command_result_event_payload_carries_terminal_region_contract() {
        let mut emitter = StreamJsonRenderEventEmitter::new();
        let response = json!({
            "subtype": "slash_command_result",
            "command": "help",
            "status": "completed",
            "commands": [
                { "name": "help", "description": "Show commands", "ready": true },
                { "name": "permissions", "description": "Manage permissions", "ready": true }
            ]
        });

        let items = emitter.emit_slash_command_result_items("slash-help-region-1", &response, None);
        let event = items
            .iter()
            .find(|item| {
                item.get("type").and_then(Value::as_str) == Some(STREAM_JSON_RENDER_EVENT_TYPE)
            })
            .expect("slash result event");
        let payload = &event["payload"];
        let terminal_region = &payload["terminalRegion"];

        assert_eq!(terminal_region["id"], "slash_result");
        assert_eq!(terminal_region["anchor"], "top");
        assert_eq!(terminal_region["placement"], "top");
        assert_eq!(terminal_region["updateMode"], "replace_slash_result");
        assert_eq!(terminal_region["drawRegionField"], "slashResultRegionId");
        assert_eq!(terminal_region["activeDuplicateSuppression"], true);
        assert_eq!(terminal_region["rawResponseIncluded"], false);
        assert!(terminal_region["retireOnKinds"]
            .as_array()
            .expect("retire-on kinds")
            .iter()
            .any(|kind| kind.as_str() == Some("clear_request_status")));

        let frame = items
            .iter()
            .find(|item| {
                item.get("type").and_then(Value::as_str) == Some(STREAM_JSON_RENDER_FRAME_TYPE)
            })
            .expect("render frame");
        assert_eq!(
            frame["terminal"]["slashResult"]["widget"]["terminalRegion"]["id"],
            "slash_result"
        );
    }

    #[test]
    fn slash_result_lifecycle_event_payload_carries_retire_contract() {
        let mut emitter = StreamJsonRenderEventEmitter::new();
        let response = json!({
            "subtype": "slash_command_result",
            "command": "clear",
            "status": "queued",
            "clear": { "action": "run", "pending": true }
        });

        let initial_event = emitter
            .emit_slash_command_result_items("slash-clear-region-1", &response, None)
            .into_iter()
            .find(|item| {
                item.get("type").and_then(Value::as_str) == Some(STREAM_JSON_RENDER_EVENT_TYPE)
            })
            .expect("slash result event");
        let lifecycle_event = emitter
            .emit_stream_items_for_sdk_message(&SdkMessage::ClearRequestStatus {
                request_id: "clear-retire-region-1".to_string(),
                status: mossen_agent::types::ClearRequestStatus::Completed,
                dry_run: false,
                message_count_before: Some(3),
                message_count_after: Some(0),
                reason: None,
                task_id: None,
            })
            .into_iter()
            .find(|item| {
                item.get("type").and_then(Value::as_str) == Some(STREAM_JSON_RENDER_EVENT_TYPE)
                    && item.get("kind").and_then(Value::as_str) == Some("clear_request_status")
            })
            .expect("clear lifecycle render event");

        assert!(lifecycle_event["payload"]["retireRegionIds"]
            .as_array()
            .expect("retire region ids")
            .iter()
            .any(|region| region.as_str() == Some("slash_result")));
        assert_eq!(
            lifecycle_event["payload"]["terminalRetireRegions"][0]["id"],
            "slash_result"
        );
        assert_eq!(
            lifecycle_event["payload"]["terminalRetireRegions"][0]["updateMode"],
            "clear_retired"
        );
        assert_eq!(
            lifecycle_event["payload"]["terminalRetireRegions"][0]["retiredByEventKind"],
            "clear_request_status"
        );
        assert_eq!(
            lifecycle_event["payload"]["terminalRegionPatch"]["replaceWholeScreen"],
            false
        );
        assert_eq!(
            lifecycle_event["payload"]["terminalRegionPatch"]["operations"][0]["op"],
            "clear_region"
        );
        assert_eq!(
            lifecycle_event["payload"]["terminalRegionPatch"]["operations"][0]["regionId"],
            "slash_result"
        );
        assert_eq!(
            lifecycle_event["payload"]["terminalRegionPatch"]["operations"][0]
                ["requiresPreviousLayout"],
            true
        );
        assert_eq!(
            lifecycle_event["payload"]["terminalRegionPatch"]["sourceEventSequence"],
            lifecycle_event["sequence"]
        );
        assert_eq!(
            lifecycle_event["payload"]["terminalRegionPatch"]["sequence"],
            lifecycle_event["sequence"]
        );
        assert_eq!(
            lifecycle_event["payload"]["terminalRegionPatch"]["skipIfRegionAbsent"],
            true
        );
        assert_eq!(
            lifecycle_event["payload"]["terminalRegionPatch"]["eventSequenceGuard"]
                ["dropIfNotIncreasing"],
            true
        );
        assert_eq!(
            lifecycle_event["payload"]["terminalRegionPatch"]["patchSafeLines"],
            true
        );
        assert_eq!(
            lifecycle_event["payload"]["terminalRegionPatch"]["maxLineCells"].as_u64(),
            Some(STREAM_JSON_RENDER_PATCH_MAX_LINE_CELLS as u64)
        );
        assert_eq!(
            lifecycle_event["payload"]["terminalRegionPatch"]["maxLineWidthCells"].as_u64(),
            Some(0)
        );
        assert_eq!(
            lifecycle_event["payload"]["terminalRegionPatch"]["lineWidthCells"]
                .as_array()
                .expect("retire patch line widths")
                .len(),
            0
        );
        assert_eq!(
            lifecycle_event["payload"]["terminalRegionPatch"]["operations"][0]["patchSafeLines"],
            true
        );
        assert_eq!(
            lifecycle_event["payload"]["terminalRegionPatch"]["eventPatchDrawPlanCompatible"],
            false
        );
        assert_eq!(
            lifecycle_event["payload"]["terminalRegionPatch"]["topStackLayout"]["layoutMode"],
            "previous_client_layout"
        );
        assert_eq!(
            lifecycle_event["payload"]["terminalRegionPatch"]["scroll"]
                ["preserveDuringManualScroll"],
            false
        );
        assert_eq!(
            lifecycle_event["payload"]["terminalRegionPatch"]["scroll"]["manualScrollPolicy"],
            "bypass_for_lifecycle_clear"
        );
        assert_eq!(
            lifecycle_event["payload"]["terminalRegionPatch"]["scroll"]["manualScrollBypass"],
            true
        );
        assert_eq!(
            lifecycle_event["payload"]["terminalRegionPatch"]["scroll"]["historyPolicy"],
            "clear_retired_region"
        );

        let mut payload_driven_retire_event = lifecycle_event.clone();
        payload_driven_retire_event["kind"] = json!("stream_started");
        let mut state = StreamJsonRenderStreamState::new();
        assert!(state.apply_render_event_value(&initial_event));
        assert!(
            state.terminal_frame_value()["draw"]["slashResultRegionId"].as_str()
                == Some("slash_result")
        );
        assert!(state.apply_render_event_value(&payload_driven_retire_event));
        let frame = state.terminal_frame_value();
        assert_eq!(frame["draw"]["slashResultRegionId"], "");
        assert_eq!(frame["terminal"]["slashResult"]["available"], false);
    }

    #[test]
    fn slash_command_result_event_payload_carries_terminal_region_render() {
        let mut emitter = StreamJsonRenderEventEmitter::new();
        let response = json!({
            "subtype": "slash_command_result",
            "command": "permissions",
            "status": "completed",
            "permissions": {
                "mode": "on-request",
                "source": "session",
                "rule_counts": { "allow": 2, "deny": 1, "ask": 3 },
                "mode_options": ["on-request", "on-failure", "never"],
                "api_token": "secret-token-value"
            }
        });

        let event = emitter
            .emit_slash_command_result_items("slash-permissions-region-render-1", &response, None)
            .into_iter()
            .find(|item| {
                item.get("type").and_then(Value::as_str) == Some(STREAM_JSON_RENDER_EVENT_TYPE)
            })
            .expect("slash result event");
        let region_render = &event["payload"]["terminalRegionRender"];
        let lines = region_render["lines"]
            .as_array()
            .expect("terminal region render lines");
        let payload_text = event["payload"].to_string();

        assert_eq!(region_render["id"], "slash_result");
        assert_eq!(region_render["anchor"], "top");
        assert_eq!(region_render["updateMode"], "replace_slash_result");
        assert_eq!(region_render["drawRegionField"], "slashResultRegionId");
        assert_eq!(region_render["bounded"], true);
        assert_eq!(region_render["redacted"], true);
        assert_eq!(region_render["rawResponseIncluded"], false);
        assert_eq!(
            region_render["lineCount"].as_u64(),
            Some(lines.len() as u64)
        );
        assert_eq!(
            region_render["maxLineCount"].as_u64(),
            Some((STREAM_JSON_RENDER_SLASH_RESULT_PREVIEW_MAX_LINES + 4) as u64)
        );
        assert!(lines
            .iter()
            .filter_map(Value::as_str)
            .any(|line| line == "slash result: /permissions"));
        assert!(lines
            .iter()
            .filter_map(Value::as_str)
            .any(|line| line == "rules: allow 2, deny 1, ask 3"));
        assert!(!payload_text.contains("secret-token-value"));
        assert!(!payload_text.contains("api_token"));
    }

    #[test]
    fn slash_command_result_event_region_render_matches_frame_region() {
        let mut emitter = StreamJsonRenderEventEmitter::new();
        let response = json!({
            "subtype": "slash_command_result",
            "command": "help",
            "status": "completed",
            "commands": [
                { "name": "help", "description": "Show commands", "ready": true },
                { "name": "permissions", "description": "Manage permissions", "ready": true },
                { "name": "compact", "description": "Manage compaction", "ready": true }
            ]
        });
        let event = emitter
            .emit_slash_command_result_items("slash-help-region-render-1", &response, None)
            .into_iter()
            .find(|item| {
                item.get("type").and_then(Value::as_str) == Some(STREAM_JSON_RENDER_EVENT_TYPE)
            })
            .expect("slash result event");
        let event_lines = event["payload"]["terminalRegionRender"]["lines"]
            .as_array()
            .expect("event region lines")
            .clone();

        let mut state = StreamJsonRenderStreamState::new();
        assert!(state.apply_render_event_value(&event));
        let frame = state.terminal_frame_value();
        let frame_lines = frame["regions"]
            .as_array()
            .expect("regions")
            .iter()
            .find(|region| region["id"] == "slash_result")
            .expect("slash result region")["lines"]
            .as_array()
            .expect("frame region lines")
            .clone();
        let frame_region = frame["regions"]
            .as_array()
            .expect("regions")
            .iter()
            .find(|region| region["id"] == "slash_result")
            .expect("slash result region");

        assert_eq!(event_lines, frame_lines);
        assert_eq!(
            event["payload"]["terminalRegionRender"]["regionHash"],
            frame_region["regionHash"]
        );
    }

    #[test]
    fn slash_command_result_event_payload_carries_terminal_region_patch() {
        let mut emitter = StreamJsonRenderEventEmitter::new();
        let response = json!({
            "subtype": "slash_command_result",
            "command": "permissions",
            "status": "completed",
            "permissions": {
                "mode": "on-request",
                "source": "session",
                "rule_counts": { "allow": 2, "deny": 1, "ask": 3 },
                "mode_options": ["on-request", "on-failure", "never"],
                "api_token": "secret-token-value"
            }
        });

        let event = emitter
            .emit_slash_command_result_items("slash-permissions-region-patch-1", &response, None)
            .into_iter()
            .find(|item| {
                item.get("type").and_then(Value::as_str) == Some(STREAM_JSON_RENDER_EVENT_TYPE)
            })
            .expect("slash result event");
        let region_render = &event["payload"]["terminalRegionRender"];
        let region_patch = &event["payload"]["terminalRegionPatch"];
        let operation = &region_patch["operations"][0];
        let payload_text = event["payload"].to_string();

        assert_eq!(region_patch["strategy"], "anchored_region_patch");
        assert_eq!(region_patch["preferredStrategy"], "patch_regions");
        assert_eq!(region_patch["replaceWholeScreen"], false);
        assert_eq!(region_patch["dynamicTopStack"], true);
        assert_eq!(region_patch["requiresFrameTopStackLayout"], true);
        assert_eq!(region_patch["drawRegionField"], "slashResultRegionId");
        assert_eq!(region_patch["regionHash"], region_render["regionHash"]);
        assert_eq!(
            region_patch["idempotencyKey"],
            format!(
                "slash_result:{}",
                region_render["regionHash"].as_str().expect("region hash")
            )
        );
        assert_eq!(region_patch["skipIfRegionHashUnchanged"], true);
        assert_eq!(region_patch["dedupeField"], "regionHash");
        assert_eq!(
            region_patch["sourceRegionHashField"],
            "terminalRegionRender.regionHash"
        );
        assert_eq!(region_patch["sequence"], event["sequence"]);
        assert_eq!(region_patch["sourceEventSequence"], event["sequence"]);
        assert_eq!(
            region_patch["eventSequenceGuard"]["dropIfNotIncreasing"],
            true
        );
        assert_eq!(region_patch["dropWhenSuperseded"], true);
        assert_eq!(region_patch["patchSafeLines"], true);
        assert_eq!(
            region_patch["maxLineCells"].as_u64(),
            Some(STREAM_JSON_RENDER_PATCH_MAX_LINE_CELLS as u64)
        );
        assert_eq!(region_patch["sourceLineCount"], region_render["lineCount"]);
        assert_eq!(region_patch["safeLineCount"], region_render["lineCount"]);
        assert_eq!(region_patch["safety"]["boundedLineCells"], true);
        assert_eq!(region_patch["eventPatchDrawPlanCompatible"], true);
        assert_eq!(
            region_patch["topStackLayout"]["topStartRow"].as_u64(),
            Some(STREAM_JSON_RENDER_SLASH_RESULT_EVENT_TOP_START_ROW as u64)
        );
        assert_eq!(
            region_patch["topStackLayout"]["conflictPolicy"],
            "prefer_frame_patch_layout"
        );
        assert_eq!(
            region_patch["topStackLayout"]["preventsStatusOverwrite"],
            true
        );
        assert_eq!(region_patch["cursor"]["preservePrompt"], true);
        assert_eq!(region_patch["cursor"]["restoreAfterDraw"], true);
        assert_eq!(region_patch["scroll"]["preserveDuringManualScroll"], true);
        assert_eq!(
            region_patch["scroll"]["manualScrollPolicy"],
            "hold_noncritical_top_region_update"
        );
        assert_eq!(region_patch["scroll"]["manualScrollBypass"], false);
        assert_eq!(region_patch["scroll"]["historyPolicy"], "update_top_region");
        assert_eq!(region_patch["scroll"]["commitToScrollback"], false);
        assert_eq!(region_patch["flush"]["shouldFlush"], true);
        assert_eq!(operation["op"], "replace_region");
        assert_eq!(operation["regionId"], "slash_result");
        assert_eq!(operation["updateMode"], "replace_slash_result");
        assert_eq!(operation["regionHash"], region_render["regionHash"]);
        assert_eq!(operation["skipIfRegionHashUnchanged"], true);
        assert_eq!(operation["sourceEventSequence"], event["sequence"]);
        assert_eq!(
            operation["topStartRow"].as_u64(),
            Some(STREAM_JSON_RENDER_SLASH_RESULT_EVENT_TOP_START_ROW as u64)
        );
        assert_eq!(operation["layoutMode"], "dynamic_top_stack");
        assert_eq!(
            operation["topStackLayout"]["rowExpression"],
            format!(
                "top+{}",
                STREAM_JSON_RENDER_SLASH_RESULT_EVENT_TOP_START_ROW
            )
        );
        assert_eq!(operation["lines"], region_render["lines"]);
        assert_eq!(operation["lineCount"], region_render["lineCount"]);
        assert_eq!(operation["sourceLineCount"], region_render["lineCount"]);
        assert_eq!(operation["safeLineCount"], region_render["lineCount"]);
        assert_eq!(operation["patchSafeLines"], true);
        assert_eq!(
            operation["maxLineCells"].as_u64(),
            Some(STREAM_JSON_RENDER_PATCH_MAX_LINE_CELLS as u64)
        );
        assert_eq!(
            operation["lineWidthCells"]
                .as_array()
                .expect("operation line widths")
                .len(),
            operation["lines"]
                .as_array()
                .expect("operation lines")
                .len()
        );
        assert!(
            operation["maxLineWidthCells"]
                .as_u64()
                .expect("max line width")
                > 0
        );
        assert_eq!(operation["rawResponseIncluded"], false);
        assert!(!payload_text.contains("secret-token-value"));
        assert!(!payload_text.contains("api_token"));
    }

    #[test]
    fn slash_result_event_patch_can_render_draw_plan_without_frame_patch() {
        let mut emitter = StreamJsonRenderEventEmitter::new();
        let response = json!({
            "subtype": "slash_command_result",
            "command": "permissions",
            "status": "completed",
            "permissions": {
                "mode": "on-request",
                "source": "session",
                "rule_counts": { "allow": 2, "deny": 1, "ask": 3 }
            }
        });

        let event = emitter
            .emit_slash_command_result_items("slash-event-draw-plan-1", &response, None)
            .into_iter()
            .find(|item| {
                item.get("type").and_then(Value::as_str) == Some(STREAM_JSON_RENDER_EVENT_TYPE)
            })
            .expect("slash result event");
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let draw_plan = scheduler.render_patch_value(&event["payload"]["terminalRegionPatch"]);

        assert_eq!(draw_plan["type"], STREAM_JSON_RENDER_DRAW_PLAN_TYPE);
        assert_eq!(draw_plan["sequence"], event["sequence"]);
        assert_eq!(draw_plan["draw"]["strategy"], "anchored_region_patch");
        assert_eq!(draw_plan["draw"]["replaceWholeScreen"], false);
        assert_eq!(draw_plan["draw"]["topLayoutMode"], "dynamic_stack");
        assert_eq!(draw_plan["schedule"]["shouldFlush"], true);
        assert_eq!(draw_plan["cursor"]["restoreAfterDraw"], true);
        let expected_row = format!(
            "top+{}",
            STREAM_JSON_RENDER_SLASH_RESULT_EVENT_TOP_START_ROW
        );
        assert_eq!(
            draw_plan["regionPlans"][0]["startRow"].as_str(),
            Some(expected_row.as_str())
        );
        assert!(draw_plan["terminalOps"]
            .as_array()
            .expect("terminal ops")
            .iter()
            .any(|op| op["op"] == "save_cursor"));
        assert!(draw_plan["terminalOps"]
            .as_array()
            .expect("terminal ops")
            .iter()
            .any(|op| op["op"] == "restore_cursor"));
        assert!(draw_plan["terminalOps"]
            .as_array()
            .expect("terminal ops")
            .iter()
            .any(
                |op| op["op"] == "move_to_row" && op["row"].as_str() == Some(expected_row.as_str())
            ));
    }

    #[test]
    fn slash_result_patch_line_safety_bounds_pathological_lines() {
        let unsafe_line = format!(
            "{}\u{1b}[31m{}",
            "x".repeat(STREAM_JSON_RENDER_PATCH_MAX_LINE_CELLS + 80),
            "tail"
        );
        let lines = vec![json!(unsafe_line)];

        let (safe_lines, line_width_cells, max_line_width_cells, truncated, stripped) =
            terminal_slash_result_patch_safe_lines(&lines);

        assert_eq!(safe_lines.len(), 1);
        assert_eq!(line_width_cells.len(), safe_lines.len());
        assert!(truncated);
        assert!(stripped);
        assert!(safe_lines[0].ends_with("..."));
        assert!(!safe_lines[0].contains('\u{1b}'));
        assert!(line_width_cells[0] <= STREAM_JSON_RENDER_PATCH_MAX_LINE_CELLS);
        assert!(max_line_width_cells <= STREAM_JSON_RENDER_PATCH_MAX_LINE_CELLS);
    }

    #[test]
    fn serializes_success_result_without_empty_final_summary_event() {
        let message = SdkMessage::Result {
            terminal: "success".to_string(),
            cost_usd: None,
            duration_ms: Some(100),
            usage: None,
            task_id: Some("agent-1".to_string()),
        };

        let events = stream_json_render_events_for_sdk_message(&message);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["kind"], "turn_finished");
        assert_eq!(events[0]["scope"]["kind"], "task");
        assert_eq!(events[0]["scope"]["taskId"], "agent-1");
        assert_eq!(events[0]["stage"], "done");
        assert_eq!(events[0]["payload"]["terminal"], "success");
    }

    #[test]
    fn serializes_final_summary_after_terminal_work_activity() {
        let mut emitter = StreamJsonRenderEventEmitter::new();
        let assistant = SdkMessage::Assistant {
            message: AssistantMessage {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolUse(ToolUseBlock {
                    id: "toolu-bash".to_string(),
                    name: "Bash".to_string(),
                    input: json!({ "command": "cargo test", "cwd": "/repo" }),
                })],
                uuid: None,
                model: None,
                stop_reason: Some("tool_use".to_string()),
                extra: Default::default(),
            },
            usage: None,
            task_id: None,
        };
        let result = SdkMessage::Result {
            terminal: "success".to_string(),
            cost_usd: None,
            duration_ms: Some(100),
            usage: None,
            task_id: None,
        };

        let _ = emitter.emit_for_sdk_message(&assistant);
        let events = emitter.emit_for_sdk_message(&result);

        assert_eq!(events.len(), 2);
        assert_eq!(events[0]["kind"], "turn_finished");
        assert_eq!(events[1]["kind"], "final_summary_recorded");
        assert_eq!(events[1]["payload"]["success"], true);
    }

    #[test]
    fn emitter_assigns_monotonic_ordering_across_messages() {
        let first = SdkMessage::StreamEvent {
            event: StreamEventData::ContentBlockDelta {
                index: 0,
                delta: ContentDelta::TextDelta {
                    text: "hello".to_string(),
                },
            },
            task_id: None,
        };
        let second = SdkMessage::Result {
            terminal: "success".to_string(),
            cost_usd: None,
            duration_ms: Some(100),
            usage: None,
            task_id: None,
        };
        let mut emitter = StreamJsonRenderEventEmitter::new();

        let first_events = emitter.emit_for_sdk_message(&first);
        let second_events = emitter.emit_for_sdk_message(&second);

        assert_eq!(first_events.len(), 1);
        assert_eq!(second_events.len(), 1);
        assert_eq!(first_events[0]["sequence"], 1);
        assert_eq!(second_events[0]["sequence"], 2);
        assert_eq!(first_events[0]["stream"]["sourceMessageSequence"], 1);
        assert_eq!(second_events[0]["stream"]["sourceMessageSequence"], 2);
        assert_eq!(second_events[0]["stream"]["eventIndexInSource"], 0);
        assert_eq!(second_events[0]["stream"]["sourceMessageType"], "result");
    }

    #[test]
    fn emits_render_snapshot_after_each_source_message() {
        let first = SdkMessage::StreamEvent {
            event: StreamEventData::ContentBlockDelta {
                index: 0,
                delta: ContentDelta::TextDelta {
                    text: "hello".to_string(),
                },
            },
            task_id: None,
        };
        let second = SdkMessage::Result {
            terminal: "success".to_string(),
            cost_usd: None,
            duration_ms: Some(100),
            usage: None,
            task_id: None,
        };
        let mut emitter = StreamJsonRenderEventEmitter::new();

        let first_items = emitter.emit_stream_items_for_sdk_message(&first);
        let second_items = emitter.emit_stream_items_for_sdk_message(&second);

        assert_eq!(first_items.len(), 5);
        assert_eq!(first_items[0]["type"], STREAM_JSON_RENDER_EVENT_TYPE);
        assert_eq!(first_items[1]["type"], STREAM_JSON_RENDER_SNAPSHOT_TYPE);
        assert_eq!(first_items[1]["lastSequence"], 1);
        assert_eq!(first_items[1]["activity"]["kind"], "assistant_message");
        assert_eq!(first_items[1]["activity"]["previewLines"][0], "hello");
        assert_eq!(first_items[1]["refresh"]["pendingThrottledRender"], true);
        assert_eq!(
            first_items[1]["history"]["preserveScrollOnUpdateActive"],
            true
        );
        assert_eq!(first_items[2]["type"], STREAM_JSON_RENDER_FRAME_TYPE);
        assert_eq!(first_items[2]["sequence"], 1);
        assert_eq!(first_items[2]["draw"]["replaceWholeScreen"], false);
        assert_eq!(first_items[3]["type"], STREAM_JSON_RENDER_PATCH_TYPE);
        assert_eq!(
            first_items[3]["sourceFrame"]["frameHash"],
            first_items[2]["frameHash"]
        );
        assert_eq!(first_items[3]["draw"]["replaceWholeScreen"], false);
        assert_eq!(first_items[4]["type"], STREAM_JSON_RENDER_DRAW_PLAN_TYPE);
        assert_eq!(
            first_items[4]["sourceFrame"]["frameHash"],
            first_items[2]["frameHash"]
        );
        assert_eq!(first_items[4]["draw"]["replaceWholeScreen"], false);

        assert_eq!(second_items.len(), 5);
        assert_eq!(second_items[1]["type"], STREAM_JSON_RENDER_SNAPSHOT_TYPE);
        assert_eq!(second_items[1]["lastSequence"], 2);
        assert_eq!(second_items[1]["terminal"]["finished"], true);
        assert_eq!(second_items[1]["terminal"]["success"], true);
        assert_eq!(second_items[1]["terminal"]["transcript"]["available"], true);
        assert_eq!(second_items[2]["type"], STREAM_JSON_RENDER_FRAME_TYPE);
        assert_eq!(second_items[2]["terminal"]["finished"], true);
        assert_eq!(second_items[2]["scroll"]["commitToScrollback"], true);
        assert_eq!(
            second_items[2]["terminal"]["transcript"]["committedToScrollback"],
            true
        );
        let final_regions = second_items[2]["regions"]
            .as_array()
            .expect("final frame regions");
        let transcript_region = final_regions
            .iter()
            .find(|region| region["id"] == "transcript")
            .expect("transcript region");
        assert_eq!(transcript_region["anchor"], "scrollback");
        assert_eq!(transcript_region["updateMode"], "append_scrollback");
        assert!(transcript_region["lines"]
            .as_array()
            .expect("transcript lines")
            .iter()
            .any(|line| line.as_str() == Some("hello")));
        assert_eq!(second_items[3]["type"], STREAM_JSON_RENDER_PATCH_TYPE);
        assert_eq!(second_items[3]["terminal"]["finished"], true);
        assert_eq!(second_items[4]["type"], STREAM_JSON_RENDER_DRAW_PLAN_TYPE);
        assert!(second_items[4]["terminalOps"]
            .as_array()
            .expect("terminal ops")
            .iter()
            .any(|op| op["op"] == "append_scrollback_block"));
        assert_eq!(
            second_items[4]["sourceFrame"]["frameHash"],
            second_items[2]["frameHash"]
        );
    }

    #[test]
    fn terminal_transcript_deduplicates_final_assistant_message_after_streaming_deltas() {
        let mut emitter = StreamJsonRenderEventEmitter::new();
        let streamed_text = "dedupe head\ndedupe tail\n";
        for text in ["dedupe head\n", "dedupe tail\n"] {
            let message = SdkMessage::StreamEvent {
                event: StreamEventData::ContentBlockDelta {
                    index: 0,
                    delta: ContentDelta::TextDelta {
                        text: text.to_string(),
                    },
                },
                task_id: None,
            };
            let _ = emitter.emit_terminal_draw_plan_items_for_sdk_message(&message);
        }

        let assistant = SdkMessage::Assistant {
            message: AssistantMessage {
                role: Role::Assistant,
                content: vec![ContentBlock::Text(TextBlock {
                    text: streamed_text.to_string(),
                })],
                uuid: None,
                model: None,
                stop_reason: Some("stop".to_string()),
                extra: Default::default(),
            },
            usage: None,
            task_id: None,
        };
        let _ = emitter.emit_terminal_draw_plan_items_for_sdk_message(&assistant);
        let result = SdkMessage::Result {
            terminal: "success".to_string(),
            cost_usd: None,
            duration_ms: Some(100),
            usage: None,
            task_id: None,
        };
        let final_items = emitter.emit_terminal_draw_plan_items_for_sdk_message(&result);
        let append_op = final_items[0]["terminalOps"]
            .as_array()
            .expect("terminal ops")
            .iter()
            .find(|op| op["op"] == "append_scrollback_block")
            .expect("append scrollback op");
        let transcript_lines = append_op["lines"].as_array().expect("transcript lines");

        assert_eq!(
            transcript_lines
                .iter()
                .filter(|line| line.as_str() == Some("dedupe head"))
                .count(),
            1
        );
        assert_eq!(
            transcript_lines
                .iter()
                .filter(|line| line.as_str() == Some("dedupe tail"))
                .count(),
            1
        );
    }

    #[test]
    fn stream_state_reduces_events_and_ignores_stale_duplicates() {
        let message = SdkMessage::StreamEvent {
            event: StreamEventData::ContentBlockDelta {
                index: 0,
                delta: ContentDelta::TextDelta {
                    text: "hello".to_string(),
                },
            },
            task_id: None,
        };
        let mut emitter = StreamJsonRenderEventEmitter::new();
        let events = emitter.emit_for_sdk_message(&message);
        let mut state = StreamJsonRenderStreamState::new();

        assert!(state.apply_render_event_value(&events[0]));
        assert!(!state.apply_render_event_value(&events[0]));
        let snapshot = state.snapshot_value();

        assert_eq!(snapshot["type"], STREAM_JSON_RENDER_SNAPSHOT_TYPE);
        assert_eq!(snapshot["schemaVersion"], 1);
        assert_eq!(snapshot["eventSchemaVersion"], 2);
        assert_eq!(snapshot["lastSequence"], 1);
        assert_eq!(snapshot["appliedCount"], 1);
        assert_eq!(snapshot["ignoredStaleCount"], 1);
        assert_eq!(snapshot["stage"], "thinking");
        assert_eq!(snapshot["activity"]["kind"], "assistant_message");
        assert_eq!(snapshot["refresh"]["pendingThrottledRender"], true);
        assert_eq!(snapshot["history"]["updateActiveCount"], 1);
    }

    #[test]
    fn emits_line_oriented_terminal_frame_after_snapshot() {
        let message = SdkMessage::StreamEvent {
            event: StreamEventData::ContentBlockDelta {
                index: 0,
                delta: ContentDelta::TextDelta {
                    text: "hello".to_string(),
                },
            },
            task_id: None,
        };
        let mut emitter = StreamJsonRenderEventEmitter::new();

        let items = emitter.emit_stream_items_for_sdk_message(&message);
        let frame = &items[2];

        assert_eq!(frame["type"], STREAM_JSON_RENDER_FRAME_TYPE);
        assert_eq!(
            frame["schemaVersion"],
            STREAM_JSON_RENDER_FRAME_SCHEMA_VERSION
        );
        assert_eq!(
            frame["eventSchemaVersion"],
            STREAM_JSON_RENDER_EVENT_SCHEMA_VERSION
        );
        assert_eq!(
            frame["snapshotSchemaVersion"],
            STREAM_JSON_RENDER_SNAPSHOT_SCHEMA_VERSION
        );
        assert_eq!(frame["draw"]["preferredStrategy"], "patch_regions");
        assert_eq!(frame["draw"]["mode"], "coalesce_then_patch");
        assert_eq!(frame["scroll"]["preserveOnActiveUpdate"], true);
        assert_eq!(frame["scroll"]["stable"], true);
        assert_eq!(frame["regions"][0]["id"], "status");
        assert_eq!(frame["regions"][0]["anchor"], "top");
        assert_eq!(frame["regions"][1]["id"], "active");
        assert_eq!(frame["regions"][1]["anchor"], "bottom");
        assert_eq!(frame["regions"][2]["id"], "footer");
        assert_eq!(frame["regions"][2]["anchor"], "bottom");
        assert!(frame["regions"][1]["lines"]
            .as_array()
            .expect("active lines")
            .iter()
            .any(|line| line.as_str() == Some("hello")));
        assert!(frame["frameHash"].as_str().unwrap_or_default().len() >= 16);

        let regions = frame["regions"].as_array().expect("frame regions");
        for region in regions {
            for line in region["lines"].as_array().expect("region lines") {
                let line = line.as_str().expect("terminal frame line");
                assert!(!line.contains('\n'));
                assert!(!line.contains('\r'));
            }
        }
        assert_eq!(
            emitter.terminal_frame_value()["frameHash"],
            frame["frameHash"]
        );
    }

    #[test]
    fn terminal_frame_renders_approval_as_independent_blocking_region() {
        let approval = RenderEvent::approval_requested("Bash");
        let value = stream_json_render_event_value(
            &approval,
            RenderEventStreamMetadata {
                event_sequence: 1,
                source_message_sequence: 1,
                source_message_type: "test",
                event_index_in_source: 0,
                emitted_at_ms: 1,
            },
        );
        let mut state = StreamJsonRenderStreamState::new();

        assert!(state.apply_render_event_value(&value));
        let snapshot = state.snapshot_value();
        assert_eq!(snapshot["terminal"]["approval"]["blocking"], true);
        assert_eq!(snapshot["terminal"]["approval"]["independentRegion"], true);
        assert_eq!(snapshot["terminal"]["approval"]["toolName"], "Bash");
        assert_eq!(
            snapshot["terminal"]["approval"]["actionModel"]["focusedAction"],
            "approve_once"
        );
        assert_eq!(
            snapshot["terminal"]["approval"]["actionModel"]["enterSelectsFocused"],
            true
        );
        assert_eq!(
            snapshot["terminal"]["approval"]["actionModel"]["pendingIntent"]["available"],
            false
        );
        assert_eq!(
            snapshot["terminal"]["approval"]["actionModel"]["actions"][2]["id"],
            "edit_command"
        );

        let frame = state.terminal_frame_value();
        assert_eq!(frame["status"]["blocking"], true);
        assert_eq!(frame["draw"]["approvalRegionId"], "approval");
        assert_eq!(frame["draw"]["blockingRegionIds"][0], "approval");
        assert_eq!(frame["terminal"]["approval"]["blocking"], true);
        let regions = frame["regions"].as_array().expect("regions");
        let active_region = regions
            .iter()
            .find(|region| region["id"] == "active")
            .expect("active region");
        assert!(active_region["lines"]
            .as_array()
            .expect("active lines")
            .is_empty());
        let approval_region = regions
            .iter()
            .find(|region| region["id"] == "approval")
            .expect("approval region");
        assert_eq!(approval_region["role"], "approval");
        assert_eq!(approval_region["anchor"], "bottom");
        assert_eq!(approval_region["updateMode"], "replace_blocking");
        assert!(approval_region["lines"]
            .as_array()
            .expect("approval lines")
            .iter()
            .any(|line| line.as_str() == Some("tool: Bash")));
        assert!(approval_region["lines"]
            .as_array()
            .expect("approval lines")
            .iter()
            .any(|line| line.as_str()
                == Some("actions: [>Approve once<] [Reject] [Edit command] [Always allow]")));
        assert!(approval_region["lines"]
            .as_array()
            .expect("approval lines")
            .iter()
            .any(|line| line.as_str() == Some("select: Enter or y/n/e/a")));
        assert!(approval_region["lines"]
            .as_array()
            .expect("approval lines")
            .iter()
            .any(|line| line.as_str() == Some("navigate: Tab/Shift-Tab or Left/Right")));

        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let patch = patch_renderer.render_frame_value(&frame);
        let draw_plan = scheduler.render_patch_value(&patch);
        assert!(patch["operations"]
            .as_array()
            .expect("patch operations")
            .iter()
            .any(|op| { op["regionId"] == "approval" && op["updateMode"] == "replace_blocking" }));
        assert_eq!(draw_plan["draw"]["hasBlockingRegion"], true);
        assert_eq!(draw_plan["draw"]["blockingRegionIds"][0], "approval");
    }

    #[test]
    fn terminal_approval_action_focus_cycles_without_resolving_decision() {
        let approval = RenderEvent::approval_requested("Bash");
        let value = stream_json_render_event_value(
            &approval,
            RenderEventStreamMetadata {
                event_sequence: 1,
                source_message_sequence: 1,
                source_message_type: "test",
                event_index_in_source: 0,
                emitted_at_ms: 1,
            },
        );
        let mut state = StreamJsonRenderStreamState::new();
        assert!(state.apply_render_event_value(&value));

        assert!(state.focus_next_approval_action());
        let frame = state.terminal_frame_value();
        assert_eq!(
            frame["terminal"]["approval"]["actionModel"]["focusedAction"],
            "reject"
        );
        assert_eq!(
            frame["terminal"]["approval"]["actionModel"]["actions"][1]["focused"],
            true
        );
        let approval_region = frame["regions"]
            .as_array()
            .expect("regions")
            .iter()
            .find(|region| region["id"] == "approval")
            .expect("approval region");
        assert!(approval_region["lines"]
            .as_array()
            .expect("approval lines")
            .iter()
            .any(|line| line.as_str()
                == Some("actions: [Approve once] [>Reject<] [Edit command] [Always allow]")));
        assert_eq!(frame["terminal"]["approval"]["blocking"], true);
        assert_eq!(frame["refresh"]["needsImmediateRender"], true);

        assert!(state.focus_previous_approval_action());
        let refocused = state.terminal_frame_value();
        assert_eq!(
            refocused["terminal"]["approval"]["actionModel"]["focusedAction"],
            "approve_once"
        );

        state.current_activity = Some(json!({
            "kind": "assistant_message",
            "summary": "approval resolved elsewhere",
        }));
        assert!(!state.focus_next_approval_action());
    }

    #[test]
    fn terminal_approval_action_activation_records_render_intent_without_resolving_decision() {
        let approval = RenderEvent::approval_requested("Bash");
        let value = stream_json_render_event_value(
            &approval,
            RenderEventStreamMetadata {
                event_sequence: 1,
                source_message_sequence: 1,
                source_message_type: "test",
                event_index_in_source: 0,
                emitted_at_ms: 1,
            },
        );
        let mut state = StreamJsonRenderStreamState::new();
        assert!(state.apply_render_event_value(&value));

        assert!(state.focus_next_approval_action());
        assert!(state.activate_focused_approval_action());
        let frame = state.terminal_frame_value();
        assert_eq!(
            frame["terminal"]["approval"]["actionModel"]["pendingIntent"]["action"],
            "reject"
        );
        assert_eq!(
            frame["terminal"]["approval"]["actionModel"]["pendingIntent"]["submitted"],
            false
        );
        assert_eq!(
            frame["terminal"]["approval"]["actionModel"]["pendingIntent"]["renderOnly"],
            true
        );
        assert_eq!(frame["terminal"]["approval"]["blocking"], true);
        assert_eq!(frame["status"]["blocking"], true);
        assert_eq!(frame["refresh"]["needsImmediateRender"], true);
        let approval_region = frame["regions"]
            .as_array()
            .expect("regions")
            .iter()
            .find(|region| region["id"] == "approval")
            .expect("approval region");
        assert!(approval_region["lines"]
            .as_array()
            .expect("approval lines")
            .iter()
            .any(|line| line.as_str() == Some("selected: Reject (decision bridge pending)")));

        assert!(state.activate_approval_action_by_key('a'));
        let shortcut_frame = state.terminal_frame_value();
        assert_eq!(
            shortcut_frame["terminal"]["approval"]["actionModel"]["focusedAction"],
            "approve_for_session"
        );
        assert_eq!(
            shortcut_frame["terminal"]["approval"]["actionModel"]["pendingIntent"]["source"],
            "shortcut"
        );
        assert_eq!(
            shortcut_frame["terminal"]["approval"]["actionModel"]["pendingIntent"]["sequence"],
            2
        );

        state.current_activity = Some(json!({
            "kind": "assistant_message",
            "summary": "approval resolved elsewhere",
        }));
        assert!(!state.activate_focused_approval_action());
        assert!(!state.activate_approval_action_by_key('y'));
    }

    #[test]
    fn terminal_approval_survives_non_activity_events_until_decision() {
        let approval = RenderEvent::approval_requested("Bash");
        let value = stream_json_render_event_value(
            &approval,
            RenderEventStreamMetadata {
                event_sequence: 1,
                source_message_sequence: 1,
                source_message_type: "test",
                event_index_in_source: 0,
                emitted_at_ms: 1,
            },
        );
        let mut state = StreamJsonRenderStreamState::new();
        assert!(state.apply_render_event_value(&value));

        let metadata = json!({
            "type": STREAM_JSON_RENDER_EVENT_TYPE,
            "sequence": 2,
            "kind": "tool_requested",
            "payload": {
                "toolName": "Bash",
                "toolId": "tool-after-approval",
            },
            "emittedAtMs": 2,
        });
        assert!(state.apply_render_event_value(&metadata));
        assert!(state.focus_next_approval_action());

        let frame = state.terminal_frame_value();
        assert_eq!(
            frame["terminal"]["approval"]["actionModel"]["focusedAction"],
            "reject"
        );
        let approval_region = frame["regions"]
            .as_array()
            .expect("regions")
            .iter()
            .find(|region| region["id"] == "approval")
            .expect("approval region");
        assert!(approval_region["lines"]
            .as_array()
            .expect("approval lines")
            .iter()
            .any(|line| line.as_str()
                == Some("actions: [Approve once] [>Reject<] [Edit command] [Always allow]")));
    }

    #[test]
    fn terminal_approval_bridge_status_retires_blocking_region_after_submit() {
        let mut emitter = StreamJsonRenderEventEmitter::new();
        let items = emitter.emit_terminal_permission_request_items(
            "Bash",
            &json!({ "command": "cargo test -p mossen-cli", "description": "run CLI tests" }),
        );
        assert!(items.iter().any(|item| {
            item.get("type").and_then(Value::as_str) == Some(STREAM_JSON_RENDER_EVENT_TYPE)
                && item
                    .get("stream")
                    .and_then(|stream| stream.get("sourceMessageType"))
                    .and_then(Value::as_str)
                    == Some("terminal_permission_request")
        }));

        let _ = emitter.emit_terminal_widget_control_items(
            StreamJsonTerminalWidgetControl::ActivateApprovalActionByKey('y'),
        );
        assert_eq!(
            emitter.pending_terminal_approval_action_id().as_deref(),
            Some("approve_once")
        );
        let bridge_items =
            emitter.emit_terminal_approval_bridge_status_items("submitted", true, false);
        assert!(!bridge_items.is_empty());
        let snapshot = emitter.snapshot_value();
        assert_eq!(snapshot["activity"]["kind"], "approval_submitted");
        assert_eq!(
            snapshot["activity"]["summary"],
            "approval submitted: Approve once"
        );
        assert_eq!(snapshot["terminal"]["approval"]["blocking"], false);
        assert_eq!(
            snapshot["terminal"]["approval"]["actionModel"]["available"],
            false
        );

        let frame = bridge_items
            .iter()
            .find(|item| {
                item.get("type").and_then(Value::as_str) == Some(STREAM_JSON_RENDER_FRAME_TYPE)
            })
            .expect("bridge status frame");
        assert_eq!(frame["status"]["blocking"], false);
        assert_eq!(frame["terminal"]["approval"]["blocking"], false);
        assert!(!frame["regions"]
            .as_array()
            .expect("regions")
            .iter()
            .any(|region| region["id"] == "approval"));
        assert!(frame["changes"]["retiredRegions"]
            .as_array()
            .expect("retired regions")
            .iter()
            .any(|region| region["id"] == "approval"));
    }

    #[test]
    fn terminal_approval_edit_command_status_renders_inline_editor() {
        let mut emitter = StreamJsonRenderEventEmitter::new();
        emitter.emit_terminal_permission_request_items(
            "Bash",
            &json!({ "command": "cargo test -q", "description": "verify changes" }),
        );
        emitter.emit_terminal_widget_control_items(
            StreamJsonTerminalWidgetControl::ActivateApprovalActionByKey('e'),
        );

        let items = emitter.emit_terminal_approval_edit_command_items(
            "editing",
            Some("cargo test -q --lib"),
            true,
        );

        assert!(!items.is_empty());
        let frame = items
            .iter()
            .find(|item| {
                item.get("type").and_then(Value::as_str) == Some(STREAM_JSON_RENDER_FRAME_TYPE)
            })
            .expect("edit command frame");
        assert_eq!(
            frame["terminal"]["approval"]["actionModel"]["pendingIntent"]["editMode"]["active"],
            true
        );
        assert_eq!(
            frame["terminal"]["approval"]["actionModel"]["pendingIntent"]["bridgeStatus"],
            "editing"
        );
        let approval_region = frame["regions"]
            .as_array()
            .expect("regions")
            .iter()
            .find(|region| region["id"] == "approval")
            .expect("approval region");
        assert!(approval_region["lines"]
            .as_array()
            .expect("approval lines")
            .iter()
            .any(|line| line.as_str() == Some("edit command: cargo test -q --lib")));
        assert!(approval_region["lines"]
            .as_array()
            .expect("approval lines")
            .iter()
            .any(|line| line.as_str() == Some("edit: type command, Enter submits, Esc cancels")));
    }

    #[test]
    fn terminal_widget_control_draw_plan_bypasses_superseded_sequence_guard() {
        let mut emitter = StreamJsonRenderEventEmitter::new();
        let approval_items = emitter.emit_terminal_permission_request_draw_plan_items(
            "Bash",
            &json!({ "command": "cargo test -q", "description": "verify changes" }),
        );
        let approval_plan = approval_items.first().expect("approval draw plan");
        let approval_sequence = approval_plan["sequence"]
            .as_u64()
            .expect("approval draw plan sequence");
        assert_eq!(approval_plan["draw"]["forcedRedraw"], false);
        assert_eq!(approval_plan["schedule"]["dropWhenSuperseded"], true);

        let focus_items = emitter.emit_terminal_widget_control_draw_plan_items(
            StreamJsonTerminalWidgetControl::FocusNextApprovalAction,
        );

        assert_eq!(focus_items.len(), 1);
        let focus_plan = &focus_items[0];
        assert_eq!(focus_plan["type"], STREAM_JSON_RENDER_DRAW_PLAN_TYPE);
        assert_eq!(focus_plan["sequence"].as_u64(), Some(approval_sequence));
        assert_eq!(focus_plan["draw"]["forcedRedraw"], true);
        assert_eq!(focus_plan["draw"]["forceRedrawReason"], "widget_control");
        assert_eq!(focus_plan["schedule"]["flushPolicy"], "immediate");
        assert_eq!(focus_plan["schedule"]["dropWhenSuperseded"], false);
        assert_eq!(focus_plan["schedule"]["supersededSequenceBypass"], true);
        assert!(focus_plan["terminalOps"]
            .as_array()
            .expect("terminal ops")
            .iter()
            .any(|operation| operation["regionId"] == "approval"));
    }

    #[test]
    fn terminal_permission_request_preview_shows_bounded_input_context() {
        let mut emitter = StreamJsonRenderEventEmitter::new();
        let items = emitter.emit_terminal_permission_request_items(
            "Bash",
            &json!({
                "command": "cargo test -p mossen-cli terminal_render_frontend_event_tests",
                "cwd": "/repo",
                "description": "verify terminal approval bridge",
                "timeout_ms": 120000,
            }),
        );
        assert!(!items.is_empty());

        let snapshot = emitter.snapshot_value();
        assert_eq!(
            snapshot["terminal"]["approval"]["inputPreview"]["available"],
            true
        );
        assert_eq!(
            snapshot["terminal"]["approval"]["inputPreview"]["maxLines"],
            STREAM_JSON_RENDER_APPROVAL_PREVIEW_MAX_LINES
        );
        assert!(snapshot["terminal"]["approval"]["inputPreview"]["lines"]
            .as_array()
            .expect("preview lines")
            .iter()
            .any(|line| line.as_str()
                == Some("command: cargo test -p mossen-cli terminal_render_frontend_event_tests")));

        let frame = emitter.terminal_frame_value();
        let approval_region = frame["regions"]
            .as_array()
            .expect("regions")
            .iter()
            .find(|region| region["id"] == "approval")
            .expect("approval region");
        let lines = approval_region["lines"].as_array().expect("approval lines");
        assert!(lines
            .iter()
            .any(|line| line.as_str() == Some("description: verify terminal approval bridge")));
        assert!(lines.len() <= 12);
    }

    #[test]
    fn terminal_frame_renders_plan_as_independent_bounded_region() {
        let plan = RenderEvent::new(
            RenderEventKind::PlanUpdated {
                tool_id: Some("plan-1".to_string()),
                step_count: 4,
                completed_count: 1,
                active_count: 1,
                pending_count: 2,
                blocked_count: 0,
                active_step: Some("Implement independent terminal plan region".to_string()),
            },
            RenderEventScope::Main,
            UiStage::Planning,
        );
        let command = RenderEvent::new(
            RenderEventKind::CommandStarted {
                tool_id: Some("cmd-1".to_string()),
                command: Some("cargo test -p mossen-cli".to_string()),
                cwd: Some("/repo".to_string()),
            },
            RenderEventScope::Main,
            UiStage::RunningCommand,
        );
        let mut state = StreamJsonRenderStreamState::new();
        for (index, event) in [plan, command].iter().enumerate() {
            let sequence = u64::try_from(index + 1).expect("sequence");
            let value = stream_json_render_event_value(
                event,
                RenderEventStreamMetadata {
                    event_sequence: sequence,
                    source_message_sequence: sequence,
                    source_message_type: "test",
                    event_index_in_source: 0,
                    emitted_at_ms: sequence,
                },
            );
            assert!(state.apply_render_event_value(&value));
        }

        let snapshot = state.snapshot_value();
        assert_eq!(snapshot["terminal"]["plan"]["available"], true);
        assert_eq!(snapshot["terminal"]["plan"]["independentRegion"], true);
        assert_eq!(
            snapshot["terminal"]["plan"]["widget"]["activeStep"],
            "Implement independent terminal plan region"
        );

        let frame = state.terminal_frame_value();
        assert_eq!(frame["draw"]["planRegionId"], "plan");
        assert_eq!(frame["terminal"]["plan"]["available"], true);
        let regions = frame["regions"].as_array().expect("regions");
        let plan_index = regions
            .iter()
            .position(|region| region["id"] == "plan")
            .expect("plan region index");
        let command_index = regions
            .iter()
            .position(|region| region["id"] == "command")
            .expect("command region index");
        assert!(plan_index < command_index);

        let plan_region = &regions[plan_index];
        assert_eq!(plan_region["role"], "plan");
        assert_eq!(plan_region["anchor"], "top");
        assert_eq!(plan_region["updateMode"], "replace_plan");
        let plan_lines = plan_region["lines"].as_array().expect("plan lines");
        assert!(plan_lines.iter().any(|line| line.as_str() == Some("plan")));
        assert!(plan_lines.iter().any(|line| {
            line.as_str() == Some("active: Implement independent terminal plan region")
        }));
        assert!(plan_lines.iter().any(|line| {
            line.as_str() == Some("progress: done 1 | active 1 | pending 2 | blocked 0")
        }));
        assert!(plan_lines.len() <= 5);

        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let draw_plan = scheduler.render_patch_value(&patch_renderer.render_frame_value(&frame));
        let terminal_ops = draw_plan["terminalOps"].as_array().expect("terminal ops");
        let plan_start_row = terminal_ops
            .iter()
            .find(|op| op["op"] == "move_to_row" && op["regionId"] == "plan")
            .and_then(|op| op["row"].as_str())
            .expect("plan top row");
        let command_start_row = terminal_ops
            .iter()
            .find(|op| op["op"] == "move_to_row" && op["regionId"] == "command")
            .and_then(|op| op["row"].as_str())
            .expect("command top row");
        assert!(plan_start_row < command_start_row);
    }

    #[test]
    fn terminal_status_bar_reports_model_mode_reasoning_and_context() {
        let _guard = terminal_status_env_lock();
        let previous = std::env::var(STREAM_JSON_TERMINAL_PERMISSION_MODE_ENV).ok();
        std::env::set_var(STREAM_JSON_TERMINAL_PERMISSION_MODE_ENV, "plan");

        let mut emitter = StreamJsonRenderEventEmitter::new();
        emitter.emit_for_sdk_message(&SdkMessage::SystemInit {
            session_id: "sess-status".to_string(),
            model: "gpt-5.3-codex-ultra-long-status-model".to_string(),
            tools: vec!["Bash".to_string()],
            task_id: None,
        });
        emitter.emit_for_sdk_message(&SdkMessage::StreamEvent {
            event: StreamEventData::ContentBlockDelta {
                index: 0,
                delta: ContentDelta::ThinkingDelta {
                    thinking: "checking the repository".to_string(),
                },
            },
            task_id: None,
        });
        emitter.emit_for_sdk_message(&SdkMessage::StreamEvent {
            event: StreamEventData::MessageDelta {
                usage: Some(ApiUsage {
                    input_tokens: 1_200,
                    output_tokens: 345,
                    cache_read_input_tokens: Some(1_000),
                    cache_creation_input_tokens: Some(55),
                }),
                stop_reason: None,
            },
            task_id: None,
        });

        let snapshot = emitter.snapshot_value();
        let status_bar = &snapshot["terminal"]["statusBar"];
        assert_eq!(status_bar["mode"]["value"], "plan");
        assert_eq!(status_bar["mode"]["label"], "Plan");
        assert_eq!(status_bar["context"]["tokens"], 2_600);
        assert_eq!(status_bar["context"]["windowTokens"], 200_000);
        assert_eq!(status_bar["context"]["label"], "2.6k/200k");
        assert_eq!(status_bar["model"], "gpt-5.3-codex-ultra-long-...");
        assert!(
            status_bar["reasoning"]["thinkingBytes"]
                .as_u64()
                .expect("thinking bytes")
                > 0
        );

        let frame = emitter.terminal_frame_value();
        assert_eq!(frame["status"]["bar"]["mode"]["value"], "plan");
        let status_line = frame["status"]["line"].as_str().expect("status line");
        assert!(status_line.contains("mode:Plan") || status_line.contains("mode:plan"));
        assert!(status_line.contains("ctx:2.6k/200k"));
        assert!(status_line.chars().count() <= STREAM_JSON_TERMINAL_STATUS_LINE_FULL_MAX_CHARS);

        match previous {
            Some(value) => std::env::set_var(STREAM_JSON_TERMINAL_PERMISSION_MODE_ENV, value),
            None => std::env::remove_var(STREAM_JSON_TERMINAL_PERMISSION_MODE_ENV),
        }
    }

    #[test]
    fn terminal_status_heartbeat_uses_seeded_model_before_sdk_metadata() {
        let mut emitter = StreamJsonRenderEventEmitter::new();

        assert!(emitter.seed_terminal_session_model("terminal-seeded-model"));
        let items = emitter.emit_terminal_status_heartbeat_draw_plan_items_at(1_000);
        let snapshot = emitter.snapshot_value();
        let frame = emitter.terminal_frame_value();
        let status_line = frame["status"]["line"].as_str().expect("status line");

        assert_eq!(items.len(), 1);
        assert_eq!(
            snapshot["terminal"]["statusBar"]["model"],
            "terminal-seeded-model"
        );
        assert!(status_line.contains("terminal-seeded-model"));
        assert!(!status_line.contains("unknown | mode:"));
    }

    #[test]
    fn terminal_status_heartbeat_uses_replace_active_update_mode() {
        let mut emitter = StreamJsonRenderEventEmitter::new();

        let _ = emitter.emit_terminal_status_heartbeat_draw_plan_items_at(1_000);
        let frame = emitter.terminal_frame_value();
        let active_region = frame["regions"]
            .as_array()
            .expect("regions")
            .iter()
            .find(|region| region["id"] == "active")
            .expect("active region");

        assert_eq!(active_region["updateMode"], "replace_active");
        assert_eq!(frame["scroll"]["historyPolicy"], "update_active");
    }

    #[test]
    fn terminal_metadata_after_seeded_heartbeat_skips_redundant_waiting_redraw() {
        let mut emitter = StreamJsonRenderEventEmitter::new();
        let model = "terminal-metadata-stable-model";

        assert!(emitter.seed_terminal_session_model(model));
        let _ = emitter.emit_terminal_status_heartbeat_draw_plan_items_at(unix_timestamp_millis());
        let items =
            emitter.emit_terminal_draw_plan_items_for_sdk_message(&SdkMessage::SystemInit {
                session_id: "sess-metadata-stable".to_string(),
                model: model.to_string(),
                tools: vec![],
                task_id: None,
            });
        let draw_plan = items.first().expect("metadata draw plan");

        assert_eq!(draw_plan["draw"]["skipped"], true);
        assert_eq!(draw_plan["draw"]["skipReason"], "frame_hash_unchanged");
        assert_eq!(draw_plan["draw"]["operationCount"], 0);
    }

    #[test]
    fn terminal_assistant_activity_prioritizes_visible_text_preview() {
        let activity = json!({
            "kind": "assistant_message",
            "summary": "assistant text: 29 bytes",
            "previewLines": ["visible first line"],
            "bytes": 29,
        });

        let lines = terminal_activity_lines(Some(&activity));

        assert_eq!(lines[0], "visible first line");
        assert_eq!(
            lines.len(),
            STREAM_JSON_RENDER_ASSISTANT_ACTIVITY_VISIBLE_LINES
        );
        assert!(!lines.iter().any(|line| line == "assistant text: 29 bytes"));
        assert!(!lines.iter().any(|line| line == "bytes: 29"));
    }

    #[test]
    fn terminal_assistant_activity_uses_stable_visible_line_budget() {
        let one_line = json!({
            "kind": "assistant_message",
            "summary": "assistant text: 29 bytes",
            "previewLines": ["line 1"],
        });
        let four_lines = json!({
            "kind": "assistant_message",
            "summary": "assistant text: 120 bytes",
            "previewLines": ["line 1", "line 2", "line 3", "line 4"],
        });

        let one = terminal_activity_lines(Some(&one_line));
        let four = terminal_activity_lines(Some(&four_lines));

        assert_eq!(
            one.len(),
            STREAM_JSON_RENDER_ASSISTANT_ACTIVITY_VISIBLE_LINES
        );
        assert_eq!(
            four.len(),
            STREAM_JSON_RENDER_ASSISTANT_ACTIVITY_VISIBLE_LINES
        );
        assert_eq!(one[0], "line 1");
        assert_eq!(one[1], "");
        assert_eq!(one[2], "");
        assert_eq!(one[3], "");
    }

    #[test]
    fn terminal_duplicate_final_assistant_keeps_existing_text_preview() {
        let mut emitter = StreamJsonRenderEventEmitter::new();
        for text in ["dup head\n", "dup tail\n"] {
            let message = SdkMessage::StreamEvent {
                event: StreamEventData::ContentBlockDelta {
                    index: 0,
                    delta: ContentDelta::TextDelta {
                        text: text.to_string(),
                    },
                },
                task_id: None,
            };
            let _ = emitter.emit_terminal_draw_plan_items_for_sdk_message(&message);
        }

        let assistant = SdkMessage::Assistant {
            message: AssistantMessage {
                role: Role::Assistant,
                content: vec![ContentBlock::Text(TextBlock {
                    text: "dup head\ndup tail\n".to_string(),
                })],
                uuid: None,
                model: None,
                stop_reason: Some("stop".to_string()),
                extra: Default::default(),
            },
            usage: None,
            task_id: None,
        };
        let items = emitter.emit_terminal_draw_plan_items_for_sdk_message(&assistant);
        let frame = emitter.terminal_frame_value();
        let active_region = frame["regions"]
            .as_array()
            .expect("regions")
            .iter()
            .find(|region| region["id"] == "active")
            .expect("active region");
        let active_lines = active_region["lines"].as_array().expect("active lines");

        assert!(active_lines
            .iter()
            .any(|line| line.as_str() == Some("dup tail")));
        assert!(!active_lines.iter().any(|line| {
            line.as_str().is_some_and(|line| {
                line.starts_with("assistant text:") || line.starts_with("bytes:")
            })
        }));
        assert_eq!(items[0]["draw"]["skipped"], true);
        assert_eq!(items[0]["draw"]["skipReason"], "frame_hash_unchanged");
    }

    #[test]
    fn terminal_sdk_metadata_overrides_seeded_model() {
        let mut emitter = StreamJsonRenderEventEmitter::new();

        assert!(emitter.seed_terminal_session_model("terminal-seeded-model"));
        emitter.emit_for_sdk_message(&SdkMessage::SystemInit {
            session_id: "sess-sdk-model".to_string(),
            model: "terminal-sdk-authoritative-model".to_string(),
            tools: vec![],
            task_id: None,
        });

        let snapshot = emitter.snapshot_value();
        assert_eq!(
            snapshot["terminal"]["statusBar"]["model"],
            "terminal-sdk-authoritativ..."
        );
    }

    #[test]
    fn terminal_status_heartbeat_advances_elapsed_without_sdk_messages() {
        let mut emitter = StreamJsonRenderEventEmitter::new();

        let first_items = emitter.emit_terminal_status_heartbeat_draw_plan_items_at(1_000);
        let first_snapshot = emitter.snapshot_value();
        assert_eq!(first_items.len(), 1);
        assert_eq!(first_items[0]["type"], STREAM_JSON_RENDER_DRAW_PLAN_TYPE);
        assert_eq!(first_snapshot["stage"], "thinking");
        assert_eq!(
            first_snapshot["activity"]["summary"],
            "waiting for model stream"
        );
        assert_eq!(first_snapshot["terminal"]["statusBar"]["elapsed"], "0s");

        let second_items = emitter.emit_terminal_status_heartbeat_draw_plan_items_at(2_100);
        let second_snapshot = emitter.snapshot_value();
        assert_eq!(second_items.len(), 1);
        assert_eq!(second_snapshot["terminal"]["statusBar"]["elapsed"], "1s");
        let status_line = emitter.terminal_frame_value()["status"]["line"]
            .as_str()
            .expect("status line")
            .to_string();
        assert!(status_line.contains("Thinking 1s"));
    }

    #[test]
    fn terminal_status_heartbeat_stops_after_terminal_finish() {
        let mut emitter = StreamJsonRenderEventEmitter::new();
        assert!(!emitter
            .emit_terminal_status_heartbeat_draw_plan_items_at(1_000)
            .is_empty());
        let result = SdkMessage::Result {
            terminal: "ok".to_string(),
            cost_usd: None,
            duration_ms: Some(12),
            usage: None,
            task_id: None,
        };
        let _ = emitter.emit_terminal_draw_plan_items_for_sdk_message(&result);

        let items = emitter.emit_terminal_status_heartbeat_draw_plan_items_at(3_000);

        assert!(items.is_empty());
        assert_eq!(emitter.snapshot_value()["terminal"]["finished"], true);
    }

    #[test]
    fn terminal_status_heartbeat_survives_metadata_only_stream_started_event() {
        let mut emitter = StreamJsonRenderEventEmitter::new();
        let _ = emitter.emit_terminal_status_heartbeat_draw_plan_items_at(1_000);

        let _ = emitter.emit_terminal_draw_plan_items_for_sdk_message(&SdkMessage::SystemInit {
            session_id: "sess-heartbeat-metadata".to_string(),
            model: "gpt-terminal-heartbeat-metadata".to_string(),
            tools: vec![],
            task_id: None,
        });

        let frame = emitter.terminal_frame_value();
        let active_region = frame["regions"]
            .as_array()
            .expect("regions")
            .iter()
            .find(|region| region["id"] == "active")
            .expect("active region");
        let active_lines = active_region["lines"].as_array().expect("active lines");
        assert!(active_lines
            .iter()
            .any(|line| line.as_str() == Some("waiting for model stream")));
        assert!(!active_lines
            .iter()
            .any(|line| line.as_str() == Some("No active render activity")));
    }

    #[test]
    fn terminal_frame_exposes_status_and_footer_viewport_line_variants() {
        let mut emitter = StreamJsonRenderEventEmitter::new();
        emitter.emit_for_sdk_message(&SdkMessage::SystemInit {
            session_id: "sess-variants".to_string(),
            model: "gpt-5-codex-super-long-render-model".to_string(),
            tools: vec!["Bash".to_string()],
            task_id: None,
        });

        let frame = emitter.terminal_frame_value();
        let regions = frame["regions"].as_array().expect("regions");
        let status_region = regions
            .iter()
            .find(|region| region["id"] == "status")
            .expect("status region");
        let footer_region = regions
            .iter()
            .find(|region| region["id"] == "footer")
            .expect("footer region");

        assert_eq!(status_region["viewportSelectableLines"], true);
        assert_eq!(
            status_region["lineVariantPolicy"],
            "choose_shortest_fitting_variant"
        );
        assert!(status_region["lineVariants"]["full"]
            .as_str()
            .expect("full status variant")
            .contains("model:gpt-5-codex"));
        assert!(status_region["lineVariants"]["minimal"]
            .as_str()
            .expect("minimal status variant")
            .contains("ctx:?/200k"));

        assert_eq!(footer_region["viewportSelectableLines"], true);
        assert!(footer_region["lineVariants"]["full"]
            .as_str()
            .expect("full footer variant")
            .contains("Ctrl+L live"));
        assert!(!footer_region["lineVariants"]["minimal"]
            .as_str()
            .expect("minimal footer variant")
            .contains("keys:"));
    }

    #[test]
    fn terminal_frame_renders_command_as_summary_widget_without_log_wall() {
        let events = [
            RenderEvent::new(
                RenderEventKind::CommandStarted {
                    tool_id: Some("cmd-1".to_string()),
                    command: Some("cargo test".to_string()),
                    cwd: Some("/repo".to_string()),
                },
                RenderEventScope::Main,
                UiStage::RunningCommand,
            ),
            RenderEvent::new(
                RenderEventKind::CommandOutput {
                    tool_id: Some("cmd-1".to_string()),
                    stream: "stdout".to_string(),
                    bytes: 4096,
                    preview_lines: 2,
                    hidden_lines: 9,
                    total_lines: Some(11),
                    full_log_available: true,
                },
                RenderEventScope::Main,
                UiStage::RunningCommand,
            ),
            RenderEvent::new(
                RenderEventKind::CommandFinished {
                    tool_id: Some("cmd-1".to_string()),
                    exit_code: Some(0),
                    duration_ms: Some(1234),
                },
                RenderEventScope::Main,
                UiStage::ReviewingResult,
            ),
        ];
        let mut state = StreamJsonRenderStreamState::new();
        for (index, event) in events.iter().enumerate() {
            let sequence = u64::try_from(index + 1).expect("sequence");
            let value = stream_json_render_event_value(
                event,
                RenderEventStreamMetadata {
                    event_sequence: sequence,
                    source_message_sequence: sequence,
                    source_message_type: "test",
                    event_index_in_source: 0,
                    emitted_at_ms: sequence,
                },
            );
            assert!(state.apply_render_event_value(&value));
        }

        let snapshot = state.snapshot_value();
        assert_eq!(snapshot["terminal"]["command"]["available"], true);
        assert_eq!(snapshot["terminal"]["command"]["summaryOnly"], true);
        assert_eq!(
            snapshot["terminal"]["command"]["widget"]["command"],
            "cargo test"
        );

        let frame = state.terminal_frame_value();
        assert_eq!(frame["draw"]["commandRegionId"], "command");
        let command_region = frame["regions"]
            .as_array()
            .expect("regions")
            .iter()
            .find(|region| region["id"] == "command")
            .expect("command region");
        assert_eq!(command_region["role"], "command");
        assert_eq!(command_region["anchor"], "top");
        assert_eq!(command_region["updateMode"], "replace_summary");
        let command_lines = command_region["lines"].as_array().expect("command lines");
        assert!(command_lines
            .iter()
            .any(|line| line.as_str() == Some("cmd: cargo test")));
        assert!(command_lines
            .iter()
            .any(|line| line.as_str() == Some("full log: available")));
        assert!(command_lines
            .iter()
            .all(|line| !line.as_str().unwrap_or_default().contains("raw stdout")));

        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let draw_plan = scheduler.render_patch_value(&patch_renderer.render_frame_value(&frame));
        assert!(draw_plan["terminalOps"]
            .as_array()
            .expect("terminal ops")
            .iter()
            .any(|op| op["op"] == "move_to_row"
                && op["regionId"] == "command"
                && op["row"] == "top+1"));
    }

    #[test]
    fn terminal_frame_includes_command_preview_without_log_wall() {
        let started = RenderEvent::new(
            RenderEventKind::CommandStarted {
                tool_id: Some("cmd-1".to_string()),
                command: Some("cargo test".to_string()),
                cwd: Some("/repo".to_string()),
            },
            RenderEventScope::Main,
            UiStage::RunningCommand,
        );
        let output = RenderEvent::new(
            RenderEventKind::CommandOutput {
                tool_id: Some("cmd-1".to_string()),
                stream: "stdout".to_string(),
                bytes: 4096,
                preview_lines: 3,
                hidden_lines: 17,
                total_lines: Some(20),
                full_log_available: true,
            },
            RenderEventScope::Main,
            UiStage::RunningCommand,
        );
        let finished = RenderEvent::new(
            RenderEventKind::CommandFinished {
                tool_id: Some("cmd-1".to_string()),
                exit_code: Some(0),
                duration_ms: Some(1234),
            },
            RenderEventScope::Main,
            UiStage::ReviewingResult,
        );
        let mut output_value = stream_json_render_event_value(
            &output,
            RenderEventStreamMetadata {
                event_sequence: 2,
                source_message_sequence: 2,
                source_message_type: "test",
                event_index_in_source: 0,
                emitted_at_ms: 2,
            },
        );
        output_value
            .get_mut("payload")
            .and_then(Value::as_object_mut)
            .expect("payload")
            .insert(
                "previewLineItems".to_string(),
                json!([
                    "Compiling mossen v0.1.0",
                    "test render::command_preview ... ok",
                    "raw stdout tail"
                ]),
            );

        let values = [
            stream_json_render_event_value(
                &started,
                RenderEventStreamMetadata {
                    event_sequence: 1,
                    source_message_sequence: 1,
                    source_message_type: "test",
                    event_index_in_source: 0,
                    emitted_at_ms: 1,
                },
            ),
            output_value,
            stream_json_render_event_value(
                &finished,
                RenderEventStreamMetadata {
                    event_sequence: 3,
                    source_message_sequence: 3,
                    source_message_type: "test",
                    event_index_in_source: 0,
                    emitted_at_ms: 3,
                },
            ),
        ];
        let mut state = StreamJsonRenderStreamState::new();
        for value in values {
            assert!(state.apply_render_event_value(&value));
        }

        let snapshot = state.snapshot_value();
        assert_eq!(
            snapshot["terminal"]["command"]["widget"]["previewLineItems"][0],
            "Compiling mossen v0.1.0"
        );
        assert_eq!(
            snapshot["terminal"]["command"]["widget"]["previewLineLimit"],
            3
        );

        let frame = state.terminal_frame_value();
        let command_region = frame["regions"]
            .as_array()
            .expect("regions")
            .iter()
            .find(|region| region["id"] == "command")
            .expect("command region");
        let command_lines = command_region["lines"].as_array().expect("command lines");
        assert!(command_lines
            .iter()
            .any(|line| line.as_str() == Some("output: 3 shown, 17 hidden")));
        assert!(command_lines
            .iter()
            .any(|line| line.as_str() == Some("stdout: Compiling mossen v0.1.0")));
        assert_eq!(command_lines.len(), 10);
        assert!(command_lines
            .iter()
            .all(|line| !line.as_str().unwrap_or_default().contains("unbounded log")));
    }

    #[test]
    fn terminal_background_bash_start_summary_keeps_task_id_and_preview() {
        let message = SdkMessage::ToolUseSummary {
            tool_name: "Bash".to_string(),
            tool_use_id: Some("toolu-bg".to_string()),
            summary: serde_json::json!({
                "stdout": "Command started in background task: shell-task-1",
                "backgroundTaskId": "shell-task-1",
                "timed_out": false,
                "interrupted": false
            })
            .to_string(),
            full_content: None,
            task_id: None,
        };
        let mut emitter = StreamJsonRenderEventEmitter::new();
        let values = emitter.emit_stream_items_for_sdk_message(&message);

        assert!(values.iter().any(|value| {
            value["type"] == STREAM_JSON_RENDER_EVENT_TYPE
                && value["kind"] == "command_output"
                && value["payload"]["backgroundTaskId"] == "shell-task-1"
                && value["payload"]["previewLineItems"][0]
                    == "Command started in background task: shell-task-1"
        }));

        let snapshot = emitter.snapshot_value();
        assert_eq!(
            snapshot["terminal"]["command"]["widget"]["backgroundTaskId"],
            "shell-task-1"
        );
        assert_eq!(
            snapshot["terminal"]["command"]["widget"]["summary"],
            "background task started: shell-task-1"
        );

        let frame = emitter.terminal_frame_value();
        let command_lines = frame["regions"]
            .as_array()
            .expect("regions")
            .iter()
            .find(|region| region["id"] == "command")
            .expect("command region")["lines"]
            .as_array()
            .expect("command lines");
        assert!(command_lines
            .iter()
            .any(|line| line.as_str() == Some("task: shell-task-1")));
        assert!(command_lines.iter().any(|line| {
            line.as_str() == Some("stdout: Command started in background task: shell-task-1")
        }));
        assert!(command_lines.len() <= 10);
    }

    #[test]
    fn terminal_task_output_background_shell_renders_bounded_task_summary() {
        let long_output = (1..=24)
            .map(|idx| format!("line-{idx}"))
            .collect::<Vec<_>>()
            .join("\n");
        let message = SdkMessage::ToolUseSummary {
            tool_name: "TaskOutput".to_string(),
            tool_use_id: Some("toolu-output".to_string()),
            summary: serde_json::json!({
                "retrieval_status": "ready",
                "task": {
                    "task_id": "shell-task-2",
                    "task_type": "background_shell",
                    "status": "completed",
                    "description": "printf long output",
                    "output": long_output,
                    "exit_code": 0
                }
            })
            .to_string(),
            full_content: None,
            task_id: None,
        };
        let mut emitter = StreamJsonRenderEventEmitter::new();
        let values = emitter.emit_stream_items_for_sdk_message(&message);

        assert!(values.iter().any(|value| {
            value["type"] == STREAM_JSON_RENDER_EVENT_TYPE
                && value["kind"] == "command_output"
                && value["scope"]["kind"] == "task"
                && value["scope"]["taskId"] == "shell-task-2"
                && value["payload"]["taskStatus"] == "completed"
                && value["payload"]["previewLineItems"][0] == "line-1"
        }));

        let snapshot = emitter.snapshot_value();
        assert_eq!(
            snapshot["terminal"]["command"]["widget"]["taskId"],
            "shell-task-2"
        );
        assert_eq!(
            snapshot["terminal"]["command"]["widget"]["summary"],
            "background task completed: shell-task-2"
        );
        assert_eq!(snapshot["terminal"]["command"]["widget"]["hiddenLines"], 21);

        let frame = emitter.terminal_frame_value();
        let command_lines = frame["regions"]
            .as_array()
            .expect("regions")
            .iter()
            .find(|region| region["id"] == "command")
            .expect("command region")["lines"]
            .as_array()
            .expect("command lines");
        assert!(command_lines
            .iter()
            .any(|line| line.as_str() == Some("task: shell-task-2")));
        assert!(command_lines
            .iter()
            .any(|line| line.as_str() == Some("status: completed")));
        assert!(command_lines.len() <= 10);
        assert!(!command_lines
            .iter()
            .any(|line| line.as_str().unwrap_or_default().contains("line-24")));
    }

    #[test]
    fn terminal_background_task_panel_persists_after_foreground_command_changes() {
        let mut emitter = StreamJsonRenderEventEmitter::new();
        let background = SdkMessage::ToolUseSummary {
            tool_name: "Bash".to_string(),
            tool_use_id: Some("toolu-bg".to_string()),
            summary: serde_json::json!({
                "stdout": "Command started in background task: shell-task-1",
                "backgroundTaskId": "shell-task-1",
                "timed_out": false
            })
            .to_string(),
            full_content: None,
            task_id: None,
        };
        emitter.emit_stream_items_for_sdk_message(&background);

        let foreground = SdkMessage::Assistant {
            message: AssistantMessage {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolUse(ToolUseBlock {
                    id: "toolu-fg".to_string(),
                    name: "Bash".to_string(),
                    input: json!({"command": "echo foreground"}),
                })],
                uuid: None,
                model: None,
                stop_reason: Some("tool_use".to_string()),
                extra: Default::default(),
            },
            usage: None,
            task_id: None,
        };
        emitter.emit_stream_items_for_sdk_message(&foreground);

        let frame = emitter.terminal_frame_value();
        assert_eq!(frame["draw"]["commandRegionId"], "command");
        assert_eq!(frame["draw"]["backgroundTaskRegionId"], "background_tasks");
        assert_eq!(
            frame["terminal"]["command"]["widget"]["command"],
            "echo foreground"
        );
        assert_eq!(frame["terminal"]["backgroundTasks"]["count"], 1);
        assert_eq!(
            frame["terminal"]["backgroundTasks"]["items"][0]["taskId"],
            "shell-task-1"
        );

        let background_region = frame["regions"]
            .as_array()
            .expect("regions")
            .iter()
            .find(|region| region["id"] == "background_tasks")
            .expect("background task region");
        let lines = background_region["lines"].as_array().expect("lines");
        assert!(lines
            .iter()
            .any(|line| line.as_str() == Some("background tasks")));
        assert!(lines.iter().any(|line| {
            line.as_str()
                .is_some_and(|line| line.contains("started: shell-task-1"))
        }));
        assert!(lines.len() <= STREAM_JSON_RENDER_BACKGROUND_TASK_MAX_ITEMS + 1);
    }

    #[test]
    fn terminal_background_task_panel_updates_completed_task_without_log_wall() {
        let long_output = (1..=24)
            .map(|idx| format!("line-{idx}"))
            .collect::<Vec<_>>()
            .join("\n");
        let message = SdkMessage::ToolUseSummary {
            tool_name: "TaskOutput".to_string(),
            tool_use_id: Some("toolu-task-output".to_string()),
            summary: serde_json::json!({
                "retrieval_status": "ready",
                "task": {
                    "task_id": "shell-task-2",
                    "task_type": "background_shell",
                    "status": "completed",
                    "description": "printf long output",
                    "output": long_output,
                    "exit_code": 0
                }
            })
            .to_string(),
            full_content: None,
            task_id: None,
        };

        let mut emitter = StreamJsonRenderEventEmitter::new();
        emitter.emit_stream_items_for_sdk_message(&message);

        let frame = emitter.terminal_frame_value();
        assert_eq!(frame["draw"]["backgroundTaskRegionId"], "background_tasks");
        assert_eq!(
            frame["terminal"]["backgroundTasks"]["items"][0]["taskStatus"],
            "completed"
        );
        let background_lines = frame["regions"]
            .as_array()
            .expect("regions")
            .iter()
            .find(|region| region["id"] == "background_tasks")
            .expect("background task region")["lines"]
            .as_array()
            .expect("lines");
        assert!(background_lines.iter().any(|line| {
            line.as_str()
                .is_some_and(|line| line.contains("completed: shell-task-2"))
        }));
        assert!(background_lines.iter().any(|line| {
            line.as_str()
                .is_some_and(|line| line.contains("output: 24 lines"))
        }));
        assert!(!background_lines
            .iter()
            .any(|line| line.as_str().unwrap_or_default().contains("line-24")));
        assert!(background_lines.len() <= STREAM_JSON_RENDER_BACKGROUND_TASK_MAX_ITEMS + 1);
    }

    #[test]
    fn terminal_background_task_panel_toggle_expands_bounded_task_list() {
        let mut state = StreamJsonRenderStreamState::new();
        for index in 1..=7_usize {
            let event = RenderEvent::new(
                RenderEventKind::BackgroundTaskUpdated {
                    tool_id: Some(format!("toolu-bg-{index}")),
                    task_id: format!("shell-task-{index}"),
                    task_type: "background_shell".to_string(),
                    status: if index % 2 == 0 {
                        "completed".to_string()
                    } else {
                        "running".to_string()
                    },
                    command: Some(format!("cargo test --package pkg-{index}")),
                    preview_lines: index,
                    hidden_lines: 10_usize.saturating_sub(index),
                    exit_code: (index % 2 == 0).then_some(0),
                },
                RenderEventScope::Main,
                UiStage::RunningCommand,
            );
            let value = stream_json_render_event_value(
                &event,
                RenderEventStreamMetadata {
                    event_sequence: u64::try_from(index).expect("sequence"),
                    source_message_sequence: u64::try_from(index).expect("sequence"),
                    source_message_type: "test",
                    event_index_in_source: 0,
                    emitted_at_ms: u64::try_from(index).expect("timestamp"),
                },
            );
            assert!(state.apply_render_event_value(&value));
        }

        let collapsed_frame = state.terminal_frame_value();
        assert_eq!(
            collapsed_frame["terminal"]["backgroundTasks"]["summaryOnly"],
            true
        );
        assert_eq!(
            collapsed_frame["terminal"]["backgroundTasks"]["expanded"],
            false
        );
        assert_eq!(
            collapsed_frame["terminal"]["interaction"]["backgroundTaskToggleKey"],
            "b"
        );
        let collapsed_lines = collapsed_frame["regions"]
            .as_array()
            .expect("regions")
            .iter()
            .find(|region| region["id"] == "background_tasks")
            .expect("background task region")["lines"]
            .as_array()
            .expect("background task lines");
        assert!(collapsed_lines
            .iter()
            .any(|line| line.as_str() == Some("background tasks")));
        assert!(collapsed_lines
            .iter()
            .all(|line| !line.as_str().unwrap_or_default().contains("shell-task-1")));
        assert!(collapsed_lines.len() <= STREAM_JSON_RENDER_BACKGROUND_TASK_MAX_ITEMS + 1);

        assert!(state.toggle_background_task_panel_expanded());
        let expanded_frame = state.terminal_frame_value();
        assert_eq!(
            expanded_frame["terminal"]["backgroundTasks"]["expanded"],
            true
        );
        assert_eq!(
            expanded_frame["terminal"]["interaction"]["backgroundTaskExpanded"],
            true
        );
        let background_region = expanded_frame["regions"]
            .as_array()
            .expect("regions")
            .iter()
            .find(|region| region["id"] == "background_tasks")
            .expect("background task region");
        assert_eq!(background_region["updateMode"], "replace_expanded_summary");
        let expanded_lines = background_region["lines"]
            .as_array()
            .expect("background task lines");
        assert!(expanded_lines
            .iter()
            .any(|line| line.as_str() == Some("background task details")));
        assert!(expanded_lines
            .iter()
            .any(|line| line.as_str().unwrap_or_default().contains("shell-task-1")));
        assert!(expanded_lines
            .iter()
            .any(|line| line.as_str().unwrap_or_default().contains("pkg-1")));
        assert!(expanded_lines.len() <= 32);
    }

    #[test]
    fn terminal_command_widget_toggle_expands_bounded_preview_lines() {
        let events = [
            RenderEvent::new(
                RenderEventKind::CommandStarted {
                    tool_id: Some("cmd-1".to_string()),
                    command: Some("cargo test".to_string()),
                    cwd: Some("/repo".to_string()),
                },
                RenderEventScope::Main,
                UiStage::RunningCommand,
            ),
            RenderEvent::new(
                RenderEventKind::CommandOutput {
                    tool_id: Some("cmd-1".to_string()),
                    stream: "stdout".to_string(),
                    bytes: 4096,
                    preview_lines: 6,
                    hidden_lines: 20,
                    total_lines: Some(26),
                    full_log_available: true,
                },
                RenderEventScope::Main,
                UiStage::RunningCommand,
            ),
        ];
        let mut output_value = stream_json_render_event_value(
            &events[1],
            RenderEventStreamMetadata {
                event_sequence: 2,
                source_message_sequence: 2,
                source_message_type: "test",
                event_index_in_source: 0,
                emitted_at_ms: 2,
            },
        );
        output_value
            .get_mut("payload")
            .and_then(Value::as_object_mut)
            .expect("payload")
            .insert(
                "previewLineItems".to_string(),
                json!(["line-1", "line-2", "line-3", "line-4", "line-5", "line-6"]),
            );

        let mut state = StreamJsonRenderStreamState::new();
        assert!(
            state.apply_render_event_value(&stream_json_render_event_value(
                &events[0],
                RenderEventStreamMetadata {
                    event_sequence: 1,
                    source_message_sequence: 1,
                    source_message_type: "test",
                    event_index_in_source: 0,
                    emitted_at_ms: 1,
                },
            ))
        );
        assert!(state.apply_render_event_value(&output_value));

        let collapsed_frame = state.terminal_frame_value();
        let collapsed_command_lines = collapsed_frame["regions"]
            .as_array()
            .expect("regions")
            .iter()
            .find(|region| region["id"] == "command")
            .expect("command region")["lines"]
            .as_array()
            .expect("command lines")
            .clone();
        assert!(collapsed_command_lines
            .iter()
            .any(|line| line.as_str() == Some("stdout: line-3")));
        assert!(collapsed_command_lines
            .iter()
            .all(|line| line.as_str() != Some("stdout: line-6")));

        assert!(state.toggle_command_widget_expanded());
        let snapshot = state.snapshot_value();
        assert_eq!(snapshot["terminal"]["command"]["expanded"], true);
        assert_eq!(snapshot["terminal"]["command"]["summaryOnly"], false);

        let expanded_frame = state.terminal_frame_value();
        let command_region = expanded_frame["regions"]
            .as_array()
            .expect("regions")
            .iter()
            .find(|region| region["id"] == "command")
            .expect("command region");
        assert_eq!(command_region["updateMode"], "replace_expanded_preview");
        let expanded_lines = command_region["lines"].as_array().expect("command lines");
        assert!(expanded_lines
            .iter()
            .any(|line| line.as_str() == Some("command details")));
        assert!(expanded_lines
            .iter()
            .any(|line| line.as_str() == Some("stdout: line-6")));
        assert!(expanded_lines.len() <= 18);
    }

    #[test]
    fn terminal_command_widget_accumulates_bounded_stream_tail_across_chunks() {
        let mut state = StreamJsonRenderStreamState::new();
        let start = RenderEvent::new(
            RenderEventKind::CommandStarted {
                tool_id: Some("cmd-tail".to_string()),
                command: Some("cargo test -- --nocapture".to_string()),
                cwd: Some("/repo".to_string()),
            },
            RenderEventScope::Main,
            UiStage::RunningCommand,
        );
        assert!(
            state.apply_render_event_value(&stream_json_render_event_value(
                &start,
                RenderEventStreamMetadata {
                    event_sequence: 1,
                    source_message_sequence: 1,
                    source_message_type: "test",
                    event_index_in_source: 0,
                    emitted_at_ms: 1,
                },
            ))
        );

        for (index, preview_lines) in [
            json!(["chunk-1-line-1", "chunk-1-line-2", "chunk-1-line-3"]),
            json!(["chunk-2-line-1", "chunk-2-line-2", "chunk-2-line-3"]),
        ]
        .into_iter()
        .enumerate()
        {
            let event = RenderEvent::new(
                RenderEventKind::CommandOutput {
                    tool_id: Some("cmd-tail".to_string()),
                    stream: "stdout".to_string(),
                    bytes: 128,
                    preview_lines: 3,
                    hidden_lines: 0,
                    total_lines: Some(3),
                    full_log_available: true,
                },
                RenderEventScope::Main,
                UiStage::RunningCommand,
            );
            let mut value = stream_json_render_event_value(
                &event,
                RenderEventStreamMetadata {
                    event_sequence: 2 + u64::try_from(index).expect("index"),
                    source_message_sequence: 2 + u64::try_from(index).expect("index"),
                    source_message_type: "test",
                    event_index_in_source: 0,
                    emitted_at_ms: 2 + u64::try_from(index).expect("index"),
                },
            );
            value
                .get_mut("payload")
                .and_then(Value::as_object_mut)
                .expect("payload")
                .insert("previewLineItems".to_string(), preview_lines);
            assert!(state.apply_render_event_value(&value));
        }

        let collapsed_frame = state.terminal_frame_value();
        assert_eq!(
            collapsed_frame["terminal"]["command"]["widget"]["outputChunkCount"],
            2
        );
        assert_eq!(
            collapsed_frame["terminal"]["command"]["widget"]["observedOutputLines"],
            6
        );
        assert_eq!(
            collapsed_frame["terminal"]["command"]["widget"]["retainedOutputLineCount"],
            3
        );
        let collapsed_lines = collapsed_frame["regions"]
            .as_array()
            .expect("regions")
            .iter()
            .find(|region| region["id"] == "command")
            .expect("command region")["lines"]
            .as_array()
            .expect("command lines");
        assert!(collapsed_lines
            .iter()
            .any(|line| { line.as_str() == Some("output: 3 tail, 3 hidden, 2 chunks") }));
        assert!(collapsed_lines
            .iter()
            .any(|line| line.as_str() == Some("stdout: chunk-2-line-3")));
        assert!(collapsed_lines
            .iter()
            .all(|line| line.as_str() != Some("stdout: chunk-1-line-1")));

        assert!(state.toggle_command_widget_expanded());
        let expanded_frame = state.terminal_frame_value();
        assert_eq!(
            expanded_frame["terminal"]["command"]["widget"]["retainedExpandedOutputLineCount"],
            6
        );
        let expanded_lines = expanded_frame["regions"]
            .as_array()
            .expect("regions")
            .iter()
            .find(|region| region["id"] == "command")
            .expect("command region")["lines"]
            .as_array()
            .expect("command lines");
        assert!(expanded_lines
            .iter()
            .any(|line| line.as_str() == Some("stdout: chunk-1-line-1")));
        assert!(expanded_lines
            .iter()
            .any(|line| line.as_str() == Some("stdout: chunk-2-line-3")));
        assert!(expanded_lines.len() <= 18);
    }

    #[test]
    fn terminal_frame_clears_retired_independent_widget_regions() {
        let mut state = StreamJsonRenderStreamState::new();
        state.last_sequence = 1;
        state.needs_immediate_render = true;
        state.current_activity = Some(json!({
            "kind": "approval_requested",
            "toolName": "Bash",
            "summary": "run cargo test",
        }));
        let (approval_frame, approval_fingerprint) = state.terminal_frame_value_with_previous(None);
        assert!(approval_frame["regions"]
            .as_array()
            .expect("approval regions")
            .iter()
            .any(|region| region["id"] == "approval"));
        let approval_line_count = approval_frame["regions"]
            .as_array()
            .expect("approval regions")
            .iter()
            .find(|region| region["id"] == "approval")
            .expect("approval region")["lines"]
            .as_array()
            .expect("approval lines")
            .len();
        assert_eq!(approval_line_count, 7);

        state.last_sequence = 2;
        state.current_activity = Some(json!({
            "kind": "assistant_message",
            "summary": "approval resolved",
        }));
        let (resolved_frame, _) =
            state.terminal_frame_value_with_previous(Some(&approval_fingerprint));

        assert!(resolved_frame["regions"]
            .as_array()
            .expect("resolved regions")
            .iter()
            .all(|region| region["id"] != "approval"));
        assert_eq!(resolved_frame["changes"]["removedRegionIds"][0], "approval");
        assert_eq!(
            resolved_frame["changes"]["retiredRegions"][0]["previousLineCount"],
            json!(approval_line_count)
        );

        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let approval_patch = patch_renderer.render_frame_value(&approval_frame);
        let _ = scheduler.render_patch_value(&approval_patch);
        let resolved_patch = patch_renderer.render_frame_value(&resolved_frame);
        let clear_op = resolved_patch["operations"]
            .as_array()
            .expect("resolved operations")
            .iter()
            .find(|operation| operation["regionId"] == "approval")
            .expect("approval clear op");
        assert_eq!(clear_op["op"], "clear_region");
        assert_eq!(clear_op["updateMode"], "clear_retired");
        assert_eq!(clear_op["previousLineCount"], json!(approval_line_count));

        let draw_plan = scheduler.render_patch_value(&resolved_patch);
        let clear_count = draw_plan["terminalOps"]
            .as_array()
            .expect("terminal ops")
            .iter()
            .filter(|operation| {
                operation["op"] == "clear_line" && operation["regionId"] == "approval"
            })
            .count();
        assert_eq!(clear_count, approval_line_count);
    }

    #[test]
    fn terminal_frame_keeps_file_change_summary_separate_from_diff() {
        let file_change = RenderEvent::new(
            RenderEventKind::FileChangeSummary {
                tool_id: Some("edit-1".to_string()),
                file_count: 2,
                additions: 13,
                deletions: 4,
            },
            RenderEventScope::Main,
            UiStage::EditingFiles,
        );
        let diff = RenderEvent::new(
            RenderEventKind::DiffAvailable {
                tool_id: Some("edit-1".to_string()),
                file_count: 2,
                additions: 13,
                deletions: 4,
            },
            RenderEventScope::Main,
            UiStage::ReviewingResult,
        );
        let mut file_change_value = stream_json_render_event_value(
            &file_change,
            RenderEventStreamMetadata {
                event_sequence: 1,
                source_message_sequence: 1,
                source_message_type: "test",
                event_index_in_source: 0,
                emitted_at_ms: 1,
            },
        );
        file_change_value
            .get_mut("payload")
            .and_then(Value::as_object_mut)
            .expect("file change payload")
            .insert(
                "files".to_string(),
                json!([
                    {"path": "src/lib.rs", "status": "modified", "additions": 10, "deletions": 2},
                    {"path": "src/render.rs", "status": "added", "additions": 3, "deletions": 0}
                ]),
            );
        let mut diff_value = stream_json_render_event_value(
            &diff,
            RenderEventStreamMetadata {
                event_sequence: 2,
                source_message_sequence: 2,
                source_message_type: "test",
                event_index_in_source: 0,
                emitted_at_ms: 2,
            },
        );
        diff_value
            .get_mut("payload")
            .and_then(Value::as_object_mut)
            .expect("diff payload")
            .insert(
                "diffText".to_string(),
                json!("diff --git a/src/lib.rs b/src/lib.rs\n@@ -1,2 +1,2 @@\n-old\n+new\n"),
            );

        let mut state = StreamJsonRenderStreamState::new();
        assert!(state.apply_render_event_value(&file_change_value));
        assert!(state.apply_render_event_value(&diff_value));

        let snapshot = state.snapshot_value();
        assert_eq!(snapshot["terminal"]["fileChanges"]["available"], true);
        assert_eq!(
            snapshot["terminal"]["fileChanges"]["independentRegion"],
            true
        );
        assert_eq!(snapshot["terminal"]["diff"]["available"], true);
        assert_eq!(
            snapshot["terminal"]["fileChanges"]["widget"]["summary"],
            "2 file(s), +13 -4"
        );

        let frame = state.terminal_frame_value();
        assert_eq!(frame["draw"]["fileChangeRegionId"], "file_changes");
        assert_eq!(frame["draw"]["diffRegionId"], "diff");
        let regions = frame["regions"].as_array().expect("regions");
        let file_change_index = regions
            .iter()
            .position(|region| region["id"] == "file_changes")
            .expect("file changes region index");
        let diff_index = regions
            .iter()
            .position(|region| region["id"] == "diff")
            .expect("diff region index");
        assert!(file_change_index < diff_index);

        let file_change_region = &regions[file_change_index];
        assert_eq!(file_change_region["role"], "file_changes");
        assert_eq!(file_change_region["anchor"], "top");
        assert_eq!(file_change_region["updateMode"], "replace_file_summary");
        let file_change_lines = file_change_region["lines"]
            .as_array()
            .expect("file change lines");
        assert!(file_change_lines
            .iter()
            .any(|line| line.as_str() == Some("M src/lib.rs +10 -2")));
        assert!(file_change_lines.len() <= 8);

        let diff_region = &regions[diff_index];
        assert_eq!(diff_region["role"], "diff");
        let diff_lines = diff_region["lines"].as_array().expect("diff lines");
        assert!(diff_lines.iter().any(|line| line.as_str() == Some("+new")));
        assert!(diff_lines
            .iter()
            .all(|line| line.as_str() != Some("file changes")));
    }

    #[test]
    fn terminal_file_change_widget_toggle_expands_bounded_file_preview() {
        let event = RenderEvent::new(
            RenderEventKind::FileChangeSummary {
                tool_id: Some("edit-1".to_string()),
                file_count: 8,
                additions: 80,
                deletions: 8,
            },
            RenderEventScope::Main,
            UiStage::ReviewingResult,
        );
        let mut value = stream_json_render_event_value(
            &event,
            RenderEventStreamMetadata {
                event_sequence: 1,
                source_message_sequence: 1,
                source_message_type: "test",
                event_index_in_source: 0,
                emitted_at_ms: 1,
            },
        );
        let payload = value
            .get_mut("payload")
            .and_then(Value::as_object_mut)
            .expect("payload");
        payload.insert(
            "files".to_string(),
            json!([
                {"path": "src/one.rs", "status": "modified", "additions": 10, "deletions": 1},
                {"path": "src/two.rs", "status": "modified", "additions": 10, "deletions": 1},
                {"path": "src/three.rs", "status": "modified", "additions": 10, "deletions": 1},
                {"path": "src/four.rs", "status": "modified", "additions": 10, "deletions": 1},
                {"path": "src/five.rs", "status": "modified", "additions": 10, "deletions": 1},
                {"path": "src/six.rs", "status": "modified", "additions": 10, "deletions": 1},
                {"path": "src/seven.rs", "status": "modified", "additions": 10, "deletions": 1},
                {"path": "src/eight.rs", "status": "modified", "additions": 10, "deletions": 1}
            ]),
        );
        let mut state = StreamJsonRenderStreamState::new();
        assert!(state.apply_render_event_value(&value));

        let collapsed_frame = state.terminal_frame_value();
        assert_eq!(
            collapsed_frame["terminal"]["fileChanges"]["summaryOnly"],
            true
        );
        assert_eq!(
            collapsed_frame["terminal"]["fileChanges"]["expanded"],
            false
        );
        assert_eq!(
            collapsed_frame["terminal"]["interaction"]["fileChangeToggleKey"],
            "f"
        );
        let collapsed_lines = collapsed_frame["regions"]
            .as_array()
            .expect("regions")
            .iter()
            .find(|region| region["id"] == "file_changes")
            .expect("file changes region")["lines"]
            .as_array()
            .expect("file change lines")
            .clone();
        assert!(collapsed_lines
            .iter()
            .all(|line| line.as_str() != Some("M src/eight.rs +10 -1")));

        assert!(state.toggle_file_change_widget_expanded());
        let snapshot = state.snapshot_value();
        assert_eq!(snapshot["terminal"]["fileChanges"]["expanded"], true);

        let expanded_frame = state.terminal_frame_value();
        assert_eq!(
            expanded_frame["terminal"]["interaction"]["fileChangeExpanded"],
            true
        );
        let file_change_region = expanded_frame["regions"]
            .as_array()
            .expect("regions")
            .iter()
            .find(|region| region["id"] == "file_changes")
            .expect("file changes region");
        assert_eq!(
            file_change_region["updateMode"],
            "replace_expanded_file_summary"
        );
        let expanded_lines = file_change_region["lines"]
            .as_array()
            .expect("file change lines");
        assert!(expanded_lines
            .iter()
            .any(|line| line.as_str() == Some("file change details")));
        assert!(expanded_lines
            .iter()
            .any(|line| line.as_str() == Some("M src/eight.rs +10 -1")));
        assert!(expanded_lines
            .iter()
            .any(|line| line.as_str() == Some("files: expanded preview")));
        assert!(expanded_lines.len() <= 18);
    }

    #[test]
    fn terminal_frame_renders_diff_as_collapsed_widget() {
        let event = RenderEvent::new(
            RenderEventKind::DiffAvailable {
                tool_id: Some("edit-1".to_string()),
                file_count: 3,
                additions: 42,
                deletions: 7,
            },
            RenderEventScope::Main,
            UiStage::ReviewingResult,
        );
        let value = stream_json_render_event_value(
            &event,
            RenderEventStreamMetadata {
                event_sequence: 1,
                source_message_sequence: 1,
                source_message_type: "test",
                event_index_in_source: 0,
                emitted_at_ms: 1,
            },
        );
        let mut state = StreamJsonRenderStreamState::new();

        assert!(state.apply_render_event_value(&value));
        let snapshot = state.snapshot_value();
        assert_eq!(snapshot["terminal"]["diff"]["available"], true);
        assert_eq!(snapshot["terminal"]["diff"]["collapsedByDefault"], true);

        let frame = state.terminal_frame_value();
        assert_eq!(frame["draw"]["diffRegionId"], "diff");
        let diff_region = frame["regions"]
            .as_array()
            .expect("regions")
            .iter()
            .find(|region| region["id"] == "diff")
            .expect("diff region");
        assert_eq!(diff_region["role"], "diff");
        assert_eq!(diff_region["anchor"], "top");
        assert_eq!(diff_region["updateMode"], "replace_collapsed");
        assert!(diff_region["lines"]
            .as_array()
            .expect("diff lines")
            .iter()
            .any(|line| line.as_str() == Some("3 file(s), +42 -7")));
        assert!(diff_region["lines"]
            .as_array()
            .expect("diff lines")
            .iter()
            .any(|line| line.as_str() == Some("diff: collapsed")));
    }

    #[test]
    fn terminal_frame_includes_diff_file_preview_while_collapsed() {
        let event = RenderEvent::new(
            RenderEventKind::DiffAvailable {
                tool_id: Some("edit-1".to_string()),
                file_count: 5,
                additions: 42,
                deletions: 7,
            },
            RenderEventScope::Main,
            UiStage::ReviewingResult,
        );
        let mut value = stream_json_render_event_value(
            &event,
            RenderEventStreamMetadata {
                event_sequence: 1,
                source_message_sequence: 1,
                source_message_type: "test",
                event_index_in_source: 0,
                emitted_at_ms: 1,
            },
        );
        let payload = value
            .get_mut("payload")
            .and_then(Value::as_object_mut)
            .expect("payload");
        payload.insert(
            "files".to_string(),
            json!([
                {"path": "src/lib.rs", "status": "modified", "additions": 12, "deletions": 3},
                {"path": "src/main.rs", "status": "modified", "additions": 8, "deletions": 2},
                {"path": "tests/render.rs", "status": "added", "additions": 20, "deletions": 0},
                {"path": "README.md", "status": "modified", "additions": 2, "deletions": 2},
                {"path": "notes/extra.md", "status": "deleted", "additions": 0, "deletions": 1}
            ]),
        );
        payload.insert(
            "diffText".to_string(),
            json!("diff --git a/src/lib.rs b/src/lib.rs\n@@ -1,2 +1,2 @@\n-old\n+new\n context\n"),
        );
        let mut state = StreamJsonRenderStreamState::new();

        assert!(state.apply_render_event_value(&value));
        let snapshot = state.snapshot_value();
        assert_eq!(
            snapshot["terminal"]["diff"]["widget"]["filePreviewLines"][0],
            "M src/lib.rs +12 -3"
        );
        assert_eq!(
            snapshot["terminal"]["diff"]["widget"]["previewFileCount"],
            4
        );
        assert_eq!(
            snapshot["terminal"]["diff"]["widget"]["omittedFileCount"],
            1
        );
        assert_eq!(
            snapshot["terminal"]["diff"]["widget"]["diffPreviewAvailable"],
            true
        );

        let frame = state.terminal_frame_value();
        let diff_region = frame["regions"]
            .as_array()
            .expect("regions")
            .iter()
            .find(|region| region["id"] == "diff")
            .expect("diff region");
        let diff_lines = diff_region["lines"].as_array().expect("diff lines");
        assert!(diff_lines
            .iter()
            .any(|line| line.as_str() == Some("M src/lib.rs +12 -3")));
        assert!(diff_lines
            .iter()
            .any(|line| line.as_str() == Some("files: 1 more hidden")));
        assert!(diff_lines
            .iter()
            .any(|line| line.as_str() == Some("diff: collapsed")));
        assert!(diff_lines
            .iter()
            .any(|line| line.as_str() == Some("diff preview:")));
        assert!(diff_lines.iter().any(|line| line.as_str() == Some("+new")));
        assert!(diff_lines.len() <= 12);
    }

    #[test]
    fn terminal_diff_widget_toggle_expands_bounded_file_and_hunk_preview() {
        let event = RenderEvent::new(
            RenderEventKind::DiffAvailable {
                tool_id: Some("edit-1".to_string()),
                file_count: 8,
                additions: 80,
                deletions: 8,
            },
            RenderEventScope::Main,
            UiStage::ReviewingResult,
        );
        let mut value = stream_json_render_event_value(
            &event,
            RenderEventStreamMetadata {
                event_sequence: 1,
                source_message_sequence: 1,
                source_message_type: "test",
                event_index_in_source: 0,
                emitted_at_ms: 1,
            },
        );
        let payload = value
            .get_mut("payload")
            .and_then(Value::as_object_mut)
            .expect("payload");
        payload.insert(
            "files".to_string(),
            json!([
                {"path": "src/one.rs", "status": "modified", "additions": 10, "deletions": 1},
                {"path": "src/two.rs", "status": "modified", "additions": 10, "deletions": 1},
                {"path": "src/three.rs", "status": "modified", "additions": 10, "deletions": 1},
                {"path": "src/four.rs", "status": "modified", "additions": 10, "deletions": 1},
                {"path": "src/five.rs", "status": "modified", "additions": 10, "deletions": 1},
                {"path": "src/six.rs", "status": "modified", "additions": 10, "deletions": 1},
                {"path": "src/seven.rs", "status": "modified", "additions": 10, "deletions": 1},
                {"path": "src/eight.rs", "status": "modified", "additions": 10, "deletions": 1}
            ]),
        );
        payload.insert(
            "diffText".to_string(),
            json!(
                "diff --git a/src/one.rs b/src/one.rs\n@@ -1,8 +1,8 @@\n-hunk-1\n+hunk-1\n-hunk-2\n+hunk-2\n-hunk-3\n+hunk-3\n-hunk-4\n+hunk-4\n"
            ),
        );
        let mut state = StreamJsonRenderStreamState::new();
        assert!(state.apply_render_event_value(&value));

        let collapsed_frame = state.terminal_frame_value();
        let collapsed_diff_lines = collapsed_frame["regions"]
            .as_array()
            .expect("regions")
            .iter()
            .find(|region| region["id"] == "diff")
            .expect("diff region")["lines"]
            .as_array()
            .expect("diff lines")
            .clone();
        assert!(collapsed_diff_lines
            .iter()
            .all(|line| line.as_str() != Some("M src/eight.rs +10 -1")));

        assert!(state.toggle_diff_widget_expanded());
        let snapshot = state.snapshot_value();
        assert_eq!(snapshot["terminal"]["diff"]["expanded"], true);

        let expanded_frame = state.terminal_frame_value();
        let diff_region = expanded_frame["regions"]
            .as_array()
            .expect("regions")
            .iter()
            .find(|region| region["id"] == "diff")
            .expect("diff region");
        assert_eq!(diff_region["updateMode"], "replace_expanded_preview");
        let expanded_lines = diff_region["lines"].as_array().expect("diff lines");
        assert!(expanded_lines
            .iter()
            .any(|line| line.as_str() == Some("diff details")));
        assert!(expanded_lines
            .iter()
            .any(|line| line.as_str() == Some("M src/eight.rs +10 -1")));
        assert!(expanded_lines
            .iter()
            .any(|line| line.as_str() == Some("diff: expanded preview")));
        assert!(expanded_lines
            .iter()
            .any(|line| line.as_str() == Some("+hunk-4")));
        assert!(expanded_lines.len() <= 28);
    }

    #[test]
    fn terminal_diff_widget_expanded_groups_unified_diff_by_file() {
        let event = RenderEvent::new(
            RenderEventKind::DiffAvailable {
                tool_id: Some("edit-1".to_string()),
                file_count: 2,
                additions: 2,
                deletions: 2,
            },
            RenderEventScope::Main,
            UiStage::ReviewingResult,
        );
        let mut value = stream_json_render_event_value(
            &event,
            RenderEventStreamMetadata {
                event_sequence: 1,
                source_message_sequence: 1,
                source_message_type: "test",
                event_index_in_source: 0,
                emitted_at_ms: 1,
            },
        );
        let payload = value
            .get_mut("payload")
            .and_then(Value::as_object_mut)
            .expect("payload");
        payload.insert(
            "diffText".to_string(),
            json!(
                "diff --git a/src/one.rs b/src/one.rs\n\
                 index 111..222 100644\n\
                 --- a/src/one.rs\n\
                 +++ b/src/one.rs\n\
                 @@ -1,2 +1,2 @@\n\
                 -one_old\n\
                 +one_new\n\
                  one_context\n\
                 diff --git a/src/two.rs b/src/two.rs\n\
                 index 333..444 100644\n\
                 --- a/src/two.rs\n\
                 +++ b/src/two.rs\n\
                 @@ -4,2 +4,2 @@\n\
                 -two_old\n\
                 +two_new\n"
            ),
        );
        let mut state = StreamJsonRenderStreamState::new();
        assert!(state.apply_render_event_value(&value));

        let snapshot = state.snapshot_value();
        assert_eq!(
            snapshot["terminal"]["diff"]["widget"]["diffFileSectionPreviewAvailable"],
            true
        );
        let sections = snapshot["terminal"]["diff"]["widget"]["expandedDiffFileSections"]
            .as_array()
            .expect("expanded diff file sections");
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0]["path"], "src/one.rs");
        assert_eq!(sections[0]["additions"], 1);
        assert_eq!(sections[0]["deletions"], 1);
        assert_eq!(sections[1]["path"], "src/two.rs");

        assert!(state.toggle_diff_widget_expanded());
        let expanded_frame = state.terminal_frame_value();
        let diff_region = expanded_frame["regions"]
            .as_array()
            .expect("regions")
            .iter()
            .find(|region| region["id"] == "diff")
            .expect("diff region");
        let expanded_lines = diff_region["lines"].as_array().expect("diff lines");
        let line_text = expanded_lines
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>();
        let one_index = line_text
            .iter()
            .position(|line| *line == "file: src/one.rs +1 -1")
            .expect("first file section");
        let two_index = line_text
            .iter()
            .position(|line| *line == "file: src/two.rs +1 -1")
            .expect("second file section");
        assert!(one_index < two_index);
        assert!(line_text.contains(&"diff files:"));
        assert!(line_text.contains(&"-one_old"));
        assert!(line_text.contains(&"+one_new"));
        assert!(line_text.contains(&"-two_old"));
        assert!(line_text.contains(&"+two_new"));
        assert!(line_text.iter().all(|line| !line.starts_with("diff --git")));
        assert!(expanded_lines.len() <= 28);
    }

    #[test]
    fn terminal_footer_exposes_contextual_keymap_controls() {
        let command_started = RenderEvent::new(
            RenderEventKind::CommandStarted {
                tool_id: Some("cmd-1".to_string()),
                command: Some("cargo test".to_string()),
                cwd: Some("/repo".to_string()),
            },
            RenderEventScope::Main,
            UiStage::RunningCommand,
        );
        let file_change_summary = RenderEvent::new(
            RenderEventKind::FileChangeSummary {
                tool_id: Some("edit-1".to_string()),
                file_count: 2,
                additions: 12,
                deletions: 3,
            },
            RenderEventScope::Main,
            UiStage::ReviewingResult,
        );
        let diff_available = RenderEvent::new(
            RenderEventKind::DiffAvailable {
                tool_id: Some("edit-1".to_string()),
                file_count: 2,
                additions: 12,
                deletions: 3,
            },
            RenderEventScope::Main,
            UiStage::ReviewingResult,
        );
        let mut state = StreamJsonRenderStreamState::new();
        for (index, event) in [command_started, file_change_summary, diff_available]
            .iter()
            .enumerate()
        {
            let sequence = u64::try_from(index + 1).expect("sequence");
            let value = stream_json_render_event_value(
                event,
                RenderEventStreamMetadata {
                    event_sequence: sequence,
                    source_message_sequence: sequence,
                    source_message_type: "test",
                    event_index_in_source: 0,
                    emitted_at_ms: sequence,
                },
            );
            assert!(state.apply_render_event_value(&value));
        }

        let snapshot = state.snapshot_value();
        assert_eq!(snapshot["terminal"]["interaction"]["visibleInFooter"], true);
        assert_eq!(snapshot["terminal"]["interaction"]["commandToggleKey"], "o");
        assert_eq!(
            snapshot["terminal"]["interaction"]["fileChangeToggleKey"],
            "f"
        );
        assert_eq!(snapshot["terminal"]["interaction"]["diffToggleKey"], "d");
        assert!(snapshot["terminal"]["interaction"]["hints"]
            .as_array()
            .expect("interaction hints")
            .iter()
            .any(|hint| hint.as_str() == Some("o expand cmd")));
        assert!(snapshot["terminal"]["interaction"]["hints"]
            .as_array()
            .expect("interaction hints")
            .iter()
            .any(|hint| hint.as_str() == Some("f expand files")));
        assert!(snapshot["terminal"]["interaction"]["hints"]
            .as_array()
            .expect("interaction hints")
            .iter()
            .any(|hint| hint.as_str() == Some("d expand diff")));

        let frame = state.terminal_frame_value();
        let footer_region = frame["regions"]
            .as_array()
            .expect("regions")
            .iter()
            .find(|region| region["id"] == "footer")
            .expect("footer region");
        let footer_line = footer_region["lines"][0].as_str().expect("footer line");
        assert!(footer_line.contains("keys:"));
        assert!(footer_line.contains("o expand cmd"));
        assert!(footer_line.contains("f expand files"));
        assert!(footer_line.contains("d expand diff"));
        assert!(footer_line.contains("PgUp hold"));
        assert!(footer_line.contains("PgDn/End live"));
        assert!(footer_line.contains("End live"));
        assert_eq!(frame["terminal"]["command"]["summaryOnly"], true);
        assert_eq!(frame["terminal"]["command"]["expanded"], false);

        assert!(state.toggle_command_widget_expanded());
        assert!(state.toggle_file_change_widget_expanded());
        assert!(state.toggle_diff_widget_expanded());
        let expanded_frame = state.terminal_frame_value();
        assert_eq!(expanded_frame["terminal"]["command"]["summaryOnly"], false);
        assert_eq!(expanded_frame["terminal"]["command"]["expanded"], true);
        assert_eq!(expanded_frame["terminal"]["fileChanges"]["expanded"], true);
        assert_eq!(expanded_frame["terminal"]["diff"]["expanded"], true);
        assert!(expanded_frame["terminal"]["interaction"]["hints"]
            .as_array()
            .expect("interaction hints")
            .iter()
            .any(|hint| hint.as_str() == Some("o collapse cmd")));
        assert!(expanded_frame["terminal"]["interaction"]["hints"]
            .as_array()
            .expect("interaction hints")
            .iter()
            .any(|hint| hint.as_str() == Some("f collapse files")));
        assert!(expanded_frame["terminal"]["interaction"]["hints"]
            .as_array()
            .expect("interaction hints")
            .iter()
            .any(|hint| hint.as_str() == Some("d collapse diff")));
    }

    #[test]
    fn terminal_footer_bounds_visible_hints_and_reports_overflow() {
        let events = [
            RenderEvent::new(
                RenderEventKind::CommandStarted {
                    tool_id: Some("cmd-1".to_string()),
                    command: Some("cargo test".to_string()),
                    cwd: Some("/repo".to_string()),
                },
                RenderEventScope::Main,
                UiStage::RunningCommand,
            ),
            RenderEvent::new(
                RenderEventKind::BackgroundTaskUpdated {
                    tool_id: Some("cmd-bg".to_string()),
                    task_id: "bg-1".to_string(),
                    task_type: "background_shell".to_string(),
                    status: "running".to_string(),
                    command: Some("cargo test --watch".to_string()),
                    preview_lines: 1,
                    hidden_lines: 0,
                    exit_code: None,
                },
                RenderEventScope::Main,
                UiStage::RunningCommand,
            ),
            RenderEvent::new(
                RenderEventKind::FileChangeSummary {
                    tool_id: Some("edit-1".to_string()),
                    file_count: 2,
                    additions: 12,
                    deletions: 3,
                },
                RenderEventScope::Main,
                UiStage::ReviewingResult,
            ),
            RenderEvent::new(
                RenderEventKind::DiffAvailable {
                    tool_id: Some("edit-1".to_string()),
                    file_count: 2,
                    additions: 12,
                    deletions: 3,
                },
                RenderEventScope::Main,
                UiStage::ReviewingResult,
            ),
            RenderEvent::new(
                RenderEventKind::ErrorRaised {
                    source: "terminal".to_string(),
                    summary: "tests failed".to_string(),
                },
                RenderEventScope::Main,
                UiStage::Failed,
            ),
        ];

        let mut state = StreamJsonRenderStreamState::new();
        for (index, event) in events.iter().enumerate() {
            let sequence = u64::try_from(index + 1).expect("sequence");
            let value = stream_json_render_event_value(
                event,
                RenderEventStreamMetadata {
                    event_sequence: sequence,
                    source_message_sequence: sequence,
                    source_message_type: "test",
                    event_index_in_source: 0,
                    emitted_at_ms: sequence,
                },
            );
            assert!(state.apply_render_event_value(&value));
        }

        let snapshot = state.snapshot_value();
        let interaction = &snapshot["terminal"]["interaction"];
        let hints = interaction["hints"].as_array().expect("full hints");
        let footer_hints = interaction["footerHints"].as_array().expect("footer hints");
        assert_eq!(interaction["footerHintMax"].as_u64(), Some(6));
        assert_eq!(interaction["footerHintsBounded"], true);
        assert_eq!(interaction["fullHints"], interaction["hints"]);
        assert!(hints.len() > STREAM_JSON_RENDER_FOOTER_VISIBLE_HINT_MAX);
        assert_eq!(
            footer_hints.len(),
            STREAM_JSON_RENDER_FOOTER_VISIBLE_HINT_MAX
        );
        assert_eq!(interaction["footerHintOverflowCount"].as_u64(), Some(3));
        assert!(footer_hints
            .iter()
            .any(|hint| hint.as_str() == Some("+3 more")));
        assert!(hints
            .iter()
            .any(|hint| hint.as_str() == Some("Ctrl+L live")));

        let frame = state.terminal_frame_value();
        let footer_region = frame["regions"]
            .as_array()
            .expect("regions")
            .iter()
            .find(|region| region["id"] == "footer")
            .expect("footer region");
        let footer_line = footer_region["lines"][0].as_str().expect("footer line");
        assert!(footer_line.contains("o expand cmd"));
        assert!(footer_line.contains("b expand bg"));
        assert!(footer_line.contains("f expand files"));
        assert!(footer_line.contains("d expand diff"));
        assert!(footer_line.contains("x expand error"));
        assert!(footer_line.contains("+3 more"));
        assert!(!footer_line.contains("PgUp hold"));
        assert!(!footer_line.contains("PgDn/End live"));
        assert!(!footer_line.contains("End live"));
        assert!(!footer_line.contains("Ctrl+L live"));
    }

    #[test]
    fn terminal_frame_renders_error_as_layered_widget() {
        let events = [
            RenderEvent::new(
                RenderEventKind::ErrorRaised {
                    source: "terminal".to_string(),
                    summary: "build failed: unresolved import".to_string(),
                },
                RenderEventScope::Main,
                UiStage::Failed,
            ),
            RenderEvent::new(
                RenderEventKind::ApiRetry {
                    attempt: 1,
                    max_retries: 3,
                    retry_in_ms: 250,
                },
                RenderEventScope::Main,
                UiStage::Retrying,
            ),
        ];
        let mut state = StreamJsonRenderStreamState::new();
        for (index, event) in events.iter().enumerate() {
            let sequence = u64::try_from(index + 1).expect("sequence");
            let value = stream_json_render_event_value(
                event,
                RenderEventStreamMetadata {
                    event_sequence: sequence,
                    source_message_sequence: sequence,
                    source_message_type: "test",
                    event_index_in_source: 0,
                    emitted_at_ms: sequence,
                },
            );
            assert!(state.apply_render_event_value(&value));
        }

        let snapshot = state.snapshot_value();
        assert_eq!(snapshot["terminal"]["error"]["available"], true);
        assert_eq!(snapshot["terminal"]["error"]["layered"], true);
        assert_eq!(snapshot["terminal"]["error"]["detailsAvailable"], true);
        assert_eq!(snapshot["terminal"]["error"]["widget"]["retrying"], true);

        let frame = state.terminal_frame_value();
        assert_eq!(frame["draw"]["errorRegionId"], "error");
        let regions = frame["regions"].as_array().expect("regions");
        let active_region = regions
            .iter()
            .find(|region| region["id"] == "active")
            .expect("active region");
        assert!(active_region["lines"]
            .as_array()
            .expect("active lines")
            .is_empty());
        let error_region = regions
            .iter()
            .find(|region| region["id"] == "error")
            .expect("error region");
        assert_eq!(error_region["role"], "error");
        assert_eq!(error_region["anchor"], "top");
        assert_eq!(error_region["updateMode"], "replace_layered");
        assert!(error_region["lines"]
            .as_array()
            .expect("error lines")
            .iter()
            .any(|line| line.as_str() == Some("details: available")));
        assert!(error_region["lines"]
            .as_array()
            .expect("error lines")
            .iter()
            .any(|line| line.as_str().unwrap_or_default().contains("next in 250ms")));

        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let draw_plan = scheduler.render_patch_value(&patch_renderer.render_frame_value(&frame));
        assert!(draw_plan["terminalOps"]
            .as_array()
            .expect("terminal ops")
            .iter()
            .any(|op| op["op"] == "move_to_row"
                && op["regionId"] == "error"
                && op["row"]
                    .as_str()
                    .is_some_and(|row| row.starts_with("top+"))));
    }

    #[test]
    fn terminal_error_widget_toggle_expands_bounded_details() {
        let event = RenderEvent::new(
            RenderEventKind::ErrorRaised {
                source: "terminal".to_string(),
                summary: "build failed: unresolved import".to_string(),
            },
            RenderEventScope::Main,
            UiStage::Failed,
        );
        let mut value = stream_json_render_event_value(
            &event,
            RenderEventStreamMetadata {
                event_sequence: 1,
                source_message_sequence: 1,
                source_message_type: "test",
                event_index_in_source: 0,
                emitted_at_ms: 1,
            },
        );
        value
            .get_mut("payload")
            .and_then(Value::as_object_mut)
            .expect("payload")
            .insert(
                "details".to_string(),
                json!(
                    "stack frame 1\nstack frame 2\nstack frame 3\nstack frame 4\nstack frame 5\nstack frame 6\nstack frame 7\nstack frame 8\nstack frame 9\nstack frame 10\nstack frame 11"
                ),
            );
        let mut state = StreamJsonRenderStreamState::new();
        assert!(state.apply_render_event_value(&value));

        let collapsed_frame = state.terminal_frame_value();
        assert_eq!(collapsed_frame["terminal"]["error"]["summaryOnly"], true);
        assert_eq!(collapsed_frame["terminal"]["error"]["expanded"], false);
        assert_eq!(
            collapsed_frame["terminal"]["interaction"]["errorToggleKey"],
            "x"
        );
        let collapsed_lines = collapsed_frame["regions"]
            .as_array()
            .expect("regions")
            .iter()
            .find(|region| region["id"] == "error")
            .expect("error region")["lines"]
            .as_array()
            .expect("error lines")
            .clone();
        assert!(collapsed_lines
            .iter()
            .any(|line| line.as_str() == Some("details: available")));
        assert!(collapsed_lines
            .iter()
            .all(|line| line.as_str() != Some("stack frame 10")));

        assert!(state.toggle_error_widget_expanded());
        let snapshot = state.snapshot_value();
        assert_eq!(snapshot["terminal"]["error"]["expanded"], true);

        let expanded_frame = state.terminal_frame_value();
        assert_eq!(
            expanded_frame["terminal"]["interaction"]["errorExpanded"],
            true
        );
        let error_region = expanded_frame["regions"]
            .as_array()
            .expect("regions")
            .iter()
            .find(|region| region["id"] == "error")
            .expect("error region");
        assert_eq!(error_region["updateMode"], "replace_error_details");
        let expanded_lines = error_region["lines"].as_array().expect("error lines");
        assert!(expanded_lines
            .iter()
            .any(|line| line.as_str() == Some("error details")));
        assert!(expanded_lines
            .iter()
            .any(|line| line.as_str() == Some("stack frame 10")));
        assert!(expanded_lines
            .iter()
            .all(|line| line.as_str() != Some("stack frame 11")));
        assert!(expanded_lines.len() <= 16);
    }

    #[test]
    fn terminal_frame_renders_final_summary_as_independent_region() {
        let events = [
            RenderEvent::new(
                RenderEventKind::CommandStarted {
                    tool_id: Some("cmd-1".to_string()),
                    command: Some("cargo test".to_string()),
                    cwd: Some("/repo".to_string()),
                },
                RenderEventScope::Main,
                UiStage::RunningCommand,
            ),
            RenderEvent::new(
                RenderEventKind::CommandFinished {
                    tool_id: Some("cmd-1".to_string()),
                    exit_code: Some(1),
                    duration_ms: Some(900),
                },
                RenderEventScope::Main,
                UiStage::ReviewingResult,
            ),
            RenderEvent::new(
                RenderEventKind::BackgroundTaskUpdated {
                    tool_id: Some("cmd-bg".to_string()),
                    task_id: "bg-1".to_string(),
                    task_type: "background_shell".to_string(),
                    status: "running".to_string(),
                    command: Some("cargo test --watch".to_string()),
                    preview_lines: 2,
                    hidden_lines: 0,
                    exit_code: None,
                },
                RenderEventScope::Main,
                UiStage::RunningCommand,
            ),
            RenderEvent::new(
                RenderEventKind::FileChangeSummary {
                    tool_id: Some("edit-1".to_string()),
                    file_count: 2,
                    additions: 12,
                    deletions: 3,
                },
                RenderEventScope::Main,
                UiStage::ReviewingResult,
            ),
            RenderEvent::new(
                RenderEventKind::DiffAvailable {
                    tool_id: Some("edit-1".to_string()),
                    file_count: 2,
                    additions: 12,
                    deletions: 3,
                },
                RenderEventScope::Main,
                UiStage::ReviewingResult,
            ),
            RenderEvent::new(
                RenderEventKind::ErrorRaised {
                    source: "terminal".to_string(),
                    summary: "tests failed".to_string(),
                },
                RenderEventScope::Main,
                UiStage::Failed,
            ),
            RenderEvent::new(
                RenderEventKind::FinalSummaryRecorded {
                    terminal: "failed".to_string(),
                    success: false,
                },
                RenderEventScope::Main,
                UiStage::Failed,
            ),
        ];
        let mut state = StreamJsonRenderStreamState::new();
        for (index, event) in events.iter().enumerate() {
            let sequence = u64::try_from(index + 1).expect("sequence");
            let value = stream_json_render_event_value(
                event,
                RenderEventStreamMetadata {
                    event_sequence: sequence,
                    source_message_sequence: sequence,
                    source_message_type: "test",
                    event_index_in_source: 0,
                    emitted_at_ms: sequence,
                },
            );
            assert!(state.apply_render_event_value(&value));
        }

        let snapshot = state.snapshot_value();
        assert_eq!(snapshot["terminal"]["finalSummary"]["available"], true);
        assert_eq!(
            snapshot["terminal"]["finalSummary"]["independentRegion"],
            true
        );
        assert_eq!(
            snapshot["terminal"]["finalSummary"]["widget"]["diffSummary"]["summary"],
            "2 file(s), +12 -3"
        );
        assert_eq!(
            snapshot["terminal"]["finalSummary"]["widget"]["fileChangeSummary"]["summary"],
            "2 file(s), +12 -3"
        );

        let frame = state.terminal_frame_value();
        assert_eq!(frame["draw"]["finalSummaryRegionId"], "final_summary");
        let regions = frame["regions"].as_array().expect("regions");
        let error_index = regions
            .iter()
            .position(|region| region["id"] == "error")
            .expect("error region index");
        let final_summary_index = regions
            .iter()
            .position(|region| region["id"] == "final_summary")
            .expect("final summary region index");
        let command_index = regions
            .iter()
            .position(|region| region["id"] == "command")
            .expect("command region index");
        let background_task_index = regions
            .iter()
            .position(|region| region["id"] == "background_tasks")
            .expect("background tasks region index");
        let file_change_index = regions
            .iter()
            .position(|region| region["id"] == "file_changes")
            .expect("file changes region index");
        let diff_index = regions
            .iter()
            .position(|region| region["id"] == "diff")
            .expect("diff region index");
        assert!(error_index < final_summary_index);
        assert!(final_summary_index < command_index);
        assert!(command_index < background_task_index);
        assert!(background_task_index < file_change_index);
        assert!(file_change_index < diff_index);
        assert!(background_task_index < diff_index);
        let active_region = regions
            .iter()
            .find(|region| region["id"] == "active")
            .expect("active region");
        assert!(active_region["lines"]
            .as_array()
            .expect("active lines")
            .is_empty());
        let summary_region = regions
            .iter()
            .find(|region| region["id"] == "final_summary")
            .expect("final summary region");
        assert_eq!(summary_region["role"], "final_summary");
        assert_eq!(summary_region["anchor"], "top");
        assert_eq!(summary_region["updateMode"], "replace_final_summary");
        let lines = summary_region["lines"]
            .as_array()
            .expect("final summary lines");
        assert!(lines
            .iter()
            .any(|line| line.as_str() == Some("result: failed")));
        assert!(lines
            .iter()
            .any(|line| line.as_str() == Some("files: 2 file(s), +12 -3")));
        assert!(lines
            .iter()
            .any(|line| line.as_str() == Some("diff: 2 file(s), +12 -3")));
        assert!(lines
            .iter()
            .any(|line| line.as_str() == Some("command: command finished: exit 1")));
        assert!(lines
            .iter()
            .any(|line| line.as_str() == Some("risk: tests failed")));

        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let patch = patch_renderer.render_frame_value(&frame);
        let patch_operations = patch["operations"].as_array().expect("patch operations");
        let final_summary_start_row = patch_operations
            .iter()
            .find(|op| op["regionId"] == "final_summary")
            .and_then(|op| op["topStartRow"].as_u64())
            .expect("final summary top row");
        let command_start_row = patch_operations
            .iter()
            .find(|op| op["regionId"] == "command")
            .and_then(|op| op["topStartRow"].as_u64())
            .expect("command top row");
        let background_task_start_row = patch_operations
            .iter()
            .find(|op| op["regionId"] == "background_tasks")
            .and_then(|op| op["topStartRow"].as_u64())
            .expect("background tasks top row");
        let diff_start_row = patch_operations
            .iter()
            .find(|op| op["regionId"] == "diff")
            .and_then(|op| op["topStartRow"].as_u64())
            .expect("diff top row");
        assert!(final_summary_start_row < command_start_row);
        assert!(command_start_row < background_task_start_row);
        assert!(background_task_start_row < diff_start_row);
        let draw_plan = scheduler.render_patch_value(&patch);
        assert!(draw_plan["terminalOps"]
            .as_array()
            .expect("terminal ops")
            .iter()
            .any(|op| op["op"] == "move_to_row"
                && op["regionId"] == "final_summary"
                && op["row"]
                    .as_str()
                    .is_some_and(|row| row.starts_with("top+"))));
    }

    #[test]
    fn terminal_final_summary_records_command_history_and_verification() {
        let events = [
            RenderEvent::new(
                RenderEventKind::CommandStarted {
                    tool_id: Some("cmd-1".to_string()),
                    command: Some("cargo fmt --check".to_string()),
                    cwd: Some("/repo".to_string()),
                },
                RenderEventScope::Main,
                UiStage::RunningCommand,
            ),
            RenderEvent::new(
                RenderEventKind::CommandFinished {
                    tool_id: Some("cmd-1".to_string()),
                    exit_code: Some(0),
                    duration_ms: Some(120),
                },
                RenderEventScope::Main,
                UiStage::ReviewingResult,
            ),
            RenderEvent::new(
                RenderEventKind::CommandStarted {
                    tool_id: Some("cmd-2".to_string()),
                    command: Some("cargo test".to_string()),
                    cwd: Some("/repo".to_string()),
                },
                RenderEventScope::Main,
                UiStage::RunningCommand,
            ),
            RenderEvent::new(
                RenderEventKind::CommandFinished {
                    tool_id: Some("cmd-2".to_string()),
                    exit_code: Some(1),
                    duration_ms: Some(900),
                },
                RenderEventScope::Main,
                UiStage::ReviewingResult,
            ),
            RenderEvent::new(
                RenderEventKind::FinalSummaryRecorded {
                    terminal: "failed".to_string(),
                    success: false,
                },
                RenderEventScope::Main,
                UiStage::Failed,
            ),
        ];
        let mut state = StreamJsonRenderStreamState::new();
        for (index, event) in events.iter().enumerate() {
            let sequence = u64::try_from(index + 1).expect("sequence");
            let value = stream_json_render_event_value(
                event,
                RenderEventStreamMetadata {
                    event_sequence: sequence,
                    source_message_sequence: sequence,
                    source_message_type: "test",
                    event_index_in_source: 0,
                    emitted_at_ms: sequence,
                },
            );
            assert!(state.apply_render_event_value(&value));
        }

        let snapshot = state.snapshot_value();
        assert_eq!(snapshot["terminal"]["command"]["historyCount"], 2);
        let final_summary = &snapshot["terminal"]["finalSummary"]["widget"];
        assert_eq!(final_summary["commandHistoryCount"], 2);
        assert_eq!(final_summary["verificationSummary"]["totalCommands"], 2);
        assert_eq!(final_summary["verificationSummary"]["passedCommands"], 1);
        assert_eq!(final_summary["verificationSummary"]["failedCommands"], 1);
        assert_eq!(final_summary["verificationSummary"]["status"], "failed");
        assert_eq!(
            final_summary["residualRiskSummary"]["summary"],
            "1 command(s) failed"
        );

        let frame = state.terminal_frame_value();
        let final_summary_lines = frame["regions"]
            .as_array()
            .expect("regions")
            .iter()
            .find(|region| region["id"] == "final_summary")
            .expect("final summary region")["lines"]
            .as_array()
            .expect("final summary lines")
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>();
        assert!(final_summary_lines.contains(&"verification: 2 command(s), 1 passed, 1 failed"));
        assert!(final_summary_lines.contains(&"commands: 2 recorded"));
        assert!(final_summary_lines.contains(&"cmd: cargo fmt --check -> exit 0"));
        assert!(final_summary_lines.contains(&"cmd: cargo test -> exit 1"));
        assert!(final_summary_lines.contains(&"risk: 1 command(s) failed"));
    }

    #[test]
    fn emits_terminal_patch_after_frame() {
        let message = SdkMessage::StreamEvent {
            event: StreamEventData::ContentBlockDelta {
                index: 0,
                delta: ContentDelta::TextDelta {
                    text: "hello".to_string(),
                },
            },
            task_id: None,
        };
        let mut emitter = StreamJsonRenderEventEmitter::new();

        let items = emitter.emit_stream_items_for_sdk_message(&message);
        let frame = &items[2];
        let patch = &items[3];
        let draw_plan = &items[4];

        assert_eq!(patch["type"], STREAM_JSON_RENDER_PATCH_TYPE);
        assert_eq!(
            patch["schemaVersion"],
            STREAM_JSON_RENDER_PATCH_SCHEMA_VERSION
        );
        assert_eq!(patch["sourceFrame"]["frameHash"], frame["frameHash"]);
        assert_eq!(patch["draw"]["preferredStrategy"], "patch_regions");
        assert_eq!(patch["draw"]["replaceWholeScreen"], false);
        assert_eq!(patch["draw"]["skipped"], false);
        assert_eq!(patch["flush"]["shouldFlush"], true);
        assert_eq!(patch["cursor"]["preservePrompt"], true);
        let operations = patch["operations"].as_array().expect("operations");
        assert_eq!(operations.len(), 3);
        assert_eq!(operations[0]["op"], "replace_region");
        assert_eq!(operations[0]["regionId"], "status");
        for operation in operations {
            for line in operation["lines"].as_array().expect("operation lines") {
                let line = line.as_str().expect("line");
                assert!(!line.contains('\n'));
                assert!(!line.contains('\r'));
            }
        }
        assert_eq!(draw_plan["type"], STREAM_JSON_RENDER_DRAW_PLAN_TYPE);
        assert_eq!(
            draw_plan["schemaVersion"],
            STREAM_JSON_RENDER_DRAW_PLAN_SCHEMA_VERSION
        );
        assert_eq!(draw_plan["sourceFrame"]["frameHash"], frame["frameHash"]);
        assert_eq!(draw_plan["draw"]["strategy"], "anchored_region_patch");
        assert_eq!(draw_plan["draw"]["replaceWholeScreen"], false);
        assert_eq!(draw_plan["schedule"]["shouldFlush"], true);
        assert_eq!(draw_plan["cursor"]["restoreAfterDraw"], true);
        assert!(draw_plan["terminalOps"]
            .as_array()
            .expect("terminal ops")
            .iter()
            .any(|op| op["op"] == "save_cursor"));
        assert!(draw_plan["terminalOps"]
            .as_array()
            .expect("terminal ops")
            .iter()
            .any(|op| op["op"] == "restore_cursor"));
    }

    #[test]
    fn terminal_frontend_sdk_emit_returns_only_draw_plan_item() {
        let message = SdkMessage::StreamEvent {
            event: StreamEventData::ContentBlockDelta {
                index: 0,
                delta: ContentDelta::TextDelta {
                    text: "hello terminal".to_string(),
                },
            },
            task_id: None,
        };
        let mut emitter = StreamJsonRenderEventEmitter::new();

        let items = emitter.emit_terminal_draw_plan_items_for_sdk_message(&message);

        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["type"], STREAM_JSON_RENDER_DRAW_PLAN_TYPE);
        assert!(items
            .iter()
            .all(|item| item["type"] != STREAM_JSON_RENDER_PATCH_TYPE));
        assert!(items
            .iter()
            .all(|item| item["type"] != STREAM_JSON_RENDER_FRAME_TYPE));
        assert!(items
            .iter()
            .all(|item| item["type"] != STREAM_JSON_RENDER_SNAPSHOT_TYPE));
    }

    #[test]
    fn terminal_frontend_permission_emit_returns_only_draw_plan_item() {
        let mut emitter = StreamJsonRenderEventEmitter::new();
        let items = emitter.emit_terminal_permission_request_draw_plan_items(
            "Bash",
            &json!({ "command": "cargo test -p mossen-cli terminal_frontend" }),
        );

        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["type"], STREAM_JSON_RENDER_DRAW_PLAN_TYPE);
        assert!(items[0]["terminalOps"]
            .as_array()
            .expect("terminal ops")
            .iter()
            .any(|op| op["regionId"] == "approval"));
        assert!(items
            .iter()
            .all(|item| item["type"] != STREAM_JSON_RENDER_PATCH_TYPE));
        assert!(items
            .iter()
            .all(|item| item["type"] != STREAM_JSON_RENDER_FRAME_TYPE));
        assert!(items
            .iter()
            .all(|item| item["type"] != STREAM_JSON_RENDER_SNAPSHOT_TYPE));
    }

    #[test]
    fn terminal_frontend_resize_emit_forces_current_draw_plan_only() {
        let message = SdkMessage::StreamEvent {
            event: StreamEventData::ContentBlockDelta {
                index: 0,
                delta: ContentDelta::TextDelta {
                    text: "resize current frame".to_string(),
                },
            },
            task_id: None,
        };
        let mut emitter = StreamJsonRenderEventEmitter::new();
        let first_items = emitter.emit_terminal_draw_plan_items_for_sdk_message(&message);
        let first_frame_hash = first_items[0]["sourceFrame"]["frameHash"].clone();

        let resize_items = emitter.emit_terminal_resize_draw_plan_items();

        assert_eq!(resize_items.len(), 1);
        let draw_plan = &resize_items[0];
        assert_eq!(draw_plan["type"], STREAM_JSON_RENDER_DRAW_PLAN_TYPE);
        assert_eq!(draw_plan["sourceFrame"]["frameHash"], first_frame_hash);
        assert_eq!(draw_plan["draw"]["forcedRedraw"], true);
        assert_eq!(draw_plan["draw"]["forceRedrawReason"], "viewport_resize");
        assert_eq!(draw_plan["draw"]["skipped"], false);
        assert_eq!(draw_plan["schedule"]["flushPolicy"], "immediate");
        assert_eq!(draw_plan["schedule"]["dropWhenSuperseded"], false);
        assert!(draw_plan["terminalOps"]
            .as_array()
            .expect("terminal ops")
            .iter()
            .any(|op| op["regionId"] == "status"));
        assert!(resize_items
            .iter()
            .all(|item| item["type"] != STREAM_JSON_RENDER_PATCH_TYPE));
        assert!(resize_items
            .iter()
            .all(|item| item["type"] != STREAM_JSON_RENDER_FRAME_TYPE));
        assert!(resize_items
            .iter()
            .all(|item| item["type"] != STREAM_JSON_RENDER_SNAPSHOT_TYPE));
    }

    #[test]
    fn terminal_frontend_resize_does_not_reappend_committed_transcript() {
        let mut emitter = StreamJsonRenderEventEmitter::new();
        let text = SdkMessage::StreamEvent {
            event: StreamEventData::ContentBlockDelta {
                index: 0,
                delta: ContentDelta::TextDelta {
                    text: "final transcript text".to_string(),
                },
            },
            task_id: None,
        };
        let result = SdkMessage::Result {
            terminal: "ok".to_string(),
            cost_usd: None,
            duration_ms: Some(12),
            usage: None,
            task_id: None,
        };
        let _ = emitter.emit_terminal_draw_plan_items_for_sdk_message(&text);
        let final_items = emitter.emit_terminal_draw_plan_items_for_sdk_message(&result);
        assert!(final_items[0]["terminalOps"]
            .as_array()
            .expect("final terminal ops")
            .iter()
            .any(|op| op["op"] == "append_scrollback_block"));

        let resize_items = emitter.emit_terminal_resize_draw_plan_items();
        let resize_plan = &resize_items[0];

        assert_eq!(resize_plan["draw"]["forcedRedraw"], true);
        assert_eq!(resize_plan["draw"]["scrollbackAppendOnceSuppressed"], true);
        assert!(resize_plan["terminalOps"]
            .as_array()
            .expect("resize terminal ops")
            .iter()
            .all(|op| op["op"] != "append_scrollback_block"));
    }

    #[test]
    fn terminal_frame_marks_unchanged_regions_for_skip_draw() {
        let first = SdkMessage::StreamEvent {
            event: StreamEventData::ContentBlockDelta {
                index: 0,
                delta: ContentDelta::ThinkingDelta {
                    thinking: "hello".to_string(),
                },
            },
            task_id: None,
        };
        let second = SdkMessage::StreamEvent {
            event: StreamEventData::ContentBlockDelta {
                index: 0,
                delta: ContentDelta::ThinkingDelta {
                    thinking: "hello".to_string(),
                },
            },
            task_id: None,
        };
        let mut emitter = StreamJsonRenderEventEmitter::new();

        let first_items = emitter.emit_stream_items_for_sdk_message(&first);
        let second_items = emitter.emit_stream_items_for_sdk_message(&second);
        let first_frame = &first_items[2];
        let second_frame = &second_items[2];

        assert_eq!(first_frame["changes"]["firstFrame"], true);
        assert_eq!(
            first_frame["changes"]["changedRegionIds"]
                .as_array()
                .expect("first changed regions")
                .len(),
            3
        );
        assert_eq!(
            first_frame["changes"]["unchangedRegionIds"]
                .as_array()
                .expect("first unchanged regions")
                .len(),
            0
        );
        assert_eq!(second_frame["changes"]["firstFrame"], false);
        assert_eq!(
            second_frame["changes"]["previousFrameHash"],
            first_frame["frameHash"]
        );
        assert_eq!(
            second_frame["changes"]["currentFrameHash"],
            first_frame["frameHash"]
        );
        assert_eq!(second_frame["draw"]["dirty"], false);
        assert_eq!(second_frame["draw"]["skipIfFrameHashUnchanged"], true);
        assert_eq!(
            second_frame["changes"]["changedRegionIds"]
                .as_array()
                .expect("second changed regions")
                .len(),
            0
        );
        let unchanged = second_frame["changes"]["unchangedRegionIds"]
            .as_array()
            .expect("second unchanged regions")
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>();
        assert_eq!(unchanged, vec!["status", "active", "footer"]);
        for region in second_frame["regions"].as_array().expect("regions") {
            assert!(region["regionHash"].as_str().unwrap_or_default().len() >= 16);
        }
        let second_patch = &second_items[3];
        let second_draw_plan = &second_items[4];
        assert_eq!(second_patch["type"], STREAM_JSON_RENDER_PATCH_TYPE);
        assert_eq!(second_patch["draw"]["skipped"], true);
        assert_eq!(second_patch["draw"]["skipReason"], "frame_hash_unchanged");
        assert_eq!(
            second_patch["operations"]
                .as_array()
                .expect("second patch operations")
                .len(),
            0
        );
        assert_eq!(second_draw_plan["type"], STREAM_JSON_RENDER_DRAW_PLAN_TYPE);
        assert_eq!(second_draw_plan["draw"]["skipped"], true);
        assert_eq!(second_draw_plan["schedule"]["shouldFlush"], false);
        assert_eq!(
            second_draw_plan["terminalOps"]
                .as_array()
                .expect("second terminal ops")
                .len(),
            0
        );
    }
}
