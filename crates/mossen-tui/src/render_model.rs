//! Semantic render model for the TUI pipeline.
//!
//! This module is the boundary between agent/message state and terminal
//! widgets. It intentionally contains viewport-independent semantics only:
//! what should be shown, not how many terminal cells it should occupy.

use crate::message_model::{display_tool_name, MessageData, MessageType};
use crate::render_events::{
    RenderEvent, RenderEventKind, RenderEventScope, RenderHistoryPolicy, RenderRefreshPolicy,
};
use crate::render_lifecycle::{
    approval_decision_from_message, LifecyclePhase, TranscriptRecord, TranscriptRecordKind,
    TranscriptRecords,
};
pub use crate::render_lifecycle::{
    approval_decision_message_content, final_summary_message_content, ApprovalDecisionKind,
    ApprovalDecisionModel, CommandSummaryModel, FileChangeSummaryModel, FinalSummaryModel,
    VerificationSummaryModel,
};
use mossen_types::{ContentBlock, Message, Role, ToolResultContent};
use mossen_utils::display_tags::strip_display_tags_allow_empty;
use mossen_utils::string_utils::{truncate_chars, truncate_chars_with_suffix};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use similar::{ChangeTag, TextDiff};
use std::borrow::Cow;
use std::collections::HashSet;

const TOOL_SUMMARY_CHARS: usize = 160;
const BLOCK_SELECTOR_SUMMARY_CHARS: usize = 220;
const ERROR_DETAIL_PREVIEW_LINES: usize = 8;
const TIMELINE_DETAIL_CHARS: usize = 180;
const COMPACT_PLAN_SUMMARY_PREVIEW_CHARS: usize = 800;
const COMPACT_PLAN_SUMMARY_MAX_CHARS: usize = 12_000;

/// A viewport-independent transcript model.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RenderTranscript {
    pub blocks: Vec<RenderBlock>,
}

impl RenderTranscript {
    pub fn from_messages(messages: &[MessageData]) -> Self {
        Self::from_messages_and_decisions(messages, &[])
    }

    pub fn from_messages_and_decisions(
        messages: &[MessageData],
        decisions: &[ApprovalDecisionModel],
    ) -> Self {
        let records = TranscriptRecords::from_messages_and_decisions(messages, decisions);
        Self::from_records(&records)
    }

    pub fn from_records(records: &TranscriptRecords) -> Self {
        let mut blocks = Vec::new();
        let mut consumed_record_indices = HashSet::new();
        let mut index = 0usize;

        while index < records.entries.len() {
            if consumed_record_indices.contains(&index) {
                index += 1;
                continue;
            }
            let record = &records.entries[index];
            if is_protocol_only_record(record) {
                index += 1;
                continue;
            }

            if matches!(record.kind, TranscriptRecordKind::ToolUse) {
                if let Some((result_index, result)) =
                    next_tool_result_record(&records.entries, index)
                {
                    if let Some(block) = RenderBlock::from_tool_record_pair(record, result) {
                        consumed_record_indices.insert(result_index);
                        blocks.push(block);
                        index += 1;
                        continue;
                    }
                }
            }

            if let Some(block) = RenderBlock::from_record(record) {
                blocks.push(block);
            }
            index += 1;
        }

        append_file_change_summary_blocks(&mut blocks, records);
        append_approval_decision_blocks(&mut blocks, &records.approval_decisions);
        append_final_summary_blocks(&mut blocks, &records.final_summaries);

        Self { blocks }
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    pub fn source_record_count(&self) -> usize {
        self.blocks
            .iter()
            .flat_map(|block| block.source_indices.iter().copied())
            .max()
            .map(|index| index.saturating_add(1))
            .unwrap_or(0)
    }
}

pub fn file_change_summaries_from_messages(
    messages: &[MessageData],
) -> Vec<FileChangeSummaryModel> {
    let records = TranscriptRecords::from_messages(messages);
    file_change_summaries_from_records(&records)
}

pub fn file_change_summaries_from_records(
    records: &TranscriptRecords,
) -> Vec<FileChangeSummaryModel> {
    collect_file_change_summary(records)
        .map(|summary| summary.files)
        .unwrap_or_default()
}

pub fn command_history_from_transcript(transcript: &RenderTranscript) -> CommandHistoryRenderModel {
    let rows = transcript
        .blocks
        .iter()
        .filter_map(command_history_row_from_block)
        .collect::<Vec<_>>();
    CommandHistoryRenderModel::from_rows(rows)
}

pub fn command_summaries_from_messages(messages: &[MessageData]) -> Vec<CommandSummaryModel> {
    let transcript = RenderTranscript::from_messages(messages);
    command_summaries_from_transcript(&transcript)
}

pub fn command_summaries_from_transcript(
    transcript: &RenderTranscript,
) -> Vec<CommandSummaryModel> {
    command_history_from_transcript(transcript)
        .rows
        .into_iter()
        .map(command_summary_from_history_row)
        .collect()
}

/// Human-readable summary preview for messages that would be compacted.
pub fn compact_plan_summary_preview_from_messages(messages: &[Message]) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "Earlier conversation summary. The following {} message(s) were compacted before the recent context:",
        messages.len()
    ));
    for (index, message) in messages.iter().enumerate() {
        let text = compact_plan_blocks_inline(&message.content);
        let text = if text.is_empty() {
            "[non-text content omitted]".to_string()
        } else {
            text
        };
        lines.push(format!(
            "{}. {}: {}",
            index + 1,
            compact_plan_role_label(message.role),
            text
        ));
    }
    compact_plan_truncate_chars(&lines.join("\n"), COMPACT_PLAN_SUMMARY_MAX_CHARS)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactPlanRenderModel {
    pub before_messages: usize,
    pub compacted_messages: usize,
    pub recent_messages: usize,
    pub after_messages: usize,
    pub before_tokens: u64,
    pub after_tokens: u64,
    pub is_running: bool,
    pub hooks_configured: bool,
    pub custom_instructions: Option<String>,
}

pub fn compact_plan_render_model(
    messages: &[Message],
    is_running: bool,
    hooks_configured: bool,
    custom_instructions: Option<String>,
) -> CompactPlanRenderModel {
    let before_messages = messages.len();
    let before_tokens = mossen_agent::token_estimation::estimate_messages_tokens(messages);
    if before_messages < 2 {
        return CompactPlanRenderModel {
            before_messages,
            compacted_messages: 0,
            recent_messages: before_messages,
            after_messages: before_messages,
            before_tokens,
            after_tokens: before_tokens,
            is_running,
            hooks_configured,
            custom_instructions,
        };
    }

    let compacted_messages = before_messages / 2;
    let recent_messages = before_messages.saturating_sub(compacted_messages);
    let summary_tokens = compact_plan_summary_tokens(&messages[..compacted_messages]);
    let recent_tokens =
        mossen_agent::token_estimation::estimate_messages_tokens(&messages[compacted_messages..]);

    CompactPlanRenderModel {
        before_messages,
        compacted_messages,
        recent_messages,
        after_messages: recent_messages.saturating_add(1),
        before_tokens,
        after_tokens: summary_tokens.saturating_add(recent_tokens),
        is_running,
        hooks_configured,
        custom_instructions,
    }
}

pub fn compact_plan_body_from_model(model: &CompactPlanRenderModel) -> String {
    let state = if model.is_running { "running" } else { "idle" };
    let token_savings = compact_plan_token_savings_label(model);
    let status = if model.before_messages < 2 {
        "Not enough messages to compact."
    } else if model.is_running {
        "Compaction is already running. Use /compact status or /compact cancel."
    } else if model.after_tokens >= model.before_tokens {
        "Preview only. Run /compact run to apply; short histories may not save tokens."
    } else {
        "Preview only. Run /compact run to apply."
    };
    let hook_runtime = compact_hooks_label(model.hooks_configured);
    let custom_instructions = model
        .custom_instructions
        .as_deref()
        .map(|instructions| compact_plan_inline(instructions, 240))
        .filter(|instructions| !instructions.is_empty())
        .unwrap_or_else(|| "none".to_string());

    format!(
        "Compact plan\nstate: {state}\nmessages: {} -> {}\ncompacted messages: {}\nrecent messages kept: {}\nestimated tokens: {} -> {}\nestimated savings: {token_savings}\nhooks: {hook_runtime}\ncustom instructions: {custom_instructions}\nstatus: {status}",
        model.before_messages,
        model.after_messages,
        model.compacted_messages,
        model.recent_messages,
        model.before_tokens,
        model.after_tokens
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactStatusRenderModel {
    pub is_running: bool,
    pub task_id: Option<u64>,
    pub pending_launch: bool,
    pub cancellable: bool,
    pub hooks_configured: bool,
    pub progress: Option<String>,
}

pub fn compact_status_body_from_model(model: &CompactStatusRenderModel) -> String {
    let state = if model.is_running { "running" } else { "idle" };
    let task = model
        .task_id
        .map(|id| id.to_string())
        .unwrap_or_else(|| "-".to_string());
    let pending = yes_no(model.pending_launch);
    let cancellable = yes_no(model.cancellable);
    let hook_runtime = compact_hooks_label(model.hooks_configured);
    let progress = model
        .progress
        .as_deref()
        .unwrap_or("No compact activity recorded.");

    format!(
        "Compact status\nstate: {state}\ntask: {task}\npending launch: {pending}\ncancellable: {cancellable}\nhooks: {hook_runtime}\nprogress: {progress}\nhint: /compact cancel"
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PermissionModeChoiceRenderModel {
    pub label: &'static str,
    pub code: &'static str,
}

const PERMISSION_MODE_CHOICES: &[PermissionModeChoiceRenderModel] = &[
    PermissionModeChoiceRenderModel {
        label: "Supervised",
        code: "default",
    },
    PermissionModeChoiceRenderModel {
        label: "Plan",
        code: "plan",
    },
    PermissionModeChoiceRenderModel {
        label: "Accept Edits",
        code: "acceptEdits",
    },
    PermissionModeChoiceRenderModel {
        label: "Full Auto",
        code: "bypassPermissions",
    },
    PermissionModeChoiceRenderModel {
        label: "Don't Ask",
        code: "dontAsk",
    },
];

pub fn permission_mode_choices() -> &'static [PermissionModeChoiceRenderModel] {
    PERMISSION_MODE_CHOICES
}

pub fn permission_mode_display_label(raw: Option<&str>) -> String {
    let value = raw.map(str::trim).unwrap_or_default();
    if value.is_empty() {
        return "Supervised".to_string();
    }
    permission_mode_choices()
        .iter()
        .find(|choice| permission_mode_choice_matches(choice, value))
        .map(|choice| choice.label.to_string())
        .unwrap_or_else(|| value.to_string())
}

pub fn permission_mode_choice_index(raw: Option<&str>) -> usize {
    let label = permission_mode_display_label(raw);
    permission_mode_choices()
        .iter()
        .position(|choice| permission_mode_choice_matches(choice, &label))
        .unwrap_or(0)
}

pub fn permission_mode_code_for_raw(raw: Option<&str>) -> &'static str {
    let value = raw.map(str::trim).unwrap_or_default();
    permission_mode_choices()
        .iter()
        .find(|choice| permission_mode_choice_matches(choice, value))
        .map(|choice| choice.code)
        .unwrap_or("default")
}

pub fn permission_mode_code_for_choice(choice: &str) -> Option<&'static str> {
    permission_mode_choices()
        .iter()
        .find(|candidate| permission_mode_choice_matches(candidate, choice))
        .map(|choice| choice.code)
}

fn permission_mode_choice_matches(choice: &PermissionModeChoiceRenderModel, raw: &str) -> bool {
    raw.eq_ignore_ascii_case(choice.label)
        || raw.eq_ignore_ascii_case(choice.code)
        || permission_mode_match_key(raw) == permission_mode_match_key(choice.label)
        || permission_mode_match_key(raw) == permission_mode_match_key(choice.code)
}

fn permission_mode_match_key(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// User-facing multi-line preview for a structured tool-call input.
pub fn tool_call_preview_from_input(tool_name: &str, input: &Value) -> String {
    let Some(obj) = input.as_object() else {
        return tool_input_summary_from_value(input);
    };

    match tool_name {
        "Bash" => tool_input_string_field(obj, "command")
            .map(|cmd| format!("command\n  {}", truncate_chars(cmd, 400)))
            .unwrap_or_else(|| tool_input_summary_from_value(input)),
        "Read" => {
            let path = tool_input_string_field(obj, "file_path")
                .or_else(|| tool_input_string_field(obj, "path"))
                .unwrap_or("(missing path)");
            let mut lines = vec![format!("path  {}", path)];
            if let Some(offset) = obj.get("offset").and_then(|value| value.as_u64()) {
                lines.push(format!("from  line {}", offset + 1));
            }
            if let Some(limit) = obj.get("limit").and_then(|value| value.as_u64()) {
                lines.push(format!("limit {} lines", limit));
            }
            lines.join("\n")
        }
        "Write" => {
            let path = tool_input_string_field(obj, "file_path")
                .or_else(|| tool_input_string_field(obj, "path"))
                .unwrap_or("(missing path)");
            let content = tool_input_string_field(obj, "content").unwrap_or("");
            format!(
                "path  {}\nwrite {} chars\n{}",
                path,
                content.chars().count(),
                tool_preview_block(content, 8)
            )
        }
        "Edit" | "MultiEdit" | "NotebookEdit" => {
            let path = tool_input_string_field(obj, "file_path")
                .or_else(|| tool_input_string_field(obj, "path"))
                .unwrap_or("(missing path)");
            let old = tool_input_string_field(obj, "old_string").unwrap_or("");
            let new = tool_input_string_field(obj, "new_string").unwrap_or("");
            let replace_all = obj
                .get("replace_all")
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
            format!(
                "path  {}\nreplace_all {}\n{}",
                path,
                replace_all,
                tool_simple_unified_diff(old, new)
            )
        }
        "Grep" => {
            let pattern = tool_input_string_field(obj, "pattern").unwrap_or("(missing pattern)");
            let path = tool_input_string_field(obj, "path").unwrap_or(".");
            let mode = tool_input_string_field(obj, "output_mode").unwrap_or("files_with_matches");
            let glob = tool_input_string_field(obj, "glob");
            let mut lines = vec![
                format!("pattern  {}", pattern),
                format!("path     {}", path),
                format!("mode     {}", mode),
            ];
            if let Some(glob) = glob {
                lines.push(format!("glob     {}", glob));
            }
            lines.join("\n")
        }
        "Glob" => {
            let pattern = tool_input_string_field(obj, "pattern").unwrap_or("(missing pattern)");
            let path = tool_input_string_field(obj, "path").unwrap_or(".");
            format!("pattern  {}\npath     {}", pattern, path)
        }
        "Task" | "Agent" => {
            let kind = tool_input_string_field(obj, "subagent_type")
                .or_else(|| tool_input_string_field(obj, "agent_type"))
                .unwrap_or("general-purpose");
            let description =
                tool_input_string_field(obj, "description").unwrap_or("(no description)");
            let prompt = tool_input_string_field(obj, "prompt").unwrap_or("");
            format!(
                "agent   {}\ntask    {}\nprompt\n{}",
                kind,
                description,
                tool_preview_block(prompt, 8)
            )
        }
        "TodoWrite" => {
            let todos = obj
                .get("todos")
                .and_then(|value| value.as_array())
                .map(|items| {
                    items
                        .iter()
                        .take(8)
                        .filter_map(|todo| {
                            let content = todo.get("content").and_then(|value| value.as_str())?;
                            let status = todo
                                .get("status")
                                .and_then(|value| value.as_str())
                                .unwrap_or("pending");
                            Some(format!("{} {}", status, content))
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if todos.is_empty() {
                tool_input_summary_from_value(input)
            } else {
                todos.join("\n")
            }
        }
        _ => tool_input_summary_from_value(input),
    }
}

/// Short, human-readable summary for structured tool-call input values.
pub fn tool_input_summary_from_value(input: &Value) -> String {
    const MAX_PREVIEW: usize = 240;

    let Some(obj) = input.as_object() else {
        if input.is_null() {
            return String::new();
        }
        let raw = serde_json::to_string(input).unwrap_or_else(|_| "{}".to_string());
        return truncate_chars(&raw, MAX_PREVIEW);
    };
    if obj.is_empty() {
        return String::new();
    }

    if obj.len() == 1 {
        let (_key, value) = obj.iter().next().unwrap();
        let rendered = match value {
            Value::String(text) => text.clone(),
            other => other.to_string(),
        };
        return truncate_chars(&rendered, MAX_PREVIEW);
    }

    let mut parts = Vec::new();
    for (key, value) in obj {
        let rendered = match value {
            Value::String(text) => text.clone(),
            other => other.to_string(),
        };
        parts.push(format!("{key}={rendered}"));
    }
    truncate_chars(&parts.join(", "), MAX_PREVIEW)
}

fn compact_plan_blocks_inline(blocks: &[ContentBlock]) -> String {
    let mut parts = Vec::new();
    for block in blocks {
        match block {
            ContentBlock::Text(text) => {
                let text = compact_plan_inline(&text.text, COMPACT_PLAN_SUMMARY_PREVIEW_CHARS);
                if !text.is_empty() {
                    parts.push(text);
                }
            }
            ContentBlock::ToolUse(tool) => {
                let input = compact_plan_inline(&tool_input_summary_from_value(&tool.input), 240);
                if input.is_empty() {
                    parts.push(format!("tool_use {}", tool.name));
                } else {
                    parts.push(format!("tool_use {} {}", tool.name, input));
                }
            }
            ContentBlock::ToolResult(result) => {
                let text = compact_plan_tool_result_inline(&result.content);
                if text.is_empty() {
                    parts.push(format!("tool_result {}", result.tool_use_id));
                } else {
                    parts.push(format!("tool_result {} {}", result.tool_use_id, text));
                }
            }
            ContentBlock::Thinking(_) => {}
            ContentBlock::Image(_) => parts.push("[image]".to_string()),
        }
    }
    compact_plan_truncate_chars(&parts.join(" "), COMPACT_PLAN_SUMMARY_PREVIEW_CHARS)
}

fn compact_plan_tool_result_inline(content: &ToolResultContent) -> String {
    match content {
        ToolResultContent::Text(text) => {
            compact_plan_inline(text, COMPACT_PLAN_SUMMARY_PREVIEW_CHARS)
        }
        ToolResultContent::Blocks(blocks) => compact_plan_blocks_inline(blocks),
    }
}

fn compact_plan_summary_tokens(messages: &[Message]) -> u64 {
    if messages.is_empty() {
        return 0;
    }
    let summary = compact_plan_summary_preview_from_messages(messages);
    mossen_agent::token_estimation::rough_estimate(&summary)
}

fn compact_plan_token_savings_label(model: &CompactPlanRenderModel) -> String {
    if model.after_tokens <= model.before_tokens {
        model
            .before_tokens
            .saturating_sub(model.after_tokens)
            .to_string()
    } else {
        format!(
            "-{} (summary overhead)",
            model.after_tokens.saturating_sub(model.before_tokens)
        )
    }
}

fn compact_hooks_label(configured: bool) -> &'static str {
    if configured {
        "configured"
    } else {
        "not configured"
    }
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

fn compact_plan_inline(input: &str, max_chars: usize) -> String {
    let compacted = input.split_whitespace().collect::<Vec<_>>().join(" ");
    compact_plan_truncate_chars(&compacted, max_chars)
}

fn compact_plan_truncate_chars(input: &str, max_chars: usize) -> String {
    if input.chars().count() <= max_chars {
        return input.to_string();
    }
    let keep_chars = max_chars.saturating_sub(3);
    let mut output: String = input.chars().take(keep_chars).collect();
    output.push_str("...");
    output
}

fn compact_plan_role_label(role: Role) -> &'static str {
    match role {
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::System => "system",
    }
}

pub fn error_history_from_transcript(transcript: &RenderTranscript) -> ErrorHistoryRenderModel {
    let rows = transcript
        .blocks
        .iter()
        .flat_map(error_history_rows_from_block)
        .collect::<Vec<_>>();
    ErrorHistoryRenderModel::from_rows(rows)
}

pub fn final_summary_history_from_transcript(
    transcript: &RenderTranscript,
) -> FinalSummaryHistoryRenderModel {
    let rows = transcript
        .blocks
        .iter()
        .filter_map(final_summary_history_row_from_block)
        .collect::<Vec<_>>();
    FinalSummaryHistoryRenderModel::from_rows(rows)
}

pub fn approval_history_from_transcript(
    transcript: &RenderTranscript,
) -> ApprovalHistoryRenderModel {
    let rows = transcript
        .blocks
        .iter()
        .filter_map(approval_history_row_from_block)
        .collect::<Vec<_>>();
    ApprovalHistoryRenderModel::from_rows(rows)
}

/// Complete viewport-independent surface for one rendered TUI frame.
///
/// The renderer may decide where each part goes, but it should not have to
/// infer blocking state or footer facts by re-reading app/protocol state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderSurface {
    pub transcript: RenderTranscript,
    pub approvals: Vec<ApprovalRenderModel>,
    pub top_status: TopStatusRenderModel,
    pub activity_panel: Option<ActivityPanelRenderModel>,
    pub footer: FooterRenderModel,
    pub blocking: Option<BlockingRenderModel>,
}

impl RenderSurface {
    pub fn new(transcript: RenderTranscript, footer: FooterRenderModel) -> Self {
        let blocking = footer.blocking.clone();
        let top_status = TopStatusRenderModel::from_footer(&footer);
        Self {
            transcript,
            approvals: Vec::new(),
            top_status,
            activity_panel: None,
            footer,
            blocking,
        }
    }

    pub fn with_activity_panel(mut self, activity_panel: Option<ActivityPanelRenderModel>) -> Self {
        self.activity_panel = activity_panel;
        self
    }

    pub fn with_approval(mut self, approval: ApprovalRenderModel) -> Self {
        let blocking = approval.blocking_model();
        self.footer.blocking = Some(blocking.clone());
        self.blocking = Some(blocking);
        self.top_status.blocking = self.blocking.clone();
        self.top_status.stage = Some("waiting approval".to_string());
        self.approvals.push(approval);
        self
    }

    pub fn is_blocked(&self) -> bool {
        self.blocking.is_some()
    }
}

/// A stable semantic transcript block.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RenderBlock {
    pub id: String,
    pub source_indices: Vec<usize>,
    pub kind: RenderBlockKind,
    pub state: RenderBlockState,
    pub nodes: Vec<RenderNode>,
    pub tool: Option<ToolCardModel>,
}

impl RenderBlock {
    pub fn selector_summary(&self) -> String {
        let mut parts = Vec::new();
        for node in &self.nodes {
            match node {
                RenderNode::Markdown(text) | RenderNode::PlainText(text) => {
                    push_selector_part(&mut parts, text);
                }
                RenderNode::Thinking(text) => {
                    let text = compact_inline_text(text);
                    if !text.is_empty() {
                        parts.push(format!("Thinking: {text}"));
                    }
                }
                RenderNode::Error(error) => {
                    parts.push(format!("{}: {}", error.title, error.summary));
                }
                RenderNode::FileChangeSummary(summary) => {
                    parts.push(format!(
                        "{} · +{} -{}",
                        summary.title(),
                        summary.total_additions(),
                        summary.total_deletions()
                    ));
                }
                RenderNode::FinalSummary(summary) => {
                    let status = if summary.success {
                        "completed"
                    } else {
                        "attention"
                    };
                    let mut summary_parts = vec![
                        format!("{} files", summary.changed_files.len()),
                        format!("{} commands", summary.commands.len()),
                    ];
                    if !summary.verification_results.is_empty() {
                        summary_parts
                            .push(format!("{} checks", summary.verification_results.len()));
                    }
                    if !summary.residual_risks.is_empty() {
                        summary_parts.push(format!("{} risks", summary.residual_risks.len()));
                    }
                    parts.push(format!(
                        "Final summary: {status} · {}",
                        summary_parts.join(" · ")
                    ));
                }
                RenderNode::ToolCard(tool) => {
                    parts.push(tool_selector_summary(tool));
                }
                RenderNode::ApprovalDecision(decision) => {
                    parts.push(decision.line());
                }
            }
        }

        if parts.is_empty() {
            if let Some(tool) = self.tool.as_ref() {
                parts.push(tool_selector_summary(tool));
            }
        }

        let summary = parts.join(" | ");
        if summary.is_empty() {
            "(empty)".to_string()
        } else {
            truncate_chars_with_suffix(&summary, BLOCK_SELECTOR_SUMMARY_CHARS, "...")
        }
    }

    fn from_record(record: &TranscriptRecord) -> Option<Self> {
        if is_protocol_only_record(record) {
            return None;
        }

        let message = record.to_message_data();
        if let Some(decision) = approval_decision_from_message(&message) {
            return Some(Self {
                id: block_id_from_record(record),
                source_indices: vec![record.source_index],
                kind: RenderBlockKind::ApprovalDecision,
                state: RenderBlockState {
                    streaming: false,
                    error: decision.is_error(),
                    expanded: false,
                },
                nodes: vec![RenderNode::ApprovalDecision(decision)],
                tool: None,
            });
        }

        let state = RenderBlockState::from_record(record);
        let tool = tool_card_from_message(&message);
        let is_error_block = tool.is_none() && record.is_error;
        let kind = if is_error_block {
            RenderBlockKind::Error
        } else {
            RenderBlockKind::from_record_kind(record.kind)
        };
        let nodes = if is_error_block {
            vec![RenderNode::Error(error_model_from_record(record))]
        } else {
            nodes_from_record(record, tool.as_ref())
        };

        Some(Self {
            id: block_id_from_record(record),
            source_indices: vec![record.source_index],
            kind,
            state,
            nodes,
            tool,
        })
    }

    fn from_tool_record_pair(
        tool_use: &TranscriptRecord,
        result: &TranscriptRecord,
    ) -> Option<Self> {
        let tool_use_message = tool_use.to_message_data();
        let result_message = result.to_message_data();
        let tool = tool_card_from_pair(&tool_use_message, &result_message)?;
        let state = RenderBlockState {
            streaming: tool_use.is_streaming || result.is_streaming,
            error: result.is_error
                || matches!(
                    result.lifecycle,
                    LifecyclePhase::Failed | LifecyclePhase::Cancelled | LifecyclePhase::TimedOut
                ),
            expanded: tool_use.expanded || result.expanded,
        };

        Some(Self {
            id: block_id_from_record(tool_use),
            source_indices: vec![tool_use.source_index, result.source_index],
            kind: RenderBlockKind::Tool,
            state,
            nodes: vec![RenderNode::ToolCard(tool.clone())],
            tool: Some(tool),
        })
    }

    fn from_approval_decision(index: usize, decision: &ApprovalDecisionModel) -> Self {
        let id = if decision.id.is_empty() {
            format!("approval-decision-sidecar-{index}")
        } else {
            decision.id.clone()
        };
        Self {
            id,
            source_indices: Vec::new(),
            kind: RenderBlockKind::ApprovalDecision,
            state: RenderBlockState {
                streaming: false,
                error: decision.is_error(),
                expanded: false,
            },
            nodes: vec![RenderNode::ApprovalDecision(decision.clone())],
            tool: None,
        }
    }

    fn from_file_change_summary(summary: CollectedFileChangeSummary) -> Self {
        let first_source = summary.source_indices.first().copied().unwrap_or_default();
        Self {
            id: format!("file-change-summary-{first_source}"),
            source_indices: summary.source_indices,
            kind: RenderBlockKind::FileChangeSummary,
            state: RenderBlockState {
                streaming: false,
                error: false,
                expanded: false,
            },
            nodes: vec![RenderNode::FileChangeSummary(
                FileChangeSummaryRenderModel {
                    files: summary.files,
                },
            )],
            tool: None,
        }
    }

    fn from_final_summary(
        index: usize,
        summary: &crate::render_lifecycle::FinalSummaryRecord,
    ) -> Self {
        let id = if summary.model.id.is_empty() {
            format!("final-summary-sidecar-{index}")
        } else {
            summary.model.id.clone()
        };
        Self {
            id,
            source_indices: vec![summary.source_index],
            kind: RenderBlockKind::FinalSummary,
            state: RenderBlockState {
                streaming: false,
                error: !summary.model.success,
                expanded: false,
            },
            nodes: vec![RenderNode::FinalSummary(summary.model.clone())],
            tool: None,
        }
    }
}

fn append_approval_decision_blocks(
    blocks: &mut Vec<RenderBlock>,
    decisions: &[ApprovalDecisionModel],
) {
    if decisions.is_empty() {
        return;
    }

    let mut anchored = Vec::new();
    let mut unanchored = Vec::new();
    for (index, decision) in decisions.iter().enumerate() {
        let block = RenderBlock::from_approval_decision(index, decision);
        if let Some(anchor) = decision.anchor_block_id.as_ref() {
            anchored.push((anchor.clone(), block));
        } else {
            unanchored.push(block);
        }
    }

    if !anchored.is_empty() {
        let mut merged = Vec::with_capacity(blocks.len() + anchored.len() + unanchored.len());
        for block in blocks.drain(..) {
            merged.push(block);
            let mut index = 0usize;
            while index < anchored.len() {
                if approval_anchor_matches(&anchored[index].0, &merged[merged.len() - 1]) {
                    merged.push(anchored.remove(index).1);
                } else {
                    index += 1;
                }
            }
        }
        merged.extend(anchored.into_iter().map(|(_, block)| block));
        *blocks = merged;
    }

    blocks.extend(unanchored);
}

fn append_file_change_summary_blocks(blocks: &mut Vec<RenderBlock>, records: &TranscriptRecords) {
    let Some(summary) = collect_file_change_summary(records) else {
        return;
    };
    let first_source = summary.source_indices.first().copied().unwrap_or_default();
    let block = RenderBlock::from_file_change_summary(summary);
    let insertion = blocks
        .iter()
        .position(|existing| {
            existing
                .source_indices
                .first()
                .is_some_and(|source| *source >= first_source)
        })
        .unwrap_or(blocks.len());
    blocks.insert(insertion, block);
}

fn append_final_summary_blocks(
    blocks: &mut Vec<RenderBlock>,
    summaries: &[crate::render_lifecycle::FinalSummaryRecord],
) {
    for (index, summary) in summaries.iter().enumerate() {
        let block = RenderBlock::from_final_summary(index, summary);
        let insertion = blocks
            .iter()
            .position(|existing| {
                existing
                    .source_indices
                    .first()
                    .is_some_and(|source| *source > summary.source_index)
            })
            .unwrap_or(blocks.len());
        blocks.insert(insertion, block);
    }
}

fn block_id_from_record(record: &TranscriptRecord) -> String {
    if !record.id.is_empty() {
        return record.id.clone();
    }

    match record.kind {
        TranscriptRecordKind::ToolUse => format!("tool-{}", record.source_index),
        TranscriptRecordKind::ToolResult => format!("tool-result-{}", record.source_index),
        _ => format!("message-{}", record.source_index),
    }
}

fn approval_anchor_matches(anchor: &str, block: &RenderBlock) -> bool {
    if anchor == block.id {
        return true;
    }

    matches!(block.kind, RenderBlockKind::Tool)
        && block.source_indices.len() == 2
        && anchor
            == format!(
                "tool-{}-{}",
                block.source_indices[0], block.source_indices[1]
            )
}

fn push_selector_part(parts: &mut Vec<String>, text: &str) {
    let text = compact_inline_text(text);
    if !text.is_empty() {
        parts.push(text);
    }
}

fn compact_inline_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn error_model_from_record(record: &TranscriptRecord) -> ErrorRenderModel {
    let content = sanitize_semantic_text(&record.content);
    let mut lines = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();

    let summary = lines
        .first()
        .cloned()
        .unwrap_or_else(|| "An operation failed without details.".to_string());
    let detail_lines = if lines.len() > 1 {
        lines.split_off(1)
    } else {
        Vec::new()
    };
    let details = (!detail_lines.is_empty()).then(|| detail_lines.join("\n"));
    let key_detail =
        select_key_error_detail(&detail_lines).filter(|detail| !same_error_line(detail, &summary));
    let detail_hidden_line_count = detail_lines
        .len()
        .saturating_sub(ERROR_DETAIL_PREVIEW_LINES);
    let lower = summary.to_ascii_lowercase();
    let title = if lower.starts_with("api retry") {
        "API retry".to_string()
    } else if matches!(record.kind, TranscriptRecordKind::CommandOutput) {
        "Command error".to_string()
    } else {
        "Error".to_string()
    };
    let retrying = lower.starts_with("api retry") || lower.contains(" retry ");
    let retry_hint = if retrying {
        Some("Automatic retry is scheduled.".to_string())
    } else {
        None
    };

    ErrorRenderModel {
        title,
        summary: truncate_chars_with_suffix(&summary, TOOL_SUMMARY_CHARS, "..."),
        key_detail,
        details,
        detail_hidden_line_count,
        retry_hint,
        retrying,
    }
}

fn select_key_error_detail(lines: &[String]) -> Option<String> {
    lines
        .iter()
        .find(|line| is_key_error_line(line))
        .or_else(|| lines.first())
        .map(|line| truncate_chars_with_suffix(line, TOOL_SUMMARY_CHARS, "..."))
}

fn is_key_error_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    [
        "error",
        "failed",
        "failure",
        "panic",
        "panicked",
        "exception",
        "denied",
        "not found",
        "cannot",
        "could not",
        "timed out",
        "timeout",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn same_error_line(left: &str, right: &str) -> bool {
    left.trim().eq_ignore_ascii_case(right.trim())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CollectedFileChangeSummary {
    source_indices: Vec<usize>,
    files: Vec<FileChangeSummaryModel>,
}

fn collect_file_change_summary(records: &TranscriptRecords) -> Option<CollectedFileChangeSummary> {
    let mut source_indices = Vec::new();
    let mut files = Vec::<FileChangeSummaryModel>::new();

    for record in &records.entries {
        if !matches!(record.kind, TranscriptRecordKind::ToolResult) {
            continue;
        }
        if !record.tool_name.as_deref().is_some_and(|tool_name| {
            ToolFamily::from_tool_name(tool_name) == ToolFamily::FileChange
        }) {
            continue;
        }
        let message = record.to_message_data();
        let Some(payload) = semantic_tool_json_payload(&message) else {
            continue;
        };
        if let Some(file) = file_change_summary_from_payload(&payload) {
            source_indices.push(record.source_index);
            merge_file_summary(&mut files, file);
        }
    }

    if files.is_empty() {
        for summary in &records.final_summaries {
            if summary.model.changed_files.is_empty() {
                continue;
            }
            source_indices.push(summary.source_index);
            for file in &summary.model.changed_files {
                merge_file_summary(&mut files, file.clone());
            }
        }
    }

    if files.is_empty() {
        return None;
    }

    source_indices.sort_unstable();
    source_indices.dedup();
    Some(CollectedFileChangeSummary {
        source_indices,
        files,
    })
}

fn semantic_tool_json_payload(message: &MessageData) -> Option<Value> {
    let content = semantic_tool_content(message);
    serde_json::from_str::<Value>(content.as_ref()).ok()
}

fn file_change_summary_from_payload(payload: &Value) -> Option<FileChangeSummaryModel> {
    let object = payload.as_object()?;
    let path = object
        .get("file_path")
        .or_else(|| object.get("path"))
        .or_else(|| object.get("notebook_path"))
        .and_then(value_as_display_text)
        .map(|path| sanitize_semantic_text(&path))
        .filter(|path| !path.trim().is_empty())?;
    let old = object
        .get("old_string")
        .or_else(|| object.get("original_file"))
        .and_then(value_as_display_text);
    let explicit_new = object
        .get("new_string")
        .or_else(|| object.get("updated_file"))
        .or_else(|| object.get("new_source"))
        .and_then(value_as_display_text);
    let content = object.get("content").and_then(value_as_display_text);
    let new = explicit_new.as_deref().or(content.as_deref());
    let (additions, deletions) = match (old.as_deref(), new) {
        (Some(old), Some(new)) => diff_line_counts(old, new),
        (None, Some(new)) => (line_count_for_change(new), 0),
        (Some(old), None) => (0, line_count_for_change(old)),
        (None, None) => (0, 0),
    };
    let status = match (old.as_deref(), new) {
        (None, Some(new)) if !new.is_empty() => "A",
        (Some(old), None) if !old.is_empty() => "D",
        (Some(old), Some(new)) if new.is_empty() && !old.is_empty() => "D",
        _ => "M",
    }
    .to_string();

    Some(FileChangeSummaryModel {
        path,
        status,
        additions,
        deletions,
    })
}

fn merge_file_summary(files: &mut Vec<FileChangeSummaryModel>, next: FileChangeSummaryModel) {
    if let Some(existing) = files.iter_mut().find(|file| file.path == next.path) {
        existing.additions = existing.additions.saturating_add(next.additions);
        existing.deletions = existing.deletions.saturating_add(next.deletions);
        existing.status = merged_file_status(&existing.status, &next.status).to_string();
        return;
    }
    files.push(next);
}

fn merged_file_status(existing: &str, next: &str) -> &'static str {
    match (existing, next) {
        ("D", _) | (_, "D") => "D",
        ("A", _) | (_, "A") => "A",
        _ => "M",
    }
}

fn diff_line_counts(old: &str, new: &str) -> (usize, usize) {
    let diff = TextDiff::from_lines(old, new);
    diff.iter_all_changes()
        .fold((0usize, 0usize), |(adds, dels), change| {
            match change.tag() {
                ChangeTag::Insert => (adds.saturating_add(1), dels),
                ChangeTag::Delete => (adds, dels.saturating_add(1)),
                ChangeTag::Equal => (adds, dels),
            }
        })
}

fn line_count_for_change(text: &str) -> usize {
    text.lines().count().max(usize::from(!text.is_empty()))
}

fn tool_selector_summary(tool: &ToolCardModel) -> String {
    let mut pieces = vec![tool.name.clone()];
    if let Some(command_run) = tool.command_run.as_ref() {
        pieces.push(command_run.selector_summary());
    }
    if let Some(summary) = tool.summary.as_deref() {
        let summary = compact_inline_text(summary);
        if !summary.is_empty() {
            pieces.push(summary);
        }
    }
    if pieces.len() == 1 {
        if let Some(section) = tool.sections.first() {
            let body = compact_inline_text(&section.body);
            if !body.is_empty() {
                pieces.push(body);
            }
        }
    }
    pieces.join(" · ")
}

fn command_history_row_from_block(block: &RenderBlock) -> Option<CommandHistoryRowRenderModel> {
    let tool = block.tool.as_ref()?;
    let run = tool.command_run.clone()?;
    let mut row = CommandHistoryRowRenderModel::from_run(block.id.clone(), run)
        .source_block_id(block.id.clone());
    if let Some(stdout) = command_preview_from_sections(&tool.sections, "stdout") {
        row = row.stdout_preview(stdout);
    }
    if let Some(stderr) = command_preview_from_sections(&tool.sections, "stderr") {
        row = row.stderr_preview(stderr);
    }
    Some(row)
}

fn command_summary_from_history_row(row: CommandHistoryRowRenderModel) -> CommandSummaryModel {
    let command = row
        .run
        .command
        .clone()
        .filter(|command| !command.trim().is_empty())
        .unwrap_or(row.title);
    let status = match row.run.status {
        CommandRunStatus::Succeeded => {
            if row.run.exit_code == Some(0) {
                "passed"
            } else {
                "finished"
            }
        }
        CommandRunStatus::Failed | CommandRunStatus::Rejected => "failed",
        CommandRunStatus::Requested
        | CommandRunStatus::Running
        | CommandRunStatus::WaitingApproval => "started",
    }
    .to_string();

    CommandSummaryModel {
        command,
        cwd: row.run.cwd,
        exit_code: row.run.exit_code,
        duration_ms: row.run.duration_ms,
        status,
    }
}

fn tool_input_string_field<'a>(
    obj: &'a serde_json::Map<String, Value>,
    key: &str,
) -> Option<&'a str> {
    obj.get(key).and_then(Value::as_str)
}

fn tool_preview_block(text: &str, max_lines: usize) -> String {
    let mut lines = text
        .lines()
        .take(max_lines)
        .map(|line| format!("  {}", truncate_chars(line, TOOL_SUMMARY_CHARS)))
        .collect::<Vec<_>>();
    let total = text.lines().count();
    if total > max_lines {
        lines.push(format!("  … {} more lines", total - max_lines));
    }
    if lines.is_empty() {
        "  (empty)".to_string()
    } else {
        lines.join("\n")
    }
}

fn tool_simple_unified_diff(old: &str, new: &str) -> String {
    let diff = TextDiff::from_lines(old, new);
    let mut lines = vec!["--- old".to_string(), "+++ new".to_string()];
    for change in diff.iter_all_changes().take(40) {
        let sign = match change.tag() {
            ChangeTag::Delete => "-",
            ChangeTag::Insert => "+",
            ChangeTag::Equal => " ",
        };
        let value = change.value().trim_end_matches('\n');
        lines.push(format!(
            "{} {}",
            sign,
            truncate_chars(value, TOOL_SUMMARY_CHARS)
        ));
    }
    if diff.iter_all_changes().count() > 40 {
        lines.push("… diff truncated".to_string());
    }
    lines.join("\n")
}

fn error_history_rows_from_block(block: &RenderBlock) -> Vec<ErrorHistoryRowRenderModel> {
    let mut rows = block
        .nodes
        .iter()
        .filter_map(|node| match node {
            RenderNode::Error(error) => Some(
                ErrorHistoryRowRenderModel::from_error(
                    block.id.clone(),
                    source_label_for_error_block(block),
                    error.clone(),
                )
                .source_block_id(block.id.clone()),
            ),
            _ => None,
        })
        .collect::<Vec<_>>();

    if let Some(command_row) = command_failure_error_row_from_block(block) {
        rows.push(command_row);
    }
    rows
}

fn final_summary_history_row_from_block(
    block: &RenderBlock,
) -> Option<FinalSummaryHistoryRowRenderModel> {
    block.nodes.iter().find_map(|node| match node {
        RenderNode::FinalSummary(summary) => Some(
            FinalSummaryHistoryRowRenderModel::from_summary(block.id.clone(), summary.clone())
                .source_block_id(block.id.clone()),
        ),
        _ => None,
    })
}

fn approval_history_row_from_block(block: &RenderBlock) -> Option<ApprovalHistoryRowRenderModel> {
    block.nodes.iter().find_map(|node| match node {
        RenderNode::ApprovalDecision(decision) => Some(
            ApprovalHistoryRowRenderModel::from_decision(block.id.clone(), decision.clone())
                .source_block_id(block.id.clone()),
        ),
        _ => None,
    })
}

fn command_failure_error_row_from_block(block: &RenderBlock) -> Option<ErrorHistoryRowRenderModel> {
    let tool = block.tool.as_ref()?;
    let run = tool.command_run.as_ref()?;
    if !run.status.is_failed() && !block.state.error && run.error_summary.is_none() {
        return None;
    }

    let command = run
        .command
        .as_deref()
        .filter(|command| !command.trim().is_empty())
        .unwrap_or("command");
    let summary = run
        .error_summary
        .clone()
        .or_else(|| first_nonempty_section_line(&tool.sections, "stderr"))
        .or_else(|| first_nonempty_section_line(&tool.sections, "stdout"))
        .unwrap_or_else(|| run.status_line());

    let mut details = Vec::new();
    if let Some(command) = run
        .command
        .as_deref()
        .filter(|command| !command.trim().is_empty())
    {
        details.push(format!("command: {command}"));
    }
    if let Some(cwd) = run.cwd.as_deref().filter(|cwd| !cwd.trim().is_empty()) {
        details.push(format!("cwd: {cwd}"));
    }
    details.push(format!("status: {}", run.status_line()));
    if let Some(stderr) = command_preview_from_sections(&tool.sections, "stderr") {
        push_labeled_preview_lines(&mut details, "stderr", &stderr);
    }
    if let Some(stdout) = command_preview_from_sections(&tool.sections, "stdout") {
        push_labeled_preview_lines(&mut details, "stdout", &stdout);
    }
    if run.full_log_available || run.has_embedded_full_log() {
        details.push("full log: inspect /commands for the complete command output".to_string());
    }

    let detail_hidden_line_count = run
        .stdout
        .hidden_line_count
        .saturating_add(run.stderr.hidden_line_count);

    Some(ErrorHistoryRowRenderModel {
        id: format!("{}:command-error", block.id),
        title: if run.status == CommandRunStatus::Rejected {
            "Command rejected".to_string()
        } else {
            "Command failed".to_string()
        },
        source: tool.name.clone(),
        summary: truncate_chars_with_suffix(&summary, TOOL_SUMMARY_CHARS, "..."),
        key_detail: Some(truncate_chars_with_suffix(
            command,
            TOOL_SUMMARY_CHARS,
            "...",
        )),
        details: (!details.is_empty()).then(|| details.join("\n")),
        detail_hidden_line_count,
        retry_hint: None,
        retrying: false,
        command_failure: true,
        source_block_id: Some(block.id.clone()),
    })
}

fn source_label_for_error_block(block: &RenderBlock) -> String {
    block
        .tool
        .as_ref()
        .map(|tool| tool.name.clone())
        .or_else(|| {
            block
                .source_indices
                .first()
                .map(|idx| format!("message {idx}"))
        })
        .unwrap_or_else(|| "transcript".to_string())
}

fn first_nonempty_section_line(sections: &[ToolSection], stream_name: &str) -> Option<String> {
    command_preview_from_sections(sections, stream_name).and_then(|body| {
        body.lines()
            .find(|line| !line.trim().is_empty())
            .map(str::to_string)
    })
}

fn push_labeled_preview_lines(details: &mut Vec<String>, label: &str, text: &str) {
    for (index, line) in text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .take(4)
        .enumerate()
    {
        if index == 0 {
            details.push(format!("{label}: {line}"));
        } else {
            details.push(format!("  {line}"));
        }
    }
}

fn command_preview_from_sections(sections: &[ToolSection], stream_name: &str) -> Option<String> {
    let stream_name_lower = stream_name.to_ascii_lowercase();
    sections
        .iter()
        .find(|section| section.title.eq_ignore_ascii_case(&stream_name_lower))
        .or_else(|| {
            sections
                .iter()
                .find(|section| match stream_name_lower.as_str() {
                    "stdout" => section.kind == ToolSectionKind::Output,
                    "stderr" => section.kind == ToolSectionKind::Error,
                    _ => false,
                })
        })
        .map(|section| section.body.clone())
        .filter(|body| !body.trim().is_empty())
}

/// High-level transcript block type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RenderBlockKind {
    User,
    Assistant,
    System,
    Error,
    FileChangeSummary,
    CommandOutput,
    Progress,
    Attachment,
    Tool,
    ApprovalDecision,
    FinalSummary,
    SkillInvocation,
}

impl RenderBlockKind {
    fn from_record_kind(kind: TranscriptRecordKind) -> Self {
        match kind {
            TranscriptRecordKind::User => Self::User,
            TranscriptRecordKind::Assistant => Self::Assistant,
            TranscriptRecordKind::System => Self::System,
            TranscriptRecordKind::CommandOutput => Self::CommandOutput,
            TranscriptRecordKind::Progress => Self::Progress,
            TranscriptRecordKind::Attachment => Self::Attachment,
            TranscriptRecordKind::ToolUse | TranscriptRecordKind::ToolResult => Self::Tool,
            TranscriptRecordKind::SkillInvocation => Self::SkillInvocation,
        }
    }
}

/// Render-state facts that are independent from viewport layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct RenderBlockState {
    pub streaming: bool,
    pub error: bool,
    pub expanded: bool,
}

impl RenderBlockState {
    fn from_record(record: &TranscriptRecord) -> Self {
        Self {
            streaming: record.is_streaming || matches!(record.lifecycle, LifecyclePhase::Streaming),
            error: record.is_error
                || matches!(
                    record.lifecycle,
                    LifecyclePhase::Failed | LifecyclePhase::Cancelled | LifecyclePhase::TimedOut
                ),
            expanded: record.expanded,
        }
    }
}

/// Semantic content nodes. Widgets can choose how to wrap or clip these.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RenderNode {
    Markdown(String),
    PlainText(String),
    Thinking(String),
    Error(ErrorRenderModel),
    FileChangeSummary(FileChangeSummaryRenderModel),
    FinalSummary(FinalSummaryModel),
    ToolCard(ToolCardModel),
    ApprovalDecision(ApprovalDecisionModel),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ErrorRenderModel {
    pub title: String,
    pub summary: String,
    pub key_detail: Option<String>,
    pub details: Option<String>,
    pub detail_hidden_line_count: usize,
    pub retry_hint: Option<String>,
    pub retrying: bool,
}

/// Error and failed-command history extracted from semantic transcript blocks.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ErrorHistoryRenderModel {
    pub summary: ErrorHistorySummaryRenderModel,
    pub rows: Vec<ErrorHistoryRowRenderModel>,
}

impl ErrorHistoryRenderModel {
    pub fn from_rows(rows: Vec<ErrorHistoryRowRenderModel>) -> Self {
        let mut summary = ErrorHistorySummaryRenderModel {
            total_count: rows.len(),
            ..ErrorHistorySummaryRenderModel::default()
        };
        for row in &rows {
            if row.command_failure {
                summary.command_failure_count += 1;
            }
            if row.retrying {
                summary.retrying_count += 1;
            }
            if row.detail_hidden_line_count > 0 {
                summary.hidden_detail_count += row.detail_hidden_line_count;
            }
        }
        Self { summary, rows }
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct ErrorHistorySummaryRenderModel {
    pub total_count: usize,
    pub command_failure_count: usize,
    pub retrying_count: usize,
    pub hidden_detail_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ErrorHistoryRowRenderModel {
    pub id: String,
    pub title: String,
    pub source: String,
    pub summary: String,
    pub key_detail: Option<String>,
    pub details: Option<String>,
    pub detail_hidden_line_count: usize,
    pub retry_hint: Option<String>,
    pub retrying: bool,
    pub command_failure: bool,
    pub source_block_id: Option<String>,
}

impl ErrorHistoryRowRenderModel {
    pub fn from_error(
        id: impl Into<String>,
        source: impl Into<String>,
        error: ErrorRenderModel,
    ) -> Self {
        Self {
            id: id.into(),
            title: error.title,
            source: source.into(),
            summary: error.summary,
            key_detail: error.key_detail,
            details: error.details,
            detail_hidden_line_count: error.detail_hidden_line_count,
            retry_hint: error.retry_hint,
            retrying: error.retrying,
            command_failure: false,
            source_block_id: None,
        }
    }

    pub fn source_block_id(mut self, source_block_id: impl Into<String>) -> Self {
        self.source_block_id = Some(source_block_id.into());
        self
    }

    pub fn detail_line_count(&self) -> usize {
        usize::from(
            self.key_detail
                .as_deref()
                .is_some_and(|detail| !detail.trim().is_empty()),
        )
        .saturating_add(
            self.details
                .as_deref()
                .map(line_count_for_change)
                .unwrap_or_default(),
        )
        .saturating_add(usize::from(self.detail_hidden_line_count > 0))
        .saturating_add(usize::from(
            self.retry_hint
                .as_deref()
                .is_some_and(|hint| !hint.trim().is_empty()),
        ))
    }

    pub fn has_details(&self) -> bool {
        self.detail_line_count() > 0
    }
}

/// Final result summaries extracted from semantic transcript blocks.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FinalSummaryHistoryRenderModel {
    pub summary: FinalSummaryHistorySummaryRenderModel,
    pub rows: Vec<FinalSummaryHistoryRowRenderModel>,
}

impl FinalSummaryHistoryRenderModel {
    pub fn from_rows(rows: Vec<FinalSummaryHistoryRowRenderModel>) -> Self {
        let mut summary = FinalSummaryHistorySummaryRenderModel {
            total_count: rows.len(),
            ..FinalSummaryHistorySummaryRenderModel::default()
        };
        for row in &rows {
            if row.success {
                summary.completed_count += 1;
            } else {
                summary.attention_count += 1;
            }
            summary.changed_file_count += row.changed_files.len();
            summary.command_count += row.commands.len();
            summary.verification_count += row.verification_results.len();
            summary.risk_count += row.residual_risks.len();
        }
        Self { summary, rows }
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct FinalSummaryHistorySummaryRenderModel {
    pub total_count: usize,
    pub completed_count: usize,
    pub attention_count: usize,
    pub changed_file_count: usize,
    pub command_count: usize,
    pub verification_count: usize,
    pub risk_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FinalSummaryHistoryRowRenderModel {
    pub id: String,
    pub title: String,
    pub success: bool,
    pub terminal: String,
    pub changed_files: Vec<FileChangeSummaryModel>,
    pub commands: Vec<CommandSummaryModel>,
    pub verification_results: Vec<VerificationSummaryModel>,
    pub residual_risks: Vec<String>,
    pub notes: Vec<String>,
    pub source_block_id: Option<String>,
}

impl FinalSummaryHistoryRowRenderModel {
    pub fn from_summary(id: impl Into<String>, summary: FinalSummaryModel) -> Self {
        let title = summary.title().to_string();
        Self {
            id: id.into(),
            title,
            success: summary.success,
            terminal: summary.terminal,
            changed_files: summary.changed_files,
            commands: summary.commands,
            verification_results: summary.verification_results,
            residual_risks: summary.residual_risks,
            notes: summary.notes,
            source_block_id: None,
        }
    }

    pub fn source_block_id(mut self, source_block_id: impl Into<String>) -> Self {
        self.source_block_id = Some(source_block_id.into());
        self
    }

    pub fn status_label(&self) -> &'static str {
        if self.success {
            "Completed"
        } else {
            "Needs attention"
        }
    }

    pub fn detail_line_count(&self) -> usize {
        usize::from(!self.terminal.trim().is_empty())
            .saturating_add(self.changed_files.len())
            .saturating_add(self.commands.len())
            .saturating_add(self.verification_results.len())
            .saturating_add(self.residual_risks.len())
            .saturating_add(self.notes.len())
    }

    pub fn has_details(&self) -> bool {
        self.detail_line_count() > 0
    }
}

/// Approval decisions and pending approval prompts extracted for inspection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalHistoryRenderModel {
    pub summary: ApprovalHistorySummaryRenderModel,
    pub rows: Vec<ApprovalHistoryRowRenderModel>,
}

impl ApprovalHistoryRenderModel {
    pub fn from_rows(rows: Vec<ApprovalHistoryRowRenderModel>) -> Self {
        let mut summary = ApprovalHistorySummaryRenderModel {
            total_count: rows.len(),
            ..ApprovalHistorySummaryRenderModel::default()
        };
        for row in &rows {
            match row.status {
                ApprovalHistoryStatus::Pending => summary.pending_count += 1,
                ApprovalHistoryStatus::Allowed | ApprovalHistoryStatus::AlwaysAllowed => {
                    summary.allowed_count += 1;
                }
                ApprovalHistoryStatus::Denied => summary.denied_count += 1,
                ApprovalHistoryStatus::Cancelled => summary.cancelled_count += 1,
            }
            if row.risk == Some(ApprovalRiskLevel::High) {
                summary.high_risk_count += 1;
            }
        }
        Self { summary, rows }
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ApprovalHistorySummaryRenderModel {
    pub total_count: usize,
    pub pending_count: usize,
    pub allowed_count: usize,
    pub denied_count: usize,
    pub cancelled_count: usize,
    pub high_risk_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalHistoryRowRenderModel {
    pub id: String,
    pub status: ApprovalHistoryStatus,
    pub tool_name: String,
    pub title: String,
    pub detail_label: String,
    pub detail: String,
    pub risk: Option<ApprovalRiskLevel>,
    pub body: Option<String>,
    pub actions: Vec<ApprovalAction>,
    pub selected_action: Option<ApprovalAction>,
    pub anchor_block_id: Option<String>,
    pub source_block_id: Option<String>,
}

impl ApprovalHistoryRowRenderModel {
    pub fn from_decision(id: impl Into<String>, decision: ApprovalDecisionModel) -> Self {
        Self {
            id: id.into(),
            status: ApprovalHistoryStatus::from_decision(decision.decision),
            tool_name: decision.tool_name.clone(),
            title: decision.decision.label().to_string(),
            detail_label: "Detail".to_string(),
            detail: decision.detail,
            risk: None,
            body: None,
            actions: Vec::new(),
            selected_action: None,
            anchor_block_id: decision.anchor_block_id,
            source_block_id: None,
        }
    }

    pub fn from_pending(model: ApprovalRenderModel) -> Self {
        Self {
            id: model.id,
            status: ApprovalHistoryStatus::Pending,
            tool_name: model.tool_name,
            title: model.title,
            detail_label: model.detail_label,
            detail: model.detail,
            risk: Some(model.risk),
            body: (!model.body.trim().is_empty()).then_some(model.body),
            actions: model.actions,
            selected_action: Some(model.selected_action),
            anchor_block_id: model.anchor_block_id,
            source_block_id: None,
        }
    }

    pub fn source_block_id(mut self, source_block_id: impl Into<String>) -> Self {
        self.source_block_id = Some(source_block_id.into());
        self
    }

    pub fn status_label(&self) -> &'static str {
        self.status.label()
    }

    pub fn detail_line_count(&self) -> usize {
        usize::from(self.selected_action.is_some())
            .saturating_add(usize::from(!self.actions.is_empty()))
            .saturating_add(
                self.body
                    .as_deref()
                    .map(line_count_for_change)
                    .unwrap_or_default(),
            )
            .saturating_add(usize::from(self.anchor_block_id.is_some()))
            .saturating_add(usize::from(self.source_block_id.is_some()))
    }

    pub fn has_details(&self) -> bool {
        self.detail_line_count() > 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ApprovalHistoryStatus {
    Pending,
    Allowed,
    AlwaysAllowed,
    Denied,
    Cancelled,
}

impl ApprovalHistoryStatus {
    fn from_decision(decision: ApprovalDecisionKind) -> Self {
        match decision {
            ApprovalDecisionKind::Allowed => Self::Allowed,
            ApprovalDecisionKind::AlwaysAllowed => Self::AlwaysAllowed,
            ApprovalDecisionKind::Denied => Self::Denied,
            ApprovalDecisionKind::Cancelled => Self::Cancelled,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Pending => "Pending",
            Self::Allowed => "Allowed",
            Self::AlwaysAllowed => "Always allowed",
            Self::Denied => "Denied",
            Self::Cancelled => "Cancelled",
        }
    }

    pub fn is_negative(self) -> bool {
        matches!(self, Self::Denied | Self::Cancelled)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FileChangeSummaryRenderModel {
    pub files: Vec<FileChangeSummaryModel>,
}

impl FileChangeSummaryRenderModel {
    pub fn title(&self) -> String {
        match self.files.len() {
            1 => "Changed 1 file".to_string(),
            count => format!("Changed {count} files"),
        }
    }

    pub fn total_additions(&self) -> usize {
        self.files
            .iter()
            .map(|file| file.additions)
            .fold(0usize, usize::saturating_add)
    }

    pub fn total_deletions(&self) -> usize {
        self.files
            .iter()
            .map(|file| file.deletions)
            .fold(0usize, usize::saturating_add)
    }

    pub fn count_with_status(&self, status: &str) -> usize {
        self.files
            .iter()
            .filter(|file| file.status == status)
            .count()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FileChangeListRenderModel {
    pub summary: FileChangeListSummaryRenderModel,
    pub rows: Vec<FileChangeRowRenderModel>,
}

impl FileChangeListRenderModel {
    pub fn from_files(files: Vec<FileChangeSummaryModel>) -> Self {
        let rows: Vec<FileChangeRowRenderModel> = files
            .into_iter()
            .map(FileChangeRowRenderModel::from_summary)
            .collect();
        let summary = FileChangeListSummaryRenderModel::from_rows(&rows);
        Self { summary, rows }
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FileChangeListSummaryRenderModel {
    pub total_count: usize,
    pub modified_count: usize,
    pub added_count: usize,
    pub deleted_count: usize,
    pub other_count: usize,
    pub total_additions: usize,
    pub total_deletions: usize,
}

impl FileChangeListSummaryRenderModel {
    fn from_rows(rows: &[FileChangeRowRenderModel]) -> Self {
        Self {
            total_count: rows.len(),
            modified_count: rows.iter().filter(|row| row.is_modified()).count(),
            added_count: rows.iter().filter(|row| row.is_added()).count(),
            deleted_count: rows.iter().filter(|row| row.is_deleted()).count(),
            other_count: rows.iter().filter(|row| row.is_other()).count(),
            total_additions: rows
                .iter()
                .map(|row| row.additions)
                .fold(0usize, usize::saturating_add),
            total_deletions: rows
                .iter()
                .map(|row| row.deletions)
                .fold(0usize, usize::saturating_add),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FileChangeRowRenderModel {
    pub path: String,
    pub status: String,
    pub additions: usize,
    pub deletions: usize,
}

impl FileChangeRowRenderModel {
    fn from_summary(summary: FileChangeSummaryModel) -> Self {
        Self {
            path: summary.path,
            status: summary.status,
            additions: summary.additions,
            deletions: summary.deletions,
        }
    }

    pub fn status_label(&self) -> &'static str {
        match self.status.as_str() {
            "A" => "A",
            "D" => "D",
            "M" => "M",
            "R" => "R",
            _ => "?",
        }
    }

    pub fn status_name(&self) -> &'static str {
        match self.status.as_str() {
            "A" => "Added",
            "D" => "Deleted",
            "M" => "Modified",
            "R" => "Renamed",
            _ => "Changed",
        }
    }

    fn is_added(&self) -> bool {
        self.status == "A"
    }

    fn is_deleted(&self) -> bool {
        self.status == "D"
    }

    fn is_modified(&self) -> bool {
        self.status == "M"
    }

    fn is_other(&self) -> bool {
        !self.is_added() && !self.is_deleted() && !self.is_modified()
    }
}

/// Append-only render lifecycle timeline extracted from structured events.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RenderTimelineRenderModel {
    pub summary: RenderTimelineSummaryRenderModel,
    pub rows: Vec<RenderTimelineRowRenderModel>,
}

impl RenderTimelineRenderModel {
    pub fn from_events(events: &[RenderEvent]) -> Self {
        let rows = events
            .iter()
            .enumerate()
            .map(|(index, event)| RenderTimelineRowRenderModel::from_event(index, event))
            .collect::<Vec<_>>();
        let summary = RenderTimelineSummaryRenderModel::from_rows(&rows);
        Self { summary, rows }
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RenderTimelineSummaryRenderModel {
    pub total_count: usize,
    pub turn_count: usize,
    pub immediate_count: usize,
    pub throttled_count: usize,
    pub passive_count: usize,
    pub append_count: usize,
    pub update_active_count: usize,
    pub freeze_history_count: usize,
}

impl RenderTimelineSummaryRenderModel {
    fn from_rows(rows: &[RenderTimelineRowRenderModel]) -> Self {
        Self {
            total_count: rows.len(),
            turn_count: rows
                .iter()
                .filter_map(|row| row.turn_id.as_deref())
                .collect::<HashSet<_>>()
                .len(),
            immediate_count: rows.iter().filter(|row| row.refresh == "immediate").count(),
            throttled_count: rows
                .iter()
                .filter(|row| row.refresh.starts_with("throttled"))
                .count(),
            passive_count: rows.iter().filter(|row| row.refresh == "passive").count(),
            append_count: rows.iter().filter(|row| row.history == "append").count(),
            update_active_count: rows
                .iter()
                .filter(|row| row.history == "update active")
                .count(),
            freeze_history_count: rows
                .iter()
                .filter(|row| row.history == "freeze history")
                .count(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RenderTimelineRowRenderModel {
    pub index: usize,
    pub event: String,
    pub turn_id: Option<String>,
    pub stage: String,
    pub scope: String,
    pub refresh: String,
    pub history: String,
    pub summary: String,
    pub detail: Option<String>,
}

impl RenderTimelineRowRenderModel {
    fn from_event(index: usize, event: &RenderEvent) -> Self {
        let (event_name, summary, detail) = render_timeline_event_text(event);
        Self {
            index,
            event: event_name.to_string(),
            turn_id: event.turn_id.clone(),
            stage: event.stage.label().to_string(),
            scope: scope_label(&event.scope),
            refresh: refresh_label(event.refresh),
            history: history_label(event.history).to_string(),
            summary,
            detail,
        }
    }

    pub fn sequence_label(&self) -> String {
        format!("#{:03}", self.index.saturating_add(1))
    }
}

fn render_timeline_event_text(event: &RenderEvent) -> (&'static str, String, Option<String>) {
    match &event.kind {
        RenderEventKind::TurnStarted => ("turn_started", "turn started".to_string(), None),
        RenderEventKind::StreamStarted => ("stream_started", "stream started".to_string(), None),
        RenderEventKind::TextDelta { bytes } => (
            "assistant_message",
            format!("assistant text delta: {bytes} bytes"),
            None,
        ),
        RenderEventKind::ThinkingDelta { bytes } => {
            ("thinking", format!("thinking delta: {bytes} bytes"), None)
        }
        RenderEventKind::ToolInputDelta { bytes } => (
            "tool_input",
            format!("tool input delta: {bytes} bytes"),
            None,
        ),
        RenderEventKind::CommandStarted {
            tool_id,
            command,
            cwd,
        } => {
            let summary = command
                .as_deref()
                .map(|command| format!("command started: {command}"))
                .unwrap_or_else(|| "command started".to_string());
            (
                "command_start",
                summary,
                Some(render_timeline_detail([
                    tool_id
                        .as_deref()
                        .map(|id| format!("tool id: {}", short_id(id))),
                    command
                        .as_deref()
                        .map(|command| format!("command: {command}")),
                    cwd.as_deref().map(|cwd| format!("cwd: {cwd}")),
                ])),
            )
        }
        RenderEventKind::CommandOutput {
            tool_id,
            stream,
            bytes,
            preview_lines,
            hidden_lines,
            total_lines,
            full_log_available,
        } => {
            let mut parts = vec![
                format!("{stream}: {preview_lines} line(s) shown"),
                format!("{bytes} bytes"),
            ];
            if *hidden_lines > 0 {
                parts.push(format!("{hidden_lines} line(s) hidden"));
            }
            if let Some(total_lines) = total_lines {
                parts.push(format!("{total_lines} line(s) total"));
            }
            if *full_log_available {
                parts.push("full log available".to_string());
            }
            (
                "command_output",
                parts.join(", "),
                tool_id
                    .as_deref()
                    .map(|id| format!("tool id: {}", short_id(id))),
            )
        }
        RenderEventKind::CommandFinished {
            tool_id,
            exit_code,
            duration_ms,
        } => {
            let exit = exit_code
                .map(|code| format!("exit {code}"))
                .unwrap_or_else(|| "exit unknown".to_string());
            let duration = duration_ms
                .map(|duration| format!("{duration}ms"))
                .unwrap_or_else(|| "duration unknown".to_string());
            (
                "command_finish",
                format!("command finished: {exit}, {duration}"),
                tool_id
                    .as_deref()
                    .map(|id| format!("tool id: {}", short_id(id))),
            )
        }
        RenderEventKind::BackgroundTaskUpdated {
            tool_id,
            task_id,
            task_type,
            status,
            command,
            preview_lines,
            hidden_lines,
            exit_code,
        } => (
            "background_task",
            format!("background task {status}: {}", short_id(task_id)),
            Some(render_timeline_detail([
                Some(format!("task type: {task_type}")),
                command
                    .as_deref()
                    .map(|command| format!("command: {command}")),
                Some(format!(
                    "output: {preview_lines} shown, {hidden_lines} hidden"
                )),
                exit_code.map(|code| format!("exit: {code}")),
                tool_id
                    .as_deref()
                    .map(|id| format!("tool id: {}", short_id(id))),
            ])),
        ),
        RenderEventKind::ToolRequested { tool_name, tool_id } => (
            "tool_requested",
            format!("tool requested: {}", display_tool_name(tool_name)),
            tool_id
                .as_deref()
                .map(|id| format!("tool id: {}", short_id(id))),
        ),
        RenderEventKind::ToolCompleted { tool_name, tool_id } => (
            "tool_completed",
            format!("tool completed: {}", display_tool_name(tool_name)),
            tool_id
                .as_deref()
                .map(|id| format!("tool id: {}", short_id(id))),
        ),
        RenderEventKind::PlanUpdated {
            tool_id,
            step_count,
            completed_count,
            active_count,
            pending_count,
            blocked_count,
            active_step,
        } => {
            let summary = plan_event_summary_line(
                *step_count,
                *completed_count,
                *active_count,
                *pending_count,
                *blocked_count,
                active_step.as_deref(),
            );
            (
                "plan_updated",
                summary.clone(),
                Some(render_timeline_detail([
                    Some(summary),
                    tool_id
                        .as_deref()
                        .map(|id| format!("tool id: {}", short_id(id))),
                ])),
            )
        }
        RenderEventKind::FileChangeSummary {
            tool_id,
            file_count,
            additions,
            deletions,
        } => (
            "file_change_summary",
            format!("file changes: {file_count} file(s), +{additions} -{deletions}"),
            tool_id
                .as_deref()
                .map(|id| format!("tool id: {}", short_id(id))),
        ),
        RenderEventKind::DiffAvailable {
            tool_id,
            file_count,
            additions,
            deletions,
        } => (
            "diff_available",
            format!("diff available: {file_count} file(s), +{additions} -{deletions}"),
            tool_id
                .as_deref()
                .map(|id| format!("tool id: {}", short_id(id))),
        ),
        RenderEventKind::ApprovalRequested { tool_name } => (
            "approval_request",
            format!("approval requested: {}", display_tool_name(tool_name)),
            None,
        ),
        RenderEventKind::ErrorRaised { source, summary } => {
            ("error", format!("error from {source}: {summary}"), None)
        }
        RenderEventKind::ApiRetry {
            attempt,
            max_retries,
            retry_in_ms,
        } => (
            "api_retry",
            format!("api retry {attempt}/{max_retries} in {retry_in_ms}ms"),
            None,
        ),
        RenderEventKind::CompactBoundary {
            before_token_count,
            after_token_count,
        } => (
            "compact_boundary",
            format!("compact boundary: {before_token_count} -> {after_token_count} tokens"),
            None,
        ),
        RenderEventKind::CompactRequestStatus {
            request_id,
            status,
            reason,
            ..
        } => (
            "compact_request_status",
            format!("compact request {status}: {}", short_id(request_id)),
            reason.clone(),
        ),
        RenderEventKind::ConversationCleared {
            message_count_before,
            message_count_after,
        } => (
            "conversation_cleared",
            format!(
                "conversation cleared: {message_count_before} -> {message_count_after} messages"
            ),
            None,
        ),
        RenderEventKind::ClearRequestStatus {
            request_id,
            status,
            reason,
            ..
        } => (
            "clear_request_status",
            format!("clear request {status}: {}", short_id(request_id)),
            reason.clone(),
        ),
        RenderEventKind::SlashCommandResult {
            command,
            status,
            summary,
            error,
            ..
        } => (
            "slash_command_result",
            format!("/{command} {status}: {summary}"),
            error.clone(),
        ),
        RenderEventKind::TurnFinished { terminal } => {
            ("turn_finished", format!("turn finished: {terminal}"), None)
        }
        RenderEventKind::FinalSummaryRecorded { terminal, success } => (
            "final_summary",
            if *success {
                "final summary recorded: success".to_string()
            } else {
                format!("final summary recorded: {terminal}")
            },
            None,
        ),
    }
}

fn render_timeline_detail(parts: impl IntoIterator<Item = Option<String>>) -> String {
    let detail = parts.into_iter().flatten().collect::<Vec<_>>().join(" · ");
    truncate_chars_with_suffix(&detail, TIMELINE_DETAIL_CHARS, "...")
}

fn plan_event_summary_line(
    step_count: usize,
    completed_count: usize,
    active_count: usize,
    pending_count: usize,
    blocked_count: usize,
    active_step: Option<&str>,
) -> String {
    let mut parts = vec![format!("plan updated: {step_count} step(s)")];
    if completed_count > 0 {
        parts.push(format!("{completed_count} done"));
    }
    if active_count > 0 {
        parts.push(format!("{active_count} active"));
    }
    if pending_count > 0 {
        parts.push(format!("{pending_count} pending"));
    }
    if blocked_count > 0 {
        parts.push(format!("{blocked_count} blocked"));
    }
    if let Some(step) = active_step.filter(|step| !step.trim().is_empty()) {
        parts.push(format!("active: {step}"));
    }
    parts.join(", ")
}

fn scope_label(scope: &RenderEventScope) -> String {
    match scope {
        RenderEventScope::Main => "main".to_string(),
        RenderEventScope::Task(task_id) => {
            let task_id = truncate_chars_with_suffix(task_id, 48, "...");
            format!("task: {task_id}")
        }
    }
}

fn refresh_label(refresh: RenderRefreshPolicy) -> String {
    match refresh {
        RenderRefreshPolicy::Immediate => "immediate".to_string(),
        RenderRefreshPolicy::Throttled { min_interval_ms } => {
            format!("throttled {min_interval_ms}ms")
        }
        RenderRefreshPolicy::Passive => "passive".to_string(),
    }
}

fn history_label(history: RenderHistoryPolicy) -> &'static str {
    match history {
        RenderHistoryPolicy::Append => "append",
        RenderHistoryPolicy::UpdateActive => "update active",
        RenderHistoryPolicy::FreezeHistory => "freeze history",
    }
}

fn short_id(id: &str) -> String {
    truncate_chars_with_suffix(id, 18, "...")
}

/// Tool card semantics shared by all viewport renderers.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ToolCardModel {
    pub name: String,
    pub phase: ToolPhase,
    pub summary: Option<String>,
    pub sections: Vec<ToolSection>,
    pub command_run: Option<CommandRunRenderModel>,
    pub plan: Option<PlanRenderModel>,
}

impl ToolCardModel {
    pub fn family(&self) -> ToolFamily {
        ToolFamily::from_tool_name(&self.name)
    }

    pub fn product_title(&self) -> String {
        let family = self.family();
        let display_name = display_tool_name(&self.name);
        if family == ToolFamily::Generic {
            display_name
        } else {
            format!("{} · {}", display_name, family.label())
        }
    }
}

/// Structured plan semantics extracted from TodoWrite-style tool payloads.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PlanRenderModel {
    pub title: String,
    pub steps: Vec<PlanStepRenderModel>,
}

impl PlanRenderModel {
    pub fn summary_line(&self) -> String {
        let mut parts = vec![pluralize(self.steps.len(), "step")];
        let completed = self.count_by_status(PlanStepStatus::Completed);
        let active = self.count_by_status(PlanStepStatus::InProgress);
        let pending = self.count_by_status(PlanStepStatus::Pending);
        if completed > 0 {
            parts.push(format!("{completed} done"));
        }
        if active > 0 {
            parts.push(format!("{active} active"));
        }
        if pending > 0 {
            parts.push(format!("{pending} pending"));
        }
        parts.join(" · ")
    }

    pub fn active_step(&self) -> Option<&PlanStepRenderModel> {
        self.steps
            .iter()
            .find(|step| step.status == PlanStepStatus::InProgress)
    }

    fn count_by_status(&self, status: PlanStepStatus) -> usize {
        self.steps
            .iter()
            .filter(|step| step.status == status)
            .count()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PlanStepRenderModel {
    pub status: PlanStepStatus,
    pub label: String,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlanStepStatus {
    Completed,
    InProgress,
    Pending,
    Blocked,
    Cancelled,
    Other,
}

impl PlanStepStatus {
    pub fn from_label(label: &str) -> Self {
        let normalized = label.trim().to_ascii_lowercase().replace(['-', ' '], "_");
        match normalized.as_str() {
            "done" | "complete" | "completed" | "success" | "succeeded" => Self::Completed,
            "active" | "current" | "doing" | "in_progress" | "running" | "started" => {
                Self::InProgress
            }
            "pending" | "todo" | "queued" | "not_started" | "open" => Self::Pending,
            "blocked" | "failed" | "error" => Self::Blocked,
            "cancelled" | "canceled" => Self::Cancelled,
            _ => Self::Other,
        }
    }

    pub fn display_label(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::InProgress => "in_progress",
            Self::Pending => "pending",
            Self::Blocked => "blocked",
            Self::Cancelled => "cancelled",
            Self::Other => "step",
        }
    }
}

/// Semantic command execution facts extracted from Bash/PowerShell tool cards.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CommandRunRenderModel {
    pub command: Option<String>,
    pub cwd: Option<String>,
    pub status: CommandRunStatus,
    pub exit_code: Option<i64>,
    pub duration_ms: Option<u64>,
    pub timed_out: bool,
    pub interrupted: bool,
    pub signal: Option<String>,
    pub error_summary: Option<String>,
    pub stdout: CommandStreamRenderModel,
    pub stderr: CommandStreamRenderModel,
    pub full_log_available: bool,
}

impl CommandRunRenderModel {
    pub fn status_line(&self) -> String {
        let mut parts = vec![self.status.label().to_string()];
        if let Some(exit) = self.exit_code {
            parts.push(format!("exit {exit}"));
        }
        if self.timed_out {
            parts.push("timeout".to_string());
        }
        if self.interrupted {
            parts.push("interrupted".to_string());
        }
        if let Some(signal) = self.signal.as_deref() {
            parts.push(format!("signal {signal}"));
        }
        if let Some(duration) = self.duration_ms {
            parts.push(format!("duration {duration}ms"));
        }
        if let Some(error) = self.error_summary.as_deref() {
            parts.push(format!(
                "error {}",
                truncate_chars_with_suffix(error, 80, "...")
            ));
        }
        parts.join(" · ")
    }

    pub fn output_summary_line(&self) -> String {
        let mut parts = Vec::new();
        if let Some(summary) = self.stdout.summary() {
            parts.push(summary);
        }
        if let Some(summary) = self.stderr.summary() {
            parts.push(summary);
        }
        if self.full_log_available {
            parts.push("full log available".to_string());
        }
        if parts.is_empty() {
            "no output recorded".to_string()
        } else {
            parts.join(" · ")
        }
    }

    pub fn selector_summary(&self) -> String {
        let command = self.command.as_deref().unwrap_or("command");
        format!("{command} · {}", self.status_line())
    }

    pub fn has_embedded_full_log(&self) -> bool {
        self.stdout.has_embedded_full_log() || self.stderr.has_embedded_full_log()
    }

    pub fn full_log_line_count(&self) -> usize {
        self.stdout
            .full_log_line_count()
            .saturating_add(self.stderr.full_log_line_count())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommandRunStatus {
    Requested,
    Running,
    Succeeded,
    Failed,
    WaitingApproval,
    Rejected,
}

impl CommandRunStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Requested => "Requested",
            Self::Running => "Running",
            Self::Succeeded => "Succeeded",
            Self::Failed => "Failed",
            Self::WaitingApproval => "Waiting for approval",
            Self::Rejected => "Rejected",
        }
    }

    pub fn is_running(self) -> bool {
        matches!(
            self,
            Self::Requested | Self::Running | Self::WaitingApproval
        )
    }

    pub fn is_failed(self) -> bool {
        matches!(self, Self::Failed | Self::Rejected)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CommandStreamRenderModel {
    pub name: String,
    pub preview_line_count: usize,
    pub hidden_line_count: usize,
    pub total_line_count: Option<usize>,
    pub has_content: bool,
    pub full_log_available: bool,
    pub full_text: Option<String>,
}

impl CommandStreamRenderModel {
    pub fn empty(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            preview_line_count: 0,
            hidden_line_count: 0,
            total_line_count: None,
            has_content: false,
            full_log_available: false,
            full_text: None,
        }
    }

    pub fn has_embedded_full_log(&self) -> bool {
        self.full_text
            .as_deref()
            .is_some_and(|text| !text.trim().is_empty())
    }

    pub fn full_log_line_count(&self) -> usize {
        self.full_text
            .as_deref()
            .map(line_count_for_change)
            .unwrap_or_default()
    }

    pub fn summary(&self) -> Option<String> {
        if !self.has_content && self.hidden_line_count == 0 && self.total_line_count.is_none() {
            return None;
        }

        let shown = pluralize(self.preview_line_count, "line");
        let mut summary = format!("{} {} shown", self.name, shown);
        if self.hidden_line_count > 0 {
            summary.push_str(&format!(
                ", {} hidden",
                pluralize(self.hidden_line_count, "line")
            ));
        }
        if let Some(total) = self.total_line_count {
            summary.push_str(&format!(", {} total", pluralize(total, "line")));
        }
        Some(summary)
    }
}

/// Command execution history extracted from semantic tool cards.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CommandHistoryRenderModel {
    pub summary: CommandHistorySummaryRenderModel,
    pub rows: Vec<CommandHistoryRowRenderModel>,
}

impl CommandHistoryRenderModel {
    pub fn from_rows(rows: Vec<CommandHistoryRowRenderModel>) -> Self {
        let mut summary = CommandHistorySummaryRenderModel {
            total_count: rows.len(),
            ..CommandHistorySummaryRenderModel::default()
        };
        for row in &rows {
            if row.run.status.is_running() {
                summary.running_count += 1;
            }
            if row.run.status.is_failed() {
                summary.failed_count += 1;
            }
            if row.run.full_log_available {
                summary.full_log_count += 1;
            }
        }
        Self { summary, rows }
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct CommandHistorySummaryRenderModel {
    pub total_count: usize,
    pub running_count: usize,
    pub failed_count: usize,
    pub full_log_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CommandHistoryRowRenderModel {
    pub id: String,
    pub title: String,
    pub run: CommandRunRenderModel,
    pub stdout_preview: Option<String>,
    pub stderr_preview: Option<String>,
    pub source_block_id: Option<String>,
}

impl CommandHistoryRowRenderModel {
    pub fn from_run(id: impl Into<String>, run: CommandRunRenderModel) -> Self {
        let title = run
            .command
            .clone()
            .filter(|command| !command.trim().is_empty())
            .unwrap_or_else(|| "Command execution".to_string());
        Self {
            id: id.into(),
            title,
            run,
            stdout_preview: None,
            stderr_preview: None,
            source_block_id: None,
        }
    }

    pub fn stdout_preview(mut self, preview: impl Into<String>) -> Self {
        let preview = preview.into();
        if !preview.trim().is_empty() {
            self.stdout_preview = Some(preview);
        }
        self
    }

    pub fn stderr_preview(mut self, preview: impl Into<String>) -> Self {
        let preview = preview.into();
        if !preview.trim().is_empty() {
            self.stderr_preview = Some(preview);
        }
        self
    }

    pub fn source_block_id(mut self, source_block_id: impl Into<String>) -> Self {
        self.source_block_id = Some(source_block_id.into());
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolFamily {
    Command,
    FileRead,
    FileChange,
    Search,
    Plan,
    SubAgent,
    Network,
    Mcp,
    Skill,
    Prompt,
    Generic,
}

impl ToolFamily {
    pub fn from_tool_name(tool_name: &str) -> Self {
        let normalized = normalize_tool_family(tool_name);
        match normalized.as_str() {
            "bash" | "powershell" => Self::Command,
            "read" | "readmcpresource" => Self::FileRead,
            "write" | "edit" | "multiedit" | "notebookedit" => Self::FileChange,
            "grep"
            | "glob"
            | "websearch"
            | "toolsearch"
            | "listmcpresources"
            | "listmcpresourcestool" => Self::Search,
            "todowrite" | "exitplanmode" | "taskcreate" | "tasklist" | "taskget" | "taskupdate" => {
                Self::Plan
            }
            "task" | "agent" | "taskoutput" | "taskstop" => Self::SubAgent,
            "webfetch" => Self::Network,
            "skill" => Self::Skill,
            "askuserquestion" => Self::Prompt,
            _ if is_mcp_tool_name(tool_name) => Self::Mcp,
            _ => Self::Generic,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Command => "Command",
            Self::FileRead => "File Read",
            Self::FileChange => "File Change",
            Self::Search => "Search",
            Self::Plan => "Plan",
            Self::SubAgent => "Sub-agent",
            Self::Network => "Network",
            Self::Mcp => "MCP",
            Self::Skill => "Skill",
            Self::Prompt => "Prompt",
            Self::Generic => "Tool",
        }
    }
}

/// Tool lifecycle as visible UI state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolPhase {
    Requested,
    Running,
    Succeeded,
    Failed,
    WaitingApproval,
    Rejected,
}

/// A semantic section inside a tool card.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ToolSection {
    pub title: String,
    pub body: String,
    pub kind: ToolSectionKind,
    pub code: Option<CodeSectionRenderModel>,
}

impl ToolSection {
    pub fn new(title: impl Into<String>, body: impl Into<String>, kind: ToolSectionKind) -> Self {
        Self {
            title: title.into(),
            body: body.into(),
            kind,
            code: None,
        }
    }

    pub fn with_code(mut self, code: CodeSectionRenderModel) -> Self {
        self.code = Some(code);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CodeSectionRenderModel {
    pub file_path: Option<String>,
    pub start_line: usize,
    pub line_numbers: bool,
    pub hidden_lines: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolSectionKind {
    Input,
    Output,
    Diff,
    Error,
    Metadata,
}

/// Footer state should be built once and rendered by profile-specific views.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FooterRenderModel {
    pub project: Option<String>,
    pub model: Option<String>,
    pub access_mode: Option<String>,
    pub reasoning: Option<String>,
    pub context: Option<ContextUsageRenderModel>,
    pub turn_state: Option<String>,
    pub activity: Option<String>,
    pub cost: Option<String>,
    pub message_count: Option<usize>,
    pub mcp_summary: Option<String>,
    pub external_status: Option<String>,
    pub blocking: Option<BlockingRenderModel>,
    pub config: FooterRenderConfig,
}

/// One-line top-of-screen status facts.
///
/// This is intentionally derived from the same Layer 2 facts as the footer so
/// status stays consistent without forcing widgets to reread App state.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TopStatusRenderModel {
    pub stage: Option<String>,
    pub activity: Option<String>,
    pub model: Option<String>,
    pub access_mode: Option<String>,
    pub reasoning: Option<String>,
    pub context: Option<ContextUsageRenderModel>,
    pub message_count: Option<usize>,
    pub blocking: Option<BlockingRenderModel>,
}

impl TopStatusRenderModel {
    pub fn from_footer(footer: &FooterRenderModel) -> Self {
        Self {
            stage: footer.turn_state.clone(),
            activity: footer.activity.clone(),
            model: footer.model.clone(),
            access_mode: footer.access_mode.clone(),
            reasoning: footer.reasoning.clone(),
            context: footer.context,
            message_count: footer.message_count,
            blocking: footer.blocking.clone(),
        }
    }
}

/// Viewport-independent session overview used by `/status`.
///
/// This is the detailed companion to the one-line top status and footer. It
/// deliberately carries semantic rows only; terminal width, clipping, and
/// colors are owned by the widget layer.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StatusOverviewRenderModel {
    pub summary: String,
    pub sections: Vec<StatusSectionRenderModel>,
    pub footer: String,
}

impl StatusOverviewRenderModel {
    pub fn new(summary: impl Into<String>) -> Self {
        Self {
            summary: summary.into(),
            sections: Vec::new(),
            footer: "Esc closes".to_string(),
        }
    }

    pub fn section(mut self, section: StatusSectionRenderModel) -> Self {
        if !section.rows.is_empty() {
            self.sections.push(section);
        }
        self
    }

    pub fn footer(mut self, footer: impl Into<String>) -> Self {
        let footer = footer.into();
        if !footer.trim().is_empty() {
            self.footer = footer;
        }
        self
    }

    pub fn is_empty(&self) -> bool {
        self.sections.iter().all(|section| section.rows.is_empty())
    }
}

/// Redacted configuration facts used by `/debug-config`.
///
/// This is intentionally separate from `/raw`: it exposes semantic
/// configuration state that helps diagnose the renderer without printing raw
/// messages, environment values, request bodies, or credentials.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DebugConfigRenderModel {
    pub summary: String,
    pub sections: Vec<StatusSectionRenderModel>,
    pub footer: String,
}

impl DebugConfigRenderModel {
    pub fn new(summary: impl Into<String>) -> Self {
        Self {
            summary: summary.into(),
            sections: Vec::new(),
            footer: "Esc closes".to_string(),
        }
    }

    pub fn section(mut self, section: StatusSectionRenderModel) -> Self {
        if !section.rows.is_empty() {
            self.sections.push(section);
        }
        self
    }

    pub fn footer(mut self, footer: impl Into<String>) -> Self {
        let footer = footer.into();
        if !footer.trim().is_empty() {
            self.footer = footer;
        }
        self
    }

    pub fn row_count(&self) -> usize {
        self.sections
            .iter()
            .map(|section| 1usize.saturating_add(section.rows.len()))
            .sum()
    }

    pub fn is_empty(&self) -> bool {
        self.sections.iter().all(|section| section.rows.is_empty())
    }
}

/// Terminal title facts used by `/title`.
///
/// This is a semantic TUI model for terminal chrome. It keeps title
/// inspection separate from raw OSC escape output so normal frames never show
/// terminal control sequences.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionTitleRenderModel {
    pub current_title: String,
    pub custom_title: Option<String>,
    pub draft: String,
    pub status: String,
    pub footer: String,
}

impl SessionTitleRenderModel {
    pub fn new(
        current_title: impl Into<String>,
        custom_title: Option<String>,
        draft: impl Into<String>,
    ) -> Self {
        Self {
            current_title: current_title.into(),
            custom_title,
            draft: draft.into(),
            status: "terminal title".to_string(),
            footer: "Enter saves".to_string(),
        }
    }

    pub fn status(mut self, status: impl Into<String>) -> Self {
        let status = status.into();
        if !status.trim().is_empty() {
            self.status = status;
        }
        self
    }

    pub fn footer(mut self, footer: impl Into<String>) -> Self {
        let footer = footer.into();
        if !footer.trim().is_empty() {
            self.footer = footer;
        }
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StatusSectionRenderModel {
    pub title: String,
    pub rows: Vec<StatusRowRenderModel>,
}

impl StatusSectionRenderModel {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            rows: Vec::new(),
        }
    }

    pub fn row(
        mut self,
        label: impl Into<String>,
        value: impl Into<String>,
        level: StatusRowLevel,
    ) -> Self {
        let label = label.into();
        let value = value.into();
        if !label.trim().is_empty() && !value.trim().is_empty() {
            self.rows.push(StatusRowRenderModel {
                label,
                value,
                level,
            });
        }
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StatusRowRenderModel {
    pub label: String,
    pub value: String,
    pub level: StatusRowLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StatusRowLevel {
    Normal,
    Good,
    Warning,
    Error,
    Info,
}

/// Compact active-turn panel shown above transcript history.
///
/// The model represents the live panel that can refresh in place while
/// completed transcript blocks stay readable below it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ActivityPanelRenderModel {
    pub stage: String,
    pub title: String,
    pub summary: Option<String>,
    pub details: Vec<ActivityPanelDetail>,
    pub severity: ActivityPanelSeverity,
}

impl ActivityPanelRenderModel {
    pub fn new(
        stage: impl Into<String>,
        title: impl Into<String>,
        severity: ActivityPanelSeverity,
    ) -> Self {
        Self {
            stage: stage.into(),
            title: title.into(),
            summary: None,
            details: Vec::new(),
            severity,
        }
    }

    pub fn summary(mut self, summary: impl Into<String>) -> Self {
        let summary = summary.into();
        if !summary.trim().is_empty() {
            self.summary = Some(summary);
        }
        self
    }

    pub fn detail(mut self, label: impl Into<String>, value: impl Into<String>) -> Self {
        let label = label.into();
        let value = value.into();
        if !label.trim().is_empty() && !value.trim().is_empty() {
            self.details.push(ActivityPanelDetail { label, value });
        }
        self
    }

    pub fn from_blocking(blocking: &BlockingRenderModel) -> Self {
        let (stage, severity) = match blocking.kind {
            BlockingKind::Approval => ("waiting approval", ActivityPanelSeverity::Waiting),
            BlockingKind::Error => ("failed", ActivityPanelSeverity::Error),
            BlockingKind::CostLimit => ("waiting approval", ActivityPanelSeverity::Warning),
            BlockingKind::IdleReturn | BlockingKind::Info => {
                ("waiting", ActivityPanelSeverity::Info)
            }
        };
        Self::new(stage, blocking.title.clone(), severity).summary(blocking.detail.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ActivityPanelDetail {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActivityPanelSeverity {
    Info,
    Working,
    Waiting,
    Success,
    Warning,
    Error,
}

/// Viewport-independent process/activity snapshot used by `/ps`.
///
/// This is a read-only inspection model. It describes what is currently
/// active, waiting, failed, or idle without reaching back into execution code.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProcessListRenderModel {
    pub summary: ProcessSummaryRenderModel,
    pub rows: Vec<ProcessRowRenderModel>,
}

impl ProcessListRenderModel {
    pub fn new(summary: ProcessSummaryRenderModel, rows: Vec<ProcessRowRenderModel>) -> Self {
        Self { summary, rows }
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProcessSummaryRenderModel {
    pub stage: String,
    pub turn_state: String,
    pub active_count: usize,
    pub waiting_count: usize,
    pub failed_count: usize,
}

impl ProcessSummaryRenderModel {
    pub fn new(stage: impl Into<String>, turn_state: impl Into<String>) -> Self {
        Self {
            stage: stage.into(),
            turn_state: turn_state.into(),
            active_count: 0,
            waiting_count: 0,
            failed_count: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProcessRowRenderModel {
    pub id: String,
    pub kind: ProcessRowKind,
    pub status: ProcessStatus,
    pub title: String,
    pub detail: Option<String>,
    pub facts: Vec<ProcessFactRenderModel>,
}

impl ProcessRowRenderModel {
    pub fn new(
        id: impl Into<String>,
        kind: ProcessRowKind,
        status: ProcessStatus,
        title: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            kind,
            status,
            title: title.into(),
            detail: None,
            facts: Vec::new(),
        }
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        let detail = detail.into();
        if !detail.trim().is_empty() {
            self.detail = Some(detail);
        }
        self
    }

    pub fn fact(mut self, label: impl Into<String>, value: impl Into<String>) -> Self {
        let label = label.into();
        let value = value.into();
        if !label.trim().is_empty() && !value.trim().is_empty() {
            self.facts.push(ProcessFactRenderModel { label, value });
        }
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProcessFactRenderModel {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProcessRowKind {
    Turn,
    Activity,
    Blocking,
    Todo,
    Agent,
    TaskStore,
    Compact,
}

impl ProcessRowKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Turn => "turn",
            Self::Activity => "activity",
            Self::Blocking => "blocking",
            Self::Todo => "todo",
            Self::Agent => "agent",
            Self::TaskStore => "task",
            Self::Compact => "compact",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProcessStatus {
    Idle,
    Running,
    Waiting,
    Completed,
    Failed,
    Info,
}

impl ProcessStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Running => "running",
            Self::Waiting => "waiting",
            Self::Completed => "done",
            Self::Failed => "failed",
            Self::Info => "info",
        }
    }

    pub fn is_active(self) -> bool {
        matches!(self, Self::Running)
    }

    pub fn is_waiting(self) -> bool {
        matches!(self, Self::Waiting)
    }

    pub fn is_failed(self) -> bool {
        matches!(self, Self::Failed)
    }
}

/// Session-local footer/status-line item configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FooterRenderConfig {
    #[serde(default = "default_footer_left_items")]
    pub left_items: Vec<FooterItem>,
    #[serde(default = "default_footer_right_items")]
    pub right_items: Vec<FooterItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_command: Option<ExternalStatusLineCommandConfig>,
}

impl Default for FooterRenderConfig {
    fn default() -> Self {
        Self::standard()
    }
}

impl FooterRenderConfig {
    pub fn standard() -> Self {
        Self {
            left_items: vec![
                FooterItem::Project,
                FooterItem::Model,
                FooterItem::AccessMode,
                FooterItem::Reasoning,
                FooterItem::Activity,
                FooterItem::TurnState,
                FooterItem::McpSummary,
            ],
            right_items: vec![
                FooterItem::Context,
                FooterItem::Cost,
                FooterItem::MessageCount,
            ],
            external_command: None,
        }
    }

    pub fn focused() -> Self {
        Self {
            left_items: vec![
                FooterItem::Model,
                FooterItem::AccessMode,
                FooterItem::Reasoning,
                FooterItem::Activity,
            ],
            right_items: vec![FooterItem::Context],
            external_command: None,
        }
    }

    pub fn minimal() -> Self {
        Self {
            left_items: vec![FooterItem::Model, FooterItem::Activity],
            right_items: vec![FooterItem::Context],
            external_command: None,
        }
    }

    pub fn full() -> Self {
        Self {
            left_items: vec![
                FooterItem::Project,
                FooterItem::Model,
                FooterItem::AccessMode,
                FooterItem::Reasoning,
                FooterItem::Activity,
                FooterItem::TurnState,
                FooterItem::McpSummary,
            ],
            right_items: vec![
                FooterItem::Context,
                FooterItem::Cost,
                FooterItem::MessageCount,
                FooterItem::ExternalStatus,
            ],
            external_command: None,
        }
    }

    pub fn is_enabled(&self, item: FooterItem) -> bool {
        self.left_items.contains(&item) || self.right_items.contains(&item)
    }

    pub fn set_enabled(&mut self, item: FooterItem, enabled: bool) {
        self.left_items.retain(|existing| *existing != item);
        self.right_items.retain(|existing| *existing != item);
        if enabled {
            match item.default_side() {
                FooterItemSide::Left => self.left_items.push(item),
                FooterItemSide::Right => self.right_items.push(item),
            }
        }
    }

    pub fn toggle(&mut self, item: FooterItem) {
        let enabled = !self.is_enabled(item);
        self.set_enabled(item, enabled);
    }

    pub fn apply_preset(&mut self, preset: FooterPreset) {
        *self = match preset {
            FooterPreset::Minimal => Self::minimal(),
            FooterPreset::Focused => Self::focused(),
            FooterPreset::Standard => Self::standard(),
            FooterPreset::Full => Self::full(),
        };
    }

    pub fn matching_preset(&self) -> Option<FooterPreset> {
        let shape_matches = |preset: FooterRenderConfig| {
            self.left_items == preset.left_items && self.right_items == preset.right_items
        };
        [
            FooterPreset::Minimal,
            FooterPreset::Focused,
            FooterPreset::Standard,
            FooterPreset::Full,
        ]
        .into_iter()
        .find(|preset| shape_matches(preset.config()))
    }

    pub fn preset_label(&self) -> &'static str {
        self.matching_preset()
            .map(FooterPreset::label)
            .unwrap_or("Custom")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FooterPreset {
    Minimal,
    Focused,
    Standard,
    Full,
}

impl FooterPreset {
    pub const ALL: [FooterPreset; 4] = [
        FooterPreset::Minimal,
        FooterPreset::Focused,
        FooterPreset::Standard,
        FooterPreset::Full,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Minimal => "Minimal",
            Self::Focused => "Focused",
            Self::Standard => "Standard",
            Self::Full => "Full",
        }
    }

    pub fn key_hint(self) -> &'static str {
        match self {
            Self::Minimal => "M",
            Self::Focused => "C",
            Self::Standard => "D",
            Self::Full => "F",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Minimal => "model, live activity, context",
            Self::Focused => "model, mode, reasoning, activity, context",
            Self::Standard => "default session status surface",
            Self::Full => "all built-in status facts plus external status",
        }
    }

    pub fn config(self) -> FooterRenderConfig {
        match self {
            Self::Minimal => FooterRenderConfig::minimal(),
            Self::Focused => FooterRenderConfig::focused(),
            Self::Standard => FooterRenderConfig::standard(),
            Self::Full => FooterRenderConfig::full(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalStatusLineCommandConfig {
    pub command: String,
    #[serde(default = "default_external_statusline_timeout_ms", alias = "timeout")]
    pub timeout_ms: u64,
    #[serde(default = "default_external_statusline_interval_ms")]
    pub interval_ms: u64,
}

impl ExternalStatusLineCommandConfig {
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            timeout_ms: default_external_statusline_timeout_ms(),
            interval_ms: default_external_statusline_interval_ms(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FooterItem {
    Project,
    Model,
    AccessMode,
    Reasoning,
    Activity,
    TurnState,
    McpSummary,
    Context,
    Cost,
    MessageCount,
    ExternalStatus,
}

impl FooterItem {
    pub const ALL: [FooterItem; 11] = [
        FooterItem::Project,
        FooterItem::Model,
        FooterItem::AccessMode,
        FooterItem::Reasoning,
        FooterItem::Activity,
        FooterItem::TurnState,
        FooterItem::McpSummary,
        FooterItem::Context,
        FooterItem::Cost,
        FooterItem::MessageCount,
        FooterItem::ExternalStatus,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Project => "Project",
            Self::Model => "Model",
            Self::AccessMode => "Access mode",
            Self::Reasoning => "Reasoning",
            Self::Activity => "Activity",
            Self::TurnState => "Turn marker",
            Self::McpSummary => "MCP summary",
            Self::Context => "Context",
            Self::Cost => "Cost",
            Self::MessageCount => "Messages",
            Self::ExternalStatus => "External status",
        }
    }

    fn default_side(self) -> FooterItemSide {
        match self {
            Self::Context | Self::Cost | Self::MessageCount | Self::ExternalStatus => {
                FooterItemSide::Right
            }
            Self::Project
            | Self::Model
            | Self::AccessMode
            | Self::Reasoning
            | Self::Activity
            | Self::TurnState
            | Self::McpSummary => FooterItemSide::Left,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum FooterItemSide {
    Left,
    Right,
}

fn default_footer_left_items() -> Vec<FooterItem> {
    FooterRenderConfig::standard().left_items
}

fn default_footer_right_items() -> Vec<FooterItem> {
    FooterRenderConfig::standard().right_items
}

fn default_external_statusline_timeout_ms() -> u64 {
    1_000
}

fn default_external_statusline_interval_ms() -> u64 {
    1_000
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ContextUsageRenderModel {
    pub used_tokens: u64,
    pub window_tokens: u64,
}

impl ContextUsageRenderModel {
    pub fn new(used_tokens: u64, window_tokens: u64) -> Option<Self> {
        if window_tokens == 0 {
            return None;
        }
        Some(Self {
            used_tokens,
            window_tokens,
        })
    }

    pub fn used_percent(self) -> u32 {
        let percent = ((self.used_tokens as f64 / self.window_tokens as f64) * 100.0).round();
        percent.clamp(0.0, 100.0) as u32
    }

    pub fn label(self) -> String {
        format!(
            "ctx {}/{}",
            token_count_label(self.used_tokens),
            token_count_label(self.window_tokens)
        )
    }
}

fn token_count_label(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}m", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.0}k", tokens as f64 / 1_000.0)
    } else {
        tokens.to_string()
    }
}

/// Visible blocking state, ordered by product priority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockingRenderModel {
    pub kind: BlockingKind,
    pub title: String,
    pub detail: String,
}

impl BlockingRenderModel {
    pub fn approval(title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            kind: BlockingKind::Approval,
            title: title.into(),
            detail: detail.into(),
        }
    }

    pub fn error(title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            kind: BlockingKind::Error,
            title: title.into(),
            detail: detail.into(),
        }
    }

    pub fn cost_limit(title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            kind: BlockingKind::CostLimit,
            title: title.into(),
            detail: detail.into(),
        }
    }

    pub fn idle_return(title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            kind: BlockingKind::IdleReturn,
            title: title.into(),
            detail: detail.into(),
        }
    }

    pub fn info(title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            kind: BlockingKind::Info,
            title: title.into(),
            detail: detail.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BlockingKind {
    Approval,
    Error,
    CostLimit,
    IdleReturn,
    Info,
}

/// Inline approval block, normally anchored below a tool card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalRenderModel {
    pub id: String,
    pub tool_name: String,
    pub title: String,
    pub detail_label: String,
    pub detail: String,
    pub risk: ApprovalRiskLevel,
    pub body: String,
    pub actions: Vec<ApprovalAction>,
    pub selected_action: ApprovalAction,
    pub anchor_block_id: Option<String>,
    pub expanded: bool,
}

impl ApprovalRenderModel {
    pub fn blocking_model(&self) -> BlockingRenderModel {
        let detail = if self.detail.is_empty() {
            self.tool_name.clone()
        } else {
            format!("{}: {}", self.detail_label, self.detail)
        };
        BlockingRenderModel::approval(
            self.title.clone(),
            format!("Risk: {} · {detail}", self.risk.label()),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ApprovalRiskLevel {
    Low,
    Medium,
    High,
}

impl ApprovalRiskLevel {
    pub fn label(self) -> &'static str {
        match self {
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
        }
    }

    pub fn from_score(score: u8) -> Self {
        match score {
            0..=3 => Self::Low,
            4..=6 => Self::Medium,
            _ => Self::High,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ApprovalAction {
    Allow,
    AlwaysAllow,
    EditCommand,
    Deny,
}

impl ApprovalAction {
    pub fn label(self) -> &'static str {
        match self {
            Self::Allow => "Allow",
            Self::AlwaysAllow => "Always",
            Self::EditCommand => "Edit command",
            Self::Deny => "Deny",
        }
    }
}

fn nodes_from_record(record: &TranscriptRecord, tool: Option<&ToolCardModel>) -> Vec<RenderNode> {
    let mut nodes = Vec::new();

    if let Some(thinking) = &record.thinking {
        let thinking = sanitize_semantic_text(thinking);
        if !thinking.trim().is_empty() {
            nodes.push(RenderNode::Thinking(thinking));
        }
    }

    if let Some(tool) = tool {
        nodes.push(RenderNode::ToolCard(tool.clone()));
        return nodes;
    }

    let content = sanitize_semantic_text(&record.content);
    if content.trim().is_empty() {
        return nodes;
    }

    match record.kind {
        TranscriptRecordKind::Assistant => nodes.push(RenderNode::Markdown(content)),
        _ => nodes.push(RenderNode::PlainText(content)),
    }

    nodes
}

fn tool_card_from_message(message: &MessageData) -> Option<ToolCardModel> {
    let name = message.tool_name.clone()?;
    let phase = match message.message_type {
        MessageType::ToolUse if message.is_streaming => ToolPhase::Running,
        MessageType::ToolUse => ToolPhase::Requested,
        MessageType::ToolResult if message.is_error => ToolPhase::Failed,
        MessageType::ToolResult => ToolPhase::Succeeded,
        _ => return None,
    };

    let normalized = sanitize_normalized_tool_content(normalize_tool_content(&name, message));
    let command_run = command_run_from_message(&name, message, phase);
    let plan = plan_model_from_message(&name, message);

    Some(ToolCardModel {
        name,
        phase,
        summary: normalized.summary,
        sections: normalized.sections,
        command_run,
        plan,
    })
}

fn tool_card_from_pair(tool_use: &MessageData, result: &MessageData) -> Option<ToolCardModel> {
    let name = tool_use
        .tool_name
        .clone()
        .or_else(|| result.tool_name.clone())?;
    if !same_tool_name(tool_use.tool_name.as_deref(), result.tool_name.as_deref()) {
        return None;
    }

    let input = sanitize_normalized_tool_content(normalize_tool_content(&name, tool_use));
    let output = sanitize_normalized_tool_content(normalize_tool_content(&name, result));
    let phase = if result.is_error {
        ToolPhase::Failed
    } else {
        ToolPhase::Succeeded
    };
    let has_input_section = !input.sections.is_empty();
    let mut sections = input.sections;
    sections.extend(output.sections.into_iter().filter(|section| {
        !(has_input_section
            && matches!(
                section.kind,
                ToolSectionKind::Input | ToolSectionKind::Metadata
            )
            && matches!(section.title.as_str(), "command" | "cwd" | "old"))
    }));
    let command_run = command_run_from_pair(&name, tool_use, result, phase);
    let plan =
        plan_model_from_message(&name, result).or_else(|| plan_model_from_message(&name, tool_use));

    Some(ToolCardModel {
        name,
        phase,
        summary: output.summary.or(input.summary),
        sections,
        command_run,
        plan,
    })
}

fn next_tool_result_record(
    records: &[TranscriptRecord],
    tool_use_index: usize,
) -> Option<(usize, &TranscriptRecord)> {
    let tool_use = records.get(tool_use_index)?;
    let next_index = tool_use_index.checked_add(1)?;
    if let Some(next) = records.get(next_index) {
        if !is_protocol_only_record(next) && is_matching_tool_result_record(tool_use, next) {
            return Some((next_index, next));
        }
    }

    if tool_use.id.is_empty() {
        return None;
    }

    records
        .iter()
        .enumerate()
        .skip(tool_use_index.saturating_add(1))
        .find(|(_, record)| {
            matches!(record.kind, TranscriptRecordKind::ToolResult)
                && record.parent_id.as_deref() == Some(tool_use.id.as_str())
                && same_tool_name(tool_use.tool_name.as_deref(), record.tool_name.as_deref())
        })
}

fn is_matching_tool_result_record(tool_use: &TranscriptRecord, result: &TranscriptRecord) -> bool {
    if !matches!(result.kind, TranscriptRecordKind::ToolResult) {
        return false;
    }
    if let Some(parent_id) = result.parent_id.as_deref() {
        return parent_id == tool_use.id
            && same_tool_name(tool_use.tool_name.as_deref(), result.tool_name.as_deref());
    }
    same_tool_name(tool_use.tool_name.as_deref(), result.tool_name.as_deref())
}

fn same_tool_name(left: Option<&str>, right: Option<&str>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => left == right,
        (Some(_), None) | (None, Some(_)) => true,
        (None, None) => false,
    }
}

fn plan_model_from_message(tool_name: &str, message: &MessageData) -> Option<PlanRenderModel> {
    if normalize_tool_family(tool_name) != "todowrite" {
        return None;
    }

    let content = semantic_tool_content(message);
    let content = content.as_ref();
    if let Some(value) = parse_json_value(content) {
        if let Some(object) = value.as_object() {
            let todos = object.get("new_todos").or_else(|| object.get("todos"))?;
            return plan_model_from_todos(todos);
        }
        return plan_model_from_todos(&value);
    }

    plan_model_from_plain_text(content)
}

fn plan_model_from_todos(value: &Value) -> Option<PlanRenderModel> {
    let steps = value
        .as_array()?
        .iter()
        .filter_map(plan_step_from_value)
        .collect::<Vec<_>>();
    (!steps.is_empty()).then(|| PlanRenderModel {
        title: "Plan".to_string(),
        steps,
    })
}

fn plan_step_from_value(value: &Value) -> Option<PlanStepRenderModel> {
    let object = value.as_object()?;
    let label = object
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("pending");
    let content = object
        .get("content")
        .or_else(|| object.get("text"))
        .or_else(|| object.get("title"))
        .and_then(value_as_display_text)?;
    plan_step_from_parts(label, &content)
}

fn plan_model_from_plain_text(content: &str) -> Option<PlanRenderModel> {
    let steps = content
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            if let Some((label, content)) = line.split_once(':') {
                return plan_step_from_parts(label, content);
            }
            let (label, content) = line.split_once(char::is_whitespace)?;
            plan_step_from_parts(label, content)
        })
        .collect::<Vec<_>>();
    (!steps.is_empty()).then(|| PlanRenderModel {
        title: "Plan".to_string(),
        steps,
    })
}

fn plan_step_from_parts(label: &str, content: &str) -> Option<PlanStepRenderModel> {
    let content = sanitize_semantic_text(content).trim().to_string();
    if content.is_empty() {
        return None;
    }
    let status = PlanStepStatus::from_label(label);
    let label = sanitize_semantic_text(label).trim().to_string();
    let label = if label.is_empty() {
        status.display_label().to_string()
    } else {
        label
    };
    Some(PlanStepRenderModel {
        status,
        label,
        content: truncate_chars_with_suffix(&content, TOOL_SUMMARY_CHARS, "..."),
    })
}

fn command_run_from_pair(
    tool_name: &str,
    tool_use: &MessageData,
    result: &MessageData,
    phase: ToolPhase,
) -> Option<CommandRunRenderModel> {
    let mut run = command_run_from_message(tool_name, result, phase)?;
    if let Some(input) = command_run_from_message(tool_name, tool_use, ToolPhase::Requested) {
        if run.command.is_none() {
            run.command = input.command;
        }
        if run.cwd.is_none() {
            run.cwd = input.cwd;
        }
    }
    Some(run)
}

fn command_run_from_message(
    tool_name: &str,
    message: &MessageData,
    phase: ToolPhase,
) -> Option<CommandRunRenderModel> {
    if ToolFamily::from_tool_name(tool_name) != ToolFamily::Command {
        return None;
    }

    let preview = parse_json_value(&message.content)
        .or_else(|| serde_json::from_str::<Value>(semantic_tool_content(message).as_ref()).ok());
    let full = message.full_content.as_deref().and_then(parse_json_value);
    let preview_object = preview.as_ref().and_then(Value::as_object);
    let full_object = full.as_ref().and_then(Value::as_object);
    if preview_object.is_none() && full_object.is_none() {
        return None;
    }

    let command = command_field(preview_object, full_object, "command");
    let cwd = command_field(preview_object, full_object, "cwd");
    let exit_code = command_i64_field(preview_object, full_object, "exit_code")
        .or_else(|| command_i64_field(preview_object, full_object, "exitCode"));
    let duration_ms = command_u64_field(preview_object, full_object, "duration_ms")
        .or_else(|| command_u64_field(preview_object, full_object, "durationMs"));
    let timed_out = command_bool_field(preview_object, full_object, "timed_out")
        .or_else(|| command_bool_field(preview_object, full_object, "timedOut"))
        .or_else(|| command_bool_field(preview_object, full_object, "timeout"))
        .unwrap_or(false);
    let interrupted = command_bool_field(preview_object, full_object, "interrupted")
        .or_else(|| command_bool_field(preview_object, full_object, "was_interrupted"))
        .or_else(|| command_bool_field(preview_object, full_object, "wasInterrupted"))
        .unwrap_or(false);
    let signal = command_field(preview_object, full_object, "signal")
        .or_else(|| command_field(preview_object, full_object, "term_signal"))
        .or_else(|| command_field(preview_object, full_object, "termSignal"));
    let error_summary = command_field(preview_object, full_object, "error")
        .or_else(|| command_field(preview_object, full_object, "message"));
    let stdout = command_stream_model("stdout", preview_object, full_object);
    let stderr = command_stream_model("stderr", preview_object, full_object);
    let full_log_available = stdout.full_log_available || stderr.full_log_available;

    Some(CommandRunRenderModel {
        command,
        cwd,
        status: command_status_from_phase(
            phase,
            exit_code,
            message.is_error,
            timed_out,
            interrupted,
            error_summary.is_some(),
        ),
        exit_code,
        duration_ms,
        timed_out,
        interrupted,
        signal,
        error_summary,
        stdout,
        stderr,
        full_log_available,
    })
}

fn parse_json_value(text: &str) -> Option<Value> {
    serde_json::from_str::<Value>(text).ok()
}

fn command_field(
    preview: Option<&serde_json::Map<String, Value>>,
    full: Option<&serde_json::Map<String, Value>>,
    key: &str,
) -> Option<String> {
    preview
        .and_then(|object| object.get(key))
        .or_else(|| full.and_then(|object| object.get(key)))
        .and_then(value_as_display_text)
        .map(|text| sanitize_semantic_text(&text))
        .filter(|text| !text.trim().is_empty())
}

fn command_i64_field(
    preview: Option<&serde_json::Map<String, Value>>,
    full: Option<&serde_json::Map<String, Value>>,
    key: &str,
) -> Option<i64> {
    preview
        .and_then(|object| object.get(key))
        .or_else(|| full.and_then(|object| object.get(key)))
        .and_then(value_as_i64)
}

fn command_u64_field(
    preview: Option<&serde_json::Map<String, Value>>,
    full: Option<&serde_json::Map<String, Value>>,
    key: &str,
) -> Option<u64> {
    preview
        .and_then(|object| object.get(key))
        .or_else(|| full.and_then(|object| object.get(key)))
        .and_then(value_as_u64)
}

fn command_bool_field(
    preview: Option<&serde_json::Map<String, Value>>,
    full: Option<&serde_json::Map<String, Value>>,
    key: &str,
) -> Option<bool> {
    preview
        .and_then(|object| object.get(key))
        .or_else(|| full.and_then(|object| object.get(key)))
        .and_then(value_as_bool)
}

fn command_stream_model(
    name: &str,
    preview: Option<&serde_json::Map<String, Value>>,
    full: Option<&serde_json::Map<String, Value>>,
) -> CommandStreamRenderModel {
    let preview_text = preview
        .and_then(|object| object.get(name))
        .and_then(value_as_display_text)
        .map(|text| sanitize_semantic_text(&text))
        .unwrap_or_default();
    let full_text = full
        .and_then(|object| object.get(name))
        .and_then(value_as_display_text)
        .map(|text| sanitize_semantic_text(&text));
    let hidden_key = format!("{name}_hidden_lines");
    let preview_hidden = preview
        .and_then(|object| object.get(&hidden_key))
        .and_then(value_as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or_default();
    let preview_lines = line_count_for_change(&preview_text);
    let full_lines = full_text.as_deref().map(line_count_for_change);
    let hidden_lines = full_lines
        .map(|total| total.saturating_sub(preview_lines))
        .unwrap_or(preview_hidden)
        .max(preview_hidden);

    CommandStreamRenderModel {
        name: name.to_string(),
        preview_line_count: preview_lines,
        hidden_line_count: hidden_lines,
        total_line_count: full_lines.filter(|total| *total > 0).or_else(|| {
            (preview_lines > 0 || hidden_lines > 0)
                .then_some(preview_lines.saturating_add(hidden_lines))
        }),
        has_content: !preview_text.trim().is_empty()
            || full_text
                .as_deref()
                .is_some_and(|text| !text.trim().is_empty()),
        full_log_available: full_text
            .as_deref()
            .is_some_and(|text| !text.trim().is_empty()),
        full_text: full_text.filter(|text| !text.trim().is_empty()),
    }
}

fn command_status_from_phase(
    phase: ToolPhase,
    exit_code: Option<i64>,
    is_error: bool,
    timed_out: bool,
    interrupted: bool,
    has_error_summary: bool,
) -> CommandRunStatus {
    if is_error
        || timed_out
        || interrupted
        || has_error_summary
        || exit_code.is_some_and(|code| code != 0)
    {
        return CommandRunStatus::Failed;
    }
    match phase {
        ToolPhase::Requested => CommandRunStatus::Requested,
        ToolPhase::Running => CommandRunStatus::Running,
        ToolPhase::Succeeded => CommandRunStatus::Succeeded,
        ToolPhase::Failed => CommandRunStatus::Failed,
        ToolPhase::WaitingApproval => CommandRunStatus::WaitingApproval,
        ToolPhase::Rejected => CommandRunStatus::Rejected,
    }
}

fn pluralize(count: usize, unit: &str) -> String {
    if count == 1 {
        format!("1 {unit}")
    } else if unit.ends_with("ch") || unit.ends_with('s') {
        format!("{count} {unit}es")
    } else {
        format!("{count} {unit}s")
    }
}

struct NormalizedToolContent {
    summary: Option<String>,
    sections: Vec<ToolSection>,
}

fn sanitize_normalized_tool_content(mut content: NormalizedToolContent) -> NormalizedToolContent {
    content.summary = content
        .summary
        .map(|summary| sanitize_semantic_text(&summary));
    for section in &mut content.sections {
        section.title = sanitize_semantic_text(&section.title);
        section.body = sanitize_semantic_text(&section.body);
        if let Some(code) = section.code.as_mut() {
            if let Some(file_path) = code.file_path.as_mut() {
                *file_path = sanitize_semantic_text(file_path);
            }
        }
    }
    content
}

fn sanitized_skill_result_value(value: Option<&Value>) -> Option<Value> {
    let text = value.and_then(value_as_display_text)?;
    let stripped = strip_display_tags_allow_empty(&text);
    Some(Value::String(if stripped.is_empty() {
        text
    } else {
        stripped
    }))
}

fn sanitize_semantic_text(text: &str) -> String {
    let expanded_tabs = text.replace('\t', "    ");
    let stripped = strip_ansi_escapes::strip_str(&expanded_tabs);
    stripped
        .chars()
        .map(|ch| {
            if ch == '\n' {
                ch
            } else if ch.is_control() {
                ' '
            } else {
                ch
            }
        })
        .collect()
}

fn normalize_tool_content(tool_name: &str, message: &MessageData) -> NormalizedToolContent {
    let content = semantic_tool_content(message);
    let content = content.as_ref();
    let normalized_name = tool_name.to_ascii_lowercase();

    if content.trim().is_empty() {
        return NormalizedToolContent {
            summary: Some(empty_tool_summary(message).to_string()),
            sections: vec![ToolSection::new(
                default_tool_section_title(message),
                empty_tool_summary(message),
                default_tool_section_kind(message),
            )],
        };
    }

    let parsed = serde_json::from_str::<Value>(content).ok();
    if matches!(parsed, Some(Value::Null)) {
        return NormalizedToolContent {
            summary: Some(empty_tool_summary(message).to_string()),
            sections: vec![ToolSection::new(
                default_tool_section_title(message),
                empty_tool_summary(message),
                default_tool_section_kind(message),
            )],
        };
    }
    if parsed.is_none() && looks_like_json_payload(content) {
        return malformed_tool_payload(tool_name, message);
    }
    let from_json = parsed.as_ref().and_then(|value| {
        if matches!(message.message_type, MessageType::ToolUse) {
            normalize_tool_input(&normalized_name, value)
        } else {
            normalize_tool_result(&normalized_name, value, message.is_error)
        }
    });

    if let Some(model) = from_json {
        return model;
    }

    if let Some(value) = parsed.as_ref() {
        return normalize_generic_json_tool(message, value);
    }

    if matches!(message.message_type, MessageType::ToolResult) {
        if let Some(model) =
            normalize_plain_tool_result(&normalized_name, content, message.is_error)
        {
            return model;
        }
    }

    NormalizedToolContent {
        summary: Some(truncate_chars_with_suffix(
            content.trim(),
            TOOL_SUMMARY_CHARS,
            "...",
        )),
        sections: vec![ToolSection::new(
            default_tool_section_title(message),
            content,
            default_tool_section_kind(message),
        )],
    }
}

fn is_known_semantic_tool(tool_name: &str) -> bool {
    let normalized = normalize_tool_family(tool_name);
    matches!(
        normalized.as_str(),
        "bash"
            | "powershell"
            | "read"
            | "grep"
            | "glob"
            | "write"
            | "edit"
            | "multiedit"
            | "notebookedit"
            | "todowrite"
            | "taskcreate"
            | "tasklist"
            | "taskget"
            | "taskupdate"
            | "taskoutput"
            | "taskstop"
            | "task"
            | "agent"
            | "webfetch"
            | "websearch"
            | "skill"
            | "readmcpresource"
            | "listmcpresources"
            | "listmcpresourcestool"
            | "exitplanmode"
            | "toolsearch"
            | "askuserquestion"
    ) || is_mcp_tool_name(tool_name)
}

fn looks_like_json_payload(content: &str) -> bool {
    let trimmed = content.trim_start();
    trimmed.starts_with('{') || trimmed.starts_with('[')
}

fn normalize_tool_family(tool_name: &str) -> String {
    tool_name.to_ascii_lowercase()
}

fn is_mcp_tool_name(tool_name: &str) -> bool {
    let lower = tool_name.to_ascii_lowercase();
    lower == "mcp" || lower.starts_with("mcp__") || lower.contains("__")
}

fn malformed_tool_payload(tool_name: &str, message: &MessageData) -> NormalizedToolContent {
    let direction = if matches!(message.message_type, MessageType::ToolUse) {
        "input"
    } else {
        "output"
    };
    let body = format!(
        "{} {direction} payload could not be parsed. The raw payload is hidden from the normal transcript.",
        tool_name
    );
    NormalizedToolContent {
        summary: Some(format!("malformed {direction} payload")),
        sections: vec![ToolSection::new(
            format!("malformed {direction}"),
            body,
            if matches!(message.message_type, MessageType::ToolResult) || message.is_error {
                ToolSectionKind::Error
            } else {
                ToolSectionKind::Metadata
            },
        )],
    }
}

fn normalize_generic_json_tool(message: &MessageData, value: &Value) -> NormalizedToolContent {
    let body = semantic_json_lines(value).join("\n");
    let body = if body.trim().is_empty() {
        empty_tool_summary(message).to_string()
    } else {
        body
    };
    NormalizedToolContent {
        summary: generic_json_summary(value)
            .or_else(|| Some(empty_tool_summary(message).to_string())),
        sections: vec![ToolSection::new(
            default_tool_section_title(message),
            body,
            default_tool_section_kind(message),
        )],
    }
}

fn generic_json_summary(value: &Value) -> Option<String> {
    let object = value.as_object()?;
    for key in [
        "status",
        "message",
        "error",
        "url",
        "uri",
        "path",
        "file_path",
        "query",
        "server",
        "id",
        "name",
    ] {
        if let Some(text) = object
            .get(key)
            .and_then(|value| semantic_object_field_text(key, value))
        {
            if !text.trim().is_empty() {
                return Some(truncate_chars_with_suffix(
                    &format!("{key} {text}"),
                    TOOL_SUMMARY_CHARS,
                    "...",
                ));
            }
        }
    }

    let first = object.iter().find_map(|(key, value)| {
        semantic_object_field_text(key, value).and_then(|text| {
            if text.trim().is_empty() {
                None
            } else {
                Some(format!("{key} {text}"))
            }
        })
    })?;
    Some(truncate_chars_with_suffix(
        &first,
        TOOL_SUMMARY_CHARS,
        "...",
    ))
}

fn semantic_json_lines(value: &Value) -> Vec<String> {
    match value {
        Value::Null => vec!["(empty)".to_string()],
        Value::String(text) => vec![text.clone()],
        Value::Number(number) => vec![number.to_string()],
        Value::Bool(flag) => vec![flag.to_string()],
        Value::Array(items) => semantic_array_lines(items),
        Value::Object(object) => object
            .iter()
            .filter_map(|(key, value)| {
                semantic_object_field_text(key, value).map(|text| (key, text))
            })
            .filter(|(_, text)| !text.trim().is_empty())
            .map(|(key, text)| format!("{key}: {text}"))
            .collect(),
    }
}

fn semantic_array_lines(items: &[Value]) -> Vec<String> {
    if items.is_empty() {
        return vec!["(empty list)".to_string()];
    }

    let mut lines = Vec::new();
    for (index, item) in items.iter().take(24).enumerate() {
        let text = value_as_display_text(item).unwrap_or_else(|| "(empty)".to_string());
        lines.push(format!("{}: {}", index + 1, text));
    }
    if items.len() > lines.len() {
        lines.push(format!("… {} more items", items.len() - lines.len()));
    }
    lines
}

fn semantic_array_inline(items: &[Value]) -> String {
    if items.is_empty() {
        return "(empty list)".to_string();
    }

    let mut parts = items
        .iter()
        .take(6)
        .filter_map(value_as_display_text)
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>();
    if items.len() > parts.len() {
        parts.push(format!("… {} more", items.len() - parts.len()));
    }
    truncate_chars_with_suffix(&parts.join(", "), TOOL_SUMMARY_CHARS, "...")
}

fn semantic_object_inline(object: &serde_json::Map<String, Value>) -> String {
    if object.is_empty() {
        return "(empty object)".to_string();
    }

    let mut parts = object
        .iter()
        .take(8)
        .filter_map(|(key, value)| semantic_object_field_text(key, value).map(|text| (key, text)))
        .filter(|(_, text)| !text.trim().is_empty())
        .map(|(key, text)| format!("{key}={text}"))
        .collect::<Vec<_>>();
    if object.len() > parts.len() {
        parts.push(format!("… {} more fields", object.len() - parts.len()));
    }
    truncate_chars_with_suffix(&parts.join(", "), TOOL_SUMMARY_CHARS, "...")
}

fn semantic_object_field_text(key: &str, value: &Value) -> Option<String> {
    if is_sensitive_json_key(key) {
        Some("redacted".to_string())
    } else {
        value_as_display_text(value)
    }
}

fn is_sensitive_json_key(key: &str) -> bool {
    let normalized = key
        .chars()
        .filter(|ch| *ch != '_' && *ch != '-' && !ch.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();
    let token_like_secret = normalized == "token"
        || (normalized.ends_with("token")
            && !matches!(
                normalized.as_str(),
                "tokencount"
                    | "inputtoken"
                    | "inputtokens"
                    | "outputtoken"
                    | "outputtokens"
                    | "totaltoken"
                    | "totaltokens"
                    | "maxtoken"
                    | "maxtokens"
                    | "remainingtoken"
                    | "remainingtokens"
            ));
    matches!(
        normalized.as_str(),
        "apikey"
            | "authorization"
            | "authorizationheader"
            | "password"
            | "secret"
            | "secrettoken"
            | "accesstoken"
            | "refreshtoken"
            | "authtoken"
            | "bearertoken"
            | "credential"
            | "credentials"
            | "privatekey"
    ) || normalized.ends_with("apikey")
        || normalized.starts_with("authorization")
        || normalized.ends_with("password")
        || normalized.ends_with("secret")
        || normalized.ends_with("secrettoken")
        || normalized.ends_with("accesstoken")
        || normalized.ends_with("refreshtoken")
        || normalized.ends_with("authtoken")
        || normalized.ends_with("bearertoken")
        || normalized.ends_with("credential")
        || normalized.ends_with("credentials")
        || normalized.ends_with("privatekey")
        || token_like_secret
}

fn push_allowed_prompt_lines(lines: &mut Vec<String>, value: Option<&Value>) {
    let Some(prompts) = value.and_then(Value::as_array) else {
        return;
    };
    for item in prompts.iter().take(12) {
        if let Some(object) = item.as_object() {
            let tool = object.get("tool").and_then(Value::as_str).unwrap_or("tool");
            let prompt = object
                .get("prompt")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if !prompt.trim().is_empty() {
                lines.push(format!("allowed {tool}: {prompt}"));
            }
        }
    }
    if prompts.len() > 12 {
        lines.push(format!("… {} more allowed prompts", prompts.len() - 12));
    }
}

fn push_question_lines(lines: &mut Vec<String>, value: Option<&Value>) {
    let Some(questions) = value.and_then(Value::as_array) else {
        return;
    };
    for question in questions.iter().take(8) {
        let Some(object) = question.as_object() else {
            continue;
        };
        let header = object
            .get("header")
            .and_then(Value::as_str)
            .unwrap_or("question");
        let text = object
            .get("question")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !text.trim().is_empty() {
            lines.push(format!("{header}: {text}"));
        }
        if let Some(options) = object.get("options").and_then(Value::as_array) {
            let labels = options
                .iter()
                .filter_map(|option| option.get("label").and_then(Value::as_str))
                .take(4)
                .collect::<Vec<_>>();
            if !labels.is_empty() {
                lines.push(format!("options: {}", labels.join(", ")));
            }
        }
    }
}

fn push_question_lines_section(sections: &mut Vec<ToolSection>, value: Option<&Value>) {
    let mut lines = Vec::new();
    push_question_lines(&mut lines, value);
    if !lines.is_empty() {
        sections.push(ToolSection::new(
            "questions",
            lines.join("\n"),
            ToolSectionKind::Input,
        ));
    }
}

fn semantic_tool_content(message: &MessageData) -> Cow<'_, str> {
    if message.expanded {
        if let Some(full) = message.full_content.as_deref() {
            return Cow::Borrowed(full);
        }
    }

    if matches!(message.message_type, MessageType::ToolResult)
        && serde_json::from_str::<Value>(&message.content).is_err()
    {
        if let Some(full) = message.full_content.as_deref() {
            if serde_json::from_str::<Value>(full).is_ok() {
                return Cow::Borrowed(full);
            }
        }
    }

    Cow::Borrowed(message.content.as_str())
}

fn normalize_tool_input(tool_name: &str, value: &Value) -> Option<NormalizedToolContent> {
    let tool_name = normalize_tool_family(tool_name);
    let object = value.as_object()?;
    let mut lines = Vec::new();

    match tool_name.as_str() {
        "bash" | "powershell" => {
            push_field_line(&mut lines, "command", object.get("command"));
            push_field_line(&mut lines, "cwd", object.get("cwd"));
            push_field_line(&mut lines, "description", object.get("description"));
        }
        "read" => {
            push_field_line(
                &mut lines,
                "file",
                object.get("file_path").or_else(|| object.get("path")),
            );
            push_read_range_line(&mut lines, object);
        }
        "grep" => {
            push_field_line(&mut lines, "pattern", object.get("pattern"));
            push_field_line(&mut lines, "path", object.get("path"));
            push_field_line(&mut lines, "glob", object.get("glob"));
            push_field_line(&mut lines, "mode", object.get("output_mode"));
            push_field_line(&mut lines, "type", object.get("type"));
        }
        "glob" => {
            push_field_line(&mut lines, "pattern", object.get("pattern"));
            push_field_line(&mut lines, "path", object.get("path"));
        }
        "write" | "edit" | "multiedit" | "notebookedit" => {
            push_field_line(
                &mut lines,
                "file",
                object
                    .get("file_path")
                    .or_else(|| object.get("path"))
                    .or_else(|| object.get("notebook_path")),
            );
            push_field_line(&mut lines, "cell", object.get("cell_id"));
            push_field_line(&mut lines, "cell type", object.get("cell_type"));
            push_field_line(&mut lines, "mode", object.get("edit_mode"));
            push_field_line(&mut lines, "replace", object.get("old_string"));
            push_field_line(&mut lines, "with", object.get("new_string"));
            push_field_line(
                &mut lines,
                "content",
                object.get("content").or_else(|| object.get("new_source")),
            );
            push_field_line(&mut lines, "edits", object.get("edits"));
        }
        "todowrite" => {
            if let Some(todos) = object.get("todos").or_else(|| object.get("new_todos")) {
                lines.extend(todo_lines(todos));
            }
        }
        "taskcreate" => {
            push_field_line(&mut lines, "subject", object.get("subject"));
            push_field_line(&mut lines, "description", object.get("description"));
            push_field_line(
                &mut lines,
                "active form",
                object
                    .get("activeForm")
                    .or_else(|| object.get("active_form")),
            );
            push_field_line(&mut lines, "metadata", object.get("metadata"));
        }
        "tasklist" => {
            lines.push("scope: all tasks".to_string());
        }
        "taskget" => {
            push_field_line(
                &mut lines,
                "task id",
                object.get("taskId").or_else(|| object.get("task_id")),
            );
        }
        "taskupdate" => {
            push_field_line(
                &mut lines,
                "task id",
                object.get("taskId").or_else(|| object.get("task_id")),
            );
            push_field_line(&mut lines, "status", object.get("status"));
            push_field_line(&mut lines, "subject", object.get("subject"));
            push_field_line(&mut lines, "description", object.get("description"));
            push_field_line(&mut lines, "owner", object.get("owner"));
            push_field_line(
                &mut lines,
                "active form",
                object
                    .get("activeForm")
                    .or_else(|| object.get("active_form")),
            );
            push_field_line(
                &mut lines,
                "add blocks",
                object.get("addBlocks").or_else(|| object.get("add_blocks")),
            );
            push_field_line(
                &mut lines,
                "add blocked by",
                object
                    .get("addBlockedBy")
                    .or_else(|| object.get("add_blocked_by")),
            );
            push_field_line(&mut lines, "metadata", object.get("metadata"));
        }
        "taskoutput" => {
            push_field_line(
                &mut lines,
                "task id",
                object.get("task_id").or_else(|| object.get("taskId")),
            );
            push_field_line(&mut lines, "block", object.get("block"));
            push_field_line(&mut lines, "timeout", object.get("timeout"));
        }
        "taskstop" => {
            push_field_line(
                &mut lines,
                "task id",
                object.get("task_id").or_else(|| object.get("taskId")),
            );
            push_field_line(&mut lines, "shell id", object.get("shell_id"));
        }
        "task" | "agent" => {
            push_field_line(
                &mut lines,
                "agent type",
                object
                    .get("agent_type")
                    .or_else(|| object.get("agentType"))
                    .or_else(|| object.get("type")),
            );
            push_field_line(&mut lines, "description", object.get("description"));
            push_field_line(&mut lines, "prompt", object.get("prompt"));
            push_field_line(
                &mut lines,
                "agent",
                object.get("agent_id").or_else(|| object.get("agent")),
            );
        }
        "webfetch" => {
            push_field_line(&mut lines, "url", object.get("url"));
            push_field_line(&mut lines, "prompt", object.get("prompt"));
        }
        "websearch" => {
            push_field_line(&mut lines, "query", object.get("query"));
            push_field_line(&mut lines, "allowed domains", object.get("allowed_domains"));
            push_field_line(&mut lines, "blocked domains", object.get("blocked_domains"));
        }
        "skill" => {
            push_field_line(&mut lines, "skill", object.get("skill"));
            push_field_line(&mut lines, "args", object.get("args"));
        }
        "readmcpresource" => {
            push_field_line(&mut lines, "server", object.get("server"));
            push_field_line(&mut lines, "uri", object.get("uri"));
        }
        "listmcpresources" | "listmcpresourcestool" => {
            push_field_line(&mut lines, "server", object.get("server"));
        }
        "exitplanmode" => {
            push_allowed_prompt_lines(
                &mut lines,
                object
                    .get("allowedPrompts")
                    .or_else(|| object.get("allowed_prompts")),
            );
        }
        "toolsearch" => {
            push_field_line(&mut lines, "query", object.get("query"));
            push_field_line(&mut lines, "max results", object.get("max_results"));
        }
        "askuserquestion" => {
            push_question_lines(&mut lines, object.get("questions"));
        }
        _ if is_mcp_tool_name(&tool_name) => {
            lines.extend(semantic_json_lines(value));
        }
        _ => return None,
    }

    if lines.is_empty() {
        return None;
    }

    Some(NormalizedToolContent {
        summary: Some(truncate_chars_with_suffix(
            &lines.join(" | "),
            TOOL_SUMMARY_CHARS,
            "...",
        )),
        sections: vec![ToolSection::new(
            "input",
            lines.join("\n"),
            ToolSectionKind::Input,
        )],
    })
}

fn normalize_tool_result(
    tool_name: &str,
    value: &Value,
    is_error: bool,
) -> Option<NormalizedToolContent> {
    let tool_name = normalize_tool_family(tool_name);
    if matches!(
        tool_name.as_str(),
        "listmcpresources" | "listmcpresourcestool"
    ) {
        if let Some(model) = normalize_mcp_resource_list_result(value) {
            return Some(model);
        }
    }

    let object = value.as_object()?;
    let mut sections = Vec::new();
    let mut summary_parts = Vec::new();

    match tool_name.as_str() {
        "bash" | "powershell" => {
            push_summary_field(&mut summary_parts, "exit", object.get("exit_code"));
            push_command_flag_summary(
                &mut summary_parts,
                "timeout",
                command_result_bool(object, &["timed_out", "timedOut", "timeout"]),
            );
            push_command_flag_summary(
                &mut summary_parts,
                "interrupted",
                command_result_bool(
                    object,
                    &["interrupted", "was_interrupted", "wasInterrupted"],
                ),
            );
            push_summary_field(
                &mut summary_parts,
                "signal",
                command_result_value(object, &["signal", "term_signal", "termSignal"]),
            );
            push_duration_summary_field(
                &mut summary_parts,
                "duration",
                object
                    .get("duration_ms")
                    .or_else(|| object.get("durationMs")),
            );
            push_summary_field(
                &mut summary_parts,
                "error",
                command_result_value(object, &["error", "message"]),
            );
            push_section_from_field(
                &mut sections,
                "command",
                object.get("command"),
                ToolSectionKind::Input,
            );
            push_section_from_field(
                &mut sections,
                "cwd",
                object.get("cwd"),
                ToolSectionKind::Metadata,
            );
            push_command_status_section(&mut sections, object);
            push_stream_section(&mut sections, object, "stdout", ToolSectionKind::Output);
            push_stream_section(&mut sections, object, "stderr", ToolSectionKind::Error);
        }
        "read" => {
            push_summary_field(&mut summary_parts, "file", object.get("file_path"));
            push_summary_field(&mut summary_parts, "lines", object.get("total_lines"));
            push_read_type_summary(&mut summary_parts, object);
            push_read_text_metadata_section(&mut sections, object);
            push_read_text_section(&mut sections, object);
            push_read_image_section(&mut sections, object);
            push_read_binary_section(&mut sections, object);
            push_section_from_field(
                &mut sections,
                "metadata",
                object.get("metadata"),
                ToolSectionKind::Metadata,
            );
            push_read_error_section(&mut sections, object, is_error);
        }
        "grep" | "glob" => {
            push_summary_field(&mut summary_parts, "pattern", object.get("pattern"));
            push_summary_field(&mut summary_parts, "path", object.get("path"));
            push_summary_field(&mut summary_parts, "glob", object.get("glob"));
            push_search_count_summary(&tool_name, &mut summary_parts, object);
            push_search_truncation_summary(&tool_name, &mut summary_parts, object);
            push_duration_summary_field(
                &mut summary_parts,
                "duration",
                object
                    .get("duration_ms")
                    .or_else(|| object.get("durationMs")),
            );
            push_search_section(&mut sections, "matches", object.get("matches"));
            push_search_section(&mut sections, "files", object.get("files"));
            push_section_from_field(
                &mut sections,
                "content",
                object.get("content"),
                ToolSectionKind::Output,
            );
            push_search_truncation_section(&mut sections, &tool_name, object);
        }
        "write" | "edit" | "multiedit" | "notebookedit" => {
            let old_value = object
                .get("old_string")
                .or_else(|| object.get("original_file"));
            let explicit_new_value = object
                .get("new_string")
                .or_else(|| object.get("updated_file"))
                .or_else(|| object.get("new_source"));
            let content_value = object.get("content");
            let new_value = object
                .get("new_string")
                .or_else(|| object.get("updated_file"))
                .or_else(|| object.get("new_source"))
                .or(content_value);
            push_summary_field(
                &mut summary_parts,
                "file",
                object
                    .get("file_path")
                    .or_else(|| object.get("path"))
                    .or_else(|| object.get("notebook_path")),
            );
            push_summary_field(&mut summary_parts, "mode", object.get("edit_mode"));
            push_summary_field(&mut summary_parts, "cell", object.get("cell_id"));
            push_summary_field(&mut summary_parts, "type", object.get("cell_type"));
            push_section_from_field(&mut sections, "old", old_value, ToolSectionKind::Input);
            push_section_from_field(
                &mut sections,
                "new",
                explicit_new_value,
                ToolSectionKind::Output,
            );
            push_diff_section_from_values(&mut sections, old_value, new_value);
            if explicit_new_value.is_some() {
                push_section_from_field(
                    &mut sections,
                    "content",
                    content_value,
                    ToolSectionKind::Output,
                );
            }
            push_section_from_field(
                &mut sections,
                "edits",
                object.get("edits"),
                ToolSectionKind::Metadata,
            );
            push_section_from_field(
                &mut sections,
                "error",
                object.get("error"),
                ToolSectionKind::Error,
            );
        }
        "todowrite" => {
            let todos = object.get("new_todos").or_else(|| object.get("todos"))?;
            let body = todo_lines(todos).join("\n");
            if body.is_empty() {
                return None;
            }
            summary_parts.push(format!("{} todos", todo_count(todos)));
            sections.push(ToolSection::new("todos", body, ToolSectionKind::Output));
        }
        "taskcreate" => {
            push_task_create_result(&mut summary_parts, &mut sections, object);
        }
        "tasklist" => {
            push_task_list_result(&mut summary_parts, &mut sections, object);
        }
        "taskget" => {
            push_task_get_result(&mut summary_parts, &mut sections, object);
        }
        "taskupdate" => {
            push_task_update_result(&mut summary_parts, &mut sections, object);
        }
        "taskoutput" => {
            push_task_output_result(&mut summary_parts, &mut sections, object);
        }
        "taskstop" => {
            push_task_stop_result(&mut summary_parts, &mut sections, object);
        }
        "task" | "agent" => {
            push_agent_summary_parts(&mut summary_parts, object);
            push_agent_approval_section(&mut sections, object);
            push_section_from_field(
                &mut sections,
                "result",
                agent_result_value(object),
                ToolSectionKind::Output,
            );
            push_agent_metadata_section(&mut sections, object);
            push_agent_nested_tools_section(&mut sections, object);
            push_section_from_field(
                &mut sections,
                "error",
                object.get("error"),
                ToolSectionKind::Error,
            );
        }
        "webfetch" => {
            push_summary_field(&mut summary_parts, "code", object.get("code"));
            push_summary_field(&mut summary_parts, "status", object.get("codeText"));
            push_summary_field(&mut summary_parts, "url", object.get("url"));
            push_duration_summary_field(
                &mut summary_parts,
                "duration",
                object
                    .get("duration_ms")
                    .or_else(|| object.get("durationMs")),
            );
            push_summary_field(&mut summary_parts, "bytes", object.get("bytes"));
            push_section_from_field(
                &mut sections,
                "result",
                object.get("result").or_else(|| object.get("content")),
                if is_error {
                    ToolSectionKind::Error
                } else {
                    ToolSectionKind::Output
                },
            );
            push_section_from_field(
                &mut sections,
                "error",
                object.get("error"),
                ToolSectionKind::Error,
            );
        }
        "websearch" => {
            push_summary_field(&mut summary_parts, "query", object.get("query"));
            if let Some(count) = object
                .get("results")
                .and_then(Value::as_array)
                .map(Vec::len)
            {
                summary_parts.push(format!("{count} results"));
            }
            push_summary_field(
                &mut summary_parts,
                "duration",
                object
                    .get("durationSeconds")
                    .or_else(|| object.get("duration_seconds")),
            );
            push_search_section(&mut sections, "results", object.get("results"));
            push_section_from_field(
                &mut sections,
                "error",
                object.get("error"),
                ToolSectionKind::Error,
            );
        }
        "skill" => {
            push_summary_field(
                &mut summary_parts,
                "skill",
                object
                    .get("commandName")
                    .or_else(|| object.get("command_name")),
            );
            push_summary_field(&mut summary_parts, "success", object.get("success"));
            let display_result = sanitized_skill_result_value(object.get("result"))
                .or_else(|| object.get("result").cloned());
            push_section_from_field(
                &mut sections,
                "result",
                display_result.as_ref(),
                ToolSectionKind::Output,
            );
            push_section_from_field(
                &mut sections,
                "allowed tools",
                object
                    .get("allowedTools")
                    .or_else(|| object.get("allowed_tools")),
                ToolSectionKind::Metadata,
            );
            push_section_from_field(
                &mut sections,
                "error",
                object.get("error"),
                ToolSectionKind::Error,
            );
        }
        "readmcpresource" => {
            push_summary_field(&mut summary_parts, "contents", object.get("contents"));
            if let Some(body) = mcp_contents_as_lines(object.get("contents")) {
                sections.push(ToolSection::new("contents", body, ToolSectionKind::Output));
            }
        }
        "exitplanmode" => {
            push_summary_field(&mut summary_parts, "message", object.get("message"));
            push_section_from_field(
                &mut sections,
                "message",
                object.get("message"),
                ToolSectionKind::Output,
            );
        }
        "toolsearch" => {
            push_summary_field(&mut summary_parts, "query", object.get("query"));
            push_summary_field(
                &mut summary_parts,
                "deferred tools",
                object.get("total_deferred_tools"),
            );
            push_search_section(&mut sections, "matches", object.get("matches"));
            push_search_section(
                &mut sections,
                "pending MCP servers",
                object.get("pending_mcp_servers"),
            );
        }
        "askuserquestion" => {
            push_question_lines_section(&mut sections, object.get("questions"));
            push_section_from_field(
                &mut sections,
                "answers",
                object.get("answers"),
                ToolSectionKind::Output,
            );
            if let Some(count) = object
                .get("questions")
                .and_then(Value::as_array)
                .map(Vec::len)
            {
                summary_parts.push(format!("{count} questions"));
            }
        }
        _ if is_mcp_tool_name(&tool_name) => {
            let body = semantic_json_lines(value).join("\n");
            if !body.trim().is_empty() {
                sections.push(ToolSection::new(
                    if is_error { "error" } else { "result" },
                    body,
                    if is_error {
                        ToolSectionKind::Error
                    } else {
                        ToolSectionKind::Output
                    },
                ));
            }
        }
        _ => return None,
    }

    if sections.is_empty() && summary_parts.is_empty() {
        return None;
    }

    Some(NormalizedToolContent {
        summary: summary_from_parts(&summary_parts),
        sections: if sections.is_empty() {
            vec![ToolSection::new(
                if is_error { "error" } else { "result" },
                json_preview(value),
                if is_error {
                    ToolSectionKind::Error
                } else {
                    ToolSectionKind::Output
                },
            )]
        } else {
            sections
        },
    })
}

fn default_tool_section_title(message: &MessageData) -> &'static str {
    match message.message_type {
        MessageType::ToolUse => "input",
        MessageType::ToolResult if message.is_error => "error",
        MessageType::ToolResult => "output",
        _ => "content",
    }
}

fn default_tool_section_kind(message: &MessageData) -> ToolSectionKind {
    match message.message_type {
        MessageType::ToolUse => ToolSectionKind::Input,
        MessageType::ToolResult if message.is_error => ToolSectionKind::Error,
        MessageType::ToolResult => ToolSectionKind::Output,
        _ => ToolSectionKind::Metadata,
    }
}

fn empty_tool_summary(message: &MessageData) -> &'static str {
    match message.message_type {
        MessageType::ToolUse => "(no input)",
        MessageType::ToolResult if message.is_error => "(no error details)",
        MessageType::ToolResult => "(no output)",
        _ => "(empty)",
    }
}

fn push_field_line(lines: &mut Vec<String>, label: &str, value: Option<&Value>) {
    if let Some(value) = value.and_then(value_as_display_text) {
        if !value.trim().is_empty() {
            lines.push(format!("{label}: {value}"));
        }
    }
}

fn push_read_range_line(lines: &mut Vec<String>, object: &serde_json::Map<String, Value>) {
    let offset = object.get("offset").and_then(Value::as_u64);
    let limit = object.get("limit").and_then(Value::as_u64);

    match (offset, limit) {
        (Some(offset), Some(limit)) if limit > 0 => {
            let start = offset.saturating_add(1);
            let end = offset.saturating_add(limit);
            lines.push(format!("range: lines {start}-{end}"));
        }
        _ => {
            push_field_line(lines, "offset", object.get("offset"));
            push_field_line(lines, "limit", object.get("limit"));
        }
    }
}

fn push_summary_field(parts: &mut Vec<String>, label: &str, value: Option<&Value>) {
    if let Some(value) = value.and_then(value_as_display_text) {
        if !value.trim().is_empty() {
            parts.push(format!("{label} {value}"));
        }
    }
}

fn push_duration_summary_field(parts: &mut Vec<String>, label: &str, value: Option<&Value>) {
    if let Some(value) = value.and_then(value_as_display_text) {
        if !value.trim().is_empty() {
            parts.push(format!("{label} {value}ms"));
        }
    }
}

fn command_result_value<'a>(
    object: &'a serde_json::Map<String, Value>,
    keys: &[&str],
) -> Option<&'a Value> {
    keys.iter().find_map(|key| object.get(*key))
}

fn command_result_bool(object: &serde_json::Map<String, Value>, keys: &[&str]) -> bool {
    command_result_value(object, keys)
        .and_then(value_as_bool)
        .unwrap_or(false)
}

fn push_command_flag_summary(parts: &mut Vec<String>, label: &str, enabled: bool) {
    if enabled {
        parts.push(label.to_string());
    }
}

fn push_command_status_section(
    sections: &mut Vec<ToolSection>,
    object: &serde_json::Map<String, Value>,
) {
    let mut lines = Vec::new();
    if command_result_bool(object, &["timed_out", "timedOut", "timeout"]) {
        lines.push("timeout: true".to_string());
    }
    if command_result_bool(
        object,
        &["interrupted", "was_interrupted", "wasInterrupted"],
    ) {
        lines.push("interrupted: true".to_string());
    }
    push_field_line(
        &mut lines,
        "signal",
        command_result_value(object, &["signal", "term_signal", "termSignal"]),
    );
    push_field_line(
        &mut lines,
        "error",
        command_result_value(object, &["error", "message"]),
    );

    if !lines.is_empty() {
        sections.push(ToolSection::new(
            "status",
            lines.join("\n"),
            ToolSectionKind::Error,
        ));
    }
}

fn task_field_value<'a>(
    object: &'a serde_json::Map<String, Value>,
    keys: &[&str],
) -> Option<&'a Value> {
    keys.iter().find_map(|key| object.get(*key))
}

fn push_task_create_result(
    parts: &mut Vec<String>,
    sections: &mut Vec<ToolSection>,
    object: &serde_json::Map<String, Value>,
) {
    let Some(task) = object.get("task") else {
        return;
    };

    if let Some(task_object) = task.as_object() {
        push_summary_field(parts, "task", task_object.get("id"));
        push_summary_field(parts, "subject", task_object.get("subject"));

        let mut lines = Vec::new();
        push_field_line(&mut lines, "id", task_object.get("id"));
        push_field_line(&mut lines, "subject", task_object.get("subject"));
        if !lines.is_empty() {
            sections.push(ToolSection::new(
                "task",
                lines.join("\n"),
                ToolSectionKind::Output,
            ));
        }
        return;
    }

    push_section_from_field(sections, "task", Some(task), ToolSectionKind::Output);
}

fn push_task_list_result(
    parts: &mut Vec<String>,
    sections: &mut Vec<ToolSection>,
    object: &serde_json::Map<String, Value>,
) {
    let Some(tasks) = object.get("tasks").and_then(Value::as_array) else {
        return;
    };

    parts.push(pluralize(tasks.len(), "task"));
    sections.push(ToolSection::new(
        "tasks",
        task_list_lines(tasks).join("\n"),
        ToolSectionKind::Output,
    ));
}

fn task_list_lines(tasks: &[Value]) -> Vec<String> {
    if tasks.is_empty() {
        return vec!["(no tasks)".to_string()];
    }

    let mut lines = Vec::new();
    for task in tasks.iter().take(24) {
        let Some(object) = task.as_object() else {
            if let Some(text) = value_as_display_text(task) {
                lines.push(text);
            }
            continue;
        };

        let status = task_field_value(object, &["status"])
            .and_then(value_as_display_text)
            .unwrap_or_else(|| "unknown".to_string());
        let id = task_field_value(object, &["id", "taskId", "task_id"])
            .and_then(value_as_display_text)
            .unwrap_or_else(|| "(no id)".to_string());
        let subject = task_field_value(object, &["subject", "description"])
            .and_then(value_as_display_text)
            .unwrap_or_default();

        let mut line = if subject.trim().is_empty() {
            format!("{status}: {id}")
        } else {
            format!("{status}: {id} - {subject}")
        };
        if let Some(owner) = task_field_value(object, &["owner"]).and_then(value_as_display_text) {
            if !owner.trim().is_empty() {
                line.push_str(&format!("; owner {owner}"));
            }
        }
        if let Some(blocked_by) =
            task_field_value(object, &["blockedBy", "blocked_by"]).and_then(value_as_display_text)
        {
            if !blocked_by.trim().is_empty() {
                line.push_str(&format!("; blocked by {blocked_by}"));
            }
        }
        lines.push(line);
    }

    if tasks.len() > lines.len() {
        lines.push(format!("... {} more tasks", tasks.len() - lines.len()));
    }
    lines
}

fn push_task_get_result(
    parts: &mut Vec<String>,
    sections: &mut Vec<ToolSection>,
    object: &serde_json::Map<String, Value>,
) {
    match object.get("task") {
        Some(Value::Object(task)) => {
            push_summary_field(parts, "status", task.get("status"));
            push_summary_field(parts, "task", task.get("id"));
            push_summary_field(parts, "subject", task.get("subject"));

            let mut lines = Vec::new();
            push_field_line(&mut lines, "id", task.get("id"));
            push_field_line(&mut lines, "status", task.get("status"));
            push_field_line(&mut lines, "subject", task.get("subject"));
            push_field_line(&mut lines, "blocks", task.get("blocks"));
            push_field_line(
                &mut lines,
                "blocked by",
                task.get("blockedBy").or_else(|| task.get("blocked_by")),
            );
            if !lines.is_empty() {
                sections.push(ToolSection::new(
                    "task",
                    lines.join("\n"),
                    ToolSectionKind::Metadata,
                ));
            }
            push_section_from_field(
                sections,
                "description",
                task.get("description"),
                ToolSectionKind::Output,
            );
        }
        Some(Value::Null) | None => {
            parts.push("task not found".to_string());
            sections.push(ToolSection::new(
                "task",
                "(not found)",
                ToolSectionKind::Metadata,
            ));
        }
        Some(value) => {
            push_section_from_field(sections, "task", Some(value), ToolSectionKind::Output);
        }
    }
}

fn push_task_update_result(
    parts: &mut Vec<String>,
    sections: &mut Vec<ToolSection>,
    object: &serde_json::Map<String, Value>,
) {
    push_summary_field(parts, "success", object.get("success"));
    push_summary_field(
        parts,
        "task",
        object.get("taskId").or_else(|| object.get("task_id")),
    );
    if let Some(count) = object
        .get("updatedFields")
        .or_else(|| object.get("updated_fields"))
        .and_then(Value::as_array)
        .map(Vec::len)
    {
        parts.push(format!("{} updated fields", count));
    }

    let mut lines = Vec::new();
    push_field_line(&mut lines, "success", object.get("success"));
    push_field_line(
        &mut lines,
        "task id",
        object.get("taskId").or_else(|| object.get("task_id")),
    );
    push_field_line(
        &mut lines,
        "updated fields",
        object
            .get("updatedFields")
            .or_else(|| object.get("updated_fields")),
    );
    if !lines.is_empty() {
        sections.push(ToolSection::new(
            "update",
            lines.join("\n"),
            ToolSectionKind::Output,
        ));
    }
    push_section_from_field(
        sections,
        "error",
        object.get("error"),
        ToolSectionKind::Error,
    );
}

fn push_task_output_result(
    parts: &mut Vec<String>,
    sections: &mut Vec<ToolSection>,
    object: &serde_json::Map<String, Value>,
) {
    push_summary_field(parts, "retrieval", object.get("retrieval_status"));

    match object.get("task") {
        Some(Value::Object(task)) => {
            push_summary_field(
                parts,
                "task",
                task.get("task_id").or_else(|| task.get("taskId")),
            );
            push_summary_field(parts, "status", task.get("status"));
            push_summary_field(parts, "exit", task.get("exit_code"));

            let mut lines = Vec::new();
            push_field_line(&mut lines, "retrieval", object.get("retrieval_status"));
            push_field_line(
                &mut lines,
                "task id",
                task.get("task_id").or_else(|| task.get("taskId")),
            );
            push_field_line(&mut lines, "task type", task.get("task_type"));
            push_field_line(&mut lines, "status", task.get("status"));
            push_field_line(&mut lines, "exit", task.get("exit_code"));
            push_field_line(&mut lines, "description", task.get("description"));
            if !lines.is_empty() {
                sections.push(ToolSection::new(
                    "task",
                    lines.join("\n"),
                    ToolSectionKind::Metadata,
                ));
            }

            if let Some(output) = task.get("output") {
                let output = value_as_display_text(output).unwrap_or_default();
                let body = if output.trim().is_empty() {
                    "(empty output)".to_string()
                } else {
                    output
                };
                sections.push(ToolSection::new("output", body, ToolSectionKind::Output));
            }
        }
        Some(Value::Null) | None => {
            parts.push("task not found".to_string());
            sections.push(ToolSection::new(
                "task",
                "(not found)",
                ToolSectionKind::Metadata,
            ));
        }
        Some(value) => {
            push_section_from_field(sections, "task", Some(value), ToolSectionKind::Output);
        }
    }
}

fn push_task_stop_result(
    parts: &mut Vec<String>,
    sections: &mut Vec<ToolSection>,
    object: &serde_json::Map<String, Value>,
) {
    push_summary_field(parts, "message", object.get("message"));
    push_summary_field(
        parts,
        "task",
        object.get("task_id").or_else(|| object.get("taskId")),
    );
    push_summary_field(parts, "type", object.get("task_type"));

    let mut lines = Vec::new();
    push_field_line(&mut lines, "message", object.get("message"));
    push_field_line(
        &mut lines,
        "task id",
        object.get("task_id").or_else(|| object.get("taskId")),
    );
    push_field_line(&mut lines, "task type", object.get("task_type"));
    push_field_line(&mut lines, "command", object.get("command"));
    if !lines.is_empty() {
        sections.push(ToolSection::new(
            "stopped",
            lines.join("\n"),
            ToolSectionKind::Output,
        ));
    }
}

fn agent_field_value<'a>(
    object: &'a serde_json::Map<String, Value>,
    keys: &[&str],
) -> Option<&'a Value> {
    keys.iter().find_map(|key| object.get(*key))
}

fn push_agent_summary_parts(parts: &mut Vec<String>, object: &serde_json::Map<String, Value>) {
    push_agent_summary_field(parts, "status", object, &["status", "state"]);
    push_agent_summary_field(
        parts,
        "agent",
        object,
        &["agent_id", "agentId", "agent", "task_id", "taskId"],
    );
    push_agent_summary_field(parts, "type", object, &["agent_type", "agentType"]);
    push_agent_summary_field(
        parts,
        "stopped",
        object,
        &["stopped_reason", "stoppedReason"],
    );

    if let Some(count) = agent_field_value(
        object,
        &[
            "total_tool_use_count",
            "totalToolUseCount",
            "tool_use_count",
        ],
    )
    .and_then(value_as_usize)
    {
        parts.push(pluralize(count, "nested tool call"));
    }
    push_agent_summary_field(
        parts,
        "last tool",
        object,
        &[
            "last_tool_use_name",
            "lastToolUseName",
            "last_tool",
            "lastTool",
        ],
    );
    push_agent_summary_field(
        parts,
        "tokens",
        object,
        &[
            "total_token_count",
            "totalTokenCount",
            "token_count",
            "tokenCount",
        ],
    );
    push_duration_summary_field(
        parts,
        "duration",
        agent_field_value(
            object,
            &[
                "total_duration_ms",
                "totalDurationMs",
                "duration_ms",
                "durationMs",
            ],
        ),
    );
}

fn push_agent_summary_field(
    parts: &mut Vec<String>,
    label: &str,
    object: &serde_json::Map<String, Value>,
    keys: &[&str],
) {
    if let Some(value) = agent_field_value(object, keys).and_then(value_as_display_text) {
        if !value.trim().is_empty() {
            parts.push(format!("{label} {value}"));
        }
    }
}

fn push_agent_metadata_section(
    sections: &mut Vec<ToolSection>,
    object: &serde_json::Map<String, Value>,
) {
    let mut lines = Vec::new();
    push_agent_field_line(&mut lines, "status", object, &["status", "state"]);
    push_agent_field_line(
        &mut lines,
        "agent type",
        object,
        &["agent_type", "agentType"],
    );
    push_agent_field_line(
        &mut lines,
        "agent",
        object,
        &["agent_id", "agentId", "agent"],
    );
    push_agent_field_line(&mut lines, "task", object, &["task_id", "taskId"]);
    push_agent_field_line(
        &mut lines,
        "stopped",
        object,
        &["stopped_reason", "stoppedReason"],
    );
    push_agent_field_line(&mut lines, "turns", object, &["turn_count", "turnCount"]);
    push_agent_field_line(
        &mut lines,
        "tokens",
        object,
        &[
            "total_token_count",
            "totalTokenCount",
            "token_count",
            "tokenCount",
        ],
    );
    if let Some(duration) = agent_field_value(
        object,
        &[
            "total_duration_ms",
            "totalDurationMs",
            "duration_ms",
            "durationMs",
        ],
    )
    .and_then(value_as_u64)
    {
        lines.push(format!("duration: {duration}ms"));
    }
    push_agent_field_line(
        &mut lines,
        "worktree",
        object,
        &["worktree_path", "worktreePath"],
    );

    if !lines.is_empty() {
        sections.push(ToolSection::new(
            "agent",
            lines.join("\n"),
            ToolSectionKind::Metadata,
        ));
    }
}

fn push_agent_field_line(
    lines: &mut Vec<String>,
    label: &str,
    object: &serde_json::Map<String, Value>,
    keys: &[&str],
) {
    if let Some(value) = agent_field_value(object, keys).and_then(value_as_display_text) {
        if !value.trim().is_empty() {
            lines.push(format!("{label}: {value}"));
        }
    }
}

fn push_agent_nested_tools_section(
    sections: &mut Vec<ToolSection>,
    object: &serde_json::Map<String, Value>,
) {
    let mut lines = Vec::new();
    if let Some(count) = agent_field_value(
        object,
        &[
            "total_tool_use_count",
            "totalToolUseCount",
            "tool_use_count",
        ],
    )
    .and_then(value_as_usize)
    {
        lines.push(format!("total: {}", pluralize(count, "tool call")));
    }
    if let Some(last_tool) = agent_field_value(
        object,
        &[
            "last_tool_use_name",
            "lastToolUseName",
            "last_tool",
            "lastTool",
        ],
    )
    .and_then(value_as_display_text)
    {
        if !last_tool.trim().is_empty() {
            lines.push(format!("last tool: {last_tool}"));
        }
    }

    let mut nested_names = Vec::new();
    for key in [
        "messages",
        "transcript",
        "conversation",
        "nested_messages",
        "nestedMessages",
        "tool_uses",
        "toolUses",
        "nested_tools",
        "nestedTools",
    ] {
        if let Some(value) = object.get(key) {
            collect_nested_tool_names(value, &mut nested_names, 64);
        }
    }
    if !nested_names.is_empty() {
        lines.push(format!(
            "transcript: {}",
            pluralize(nested_names.len(), "tool call")
        ));
        let mut recent = nested_names
            .iter()
            .rev()
            .take(5)
            .cloned()
            .collect::<Vec<_>>();
        recent.reverse();
        lines.push(format!("recent tools: {}", recent.join(", ")));
    }

    if !lines.is_empty() {
        sections.push(ToolSection::new(
            "nested tools",
            lines.join("\n"),
            ToolSectionKind::Metadata,
        ));
    }
}

fn collect_nested_tool_names(value: &Value, out: &mut Vec<String>, budget: usize) {
    if out.len() >= budget {
        return;
    }
    match value {
        Value::Array(items) => {
            for item in items {
                collect_nested_tool_names(item, out, budget);
                if out.len() >= budget {
                    break;
                }
            }
        }
        Value::Object(object) => {
            if object.get("type").and_then(Value::as_str) == Some("tool_use") {
                if let Some(name) = object.get("name").and_then(Value::as_str) {
                    if !name.trim().is_empty() {
                        out.push(name.to_string());
                    }
                }
            }
            for key in [
                "content",
                "message",
                "messages",
                "tool_uses",
                "toolUses",
                "children",
                "events",
                "items",
            ] {
                if let Some(child) = object.get(key) {
                    collect_nested_tool_names(child, out, budget);
                    if out.len() >= budget {
                        break;
                    }
                }
            }
        }
        _ => {}
    }
}

fn push_agent_approval_section(
    sections: &mut Vec<ToolSection>,
    object: &serde_json::Map<String, Value>,
) {
    let Some(value) = agent_field_value(
        object,
        &[
            "pending_approval",
            "pendingApproval",
            "approval_request",
            "approvalRequest",
            "approval",
            "requires_approval",
            "requiresApproval",
            "approval_required",
            "approvalRequired",
            "waiting_approval",
            "waitingApproval",
        ],
    ) else {
        return;
    };

    let mut lines = match value {
        Value::Bool(true) => vec!["approval required".to_string()],
        Value::Bool(false) | Value::Null => Vec::new(),
        Value::String(text) if text.trim().is_empty() => Vec::new(),
        Value::String(text) => vec![text.clone()],
        Value::Array(_) | Value::Object(_) | Value::Number(_) => semantic_json_lines(value),
    };
    lines.retain(|line| !line.trim().is_empty());

    if !lines.is_empty() {
        sections.push(ToolSection::new(
            "nested approval",
            lines.join("\n"),
            ToolSectionKind::Metadata,
        ));
    }
}

fn agent_result_value(object: &serde_json::Map<String, Value>) -> Option<&Value> {
    agent_field_value(
        object,
        &[
            "result_text",
            "resultText",
            "result",
            "output",
            "final_summary",
            "finalSummary",
        ],
    )
}

fn push_read_type_summary(parts: &mut Vec<String>, object: &serde_json::Map<String, Value>) {
    match object.get("type").and_then(Value::as_str) {
        Some("image") => {
            parts.push("image".to_string());
            push_summary_field(parts, "media", object.get("media_type"));
            if let Some(bytes) = object.get("size_bytes").and_then(Value::as_u64) {
                parts.push(format!("{bytes} bytes"));
            }
        }
        Some("binary") => {
            parts.push("binary".to_string());
            if let Some(bytes) = object.get("size_bytes").and_then(Value::as_u64) {
                parts.push(format!("{bytes} bytes"));
            }
        }
        Some("error") => {
            parts.push("error".to_string());
        }
        _ => {}
    }
}

fn push_read_text_metadata_section(
    sections: &mut Vec<ToolSection>,
    object: &serde_json::Map<String, Value>,
) {
    if object.get("type").and_then(Value::as_str) == Some("image")
        || object.get("type").and_then(Value::as_str) == Some("binary")
        || object.get("type").and_then(Value::as_str) == Some("error")
    {
        return;
    }
    if object
        .get("content")
        .and_then(value_as_display_text)
        .is_none()
    {
        return;
    }

    let mut lines = Vec::new();
    push_field_line(&mut lines, "file", object.get("file_path"));
    if let Some(range) = read_visible_range_label(object) {
        lines.push(format!("range: {range}"));
    }
    if let Some(total) = object.get("total_lines").and_then(value_as_usize) {
        lines.push(format!("total: {}", pluralize(total, "line")));
    }

    if !lines.is_empty() {
        sections.push(ToolSection::new(
            "read",
            lines.join("\n"),
            ToolSectionKind::Metadata,
        ));
    }
}

fn push_read_text_section(
    sections: &mut Vec<ToolSection>,
    object: &serde_json::Map<String, Value>,
) {
    let Some(content) = object.get("content").and_then(value_as_display_text) else {
        return;
    };
    if content.trim().is_empty() {
        return;
    }

    let fallback_start_line = read_start_line(object);
    let (body, start_line) = read_code_body_and_start_line(&content, fallback_start_line);
    let hidden_lines = read_hidden_line_count(object, start_line, &content).unwrap_or(0);
    let file_path = object
        .get("file_path")
        .or_else(|| object.get("path"))
        .and_then(value_as_display_text);

    sections.push(
        ToolSection::new("content", body, ToolSectionKind::Output).with_code(
            CodeSectionRenderModel {
                file_path,
                start_line,
                line_numbers: true,
                hidden_lines,
            },
        ),
    );
}

fn push_read_image_section(
    sections: &mut Vec<ToolSection>,
    object: &serde_json::Map<String, Value>,
) {
    if object.get("type").and_then(Value::as_str) != Some("image") {
        return;
    }

    let mut lines = Vec::new();
    push_field_line(&mut lines, "file", object.get("file_path"));
    push_field_line(&mut lines, "media", object.get("media_type"));
    if let Some(bytes) = object.get("size_bytes").and_then(Value::as_u64) {
        lines.push(format!("{bytes} bytes"));
    }

    if !lines.is_empty() {
        sections.push(ToolSection::new(
            "image",
            lines.join("\n"),
            ToolSectionKind::Metadata,
        ));
    }
}

fn push_read_binary_section(
    sections: &mut Vec<ToolSection>,
    object: &serde_json::Map<String, Value>,
) {
    if object.get("type").and_then(Value::as_str) != Some("binary") {
        return;
    }

    let mut lines = Vec::new();
    push_field_line(&mut lines, "file", object.get("file_path"));
    push_field_line(&mut lines, "message", object.get("message"));
    if let Some(bytes) = object.get("size_bytes").and_then(Value::as_u64) {
        lines.push(format!("{bytes} bytes"));
    }

    if !lines.is_empty() {
        sections.push(ToolSection::new(
            "binary",
            lines.join("\n"),
            ToolSectionKind::Output,
        ));
    }
}

fn push_read_error_section(
    sections: &mut Vec<ToolSection>,
    object: &serde_json::Map<String, Value>,
    is_error: bool,
) {
    if !is_error && object.get("type").and_then(Value::as_str) != Some("error") {
        return;
    }
    push_section_from_field(
        sections,
        "error",
        object.get("error").or_else(|| object.get("message")),
        ToolSectionKind::Error,
    );
}

fn read_visible_range_label(object: &serde_json::Map<String, Value>) -> Option<String> {
    let content = object.get("content").and_then(value_as_display_text)?;
    let visible = count_visible_lines(&content);
    if visible == 0 {
        return None;
    }
    let start =
        read_content_number_gutter_start_line(&content).unwrap_or_else(|| read_start_line(object));
    let end = start.saturating_add(visible.saturating_sub(1));
    if start == end {
        Some(format!("line {start}"))
    } else {
        Some(format!("lines {start}-{end}"))
    }
}

fn read_start_line(object: &serde_json::Map<String, Value>) -> usize {
    object
        .get("start_line")
        .or_else(|| object.get("startLine"))
        .or_else(|| object.get("line_start"))
        .or_else(|| object.get("lineStart"))
        .and_then(value_as_usize)
        .or_else(|| {
            object
                .get("offset")
                .and_then(value_as_usize)
                .map(|offset| offset.saturating_add(1))
        })
        .unwrap_or(1)
}

fn read_code_body_and_start_line(content: &str, fallback_start_line: usize) -> (String, usize) {
    read_content_without_number_gutters(content)
        .unwrap_or_else(|| (content.to_string(), fallback_start_line))
}

fn read_content_number_gutter_start_line(content: &str) -> Option<usize> {
    content
        .lines()
        .find_map(|line| strip_read_number_gutter(line).map(|(line_number, _)| line_number))
}

fn read_content_without_number_gutters(content: &str) -> Option<(String, usize)> {
    let mut start_line = None;
    let mut stripped_any = false;
    let mut stripped_lines = Vec::new();

    for line in content.lines() {
        if let Some((line_number, code)) = strip_read_number_gutter(line) {
            start_line.get_or_insert(line_number);
            stripped_any = true;
            stripped_lines.push(code.to_string());
        } else {
            stripped_lines.push(line.to_string());
        }
    }

    if stripped_any {
        start_line.map(|line| (stripped_lines.join("\n"), line))
    } else {
        None
    }
}

fn strip_read_number_gutter(line: &str) -> Option<(usize, &str)> {
    let trimmed = line.trim_start();
    let mut digit_end = 0usize;
    for (index, ch) in trimmed.char_indices() {
        if ch.is_ascii_digit() {
            digit_end = index + ch.len_utf8();
        } else {
            break;
        }
    }
    if digit_end == 0 {
        return None;
    }

    let number = trimmed[..digit_end].parse::<usize>().ok()?;
    let rest = &trimmed[digit_end..];
    let separator = rest.chars().next()?;
    if !matches!(separator, '│' | '|' | ':') {
        return None;
    }
    Some((number, &rest[separator.len_utf8()..]))
}

fn read_hidden_line_count(
    object: &serde_json::Map<String, Value>,
    start_line: usize,
    content: &str,
) -> Option<usize> {
    let total = object.get("total_lines").and_then(value_as_usize)?;
    let visible = count_visible_lines(content);
    if visible == 0 {
        return None;
    }
    let shown_through = start_line.saturating_add(visible.saturating_sub(1));
    (total > shown_through).then_some(total - shown_through)
}

fn normalize_mcp_resource_list_result(value: &Value) -> Option<NormalizedToolContent> {
    let resources = if let Some(array) = value.as_array() {
        array
    } else {
        value
            .as_object()
            .and_then(|object| object.get("resources"))
            .and_then(Value::as_array)?
    };

    let count = resources.len();
    let body = if count == 0 {
        "(no resources)".to_string()
    } else {
        resources
            .iter()
            .take(24)
            .map(|resource| {
                if let Some(object) = resource.as_object() {
                    let server = object
                        .get("server")
                        .and_then(Value::as_str)
                        .unwrap_or("server");
                    let name = object.get("name").and_then(Value::as_str).unwrap_or("");
                    let uri = object.get("uri").and_then(Value::as_str).unwrap_or("");
                    let mime = object
                        .get("mimeType")
                        .or_else(|| object.get("mime_type"))
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    return [
                        format!("[{server}] {name}"),
                        uri.to_string(),
                        mime.to_string(),
                    ]
                    .into_iter()
                    .filter(|part| !part.trim().is_empty())
                    .collect::<Vec<_>>()
                    .join(" · ");
                }
                value_as_display_text(resource).unwrap_or_else(|| "(resource)".to_string())
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    Some(NormalizedToolContent {
        summary: Some(format!("{count} resources")),
        sections: vec![ToolSection::new("resources", body, ToolSectionKind::Output)],
    })
}

fn mcp_contents_as_lines(value: Option<&Value>) -> Option<String> {
    let contents = value?.as_array()?;
    if contents.is_empty() {
        return Some("(no contents)".to_string());
    }

    let lines = contents
        .iter()
        .take(12)
        .map(|content| {
            if let Some(object) = content.as_object() {
                let uri = object.get("uri").and_then(Value::as_str).unwrap_or("");
                let mime = object
                    .get("mimeType")
                    .or_else(|| object.get("mime_type"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let text = object.get("text").and_then(Value::as_str).unwrap_or("");
                let mut parts = Vec::new();
                if !uri.is_empty() {
                    parts.push(format!("uri: {uri}"));
                }
                if !mime.is_empty() {
                    parts.push(format!("mime: {mime}"));
                }
                if !text.is_empty() {
                    parts.push(format!("text: {text}"));
                }
                return parts.join("\n");
            }
            value_as_display_text(content).unwrap_or_else(|| "(content)".to_string())
        })
        .collect::<Vec<_>>();
    Some(lines.join("\n"))
}

fn push_stream_section(
    sections: &mut Vec<ToolSection>,
    object: &serde_json::Map<String, Value>,
    key: &str,
    kind: ToolSectionKind,
) {
    let Some(mut body) = object.get(key).and_then(value_as_display_text) else {
        return;
    };
    if body.trim().is_empty() {
        return;
    }

    let hidden_key = format!("{key}_hidden_lines");
    let hidden = object.get(&hidden_key).and_then(Value::as_u64).unwrap_or(0);
    let truncated_key = format!("{key}_truncated_lines");
    let truncated = object
        .get(&truncated_key)
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if hidden > 0 {
        if !body.ends_with('\n') {
            body.push('\n');
        }
        body.push_str(&format!("… {hidden} more lines"));
    }
    if truncated {
        if !body.ends_with('\n') {
            body.push('\n');
        }
        body.push_str("… long lines clipped");
    }

    sections.push(ToolSection::new(key, body, kind));
}

fn push_section_from_field(
    sections: &mut Vec<ToolSection>,
    title: &str,
    value: Option<&Value>,
    kind: ToolSectionKind,
) {
    if let Some(body) = value.and_then(value_as_display_text) {
        if !body.trim().is_empty() {
            sections.push(ToolSection::new(title, body, kind));
        }
    }
}

fn push_diff_section_from_values(
    sections: &mut Vec<ToolSection>,
    old_value: Option<&Value>,
    new_value: Option<&Value>,
) {
    let old = old_value
        .and_then(value_as_display_text)
        .unwrap_or_default();
    let Some(new) = new_value.and_then(value_as_display_text) else {
        return;
    };
    if new.trim().is_empty() || old == new {
        return;
    }

    sections.push(ToolSection::new(
        "diff",
        unified_diff_preview(&old, &new, 80),
        ToolSectionKind::Diff,
    ));
}

fn unified_diff_preview(old: &str, new: &str, max_changes: usize) -> String {
    let diff = TextDiff::from_lines(old, new);
    let mut lines = vec!["--- before".to_string(), "+++ after".to_string()];
    let mut count = 0usize;
    let mut clipped = false;

    for change in diff.iter_all_changes() {
        if count >= max_changes {
            clipped = true;
            break;
        }
        let sign = match change.tag() {
            ChangeTag::Delete => "-",
            ChangeTag::Insert => "+",
            ChangeTag::Equal => " ",
        };
        let value = change.value().trim_end_matches('\n');
        lines.push(format!("{} {}", sign, value));
        count += 1;
    }

    if clipped {
        lines.push("… diff truncated".to_string());
    }
    lines.join("\n")
}

fn push_search_count_summary(
    tool_name: &str,
    parts: &mut Vec<String>,
    object: &serde_json::Map<String, Value>,
) {
    if let Some(value) = object.get("count").or_else(|| object.get("total")) {
        push_summary_field(parts, "count", Some(value));
        return;
    }

    let label = if tool_name == "glob" {
        "files"
    } else {
        "matches"
    };
    if let Some(count) = object
        .get("matches")
        .or_else(|| object.get("results"))
        .or_else(|| object.get("files"))
        .and_then(Value::as_array)
        .map(Vec::len)
    {
        parts.push(format!("{count} {label}"));
    }
}

fn push_search_truncation_summary(
    tool_name: &str,
    parts: &mut Vec<String>,
    object: &serde_json::Map<String, Value>,
) {
    if search_is_upstream_truncated(tool_name, object) {
        parts.push("upstream result truncated".to_string());
    }
}

fn push_search_truncation_section(
    sections: &mut Vec<ToolSection>,
    tool_name: &str,
    object: &serde_json::Map<String, Value>,
) {
    if !search_is_upstream_truncated(tool_name, object) {
        return;
    }

    let label = search_result_label(tool_name);
    let mut lines = vec!["upstream result truncated".to_string()];
    if let Some(shown) = search_visible_count(tool_name, object) {
        if let Some(total) = search_total_count(object).filter(|total| *total >= shown) {
            lines.push(format!(
                "shown: {} of {}",
                pluralize(shown, label),
                pluralize(total, label)
            ));
        } else {
            lines.push(format!("shown: {}", pluralize(shown, label)));
        }
    }
    if let Some(hidden) = search_hidden_count(tool_name, object) {
        lines.push(format!("hidden: {}", pluralize(hidden, label)));
    }
    if let Some(limit) = search_limit(object) {
        lines.push(format!("limit: {}", pluralize(limit, label)));
    }
    if let Some(note) = search_truncation_message(object) {
        lines.push(format!("note: {note}"));
    }

    sections.push(ToolSection::new(
        "truncation",
        lines.join("\n"),
        ToolSectionKind::Metadata,
    ));
}

fn search_result_label(tool_name: &str) -> &'static str {
    if tool_name == "glob" {
        "file"
    } else {
        "match"
    }
}

fn search_result_key(tool_name: &str) -> &'static str {
    if tool_name == "glob" {
        "files"
    } else {
        "matches"
    }
}

fn search_is_upstream_truncated(tool_name: &str, object: &serde_json::Map<String, Value>) -> bool {
    search_truncation_flag(object)
        || search_truncation_message(object).is_some()
        || search_hidden_count(tool_name, object).is_some_and(|count| count > 0)
        || search_visible_count(tool_name, object)
            .zip(search_total_count(object))
            .is_some_and(|(shown, total)| total > shown)
}

fn search_truncation_flag(object: &serde_json::Map<String, Value>) -> bool {
    [
        "truncated",
        "is_truncated",
        "isTruncated",
        "result_truncated",
        "resultTruncated",
        "results_truncated",
        "resultsTruncated",
        "upstream_truncated",
        "upstreamTruncated",
    ]
    .iter()
    .any(|key| object.get(*key).and_then(Value::as_bool).unwrap_or(false))
}

fn search_truncation_message(object: &serde_json::Map<String, Value>) -> Option<String> {
    [
        "message",
        "warning",
        "note",
        "truncation",
        "truncation_message",
        "truncationMessage",
    ]
    .iter()
    .filter_map(|key| object.get(*key).and_then(value_as_display_text))
    .find(|message| {
        let lower = message.to_ascii_lowercase();
        lower.contains("truncat")
            || lower.contains("omitted")
            || lower.contains("hidden")
            || lower.contains("not shown")
    })
}

fn search_visible_count(tool_name: &str, object: &serde_json::Map<String, Value>) -> Option<usize> {
    let primary = search_result_key(tool_name);
    object
        .get(primary)
        .or_else(|| object.get("results"))
        .or_else(|| object.get("content"))
        .and_then(|value| match value {
            Value::Array(items) => Some(items.len()),
            Value::String(content) => Some(count_visible_lines(content)),
            _ => None,
        })
}

fn search_total_count(object: &serde_json::Map<String, Value>) -> Option<usize> {
    [
        "total",
        "count",
        "total_count",
        "totalCount",
        "total_matches",
        "totalMatches",
        "total_files",
        "totalFiles",
        "result_count",
        "resultCount",
    ]
    .iter()
    .find_map(|key| object.get(*key).and_then(value_as_usize))
}

fn search_hidden_count(tool_name: &str, object: &serde_json::Map<String, Value>) -> Option<usize> {
    let key = search_result_key(tool_name);
    let label_hidden = format!("{key}_hidden");
    let label_hidden_count = format!("{key}_hidden_count");
    [
        label_hidden.as_str(),
        label_hidden_count.as_str(),
        "hidden",
        "hidden_count",
        "hiddenCount",
        "omitted",
        "omitted_count",
        "omittedCount",
        "remaining",
        "remaining_count",
        "remainingCount",
    ]
    .iter()
    .find_map(|key| object.get(*key).and_then(value_as_usize))
}

fn search_limit(object: &serde_json::Map<String, Value>) -> Option<usize> {
    [
        "limit",
        "max_results",
        "maxResults",
        "max_count",
        "maxCount",
    ]
    .iter()
    .find_map(|key| object.get(*key).and_then(value_as_usize))
}

fn push_search_section(sections: &mut Vec<ToolSection>, title: &str, value: Option<&Value>) {
    let Some(value) = value else {
        return;
    };
    let Some(body) = search_value_as_lines(value) else {
        return;
    };
    if body.trim().is_empty() {
        return;
    }
    sections.push(ToolSection::new(title, body, ToolSectionKind::Output));
}

fn search_value_as_lines(value: &Value) -> Option<String> {
    match value {
        Value::Array(items) if items.is_empty() => None,
        Value::Array(items) => Some(
            items
                .iter()
                .map(|item| {
                    if let Some(path) = item.as_str() {
                        return path.to_string();
                    }
                    if let Some(object) = item.as_object() {
                        if object.contains_key("title")
                            || object.contains_key("url")
                            || object.contains_key("snippet")
                        {
                            let title = object.get("title").and_then(Value::as_str).unwrap_or("");
                            let url = object.get("url").and_then(Value::as_str).unwrap_or("");
                            let snippet = object
                                .get("snippet")
                                .or_else(|| object.get("description"))
                                .and_then(Value::as_str)
                                .unwrap_or("");
                            return [title, url, snippet]
                                .into_iter()
                                .filter(|part| !part.trim().is_empty())
                                .collect::<Vec<_>>()
                                .join(" - ");
                        }
                        let path = object
                            .get("path")
                            .and_then(Value::as_str)
                            .unwrap_or("(unknown)");
                        let line = object.get("line").and_then(Value::as_u64);
                        let text = object.get("text").and_then(Value::as_str);
                        return match (line, text) {
                            (Some(line), Some(text)) => format!("{path}:{line}: {text}"),
                            (Some(line), None) => format!("{path}:{line}"),
                            (None, Some(text)) => format!("{path}: {text}"),
                            (None, None) => path.to_string(),
                        };
                    }
                    json_preview(item)
                })
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        _ => value_as_display_text(value),
    }
}

fn normalize_plain_tool_result(
    tool_name: &str,
    content: &str,
    is_error: bool,
) -> Option<NormalizedToolContent> {
    match tool_name {
        "glob" => normalize_plain_search_result("files", content),
        "grep" => normalize_plain_search_result("matches", content),
        _ if is_error => Some(NormalizedToolContent {
            summary: Some(truncate_chars_with_suffix(
                content.trim(),
                TOOL_SUMMARY_CHARS,
                "...",
            )),
            sections: vec![ToolSection::new("error", content, ToolSectionKind::Error)],
        }),
        _ => None,
    }
}

fn normalize_plain_search_result(label: &str, content: &str) -> Option<NormalizedToolContent> {
    let mut truncated = false;
    let lines = content
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                return None;
            }
            if trimmed.starts_with("(Results are truncated.") {
                truncated = true;
                return None;
            }
            Some(trimmed.to_string())
        })
        .collect::<Vec<_>>();

    if lines.is_empty() {
        return None;
    }

    let mut summary_parts = vec![format!("{} {label}", lines.len())];
    if truncated {
        summary_parts.push("upstream result truncated".to_string());
    }
    Some(NormalizedToolContent {
        summary: Some(summary_parts.join(" | ")),
        sections: vec![ToolSection::new(
            label,
            lines.join("\n"),
            ToolSectionKind::Output,
        )],
    })
}

fn value_as_display_text(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Array(values) => {
            if values.is_empty() {
                None
            } else {
                Some(semantic_array_inline(values))
            }
        }
        Value::Object(object) => Some(semantic_object_inline(object)),
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

fn value_as_bool(value: &Value) -> Option<bool> {
    value.as_bool().or_else(|| {
        value.as_str().and_then(|text| {
            let normalized = text.trim().to_ascii_lowercase();
            match normalized.as_str() {
                "true" | "yes" | "1" => Some(true),
                "false" | "no" | "0" => Some(false),
                _ => None,
            }
        })
    })
}

fn value_as_usize(value: &Value) -> Option<usize> {
    value_as_u64(value).and_then(|value| usize::try_from(value).ok())
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

fn json_preview(value: &Value) -> String {
    serde_json::to_string_pretty(value)
        .or_else(|_| serde_json::to_string(value))
        .unwrap_or_else(|_| "<unrenderable json>".to_string())
}

fn summary_from_parts(parts: &[String]) -> Option<String> {
    if parts.is_empty() {
        None
    } else {
        Some(truncate_chars_with_suffix(
            &parts.join(" | "),
            TOOL_SUMMARY_CHARS,
            "...",
        ))
    }
}

fn todo_lines(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|todos| {
            todos
                .iter()
                .filter_map(|todo| {
                    let status = todo
                        .get("status")
                        .and_then(Value::as_str)
                        .unwrap_or("pending");
                    let content = todo
                        .get("content")
                        .and_then(Value::as_str)
                        .or_else(|| todo.get("text").and_then(Value::as_str))?;
                    Some(format!("{status}: {content}"))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn todo_count(value: &Value) -> usize {
    value.as_array().map(Vec::len).unwrap_or(0)
}

fn is_protocol_only_record(record: &TranscriptRecord) -> bool {
    let content = record.content.trim();
    let is_tool = matches!(
        record.kind,
        TranscriptRecordKind::ToolUse | TranscriptRecordKind::ToolResult
    ) || record.tool_name.is_some();
    if content.is_empty() {
        return !is_tool;
    }
    if is_tool {
        return false;
    }

    content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .all(is_protocol_noise_line)
}

fn is_protocol_noise_line(line: &str) -> bool {
    matches!(
        line,
        "(no content - terminal=Completed)"
            | "no content - terminal=Completed"
            | "terminal=Completed"
            | "(terminal=Completed)"
            | "… (stop: tool_use)"
            | "... (stop: tool_use)"
            | "(stop: tool_use)"
            | "stop: tool_use"
            | "null"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::UiStage;
    use mossen_types::{TextBlock, ToolResultBlock, ToolUseBlock};

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

    fn engine_message(role: Role, content: Vec<ContentBlock>) -> Message {
        Message {
            role,
            content,
            uuid: None,
            is_meta: None,
            origin: None,
            timestamp: None,
            extra: Default::default(),
        }
    }

    fn tool_message(
        message_type: MessageType,
        name: &str,
        content: impl Into<String>,
    ) -> MessageData {
        let mut msg = message(message_type, content);
        msg.tool_name = Some(name.to_string());
        msg
    }

    fn transcript_record(
        id: &str,
        source_index: usize,
        kind: TranscriptRecordKind,
        tool_name: Option<&str>,
        content: impl Into<String>,
    ) -> TranscriptRecord {
        TranscriptRecord {
            id: id.to_string(),
            parent_id: None,
            turn_id: None,
            source_index,
            kind,
            lifecycle: match kind {
                TranscriptRecordKind::ToolUse => LifecyclePhase::RunningTool,
                _ => LifecyclePhase::Completed,
            },
            content: content.into(),
            timestamp: None,
            tool_name: tool_name.map(str::to_string),
            is_streaming: false,
            is_error: false,
            thinking: None,
            thinking_completed: false,
            full_content: None,
            expanded: false,
        }
    }

    #[test]
    fn converts_messages_to_semantic_blocks() {
        let messages = vec![
            message(MessageType::User, "分析当前项目"),
            message(MessageType::Assistant, "## Plan\n\n- Read files"),
        ];

        let transcript = RenderTranscript::from_messages(&messages);

        assert_eq!(transcript.blocks.len(), 2);
        assert_eq!(transcript.blocks[0].kind, RenderBlockKind::User);
        assert_eq!(transcript.blocks[1].kind, RenderBlockKind::Assistant);
        assert_eq!(
            transcript.blocks[1].nodes,
            vec![RenderNode::Markdown("## Plan\n\n- Read files".to_string())]
        );
    }

    #[test]
    fn approval_decision_marker_becomes_semantic_block() {
        let decision = ApprovalDecisionModel {
            id: "approval-decision-1".to_string(),
            tool_name: "Bash".to_string(),
            decision: ApprovalDecisionKind::AlwaysAllowed,
            detail: "cargo test".to_string(),
            anchor_block_id: Some("tool-0-1".to_string()),
        };
        let messages = vec![message(
            MessageType::System,
            approval_decision_message_content(&decision),
        )];

        let transcript = RenderTranscript::from_messages(&messages);

        assert_eq!(transcript.blocks.len(), 1);
        assert_eq!(transcript.blocks[0].kind, RenderBlockKind::ApprovalDecision);
        assert_eq!(
            transcript.blocks[0].nodes,
            vec![RenderNode::ApprovalDecision(decision)]
        );
    }

    #[test]
    fn sidecar_approval_decision_is_inserted_after_anchor_block() {
        let messages = vec![
            tool_message(MessageType::ToolUse, "Bash", r#"{"command":"cargo test"}"#),
            tool_message(
                MessageType::ToolResult,
                "Bash",
                r#"{"stdout":"ok","exit_code":0}"#,
            ),
            message(MessageType::Assistant, "done"),
        ];
        let decision = ApprovalDecisionModel {
            id: "approval-decision-1".to_string(),
            tool_name: "Bash".to_string(),
            decision: ApprovalDecisionKind::Allowed,
            detail: "cargo test".to_string(),
            anchor_block_id: Some("tool-0-1".to_string()),
        };

        let transcript = RenderTranscript::from_messages_and_decisions(&messages, &[decision]);

        assert_eq!(transcript.blocks.len(), 3);
        assert_eq!(transcript.blocks[0].id, "tool-0");
        assert_eq!(transcript.blocks[0].kind, RenderBlockKind::Tool);
        assert_eq!(transcript.blocks[1].kind, RenderBlockKind::ApprovalDecision);
        assert_eq!(transcript.blocks[2].kind, RenderBlockKind::Assistant);
    }

    #[test]
    fn approval_history_collects_decision_rows() {
        let decisions = vec![
            ApprovalDecisionModel {
                id: "approval-allowed".to_string(),
                tool_name: "Bash".to_string(),
                decision: ApprovalDecisionKind::Allowed,
                detail: "cargo test".to_string(),
                anchor_block_id: Some("tool-0".to_string()),
            },
            ApprovalDecisionModel {
                id: "approval-denied".to_string(),
                tool_name: "Write".to_string(),
                decision: ApprovalDecisionKind::Denied,
                detail: "/tmp/output.md".to_string(),
                anchor_block_id: None,
            },
        ];
        let transcript = RenderTranscript::from_messages_and_decisions(&[], &decisions);

        let history = approval_history_from_transcript(&transcript);

        assert_eq!(history.summary.total_count, 2);
        assert_eq!(history.summary.allowed_count, 1);
        assert_eq!(history.summary.denied_count, 1);
        assert_eq!(history.summary.pending_count, 0);
        let row = &history.rows[0];
        assert_eq!(row.status_label(), "Allowed");
        assert_eq!(row.tool_name, "Bash");
        assert_eq!(row.detail, "cargo test");
        assert_eq!(row.anchor_block_id.as_deref(), Some("tool-0"));
        assert_eq!(row.source_block_id.as_deref(), Some("approval-allowed"));
    }

    #[test]
    fn tool_anchor_id_stays_stable_when_result_arrives() {
        let requested = vec![tool_message(
            MessageType::ToolUse,
            "Bash",
            r#"{"command":"cargo test"}"#,
        )];
        let completed = vec![
            tool_message(MessageType::ToolUse, "Bash", r#"{"command":"cargo test"}"#),
            tool_message(
                MessageType::ToolResult,
                "Bash",
                r#"{"stdout":"ok","exit_code":0}"#,
            ),
        ];

        let requested = RenderTranscript::from_messages(&requested);
        let completed = RenderTranscript::from_messages(&completed);

        assert_eq!(requested.blocks[0].id, "tool-0");
        assert_eq!(completed.blocks[0].id, "tool-0");
    }

    #[test]
    fn from_records_uses_layer1_ids_source_indices_and_anchors() {
        let records = TranscriptRecords {
            entries: vec![
                transcript_record(
                    "tool-call-shell-42",
                    12,
                    TranscriptRecordKind::ToolUse,
                    Some("Bash"),
                    r#"{"command":"cargo test"}"#,
                ),
                transcript_record(
                    "tool-call-shell-42-result",
                    77,
                    TranscriptRecordKind::ToolResult,
                    Some("Bash"),
                    r#"{"stdout":"ok","exit_code":0}"#,
                ),
            ],
            approval_decisions: vec![ApprovalDecisionModel {
                id: "approval-shell-42".to_string(),
                tool_name: "Bash".to_string(),
                decision: ApprovalDecisionKind::Allowed,
                detail: "cargo test".to_string(),
                anchor_block_id: Some("tool-call-shell-42".to_string()),
            }],
            final_summaries: Vec::new(),
        };

        let transcript = RenderTranscript::from_records(&records);

        assert_eq!(transcript.blocks.len(), 2);
        assert_eq!(transcript.blocks[0].id, "tool-call-shell-42");
        assert_eq!(transcript.blocks[0].source_indices, vec![12, 77]);
        assert_eq!(transcript.blocks[1].id, "approval-shell-42");
        assert_eq!(transcript.blocks[1].kind, RenderBlockKind::ApprovalDecision);
    }

    #[test]
    fn from_records_pairs_tool_result_by_parent_id() {
        let tool_use = transcript_record(
            "toolu-stable-1",
            4,
            TranscriptRecordKind::ToolUse,
            Some("Bash"),
            r#"{"command":"cargo test"}"#,
        );
        let progress = transcript_record(
            "progress-1",
            5,
            TranscriptRecordKind::Progress,
            None,
            "running",
        );
        let mut result = transcript_record(
            "toolu-stable-1:result",
            6,
            TranscriptRecordKind::ToolResult,
            Some("Bash"),
            r#"{"stdout":"ok","exit_code":0}"#,
        );
        result.parent_id = Some("toolu-stable-1".to_string());
        let records = TranscriptRecords {
            entries: vec![tool_use, progress, result],
            approval_decisions: Vec::new(),
            final_summaries: Vec::new(),
        };

        let transcript = RenderTranscript::from_records(&records);

        assert_eq!(transcript.blocks.len(), 2);
        assert_eq!(transcript.blocks[0].id, "toolu-stable-1");
        assert_eq!(transcript.blocks[0].source_indices, vec![4, 6]);
        assert_eq!(
            transcript.blocks[0].tool.as_ref().map(|tool| tool.phase),
            Some(ToolPhase::Succeeded)
        );
        assert_eq!(transcript.blocks[1].kind, RenderBlockKind::Progress);
    }

    #[test]
    fn maps_tool_lifecycle_to_tool_card_phase() {
        let mut running = tool_message(MessageType::ToolUse, "Bash", "command  cargo test");
        running.is_streaming = true;
        let mut failed = tool_message(MessageType::ToolResult, "Bash", "stderr  error");
        failed.is_error = true;
        let messages = vec![
            running,
            tool_message(MessageType::ToolUse, "Read", "file_path Cargo.toml"),
            tool_message(MessageType::ToolResult, "Read", "content [workspace]"),
            failed,
        ];

        let transcript = RenderTranscript::from_messages(&messages);

        let phases: Vec<ToolPhase> = transcript
            .blocks
            .iter()
            .map(|block| block.tool.as_ref().unwrap().phase)
            .collect();
        assert_eq!(
            phases,
            vec![ToolPhase::Running, ToolPhase::Succeeded, ToolPhase::Failed]
        );
    }

    #[test]
    fn tool_cards_expose_engineering_product_families() {
        let messages = vec![
            tool_message(MessageType::ToolUse, "Bash", r#"{"command":"cargo test"}"#),
            tool_message(
                MessageType::ToolResult,
                "Edit",
                serde_json::json!({
                    "file_path": "src/lib.rs",
                    "old_string": "fn demo() {\n    old();\n}\n",
                    "new_string": "fn demo() {\n    new();\n}\n"
                })
                .to_string(),
            ),
        ];

        let transcript = RenderTranscript::from_messages(&messages);
        let command = transcript
            .blocks
            .iter()
            .filter_map(|block| block.tool.as_ref())
            .find(|tool| tool.family() == ToolFamily::Command)
            .expect("command card");
        let file_change = transcript
            .blocks
            .iter()
            .filter_map(|block| block.tool.as_ref())
            .find(|tool| tool.family() == ToolFamily::FileChange)
            .expect("file change card");

        assert_eq!(command.family(), ToolFamily::Command);
        assert_eq!(command.product_title(), "Bash · Command");
        assert_eq!(file_change.family(), ToolFamily::FileChange);
        assert_eq!(file_change.product_title(), "Edit · File Change");
    }

    #[test]
    fn file_change_results_emit_summary_before_diff_card() {
        let messages = vec![
            tool_message(
                MessageType::ToolResult,
                "Write",
                serde_json::json!({
                    "file_path": "src/new.rs",
                    "content": "one\ntwo\n"
                })
                .to_string(),
            ),
            tool_message(
                MessageType::ToolResult,
                "Edit",
                serde_json::json!({
                    "file_path": "src/lib.rs",
                    "old_string": "alpha\nbeta\n",
                    "new_string": "alpha\ngamma\n"
                })
                .to_string(),
            ),
        ];

        let transcript = RenderTranscript::from_messages(&messages);

        assert_eq!(
            transcript.blocks[0].kind,
            RenderBlockKind::FileChangeSummary
        );
        let RenderNode::FileChangeSummary(summary) = &transcript.blocks[0].nodes[0] else {
            panic!("first block should carry a file change summary node");
        };
        assert_eq!(summary.files.len(), 2);
        assert_eq!(summary.count_with_status("A"), 1);
        assert_eq!(summary.count_with_status("M"), 1);
        assert_eq!(summary.total_additions(), 3);
        assert_eq!(summary.total_deletions(), 1);
        assert_eq!(transcript.blocks[1].kind, RenderBlockKind::Tool);
    }

    #[test]
    fn render_timeline_collects_structured_event_rows() {
        let events = vec![
            RenderEvent::new(
                RenderEventKind::CommandStarted {
                    tool_id: Some("toolu-bash-1234567890".to_string()),
                    command: Some("cargo test -p mossen-tui timeline".to_string()),
                    cwd: Some("/Users/allen/Documents/rustmossen".to_string()),
                },
                RenderEventScope::Main,
                UiStage::RunningCommand,
            )
            .with_turn_id("turn-0001"),
            RenderEvent::new(
                RenderEventKind::CommandOutput {
                    tool_id: Some("toolu-bash-1234567890".to_string()),
                    stream: "stdout".to_string(),
                    bytes: 128,
                    preview_lines: 4,
                    hidden_lines: 12,
                    total_lines: Some(16),
                    full_log_available: true,
                },
                RenderEventScope::Main,
                UiStage::RunningCommand,
            )
            .with_turn_id("turn-0001"),
        ];

        let timeline = RenderTimelineRenderModel::from_events(&events);

        assert_eq!(timeline.summary.total_count, 2);
        assert_eq!(timeline.summary.turn_count, 1);
        assert_eq!(timeline.summary.immediate_count, 1);
        assert_eq!(timeline.summary.throttled_count, 1);
        assert_eq!(timeline.summary.freeze_history_count, 1);
        assert_eq!(timeline.summary.update_active_count, 1);
        assert_eq!(timeline.rows[0].event, "command_start");
        assert_eq!(timeline.rows[0].turn_id.as_deref(), Some("turn-0001"));
        assert_eq!(timeline.rows[0].stage, "running command");
        assert_eq!(timeline.rows[0].scope, "main");
        assert!(timeline.rows[0]
            .summary
            .contains("cargo test -p mossen-tui timeline"));
        assert!(timeline.rows[1].summary.contains("full log available"));
    }

    #[test]
    fn render_timeline_preserves_plan_progress_counts() {
        let events = vec![RenderEvent::new(
            RenderEventKind::PlanUpdated {
                tool_id: Some("toolu-plan-1234567890".to_string()),
                step_count: 4,
                completed_count: 1,
                active_count: 1,
                pending_count: 1,
                blocked_count: 1,
                active_step: Some("Verify timeline plan progress".to_string()),
            },
            RenderEventScope::Main,
            UiStage::Planning,
        )
        .with_turn_id("turn-0001")];

        let timeline = RenderTimelineRenderModel::from_events(&events);
        let row = &timeline.rows[0];

        assert_eq!(row.event, "plan_updated");
        assert!(row.summary.contains("4 step(s)"));
        assert!(row.summary.contains("1 done"));
        assert!(row.summary.contains("1 active"));
        assert!(row.summary.contains("1 pending"));
        assert!(row.summary.contains("1 blocked"));
        assert!(row
            .summary
            .contains("active: Verify timeline plan progress"));
        let detail = row.detail.as_deref().expect("plan row should have detail");
        assert!(detail.contains("4 step(s)"));
        assert!(detail.contains("1 blocked"));
        assert!(detail.contains("tool id: toolu-plan"));
    }

    #[test]
    fn edit_results_build_semantic_diff_sections() {
        let messages = vec![tool_message(
            MessageType::ToolResult,
            "Edit",
            serde_json::json!({
                "file_path": "src/lib.rs",
                "old_string": "alpha\nbeta\n",
                "new_string": "alpha\ngamma\n"
            })
            .to_string(),
        )];

        let transcript = RenderTranscript::from_messages(&messages);
        let tool = transcript
            .blocks
            .iter()
            .filter_map(|block| block.tool.as_ref())
            .find(|tool| tool.name == "Edit")
            .expect("Edit card");
        let diff = tool
            .sections
            .iter()
            .find(|section| section.title == "diff")
            .expect("Edit result should expose a semantic diff section");

        assert!(diff.body.contains("--- before"), "{diff:#?}");
        assert!(diff.body.contains("+++ after"), "{diff:#?}");
        assert!(diff.body.contains("- beta"), "{diff:#?}");
        assert!(diff.body.contains("+ gamma"), "{diff:#?}");
        assert_eq!(diff.kind, ToolSectionKind::Diff);
    }

    #[test]
    fn read_text_results_emit_metadata_and_code_content() {
        let messages = vec![tool_message(
            MessageType::ToolResult,
            "Read",
            serde_json::json!({
                "type": "text",
                "file_path": "src/lib.rs",
                "offset": 9,
                "limit": 2,
                "total_lines": 40,
                "content": "fn render() {}\nfn test() {}"
            })
            .to_string(),
        )];

        let transcript = RenderTranscript::from_messages(&messages);
        let tool = transcript.blocks[0].tool.as_ref().expect("Read card");
        let metadata = tool
            .sections
            .iter()
            .find(|section| section.title == "read")
            .expect("Read result should expose file/range metadata");
        let content = tool
            .sections
            .iter()
            .find(|section| section.title == "content")
            .expect("Read result should expose content section");

        assert_eq!(metadata.kind, ToolSectionKind::Metadata);
        assert!(metadata.body.contains("file: src/lib.rs"), "{metadata:#?}");
        assert!(
            metadata.body.contains("range: lines 10-11"),
            "{metadata:#?}"
        );
        assert!(metadata.body.contains("total: 40 lines"), "{metadata:#?}");
        assert_eq!(content.kind, ToolSectionKind::Output);
        assert!(content.body.contains("fn render() {}"), "{content:#?}");
        assert!(content.body.contains("fn test() {}"), "{content:#?}");
        assert!(!content.body.contains("10│"), "{content:#?}");
        let code = content
            .code
            .as_ref()
            .expect("Read content should carry code render metadata");
        assert_eq!(code.file_path.as_deref(), Some("src/lib.rs"));
        assert_eq!(code.start_line, 10);
        assert!(code.line_numbers);
        assert_eq!(code.hidden_lines, 29);
    }

    #[test]
    fn read_text_results_strip_existing_gutters_for_code_rendering() {
        let messages = vec![tool_message(
            MessageType::ToolResult,
            "Read",
            serde_json::json!({
                "type": "text",
                "file_path": "src/lib.rs",
                "total_lines": 12,
                "content": "    10│fn render() {}\n    11│fn test() {}"
            })
            .to_string(),
        )];

        let transcript = RenderTranscript::from_messages(&messages);
        let tool = transcript.blocks[0].tool.as_ref().expect("Read card");
        let content = tool
            .sections
            .iter()
            .find(|section| section.title == "content")
            .expect("Read result should expose content section");
        let code = content
            .code
            .as_ref()
            .expect("Read content should carry code render metadata");

        assert_eq!(content.body, "fn render() {}\nfn test() {}");
        assert_eq!(code.start_line, 10);
        assert_eq!(code.hidden_lines, 1);
    }

    #[test]
    fn read_binary_result_is_not_rendered_as_error_log() {
        let messages = vec![tool_message(
            MessageType::ToolResult,
            "Read",
            serde_json::json!({
                "type": "binary",
                "file_path": "target/app.bin",
                "size_bytes": 2048,
                "message": "File appears to be binary."
            })
            .to_string(),
        )];

        let transcript = RenderTranscript::from_messages(&messages);
        let tool = transcript.blocks[0].tool.as_ref().expect("Read card");
        let binary = tool
            .sections
            .iter()
            .find(|section| section.title == "binary")
            .expect("binary read result should expose a binary section");

        assert_eq!(binary.kind, ToolSectionKind::Output);
        assert!(binary.body.contains("target/app.bin"), "{binary:#?}");
        assert!(binary.body.contains("2048 bytes"), "{binary:#?}");
        assert!(
            !tool
                .sections
                .iter()
                .any(|section| section.kind == ToolSectionKind::Error),
            "{tool:#?}"
        );
    }

    #[test]
    fn search_json_results_emit_truncation_section() {
        let messages = vec![tool_message(
            MessageType::ToolResult,
            "Grep",
            serde_json::json!({
                "pattern": "ToolResult",
                "matches": [
                    {"path": "src/a.rs", "line": 10, "text": "ToolResult one"},
                    {"path": "src/b.rs", "line": 20, "text": "ToolResult two"}
                ],
                "total": 40,
                "limit": 2,
                "truncated": true,
                "message": "Results are truncated. Use a narrower pattern."
            })
            .to_string(),
        )];

        let transcript = RenderTranscript::from_messages(&messages);
        let tool = transcript.blocks[0].tool.as_ref().expect("Grep card");
        let truncation = tool
            .sections
            .iter()
            .find(|section| section.title == "truncation")
            .expect("truncated search result should expose metadata");

        assert!(
            tool.summary
                .as_deref()
                .is_some_and(|summary| summary.contains("upstream result truncated")),
            "{tool:#?}"
        );
        assert_eq!(truncation.kind, ToolSectionKind::Metadata);
        assert!(
            truncation.body.contains("shown: 2 matches of 40 matches"),
            "{truncation:#?}"
        );
        assert!(
            truncation.body.contains("limit: 2 matches"),
            "{truncation:#?}"
        );
        assert!(
            truncation.body.contains("Results are truncated"),
            "{truncation:#?}"
        );
    }

    #[test]
    fn merges_adjacent_tool_use_and_result_into_one_card() {
        let input = serde_json::json!({"command": "ls -la"}).to_string();
        let output = serde_json::json!({"stdout": "ok\n", "exit_code": 0}).to_string();
        let messages = vec![
            tool_message(MessageType::ToolUse, "Bash", input),
            tool_message(MessageType::ToolResult, "Bash", output),
        ];

        let transcript = RenderTranscript::from_messages(&messages);

        assert_eq!(transcript.blocks.len(), 1);
        let block = &transcript.blocks[0];
        assert_eq!(block.source_indices, vec![0, 1]);
        let tool = block.tool.as_ref().unwrap();
        assert_eq!(tool.phase, ToolPhase::Succeeded);
        assert!(tool
            .sections
            .iter()
            .any(|section| section.title == "input" && section.body.contains("command: ls -la")));
        assert!(tool
            .sections
            .iter()
            .any(|section| section.title == "stdout" && section.body == "ok\n"));
    }

    #[test]
    fn command_cards_expose_run_status_and_full_log_summary() {
        let preview = (1..=8)
            .map(|idx| format!("line {idx:03} output from a long command"))
            .collect::<Vec<_>>()
            .join("\n");
        let full = (1..=120)
            .map(|idx| format!("line {idx:03} output from a long command"))
            .collect::<Vec<_>>()
            .join("\n");
        let input = serde_json::json!({
            "command": "cargo test -p mossen-tui render_model",
            "cwd": "/Users/allen/Documents/rustmossen"
        })
        .to_string();
        let mut result = tool_message(
            MessageType::ToolResult,
            "Bash",
            serde_json::json!({
                "stdout": preview,
                "stdout_hidden_lines": 112,
                "stderr": "",
                "exit_code": 0,
                "duration_ms": 42
            })
            .to_string(),
        );
        result.full_content = Some(
            serde_json::json!({
                "stdout": full,
                "stderr": "",
                "exit_code": 0,
                "duration_ms": 42
            })
            .to_string(),
        );
        let messages = vec![tool_message(MessageType::ToolUse, "Bash", input), result];

        let transcript = RenderTranscript::from_messages(&messages);
        let tool = transcript.blocks[0].tool.as_ref().unwrap();
        let command = tool.command_run.as_ref().expect("command run model");

        assert_eq!(
            command.command.as_deref(),
            Some("cargo test -p mossen-tui render_model")
        );
        assert_eq!(
            command.cwd.as_deref(),
            Some("/Users/allen/Documents/rustmossen")
        );
        assert_eq!(command.status, CommandRunStatus::Succeeded);
        assert_eq!(command.exit_code, Some(0));
        assert_eq!(command.duration_ms, Some(42));
        assert_eq!(command.stdout.preview_line_count, 8);
        assert_eq!(command.stdout.hidden_line_count, 112);
        assert_eq!(command.stdout.total_line_count, Some(120));
        assert!(command.full_log_available);
        assert!(command.has_embedded_full_log());
        assert!(command
            .stdout
            .full_text
            .as_deref()
            .is_some_and(|text| text.contains("line 120 output from a long command")));
        assert!(transcript.blocks[0]
            .selector_summary()
            .contains("cargo test -p mossen-tui render_model"));
    }

    #[test]
    fn command_history_collects_semantic_command_runs() {
        let input = serde_json::json!({
            "command": "cargo test -p mossen-tui command_history",
            "cwd": "/Users/allen/Documents/rustmossen"
        })
        .to_string();
        let mut result = tool_message(
            MessageType::ToolResult,
            "Bash",
            serde_json::json!({
                "stdout": "running command history tests\nok\n",
                "stderr": "warning: existing warning noise\n",
                "stderr_hidden_lines": 2,
                "exit_code": 0,
                "duration_ms": 88
            })
            .to_string(),
        );
        result.full_content = Some(
            serde_json::json!({
                "stdout": "running command history tests\nok\nfull log tail\n",
                "stderr": "warning: existing warning noise\nmore warning\n",
                "exit_code": 0,
                "duration_ms": 88
            })
            .to_string(),
        );
        let transcript = RenderTranscript::from_messages(&[
            tool_message(MessageType::ToolUse, "Bash", input),
            result,
        ]);

        let history = command_history_from_transcript(&transcript);

        assert_eq!(history.summary.total_count, 1);
        assert_eq!(history.summary.failed_count, 0);
        assert_eq!(history.summary.full_log_count, 1);
        let row = &history.rows[0];
        assert_eq!(
            row.run.command.as_deref(),
            Some("cargo test -p mossen-tui command_history")
        );
        assert_eq!(row.run.status, CommandRunStatus::Succeeded);
        assert_eq!(row.run.duration_ms, Some(88));
        assert!(row
            .stdout_preview
            .as_deref()
            .is_some_and(|stdout| stdout.contains("running command history tests")));
        assert!(row
            .stderr_preview
            .as_deref()
            .is_some_and(|stderr| stderr.contains("existing warning noise")));
        assert!(row.run.has_embedded_full_log());
        assert!(row
            .run
            .stdout
            .full_text
            .as_deref()
            .is_some_and(|stdout| stdout.contains("full log tail")));
    }

    #[test]
    fn command_summaries_are_derived_from_semantic_transcript_runs() {
        let completed_input = serde_json::json!({
            "command": "cargo check -p mossen-tui",
            "cwd": "/Users/allen/Documents/rustmossen"
        })
        .to_string();
        let completed_result = tool_message(
            MessageType::ToolResult,
            "Bash",
            serde_json::json!({
                "stdout": "finished dev profile\n",
                "exit_code": 0,
                "duration_ms": 144
            })
            .to_string(),
        );
        let pending_input = serde_json::json!({
            "command": "cargo test -p mossen-tui render_model::tests::pending",
            "cwd": "/Users/allen/Documents/rustmossen"
        })
        .to_string();

        let summaries = command_summaries_from_messages(&[
            tool_message(MessageType::ToolUse, "Bash", completed_input),
            completed_result,
            tool_message(MessageType::ToolUse, "Bash", pending_input),
        ]);

        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0].command, "cargo check -p mossen-tui");
        assert_eq!(
            summaries[0].cwd.as_deref(),
            Some("/Users/allen/Documents/rustmossen")
        );
        assert_eq!(summaries[0].exit_code, Some(0));
        assert_eq!(summaries[0].duration_ms, Some(144));
        assert_eq!(summaries[0].status, "passed");
        assert_eq!(
            summaries[1].command,
            "cargo test -p mossen-tui render_model::tests::pending"
        );
        assert_eq!(summaries[1].status, "started");
        assert_eq!(summaries[1].exit_code, None);
    }

    #[test]
    fn tool_input_summary_formats_structured_values_semantically() {
        assert_eq!(
            tool_input_summary_from_value(&serde_json::json!({"command": "echo hi"})),
            "echo hi"
        );

        let summary = tool_input_summary_from_value(&serde_json::json!({
            "file_path": "/tmp/a.txt",
            "limit": 100
        }));
        assert!(summary.contains("file_path=/tmp/a.txt"));
        assert!(summary.contains("limit=100"));

        assert_eq!(tool_input_summary_from_value(&serde_json::json!({})), "");
        assert_eq!(tool_input_summary_from_value(&serde_json::json!(null)), "");

        let long = "x".repeat(500);
        let truncated = tool_input_summary_from_value(&serde_json::json!({"command": long}));
        assert!(truncated.ends_with('…'));
        assert!(truncated.chars().count() <= 241);
    }

    #[test]
    fn compact_plan_preview_formats_messages_without_raw_tool_json() {
        let messages = vec![
            engine_message(
                Role::User,
                vec![ContentBlock::Text(TextBlock {
                    text: "hello\n\nworld".to_string(),
                })],
            ),
            engine_message(
                Role::Assistant,
                vec![
                    ContentBlock::ToolUse(ToolUseBlock {
                        id: "toolu-1".to_string(),
                        name: "Bash".to_string(),
                        input: serde_json::json!({"command": "cargo test -q"}),
                    }),
                    ContentBlock::ToolResult(ToolResultBlock {
                        tool_use_id: "toolu-1".to_string(),
                        content: ToolResultContent::Text("ok\nnext".to_string()),
                        is_error: None,
                    }),
                ],
            ),
        ];

        let preview = compact_plan_summary_preview_from_messages(&messages);

        assert!(preview.contains("2 message(s) were compacted"));
        assert!(preview.contains("1. user: hello world"));
        assert!(preview.contains("2. assistant: tool_use Bash cargo test -q"));
        assert!(preview.contains("tool_result toolu-1 ok next"));
        assert!(!preview.contains("{\"command\""));
    }

    #[test]
    fn compact_plan_model_formats_dry_run_body() {
        let messages = vec![
            engine_message(
                Role::User,
                vec![ContentBlock::Text(TextBlock {
                    text: "one".to_string(),
                })],
            ),
            engine_message(
                Role::Assistant,
                vec![ContentBlock::Text(TextBlock {
                    text: "two".to_string(),
                })],
            ),
            engine_message(
                Role::User,
                vec![ContentBlock::Text(TextBlock {
                    text: "three".to_string(),
                })],
            ),
            engine_message(
                Role::Assistant,
                vec![ContentBlock::Text(TextBlock {
                    text: "four".to_string(),
                })],
            ),
        ];

        let model = compact_plan_render_model(
            &messages,
            false,
            true,
            Some("keep permission decisions".to_string()),
        );
        let body = compact_plan_body_from_model(&model);

        assert_eq!(model.before_messages, 4);
        assert_eq!(model.compacted_messages, 2);
        assert_eq!(model.recent_messages, 2);
        assert_eq!(model.after_messages, 3);
        assert!(body.contains("Compact plan"));
        assert!(body.contains("state: idle"), "{body}");
        assert!(body.contains("messages: 4 -> 3"), "{body}");
        assert!(body.contains("compacted messages: 2"), "{body}");
        assert!(body.contains("hooks: configured"), "{body}");
        assert!(
            body.contains("custom instructions: keep permission decisions"),
            "{body}"
        );
        assert!(body.contains("Preview only"), "{body}");
    }

    #[test]
    fn compact_status_model_formats_lifecycle_body() {
        let body = compact_status_body_from_model(&CompactStatusRenderModel {
            is_running: true,
            task_id: Some(7),
            pending_launch: true,
            cancellable: false,
            hooks_configured: false,
            progress: Some("Compacting conversation history...".to_string()),
        });

        assert!(body.contains("Compact status"));
        assert!(body.contains("state: running"), "{body}");
        assert!(body.contains("task: 7"), "{body}");
        assert!(body.contains("pending launch: yes"), "{body}");
        assert!(body.contains("cancellable: no"), "{body}");
        assert!(body.contains("hooks: not configured"), "{body}");
        assert!(
            body.contains("progress: Compacting conversation history..."),
            "{body}"
        );
        assert!(body.contains("hint: /compact cancel"), "{body}");
    }

    #[test]
    fn permission_mode_choices_normalize_labels_and_codes() {
        let choices = permission_mode_choices();

        assert_eq!(
            choices
                .iter()
                .map(|choice| (choice.label, choice.code))
                .collect::<Vec<_>>(),
            vec![
                ("Supervised", "default"),
                ("Plan", "plan"),
                ("Accept Edits", "acceptEdits"),
                ("Full Auto", "bypassPermissions"),
                ("Don't Ask", "dontAsk"),
            ]
        );
        assert_eq!(permission_mode_display_label(None), "Supervised");
        assert_eq!(permission_mode_display_label(Some("default")), "Supervised");
        assert_eq!(
            permission_mode_display_label(Some("Accept Edits")),
            "Accept Edits"
        );
        assert_eq!(
            permission_mode_display_label(Some("bypassPermissions")),
            "Full Auto"
        );
        assert_eq!(permission_mode_choice_index(Some("dontAsk")), 4);
        assert_eq!(
            permission_mode_code_for_choice("Full Auto"),
            Some("bypassPermissions")
        );
        assert_eq!(
            permission_mode_code_for_choice("acceptEdits"),
            Some("acceptEdits")
        );
        assert_eq!(
            permission_mode_code_for_choice("full-auto"),
            Some("bypassPermissions")
        );
        assert_eq!(permission_mode_code_for_choice("dont ask"), Some("dontAsk"));
        assert_eq!(
            permission_mode_code_for_choice("don't-ask"),
            Some("dontAsk")
        );
        assert_eq!(permission_mode_code_for_raw(Some("unknown")), "default");
    }

    #[test]
    fn tool_call_preview_formats_known_tool_inputs_without_root_helpers() {
        let bash = tool_call_preview_from_input(
            "Bash",
            &serde_json::json!({"command": "cargo test -p mossen-tui"}),
        );
        assert!(bash.contains("command"));
        assert!(bash.contains("cargo test -p mossen-tui"));

        let edit = tool_call_preview_from_input(
            "Edit",
            &serde_json::json!({
                "file_path": "src/lib.rs",
                "old_string": "old\n",
                "new_string": "new\n",
                "replace_all": true
            }),
        );
        assert!(edit.contains("path  src/lib.rs"));
        assert!(edit.contains("replace_all true"));
        assert!(edit.contains("--- old"));
        assert!(edit.contains("+++ new"));
        assert!(edit.contains("- old"));
        assert!(edit.contains("+ new"));
    }

    #[test]
    fn error_history_collects_error_blocks_and_failed_commands() {
        let mut error_message = message(
            MessageType::System,
            "Build failed\nerror[E0425]: cannot find value `missing`\nretry scheduled",
        );
        error_message.is_error = true;
        let input = serde_json::json!({
            "command": "cargo test -p mossen-tui error_history",
            "cwd": "/Users/allen/Documents/rustmossen"
        })
        .to_string();
        let result = tool_message(
            MessageType::ToolResult,
            "Bash",
            serde_json::json!({
                "stdout": "running tests\n",
                "stderr": "thread 'render' panicked\nassertion failed\n",
                "stderr_hidden_lines": 4,
                "exit_code": 1,
                "duration_ms": 42,
                "error": "tests failed"
            })
            .to_string(),
        );
        let transcript = RenderTranscript::from_messages(&[
            error_message,
            tool_message(MessageType::ToolUse, "Bash", input),
            result,
        ]);

        let history = error_history_from_transcript(&transcript);

        assert_eq!(history.summary.total_count, 2);
        assert_eq!(history.summary.command_failure_count, 1);
        assert_eq!(history.summary.hidden_detail_count, 4);
        assert!(history.rows.iter().any(|row| {
            row.title == "Error"
                && row.summary == "Build failed"
                && row
                    .key_detail
                    .as_deref()
                    .is_some_and(|detail| detail.contains("cannot find value"))
        }));
        assert!(history.rows.iter().any(|row| {
            row.command_failure
                && row.title == "Command failed"
                && row.summary.contains("tests failed")
                && row.details.as_deref().is_some_and(|details| {
                    details.contains("cargo test -p mossen-tui error_history")
                        && details.contains("thread 'render' panicked")
                })
        }));
    }

    #[test]
    fn final_summary_history_collects_structured_final_summaries() {
        let summary = FinalSummaryModel {
            id: "summary-history-1".to_string(),
            success: false,
            terminal: "Completed with residual risk".to_string(),
            changed_files: vec![FileChangeSummaryModel {
                path: "crates/mossen-tui/src/app.rs".to_string(),
                status: "M".to_string(),
                additions: 8,
                deletions: 2,
            }],
            commands: vec![CommandSummaryModel {
                command: "cargo test -p mossen-tui render_model".to_string(),
                cwd: Some("/Users/allen/Documents/rustmossen".to_string()),
                exit_code: Some(0),
                duration_ms: Some(1200),
                status: "passed".to_string(),
            }],
            verification_results: vec![VerificationSummaryModel {
                command: "cargo check -p mossen-tui".to_string(),
                status: "passed".to_string(),
                passed: true,
                exit_code: Some(0),
                duration_ms: Some(900),
            }],
            residual_risks: vec!["Snapshot review remains manual".to_string()],
            notes: vec!["Task execution code was untouched".to_string()],
        };
        let transcript = RenderTranscript::from_messages(&[message(
            MessageType::System,
            final_summary_message_content(&summary),
        )]);

        let history = final_summary_history_from_transcript(&transcript);

        assert_eq!(history.summary.total_count, 1);
        assert_eq!(history.summary.completed_count, 0);
        assert_eq!(history.summary.attention_count, 1);
        assert_eq!(history.summary.changed_file_count, 1);
        assert_eq!(history.summary.command_count, 1);
        assert_eq!(history.summary.verification_count, 1);
        assert_eq!(history.summary.risk_count, 1);
        let row = &history.rows[0];
        assert_eq!(row.id, "summary-history-1");
        assert_eq!(row.status_label(), "Needs attention");
        assert_eq!(row.terminal, "Completed with residual risk");
        assert_eq!(row.changed_files[0].path, "crates/mossen-tui/src/app.rs");
        assert_eq!(
            row.commands[0].command,
            "cargo test -p mossen-tui render_model"
        );
        assert_eq!(
            row.verification_results[0].command,
            "cargo check -p mossen-tui"
        );
        assert_eq!(row.residual_risks[0], "Snapshot review remains manual");
        assert_eq!(row.notes[0], "Task execution code was untouched");
        assert_eq!(row.source_block_id.as_deref(), Some("summary-history-1"));
    }

    #[test]
    fn command_cards_expose_timeout_interruption_and_error_summary() {
        let mut result = tool_message(
            MessageType::ToolResult,
            "Bash",
            serde_json::json!({
                "command": "cargo test -- --nocapture",
                "stderr": "test timed out\n",
                "exit_code": 124,
                "timed_out": true,
                "interrupted": true,
                "signal": "SIGTERM",
                "duration_ms": 30000,
                "error": "command exceeded timeout"
            })
            .to_string(),
        );
        result.is_error = true;

        let transcript = RenderTranscript::from_messages(&[result]);
        let tool = transcript.blocks[0].tool.as_ref().unwrap();
        let command = tool.command_run.as_ref().expect("command run model");

        assert_eq!(command.status, CommandRunStatus::Failed);
        assert_eq!(command.exit_code, Some(124));
        assert!(command.timed_out);
        assert!(command.interrupted);
        assert_eq!(command.signal.as_deref(), Some("SIGTERM"));
        assert_eq!(
            command.error_summary.as_deref(),
            Some("command exceeded timeout")
        );

        let status = command.status_line();
        assert!(status.contains("Failed"), "{status}");
        assert!(status.contains("timeout"), "{status}");
        assert!(status.contains("interrupted"), "{status}");
        assert!(status.contains("signal SIGTERM"), "{status}");
        assert!(
            status.contains("error command exceeded timeout"),
            "{status}"
        );

        let status_section = tool
            .sections
            .iter()
            .find(|section| section.title == "status")
            .expect("command failure metadata should be visible semantically");
        assert!(status_section.body.contains("timeout: true"));
        assert!(status_section.body.contains("interrupted: true"));
        assert!(status_section.body.contains("signal: SIGTERM"));
    }

    #[test]
    fn error_blocks_extract_key_detail_and_hidden_log_count() {
        let mut error = message(
            MessageType::CommandOutput,
            concat!(
                "Build failed\n",
                "checking workspace\n",
                "thread 'main' panicked at src/main.rs:10\n",
                "detail line 01\n",
                "detail line 02\n",
                "detail line 03\n",
                "detail line 04\n",
                "detail line 05\n",
                "detail line 06\n",
                "detail line 07\n",
                "detail line 08\n",
                "detail line 09\n",
            ),
        );
        error.is_error = true;

        let transcript = RenderTranscript::from_messages(&[error]);
        let model = transcript.blocks[0]
            .nodes
            .iter()
            .find_map(|node| match node {
                RenderNode::Error(error) => Some(error),
                _ => None,
            })
            .expect("error node should be layered");

        assert_eq!(transcript.blocks[0].kind, RenderBlockKind::Error);
        assert_eq!(model.title, "Command error");
        assert_eq!(model.summary, "Build failed");
        assert_eq!(
            model.key_detail.as_deref(),
            Some("thread 'main' panicked at src/main.rs:10")
        );
        assert_eq!(model.detail_hidden_line_count, 3);
        assert!(!model.retrying);
    }

    #[test]
    fn hides_protocol_only_messages() {
        let messages = vec![
            message(MessageType::Assistant, "(no content - terminal=Completed)"),
            message(MessageType::Assistant, "(stop: tool_use)"),
            message(MessageType::Assistant, "最终结果"),
        ];

        let transcript = RenderTranscript::from_messages(&messages);

        assert_eq!(transcript.blocks.len(), 1);
        assert_eq!(
            transcript.blocks[0].nodes,
            vec![RenderNode::Markdown("最终结果".to_string())]
        );
    }

    #[test]
    fn hides_protocol_only_messages_with_variant_stop_lines() {
        let messages = vec![
            message(
                MessageType::Assistant,
                "  (no content - terminal=Completed)\n\n... (stop: tool_use)  ",
            ),
            message(MessageType::Assistant, "terminal=Completed\nstop: tool_use"),
            message(MessageType::Assistant, "最终结果"),
        ];

        let transcript = RenderTranscript::from_messages(&messages);

        assert_eq!(transcript.blocks.len(), 1);
        assert_eq!(
            transcript.blocks[0].nodes,
            vec![RenderNode::Markdown("最终结果".to_string())]
        );
    }

    #[test]
    fn keeps_tool_invocation_with_null_input_visible() {
        let messages = vec![tool_message(MessageType::ToolUse, "Glob", "null")];

        let transcript = RenderTranscript::from_messages(&messages);

        assert_eq!(transcript.blocks.len(), 1);
        assert_eq!(transcript.blocks[0].kind, RenderBlockKind::Tool);
        assert_eq!(
            transcript.blocks[0]
                .tool
                .as_ref()
                .unwrap()
                .summary
                .as_deref(),
            Some("(no input)")
        );
    }

    #[test]
    fn preserves_multibyte_content_in_model_conversion() {
        let messages = vec![message(
            MessageType::Assistant,
            "读取项目：渲染、审批、工具卡、代码块。",
        )];

        let transcript = RenderTranscript::from_messages(&messages);

        assert_eq!(
            transcript.blocks[0].nodes,
            vec![RenderNode::Markdown(
                "读取项目：渲染、审批、工具卡、代码块。".to_string()
            )]
        );
    }

    #[test]
    fn strips_ansi_from_all_visible_semantic_text() {
        let mut assistant = message(
            MessageType::Assistant,
            "\u{1b}[31m## 红色标题\u{1b}[0m\t正文",
        );
        assistant.thinking = Some("\u{1b}[32mthinking\u{1b}[0m\tstep".to_string());
        let messages = vec![
            assistant,
            message(MessageType::User, "\u{1b}[31m用户输入\u{1b}[0m"),
        ];

        let transcript = RenderTranscript::from_messages(&messages);
        let debug = format!("{transcript:#?}");

        assert!(!debug.contains('\u{1b}'), "{debug}");
        assert!(debug.contains("## 红色标题    正文"), "{debug}");
        assert!(debug.contains("thinking    step"), "{debug}");
        assert!(debug.contains("用户输入"), "{debug}");
    }

    #[test]
    fn render_surface_promotes_approval_to_blocking_footer_state() {
        let transcript = RenderTranscript::from_messages(&[message(MessageType::User, "跑测试")]);
        let footer = FooterRenderModel {
            project: Some("/Users/allen/Documents/rustmossen".to_string()),
            model: Some("MiniMax-M2.7".to_string()),
            access_mode: Some("Supervised".to_string()),
            turn_state: Some("streaming".to_string()),
            message_count: Some(1),
            ..FooterRenderModel::default()
        };
        let approval = ApprovalRenderModel {
            id: "approval-1".to_string(),
            tool_name: "Bash".to_string(),
            title: "Shell Command".to_string(),
            detail_label: "Command".to_string(),
            detail: "cargo test".to_string(),
            risk: ApprovalRiskLevel::Medium,
            body: "Command requires shell execution.".to_string(),
            actions: vec![
                ApprovalAction::Allow,
                ApprovalAction::AlwaysAllow,
                ApprovalAction::Deny,
            ],
            selected_action: ApprovalAction::AlwaysAllow,
            anchor_block_id: Some("tool-0-1".to_string()),
            expanded: true,
        };

        let surface = RenderSurface::new(transcript, footer).with_approval(approval);

        assert!(surface.is_blocked());
        assert_eq!(surface.approvals.len(), 1);
        assert_eq!(
            surface.blocking.as_ref().unwrap().kind,
            BlockingKind::Approval
        );
        assert_eq!(
            surface.footer.blocking.as_ref().unwrap().detail,
            "Risk: Medium · Command: cargo test"
        );
    }

    #[test]
    fn normalizes_bash_result_json_into_semantic_sections() {
        let content = serde_json::json!({
            "command": "cargo test -p mossen-tui render_model",
            "cwd": "/Users/allen/Documents/rustmossen",
            "stdout": "ok\n",
            "stderr": "",
            "exit_code": 0,
            "duration_ms": 42
        })
        .to_string();
        let messages = vec![tool_message(MessageType::ToolResult, "Bash", content)];

        let transcript = RenderTranscript::from_messages(&messages);
        let tool = transcript.blocks[0].tool.as_ref().unwrap();

        assert_eq!(tool.phase, ToolPhase::Succeeded);
        assert_eq!(tool.summary.as_deref(), Some("exit 0 | duration 42ms"));
        assert!(tool.sections.iter().any(|section| section.title == "stdout"
            && section.body == "ok\n"
            && section.kind == ToolSectionKind::Output));
        assert!(tool
            .sections
            .iter()
            .all(|section| !section.body.contains("\"stdout\"")));
    }

    #[test]
    fn strips_ansi_before_tool_content_reaches_semantic_model() {
        let content = serde_json::json!({
            "stdout": "\u{1b}[31mred output\u{1b}[0m\taligned\n",
            "stderr": "",
            "exit_code": 0
        })
        .to_string();
        let messages = vec![tool_message(MessageType::ToolResult, "Bash", content)];

        let transcript = RenderTranscript::from_messages(&messages);
        let tool = transcript.blocks[0].tool.as_ref().unwrap();
        let stdout = tool
            .sections
            .iter()
            .find(|section| section.title == "stdout")
            .expect("stdout section should be present");

        assert_eq!(stdout.body, "red output    aligned\n");
        assert!(!stdout.body.contains('\u{1b}'));
    }

    #[test]
    fn normalizes_tool_input_json_without_raw_payload() {
        let content = serde_json::json!({
            "command": "ls -la",
            "cwd": "/Users/allen/Documents/rustmossen"
        })
        .to_string();
        let messages = vec![tool_message(MessageType::ToolUse, "Bash", content)];

        let transcript = RenderTranscript::from_messages(&messages);
        let tool = transcript.blocks[0].tool.as_ref().unwrap();

        assert_eq!(tool.phase, ToolPhase::Requested);
        assert_eq!(tool.sections.len(), 1);
        assert_eq!(tool.sections[0].title, "input");
        assert!(tool.sections[0].body.contains("command: ls -la"));
        assert!(tool.sections[0]
            .body
            .contains("cwd: /Users/allen/Documents/rustmossen"));
        assert!(!tool.sections[0].body.contains("\"command\""));
    }

    #[test]
    fn malformed_known_tool_json_is_hidden_from_normal_transcript() {
        let messages = vec![tool_message(
            MessageType::ToolResult,
            "Bash",
            "{\"stdout\":\"逐行读代码",
        )];

        let transcript = RenderTranscript::from_messages(&messages);
        let tool = transcript.blocks[0].tool.as_ref().unwrap();

        assert_eq!(tool.summary.as_deref(), Some("malformed output payload"));
        assert_eq!(tool.sections[0].title, "malformed output");
        assert_eq!(tool.sections[0].kind, ToolSectionKind::Error);
        assert!(!tool.sections[0].body.contains("\"stdout\""));
        assert!(tool.sections[0].body.contains("raw payload is hidden"));
    }

    #[test]
    fn generic_json_tool_payload_redacts_sensitive_fields() {
        let content = serde_json::json!({
            "status": "ok",
            "api_key": "raw-api-secret",
            "nested": {
                "secret_token": "raw-nested-secret",
                "visible": "kept"
            },
            "items": [
                {
                    "password": "raw-array-secret",
                    "name": "visible-item"
                }
            ],
            "token_count": 1234
        })
        .to_string();
        let messages = vec![tool_message(
            MessageType::ToolResult,
            "ThirdPartyTool",
            content,
        )];

        let transcript = RenderTranscript::from_messages(&messages);
        let tool = transcript.blocks[0].tool.as_ref().unwrap();
        let body = tool
            .sections
            .iter()
            .map(|section| section.body.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        assert_eq!(tool.summary.as_deref(), Some("status ok"));
        assert!(body.contains("api_key: redacted"));
        assert!(body.contains("secret_token=redacted"));
        assert!(body.contains("password=redacted"));
        assert!(body.contains("token_count: 1234"));
        for forbidden in ["raw-api-secret", "raw-nested-secret", "raw-array-secret"] {
            assert!(
                !body.contains(forbidden),
                "generic JSON renderer leaked sensitive field {forbidden:?}\n{body}"
            );
        }
    }

    #[test]
    fn arbitrary_json_tool_payloads_scrub_controls_and_redact_token_secrets() {
        let content = serde_json::json!({
            "status": "ok\u{7}still-ok",
            "authorization_header": "Bearer raw-auth-secret",
            "session_token": "raw-session-token",
            "private_key": "raw-private-key",
            "nested": {
                "visible": "plain\u{8}text",
                "osc": "\u{1b}]2;raw title\u{7}safe tail"
            },
            "items": [
                {
                    "accessToken": "raw-access-token",
                    "name": "\u{1b}[31mvisible-item\u{1b}[0m"
                }
            ],
            "token_count": 1234,
            "total_tokens": 5678
        })
        .to_string();
        let messages = vec![tool_message(
            MessageType::ToolResult,
            "ThirdPartyTool",
            content,
        )];

        let transcript = RenderTranscript::from_messages(&messages);
        let tool = transcript.blocks[0].tool.as_ref().unwrap();
        let rendered = std::iter::once(tool.summary.as_deref().unwrap_or_default())
            .chain(tool.sections.iter().map(|section| section.body.as_str()))
            .collect::<Vec<_>>()
            .join("\n");

        assert!(rendered.contains("authorization_header: redacted"));
        assert!(rendered.contains("session_token: redacted"));
        assert!(rendered.contains("private_key: redacted"));
        assert!(rendered.contains("accessToken=redacted"));
        assert!(rendered.contains("token_count: 1234"));
        assert!(rendered.contains("total_tokens: 5678"));
        assert!(rendered.contains("visible-item"));
        for forbidden in [
            "raw-auth-secret",
            "raw-session-token",
            "raw-private-key",
            "raw-access-token",
            "\u{1b}",
            "\u{7}",
            "\u{8}",
            "[31m",
            "[0m",
        ] {
            assert!(
                !rendered.contains(forbidden),
                "arbitrary JSON tool renderer leaked {forbidden:?}\n{rendered}"
            );
        }
    }

    #[test]
    fn normalizes_todo_result_json() {
        let content = serde_json::json!({
            "new_todos": [
                {"status": "completed", "content": "建立 RenderModel"},
                {"status": "in_progress", "content": "迁移工具卡"}
            ]
        })
        .to_string();
        let messages = vec![tool_message(MessageType::ToolResult, "TodoWrite", content)];

        let transcript = RenderTranscript::from_messages(&messages);
        let tool = transcript.blocks[0].tool.as_ref().unwrap();

        assert_eq!(tool.summary.as_deref(), Some("2 todos"));
        assert_eq!(tool.sections[0].title, "todos");
        assert!(tool.sections[0]
            .body
            .contains("completed: 建立 RenderModel"));
        assert!(tool.sections[0].body.contains("in_progress: 迁移工具卡"));
        let plan = tool.plan.as_ref().expect("TodoWrite should expose a plan");
        assert_eq!(plan.summary_line(), "2 steps · 1 done · 1 active");
        assert_eq!(plan.steps[0].status, PlanStepStatus::Completed);
        assert_eq!(plan.steps[0].content, "建立 RenderModel");
        assert_eq!(plan.steps[1].status, PlanStepStatus::InProgress);
        assert_eq!(
            plan.active_step().map(|step| step.content.as_str()),
            Some("迁移工具卡")
        );
    }

    #[test]
    fn normalizes_workitem_task_tool_results() {
        let create = serde_json::json!({
            "task": {
                "id": "task-1",
                "subject": "补齐终端渲染"
            }
        })
        .to_string();
        let transcript = RenderTranscript::from_messages(&[tool_message(
            MessageType::ToolResult,
            "TaskCreate",
            create,
        )]);
        let tool = transcript.blocks[0].tool.as_ref().unwrap();
        assert_eq!(
            tool.summary.as_deref(),
            Some("task task-1 | subject 补齐终端渲染")
        );
        assert!(tool.sections[0].body.contains("id: task-1"));
        assert!(!tool.sections[0].body.contains("\"task\""));

        let list = serde_json::json!({
            "tasks": [
                {"id": "task-1", "subject": "补齐终端渲染", "status": "in_progress", "blockedBy": []},
                {"id": "task-2", "subject": "跑回归", "status": "pending", "owner": "allen"}
            ]
        })
        .to_string();
        let transcript = RenderTranscript::from_messages(&[tool_message(
            MessageType::ToolResult,
            "TaskList",
            list,
        )]);
        let tool = transcript.blocks[0].tool.as_ref().unwrap();
        assert_eq!(tool.summary.as_deref(), Some("2 tasks"));
        let tasks = tool
            .sections
            .iter()
            .find(|section| section.title == "tasks")
            .expect("TaskList should expose task rows");
        assert!(
            tasks.body.contains("in_progress: task-1 - 补齐终端渲染"),
            "{tasks:#?}"
        );
        assert!(tasks.body.contains("owner allen"), "{tasks:#?}");
        assert!(!tasks.body.contains("\"tasks\""));

        let output = serde_json::json!({
            "retrieval_status": "ready",
            "task": {
                "task_id": "task-bg-1",
                "task_type": "agent",
                "status": "completed",
                "description": "检查渲染状态",
                "output": "完成检查\n无阻塞",
                "exit_code": 0
            }
        })
        .to_string();
        let transcript = RenderTranscript::from_messages(&[tool_message(
            MessageType::ToolResult,
            "TaskOutput",
            output,
        )]);
        let tool = transcript.blocks[0].tool.as_ref().unwrap();
        let summary = tool.summary.as_deref().unwrap_or_default();
        assert!(summary.contains("retrieval ready"), "{summary}");
        assert!(summary.contains("task task-bg-1"), "{summary}");
        let output = tool
            .sections
            .iter()
            .find(|section| section.title == "output")
            .expect("TaskOutput should expose child output");
        assert!(output.body.contains("完成检查"), "{output:#?}");
    }

    #[test]
    fn normalizes_agent_result_json() {
        let content = serde_json::json!({
            "status": "completed",
            "agent_id": "agent-render-1",
            "result": "完成项目分析"
        })
        .to_string();
        let messages = vec![tool_message(MessageType::ToolResult, "Agent", content)];

        let transcript = RenderTranscript::from_messages(&messages);
        let tool = transcript.blocks[0].tool.as_ref().unwrap();

        assert_eq!(
            tool.summary.as_deref(),
            Some("status completed | agent agent-render-1")
        );
        let result = tool
            .sections
            .iter()
            .find(|section| section.title == "result")
            .expect("Agent result should be rendered semantically");
        assert_eq!(result.body, "完成项目分析");
    }

    #[test]
    fn normalizes_agent_result_with_nested_tool_summary() {
        let content = serde_json::json!({
            "agent_type": "render-review",
            "task_id": "task-render-7",
            "total_tool_use_count": 3,
            "total_token_count": 1234,
            "total_duration_ms": 5678,
            "stopped_reason": "EndTurn",
            "last_tool_use_name": "Read",
            "result_text": "完成子代理检查",
            "messages": [
                {
                    "type": "assistant",
                    "content": [
                        {"type": "tool_use", "name": "Grep", "input": {"pattern": "RenderModel"}},
                        {"type": "tool_use", "name": "Read", "input": {"file_path": "src/render_model.rs"}}
                    ]
                }
            ],
            "pending_approval": {
                "tool": "Bash",
                "command": "cargo test -p mossen-tui"
            }
        })
        .to_string();
        let messages = vec![tool_message(MessageType::ToolResult, "Task", content)];

        let transcript = RenderTranscript::from_messages(&messages);
        let tool = transcript.blocks[0].tool.as_ref().unwrap();

        let summary = tool.summary.as_deref().unwrap_or_default();
        assert!(summary.contains("agent task-render-7"), "{summary}");
        assert!(summary.contains("type render-review"), "{summary}");
        assert!(summary.contains("3 nested tool calls"), "{summary}");
        assert!(summary.contains("last tool Read"), "{summary}");

        let nested = tool
            .sections
            .iter()
            .find(|section| section.title == "nested tools")
            .expect("Agent result should expose nested tool activity");
        assert!(nested.body.contains("total: 3 tool calls"), "{nested:#?}");
        assert!(
            nested.body.contains("recent tools: Grep, Read"),
            "{nested:#?}"
        );

        let approval = tool
            .sections
            .iter()
            .find(|section| section.title == "nested approval")
            .expect("Agent result should expose nested approval activity");
        assert!(approval.body.contains("tool: Bash"), "{approval:#?}");
        assert!(
            approval.body.contains("cargo test -p mossen-tui"),
            "{approval:#?}"
        );

        let result = tool
            .sections
            .iter()
            .find(|section| section.title == "result")
            .expect("Agent result should expose final output");
        assert_eq!(result.body, "完成子代理检查");
    }
}
