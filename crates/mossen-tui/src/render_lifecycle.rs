//! Viewport-independent lifecycle records that feed the render model.
//!
//! This is Layer 1 of the TUI rendering pipeline. It preserves transcript
//! facts and approval decisions without terminal layout, box drawing, or
//! footer strings. The current implementation is still a compatibility
//! adapter over `MessageData`, but it gives Layer 2 a stable record boundary.

use crate::message_model::{MessageData, MessageType};
use mossen_agent::types::{ContentDelta, SdkMessage, StreamEventData};
use mossen_types::{AssistantMessage, ContentBlock};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

const APPROVAL_DECISION_PREFIX: &str = "mossen-render:approval-decision:";
const FINAL_SUMMARY_PREFIX: &str = "mossen-render:final-summary:";
const RAW_ENGINE_EVENT_PAYLOAD_PREVIEW_LIMIT: usize = 4096;
pub const RENDER_SESSION_SNAPSHOT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq)]
pub struct AssistantContentFacts {
    pub text: String,
    pub tool_uses: Vec<AssistantToolUseFacts>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AssistantToolUseFacts {
    pub id: String,
    pub name: String,
    pub input: Value,
}

pub fn assistant_content_facts(message: &AssistantMessage) -> AssistantContentFacts {
    let mut text = String::new();
    let mut tool_uses = Vec::new();

    for block in &message.content {
        match block {
            ContentBlock::Text(block) => text.push_str(&block.text),
            ContentBlock::ToolUse(block) => tool_uses.push(AssistantToolUseFacts {
                id: block.id.clone(),
                name: block.name.clone(),
                input: block.input.clone(),
            }),
            _ => {}
        }
    }

    AssistantContentFacts { text, tool_uses }
}

pub fn system_transcript_message(content: impl Into<String>, is_error: bool) -> MessageData {
    MessageData {
        message_type: MessageType::System,
        content: content.into(),
        timestamp: None,
        is_streaming: false,
        tool_name: None,
        is_error,
        thinking: None,
        thinking_completed_at: None,
        full_content: None,
        expanded: false,
    }
}

pub fn user_transcript_message(content: impl Into<String>) -> MessageData {
    MessageData {
        message_type: MessageType::User,
        content: content.into(),
        timestamp: None,
        is_streaming: false,
        tool_name: None,
        is_error: false,
        thinking: None,
        thinking_completed_at: None,
        full_content: None,
        expanded: false,
    }
}

pub fn command_output_transcript_message(
    command_name: &str,
    content: impl Into<String>,
    is_error: bool,
) -> MessageData {
    MessageData {
        message_type: MessageType::CommandOutput,
        content: format!("/{command_name}\n{}", content.into()),
        timestamp: None,
        is_streaming: false,
        tool_name: None,
        is_error,
        thinking: None,
        thinking_completed_at: None,
        full_content: None,
        expanded: false,
    }
}

pub fn skill_invocation_transcript_message(
    skill_name: &str,
    source: &str,
    preview: &str,
) -> MessageData {
    MessageData {
        message_type: MessageType::SkillInvocation,
        content: format!("/{skill_name}  ({source})\nresolving template:\n{preview}"),
        timestamp: None,
        is_streaming: false,
        tool_name: None,
        is_error: false,
        thinking: None,
        thinking_completed_at: None,
        full_content: None,
        expanded: false,
    }
}

pub fn cancelled_transcript_message() -> MessageData {
    system_transcript_message("↯ Cancelled", false)
}

pub fn unknown_command_transcript_message(command: &str) -> MessageData {
    system_transcript_message(format!("Unknown command: /{command}"), true)
}

pub fn final_summary_transcript_message(model: &FinalSummaryModel) -> MessageData {
    system_transcript_message(final_summary_message_content(model), !model.success)
}

#[derive(Debug, Clone)]
pub struct AssistantTranscriptFacts {
    pub parent_id: Option<String>,
    pub message: MessageData,
}

#[derive(Debug, Clone)]
pub struct ToolUseTranscriptFacts {
    pub record_id: String,
    pub parent_id: Option<String>,
    pub message: MessageData,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingAssistantFinalization {
    Remove,
    Keep,
}

pub fn assistant_transcript_message(
    content: String,
    thinking: Option<String>,
    is_streaming: bool,
) -> MessageData {
    MessageData {
        message_type: MessageType::Assistant,
        content,
        timestamp: None,
        is_streaming,
        tool_name: None,
        is_error: false,
        thinking,
        thinking_completed_at: (!is_streaming).then(Instant::now),
        full_content: None,
        expanded: false,
    }
}

pub fn pending_assistant_transcript_message() -> MessageData {
    assistant_transcript_message(String::new(), None, true)
}

pub fn task_assistant_transcript_facts(
    task_id: &str,
    content: String,
    thinking: Option<String>,
) -> AssistantTranscriptFacts {
    AssistantTranscriptFacts {
        parent_id: Some(task_record_id(task_id)),
        message: assistant_transcript_message(format!("│ {task_id}\n{content}"), thinking, false),
    }
}

pub fn tool_use_transcript_facts(
    task_id: Option<&str>,
    tool_use_id: &str,
    tool_name: &str,
    preview: String,
    full_content: Option<String>,
) -> ToolUseTranscriptFacts {
    let task_id = task_id.and_then(non_empty_str);
    let content = if let Some(task_id) = task_id {
        format!("agent  {task_id}\n{preview}")
    } else {
        preview
    };

    ToolUseTranscriptFacts {
        record_id: scoped_tool_record_id(task_id, tool_use_id),
        parent_id: task_id.map(task_record_id),
        message: MessageData {
            message_type: MessageType::ToolUse,
            content,
            timestamp: None,
            is_streaming: false,
            tool_name: Some(tool_name.to_string()),
            is_error: false,
            thinking: None,
            thinking_completed_at: None,
            full_content,
            expanded: false,
        },
    }
}

pub fn finalize_pending_assistant_transcript_message(
    message: &mut MessageData,
    terminal: Option<&str>,
) -> PendingAssistantFinalization {
    if assistant_transcript_is_empty(message) {
        return PendingAssistantFinalization::Remove;
    }

    message.is_streaming = false;
    message.thinking_completed_at = Some(Instant::now());
    if message.content.trim().is_empty() {
        if let Some(terminal) = terminal.and_then(non_empty_str) {
            if terminal != "Completed" {
                message.content = format!("({terminal})");
            }
        }
    }

    PendingAssistantFinalization::Keep
}

fn assistant_transcript_is_empty(message: &MessageData) -> bool {
    message.content.trim().is_empty()
        && message
            .thinking
            .as_deref()
            .map(str::trim)
            .map(str::is_empty)
            .unwrap_or(true)
}

#[derive(Debug, Clone)]
pub struct ToolSummaryTranscriptFacts {
    pub record_id: Option<String>,
    pub parent_id: Option<String>,
    pub message: MessageData,
}

pub fn tool_summary_transcript_facts(
    task_id: Option<&str>,
    tool_name: &str,
    summary: &str,
    full_content: Option<&str>,
    tool_use_id: Option<&str>,
    latest_tool_parent_id: Option<String>,
) -> ToolSummaryTranscriptFacts {
    let tool_parent_id = tool_use_id
        .and_then(non_empty_str)
        .map(|id| scoped_tool_record_id(task_id, id))
        .or(latest_tool_parent_id);
    let record_id = tool_parent_id.as_deref().map(tool_result_record_id);
    let parent_id = tool_parent_id.or_else(|| task_id.and_then(non_empty_str).map(task_record_id));

    ToolSummaryTranscriptFacts {
        record_id,
        parent_id,
        message: MessageData {
            message_type: MessageType::ToolResult,
            content: summary.to_string(),
            timestamp: None,
            is_streaming: false,
            tool_name: Some(tool_name.to_string()),
            is_error: false,
            thinking: None,
            thinking_completed_at: None,
            full_content: full_content.map(str::to_string),
            expanded: false,
        },
    }
}

#[derive(Debug, Clone)]
pub struct CompactBoundaryTranscriptFacts {
    pub progress: String,
    pub message: MessageData,
}

pub fn compact_boundary_transcript_facts(
    before_token_count: u64,
    after_token_count: u64,
) -> CompactBoundaryTranscriptFacts {
    CompactBoundaryTranscriptFacts {
        progress: format!("Tokens {before_token_count} -> {after_token_count}"),
        message: MessageData {
            message_type: MessageType::System,
            content: format!("(compact) tokens {before_token_count} -> {after_token_count}"),
            timestamp: None,
            is_streaming: false,
            tool_name: None,
            is_error: false,
            thinking: None,
            thinking_completed_at: None,
            full_content: None,
            expanded: false,
        },
    }
}

pub fn api_retry_transcript_message(
    error: &str,
    attempt: u32,
    max_retries: u32,
    retry_in_ms: u64,
) -> MessageData {
    MessageData {
        message_type: MessageType::System,
        content: format!("API retry {attempt}/{max_retries} in {retry_in_ms}ms: {error}"),
        timestamp: None,
        is_streaming: false,
        tool_name: None,
        is_error: true,
        thinking: None,
        thinking_completed_at: None,
        full_content: None,
        expanded: false,
    }
}

#[derive(Debug, Clone)]
pub struct TaskProgressTranscriptFacts {
    pub record_id: String,
    pub parent_id: Option<String>,
    pub message: MessageData,
}

pub fn task_started_transcript_facts(task_id: &str, model: &str) -> TaskProgressTranscriptFacts {
    TaskProgressTranscriptFacts {
        record_id: task_record_id(task_id),
        parent_id: None,
        message: progress_message(format!("│ {task_id} started ({model})")),
    }
}

pub fn task_completed_transcript_facts(
    task_id: &str,
    terminal: &str,
) -> TaskProgressTranscriptFacts {
    TaskProgressTranscriptFacts {
        record_id: task_result_record_id(task_id),
        parent_id: Some(task_record_id(task_id)),
        message: progress_message(format!("│ {task_id} completed ({terminal})")),
    }
}

pub fn exceptional_stop_reason_transcript_message(reason: &str) -> Option<MessageData> {
    let reason = reason.trim();
    if reason.is_empty() || matches!(reason, "end_turn" | "tool_use") {
        None
    } else {
        Some(progress_message(format!("(stop: {reason})")))
    }
}

pub fn scoped_tool_record_id(task_id: Option<&str>, tool_use_id: &str) -> String {
    if let Some(task_id) = task_id.and_then(non_empty_str) {
        let prefix = format!("{task_id}:");
        if tool_use_id.starts_with(&prefix) {
            tool_use_id.to_string()
        } else {
            format!("{task_id}:{tool_use_id}")
        }
    } else {
        tool_use_id.to_string()
    }
}

pub fn task_record_id(task_id: &str) -> String {
    format!("task:{task_id}")
}

pub fn task_result_record_id(task_id: &str) -> String {
    format!("{}:result", task_record_id(task_id))
}

fn tool_result_record_id(parent_id: &str) -> String {
    format!("{parent_id}:result")
}

fn progress_message(content: String) -> MessageData {
    MessageData {
        message_type: MessageType::Progress,
        content,
        timestamp: None,
        is_streaming: false,
        tool_name: None,
        is_error: false,
        thinking: None,
        thinking_completed_at: None,
        full_content: None,
        expanded: false,
    }
}

fn non_empty_str(value: &str) -> Option<&str> {
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderSessionSnapshot {
    pub version: u32,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub current_turn_id: Option<String>,
    #[serde(default)]
    pub latest_turn_id: Option<String>,
    pub next_render_record_seq: u64,
    pub next_render_turn_seq: u64,
    pub next_raw_engine_event_seq: u64,
    pub records: TranscriptRecords,
    #[serde(default)]
    pub raw_engine_events: Vec<RawEngineEventRecord>,
}

impl RenderSessionSnapshot {
    pub fn new(
        session_id: Option<String>,
        current_turn_id: Option<String>,
        latest_turn_id: Option<String>,
        next_render_record_seq: u64,
        next_render_turn_seq: u64,
        next_raw_engine_event_seq: u64,
        records: TranscriptRecords,
        raw_engine_events: Vec<RawEngineEventRecord>,
    ) -> Self {
        Self {
            version: RENDER_SESSION_SNAPSHOT_VERSION,
            session_id,
            current_turn_id,
            latest_turn_id,
            next_render_record_seq,
            next_render_turn_seq,
            next_raw_engine_event_seq,
            records,
            raw_engine_events,
        }
    }

    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string(self)
    }

    pub fn to_json_pretty(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(payload: &str) -> serde_json::Result<Self> {
        serde_json::from_str(payload)
    }

    pub fn save_json_file(&self, path: impl AsRef<Path>) -> io::Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }

        let payload = self.to_json_pretty().map_err(json_to_io_error)?;
        let tmp_path = render_session_snapshot_tmp_path(path);
        {
            let mut file = File::create(&tmp_path)?;
            file.write_all(payload.as_bytes())?;
            file.write_all(b"\n")?;
            file.sync_all()?;
        }
        fs::rename(&tmp_path, path).inspect_err(|error| {
            let _ = fs::remove_file(&tmp_path);
        })
    }

    pub fn load_json_file(path: impl AsRef<Path>) -> io::Result<Self> {
        let payload = fs::read_to_string(path)?;
        Self::from_json(&payload).map_err(json_to_io_error)
    }

    pub fn record_count(&self) -> usize {
        self.records.entries.len()
    }

    pub fn raw_event_count(&self) -> usize {
        self.raw_engine_events.len()
    }
}

fn json_to_io_error(error: serde_json::Error) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

fn render_session_snapshot_tmp_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("render-session.snapshot.json");
    path.with_file_name(format!(".{file_name}.tmp"))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawEngineEventRecord {
    pub sequence: u64,
    #[serde(default)]
    pub turn_id: Option<String>,
    #[serde(default)]
    pub task_id: Option<String>,
    pub kind: RawEngineEventKind,
    pub summary: String,
    pub payload_preview: String,
}

impl RawEngineEventRecord {
    pub fn from_sdk_message(sequence: u64, turn_id: Option<String>, message: &SdkMessage) -> Self {
        Self {
            sequence,
            turn_id,
            task_id: message.task_id().map(ToOwned::to_owned),
            kind: RawEngineEventKind::from_sdk_message(message),
            summary: raw_engine_event_summary(message),
            payload_preview: raw_engine_event_payload_preview(message),
        }
    }

    pub fn scope_label(&self) -> String {
        self.task_id
            .as_deref()
            .map(|task_id| format!("task:{task_id}"))
            .unwrap_or_else(|| "main".to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RawEngineEventKind {
    SystemInit,
    Assistant,
    User,
    StreamEvent,
    Result,
    ToolUseSummary,
    CompactBoundary,
    CompactRequestStatus,
    ConversationCleared,
    ClearRequestStatus,
    ApiRetry,
    ThreadGoalUpdated,
    ThreadGoalCleared,
}

impl RawEngineEventKind {
    pub fn as_str(self) -> &'static str {
        match self {
            RawEngineEventKind::SystemInit => "system_init",
            RawEngineEventKind::Assistant => "assistant",
            RawEngineEventKind::User => "user",
            RawEngineEventKind::StreamEvent => "stream_event",
            RawEngineEventKind::Result => "result",
            RawEngineEventKind::ToolUseSummary => "tool_use_summary",
            RawEngineEventKind::CompactBoundary => "compact_boundary",
            RawEngineEventKind::CompactRequestStatus => "compact_request_status",
            RawEngineEventKind::ConversationCleared => "conversation_cleared",
            RawEngineEventKind::ClearRequestStatus => "clear_request_status",
            RawEngineEventKind::ApiRetry => "api_retry",
            RawEngineEventKind::ThreadGoalUpdated => "thread_goal_updated",
            RawEngineEventKind::ThreadGoalCleared => "thread_goal_cleared",
        }
    }

    fn from_sdk_message(message: &SdkMessage) -> Self {
        match message {
            SdkMessage::SystemInit { .. } => RawEngineEventKind::SystemInit,
            SdkMessage::Assistant { .. } => RawEngineEventKind::Assistant,
            SdkMessage::User { .. } => RawEngineEventKind::User,
            SdkMessage::StreamEvent { .. } => RawEngineEventKind::StreamEvent,
            SdkMessage::Result { .. } => RawEngineEventKind::Result,
            SdkMessage::ToolUseSummary { .. } => RawEngineEventKind::ToolUseSummary,
            SdkMessage::CompactBoundary { .. } => RawEngineEventKind::CompactBoundary,
            SdkMessage::CompactRequestStatus { .. } => RawEngineEventKind::CompactRequestStatus,
            SdkMessage::ConversationCleared { .. } => RawEngineEventKind::ConversationCleared,
            SdkMessage::ClearRequestStatus { .. } => RawEngineEventKind::ClearRequestStatus,
            SdkMessage::ApiRetry { .. } => RawEngineEventKind::ApiRetry,
            SdkMessage::ThreadGoalUpdated { .. } => RawEngineEventKind::ThreadGoalUpdated,
            SdkMessage::ThreadGoalCleared { .. } => RawEngineEventKind::ThreadGoalCleared,
        }
    }
}

fn raw_engine_event_summary(message: &SdkMessage) -> String {
    match message {
        SdkMessage::SystemInit {
            session_id,
            model,
            tools,
            ..
        } => format!(
            "session={} model={} tools={}",
            preview_token(session_id, 48),
            preview_token(model, 48),
            tools.len()
        ),
        SdkMessage::Assistant { message, usage, .. } => {
            let mut text_blocks = 0usize;
            let mut thinking_blocks = 0usize;
            let mut tool_uses = 0usize;
            for block in &message.content {
                match block {
                    ContentBlock::Text(_) => text_blocks += 1,
                    ContentBlock::Thinking(_) => thinking_blocks += 1,
                    ContentBlock::ToolUse(_) => tool_uses += 1,
                    _ => {}
                }
            }
            let usage = usage
                .as_ref()
                .map(|usage| format!(" usage={}/{}", usage.input_tokens, usage.output_tokens))
                .unwrap_or_default();
            format!(
                "blocks={} text={} thinking={} tool_uses={} stop={}{}",
                message.content.len(),
                text_blocks,
                thinking_blocks,
                tool_uses,
                message.stop_reason.as_deref().unwrap_or("-"),
                usage
            )
        }
        SdkMessage::User { message, .. } => {
            format!(
                "blocks={} meta={} origin={}",
                message.content.len(),
                message
                    .is_meta
                    .map(|is_meta| is_meta.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                message
                    .origin
                    .as_ref()
                    .map(|origin| format!("{origin:?}"))
                    .unwrap_or_else(|| "-".to_string())
            )
        }
        SdkMessage::StreamEvent { event, .. } => stream_event_summary(event),
        SdkMessage::Result {
            terminal,
            cost_usd,
            duration_ms,
            usage,
            ..
        } => {
            let cost = cost_usd
                .map(|cost| format!("{cost:.6}"))
                .unwrap_or_else(|| "-".to_string());
            let duration = duration_ms
                .map(|duration| duration.to_string())
                .unwrap_or_else(|| "-".to_string());
            let usage = usage
                .as_ref()
                .map(|usage| format!(" usage={}/{}", usage.input_tokens, usage.output_tokens))
                .unwrap_or_default();
            format!(
                "terminal={} cost={} duration_ms={}{}",
                preview_token(terminal, 48),
                cost,
                duration,
                usage
            )
        }
        SdkMessage::ToolUseSummary {
            tool_name,
            tool_use_id,
            summary,
            full_content,
            ..
        } => format!(
            "tool={} tool_use_id={} summary_chars={} full_chars={}",
            preview_token(tool_name, 48),
            tool_use_id.as_deref().unwrap_or("-"),
            summary.chars().count(),
            full_content
                .as_deref()
                .map(|content| content.chars().count())
                .unwrap_or(0)
        ),
        SdkMessage::CompactBoundary {
            before_token_count,
            after_token_count,
            ..
        } => format!("tokens {} -> {}", before_token_count, after_token_count),
        SdkMessage::CompactRequestStatus {
            request_id,
            status,
            dry_run,
            reason,
            ..
        } => {
            let reason = reason
                .as_deref()
                .map(|reason| format!(" reason={}", preview_token(reason, 96)))
                .unwrap_or_default();
            format!(
                "request={} status={} dry_run={}{}",
                preview_token(request_id, 48),
                status.as_str(),
                dry_run,
                reason
            )
        }
        SdkMessage::ConversationCleared {
            message_count_before,
            message_count_after,
            ..
        } => format!(
            "messages {} -> {}",
            message_count_before, message_count_after
        ),
        SdkMessage::ClearRequestStatus {
            request_id,
            status,
            dry_run,
            reason,
            ..
        } => {
            let reason = reason
                .as_deref()
                .map(|reason| format!(" reason={}", preview_token(reason, 96)))
                .unwrap_or_default();
            format!(
                "request={} status={} dry_run={}{}",
                preview_token(request_id, 48),
                status.as_str(),
                dry_run,
                reason
            )
        }
        SdkMessage::ApiRetry {
            error,
            attempt,
            max_retries,
            retry_in_ms,
            ..
        } => format!(
            "attempt={}/{} retry_in_ms={} error={}",
            attempt,
            max_retries,
            retry_in_ms,
            preview_token(error, 96)
        ),
        SdkMessage::ThreadGoalUpdated {
            thread_id,
            turn_id,
            goal,
            ..
        } => format!(
            "thread={} turn={} status={} objective_chars={}",
            preview_token(thread_id, 48),
            turn_id.as_deref().unwrap_or("-"),
            mossen_agent::goal::goal_status_label(goal.status),
            goal.objective.chars().count()
        ),
        SdkMessage::ThreadGoalCleared { thread_id, .. } => {
            format!("thread={}", preview_token(thread_id, 48))
        }
    }
}

fn stream_event_summary(event: &StreamEventData) -> String {
    match event {
        StreamEventData::ContentBlockStart { index } => {
            format!("content_block_start index={index}")
        }
        StreamEventData::ContentBlockDelta { index, delta } => match delta {
            ContentDelta::TextDelta { text } => {
                format!(
                    "content_block_delta index={index} text_bytes={}",
                    text.len()
                )
            }
            ContentDelta::ThinkingDelta { thinking } => format!(
                "content_block_delta index={index} thinking_bytes={}",
                thinking.len()
            ),
            ContentDelta::InputJsonDelta { partial_json } => format!(
                "content_block_delta index={index} input_json_bytes={}",
                partial_json.len()
            ),
        },
        StreamEventData::ContentBlockStop { index } => format!("content_block_stop index={index}"),
        StreamEventData::MessageStart => "message_start".to_string(),
        StreamEventData::MessageDelta { usage, stop_reason } => {
            let usage = usage
                .as_ref()
                .map(|usage| format!("{}/{}", usage.input_tokens, usage.output_tokens))
                .unwrap_or_else(|| "-".to_string());
            format!(
                "message_delta usage={} stop={}",
                usage,
                stop_reason.as_deref().unwrap_or("-")
            )
        }
        StreamEventData::MessageStop => "message_stop".to_string(),
    }
}

fn raw_engine_event_payload_preview(message: &SdkMessage) -> String {
    let payload = serde_json::to_string(message).unwrap_or_else(|_| format!("{message:?}"));
    truncate_chars(&payload, RAW_ENGINE_EVENT_PAYLOAD_PREVIEW_LIMIT)
}

fn preview_token(value: &str, limit: usize) -> String {
    truncate_chars(value, limit)
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

fn truncate_chars(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        return value.to_string();
    }
    let mut truncated: String = value.chars().take(limit).collect();
    truncated.push_str("...");
    truncated
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptRelationIndex {
    #[serde(default)]
    pub roots: Vec<String>,
    #[serde(default)]
    pub parents_by_child: BTreeMap<String, String>,
    #[serde(default)]
    pub children_by_parent: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub missing_parent_ids: Vec<String>,
}

impl TranscriptRelationIndex {
    pub fn parented_count(&self) -> usize {
        self.parents_by_child.len()
    }

    pub fn parent_count(&self) -> usize {
        self.children_by_parent.len()
    }

    pub fn orphan_count(&self) -> usize {
        self.missing_parent_ids.len()
    }

    pub fn children_of(&self, parent_id: &str) -> Option<&[String]> {
        self.children_by_parent.get(parent_id).map(Vec::as_slice)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptRecords {
    #[serde(default)]
    pub entries: Vec<TranscriptRecord>,
    #[serde(default)]
    pub approval_decisions: Vec<ApprovalDecisionModel>,
    #[serde(default)]
    pub final_summaries: Vec<FinalSummaryRecord>,
}

impl TranscriptRecords {
    pub fn from_messages(messages: &[MessageData]) -> Self {
        Self::from_messages_and_decisions(messages, &[])
    }

    pub fn from_messages_and_decisions(
        messages: &[MessageData],
        decisions: &[ApprovalDecisionModel],
    ) -> Self {
        Self::from_messages_and_decisions_with_record_ids(messages, decisions, &HashMap::new())
    }

    pub fn from_messages_and_decisions_with_record_ids(
        messages: &[MessageData],
        decisions: &[ApprovalDecisionModel],
        record_ids: &HashMap<usize, String>,
    ) -> Self {
        Self::from_messages_and_decisions_with_record_metadata(
            messages,
            decisions,
            record_ids,
            &HashMap::new(),
        )
    }

    pub fn from_messages_and_decisions_with_record_metadata(
        messages: &[MessageData],
        decisions: &[ApprovalDecisionModel],
        record_ids: &HashMap<usize, String>,
        parent_ids: &HashMap<usize, String>,
    ) -> Self {
        Self::from_messages_and_decisions_with_full_record_metadata(
            messages,
            decisions,
            record_ids,
            parent_ids,
            &HashMap::new(),
        )
    }

    pub fn from_messages_and_decisions_with_full_record_metadata(
        messages: &[MessageData],
        decisions: &[ApprovalDecisionModel],
        record_ids: &HashMap<usize, String>,
        parent_ids: &HashMap<usize, String>,
        turn_ids: &HashMap<usize, String>,
    ) -> Self {
        let mut entries = Vec::new();
        let mut approval_decisions = Vec::new();
        let mut final_summaries = Vec::new();

        for (index, message) in messages.iter().enumerate() {
            if let Some(mut decision) = approval_decision_from_message(message) {
                if decision.id.is_empty() {
                    decision.id = format!("legacy-approval-decision-{index}");
                }
                approval_decisions.push(decision);
                continue;
            }
            if let Some(mut summary) = final_summary_from_message(message) {
                if summary.id.is_empty() {
                    summary.id = format!("legacy-final-summary-{index}");
                }
                final_summaries.push(FinalSummaryRecord {
                    source_index: index,
                    model: summary,
                });
                continue;
            }

            let mut record = TranscriptRecord::from_message(index, message);
            if let Some(id) = record_ids.get(&index).filter(|id| !id.is_empty()) {
                record.id = id.clone();
            }
            if let Some(parent_id) = parent_ids.get(&index).filter(|id| !id.is_empty()) {
                record.parent_id = Some(parent_id.clone());
            }
            if let Some(turn_id) = turn_ids.get(&index).filter(|id| !id.is_empty()) {
                record.turn_id = Some(turn_id.clone());
            }
            entries.push(record);
        }

        approval_decisions.extend(decisions.iter().cloned());

        Self {
            entries,
            approval_decisions,
            final_summaries,
        }
    }

    pub fn relation_index(&self) -> TranscriptRelationIndex {
        let known_ids: BTreeSet<&str> = self
            .entries
            .iter()
            .filter_map(|record| {
                if record.id.is_empty() {
                    None
                } else {
                    Some(record.id.as_str())
                }
            })
            .collect();
        let mut roots = Vec::new();
        let mut parents_by_child = BTreeMap::new();
        let mut children_by_parent: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut missing_parent_ids = BTreeSet::new();

        for record in &self.entries {
            if record.id.is_empty() {
                continue;
            }
            let Some(parent_id) = record.parent_id.as_deref().filter(|id| !id.is_empty()) else {
                roots.push(record.id.clone());
                continue;
            };

            parents_by_child.insert(record.id.clone(), parent_id.to_string());
            children_by_parent
                .entry(parent_id.to_string())
                .or_default()
                .push(record.id.clone());
            if !known_ids.contains(parent_id) {
                missing_parent_ids.insert(parent_id.to_string());
            }
        }

        TranscriptRelationIndex {
            roots,
            parents_by_child,
            children_by_parent,
            missing_parent_ids: missing_parent_ids.into_iter().collect(),
        }
    }

    pub fn record_by_id(&self, record_id: &str) -> Option<&TranscriptRecord> {
        self.entries.iter().find(|record| record.id == record_id)
    }

    pub fn child_records(&self, parent_id: &str) -> Vec<&TranscriptRecord> {
        self.entries
            .iter()
            .filter(|record| record.parent_id.as_deref() == Some(parent_id))
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptRecord {
    pub id: String,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub turn_id: Option<String>,
    pub source_index: usize,
    pub kind: TranscriptRecordKind,
    pub lifecycle: LifecyclePhase,
    pub content: String,
    #[serde(default)]
    pub timestamp: Option<String>,
    #[serde(default)]
    pub tool_name: Option<String>,
    pub is_streaming: bool,
    pub is_error: bool,
    #[serde(default)]
    pub thinking: Option<String>,
    pub thinking_completed: bool,
    #[serde(default)]
    pub full_content: Option<String>,
    pub expanded: bool,
}

impl TranscriptRecord {
    fn from_message(index: usize, message: &MessageData) -> Self {
        let id = match message.message_type {
            MessageType::ToolUse => format!("tool-{index}"),
            MessageType::ToolResult => format!("tool-result-{index}"),
            _ => format!("message-{index}"),
        };
        Self {
            id,
            parent_id: None,
            turn_id: None,
            source_index: index,
            kind: TranscriptRecordKind::from_message_type(message.message_type),
            lifecycle: LifecyclePhase::from_message(message),
            content: message.content.clone(),
            timestamp: message.timestamp.clone(),
            tool_name: message.tool_name.clone(),
            is_streaming: message.is_streaming,
            is_error: message.is_error,
            thinking: message.thinking.clone(),
            thinking_completed: message.thinking_completed_at.is_some(),
            full_content: message.full_content.clone(),
            expanded: message.expanded,
        }
    }

    pub fn to_message_data(&self) -> MessageData {
        MessageData {
            message_type: self.kind.to_message_type(),
            content: self.content.clone(),
            timestamp: self.timestamp.clone(),
            is_streaming: self.is_streaming,
            tool_name: self.tool_name.clone(),
            is_error: self.is_error,
            thinking: self.thinking.clone(),
            thinking_completed_at: None,
            full_content: self.full_content.clone(),
            expanded: self.expanded,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptRecordKind {
    User,
    Assistant,
    System,
    CommandOutput,
    Progress,
    Attachment,
    ToolUse,
    ToolResult,
    SkillInvocation,
}

impl TranscriptRecordKind {
    fn from_message_type(message_type: MessageType) -> Self {
        match message_type {
            MessageType::User => Self::User,
            MessageType::Assistant => Self::Assistant,
            MessageType::System => Self::System,
            MessageType::CommandOutput => Self::CommandOutput,
            MessageType::Progress => Self::Progress,
            MessageType::Attachment => Self::Attachment,
            MessageType::ToolUse => Self::ToolUse,
            MessageType::ToolResult => Self::ToolResult,
            MessageType::SkillInvocation => Self::SkillInvocation,
        }
    }

    fn to_message_type(self) -> MessageType {
        match self {
            Self::User => MessageType::User,
            Self::Assistant => MessageType::Assistant,
            Self::System => MessageType::System,
            Self::CommandOutput => MessageType::CommandOutput,
            Self::Progress => MessageType::Progress,
            Self::Attachment => MessageType::Attachment,
            Self::ToolUse => MessageType::ToolUse,
            Self::ToolResult => MessageType::ToolResult,
            Self::SkillInvocation => MessageType::SkillInvocation,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecyclePhase {
    Queued,
    Streaming,
    RunningTool,
    Completed,
    Failed,
    Cancelled,
    TimedOut,
}

impl LifecyclePhase {
    fn from_message(message: &MessageData) -> Self {
        if message.is_error {
            Self::Failed
        } else if message.is_streaming {
            Self::Streaming
        } else if matches!(message.message_type, MessageType::ToolUse) {
            Self::RunningTool
        } else {
            Self::Completed
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ApprovalDecisionModel {
    #[serde(default)]
    pub id: String,
    pub tool_name: String,
    pub decision: ApprovalDecisionKind,
    pub detail: String,
    pub anchor_block_id: Option<String>,
}

impl ApprovalDecisionModel {
    pub fn line(&self) -> String {
        let label = self.decision.label();
        if self.detail.trim().is_empty() {
            format!("{label} {}", self.tool_name)
        } else {
            format!("{label} {} · {}", self.tool_name, self.detail)
        }
    }

    pub fn is_error(&self) -> bool {
        matches!(
            self.decision,
            ApprovalDecisionKind::Denied | ApprovalDecisionKind::Cancelled
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecisionKind {
    Allowed,
    AlwaysAllowed,
    Denied,
    Cancelled,
}

impl ApprovalDecisionKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Allowed => "Allowed",
            Self::AlwaysAllowed => "Always allowed",
            Self::Denied => "Denied",
            Self::Cancelled => "Cancelled",
        }
    }

    fn as_payload(self) -> &'static str {
        match self {
            Self::Allowed => "allowed",
            Self::AlwaysAllowed => "always_allowed",
            Self::Denied => "denied",
            Self::Cancelled => "cancelled",
        }
    }

    fn from_payload(value: &str) -> Option<Self> {
        match value {
            "allowed" => Some(Self::Allowed),
            "always_allowed" => Some(Self::AlwaysAllowed),
            "denied" => Some(Self::Denied),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ApprovalDecisionPayload {
    #[serde(default)]
    id: String,
    tool_name: String,
    decision: String,
    detail: String,
    anchor_block_id: Option<String>,
}

pub fn approval_decision_message_content(model: &ApprovalDecisionModel) -> String {
    let payload = ApprovalDecisionPayload {
        id: model.id.clone(),
        tool_name: model.tool_name.clone(),
        decision: model.decision.as_payload().to_string(),
        detail: model.detail.clone(),
        anchor_block_id: model.anchor_block_id.clone(),
    };
    format!(
        "{}{}",
        APPROVAL_DECISION_PREFIX,
        serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string())
    )
}

pub fn approval_decision_from_message(message: &MessageData) -> Option<ApprovalDecisionModel> {
    let payload = message.content.strip_prefix(APPROVAL_DECISION_PREFIX)?;
    let payload = serde_json::from_str::<ApprovalDecisionPayload>(payload).ok()?;
    Some(ApprovalDecisionModel {
        id: payload.id,
        tool_name: payload.tool_name,
        decision: ApprovalDecisionKind::from_payload(&payload.decision)?,
        detail: payload.detail,
        anchor_block_id: payload.anchor_block_id,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FinalSummaryRecord {
    pub source_index: usize,
    pub model: FinalSummaryModel,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FinalSummaryModel {
    #[serde(default)]
    pub id: String,
    pub success: bool,
    pub terminal: String,
    #[serde(default)]
    pub changed_files: Vec<FileChangeSummaryModel>,
    #[serde(default)]
    pub commands: Vec<CommandSummaryModel>,
    #[serde(default)]
    pub verification_results: Vec<VerificationSummaryModel>,
    #[serde(default)]
    pub residual_risks: Vec<String>,
    #[serde(default)]
    pub notes: Vec<String>,
}

impl FinalSummaryModel {
    pub fn needs_attention(&self) -> bool {
        !self.success
            || !self.residual_risks.is_empty()
            || self
                .verification_results
                .iter()
                .any(|result| !result.passed)
            || self
                .commands
                .iter()
                .any(|command| command.exit_code.is_some_and(|code| code != 0))
    }

    pub fn title(&self) -> &'static str {
        if self.needs_attention() {
            "Final Summary · Attention"
        } else {
            "Final Summary"
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FileChangeSummaryModel {
    pub path: String,
    pub status: String,
    pub additions: usize,
    pub deletions: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CommandSummaryModel {
    pub command: String,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub exit_code: Option<i64>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VerificationSummaryModel {
    pub command: String,
    pub status: String,
    pub passed: bool,
    #[serde(default)]
    pub exit_code: Option<i64>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
}

pub fn final_summary_message_content(model: &FinalSummaryModel) -> String {
    format!(
        "{}{}",
        FINAL_SUMMARY_PREFIX,
        serde_json::to_string(model).unwrap_or_else(|_| "{}".to_string())
    )
}

pub fn final_summary_from_message(message: &MessageData) -> Option<FinalSummaryModel> {
    let payload = message.content.strip_prefix(FINAL_SUMMARY_PREFIX)?;
    serde_json::from_str::<FinalSummaryModel>(payload).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mossen_types::{Role, TextBlock, ThinkingBlock, ToolUseBlock};

    fn message(message_type: MessageType, content: impl Into<String>) -> MessageData {
        MessageData {
            message_type,
            content: content.into(),
            timestamp: None,
            is_streaming: false,
            tool_name: None,
            is_error: false,
            thinking: None,
            thinking_completed_at: None,
            full_content: None,
            expanded: false,
        }
    }

    #[test]
    fn assistant_content_facts_extract_text_and_tool_uses_once() {
        let message = AssistantMessage {
            role: Role::Assistant,
            content: vec![
                ContentBlock::Text(TextBlock {
                    text: "hello ".to_string(),
                }),
                ContentBlock::Thinking(ThinkingBlock {
                    thinking: "private chain".to_string(),
                    signature: None,
                }),
                ContentBlock::Text(TextBlock {
                    text: "world".to_string(),
                }),
                ContentBlock::ToolUse(ToolUseBlock {
                    id: "toolu-1".to_string(),
                    name: "Bash".to_string(),
                    input: serde_json::json!({"command": "cargo test"}),
                }),
            ],
            uuid: None,
            model: None,
            stop_reason: Some("tool_use".to_string()),
            extra: HashMap::new(),
        };

        let facts = assistant_content_facts(&message);

        assert_eq!(facts.text, "hello world");
        assert_eq!(facts.tool_uses.len(), 1);
        assert_eq!(facts.tool_uses[0].id, "toolu-1");
        assert_eq!(facts.tool_uses[0].name, "Bash");
        assert_eq!(facts.tool_uses[0].input["command"], "cargo test");
    }

    #[test]
    fn basic_transcript_messages_format_user_system_command_skill_and_summary() {
        let user = user_transcript_message("hello");
        assert_eq!(user.message_type, MessageType::User);
        assert_eq!(user.content, "hello");
        assert!(!user.is_error);

        let system = system_transcript_message("warning", true);
        assert_eq!(system.message_type, MessageType::System);
        assert_eq!(system.content, "warning");
        assert!(system.is_error);

        let command = command_output_transcript_message("status", "ok", false);
        assert_eq!(command.message_type, MessageType::CommandOutput);
        assert_eq!(command.content, "/status\nok");
        assert!(!command.is_error);

        let skill = skill_invocation_transcript_message("review", "user", "scan files");
        assert_eq!(skill.message_type, MessageType::SkillInvocation);
        assert_eq!(
            skill.content,
            "/review  (user)\nresolving template:\nscan files"
        );

        let summary = final_summary_transcript_message(&FinalSummaryModel {
            id: "summary-1".to_string(),
            success: false,
            terminal: "Failed".to_string(),
            changed_files: Vec::new(),
            commands: Vec::new(),
            verification_results: Vec::new(),
            residual_risks: Vec::new(),
            notes: Vec::new(),
        });
        assert_eq!(summary.message_type, MessageType::System);
        assert!(summary.content.starts_with(FINAL_SUMMARY_PREFIX));
        assert!(summary.is_error);

        let cancelled = cancelled_transcript_message();
        assert_eq!(cancelled.message_type, MessageType::System);
        assert_eq!(cancelled.content, "↯ Cancelled");

        let unknown = unknown_command_transcript_message("wat");
        assert_eq!(unknown.message_type, MessageType::System);
        assert_eq!(unknown.content, "Unknown command: /wat");
        assert!(unknown.is_error);
    }

    #[test]
    fn assistant_transcript_facts_format_main_task_tool_and_pending_rows() {
        let pending = pending_assistant_transcript_message();
        assert_eq!(pending.message_type, MessageType::Assistant);
        assert_eq!(pending.content, "");
        assert!(pending.is_streaming);
        assert!(pending.thinking_completed_at.is_none());

        let final_message =
            assistant_transcript_message("answer".to_string(), Some("thinking".to_string()), false);
        assert_eq!(final_message.message_type, MessageType::Assistant);
        assert_eq!(final_message.content, "answer");
        assert_eq!(final_message.thinking.as_deref(), Some("thinking"));
        assert!(!final_message.is_streaming);
        assert!(final_message.thinking_completed_at.is_some());

        let task = task_assistant_transcript_facts(
            "agent-1",
            "checking".to_string(),
            Some("plan".to_string()),
        );
        assert_eq!(task.parent_id.as_deref(), Some("task:agent-1"));
        assert_eq!(task.message.message_type, MessageType::Assistant);
        assert_eq!(task.message.content, "│ agent-1\nchecking");
        assert_eq!(task.message.thinking.as_deref(), Some("plan"));

        let main_tool = tool_use_transcript_facts(
            None,
            "toolu-1",
            "Bash",
            "cargo test".to_string(),
            Some(r#"{"command":"cargo test"}"#.to_string()),
        );
        assert_eq!(main_tool.record_id, "toolu-1");
        assert_eq!(main_tool.parent_id, None);
        assert_eq!(main_tool.message.message_type, MessageType::ToolUse);
        assert_eq!(main_tool.message.content, "cargo test");
        assert_eq!(main_tool.message.tool_name.as_deref(), Some("Bash"));
        assert_eq!(
            main_tool.message.full_content.as_deref(),
            Some(r#"{"command":"cargo test"}"#)
        );

        let task_tool = tool_use_transcript_facts(
            Some("agent-1"),
            "toolu-1",
            "Bash",
            "cargo test".to_string(),
            None,
        );
        assert_eq!(task_tool.record_id, "agent-1:toolu-1");
        assert_eq!(task_tool.parent_id.as_deref(), Some("task:agent-1"));
        assert_eq!(task_tool.message.content, "agent  agent-1\ncargo test");
    }

    #[test]
    fn pending_assistant_finalization_owns_empty_and_terminal_rules() {
        let mut empty = pending_assistant_transcript_message();
        assert_eq!(
            finalize_pending_assistant_transcript_message(&mut empty, Some("Failed")),
            PendingAssistantFinalization::Remove
        );

        let mut thinking_only =
            assistant_transcript_message(String::new(), Some("thinking".to_string()), true);
        assert_eq!(
            finalize_pending_assistant_transcript_message(&mut thinking_only, Some("Failed")),
            PendingAssistantFinalization::Keep
        );
        assert!(!thinking_only.is_streaming);
        assert_eq!(thinking_only.content, "(Failed)");
        assert!(thinking_only.thinking_completed_at.is_some());

        let mut completed =
            assistant_transcript_message(String::new(), Some("thinking".to_string()), true);
        assert_eq!(
            finalize_pending_assistant_transcript_message(&mut completed, Some("Completed")),
            PendingAssistantFinalization::Keep
        );
        assert_eq!(completed.content, "");
    }

    #[test]
    fn tool_summary_transcript_facts_scope_parent_ids_and_message() {
        let facts = tool_summary_transcript_facts(
            Some("Agent:toolu-agent"),
            "Read",
            "read complete",
            Some("full file content"),
            Some("toolu-read"),
            None,
        );

        assert_eq!(
            facts.record_id.as_deref(),
            Some("Agent:toolu-agent:toolu-read:result")
        );
        assert_eq!(
            facts.parent_id.as_deref(),
            Some("Agent:toolu-agent:toolu-read")
        );
        assert_eq!(facts.message.message_type, MessageType::ToolResult);
        assert_eq!(facts.message.content, "read complete");
        assert_eq!(facts.message.tool_name.as_deref(), Some("Read"));
        assert_eq!(
            facts.message.full_content.as_deref(),
            Some("full file content")
        );

        let already_scoped = tool_summary_transcript_facts(
            Some("Agent:toolu-agent"),
            "Read",
            "ok",
            None,
            Some("Agent:toolu-agent:toolu-read"),
            None,
        );
        assert_eq!(
            already_scoped.parent_id.as_deref(),
            Some("Agent:toolu-agent:toolu-read")
        );

        let task_fallback = tool_summary_transcript_facts(
            Some("Agent:toolu-agent"),
            "Bash",
            "ok",
            None,
            None,
            None,
        );
        assert_eq!(
            task_fallback.parent_id.as_deref(),
            Some("task:Agent:toolu-agent")
        );
        assert_eq!(task_fallback.record_id, None);

        let latest_fallback = tool_summary_transcript_facts(
            None,
            "Bash",
            "ok",
            None,
            None,
            Some("toolu-latest".to_string()),
        );
        assert_eq!(latest_fallback.parent_id.as_deref(), Some("toolu-latest"));
        assert_eq!(
            latest_fallback.record_id.as_deref(),
            Some("toolu-latest:result")
        );
    }

    #[test]
    fn engine_notice_transcript_facts_format_compact_and_retry_rows() {
        let compact = compact_boundary_transcript_facts(1000, 320);

        assert_eq!(compact.progress, "Tokens 1000 -> 320");
        assert_eq!(compact.message.message_type, MessageType::System);
        assert_eq!(compact.message.content, "(compact) tokens 1000 -> 320");
        assert!(!compact.message.is_error);

        let retry = api_retry_transcript_message("rate limited", 2, 5, 1500);

        assert_eq!(retry.message_type, MessageType::System);
        assert_eq!(retry.content, "API retry 2/5 in 1500ms: rate limited");
        assert!(retry.is_error);
    }

    #[test]
    fn progress_transcript_facts_format_task_and_stop_rows() {
        let started = task_started_transcript_facts("agent-1", "test-model");

        assert_eq!(started.record_id, "task:agent-1");
        assert_eq!(started.parent_id, None);
        assert_eq!(started.message.message_type, MessageType::Progress);
        assert_eq!(started.message.content, "│ agent-1 started (test-model)");

        let completed = task_completed_transcript_facts("agent-1", "Completed");

        assert_eq!(completed.record_id, "task:agent-1:result");
        assert_eq!(completed.parent_id.as_deref(), Some("task:agent-1"));
        assert_eq!(completed.message.message_type, MessageType::Progress);
        assert_eq!(completed.message.content, "│ agent-1 completed (Completed)");

        assert_eq!(
            exceptional_stop_reason_transcript_message("max_tokens")
                .expect("exceptional stop reason should render")
                .content,
            "(stop: max_tokens)"
        );
        assert!(exceptional_stop_reason_transcript_message("tool_use").is_none());
        assert!(exceptional_stop_reason_transcript_message("end_turn").is_none());
    }

    #[test]
    fn extracts_legacy_approval_decisions_out_of_message_records() {
        let decision = ApprovalDecisionModel {
            id: "approval-1".to_string(),
            tool_name: "Bash".to_string(),
            decision: ApprovalDecisionKind::Allowed,
            detail: "cargo test".to_string(),
            anchor_block_id: Some("tool-0-1".to_string()),
        };
        let messages = vec![
            message(MessageType::User, "run tests"),
            message(
                MessageType::System,
                approval_decision_message_content(&decision),
            ),
        ];

        let records = TranscriptRecords::from_messages(&messages);

        assert_eq!(records.entries.len(), 1);
        assert_eq!(records.approval_decisions, vec![decision]);
    }

    #[test]
    fn assigns_lifecycle_without_terminal_layout() {
        let mut streaming = message(MessageType::Assistant, "hello");
        streaming.is_streaming = true;
        let mut failed = message(MessageType::ToolResult, "boom");
        failed.is_error = true;
        let records = TranscriptRecords::from_messages(&[streaming, failed]);

        assert_eq!(records.entries[0].lifecycle, LifecyclePhase::Streaming);
        assert_eq!(records.entries[1].lifecycle, LifecyclePhase::Failed);
    }

    #[test]
    fn applies_record_id_overrides_at_layer1_boundary() {
        let messages = vec![message(MessageType::ToolUse, "command\n  cargo test")];
        let record_ids = HashMap::from([(0usize, "toolu-engine-42".to_string())]);

        let records = TranscriptRecords::from_messages_and_decisions_with_record_ids(
            &messages,
            &[],
            &record_ids,
        );

        assert_eq!(records.entries[0].id, "toolu-engine-42");
        assert_eq!(records.entries[0].source_index, 0);
    }

    #[test]
    fn applies_parent_id_overrides_at_layer1_boundary() {
        let messages = vec![message(MessageType::ToolResult, "ok")];
        let record_ids = HashMap::from([(0usize, "toolu-engine-42:result".to_string())]);
        let parent_ids = HashMap::from([(0usize, "toolu-engine-42".to_string())]);

        let records = TranscriptRecords::from_messages_and_decisions_with_record_metadata(
            &messages,
            &[],
            &record_ids,
            &parent_ids,
        );

        assert_eq!(records.entries[0].id, "toolu-engine-42:result");
        assert_eq!(
            records.entries[0].parent_id.as_deref(),
            Some("toolu-engine-42")
        );
    }

    #[test]
    fn relation_index_groups_arbitrary_parent_child_records() {
        let messages = vec![
            message(MessageType::User, "inspect repo"),
            message(MessageType::Assistant, "starting task"),
            message(MessageType::ToolUse, "command\n  cargo test"),
            message(MessageType::ToolResult, "ok"),
            message(MessageType::Progress, "orphaned subtask event"),
        ];
        let record_ids = HashMap::from([
            (0usize, "turn-root".to_string()),
            (1usize, "assistant-step".to_string()),
            (2usize, "toolu-stable-1".to_string()),
            (3usize, "toolu-stable-1:result".to_string()),
            (4usize, "orphan-event".to_string()),
        ]);
        let parent_ids = HashMap::from([
            (1usize, "turn-root".to_string()),
            (2usize, "assistant-step".to_string()),
            (3usize, "toolu-stable-1".to_string()),
            (4usize, "missing-task-root".to_string()),
        ]);

        let records = TranscriptRecords::from_messages_and_decisions_with_record_metadata(
            &messages,
            &[],
            &record_ids,
            &parent_ids,
        );
        let relations = records.relation_index();

        assert_eq!(relations.roots, vec!["turn-root".to_string()]);
        assert_eq!(relations.parented_count(), 4);
        assert_eq!(relations.parent_count(), 4);
        assert_eq!(relations.orphan_count(), 1);
        assert_eq!(
            relations.children_of("assistant-step"),
            Some(vec!["toolu-stable-1".to_string()].as_slice())
        );
        assert_eq!(
            relations.children_of("toolu-stable-1"),
            Some(vec!["toolu-stable-1:result".to_string()].as_slice())
        );
        assert_eq!(
            records
                .record_by_id("toolu-stable-1")
                .and_then(|record| record.parent_id.as_deref()),
            Some("assistant-step")
        );
        assert_eq!(
            records
                .child_records("turn-root")
                .iter()
                .map(|record| record.id.as_str())
                .collect::<Vec<_>>(),
            vec!["assistant-step"]
        );
        assert_eq!(
            relations.missing_parent_ids,
            vec!["missing-task-root".to_string()]
        );
    }

    #[test]
    fn applies_turn_id_overrides_at_layer1_boundary() {
        let messages = vec![
            message(MessageType::User, "run tests"),
            message(MessageType::Assistant, "running"),
        ];
        let turn_ids = HashMap::from([
            (0usize, "turn-0001".to_string()),
            (1usize, "turn-0001".to_string()),
        ]);

        let records = TranscriptRecords::from_messages_and_decisions_with_full_record_metadata(
            &messages,
            &[],
            &HashMap::new(),
            &HashMap::new(),
            &turn_ids,
        );

        assert_eq!(records.entries[0].turn_id.as_deref(), Some("turn-0001"));
        assert_eq!(records.entries[1].turn_id.as_deref(), Some("turn-0001"));
    }

    #[test]
    fn raw_engine_event_record_preserves_sdk_message_identity() {
        let message = SdkMessage::ToolUseSummary {
            tool_name: "Bash".to_string(),
            tool_use_id: Some("toolu-raw-1".to_string()),
            summary: "ok".to_string(),
            full_content: Some("full output".to_string()),
            task_id: None,
        };

        let record =
            RawEngineEventRecord::from_sdk_message(7, Some("turn-0001".to_string()), &message);

        assert_eq!(record.sequence, 7);
        assert_eq!(record.turn_id.as_deref(), Some("turn-0001"));
        assert_eq!(record.scope_label(), "main");
        assert_eq!(record.kind, RawEngineEventKind::ToolUseSummary);
        assert!(record.summary.contains("tool=Bash"), "{record:?}");
        assert!(record.summary.contains("tool_use_id=toolu-raw-1"));
        assert!(
            record
                .payload_preview
                .contains("\"type\":\"tool_use_summary\""),
            "{}",
            record.payload_preview
        );
        assert!(record
            .payload_preview
            .contains("\"tool_use_id\":\"toolu-raw-1\""));
    }

    #[test]
    fn render_session_snapshot_roundtrips_layer1_records_and_events() {
        let messages = vec![
            message(MessageType::User, "run tests"),
            message(MessageType::Assistant, "running"),
        ];
        let turn_ids = HashMap::from([
            (0usize, "turn-0001".to_string()),
            (1usize, "turn-0001".to_string()),
        ]);
        let records = TranscriptRecords::from_messages_and_decisions_with_full_record_metadata(
            &messages,
            &[],
            &HashMap::new(),
            &HashMap::new(),
            &turn_ids,
        );
        let raw_event = RawEngineEventRecord::from_sdk_message(
            1,
            Some("turn-0001".to_string()),
            &SdkMessage::ApiRetry {
                error: "temporary".to_string(),
                attempt: 1,
                max_retries: 3,
                retry_in_ms: 250,
                task_id: None,
            },
        );
        let snapshot = RenderSessionSnapshot::new(
            Some("session-1".to_string()),
            None,
            Some("turn-0001".to_string()),
            4,
            1,
            1,
            records,
            vec![raw_event],
        );

        let payload = snapshot
            .to_json()
            .expect("render session snapshot should serialize");
        assert!(payload.contains("\"version\":1"), "{payload}");
        assert!(payload.contains("\"raw_engine_events\""), "{payload}");

        let restored = RenderSessionSnapshot::from_json(&payload)
            .expect("render session snapshot should deserialize");
        assert_eq!(restored, snapshot);
        assert_eq!(restored.record_count(), 2);
        assert_eq!(restored.raw_event_count(), 1);
        assert_eq!(
            restored.records.entries[0].turn_id.as_deref(),
            Some("turn-0001")
        );
        assert_eq!(
            restored.raw_engine_events[0].kind,
            RawEngineEventKind::ApiRetry
        );
    }

    #[test]
    fn render_session_snapshot_saves_and_loads_json_file() {
        let messages = vec![
            message(MessageType::User, "persist render session"),
            message(MessageType::Assistant, "persisted"),
        ];
        let turn_ids = HashMap::from([
            (0usize, "turn-0001".to_string()),
            (1usize, "turn-0001".to_string()),
        ]);
        let records = TranscriptRecords::from_messages_and_decisions_with_full_record_metadata(
            &messages,
            &[],
            &HashMap::new(),
            &HashMap::new(),
            &turn_ids,
        );
        let raw_event = RawEngineEventRecord::from_sdk_message(
            1,
            Some("turn-0001".to_string()),
            &SdkMessage::SystemInit {
                session_id: "session-persisted".to_string(),
                model: "test-model".to_string(),
                tools: vec!["Bash".to_string()],
                task_id: None,
            },
        );
        let snapshot = RenderSessionSnapshot::new(
            Some("session-persisted".to_string()),
            None,
            Some("turn-0001".to_string()),
            7,
            2,
            1,
            records,
            vec![raw_event],
        );
        let dir = tempfile::tempdir().expect("tempdir should be created");
        let path = dir.path().join("nested").join("render-session.json");

        snapshot
            .save_json_file(&path)
            .expect("render session snapshot should save");

        let payload =
            std::fs::read_to_string(&path).expect("render session snapshot file should exist");
        assert!(payload.contains("\"version\": 1"), "{payload}");
        assert!(
            !path.with_file_name(".render-session.json.tmp").exists(),
            "temporary snapshot file should be renamed away"
        );

        let restored = RenderSessionSnapshot::load_json_file(&path)
            .expect("render session snapshot should load");
        assert_eq!(restored, snapshot);
        assert_eq!(restored.record_count(), 2);
        assert_eq!(restored.raw_event_count(), 1);
    }

    #[test]
    fn extracts_final_summary_out_of_message_records() {
        let summary = FinalSummaryModel {
            id: "summary-1".to_string(),
            success: true,
            terminal: "Completed".to_string(),
            changed_files: vec![FileChangeSummaryModel {
                path: "src/lib.rs".to_string(),
                status: "M".to_string(),
                additions: 2,
                deletions: 1,
            }],
            commands: vec![CommandSummaryModel {
                command: "cargo test".to_string(),
                cwd: None,
                exit_code: Some(0),
                duration_ms: Some(42),
                status: "passed".to_string(),
            }],
            verification_results: vec![VerificationSummaryModel {
                command: "cargo test".to_string(),
                status: "passed".to_string(),
                passed: true,
                exit_code: Some(0),
                duration_ms: Some(42),
            }],
            residual_risks: Vec::new(),
            notes: vec!["verified".to_string()],
        };
        let messages = vec![
            message(MessageType::User, "finish the task"),
            message(MessageType::System, final_summary_message_content(&summary)),
        ];

        let records = TranscriptRecords::from_messages(&messages);

        assert_eq!(records.entries.len(), 1);
        assert_eq!(records.final_summaries.len(), 1);
        assert_eq!(records.final_summaries[0].source_index, 1);
        assert_eq!(records.final_summaries[0].model, summary);
    }
}
