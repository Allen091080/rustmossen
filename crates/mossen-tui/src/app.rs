//! App framework — main TUI application struct and event loop.
//!
//! Main ratatui App struct and REPL lifecycle.
//!
//! This is the wired-up version: keyboard Enter dispatches into
//! `mossen_agent::engine::submit_prompt`, the `SdkMessage` stream is consumed
//! from `engine_rx`, slash commands route to the `mossen_commands` directive
//! registry, and modal overlays (permission prompts, tool-use confirms) are
//! drawn on top of the main UI when active.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeSet, HashMap};
use std::future::Future;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::mpsc as std_mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::{KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::widgets::{Clear, Scrollbar, ScrollbarOrientation, ScrollbarState};
use ratatui::{layout::Rect, style::Style, widgets::Widget, Frame};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

use crate::approval_state::{
    PermissionAction, PermissionKind, PermissionPromptState, ToolUseConfirm,
};
use crate::event::{
    spawn_crossterm_reader, spawn_tick_timer, AppEvent, EventBus, InputAction,
    DEFAULT_EVENT_BATCH_LIMIT,
};
use crate::layout::{split_auxiliary_panels, AppLayout, VirtualScroll};
use crate::message_model::{display_tool_name, MessageData, MessageType};
use crate::render_cache::RenderHeightCache;
use crate::render_events::{
    render_events_for_sdk_message, RenderEvent, RenderEventKind, RenderRefreshPolicy,
    STREAM_THROTTLE_MS,
};
use crate::render_glyphs::{RenderGlyphMode, RenderGlyphs};
use crate::render_lifecycle::{
    api_retry_transcript_message, assistant_content_facts, assistant_transcript_message,
    cancelled_transcript_message, command_output_transcript_message,
    compact_boundary_transcript_facts, exceptional_stop_reason_transcript_message,
    final_summary_transcript_message, finalize_pending_assistant_transcript_message,
    pending_assistant_transcript_message, skill_invocation_transcript_message,
    system_transcript_message, task_assistant_transcript_facts, task_completed_transcript_facts,
    task_record_id, task_started_transcript_facts, tool_summary_transcript_facts,
    tool_use_transcript_facts, unknown_command_transcript_message, user_transcript_message,
    ApprovalDecisionKind, ApprovalDecisionModel, PendingAssistantFinalization,
    RawEngineEventRecord, RenderSessionSnapshot, TranscriptRecords,
};
use crate::render_model::{
    approval_history_from_transcript, command_history_from_transcript,
    command_summaries_from_messages, compact_plan_body_from_model, compact_plan_render_model,
    compact_status_body_from_model, error_history_from_transcript,
    file_change_summaries_from_messages, final_summary_history_from_transcript,
    permission_mode_choice_index, permission_mode_choices, permission_mode_code_for_choice,
    permission_mode_code_for_raw, permission_mode_display_label, tool_call_preview_from_input,
    tool_input_summary_from_value, ActivityPanelRenderModel, ActivityPanelSeverity,
    ApprovalAction as RenderApprovalAction, ApprovalHistoryRenderModel,
    ApprovalHistoryRowRenderModel, ApprovalRenderModel, ApprovalRiskLevel, BlockingKind,
    BlockingRenderModel, CommandHistoryRenderModel, CommandHistoryRowRenderModel,
    CommandRunRenderModel, CommandRunStatus, CommandStreamRenderModel, CommandSummaryModel,
    CompactPlanRenderModel, CompactStatusRenderModel, ContextUsageRenderModel,
    DebugConfigRenderModel, ErrorHistoryRenderModel, ErrorHistoryRowRenderModel, ErrorRenderModel,
    ExternalStatusLineCommandConfig, FileChangeListRenderModel, FileChangeSummaryModel,
    FinalSummaryHistoryRenderModel, FinalSummaryModel, FooterItem, FooterPreset,
    FooterRenderConfig, FooterRenderModel, ProcessListRenderModel, ProcessRowKind,
    ProcessRowRenderModel, ProcessStatus, ProcessSummaryRenderModel, RenderNode, RenderSurface,
    RenderTimelineRenderModel, RenderTranscript, SessionTitleRenderModel,
    StatusOverviewRenderModel, StatusRowLevel, StatusSectionRenderModel, ToolSectionKind,
    TopStatusRenderModel, VerificationSummaryModel,
};
use crate::state::{
    AppState, AppStore, McpConnectionState, McpServerStatus, RenderActivity, SlashCommandInfo,
    SlashCommandKind, TeammateState, TurnState, UiStage,
};
use crate::theme::Theme;
use crate::widgets::activity_panel::ActivityPanelWidget;
use crate::widgets::approval::ApprovalBlockWidget;
use crate::widgets::approval_history::ApprovalHistoryWidget;
use crate::widgets::command_history::CommandHistoryWidget;
use crate::widgets::debug_config::DebugConfigWidget;
use crate::widgets::diff::{parse_unified_diff, DiffDialogWidget, FileDiff};
use crate::widgets::error_history::ErrorHistoryWidget;
use crate::widgets::file_changes::FileChangesWidget;
use crate::widgets::messages::MessagesWidget;
use crate::widgets::process_list::ProcessListWidget;
use crate::widgets::prompt_input::{
    PromptInputState, PromptInputWidget, Suggestion, SuggestionKind,
};
use crate::widgets::render_timeline::RenderTimelineWidget;
use crate::widgets::session_title::SessionTitleWidget;
use crate::widgets::spinner::{SpinnerRowWidget, SpinnerState};
use crate::widgets::status_header::StatusHeaderWidget;
use crate::widgets::status_overview::StatusOverviewWidget;
use crate::widgets::summary_history::SummaryHistoryWidget;

use mossen_agent::engine::submit_prompt;

const MAX_RENDER_EVENT_HISTORY: usize = 256;
const MAX_RAW_ENGINE_EVENT_HISTORY: usize = 256;
const RENDER_SESSION_SNAPSHOT_DIR: &str = ".mossen/render-sessions";
const RENDER_STATUSLINE_CONFIG_PATH: &str = ".mossen/render-ui/statusline.json";
const ACTIVE_RENDER_FRAME_INTERVAL: Duration = Duration::from_millis(66);
const MAX_ACTIVE_RENDER_FRAME_INTERVAL: Duration = Duration::from_millis(250);
const MOUSE_WHEEL_SCROLL_ROWS: usize = 3;
const PERMISSION_MODE_ENV: &str = "MOSSEN_PERMISSION_MODE";
use mossen_agent::types::ContentDelta;
use mossen_agent::types::{
    EffortLevel, OriginTag, PermissionMode, PromptParams, SdkMessage, StreamEventData,
};
use mossen_commands::access::{PERMISSION_ALLOW_RULES_ENV, PERMISSION_DENY_RULES_ENV};
use mossen_commands::{
    find_directive, BoxedDirective, CommandContext, CommandCostSnapshot, CommandResult,
};
use mossen_tools::todo::TaskNotePadInput;
use mossen_types::{ContentBlock, Message, Role, TextBlock, ToolDefinition, ToolUseContext};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

// ---------------------------------------------------------------------------
// Engine integration config
// ---------------------------------------------------------------------------

pub type McpReloadFuture =
    Pin<Box<dyn Future<Output = Result<McpRuntimeReloadResult, String>> + Send>>;
pub type McpReloadCallback = Arc<dyn Fn() -> McpReloadFuture + Send + Sync>;

#[derive(Debug, Clone, Default)]
pub struct McpRuntimeReloadResult {
    pub tool_definitions: Vec<ToolDefinition>,
    pub server_count: usize,
    pub connected_count: usize,
}

/// Static engine configuration the App needs to build `PromptParams` for each
/// turn. Built once by the launcher and threaded through `App::with_engine`.
#[derive(Clone)]
pub struct EngineConfig {
    /// Default model id (for example, "example-fast" in tests).
    pub model: String,
    /// Pre-assembled system prompt blocks. Built once by the launcher
    /// (`mossen_cli::system_prompt::assemble`) and reused across every turn
    /// in the session — `EngineConfig` is meant to be static.
    pub system_prompt: Vec<mossen_agent::types::SystemBlock>,
    /// Working directory for `ToolUseContext`.
    pub cwd: String,
    /// API base URL override.
    pub api_base_url: Option<String>,
    /// API key (read from env in launcher).
    pub api_key: Option<String>,
    /// Origin tag used for telemetry / dialog routing.
    pub origin_tag: OriginTag,
    /// Max conversation turns per dispatch (None = engine default).
    pub max_turns: Option<u32>,
    /// Live fast-mode override forwarded into the engine request path.
    pub fast_mode: Option<bool>,
    /// Live effort override forwarded into provider-specific request fields.
    pub effort: Option<EffortLevel>,
    /// Extra request body fields passed verbatim to the backend.
    pub extra_body: HashMap<String, serde_json::Value>,
    /// Output style selected via `/output-style` picker. `None` = the
    /// composer's default; otherwise the launcher's system-prompt
    /// assembler appends a guidance section that pushes the model
    /// toward the requested style (e.g. "Concise", "Explanatory").
    /// Stored on EngineConfig so the picker can update it live and the
    /// next `handle_submit` reassembles the prompt automatically.
    pub output_style: Option<String>,
    /// Hook runtime context used by dialogue and `/compact` to execute hooks
    /// from the same session/plugin configuration as the launcher.
    pub compact_hook_context: Option<Arc<mossen_utils::hooks_utils::HooksContext>>,
}

impl std::fmt::Debug for EngineConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EngineConfig")
            .field("model", &self.model)
            .field("system_prompt_len", &self.system_prompt.len())
            .field("cwd", &self.cwd)
            .field("api_base_url", &self.api_base_url)
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .field("origin_tag", &self.origin_tag)
            .field("max_turns", &self.max_turns)
            .field("fast_mode", &self.fast_mode)
            .field("effort", &self.effort)
            .field("extra_body", &self.extra_body)
            .field("output_style", &self.output_style)
            .field(
                "compact_hook_context",
                &self.compact_hook_context.as_ref().map(|_| "<configured>"),
            )
            .finish()
    }
}

struct CompactTaskRequest {
    task_id: u64,
    before_messages: usize,
    history: Vec<Message>,
    hook_context: Option<Arc<mossen_utils::hooks_utils::HooksContext>>,
    custom_instructions: Option<String>,
    cancel_token: tokio_util::sync::CancellationToken,
}

struct CompactTaskResult {
    task_id: u64,
    before_messages: usize,
    result: mossen_agent::services::compact::compact::CompactConversationResult,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TranscriptExportFormat {
    Markdown,
    Json,
    Text,
}

impl TranscriptExportFormat {
    fn from_arg(arg: &str) -> Option<Self> {
        match arg.trim().to_ascii_lowercase().as_str() {
            "md" | "markdown" => Some(Self::Markdown),
            "json" => Some(Self::Json),
            "txt" | "text" => Some(Self::Text),
            _ => None,
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Self::Markdown => "md",
            Self::Json => "json",
            Self::Text => "txt",
        }
    }
}

impl EngineConfig {
    /// Build a sensible default suitable for the TUI: model from env or
    /// hardcoded test value, custom backend base URL if present, etc.
    pub fn from_env(default_model: &str) -> Self {
        Self {
            model: std::env::var("MOSSEN_MODEL")
                .ok()
                .unwrap_or_else(|| default_model.to_string()),
            system_prompt: Vec::new(),
            cwd: std::env::current_dir()
                .ok()
                .and_then(|p| p.to_str().map(|s| s.to_string()))
                .unwrap_or_else(|| ".".to_string()),
            api_base_url: std::env::var("MOSSEN_API_BASE_URL").ok(),
            api_key: std::env::var("MOSSEN_API_KEY").ok(),
            origin_tag: OriginTag::Repl,
            max_turns: None,
            fast_mode: parse_env_bool("MOSSEN_FAST_MODE"),
            effort: std::env::var("MOSSEN_CODE_EFFORT_LEVEL")
                .ok()
                .and_then(|value| parse_effort_level(&value)),
            extra_body: HashMap::new(),
            output_style: None,
            compact_hook_context: None,
        }
    }
}

fn parse_env_bool(name: &str) -> Option<bool> {
    std::env::var(name)
        .ok()
        .and_then(|value| parse_bool_arg(&value))
}

fn parse_bool_arg(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "on" | "yes" | "enable" | "enabled" => Some(true),
        "0" | "false" | "off" | "no" | "disable" | "disabled" => Some(false),
        _ => None,
    }
}

fn parse_effort_level(value: &str) -> Option<EffortLevel> {
    match value.trim().to_ascii_lowercase().as_str() {
        "low" => Some(EffortLevel::Low),
        "medium" => Some(EffortLevel::Medium),
        "high" => Some(EffortLevel::High),
        "max" => Some(EffortLevel::Max),
        _ => None,
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct SessionPermissionRules {
    allow: Vec<String>,
    deny: Vec<String>,
}

#[derive(Debug)]
struct SessionPermissionGate {
    rules: SessionPermissionRules,
    fallback: Arc<dyn mossen_agent::types::PermissionGate>,
}

impl SessionPermissionGate {
    fn new(
        rules: SessionPermissionRules,
        fallback: Arc<dyn mossen_agent::types::PermissionGate>,
    ) -> Self {
        Self { rules, fallback }
    }

    fn matching_decision(
        &self,
        tool_name: &str,
        input: &serde_json::Value,
    ) -> Option<mossen_agent::types::PermissionDecision> {
        if permission_rules_match(&self.rules.deny, tool_name, input) {
            return Some(mossen_agent::types::PermissionDecision::Deny);
        }
        if permission_rules_match(&self.rules.allow, tool_name, input) {
            return Some(mossen_agent::types::PermissionDecision::Allow);
        }
        None
    }
}

#[async_trait::async_trait]
impl mossen_agent::types::PermissionGate for SessionPermissionGate {
    async fn check(
        &self,
        tool_name: &str,
        tool_id: &str,
        input: &serde_json::Value,
    ) -> mossen_agent::types::PermissionDecision {
        if let Some(decision) = self.matching_decision(tool_name, input) {
            return decision;
        }
        self.fallback.check(tool_name, tool_id, input).await
    }
}

fn permission_rules_from_env(env: &HashMap<String, String>) -> SessionPermissionRules {
    SessionPermissionRules {
        allow: permission_rule_lines_from_env(env, PERMISSION_ALLOW_RULES_ENV),
        deny: permission_rule_lines_from_env(env, PERMISSION_DENY_RULES_ENV),
    }
}

fn permission_rule_lines_from_env(env: &HashMap<String, String>, key: &str) -> Vec<String> {
    env.get(key)
        .map(|value| {
            value
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn permission_rules_match(rules: &[String], tool_name: &str, input: &serde_json::Value) -> bool {
    if rules.is_empty() {
        return false;
    }

    let candidates = permission_rule_candidates(tool_name, input);
    rules.iter().any(|rule| {
        let rule = normalize_permission_rule(rule);
        !rule.is_empty()
            && candidates
                .iter()
                .any(|candidate| permission_rule_matches_candidate(&rule, candidate))
    })
}

fn permission_rule_candidates(tool_name: &str, input: &serde_json::Value) -> Vec<String> {
    const INPUT_KEYS: &[&str] = &[
        "command",
        "file_path",
        "path",
        "url",
        "description",
        "prompt",
    ];

    let mut candidates = Vec::new();
    push_permission_rule_candidate(&mut candidates, tool_name.to_string());

    if let Some(object) = input.as_object() {
        for key in INPUT_KEYS {
            if let Some(value) = object.get(*key).and_then(serde_json::Value::as_str) {
                push_permission_rule_candidate(&mut candidates, value.to_string());
                push_permission_rule_candidate(&mut candidates, format!("{tool_name} {value}"));
                push_permission_rule_candidate(&mut candidates, format!("{tool_name}:{value}"));
            }
        }
    }

    candidates
}

fn push_permission_rule_candidate(candidates: &mut Vec<String>, candidate: String) {
    let candidate = normalize_permission_rule(&candidate);
    if !candidate.is_empty() && !candidates.iter().any(|existing| existing == &candidate) {
        candidates.push(candidate);
    }
}

fn normalize_permission_rule(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn permission_rule_matches_candidate(rule: &str, candidate: &str) -> bool {
    if rule == candidate {
        return true;
    }
    if rule.contains('*') && wildcard_permission_rule_matches(rule, candidate) {
        return true;
    }
    if let Some(tail) = candidate.strip_prefix(rule) {
        if tail.starts_with(' ') || tail.starts_with(':') {
            return true;
        }
    }
    if permission_rule_path_prefix_matches(rule, candidate) {
        return true;
    }
    false
}

fn wildcard_permission_rule_matches(pattern: &str, candidate: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    let parts: Vec<&str> = pattern.split('*').filter(|part| !part.is_empty()).collect();
    if parts.is_empty() {
        return true;
    }

    let mut position = 0;
    let mut index = 0;
    if !pattern.starts_with('*') {
        let first = parts[0];
        if !candidate.starts_with(first) {
            return false;
        }
        position = first.len();
        index = 1;
    }

    for part in parts.iter().skip(index) {
        let Some(found_at) = candidate[position..].find(part) else {
            return false;
        };
        position += found_at + part.len();
    }

    if !pattern.ends_with('*') {
        if let Some(last) = parts.last() {
            return candidate.ends_with(last);
        }
    }

    true
}

fn permission_rule_path_prefix_matches(rule: &str, candidate: &str) -> bool {
    if !rule.contains('/') && !rule.contains('\\') {
        return false;
    }
    let prefix = rule.trim_end_matches(['/', '\\']);
    if prefix.is_empty() {
        return false;
    }
    candidate == prefix
        || candidate
            .strip_prefix(prefix)
            .map(|tail| tail.starts_with('/') || tail.starts_with('\\'))
            .unwrap_or(false)
}

fn compact_instruction_tail(args: &[&str], start: usize) -> Option<String> {
    args.get(start..)
        .map(|tail| tail.join(" "))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn permission_rule_pattern(args: &[&str]) -> Option<String> {
    args.get(1..)
        .map(|tail| tail.join(" "))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn add_permission_rule(rules: &mut Vec<String>, rule: String) {
    if !rules
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(&rule))
    {
        rules.push(rule);
    }
}

fn remove_permission_rule(rules: &mut Vec<String>, rule: &str) {
    rules.retain(|existing| !existing.eq_ignore_ascii_case(rule));
}

fn sync_permission_rule_env_value(
    env_vars: &mut HashMap<String, String>,
    key: &str,
    rules: &[String],
) {
    if rules.is_empty() {
        env_vars.remove(key);
    } else {
        env_vars.insert(key.to_string(), rules.join("\n"));
    }
}

// ---------------------------------------------------------------------------
// Active modal
// ---------------------------------------------------------------------------

/// Tracks which (if any) modal overlay is currently displayed on top of the
/// REPL surface. Maintains the modal-stack contract: exactly one
/// modal is interactive at a time, and key events are routed to its handler.
pub enum ActiveModal {
    None,
    PermissionRequest(PermissionPromptState),
    ToolUseConfirm {
        confirm: ToolUseConfirm,
        prompt: PermissionPromptState,
    },
    /// Reserved variants — wired into the enum so future work can target
    /// them without changing the App shape. They're currently not produced
    /// by `handle_engine_message` but are routable through the same dispatcher.
    CostThreshold(String),
    IdleReturn(String),
    MessageSelector(usize),
    Search(String),
    HelpDialog(HelpDialogState),
    ConfirmClear,
    StatusDialog,
    StatusLineConfig(StatusLineConfigState),
    TitleConfig(TitleConfigState),
    RawTranscript(RawTranscriptState),
    DiffReview(DiffReviewState),
    FileChanges(FileChangesState),
    RenderTimeline(RenderTimelineState),
    ProcessList(ProcessListState),
    CommandHistory(CommandHistoryState),
    ErrorHistory(ErrorHistoryState),
    FinalSummaryHistory(FinalSummaryHistoryState),
    ApprovalHistory(ApprovalHistoryState),
    DebugConfig(DebugConfigState),
    TasksDialog,
    McpServersDialog,
    McpChannelApproval(mossen_agent::mcp::channel_approval::ChannelApprovalRequest),
    ModelPicker(crate::widgets::panels::ModelPickerState),
    SkillsPanel(crate::widgets::panels::SkillsPanelState),
    MemoryPanel(crate::widgets::panels::MemoryPanelState),
    CommandOutput {
        title: String,
        body: String,
        is_error: bool,
    },
    /// Generic single-select picker — used by `/theme`, `/output-style`,
    /// etc. The renderer simply walks `items` with `▸` next to `selected`,
    /// and the App reacts to Enter based on `kind`.
    Picker {
        kind: PickerKind,
        title: String,
        items: Vec<String>,
        selected: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawTranscriptState {
    pub lines: Vec<String>,
    pub scroll: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HelpDialogState {
    pub scroll: usize,
    pub query: String,
}

impl HelpDialogState {
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            scroll: 0,
            query: query.into(),
        }
    }

    pub fn scroll_up(&mut self, rows: usize) {
        self.scroll = self.scroll.saturating_sub(rows);
    }

    pub fn scroll_down(&mut self, rows: usize, total_rows: usize, viewport_rows: usize) {
        self.scroll = self
            .scroll
            .saturating_add(rows)
            .min(help_scroll_max(total_rows, viewport_rows));
    }

    pub fn scroll_to_end(&mut self, total_rows: usize, viewport_rows: usize) {
        self.scroll = help_scroll_max(total_rows, viewport_rows);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffReviewState {
    pub files: Vec<FileDiff>,
    pub selected_file: usize,
    pub scroll: usize,
    pub collapsed_files: BTreeSet<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChangesState {
    pub model: FileChangeListRenderModel,
    pub selected: usize,
    pub scroll: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderTimelineState {
    pub model: RenderTimelineRenderModel,
    pub selected: usize,
    pub scroll: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessListState {
    pub model: ProcessListRenderModel,
    pub selected: usize,
    pub scroll: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandHistoryState {
    pub model: CommandHistoryRenderModel,
    pub selected: usize,
    pub scroll: usize,
    pub expanded_rows: BTreeSet<String>,
    pub detail_scroll: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorHistoryState {
    pub model: ErrorHistoryRenderModel,
    pub selected: usize,
    pub scroll: usize,
    pub expanded_rows: BTreeSet<String>,
    pub detail_scroll: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinalSummaryHistoryState {
    pub model: FinalSummaryHistoryRenderModel,
    pub selected: usize,
    pub scroll: usize,
    pub expanded_rows: BTreeSet<String>,
    pub detail_scroll: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalHistoryState {
    pub model: ApprovalHistoryRenderModel,
    pub selected: usize,
    pub scroll: usize,
    pub expanded_rows: BTreeSet<String>,
    pub detail_scroll: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebugConfigState {
    pub model: DebugConfigRenderModel,
    pub scroll: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModalScrollbarKind {
    Help,
    CommandOutput,
    RawTranscript,
    DiffReview,
    FileChanges,
    RenderTimeline,
    ProcessList,
    CommandHistory,
    ErrorHistory,
    FinalSummaryHistory,
    ApprovalHistory,
    DebugConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ModalScrollbarHitTarget {
    kind: ModalScrollbarKind,
    area: Rect,
    total_rows: usize,
    viewport_rows: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TitleConfigState {
    pub draft: String,
    pub notice: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusLineConfigState {
    pub items: Vec<FooterItem>,
    pub selected: usize,
}

impl TitleConfigState {
    fn new(draft: impl Into<String>) -> Self {
        Self {
            draft: draft.into(),
            notice: "terminal title".to_string(),
        }
    }

    fn notice(mut self, notice: impl Into<String>) -> Self {
        self.notice = notice.into();
        self
    }

    fn push_char(&mut self, ch: char) {
        if !ch.is_control() {
            self.draft.push(ch);
        }
    }

    fn backspace(&mut self) {
        self.draft.pop();
    }

    fn clear(&mut self) {
        self.draft.clear();
    }
}

impl StatusLineConfigState {
    fn new() -> Self {
        Self {
            items: FooterItem::ALL.to_vec(),
            selected: 0,
        }
    }

    fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    fn move_down(&mut self) {
        if self.selected + 1 < self.items.len() {
            self.selected += 1;
        }
    }

    fn selected_item(&self) -> Option<FooterItem> {
        self.items.get(self.selected).copied()
    }
}

impl RawTranscriptState {
    fn new(lines: Vec<String>) -> Self {
        Self { lines, scroll: 0 }
    }

    fn max_scroll(&self, viewport_height: usize) -> usize {
        self.lines.len().saturating_sub(viewport_height)
    }

    fn scroll_up(&mut self, amount: usize, viewport_height: usize) {
        self.scroll = self.scroll.saturating_sub(amount);
        self.scroll = self.scroll.min(self.max_scroll(viewport_height));
    }

    fn scroll_down(&mut self, amount: usize, viewport_height: usize) {
        self.scroll = self
            .scroll
            .saturating_add(amount)
            .min(self.max_scroll(viewport_height));
    }

    fn scroll_to_top(&mut self) {
        self.scroll = 0;
    }

    fn scroll_to_bottom(&mut self, viewport_height: usize) {
        self.scroll = self.max_scroll(viewport_height);
    }
}

impl DiffReviewState {
    fn new(files: Vec<FileDiff>) -> Self {
        Self {
            files,
            selected_file: 0,
            scroll: 0,
            collapsed_files: BTreeSet::new(),
        }
    }

    fn move_prev_file(&mut self) {
        self.selected_file = self.selected_file.saturating_sub(1);
        self.scroll = 0;
    }

    fn move_next_file(&mut self) {
        if self.selected_file + 1 < self.files.len() {
            self.selected_file += 1;
            self.scroll = 0;
        }
    }

    fn toggle_selected_file(&mut self) {
        if self.collapsed_files.contains(&self.selected_file) {
            self.collapsed_files.remove(&self.selected_file);
        } else {
            self.collapsed_files.insert(self.selected_file);
        }
        self.scroll = 0;
    }

    fn max_scroll(&self, viewport_height: usize) -> usize {
        self.selected_line_count().saturating_sub(viewport_height)
    }

    fn scroll_up(&mut self, amount: usize, viewport_height: usize) {
        self.scroll = self.scroll.saturating_sub(amount);
        self.scroll = self.scroll.min(self.max_scroll(viewport_height));
    }

    fn scroll_down(&mut self, amount: usize, viewport_height: usize) {
        self.scroll = self
            .scroll
            .saturating_add(amount)
            .min(self.max_scroll(viewport_height));
    }

    fn scroll_to_top(&mut self) {
        self.scroll = 0;
    }

    fn scroll_to_bottom(&mut self, viewport_height: usize) {
        self.scroll = self.max_scroll(viewport_height);
    }

    fn selected_line_count(&self) -> usize {
        if self.collapsed_files.contains(&self.selected_file) {
            return 3;
        }
        self.files
            .get(self.selected_file)
            .map(diff_detail_line_count)
            .unwrap_or_default()
    }
}

fn diff_detail_line_count(file: &FileDiff) -> usize {
    2 + file
        .hunks
        .iter()
        .map(|hunk| hunk.lines.len().saturating_add(2))
        .fold(0usize, usize::saturating_add)
}

impl FileChangesState {
    fn new(model: FileChangeListRenderModel) -> Self {
        Self {
            model,
            selected: 0,
            scroll: 0,
        }
    }

    fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
        if self.selected < self.scroll {
            self.scroll = self.selected;
        }
    }

    fn move_down(&mut self, viewport_rows: usize) {
        if self.selected + 1 < self.model.len() {
            self.selected += 1;
        }
        self.ensure_visible(viewport_rows);
    }

    fn page_up(&mut self, viewport_rows: usize) {
        self.selected = self.selected.saturating_sub(viewport_rows.max(1));
        self.ensure_visible(viewport_rows);
    }

    fn page_down(&mut self, viewport_rows: usize) {
        if self.model.is_empty() {
            return;
        }
        self.selected = self
            .selected
            .saturating_add(viewport_rows.max(1))
            .min(self.model.len().saturating_sub(1));
        self.ensure_visible(viewport_rows);
    }

    fn select_first(&mut self) {
        self.selected = 0;
        self.scroll = 0;
    }

    fn select_last(&mut self, viewport_rows: usize) {
        self.selected = self.model.len().saturating_sub(1);
        self.ensure_visible(viewport_rows);
    }

    fn ensure_visible(&mut self, viewport_rows: usize) {
        let viewport_rows = viewport_rows.max(1);
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll.saturating_add(viewport_rows) {
            self.scroll = self
                .selected
                .saturating_add(1)
                .saturating_sub(viewport_rows);
        }
        self.scroll = self.scroll.min(self.model.len().saturating_sub(1));
    }
}

impl RenderTimelineState {
    fn new(model: RenderTimelineRenderModel) -> Self {
        let selected = model.len().saturating_sub(1);
        Self {
            model,
            selected,
            scroll: 0,
        }
    }

    fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
        if self.selected < self.scroll {
            self.scroll = self.selected;
        }
    }

    fn move_down(&mut self, viewport_rows: usize) {
        if self.selected + 1 < self.model.len() {
            self.selected += 1;
        }
        self.ensure_visible(viewport_rows);
    }

    fn page_up(&mut self, viewport_rows: usize) {
        self.selected = self.selected.saturating_sub(viewport_rows.max(1));
        self.ensure_visible(viewport_rows);
    }

    fn page_down(&mut self, viewport_rows: usize) {
        if self.model.is_empty() {
            return;
        }
        self.selected = self
            .selected
            .saturating_add(viewport_rows.max(1))
            .min(self.model.len().saturating_sub(1));
        self.ensure_visible(viewport_rows);
    }

    fn select_first(&mut self) {
        self.selected = 0;
        self.scroll = 0;
    }

    fn select_last(&mut self, viewport_rows: usize) {
        self.selected = self.model.len().saturating_sub(1);
        self.ensure_visible(viewport_rows);
    }

    fn ensure_visible(&mut self, viewport_rows: usize) {
        let viewport_rows = viewport_rows.max(1);
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll.saturating_add(viewport_rows) {
            self.scroll = self
                .selected
                .saturating_add(1)
                .saturating_sub(viewport_rows);
        }
        self.scroll = self.scroll.min(self.model.len().saturating_sub(1));
    }
}

impl ProcessListState {
    fn new(model: ProcessListRenderModel) -> Self {
        Self {
            model,
            selected: 0,
            scroll: 0,
        }
    }

    fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
        if self.selected < self.scroll {
            self.scroll = self.selected;
        }
    }

    fn move_down(&mut self, viewport_rows: usize) {
        if self.selected + 1 < self.model.len() {
            self.selected += 1;
        }
        self.ensure_visible(viewport_rows);
    }

    fn page_up(&mut self, viewport_rows: usize) {
        self.selected = self.selected.saturating_sub(viewport_rows.max(1));
        self.ensure_visible(viewport_rows);
    }

    fn page_down(&mut self, viewport_rows: usize) {
        if self.model.is_empty() {
            return;
        }
        self.selected = self
            .selected
            .saturating_add(viewport_rows.max(1))
            .min(self.model.len().saturating_sub(1));
        self.ensure_visible(viewport_rows);
    }

    fn select_first(&mut self) {
        self.selected = 0;
        self.scroll = 0;
    }

    fn select_last(&mut self, viewport_rows: usize) {
        self.selected = self.model.len().saturating_sub(1);
        self.ensure_visible(viewport_rows);
    }

    fn ensure_visible(&mut self, viewport_rows: usize) {
        let viewport_rows = viewport_rows.max(1);
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll.saturating_add(viewport_rows) {
            self.scroll = self
                .selected
                .saturating_add(1)
                .saturating_sub(viewport_rows);
        }
        self.scroll = self.scroll.min(self.model.len().saturating_sub(1));
    }
}

impl CommandHistoryState {
    fn new(model: CommandHistoryRenderModel) -> Self {
        Self {
            model,
            selected: 0,
            scroll: 0,
            expanded_rows: BTreeSet::new(),
            detail_scroll: 0,
        }
    }

    fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
        if self.selected < self.scroll {
            self.scroll = self.selected;
        }
        self.detail_scroll = 0;
    }

    fn move_down(&mut self, viewport_rows: usize) {
        if self.selected + 1 < self.model.len() {
            self.selected += 1;
        }
        self.ensure_visible(viewport_rows);
        self.detail_scroll = 0;
    }

    fn page_up(&mut self, viewport_rows: usize) {
        self.selected = self.selected.saturating_sub(viewport_rows.max(1));
        self.ensure_visible(viewport_rows);
        self.detail_scroll = 0;
    }

    fn page_down(&mut self, viewport_rows: usize) {
        if self.model.is_empty() {
            return;
        }
        self.selected = self
            .selected
            .saturating_add(viewport_rows.max(1))
            .min(self.model.len().saturating_sub(1));
        self.ensure_visible(viewport_rows);
        self.detail_scroll = 0;
    }

    fn select_first(&mut self) {
        self.selected = 0;
        self.scroll = 0;
        self.detail_scroll = 0;
    }

    fn select_last(&mut self, viewport_rows: usize) {
        if self.model.is_empty() {
            return;
        }
        self.selected = self.model.len().saturating_sub(1);
        self.ensure_visible(viewport_rows);
        self.detail_scroll = 0;
    }

    fn ensure_visible(&mut self, viewport_rows: usize) {
        let viewport_rows = viewport_rows.max(1);
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll.saturating_add(viewport_rows) {
            self.scroll = self
                .selected
                .saturating_add(1)
                .saturating_sub(viewport_rows);
        }
        self.scroll = self.scroll.min(self.model.len().saturating_sub(1));
    }

    fn selected_row_id(&self) -> Option<&str> {
        self.model
            .rows
            .get(self.selected)
            .map(|row| row.id.as_str())
    }

    fn selected_expanded(&self) -> bool {
        self.selected_row_id()
            .is_some_and(|id| self.expanded_rows.contains(id))
    }

    fn toggle_selected_log(&mut self) {
        let Some(row) = self.model.rows.get(self.selected) else {
            return;
        };
        if !row.run.has_embedded_full_log() {
            return;
        }
        let id = row.id.clone();
        if !self.expanded_rows.remove(&id) {
            self.expanded_rows.insert(id);
        }
        self.detail_scroll = 0;
    }

    fn detail_page_up(&mut self, viewport_rows: usize) {
        self.detail_scroll = self.detail_scroll.saturating_sub(viewport_rows.max(1));
    }

    fn detail_page_down(&mut self, viewport_rows: usize) {
        let max_scroll = self
            .model
            .rows
            .get(self.selected)
            .map(|row| row.run.full_log_line_count().saturating_sub(1))
            .unwrap_or_default();
        self.detail_scroll = self
            .detail_scroll
            .saturating_add(viewport_rows.max(1))
            .min(max_scroll);
    }
}

impl ErrorHistoryState {
    fn new(model: ErrorHistoryRenderModel) -> Self {
        Self {
            model,
            selected: 0,
            scroll: 0,
            expanded_rows: BTreeSet::new(),
            detail_scroll: 0,
        }
    }

    fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
        if self.selected < self.scroll {
            self.scroll = self.selected;
        }
        self.detail_scroll = 0;
    }

    fn move_down(&mut self, viewport_rows: usize) {
        if self.selected + 1 < self.model.len() {
            self.selected += 1;
        }
        self.ensure_visible(viewport_rows);
        self.detail_scroll = 0;
    }

    fn page_up(&mut self, viewport_rows: usize) {
        self.selected = self.selected.saturating_sub(viewport_rows.max(1));
        self.ensure_visible(viewport_rows);
        self.detail_scroll = 0;
    }

    fn page_down(&mut self, viewport_rows: usize) {
        if self.model.is_empty() {
            return;
        }
        self.selected = self
            .selected
            .saturating_add(viewport_rows.max(1))
            .min(self.model.len().saturating_sub(1));
        self.ensure_visible(viewport_rows);
        self.detail_scroll = 0;
    }

    fn select_first(&mut self) {
        self.selected = 0;
        self.scroll = 0;
        self.detail_scroll = 0;
    }

    fn select_last(&mut self, viewport_rows: usize) {
        if self.model.is_empty() {
            return;
        }
        self.selected = self.model.len().saturating_sub(1);
        self.ensure_visible(viewport_rows);
        self.detail_scroll = 0;
    }

    fn ensure_visible(&mut self, viewport_rows: usize) {
        let viewport_rows = viewport_rows.max(1);
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll.saturating_add(viewport_rows) {
            self.scroll = self
                .selected
                .saturating_add(1)
                .saturating_sub(viewport_rows);
        }
        self.scroll = self.scroll.min(self.model.len().saturating_sub(1));
    }

    fn selected_row_id(&self) -> Option<&str> {
        self.model
            .rows
            .get(self.selected)
            .map(|row| row.id.as_str())
    }

    fn selected_expanded(&self) -> bool {
        self.selected_row_id()
            .is_some_and(|id| self.expanded_rows.contains(id))
    }

    fn toggle_selected_details(&mut self) {
        let Some(row) = self.model.rows.get(self.selected) else {
            return;
        };
        if !row.has_details() {
            return;
        }
        let id = row.id.clone();
        if !self.expanded_rows.remove(&id) {
            self.expanded_rows.insert(id);
        }
        self.detail_scroll = 0;
    }

    fn detail_page_up(&mut self, viewport_rows: usize) {
        self.detail_scroll = self.detail_scroll.saturating_sub(viewport_rows.max(1));
    }

    fn detail_page_down(&mut self, viewport_rows: usize) {
        let max_scroll = self
            .model
            .rows
            .get(self.selected)
            .map(|row| row.detail_line_count().saturating_sub(1))
            .unwrap_or_default();
        self.detail_scroll = self
            .detail_scroll
            .saturating_add(viewport_rows.max(1))
            .min(max_scroll);
    }
}

impl FinalSummaryHistoryState {
    fn new(model: FinalSummaryHistoryRenderModel) -> Self {
        Self {
            model,
            selected: 0,
            scroll: 0,
            expanded_rows: BTreeSet::new(),
            detail_scroll: 0,
        }
    }

    fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
        if self.selected < self.scroll {
            self.scroll = self.selected;
        }
        self.detail_scroll = 0;
    }

    fn move_down(&mut self, viewport_rows: usize) {
        if self.selected + 1 < self.model.len() {
            self.selected += 1;
        }
        self.ensure_visible(viewport_rows);
        self.detail_scroll = 0;
    }

    fn page_up(&mut self, viewport_rows: usize) {
        self.selected = self.selected.saturating_sub(viewport_rows.max(1));
        self.ensure_visible(viewport_rows);
        self.detail_scroll = 0;
    }

    fn page_down(&mut self, viewport_rows: usize) {
        if self.model.is_empty() {
            return;
        }
        self.selected = self
            .selected
            .saturating_add(viewport_rows.max(1))
            .min(self.model.len().saturating_sub(1));
        self.ensure_visible(viewport_rows);
        self.detail_scroll = 0;
    }

    fn select_first(&mut self) {
        self.selected = 0;
        self.scroll = 0;
        self.detail_scroll = 0;
    }

    fn select_last(&mut self, viewport_rows: usize) {
        if self.model.is_empty() {
            return;
        }
        self.selected = self.model.len().saturating_sub(1);
        self.ensure_visible(viewport_rows);
        self.detail_scroll = 0;
    }

    fn ensure_visible(&mut self, viewport_rows: usize) {
        let viewport_rows = viewport_rows.max(1);
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll.saturating_add(viewport_rows) {
            self.scroll = self
                .selected
                .saturating_add(1)
                .saturating_sub(viewport_rows);
        }
        self.scroll = self.scroll.min(self.model.len().saturating_sub(1));
    }

    fn selected_row_id(&self) -> Option<&str> {
        self.model
            .rows
            .get(self.selected)
            .map(|row| row.id.as_str())
    }

    fn selected_expanded(&self) -> bool {
        self.selected_row_id()
            .is_some_and(|id| self.expanded_rows.contains(id))
    }

    fn toggle_selected_details(&mut self) {
        let Some(row) = self.model.rows.get(self.selected) else {
            return;
        };
        if !row.has_details() {
            return;
        }
        let id = row.id.clone();
        if !self.expanded_rows.remove(&id) {
            self.expanded_rows.insert(id);
        }
        self.detail_scroll = 0;
    }

    fn detail_page_up(&mut self, viewport_rows: usize) {
        self.detail_scroll = self.detail_scroll.saturating_sub(viewport_rows.max(1));
    }

    fn detail_page_down(&mut self, viewport_rows: usize) {
        let max_scroll = self
            .model
            .rows
            .get(self.selected)
            .map(|row| row.detail_line_count().saturating_sub(1))
            .unwrap_or_default();
        self.detail_scroll = self
            .detail_scroll
            .saturating_add(viewport_rows.max(1))
            .min(max_scroll);
    }
}

impl ApprovalHistoryState {
    fn new(model: ApprovalHistoryRenderModel) -> Self {
        Self {
            model,
            selected: 0,
            scroll: 0,
            expanded_rows: BTreeSet::new(),
            detail_scroll: 0,
        }
    }

    fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
        if self.selected < self.scroll {
            self.scroll = self.selected;
        }
        self.detail_scroll = 0;
    }

    fn move_down(&mut self, viewport_rows: usize) {
        if self.selected + 1 < self.model.len() {
            self.selected += 1;
        }
        self.ensure_visible(viewport_rows);
        self.detail_scroll = 0;
    }

    fn page_up(&mut self, viewport_rows: usize) {
        self.selected = self.selected.saturating_sub(viewport_rows.max(1));
        self.ensure_visible(viewport_rows);
        self.detail_scroll = 0;
    }

    fn page_down(&mut self, viewport_rows: usize) {
        if self.model.is_empty() {
            return;
        }
        self.selected = self
            .selected
            .saturating_add(viewport_rows.max(1))
            .min(self.model.len().saturating_sub(1));
        self.ensure_visible(viewport_rows);
        self.detail_scroll = 0;
    }

    fn select_first(&mut self) {
        self.selected = 0;
        self.scroll = 0;
        self.detail_scroll = 0;
    }

    fn select_last(&mut self, viewport_rows: usize) {
        if self.model.is_empty() {
            return;
        }
        self.selected = self.model.len().saturating_sub(1);
        self.ensure_visible(viewport_rows);
        self.detail_scroll = 0;
    }

    fn ensure_visible(&mut self, viewport_rows: usize) {
        let viewport_rows = viewport_rows.max(1);
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll.saturating_add(viewport_rows) {
            self.scroll = self
                .selected
                .saturating_add(1)
                .saturating_sub(viewport_rows);
        }
        self.scroll = self.scroll.min(self.model.len().saturating_sub(1));
    }

    fn selected_row_id(&self) -> Option<&str> {
        self.model
            .rows
            .get(self.selected)
            .map(|row| row.id.as_str())
    }

    fn selected_expanded(&self) -> bool {
        self.selected_row_id()
            .is_some_and(|id| self.expanded_rows.contains(id))
    }

    fn toggle_selected_details(&mut self) {
        let Some(row) = self.model.rows.get(self.selected) else {
            return;
        };
        if !row.has_details() {
            return;
        }
        let id = row.id.clone();
        if !self.expanded_rows.remove(&id) {
            self.expanded_rows.insert(id);
        }
        self.detail_scroll = 0;
    }

    fn detail_page_up(&mut self, viewport_rows: usize) {
        self.detail_scroll = self.detail_scroll.saturating_sub(viewport_rows.max(1));
    }

    fn detail_page_down(&mut self, viewport_rows: usize) {
        let max_scroll = self
            .model
            .rows
            .get(self.selected)
            .map(|row| row.detail_line_count().saturating_sub(1))
            .unwrap_or_default();
        self.detail_scroll = self
            .detail_scroll
            .saturating_add(viewport_rows.max(1))
            .min(max_scroll);
    }
}

impl DebugConfigState {
    fn new(model: DebugConfigRenderModel) -> Self {
        Self { model, scroll: 0 }
    }

    fn scroll_max(&self, viewport_rows: usize) -> usize {
        self.model.row_count().saturating_sub(viewport_rows.max(1))
    }

    fn visible_scroll(&self, viewport_rows: usize) -> usize {
        self.scroll.min(self.scroll_max(viewport_rows))
    }

    fn scroll_up(&mut self, rows: usize) {
        self.scroll = self.scroll.saturating_sub(rows.max(1));
    }

    fn scroll_down(&mut self, rows: usize, viewport_rows: usize) {
        let max_scroll = self.scroll_max(viewport_rows);
        self.scroll = self.scroll.saturating_add(rows.max(1)).min(max_scroll);
    }

    fn scroll_to_top(&mut self) {
        self.scroll = 0;
    }

    fn scroll_to_bottom(&mut self, viewport_rows: usize) {
        self.scroll = self.scroll_max(viewport_rows);
    }
}

#[derive(Debug, Clone, Copy)]
pub enum PickerKind {
    Theme,
    OutputStyle,
    BackgroundTasks,
    PermissionMode,
}

impl ActiveModal {
    pub fn is_open(&self) -> bool {
        !matches!(self, ActiveModal::None)
    }

    pub fn is_inline_approval(&self) -> bool {
        matches!(
            self,
            ActiveModal::PermissionRequest(_)
                | ActiveModal::ToolUseConfirm { .. }
                | ActiveModal::McpChannelApproval(_)
        )
    }

    pub fn can_yield_to_approval(&self) -> bool {
        matches!(
            self,
            ActiveModal::None
                | ActiveModal::IdleReturn(_)
                | ActiveModal::CostThreshold(_)
                | ActiveModal::HelpDialog(_)
                | ActiveModal::StatusDialog
                | ActiveModal::StatusLineConfig(_)
                | ActiveModal::TitleConfig(_)
                | ActiveModal::RawTranscript(_)
                | ActiveModal::DiffReview(_)
                | ActiveModal::FileChanges(_)
                | ActiveModal::RenderTimeline(_)
                | ActiveModal::ProcessList(_)
                | ActiveModal::CommandHistory(_)
                | ActiveModal::ErrorHistory(_)
                | ActiveModal::FinalSummaryHistory(_)
                | ActiveModal::ApprovalHistory(_)
                | ActiveModal::DebugConfig(_)
                | ActiveModal::TasksDialog
                | ActiveModal::McpServersDialog
                | ActiveModal::CommandOutput { .. }
        )
    }
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

/// The main TUI application.
///
/// Owns all state and orchestrates rendering + event handling.
/// Main TUI app and render loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderSnapshotStartupRestoreStatus {
    Pending,
    Restored,
    Unavailable,
    Skipped,
    Error,
}

impl RenderSnapshotStartupRestoreStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Restored => "restored",
            Self::Unavailable => "unavailable",
            Self::Skipped => "skipped",
            Self::Error => "error",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FooterConfigPersistenceStatus {
    Pending,
    Loaded,
    Saved,
    Unavailable,
    Skipped,
    Error,
}

impl FooterConfigPersistenceStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Loaded => "loaded",
            Self::Saved => "saved",
            Self::Unavailable => "unavailable",
            Self::Skipped => "skipped",
            Self::Error => "error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ExternalStatusLineCommandResult {
    sequence: u64,
    output: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CachedRenderTranscript {
    key: RenderTranscriptCacheKey,
    transcript: RenderTranscript,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RenderTranscriptCacheKey {
    revision: u64,
    messages_len: usize,
    approvals_len: usize,
    id_overrides_len: usize,
    parent_overrides_len: usize,
    turn_overrides_len: usize,
    message_shape: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RenderTranscriptCacheStats {
    revision: u64,
    cached: bool,
    hits: u64,
    misses: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RenderFrameSchedulerStats {
    dirty: bool,
    throttled_due_in_ms: Option<u128>,
    next_frame_due_in_ms: Option<u128>,
    active_animation: bool,
    drawn: u64,
    skipped: u64,
    last_frame_age_ms: Option<u128>,
    last_frame_duration_ms: Option<u128>,
    max_frame_duration_ms: u128,
    avg_frame_duration_ms: Option<u128>,
    active_frame_interval_ms: u128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MouseScrollFingerprint {
    Transcript {
        offset: usize,
        sticky: bool,
    },
    Help {
        scroll: usize,
    },
    CommandOutput {
        scroll: usize,
    },
    LineScroll {
        scroll: usize,
    },
    Selectable {
        selected: usize,
        scroll: usize,
    },
    DetailSelectable {
        selected: usize,
        scroll: usize,
        detail_scroll: usize,
    },
    Picker {
        selected: usize,
    },
    StaticModal,
}

pub struct App {
    // --- Core state ---
    pub state: AppState,
    pub store: AppStore,
    pub theme: Theme,
    pub glyphs: RenderGlyphs,

    // --- UI state ---
    pub prompt: PromptInputState,
    pub spinner: SpinnerState,
    pub messages: Vec<MessageData>,
    pub approval_decisions: Vec<ApprovalDecisionModel>,
    pub next_render_record_seq: u64,
    pub next_render_turn_seq: u64,
    pub current_render_turn_id: Option<String>,
    pub render_record_id_overrides: HashMap<usize, String>,
    pub render_record_parent_overrides: HashMap<usize, String>,
    pub render_record_turn_overrides: HashMap<usize, String>,
    pub render_event_history: Vec<RenderEvent>,
    pub raw_engine_event_history: Vec<RawEngineEventRecord>,
    pub next_raw_engine_event_seq: u64,
    pub scroll: VirtualScroll,
    message_content_area: Option<Rect>,
    message_scrollbar_area: Option<Rect>,
    modal_scrollbar_target: Cell<Option<ModalScrollbarHitTarget>>,
    command_output_scroll: usize,
    pub render_height_cache: RenderHeightCache,
    render_transcript_revision: u64,
    render_transcript_cache: RefCell<Option<CachedRenderTranscript>>,
    render_transcript_cache_hits: Cell<u64>,
    render_transcript_cache_misses: Cell<u64>,
    render_dirty: bool,
    render_throttled_dirty_at: Option<Instant>,
    render_last_frame_at: Option<Instant>,
    render_last_frame_duration: Option<Duration>,
    render_max_frame_duration: Duration,
    render_total_frame_duration: Duration,
    render_frame_count: u64,
    render_skipped_frame_count: u64,

    // --- Lifecycle ---
    pub started_at: Instant,
    pub should_quit: bool,
    pub fullscreen: bool,

    // --- Engine integration ---
    pub engine_config: EngineConfig,
    pub engine_rx: Option<mpsc::Receiver<SdkMessage>>,
    pub engine_session_id: Option<String>,
    /// Text-only conversation history forwarded to the engine on each
    /// turn. `/compact` trims this model-facing context while the visual
    /// transcript can remain available for the user.
    pub engine_history: Vec<Message>,
    pending_compact: Option<CompactTaskRequest>,
    compact_result_tx: mpsc::UnboundedSender<CompactTaskResult>,
    compact_result_rx: mpsc::UnboundedReceiver<CompactTaskResult>,
    next_compact_task_id: u64,
    active_compact_task_id: Option<u64>,
    active_compact_cancel_token: Option<tokio_util::sync::CancellationToken>,
    /// Index in `self.messages` of the assistant message currently being
    /// filled by streaming deltas, or `None` when there is no pending turn.
    pub pending_assistant_idx: Option<usize>,
    /// Set by `handle_submit` when a user prompt should be dispatched to
    /// the engine on the next iteration of the main loop. The main loop
    /// consumes this to perform the async `submit_prompt` call.
    pub pending_submit: Option<PromptParams>,
    /// Accumulated text for the in-flight assistant message — used as a
    /// safety net when the engine produces only `StreamEvent` deltas.
    pub assistant_buf: String,
    /// Prevents a final Assistant event plus Result from recording the
    /// same assistant text twice into `engine_history`.
    pub pending_assistant_history_recorded: bool,

    // --- Modal overlay ---
    pub active_modal: ActiveModal,

    // --- Commands ---
    /// Shared directive list. Optional because some tests construct an
    /// `App` without command support. Stored as `Arc<Vec<BoxedDirective>>`
    /// so the App can be cloned cheaply and so that callers (e.g. the CLI
    /// repl launcher) can build the registry once and share it.
    pub directives: Option<Arc<Vec<BoxedDirective>>>,
    /// Cached `CommandContext` used when invoking directives.
    pub command_context: CommandContext,

    // --- Terminal services (chrome / dialogs / search / message-selector) ---
    /// Auxiliary services: terminal title + tab status + cost/idle/search/
    /// message-selector dialog state. See `app_services.rs`.
    pub services: crate::app_services::TerminalServices,

    /// Shared skill registry — loaded once by the launcher and passed in via
    /// [`with_engine`]. Stored so slash commands, agent-side hooks, and the
    /// engine's tool execution path can look up skills by id/name. The legacy
    /// [`App::new`] constructor leaves this as `None`; production callers
    /// always wire a registry.
    pub skill_registry: Option<mossen_skills::SharedCraftRegistry>,
    /// User-invocable skills already reflected in the slash catalog. Ticks
    /// compare against this set so dynamic skill activation becomes visible
    /// without spamming the transcript.
    pub known_skill_names: std::collections::HashSet<String>,

    /// Running session cost in USD — accumulated from every
    /// `SdkMessage::Result.cost_usd` the engine emits at the end of a turn.
    /// Surfaced to the status bar and the cost-threshold dialog via
    /// `services_tick`.
    pub total_cost_usd: f64,

    /// Receiver end of the channel the engine's `InteractiveGate` posts
    /// `PermissionRequest`s on. The main tick loop pulls from this and, when
    /// no other modal is active, opens a `ToolUseConfirm` overlay populated
    /// from the request; the modal's Allow / Deny / Allow-Always buttons
    /// then drive `active_permission_responder` below.
    pub permission_rx: Option<tokio::sync::mpsc::Receiver<mossen_agent::types::PermissionRequest>>,

    /// One-shot reply channel for the currently-pending permission request.
    /// `Some` only while a `ToolUseConfirm` modal is awaiting user input;
    /// cleared once `Allow` / `Deny` is sent back to the engine.
    pub active_permission_responder:
        Option<tokio::sync::oneshot::Sender<mossen_agent::types::PermissionDecision>>,

    /// Session-scoped allow/deny rules configured through `/permissions`.
    /// These are mirrored into `command_context.env_vars` so the command
    /// registry can render the same state the agent permission gate uses.
    permission_rules: SessionPermissionRules,
    /// Additional working directories admitted with `/add-dir` for the current
    /// session. Forwarded to every tool-use context on subsequent turns.
    additional_working_directories: Vec<String>,

    /// Executable tool registry, shared with the agent. The CLI builds this
    /// from the runtime-gated mossen tool registry and injects via
    /// [`App::with_tool_registry`]. When `Some`, `handle_submit` extracts
    /// `ToolDefinition`s for the request body and clones the `Arc` into
    /// `PromptParams::tool_registry` so the dialogue loop can actually
    /// execute the `tool_use` blocks the model emits.
    pub tool_registry: Option<std::sync::Arc<mossen_agent::tool_registry::ToolRegistry>>,
    /// Extra tool definitions that are model-visible but not executed through
    /// the built-in tool registry. MCP tools use this path: dialogue.rs
    /// recognizes `mcp__...` names and routes them to the live MCP manager.
    pub extra_tool_definitions: Vec<ToolDefinition>,
    /// Host-provided hook that reconnects the live MCP runtime after
    /// `/reload-plugins`. Kept as a callback so mossen-tui does not depend on
    /// mossen-cli or own the MCP manager lifecycle.
    pub mcp_reload_callback: Option<McpReloadCallback>,

    /// Ctrl+E toggle — when true, every assistant message's thinking
    /// block stays rendered regardless of the 30s auto-fade timer.
    pub show_all_thinking: bool,

    /// Pluggable task-list snapshot provider — set by the launcher so
    /// Ctrl+T can dump the live TaskStore content without forcing
    /// mossen-tui to depend on mossen-tools directly. Each entry is
    /// `(status, id, subject)`.
    pub task_snapshot_provider:
        Option<std::sync::Arc<dyn Fn() -> Vec<(String, String, String)> + Send + Sync>>,
    pub task_notification_rx: Option<std_mpsc::Receiver<mossen_tools::task_store::TaskStoreEvent>>,

    /// Indices of `ToolUse` messages whose following `ToolResult` is
    /// currently collapsed (hidden from view). Press Space/Enter while a
    /// `ToolUse` row is focused to toggle. Tool-use blocks are auto-added
    /// here when their stream finishes so the default UX is a tidy log.
    pub collapsed_tool_groups: std::collections::HashSet<usize>,

    /// Index of the message currently receiving keyboard focus, or
    /// `None` when the prompt has focus instead. Up/Down arrows move it
    /// while the prompt is empty and no stream is active.
    pub focused_message_idx: Option<usize>,

    /// Images the user has pasted (Ctrl+V) but not yet submitted. Each
    /// entry is `(mime, base64)`. The prompt input shows a `[Image #N]`
    /// marker for each, and `handle_submit` folds them into the User
    /// message's content as `ContentBlock::Image` blocks so the API
    /// gets the actual bytes (not just the textual marker).
    pub pending_images: Vec<(String, String)>,
    /// Hook-provided context blocks that should be attached to the first
    /// model-bound user prompt, then drained. This keeps startup hook context
    /// in the model input without replaying it on every turn.
    pub startup_additional_blocks: Vec<ContentBlock>,

    /// Stashed TodoWrite tool input captured from `ToolUse` blocks in
    /// `SdkMessage::Assistant`. Applied when the corresponding
    /// `ToolUseSummary` arrives, so the TUI can update the task list
    /// state from the structured input (not the summary text).
    pub pending_todo_write_input: Option<serde_json::Value>,

    /// Last automatic render-session snapshot written at shutdown. This is
    /// render-layer persistence only; it does not imply engine/tool resume.
    pub render_snapshot_autosave_path: Option<PathBuf>,
    pub render_snapshot_autosave_error: Option<String>,
    /// Startup restore state for the latest render-session snapshot. This is
    /// render-layer hydration only; it never resumes engine/tool execution.
    pub startup_render_session_restore_enabled: bool,
    pub render_snapshot_startup_restore_status: RenderSnapshotStartupRestoreStatus,
    pub render_snapshot_startup_restore_path: Option<PathBuf>,
    pub render_snapshot_startup_restore_error: Option<String>,
    /// Project-local footer/status-line render config persistence. This is UI
    /// configuration only and does not affect engine/tool execution.
    pub footer_config_persistence_status: FooterConfigPersistenceStatus,
    pub footer_config_persistence_path: Option<PathBuf>,
    pub footer_config_persistence_error: Option<String>,
    pub external_statusline_output: Option<String>,
    pub external_statusline_error: Option<String>,
    pub external_statusline_in_flight: bool,
    pub external_statusline_next_sequence: u64,
    pub external_statusline_last_sequence: u64,
    pub external_statusline_last_started: Option<Instant>,
    external_statusline_result_tx: mpsc::UnboundedSender<ExternalStatusLineCommandResult>,
    external_statusline_result_rx: mpsc::UnboundedReceiver<ExternalStatusLineCommandResult>,
}

impl App {
    /// Create a new App instance without engine wiring (legacy path — keeps
    /// existing tests/screens working; will not actually call the model).
    pub fn new() -> Self {
        let state = AppState::default();
        let theme = Theme::from_env_for_name(state.theme);
        let cwd = std::env::current_dir()
            .ok()
            .and_then(|p| p.to_str().map(|s| s.to_string()))
            .unwrap_or_else(|| ".".to_string());
        let command_context = CommandContext {
            cwd: std::path::PathBuf::from(&cwd),
            is_non_interactive: false,
            is_remote_mode: false,
            is_custom_backend: false,
            user_type: None,
            env_vars: std::env::vars().collect(),
            product_name: "Mossen".to_string(),
            cli_name: "mossen".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            build_time: None,
            cost_snapshot: Default::default(),
        };
        let permission_rules = permission_rules_from_env(&command_context.env_vars);
        let (external_statusline_result_tx, external_statusline_result_rx) =
            mpsc::unbounded_channel();
        let (compact_result_tx, compact_result_rx) = mpsc::unbounded_channel();

        let mut app = Self {
            store: AppStore::new(state.clone()),
            state,
            theme,
            glyphs: RenderGlyphs::default(),
            prompt: PromptInputState::new(),
            spinner: SpinnerState::new(),
            messages: Vec::new(),
            approval_decisions: Vec::new(),
            next_render_record_seq: 0,
            next_render_turn_seq: 0,
            current_render_turn_id: None,
            render_record_id_overrides: HashMap::new(),
            render_record_parent_overrides: HashMap::new(),
            render_record_turn_overrides: HashMap::new(),
            render_event_history: Vec::new(),
            raw_engine_event_history: Vec::new(),
            next_raw_engine_event_seq: 0,
            scroll: VirtualScroll::new(24),
            message_content_area: None,
            message_scrollbar_area: None,
            modal_scrollbar_target: Cell::new(None),
            command_output_scroll: 0,
            render_height_cache: RenderHeightCache::default(),
            render_transcript_revision: 0,
            render_transcript_cache: RefCell::new(None),
            render_transcript_cache_hits: Cell::new(0),
            render_transcript_cache_misses: Cell::new(0),
            render_dirty: true,
            render_throttled_dirty_at: None,
            render_last_frame_at: None,
            render_last_frame_duration: None,
            render_max_frame_duration: Duration::ZERO,
            render_total_frame_duration: Duration::ZERO,
            render_frame_count: 0,
            render_skipped_frame_count: 0,
            started_at: Instant::now(),
            should_quit: false,
            fullscreen: true,
            engine_config: EngineConfig::from_env("example-default"),
            engine_rx: None,
            engine_session_id: None,
            engine_history: Vec::new(),
            pending_compact: None,
            compact_result_tx,
            compact_result_rx,
            next_compact_task_id: 0,
            active_compact_task_id: None,
            active_compact_cancel_token: None,
            pending_assistant_idx: None,
            pending_submit: None,
            assistant_buf: String::new(),
            pending_assistant_history_recorded: false,
            active_modal: ActiveModal::None,
            directives: None,
            command_context,
            services: crate::app_services::TerminalServices::new(),
            skill_registry: None,
            known_skill_names: std::collections::HashSet::new(),
            total_cost_usd: 0.0,
            permission_rx: None,
            active_permission_responder: None,
            permission_rules,
            additional_working_directories: Vec::new(),
            tool_registry: None,
            extra_tool_definitions: Vec::new(),
            mcp_reload_callback: None,
            show_all_thinking: false,
            task_snapshot_provider: None,
            task_notification_rx: None,
            collapsed_tool_groups: std::collections::HashSet::new(),
            focused_message_idx: None,
            pending_images: Vec::new(),
            startup_additional_blocks: Vec::new(),
            pending_todo_write_input: None,
            render_snapshot_autosave_path: None,
            render_snapshot_autosave_error: None,
            startup_render_session_restore_enabled: false,
            render_snapshot_startup_restore_status: RenderSnapshotStartupRestoreStatus::Pending,
            render_snapshot_startup_restore_path: None,
            render_snapshot_startup_restore_error: None,
            footer_config_persistence_status: FooterConfigPersistenceStatus::Pending,
            footer_config_persistence_path: None,
            footer_config_persistence_error: None,
            external_statusline_output: None,
            external_statusline_error: None,
            external_statusline_in_flight: false,
            external_statusline_next_sequence: 0,
            external_statusline_last_sequence: 0,
            external_statusline_last_started: None,
            external_statusline_result_tx,
            external_statusline_result_rx,
        };
        app.refresh_slash_catalog();
        app.refresh_mcp_statuses();
        app
    }

    /// Wire a TaskStore snapshot provider so Ctrl+T can dump live tasks.
    pub fn with_task_snapshot_provider(
        mut self,
        provider: std::sync::Arc<dyn Fn() -> Vec<(String, String, String)> + Send + Sync>,
    ) -> Self {
        self.task_snapshot_provider = Some(provider);
        self
    }

    pub fn with_task_notification_receiver(
        mut self,
        receiver: std_mpsc::Receiver<mossen_tools::task_store::TaskStoreEvent>,
    ) -> Self {
        self.task_notification_rx = Some(receiver);
        self
    }

    pub fn with_glyphs(mut self, glyphs: RenderGlyphs) -> Self {
        self.glyphs = glyphs;
        self
    }

    /// Ctrl+T helper — render the current TaskStore snapshot as a
    /// multi-line string. Uses the launcher-injected provider so TUI
    /// stays decoupled from `mossen-tools`.
    fn snapshot_task_list(&self) -> String {
        let Some(provider) = &self.task_snapshot_provider else {
            return "(task provider unavailable)".to_string();
        };
        let tasks = provider();
        if tasks.is_empty() {
            return "(no tasks in store)".to_string();
        }
        let mut out = String::from("Task store snapshot:\n");
        for (status, id, subject) in &tasks {
            out.push_str(&format!(" • [{}] {} — {}\n", status, id, subject));
        }
        out
    }

    fn poll_task_notifications(&mut self) {
        let mut notifications = Vec::new();
        if let Some(rx) = self.task_notification_rx.as_ref() {
            while let Ok(notification) = rx.try_recv() {
                notifications.push(notification);
                if notifications.len() >= 16 {
                    break;
                }
            }
        }

        for notification in notifications {
            self.push_task_completion_notification(notification);
        }
    }

    fn push_task_completion_notification(
        &mut self,
        notification: mossen_tools::task_store::TaskStoreEvent,
    ) {
        let status = notification.status.as_str();
        let action = match status {
            "completed" => "completed",
            "failed" => "failed",
            "cancelled" | "canceled" => "cancelled",
            "deleted" => "deleted",
            _ => return,
        };
        let title = if notification.task_type.as_deref() == Some("background_agent") {
            "Agent"
        } else {
            "Background task"
        };
        let mut content = format!(
            "{title} {action}: {}\nTask: {}\nUse /agents logs {} to inspect output.",
            compact_task_subject(&notification.subject),
            notification.id,
            notification.id
        );
        if let Some(code) = notification.exit_code {
            if status != "completed" || code != 0 {
                content.push_str(&format!("\nExit code: {code}"));
            }
        }
        self.push_system_message(content, status == "failed");
    }

    /// Ctrl+S helper — append the stash payload to a per-user file so it
    /// survives across sessions. Best-effort: any IO failure is silently
    /// swallowed so a missing cache dir doesn't crash the input flow.
    fn save_stash(&self, text: &str) {
        let Some(cache) = dirs::cache_dir() else {
            return;
        };
        let dir = cache.join("mossen");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("stash.txt");
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            let _ = writeln!(
                f,
                "---\n# {}\n{}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                text
            );
        }
    }

    /// Move keyboard focus across the message list. Steps are signed
    /// (+1 down / -1 up). Wraps to the first/last *visible* message —
    /// collapsed `ToolResult` rows are skipped because they're not
    /// rendered. Empty message list → no-op.
    fn move_focus(&mut self, delta: i32) {
        let n = self.messages.len();
        if n == 0 {
            return;
        }
        let mut idx = self.focused_message_idx.unwrap_or(n - 1) as i32;
        let mut tries = n as i32 * 2;
        loop {
            idx = (idx + delta).rem_euclid(n as i32);
            tries -= 1;
            if tries <= 0 {
                break;
            }
            if self.row_visible(idx as usize) {
                break;
            }
        }
        self.focused_message_idx = Some(idx as usize);
        self.ensure_focused_message_visible();
    }

    fn ensure_focused_message_visible(&mut self) {
        let Some(focused_idx) = self.focused_message_idx else {
            return;
        };
        let Some(area) = self.message_content_area else {
            return;
        };
        if area.width == 0 || area.height == 0 {
            return;
        }

        let surface = self.render_surface_model();
        let source_record_count = self
            .messages
            .len()
            .max(surface.transcript.source_record_count());
        let Some((target_start, target_end)) =
            MessagesWidget::content_row_range_for_source_index_from_transcript_with_cache_and_glyphs(
                focused_idx,
                source_record_count,
                &surface.transcript,
                &self.theme,
                area.width,
                self.show_all_thinking,
                &self.collapsed_tool_groups,
                Some(&self.render_height_cache),
                self.glyphs,
            )
        else {
            return;
        };

        let total_rows = self.message_total_rows(&surface, area.width);
        let viewport_rows = area.height as usize;
        if viewport_rows == 0 || total_rows <= viewport_rows {
            self.scroll.offset = 0;
            self.scroll.sticky = focused_idx + 1 >= self.messages.len();
            return;
        }

        let max_offset = total_rows.saturating_sub(viewport_rows);
        let current_start = if self.scroll.sticky {
            max_offset
        } else {
            self.scroll.offset.min(max_offset)
        };
        let current_end = current_start.saturating_add(viewport_rows);

        let mut target_offset = current_start;
        if target_start < current_start {
            target_offset = target_start;
        } else if target_end > current_end {
            target_offset = target_end.saturating_sub(viewport_rows);
        }

        self.scroll.offset = target_offset.min(max_offset);
        self.scroll.sticky =
            self.scroll.offset >= max_offset && focused_idx + 1 >= self.messages.len();
    }

    fn transcript_page_scroll_rows(&self) -> usize {
        self.message_content_area
            .map(|area| area.height as usize)
            .filter(|rows| *rows > 0)
            .unwrap_or_else(|| self.scroll.visible_count.max(1))
    }

    fn refresh_transcript_scroll_metrics_for_input(&mut self) {
        let Some(area) = self.message_content_area else {
            return;
        };
        if area.width == 0 || area.height == 0 {
            return;
        }

        let surface = self.render_surface_model();
        if surface.transcript.is_empty() {
            return;
        }

        self.scroll.set_viewport_height(area.height);
        let total_rows = self.message_total_rows(&surface, area.width);
        self.scroll.set_total_items(total_rows);
    }

    /// True when row `i` would be rendered — i.e. it isn't a
    /// `ToolResult` whose preceding `ToolUse` is in `collapsed_tool_groups`.
    fn row_visible(&self, i: usize) -> bool {
        let Some(msg) = self.messages.get(i) else {
            return false;
        };
        if matches!(msg.message_type, MessageType::ToolResult)
            && i > 0
            && matches!(self.messages[i - 1].message_type, MessageType::ToolUse)
            && self.collapsed_tool_groups.contains(&(i - 1))
        {
            return false;
        }
        true
    }

    /// Toggle the collapsed state of the currently-focused ToolUse row.
    /// Returns `true` when the press was consumed (caller short-circuits
    /// further key dispatch). When the focused row isn't a ToolUse the
    /// press falls through to the prompt.
    fn toggle_focused_group(&mut self) -> bool {
        let Some(idx) = self.focused_message_idx else {
            return false;
        };
        let Some(msg) = self.messages.get(idx) else {
            return false;
        };
        if !matches!(msg.message_type, MessageType::ToolUse) {
            return false;
        }
        if !self.collapsed_tool_groups.remove(&idx) {
            self.collapsed_tool_groups.insert(idx);
        }
        true
    }

    /// Toggle the `expanded` flag on the focused ToolResult row.
    /// `to_expanded` carries the desired state — Right key expands, Left
    /// collapses. Returns `true` when consumed.
    fn toggle_focused_expand(&mut self, to_expanded: bool) -> bool {
        let Some(idx) = self.focused_message_idx else {
            return false;
        };
        let Some(msg) = self.messages.get_mut(idx) else {
            return false;
        };
        if !matches!(msg.message_type, MessageType::ToolResult) {
            return false;
        }
        if msg.full_content.is_none() {
            return false;
        }
        msg.expanded = to_expanded;
        true
    }

    /// Read an image off the OS clipboard, base64-encode it, store as a
    /// pending paste, and insert a `[Image #N]` marker in the prompt.
    /// Returns `true` when an image was found and queued. The actual
    /// bytes ride along in `pending_images`; submission folds them into
    /// `ContentBlock::Image` so the model gets the multimodal payload,
    /// not just a textual marker.
    fn try_paste_image(&mut self) -> bool {
        let bytes = read_clipboard_image_bytes();
        let Some(bytes) = bytes else {
            return false;
        };
        // base64 encode for the data URI the API serializer expects.
        use base64::Engine;
        let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
        self.pending_images.push(("image/png".to_string(), encoded));
        let n = self.pending_images.len();
        self.prompt.input.insert_str(&format!("[Image #{}]", n));
        true
    }

    /// Ctrl+G helper — spawn `$EDITOR` (falls back to `vi`) on a temp file
    /// seeded with current prompt input. When the editor exits cleanly,
    /// replace the prompt with the edited content. Raw mode is suspended
    /// for the duration so vim/nano work normally.
    fn spawn_external_editor(&mut self) {
        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
        let tmp = std::env::temp_dir().join(format!("mossen-prompt-{}.txt", std::process::id()));
        if std::fs::write(&tmp, self.prompt.input.value.as_bytes()).is_err() {
            return;
        }
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::event::DisableMouseCapture,
            crossterm::terminal::LeaveAlternateScreen
        );
        let status = std::process::Command::new(&editor).arg(&tmp).status();
        let _ = crossterm::terminal::enable_raw_mode();
        let _ = crossterm::execute!(std::io::stdout(), crossterm::terminal::EnterAlternateScreen);
        if tui_mouse_capture_enabled() {
            let _ = crossterm::execute!(std::io::stdout(), crossterm::event::EnableMouseCapture);
        }
        if status.map(|s| s.success()).unwrap_or(false) {
            if let Ok(text) = std::fs::read_to_string(&tmp) {
                self.prompt.input.clear();
                self.prompt.input.insert_str(text.trim_end());
            }
        }
        let _ = std::fs::remove_file(&tmp);
    }

    /// Attach an executable tool registry built by the launcher. Without
    /// this the engine has no tools to call and the model falls back to
    /// describing actions as plain text.
    pub fn with_tool_registry(
        mut self,
        registry: std::sync::Arc<mossen_agent::tool_registry::ToolRegistry>,
    ) -> Self {
        self.tool_registry = Some(registry);
        self
    }

    /// Attach model-visible tool definitions that are executed outside the
    /// built-in registry. Used for dynamically connected MCP tools.
    pub fn with_extra_tool_definitions(mut self, tools: Vec<ToolDefinition>) -> Self {
        self.extra_tool_definitions = tools;
        self
    }

    pub fn with_mcp_reload_callback(mut self, callback: McpReloadCallback) -> Self {
        self.mcp_reload_callback = Some(callback);
        self
    }

    fn rebuild_tool_registry_for_mcp_resources(&mut self, enabled: bool) {
        let mut registry = mossen_agent::tool_registry::ToolRegistry::new();
        registry.register_all(mossen_tools::all_tools_for_runtime(
            mossen_tools::ToolRuntimeOptions {
                mcp_resources: enabled,
            },
        ));
        self.tool_registry = Some(Arc::new(registry));
    }

    fn reload_mcp_runtime_after_plugin_reload(
        &mut self,
    ) -> Option<Result<McpRuntimeReloadResult, String>> {
        let callback = self.mcp_reload_callback.as_ref()?.clone();
        let result = block_on_current_runtime((callback)());
        if let Ok(outcome) = &result {
            self.extra_tool_definitions = outcome.tool_definitions.clone();
            if outcome.server_count > 0 {
                self.rebuild_tool_registry_for_mcp_resources(true);
            }
            self.refresh_mcp_statuses();
        }
        Some(result)
    }

    /// Attach startup hook output to the first submitted prompt and surface
    /// hook messages in the transcript.
    pub fn with_startup_hook_messages(
        mut self,
        messages: Vec<mossen_utils::session_start::HookResultMessage>,
    ) -> Self {
        for message in messages {
            if message.message_type == "hook_additional_context" {
                self.startup_additional_blocks
                    .push(ContentBlock::Text(TextBlock {
                        text: message.content.clone(),
                    }));
            }

            let content = if message.message_type == "hook_additional_context" {
                format!("{} supplied startup context.", message.hook_name)
            } else {
                format!("{}: {}", message.hook_name, message.content)
            };
            self.push_system_message(content, false);
        }
        self
    }

    /// Enable or disable startup render-session snapshot hydration. Normal
    /// fresh launches keep this off so old transcript rows do not masquerade
    /// as the current session; explicit restore paths turn it on.
    pub fn with_startup_render_session_restore(mut self, enabled: bool) -> Self {
        self.startup_render_session_restore_enabled = enabled;
        self
    }

    /// Queue a prompt before the event loop starts. Used by startup hooks and
    /// restore/continue flows so the normal submit path still owns rendering,
    /// history, permissions, and tool dispatch.
    pub fn queue_startup_prompt(&mut self, prompt: String) {
        self.handle_submit(prompt);
    }

    /// Create a new App with engine + directive registry wired up. This is
    /// the production path: launcher builds an `EngineConfig` + the global
    /// `DirectiveRegistry` and passes them in here.
    pub fn with_engine(engine_config: EngineConfig, directives: Arc<Vec<BoxedDirective>>) -> Self {
        let mut app = Self::new();
        app.engine_config = engine_config;
        app.directives = Some(directives);
        app.refresh_slash_catalog();
        app.refresh_mcp_statuses();
        app
    }

    /// Inject a pre-built skill registry. Separated from [`with_engine`] so the
    /// caller can construct the registry lazily (e.g. discover skills on disk
    /// off the hot path) and attach it once ready.
    pub fn with_skill_registry(mut self, registry: mossen_skills::SharedCraftRegistry) -> Self {
        self.skill_registry = Some(registry);
        self.refresh_slash_catalog();
        self
    }

    fn refresh_slash_catalog(&mut self) {
        self.refresh_slash_catalog_with_notice(false);
    }

    fn refresh_slash_catalog_with_notice(&mut self, notify_new_skills: bool) {
        let mut entries = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut skill_names = std::collections::HashSet::new();

        if let Some(reg) = &self.directives {
            collect_directive_suggestions(
                reg.as_slice(),
                &self.command_context,
                &mut entries,
                &mut seen,
            );
        } else {
            let directives = mossen_commands::all_directives();
            collect_directive_suggestions(
                directives.as_slice(),
                &self.command_context,
                &mut entries,
                &mut seen,
            );
        }
        push_builtin_tui_suggestion(
            &mut entries,
            &mut seen,
            "raw",
            "Show the explicit raw transcript debug view",
        );
        push_builtin_tui_suggestion(
            &mut entries,
            &mut seen,
            "render-snapshot",
            "Export or validate the render session snapshot",
        );
        push_builtin_tui_suggestion(
            &mut entries,
            &mut seen,
            "snapshot",
            "Export or validate the render session snapshot",
        );
        push_builtin_tui_suggestion(
            &mut entries,
            &mut seen,
            "debug-config",
            "Inspect redacted runtime and renderer configuration",
        );
        push_builtin_tui_suggestion(
            &mut entries,
            &mut seen,
            "diff",
            "Open the semantic diff review viewer",
        );
        push_builtin_tui_suggestion(
            &mut entries,
            &mut seen,
            "files",
            "Inspect semantic file changes from this session",
        );
        push_builtin_tui_suggestion(
            &mut entries,
            &mut seen,
            "changes",
            "Inspect semantic file changes from this session",
        );
        push_builtin_tui_suggestion(
            &mut entries,
            &mut seen,
            "timeline",
            "Inspect structured render lifecycle events",
        );
        push_builtin_tui_suggestion(
            &mut entries,
            &mut seen,
            "events",
            "Inspect structured render lifecycle events",
        );
        push_builtin_tui_suggestion(
            &mut entries,
            &mut seen,
            "ps",
            "Inspect active turn processes and background activity",
        );
        push_builtin_tui_suggestion(
            &mut entries,
            &mut seen,
            "commands",
            "Inspect semantic command execution history",
        );
        push_builtin_tui_suggestion(
            &mut entries,
            &mut seen,
            "errors",
            "Inspect semantic errors and failed command details",
        );
        push_builtin_tui_suggestion(
            &mut entries,
            &mut seen,
            "results",
            "Inspect final task summaries and verification results",
        );
        push_builtin_tui_suggestion(
            &mut entries,
            &mut seen,
            "approvals",
            "Inspect approval requests and decisions",
        );
        push_builtin_tui_suggestion(
            &mut entries,
            &mut seen,
            "statusline",
            "Configure footer status-line items",
        );
        push_builtin_tui_suggestion(
            &mut entries,
            &mut seen,
            "title",
            "Inspect or set the terminal session title",
        );

        if let Some(registry) = &self.skill_registry {
            if let Ok(reg) = registry.read() {
                for craft in reg.all_crafts() {
                    if craft.is_user_invocable() {
                        skill_names.insert(craft.name().to_string());
                        push_skill_suggestion(craft, &mut entries, &mut seen);
                    }
                }
            }
        }
        for craft in mossen_skills::get_dynamic_skills() {
            if craft.is_user_invocable() {
                skill_names.insert(craft.name().to_string());
                push_skill_suggestion(&craft, &mut entries, &mut seen);
            }
        }
        for craft in mossen_skills::get_bundled_crafts() {
            if craft.is_user_invocable() {
                skill_names.insert(craft.name().to_string());
                push_skill_suggestion(&craft, &mut entries, &mut seen);
            }
        }

        if notify_new_skills {
            let mut newly_available: Vec<String> = skill_names
                .difference(&self.known_skill_names)
                .cloned()
                .collect();
            newly_available.sort();
            if !newly_available.is_empty() {
                self.push_system_message(
                    format!(
                        "Skill discovered: {}",
                        newly_available
                            .iter()
                            .map(|name| format!("/{}", name))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    false,
                );
            }
        }

        entries.sort_by(|a, b| {
            a.category
                .cmp(&b.category)
                .then_with(|| a.name.cmp(&b.name))
        });
        self.known_skill_names = skill_names;
        self.state.all_slash_commands = entries;
    }

    fn refresh_mcp_statuses(&mut self) {
        if let Some(runtime_statuses) =
            block_on_current_runtime(mossen_agent::mcp::runtime_status::snapshot())
        {
            if !runtime_statuses.is_empty() {
                self.state.mcp_servers = runtime_statuses
                    .into_iter()
                    .map(|status| McpServerStatus {
                        name: status.name,
                        state: mcp_runtime_state(status.state),
                        transport: status.transport,
                        tools_count: status.tools_count,
                        prompts_count: status.prompts_count,
                        resources_count: status.resources_count,
                        scope: status.scope,
                        last_error: status.last_error,
                    })
                    .collect();
                return;
            }
        }

        let configs = mossen_agent::mcp::config::get_project_mcp_configs_from_cwd();
        let mut statuses: Vec<McpServerStatus> = configs
            .into_iter()
            .map(|(name, scoped)| McpServerStatus {
                name,
                state: McpConnectionState::Configured,
                transport: mcp_transport_label(&scoped.config).to_string(),
                tools_count: 0,
                prompts_count: 0,
                resources_count: 0,
                scope: format!("{:?}", scoped.scope).to_lowercase(),
                last_error: None,
            })
            .collect();
        statuses.sort_by(|a, b| a.name.cmp(&b.name));
        self.state.mcp_servers = statuses;
    }

    fn push_system_message(&mut self, content: impl Into<String>, is_error: bool) {
        self.messages
            .push(system_transcript_message(content, is_error));
        self.note_transcript_changed();
    }

    fn note_transcript_changed(&mut self) {
        self.note_transcript_changed_for_refresh(RenderRefreshPolicy::Immediate);
    }

    fn note_transcript_changed_for_refresh(&mut self, refresh: RenderRefreshPolicy) {
        self.state.message_count = self.messages.len();
        self.render_transcript_revision = self.render_transcript_revision.saturating_add(1);
        self.render_transcript_cache.borrow_mut().take();
        self.mark_render_dirty_for_refresh(refresh);
        if self
            .focused_message_idx
            .is_some_and(|idx| idx >= self.messages.len())
        {
            self.focused_message_idx = self.messages.len().checked_sub(1);
        }
    }

    fn apply_main_render_events(&mut self, events: impl IntoIterator<Item = RenderEvent>) {
        for event in events {
            if event.scope.is_main() {
                let event = self.render_event_with_current_turn(event);
                self.apply_render_event(&event);
            }
        }
    }

    fn render_event_with_current_turn(&self, mut event: RenderEvent) -> RenderEvent {
        if event.turn_id.is_none() {
            event.turn_id = self.current_render_turn_id.clone();
        }
        event
    }

    fn apply_render_event(&mut self, event: &RenderEvent) {
        let mut event = event.clone();
        if event.scope.is_main() && event.turn_id.is_none() {
            event.turn_id = self.current_render_turn_id.clone();
        }
        self.render_event_history.push(event.clone());
        let overflow = self
            .render_event_history
            .len()
            .saturating_sub(MAX_RENDER_EVENT_HISTORY);
        if overflow > 0 {
            self.render_event_history.drain(..overflow);
        }
        if event.stage != UiStage::Idle {
            self.state.ui_stage = event.stage;
        }
        if matches!(&event.kind, RenderEventKind::TurnStarted) {
            self.state.render_activity.clear();
        }
        if let Some(activity) = render_activity_from_event(&event) {
            self.state.render_activity.set(activity);
        }
        if event.refresh.is_immediate() {
            self.spinner.mark_activity();
        }
        self.mark_render_dirty_for_refresh(event.refresh);
    }

    fn record_raw_engine_event(&mut self, msg: &SdkMessage) {
        self.next_raw_engine_event_seq = self.next_raw_engine_event_seq.saturating_add(1);
        let turn_id = if msg.task_id().is_none() {
            self.current_render_turn_id.clone()
        } else {
            None
        };
        let record =
            RawEngineEventRecord::from_sdk_message(self.next_raw_engine_event_seq, turn_id, msg);
        self.raw_engine_event_history.push(record);
        let overflow = self
            .raw_engine_event_history
            .len()
            .saturating_sub(MAX_RAW_ENGINE_EVENT_HISTORY);
        if overflow > 0 {
            self.raw_engine_event_history.drain(..overflow);
        }
    }

    fn record_approval_decision(
        &mut self,
        tool_name: impl Into<String>,
        decision: ApprovalDecisionKind,
        detail: impl Into<String>,
        anchor_block_id: Option<String>,
    ) {
        let id = self.allocate_render_record_id("approval-decision");
        let tool_name = tool_name.into();
        let model = ApprovalDecisionModel {
            id,
            tool_name: tool_name.clone(),
            decision,
            detail: detail.into(),
            anchor_block_id,
        };
        self.approval_decisions.push(model);
        self.note_transcript_changed();
    }

    fn record_final_summary(&mut self, terminal: Option<&str>) {
        let terminal = terminal.unwrap_or("Completed");
        let id = self.allocate_render_record_id("final-summary");
        let model = final_summary_model_from_messages(id, terminal, &self.messages);
        if !final_summary_should_record(&model) {
            return;
        }
        let source_index = self.messages.len();
        self.set_render_record_current_turn_override(source_index);
        self.messages.push(final_summary_transcript_message(&model));
        self.note_transcript_changed();
    }

    fn allocate_render_record_id(&mut self, prefix: &str) -> String {
        self.next_render_record_seq = self.next_render_record_seq.saturating_add(1);
        format!("{}-{}", prefix, self.next_render_record_seq)
    }

    fn ensure_current_render_turn_id(&mut self) -> String {
        if let Some(turn_id) = self.current_render_turn_id.clone() {
            return turn_id;
        }
        self.next_render_turn_seq = self.next_render_turn_seq.saturating_add(1);
        let turn_id = format!("turn-{seq:04}", seq = self.next_render_turn_seq);
        self.current_render_turn_id = Some(turn_id.clone());
        turn_id
    }

    fn clear_current_render_turn_id(&mut self) {
        self.current_render_turn_id = None;
    }

    fn set_render_record_id_override(&mut self, source_index: usize, id: impl Into<String>) {
        let id = id.into();
        if !id.is_empty() {
            self.render_record_id_overrides.insert(source_index, id);
        }
    }

    fn set_render_record_parent_override(
        &mut self,
        source_index: usize,
        parent_id: impl Into<String>,
    ) {
        let parent_id = parent_id.into();
        if !parent_id.is_empty() {
            self.render_record_parent_overrides
                .insert(source_index, parent_id);
        }
    }

    fn set_render_record_turn_override(&mut self, source_index: usize, turn_id: impl Into<String>) {
        let turn_id = turn_id.into();
        if !turn_id.is_empty() {
            self.render_record_turn_overrides
                .insert(source_index, turn_id);
        }
    }

    fn set_render_record_current_turn_override(&mut self, source_index: usize) {
        let turn_id = self.ensure_current_render_turn_id();
        self.set_render_record_turn_override(source_index, turn_id);
    }

    fn latest_tool_record_id_for_result(&self, tool_name: &str) -> Option<String> {
        self.messages
            .iter()
            .enumerate()
            .rev()
            .find(|(_, message)| {
                matches!(message.message_type, MessageType::ToolUse)
                    && message
                        .tool_name
                        .as_deref()
                        .is_some_and(|name| name == tool_name)
            })
            .map(|(index, _)| {
                self.render_record_id_overrides
                    .get(&index)
                    .cloned()
                    .unwrap_or_else(|| format!("tool-{index}"))
            })
    }

    fn truncate_render_record_id_overrides(&mut self, len: usize) {
        self.render_record_id_overrides
            .retain(|source_index, _| *source_index < len);
        self.render_record_parent_overrides
            .retain(|source_index, _| *source_index < len);
        self.render_record_turn_overrides
            .retain(|source_index, _| *source_index < len);
    }

    fn remove_render_record_id_override_at(&mut self, removed_index: usize) {
        shift_render_record_overrides(&mut self.render_record_id_overrides, removed_index);
        shift_render_record_overrides(&mut self.render_record_parent_overrides, removed_index);
        shift_render_record_overrides(&mut self.render_record_turn_overrides, removed_index);
    }

    fn clear_render_record_state(&mut self) {
        self.approval_decisions.clear();
        self.render_record_id_overrides.clear();
        self.render_record_parent_overrides.clear();
        self.render_record_turn_overrides.clear();
        self.raw_engine_event_history.clear();
        self.next_render_record_seq = 0;
        self.next_render_turn_seq = 0;
        self.next_raw_engine_event_seq = 0;
        self.current_render_turn_id = None;
    }

    fn render_transcript_model(&self) -> RenderTranscript {
        let key = self.render_transcript_cache_key();
        if let Some(transcript) = {
            let cache = self.render_transcript_cache.borrow();
            cache
                .as_ref()
                .filter(|cached| cached.key == key)
                .map(|cached| cached.transcript.clone())
        } {
            self.render_transcript_cache_hits
                .set(self.render_transcript_cache_hits.get().saturating_add(1));
            return transcript;
        }

        let records = TranscriptRecords::from_messages_and_decisions_with_full_record_metadata(
            &self.messages,
            &self.approval_decisions,
            &self.render_record_id_overrides,
            &self.render_record_parent_overrides,
            &self.render_record_turn_overrides,
        );
        let transcript = RenderTranscript::from_records(&records);
        self.render_transcript_cache_misses
            .set(self.render_transcript_cache_misses.get().saturating_add(1));
        self.render_transcript_cache
            .borrow_mut()
            .replace(CachedRenderTranscript {
                key,
                transcript: transcript.clone(),
            });
        transcript
    }

    fn render_transcript_cache_key(&self) -> RenderTranscriptCacheKey {
        RenderTranscriptCacheKey {
            revision: self.render_transcript_revision,
            messages_len: self.messages.len(),
            approvals_len: self.approval_decisions.len(),
            id_overrides_len: self.render_record_id_overrides.len(),
            parent_overrides_len: self.render_record_parent_overrides.len(),
            turn_overrides_len: self.render_record_turn_overrides.len(),
            message_shape: self.render_transcript_message_shape(),
        }
    }

    fn render_transcript_message_shape(&self) -> u64 {
        self.messages
            .iter()
            .fold(0xcbf2_9ce4_8422_2325, |acc, message| {
                let mut acc =
                    render_cache_shape_mix(acc, message_type_shape_key(message.message_type));
                acc = render_cache_shape_mix(acc, message.content.len() as u64);
                acc = render_cache_shape_mix(
                    acc,
                    message
                        .tool_name
                        .as_ref()
                        .map(|value| value.len() as u64)
                        .unwrap_or(0),
                );
                acc = render_cache_shape_mix(
                    acc,
                    message
                        .thinking
                        .as_ref()
                        .map(|value| value.len() as u64)
                        .unwrap_or(0),
                );
                acc = render_cache_shape_mix(
                    acc,
                    message
                        .full_content
                        .as_ref()
                        .map(|value| value.len() as u64)
                        .unwrap_or(0),
                );
                acc = render_cache_shape_mix(acc, u64::from(message.is_streaming));
                acc = render_cache_shape_mix(acc, u64::from(message.is_error));
                render_cache_shape_mix(acc, u64::from(message.expanded))
            })
    }

    fn render_transcript_cache_stats(&self) -> RenderTranscriptCacheStats {
        let key = self.render_transcript_cache_key();
        let cached = self
            .render_transcript_cache
            .borrow()
            .as_ref()
            .is_some_and(|cached| cached.key == key);
        RenderTranscriptCacheStats {
            revision: self.render_transcript_revision,
            cached,
            hits: self.render_transcript_cache_hits.get(),
            misses: self.render_transcript_cache_misses.get(),
        }
    }

    fn mark_render_dirty(&mut self) {
        self.render_dirty = true;
        self.render_throttled_dirty_at = None;
    }

    fn mark_render_dirty_for_refresh(&mut self, refresh: RenderRefreshPolicy) {
        match refresh {
            RenderRefreshPolicy::Immediate => self.mark_render_dirty(),
            RenderRefreshPolicy::Throttled { min_interval_ms } => {
                self.mark_render_dirty_throttled(Duration::from_millis(min_interval_ms))
            }
            RenderRefreshPolicy::Passive => {
                if self.render_last_frame_at.is_none() {
                    self.mark_render_dirty();
                }
            }
        }
    }

    fn mark_render_dirty_throttled(&mut self, min_interval: Duration) {
        if self.render_dirty || self.render_last_frame_at.is_none() {
            self.mark_render_dirty();
            return;
        }

        let adaptive_interval = self
            .render_last_frame_duration
            .unwrap_or(Duration::ZERO)
            .saturating_mul(2)
            .min(MAX_ACTIVE_RENDER_FRAME_INTERVAL);
        let interval = min_interval.max(adaptive_interval);
        if let Some(last) = self.render_last_frame_at {
            if last.elapsed() >= interval {
                self.mark_render_dirty();
            } else {
                self.render_throttled_dirty_at = Some(last + interval);
            }
        }
    }

    fn streaming_render_refresh_policy() -> RenderRefreshPolicy {
        RenderRefreshPolicy::Throttled {
            min_interval_ms: STREAM_THROTTLE_MS,
        }
    }

    fn should_render_frame_for_run(&self) -> bool {
        if self.render_dirty || self.render_last_frame_at.is_none() {
            return true;
        }
        if self
            .next_render_frame_due_at()
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return true;
        }
        false
    }

    fn next_render_frame_due_at(&self) -> Option<Instant> {
        if self.render_dirty || self.render_last_frame_at.is_none() {
            return Some(Instant::now());
        }

        let mut due_at = self.render_throttled_dirty_at;
        if !self.has_active_render_animation() {
            return due_at;
        }

        if let Some(last) = self.render_last_frame_at {
            let active_due_at = last + self.active_render_frame_interval();
            due_at = Some(match due_at {
                Some(throttled_due_at) => throttled_due_at.min(active_due_at),
                None => active_due_at,
            });
        }
        due_at
    }

    fn note_render_frame_drawn(&mut self) {
        self.note_render_frame_drawn_with_duration(Duration::ZERO);
    }

    fn note_render_frame_drawn_with_duration(&mut self, duration: Duration) {
        self.render_dirty = false;
        self.render_throttled_dirty_at = None;
        self.render_last_frame_at = Some(Instant::now());
        self.render_last_frame_duration = Some(duration);
        self.render_max_frame_duration = self.render_max_frame_duration.max(duration);
        self.render_total_frame_duration =
            self.render_total_frame_duration.saturating_add(duration);
        self.render_frame_count = self.render_frame_count.saturating_add(1);
    }

    fn note_render_frame_skipped(&mut self) {
        self.render_skipped_frame_count = self.render_skipped_frame_count.saturating_add(1);
    }

    fn has_active_render_animation(&self) -> bool {
        self.state.is_streaming
            || self.state.is_waiting_for_response
            || self.engine_rx.is_some()
            || self.pending_submit.is_some()
            || self.state.compact_in_progress
            || self.pending_compact.is_some()
            || self.active_compact_task_id.is_some()
    }

    fn active_render_frame_interval(&self) -> Duration {
        self.render_last_frame_duration
            .unwrap_or(Duration::ZERO)
            .saturating_mul(2)
            .max(ACTIVE_RENDER_FRAME_INTERVAL)
            .min(MAX_ACTIVE_RENDER_FRAME_INTERVAL)
    }

    fn render_frame_scheduler_stats(&self) -> RenderFrameSchedulerStats {
        RenderFrameSchedulerStats {
            dirty: self.render_dirty,
            throttled_due_in_ms: self.render_throttled_dirty_at.map(|deadline| {
                deadline
                    .saturating_duration_since(Instant::now())
                    .as_millis()
            }),
            next_frame_due_in_ms: self.next_render_frame_due_at().map(|deadline| {
                deadline
                    .saturating_duration_since(Instant::now())
                    .as_millis()
            }),
            active_animation: self.has_active_render_animation(),
            drawn: self.render_frame_count,
            skipped: self.render_skipped_frame_count,
            last_frame_age_ms: self
                .render_last_frame_at
                .map(|instant| instant.elapsed().as_millis()),
            last_frame_duration_ms: self
                .render_last_frame_duration
                .map(|duration| duration.as_millis()),
            max_frame_duration_ms: self.render_max_frame_duration.as_millis(),
            avg_frame_duration_ms: (self.render_frame_count > 0).then(|| {
                self.render_total_frame_duration.as_millis() / u128::from(self.render_frame_count)
            }),
            active_frame_interval_ms: self.active_render_frame_interval().as_millis(),
        }
    }

    fn render_tick_fingerprint(&self) -> u64 {
        let mut acc = 0xcbf2_9ce4_8422_2325;
        acc = render_cache_shape_mix(acc, self.messages.len() as u64);
        acc = render_cache_shape_mix(acc, self.state.message_count as u64);
        acc = render_cache_shape_mix(acc, self.render_event_history.len() as u64);
        acc = render_cache_shape_mix(acc, self.state.all_slash_commands.len() as u64);
        acc = render_cache_shape_mix(acc, self.known_skill_names.len() as u64);
        acc = render_cache_shape_mix(acc, active_modal_shape_key(&self.active_modal));
        acc = render_cache_shape_mix(acc, ui_stage_shape_key(self.state.ui_stage));
        acc = render_cache_shape_mix(acc, turn_state_shape_key(self.state.turn_state));
        acc = render_cache_shape_mix(acc, u64::from(self.state.is_streaming));
        acc = render_cache_shape_mix(acc, u64::from(self.state.is_waiting_for_response));
        acc = render_cache_shape_mix(acc, u64::from(self.state.compact_in_progress));
        acc = render_cache_shape_mix(
            acc,
            option_string_shape(self.state.compact_progress.as_ref()),
        );
        acc = render_cache_shape_mix(acc, self.state.mcp_servers.len() as u64);
        acc = render_cache_shape_mix(
            acc,
            option_string_shape(self.external_statusline_output.as_ref()),
        );
        acc = render_cache_shape_mix(
            acc,
            option_string_shape(self.external_statusline_error.as_ref()),
        );
        if matches!(self.active_modal, ActiveModal::DebugConfig(_)) {
            acc = render_cache_shape_mix(acc, u64::from(self.external_statusline_in_flight));
        }
        acc = render_cache_shape_mix(acc, option_string_shape(Some(&self.services.title.title)));
        acc = render_cache_shape_mix(acc, option_string_shape(self.services.tab.status.as_ref()));
        acc = render_cache_shape_mix(
            acc,
            option_string_shape(self.services.tab.indicator.as_ref()),
        );
        acc = render_cache_shape_mix(acc, u64::from(self.services.idle_dialog_shown));
        acc = render_cache_shape_mix(acc, u64::from(self.services.was_streaming));
        acc = render_cache_shape_mix(acc, u64::from(self.services.cost_threshold_state.is_some()));
        acc = render_cache_shape_mix(acc, u64::from(self.services.idle_return_state.is_some()));
        acc = render_cache_shape_mix(
            acc,
            u64::from(self.services.message_selector_state.is_some()),
        );
        render_cache_shape_mix(acc, u64::from(self.services.search_panel_state.is_some()))
    }

    fn render_session_snapshot(&self) -> RenderSessionSnapshot {
        let records = TranscriptRecords::from_messages_and_decisions_with_full_record_metadata(
            &self.messages,
            &self.approval_decisions,
            &self.render_record_id_overrides,
            &self.render_record_parent_overrides,
            &self.render_record_turn_overrides,
        );
        self.render_session_snapshot_from_records(records)
    }

    fn render_session_snapshot_from_records(
        &self,
        records: TranscriptRecords,
    ) -> RenderSessionSnapshot {
        RenderSessionSnapshot::new(
            self.engine_session_id.clone(),
            self.current_render_turn_id.clone(),
            self.latest_render_turn_id().map(ToOwned::to_owned),
            self.next_render_record_seq,
            self.next_render_turn_seq,
            self.next_raw_engine_event_seq,
            records,
            self.raw_engine_event_history.clone(),
        )
    }

    fn save_render_session_snapshot(
        &self,
        path: impl AsRef<Path>,
    ) -> std::io::Result<RenderSessionSnapshot> {
        let snapshot = self.render_session_snapshot();
        snapshot.save_json_file(path)?;
        Ok(snapshot)
    }

    pub fn autosave_render_session_snapshot(&mut self) -> std::io::Result<Option<PathBuf>> {
        if !self.has_render_session_snapshot_content() {
            self.render_snapshot_autosave_path = None;
            self.render_snapshot_autosave_error = None;
            return Ok(None);
        }

        let path = self.default_render_session_snapshot_path();
        match self.save_render_session_snapshot(&path) {
            Ok(_) => {
                self.render_snapshot_autosave_path = Some(path.clone());
                self.render_snapshot_autosave_error = None;
                Ok(Some(path))
            }
            Err(error) => {
                self.render_snapshot_autosave_path = Some(path);
                self.render_snapshot_autosave_error = Some(error.to_string());
                Err(error)
            }
        }
    }

    fn autosave_render_session_snapshot_best_effort(&mut self) {
        if let Err(error) = self.autosave_render_session_snapshot() {
            tracing::warn!(
                error = %error,
                "failed to autosave render session snapshot"
            );
        }
    }

    fn has_render_session_snapshot_content(&self) -> bool {
        !self.messages.is_empty()
            || !self.raw_engine_event_history.is_empty()
            || !self.approval_decisions.is_empty()
    }

    fn load_render_session_snapshot(
        path: impl AsRef<Path>,
    ) -> std::io::Result<RenderSessionSnapshot> {
        RenderSessionSnapshot::load_json_file(path)
    }

    fn default_footer_render_config_path(&self) -> PathBuf {
        PathBuf::from(&self.engine_config.cwd).join(RENDER_STATUSLINE_CONFIG_PATH)
    }

    pub fn load_footer_render_config_on_startup(&mut self) -> std::io::Result<Option<PathBuf>> {
        let path = self.default_footer_render_config_path();
        self.footer_config_persistence_path = Some(path.clone());
        self.footer_config_persistence_error = None;

        if self.state.footer_config != FooterRenderConfig::default() {
            self.footer_config_persistence_status = FooterConfigPersistenceStatus::Skipped;
            return Ok(None);
        }

        self.load_footer_render_config_from_path(path)
    }

    pub fn load_footer_render_config_from_default_path(
        &mut self,
    ) -> std::io::Result<Option<PathBuf>> {
        let path = self.default_footer_render_config_path();
        self.load_footer_render_config_from_path(path)
    }

    fn load_footer_render_config_from_path(
        &mut self,
        path: PathBuf,
    ) -> std::io::Result<Option<PathBuf>> {
        self.footer_config_persistence_path = Some(path.clone());
        self.footer_config_persistence_error = None;

        match Self::load_footer_render_config_file(&path) {
            Ok(config) => {
                self.state.footer_config = config;
                self.reset_external_statusline_runtime();
                self.footer_config_persistence_status = FooterConfigPersistenceStatus::Loaded;
                Ok(Some(path))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                self.footer_config_persistence_status = FooterConfigPersistenceStatus::Unavailable;
                Ok(None)
            }
            Err(error) => {
                self.footer_config_persistence_status = FooterConfigPersistenceStatus::Error;
                self.footer_config_persistence_error = Some(error.to_string());
                Err(error)
            }
        }
    }

    fn save_footer_render_config_to_default_path(&mut self) -> std::io::Result<PathBuf> {
        let path = self.default_footer_render_config_path();
        self.footer_config_persistence_path = Some(path.clone());
        match Self::save_footer_render_config_file(&path, &self.state.footer_config) {
            Ok(()) => {
                self.footer_config_persistence_status = FooterConfigPersistenceStatus::Saved;
                self.footer_config_persistence_error = None;
                Ok(path)
            }
            Err(error) => {
                self.footer_config_persistence_status = FooterConfigPersistenceStatus::Error;
                self.footer_config_persistence_error = Some(error.to_string());
                Err(error)
            }
        }
    }

    fn set_footer_render_config_and_persist(&mut self, config: FooterRenderConfig) {
        self.state.footer_config = config;
        self.reset_external_statusline_runtime();
        let _ = self.save_footer_render_config_to_default_path();
    }

    fn toggle_footer_render_item_and_persist(&mut self, item: FooterItem) {
        self.state.footer_config.toggle(item);
        let _ = self.save_footer_render_config_to_default_path();
    }

    fn reset_external_statusline_runtime(&mut self) {
        self.external_statusline_next_sequence =
            self.external_statusline_next_sequence.saturating_add(1);
        self.external_statusline_last_sequence = self.external_statusline_next_sequence;
        self.external_statusline_in_flight = false;
        self.external_statusline_last_started = None;
        self.external_statusline_error = None;
        if self.state.footer_config.external_command.is_none() {
            self.external_statusline_output = None;
        }
    }

    fn poll_external_statusline_command(&mut self) {
        while let Ok(result) = self.external_statusline_result_rx.try_recv() {
            if result.sequence != self.external_statusline_last_sequence {
                continue;
            }
            self.external_statusline_in_flight = false;
            self.external_statusline_error = result.error;
            if let Some(output) = result.output {
                self.external_statusline_output = Some(output);
            }
        }

        let Some(config) = self.state.footer_config.external_command.clone() else {
            self.external_statusline_in_flight = false;
            return;
        };
        if config.command.trim().is_empty() || self.external_statusline_in_flight {
            return;
        }

        let now = Instant::now();
        let interval_ms = config.interval_ms.clamp(250, 60_000);
        if let Some(last_started) = self.external_statusline_last_started {
            if now.duration_since(last_started) < Duration::from_millis(interval_ms) {
                return;
            }
        }

        let sequence = self.external_statusline_next_sequence.saturating_add(1);
        self.external_statusline_next_sequence = sequence;
        self.external_statusline_last_sequence = sequence;
        self.external_statusline_last_started = Some(now);
        self.external_statusline_in_flight = true;

        let tx = self.external_statusline_result_tx.clone();
        let input = self.external_statusline_input();
        let cwd = self.engine_config.cwd.clone();
        let env_vars = self.command_context.env_vars.clone();
        tokio::spawn(async move {
            let (output, error) =
                run_external_statusline_command(config, input, cwd, env_vars).await;
            let _ = tx.send(ExternalStatusLineCommandResult {
                sequence,
                output,
                error,
            });
        });
    }

    fn external_statusline_input(&self) -> serde_json::Value {
        let context = self.context_usage_render_model();
        serde_json::json!({
            "session_id": self.engine_session_id.as_deref(),
            "cwd": self.engine_config.cwd.as_str(),
            "model": self.engine_config.model.as_str(),
            "permission_mode": self.current_permission_mode_label(),
            "turn_state": self.turn_state_label(),
            "ui_stage": self.state.ui_stage.label(),
            "activity": self.state.render_activity.status_line(),
            "message_count": self.messages.len(),
            "cost_usd": self.total_cost_usd,
            "context": context.map(|value| serde_json::json!({
                "used_tokens": value.used_tokens,
                "window_tokens": value.window_tokens,
                "label": value.label(),
            })),
        })
    }

    fn current_permission_mode_label(&self) -> String {
        permission_mode_display_label(
            self.command_context
                .env_vars
                .get(PERMISSION_MODE_ENV)
                .map(String::as_str),
        )
    }

    fn current_permission_mode_picker_index(&self) -> usize {
        permission_mode_choice_index(
            self.command_context
                .env_vars
                .get(PERMISSION_MODE_ENV)
                .map(String::as_str),
        )
    }

    fn current_permission_mode_code(&self) -> &'static str {
        permission_mode_code_for_raw(
            self.command_context
                .env_vars
                .get(PERMISSION_MODE_ENV)
                .map(String::as_str),
        )
    }

    fn load_footer_render_config_file(
        path: impl AsRef<Path>,
    ) -> std::io::Result<FooterRenderConfig> {
        let payload = std::fs::read_to_string(path)?;
        parse_footer_render_config_payload(&payload).map_err(app_json_to_io_error)
    }

    fn save_footer_render_config_file(
        path: impl AsRef<Path>,
        config: &FooterRenderConfig,
    ) -> std::io::Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)?;
        }

        let payload = serde_json::to_string_pretty(config).map_err(app_json_to_io_error)?;
        let tmp_path = footer_render_config_tmp_path(path);
        {
            let mut file = std::fs::File::create(&tmp_path)?;
            file.write_all(payload.as_bytes())?;
            file.write_all(b"\n")?;
            file.sync_all()?;
        }
        std::fs::rename(&tmp_path, path).inspect_err(|error| {
            let _ = std::fs::remove_file(&tmp_path);
        })
    }

    pub fn restore_latest_render_session_snapshot_on_startup(
        &mut self,
    ) -> std::io::Result<Option<PathBuf>> {
        self.render_snapshot_startup_restore_path = None;
        self.render_snapshot_startup_restore_error = None;

        if !self.can_restore_render_session_snapshot_on_startup() {
            self.render_snapshot_startup_restore_status =
                RenderSnapshotStartupRestoreStatus::Skipped;
            return Ok(None);
        }

        let path = match self.latest_render_session_snapshot_path() {
            Ok(Some(path)) => path,
            Ok(None) => {
                self.render_snapshot_startup_restore_status =
                    RenderSnapshotStartupRestoreStatus::Unavailable;
                return Ok(None);
            }
            Err(error) => {
                self.render_snapshot_startup_restore_status =
                    RenderSnapshotStartupRestoreStatus::Error;
                self.render_snapshot_startup_restore_error = Some(error.to_string());
                return Err(error);
            }
        };

        match Self::load_render_session_snapshot(&path) {
            Ok(snapshot) => {
                self.restore_render_session_snapshot(&snapshot);
                self.render_snapshot_startup_restore_status =
                    RenderSnapshotStartupRestoreStatus::Restored;
                self.render_snapshot_startup_restore_path = Some(path.clone());
                self.render_snapshot_startup_restore_error = None;
                Ok(Some(path))
            }
            Err(error) => {
                self.render_snapshot_startup_restore_status =
                    RenderSnapshotStartupRestoreStatus::Error;
                self.render_snapshot_startup_restore_path = Some(path);
                self.render_snapshot_startup_restore_error = Some(error.to_string());
                Err(error)
            }
        }
    }

    fn can_restore_render_session_snapshot_on_startup(&self) -> bool {
        !self.has_render_session_snapshot_content()
            && self.engine_session_id.is_none()
            && self.engine_history.is_empty()
            && self.pending_submit.is_none()
            && self.pending_compact.is_none()
            && self.active_compact_task_id.is_none()
            && self.pending_assistant_idx.is_none()
            && self.engine_rx.is_none()
    }

    fn restore_render_session_snapshot(&mut self, snapshot: &RenderSessionSnapshot) {
        struct RestoredRow {
            source_index: usize,
            order: usize,
            record_id: Option<String>,
            parent_id: Option<String>,
            turn_id: Option<String>,
            message: MessageData,
        }

        let mut rows = Vec::new();
        for (order, record) in snapshot.records.entries.iter().enumerate() {
            rows.push(RestoredRow {
                source_index: record.source_index,
                order,
                record_id: Some(record.id.clone()).filter(|id| !id.is_empty()),
                parent_id: record.parent_id.clone().filter(|id| !id.is_empty()),
                turn_id: record.turn_id.clone().filter(|id| !id.is_empty()),
                message: record.to_message_data(),
            });
        }
        let summary_order_start = rows.len();
        for (offset, summary) in snapshot.records.final_summaries.iter().enumerate() {
            rows.push(RestoredRow {
                source_index: summary.source_index,
                order: summary_order_start + offset,
                record_id: None,
                parent_id: None,
                turn_id: None,
                message: final_summary_message_data(&summary.model),
            });
        }
        rows.sort_by_key(|row| (row.source_index, row.order));

        self.messages.clear();
        self.approval_decisions = snapshot.records.approval_decisions.clone();
        self.render_record_id_overrides.clear();
        self.render_record_parent_overrides.clear();
        self.render_record_turn_overrides.clear();
        self.collapsed_tool_groups.clear();
        self.focused_message_idx = None;
        self.pending_assistant_idx = None;
        self.pending_submit = None;
        self.assistant_buf.clear();
        self.engine_history.clear();
        self.engine_rx = None;
        self.pending_compact = None;
        self.active_compact_task_id = None;
        if let Some(token) = self.active_compact_cancel_token.take() {
            token.cancel();
        }
        self.permission_rx = None;
        self.active_permission_responder = None;

        for row in rows {
            let index = self.messages.len();
            if let Some(record_id) = row.record_id {
                self.set_render_record_id_override(index, record_id);
            }
            if let Some(parent_id) = row.parent_id {
                self.set_render_record_parent_override(index, parent_id);
            }
            if let Some(turn_id) = row.turn_id {
                self.set_render_record_turn_override(index, turn_id);
            }
            self.messages.push(row.message);
        }

        self.engine_session_id = snapshot.session_id.clone();
        self.current_render_turn_id = snapshot.current_turn_id.clone();
        self.next_render_record_seq = snapshot.next_render_record_seq;
        self.next_render_turn_seq = snapshot.next_render_turn_seq;
        self.next_raw_engine_event_seq = snapshot.next_raw_engine_event_seq;
        self.raw_engine_event_history = snapshot.raw_engine_events.clone();
        self.render_event_history.clear();
        self.render_height_cache.clear();
        self.scroll.scroll_to_bottom();

        let restored_streaming = snapshot.current_turn_id.is_some()
            || self.messages.iter().any(|message| message.is_streaming);
        self.state.is_streaming = restored_streaming;
        self.state.is_waiting_for_response = restored_streaming;
        self.state.turn_state = if restored_streaming {
            TurnState::Streaming
        } else {
            TurnState::Idle
        };
        self.state.ui_stage = restored_snapshot_ui_stage(snapshot, restored_streaming);
        self.state.render_activity.clear();
        self.note_transcript_changed();
    }

    fn default_render_session_snapshot_path(&self) -> PathBuf {
        let file_stem = render_session_snapshot_file_stem(
            self.engine_session_id
                .as_deref()
                .or(self.current_render_turn_id.as_deref()),
        );
        self.render_session_snapshot_dir_path()
            .join(format!("{file_stem}.json"))
    }

    fn render_session_snapshot_dir_path(&self) -> PathBuf {
        PathBuf::from(&self.engine_config.cwd).join(RENDER_SESSION_SNAPSHOT_DIR)
    }

    fn latest_render_session_snapshot_path(&self) -> std::io::Result<Option<PathBuf>> {
        let dir = self.render_session_snapshot_dir_path();
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };

        let mut latest: Option<(std::time::SystemTime, PathBuf)> = None;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if !path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
            {
                continue;
            }
            let metadata = entry.metadata()?;
            if !metadata.is_file() {
                continue;
            }
            if RenderSessionSnapshot::load_json_file(&path).is_err() {
                continue;
            }
            let modified = metadata
                .modified()
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            if latest
                .as_ref()
                .map_or(true, |(current_modified, current_path)| {
                    modified > *current_modified
                        || (modified == *current_modified && path > *current_path)
                })
            {
                latest = Some((modified, path));
            }
        }

        Ok(latest.map(|(_, path)| path))
    }

    fn resolve_render_session_snapshot_path(&self, raw_path: &str) -> PathBuf {
        let path = PathBuf::from(raw_path);
        if path.is_absolute() {
            path
        } else {
            PathBuf::from(&self.engine_config.cwd).join(path)
        }
    }

    fn handle_render_snapshot_command(&mut self, args_raw: &str) {
        let args = args_raw.trim();
        let mut parts = args.splitn(2, char::is_whitespace);
        let action = parts.next().unwrap_or_default();
        let rest = parts.next().unwrap_or_default().trim();

        match action {
            "" | "save" | "export" => {
                let path = if rest.is_empty() {
                    self.default_render_session_snapshot_path()
                } else {
                    self.resolve_render_session_snapshot_path(rest)
                };
                self.open_render_snapshot_save_result(path);
            }
            "path" | "default-path" => {
                let path = self.default_render_session_snapshot_path();
                self.active_modal = ActiveModal::CommandOutput {
                    title: "Render Snapshot".to_string(),
                    body: format!(
                        "Default render session snapshot path\npath: {}",
                        path.display()
                    ),
                    is_error: false,
                };
            }
            "load" | "inspect" | "validate" => {
                if rest.is_empty() {
                    self.active_modal = ActiveModal::CommandOutput {
                        title: "Render Snapshot".to_string(),
                        body: "Usage: /render-snapshot load <path>".to_string(),
                        is_error: true,
                    };
                    return;
                }
                if render_snapshot_arg_is_latest(rest) {
                    self.open_render_snapshot_latest_load_result();
                } else {
                    let path = self.resolve_render_session_snapshot_path(rest);
                    self.open_render_snapshot_load_result(path);
                }
            }
            "latest" | "last" => {
                self.open_render_snapshot_latest_load_result();
            }
            "restore" | "hydrate" => {
                if rest.is_empty() || render_snapshot_arg_is_latest(rest) {
                    self.open_render_snapshot_latest_restore_result();
                } else {
                    let path = self.resolve_render_session_snapshot_path(rest);
                    self.open_render_snapshot_restore_result(path);
                }
            }
            _ => {
                let path = self.resolve_render_session_snapshot_path(args);
                self.open_render_snapshot_save_result(path);
            }
        }
    }

    fn handle_resume_command(&mut self, args_raw: &str) {
        let args = args_raw.trim();
        if args.is_empty() || render_snapshot_arg_is_latest(args) {
            self.open_render_snapshot_latest_restore_result();
        } else {
            let path = self.resolve_render_session_snapshot_path(args);
            self.open_render_snapshot_restore_result(path);
        }
    }

    fn open_render_snapshot_save_result(&mut self, path: PathBuf) {
        match self.save_render_session_snapshot(&path) {
            Ok(snapshot) => {
                self.active_modal = ActiveModal::CommandOutput {
                    title: "Render Snapshot".to_string(),
                    body: render_session_snapshot_saved_body(
                        &path,
                        Path::new(&self.engine_config.cwd),
                        &snapshot,
                    ),
                    is_error: false,
                };
            }
            Err(error) => {
                self.active_modal = ActiveModal::CommandOutput {
                    title: "Render Snapshot".to_string(),
                    body: format!(
                        "Failed to save render session snapshot\npath: {}\nerror: {error}",
                        path.display()
                    ),
                    is_error: true,
                };
            }
        }
    }

    fn open_render_snapshot_load_result(&mut self, path: PathBuf) {
        match Self::load_render_session_snapshot(&path) {
            Ok(snapshot) => {
                self.active_modal = ActiveModal::CommandOutput {
                    title: "Render Snapshot".to_string(),
                    body: render_session_snapshot_loaded_body(
                        &path,
                        Path::new(&self.engine_config.cwd),
                        &snapshot,
                    ),
                    is_error: false,
                };
            }
            Err(error) => {
                self.active_modal = ActiveModal::CommandOutput {
                    title: "Render Snapshot".to_string(),
                    body: format!(
                        "Failed to load render session snapshot\npath: {}\nerror: {error}",
                        path.display()
                    ),
                    is_error: true,
                };
            }
        }
    }

    fn open_render_snapshot_latest_load_result(&mut self) {
        match self.latest_render_session_snapshot_path() {
            Ok(Some(path)) => self.open_render_snapshot_load_result(path),
            Ok(None) => {
                self.active_modal = ActiveModal::CommandOutput {
                    title: "Render Snapshot".to_string(),
                    body: format!(
                        "No valid render session snapshot found\npath: {}",
                        self.render_session_snapshot_dir_path().display()
                    ),
                    is_error: false,
                };
            }
            Err(error) => {
                self.active_modal = ActiveModal::CommandOutput {
                    title: "Render Snapshot".to_string(),
                    body: format!(
                        "Failed to inspect render session snapshots\npath: {}\nerror: {error}",
                        self.render_session_snapshot_dir_path().display()
                    ),
                    is_error: true,
                };
            }
        }
    }

    fn open_render_snapshot_restore_result(&mut self, path: PathBuf) {
        match Self::load_render_session_snapshot(&path) {
            Ok(snapshot) => {
                let body = render_session_snapshot_restored_body(
                    &path,
                    Path::new(&self.engine_config.cwd),
                    &snapshot,
                );
                self.restore_render_session_snapshot(&snapshot);
                self.active_modal = ActiveModal::CommandOutput {
                    title: "Render Snapshot".to_string(),
                    body,
                    is_error: false,
                };
            }
            Err(error) => {
                self.active_modal = ActiveModal::CommandOutput {
                    title: "Render Snapshot".to_string(),
                    body: format!(
                        "Failed to restore render session snapshot\npath: {}\nerror: {error}",
                        path.display()
                    ),
                    is_error: true,
                };
            }
        }
    }

    fn open_render_snapshot_latest_restore_result(&mut self) {
        match self.latest_render_session_snapshot_path() {
            Ok(Some(path)) => self.open_render_snapshot_restore_result(path),
            Ok(None) => {
                self.active_modal = ActiveModal::CommandOutput {
                    title: "Resume".to_string(),
                    body: format!(
                        "No render session snapshot is available to resume\npath: {}",
                        self.render_session_snapshot_dir_path().display()
                    ),
                    is_error: false,
                };
            }
            Err(error) => {
                self.active_modal = ActiveModal::CommandOutput {
                    title: "Resume".to_string(),
                    body: format!(
                        "Failed to inspect render session snapshots\npath: {}\nerror: {error}",
                        self.render_session_snapshot_dir_path().display()
                    ),
                    is_error: true,
                };
            }
        }
    }

    fn prune_approval_decisions_to_current_messages(&mut self) {
        let live_anchor_ids: std::collections::HashSet<String> = self
            .render_transcript_model()
            .blocks
            .into_iter()
            .flat_map(render_block_anchor_ids)
            .collect();
        self.approval_decisions.retain(|decision| {
            decision
                .anchor_block_id
                .as_ref()
                .map_or(true, |anchor| live_anchor_ids.contains(anchor))
        });
    }

    fn push_command_output(
        &mut self,
        command_name: &str,
        content: impl Into<String>,
        is_error: bool,
    ) {
        self.messages.push(command_output_transcript_message(
            command_name,
            content,
            is_error,
        ));
        self.note_transcript_changed();
    }

    /// Run the main event loop.
    ///
    /// This is the main ratatui render loop:
    /// ```text
    /// loop {
    ///   terminal.draw(|f| app.render(f));
    ///   match event { ... }
    /// }
    /// ```
    pub async fn run(
        &mut self,
        terminal: ratatui::Terminal<impl ratatui::backend::Backend>,
    ) -> anyhow::Result<()> {
        let _ = self.load_footer_render_config_on_startup();
        if self.startup_render_session_restore_enabled {
            let _ = self.restore_latest_render_session_snapshot_on_startup();
        } else if matches!(
            self.render_snapshot_startup_restore_status,
            RenderSnapshotStartupRestoreStatus::Pending
        ) {
            self.render_snapshot_startup_restore_status =
                RenderSnapshotStartupRestoreStatus::Skipped;
        }
        if self.should_quit {
            self.autosave_render_session_snapshot_best_effort();
            return Ok(());
        }

        let event_bus = EventBus::new();
        let tx = event_bus.sender();

        // Spawn event readers
        spawn_crossterm_reader(tx.clone());
        spawn_tick_timer(tx, 33); // ~30fps

        self.run_event_loop_with_bus(terminal, event_bus).await
    }

    async fn run_event_loop_with_bus(
        &mut self,
        mut terminal: ratatui::Terminal<impl ratatui::backend::Backend>,
        mut event_bus: EventBus,
    ) -> anyhow::Result<()> {
        while !self.should_quit {
            self.launch_pending_compact_task();
            self.poll_compact_result();

            // If there's a queued submit, fire it off (async) before drawing.
            if let Some(params) = self.pending_submit.take() {
                let rx = submit_prompt(params).await;
                self.engine_rx = Some(rx);
                self.mark_render_dirty();
            }

            if self.should_render_frame_for_run() {
                let frame_started = Instant::now();
                if let Err(error) = terminal.draw(|frame| {
                    self.render_frame_safely(frame);
                }) {
                    self.autosave_render_session_snapshot_best_effort();
                    return Err(anyhow::anyhow!(
                        "terminal draw failed after render snapshot autosave attempt: {}",
                        error
                    ));
                }
                self.note_render_frame_drawn_with_duration(frame_started.elapsed());
            } else {
                self.note_render_frame_skipped();
            }

            // Wait for either an input event or an engine message. We pull
            // the receiver out of `self` for the select to satisfy the
            // borrow checker, then put it back if it's still live.
            let render_deadline = self.next_render_frame_due_at();
            let engine_rx = self.engine_rx.take();
            match engine_rx {
                Some(mut rx) => {
                    if let Some(deadline) = render_deadline {
                        let sleep =
                            tokio::time::sleep_until(tokio::time::Instant::from_std(deadline));
                        tokio::pin!(sleep);
                        tokio::select! {
                            biased;
                            ev = event_bus.recv_coalesced(DEFAULT_EVENT_BATCH_LIMIT) => {
                                // Put receiver back; engine is still streaming.
                                self.engine_rx = Some(rx);
                                if let Some(events) = ev {
                                    self.handle_event_batch(events);
                                }
                            }
                            msg = rx.recv() => {
                                match msg {
                                    Some(m) => {
                                        self.engine_rx = Some(rx);
                                        self.handle_engine_message(m);
                                    }
                                    None => {
                                        // Channel closed — finalize stream.
                                        self.finalize_assistant_turn(None);
                                        self.mark_render_dirty();
                                    }
                                }
                            }
                            _ = &mut sleep => {
                                self.engine_rx = Some(rx);
                            }
                        }
                    } else {
                        tokio::select! {
                            biased;
                            ev = event_bus.recv_coalesced(DEFAULT_EVENT_BATCH_LIMIT) => {
                                // Put receiver back; engine is still streaming.
                                self.engine_rx = Some(rx);
                                if let Some(events) = ev {
                                    self.handle_event_batch(events);
                                }
                            }
                            msg = rx.recv() => {
                                match msg {
                                    Some(m) => {
                                        self.engine_rx = Some(rx);
                                        self.handle_engine_message(m);
                                    }
                                    None => {
                                        // Channel closed — finalize stream.
                                        self.finalize_assistant_turn(None);
                                        self.mark_render_dirty();
                                    }
                                }
                            }
                        }
                    }
                }
                None => {
                    if let Some(deadline) = render_deadline {
                        let sleep =
                            tokio::time::sleep_until(tokio::time::Instant::from_std(deadline));
                        tokio::pin!(sleep);
                        tokio::select! {
                            biased;
                            ev = event_bus.recv_coalesced(DEFAULT_EVENT_BATCH_LIMIT) => {
                                if let Some(events) = ev {
                                    self.handle_event_batch(events);
                                }
                            }
                            _ = &mut sleep => {}
                        }
                    } else {
                        if let Some(events) =
                            event_bus.recv_coalesced(DEFAULT_EVENT_BATCH_LIMIT).await
                        {
                            self.handle_event_batch(events);
                        }
                    }
                }
            }
        }

        self.autosave_render_session_snapshot_best_effort();

        Ok(())
    }

    /// Render a single frame.
    fn render_frame_safely(&mut self, frame: &mut Frame) {
        let theme = self.theme.clone();
        let _ = Self::render_with_panic_boundary(frame, &theme, |frame| {
            self.render_frame(frame);
        });
    }

    fn render_with_panic_boundary(
        frame: &mut Frame,
        theme: &Theme,
        render: impl FnOnce(&mut Frame),
    ) -> Result<(), String> {
        let previous_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| render(frame)));
        std::panic::set_hook(previous_hook);

        match result {
            Ok(()) => Ok(()),
            Err(payload) => {
                let message = panic_payload_message(payload.as_ref());
                render_render_error_frame(frame, theme, &message);
                Err(message)
            }
        }
    }

    fn render_frame(&mut self, frame: &mut Frame) {
        let area = frame.area();
        self.state.terminal_width = area.width;
        self.state.terminal_height = area.height;
        let surface = self.render_surface_model();

        if self.fullscreen {
            self.render_fullscreen(frame, area, &surface);
        } else {
            self.render_inline(frame, area, &surface);
        }

        if let Some(until) = self.state.compact_notice_until {
            if std::time::Instant::now() >= until {
                self.state.compact_notice_until = None;
                self.state.compact_progress = None;
                self.state.compact_in_progress = false;
            }
        }
        self.render_compact_banner(frame, area);

        // Modal overlay drawn last so it stacks above the REPL surface.
        self.render_modal(frame, area);
    }

    fn render_surface_model(&self) -> RenderSurface {
        let transcript = self.render_transcript_model();
        let footer = self.footer_render_model();
        let approval = self.active_approval_render_model();
        let activity_panel = if approval.is_some() {
            None
        } else {
            self.active_panel_render_model(&footer)
        };
        let surface = RenderSurface::new(transcript, footer).with_activity_panel(activity_panel);
        if let Some(approval) = approval {
            surface.with_approval(approval)
        } else {
            surface
        }
    }

    /// Fullscreen layout rendering.
    fn render_fullscreen(&mut self, frame: &mut Frame, area: Rect, surface: &RenderSurface) {
        let top_status_height = self.top_status_header_height(area);
        let activity_panel_height = self.activity_panel_height(area, surface);
        let header_height = top_status_height
            .saturating_add(activity_panel_height)
            .max(self.sticky_prompt_header_height());
        let prompt_height = PromptInputWidget::new(&self.prompt, &self.theme)
            .glyphs(self.glyphs)
            .required_height();
        // Bottom area: prompt + 1-line spinner (when streaming) + 1-line status bar + 1-line hint bar.
        // Hint bar carries `? for shortcuts · ↵ send · ↑↓ history`.
        let bottom_height = prompt_height + if self.state.is_streaming { 1 } else { 0 } + 2; // status + hint

        let layout = AppLayout::fullscreen(area, header_height, bottom_height);
        self.render_top_status_header(frame, layout.header, top_status_height, &surface.top_status);
        self.render_activity_panel(
            frame,
            layout.header,
            top_status_height,
            activity_panel_height,
            surface,
        );
        let (content_area, approval_area) =
            self.split_content_for_inline_approval(layout.content, surface);
        let aux_layout = split_auxiliary_panels(
            content_area,
            self.state.task_list.tasks.len(),
            self.state.teammate_states.len(),
        );
        let (messages_area, scrollbar_area) =
            self.sync_message_scroll_with_scrollbar(aux_layout.messages, surface);
        self.message_content_area = Some(messages_area);
        self.message_scrollbar_area = scrollbar_area;

        // Messages area — or Welcome placeholder when empty.
        if surface.transcript.is_empty() {
            self.render_welcome(frame, messages_area);
        } else {
            let messages_widget = MessagesWidget::new(&self.messages, &self.theme, &self.scroll)
                .transcript(&surface.transcript)
                .show_all_thinking(self.show_all_thinking)
                .collapsed_tool_groups(&self.collapsed_tool_groups)
                .focused_idx(self.focused_message_idx)
                .height_cache(&self.render_height_cache)
                .glyphs(self.glyphs);
            frame.render_widget(messages_widget, messages_area);
            self.render_message_scrollbar(frame, scrollbar_area);
        }
        self.render_inline_approval(frame, approval_area, surface);
        self.render_auxiliary_panels(frame, aux_layout.task_list, aux_layout.teammates);

        // Bottom area: spinner + prompt + status bar
        let mut y = layout.bottom.y;
        let mut remaining = layout.bottom.height;
        if self.state.is_streaming && remaining > 0 {
            let spinner_area = Rect::new(layout.bottom.x, y, layout.bottom.width, 1);
            let status_text = self.spinner_status_text(surface);
            let spinner_widget = SpinnerRowWidget::new(&self.spinner, &status_text)
                .glyphs(self.glyphs)
                .color_mode(self.theme.color_mode);
            frame.render_widget(spinner_widget, spinner_area);
            y = y.saturating_add(1);
            remaining = remaining.saturating_sub(1);
        }
        // Status + hint rows sit *above* the prompt so the input row always
        // anchors to the bottom.
        if remaining > 2 {
            let prompt_h = remaining.saturating_sub(2);
            let prompt_area = Rect::new(layout.bottom.x, y, layout.bottom.width, prompt_h);
            let prompt_widget =
                PromptInputWidget::new(&self.prompt, &self.theme).glyphs(self.glyphs);
            frame.render_widget(prompt_widget, prompt_area);
            let status_area = Rect::new(layout.bottom.x, y + prompt_h, layout.bottom.width, 1);
            self.render_status_bar(frame, status_area, &surface.footer);
            let hint_area = Rect::new(layout.bottom.x, y + prompt_h + 1, layout.bottom.width, 1);
            self.render_hint_bar(frame, hint_area);
        } else if remaining == 2 {
            let status_area = Rect::new(layout.bottom.x, y, layout.bottom.width, 1);
            self.render_status_bar(frame, status_area, &surface.footer);
            let hint_area = Rect::new(layout.bottom.x, y + 1, layout.bottom.width, 1);
            self.render_hint_bar(frame, hint_area);
        } else if remaining == 1 {
            let status_area = Rect::new(layout.bottom.x, y, layout.bottom.width, 1);
            self.render_status_bar(frame, status_area, &surface.footer);
        }
    }

    /// Inline (non-fullscreen) layout rendering.
    fn render_inline(&mut self, frame: &mut Frame, area: Rect, surface: &RenderSurface) {
        let top_status_height = self.top_status_header_height(area);
        let activity_panel_height = self.activity_panel_height(area, surface);
        let top_chrome_height = top_status_height.saturating_add(activity_panel_height);
        let body_area = if top_chrome_height > 0 {
            Rect::new(
                area.x,
                area.y.saturating_add(top_chrome_height),
                area.width,
                area.height.saturating_sub(top_chrome_height),
            )
        } else {
            area
        };
        let prompt_height = PromptInputWidget::new(&self.prompt, &self.theme)
            .glyphs(self.glyphs)
            .required_height();
        // Inline layout reserves the prompt widget's measured height plus
        // one status row. Spinner is rendered above the prompt while a turn
        // is in flight.
        let bottom_height = prompt_height + if self.state.is_streaming { 1 } else { 0 } + 1;
        let layout = AppLayout::inline(body_area, bottom_height);
        self.render_top_status_header(frame, area, top_status_height, &surface.top_status);
        self.render_activity_panel(
            frame,
            area,
            top_status_height,
            activity_panel_height,
            surface,
        );
        let (content_area, approval_area) =
            self.split_content_for_inline_approval(layout.content, surface);
        let aux_layout = split_auxiliary_panels(
            content_area,
            self.state.task_list.tasks.len(),
            self.state.teammate_states.len(),
        );
        let (messages_area, scrollbar_area) =
            self.sync_message_scroll_with_scrollbar(aux_layout.messages, surface);
        self.message_content_area = Some(messages_area);
        self.message_scrollbar_area = scrollbar_area;

        let messages_widget = MessagesWidget::new(&self.messages, &self.theme, &self.scroll)
            .transcript(&surface.transcript)
            .show_all_thinking(self.show_all_thinking)
            .collapsed_tool_groups(&self.collapsed_tool_groups)
            .focused_idx(self.focused_message_idx)
            .height_cache(&self.render_height_cache)
            .glyphs(self.glyphs);
        frame.render_widget(messages_widget, messages_area);
        self.render_message_scrollbar(frame, scrollbar_area);
        self.render_inline_approval(frame, approval_area, surface);
        self.render_auxiliary_panels(frame, aux_layout.task_list, aux_layout.teammates);

        let mut y = layout.bottom.y;
        let mut remaining = layout.bottom.height;
        if self.state.is_streaming && remaining > 0 {
            let spinner_area = Rect::new(layout.bottom.x, y, layout.bottom.width, 1);
            let status_text = self.spinner_status_text(surface);
            let spinner_widget = SpinnerRowWidget::new(&self.spinner, &status_text)
                .glyphs(self.glyphs)
                .color_mode(self.theme.color_mode);
            frame.render_widget(spinner_widget, spinner_area);
            y = y.saturating_add(1);
            remaining = remaining.saturating_sub(1);
        }
        if remaining > 1 {
            let prompt_h = remaining.saturating_sub(1);
            let prompt_area = Rect::new(layout.bottom.x, y, layout.bottom.width, prompt_h);
            let prompt_widget =
                PromptInputWidget::new(&self.prompt, &self.theme).glyphs(self.glyphs);
            frame.render_widget(prompt_widget, prompt_area);
            let status_area = Rect::new(layout.bottom.x, y + prompt_h, layout.bottom.width, 1);
            self.render_status_bar(frame, status_area, &surface.footer);
        } else if remaining == 1 {
            let status_area = Rect::new(layout.bottom.x, y, layout.bottom.width, 1);
            self.render_status_bar(frame, status_area, &surface.footer);
        }
    }

    fn render_auxiliary_panels(
        &self,
        frame: &mut Frame,
        task_area: Option<Rect>,
        teammate_area: Option<Rect>,
    ) {
        if let Some(area) = task_area {
            if !self.state.task_list.tasks.is_empty() && area.width >= 24 && area.height >= 3 {
                let task_widget = crate::widgets::task_list::TaskListV2Widget::new(
                    &self.state.task_list.tasks,
                    &self.theme,
                )
                .glyphs(self.glyphs);
                frame.render_widget(task_widget, area);
            }
        }

        if let Some(area) = teammate_area {
            if !self.state.teammate_states.is_empty() && area.width >= 24 && area.height >= 2 {
                use crate::widgets::spinner::{
                    TeammateSpinnerLineState, TeammateSpinnerTreeState, TeammateSpinnerTreeWidget,
                    TeammateStatus,
                };
                let mut lines: Vec<TeammateSpinnerLineState> = Vec::new();
                for (tid, st) in &self.state.teammate_states {
                    let status = match st {
                        TeammateState::Running => TeammateStatus::Active,
                        TeammateState::Completed(_) => TeammateStatus::Done,
                        TeammateState::Failed(_) => TeammateStatus::Done,
                    };
                    lines.push(TeammateSpinnerLineState::new(tid.clone(), status));
                }
                let tree_state = TeammateSpinnerTreeState::new(lines);
                let tree_widget =
                    TeammateSpinnerTreeWidget::new(&tree_state, &self.theme).glyphs(self.glyphs);
                frame.render_widget(tree_widget, area);
            }
        }
    }

    fn spinner_status_text(&self, surface: &RenderSurface) -> String {
        let elapsed = self.spinner.elapsed().as_secs();
        let wait = self.glyphs.ellipsis();
        if let Some(blocking) = &surface.blocking {
            return match blocking.kind {
                BlockingKind::Approval => format!("Waiting approval{wait} {elapsed}s"),
                BlockingKind::Error => format!("Blocked by error{wait} {elapsed}s"),
                BlockingKind::CostLimit => format!("Cost threshold{wait} {elapsed}s"),
                BlockingKind::IdleReturn => format!("Welcome back{wait} {elapsed}s"),
                BlockingKind::Info => format!("Waiting{wait} {elapsed}s"),
            };
        }

        match surface.footer.turn_state.as_deref() {
            Some("running tool") => format!("Running tool{wait} {elapsed}s"),
            Some("waiting approval") => format!("Waiting approval{wait} {elapsed}s"),
            Some("streaming") | None => format!("Thinking{wait} {elapsed}s"),
            Some(state) => format!("{state}{wait} {elapsed}s"),
        }
    }

    fn active_inline_approval_height(&self, width: u16, surface: &RenderSurface) -> u16 {
        if width < 24 {
            return 0;
        }
        if let Some(model) = surface.approvals.first() {
            let panel_width = self.inline_panel_width(width, 82);
            ApprovalBlockWidget::new(model, &self.theme)
                .glyphs(self.glyphs)
                .required_height(panel_width)
        } else {
            0
        }
    }

    fn split_content_for_inline_approval(
        &self,
        content: Rect,
        surface: &RenderSurface,
    ) -> (Rect, Option<Rect>) {
        let height = self
            .active_inline_approval_height(content.width, surface)
            .min(content.height.saturating_sub(3));
        if height == 0 {
            return (content, None);
        }
        let messages = Rect::new(
            content.x,
            content.y,
            content.width,
            content.height.saturating_sub(height),
        );
        let approval = Rect::new(
            content.x,
            content.y + messages.height,
            content.width,
            height,
        );
        (messages, Some(approval))
    }

    fn render_inline_approval(
        &self,
        frame: &mut Frame,
        area: Option<Rect>,
        surface: &RenderSurface,
    ) {
        let Some(area) = area else {
            return;
        };
        if area.width < 24 || area.height < 4 {
            return;
        }
        if !surface.approvals.is_empty() {
            self.render_inline_approval_model(frame, area, surface);
        }
    }

    fn inline_panel_rect(&self, area: Rect, preferred_width: u16) -> Rect {
        let width = self.inline_panel_width(area.width, preferred_width);
        Rect::new(area.x, area.y, width, area.height)
    }

    fn inline_panel_width(&self, available_width: u16, preferred_width: u16) -> u16 {
        preferred_width
            .min(available_width)
            .max(available_width.min(24))
    }

    fn render_inline_approval_model(&self, frame: &mut Frame, area: Rect, surface: &RenderSurface) {
        use ratatui::widgets::Clear;
        let Some(model) = surface.approvals.first() else {
            return;
        };
        let panel = self.inline_panel_rect(area, 82);
        frame.render_widget(Clear, panel);
        let widget = ApprovalBlockWidget::new(model, &self.theme).glyphs(self.glyphs);
        frame.render_widget(widget, panel);
    }

    fn active_approval_render_model(&self) -> Option<ApprovalRenderModel> {
        match &self.active_modal {
            ActiveModal::PermissionRequest(state) => {
                Some(self.approval_model_from_prompt("permission-request", state, None, None))
            }
            ActiveModal::ToolUseConfirm { confirm, prompt } => {
                Some(self.approval_model_from_prompt(
                    &confirm.tool_use_id,
                    prompt,
                    self.last_tool_anchor_block_id(&confirm.tool_name),
                    Some(confirm.risk_level),
                ))
            }
            ActiveModal::McpChannelApproval(request) => {
                Some(self.approval_model_from_mcp_channel(request))
            }
            _ => None,
        }
    }

    fn approval_model_from_prompt(
        &self,
        id: &str,
        state: &PermissionPromptState,
        anchor_block_id: Option<String>,
        risk_score: Option<u8>,
    ) -> ApprovalRenderModel {
        ApprovalRenderModel {
            id: id.to_string(),
            tool_name: state.tool_name.clone(),
            title: state.kind.label().to_string(),
            detail_label: state.kind.detail_label().to_string(),
            detail: state.kind.detail(),
            risk: risk_score
                .map(ApprovalRiskLevel::from_score)
                .unwrap_or_else(|| approval_risk_from_permission_kind(&state.kind)),
            body: state.explanation.clone().unwrap_or_default(),
            actions: state
                .available_actions()
                .into_iter()
                .map(render_approval_action_from_permission)
                .collect(),
            selected_action: render_approval_action_from_permission(state.selected_action),
            anchor_block_id,
            expanded: state.show_details,
        }
    }

    fn stage_command_edit_from_approval(&mut self, command: &str) {
        let command = command.trim();
        if command.is_empty() {
            return;
        }
        self.prompt.input.clear();
        self.prompt
            .input
            .insert_str(&format!("Edit command before running: {command}"));
        self.prompt.active = true;
        self.prompt.show_suggestions = false;
        self.prompt.selected_suggestion = None;
        self.update_suggestions();
    }

    fn approval_model_from_mcp_channel(
        &self,
        request: &mossen_agent::mcp::channel_approval::ChannelApprovalRequest,
    ) -> ApprovalRenderModel {
        let plugin = request.plugin.as_deref().unwrap_or("unknown plugin");
        let marketplace = request
            .marketplace
            .as_deref()
            .unwrap_or("unknown marketplace");
        ApprovalRenderModel {
            id: request.id.clone(),
            tool_name: "MCP Channel".to_string(),
            title: "MCP Channel Approval".to_string(),
            detail_label: "Server".to_string(),
            detail: request.server_name.clone(),
            risk: ApprovalRiskLevel::Medium,
            body: format!(
                "Plugin: {plugin}\nMarketplace: {marketplace}\n{}",
                request.reason
            ),
            actions: vec![RenderApprovalAction::Allow, RenderApprovalAction::Deny],
            selected_action: RenderApprovalAction::Allow,
            anchor_block_id: self.last_mcp_tool_anchor_block_id(&request.server_name),
            expanded: true,
        }
    }

    fn last_tool_anchor_block_id(&self, tool_name: &str) -> Option<String> {
        self.render_transcript_model()
            .blocks
            .iter()
            .rev()
            .find(|block| {
                block
                    .tool
                    .as_ref()
                    .is_some_and(|tool| tool.name == tool_name)
            })
            .map(|block| block.id.clone())
    }

    fn last_mcp_tool_anchor_block_id(&self, server_name: &str) -> Option<String> {
        self.render_transcript_model()
            .blocks
            .iter()
            .rev()
            .find(|block| {
                block
                    .tool
                    .as_ref()
                    .is_some_and(|tool| mcp_tool_name_matches_server(&tool.name, server_name))
            })
            .map(|block| block.id.clone())
    }

    fn sync_message_scroll_with_scrollbar(
        &mut self,
        content_area: Rect,
        surface: &RenderSurface,
    ) -> (Rect, Option<Rect>) {
        if content_area.width == 0 || content_area.height == 0 || surface.transcript.is_empty() {
            self.sync_message_scroll(content_area, surface);
            return (content_area, None);
        }

        let full_width_total_rows = self.message_total_rows(surface, content_area.width);
        if full_width_total_rows <= content_area.height as usize {
            self.scroll.set_viewport_height(content_area.height);
            self.scroll.set_total_items(full_width_total_rows);
            return (content_area, None);
        }

        if content_area.width >= 24 && content_area.height >= 3 {
            let rail_x = content_area
                .x
                .saturating_add(content_area.width.saturating_sub(1));
            let scroll_content = Rect::new(
                content_area.x,
                content_area.y,
                content_area.width.saturating_sub(1),
                content_area.height,
            );
            let total_rows = self.message_total_rows(surface, scroll_content.width);
            if total_rows > scroll_content.height as usize {
                self.scroll.set_viewport_height(scroll_content.height);
                self.scroll.set_total_items(total_rows);
                return (
                    scroll_content,
                    Some(Rect::new(rail_x, content_area.y, 1, content_area.height)),
                );
            }
        }

        self.scroll.set_viewport_height(content_area.height);
        self.scroll.set_total_items(full_width_total_rows);
        (content_area, None)
    }

    fn sync_message_scroll(&mut self, content_area: Rect, surface: &RenderSurface) {
        self.scroll.set_viewport_height(content_area.height);
        let total_rows = self.message_total_rows(surface, content_area.width);
        self.scroll.set_total_items(total_rows);
    }

    fn message_total_rows(&self, surface: &RenderSurface, width: u16) -> usize {
        MessagesWidget::required_content_height_from_transcript_with_cache_and_glyphs(
            self.messages
                .len()
                .max(surface.transcript.source_record_count()),
            &surface.transcript,
            &self.theme,
            width,
            self.show_all_thinking,
            &self.collapsed_tool_groups,
            Some(&self.render_height_cache),
            self.glyphs,
        )
    }

    fn render_message_scrollbar(&self, frame: &mut Frame, scrollbar_area: Option<Rect>) {
        let Some(area) = scrollbar_area else {
            return;
        };
        if area.width == 0
            || area.height == 0
            || self.scroll.visible_count == 0
            || self.scroll.total_items <= self.scroll.visible_count
        {
            return;
        }

        self.render_scrollbar_widget(
            frame,
            area,
            self.scroll.total_items,
            self.scroll.visible_count,
            self.scroll.offset,
        );
    }

    fn render_modal_scrollbar(
        &self,
        frame: &mut Frame,
        content_area: Rect,
        kind: ModalScrollbarKind,
        total_rows: usize,
        viewport_rows: usize,
        offset: usize,
    ) {
        if content_area.width == 0
            || content_area.height == 0
            || viewport_rows == 0
            || total_rows <= viewport_rows
            || content_area.height < 3
        {
            return;
        }

        let area = Rect::new(
            content_area
                .x
                .saturating_add(content_area.width.saturating_sub(1)),
            content_area.y,
            1,
            content_area.height,
        );
        self.modal_scrollbar_target
            .set(Some(ModalScrollbarHitTarget {
                kind,
                area,
                total_rows,
                viewport_rows,
            }));
        self.render_scrollbar_widget(frame, area, total_rows, viewport_rows, offset);
    }

    fn render_scrollbar_widget(
        &self,
        frame: &mut Frame,
        area: Rect,
        total_rows: usize,
        viewport_rows: usize,
        offset: usize,
    ) {
        let (track, thumb) = match self.glyphs.mode {
            RenderGlyphMode::Unicode => ("│", "┃"),
            RenderGlyphMode::Ascii => ("|", "#"),
        };
        let mut state = ScrollbarState::new(total_rows)
            .position(offset.min(total_rows.saturating_sub(viewport_rows)))
            .viewport_content_length(viewport_rows);
        let widget = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .track_symbol(Some(track))
            .thumb_symbol(thumb)
            .track_style(Style::default().fg(self.theme.text_subtle))
            .thumb_style(Style::default().fg(self.theme.info));
        frame.render_stateful_widget(widget, area, &mut state);
    }

    /// Render the semantic footer model directly through the active footer
    /// widget, keeping state assembly out of the hot path while making sure
    /// every relevant value reaches the bottom chrome.
    /// Welcome screen — drawn when `messages` is empty. Mirrors the TS
    /// REPL splash: an ASCII tear-drop glyph, the cwd, the active model,
    /// and a short hint that nudges the user to type. Stays out of the
    /// way as soon as the first turn lands.
    fn render_welcome(&self, frame: &mut Frame, area: Rect) {
        use ratatui::style::{Color, Modifier, Style};
        use ratatui::text::{Line, Span};
        use ratatui::widgets::Paragraph;
        if area.height < 4 || area.width < 20 {
            return;
        }
        let cwd = &self.engine_config.cwd;
        // Shorten home path so the line doesn't dominate the screen.
        let display_cwd = if let Some(home) = dirs::home_dir() {
            let h = home.to_string_lossy().to_string();
            if cwd.starts_with(&h) {
                format!("~{}", &cwd[h.len()..])
            } else {
                cwd.clone()
            }
        } else {
            cwd.clone()
        };
        let model = &self.engine_config.model;
        let accent = Style::default()
            .fg(self.theme.success)
            .add_modifier(Modifier::BOLD);
        let dim = Style::default().fg(self.theme.text_dim);
        let lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled(format!("  {}  ", self.glyphs.assistant), accent),
                Span::styled("Welcome to Mossen", accent),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("     cwd  ", dim),
                Span::raw(display_cwd),
            ]),
            Line::from(vec![
                Span::styled("    model ", dim),
                Span::raw(model.clone()),
            ]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "     Type your question to begin.  /help for commands.",
                dim,
            )]),
        ];
        let p = Paragraph::new(lines);
        let centered_y = area.y + area.height / 3;
        let inner = Rect::new(
            area.x,
            centered_y,
            area.width,
            area.height - area.height / 3,
        );
        frame.render_widget(p, inner);
    }

    /// Bottom hint bar — a single dim line listing the canonical
    /// keystrokes the prompt understands. Renders below the status bar
    /// so the user sees both cwd/model/cost (status) and shortcuts (hint)
    /// without taking either's slot.
    fn render_hint_bar(&self, frame: &mut Frame, area: Rect) {
        use ratatui::style::{Color, Modifier, Style};
        use ratatui::text::{Line, Span};
        use ratatui::widgets::Paragraph;
        if area.height == 0 || area.width < 10 {
            return;
        }
        let dim = Style::default()
            .fg(self.theme.text_subtle)
            .add_modifier(Modifier::DIM);
        let sep = Span::styled(
            match self.glyphs.mode {
                RenderGlyphMode::Unicode => "  ·  ",
                RenderGlyphMode::Ascii => "  |  ",
            },
            dim,
        );
        let hints: &[&str] = match self.glyphs.mode {
            RenderGlyphMode::Unicode => &[
                "  ↵ send",
                "↑↓ history",
                "Esc selector",
                "Ctrl+T tasks",
                "Ctrl+E ⇄ think",
                "Ctrl+G editor",
                "/help",
            ],
            RenderGlyphMode::Ascii => &[
                "  Enter send",
                "Up/Down history",
                "Esc selector",
                "Ctrl+T tasks",
                "Ctrl+E think",
                "Ctrl+G editor",
                "/help",
            ],
        };
        let mut spans = Vec::new();
        for (idx, hint) in hints.iter().enumerate() {
            if idx > 0 {
                spans.push(sep.clone());
            }
            spans.push(Span::styled(*hint, dim));
        }
        let line = Line::from(spans);
        frame.render_widget(Paragraph::new(line), area);
    }

    fn top_status_header_height(&self, area: Rect) -> u16 {
        if tui_top_status_enabled() && area.width >= 20 && area.height >= 8 {
            1
        } else {
            0
        }
    }

    fn activity_panel_height(&self, area: Rect, surface: &RenderSurface) -> u16 {
        let Some(model) = surface.activity_panel.as_ref() else {
            return 0;
        };
        if area.width < 24 || area.height < 10 {
            return 0;
        }
        let required = ActivityPanelWidget::new(model, &self.theme)
            .glyphs(self.glyphs)
            .required_height(area.width);
        if required >= 3 && area.height < 14 {
            1
        } else {
            required
        }
    }

    fn render_top_status_header(
        &self,
        frame: &mut Frame,
        header_area: Rect,
        top_status_height: u16,
        status: &TopStatusRenderModel,
    ) {
        if top_status_height == 0 || header_area.height == 0 || header_area.width < 20 {
            return;
        }
        let area = Rect::new(header_area.x, header_area.y, header_area.width, 1);
        let widget = StatusHeaderWidget::new(status, &self.theme).glyphs(self.glyphs);
        frame.render_widget(widget, area);
    }

    fn render_activity_panel(
        &self,
        frame: &mut Frame,
        header_area: Rect,
        top_status_height: u16,
        activity_panel_height: u16,
        surface: &RenderSurface,
    ) {
        let Some(model) = surface.activity_panel.as_ref() else {
            return;
        };
        if activity_panel_height == 0 || header_area.width < 24 {
            return;
        }
        let y = header_area.y.saturating_add(top_status_height);
        if y >= header_area.bottom() {
            return;
        }
        let height = activity_panel_height.min(header_area.bottom().saturating_sub(y));
        if height == 0 {
            return;
        }
        let area = Rect::new(header_area.x, y, header_area.width, height);
        let widget = ActivityPanelWidget::new(model, &self.theme).glyphs(self.glyphs);
        frame.render_widget(widget, area);
    }

    fn render_status_bar(&self, frame: &mut Frame, area: Rect, footer: &FooterRenderModel) {
        let widget =
            crate::widgets::footer::FooterWidget::new(footer, &self.theme).glyphs(self.glyphs);
        frame.render_widget(widget, area);
    }

    fn active_panel_render_model(
        &self,
        footer: &FooterRenderModel,
    ) -> Option<ActivityPanelRenderModel> {
        if let Some(blocking) = footer.blocking.as_ref() {
            return Some(ActivityPanelRenderModel::from_blocking(blocking));
        }

        let activity = self.state.render_activity.current.as_ref()?;
        if self.state.ui_stage == UiStage::Idle {
            return None;
        }
        if matches!(activity, RenderActivity::Final { .. }) {
            return None;
        }
        Some(activity_panel_from_render_activity(
            self.state.ui_stage.label(),
            activity,
        ))
    }

    fn footer_render_model(&self) -> FooterRenderModel {
        let mode = self.current_permission_mode_label();
        let blocking = self.current_blocking_render_model();
        FooterRenderModel {
            project: Some(self.engine_config.cwd.clone()),
            model: Some(self.engine_config.model.clone()),
            access_mode: Some(mode),
            reasoning: self.reasoning_status_label(),
            context: self.context_usage_render_model(),
            turn_state: Some(self.turn_state_label().to_string()),
            activity: self.state.render_activity.status_line(),
            cost: if self.total_cost_usd > 0.0 {
                Some(format!("${:.2}", self.total_cost_usd))
            } else {
                None
            },
            message_count: Some(self.messages.len()),
            mcp_summary: self.mcp_status_summary(),
            external_status: self.external_statusline_output.clone(),
            blocking,
            config: self.state.footer_config.clone(),
        }
    }

    fn status_overview_render_model(&self) -> StatusOverviewRenderModel {
        let footer = self.footer_render_model();
        let process = self.process_list_render_model();
        let model = footer
            .model
            .as_deref()
            .unwrap_or(self.engine_config.model.as_str());
        let access_mode = footer.access_mode.as_deref().unwrap_or("Supervised");
        let turn_state = footer.turn_state.as_deref().unwrap_or("idle");
        let turn_id_label = self.render_turn_id_label();
        let summary = if turn_id_label == "none" {
            format!("model {model} | turn {turn_state} | mode {access_mode}")
        } else {
            format!("model {model} | turn {turn_state} | id {turn_id_label} | mode {access_mode}")
        };

        let backend = self
            .engine_config
            .api_base_url
            .as_deref()
            .unwrap_or("default");
        let session = self
            .engine_session_id
            .as_deref()
            .unwrap_or("(not initialized)");
        let output_style = self
            .engine_config
            .output_style
            .as_deref()
            .unwrap_or("default");
        let origin = format!("{:?}", self.engine_config.origin_tag);

        let user_turns = self
            .messages
            .iter()
            .filter(|m| matches!(m.message_type, MessageType::User))
            .count();
        let activity = footer.activity.as_deref().unwrap_or("none").to_string();
        let blocking_value = footer
            .blocking
            .as_ref()
            .map(|blocking| format!("{}: {}", blocking.title, blocking.detail))
            .unwrap_or_else(|| "none".to_string());
        let blocking_level = footer
            .blocking
            .as_ref()
            .map(|blocking| status_level_for_blocking(blocking.kind))
            .unwrap_or(StatusRowLevel::Good);
        let compact = if self.state.compact_in_progress || self.state.compact_progress.is_some() {
            self.state
                .compact_progress
                .clone()
                .unwrap_or_else(|| "compacting conversation history".to_string())
        } else {
            "idle".to_string()
        };
        let compact_level = if self.state.compact_in_progress {
            StatusRowLevel::Info
        } else {
            StatusRowLevel::Good
        };

        let (todo_value, todo_level) = status_todo_summary(&self.state.task_list.tasks);
        let (agent_value, agent_level) = status_agent_summary(&self.state.teammate_states);
        let (mcp_value, mcp_level) = status_mcp_summary(&self.state.mcp_servers);

        StatusOverviewRenderModel::new(summary)
            .footer("Esc closes")
            .section(
                StatusSectionRenderModel::new("Session")
                    .row("Model", model, StatusRowLevel::Good)
                    .row(
                        "Backend",
                        format!("{backend} / origin {origin}"),
                        StatusRowLevel::Info,
                    )
                    .row("CWD", &self.engine_config.cwd, StatusRowLevel::Normal)
                    .row("Session", session, StatusRowLevel::Info)
                    .row("Output Style", output_style, StatusRowLevel::Normal),
            )
            .section(
                StatusSectionRenderModel::new("Turn")
                    .row(
                        "State",
                        turn_state,
                        status_level_for_turn(turn_state, footer.blocking.as_ref()),
                    )
                    .row("Activity", activity, StatusRowLevel::Info)
                    .row("Blocking", blocking_value, blocking_level)
                    .row(
                        "Processes",
                        format!(
                            "{} active / {} waiting / {} failed",
                            process.summary.active_count,
                            process.summary.waiting_count,
                            process.summary.failed_count
                        ),
                        status_level_for_process_summary(&process.summary),
                    )
                    .row("Compact", compact, compact_level),
            )
            .section(
                StatusSectionRenderModel::new("Policy")
                    .row("Access Mode", access_mode, StatusRowLevel::Normal)
                    .row(
                        "Reasoning",
                        footer.reasoning.as_deref().unwrap_or("auto"),
                        StatusRowLevel::Info,
                    )
                    .row(
                        "Context",
                        status_context_value(footer.context),
                        status_level_for_context(footer.context),
                    )
                    .row(
                        "Cost",
                        format!("${:.4}", self.total_cost_usd),
                        StatusRowLevel::Normal,
                    )
                    .row(
                        "API Key",
                        if self
                            .engine_config
                            .api_key
                            .as_deref()
                            .is_some_and(|key| !key.trim().is_empty())
                        {
                            "configured"
                        } else {
                            "missing"
                        },
                        if self
                            .engine_config
                            .api_key
                            .as_deref()
                            .is_some_and(|key| !key.trim().is_empty())
                        {
                            StatusRowLevel::Good
                        } else {
                            StatusRowLevel::Warning
                        },
                    ),
            )
            .section(
                StatusSectionRenderModel::new("Workspace")
                    .row(
                        "Messages",
                        format!(
                            "{} messages / {} user turns",
                            self.messages.len(),
                            user_turns
                        ),
                        StatusRowLevel::Info,
                    )
                    .row("Todos", todo_value, todo_level)
                    .row("Agents", agent_value, agent_level)
                    .row("MCP", mcp_value, mcp_level),
            )
    }

    fn reasoning_status_label(&self) -> Option<String> {
        self.engine_config
            .effort
            .map(|level| level.as_str().to_string())
            .or_else(|| {
                self.engine_config
                    .extra_body
                    .get("effort")
                    .or_else(|| self.engine_config.extra_body.get("reasoning_effort"))
                    .or_else(|| self.engine_config.extra_body.get("reasoningEffort"))
                    .and_then(json_string_value)
            })
            .or_else(|| std::env::var("MOSSEN_CODE_EFFORT_LEVEL").ok())
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty())
    }

    fn context_usage_render_model(&self) -> Option<ContextUsageRenderModel> {
        let window_tokens = context_window_tokens_for_footer(&self.engine_config.model);
        let used_tokens =
            mossen_agent::token_estimation::estimate_messages_tokens(&self.engine_history);
        ContextUsageRenderModel::new(used_tokens, window_tokens)
    }

    fn current_blocking_render_model(&self) -> Option<BlockingRenderModel> {
        if let Some(approval) = self.active_approval_render_model() {
            return Some(approval.blocking_model());
        }
        if self.active_permission_responder.is_some() {
            return Some(BlockingRenderModel::approval(
                "Approval required",
                "tool permission response pending",
            ));
        }
        match &self.active_modal {
            ActiveModal::CostThreshold(summary) => Some(BlockingRenderModel::cost_limit(
                "Cost threshold",
                summary.clone(),
            )),
            ActiveModal::IdleReturn(summary) => Some(BlockingRenderModel::idle_return(
                "Welcome back",
                summary.clone(),
            )),
            ActiveModal::CommandOutput {
                title,
                body,
                is_error,
            } if *is_error => Some(BlockingRenderModel::error(title.clone(), body.clone())),
            _ => None,
        }
    }

    fn turn_state_label(&self) -> &'static str {
        if self.active_modal.is_inline_approval() || self.active_permission_responder.is_some() {
            return "waiting approval";
        }
        if self.state.ui_stage != UiStage::Idle {
            return self.state.ui_stage.label();
        }
        if self.state.is_streaming {
            return "streaming";
        }
        match self.state.turn_state {
            TurnState::Idle => {
                if self.state.is_waiting_for_response {
                    "waiting"
                } else {
                    "idle"
                }
            }
            TurnState::Streaming => "streaming",
            TurnState::Cancelling => "cancelling",
            TurnState::Cancelled => "cancelled",
        }
    }

    fn render_turn_id_label(&self) -> String {
        if let Some(turn_id) = self.current_render_turn_id.as_deref() {
            return turn_id.to_string();
        }
        self.latest_render_turn_id()
            .map(|turn_id| format!("{turn_id} (last)"))
            .unwrap_or_else(|| "none".to_string())
    }

    fn latest_render_turn_id(&self) -> Option<&str> {
        self.messages
            .iter()
            .enumerate()
            .rev()
            .find_map(|(index, _)| self.render_record_turn_overrides.get(&index))
            .map(String::as_str)
    }

    fn semantic_preview_for_message(
        &self,
        transcript: &RenderTranscript,
        msg_idx: usize,
    ) -> String {
        transcript
            .blocks
            .iter()
            .find(|block| block.source_indices.contains(&msg_idx))
            .map(|block| block.selector_summary())
            .filter(|summary| !summary.trim().is_empty())
            .or_else(|| self.messages.get(msg_idx).map(fallback_search_preview))
            .unwrap_or_default()
    }

    fn render_compact_banner(&self, frame: &mut Frame, area: Rect) {
        if self.state.compact_progress.is_none() && !self.state.compact_in_progress {
            return;
        }
        if area.width < 20 || area.height == 0 {
            return;
        }
        use ratatui::style::{Modifier, Style};
        use ratatui::widgets::Paragraph;
        let text = self
            .state
            .compact_progress
            .as_deref()
            .unwrap_or("Compacting conversation...");
        let banner = format!(" Compact  {} ", text);
        let style = Style::default()
            .fg(self.theme.background)
            .bg(self.theme.warning)
            .add_modifier(Modifier::BOLD);
        let width = area.width.saturating_sub(4).min(80);
        let banner_area = Rect::new(area.x + 2, area.y, width, 1);
        frame.render_widget(Paragraph::new(banner).style(style), banner_area);
    }

    fn mcp_status_summary(&self) -> Option<String> {
        if self.state.mcp_servers.is_empty() {
            return None;
        }
        let total = self.state.mcp_servers.len();
        let configured = self
            .state
            .mcp_servers
            .iter()
            .filter(|s| {
                matches!(
                    s.state,
                    McpConnectionState::Configured | McpConnectionState::Connected
                )
            })
            .count();
        let tools: usize = self.state.mcp_servers.iter().map(|s| s.tools_count).sum();
        let marker = if configured == total { "MCP" } else { "MCP!" };
        if tools > 0 {
            Some(format!("{} {} servers/{} tools", marker, total, tools))
        } else {
            Some(format!("{} {} servers", marker, total))
        }
    }

    /// Render the active modal overlay (if any).
    fn render_modal(&self, frame: &mut Frame, area: Rect) {
        self.modal_scrollbar_target.set(None);
        if self.active_modal.is_inline_approval() {
            return;
        }
        match &self.active_modal {
            ActiveModal::None => {}
            ActiveModal::PermissionRequest(_) | ActiveModal::ToolUseConfirm { .. } => {}
            ActiveModal::CostThreshold(_) => {
                if let Some(state) = &self.services.cost_threshold_state {
                    let width = 56u16.min(area.width.saturating_sub(4));
                    let height = 10u16;
                    let modal_area = crate::layout::center(area, width, height);
                    frame.render_widget(Clear, modal_area);
                    let widget = crate::widgets::cost_threshold::CostThresholdDialogWidget::new(
                        state,
                        &self.theme,
                    )
                    .glyphs(self.glyphs);
                    frame.render_widget(widget, modal_area);
                }
            }
            ActiveModal::IdleReturn(_) => {
                if let Some(state) = &self.services.idle_return_state {
                    let width = 60u16.min(area.width.saturating_sub(4));
                    let height = 6u16;
                    let modal_area = crate::layout::center(area, width, height);
                    frame.render_widget(Clear, modal_area);
                    let widget = crate::widgets::idle_return::IdleReturnDialogWidget::new(
                        state,
                        &self.theme,
                    )
                    .glyphs(self.glyphs);
                    frame.render_widget(widget, modal_area);
                }
            }
            ActiveModal::MessageSelector(_) => {
                if let Some(state) = &self.services.message_selector_state {
                    let width = (area.width.saturating_sub(4)).min(80);
                    let height = (area.height.saturating_sub(4)).min(20);
                    let modal_area = crate::layout::center(area, width, height);
                    frame.render_widget(Clear, modal_area);
                    let widget = crate::widgets::message_selector::MessageSelectorWidget {
                        state,
                        theme: &self.theme,
                        glyphs: self.glyphs,
                    };
                    frame.render_widget(widget, modal_area);
                }
            }
            ActiveModal::Search(query) => {
                use ratatui::style::Style;
                use ratatui::widgets::{Block, Borders, Paragraph};
                let width = (area.width.saturating_sub(4)).min(70);
                let height = 14u16;
                let modal_area = crate::layout::center(area, width, height);
                frame.render_widget(Clear, modal_area);
                let block = Block::default()
                    .title(" Search ")
                    .borders(Borders::ALL)
                    .border_set(self.glyphs.border)
                    .border_style(Style::default().fg(self.theme.border_focused));
                let inner = block.inner(modal_area);
                frame.render_widget(block, modal_area);
                // Query line
                let q_line = truncate_display_width(&format!("> {}", query), inner.width as usize);
                frame.render_widget(
                    Paragraph::new(q_line).style(Style::default().fg(self.theme.info)),
                    Rect::new(inner.x, inner.y, inner.width, 1),
                );
                // Matches preview from search_panel_state
                if let Some(panel) = &self.services.search_panel_state {
                    let transcript = self.render_transcript_model();
                    let mut y = inner.y + 2;
                    let max_y = inner.y + inner.height;
                    for (i, &msg_idx) in panel.matches.iter().take(8).enumerate() {
                        if y >= max_y {
                            break;
                        }
                        let prefix = if i == panel.selected {
                            format!("{} ", self.glyphs.selected_indicator())
                        } else {
                            "  ".to_string()
                        };
                        let preview = self.semantic_preview_for_message(&transcript, msg_idx);
                        let line = truncate_display_width(
                            &format!("{}{}", prefix, preview),
                            inner.width as usize,
                        );
                        frame.render_widget(
                            Paragraph::new(line).style(Style::default().fg(self.theme.text)),
                            Rect::new(inner.x, y, inner.width, 1),
                        );
                        y += 1;
                    }
                }
            }
            ActiveModal::HelpDialog(state) => {
                self.render_help_dialog(frame, area, state);
            }
            ActiveModal::ConfirmClear => {
                self.render_confirm_clear_dialog(frame, area);
            }
            ActiveModal::StatusDialog => {
                self.render_status_dialog(frame, area);
            }
            ActiveModal::StatusLineConfig(state) => {
                self.render_statusline_config_dialog(frame, area, state);
            }
            ActiveModal::TitleConfig(state) => {
                self.render_title_config_dialog(frame, area, state);
            }
            ActiveModal::RawTranscript(state) => {
                self.render_raw_transcript_dialog(frame, area, state);
            }
            ActiveModal::DiffReview(state) => {
                self.render_diff_review_dialog(frame, area, state);
            }
            ActiveModal::FileChanges(state) => {
                self.render_file_changes_dialog(frame, area, state);
            }
            ActiveModal::RenderTimeline(state) => {
                self.render_timeline_dialog(frame, area, state);
            }
            ActiveModal::ProcessList(state) => {
                self.render_process_list_dialog(frame, area, state);
            }
            ActiveModal::CommandHistory(state) => {
                self.render_command_history_dialog(frame, area, state);
            }
            ActiveModal::ErrorHistory(state) => {
                self.render_error_history_dialog(frame, area, state);
            }
            ActiveModal::FinalSummaryHistory(state) => {
                self.render_final_summary_history_dialog(frame, area, state);
            }
            ActiveModal::ApprovalHistory(state) => {
                self.render_approval_history_dialog(frame, area, state);
            }
            ActiveModal::DebugConfig(state) => {
                self.render_debug_config_dialog(frame, area, state);
            }
            ActiveModal::TasksDialog => {
                self.render_tasks_dialog(frame, area);
            }
            ActiveModal::McpServersDialog => {
                self.render_mcp_servers_dialog(frame, area);
            }
            ActiveModal::McpChannelApproval(request) => {
                self.render_mcp_channel_approval_dialog(frame, area, request);
            }
            ActiveModal::ModelPicker(state) => {
                let width = (area.width.saturating_sub(4)).min(76).max(42);
                let height = (area.height.saturating_sub(4)).min(22).max(10);
                let modal_area = crate::layout::center(area, width, height);
                frame.render_widget(Clear, modal_area);
                let widget = crate::widgets::panels::ModelPickerWidget::new(state, &self.theme)
                    .glyphs(self.glyphs);
                frame.render_widget(widget, modal_area);
            }
            ActiveModal::SkillsPanel(state) => {
                let width = (area.width.saturating_sub(4)).min(76).max(42);
                let height = (area.height.saturating_sub(4)).min(22).max(10);
                let modal_area = crate::layout::center(area, width, height);
                frame.render_widget(Clear, modal_area);
                let widget = crate::widgets::panels::SkillsPanelWidget::new(state, &self.theme)
                    .glyphs(self.glyphs);
                frame.render_widget(widget, modal_area);
            }
            ActiveModal::MemoryPanel(state) => {
                let width = (area.width.saturating_sub(4)).min(76).max(42);
                let height = (area.height.saturating_sub(4)).min(18).max(8);
                let modal_area = crate::layout::center(area, width, height);
                frame.render_widget(Clear, modal_area);
                let widget = crate::widgets::panels::MemoryPanelWidget::new(state, &self.theme)
                    .glyphs(self.glyphs);
                frame.render_widget(widget, modal_area);
            }
            ActiveModal::CommandOutput {
                title,
                body,
                is_error,
            } => {
                self.render_command_output_dialog(
                    frame,
                    area,
                    title,
                    body,
                    *is_error,
                    self.command_output_scroll,
                );
            }
            ActiveModal::Picker {
                title,
                items,
                selected,
                ..
            } => {
                use ratatui::style::{Modifier, Style};
                use ratatui::widgets::{Block, Borders, Clear, Paragraph};
                let width = 50u16.min(area.width.saturating_sub(4));
                let height = (items.len() as u16 + 2)
                    .min(area.height.saturating_sub(4))
                    .max(4);
                let modal_area = crate::layout::center(area, width, height);
                frame.render_widget(Clear, modal_area);
                let block = Block::default()
                    .title(format!(" {} ", title))
                    .borders(Borders::ALL)
                    .border_set(self.glyphs.border)
                    .border_style(Style::default().fg(self.theme.border_focused));
                let inner = block.inner(modal_area);
                frame.render_widget(block, modal_area);
                for (i, label) in items.iter().enumerate() {
                    if i as u16 >= inner.height {
                        break;
                    }
                    let style = if i == *selected {
                        Style::default()
                            .fg(self.theme.background)
                            .bg(self.theme.border_focused)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(self.theme.text)
                    };
                    let prefix = if i == *selected {
                        format!("{} ", self.glyphs.selected_indicator())
                    } else {
                        "  ".to_string()
                    };
                    let line = truncate_display_width(
                        &format!("{}{}", prefix, label),
                        inner.width as usize,
                    );
                    frame.render_widget(
                        Paragraph::new(line).style(style),
                        Rect::new(inner.x, inner.y + i as u16, inner.width, 1),
                    );
                }
            }
        }
    }

    fn render_help_dialog(&self, frame: &mut Frame, area: Rect, state: &HelpDialogState) {
        use ratatui::style::{Modifier, Style};
        use ratatui::text::{Line, Span};
        use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

        let width = (area.width.saturating_sub(4)).min(88).max(40);
        let height = help_dialog_modal_height(area.height);
        let modal_area = crate::layout::center(area, width, height);
        frame.render_widget(Clear, modal_area);
        let block = Block::default()
            .title(" Mossen Help ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.border_focused));
        let inner = block.inner(modal_area);
        frame.render_widget(block, modal_area);

        let footer_height = 1u16.min(inner.height);
        let body_height = inner.height.saturating_sub(footer_height);
        let body_area = Rect::new(inner.x, inner.y, inner.width, body_height);
        let footer_area = Rect::new(
            inner.x,
            inner.y.saturating_add(body_height),
            inner.width,
            footer_height,
        );
        let lines = self.help_dialog_content_lines(&state.query, inner.width as usize);
        let total_rows = lines.len();
        let viewport_rows = body_area.height as usize;
        let max_scroll = help_scroll_max(total_rows, viewport_rows);
        let start = state.scroll.min(max_scroll);
        let end = start.saturating_add(viewport_rows).min(total_rows);
        let visible: Vec<Line> = lines.into_iter().skip(start).take(viewport_rows).collect();

        frame.render_widget(
            Paragraph::new(visible).wrap(Wrap { trim: false }),
            body_area,
        );
        self.render_modal_scrollbar(
            frame,
            body_area,
            ModalScrollbarKind::Help,
            total_rows,
            viewport_rows,
            start,
        );

        if footer_height > 0 {
            let showing = if total_rows == 0 {
                "0/0".to_string()
            } else {
                format!("{}-{}/{}", start + 1, end.max(start + 1), total_rows)
            };
            let footer = format!("{}  Up/Down PgUp/PgDn scroll · Esc closes", showing);
            frame.render_widget(
                Paragraph::new(truncate_display_width(&footer, footer_area.width as usize))
                    .style(Style::default().fg(self.theme.text_subtle)),
                footer_area,
            );
        }
    }

    fn help_dialog_content_line_count(&self, query: &str) -> usize {
        self.help_dialog_content_lines(query, 88).len()
    }

    fn help_dialog_content_lines(
        &self,
        query: &str,
        width: usize,
    ) -> Vec<ratatui::text::Line<'static>> {
        use ratatui::style::{Modifier, Style};
        use ratatui::text::{Line, Span};

        let query = query.trim();
        let mut lines: Vec<Line<'static>> = Vec::new();
        if !query.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("Filter: ", Style::default().fg(self.theme.text_subtle)),
                Span::styled(query.to_string(), Style::default().fg(self.theme.info)),
            ]));
            lines.push(Line::from(""));
        }

        let mut current_category = String::new();
        for entry in self
            .state
            .all_slash_commands
            .iter()
            .filter(|entry| slash_help_query_matches(entry, query))
        {
            if entry.category != current_category {
                if !lines.is_empty() && !matches!(lines.last(), Some(line) if line.spans.is_empty())
                {
                    lines.push(Line::from(""));
                }
                current_category = entry.category.clone();
                lines.push(Line::from(Span::styled(
                    current_category.clone(),
                    Style::default()
                        .fg(self.theme.primary)
                        .add_modifier(Modifier::BOLD),
                )));
            }
            let name_style = match entry.kind {
                SlashCommandKind::Command => Style::default()
                    .fg(self.theme.text)
                    .add_modifier(Modifier::BOLD),
                SlashCommandKind::Skill => Style::default()
                    .fg(self.theme.info)
                    .add_modifier(Modifier::BOLD),
            };
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    pad_display_width(&slash_command_usage_label(entry), 28),
                    name_style,
                ),
                Span::styled(
                    truncate_display_width(&entry.description, width.saturating_sub(32)),
                    Style::default().fg(self.theme.text_dim),
                ),
            ]));
            let metadata = slash_catalog_metadata(entry);
            if !metadata.is_empty() {
                lines.push(Line::from(vec![
                    Span::raw("    "),
                    Span::styled(
                        truncate_display_width(&metadata, width.saturating_sub(6)),
                        Style::default().fg(self.theme.text_subtle),
                    ),
                ]));
            }
        }
        if lines.is_empty() || (lines.len() == 2 && !query.is_empty()) {
            lines.push(Line::from(if query.is_empty() {
                "No commands are registered.".to_string()
            } else {
                format!("No commands match \"{query}\".")
            }));
        }
        lines
    }

    fn render_confirm_clear_dialog(&self, frame: &mut Frame, area: Rect) {
        use ratatui::style::{Modifier, Style};
        use ratatui::text::{Line, Span};
        use ratatui::widgets::{Block, Borders, Clear, Paragraph};

        let modal_area = crate::layout::center(area, 44u16.min(area.width.saturating_sub(4)), 6);
        frame.render_widget(Clear, modal_area);
        let block = Block::default()
            .title(" Clear Conversation? ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.warning));
        let inner = block.inner(modal_area);
        frame.render_widget(block, modal_area);
        let lines = vec![
            Line::from(format!(
                "Current {} messages will be cleared.",
                self.messages.len()
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    "Enter",
                    Style::default()
                        .fg(self.theme.warning)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" confirm   "),
                Span::styled(
                    "Esc",
                    Style::default()
                        .fg(self.theme.text_dim)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" cancel"),
            ]),
        ];
        frame.render_widget(Paragraph::new(lines), inner);
    }

    fn render_status_dialog(&self, frame: &mut Frame, area: Rect) {
        let width = (area.width.saturating_sub(4)).min(96).max(52);
        let height = (area.height.saturating_sub(4)).min(28).max(12);
        let modal_area = crate::layout::center(area, width, height);
        frame.render_widget(Clear, modal_area);
        let model = self.status_overview_render_model();
        let widget = StatusOverviewWidget::new(&model, &self.theme).glyphs(self.glyphs);
        frame.render_widget(widget, modal_area);
    }

    fn render_statusline_config_dialog(
        &self,
        frame: &mut Frame,
        area: Rect,
        state: &StatusLineConfigState,
    ) {
        use ratatui::style::{Modifier, Style};
        use ratatui::text::{Line, Span};
        use ratatui::widgets::{Block, Borders, Clear, Paragraph};

        let width = (area.width.saturating_sub(4)).min(60).max(40);
        let height = (state.items.len() as u16 + 7)
            .min(area.height.saturating_sub(4))
            .max(8);
        let modal_area = crate::layout::center(area, width, height);
        frame.render_widget(Clear, modal_area);
        let block = Block::default()
            .title(" Status Line ")
            .borders(Borders::ALL)
            .border_set(self.glyphs.border)
            .border_style(Style::default().fg(self.theme.border_focused));
        let inner = block.inner(modal_area);
        frame.render_widget(block, modal_area);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(vec![
            Span::raw("  [x] "),
            Span::styled(
                pad_display_width("Core status", 16),
                Style::default()
                    .fg(self.theme.primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" locked", Style::default().fg(self.theme.text_subtle)),
        ]));
        let preset = self.state.footer_config.preset_label();
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                pad_display_width("Preset", 16),
                Style::default()
                    .fg(self.theme.primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(preset, Style::default().fg(self.theme.text)),
        ]));
        let preset_help = FooterPreset::ALL
            .iter()
            .map(|preset| format!("{} {}", preset.key_hint(), preset.label().to_lowercase()))
            .collect::<Vec<_>>()
            .join(" · ");
        lines.push(Line::from(Span::styled(
            truncate_display_width(&format!("  {preset_help}"), inner.width as usize),
            Style::default().fg(self.theme.text_subtle),
        )));
        lines.push(Line::from(""));

        let available_rows = inner.height.saturating_sub(1) as usize;
        for (idx, item) in state.items.iter().copied().enumerate() {
            if lines.len() >= available_rows {
                break;
            }
            let enabled = self.state.footer_config.is_enabled(item);
            let selected = idx == state.selected;
            let marker = if selected {
                self.glyphs.selected_indicator()
            } else {
                " "
            };
            let check = if enabled { "[x]" } else { "[ ]" };
            let mut style = Style::default().fg(self.theme.text);
            if selected {
                style = Style::default()
                    .fg(self.theme.background)
                    .bg(self.theme.border_focused)
                    .add_modifier(Modifier::BOLD);
            } else if !enabled {
                style = Style::default().fg(self.theme.text_subtle);
            }
            let line = truncate_display_width(
                &format!("{marker} {check} {}", item.label()),
                inner.width as usize,
            );
            lines.push(Line::from(Span::styled(line, style)));
        }

        while lines.len() + 1 < inner.height as usize {
            lines.push(Line::from(""));
        }
        if lines.len() < inner.height as usize {
            lines.push(Line::from(Span::styled(
                truncate_display_width(
                    "Space toggles · M/C/D/F apply presets · Esc closes",
                    inner.width as usize,
                ),
                Style::default()
                    .fg(self.theme.text_subtle)
                    .add_modifier(Modifier::ITALIC),
            )));
        }

        frame.render_widget(Paragraph::new(lines), inner);
    }

    fn render_title_config_dialog(&self, frame: &mut Frame, area: Rect, state: &TitleConfigState) {
        let width = (area.width.saturating_sub(4)).min(84).max(42);
        let height = (area.height.saturating_sub(4)).min(10).max(8);
        let modal_area = crate::layout::center(area, width, height);
        frame.render_widget(Clear, modal_area);
        let model = self.session_title_render_model(state);
        let widget = SessionTitleWidget::new(&model, &self.theme).glyphs(self.glyphs);
        frame.render_widget(widget, modal_area);
    }

    fn render_raw_transcript_dialog(
        &self,
        frame: &mut Frame,
        area: Rect,
        state: &RawTranscriptState,
    ) {
        use ratatui::style::{Modifier, Style};
        use ratatui::text::{Line, Span};
        use ratatui::widgets::{Block, Borders, Clear, Paragraph};

        let width = (area.width.saturating_sub(4)).min(96).max(48);
        let height = raw_transcript_modal_height(area.height);
        let modal_area = crate::layout::center(area, width, height);
        frame.render_widget(Clear, modal_area);
        let block = Block::default()
            .title(" Raw Transcript ")
            .borders(Borders::ALL)
            .border_set(self.glyphs.border)
            .border_style(Style::default().fg(self.theme.border_focused));
        let inner = block.inner(modal_area);
        frame.render_widget(block, modal_area);

        if inner.height == 0 || inner.width == 0 {
            return;
        }

        let viewport_height = inner.height.saturating_sub(1) as usize;
        let mut lines: Vec<Line> = Vec::new();
        for raw_line in state.lines.iter().skip(state.scroll).take(viewport_height) {
            let style = if raw_line.starts_with("message ")
                || raw_line.starts_with("record ")
                || raw_line.starts_with("sidecar ")
                || raw_line.starts_with("visible ")
            {
                Style::default()
                    .fg(self.theme.primary)
                    .add_modifier(Modifier::BOLD)
            } else if raw_line.starts_with("  content:")
                || raw_line.starts_with("  full_content:")
                || raw_line.starts_with("  thinking:")
            {
                Style::default().fg(self.theme.warning)
            } else {
                Style::default().fg(self.theme.text_dim)
            };
            lines.push(Line::from(Span::styled(
                truncate_display_width(raw_line, inner.width as usize),
                style,
            )));
        }

        while lines.len() < viewport_height {
            lines.push(Line::from(""));
        }

        let footer = format!(
            "Esc closes  {}/{}",
            state.scroll.saturating_add(1).min(state.lines.len().max(1)),
            state.lines.len().max(1)
        );
        lines.push(Line::from(Span::styled(
            truncate_display_width(&footer, inner.width as usize),
            Style::default().fg(self.theme.text_subtle),
        )));
        frame.render_widget(Paragraph::new(lines), inner);
        self.render_modal_scrollbar(
            frame,
            Rect::new(
                inner.x,
                inner.y,
                inner.width,
                inner.height.saturating_sub(1),
            ),
            ModalScrollbarKind::RawTranscript,
            state.lines.len().max(1),
            viewport_height,
            state.scroll,
        );
    }

    fn render_diff_review_dialog(&self, frame: &mut Frame, area: Rect, state: &DiffReviewState) {
        use ratatui::style::{Modifier, Style};
        use ratatui::text::{Line, Span};
        use ratatui::widgets::{Block, Borders, Clear, Paragraph};

        let width = (area.width.saturating_sub(4)).min(110).max(44);
        let height = diff_review_modal_height(area.height);
        let modal_area = crate::layout::center(area, width, height);
        frame.render_widget(Clear, modal_area);
        let block = Block::default()
            .title(" Diff Review ")
            .borders(Borders::ALL)
            .border_set(self.glyphs.border)
            .border_style(Style::default().fg(self.theme.border_focused));
        let inner = block.inner(modal_area);
        frame.render_widget(block, modal_area);

        if inner.height == 0 || inner.width == 0 {
            return;
        }

        if state.files.is_empty() {
            frame.render_widget(
                Paragraph::new("No semantic diff is available.")
                    .style(Style::default().fg(self.theme.text_dim)),
                inner,
            );
            return;
        }

        let content_height = inner.height.saturating_sub(1);
        let content_area = Rect::new(inner.x, inner.y, inner.width, content_height);
        let selected = state.selected_file.min(state.files.len().saturating_sub(1));
        let viewport_rows = content_area.height as usize;
        let mut visible_state = state.clone();
        visible_state.selected_file = selected;
        visible_state.scroll = visible_state
            .scroll
            .min(visible_state.max_scroll(viewport_rows));
        let widget = DiffDialogWidget::new(&visible_state.files, &self.theme)
            .selected_file(selected)
            .scroll(visible_state.scroll)
            .collapsed_files(&visible_state.collapsed_files);
        frame.render_widget(widget, content_area);
        self.render_modal_scrollbar(
            frame,
            content_area,
            ModalScrollbarKind::DiffReview,
            visible_state.selected_line_count(),
            viewport_rows,
            visible_state.scroll,
        );

        if inner.height > 1 {
            let selected_label = format!("{}/{}", selected + 1, state.files.len());
            let fold = if visible_state.collapsed_files.contains(&selected) {
                "collapsed"
            } else {
                "expanded"
            };
            let total_rows = visible_state.selected_line_count();
            let end = visible_state
                .scroll
                .saturating_add(viewport_rows)
                .min(total_rows);
            let showing = if total_rows == 0 {
                "0/0".to_string()
            } else {
                format!(
                    "{}-{}/{}",
                    visible_state.scroll + 1,
                    end.max(visible_state.scroll + 1),
                    total_rows
                )
            };
            let footer = format!(
                "{showing}  Left/Right files · Up/Down PgUp/PgDn scroll · Space fold · Esc closes · {selected_label} · {fold}"
            );
            let footer = truncate_display_width(&footer, inner.width as usize);
            let footer_area = Rect::new(inner.x, inner.y + content_height, inner.width, 1);
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    footer,
                    Style::default()
                        .fg(self.theme.text_subtle)
                        .add_modifier(Modifier::ITALIC),
                ))),
                footer_area,
            );
        }
    }

    fn render_file_changes_dialog(&self, frame: &mut Frame, area: Rect, state: &FileChangesState) {
        use ratatui::widgets::Clear;

        let width = (area.width.saturating_sub(4)).min(96).max(44);
        let height = file_changes_modal_height(area.height);
        let modal_area = crate::layout::center(area, width, height);
        frame.render_widget(Clear, modal_area);
        let mut visible_state = state.clone();
        visible_state.ensure_visible(file_changes_content_viewport_rows(area.height));
        let widget = FileChangesWidget::new(&state.model, &self.theme)
            .glyphs(self.glyphs)
            .selected(visible_state.selected)
            .scroll(visible_state.scroll);
        frame.render_widget(widget, modal_area);
        self.render_modal_scrollbar(
            frame,
            list_modal_scroll_area(modal_area),
            ModalScrollbarKind::FileChanges,
            state.model.len(),
            file_changes_content_viewport_rows(area.height),
            visible_state.scroll,
        );
    }

    fn render_timeline_dialog(&self, frame: &mut Frame, area: Rect, state: &RenderTimelineState) {
        use ratatui::widgets::Clear;

        let width = (area.width.saturating_sub(4)).min(104).max(44);
        let height = render_timeline_modal_height(area.height);
        let modal_area = crate::layout::center(area, width, height);
        let mut visible_state = state.clone();
        visible_state.ensure_visible(render_timeline_content_viewport_rows(area.height));
        frame.render_widget(Clear, modal_area);
        let widget = RenderTimelineWidget::new(&visible_state.model, &self.theme)
            .glyphs(self.glyphs)
            .selected(visible_state.selected)
            .scroll(visible_state.scroll);
        frame.render_widget(widget, modal_area);
        self.render_modal_scrollbar(
            frame,
            list_modal_scroll_area(modal_area),
            ModalScrollbarKind::RenderTimeline,
            visible_state.model.len(),
            render_timeline_content_viewport_rows(area.height),
            visible_state.scroll,
        );
    }

    fn render_process_list_dialog(&self, frame: &mut Frame, area: Rect, state: &ProcessListState) {
        use ratatui::widgets::Clear;

        let width = (area.width.saturating_sub(4)).min(96).max(44);
        let height = process_list_modal_height(area.height);
        let modal_area = crate::layout::center(area, width, height);
        frame.render_widget(Clear, modal_area);
        let mut visible_state = state.clone();
        visible_state.ensure_visible(process_list_content_viewport_rows(area.height));
        let widget = ProcessListWidget::new(&visible_state.model, &self.theme)
            .glyphs(self.glyphs)
            .selected(visible_state.selected)
            .scroll(visible_state.scroll);
        frame.render_widget(widget, modal_area);
        self.render_modal_scrollbar(
            frame,
            list_modal_scroll_area(modal_area),
            ModalScrollbarKind::ProcessList,
            visible_state.model.len(),
            process_list_content_viewport_rows(area.height),
            visible_state.scroll,
        );
    }

    fn render_command_history_dialog(
        &self,
        frame: &mut Frame,
        area: Rect,
        state: &CommandHistoryState,
    ) {
        use ratatui::widgets::Clear;

        let width = (area.width.saturating_sub(4)).min(104).max(48);
        let height = command_history_modal_height(area.height);
        let modal_area = crate::layout::center(area, width, height);
        frame.render_widget(Clear, modal_area);
        let mut visible_state = state.clone();
        visible_state.ensure_visible(command_history_content_viewport_rows(area.height));
        let widget = CommandHistoryWidget::new(&visible_state.model, &self.theme)
            .glyphs(self.glyphs)
            .selected(visible_state.selected)
            .scroll(visible_state.scroll)
            .expanded(visible_state.selected_expanded())
            .detail_scroll(visible_state.detail_scroll);
        frame.render_widget(widget, modal_area);
        if !visible_state.selected_expanded() {
            self.render_modal_scrollbar(
                frame,
                list_modal_scroll_area(modal_area),
                ModalScrollbarKind::CommandHistory,
                visible_state.model.len(),
                command_history_content_viewport_rows(area.height),
                visible_state.scroll,
            );
        }
    }

    fn render_error_history_dialog(
        &self,
        frame: &mut Frame,
        area: Rect,
        state: &ErrorHistoryState,
    ) {
        use ratatui::widgets::Clear;

        let width = (area.width.saturating_sub(4)).min(104).max(48);
        let height = error_history_modal_height(area.height);
        let modal_area = crate::layout::center(area, width, height);
        frame.render_widget(Clear, modal_area);
        let mut visible_state = state.clone();
        visible_state.ensure_visible(error_history_content_viewport_rows(area.height));
        let widget = ErrorHistoryWidget::new(&visible_state.model, &self.theme)
            .glyphs(self.glyphs)
            .selected(visible_state.selected)
            .scroll(visible_state.scroll)
            .expanded(visible_state.selected_expanded())
            .detail_scroll(visible_state.detail_scroll);
        frame.render_widget(widget, modal_area);
        if !visible_state.selected_expanded() {
            self.render_modal_scrollbar(
                frame,
                list_modal_scroll_area(modal_area),
                ModalScrollbarKind::ErrorHistory,
                visible_state.model.len(),
                error_history_content_viewport_rows(area.height),
                visible_state.scroll,
            );
        }
    }

    fn render_final_summary_history_dialog(
        &self,
        frame: &mut Frame,
        area: Rect,
        state: &FinalSummaryHistoryState,
    ) {
        use ratatui::widgets::Clear;

        let width = (area.width.saturating_sub(4)).min(108).max(50);
        let height = final_summary_history_modal_height(area.height);
        let modal_area = crate::layout::center(area, width, height);
        frame.render_widget(Clear, modal_area);
        let mut visible_state = state.clone();
        visible_state.ensure_visible(final_summary_history_content_viewport_rows(area.height));
        let widget = SummaryHistoryWidget::new(&visible_state.model, &self.theme)
            .glyphs(self.glyphs)
            .selected(visible_state.selected)
            .scroll(visible_state.scroll)
            .expanded(visible_state.selected_expanded())
            .detail_scroll(visible_state.detail_scroll);
        frame.render_widget(widget, modal_area);
        if !visible_state.selected_expanded() {
            self.render_modal_scrollbar(
                frame,
                list_modal_scroll_area(modal_area),
                ModalScrollbarKind::FinalSummaryHistory,
                visible_state.model.len(),
                final_summary_history_content_viewport_rows(area.height),
                visible_state.scroll,
            );
        }
    }

    fn render_approval_history_dialog(
        &self,
        frame: &mut Frame,
        area: Rect,
        state: &ApprovalHistoryState,
    ) {
        use ratatui::widgets::Clear;

        let width = (area.width.saturating_sub(4)).min(104).max(48);
        let height = approval_history_modal_height(area.height);
        let modal_area = crate::layout::center(area, width, height);
        frame.render_widget(Clear, modal_area);
        let mut visible_state = state.clone();
        visible_state.ensure_visible(approval_history_content_viewport_rows(area.height));
        let widget = ApprovalHistoryWidget::new(&visible_state.model, &self.theme)
            .glyphs(self.glyphs)
            .selected(visible_state.selected)
            .scroll(visible_state.scroll)
            .expanded(visible_state.selected_expanded())
            .detail_scroll(visible_state.detail_scroll);
        frame.render_widget(widget, modal_area);
        if !visible_state.selected_expanded() {
            self.render_modal_scrollbar(
                frame,
                list_modal_scroll_area(modal_area),
                ModalScrollbarKind::ApprovalHistory,
                visible_state.model.len(),
                approval_history_content_viewport_rows(area.height),
                visible_state.scroll,
            );
        }
    }

    fn render_debug_config_dialog(&self, frame: &mut Frame, area: Rect, state: &DebugConfigState) {
        use ratatui::widgets::Clear;

        let width = (area.width.saturating_sub(4)).min(108).max(50);
        let height = debug_config_modal_height(area.height);
        let modal_area = crate::layout::center(area, width, height);
        let viewport_rows = debug_config_content_viewport_rows(area.height);
        let visible_scroll = state.visible_scroll(viewport_rows);
        frame.render_widget(Clear, modal_area);
        let widget = DebugConfigWidget::new(&state.model, &self.theme)
            .glyphs(self.glyphs)
            .scroll(visible_scroll);
        frame.render_widget(widget, modal_area);
        self.render_modal_scrollbar(
            frame,
            list_modal_scroll_area(modal_area),
            ModalScrollbarKind::DebugConfig,
            state.model.row_count(),
            viewport_rows,
            visible_scroll,
        );
    }

    fn render_tasks_dialog(&self, frame: &mut Frame, area: Rect) {
        use ratatui::style::{Modifier, Style};
        use ratatui::text::{Line, Span};
        use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

        let width = (area.width.saturating_sub(4)).min(76).max(42);
        let height = (area.height.saturating_sub(4)).min(22).max(10);
        let modal_area = crate::layout::center(area, width, height);
        frame.render_widget(Clear, modal_area);
        let block = Block::default()
            .title(" Tasks ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.border_focused));
        let inner = block.inner(modal_area);
        frame.render_widget(block, modal_area);

        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(Span::styled(
            format!("TodoWrite tasks ({})", self.state.task_list.tasks.len()),
            Style::default()
                .fg(self.theme.primary)
                .add_modifier(Modifier::BOLD),
        )));
        if self.state.task_list.tasks.is_empty() {
            lines.push(Line::from(Span::styled(
                "  No TodoWrite tasks yet.",
                Style::default().fg(self.theme.text_dim),
            )));
        } else {
            for task in &self.state.task_list.tasks {
                let status = task.status.to_lowercase();
                let glyph = if status.contains("completed") {
                    "x"
                } else if status.contains("progress") {
                    "~"
                } else {
                    " "
                };
                let prefix = format!("  [{}] ", glyph);
                let status_text = format!("  {}", status);
                let content_budget = (inner.width as usize)
                    .saturating_sub(UnicodeWidthStr::width(prefix.as_str()))
                    .saturating_sub(UnicodeWidthStr::width(status_text.as_str()));
                let content = truncate_display_width(&task.content, content_budget);
                lines.push(Line::from(vec![
                    Span::styled(prefix, Style::default().fg(self.theme.info)),
                    Span::styled(content, Style::default().fg(self.theme.text)),
                    Span::styled(status_text, Style::default().fg(self.theme.text_dim)),
                ]));
            }
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("Background agents ({})", self.state.teammate_states.len()),
            Style::default()
                .fg(self.theme.primary)
                .add_modifier(Modifier::BOLD),
        )));
        if self.state.teammate_states.is_empty() {
            lines.push(Line::from(Span::styled(
                "  No background agents.",
                Style::default().fg(self.theme.text_dim),
            )));
        } else {
            for (id, state) in &self.state.teammate_states {
                let label = match state {
                    TeammateState::Running => "running",
                    TeammateState::Completed(_) => "completed",
                    TeammateState::Failed(_) => "failed",
                };
                let prefix = "  ";
                let label_text = format!("  {}", label);
                let id_budget = (inner.width as usize)
                    .saturating_sub(UnicodeWidthStr::width(prefix))
                    .saturating_sub(UnicodeWidthStr::width(label_text.as_str()));
                let id = truncate_display_width(id, id_budget);
                lines.push(Line::from(vec![
                    Span::raw(prefix),
                    Span::styled(id, Style::default().fg(self.theme.text)),
                    Span::styled(label_text, Style::default().fg(self.theme.text_dim)),
                ]));
            }
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Esc closes",
            Style::default().fg(self.theme.text_subtle),
        )));
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
    }

    fn render_mcp_servers_dialog(&self, frame: &mut Frame, area: Rect) {
        use ratatui::style::{Modifier, Style};
        use ratatui::text::{Line, Span};
        use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

        let width = (area.width.saturating_sub(4)).min(78).max(42);
        let height = (area.height.saturating_sub(4)).min(18).max(8);
        let modal_area = crate::layout::center(area, width, height);
        frame.render_widget(Clear, modal_area);
        let block = Block::default()
            .title(" MCP Servers ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.border_focused));
        let inner = block.inner(modal_area);
        frame.render_widget(block, modal_area);

        let mut lines: Vec<Line> = Vec::new();
        if self.state.mcp_servers.is_empty() {
            lines.push(Line::from("No MCP servers configured for this project."));
            lines.push(Line::from(Span::styled(
                "Checked .mossen/mcp.json in the current working directory.",
                Style::default().fg(self.theme.text_dim),
            )));
        } else {
            for server in &self.state.mcp_servers {
                let prefix_width = 12usize;
                let name_width = 18usize;
                let detail_budget = (inner.width as usize)
                    .saturating_sub(prefix_width)
                    .saturating_sub(name_width);
                let details = format!(
                    "{}  {} tools  {} prompts  {} resources  {}",
                    server.transport,
                    server.tools_count,
                    server.prompts_count,
                    server.resources_count,
                    server.scope
                );
                lines.push(Line::from(vec![
                    Span::styled(
                        pad_display_width(server.state.label(), prefix_width),
                        Style::default()
                            .fg(self.theme.info)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        pad_display_width(&server.name, name_width),
                        Style::default().fg(self.theme.text),
                    ),
                    Span::styled(
                        truncate_display_width(&details, detail_budget),
                        Style::default().fg(self.theme.text_dim),
                    ),
                ]));
                if let Some(err) = &server.last_error {
                    lines.push(Line::from(Span::styled(
                        format!("  error: {}", err),
                        Style::default().fg(self.theme.error),
                    )));
                }
            }
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Esc closes",
            Style::default().fg(self.theme.text_subtle),
        )));
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
    }

    fn render_command_output_dialog(
        &self,
        frame: &mut Frame,
        area: Rect,
        title: &str,
        body: &str,
        is_error: bool,
        scroll: usize,
    ) {
        use ratatui::style::{Modifier, Style};
        use ratatui::text::{Line, Span};
        use ratatui::widgets::{Block, Borders, Clear, Paragraph};

        let width = (area.width.saturating_sub(4)).min(84).max(42);
        let height = command_output_modal_height(area.height);
        let modal_area = crate::layout::center(area, width, height);
        frame.render_widget(Clear, modal_area);
        let border = if is_error {
            self.theme.error
        } else {
            self.theme.border_focused
        };
        let block = Block::default()
            .title(format!(" {} ", title))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border));
        let inner = block.inner(modal_area);
        frame.render_widget(block, modal_area);

        let footer_height = 1u16.min(inner.height);
        let body_height = inner.height.saturating_sub(footer_height);
        let body_area = Rect::new(inner.x, inner.y, inner.width, body_height);
        let footer_area = Rect::new(
            inner.x,
            inner.y.saturating_add(body_height),
            inner.width,
            footer_height,
        );

        let content_width = inner.width as usize;
        let mut lines: Vec<Line> = body
            .lines()
            .map(|line| {
                Line::from(Span::styled(
                    truncate_display_width(line, content_width),
                    Style::default().fg(if is_error {
                        self.theme.error
                    } else {
                        self.theme.text
                    }),
                ))
            })
            .collect();
        if lines.is_empty() {
            lines.push(Line::from(""));
        }
        let total_rows = lines.len();
        let viewport_rows = body_area.height as usize;
        let max_scroll = command_output_scroll_max(total_rows, viewport_rows);
        let start = scroll.min(max_scroll);
        let end = start.saturating_add(viewport_rows).min(total_rows);
        let visible: Vec<Line> = lines.into_iter().skip(start).take(viewport_rows).collect();

        frame.render_widget(Paragraph::new(visible), body_area);
        self.render_modal_scrollbar(
            frame,
            body_area,
            ModalScrollbarKind::CommandOutput,
            total_rows,
            viewport_rows,
            start,
        );

        if footer_height > 0 {
            let showing = if total_rows == 0 {
                "0/0".to_string()
            } else {
                format!("{}-{}/{}", start + 1, end.max(start + 1), total_rows)
            };
            let footer = format!("{}  Up/Down PgUp/PgDn scroll · Esc closes", showing);
            frame.render_widget(
                Paragraph::new(truncate_display_width(&footer, footer_area.width as usize)).style(
                    Style::default()
                        .fg(self.theme.text_subtle)
                        .add_modifier(Modifier::ITALIC),
                ),
                footer_area,
            );
        }
    }

    fn render_mcp_channel_approval_dialog(
        &self,
        frame: &mut Frame,
        area: Rect,
        request: &mossen_agent::mcp::channel_approval::ChannelApprovalRequest,
    ) {
        use ratatui::style::{Modifier, Style};
        use ratatui::text::{Line, Span};
        use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

        let width = (area.width.saturating_sub(4)).min(76).max(44);
        let height = (area.height.saturating_sub(4)).min(13).max(8);
        let modal_area = crate::layout::center(area, width, height);
        frame.render_widget(Clear, modal_area);
        let block = Block::default()
            .title(" MCP Channel Approval ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.warning));
        let inner = block.inner(modal_area);
        frame.render_widget(block, modal_area);

        let label = match (&request.plugin, &request.marketplace) {
            (Some(plugin), Some(marketplace)) => format!("{}@{}", plugin, marketplace),
            (Some(plugin), None) => plugin.clone(),
            _ => request.server_name.clone(),
        };
        let lines = vec![
            Line::from(vec![
                Span::styled("Channel ", Style::default().fg(self.theme.text_dim)),
                Span::styled(label, Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(
                    " wants to register for this session.",
                    Style::default().fg(self.theme.text),
                ),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("Server  ", Style::default().fg(self.theme.text_dim)),
                Span::raw(request.server_name.clone()),
            ]),
            Line::from(vec![
                Span::styled("Reason  ", Style::default().fg(self.theme.text_dim)),
                Span::raw(request.reason.clone()),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "Enter allows for this session. Esc denies.",
                Style::default().fg(self.theme.text_subtle),
            )),
            Line::from(Span::styled(
                "After allowing, use /mcp to reconnect or refresh the server.",
                Style::default().fg(self.theme.text_subtle),
            )),
        ];
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
    }

    /// Handle an application event.
    fn handle_event_batch(&mut self, events: Vec<AppEvent>) {
        for event in events {
            self.handle_event(event);
            if self.should_quit {
                break;
            }
        }
    }

    /// Handle an application event.
    fn handle_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::Key(key) => {
                let transcript_page_key = !self.active_modal.is_open()
                    && matches!(
                        InputAction::from_key_event(&key),
                        Some(InputAction::PageUp | InputAction::PageDown)
                    );
                let transcript_arrow_key = !self.active_modal.is_open()
                    && self.prompt.input.value.is_empty()
                    && !self.prompt.show_suggestions
                    && key.modifiers.is_empty()
                    && matches!(
                        key.code,
                        crossterm::event::KeyCode::Up | crossterm::event::KeyCode::Down
                    );
                let transcript_focus_key = !self.active_modal.is_open()
                    && self.prompt.input.value.is_empty()
                    && !self.prompt.show_suggestions
                    && !self.state.is_streaming
                    && key.modifiers.contains(crossterm::event::KeyModifiers::ALT)
                    && matches!(
                        key.code,
                        crossterm::event::KeyCode::Up | crossterm::event::KeyCode::Down
                    );
                let before_transcript_key_state = (
                    self.focused_message_idx,
                    self.scroll.offset,
                    self.scroll.sticky,
                );
                self.handle_key(key);
                if !(transcript_page_key || transcript_arrow_key || transcript_focus_key)
                    || before_transcript_key_state
                        != (
                            self.focused_message_idx,
                            self.scroll.offset,
                            self.scroll.sticky,
                        )
                {
                    self.mark_render_dirty();
                }
            }
            AppEvent::Mouse(mouse) => {
                self.services.note_interaction();
                if self.handle_mouse(mouse) {
                    self.mark_render_dirty();
                }
            }
            AppEvent::Resize { width, height } => {
                let before_resize_state = (
                    self.state.terminal_width,
                    self.state.terminal_height,
                    self.scroll.viewport_height,
                    self.scroll.visible_count,
                    self.scroll.offset,
                    self.scroll.sticky,
                );
                self.state.terminal_width = width;
                self.state.terminal_height = height;
                self.scroll.set_viewport_height(height.saturating_sub(4));
                let after_resize_state = (
                    self.state.terminal_width,
                    self.state.terminal_height,
                    self.scroll.viewport_height,
                    self.scroll.visible_count,
                    self.scroll.offset,
                    self.scroll.sticky,
                );
                if before_resize_state != after_resize_state {
                    self.mark_render_dirty();
                }
            }
            AppEvent::Tick => {
                let before = self.render_tick_fingerprint();
                // Tick animations
                self.spinner.frame_index(4); // keep animation running
                                             // Drive terminal services (chrome on streaming edges, idle dialog,
                                             // cost threshold). The services_* methods take a separate
                                             // `&mut TerminalServices` arg, so we temporarily move it out via
                                             // mem::take to satisfy the borrow checker.
                let mut svc = std::mem::take(&mut self.services);
                // Pulls the live total cost the engine reports via
                // `SdkMessage::Result.cost_usd` — see `handle_engine_message`.
                let current_cost = self.total_cost_usd;
                let _escape = self.services_tick(&mut svc, current_cost);
                self.services = svc;
                self.poll_external_statusline_command();

                // Drain a pending `PermissionRequest` off the gate channel
                // when no other modal is up. The gate stays blocked on the
                // oneshot until the user clicks Allow / Deny in the modal,
                // so we only ever surface one request at a time.
                self.poll_permission_request();
                self.poll_mcp_channel_approval();
                self.launch_pending_compact_task();
                self.poll_compact_result();
                self.poll_task_notifications();
                self.refresh_slash_catalog_with_notice(true);
                if self.render_tick_fingerprint() != before {
                    self.mark_render_dirty();
                }
            }
            AppEvent::FocusChange(focused) => {
                let before = self.render_tick_fingerprint();
                let mut svc = std::mem::take(&mut self.services);
                self.services_on_focus_change(&mut svc, focused);
                self.services = svc;
                if self.render_tick_fingerprint() != before {
                    self.mark_render_dirty();
                }
            }
            AppEvent::Quit => {
                self.should_quit = true;
                self.mark_render_dirty();
            }
        }
    }

    /// Handle keyboard input.
    /// Public for integration tests — drives a single key event end-to-end
    /// through the same dispatcher the event loop uses.
    pub fn dispatch_key_for_test(&mut self, key: KeyEvent) {
        self.handle_key(key);
    }

    /// Public for integration tests — drives one mouse event without terminal I/O.
    pub fn dispatch_mouse_for_test(&mut self, mouse: MouseEvent) {
        self.handle_event(AppEvent::Mouse(mouse));
    }

    /// Public for integration tests — renders one frame into a test backend.
    #[doc(hidden)]
    pub fn render_for_test(&mut self, frame: &mut Frame) {
        self.render_frame_safely(frame);
    }

    /// Public for integration tests — drives one tick without terminal I/O.
    pub fn dispatch_tick_for_test(&mut self) {
        self.handle_event(AppEvent::Tick);
    }

    fn has_in_flight_turn(&self) -> bool {
        self.state.is_streaming
            || self.state.is_waiting_for_response
            || self.engine_rx.is_some()
            || self.pending_submit.is_some()
            || self.state.compact_in_progress
            || self.pending_compact.is_some()
            || self.active_compact_task_id.is_some()
    }

    fn cancel_in_flight_turn(&mut self) {
        self.state.turn_state = TurnState::Cancelling;
        self.pending_submit = None;
        self.engine_rx = None;
        self.pending_compact = None;
        self.active_compact_task_id = None;
        if let Some(token) = self.active_compact_cancel_token.take() {
            token.cancel();
        }
        if self.state.compact_in_progress {
            self.state.compact_in_progress = false;
            self.state.compact_progress = Some("Compaction cancelled.".to_string());
            self.state.compact_notice_until =
                Some(std::time::Instant::now() + std::time::Duration::from_secs(6));
        }
        self.permission_rx = None;
        if let Some(responder) = self.active_permission_responder.take() {
            let _ = responder.send(mossen_agent::types::PermissionDecision::Deny);
        }
        self.active_modal = ActiveModal::None;

        if let Some(idx) = self.pending_assistant_idx.take() {
            if let Some(message) = self.messages.get_mut(idx) {
                message.is_streaming = false;
                message.thinking_completed_at = Some(std::time::Instant::now());
                if message.content.is_empty() {
                    message.content = "(cancelled)".to_string();
                }
            }
        }

        self.assistant_buf.clear();
        self.state.is_streaming = false;
        self.state.is_waiting_for_response = false;
        self.state.turn_state = TurnState::Idle;
        self.state.ui_stage = UiStage::Cancelled;
        let cancelled_index = self.messages.len();
        self.set_render_record_current_turn_override(cancelled_index);
        self.messages.push(cancelled_transcript_message());
        self.note_transcript_changed();
        self.record_final_summary(Some("Cancelled"));
        self.clear_current_render_turn_id();
    }

    fn handle_key(&mut self, key: KeyEvent) {
        use crossterm::event::{KeyCode, KeyModifiers};

        if matches!(key.code, KeyCode::Char('c'))
            && key.modifiers.contains(KeyModifiers::CONTROL)
            && (self.has_in_flight_turn() || self.active_permission_responder.is_some())
        {
            self.cancel_in_flight_turn();
            return;
        }

        if matches!(key.code, KeyCode::Char('c'))
            && key.modifiers.contains(KeyModifiers::CONTROL)
            && self.active_modal.is_open()
        {
            self.active_modal = ActiveModal::None;
            return;
        }

        // If a modal is active, route the key to its handler first.
        if self.active_modal.is_open() {
            self.handle_modal_key(key);
            return;
        }

        if matches!(key.code, KeyCode::Esc) && self.prompt.show_suggestions {
            self.prompt.show_suggestions = false;
            self.prompt.suggestions.clear();
            self.prompt.selected_suggestion = None;
            return;
        }

        // Ctrl+R → open search panel (out-of-band, not part of InputAction).
        if let KeyCode::Char('r') = key.code {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                let mut svc = std::mem::take(&mut self.services);
                let _ = self.services_handle_ctrl_r(&mut svc);
                self.services = svc;
                return;
            }
        }

        // Ctrl+V → read an image off the system clipboard, store in
        // `pending_images`, and insert a `[Image #N]` marker into the
        // prompt. macOS-first: uses `osascript`/`pbpaste -Prefer 'png'`
        // semantics; Linux gets `xclip -selection clipboard -t image/png
        // -o`. If no image is on the clipboard we fall through to plain
        // text paste (which the prompt's normal char handler covers).
        if let KeyCode::Char('v') = key.code {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                if self.try_paste_image() {
                    return;
                }
                // Fall-through: no image on clipboard → let prompt
                // handle Ctrl+V (which does nothing today, but at least
                // doesn't trigger the picker / Ctrl+E branches).
                return;
            }
        }

        // Esc -> open MessageSelector. The
        // selector lets the user pick a prior message / session to resume,
        // and is the canonical "back out" gesture when no modal is active.
        if matches!(key.code, KeyCode::Esc) {
            // If focus is on a message, clear focus first (one-step
            // escape — pressing Esc twice opens the selector).
            if self.focused_message_idx.take().is_some() {
                return;
            }
            let mut svc = std::mem::take(&mut self.services);
            let _consumed = self.services_handle_escape(&mut svc);
            self.services = svc;
            return;
        }

        // ── Transcript scroll / message focus interactions ───────────
        // With an empty prompt, unmodified arrow keys belong to the transcript.
        // Message focus remains available via Alt+Up/Alt+Down when idle.
        let prompt_empty = self.prompt.input.value.is_empty();
        let idle = !self.state.is_streaming;
        if prompt_empty && !self.prompt.show_suggestions {
            match key.code {
                KeyCode::Up if key.modifiers.is_empty() => {
                    self.refresh_transcript_scroll_metrics_for_input();
                    self.scroll.scroll_up(1);
                    return;
                }
                KeyCode::Down if key.modifiers.is_empty() => {
                    self.refresh_transcript_scroll_metrics_for_input();
                    self.scroll.scroll_down(1);
                    return;
                }
                KeyCode::Up if idle && key.modifiers.contains(KeyModifiers::ALT) => {
                    self.move_focus(-1);
                    return;
                }
                KeyCode::Down if idle && key.modifiers.contains(KeyModifiers::ALT) => {
                    self.move_focus(1);
                    return;
                }
                KeyCode::Char(' ') | KeyCode::Enter if self.toggle_focused_group() => {
                    return;
                }
                KeyCode::Right if self.toggle_focused_expand(true) => {
                    return;
                }
                KeyCode::Left if self.toggle_focused_expand(false) => {
                    return;
                }
                _ => {}
            }
        }

        // ── Ctrl+E/G/L/T/S — the five shortcuts the TS keybindings.ts
        //    surfaces by default. Implemented here at the source so we
        //    don't reroute through the full keybinding-context machinery
        //    (which is overkill for the fixed default set).
        if let KeyCode::Char(c) = key.code {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                match c {
                    // Ctrl+L → force redraw. Ratatui repaints on every
                    // tick already, but pressing Ctrl+L should also reset
                    // the virtual scroll to the tail so a user pinned
                    // mid-history can re-anchor on the latest message.
                    'l' => {
                        self.refresh_transcript_scroll_metrics_for_input();
                        self.scroll.scroll_to_bottom();
                        return;
                    }
                    // Ctrl+B → open background task picker.
                    'b' => {
                        if !self.state.teammate_states.is_empty() {
                            let items: Vec<String> =
                                self.state.teammate_states.keys().cloned().collect();
                            self.active_modal = ActiveModal::Picker {
                                kind: PickerKind::BackgroundTasks,
                                title: "Background Tasks".into(),
                                items,
                                selected: 0,
                            };
                        }
                        return;
                    }
                    // Ctrl+T → dump current TaskStore snapshot into the
                    // message stream so the user can see live todo state
                    // without typing /tasks. Lists subject + status.
                    't' => {
                        let lines = self.snapshot_task_list();
                        self.messages.push(system_transcript_message(lines, false));
                        self.note_transcript_changed();
                        return;
                    }
                    // Ctrl+S → stash the current prompt input to
                    // `~/.cache/mossen/stash.txt` and clear the input.
                    // Survives across sessions; `Ctrl+G` (editor) can pull
                    // it back.
                    's' => {
                        let text = self.prompt.input.value.clone();
                        if !text.is_empty() {
                            self.save_stash(&text);
                            self.prompt.input.clear();
                            self.messages.push(system_transcript_message(
                                format!("(stashed {} chars)", text.len()),
                                false,
                            ));
                            self.note_transcript_changed();
                        }
                        return;
                    }
                    // Ctrl+E → toggle "show all thinking" — when on,
                    // every assistant message's thinking block stays
                    // visible regardless of the 30s fade timer.
                    'e' => {
                        self.show_all_thinking = !self.show_all_thinking;
                        return;
                    }
                    // Ctrl+G → launch $EDITOR on a temp file seeded with
                    // current prompt input; replace the prompt with the
                    // edited content when the editor exits. Crossterm's
                    // raw mode is suspended for the duration so the
                    // editor (vim/nvim/nano) has a normal terminal.
                    'g' => {
                        self.spawn_external_editor();
                        return;
                    }
                    _ => {}
                }
            }
        }

        if let Some(action) = InputAction::from_key_event(&key) {
            match action {
                InputAction::Interrupt => {
                    if self.has_in_flight_turn() {
                        self.cancel_in_flight_turn();
                    } else {
                        self.should_quit = true;
                    }
                }
                InputAction::Eof => {
                    if self.prompt.input.value.is_empty() {
                        self.should_quit = true;
                    }
                }
                InputAction::Submit => {
                    if self.prompt.show_suggestions && !self.prompt_input_is_exact_slash_command() {
                        self.prompt.accept_suggestion();
                        self.update_suggestions();
                        return;
                    }
                    if let Some(input) = self.prompt.submit() {
                        let mut svc = std::mem::take(&mut self.services);
                        self.services_on_submit(&mut svc);
                        self.services = svc;
                        self.handle_submit(input);
                    }
                }
                InputAction::Char(c) => {
                    self.prompt.input.insert_char(c);
                    self.update_suggestions();
                }
                InputAction::Backspace => {
                    self.prompt.input.delete_backward();
                    self.update_suggestions();
                }
                InputAction::Delete => {
                    self.prompt.input.delete_forward();
                    self.update_suggestions();
                }
                InputAction::Left => {
                    self.prompt.input.move_left();
                    self.update_suggestions();
                }
                InputAction::Right => {
                    self.prompt.input.move_right();
                    self.update_suggestions();
                }
                InputAction::Home => {
                    if self.prompt.input.value.is_empty() && !self.prompt.show_suggestions {
                        self.refresh_transcript_scroll_metrics_for_input();
                        self.scroll.scroll_to_top();
                    } else {
                        self.prompt.input.move_home();
                        self.update_suggestions();
                    }
                }
                InputAction::End => {
                    if self.prompt.input.value.is_empty() && !self.prompt.show_suggestions {
                        self.refresh_transcript_scroll_metrics_for_input();
                        self.scroll.scroll_to_bottom();
                    } else {
                        self.prompt.input.move_end();
                        self.update_suggestions();
                    }
                }
                InputAction::Up => {
                    if self.prompt.show_suggestions {
                        self.prompt.suggestion_up();
                    } else {
                        self.prompt.input.history_up();
                    }
                }
                InputAction::Down => {
                    if self.prompt.show_suggestions {
                        self.prompt.suggestion_down();
                    } else {
                        self.prompt.input.history_down();
                    }
                }
                InputAction::Tab => {
                    if self.prompt.show_suggestions {
                        self.prompt.accept_suggestion();
                    }
                }
                InputAction::Escape => {
                    if self.prompt.show_suggestions {
                        self.prompt.show_suggestions = false;
                    } else if self.prompt.show_help {
                        self.prompt.show_help = false;
                    } else {
                        // Esc → message-selector (when not streaming) or dismiss
                        // an open modal. Routed through TerminalServices.
                        let mut svc = std::mem::take(&mut self.services);
                        let _consumed = self.services_handle_escape(&mut svc);
                        self.services = svc;
                    }
                }
                InputAction::PageUp => {
                    if self.prompt.show_suggestions {
                        self.prompt.suggestion_page_up(5);
                    } else {
                        self.refresh_transcript_scroll_metrics_for_input();
                        self.scroll.scroll_up(self.transcript_page_scroll_rows());
                    }
                }
                InputAction::PageDown => {
                    if self.prompt.show_suggestions {
                        self.prompt.suggestion_page_down(5);
                    } else {
                        self.refresh_transcript_scroll_metrics_for_input();
                        self.scroll.scroll_down(self.transcript_page_scroll_rows());
                    }
                }
                InputAction::Paste(text) => {
                    self.prompt.input.insert_str(&text);
                    self.update_suggestions();
                }
            }
        }
    }

    /// Route keys to the active modal.
    fn handle_modal_key(&mut self, key: KeyEvent) {
        use crossterm::event::KeyCode;

        // Map the key into a coarse modal verb first.
        let action = InputAction::from_key_event(&key);
        let mut chosen_model_profile: Option<String> = None;
        let mut pending_approval_decision: Option<(
            String,
            ApprovalDecisionKind,
            String,
            Option<String>,
        )> = None;
        let mut pending_command_edit: Option<String> = None;
        let mut pending_footer_toggle: Option<FooterItem> = None;
        let mut pending_footer_config: Option<FooterRenderConfig> = None;
        let mut pending_title_save: Option<String> = None;
        let mut pending_title_reset = false;
        let help_key_context = match &self.active_modal {
            ActiveModal::HelpDialog(state) => Some((
                self.help_dialog_content_line_count(&state.query),
                help_dialog_content_viewport_rows(self.state.terminal_height),
            )),
            _ => None,
        };
        let command_output_key_context = match &self.active_modal {
            ActiveModal::CommandOutput { body, .. } => Some((
                command_output_content_line_count(body),
                command_output_content_viewport_rows(self.state.terminal_height),
            )),
            _ => None,
        };

        match &mut self.active_modal {
            ActiveModal::PermissionRequest(state) => match action {
                Some(InputAction::Tab) | Some(InputAction::Right) | Some(InputAction::Down) => {
                    state.cycle_action();
                }
                Some(InputAction::Left) | Some(InputAction::Up) => {
                    state.cycle_action_back();
                }
                Some(InputAction::Submit) => {
                    let decision = state.selected_action;
                    if decision == PermissionAction::EditCommand {
                        let command = state.kind.detail();
                        pending_command_edit = Some(command.clone());
                        pending_approval_decision = Some((
                            state.tool_name.clone(),
                            ApprovalDecisionKind::Cancelled,
                            format!("edit command requested: {command}"),
                            None,
                        ));
                    } else {
                        state.confirm();
                        let render_decision = approval_decision_kind_from_permission(decision);
                        pending_approval_decision = Some((
                            state.tool_name.clone(),
                            render_decision,
                            state.kind.detail(),
                            None,
                        ));
                    }
                    self.active_modal = ActiveModal::None;
                }
                Some(InputAction::Escape) | Some(InputAction::Interrupt) => {
                    self.active_modal = ActiveModal::None;
                }
                _ => {}
            },
            ActiveModal::ToolUseConfirm { confirm, prompt } => {
                match action {
                    Some(InputAction::Tab) | Some(InputAction::Right) | Some(InputAction::Down) => {
                        prompt.cycle_action();
                    }
                    Some(InputAction::Left) | Some(InputAction::Up) => {
                        prompt.cycle_action_back();
                    }
                    Some(InputAction::Submit) => {
                        let decision = prompt.selected_action;
                        let tool_name = confirm.tool_name.clone();
                        let detail = prompt.kind.detail();
                        let anchor_block_id = last_tool_anchor_block_id_in_messages(
                            &self.messages,
                            &self.render_record_id_overrides,
                            &self.render_record_parent_overrides,
                            &tool_name,
                        );
                        if decision == PermissionAction::EditCommand {
                            pending_command_edit = Some(detail.clone());
                            pending_approval_decision = Some((
                                tool_name.clone(),
                                ApprovalDecisionKind::Cancelled,
                                format!("edit command requested: {detail}"),
                                anchor_block_id,
                            ));
                            if let Some(responder) = self.active_permission_responder.take() {
                                let _ =
                                    responder.send(mossen_agent::types::PermissionDecision::Deny);
                            }
                        } else {
                            prompt.confirm();
                            let render_decision = approval_decision_kind_from_permission(decision);
                            pending_approval_decision =
                                Some((tool_name.clone(), render_decision, detail, anchor_block_id));
                            // Round-trip the decision back into the engine via
                            // the gate's oneshot reply channel. Mapping:
                            //   UI Allow / AllowAlways / Deny
                            //     → engine PermissionDecision::{Allow,
                            //                                   AllowAlways,
                            //                                   Deny}
                            // The engine's `InteractiveGate::check` is awaiting
                            // exactly this send.
                            if let Some(responder) = self.active_permission_responder.take() {
                                let engine_decision = match decision {
                                    PermissionAction::Allow => {
                                        mossen_agent::types::PermissionDecision::Allow
                                    }
                                    PermissionAction::AllowAlways => {
                                        mossen_agent::types::PermissionDecision::AllowAlways
                                    }
                                    PermissionAction::Deny => {
                                        mossen_agent::types::PermissionDecision::Deny
                                    }
                                    PermissionAction::EditCommand => {
                                        mossen_agent::types::PermissionDecision::Deny
                                    }
                                };
                                // Dropping the receiver is harmless — engine
                                // treats a missing reply as Deny.
                                let _ = responder.send(engine_decision);
                            }
                        }
                        self.active_modal = ActiveModal::None;
                    }
                    Some(InputAction::Escape) | Some(InputAction::Interrupt) => {
                        // Treat ESC as deny — also round-trip it so the
                        // engine doesn't hang waiting on the gate.
                        let tool_name = confirm.tool_name.clone();
                        let detail = prompt.kind.detail();
                        let anchor_block_id = last_tool_anchor_block_id_in_messages(
                            &self.messages,
                            &self.render_record_id_overrides,
                            &self.render_record_parent_overrides,
                            &tool_name,
                        );
                        pending_approval_decision = Some((
                            tool_name,
                            ApprovalDecisionKind::Cancelled,
                            detail,
                            anchor_block_id,
                        ));
                        if let Some(responder) = self.active_permission_responder.take() {
                            let _ = responder.send(mossen_agent::types::PermissionDecision::Deny);
                        }
                        self.active_modal = ActiveModal::None;
                    }
                    _ => {}
                }
            }
            ActiveModal::MessageSelector(_) => {
                // ↑/↓ moves focus, Enter selects (and triggers the
                // restore-option submenu), Esc closes. Submenu mode
                // (after Enter) rebinds ↑/↓ to option selection +
                // Enter to commit the chosen restore action; Esc
                // backs out.
                match key.code {
                    KeyCode::Up => {
                        if let Some(state) = self.services.message_selector_state.as_mut() {
                            if state.message_to_restore.is_some() {
                                // Submenu: cycle restore option up.
                                let opts = state.get_restore_options(state.file_history_enabled);
                                let cur_pos = opts
                                    .iter()
                                    .position(|o| *o == state.selected_restore_option)
                                    .unwrap_or(0);
                                let next = cur_pos.saturating_sub(1);
                                state.selected_restore_option = opts[next].clone();
                            } else {
                                state.focus_prev();
                            }
                        }
                    }
                    KeyCode::Down => {
                        if let Some(state) = self.services.message_selector_state.as_mut() {
                            if state.message_to_restore.is_some() {
                                let opts = state.get_restore_options(state.file_history_enabled);
                                let cur_pos = opts
                                    .iter()
                                    .position(|o| *o == state.selected_restore_option)
                                    .unwrap_or(0);
                                let next = (cur_pos + 1).min(opts.len() - 1);
                                state.selected_restore_option = opts[next].clone();
                            } else {
                                state.focus_next();
                            }
                        }
                    }
                    KeyCode::Enter => {
                        if let Some(state) = self.services.message_selector_state.as_mut() {
                            if state.message_to_restore.is_some() {
                                // Commit the restore action.
                                state.start_restore();
                                // Trim messages back to the selected point.
                                // Conversation and Nevermind are handled here;
                                // code restore and summarize need persistence
                                // wiring beyond this iteration.
                                let restore_to = state.selected_index;
                                let nevermind = matches!(
                                    state.selected_restore_option,
                                    crate::widgets::message_selector::RestoreOption::Nevermind
                                );
                                if !nevermind {
                                    self.messages.truncate(restore_to + 1);
                                    self.truncate_render_record_id_overrides(self.messages.len());
                                    self.prune_approval_decisions_to_current_messages();
                                    self.note_transcript_changed();
                                }
                                self.active_modal = ActiveModal::None;
                                self.services.message_selector_state = None;
                            } else {
                                state.confirm_selection();
                            }
                        }
                    }
                    KeyCode::Esc => {
                        if let Some(state) = self.services.message_selector_state.as_mut() {
                            if state.message_to_restore.is_some() {
                                state.back();
                                return;
                            }
                        }
                        self.services.message_selector_state = None;
                        self.active_modal = ActiveModal::None;
                    }
                    _ => {}
                }
            }
            ActiveModal::ConfirmClear => match key.code {
                KeyCode::Enter => {
                    self.messages.clear();
                    self.clear_render_record_state();
                    self.engine_history.clear();
                    self.note_transcript_changed();
                    self.scroll.set_total_items(0);
                    self.assistant_buf.clear();
                    self.pending_assistant_history_recorded = false;
                    self.pending_assistant_idx = None;
                    self.focused_message_idx = None;
                    self.collapsed_tool_groups.clear();
                    self.render_event_history.clear();
                    self.active_modal = ActiveModal::None;
                }
                KeyCode::Esc => {
                    self.active_modal = ActiveModal::None;
                }
                _ => {}
            },
            ActiveModal::HelpDialog(state) => {
                let (total_rows, viewport_rows) = help_key_context.unwrap_or((
                    0,
                    help_dialog_content_viewport_rows(self.state.terminal_height),
                ));
                match key.code {
                    KeyCode::Esc => {
                        self.active_modal = ActiveModal::None;
                    }
                    KeyCode::Up => state.scroll_up(1),
                    KeyCode::Down => state.scroll_down(1, total_rows, viewport_rows),
                    KeyCode::PageUp => state.scroll_up(viewport_rows.max(1)),
                    KeyCode::PageDown => {
                        state.scroll_down(viewport_rows.max(1), total_rows, viewport_rows)
                    }
                    KeyCode::Home => state.scroll = 0,
                    KeyCode::End => state.scroll_to_end(total_rows, viewport_rows),
                    _ => {}
                }
            }
            ActiveModal::CommandOutput { .. } => {
                let (total_rows, viewport_rows) = command_output_key_context.unwrap_or((
                    0,
                    command_output_content_viewport_rows(self.state.terminal_height),
                ));
                match key.code {
                    KeyCode::Esc => {
                        self.active_modal = ActiveModal::None;
                    }
                    KeyCode::Up => {
                        self.command_output_scroll = self.command_output_scroll.saturating_sub(1);
                    }
                    KeyCode::Down => {
                        self.command_output_scroll = self
                            .command_output_scroll
                            .saturating_add(1)
                            .min(command_output_scroll_max(total_rows, viewport_rows));
                    }
                    KeyCode::PageUp => {
                        self.command_output_scroll = self
                            .command_output_scroll
                            .saturating_sub(viewport_rows.max(1));
                    }
                    KeyCode::PageDown => {
                        self.command_output_scroll = self
                            .command_output_scroll
                            .saturating_add(viewport_rows.max(1))
                            .min(command_output_scroll_max(total_rows, viewport_rows));
                    }
                    KeyCode::Home => self.command_output_scroll = 0,
                    KeyCode::End => {
                        self.command_output_scroll =
                            command_output_scroll_max(total_rows, viewport_rows);
                    }
                    _ => {}
                }
            }
            ActiveModal::StatusDialog | ActiveModal::McpServersDialog => {
                if matches!(key.code, KeyCode::Esc) {
                    self.active_modal = ActiveModal::None;
                }
            }
            ActiveModal::StatusLineConfig(state) => match key.code {
                KeyCode::Up => state.move_up(),
                KeyCode::Down => state.move_down(),
                KeyCode::Char(' ') | KeyCode::Enter => {
                    pending_footer_toggle = state.selected_item();
                }
                KeyCode::Char('d') => {
                    pending_footer_config = Some(FooterRenderConfig::standard());
                }
                KeyCode::Char('m') => {
                    pending_footer_config = Some(FooterRenderConfig::minimal());
                }
                KeyCode::Char('c') => {
                    pending_footer_config = Some(FooterRenderConfig::focused());
                }
                KeyCode::Char('f') => {
                    pending_footer_config = Some(FooterRenderConfig::full());
                }
                KeyCode::Esc => {
                    self.active_modal = ActiveModal::None;
                }
                _ => {}
            },
            ActiveModal::TitleConfig(state) => match key.code {
                KeyCode::Enter => {
                    pending_title_save = Some(state.draft.clone());
                }
                KeyCode::Esc => {
                    self.active_modal = ActiveModal::None;
                }
                KeyCode::Backspace => state.backspace(),
                KeyCode::Char('u')
                    if key
                        .modifiers
                        .contains(crossterm::event::KeyModifiers::CONTROL) =>
                {
                    state.clear();
                    pending_title_reset = true;
                }
                KeyCode::Char(ch) => state.push_char(ch),
                _ => {}
            },
            ActiveModal::RawTranscript(state) => {
                let viewport_height =
                    raw_transcript_content_viewport_rows(self.state.terminal_height);
                match key.code {
                    KeyCode::Esc => {
                        self.active_modal = ActiveModal::None;
                    }
                    KeyCode::Up => state.scroll_up(1, viewport_height),
                    KeyCode::Down => state.scroll_down(1, viewport_height),
                    KeyCode::PageUp => state.scroll_up(viewport_height, viewport_height),
                    KeyCode::PageDown => state.scroll_down(viewport_height, viewport_height),
                    KeyCode::Home => state.scroll_to_top(),
                    KeyCode::End => state.scroll_to_bottom(viewport_height),
                    _ => {}
                }
            }
            ActiveModal::DiffReview(state) => {
                let viewport_height = diff_review_content_viewport_rows(self.state.terminal_height);
                match key.code {
                    KeyCode::Esc => {
                        self.active_modal = ActiveModal::None;
                    }
                    KeyCode::Left => state.move_prev_file(),
                    KeyCode::Right => state.move_next_file(),
                    KeyCode::Up => state.scroll_up(1, viewport_height),
                    KeyCode::Down => state.scroll_down(1, viewport_height),
                    KeyCode::PageUp => state.scroll_up(viewport_height, viewport_height),
                    KeyCode::PageDown => state.scroll_down(viewport_height, viewport_height),
                    KeyCode::Home => state.scroll_to_top(),
                    KeyCode::End => state.scroll_to_bottom(viewport_height),
                    KeyCode::Char(' ') | KeyCode::Enter => state.toggle_selected_file(),
                    _ => {}
                }
            }
            ActiveModal::FileChanges(state) => {
                let viewport_rows = file_changes_content_viewport_rows(self.state.terminal_height);
                match key.code {
                    KeyCode::Esc => {
                        self.active_modal = ActiveModal::None;
                    }
                    KeyCode::Up => state.move_up(),
                    KeyCode::Down => state.move_down(viewport_rows),
                    KeyCode::PageUp => state.page_up(viewport_rows),
                    KeyCode::PageDown => state.page_down(viewport_rows),
                    KeyCode::Home => state.select_first(),
                    KeyCode::End => state.select_last(viewport_rows),
                    _ => {}
                }
            }
            ActiveModal::RenderTimeline(state) => {
                let viewport_rows =
                    render_timeline_content_viewport_rows(self.state.terminal_height);
                match key.code {
                    KeyCode::Esc => {
                        self.active_modal = ActiveModal::None;
                    }
                    KeyCode::Up => state.move_up(),
                    KeyCode::Down => state.move_down(viewport_rows),
                    KeyCode::PageUp => state.page_up(viewport_rows),
                    KeyCode::PageDown => state.page_down(viewport_rows),
                    KeyCode::Home => state.select_first(),
                    KeyCode::End => state.select_last(viewport_rows),
                    _ => {}
                }
            }
            ActiveModal::ProcessList(state) => {
                let viewport_rows = process_list_content_viewport_rows(self.state.terminal_height);
                match key.code {
                    KeyCode::Esc => {
                        self.active_modal = ActiveModal::None;
                    }
                    KeyCode::Up => state.move_up(),
                    KeyCode::Down => state.move_down(viewport_rows),
                    KeyCode::PageUp => state.page_up(viewport_rows),
                    KeyCode::PageDown => state.page_down(viewport_rows),
                    KeyCode::Home => state.select_first(),
                    KeyCode::End => state.select_last(viewport_rows),
                    _ => {}
                }
            }
            ActiveModal::CommandHistory(state) => {
                let viewport_rows =
                    command_history_content_viewport_rows(self.state.terminal_height);
                match key.code {
                    KeyCode::Esc => {
                        self.active_modal = ActiveModal::None;
                    }
                    KeyCode::Up => state.move_up(),
                    KeyCode::Down => state.move_down(viewport_rows),
                    KeyCode::PageUp if state.selected_expanded() => {
                        state.detail_page_up(viewport_rows)
                    }
                    KeyCode::PageUp => state.page_up(viewport_rows),
                    KeyCode::PageDown if state.selected_expanded() => {
                        state.detail_page_down(viewport_rows)
                    }
                    KeyCode::PageDown => state.page_down(viewport_rows),
                    KeyCode::Home => state.select_first(),
                    KeyCode::End => state.select_last(viewport_rows),
                    KeyCode::Char(' ') | KeyCode::Enter => state.toggle_selected_log(),
                    _ => {}
                }
            }
            ActiveModal::ErrorHistory(state) => {
                let viewport_rows = error_history_content_viewport_rows(self.state.terminal_height);
                match key.code {
                    KeyCode::Esc => {
                        self.active_modal = ActiveModal::None;
                    }
                    KeyCode::Up => state.move_up(),
                    KeyCode::Down => state.move_down(viewport_rows),
                    KeyCode::PageUp if state.selected_expanded() => {
                        state.detail_page_up(viewport_rows)
                    }
                    KeyCode::PageUp => state.page_up(viewport_rows),
                    KeyCode::PageDown if state.selected_expanded() => {
                        state.detail_page_down(viewport_rows)
                    }
                    KeyCode::PageDown => state.page_down(viewport_rows),
                    KeyCode::Home => state.select_first(),
                    KeyCode::End => state.select_last(viewport_rows),
                    KeyCode::Char(' ') | KeyCode::Enter => state.toggle_selected_details(),
                    _ => {}
                }
            }
            ActiveModal::FinalSummaryHistory(state) => {
                let viewport_rows =
                    final_summary_history_content_viewport_rows(self.state.terminal_height);
                match key.code {
                    KeyCode::Esc => {
                        self.active_modal = ActiveModal::None;
                    }
                    KeyCode::Up => state.move_up(),
                    KeyCode::Down => state.move_down(viewport_rows),
                    KeyCode::PageUp if state.selected_expanded() => {
                        state.detail_page_up(viewport_rows)
                    }
                    KeyCode::PageUp => state.page_up(viewport_rows),
                    KeyCode::PageDown if state.selected_expanded() => {
                        state.detail_page_down(viewport_rows)
                    }
                    KeyCode::PageDown => state.page_down(viewport_rows),
                    KeyCode::Home => state.select_first(),
                    KeyCode::End => state.select_last(viewport_rows),
                    KeyCode::Char(' ') | KeyCode::Enter => state.toggle_selected_details(),
                    _ => {}
                }
            }
            ActiveModal::ApprovalHistory(state) => {
                let viewport_rows =
                    approval_history_content_viewport_rows(self.state.terminal_height);
                match key.code {
                    KeyCode::Esc => {
                        self.active_modal = ActiveModal::None;
                    }
                    KeyCode::Up => state.move_up(),
                    KeyCode::Down => state.move_down(viewport_rows),
                    KeyCode::PageUp if state.selected_expanded() => {
                        state.detail_page_up(viewport_rows)
                    }
                    KeyCode::PageUp => state.page_up(viewport_rows),
                    KeyCode::PageDown if state.selected_expanded() => {
                        state.detail_page_down(viewport_rows)
                    }
                    KeyCode::PageDown => state.page_down(viewport_rows),
                    KeyCode::Home => state.select_first(),
                    KeyCode::End => state.select_last(viewport_rows),
                    KeyCode::Char(' ') | KeyCode::Enter => state.toggle_selected_details(),
                    _ => {}
                }
            }
            ActiveModal::DebugConfig(state) => {
                let viewport_rows = debug_config_content_viewport_rows(self.state.terminal_height);
                match key.code {
                    KeyCode::Esc => {
                        self.active_modal = ActiveModal::None;
                    }
                    KeyCode::Up => state.scroll_up(1),
                    KeyCode::Down => state.scroll_down(1, viewport_rows),
                    KeyCode::PageUp => state.scroll_up(viewport_rows),
                    KeyCode::PageDown => state.scroll_down(viewport_rows, viewport_rows),
                    KeyCode::Home => state.scroll_to_top(),
                    KeyCode::End => state.scroll_to_bottom(viewport_rows),
                    _ => {}
                }
            }
            ActiveModal::TasksDialog => {
                if matches!(key.code, KeyCode::Esc) {
                    self.active_modal = ActiveModal::None;
                }
            }
            ActiveModal::IdleReturn(_) => {
                self.services.idle_return_state = None;
                self.active_modal = ActiveModal::None;
            }
            ActiveModal::CostThreshold(_) => {
                self.active_modal = ActiveModal::None;
            }
            ActiveModal::ModelPicker(state) => match key.code {
                KeyCode::Up => state.move_up(),
                KeyCode::Down => state.move_down(),
                KeyCode::Enter => {
                    if let Some((_, model)) = state.filtered().get(state.selected) {
                        chosen_model_profile = Some(model.id.clone());
                    }
                    self.active_modal = ActiveModal::None;
                }
                KeyCode::Esc => {
                    self.active_modal = ActiveModal::None;
                }
                KeyCode::Backspace => {
                    state.filter.pop();
                    state.selected = 0;
                }
                KeyCode::Char(c)
                    if !key
                        .modifiers
                        .contains(crossterm::event::KeyModifiers::CONTROL)
                        && !key.modifiers.contains(crossterm::event::KeyModifiers::ALT) =>
                {
                    state.filter.push(c);
                    state.selected = 0;
                }
                _ => {}
            },
            ActiveModal::SkillsPanel(state) => match key.code {
                KeyCode::Up => {
                    state.selected = state.selected.saturating_sub(1);
                }
                KeyCode::Down if state.selected + 1 < state.skills.len() => {
                    state.selected += 1;
                }
                KeyCode::Esc => {
                    self.active_modal = ActiveModal::None;
                }
                _ => {}
            },
            ActiveModal::MemoryPanel(state) => match key.code {
                KeyCode::Up => {
                    state.selected = state.selected.saturating_sub(1);
                }
                KeyCode::Down if state.selected + 1 < state.entries.len() => {
                    state.selected += 1;
                }
                KeyCode::Esc => {
                    self.active_modal = ActiveModal::None;
                }
                _ => {}
            },
            ActiveModal::McpChannelApproval(request) => match key.code {
                KeyCode::Enter => {
                    let id = request.id.clone();
                    let server_name = request.server_name.clone();
                    let anchor_block_id = last_mcp_tool_anchor_block_id_in_messages(
                        &self.messages,
                        &self.render_record_id_overrides,
                        &self.render_record_parent_overrides,
                        &server_name,
                    );
                    mossen_agent::mcp::channel_approval::submit_decision(
                        &id,
                        mossen_agent::mcp::channel_approval::ChannelApprovalDecision::Allow,
                    );
                    self.active_modal = ActiveModal::None;
                    pending_approval_decision = Some((
                        "MCP Channel".to_string(),
                        ApprovalDecisionKind::Allowed,
                        format!("{server_name} · use /mcp to reconnect"),
                        anchor_block_id,
                    ));
                }
                KeyCode::Esc => {
                    let id = request.id.clone();
                    let server_name = request.server_name.clone();
                    let anchor_block_id = last_mcp_tool_anchor_block_id_in_messages(
                        &self.messages,
                        &self.render_record_id_overrides,
                        &self.render_record_parent_overrides,
                        &server_name,
                    );
                    mossen_agent::mcp::channel_approval::submit_decision(
                        &id,
                        mossen_agent::mcp::channel_approval::ChannelApprovalDecision::Deny,
                    );
                    self.active_modal = ActiveModal::None;
                    pending_approval_decision = Some((
                        "MCP Channel".to_string(),
                        ApprovalDecisionKind::Denied,
                        server_name,
                        anchor_block_id,
                    ));
                }
                _ => {}
            },
            ActiveModal::Picker {
                kind,
                items,
                selected,
                ..
            } => {
                let kind = *kind;
                let len = items.len();
                match key.code {
                    KeyCode::Up if *selected > 0 => {
                        *selected -= 1;
                    }
                    KeyCode::Down if *selected + 1 < len => {
                        *selected += 1;
                    }
                    KeyCode::Enter => {
                        let choice = items.get(*selected).cloned().unwrap_or_default();
                        self.active_modal = ActiveModal::None;
                        self.apply_picker_choice(kind, &choice);
                    }
                    KeyCode::Esc => {
                        self.active_modal = ActiveModal::None;
                    }
                    _ => {}
                }
            }
            _ => {
                // For scaffolded modals, ESC cancels.
                if matches!(key.code, KeyCode::Esc) {
                    self.active_modal = ActiveModal::None;
                }
            }
        }

        if let Some(config) = pending_footer_config {
            self.set_footer_render_config_and_persist(config);
        }
        if pending_title_reset {
            self.services.clear_manual_title();
            if let ActiveModal::TitleConfig(state) = &mut self.active_modal {
                state.draft.clear();
                state.notice = "reset to default".to_string();
            }
        }
        if let Some(title) = pending_title_save {
            let saved = self.services.set_manual_title(&title);
            if let ActiveModal::TitleConfig(state) = &mut self.active_modal {
                state.draft = saved.clone().unwrap_or_default();
                state.notice = saved
                    .map(|_| "saved".to_string())
                    .unwrap_or_else(|| "reset to default".to_string());
            }
        }
        if let Some(item) = pending_footer_toggle {
            self.toggle_footer_render_item_and_persist(item);
        }

        if let Some(command) = pending_command_edit {
            self.stage_command_edit_from_approval(&command);
        }

        if let Some((tool_name, decision, detail, anchor_block_id)) = pending_approval_decision {
            let next_stage = match decision {
                ApprovalDecisionKind::Allowed | ApprovalDecisionKind::AlwaysAllowed => {
                    UiStage::from_tool_name(&tool_name)
                }
                ApprovalDecisionKind::Denied | ApprovalDecisionKind::Cancelled => {
                    UiStage::ReviewingResult
                }
            };
            self.record_approval_decision(tool_name, decision, detail, anchor_block_id);
            self.state.ui_stage = next_stage;
        }

        if let Some(profile) = chosen_model_profile {
            self.apply_model_picker_choice(&profile);
        }
    }

    /// React to a Picker selection. Wires both `/theme` (rebuilds the
    /// active Theme by name) and `/output-style` (stores the chosen
    /// style id so future Assistant renders use it).
    fn apply_picker_choice(&mut self, kind: PickerKind, choice: &str) {
        match kind {
            PickerKind::Theme => {
                let Some((name, display)) = parse_theme_choice(choice) else {
                    return;
                };
                self.apply_theme_name(name);
                self.messages.push(system_transcript_message(
                    format!("Theme set to: {}", display),
                    false,
                ));
                self.note_transcript_changed();
            }
            PickerKind::BackgroundTasks => {
                // Switch foreground to the selected background task.
                self.state.foreground_task_id = Some(choice.to_string());
                self.state.message_count = self.messages.len();
            }
            PickerKind::PermissionMode => {
                self.apply_permission_mode_choice(choice);
            }
            PickerKind::OutputStyle => {
                let Some(display) = self.apply_output_style_choice(choice) else {
                    return;
                };
                self.messages.push(system_transcript_message(
                    format!("Output style set to: {}", display),
                    false,
                ));
                self.note_transcript_changed();
            }
        }
    }

    fn apply_theme_name(&mut self, name: crate::theme::ThemeName) {
        self.state.theme = name;
        self.theme = crate::theme::Theme::for_name_with_color_mode(name, self.theme.color_mode);
        self.mark_render_dirty();
    }

    fn apply_output_style_choice(&mut self, raw: &str) -> Option<&'static str> {
        let (id, display, guidance) = output_style_choice(raw)?;
        self.engine_config.output_style = if id == "default" {
            None
        } else {
            Some(display.to_string())
        };
        self.engine_config
            .system_prompt
            .retain(|b| !b.text.starts_with("# Output style:"));
        if let Some(text) = guidance {
            self.engine_config
                .system_prompt
                .push(mossen_agent::types::SystemBlock {
                    text: text.to_string(),
                    cache_control: None,
                });
        }
        Some(display)
    }

    fn apply_proactive_mode(&mut self, enabled: bool) {
        self.engine_config
            .system_prompt
            .retain(|b| !b.text.starts_with("# Proactive mode:"));
        if enabled {
            self.engine_config
                .system_prompt
                .push(mossen_agent::types::SystemBlock {
                    text: "# Proactive mode: Enabled\n\nWhen it is relevant and low-noise, point out likely bugs, missing tests, risky assumptions, or follow-up improvements without waiting for a separate prompt.".to_string(),
                    cache_control: None,
                });
        }
        self.command_context.env_vars.insert(
            "MOSSEN_PROACTIVE".to_string(),
            if enabled { "1" } else { "0" }.to_string(),
        );
    }

    /// Handle submitted input.
    fn handle_submit(&mut self, input: String) {
        if input.starts_with('/') {
            self.handle_command(input[1..].trim());
            return;
        }

        self.push_user_message(input.clone());

        // Drain any pasted images into ContentBlock::Image so the
        // multimodal API gets the actual bytes. The textual `[Image #N]`
        // markers stay in the prompt so the model can reference them by
        // ordinal in its reply.
        let mut additional_blocks: Vec<mossen_types::ContentBlock> =
            self.startup_additional_blocks.drain(..).collect();
        additional_blocks.extend(self.pending_images.drain(..).map(|(mime, data)| {
            mossen_types::ContentBlock::Image(mossen_types::ImageBlock {
                source: mossen_types::ImageSource {
                    source_type: "base64".to_string(),
                    media_type: mime,
                    data,
                },
            })
        }));

        self.submit_prompt_to_engine(input, additional_blocks);
    }

    fn push_user_message(&mut self, content: String) -> usize {
        let source_index = self.messages.len();
        self.set_render_record_current_turn_override(source_index);
        self.messages.push(user_transcript_message(content));
        self.note_transcript_changed();
        source_index
    }

    fn submit_prompt_to_engine(
        &mut self,
        prompt: String,
        additional_blocks: Vec<mossen_types::ContentBlock>,
    ) {
        // Build PromptParams for the engine.
        let cfg = &self.engine_config;
        let system_prompt = cfg.system_prompt.clone();

        // Build the interactive permission gate. We pair a fresh mpsc
        // channel per dispatch (capacity 16 is plenty — at most one
        // outstanding request at a time today). The TX side is wrapped in
        // an `InteractiveGate` and shipped down through `PromptParams`; the
        // RX side is stashed on `self` so the tick loop can drain
        // `PermissionRequest`s and open the modal.
        let (perm_tx, perm_rx) =
            tokio::sync::mpsc::channel::<mossen_agent::types::PermissionRequest>(16);
        let interactive_gate: std::sync::Arc<dyn mossen_agent::types::PermissionGate> =
            std::sync::Arc::new(mossen_agent::types::InteractiveGate::new(perm_tx));
        let gate: std::sync::Arc<dyn mossen_agent::types::PermissionGate> = std::sync::Arc::new(
            SessionPermissionGate::new(self.permission_rules.clone(), interactive_gate),
        );
        self.permission_rx = Some(perm_rx);

        // Pull executable tool definitions from the registry the launcher
        // attached via `with_tool_registry`. Falling back to `Vec::new()`
        // keeps the empty-registry test path working; production runs
        // always carry the full built-in tool list so the model knows what
        // it can actually call (without this, some models fall back to
        // emitting bash commands inside markdown code blocks).
        let mut tools = self
            .tool_registry
            .as_ref()
            .map(|r| r.definitions())
            .unwrap_or_default();
        tools.extend(self.extra_tool_definitions.clone());

        let params = PromptParams {
            prompt,
            history_messages: self.engine_history.clone(),
            additional_blocks,
            model: cfg.model.clone(),
            system_prompt,
            tools,
            tool_use_context: ToolUseContext {
                cwd: cfg.cwd.clone(),
                additional_working_directories: if self.additional_working_directories.is_empty() {
                    None
                } else {
                    Some(self.additional_working_directories.clone())
                },
                extra: Default::default(),
            },
            origin_tag: cfg.origin_tag.clone(),
            max_turns: cfg.max_turns,
            cancel_token: None,
            api_base_url: cfg.api_base_url.clone(),
            api_key: cfg.api_key.clone(),
            extra_body: cfg.extra_body.clone(),
            fast_mode: cfg.fast_mode,
            effort: cfg.effort,
            permission_mode: PermissionMode::parse(self.current_permission_mode_code()),
            permission_gate: Some(gate),
            tool_registry: self.tool_registry.clone(),
            hook_context: cfg.compact_hook_context.clone(),
        };

        // The stream emits MessageStart for each model turn. Create the
        // assistant row from that event so tool loops get one row per model
        // response instead of reusing a stale placeholder.
        self.pending_assistant_idx = None;
        self.assistant_buf.clear();
        self.pending_assistant_history_recorded = false;

        self.pending_submit = Some(params);
        self.state.is_streaming = true;
        self.state.is_waiting_for_response = true;
        self.state.ui_stage = UiStage::Thinking;
        self.spinner.reset();
    }

    fn submit_prompt_directive(&mut self, command_name: &str, args_raw: &str, prompt: String) {
        let args = args_raw.trim();
        let display = if args.is_empty() {
            format!("/{command_name}")
        } else {
            format!("/{command_name} {args}")
        };
        self.push_user_message(display);
        self.submit_prompt_to_engine(prompt, Vec::new());
    }

    fn record_engine_assistant_text(&mut self, text: &str) {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return;
        }
        self.engine_history
            .push(text_message(Role::Assistant, trimmed));
        self.pending_assistant_history_recorded = true;
    }

    fn compact_engine_history(&mut self) {
        self.compact_engine_history_with_instructions(None);
    }

    fn compact_engine_history_with_instructions(&mut self, custom_instructions: Option<String>) {
        let before_messages = self.engine_history.len();
        if before_messages < 2 {
            self.state.compact_in_progress = false;
            self.state.compact_progress = Some("Not enough messages to compact.".to_string());
            self.state.compact_notice_until =
                Some(std::time::Instant::now() + std::time::Duration::from_secs(6));
            self.push_system_message("Not enough messages to compact.", true);
            return;
        }

        if self.state.compact_in_progress
            || self.pending_compact.is_some()
            || self.active_compact_task_id.is_some()
        {
            self.state.compact_progress = Some("Compaction is already running.".to_string());
            self.state.compact_notice_until =
                Some(std::time::Instant::now() + std::time::Duration::from_secs(6));
            return;
        }

        mossen_agent::services::compact::compact_warning_state::clear_compact_warning_suppression();
        self.next_compact_task_id = self.next_compact_task_id.saturating_add(1);
        let task_id = self.next_compact_task_id;
        let cancel_token = tokio_util::sync::CancellationToken::new();
        self.state.compact_in_progress = true;
        self.state.compact_progress = Some(if custom_instructions.is_some() {
            "Compacting conversation history with custom instructions...".to_string()
        } else {
            "Compacting conversation history...".to_string()
        });
        self.active_compact_task_id = Some(task_id);
        self.active_compact_cancel_token = Some(cancel_token.clone());
        self.pending_compact = Some(CompactTaskRequest {
            task_id,
            before_messages,
            history: self.engine_history.clone(),
            hook_context: self.engine_config.compact_hook_context.clone(),
            custom_instructions,
            cancel_token,
        });
        self.mark_render_dirty();
    }

    fn launch_pending_compact_task(&mut self) {
        let Some(request) = self.pending_compact.take() else {
            return;
        };
        let tx = self.compact_result_tx.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let result =
                    if request.hook_context.is_some() || request.custom_instructions.is_some() {
                        mossen_agent::services::compact::compact::compact_conversation_with_options(
                            &request.history,
                            "Read",
                            mossen_agent::services::compact::compact::CompactConversationOptions {
                                hook_context: request.hook_context.as_deref(),
                                trigger: "manual",
                                custom_instructions: request.custom_instructions.as_deref(),
                                cancel_token: Some(&request.cancel_token),
                                hook_timeout_ms:
                                    mossen_utils::hooks_utils::TOOL_HOOK_EXECUTION_TIMEOUT_MS,
                            },
                        )
                        .await
                    } else {
                        mossen_agent::services::compact::compact::compact_conversation(
                            &request.history,
                            "Read",
                        )
                        .await
                    };
                let _ = tx.send(CompactTaskResult {
                    task_id: request.task_id,
                    before_messages: request.before_messages,
                    result,
                });
            });
        } else {
            let result = if request.hook_context.is_some() || request.custom_instructions.is_some()
            {
                block_on_current_runtime(
                    mossen_agent::services::compact::compact::compact_conversation_with_options(
                        &request.history,
                        "Read",
                        mossen_agent::services::compact::compact::CompactConversationOptions {
                            hook_context: request.hook_context.as_deref(),
                            trigger: "manual",
                            custom_instructions: request.custom_instructions.as_deref(),
                            cancel_token: Some(&request.cancel_token),
                            hook_timeout_ms:
                                mossen_utils::hooks_utils::TOOL_HOOK_EXECUTION_TIMEOUT_MS,
                        },
                    ),
                )
            } else {
                block_on_current_runtime(
                    mossen_agent::services::compact::compact::compact_conversation(
                        &request.history,
                        "Read",
                    ),
                )
            };
            let _ = tx.send(CompactTaskResult {
                task_id: request.task_id,
                before_messages: request.before_messages,
                result,
            });
        }
    }

    fn poll_compact_result(&mut self) {
        let mut changed = false;
        while let Ok(result) = self.compact_result_rx.try_recv() {
            if self.active_compact_task_id == Some(result.task_id) {
                self.apply_compact_result(result);
                changed = true;
            }
        }
        if changed {
            self.mark_render_dirty();
        }
    }

    fn apply_compact_result(&mut self, task: CompactTaskResult) {
        let before_messages = task.before_messages;
        let result = task.result;
        self.active_compact_task_id = None;
        self.active_compact_cancel_token = None;
        self.state.compact_in_progress = false;
        self.state.compact_notice_until =
            Some(std::time::Instant::now() + std::time::Duration::from_secs(6));

        if !result.success {
            let error = result
                .error
                .unwrap_or_else(|| "Compaction failed.".to_string());
            self.state.compact_progress = Some(error.clone());
            self.push_system_message(format!("Compaction failed: {}", error), true);
            return;
        }

        let pre_compact_hook_message = result.pre_compact_hook_message.clone();
        let post_compact_hook_message = result.post_compact_hook_message.clone();
        let compacted_context_messages = result.new_messages.len();
        let compacted_message_count = result.compacted_message_count;
        let before_tokens_u64 =
            mossen_agent::token_estimation::estimate_messages_tokens(&self.engine_history);
        let before_tokens = usize::try_from(before_tokens_u64).unwrap_or(usize::MAX);
        let (compacted_history, after_tokens) =
            mossen_agent::services::compact::compact::prepend_compact_boundary_to_messages(
                result.new_messages,
                "manual",
                compacted_message_count,
                before_tokens,
            );
        self.engine_history = compacted_history;
        mossen_agent::services::compact::post_compact_cleanup::run_post_compact_cleanup(Some(
            "repl_main_thread",
        ));
        self.state.compact_progress = Some(format!(
            "Messages {} -> {} compacted context (+1 boundary), tokens {} -> {}",
            before_messages, compacted_context_messages, before_tokens, after_tokens
        ));
        if let Some(message) = pre_compact_hook_message.filter(|message| !message.trim().is_empty())
        {
            self.push_system_message(format!("(compact hooks)\n{}", message), false);
        }
        self.push_system_message(
            format!(
                "(compact) messages {} -> {} compacted context (+1 boundary), tokens {} -> {}",
                before_messages, compacted_context_messages, before_tokens, after_tokens
            ),
            false,
        );
        if let Some(message) =
            post_compact_hook_message.filter(|message| !message.trim().is_empty())
        {
            self.push_system_message(format!("(compact hooks)\n{}", message), false);
        }
    }

    fn handle_compact_command(&mut self, args: &[&str]) {
        match args.first().copied() {
            Some("plan" | "preview") => {
                self.active_modal = ActiveModal::CommandOutput {
                    title: "Compact Plan".to_string(),
                    body: compact_plan_body_from_model(
                        &self.compact_plan_model(compact_instruction_tail(args, 1)),
                    ),
                    is_error: false,
                };
            }
            Some("status") => {
                self.active_modal = ActiveModal::CommandOutput {
                    title: "Compact Status".to_string(),
                    body: compact_status_body_from_model(&self.compact_status_model()),
                    is_error: false,
                };
            }
            Some("cancel" | "stop") => self.cancel_compact_task(true),
            Some("run" | "now" | "apply") => {
                self.compact_engine_history_with_instructions(compact_instruction_tail(args, 1))
            }
            None => self.compact_engine_history(),
            Some(_) => {
                self.compact_engine_history_with_instructions(compact_instruction_tail(args, 0))
            }
        }
    }

    fn cancel_compact_task(&mut self, push_transcript: bool) {
        let was_running = self.state.compact_in_progress
            || self.pending_compact.is_some()
            || self.active_compact_task_id.is_some();
        if !was_running {
            self.active_modal = ActiveModal::CommandOutput {
                title: "Compact Status".to_string(),
                body: "No compaction is running.".to_string(),
                is_error: false,
            };
            return;
        }

        self.pending_compact = None;
        self.active_compact_task_id = None;
        if let Some(token) = self.active_compact_cancel_token.take() {
            token.cancel();
        }
        self.state.compact_in_progress = false;
        self.state.compact_progress = Some("Compaction cancelled.".to_string());
        self.state.compact_notice_until =
            Some(std::time::Instant::now() + std::time::Duration::from_secs(6));
        self.active_modal = ActiveModal::None;
        if push_transcript {
            self.push_system_message("(compact) cancelled", false);
        }
        self.mark_render_dirty();
    }

    fn compact_status_model(&self) -> CompactStatusRenderModel {
        CompactStatusRenderModel {
            is_running: self.compact_is_running(),
            task_id: self.active_compact_task_id,
            pending_launch: self.pending_compact.is_some(),
            cancellable: self.active_compact_cancel_token.is_some(),
            hooks_configured: self.engine_config.compact_hook_context.is_some(),
            progress: self.state.compact_progress.clone(),
        }
    }

    fn compact_plan_model(&self, custom_instructions: Option<String>) -> CompactPlanRenderModel {
        compact_plan_render_model(
            &self.engine_history,
            self.compact_is_running(),
            self.engine_config.compact_hook_context.is_some(),
            custom_instructions,
        )
    }

    fn compact_is_running(&self) -> bool {
        self.state.compact_in_progress
            || self.pending_compact.is_some()
            || self.active_compact_task_id.is_some()
    }

    fn sync_permission_rule_env(&mut self) {
        sync_permission_rule_env_value(
            &mut self.command_context.env_vars,
            PERMISSION_ALLOW_RULES_ENV,
            &self.permission_rules.allow,
        );
        sync_permission_rule_env_value(
            &mut self.command_context.env_vars,
            PERMISSION_DENY_RULES_ENV,
            &self.permission_rules.deny,
        );
    }

    fn apply_permission_rule_command_side_effect(&mut self, args: &[&str]) {
        let Some(action) = args.first().map(|value| value.to_ascii_lowercase()) else {
            return;
        };

        match action.as_str() {
            "allow" => {
                if let Some(rule) = permission_rule_pattern(args) {
                    remove_permission_rule(&mut self.permission_rules.deny, &rule);
                    add_permission_rule(&mut self.permission_rules.allow, rule);
                    self.sync_permission_rule_env();
                }
            }
            "deny" => {
                if let Some(rule) = permission_rule_pattern(args) {
                    remove_permission_rule(&mut self.permission_rules.allow, &rule);
                    add_permission_rule(&mut self.permission_rules.deny, rule);
                    self.sync_permission_rule_env();
                }
            }
            "reset" | "clear" => {
                self.permission_rules.allow.clear();
                self.permission_rules.deny.clear();
                self.sync_permission_rule_env();
            }
            "list" | "show" => {}
            _ => {}
        }
    }

    fn open_permission_mode_picker(&mut self) {
        self.active_modal = ActiveModal::Picker {
            kind: PickerKind::PermissionMode,
            title: "Select permission mode".to_string(),
            items: permission_mode_choices()
                .iter()
                .map(|choice| choice.label.to_string())
                .collect(),
            selected: self.current_permission_mode_picker_index(),
        };
    }

    fn apply_permission_mode_choice(&mut self, choice: &str) {
        let Some(code) = permission_mode_code_for_choice(choice) else {
            return;
        };
        self.apply_permission_mode_code(code);
    }

    fn apply_permission_mode_code(&mut self, code: &str) {
        self.command_context
            .env_vars
            .insert(PERMISSION_MODE_ENV.to_string(), code.to_string());
        self.push_system_message(
            format!(
                "Permission mode set to: {}",
                permission_mode_display_label(Some(code))
            ),
            false,
        );
    }

    fn try_handle_permissions_mode_command(&mut self, args: &[&str], args_raw: &str) -> bool {
        if args.is_empty() {
            self.open_permission_mode_picker();
            return true;
        }

        if permission_rule_subcommand(args[0]) {
            return false;
        }

        if permission_mode_selector_subcommand(args[0]) {
            let raw_mode = args.get(1..).map(|tail| tail.join(" ")).unwrap_or_default();
            if raw_mode.trim().is_empty() {
                self.open_permission_mode_picker();
            } else {
                self.handle_permission_mode_command("permissions", raw_mode.trim());
            }
            return true;
        }

        if let Some(code) = permission_mode_code_for_choice(args_raw.trim()) {
            self.apply_permission_mode_code(code);
            return true;
        }

        false
    }

    fn handle_permission_mode_command(&mut self, command_name: &str, args_raw: &str) {
        let raw_mode = args_raw.trim();
        if raw_mode.is_empty() {
            self.open_permission_mode_picker();
            return;
        }

        let Some(code) = permission_mode_code_for_choice(raw_mode) else {
            self.push_command_output(
                command_name,
                format!(
                    "Unknown permission mode: {raw_mode}\nUsage: /permission-mode [supervised|plan|accept-edits|full-auto|dont-ask]"
                ),
                true,
            );
            return;
        };

        self.apply_permission_mode_code(code);
    }

    fn handle_add_dir_command(&mut self, args_raw: &str) {
        let path = args_raw.trim();
        if path.is_empty() {
            self.push_command_output(
                "add-dir",
                "Usage: /add-dir <path>\nAdd a directory to this session's working directories.",
                true,
            );
            return;
        }

        let cwd = PathBuf::from(&self.engine_config.cwd);
        let validation = mossen_commands::add_dir_validation::validate_add_dir(path, &cwd);
        if !validation.is_valid {
            self.push_command_output(
                "add-dir",
                validation
                    .reason
                    .unwrap_or_else(|| "Directory cannot be added.".to_string()),
                true,
            );
            return;
        }
        let Some(resolved) = validation.resolved_path else {
            self.push_command_output("add-dir", "Directory path could not be resolved.", true);
            return;
        };

        let mut current_dirs = Vec::new();
        current_dirs.push(cwd.canonicalize().unwrap_or(cwd));
        current_dirs.extend(
            self.additional_working_directories
                .iter()
                .map(PathBuf::from)
                .map(|path| path.canonicalize().unwrap_or(path)),
        );
        if let Some(existing) = current_dirs
            .iter()
            .find(|working_dir| resolved.starts_with(working_dir))
        {
            self.push_command_output(
                "add-dir",
                format!(
                    "{} is already accessible within the existing working directory {}.",
                    resolved.display(),
                    existing.display()
                ),
                false,
            );
            return;
        }

        let absolute_path = resolved.to_string_lossy().to_string();
        if !self
            .additional_working_directories
            .iter()
            .any(|existing| existing == &absolute_path)
        {
            self.additional_working_directories
                .push(absolute_path.clone());
        }
        self.push_command_output(
            "add-dir",
            format!(
                "Added {} as a working directory for future tool calls.",
                absolute_path
            ),
            false,
        );
    }

    fn handle_copy_command(&mut self, args_raw: &str) {
        let (text, label) = match self.copy_command_payload(args_raw) {
            Ok(payload) => payload,
            Err(message) => {
                self.push_command_output("copy", message, true);
                return;
            }
        };
        match write_clipboard_text(&text) {
            Ok(()) => {
                self.push_command_output("copy", format!("Copied {label} to clipboard."), false);
            }
            Err(error) => {
                self.push_command_output(
                    "copy",
                    format!("Failed to copy to clipboard: {error}"),
                    true,
                );
            }
        }
    }

    fn copy_command_payload(&self, args_raw: &str) -> Result<(String, String), String> {
        let trimmed = args_raw.trim();
        if matches!(trimmed, "help" | "-h" | "--help") {
            return Err(
                "Usage: /copy [N|transcript|all]\nN copies an assistant response; transcript/all copies the current conversation as text."
                    .to_string(),
            );
        }
        if matches!(trimmed, "transcript" | "all") {
            let body = self.export_transcript_text();
            if body.trim().is_empty() {
                return Err("No transcript content to copy.".to_string());
            }
            return Ok((body, "conversation transcript".to_string()));
        }

        let age = match copy_response_index(args_raw) {
            Ok(age) => age,
            Err(message) => return Err(message),
        };
        let Some(message) = self
            .messages
            .iter()
            .rev()
            .filter(|message| {
                message.message_type == MessageType::Assistant && !message.content.trim().is_empty()
            })
            .nth(age)
        else {
            return Err("No assistant message to copy.".to_string());
        };
        let text = message
            .full_content
            .as_deref()
            .unwrap_or(message.content.as_str());
        let label = if age == 0 {
            "latest assistant response".to_string()
        } else {
            format!("assistant response #{}", age + 1)
        };
        Ok((text.to_string(), label))
    }

    fn handle_export_command(&mut self, args_raw: &str) {
        let trimmed = args_raw.trim();
        if matches!(trimmed, "help" | "-h" | "--help") {
            self.push_command_output(
                "export",
                "Usage: /export [md|json|txt] [path]\nDefault: /export md conversation-export.md",
                false,
            );
            return;
        }

        let (format, raw_path) = parse_export_request(trimmed);
        let path = self.resolve_export_path(raw_path.as_deref(), format);
        let body = match self.export_transcript_body(format) {
            Ok(body) => body,
            Err(error) => {
                self.push_command_output(
                    "export",
                    format!("Failed to render export: {error}"),
                    true,
                );
                return;
            }
        };

        if let Some(parent) = path.parent() {
            if let Err(error) = std::fs::create_dir_all(parent) {
                self.push_command_output(
                    "export",
                    format!(
                        "Failed to create export directory {}: {error}",
                        parent.display()
                    ),
                    true,
                );
                return;
            }
        }

        match std::fs::write(&path, body) {
            Ok(()) => self.push_command_output(
                "export",
                format!("Exported conversation transcript to {}", path.display()),
                false,
            ),
            Err(error) => self.push_command_output(
                "export",
                format!("Failed to write export {}: {error}", path.display()),
                true,
            ),
        }
    }

    fn resolve_export_path(
        &self,
        raw_path: Option<&str>,
        format: TranscriptExportFormat,
    ) -> PathBuf {
        let path = raw_path
            .filter(|path| !path.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(format!("conversation-export.{}", format.extension()))
            });
        let path = if path.is_absolute() {
            path
        } else {
            PathBuf::from(&self.engine_config.cwd).join(path)
        };
        if path.extension().is_some() {
            path
        } else {
            path.with_extension(format.extension())
        }
    }

    fn export_transcript_body(
        &self,
        format: TranscriptExportFormat,
    ) -> Result<String, serde_json::Error> {
        match format {
            TranscriptExportFormat::Markdown => Ok(self.export_transcript_markdown()),
            TranscriptExportFormat::Text => Ok(self.export_transcript_text()),
            TranscriptExportFormat::Json => self.export_transcript_json(),
        }
    }

    fn export_transcript_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str("# Mossen Conversation Export\n\n");
        out.push_str(&format!("Messages: {}\n\n", self.messages.len()));
        for message in &self.messages {
            out.push_str("## ");
            out.push_str(export_message_label(message));
            out.push_str("\n\n");
            out.push_str(export_message_content(message));
            out.push_str("\n\n");
        }
        out
    }

    fn export_transcript_text(&self) -> String {
        let mut out = String::new();
        for message in &self.messages {
            out.push_str(export_message_label(message));
            out.push_str(":\n");
            out.push_str(export_message_content(message));
            out.push_str("\n\n");
        }
        out
    }

    fn export_transcript_json(&self) -> Result<String, serde_json::Error> {
        let messages = self
            .messages
            .iter()
            .enumerate()
            .map(|(index, message)| {
                serde_json::json!({
                    "index": index,
                    "type": export_message_type(message),
                    "isError": message.is_error,
                    "isStreaming": message.is_streaming,
                    "toolName": message.tool_name,
                    "content": export_message_content(message),
                    "thinking": message.thinking,
                })
            })
            .collect::<Vec<_>>();
        serde_json::to_string_pretty(&serde_json::json!({
            "version": 1,
            "messageCount": self.messages.len(),
            "messages": messages,
        }))
    }

    fn handle_context_command(&mut self) {
        let context = self.context_usage_render_model();
        let mut lines = Vec::new();
        lines.push("Context Usage".to_string());
        lines.push(format!("Model: {}", self.engine_config.model));
        if let Some(context) = context {
            lines.push(format!(
                "Estimated tokens: {} / {} ({})",
                context.used_tokens,
                context.window_tokens,
                context.label()
            ));
        } else {
            lines.push("Estimated tokens: unavailable".to_string());
        }
        lines.push(format!(
            "Engine history messages: {}",
            self.engine_history.len()
        ));
        lines.push(format!(
            "Visible transcript messages: {}",
            self.messages.len()
        ));
        lines.push(format!(
            "Compact: {}",
            if self.state.compact_in_progress {
                self.state
                    .compact_progress
                    .as_deref()
                    .unwrap_or("in progress")
            } else {
                "idle"
            }
        ));
        self.push_command_output("context", lines.join("\n"), false);
    }

    fn open_widget_command(&mut self, command: &str, args_raw: &str) -> bool {
        let args = args_raw.trim();
        match command {
            "model" if args.is_empty() => {
                self.active_modal = ActiveModal::ModelPicker(self.build_model_picker_state());
                true
            }
            "skills" if args.is_empty() || args == "list" => {
                self.active_modal = ActiveModal::SkillsPanel(self.build_skills_panel_state());
                true
            }
            "memory" if args.is_empty() => {
                self.active_modal = ActiveModal::MemoryPanel(self.build_memory_panel_state());
                true
            }
            _ => false,
        }
    }

    fn build_raw_transcript_state(&self) -> RawTranscriptState {
        let records = TranscriptRecords::from_messages_and_decisions_with_full_record_metadata(
            &self.messages,
            &self.approval_decisions,
            &self.render_record_id_overrides,
            &self.render_record_parent_overrides,
            &self.render_record_turn_overrides,
        );
        let transcript = RenderTranscript::from_records(&records);
        let snapshot = self.render_session_snapshot_from_records(records.clone());
        let relations = records.relation_index();
        let transcript_cache = self.render_transcript_cache_stats();
        let frame_scheduler = self.render_frame_scheduler_stats();
        let mut lines = Vec::new();

        lines.push("explicit /raw debug view; normal transcript keeps semantic rendering".into());
        lines.push(format!(
            "messages={} records={} visible_blocks={} approval_sidecars={} final_summaries={} raw_events={} current_turn={}",
            self.messages.len(),
            records.entries.len(),
            transcript.blocks.len(),
            records.approval_decisions.len(),
            records.final_summaries.len(),
            self.raw_engine_event_history.len(),
            self.current_render_turn_id.as_deref().unwrap_or("-")
        ));
        lines.push(format!(
            "snapshot version={} session={} records={} raw_events={} latest_turn={} json_bytes={}",
            snapshot.version,
            snapshot.session_id.as_deref().unwrap_or("-"),
            snapshot.record_count(),
            snapshot.raw_event_count(),
            snapshot.latest_turn_id.as_deref().unwrap_or("-"),
            snapshot.to_json().map(|payload| payload.len()).unwrap_or(0)
        ));
        lines.push(format!(
            "autosave status={} path={} error={}",
            if self.render_snapshot_autosave_error.is_some() {
                "error"
            } else if self.render_snapshot_autosave_path.is_some() {
                "saved"
            } else {
                "pending"
            },
            self.render_snapshot_autosave_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| self
                    .default_render_session_snapshot_path()
                    .display()
                    .to_string()),
            self.render_snapshot_autosave_error
                .as_deref()
                .unwrap_or("-")
        ));
        lines.push(format!(
            "startup restore status={} path={} error={}",
            self.render_snapshot_startup_restore_status.as_str(),
            self.render_snapshot_startup_restore_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| self
                    .render_session_snapshot_dir_path()
                    .display()
                    .to_string()),
            self.render_snapshot_startup_restore_error
                .as_deref()
                .unwrap_or("-")
        ));
        lines.push(format!(
            "external statusline configured={} in_flight={} output={} error={}",
            self.state.footer_config.external_command.is_some(),
            bool_label(self.external_statusline_in_flight),
            self.external_statusline_output
                .as_deref()
                .map(raw_debug_preview)
                .unwrap_or_else(|| "-".to_string()),
            self.external_statusline_error
                .as_deref()
                .map(raw_debug_preview)
                .unwrap_or_else(|| "-".to_string())
        ));
        lines.push(format!(
            "statusline config status={} path={} error={}",
            self.footer_config_persistence_status.as_str(),
            self.footer_config_persistence_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| self
                    .default_footer_render_config_path()
                    .display()
                    .to_string()),
            self.footer_config_persistence_error
                .as_deref()
                .unwrap_or("-")
        ));
        lines.push(format!(
            "relations roots={} parented={} parents={} orphans={}",
            relations.roots.len(),
            relations.parented_count(),
            relations.parent_count(),
            relations.orphan_count()
        ));

        if !self.raw_engine_event_history.is_empty() {
            lines.push("engine events".to_string());
            for event in &self.raw_engine_event_history {
                lines.push(format!(
                    "event {} turn={} scope={} kind={} summary={}",
                    event.sequence,
                    event.turn_id.as_deref().unwrap_or("-"),
                    event.scope_label(),
                    event.kind.as_str(),
                    event.summary
                ));
                push_raw_line(&mut lines, "payload", &event.payload_preview);
            }
        }

        if !relations.children_by_parent.is_empty() {
            lines.push("record relations".to_string());
            for (parent_id, child_ids) in &relations.children_by_parent {
                lines.push(format!(
                    "parent {} children={}",
                    parent_id,
                    child_ids.join(",")
                ));
            }
            if !relations.missing_parent_ids.is_empty() {
                lines.push(format!(
                    "missing parents {}",
                    relations.missing_parent_ids.join(",")
                ));
            }
        }

        if self.messages.is_empty() {
            lines.push("message list is empty".to_string());
        } else {
            lines.push("messages".to_string());
            for (index, message) in self.messages.iter().enumerate() {
                lines.push(format!(
                    "message {index} turn={} kind={:?} streaming={} error={} tool={}",
                    self.render_record_turn_overrides
                        .get(&index)
                        .map(String::as_str)
                        .unwrap_or("-"),
                    message.message_type,
                    message.is_streaming,
                    message.is_error,
                    message.tool_name.as_deref().unwrap_or("-")
                ));
                push_raw_line(&mut lines, "content", &message.content);
                if let Some(full_content) = message.full_content.as_ref() {
                    push_raw_line(&mut lines, "full_content", full_content);
                }
                if let Some(thinking) = message.thinking.as_ref() {
                    push_raw_line(&mut lines, "thinking", thinking);
                }
            }
        }

        if !records.entries.is_empty() {
            lines.push("records".to_string());
            for record in &records.entries {
                lines.push(format!(
                    "record {} source={} turn={} kind={:?} phase={:?} parent={} tool={}",
                    record.id,
                    record.source_index,
                    record.turn_id.as_deref().unwrap_or("-"),
                    record.kind,
                    record.lifecycle,
                    record.parent_id.as_deref().unwrap_or("-"),
                    record.tool_name.as_deref().unwrap_or("-")
                ));
                push_raw_line(&mut lines, "content", &record.content);
                if let Some(full_content) = record.full_content.as_ref() {
                    push_raw_line(&mut lines, "full_content", full_content);
                }
            }
        }

        if !records.approval_decisions.is_empty() {
            lines.push("approval sidecars".to_string());
            for decision in &records.approval_decisions {
                lines.push(format!(
                    "sidecar approval id={} tool={} decision={:?} anchor={}",
                    decision.id,
                    decision.tool_name,
                    decision.decision,
                    decision.anchor_block_id.as_deref().unwrap_or("-")
                ));
                push_raw_line(&mut lines, "detail", &decision.detail);
            }
        }

        if !records.final_summaries.is_empty() {
            lines.push("final summary sidecars".to_string());
            for summary in &records.final_summaries {
                lines.push(format!(
                    "sidecar final source={} id={} success={} terminal={}",
                    summary.source_index,
                    summary.model.id,
                    summary.model.success,
                    summary.model.terminal
                ));
            }
        }

        if !transcript.blocks.is_empty() {
            lines.push("visible semantic blocks".to_string());
            for block in &transcript.blocks {
                lines.push(format!(
                    "visible {} kind={:?} sources={:?} streaming={} error={}",
                    block.id,
                    block.kind,
                    block.source_indices,
                    block.state.streaming,
                    block.state.error
                ));
            }
        }

        lines.push(format!(
            "render transcript cache revision={} cached={} hits={} misses={}",
            transcript_cache.revision,
            bool_label(transcript_cache.cached),
            transcript_cache.hits,
            transcript_cache.misses
        ));
        lines.push(format!(
            "render frame scheduler dirty={} throttled_due_in_ms={} next_frame_due_in_ms={} active_animation={} drawn={} skipped={} last_frame_age_ms={} last_frame_duration_ms={} max_frame_duration_ms={} avg_frame_duration_ms={} active_frame_interval_ms={}",
            bool_label(frame_scheduler.dirty),
            frame_scheduler
                .throttled_due_in_ms
                .map(|due| due.to_string())
                .unwrap_or_else(|| "-".to_string()),
            frame_scheduler
                .next_frame_due_in_ms
                .map(|due| due.to_string())
                .unwrap_or_else(|| "-".to_string()),
            bool_label(frame_scheduler.active_animation),
            frame_scheduler.drawn,
            frame_scheduler.skipped,
            frame_scheduler
                .last_frame_age_ms
                .map(|age| age.to_string())
                .unwrap_or_else(|| "-".to_string()),
            frame_scheduler
                .last_frame_duration_ms
                .map(|duration| duration.to_string())
                .unwrap_or_else(|| "-".to_string()),
            frame_scheduler.max_frame_duration_ms,
            frame_scheduler
                .avg_frame_duration_ms
                .map(|duration| duration.to_string())
                .unwrap_or_else(|| "-".to_string()),
            frame_scheduler.active_frame_interval_ms
        ));

        RawTranscriptState::new(lines)
    }

    fn build_diff_review_state(&self) -> Option<DiffReviewState> {
        let transcript = self.render_transcript_model();
        let mut files = Vec::<FileDiff>::new();

        for block in &transcript.blocks {
            for node in &block.nodes {
                let RenderNode::ToolCard(tool) = node else {
                    continue;
                };
                for section in &tool.sections {
                    if section.kind != ToolSectionKind::Diff && !section.body.contains("diff --git")
                    {
                        continue;
                    }
                    files.extend(parse_unified_diff(&section.body));
                }
            }
        }

        if files.is_empty() {
            None
        } else {
            Some(DiffReviewState::new(files))
        }
    }

    fn build_process_list_state(&self) -> ProcessListState {
        ProcessListState::new(self.process_list_render_model())
    }

    fn build_file_changes_state(&self) -> FileChangesState {
        FileChangesState::new(self.file_changes_render_model())
    }

    fn build_render_timeline_state(&self) -> RenderTimelineState {
        RenderTimelineState::new(self.render_timeline_render_model())
    }

    fn build_command_history_state(&self) -> CommandHistoryState {
        CommandHistoryState::new(self.command_history_render_model())
    }

    fn build_error_history_state(&self) -> ErrorHistoryState {
        ErrorHistoryState::new(self.error_history_render_model())
    }

    fn build_final_summary_history_state(&self) -> FinalSummaryHistoryState {
        FinalSummaryHistoryState::new(self.final_summary_history_render_model())
    }

    fn build_approval_history_state(&self) -> ApprovalHistoryState {
        ApprovalHistoryState::new(self.approval_history_render_model())
    }

    fn build_debug_config_state(&self) -> DebugConfigState {
        DebugConfigState::new(self.debug_config_render_model())
    }

    fn build_title_config_state(&self) -> TitleConfigState {
        TitleConfigState::new(self.services.manual_title.clone().unwrap_or_default())
    }

    fn session_title_render_model(&self, state: &TitleConfigState) -> SessionTitleRenderModel {
        SessionTitleRenderModel::new(
            self.services.visible_title(),
            self.services.manual_title.clone(),
            state.draft.clone(),
        )
        .status(state.notice.clone())
        .footer("Enter saves")
    }

    fn file_changes_render_model(&self) -> FileChangeListRenderModel {
        FileChangeListRenderModel::from_files(file_change_summaries_from_messages(&self.messages))
    }

    fn render_timeline_render_model(&self) -> RenderTimelineRenderModel {
        RenderTimelineRenderModel::from_events(&self.render_event_history)
    }

    fn command_history_render_model(&self) -> CommandHistoryRenderModel {
        let transcript = self.render_transcript_model();
        let mut rows = command_history_from_transcript(&transcript).rows;
        if let Some(activity) = self.state.render_activity.current.as_ref() {
            if let Some(row) = command_history_row_from_activity(activity) {
                rows.insert(0, row);
            }
        }
        CommandHistoryRenderModel::from_rows(rows)
    }

    fn error_history_render_model(&self) -> ErrorHistoryRenderModel {
        let transcript = self.render_transcript_model();
        let mut rows = error_history_from_transcript(&transcript).rows;
        if let Some(activity) = self.state.render_activity.current.as_ref() {
            if let Some(row) = error_history_row_from_activity(activity) {
                rows.insert(0, row);
            }
        }
        ErrorHistoryRenderModel::from_rows(rows)
    }

    fn final_summary_history_render_model(&self) -> FinalSummaryHistoryRenderModel {
        let transcript = self.render_transcript_model();
        final_summary_history_from_transcript(&transcript)
    }

    fn approval_history_render_model(&self) -> ApprovalHistoryRenderModel {
        let transcript = self.render_transcript_model();
        let mut rows = approval_history_from_transcript(&transcript).rows;
        if let Some(approval) = self.active_approval_render_model() {
            rows.insert(0, ApprovalHistoryRowRenderModel::from_pending(approval));
        }
        ApprovalHistoryRenderModel::from_rows(rows)
    }

    fn debug_config_render_model(&self) -> DebugConfigRenderModel {
        let footer = self.footer_render_model();
        let transcript = self.render_transcript_model();
        let transcript_cache = self.render_transcript_cache_stats();
        let frame_scheduler = self.render_frame_scheduler_stats();
        let cache = self.render_height_cache.stats();
        let permission_mode = footer.access_mode.as_deref().unwrap_or("Supervised");
        let output_style = self
            .engine_config
            .output_style
            .as_deref()
            .unwrap_or("default");
        let summary = format!(
            "model {} | mode {} | glyphs {} | secrets redacted",
            self.engine_config.model,
            permission_mode,
            glyph_mode_label(self.glyphs.mode)
        );
        let session_id = self
            .engine_session_id
            .as_deref()
            .map(short_debug_id)
            .unwrap_or_else(|| "(not initialized)".to_string());
        let build_time = self
            .command_context
            .build_time
            .as_deref()
            .unwrap_or("(unknown)");
        let user_type = self
            .command_context
            .user_type
            .as_deref()
            .unwrap_or("(unset)");

        DebugConfigRenderModel::new(summary)
            .footer("Esc closes")
            .section(
                StatusSectionRenderModel::new("Session")
                    .row(
                        "Product",
                        format!(
                            "{} / {}",
                            self.command_context.product_name, self.command_context.cli_name
                        ),
                        StatusRowLevel::Info,
                    )
                    .row(
                        "Version",
                        &self.command_context.version,
                        StatusRowLevel::Info,
                    )
                    .row("Build Time", build_time, StatusRowLevel::Info)
                    .row(
                        "Session ID",
                        session_id,
                        if self.engine_session_id.is_some() {
                            StatusRowLevel::Info
                        } else {
                            StatusRowLevel::Warning
                        },
                    )
                    .row(
                        "Origin",
                        format!("{:?}", self.engine_config.origin_tag),
                        StatusRowLevel::Info,
                    ),
            )
            .section(
                StatusSectionRenderModel::new("Engine")
                    .row("Model", &self.engine_config.model, StatusRowLevel::Good)
                    .row("CWD", &self.engine_config.cwd, StatusRowLevel::Normal)
                    .row(
                        "API Base",
                        api_base_debug_label(self.engine_config.api_base_url.as_deref()),
                        StatusRowLevel::Info,
                    )
                    .row(
                        "API Key",
                        configured_label(self.engine_config.api_key.as_deref()),
                        if self
                            .engine_config
                            .api_key
                            .as_deref()
                            .is_some_and(|key| !key.trim().is_empty())
                        {
                            StatusRowLevel::Good
                        } else {
                            StatusRowLevel::Warning
                        },
                    )
                    .row(
                        "Max Turns",
                        self.engine_config
                            .max_turns
                            .map(|turns| turns.to_string())
                            .unwrap_or_else(|| "engine default".to_string()),
                        StatusRowLevel::Info,
                    )
                    .row(
                        "System Prompt",
                        format!("{} block(s)", self.engine_config.system_prompt.len()),
                        StatusRowLevel::Info,
                    )
                    .row(
                        "Extra Body",
                        redacted_extra_body_keys(&self.engine_config.extra_body),
                        StatusRowLevel::Info,
                    )
                    .row("Output Style", output_style, StatusRowLevel::Info),
            )
            .section(
                StatusSectionRenderModel::new("Policy")
                    .row("Permission Mode", permission_mode, StatusRowLevel::Normal)
                    .row(
                        "Noninteractive",
                        bool_label(self.command_context.is_non_interactive),
                        StatusRowLevel::Info,
                    )
                    .row(
                        "Remote Mode",
                        bool_label(self.command_context.is_remote_mode),
                        StatusRowLevel::Info,
                    )
                    .row(
                        "Custom Backend",
                        bool_label(self.command_context.is_custom_backend),
                        StatusRowLevel::Info,
                    )
                    .row("User Type", user_type, StatusRowLevel::Info),
            )
            .section(
                StatusSectionRenderModel::new("Renderer")
                    .row(
                        "Fullscreen",
                        bool_label(self.fullscreen),
                        StatusRowLevel::Info,
                    )
                    .row("Theme", self.theme.name.to_string(), StatusRowLevel::Info)
                    .row(
                        "Color Mode",
                        color_mode_label(self.theme.color_mode),
                        StatusRowLevel::Info,
                    )
                    .row(
                        "Glyphs",
                        glyph_mode_label(self.glyphs.mode),
                        StatusRowLevel::Info,
                    )
                    .row(
                        "Width Profiles",
                        "small <=79, medium 80-119, large >=120",
                        StatusRowLevel::Info,
                    )
                    .row(
                        "Height Cache",
                        format!(
                            "{} entries, {} hits, {} misses, {} clears",
                            cache.entries, cache.hits, cache.misses, cache.clears
                        ),
                        StatusRowLevel::Info,
                    )
                    .row(
                        "Transcript Cache",
                        format!(
                            "rev {}, cached {}, {} hits, {} misses",
                            transcript_cache.revision,
                            bool_label(transcript_cache.cached),
                            transcript_cache.hits,
                            transcript_cache.misses
                        ),
                        StatusRowLevel::Info,
                    )
                    .row(
                        "Frame Scheduler",
                        format!(
                            "dirty {}, next {}, active {}, drawn {}, skipped {}",
                            bool_label(frame_scheduler.dirty),
                            frame_scheduler
                                .next_frame_due_in_ms
                                .map(|due| format!("{due}ms"))
                                .unwrap_or_else(|| "-".to_string()),
                            bool_label(frame_scheduler.active_animation),
                            frame_scheduler.drawn,
                            frame_scheduler.skipped
                        ),
                        StatusRowLevel::Info,
                    ),
            )
            .section(
                StatusSectionRenderModel::new("Footer")
                    .row(
                        "Left Items",
                        footer_item_labels(&footer.config.left_items),
                        StatusRowLevel::Info,
                    )
                    .row(
                        "Right Items",
                        footer_item_labels(&footer.config.right_items),
                        StatusRowLevel::Info,
                    )
                    .row(
                        "Context",
                        status_context_value(footer.context),
                        status_level_for_context(footer.context),
                    )
                    .row(
                        "Blocking",
                        footer
                            .blocking
                            .as_ref()
                            .map(|blocking| format!("{}: {}", blocking.title, blocking.detail))
                            .unwrap_or_else(|| "none".to_string()),
                        footer
                            .blocking
                            .as_ref()
                            .map(|blocking| status_level_for_blocking(blocking.kind))
                            .unwrap_or(StatusRowLevel::Good),
                    ),
            )
            .section(
                StatusSectionRenderModel::new("Runtime")
                    .row(
                        "Messages",
                        format!(
                            "{} visual / {} engine",
                            self.messages.len(),
                            self.engine_history.len()
                        ),
                        StatusRowLevel::Info,
                    )
                    .row(
                        "Transcript",
                        format!(
                            "{} blocks from {} source records",
                            transcript.blocks.len(),
                            transcript.source_record_count()
                        ),
                        StatusRowLevel::Info,
                    )
                    .row(
                        "Approvals",
                        format!("{} decision sidecar(s)", self.approval_decisions.len()),
                        StatusRowLevel::Info,
                    )
                    .row(
                        "Slash Catalog",
                        format!("{} visible command(s)", self.state.all_slash_commands.len()),
                        StatusRowLevel::Info,
                    )
                    .row(
                        "Skills",
                        format!(
                            "{} known / registry {}",
                            self.known_skill_names.len(),
                            if self.skill_registry.is_some() {
                                "configured"
                            } else {
                                "unavailable"
                            }
                        ),
                        StatusRowLevel::Info,
                    )
                    .row(
                        "Tool Registry",
                        if self.tool_registry.is_some() {
                            "configured"
                        } else {
                            "unavailable"
                        },
                        StatusRowLevel::Info,
                    )
                    .row(
                        "MCP Servers",
                        format!("{} tracked", self.state.mcp_servers.len()),
                        StatusRowLevel::Info,
                    )
                    .row(
                        "Pending Images",
                        format!("{} attachment(s)", self.pending_images.len()),
                        StatusRowLevel::Info,
                    ),
            )
    }

    fn process_list_render_model(&self) -> ProcessListRenderModel {
        let blocking = self.current_blocking_render_model();
        let footer = self.footer_render_model();
        let mut rows = Vec::<ProcessRowRenderModel>::new();

        let turn_detail = blocking
            .as_ref()
            .map(|blocking| format!("{}: {}", blocking.title, blocking.detail))
            .or_else(|| self.state.render_activity.status_line())
            .unwrap_or_else(|| "No active turn activity.".to_string());
        let mut turn_row = ProcessRowRenderModel::new(
            "turn",
            ProcessRowKind::Turn,
            process_status_for_turn(
                self.state.turn_state,
                self.state.ui_stage,
                blocking.as_ref(),
            ),
            "Current turn",
        )
        .detail(turn_detail)
        .fact("id", self.render_turn_id_label())
        .fact("stage", self.state.ui_stage.label())
        .fact("turn", self.turn_state_label())
        .fact("messages", self.messages.len().to_string());
        if let Some(model) = footer.model.as_ref() {
            turn_row = turn_row.fact("model", model.clone());
        }
        if let Some(reasoning) = footer.reasoning.as_ref() {
            turn_row = turn_row.fact("reasoning", reasoning.clone());
        }
        rows.push(turn_row);

        if let Some(blocking) = blocking.as_ref() {
            rows.push(
                ProcessRowRenderModel::new(
                    "blocking",
                    ProcessRowKind::Blocking,
                    process_status_for_blocking(blocking.kind),
                    blocking.title.clone(),
                )
                .detail(blocking.detail.clone())
                .fact("kind", format!("{:?}", blocking.kind).to_ascii_lowercase()),
            );
        }

        if let Some(activity) = self.state.render_activity.current.as_ref() {
            if self.state.ui_stage != UiStage::Idle
                && !matches!(activity, RenderActivity::Final { .. })
            {
                rows.push(process_row_from_activity(self.state.ui_stage, activity));
            }
        }

        if self.state.compact_in_progress || self.state.compact_progress.is_some() {
            let status = if self.state.compact_in_progress {
                ProcessStatus::Running
            } else {
                ProcessStatus::Info
            };
            rows.push(
                ProcessRowRenderModel::new("compact", ProcessRowKind::Compact, status, "Compact")
                    .detail(
                        self.state
                            .compact_progress
                            .clone()
                            .unwrap_or_else(|| "Compacting conversation history".to_string()),
                    ),
            );
        }

        if let Some(task_id) = self.state.foreground_task_id.as_ref() {
            rows.push(
                ProcessRowRenderModel::new(
                    format!("foreground-{task_id}"),
                    ProcessRowKind::TaskStore,
                    ProcessStatus::Running,
                    "Foreground task",
                )
                .detail(task_id.clone()),
            );
        }
        if self.state.background_task_count > 0 {
            rows.push(
                ProcessRowRenderModel::new(
                    "background-count",
                    ProcessRowKind::TaskStore,
                    ProcessStatus::Running,
                    "Background tasks",
                )
                .detail(format!(
                    "{} task(s) running",
                    self.state.background_task_count
                )),
            );
        }

        for todo in &self.state.task_list.tasks {
            rows.push(
                ProcessRowRenderModel::new(
                    format!("todo-{}", todo.id),
                    ProcessRowKind::Todo,
                    process_status_from_todo(&todo.status),
                    todo.content.clone(),
                )
                .fact("id", todo.id.clone())
                .fact("status", todo.status.clone()),
            );
        }

        if let Some(provider) = &self.task_snapshot_provider {
            for (status, id, subject) in provider() {
                rows.push(
                    ProcessRowRenderModel::new(
                        format!("taskstore-{id}"),
                        ProcessRowKind::TaskStore,
                        process_status_from_task_store(&status),
                        subject,
                    )
                    .fact("id", id)
                    .fact("status", status),
                );
            }
        }

        let mut teammate_rows: Vec<_> = self
            .state
            .teammate_states
            .iter()
            .map(|(id, state)| process_row_from_teammate(id, state))
            .collect();
        teammate_rows.sort_by(|a, b| a.id.cmp(&b.id));
        rows.extend(teammate_rows);

        let mut summary =
            ProcessSummaryRenderModel::new(self.state.ui_stage.label(), self.turn_state_label());
        summary.active_count = rows.iter().filter(|row| row.status.is_active()).count();
        summary.waiting_count = rows.iter().filter(|row| row.status.is_waiting()).count();
        summary.failed_count = rows.iter().filter(|row| row.status.is_failed()).count();
        ProcessListRenderModel::new(summary, rows)
    }

    fn build_model_picker_state(&self) -> crate::widgets::panels::ModelPickerState {
        use crate::widgets::panels::{ModelInfo, ModelPickerState};

        let current_profile =
            mossen_agent::services::config::profiles::get_current_profile().map(|p| p.name);
        let profiles = mossen_agent::services::config::profiles::list_all_profiles();
        let mut models: Vec<ModelInfo> = profiles
            .into_iter()
            .map(|profile| {
                let is_current = current_profile.as_deref() == Some(profile.name.as_str())
                    || profile.profile.model == self.engine_config.model;
                let model = profile.profile.model.clone();
                ModelInfo {
                    id: profile.name.clone(),
                    name: model.clone(),
                    provider: format!("profile: {}", profile.name),
                    supports_thinking: model.to_lowercase().contains("m2"),
                    supports_streaming: true,
                    is_current,
                }
            })
            .collect();

        if models.is_empty() {
            models.push(ModelInfo {
                id: self.engine_config.model.clone(),
                name: self.engine_config.model.clone(),
                provider: self
                    .engine_config
                    .api_base_url
                    .clone()
                    .unwrap_or_else(|| "current session".to_string()),
                supports_thinking: self.engine_config.model.to_lowercase().contains("m2"),
                supports_streaming: true,
                is_current: true,
            });
        }

        ModelPickerState::new(models)
    }

    fn model_status_body(&self) -> String {
        let current_profile = mossen_agent::services::config::profiles::get_current_profile();
        let default_profile = mossen_agent::services::config::profiles::get_default_profile();
        let profiles = mossen_agent::services::config::profiles::list_all_profiles();
        let mut lines = vec![
            format!("Current model: {}", self.engine_config.model),
            format!(
                "Current profile: {}",
                current_profile
                    .as_ref()
                    .map(|profile| format!("{} ({})", profile.name, profile.profile.model))
                    .unwrap_or_else(|| "<none>".to_string())
            ),
            format!(
                "Default profile: {}",
                default_profile
                    .as_ref()
                    .map(|profile| format!("{} ({})", profile.name, profile.profile.model))
                    .unwrap_or_else(|| "<none>".to_string())
            ),
            format!("Configured profiles: {}", profiles.len()),
        ];
        for profile in profiles {
            let current_marker = if current_profile.as_ref().map(|p| p.name.as_str())
                == Some(profile.name.as_str())
            {
                " [current]"
            } else {
                ""
            };
            lines.push(format!(
                "  {}{} -> {}",
                profile.name, current_marker, profile.profile.model
            ));
        }
        lines.push("Usage: /model, /model <profile>, /model <model-id>, /model reset".to_string());
        lines.join("\n")
    }

    fn set_custom_backend_env_for_model_profile(
        profile: &mossen_agent::services::config::profiles::ListedProfile,
    ) {
        std::env::set_var("MOSSEN_CODE_USE_CUSTOM_BACKEND", "1");
        std::env::set_var(
            "MOSSEN_CODE_CUSTOM_BACKEND_PROTOCOL",
            profile.profile.provider.runtime_protocol(),
        );
        std::env::set_var("MOSSEN_CODE_CUSTOM_NAME", &profile.name);
        std::env::set_var("MOSSEN_CODE_CUSTOM_BASE_URL", &profile.profile.base_url);
        std::env::set_var("MOSSEN_CODE_CUSTOM_API_KEY", &profile.profile.api_key);
        std::env::set_var("MOSSEN_CODE_CUSTOM_MODEL", &profile.profile.model);
        std::env::set_var("MOSSEN_API_BASE_URL", &profile.profile.base_url);
        std::env::set_var("MOSSEN_API_KEY", &profile.profile.api_key);
    }

    fn apply_listed_model_profile(
        &mut self,
        profile: mossen_agent::services::config::profiles::ListedProfile,
        set_session_override: bool,
    ) -> Result<(), String> {
        if set_session_override {
            mossen_agent::services::config::profiles::set_session_active_profile(&profile.name)
                .map_err(|error| error.to_string())?;
        }
        Self::set_custom_backend_env_for_model_profile(&profile);
        self.engine_config.model = profile.profile.model.clone();
        self.engine_config.api_base_url = Some(profile.profile.base_url.clone());
        self.engine_config.api_key = Some(profile.profile.api_key.clone());
        self.state.current_model = Some(profile.profile.model.clone());
        Ok(())
    }

    fn find_model_profile(
        requested: &str,
    ) -> Option<mossen_agent::services::config::profiles::ListedProfile> {
        let requested = requested.trim();
        mossen_agent::services::config::profiles::list_all_profiles()
            .into_iter()
            .find(|profile| {
                profile.name == requested
                    || profile.name.eq_ignore_ascii_case(requested)
                    || profile.profile.model == requested
            })
    }

    fn build_skills_panel_state(&self) -> crate::widgets::panels::SkillsPanelState {
        use crate::widgets::panels::{SkillInfo, SkillsPanelState};
        let mut skills = Vec::new();
        let mut seen = std::collections::HashSet::new();

        if let Some(registry) = &self.skill_registry {
            if let Ok(reg) = registry.read() {
                for craft in reg.all_crafts() {
                    if craft.is_user_invocable() && seen.insert(craft.name().to_string()) {
                        skills.push(SkillInfo {
                            name: craft.name().to_string(),
                            description: craft.base.description.clone(),
                            enabled: true,
                        });
                    }
                }
            }
        }

        for craft in mossen_skills::get_dynamic_skills() {
            if craft.is_user_invocable() && seen.insert(craft.name().to_string()) {
                skills.push(SkillInfo {
                    name: craft.name().to_string(),
                    description: craft.base.description.clone(),
                    enabled: true,
                });
            }
        }

        for craft in mossen_skills::get_bundled_crafts() {
            if craft.is_user_invocable() && seen.insert(craft.name().to_string()) {
                skills.push(SkillInfo {
                    name: craft.name().to_string(),
                    description: craft.base.description.clone(),
                    enabled: true,
                });
            }
        }

        skills.sort_by(|a, b| a.name.cmp(&b.name));
        if skills.is_empty() {
            skills.push(SkillInfo {
                name: "No skills discovered".to_string(),
                description: "Install or add a SKILL.md to make it invocable here.".to_string(),
                enabled: false,
            });
        }
        SkillsPanelState::new(skills)
    }

    fn build_memory_panel_state(&self) -> crate::widgets::panels::MemoryPanelState {
        use crate::widgets::panels::{MemoryEntry, MemoryPanelState};
        let cwd = std::path::PathBuf::from(&self.engine_config.cwd);
        let mut entries = Vec::new();
        for name in ["MOSSEN.md", "MOSSEN.local.md"] {
            let path = cwd.join(name);
            if path.exists() {
                entries.push(memory_entry_for_path(&cwd, &path, "project"));
            }
        }
        let mossen_dir = cwd.join(".mossen");
        if let Ok(dir) = std::fs::read_dir(&mossen_dir) {
            for entry in dir.flatten() {
                let path = entry.path();
                if path.is_file() {
                    entries.push(memory_entry_for_path(&cwd, &path, ".mossen"));
                }
            }
        }
        let rules_dir = mossen_dir.join("rules");
        if let Ok(dir) = std::fs::read_dir(&rules_dir) {
            for entry in dir.flatten() {
                let path = entry.path();
                if path.is_file() {
                    entries.push(memory_entry_for_path(&cwd, &path, "rules"));
                }
            }
        }
        entries.sort_by(|a, b| a.title.cmp(&b.title));
        if entries.is_empty() {
            entries.push(MemoryEntry {
                title: "No memory files found".to_string(),
                category: "absent".to_string(),
                preview: "Create MOSSEN.md or .mossen/rules/*.md for durable project guidance."
                    .to_string(),
            });
        }
        MemoryPanelState::new(entries)
    }

    fn apply_model_picker_choice(&mut self, profile_name: &str) {
        let selected = Self::find_model_profile(profile_name);

        match selected {
            Some(profile) => {
                let profile_name = profile.name.clone();
                let model = profile.profile.model.clone();
                let base_url = profile.profile.base_url.clone();
                match self.apply_listed_model_profile(profile, true) {
                    Ok(()) => self.push_command_output(
                        "model",
                        format!(
                            "Switched session profile to \"{}\".\nmodel: {}\nbackend: {}",
                            profile_name, model, base_url
                        ),
                        false,
                    ),
                    Err(error) => self.push_command_output("model", error, true),
                }
            }
            None if profile_name == self.engine_config.model => {
                self.push_command_output(
                    "model",
                    format!("Keeping current model: {}", self.engine_config.model),
                    false,
                );
            }
            None => {
                self.engine_config.model = profile_name.to_string();
                self.state.current_model = Some(profile_name.to_string());
                if mossen_utils::custom_backend::is_custom_backend_enabled() {
                    std::env::set_var("MOSSEN_CODE_CUSTOM_MODEL", profile_name);
                }
                self.push_command_output(
                    "model",
                    format!("Session model set to: {}", profile_name),
                    false,
                );
            }
        }
    }

    fn reset_model_choice(&mut self) {
        mossen_agent::services::config::profiles::clear_session_active_profile();
        if let Some(profile) = mossen_agent::services::config::profiles::get_current_profile() {
            let profile_name = profile.name.clone();
            let model = profile.profile.model.clone();
            match self.apply_listed_model_profile(profile, false) {
                Ok(()) => self.push_command_output(
                    "model",
                    format!(
                        "Reset session model to default profile \"{}\".\nmodel: {}",
                        profile_name, model
                    ),
                    false,
                ),
                Err(error) => self.push_command_output("model", error, true),
            }
            return;
        }

        self.push_command_output(
            "model",
            "No default model profile is configured. Use /model <profile> or add one with --add-model-profile.",
            true,
        );
    }

    fn handle_theme_command(&mut self, args: &[&str]) {
        if args.is_empty() {
            self.active_modal = ActiveModal::Picker {
                kind: PickerKind::Theme,
                title: "Select theme".to_string(),
                items: theme_picker_items(),
                selected: 0,
            };
            return;
        }

        let requested = args[0];
        if matches!(requested, "help" | "-h" | "--help") {
            self.push_command_output(
                "theme",
                "Usage: /theme [dark|light|dark-high-contrast|light-high-contrast]",
                false,
            );
            return;
        }

        let Some((name, display)) = parse_theme_choice(requested) else {
            self.push_command_output(
                "theme",
                format!(
                    "Unknown theme: \"{}\". Available themes: dark, light, dark-high-contrast, light-high-contrast",
                    requested
                ),
                true,
            );
            return;
        };

        self.apply_theme_name(name);
        self.push_command_output("theme", format!("Theme set to: {}", display), false);
    }

    fn handle_fast_command(&mut self, args: &[&str]) {
        let first = args.first().copied().unwrap_or("");
        if matches!(first, "help" | "-h" | "--help") {
            self.push_command_output("fast", "Usage: /fast [on|off|status]", false);
            return;
        }

        if matches!(first, "status" | "current" | "show") {
            let enabled = self.engine_config.fast_mode.unwrap_or(self.state.fast_mode);
            self.push_command_output(
                "fast",
                format!(
                    "Fast mode: {}",
                    if enabled { "enabled" } else { "disabled" }
                ),
                false,
            );
            return;
        }

        let next = if first.is_empty() {
            !self.engine_config.fast_mode.unwrap_or(self.state.fast_mode)
        } else if let Some(value) = parse_bool_arg(first) {
            value
        } else {
            self.push_command_output(
                "fast",
                format!("Invalid value: \"{}\". Use on/off/status.", first),
                true,
            );
            return;
        };

        self.engine_config.fast_mode = Some(next);
        self.state.fast_mode = next;
        self.command_context.env_vars.insert(
            "MOSSEN_FAST_MODE".to_string(),
            if next { "1" } else { "0" }.to_string(),
        );
        self.push_command_output(
            "fast",
            format!(
                "Fast mode: {}. The next model request will use this mode.",
                if next { "enabled" } else { "disabled" }
            ),
            false,
        );
    }

    fn handle_effort_command(&mut self, args: &[&str]) {
        let first = args.first().copied().unwrap_or("");
        if matches!(first, "help" | "-h" | "--help") {
            self.push_command_output(
                "effort",
                "Usage: /effort [low|medium|high|max|auto|status]",
                false,
            );
            return;
        }

        if first.is_empty() || matches!(first, "status" | "current" | "show") {
            let label = self
                .engine_config
                .effort
                .map(|level| level.as_str().to_string())
                .or_else(|| std::env::var("MOSSEN_CODE_EFFORT_LEVEL").ok())
                .unwrap_or_else(|| "auto".to_string());
            self.push_command_output("effort", format!("Effort level: {}", label), false);
            return;
        }

        if matches!(first, "auto" | "unset" | "reset" | "default") {
            self.engine_config.effort = None;
            self.command_context
                .env_vars
                .remove("MOSSEN_CODE_EFFORT_LEVEL");
            self.push_command_output(
                "effort",
                "Effort level reset to auto. The next model request will use the backend default.",
                false,
            );
            return;
        }

        let Some(level) = parse_effort_level(first) else {
            self.push_command_output(
                "effort",
                format!(
                    "Invalid argument: {}. Valid options are: low, medium, high, max, auto",
                    first
                ),
                true,
            );
            return;
        };

        self.engine_config.effort = Some(level);
        self.command_context.env_vars.insert(
            "MOSSEN_CODE_EFFORT_LEVEL".to_string(),
            level.as_str().to_string(),
        );
        self.push_command_output(
            "effort",
            format!(
                "Effort level set to: {}. The next model request will carry the provider-specific reasoning control.",
                level.as_str()
            ),
            false,
        );
    }

    fn handle_output_style_command(&mut self, args: &[&str]) {
        let first = args.first().copied().unwrap_or("");
        if first.is_empty() {
            self.active_modal = ActiveModal::Picker {
                kind: PickerKind::OutputStyle,
                title: "Select output style".to_string(),
                items: output_style_picker_items(),
                selected: 0,
            };
            return;
        }

        if matches!(first, "help" | "-h" | "--help") {
            self.push_command_output(
                "output-style",
                "Usage: /output-style [default|concise|explanatory|code-first]",
                false,
            );
            return;
        }

        if matches!(first, "list" | "status" | "current" | "show") {
            let label = self
                .engine_config
                .output_style
                .as_deref()
                .unwrap_or("Default");
            self.push_command_output("output-style", format!("Output style: {}", label), false);
            return;
        }

        let Some(display) = self.apply_output_style_choice(first) else {
            self.push_command_output(
                "output-style",
                format!(
                    "Unknown output style: \"{}\". Available: default, concise, explanatory, code-first",
                    first
                ),
                true,
            );
            return;
        };

        self.push_command_output(
            "output-style",
            format!("Output style set to: {}", display),
            false,
        );
    }

    fn handle_proactive_command(&mut self, args: &[&str]) {
        let first = args.first().copied().unwrap_or("");
        if matches!(first, "help" | "-h" | "--help") {
            self.push_command_output("proactive", "Usage: /proactive [on|off|status]", false);
            return;
        }

        if first.is_empty() || matches!(first, "status" | "current" | "show") {
            let enabled = self
                .command_context
                .env_vars
                .get("MOSSEN_PROACTIVE")
                .and_then(|value| parse_bool_arg(value))
                .unwrap_or(false);
            self.push_command_output(
                "proactive",
                format!(
                    "Proactive suggestions: {}",
                    if enabled { "enabled" } else { "disabled" }
                ),
                false,
            );
            return;
        }

        let Some(next) = parse_bool_arg(first) else {
            self.push_command_output(
                "proactive",
                format!("Invalid value: \"{}\". Use on/off/status.", first),
                true,
            );
            return;
        };

        self.apply_proactive_mode(next);
        self.push_command_output(
            "proactive",
            format!(
                "Proactive suggestions: {}. The next model request will include this instruction.",
                if next { "enabled" } else { "disabled" }
            ),
            false,
        );
    }

    fn open_help_dialog(&mut self, query: &str) {
        self.active_modal = ActiveModal::HelpDialog(HelpDialogState::new(query.trim()));
    }

    fn command_cost_snapshot(&self) -> CommandCostSnapshot {
        CommandCostSnapshot {
            total_cost_usd: self.total_cost_usd,
            ..Default::default()
        }
    }

    /// Handle slash commands.
    fn handle_command(&mut self, cmd: &str) {
        self.command_output_scroll = 0;
        let parts: Vec<&str> = cmd.splitn(2, ' ').collect();
        let command = parts[0];
        let args_raw = parts.get(1).copied().unwrap_or("");
        let args: Vec<&str> = if args_raw.is_empty() {
            Vec::new()
        } else {
            args_raw.split_whitespace().collect()
        };

        // Built-in fast-path commands (UI-only, never reach the registry).
        match command {
            "" => {
                self.open_help_dialog("");
                return;
            }
            "help" | "?" => {
                self.open_help_dialog(args_raw);
                return;
            }
            "quit" | "exit" => {
                self.should_quit = true;
                return;
            }
            "clear" => {
                self.active_modal = ActiveModal::ConfirmClear;
                return;
            }
            "status" | "info" => {
                self.active_modal = ActiveModal::StatusDialog;
                return;
            }
            "model" => {
                let requested_model = if matches!(args.first().copied(), Some("set" | "use")) {
                    args_raw
                        .split_whitespace()
                        .skip(1)
                        .collect::<Vec<_>>()
                        .join(" ")
                } else {
                    args_raw.trim().to_string()
                };
                let requested_model = requested_model.trim();
                let requested_action = requested_model.to_ascii_lowercase();
                if requested_model.is_empty()
                    || matches!(requested_action.as_str(), "list" | "options")
                {
                    self.active_modal = ActiveModal::ModelPicker(self.build_model_picker_state());
                } else if matches!(requested_action.as_str(), "status" | "current" | "show") {
                    self.active_modal = ActiveModal::CommandOutput {
                        title: "Model".to_string(),
                        body: self.model_status_body(),
                        is_error: false,
                    };
                } else if matches!(
                    requested_action.as_str(),
                    "reset" | "default" | "clear" | "none"
                ) {
                    self.reset_model_choice();
                } else {
                    self.apply_model_picker_choice(requested_model);
                }
                return;
            }
            "add-dir" => {
                self.handle_add_dir_command(args_raw);
                return;
            }
            "copy" => {
                self.handle_copy_command(args_raw);
                return;
            }
            "export" => {
                self.handle_export_command(args_raw);
                return;
            }
            "context" | "ctx" => {
                self.handle_context_command();
                return;
            }
            "rewind" | "undo" | "checkpoint" => {
                let mut svc = std::mem::take(&mut self.services);
                crate::app_services::open_message_selector(self, &mut svc, true);
                self.services = svc;
                return;
            }
            "statusline" | "status-line" => {
                match args.first().copied() {
                    Some("path" | "default-path") => {
                        self.active_modal = ActiveModal::CommandOutput {
                            title: "Status Line".to_string(),
                            body: format!(
                                "Status line config path\npath: {}",
                                self.default_footer_render_config_path().display()
                            ),
                            is_error: false,
                        };
                    }
                    Some("load" | "reload") => {
                        match self.load_footer_render_config_from_default_path() {
                            Ok(Some(path)) => {
                                self.active_modal = ActiveModal::CommandOutput {
                                    title: "Status Line".to_string(),
                                    body: format!(
                                        "Loaded status line config\npath: {}",
                                        path.display()
                                    ),
                                    is_error: false,
                                };
                            }
                            Ok(None) => {
                                self.active_modal = ActiveModal::CommandOutput {
                                    title: "Status Line".to_string(),
                                    body: format!(
                                        "No status line config found\npath: {}",
                                        self.default_footer_render_config_path().display()
                                    ),
                                    is_error: false,
                                };
                            }
                            Err(error) => {
                                self.active_modal = ActiveModal::CommandOutput {
                                    title: "Status Line".to_string(),
                                    body: format!(
                                    "Failed to load status line config\npath: {}\nerror: {error}",
                                    self.default_footer_render_config_path().display()
                                ),
                                    is_error: true,
                                };
                            }
                        }
                    }
                    Some("save") => match self.save_footer_render_config_to_default_path() {
                        Ok(path) => {
                            self.active_modal = ActiveModal::CommandOutput {
                                title: "Status Line".to_string(),
                                body: format!("Saved status line config\npath: {}", path.display()),
                                is_error: false,
                            };
                        }
                        Err(error) => {
                            self.active_modal = ActiveModal::CommandOutput {
                                title: "Status Line".to_string(),
                                body: format!(
                                    "Failed to save status line config\npath: {}\nerror: {error}",
                                    self.default_footer_render_config_path().display()
                                ),
                                is_error: true,
                            };
                        }
                    },
                    Some("command" | "external-command" | "hook") => {
                        let command_text = statusline_subcommand_tail(args_raw);
                        if command_text.is_empty() {
                            self.active_modal = ActiveModal::CommandOutput {
                                title: "Status Line".to_string(),
                                body: "Usage: /statusline command <shell command>".to_string(),
                                is_error: true,
                            };
                        } else {
                            let mut config = self.state.footer_config.clone();
                            config.external_command =
                                Some(ExternalStatusLineCommandConfig::new(command_text));
                            config.set_enabled(FooterItem::ExternalStatus, true);
                            self.set_footer_render_config_and_persist(config);
                            self.active_modal = ActiveModal::CommandOutput {
                                title: "Status Line".to_string(),
                                body: "Configured external status line command".to_string(),
                                is_error: false,
                            };
                        }
                    }
                    Some("clear-command" | "disable-command" | "clear-external") => {
                        let mut config = self.state.footer_config.clone();
                        config.external_command = None;
                        config.set_enabled(FooterItem::ExternalStatus, false);
                        self.set_footer_render_config_and_persist(config);
                        self.active_modal = ActiveModal::CommandOutput {
                            title: "Status Line".to_string(),
                            body: "Cleared external status line command".to_string(),
                            is_error: false,
                        };
                    }
                    Some(arg) if footer_preset_from_arg(arg).is_some() => {
                        let preset = footer_preset_from_arg(arg).expect("preset checked above");
                        let mut config = self.state.footer_config.clone();
                        config.apply_preset(preset);
                        self.set_footer_render_config_and_persist(config);
                    }
                    Some(arg) => {
                        self.active_modal = ActiveModal::CommandOutput {
                            title: "Status Line".to_string(),
                            body: format!("Unknown status line preset: {arg}"),
                            is_error: true,
                        };
                    }
                    None => {
                        self.active_modal =
                            ActiveModal::StatusLineConfig(StatusLineConfigState::new());
                    }
                }
                return;
            }
            "title" | "session-title" | "rename" => {
                let mut state = self.build_title_config_state();
                let title_arg = args_raw.trim();
                if title_arg.eq_ignore_ascii_case("reset")
                    || title_arg.eq_ignore_ascii_case("clear")
                    || title_arg.eq_ignore_ascii_case("default")
                {
                    self.services.clear_manual_title();
                    state = self.build_title_config_state().notice("reset to default");
                } else if !title_arg.is_empty() {
                    let saved = self.services.set_manual_title(title_arg);
                    state = TitleConfigState::new(saved.unwrap_or_default()).notice("saved");
                }
                self.active_modal = ActiveModal::TitleConfig(state);
                return;
            }
            "raw" | "debug-raw" => {
                self.active_modal = ActiveModal::RawTranscript(self.build_raw_transcript_state());
                return;
            }
            "render-snapshot" | "snapshot" | "render-session" => {
                self.handle_render_snapshot_command(args_raw);
                return;
            }
            "debug-config" | "debugconfig" => {
                self.active_modal = ActiveModal::DebugConfig(self.build_debug_config_state());
                return;
            }
            "diff" => {
                if let Some(state) = self.build_diff_review_state() {
                    self.active_modal = ActiveModal::DiffReview(state);
                } else {
                    self.active_modal = ActiveModal::CommandOutput {
                        title: "Diff Review".to_string(),
                        body: "No semantic diff is available in the current transcript."
                            .to_string(),
                        is_error: false,
                    };
                }
                return;
            }
            "files" | "changes" | "changed-files" => {
                self.active_modal = ActiveModal::FileChanges(self.build_file_changes_state());
                return;
            }
            "timeline" | "events" | "render-events" => {
                self.active_modal = ActiveModal::RenderTimeline(self.build_render_timeline_state());
                return;
            }
            "ps" | "processes" => {
                self.active_modal = ActiveModal::ProcessList(self.build_process_list_state());
                return;
            }
            "commands" | "cmds" | "logs" => {
                self.active_modal = ActiveModal::CommandHistory(self.build_command_history_state());
                return;
            }
            "errors" | "errs" | "failures" => {
                self.active_modal = ActiveModal::ErrorHistory(self.build_error_history_state());
                return;
            }
            "results" | "summaries" | "final-summary" => {
                self.active_modal =
                    ActiveModal::FinalSummaryHistory(self.build_final_summary_history_state());
                return;
            }
            "approvals" | "approval-history" | "approval-log" => {
                self.active_modal =
                    ActiveModal::ApprovalHistory(self.build_approval_history_state());
                return;
            }
            "tasks" | "workitems" => {
                self.active_modal = ActiveModal::TasksDialog;
                return;
            }
            "resume" | "continue" => {
                self.handle_resume_command(args_raw);
                return;
            }
            "mcp" => {
                self.refresh_mcp_statuses();
                self.active_modal = ActiveModal::McpServersDialog;
                return;
            }
            "compact" | "condense" => {
                self.handle_compact_command(&args);
                return;
            }
            "permissions" if self.try_handle_permissions_mode_command(&args, args_raw) => {
                return;
            }
            "permission-mode" | "approval-mode" => {
                self.handle_permission_mode_command(command, args_raw);
                return;
            }
            "theme" => {
                self.handle_theme_command(&args);
                return;
            }
            "fast" | "turbo" => {
                self.handle_fast_command(&args);
                return;
            }
            "effort" => {
                self.handle_effort_command(&args);
                return;
            }
            "proactive" => {
                self.handle_proactive_command(&args);
                return;
            }
            "output-style" | "output_style" => {
                self.handle_output_style_command(&args);
                return;
            }
            _ => {}
        }

        if self.open_widget_command(command, args_raw) {
            return;
        }

        if self.try_handle_skill_command(command, args_raw) {
            return;
        }

        // Try the directive registry.
        if let Some(reg) = self.directives.clone() {
            if let Some(directive) = find_directive(reg.as_slice(), command) {
                let name = directive.name().to_string();
                let mut ctx = self.command_context.clone();
                ctx.cost_snapshot = self.command_cost_snapshot();
                if !directive.is_enabled(&ctx) || directive.is_hidden() {
                    self.push_command_output(
                        &name,
                        format!("/{name} is not available in this session."),
                        true,
                    );
                    return;
                }
                if name == "permissions"
                    && args
                        .first()
                        .map(|arg| permission_rule_subcommand(arg))
                        .unwrap_or(false)
                {
                    self.apply_permission_rule_command_side_effect(&args);
                }
                let dtype = directive.directive_type();
                let result =
                    block_on_current_runtime(async { directive.execute(&args, &ctx).await });

                if matches!(dtype, mossen_commands::DirectiveType::Prompt) {
                    match result {
                        Ok(CommandResult::Text(prompt)) | Ok(CommandResult::System(prompt)) => {
                            if prompt.trim().is_empty() {
                                self.push_command_output(
                                    &name,
                                    format!("/{name} produced an empty prompt"),
                                    true,
                                );
                            } else {
                                self.submit_prompt_directive(command, args_raw, prompt);
                            }
                        }
                        Ok(CommandResult::Error(e)) => {
                            self.push_command_output(&name, e, true);
                        }
                        Ok(CommandResult::Empty) => {
                            self.push_command_output(
                                &name,
                                format!("/{name} produced no prompt"),
                                true,
                            );
                        }
                        Ok(CommandResult::Widget) => {
                            self.push_command_output(
                                &name,
                                format!("/{name} produced a widget instead of a model prompt"),
                                true,
                            );
                        }
                        Ok(CommandResult::Exit(text)) => {
                            self.should_quit = true;
                            self.push_command_output(
                                &name,
                                text.unwrap_or_else(|| "Exiting...".to_string()),
                                false,
                            );
                        }
                        Err(e) => {
                            self.push_command_output(&name, format!("/{name} failed: {e}"), true);
                        }
                    }
                    return;
                }

                let mut opened_widget = false;
                let (mut msg, mut is_error) = match result {
                    Ok(CommandResult::Text(t)) => (t, false),
                    Ok(CommandResult::System(t)) => (t, false),
                    Ok(CommandResult::Empty) => {
                        if matches!(dtype, mossen_commands::DirectiveType::LocalWidget)
                            && self.open_widget_command(&name, args_raw)
                        {
                            opened_widget = true;
                            (String::new(), false)
                        } else {
                            (format!("/{} executed", name), false)
                        }
                    }
                    Ok(CommandResult::Widget) => {
                        if self.open_widget_command(&name, args_raw) {
                            opened_widget = true;
                            (String::new(), false)
                        } else {
                            (
                                format!(
                                    "/{} finished, but no dedicated TUI panel is registered yet.",
                                    name
                                ),
                                false,
                            )
                        }
                    }
                    Ok(CommandResult::Exit(text)) => {
                        self.should_quit = true;
                        (text.unwrap_or_else(|| "Exiting...".to_string()), false)
                    }
                    Ok(CommandResult::Error(e)) => (e, true),
                    Err(e) => (format!("/{} failed: {}", name, e), true),
                };

                if opened_widget {
                    return;
                }

                if name == "compact" || command == "compact" || command == "condense" {
                    self.state.compact_in_progress = false;
                    self.state.compact_progress = Some(msg.clone());
                    self.state.compact_notice_until =
                        Some(std::time::Instant::now() + std::time::Duration::from_secs(6));
                }
                if name == "reload-plugins" {
                    self.refresh_slash_catalog_with_notice(true);
                    if let Some(result) = self.reload_mcp_runtime_after_plugin_reload() {
                        match result {
                            Ok(outcome) => {
                                msg.push_str(&format!(
                                    "\nMCP runtime: {}/{} server(s), {} tool(s) visible next turn.",
                                    outcome.connected_count,
                                    outcome.server_count,
                                    outcome.tool_definitions.len()
                                ));
                            }
                            Err(error) => {
                                is_error = true;
                                msg.push_str(&format!("\nMCP runtime reload failed: {error}"));
                            }
                        }
                    }
                }

                self.push_command_output(&name, msg, is_error);
                return;
            }
        }

        // Built-in fallback help (when registry missing or command unknown).
        if command == "help" {
            self.open_help_dialog(args_raw);
            return;
        }

        self.messages
            .push(unknown_command_transcript_message(command));
        self.note_transcript_changed();
    }

    fn try_handle_skill_command(&mut self, command: &str, args_raw: &str) -> bool {
        let mut skills: Vec<mossen_skills::CraftCommand> = Vec::new();
        if let Some(registry) = &self.skill_registry {
            if let Ok(reg) = registry.read() {
                skills.extend(reg.all_crafts().into_iter().cloned());
            }
        }
        skills.extend(mossen_skills::get_dynamic_skills());
        skills.extend(mossen_skills::get_bundled_crafts());

        let Some(craft) = skills.into_iter().find(|craft| {
            craft.is_user_invocable()
                && (craft.name() == command
                    || craft
                        .base
                        .aliases
                        .as_ref()
                        .is_some_and(|aliases| aliases.iter().any(|a| a == command)))
        }) else {
            return false;
        };

        let context = mossen_skills::CraftExecutionContext {
            session_id: self
                .engine_session_id
                .clone()
                .unwrap_or_else(|| "current-session".to_string()),
            cwd: self.engine_config.cwd.clone(),
            platform: std::env::consts::OS.to_string(),
        };
        let blocks =
            block_on_current_runtime(mossen_skills::execute_craft(&craft, args_raw, &context));
        let mut prompt = String::new();
        for block in blocks {
            match block {
                mossen_skills::ContentBlock::Text { text } => {
                    if !prompt.is_empty() {
                        prompt.push_str("\n\n");
                    }
                    prompt.push_str(&text);
                }
                mossen_skills::ContentBlock::Image { source } => {
                    if !prompt.is_empty() {
                        prompt.push('\n');
                    }
                    prompt.push_str(&format!("[skill image: {}]", source));
                }
            }
        }
        if prompt.trim().is_empty() {
            prompt = craft.markdown_content.clone().unwrap_or_else(|| {
                format!(
                    "Run skill /{} with arguments: {}",
                    craft.name(),
                    args_raw.trim()
                )
            });
        }

        let preview = truncate_plain(prompt.trim(), 220);
        let source = format!("{:?}", craft.loaded_from).to_lowercase();
        self.messages.push(skill_invocation_transcript_message(
            craft.name(),
            &source,
            &preview,
        ));
        self.note_transcript_changed();
        let model_prompt =
            mossen_skills::format_invoked_skill_prompt(craft.name(), args_raw, &prompt);
        self.submit_prompt_to_engine(model_prompt, Vec::new());
        true
    }

    /// Update suggestions based on current input.
    fn update_suggestions(&mut self) {
        let before_cursor: String = self
            .prompt
            .input
            .value
            .chars()
            .take(self.prompt.input.cursor)
            .collect();
        let slash_query = before_cursor
            .strip_prefix('/')
            .filter(|rest| !rest.chars().any(char::is_whitespace));

        if let Some(query) = slash_query {
            let filtered = self.filtered_slash_suggestions(query);
            self.prompt.show_suggestions = !filtered.is_empty();
            self.prompt.suggestions = filtered;
            if self.prompt.show_suggestions {
                let selected = self.prompt.selected_suggestion.unwrap_or(0);
                self.prompt.selected_suggestion =
                    Some(selected.min(self.prompt.suggestions.len().saturating_sub(1)));
            } else {
                self.prompt.selected_suggestion = None;
            }
        } else {
            self.prompt.show_suggestions = false;
            self.prompt.suggestions.clear();
            self.prompt.selected_suggestion = None;
        }
    }

    fn prompt_input_is_exact_slash_command(&self) -> bool {
        let input = self.prompt.input.value.trim();
        let Some(command) = input.strip_prefix('/') else {
            return false;
        };
        if command.is_empty() || command.chars().any(char::is_whitespace) {
            return false;
        }

        self.state.all_slash_commands.iter().any(|entry| {
            entry.name.eq_ignore_ascii_case(command)
                || entry
                    .aliases
                    .iter()
                    .any(|alias| alias.eq_ignore_ascii_case(command))
        })
    }

    fn filtered_slash_suggestions(&self, query: &str) -> Vec<Suggestion> {
        let q = query.trim_start_matches('/').to_lowercase();
        let mut scored: Vec<(i32, Suggestion)> = self
            .state
            .all_slash_commands
            .iter()
            .filter_map(|entry| {
                slash_entry_match_score(entry, &q).map(|score| {
                    (
                        score,
                        Suggestion {
                            label: entry.name.clone(),
                            description: Some(slash_catalog_description(entry)),
                            kind: match entry.kind {
                                SlashCommandKind::Command => SuggestionKind::Command,
                                SlashCommandKind::Skill => SuggestionKind::Skill,
                            },
                        },
                    )
                })
            })
            .collect();
        scored.sort_by(|(a_score, a), (b_score, b)| {
            a_score.cmp(b_score).then_with(|| a.label.cmp(&b.label))
        });
        scored.into_iter().map(|(_, s)| s).collect()
    }

    /// Sticky prompt header height — 1 row when there is at least one user
    /// message to anchor to, 0 otherwise. Mirrors TS `StickyPromptHeader`
    /// which fixes its height at 1 row (truncate-end) to keep the scroll
    /// region stable across header text changes.
    fn sticky_prompt_header_height(&self) -> u16 {
        let has_user_message = self
            .messages
            .iter()
            .any(|m| matches!(m.message_type, MessageType::User));
        if has_user_message {
            1
        } else {
            0
        }
    }

    /// Forward mouse events from the ratatui pipeline. Scroll wheel events are
    /// routed to the active modal first so the visible surface owns the input.
    /// Transcript scrollbar click/drag uses the last rendered rail area.
    fn handle_mouse(&mut self, mouse: MouseEvent) -> bool {
        match mouse.kind {
            MouseEventKind::ScrollUp => self.handle_mouse_wheel_scroll(false),
            MouseEventKind::ScrollDown => self.handle_mouse_wheel_scroll(true),
            MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Drag(MouseButton::Left) => {
                self.handle_modal_scrollbar_pointer(mouse)
                    || self.handle_transcript_scrollbar_pointer(mouse)
            }
            _ => false,
        }
    }

    fn handle_mouse_wheel_scroll(&mut self, down: bool) -> bool {
        if matches!(self.active_modal, ActiveModal::IdleReturn(_)) {
            self.services.idle_return_state = None;
            self.active_modal = ActiveModal::None;
        }

        if self.prompt.show_suggestions {
            if down {
                return self.prompt.suggestion_page_down(MOUSE_WHEEL_SCROLL_ROWS);
            }
            return self.prompt.suggestion_page_up(MOUSE_WHEEL_SCROLL_ROWS);
        }

        let before = self.mouse_scroll_fingerprint();
        if self.handle_active_modal_mouse_scroll(down, MOUSE_WHEEL_SCROLL_ROWS) {
            return self.mouse_scroll_fingerprint() != before;
        }

        let before = (self.scroll.offset, self.scroll.sticky);
        let before_total = self.scroll.total_items;
        let before_visible = self.scroll.visible_count;
        self.refresh_transcript_scroll_metrics_for_input();
        let refreshed_total = self.scroll.total_items;
        let refreshed_visible = self.scroll.visible_count;
        if down {
            self.scroll.scroll_down(MOUSE_WHEEL_SCROLL_ROWS);
        } else {
            self.scroll.scroll_up(MOUSE_WHEEL_SCROLL_ROWS);
        }
        append_tui_scroll_log_line(format!(
            "transcript_wheel down={down} before_offset={} before_sticky={} before_total={} before_visible={} refreshed_total={} refreshed_visible={} after_offset={} after_sticky={} active_modal_key={} messages={} streaming={}",
            before.0,
            before.1,
            before_total,
            before_visible,
            refreshed_total,
            refreshed_visible,
            self.scroll.offset,
            self.scroll.sticky,
            active_modal_shape_key(&self.active_modal),
            self.messages.len(),
            self.state.is_streaming
        ));
        before != (self.scroll.offset, self.scroll.sticky)
    }

    fn handle_modal_scrollbar_pointer(&mut self, mouse: MouseEvent) -> bool {
        let Some(target) = self.modal_scrollbar_target.get() else {
            return false;
        };
        if target.area.width == 0
            || target.area.height == 0
            || target.viewport_rows == 0
            || target.total_rows <= target.viewport_rows
        {
            return false;
        }
        if mouse.column < target.area.x
            || mouse.column >= target.area.x.saturating_add(target.area.width)
            || mouse.row < target.area.y
            || mouse.row >= target.area.y.saturating_add(target.area.height)
        {
            return false;
        }

        let before = self.mouse_scroll_fingerprint();
        let max_offset = target.total_rows.saturating_sub(target.viewport_rows);
        let target_offset = scrollbar_pointer_target_offset(mouse.row, target.area, max_offset);
        match (&mut self.active_modal, target.kind) {
            (ActiveModal::HelpDialog(state), ModalScrollbarKind::Help) => {
                state.scroll = target_offset;
            }
            (ActiveModal::CommandOutput { .. }, ModalScrollbarKind::CommandOutput) => {
                self.command_output_scroll = target_offset;
            }
            (ActiveModal::RawTranscript(state), ModalScrollbarKind::RawTranscript) => {
                state.scroll = target_offset;
            }
            (ActiveModal::DiffReview(state), ModalScrollbarKind::DiffReview) => {
                state.scroll = target_offset;
            }
            (ActiveModal::FileChanges(state), ModalScrollbarKind::FileChanges) => {
                set_selectable_scroll_from_pointer(
                    &mut state.scroll,
                    &mut state.selected,
                    state.model.len(),
                    target.viewport_rows,
                    target_offset,
                );
            }
            (ActiveModal::RenderTimeline(state), ModalScrollbarKind::RenderTimeline) => {
                set_selectable_scroll_from_pointer(
                    &mut state.scroll,
                    &mut state.selected,
                    state.model.len(),
                    target.viewport_rows,
                    target_offset,
                );
            }
            (ActiveModal::ProcessList(state), ModalScrollbarKind::ProcessList) => {
                set_selectable_scroll_from_pointer(
                    &mut state.scroll,
                    &mut state.selected,
                    state.model.len(),
                    target.viewport_rows,
                    target_offset,
                );
            }
            (ActiveModal::CommandHistory(state), ModalScrollbarKind::CommandHistory) => {
                set_selectable_scroll_from_pointer(
                    &mut state.scroll,
                    &mut state.selected,
                    state.model.len(),
                    target.viewport_rows,
                    target_offset,
                );
                state.detail_scroll = 0;
            }
            (ActiveModal::ErrorHistory(state), ModalScrollbarKind::ErrorHistory) => {
                set_selectable_scroll_from_pointer(
                    &mut state.scroll,
                    &mut state.selected,
                    state.model.len(),
                    target.viewport_rows,
                    target_offset,
                );
                state.detail_scroll = 0;
            }
            (ActiveModal::FinalSummaryHistory(state), ModalScrollbarKind::FinalSummaryHistory) => {
                set_selectable_scroll_from_pointer(
                    &mut state.scroll,
                    &mut state.selected,
                    state.model.len(),
                    target.viewport_rows,
                    target_offset,
                );
                state.detail_scroll = 0;
            }
            (ActiveModal::ApprovalHistory(state), ModalScrollbarKind::ApprovalHistory) => {
                set_selectable_scroll_from_pointer(
                    &mut state.scroll,
                    &mut state.selected,
                    state.model.len(),
                    target.viewport_rows,
                    target_offset,
                );
                state.detail_scroll = 0;
            }
            (ActiveModal::DebugConfig(state), ModalScrollbarKind::DebugConfig) => {
                state.scroll = target_offset;
            }
            _ => return false,
        }
        before != self.mouse_scroll_fingerprint()
    }

    fn handle_transcript_scrollbar_pointer(&mut self, mouse: MouseEvent) -> bool {
        if !matches!(self.active_modal, ActiveModal::None) {
            return false;
        }
        let Some(area) = self.message_scrollbar_area else {
            return false;
        };
        if area.width == 0
            || area.height == 0
            || self.scroll.visible_count == 0
            || self.scroll.total_items <= self.scroll.visible_count
        {
            return false;
        }
        if mouse.column < area.x
            || mouse.column >= area.x.saturating_add(area.width)
            || mouse.row < area.y
            || mouse.row >= area.y.saturating_add(area.height)
        {
            return false;
        }

        let before = (self.scroll.offset, self.scroll.sticky);
        let max_offset = self
            .scroll
            .total_items
            .saturating_sub(self.scroll.visible_count);
        if max_offset == 0 {
            return false;
        }

        let target = scrollbar_pointer_target_offset(mouse.row, area, max_offset);
        self.scroll.offset = target;
        self.scroll.sticky = target >= max_offset;
        before != (self.scroll.offset, self.scroll.sticky)
    }

    fn mouse_scroll_fingerprint(&self) -> MouseScrollFingerprint {
        match &self.active_modal {
            ActiveModal::None => MouseScrollFingerprint::Transcript {
                offset: self.scroll.offset,
                sticky: self.scroll.sticky,
            },
            ActiveModal::HelpDialog(state) => MouseScrollFingerprint::Help {
                scroll: state.scroll,
            },
            ActiveModal::CommandOutput { .. } => MouseScrollFingerprint::CommandOutput {
                scroll: self.command_output_scroll,
            },
            ActiveModal::RawTranscript(state) => MouseScrollFingerprint::LineScroll {
                scroll: state.scroll,
            },
            ActiveModal::DiffReview(state) => MouseScrollFingerprint::LineScroll {
                scroll: state.scroll,
            },
            ActiveModal::FileChanges(state) => MouseScrollFingerprint::Selectable {
                selected: state.selected,
                scroll: state.scroll,
            },
            ActiveModal::RenderTimeline(state) => MouseScrollFingerprint::Selectable {
                selected: state.selected,
                scroll: state.scroll,
            },
            ActiveModal::ProcessList(state) => MouseScrollFingerprint::Selectable {
                selected: state.selected,
                scroll: state.scroll,
            },
            ActiveModal::CommandHistory(state) => MouseScrollFingerprint::DetailSelectable {
                selected: state.selected,
                scroll: state.scroll,
                detail_scroll: state.detail_scroll,
            },
            ActiveModal::ErrorHistory(state) => MouseScrollFingerprint::DetailSelectable {
                selected: state.selected,
                scroll: state.scroll,
                detail_scroll: state.detail_scroll,
            },
            ActiveModal::FinalSummaryHistory(state) => MouseScrollFingerprint::DetailSelectable {
                selected: state.selected,
                scroll: state.scroll,
                detail_scroll: state.detail_scroll,
            },
            ActiveModal::ApprovalHistory(state) => MouseScrollFingerprint::DetailSelectable {
                selected: state.selected,
                scroll: state.scroll,
                detail_scroll: state.detail_scroll,
            },
            ActiveModal::DebugConfig(state) => MouseScrollFingerprint::LineScroll {
                scroll: state.scroll,
            },
            ActiveModal::ModelPicker(state) => MouseScrollFingerprint::Picker {
                selected: state.selected,
            },
            ActiveModal::SkillsPanel(state) => MouseScrollFingerprint::Picker {
                selected: state.selected,
            },
            ActiveModal::MemoryPanel(state) => MouseScrollFingerprint::Picker {
                selected: state.selected,
            },
            ActiveModal::Picker { selected, .. } => MouseScrollFingerprint::Picker {
                selected: *selected,
            },
            _ => MouseScrollFingerprint::StaticModal,
        }
    }

    fn handle_active_modal_mouse_scroll(&mut self, down: bool, rows: usize) -> bool {
        let rows = rows.max(1);
        let help_context = match &self.active_modal {
            ActiveModal::HelpDialog(state) => Some((
                self.help_dialog_content_line_count(&state.query),
                help_dialog_content_viewport_rows(self.state.terminal_height),
            )),
            _ => None,
        };
        let command_output_context = match &self.active_modal {
            ActiveModal::CommandOutput { body, .. } => Some((
                command_output_content_line_count(body),
                command_output_content_viewport_rows(self.state.terminal_height),
            )),
            _ => None,
        };

        match &mut self.active_modal {
            ActiveModal::None => false,
            ActiveModal::HelpDialog(state) => {
                let (total_rows, viewport_rows) = help_context.unwrap_or((
                    0,
                    help_dialog_content_viewport_rows(self.state.terminal_height),
                ));
                if down {
                    state.scroll_down(rows, total_rows, viewport_rows);
                } else {
                    state.scroll_up(rows);
                }
                true
            }
            ActiveModal::CommandOutput { .. } => {
                let (total_rows, viewport_rows) = command_output_context.unwrap_or((
                    0,
                    command_output_content_viewport_rows(self.state.terminal_height),
                ));
                if down {
                    self.command_output_scroll = self
                        .command_output_scroll
                        .saturating_add(rows)
                        .min(command_output_scroll_max(total_rows, viewport_rows));
                } else {
                    self.command_output_scroll = self.command_output_scroll.saturating_sub(rows);
                }
                true
            }
            ActiveModal::RawTranscript(state) => {
                let viewport_height =
                    raw_transcript_content_viewport_rows(self.state.terminal_height);
                if down {
                    state.scroll_down(rows, viewport_height);
                } else {
                    state.scroll_up(rows, viewport_height);
                }
                true
            }
            ActiveModal::DiffReview(state) => {
                let viewport_height = diff_review_content_viewport_rows(self.state.terminal_height);
                if down {
                    state.scroll_down(rows, viewport_height);
                } else {
                    state.scroll_up(rows, viewport_height);
                }
                true
            }
            ActiveModal::FileChanges(state) => {
                let viewport_rows = file_changes_content_viewport_rows(self.state.terminal_height);
                scroll_selectable_modal_by_wheel(rows, down, |down| {
                    if down {
                        state.move_down(viewport_rows);
                    } else {
                        state.move_up();
                    }
                });
                true
            }
            ActiveModal::RenderTimeline(state) => {
                let viewport_rows =
                    render_timeline_content_viewport_rows(self.state.terminal_height);
                scroll_selectable_modal_by_wheel(rows, down, |down| {
                    if down {
                        state.move_down(viewport_rows);
                    } else {
                        state.move_up();
                    }
                });
                true
            }
            ActiveModal::ProcessList(state) => {
                let viewport_rows = process_list_content_viewport_rows(self.state.terminal_height);
                scroll_selectable_modal_by_wheel(rows, down, |down| {
                    if down {
                        state.move_down(viewport_rows);
                    } else {
                        state.move_up();
                    }
                });
                true
            }
            ActiveModal::CommandHistory(state) => {
                let viewport_rows =
                    command_history_content_viewport_rows(self.state.terminal_height);
                if state.selected_expanded() {
                    if down {
                        state.detail_page_down(rows);
                    } else {
                        state.detail_page_up(rows);
                    }
                } else {
                    scroll_selectable_modal_by_wheel(rows, down, |down| {
                        if down {
                            state.move_down(viewport_rows);
                        } else {
                            state.move_up();
                        }
                    });
                }
                true
            }
            ActiveModal::ErrorHistory(state) => {
                let viewport_rows = error_history_content_viewport_rows(self.state.terminal_height);
                if state.selected_expanded() {
                    if down {
                        state.detail_page_down(rows);
                    } else {
                        state.detail_page_up(rows);
                    }
                } else {
                    scroll_selectable_modal_by_wheel(rows, down, |down| {
                        if down {
                            state.move_down(viewport_rows);
                        } else {
                            state.move_up();
                        }
                    });
                }
                true
            }
            ActiveModal::FinalSummaryHistory(state) => {
                let viewport_rows =
                    final_summary_history_content_viewport_rows(self.state.terminal_height);
                if state.selected_expanded() {
                    if down {
                        state.detail_page_down(rows);
                    } else {
                        state.detail_page_up(rows);
                    }
                } else {
                    scroll_selectable_modal_by_wheel(rows, down, |down| {
                        if down {
                            state.move_down(viewport_rows);
                        } else {
                            state.move_up();
                        }
                    });
                }
                true
            }
            ActiveModal::ApprovalHistory(state) => {
                let viewport_rows =
                    approval_history_content_viewport_rows(self.state.terminal_height);
                if state.selected_expanded() {
                    if down {
                        state.detail_page_down(rows);
                    } else {
                        state.detail_page_up(rows);
                    }
                } else {
                    scroll_selectable_modal_by_wheel(rows, down, |down| {
                        if down {
                            state.move_down(viewport_rows);
                        } else {
                            state.move_up();
                        }
                    });
                }
                true
            }
            ActiveModal::DebugConfig(state) => {
                let viewport_rows = debug_config_content_viewport_rows(self.state.terminal_height);
                if down {
                    state.scroll_down(rows, viewport_rows);
                } else {
                    state.scroll_up(rows);
                }
                true
            }
            ActiveModal::ModelPicker(state) => {
                scroll_selectable_modal_by_wheel(rows, down, |down| {
                    if down {
                        state.move_down();
                    } else {
                        state.move_up();
                    }
                });
                true
            }
            ActiveModal::SkillsPanel(state) => {
                if down {
                    state.selected = state
                        .selected
                        .saturating_add(rows)
                        .min(state.skills.len().saturating_sub(1));
                } else {
                    state.selected = state.selected.saturating_sub(rows);
                }
                true
            }
            ActiveModal::MemoryPanel(state) => {
                if down {
                    state.selected = state
                        .selected
                        .saturating_add(rows)
                        .min(state.entries.len().saturating_sub(1));
                } else {
                    state.selected = state.selected.saturating_sub(rows);
                }
                true
            }
            ActiveModal::Picker {
                items, selected, ..
            } => {
                if down {
                    *selected = (*selected)
                        .saturating_add(rows)
                        .min(items.len().saturating_sub(1));
                } else {
                    *selected = (*selected).saturating_sub(rows);
                }
                true
            }
            _ => true,
        }
    }

    /// Add an assistant message (called when streaming completes).
    pub fn add_assistant_message(&mut self, content: String) {
        self.messages
            .push(assistant_transcript_message(content, None, false));
        self.note_transcript_changed();
        self.state.is_streaming = false;
        self.state.is_waiting_for_response = false;
    }

    // -----------------------------------------------------------------
    // Engine message handling
    // -----------------------------------------------------------------

    /// Handle a single `SdkMessage` produced by the engine. Routes streaming
    /// deltas into the pending assistant message, surfaces tool-use blocks
    /// as their own message rows, and finalizes the turn on `Result`.
    /// Pull a single pending `PermissionRequest` off the gate channel and
    /// open a `ToolUseConfirm` modal for it. Called from the tick handler
    /// once per frame. The gate guarantees only one request is in flight at
    /// a time (the engine awaits the oneshot reply before issuing another).
    fn poll_permission_request(&mut self) {
        if self.active_permission_responder.is_some() {
            return;
        }
        if self.active_modal.is_inline_approval() || !self.active_modal.can_yield_to_approval() {
            return;
        }
        let Some(rx) = self.permission_rx.as_mut() else {
            return;
        };
        let request = match rx.try_recv() {
            Ok(req) => req,
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => return,
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                self.permission_rx = None;
                return;
            }
        };

        if self.active_modal.is_open() {
            if matches!(self.active_modal, ActiveModal::IdleReturn(_)) {
                self.services.idle_return_state = None;
            }
            self.active_modal = ActiveModal::None;
        }

        let mossen_agent::types::PermissionRequest {
            tool_id,
            tool_name,
            input,
            responder,
        } = request;

        let input_summary = tool_input_summary_from_value(&input);

        let confirm = ToolUseConfirm {
            tool_use_id: tool_id,
            tool_name: tool_name.clone(),
            raw_input: input,
            input_summary,
            risk_level: 1, // medium until we wire per-tool risk classification
        };
        let mut prompt = PermissionPromptState::new(
            permission_kind_for_tool(&tool_name, &confirm.raw_input),
            tool_name,
        );
        let detail = tool_call_preview_from_input(&prompt.tool_name, &confirm.raw_input);
        if !detail.is_empty() {
            prompt.explanation = Some(detail);
            prompt.show_details = true;
        }
        let render_event = RenderEvent::approval_requested(prompt.tool_name.clone());
        self.active_modal = ActiveModal::ToolUseConfirm { confirm, prompt };
        self.active_permission_responder = Some(responder);
        self.apply_render_event(&render_event);
    }

    fn poll_mcp_channel_approval(&mut self) {
        if self.active_modal.is_inline_approval() || !self.active_modal.can_yield_to_approval() {
            return;
        }
        if let Some(request) = mossen_agent::mcp::channel_approval::pop_pending() {
            if self.active_modal.is_open() {
                if matches!(self.active_modal, ActiveModal::IdleReturn(_)) {
                    self.services.idle_return_state = None;
                }
                self.active_modal = ActiveModal::None;
            }
            self.active_modal = ActiveModal::McpChannelApproval(request);
            self.apply_render_event(&RenderEvent::approval_requested("MCP Channel"));
        }
    }

    pub fn handle_engine_message(&mut self, msg: SdkMessage) {
        let stream_visible_schedule_before = match &msg {
            SdkMessage::StreamEvent {
                event:
                    StreamEventData::ContentBlockDelta {
                        delta: ContentDelta::TextDelta { .. } | ContentDelta::ThinkingDelta { .. },
                        ..
                    },
                task_id: None,
            } => Some((self.render_dirty, self.render_throttled_dirty_at)),
            _ => None,
        };
        self.prepare_render_turn_for_engine_message(&msg);
        self.record_raw_engine_event(&msg);
        let mut render_events = render_events_for_sdk_message(&msg);
        if let SdkMessage::Result { terminal, .. } = &msg {
            let model = final_summary_model_from_messages(String::new(), terminal, &self.messages);
            if !final_summary_should_record(&model) {
                render_events.retain(|event| {
                    !matches!(event.kind, RenderEventKind::FinalSummaryRecorded { .. })
                });
            }
        }
        self.apply_main_render_events(render_events);
        if self.state.is_streaming {
            self.spinner.mark_activity();
        }

        // Route sub-agent messages to teammate state.
        if let Some(tid) = msg.task_id() {
            let tid = tid.to_string();
            match &msg {
                SdkMessage::SystemInit { model, .. } => {
                    self.state
                        .teammate_states
                        .entry(tid.clone())
                        .or_insert(TeammateState::Running);
                    let transcript_facts = task_started_transcript_facts(&tid, model);
                    let source_index = self.messages.len();
                    self.set_render_record_id_override(source_index, transcript_facts.record_id);
                    self.messages.push(transcript_facts.message);
                    self.note_transcript_changed();
                }
                SdkMessage::Assistant { message, .. } => {
                    self.state
                        .teammate_states
                        .entry(tid.clone())
                        .or_insert(TeammateState::Running);
                    let content_facts = assistant_content_facts(message);
                    let full_text = content_facts.text;
                    if !full_text.trim().is_empty() {
                        let (thinking, content) = split_thinking_and_content(&full_text);
                        let transcript_facts =
                            task_assistant_transcript_facts(&tid, content, thinking);
                        let source_index = self.messages.len();
                        if let Some(parent_id) = transcript_facts.parent_id {
                            self.set_render_record_parent_override(source_index, parent_id);
                        }
                        self.messages.push(transcript_facts.message);
                    }
                    for tool_use in content_facts.tool_uses {
                        let id = tool_use.id;
                        let name = tool_use.name;
                        let input = tool_use.input;
                        self.state.ui_stage = UiStage::from_tool_name(&name);
                        let source_index = self.messages.len();
                        let transcript_facts = tool_use_transcript_facts(
                            Some(&tid),
                            &id,
                            &name,
                            tool_call_preview_from_input(&name, &input),
                            Some(input.to_string()),
                        );
                        self.set_render_record_id_override(
                            source_index,
                            transcript_facts.record_id,
                        );
                        if let Some(parent_id) = transcript_facts.parent_id {
                            self.set_render_record_parent_override(source_index, parent_id);
                        }
                        self.messages.push(transcript_facts.message);
                    }
                    self.note_transcript_changed();
                }
                SdkMessage::StreamEvent { .. } => {
                    self.state
                        .teammate_states
                        .entry(tid.clone())
                        .or_insert(TeammateState::Running);
                }
                SdkMessage::ToolUseSummary {
                    tool_name,
                    summary,
                    full_content,
                    tool_use_id,
                    ..
                } => {
                    self.state
                        .teammate_states
                        .entry(tid.clone())
                        .or_insert(TeammateState::Running);
                    let transcript_facts = tool_summary_transcript_facts(
                        Some(&tid),
                        tool_name,
                        summary,
                        full_content.as_deref(),
                        tool_use_id.as_deref(),
                        self.latest_tool_record_id_for_result(tool_name),
                    );
                    let source_index = self.messages.len();
                    if let Some(record_id) = transcript_facts.record_id.as_deref() {
                        self.set_render_record_id_override(source_index, record_id);
                    }
                    if let Some(parent_id) = transcript_facts.parent_id.as_deref() {
                        self.set_render_record_parent_override(source_index, parent_id);
                    }
                    self.messages.push(transcript_facts.message);
                    self.state.ui_stage = ui_stage_after_tool_summary(tool_name);
                    self.note_transcript_changed();
                }
                SdkMessage::Result { terminal, .. } => {
                    self.state
                        .teammate_states
                        .insert(tid.clone(), TeammateState::Completed(terminal.clone()));
                    let transcript_facts = task_completed_transcript_facts(&tid, terminal);
                    let source_index = self.messages.len();
                    self.set_render_record_id_override(source_index, transcript_facts.record_id);
                    if let Some(parent_id) = transcript_facts.parent_id {
                        self.set_render_record_parent_override(source_index, parent_id);
                    }
                    self.messages.push(transcript_facts.message);
                    self.note_transcript_changed();
                }
                _ => {}
            }
            self.state
                .teammate_messages
                .entry(tid.clone())
                .or_default()
                .push(format!("event from {}", tid));
            match msg {
                SdkMessage::StreamEvent { .. } => {
                    self.mark_render_dirty_for_refresh(Self::streaming_render_refresh_policy());
                }
                _ => self.mark_render_dirty(),
            }
            return;
        }

        match msg {
            SdkMessage::SystemInit { session_id, .. } => {
                self.engine_session_id = Some(session_id);
            }
            SdkMessage::User { message, .. } => {
                // User echo — already appended to the visual transcript in
                // handle_submit. Persist the exact model-facing blocks here
                // so the next turn receives real conversation history.
                self.engine_history.push(Message {
                    role: Role::User,
                    content: message.content,
                    uuid: message.uuid,
                    is_meta: message.is_meta,
                    origin: message.origin,
                    timestamp: Some(chrono::Utc::now().to_rfc3339()),
                    extra: message.extra,
                });
            }
            SdkMessage::Assistant { message, .. } => {
                // Replace any partial buffer with the final, full content.
                // Some backends emit a single Assistant message with all
                // text blocks instead of delta events; others emit deltas
                // followed by an Assistant with the full content. Either
                // way, the *final* Assistant payload is authoritative.
                let content_facts = assistant_content_facts(&message);
                let full_text = content_facts.text;

                if !full_text.is_empty() {
                    // Re-derive the (thinking, content) split from the
                    // authoritative final text — same routine the streaming
                    // path uses, so the placeholder we filled in via deltas
                    // ends up identical to what we'd render from scratch.
                    let (final_thinking, final_content) = split_thinking_and_content(&full_text);
                    if let Some(idx) = self.pending_assistant_idx {
                        if let Some(m) = self.messages.get_mut(idx) {
                            m.thinking = final_thinking;
                            m.content = final_content.clone();
                        }
                    } else {
                        let source_index = self.messages.len();
                        self.set_render_record_current_turn_override(source_index);
                        self.messages.push(assistant_transcript_message(
                            final_content.clone(),
                            final_thinking,
                            false,
                        ));
                        self.note_transcript_changed();
                    }
                    self.record_engine_assistant_text(&final_content);
                    self.assistant_buf = full_text;
                }
                self.finish_pending_assistant_message(None);

                for tool_use in content_facts.tool_uses {
                    let id = tool_use.id;
                    let name = tool_use.name;
                    let input = tool_use.input;
                    self.state.ui_stage = UiStage::from_tool_name(&name);
                    // Stash TodoWrite input so we can update the task list
                    // state when the corresponding ToolUseSummary arrives.
                    if name == "TodoWrite" {
                        self.pending_todo_write_input = Some(input.clone());
                    }
                    if name == "Agent" || name == "Task" {
                        let label = input
                            .get("subagent_type")
                            .or_else(|| input.get("agent_type"))
                            .or_else(|| input.get("description"))
                            .and_then(|v| v.as_str())
                            .unwrap_or(&name);
                        let task_id = format!("{}:{}", name, id);
                        self.state
                            .teammate_states
                            .entry(task_id.clone())
                            .or_insert(TeammateState::Running);
                        self.state
                            .teammate_messages
                            .entry(task_id)
                            .or_default()
                            .push(format!("started {}", label));
                    }

                    // Render a compact, human-readable argument list. For
                    // single-arg tools (Bash → command, Read → file_path)
                    // we show the bare value; for multi-arg tools we show
                    // `key=value, key=value`. Falls back to compact JSON
                    // when the input isn't an object.
                    let transcript_facts = tool_use_transcript_facts(
                        None,
                        &id,
                        &name,
                        tool_call_preview_from_input(&name, &input),
                        Some(input.to_string()),
                    );
                    let source_index = self.messages.len();
                    self.set_render_record_current_turn_override(source_index);
                    self.set_render_record_id_override(source_index, transcript_facts.record_id);
                    if let Some(parent_id) = transcript_facts.parent_id {
                        self.set_render_record_parent_override(source_index, parent_id);
                    }
                    self.messages.push(transcript_facts.message);
                    self.note_transcript_changed();
                }
            }
            SdkMessage::StreamEvent { event, .. } => {
                let transcript_changed = self.handle_stream_event(event);
                if !transcript_changed {
                    if let Some((render_dirty, throttled_dirty_at)) = stream_visible_schedule_before
                    {
                        self.render_dirty = render_dirty;
                        self.render_throttled_dirty_at = throttled_dirty_at;
                    }
                }
            }
            SdkMessage::Result {
                terminal, cost_usd, ..
            } => {
                // Accumulate live cost so the status-bar segment + cost
                // threshold dialog can read real numbers instead of the
                // hard-coded 0.0 placeholder we used before the engine
                // started reporting per-turn cost.
                if let Some(cost) = cost_usd {
                    self.total_cost_usd += cost;
                }
                if self.state.compact_in_progress {
                    self.state.compact_in_progress = false;
                    self.state.compact_progress = Some("Compaction step completed".to_string());
                    self.state.compact_notice_until =
                        Some(std::time::Instant::now() + std::time::Duration::from_secs(6));
                }
                self.finalize_assistant_turn(Some(terminal));
            }
            SdkMessage::ToolUseSummary {
                tool_name,
                summary,
                full_content,
                tool_use_id,
                ..
            } => {
                // If this is a TodoWrite result, parse the stashed input
                // to update the task list state (structured data, not summary text).
                if tool_name == "TodoWrite" {
                    if let Some(input) = self.pending_todo_write_input.take() {
                        if let Ok(parsed) = serde_json::from_value::<TaskNotePadInput>(input) {
                            self.state.task_list.tasks = parsed.todos;
                            self.state.task_list.last_update = Some(std::time::Instant::now());
                        }
                    }
                }

                let transcript_facts = tool_summary_transcript_facts(
                    None,
                    &tool_name,
                    &summary,
                    full_content.as_deref(),
                    tool_use_id.as_deref(),
                    self.latest_tool_record_id_for_result(&tool_name),
                );
                let source_index = self.messages.len();
                self.set_render_record_current_turn_override(source_index);
                if let Some(record_id) = transcript_facts.record_id.as_deref() {
                    self.set_render_record_id_override(source_index, record_id);
                }
                if let Some(parent_id) = transcript_facts.parent_id.as_deref() {
                    self.set_render_record_parent_override(source_index, parent_id);
                }
                let next_stage = ui_stage_after_tool_summary(&tool_name);
                self.messages.push(transcript_facts.message);
                self.state.ui_stage = next_stage;
                self.note_transcript_changed();
            }
            SdkMessage::CompactBoundary {
                before_token_count,
                after_token_count,
                ..
            } => {
                let transcript_facts =
                    compact_boundary_transcript_facts(before_token_count, after_token_count);
                self.state.compact_in_progress = false;
                self.state.compact_progress = Some(transcript_facts.progress);
                self.state.compact_notice_until =
                    Some(std::time::Instant::now() + std::time::Duration::from_secs(6));
                let compact_index = self.messages.len();
                self.messages.push(transcript_facts.message);
                self.set_render_record_current_turn_override(compact_index);
                self.note_transcript_changed();
            }
            SdkMessage::CompactRequestStatus {
                status,
                dry_run,
                before_token_count,
                after_token_count,
                message_count_before,
                message_count_after,
                compacted_message_count,
                reason,
                ..
            } => {
                self.state.compact_in_progress = false;
                let mut parts = vec![format!("request {}", status.as_str())];
                if dry_run {
                    parts.push("dry run".to_string());
                }
                if let (Some(before), Some(after)) = (before_token_count, after_token_count) {
                    parts.push(format!("tokens {before} -> {after}"));
                } else if let Some(before) = before_token_count {
                    parts.push(format!("tokens {before}"));
                }
                if let (Some(before), Some(after)) = (message_count_before, message_count_after) {
                    parts.push(format!("messages {before} -> {after}"));
                }
                if let Some(count) = compacted_message_count {
                    parts.push(format!("compacted {count}"));
                }
                if let Some(reason) = reason.as_deref().filter(|reason| !reason.trim().is_empty()) {
                    parts.push(reason.to_string());
                }
                self.state.compact_progress = Some(parts.join(", "));
                self.state.compact_notice_until =
                    Some(std::time::Instant::now() + std::time::Duration::from_secs(6));
            }
            SdkMessage::ConversationCleared {
                message_count_before,
                message_count_after,
                ..
            } => {
                self.state.compact_in_progress = false;
                self.state.compact_progress = Some(format!(
                    "messages {} -> {}",
                    message_count_before, message_count_after
                ));
                self.state.compact_notice_until =
                    Some(std::time::Instant::now() + std::time::Duration::from_secs(6));
            }
            SdkMessage::ClearRequestStatus {
                status,
                dry_run,
                message_count_before,
                message_count_after,
                reason,
                ..
            } => {
                let mut parts = vec![format!("clear request {}", status.as_str())];
                if dry_run {
                    parts.push("dry run".to_string());
                }
                if let (Some(before), Some(after)) = (message_count_before, message_count_after) {
                    parts.push(format!("messages {before} -> {after}"));
                }
                if let Some(reason) = reason.as_deref().filter(|reason| !reason.trim().is_empty()) {
                    parts.push(reason.to_string());
                }
                self.state.compact_progress = Some(parts.join(", "));
                self.state.compact_notice_until =
                    Some(std::time::Instant::now() + std::time::Duration::from_secs(6));
            }
            SdkMessage::ApiRetry {
                error,
                attempt,
                max_retries,
                retry_in_ms,
                ..
            } => {
                self.state.ui_stage = UiStage::Retrying;
                let source_index = self.messages.len();
                self.set_render_record_current_turn_override(source_index);
                self.messages.push(api_retry_transcript_message(
                    &error,
                    attempt,
                    max_retries,
                    retry_in_ms,
                ));
                self.note_transcript_changed();
            }
        }
    }

    fn prepare_render_turn_for_engine_message(&mut self, msg: &SdkMessage) {
        if msg.task_id().is_some() {
            return;
        }
        match msg {
            SdkMessage::SystemInit { .. }
            | SdkMessage::Assistant { .. }
            | SdkMessage::StreamEvent { .. }
            | SdkMessage::ToolUseSummary { .. }
            | SdkMessage::CompactBoundary { .. }
            | SdkMessage::CompactRequestStatus { .. }
            | SdkMessage::ConversationCleared { .. }
            | SdkMessage::ClearRequestStatus { .. }
            | SdkMessage::ApiRetry { .. }
            | SdkMessage::Result { .. } => {
                self.ensure_current_render_turn_id();
            }
            SdkMessage::User { .. } => {}
        }
    }

    /// Handle a streaming `StreamEventData` from the engine — text deltas
    /// are appended to the pending assistant message.
    fn handle_stream_event(&mut self, event: StreamEventData) -> bool {
        // First stream event → turn is now Streaming.
        if self.state.turn_state == TurnState::Idle {
            self.state.turn_state = TurnState::Streaming;
        }
        match event {
            StreamEventData::MessageStart => {
                self.state.ui_stage = UiStage::Thinking;
                self.begin_pending_assistant_message();
                true
            }
            StreamEventData::ContentBlockDelta { delta, .. } => match delta {
                ContentDelta::TextDelta { text } => {
                    self.state.ui_stage = UiStage::Thinking;
                    if text.is_empty() {
                        return false;
                    }
                    // Accumulate the full streamed text, then re-derive the
                    // `(thinking, content)` split on every chunk. Recomputing
                    // from the whole buffer (rather than tracking incremental
                    // open/close state) makes us robust against `<think>`
                    // / `</think>` tags arriving split across chunk
                    // boundaries, which streaming model providers can do.
                    if self.pending_assistant_idx.is_none() {
                        self.begin_pending_assistant_message();
                    }
                    self.assistant_buf.push_str(&text);
                    let (thinking, content) = split_thinking_and_content(&self.assistant_buf);
                    let mut transcript_updated = false;
                    if let Some(idx) = self.pending_assistant_idx {
                        if let Some(m) = self.messages.get_mut(idx) {
                            if m.thinking != thinking || m.content != content {
                                m.thinking = thinking;
                                m.content = content;
                                transcript_updated = true;
                            }
                        }
                    }
                    if transcript_updated {
                        self.note_transcript_changed_for_refresh(
                            Self::streaming_render_refresh_policy(),
                        );
                    }
                    transcript_updated
                }
                ContentDelta::ThinkingDelta { thinking } => {
                    self.state.ui_stage = UiStage::Thinking;
                    if thinking.is_empty() {
                        return false;
                    }
                    // Some backends emit reasoning via the dedicated
                    // `thinking_delta` channel instead of inline `<think>`
                    // tags. Append to the pending message's thinking buffer.
                    if self.pending_assistant_idx.is_none() {
                        self.begin_pending_assistant_message();
                    }
                    let mut transcript_updated = false;
                    if let Some(idx) = self.pending_assistant_idx {
                        if let Some(m) = self.messages.get_mut(idx) {
                            match &mut m.thinking {
                                Some(buf) => buf.push_str(&thinking),
                                None => m.thinking = Some(thinking),
                            }
                            transcript_updated = true;
                        }
                    }
                    if transcript_updated {
                        self.note_transcript_changed_for_refresh(
                            Self::streaming_render_refresh_policy(),
                        );
                    }
                    transcript_updated
                }
                ContentDelta::InputJsonDelta { .. } => {
                    // Tool-input deltas accumulate on the engine side; the
                    // finalized Assistant message provides the parsed input.
                    false
                }
            },
            StreamEventData::MessageDelta {
                stop_reason: Some(reason),
                ..
            } => {
                if let Some(message) = exceptional_stop_reason_transcript_message(&reason) {
                    self.messages.push(message);
                    self.note_transcript_changed();
                }
                true
            }
            _ => false,
        }
    }

    fn begin_pending_assistant_message(&mut self) {
        if self.pending_assistant_idx.is_some() {
            return;
        }
        let idx = self.messages.len();
        self.set_render_record_current_turn_override(idx);
        self.messages.push(pending_assistant_transcript_message());
        self.note_transcript_changed();
        self.pending_assistant_idx = Some(idx);
        self.assistant_buf.clear();
        self.pending_assistant_history_recorded = false;
    }

    fn finish_pending_assistant_message(&mut self, terminal: Option<&str>) {
        if !self.pending_assistant_history_recorded && !self.assistant_buf.trim().is_empty() {
            let (_, content) = split_thinking_and_content(&self.assistant_buf);
            self.record_engine_assistant_text(&content);
        }

        if let Some(idx) = self.pending_assistant_idx.take() {
            let should_remove = self
                .messages
                .get_mut(idx)
                .map(|m| {
                    matches!(
                        finalize_pending_assistant_transcript_message(m, terminal),
                        PendingAssistantFinalization::Remove
                    )
                })
                .unwrap_or(false);

            if should_remove {
                self.messages.remove(idx);
                self.remove_render_record_id_override_at(idx);
            }
            self.note_transcript_changed();
        }

        self.assistant_buf.clear();
        self.pending_assistant_history_recorded = false;
    }

    /// Mark the in-flight assistant turn as finished.
    fn finalize_assistant_turn(&mut self, terminal: Option<String>) {
        self.finish_pending_assistant_message(terminal.as_deref());
        // Keep tool results visible by default. The earlier auto-collapse
        // made successful tool calls look like they vanished, especially
        // during permission-heavy runs; users can still collapse focused
        // tool groups manually with Space/Enter.
        self.engine_rx = None;
        self.assistant_buf.clear();
        self.pending_assistant_history_recorded = false;
        self.state.is_streaming = false;
        self.state.is_waiting_for_response = false;
        // Turn state transition: Cancelling → Cancelled → Idle.
        if self.state.turn_state == TurnState::Cancelling {
            self.state.turn_state = TurnState::Cancelled;
        }
        self.state.ui_stage = terminal
            .as_deref()
            .map(ui_stage_from_terminal)
            .unwrap_or(UiStage::Done);
        self.state.turn_state = TurnState::Idle;
        self.record_final_summary(terminal.as_deref());
        self.clear_current_render_turn_id();
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

fn process_status_for_turn(
    turn_state: TurnState,
    ui_stage: UiStage,
    blocking: Option<&BlockingRenderModel>,
) -> ProcessStatus {
    if let Some(blocking) = blocking {
        return process_status_for_blocking(blocking.kind);
    }
    match turn_state {
        TurnState::Streaming => ProcessStatus::Running,
        TurnState::Cancelling => ProcessStatus::Waiting,
        TurnState::Cancelled => ProcessStatus::Failed,
        TurnState::Idle => match ui_stage {
            UiStage::Idle => ProcessStatus::Idle,
            UiStage::WaitingApproval => ProcessStatus::Waiting,
            UiStage::Done => ProcessStatus::Completed,
            UiStage::Failed | UiStage::Cancelled => ProcessStatus::Failed,
            _ => ProcessStatus::Running,
        },
    }
}

fn process_status_for_blocking(kind: BlockingKind) -> ProcessStatus {
    match kind {
        BlockingKind::Approval | BlockingKind::CostLimit | BlockingKind::IdleReturn => {
            ProcessStatus::Waiting
        }
        BlockingKind::Error => ProcessStatus::Failed,
        BlockingKind::Info => ProcessStatus::Info,
    }
}

fn process_row_from_activity(
    ui_stage: UiStage,
    activity: &RenderActivity,
) -> ProcessRowRenderModel {
    let (status, title) = match activity {
        RenderActivity::Approval { .. } => (ProcessStatus::Waiting, "Approval requested"),
        RenderActivity::Error { .. } => (ProcessStatus::Failed, "Error"),
        RenderActivity::Retry { .. } => (ProcessStatus::Waiting, "Retry scheduled"),
        RenderActivity::Diff { .. } => (ProcessStatus::Info, "Diff available"),
        RenderActivity::FileChange { .. } => (ProcessStatus::Info, "File changes"),
        RenderActivity::CommandFinished { exit_code, .. } if exit_code.unwrap_or(0) != 0 => {
            (ProcessStatus::Failed, "Command failed")
        }
        RenderActivity::CommandFinished { .. } | RenderActivity::Final { success: true } => {
            (ProcessStatus::Completed, "Activity complete")
        }
        RenderActivity::Final { success: false } => (ProcessStatus::Failed, "Activity failed"),
        RenderActivity::BackgroundTask {
            status, exit_code, ..
        } => {
            if exit_code.is_some_and(|code| code != 0)
                || matches!(status.as_str(), "failed" | "cancelled" | "canceled")
            {
                (ProcessStatus::Failed, "Background task")
            } else if matches!(status.as_str(), "completed" | "deleted") {
                (ProcessStatus::Completed, "Background task")
            } else {
                (ProcessStatus::Running, "Background task")
            }
        }
        RenderActivity::CommandStarted { .. } | RenderActivity::CommandOutput { .. } => {
            (ProcessStatus::Running, "Command activity")
        }
        RenderActivity::Plan { .. } => (ProcessStatus::Running, "Plan activity"),
        RenderActivity::Compact { .. } => (ProcessStatus::Running, "Compact activity"),
        RenderActivity::CompactStatus { status, .. } => {
            if matches!(status.as_str(), "failed" | "timed_out") {
                (ProcessStatus::Failed, "Compact status")
            } else if matches!(status.as_str(), "completed" | "skipped" | "dry_run") {
                (ProcessStatus::Completed, "Compact status")
            } else {
                (ProcessStatus::Running, "Compact status")
            }
        }
        RenderActivity::ClearStatus { status, .. } => {
            if matches!(status.as_str(), "timed_out") {
                (ProcessStatus::Failed, "Clear status")
            } else if matches!(status.as_str(), "completed" | "dry_run") {
                (ProcessStatus::Completed, "Clear status")
            } else {
                (ProcessStatus::Running, "Clear status")
            }
        }
        RenderActivity::SlashCommand { status, error, .. } => {
            if error.is_some() || matches!(status.as_str(), "error" | "failed") {
                (ProcessStatus::Failed, "Slash command")
            } else if matches!(status.as_str(), "queued" | "submitted") {
                (ProcessStatus::Running, "Slash command")
            } else {
                (ProcessStatus::Completed, "Slash command")
            }
        }
        RenderActivity::AssistantMessage { .. }
        | RenderActivity::Thinking { .. }
        | RenderActivity::ToolInput { .. }
        | RenderActivity::Tool { .. } => (ProcessStatus::Running, "Agent activity"),
    };

    ProcessRowRenderModel::new("activity", ProcessRowKind::Activity, status, title)
        .detail(activity.status_line())
        .fact("stage", ui_stage.label())
}

fn command_history_row_from_activity(
    activity: &RenderActivity,
) -> Option<CommandHistoryRowRenderModel> {
    match activity {
        RenderActivity::CommandStarted { command, cwd } => {
            let run = CommandRunRenderModel {
                command: command.clone(),
                cwd: cwd.clone(),
                status: CommandRunStatus::Running,
                exit_code: None,
                duration_ms: None,
                timed_out: false,
                interrupted: false,
                signal: None,
                error_summary: None,
                stdout: CommandStreamRenderModel::empty("stdout"),
                stderr: CommandStreamRenderModel::empty("stderr"),
                full_log_available: false,
            };
            Some(CommandHistoryRowRenderModel::from_run(
                "active-command",
                run,
            ))
        }
        RenderActivity::CommandOutput {
            stream,
            preview_lines,
            hidden_lines,
            total_lines,
            full_log_available,
            ..
        } => {
            let stream_model = CommandStreamRenderModel {
                name: stream.clone(),
                preview_line_count: *preview_lines,
                hidden_line_count: *hidden_lines,
                total_line_count: *total_lines,
                has_content: *preview_lines > 0 || *hidden_lines > 0 || total_lines.is_some(),
                full_log_available: *full_log_available,
                full_text: None,
            };
            let stream_is_stderr = stream.to_ascii_lowercase().contains("stderr");
            let run = CommandRunRenderModel {
                command: Some("Active command output".to_string()),
                cwd: None,
                status: CommandRunStatus::Running,
                exit_code: None,
                duration_ms: None,
                timed_out: false,
                interrupted: false,
                signal: None,
                error_summary: None,
                stdout: if stream_is_stderr {
                    CommandStreamRenderModel::empty("stdout")
                } else {
                    stream_model.clone()
                },
                stderr: if stream_is_stderr {
                    stream_model
                } else {
                    CommandStreamRenderModel::empty("stderr")
                },
                full_log_available: *full_log_available,
            };
            Some(CommandHistoryRowRenderModel::from_run(
                "active-command-output",
                run,
            ))
        }
        RenderActivity::CommandFinished {
            exit_code,
            duration_ms,
        } => {
            let status = if exit_code.is_some_and(|code| code != 0) {
                CommandRunStatus::Failed
            } else {
                CommandRunStatus::Succeeded
            };
            let run = CommandRunRenderModel {
                command: Some("Recent command".to_string()),
                cwd: None,
                status,
                exit_code: *exit_code,
                duration_ms: *duration_ms,
                timed_out: false,
                interrupted: false,
                signal: None,
                error_summary: None,
                stdout: CommandStreamRenderModel::empty("stdout"),
                stderr: CommandStreamRenderModel::empty("stderr"),
                full_log_available: false,
            };
            Some(CommandHistoryRowRenderModel::from_run(
                "active-command-finished",
                run,
            ))
        }
        _ => None,
    }
}

fn error_history_row_from_activity(
    activity: &RenderActivity,
) -> Option<ErrorHistoryRowRenderModel> {
    match activity {
        RenderActivity::Error { source, summary } => Some(
            ErrorHistoryRowRenderModel::from_error(
                "active-error",
                source.clone(),
                ErrorRenderModel {
                    title: "Active error".to_string(),
                    summary: summary.clone(),
                    key_detail: Some(summary.clone()),
                    details: Some(format!("source: {source}\nsummary: {summary}")),
                    detail_hidden_line_count: 0,
                    retry_hint: None,
                    retrying: false,
                },
            )
            .source_block_id("active-activity"),
        ),
        RenderActivity::Retry {
            attempt,
            max_retries,
            retry_in_ms,
        } => Some(
            ErrorHistoryRowRenderModel::from_error(
                "active-retry",
                "retry scheduler",
                ErrorRenderModel {
                    title: "Retry scheduled".to_string(),
                    summary: format!("attempt {attempt}/{max_retries}"),
                    key_detail: Some(format!("retry in {retry_in_ms}ms")),
                    details: Some(format!(
                        "attempt: {attempt}/{max_retries}\nnext retry: {retry_in_ms}ms"
                    )),
                    detail_hidden_line_count: 0,
                    retry_hint: Some(format!("Automatic retry in {retry_in_ms}ms.")),
                    retrying: true,
                },
            )
            .source_block_id("active-activity"),
        ),
        RenderActivity::CommandFinished {
            exit_code: Some(code),
            duration_ms,
        } if *code != 0 => {
            let mut details = format!("exit code: {code}");
            if let Some(duration) = duration_ms {
                details.push_str(&format!("\nduration: {duration}ms"));
            }
            let mut row = ErrorHistoryRowRenderModel::from_error(
                "active-command-error",
                "command",
                ErrorRenderModel {
                    title: "Command failed".to_string(),
                    summary: format!("Recent command exited with {code}"),
                    key_detail: Some(format!("exit code {code}")),
                    details: Some(details),
                    detail_hidden_line_count: 0,
                    retry_hint: None,
                    retrying: false,
                },
            )
            .source_block_id("active-activity");
            row.command_failure = true;
            Some(row)
        }
        _ => None,
    }
}

fn process_status_from_todo(status: &str) -> ProcessStatus {
    let normalized = status.to_ascii_lowercase();
    if normalized.contains("complete") || normalized == "done" {
        ProcessStatus::Completed
    } else if normalized.contains("progress") || normalized.contains("running") {
        ProcessStatus::Running
    } else if normalized.contains("fail") || normalized.contains("error") {
        ProcessStatus::Failed
    } else {
        ProcessStatus::Waiting
    }
}

fn process_status_from_task_store(status: &str) -> ProcessStatus {
    let normalized = status.to_ascii_lowercase();
    if normalized.contains("running")
        || normalized.contains("active")
        || normalized.contains("progress")
    {
        ProcessStatus::Running
    } else if normalized.contains("complete") || normalized == "done" {
        ProcessStatus::Completed
    } else if normalized.contains("fail") || normalized.contains("error") {
        ProcessStatus::Failed
    } else {
        ProcessStatus::Waiting
    }
}

fn process_row_from_teammate(id: &str, state: &TeammateState) -> ProcessRowRenderModel {
    match state {
        TeammateState::Running => ProcessRowRenderModel::new(
            format!("agent-{id}"),
            ProcessRowKind::Agent,
            ProcessStatus::Running,
            id.to_string(),
        ),
        TeammateState::Completed(summary) => ProcessRowRenderModel::new(
            format!("agent-{id}"),
            ProcessRowKind::Agent,
            ProcessStatus::Completed,
            id.to_string(),
        )
        .detail(summary.clone()),
        TeammateState::Failed(summary) => ProcessRowRenderModel::new(
            format!("agent-{id}"),
            ProcessRowKind::Agent,
            ProcessStatus::Failed,
            id.to_string(),
        )
        .detail(summary.clone()),
    }
}

fn status_level_for_blocking(kind: BlockingKind) -> StatusRowLevel {
    match kind {
        BlockingKind::Approval | BlockingKind::CostLimit | BlockingKind::IdleReturn => {
            StatusRowLevel::Warning
        }
        BlockingKind::Error => StatusRowLevel::Error,
        BlockingKind::Info => StatusRowLevel::Info,
    }
}

fn status_level_for_turn(
    turn_state: &str,
    blocking: Option<&BlockingRenderModel>,
) -> StatusRowLevel {
    if let Some(blocking) = blocking {
        return status_level_for_blocking(blocking.kind);
    }
    let normalized = turn_state.to_ascii_lowercase();
    if normalized.contains("failed") || normalized.contains("cancelled") {
        StatusRowLevel::Error
    } else if normalized.contains("waiting")
        || normalized.contains("approval")
        || normalized.contains("cancelling")
    {
        StatusRowLevel::Warning
    } else if normalized.contains("running")
        || normalized.contains("streaming")
        || normalized.contains("thinking")
        || normalized.contains("planning")
        || normalized.contains("reading")
        || normalized.contains("editing")
        || normalized.contains("reviewing")
    {
        StatusRowLevel::Info
    } else if normalized.contains("done") || normalized.contains("idle") {
        StatusRowLevel::Good
    } else {
        StatusRowLevel::Normal
    }
}

fn status_level_for_process_summary(summary: &ProcessSummaryRenderModel) -> StatusRowLevel {
    if summary.failed_count > 0 {
        StatusRowLevel::Error
    } else if summary.waiting_count > 0 {
        StatusRowLevel::Warning
    } else if summary.active_count > 0 {
        StatusRowLevel::Info
    } else {
        StatusRowLevel::Good
    }
}

fn status_context_value(context: Option<ContextUsageRenderModel>) -> String {
    context
        .map(|context| {
            format!(
                "{} ({}%, {} / {} tokens)",
                context.label(),
                context.used_percent(),
                context.used_tokens,
                context.window_tokens
            )
        })
        .unwrap_or_else(|| "unknown".to_string())
}

fn status_level_for_context(context: Option<ContextUsageRenderModel>) -> StatusRowLevel {
    match context.map(ContextUsageRenderModel::used_percent) {
        Some(percent) if percent >= 95 => StatusRowLevel::Error,
        Some(percent) if percent >= 80 => StatusRowLevel::Warning,
        Some(_) => StatusRowLevel::Good,
        None => StatusRowLevel::Info,
    }
}

fn bool_label(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

fn render_cache_shape_mix(acc: u64, value: u64) -> u64 {
    acc.wrapping_mul(0x0000_0100_0000_01b3).wrapping_add(value)
}

fn option_string_shape(value: Option<&String>) -> u64 {
    value.map(|value| value.len() as u64).unwrap_or(0)
}

fn message_type_shape_key(message_type: MessageType) -> u64 {
    match message_type {
        MessageType::User => 1,
        MessageType::Assistant => 2,
        MessageType::System => 3,
        MessageType::CommandOutput => 4,
        MessageType::Progress => 5,
        MessageType::Attachment => 6,
        MessageType::ToolUse => 7,
        MessageType::ToolResult => 8,
        MessageType::SkillInvocation => 9,
    }
}

fn active_modal_shape_key(modal: &ActiveModal) -> u64 {
    match modal {
        ActiveModal::None => 0,
        ActiveModal::PermissionRequest(_) => 1,
        ActiveModal::ToolUseConfirm { .. } => 2,
        ActiveModal::CostThreshold(_) => 3,
        ActiveModal::IdleReturn(_) => 4,
        ActiveModal::MessageSelector(_) => 5,
        ActiveModal::Search(_) => 6,
        ActiveModal::HelpDialog(_) => 7,
        ActiveModal::ConfirmClear => 8,
        ActiveModal::StatusDialog => 9,
        ActiveModal::StatusLineConfig(_) => 10,
        ActiveModal::TitleConfig(_) => 11,
        ActiveModal::RawTranscript(_) => 12,
        ActiveModal::DiffReview(_) => 13,
        ActiveModal::FileChanges(_) => 14,
        ActiveModal::RenderTimeline(_) => 15,
        ActiveModal::ProcessList(_) => 16,
        ActiveModal::CommandHistory(_) => 17,
        ActiveModal::ErrorHistory(_) => 18,
        ActiveModal::FinalSummaryHistory(_) => 19,
        ActiveModal::ApprovalHistory(_) => 20,
        ActiveModal::DebugConfig(_) => 21,
        ActiveModal::TasksDialog => 22,
        ActiveModal::McpServersDialog => 23,
        ActiveModal::McpChannelApproval(_) => 24,
        ActiveModal::ModelPicker(_) => 25,
        ActiveModal::SkillsPanel(_) => 26,
        ActiveModal::MemoryPanel(_) => 27,
        ActiveModal::CommandOutput { .. } => 28,
        ActiveModal::Picker { .. } => 29,
    }
}

fn ui_stage_shape_key(stage: UiStage) -> u64 {
    match stage {
        UiStage::Idle => 0,
        UiStage::Thinking => 1,
        UiStage::Planning => 2,
        UiStage::ReadingRepo => 3,
        UiStage::EditingFiles => 4,
        UiStage::WaitingApproval => 5,
        UiStage::RunningCommand => 6,
        UiStage::ReviewingResult => 7,
        UiStage::Retrying => 8,
        UiStage::Done => 9,
        UiStage::Failed => 10,
        UiStage::Cancelled => 11,
    }
}

fn turn_state_shape_key(state: TurnState) -> u64 {
    match state {
        TurnState::Idle => 0,
        TurnState::Streaming => 1,
        TurnState::Cancelling => 2,
        TurnState::Cancelled => 3,
    }
}

fn configured_label(value: Option<&str>) -> &'static str {
    if value.is_some_and(|value| !value.trim().is_empty()) {
        "configured"
    } else {
        "missing"
    }
}

fn glyph_mode_label(mode: RenderGlyphMode) -> &'static str {
    match mode {
        RenderGlyphMode::Unicode => "unicode",
        RenderGlyphMode::Ascii => "ascii",
    }
}

fn color_mode_label(mode: crate::render_profile::RenderColorMode) -> &'static str {
    match mode {
        crate::render_profile::RenderColorMode::Color => "color",
        crate::render_profile::RenderColorMode::Plain => "plain",
    }
}

fn api_base_debug_label(api_base_url: Option<&str>) -> String {
    api_base_url
        .filter(|value| !value.trim().is_empty())
        .map(|value| format!("custom: {value}"))
        .unwrap_or_else(|| "default".to_string())
}

fn redacted_extra_body_keys(extra_body: &HashMap<String, serde_json::Value>) -> String {
    if extra_body.is_empty() {
        return "none".to_string();
    }
    let mut keys = extra_body.keys().cloned().collect::<Vec<_>>();
    keys.sort();
    format!("{} key(s): {}", keys.len(), keys.join(", "))
}

fn footer_item_labels(items: &[FooterItem]) -> String {
    if items.is_empty() {
        return "none".to_string();
    }
    items
        .iter()
        .map(|item| item.label())
        .collect::<Vec<_>>()
        .join(", ")
}

fn short_debug_id(id: &str) -> String {
    let id = id.trim();
    if id.chars().count() <= 18 {
        id.to_string()
    } else {
        format!("{}...", id.chars().take(18).collect::<String>())
    }
}

fn status_todo_summary(todos: &[mossen_tools::todo::TodoItem]) -> (String, StatusRowLevel) {
    if todos.is_empty() {
        return ("none".to_string(), StatusRowLevel::Normal);
    }

    let mut running = 0usize;
    let mut waiting = 0usize;
    let mut completed = 0usize;
    let mut failed = 0usize;
    for todo in todos {
        match process_status_from_todo(&todo.status) {
            ProcessStatus::Running => running += 1,
            ProcessStatus::Waiting => waiting += 1,
            ProcessStatus::Completed => completed += 1,
            ProcessStatus::Failed => failed += 1,
            ProcessStatus::Idle | ProcessStatus::Info => waiting += 1,
        }
    }

    let level = if failed > 0 {
        StatusRowLevel::Error
    } else if running > 0 {
        StatusRowLevel::Info
    } else if waiting > 0 {
        StatusRowLevel::Warning
    } else {
        StatusRowLevel::Good
    };
    (
        format!(
            "{} total / {} running / {} waiting / {} done / {} failed",
            todos.len(),
            running,
            waiting,
            completed,
            failed
        ),
        level,
    )
}

fn status_agent_summary(agents: &HashMap<String, TeammateState>) -> (String, StatusRowLevel) {
    if agents.is_empty() {
        return ("none".to_string(), StatusRowLevel::Normal);
    }

    let mut running = 0usize;
    let mut completed = 0usize;
    let mut failed = 0usize;
    for state in agents.values() {
        match state {
            TeammateState::Running => running += 1,
            TeammateState::Completed(_) => completed += 1,
            TeammateState::Failed(_) => failed += 1,
        }
    }

    let level = if failed > 0 {
        StatusRowLevel::Error
    } else if running > 0 {
        StatusRowLevel::Info
    } else {
        StatusRowLevel::Good
    };
    (
        format!(
            "{} total / {} running / {} done / {} failed",
            agents.len(),
            running,
            completed,
            failed
        ),
        level,
    )
}

fn status_mcp_summary(servers: &[McpServerStatus]) -> (String, StatusRowLevel) {
    if servers.is_empty() {
        return ("none configured".to_string(), StatusRowLevel::Info);
    }

    let total = servers.len();
    let tools: usize = servers.iter().map(|server| server.tools_count).sum();
    let ready = servers
        .iter()
        .filter(|server| {
            matches!(
                server.state,
                McpConnectionState::Configured | McpConnectionState::Connected
            )
        })
        .count();
    let pending = servers
        .iter()
        .filter(|server| server.state == McpConnectionState::Pending)
        .count();
    let needs_auth = servers
        .iter()
        .filter(|server| server.state == McpConnectionState::NeedsAuth)
        .count();
    let failed = servers
        .iter()
        .filter(|server| server.state == McpConnectionState::Failed)
        .count();
    let disabled = servers
        .iter()
        .filter(|server| server.state == McpConnectionState::Disabled)
        .count();

    let mut parts = vec![
        format!("{total} servers"),
        format!("{tools} tools"),
        format!("{ready} ready"),
    ];
    if pending > 0 {
        parts.push(format!("{pending} pending"));
    }
    if needs_auth > 0 {
        parts.push(format!("{needs_auth} needs auth"));
    }
    if failed > 0 {
        parts.push(format!("{failed} failed"));
    }
    if disabled > 0 {
        parts.push(format!("{disabled} disabled"));
    }

    let level = if failed > 0 {
        StatusRowLevel::Error
    } else if needs_auth > 0 || pending > 0 {
        StatusRowLevel::Warning
    } else {
        StatusRowLevel::Good
    };
    (parts.join(", "), level)
}

fn render_activity_from_event(event: &RenderEvent) -> Option<RenderActivity> {
    match &event.kind {
        RenderEventKind::TextDelta { bytes } => {
            Some(RenderActivity::AssistantMessage { bytes: *bytes })
        }
        RenderEventKind::ThinkingDelta { bytes } => {
            Some(RenderActivity::Thinking { bytes: *bytes })
        }
        RenderEventKind::ToolInputDelta { bytes } => {
            Some(RenderActivity::ToolInput { bytes: *bytes })
        }
        RenderEventKind::CommandStarted { command, cwd, .. } => {
            Some(RenderActivity::CommandStarted {
                command: command.clone(),
                cwd: cwd.clone(),
            })
        }
        RenderEventKind::CommandFinished {
            exit_code,
            duration_ms,
            ..
        } => Some(RenderActivity::CommandFinished {
            exit_code: *exit_code,
            duration_ms: *duration_ms,
        }),
        RenderEventKind::CommandOutput {
            stream,
            bytes,
            preview_lines,
            hidden_lines,
            total_lines,
            full_log_available,
            ..
        } => Some(RenderActivity::CommandOutput {
            stream: stream.clone(),
            bytes: *bytes,
            preview_lines: *preview_lines,
            hidden_lines: *hidden_lines,
            total_lines: *total_lines,
            full_log_available: *full_log_available,
        }),
        RenderEventKind::BackgroundTaskUpdated {
            task_id,
            status,
            command,
            preview_lines,
            hidden_lines,
            exit_code,
            ..
        } => Some(RenderActivity::BackgroundTask {
            task_id: task_id.clone(),
            status: status.clone(),
            command: command.clone(),
            preview_lines: *preview_lines,
            hidden_lines: *hidden_lines,
            exit_code: *exit_code,
        }),
        RenderEventKind::ToolRequested { tool_name, .. }
        | RenderEventKind::ToolCompleted { tool_name, .. } => Some(RenderActivity::Tool {
            name: tool_name.clone(),
        }),
        RenderEventKind::PlanUpdated {
            step_count,
            completed_count,
            active_count,
            pending_count,
            blocked_count,
            active_step,
            ..
        } => Some(RenderActivity::Plan {
            step_count: *step_count,
            completed_count: *completed_count,
            active_count: *active_count,
            pending_count: *pending_count,
            blocked_count: *blocked_count,
            active_step: active_step.clone(),
        }),
        RenderEventKind::FileChangeSummary {
            file_count,
            additions,
            deletions,
            ..
        } => Some(RenderActivity::FileChange {
            file_count: *file_count,
            additions: *additions,
            deletions: *deletions,
        }),
        RenderEventKind::DiffAvailable {
            file_count,
            additions,
            deletions,
            ..
        } => Some(RenderActivity::Diff {
            file_count: *file_count,
            additions: *additions,
            deletions: *deletions,
        }),
        RenderEventKind::ApprovalRequested { tool_name } => Some(RenderActivity::Approval {
            tool_name: tool_name.clone(),
        }),
        RenderEventKind::ErrorRaised { source, summary } => Some(RenderActivity::Error {
            source: source.clone(),
            summary: summary.clone(),
        }),
        RenderEventKind::ApiRetry {
            attempt,
            max_retries,
            retry_in_ms,
        } => Some(RenderActivity::Retry {
            attempt: *attempt,
            max_retries: *max_retries,
            retry_in_ms: *retry_in_ms,
        }),
        RenderEventKind::CompactBoundary {
            before_token_count,
            after_token_count,
        } => Some(RenderActivity::Compact {
            before_token_count: *before_token_count,
            after_token_count: *after_token_count,
        }),
        RenderEventKind::CompactRequestStatus {
            status,
            dry_run,
            reason,
            ..
        } => Some(RenderActivity::CompactStatus {
            status: status.clone(),
            dry_run: *dry_run,
            reason: reason.clone(),
        }),
        RenderEventKind::ClearRequestStatus {
            status,
            dry_run,
            reason,
            ..
        } => Some(RenderActivity::ClearStatus {
            status: status.clone(),
            dry_run: *dry_run,
            reason: reason.clone(),
        }),
        RenderEventKind::SlashCommandResult {
            command,
            status,
            summary,
            error,
            ..
        } => Some(RenderActivity::SlashCommand {
            command: command.clone(),
            status: status.clone(),
            summary: summary.clone(),
            error: error.clone(),
        }),
        RenderEventKind::FinalSummaryRecorded { success, .. } => {
            Some(RenderActivity::Final { success: *success })
        }
        RenderEventKind::TurnStarted
        | RenderEventKind::StreamStarted
        | RenderEventKind::ConversationCleared { .. }
        | RenderEventKind::TurnFinished { .. } => None,
    }
}

fn activity_panel_from_render_activity(
    stage: &str,
    activity: &RenderActivity,
) -> ActivityPanelRenderModel {
    match activity {
        RenderActivity::AssistantMessage { bytes } => ActivityPanelRenderModel::new(
            stage,
            "Assistant response",
            ActivityPanelSeverity::Working,
        )
        .summary(format!("{bytes} bytes streamed")),
        RenderActivity::Thinking { bytes } => {
            ActivityPanelRenderModel::new(stage, "Reasoning", ActivityPanelSeverity::Working)
                .summary(format!("{bytes} bytes streamed"))
        }
        RenderActivity::ToolInput { bytes } => {
            ActivityPanelRenderModel::new(stage, "Tool input", ActivityPanelSeverity::Working)
                .summary(format!("{bytes} bytes received"))
        }
        RenderActivity::Tool { name } => {
            ActivityPanelRenderModel::new(stage, "Tool activity", ActivityPanelSeverity::Working)
                .detail("tool", name.clone())
        }
        RenderActivity::Plan {
            step_count,
            completed_count,
            active_count,
            pending_count,
            blocked_count,
            active_step,
        } => {
            let model =
                ActivityPanelRenderModel::new(stage, "Plan", ActivityPanelSeverity::Working)
                    .summary(plan_activity_progress_summary(
                        *step_count,
                        *completed_count,
                        *active_count,
                        *pending_count,
                        *blocked_count,
                    ));
            match active_step {
                Some(step) => model.detail("active", step.clone()),
                None => model,
            }
        }
        RenderActivity::FileChange {
            file_count,
            additions,
            deletions,
        } => ActivityPanelRenderModel::new(stage, "File changes", ActivityPanelSeverity::Working)
            .summary(format!("{file_count} files"))
            .detail("lines", format!("+{additions} -{deletions}")),
        RenderActivity::Diff {
            file_count,
            additions,
            deletions,
        } => ActivityPanelRenderModel::new(stage, "Diff available", ActivityPanelSeverity::Info)
            .summary(format!("{file_count} files"))
            .detail("lines", format!("+{additions} -{deletions}")),
        RenderActivity::CommandStarted { command, cwd } => {
            let model = ActivityPanelRenderModel::new(
                stage,
                "Command running",
                ActivityPanelSeverity::Working,
            )
            .summary(command.clone().unwrap_or_else(|| "<command>".to_string()));
            match cwd {
                Some(cwd) => model.detail("cwd", cwd.clone()),
                None => model,
            }
        }
        RenderActivity::CommandOutput {
            stream,
            bytes,
            preview_lines,
            hidden_lines,
            total_lines,
            full_log_available,
        } => {
            let mut model = ActivityPanelRenderModel::new(
                stage,
                "Command output",
                ActivityPanelSeverity::Working,
            )
            .summary(command_output_panel_summary(
                stream,
                *preview_lines,
                *hidden_lines,
                *total_lines,
                *full_log_available,
            ))
            .detail("bytes", bytes.to_string());
            if *full_log_available {
                model = model.detail("log", "full");
            }
            model
        }
        RenderActivity::CommandFinished {
            exit_code,
            duration_ms,
        } => {
            let severity = match exit_code {
                Some(0) => ActivityPanelSeverity::Success,
                Some(_) => ActivityPanelSeverity::Error,
                None => ActivityPanelSeverity::Info,
            };
            let model = ActivityPanelRenderModel::new(stage, "Command finished", severity).summary(
                exit_code
                    .map(|code| format!("exit {code}"))
                    .unwrap_or_else(|| "finished".to_string()),
            );
            match duration_ms {
                Some(ms) => model.detail("duration", format!("{ms}ms")),
                None => model,
            }
        }
        RenderActivity::BackgroundTask {
            task_id,
            status,
            command,
            preview_lines,
            hidden_lines,
            exit_code,
        } => {
            let severity = if exit_code.is_some_and(|code| code != 0)
                || matches!(status.as_str(), "failed" | "cancelled" | "canceled")
            {
                ActivityPanelSeverity::Error
            } else if matches!(status.as_str(), "completed" | "deleted") {
                ActivityPanelSeverity::Success
            } else {
                ActivityPanelSeverity::Working
            };
            let mut model = ActivityPanelRenderModel::new(stage, "Background task", severity)
                .summary(format!("{status}: {task_id}"))
                .detail(
                    "output",
                    format!("{preview_lines} lines, {hidden_lines} hidden"),
                );
            if let Some(command) = command {
                model = model.detail("cmd", command.clone());
            }
            if let Some(code) = exit_code {
                model = model.detail("exit", code.to_string());
            }
            model
        }
        RenderActivity::Approval { tool_name } => ActivityPanelRenderModel::new(
            stage,
            "Approval required",
            ActivityPanelSeverity::Waiting,
        )
        .detail("tool", tool_name.clone()),
        RenderActivity::Error { source, summary } => {
            ActivityPanelRenderModel::new(stage, "Error", ActivityPanelSeverity::Error)
                .summary(summary.clone())
                .detail("source", source.clone())
        }
        RenderActivity::Retry {
            attempt,
            max_retries,
            retry_in_ms,
        } => ActivityPanelRenderModel::new(stage, "Retrying", ActivityPanelSeverity::Warning)
            .summary(format!("attempt {attempt}/{max_retries}"))
            .detail("next", format!("{retry_in_ms}ms")),
        RenderActivity::Compact {
            before_token_count,
            after_token_count,
        } => ActivityPanelRenderModel::new(stage, "Context compacted", ActivityPanelSeverity::Info)
            .summary(format!("{before_token_count}->{after_token_count} tokens")),
        RenderActivity::CompactStatus {
            status,
            dry_run,
            reason,
        } => {
            let severity = if matches!(status.as_str(), "failed" | "timed_out") {
                ActivityPanelSeverity::Error
            } else if matches!(status.as_str(), "completed") {
                ActivityPanelSeverity::Success
            } else {
                ActivityPanelSeverity::Info
            };
            let mut model = ActivityPanelRenderModel::new(stage, "Compact status", severity)
                .summary(status.clone());
            if *dry_run {
                model = model.detail("mode", "dry run");
            }
            if let Some(reason) = reason.as_deref().filter(|reason| !reason.trim().is_empty()) {
                model = model.detail("reason", reason.to_string());
            }
            model
        }
        RenderActivity::ClearStatus {
            status,
            dry_run,
            reason,
        } => {
            let severity = if matches!(status.as_str(), "timed_out") {
                ActivityPanelSeverity::Error
            } else if matches!(status.as_str(), "completed") {
                ActivityPanelSeverity::Success
            } else {
                ActivityPanelSeverity::Info
            };
            let mut model = ActivityPanelRenderModel::new(stage, "Clear status", severity)
                .summary(status.clone());
            if *dry_run {
                model = model.detail("mode", "dry run");
            }
            if let Some(reason) = reason.as_deref().filter(|reason| !reason.trim().is_empty()) {
                model = model.detail("reason", reason.to_string());
            }
            model
        }
        RenderActivity::SlashCommand {
            command,
            status,
            summary,
            error,
        } => {
            let severity = if error.is_some() || matches!(status.as_str(), "error" | "failed") {
                ActivityPanelSeverity::Error
            } else if matches!(status.as_str(), "queued" | "submitted") {
                ActivityPanelSeverity::Working
            } else {
                ActivityPanelSeverity::Info
            };
            let mut model = ActivityPanelRenderModel::new(stage, "Slash command", severity)
                .summary(summary.clone())
                .detail("command", format!("/{command}"));
            if let Some(error) = error.as_deref().filter(|error| !error.trim().is_empty()) {
                model = model.detail("error", error.to_string());
            }
            model
        }
        RenderActivity::Final { success } => {
            let (title, severity, summary) = if *success {
                ("Final summary", ActivityPanelSeverity::Success, "done")
            } else {
                (
                    "Final summary",
                    ActivityPanelSeverity::Error,
                    "needs attention",
                )
            };
            ActivityPanelRenderModel::new(stage, title, severity).summary(summary)
        }
    }
}

fn plan_activity_progress_summary(
    step_count: usize,
    completed_count: usize,
    active_count: usize,
    pending_count: usize,
    blocked_count: usize,
) -> String {
    let mut parts = vec![format!("{step_count} steps")];
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
    parts.join(" · ")
}

fn command_output_panel_summary(
    stream: &str,
    preview_lines: usize,
    hidden_lines: usize,
    total_lines: Option<usize>,
    full_log_available: bool,
) -> String {
    let mut parts = vec![format!("{stream}: {preview_lines} shown")];
    if hidden_lines > 0 {
        parts.push(format!("{hidden_lines} hidden"));
    }
    if let Some(total) = total_lines {
        parts.push(format!("{total} total"));
    }
    if full_log_available {
        parts.push("full log".to_string());
    }
    parts.join(" · ")
}

fn ui_stage_after_tool_summary(tool_name: &str) -> UiStage {
    if matches!(
        tool_name.to_ascii_lowercase().as_str(),
        "bash" | "powershell"
    ) {
        UiStage::ReviewingResult
    } else {
        UiStage::from_tool_name(tool_name)
    }
}

fn text_message(role: Role, text: &str) -> Message {
    Message {
        role,
        content: vec![ContentBlock::Text(TextBlock {
            text: text.to_string(),
        })],
        uuid: Some(uuid::Uuid::new_v4().to_string()),
        is_meta: None,
        origin: None,
        timestamp: Some(chrono::Utc::now().to_rfc3339()),
        extra: HashMap::new(),
    }
}

fn final_summary_model_from_messages(
    id: String,
    terminal: &str,
    messages: &[MessageData],
) -> FinalSummaryModel {
    let stage = UiStage::from_terminal(terminal);
    let changed_files = file_change_summaries_from_messages(messages);
    let commands = command_summaries_from_messages(messages);
    let verification_results = verification_summaries_from_commands(&commands);
    let residual_risks =
        residual_risks_from_summary(stage, &changed_files, &commands, &verification_results);
    let mut notes = Vec::new();
    if matches!(stage, UiStage::Done) && verification_results.iter().any(|result| result.passed) {
        notes.push("Validation was recorded in the final summary.".to_string());
    }

    FinalSummaryModel {
        id,
        success: matches!(stage, UiStage::Done),
        terminal: terminal.to_string(),
        changed_files,
        commands,
        verification_results,
        residual_risks,
        notes,
    }
}

fn final_summary_should_record(model: &FinalSummaryModel) -> bool {
    model.needs_attention() || !model.changed_files.is_empty()
}

fn verification_summaries_from_commands(
    commands: &[CommandSummaryModel],
) -> Vec<VerificationSummaryModel> {
    commands
        .iter()
        .filter(|command| command_looks_like_verification(&command.command))
        .map(|command| VerificationSummaryModel {
            command: command.command.clone(),
            status: command.status.clone(),
            passed: command.exit_code == Some(0) || command.status.eq_ignore_ascii_case("passed"),
            exit_code: command.exit_code,
            duration_ms: command.duration_ms,
        })
        .collect()
}

fn residual_risks_from_summary(
    stage: UiStage,
    changed_files: &[FileChangeSummaryModel],
    commands: &[CommandSummaryModel],
    verification_results: &[VerificationSummaryModel],
) -> Vec<String> {
    let mut risks = Vec::new();
    if commands
        .iter()
        .any(|command| command.exit_code.is_some_and(|code| code != 0))
    {
        risks.push("Some commands exited with a non-zero status.".to_string());
    }
    if verification_results.iter().any(|result| !result.passed) {
        risks.push("At least one validation command failed.".to_string());
    }
    if verification_results.is_empty()
        && matches!(stage, UiStage::Done)
        && (!changed_files.is_empty() || !commands.is_empty())
    {
        risks.push("No validation command was recorded.".to_string());
    }
    if matches!(stage, UiStage::Cancelled) {
        risks.push("The turn was cancelled before normal completion.".to_string());
    }
    if matches!(stage, UiStage::Failed) && risks.is_empty() {
        risks.push("The turn ended before normal completion.".to_string());
    }
    risks
}

fn command_looks_like_verification(command: &str) -> bool {
    let command = command.trim().to_ascii_lowercase();
    if command.is_empty() {
        return false;
    }

    [
        "cargo test",
        "cargo check",
        "cargo clippy",
        "cargo fmt",
        "npm test",
        "npm run test",
        "npm run lint",
        "npm run typecheck",
        "pnpm test",
        "pnpm lint",
        "pnpm typecheck",
        "yarn test",
        "yarn lint",
        "pytest",
        "go test",
        "dotnet test",
        "mvn test",
        "gradle test",
        "./gradlew test",
        "make test",
    ]
    .iter()
    .any(|prefix| command.starts_with(prefix))
        || command.contains(" tsc ")
        || command.starts_with("tsc ")
        || command.ends_with(" tsc")
}

fn json_string_value(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(text) => Some(text.clone()),
        serde_json::Value::Number(number) => Some(number.to_string()),
        serde_json::Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

fn context_window_tokens_for_footer(model: &str) -> u64 {
    mossen_utils::context::terminal_context_window_tokens(model)
        .unwrap_or(mossen_utils::context::MODEL_CONTEXT_WINDOW_DEFAULT)
}

fn env_flag_enabled(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "on"))
}

fn append_tui_scroll_log_line(line: String) {
    let Some(path) = std::env::var_os("MOSSEN_TUI_EVENT_LOG_PATH") else {
        return;
    };
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "{line}");
    }
}

fn tui_mouse_capture_enabled() -> bool {
    env_flag_enabled("MOSSEN_TERMINAL_RENDER_CAPTURE_MOUSE")
}

fn tui_top_status_enabled() -> bool {
    env_flag_enabled("MOSSEN_TUI_TOP_STATUS")
}

fn copy_response_index(args_raw: &str) -> Result<usize, String> {
    let trimmed = args_raw.trim();
    if trimmed.is_empty() {
        return Ok(0);
    }
    match trimmed.parse::<usize>() {
        Ok(value) if value >= 1 => Ok(value - 1),
        _ => Err(format!(
            "Usage: /copy [N|transcript|all] where N is 1 for the latest assistant response. Got: {trimmed}"
        )),
    }
}

fn write_clipboard_text(text: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let mut child = std::process::Command::new("pbcopy")
            .stdin(std::process::Stdio::piped())
            .spawn()
            .map_err(|error| error.to_string())?;
        child
            .stdin
            .as_mut()
            .ok_or_else(|| "pbcopy stdin unavailable".to_string())?
            .write_all(text.as_bytes())
            .map_err(|error| error.to_string())?;
        let status = child.wait().map_err(|error| error.to_string())?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("pbcopy exited with status {status}"))
        }
    }

    #[cfg(target_os = "linux")]
    {
        for command in ["wl-copy", "xclip"] {
            let mut process = std::process::Command::new(command);
            if command == "xclip" {
                process.args(["-selection", "clipboard"]);
            }
            let mut child = match process.stdin(std::process::Stdio::piped()).spawn() {
                Ok(child) => child,
                Err(_) => continue,
            };
            if let Some(stdin) = child.stdin.as_mut() {
                if stdin.write_all(text.as_bytes()).is_err() {
                    continue;
                }
            }
            if child.wait().map(|status| status.success()).unwrap_or(false) {
                return Ok(());
            }
        }
        Err("install wl-copy or xclip to enable clipboard writes".to_string())
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = text;
        Err("clipboard writes are not supported on this platform".to_string())
    }
}

/// Truncate `s` to at most `max` chars (byte-safe over UTF-8 codepoints).
/// Read an image off the OS clipboard. Platform-specific helpers:
///   * macOS — `osascript` extracts PNG from `«class PNGf»` and writes
///     it to a temp file we then read; `pbpaste` does not handle binary
///     clipboard data so this is the canonical path.
///   * Linux — try `xclip -selection clipboard -t image/png -o`.
///   * Anything else — return None (text-only paste).
/// Returns the raw PNG bytes when an image is present, or None when the
/// clipboard holds text / nothing / on unsupported platforms.
fn read_clipboard_image_bytes() -> Option<Vec<u8>> {
    #[cfg(target_os = "macos")]
    {
        let tmp = std::env::temp_dir().join(format!("mossen-clipboard-{}.png", std::process::id()));
        let tmp_str = tmp.to_string_lossy().to_string();
        let script = format!(
            "set png_data to the clipboard as «class PNGf»\n\
             set f to open for access POSIX file \"{}\" with write permission\n\
             try\n  set eof of f to 0\n  write png_data to f\nend try\n\
             close access f",
            tmp_str.replace('"', "\\\"")
        );
        let status = std::process::Command::new("osascript")
            .args(["-e", &script])
            .status()
            .ok()?;
        if !status.success() {
            let _ = std::fs::remove_file(&tmp);
            return None;
        }
        let bytes = std::fs::read(&tmp).ok();
        let _ = std::fs::remove_file(&tmp);
        bytes.filter(|b| !b.is_empty())
    }
    #[cfg(target_os = "linux")]
    {
        let out = std::process::Command::new("xclip")
            .args(["-selection", "clipboard", "-t", "image/png", "-o"])
            .output()
            .ok()?;
        if !out.status.success() || out.stdout.is_empty() {
            return None;
        }
        Some(out.stdout)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

fn truncate(s: &str, max: usize) -> String {
    mossen_utils::string_utils::truncate_chars(s, max)
}

fn compact_task_subject(subject: &str) -> String {
    let subject = subject.trim().replace(['\r', '\n'], " ");
    if subject.is_empty() {
        "(untitled task)".to_string()
    } else {
        truncate(&subject, 120)
    }
}

fn render_session_snapshot_file_stem(value: Option<&str>) -> String {
    let raw = value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("current-session");
    let mut out = String::with_capacity(raw.len().min(96));
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
            out.push(ch);
        } else {
            out.push('-');
        }
        if out.len() >= 96 {
            break;
        }
    }
    let stem = out.trim_matches(|ch| matches!(ch, '-' | '_' | '.'));
    if stem.is_empty() {
        "current-session".to_string()
    } else {
        stem.to_string()
    }
}

fn render_snapshot_arg_is_latest(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "" | "latest" | "last" | "autosave"
    )
}

fn statusline_subcommand_tail(args_raw: &str) -> &str {
    args_raw
        .split_once(char::is_whitespace)
        .map(|(_, tail)| tail.trim())
        .unwrap_or_default()
}

fn footer_render_config_tmp_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("statusline.json");
    path.with_file_name(format!(".{file_name}.tmp"))
}

fn app_json_to_io_error(error: serde_json::Error) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, error)
}

fn parse_footer_render_config_payload(payload: &str) -> serde_json::Result<FooterRenderConfig> {
    let value: serde_json::Value = serde_json::from_str(payload)?;
    let mut config: FooterRenderConfig = serde_json::from_value(value.clone())?;
    if let Some(external) = external_statusline_command_config_from_payload(&value) {
        config.external_command = Some(external);
    }
    if config.external_command.is_some() {
        config.set_enabled(FooterItem::ExternalStatus, true);
    }
    Ok(config)
}

fn external_statusline_command_config_from_payload(
    value: &serde_json::Value,
) -> Option<ExternalStatusLineCommandConfig> {
    let source = value.get("statusLine").unwrap_or(value);
    if source.get("type").and_then(|value| value.as_str()) != Some("command")
        && source.get("command").is_none()
    {
        return None;
    }
    let command = source.get("command")?.as_str()?.trim();
    if command.is_empty() {
        return None;
    }
    let timeout_ms = source
        .get("timeout_ms")
        .or_else(|| source.get("timeout"))
        .and_then(|value| value.as_u64())
        .unwrap_or(1_000);
    let interval_ms = source
        .get("interval_ms")
        .and_then(|value| value.as_u64())
        .unwrap_or(1_000);
    Some(ExternalStatusLineCommandConfig {
        command: command.to_string(),
        timeout_ms,
        interval_ms,
    })
}

async fn run_external_statusline_command(
    config: ExternalStatusLineCommandConfig,
    input: serde_json::Value,
    cwd: String,
    env_vars: HashMap<String, String>,
) -> (Option<String>, Option<String>) {
    let command = config.command.trim().to_string();
    if command.is_empty() {
        return (None, Some("empty command".to_string()));
    }

    let json_input = match serde_json::to_string(&input) {
        Ok(value) => value,
        Err(error) => return (None, Some(error.to_string())),
    };
    let timeout_ms = config.timeout_ms.clamp(50, 2_000);

    let mut child = match tokio::process::Command::new("/bin/sh")
        .arg("-lc")
        .arg(&command)
        .current_dir(cwd)
        .envs(env_vars)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
    {
        Ok(child) => child,
        Err(error) => return (None, Some(error.to_string())),
    };

    if let Some(mut stdin) = child.stdin.take() {
        tokio::spawn(async move {
            let _ = stdin.write_all(json_input.as_bytes()).await;
        });
    }

    match tokio::time::timeout(Duration::from_millis(timeout_ms), child.wait_with_output()).await {
        Ok(Ok(output)) if output.status.success() => {
            let output = sanitize_external_statusline_output(&output.stdout);
            if output.is_empty() {
                (None, None)
            } else {
                (Some(output), None)
            }
        }
        Ok(Ok(output)) => (
            None,
            Some(format!(
                "exit {}",
                output.status.code().map_or(-1, |code| code)
            )),
        ),
        Ok(Err(error)) => (None, Some(error.to_string())),
        Err(_) => (None, Some(format!("timeout after {timeout_ms}ms"))),
    }
}

fn sanitize_external_statusline_output(output: &[u8]) -> String {
    let raw = String::from_utf8_lossy(output);
    let clean = strip_ansi_escapes::strip_str(&raw);
    let joined = clean
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    let safe = joined
        .chars()
        .map(|ch| {
            if ch == '\t' {
                ' '
            } else if ch.is_control() {
                ' '
            } else {
                ch
            }
        })
        .collect::<String>();
    truncate_display_width(safe.trim(), 96)
}

fn render_session_snapshot_saved_body(
    path: &Path,
    cwd: &Path,
    snapshot: &RenderSessionSnapshot,
) -> String {
    format!(
        "Saved render session snapshot\n{}\npath: {}",
        render_session_snapshot_metadata(snapshot),
        render_session_snapshot_display_path(path, cwd)
    )
}

fn render_session_snapshot_loaded_body(
    path: &Path,
    cwd: &Path,
    snapshot: &RenderSessionSnapshot,
) -> String {
    format!(
        "Loaded render session snapshot\nmode: metadata validation only; live transcript unchanged\nuse /render-snapshot restore <path> to hydrate the TUI transcript\n{}\npath: {}",
        render_session_snapshot_metadata(snapshot),
        render_session_snapshot_display_path(path, cwd)
    )
}

fn render_session_snapshot_restored_body(
    path: &Path,
    cwd: &Path,
    snapshot: &RenderSessionSnapshot,
) -> String {
    format!(
        "Restored render session snapshot\nmode: live TUI transcript hydrated; engine execution not resumed\n{}\npath: {}",
        render_session_snapshot_metadata(snapshot),
        render_session_snapshot_display_path(path, cwd)
    )
}

fn render_session_snapshot_display_path(path: &Path, cwd: &Path) -> String {
    path.strip_prefix(cwd).unwrap_or(path).display().to_string()
}

fn render_session_snapshot_metadata(snapshot: &RenderSessionSnapshot) -> String {
    format!(
        "version: {}\nsession: {}\ncurrent turn: {}\nlatest turn: {}\nrecords: {}\nraw events: {}\nnext record seq: {}\nnext turn seq: {}\nnext raw event seq: {}",
        snapshot.version,
        snapshot.session_id.as_deref().unwrap_or("-"),
        snapshot.current_turn_id.as_deref().unwrap_or("-"),
        snapshot.latest_turn_id.as_deref().unwrap_or("-"),
        snapshot.record_count(),
        snapshot.raw_event_count(),
        snapshot.next_render_record_seq,
        snapshot.next_render_turn_seq,
        snapshot.next_raw_engine_event_seq
    )
}

fn final_summary_message_data(model: &FinalSummaryModel) -> MessageData {
    final_summary_transcript_message(model)
}

fn restored_snapshot_ui_stage(
    snapshot: &RenderSessionSnapshot,
    restored_streaming: bool,
) -> UiStage {
    if restored_streaming {
        return UiStage::Thinking;
    }
    if let Some(summary) = snapshot.records.final_summaries.last() {
        return if summary.model.success {
            UiStage::Done
        } else {
            UiStage::Failed
        };
    }
    if snapshot
        .records
        .entries
        .iter()
        .rev()
        .any(|record| record.is_error)
    {
        return UiStage::Failed;
    }
    if snapshot.records.entries.is_empty() {
        UiStage::Idle
    } else {
        UiStage::Done
    }
}

fn string_field<'a>(
    obj: &'a serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Option<&'a str> {
    obj.get(key).and_then(|v| v.as_str())
}

fn permission_kind_for_tool(tool_name: &str, input: &serde_json::Value) -> PermissionKind {
    let obj = input.as_object();
    match tool_name {
        "Bash" => PermissionKind::Shell {
            command: obj
                .and_then(|o| string_field(o, "command"))
                .unwrap_or(tool_name)
                .to_string(),
        },
        "Read" => PermissionKind::FileRead {
            path: obj
                .and_then(|o| string_field(o, "file_path").or_else(|| string_field(o, "path")))
                .unwrap_or(tool_name)
                .to_string(),
        },
        "Write" => PermissionKind::FileWrite {
            path: obj
                .and_then(|o| string_field(o, "file_path").or_else(|| string_field(o, "path")))
                .unwrap_or(tool_name)
                .to_string(),
        },
        "Edit" | "MultiEdit" | "NotebookEdit" => PermissionKind::FileEdit {
            path: obj
                .and_then(|o| string_field(o, "file_path").or_else(|| string_field(o, "path")))
                .unwrap_or(tool_name)
                .to_string(),
        },
        "WebFetch" => PermissionKind::WebFetch {
            url: obj
                .and_then(|o| string_field(o, "url"))
                .unwrap_or(tool_name)
                .to_string(),
        },
        _ => PermissionKind::ToolUse {
            name: tool_name.to_string(),
        },
    }
}

/// Built-in slash commands surfaced via prompt suggestions.
///
/// Mirrors the command set referenced by `App::handle_command`. Kept in sync
/// manually because the full command registry lives in `mossen-commands`
/// and TUI consumes only this top-level subset for autocomplete display.
fn available_commands() -> Vec<Suggestion> {
    const COMMANDS: &[(&str, &str)] = &[
        ("help", "Show available commands"),
        ("clear", "Clear conversation"),
        ("exit", "Exit the application"),
        ("quit", "Exit the application"),
        ("model", "Change the active model"),
        ("compact", "Compact conversation history"),
        ("permissions", "Select permission mode or manage rules"),
        ("permission-mode", "Select the session permission mode"),
        (
            "debug-config",
            "Inspect redacted runtime and renderer configuration",
        ),
        ("diff", "Open the semantic diff review viewer"),
        ("files", "Inspect semantic file changes from this session"),
        ("changes", "Inspect semantic file changes from this session"),
        ("timeline", "Inspect structured render lifecycle events"),
        ("events", "Inspect structured render lifecycle events"),
        (
            "ps",
            "Inspect active turn processes and background activity",
        ),
        ("commands", "Inspect semantic command execution history"),
        ("errors", "Inspect semantic errors and failed commands"),
        (
            "results",
            "Inspect final task summaries and verification results",
        ),
        ("approvals", "Inspect approval requests and decisions"),
        ("raw", "Show the explicit raw transcript debug view"),
        (
            "render-snapshot",
            "Export or validate the render session snapshot",
        ),
        ("snapshot", "Export or validate the render session snapshot"),
        ("statusline", "Configure footer status-line items"),
        ("title", "Inspect or set the terminal session title"),
        ("resume", "Resume a previous session"),
        ("cost", "Show session cost"),
        ("config", "Open configuration"),
    ];
    COMMANDS
        .iter()
        .map(|(label, desc)| Suggestion {
            label: (*label).to_string(),
            description: Some((*desc).to_string()),
            kind: SuggestionKind::Command,
        })
        .collect()
}

fn theme_picker_items() -> Vec<String> {
    [
        "Dark",
        "Light",
        "Dark (high contrast)",
        "Light (high contrast)",
    ]
    .iter()
    .map(|item| (*item).to_string())
    .collect()
}

fn parse_theme_choice(raw: &str) -> Option<(crate::theme::ThemeName, &'static str)> {
    let normalized = raw.trim().to_ascii_lowercase().replace('_', "-");
    match normalized.as_str() {
        "dark" => Some((crate::theme::ThemeName::Dark, "dark")),
        "light" => Some((crate::theme::ThemeName::Light, "light")),
        "dark-high-contrast" | "dark-highcontrast" | "dark-contrast" => Some((
            crate::theme::ThemeName::DarkHighContrast,
            "dark-high-contrast",
        )),
        "light-high-contrast" | "light-highcontrast" | "light-contrast" => Some((
            crate::theme::ThemeName::LightHighContrast,
            "light-high-contrast",
        )),
        "dark (high contrast)" => Some((
            crate::theme::ThemeName::DarkHighContrast,
            "dark-high-contrast",
        )),
        "light (high contrast)" => Some((
            crate::theme::ThemeName::LightHighContrast,
            "light-high-contrast",
        )),
        _ => None,
    }
}

fn output_style_picker_items() -> Vec<String> {
    ["Default", "Concise", "Explanatory", "Code-first"]
        .iter()
        .map(|item| (*item).to_string())
        .collect()
}

fn output_style_choice(raw: &str) -> Option<(&'static str, &'static str, Option<&'static str>)> {
    let normalized = raw.trim().to_ascii_lowercase().replace('_', "-");
    match normalized.as_str() {
        "default" | "markdown" | "plain" => Some(("default", "Default", None)),
        "concise" | "short" => Some((
            "concise",
            "Concise",
            Some(
                "# Output style: Concise\n\nKeep responses tight: one sentence per idea, no preamble, no \"Sure!\" or \"Of course!\" lead-ins. If the answer is one line, the response is one line.",
            ),
        )),
        "explanatory" | "detailed" | "detail" => Some((
            "explanatory",
            "Explanatory",
            Some(
                "# Output style: Explanatory\n\nWalk the user through your reasoning. State assumptions, explain why one approach beats another, and call out edge cases. Bias toward depth over brevity, but stay focused.",
            ),
        )),
        "code-first" | "code-only" | "code" => Some((
            "code-first",
            "Code-first",
            Some(
                "# Output style: Code-first\n\nLead with the code that solves the problem. Place explanatory prose *after* the code block. If the answer is purely conceptual (no code), say so up front and skip the empty code fence.",
            ),
        )),
        _ => None,
    }
}

fn parse_export_request(args_raw: &str) -> (TranscriptExportFormat, Option<String>) {
    let args = args_raw.split_whitespace().collect::<Vec<_>>();
    let Some(first) = args.first().copied() else {
        return (TranscriptExportFormat::Markdown, None);
    };

    if let Some(format) = TranscriptExportFormat::from_arg(first) {
        let path = if args.len() > 1 {
            Some(args[1..].join(" "))
        } else {
            None
        };
        return (format, path);
    }

    let path = args.join(" ");
    let format = Path::new(&path)
        .extension()
        .and_then(|extension| extension.to_str())
        .and_then(TranscriptExportFormat::from_arg)
        .unwrap_or(TranscriptExportFormat::Markdown);
    (format, Some(path))
}

fn export_message_type(message: &MessageData) -> &'static str {
    match message.message_type {
        MessageType::User => "user",
        MessageType::Assistant => "assistant",
        MessageType::System => "system",
        MessageType::CommandOutput => "command_output",
        MessageType::Progress => "progress",
        MessageType::Attachment => "attachment",
        MessageType::ToolUse => "tool_use",
        MessageType::ToolResult => "tool_result",
        MessageType::SkillInvocation => "skill_invocation",
    }
}

fn export_message_label(message: &MessageData) -> &'static str {
    match message.message_type {
        MessageType::User => "User",
        MessageType::Assistant => "Assistant",
        MessageType::System => "System",
        MessageType::CommandOutput => "Command Output",
        MessageType::Progress => "Progress",
        MessageType::Attachment => "Attachment",
        MessageType::ToolUse => "Tool Use",
        MessageType::ToolResult => "Tool Result",
        MessageType::SkillInvocation => "Skill Invocation",
    }
}

fn export_message_content(message: &MessageData) -> &str {
    message
        .full_content
        .as_deref()
        .unwrap_or(message.content.as_str())
}

fn collect_directive_suggestions(
    directives: &[BoxedDirective],
    ctx: &CommandContext,
    entries: &mut Vec<SlashCommandInfo>,
    seen: &mut std::collections::HashSet<String>,
) {
    for directive in mossen_commands::visible_directives(directives, ctx) {
        let name = directive.name();
        if seen.insert(name.to_string()) {
            entries.push(SlashCommandInfo {
                name: name.to_string(),
                description: directive.description().to_string(),
                category: command_category(name).to_string(),
                aliases: directive
                    .aliases()
                    .iter()
                    .map(|alias| (*alias).to_string())
                    .collect(),
                argument_hint: directive.argument_hint().to_string(),
                kind: SlashCommandKind::Command,
            });
        }
    }
}

fn push_builtin_tui_suggestion(
    entries: &mut Vec<SlashCommandInfo>,
    seen: &mut std::collections::HashSet<String>,
    name: &str,
    description: &str,
) {
    if seen.insert(name.to_string()) {
        entries.push(SlashCommandInfo {
            name: name.to_string(),
            description: description.to_string(),
            category: command_category(name).to_string(),
            aliases: builtin_tui_command_aliases(name)
                .iter()
                .map(|alias| (*alias).to_string())
                .collect(),
            argument_hint: builtin_tui_command_argument_hint(name).to_string(),
            kind: SlashCommandKind::Command,
        });
    }
}

fn push_skill_suggestion(
    craft: &mossen_skills::CraftCommand,
    entries: &mut Vec<SlashCommandInfo>,
    seen: &mut std::collections::HashSet<String>,
) {
    let name = craft.name();
    if seen.insert(name.to_string()) {
        entries.push(SlashCommandInfo {
            name: name.to_string(),
            description: craft.base.description.clone(),
            category: "Skills".to_string(),
            aliases: Vec::new(),
            argument_hint: String::new(),
            kind: SlashCommandKind::Skill,
        });
    }
}

fn builtin_tui_command_aliases(name: &str) -> &'static [&'static str] {
    match name {
        "raw" => &["debug-raw"],
        "render-snapshot" => &["snapshot", "render-session"],
        "debug-config" => &["debugconfig"],
        "files" => &["changes", "changed-files"],
        "timeline" => &["events", "render-events"],
        "ps" => &["processes"],
        "commands" => &["cmds", "logs"],
        "errors" => &["errs", "failures"],
        "results" => &["summaries", "final-summary"],
        "approvals" => &["approval-history", "approval-log"],
        "statusline" => &["status-line"],
        "title" => &["session-title"],
        _ => &[],
    }
}

fn builtin_tui_command_argument_hint(name: &str) -> &'static str {
    match name {
        "compact" => "[run|plan|status|cancel]",
        "permissions" => "[mode|allow|deny|list|reset]",
        "permission-mode" => "[supervised|plan|accept-edits|full-auto|dont-ask]",
        "render-snapshot" => "[write|check]",
        "statusline" => "[preset|command|load|save]",
        "title" => "[new-title|reset]",
        "help" => "[command-or-filter]",
        _ => "",
    }
}

fn command_category(name: &str) -> &'static str {
    match name {
        "help" | "status" | "cost" | "files" | "changes" | "changed-files" | "diff"
        | "timeline" | "events" | "render-events" | "ps" | "processes" | "commands" | "cmds"
        | "logs" | "errors" | "errs" | "failures" | "approvals" | "approval-history"
        | "approval-log" | "version" | "doctor" | "usage" | "summary" | "summaries" | "results"
        | "final-summary" | "raw" | "debug-raw" | "debug-config" | "debugconfig" => "Info",
        "clear" | "reset" | "new" | "compact" | "resume" | "continue" | "session" | "exit"
        | "quit" | "rename" | "copy" | "export" | "title" | "session-title" | "render-snapshot"
        | "snapshot" | "render-session" => "Session",
        "model" | "theme" | "output-style" | "permissions" | "permission-mode"
        | "approval-mode" | "statusline" | "status-line" | "config" | "settings" | "memory"
        | "effort" | "lang" | "vim" | "color" => "Config",
        "mcp" | "plugin" | "skills" | "agents" | "tasks" | "hooks" => "MCP / Plugins",
        "review" | "commit" | "ship" | "branch" | "plan" | "project" | "feedback" | "advisor" => {
            "Code"
        }
        _ => "Other",
    }
}

fn permission_rule_subcommand(arg: &str) -> bool {
    matches!(
        arg.to_ascii_lowercase().as_str(),
        "list" | "show" | "allow" | "deny" | "reset" | "clear"
    )
}

fn permission_mode_selector_subcommand(arg: &str) -> bool {
    matches!(
        arg.to_ascii_lowercase().as_str(),
        "mode" | "modes" | "picker" | "select" | "approval-mode"
    )
}

fn slash_command_usage_label(entry: &SlashCommandInfo) -> String {
    if entry.argument_hint.trim().is_empty() {
        format!("/{}", entry.name)
    } else {
        format!("/{} {}", entry.name, entry.argument_hint.trim())
    }
}

fn slash_catalog_description(entry: &SlashCommandInfo) -> String {
    let mut description = entry.description.clone();
    let metadata = slash_catalog_metadata(entry);
    if !metadata.is_empty() {
        description.push_str(" · ");
        description.push_str(&metadata);
    }
    description
}

fn slash_catalog_metadata(entry: &SlashCommandInfo) -> String {
    let mut parts = Vec::new();
    if !entry.argument_hint.trim().is_empty() {
        parts.push(format!("args: {}", entry.argument_hint.trim()));
    }
    if !entry.aliases.is_empty() {
        parts.push(format!(
            "aliases: {}",
            entry
                .aliases
                .iter()
                .map(|alias| format!("/{alias}"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    parts.join(" · ")
}

fn slash_help_query_matches(entry: &SlashCommandInfo, query: &str) -> bool {
    let query = query.trim().to_ascii_lowercase();
    if query.is_empty() {
        return true;
    }
    entry.name.to_ascii_lowercase().contains(&query)
        || entry.description.to_ascii_lowercase().contains(&query)
        || entry.category.to_ascii_lowercase().contains(&query)
        || entry
            .aliases
            .iter()
            .any(|alias| alias.to_ascii_lowercase().contains(&query))
        || entry.argument_hint.to_ascii_lowercase().contains(&query)
}

fn help_scroll_max(total_rows: usize, viewport_rows: usize) -> usize {
    total_rows.saturating_sub(viewport_rows)
}

fn bounded_modal_height(area_height: u16, max_height: u16, min_height: u16) -> u16 {
    (area_height.saturating_sub(4))
        .min(max_height)
        .max(min_height)
        .min(area_height)
}

fn modal_content_rows(modal_height: u16, reserved_rows: u16) -> usize {
    modal_height
        .saturating_sub(2)
        .saturating_sub(reserved_rows)
        .max(1) as usize
}

fn list_modal_scroll_area(modal_area: Rect) -> Rect {
    let inner = Rect::new(
        modal_area.x.saturating_add(1),
        modal_area.y.saturating_add(1),
        modal_area.width.saturating_sub(2),
        modal_area.height.saturating_sub(2),
    );
    Rect::new(
        inner.x,
        inner.y.saturating_add(2),
        inner.width,
        inner.height.saturating_sub(3),
    )
}

fn help_dialog_modal_height(area_height: u16) -> u16 {
    bounded_modal_height(area_height, 26, 10)
}

fn help_dialog_content_viewport_rows(area_height: u16) -> usize {
    modal_content_rows(help_dialog_modal_height(area_height), 1)
}

fn raw_transcript_modal_height(area_height: u16) -> u16 {
    bounded_modal_height(area_height, 26, 10)
}

fn raw_transcript_content_viewport_rows(area_height: u16) -> usize {
    modal_content_rows(raw_transcript_modal_height(area_height), 1)
}

fn command_output_content_line_count(body: &str) -> usize {
    body.lines().count().max(1)
}

fn command_output_modal_height(area_height: u16) -> u16 {
    bounded_modal_height(area_height, 24, 8)
}

fn command_output_content_viewport_rows(area_height: u16) -> usize {
    modal_content_rows(command_output_modal_height(area_height), 1)
}

fn command_output_scroll_max(total_rows: usize, viewport_rows: usize) -> usize {
    total_rows.saturating_sub(viewport_rows.max(1))
}

fn diff_review_modal_height(area_height: u16) -> u16 {
    bounded_modal_height(area_height, 28, 10)
}

fn diff_review_content_viewport_rows(area_height: u16) -> usize {
    modal_content_rows(diff_review_modal_height(area_height), 1)
}

fn list_modal_content_viewport_rows(area_height: u16, max_height: u16) -> usize {
    modal_content_rows(bounded_modal_height(area_height, max_height, 10), 3)
}

fn file_changes_modal_height(area_height: u16) -> u16 {
    bounded_modal_height(area_height, 24, 10)
}

fn file_changes_content_viewport_rows(area_height: u16) -> usize {
    list_modal_content_viewport_rows(area_height, 24)
}

fn render_timeline_modal_height(area_height: u16) -> u16 {
    bounded_modal_height(area_height, 26, 10)
}

fn render_timeline_content_viewport_rows(area_height: u16) -> usize {
    list_modal_content_viewport_rows(area_height, 26)
}

fn process_list_modal_height(area_height: u16) -> u16 {
    bounded_modal_height(area_height, 26, 10)
}

fn process_list_content_viewport_rows(area_height: u16) -> usize {
    list_modal_content_viewport_rows(area_height, 26)
}

fn command_history_modal_height(area_height: u16) -> u16 {
    bounded_modal_height(area_height, 28, 10)
}

fn command_history_content_viewport_rows(area_height: u16) -> usize {
    list_modal_content_viewport_rows(area_height, 28)
}

fn error_history_modal_height(area_height: u16) -> u16 {
    bounded_modal_height(area_height, 28, 10)
}

fn error_history_content_viewport_rows(area_height: u16) -> usize {
    list_modal_content_viewport_rows(area_height, 28)
}

fn final_summary_history_modal_height(area_height: u16) -> u16 {
    bounded_modal_height(area_height, 28, 10)
}

fn final_summary_history_content_viewport_rows(area_height: u16) -> usize {
    list_modal_content_viewport_rows(area_height, 28)
}

fn approval_history_modal_height(area_height: u16) -> u16 {
    bounded_modal_height(area_height, 28, 10)
}

fn approval_history_content_viewport_rows(area_height: u16) -> usize {
    list_modal_content_viewport_rows(area_height, 28)
}

fn debug_config_modal_height(area_height: u16) -> u16 {
    bounded_modal_height(area_height, 30, 10)
}

fn debug_config_content_viewport_rows(area_height: u16) -> usize {
    modal_content_rows(debug_config_modal_height(area_height), 3)
}

fn scroll_selectable_modal_by_wheel(rows: usize, down: bool, mut scroll_one_row: impl FnMut(bool)) {
    for _ in 0..rows.max(1) {
        scroll_one_row(down);
    }
}

fn scrollbar_pointer_target_offset(row: u16, area: Rect, max_offset: usize) -> usize {
    let track_last = area.height.saturating_sub(1) as usize;
    let row = row.saturating_sub(area.y) as usize;
    if track_last == 0 {
        0
    } else {
        row.saturating_mul(max_offset)
            .saturating_add(track_last / 2)
            / track_last
    }
    .min(max_offset)
}

fn set_selectable_scroll_from_pointer(
    scroll: &mut usize,
    selected: &mut usize,
    total_rows: usize,
    viewport_rows: usize,
    target_offset: usize,
) {
    if total_rows == 0 {
        *scroll = 0;
        *selected = 0;
        return;
    }
    let max_offset = total_rows.saturating_sub(viewport_rows.max(1));
    let target_offset = target_offset.min(max_offset);
    *scroll = target_offset;
    if *selected < target_offset
        || *selected
            >= target_offset
                .saturating_add(viewport_rows.max(1))
                .min(total_rows)
    {
        *selected = target_offset.min(total_rows.saturating_sub(1));
    }
}

fn footer_preset_from_arg(arg: &str) -> Option<FooterPreset> {
    match arg {
        "minimal" | "compact" => Some(FooterPreset::Minimal),
        "focused" | "focus" | "codex" => Some(FooterPreset::Focused),
        "default" | "standard" => Some(FooterPreset::Standard),
        "full" => Some(FooterPreset::Full),
        _ => None,
    }
}

fn slash_entry_match_score(entry: &SlashCommandInfo, query: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    let name = entry.name.to_lowercase();
    if name.starts_with(query) {
        return Some(0);
    }
    for alias in &entry.aliases {
        let alias = alias.to_lowercase();
        if alias.starts_with(query) {
            return Some(10);
        }
    }
    if let Some(pos) = name.find(query) {
        return Some(20 + pos as i32);
    }
    for alias in &entry.aliases {
        let alias = alias.to_lowercase();
        if let Some(pos) = alias.find(query) {
            return Some(30 + pos as i32);
        }
    }
    if fuzzy_subsequence(&name, query) {
        return Some(60 + name.len() as i32);
    }
    for alias in &entry.aliases {
        let alias = alias.to_lowercase();
        if fuzzy_subsequence(&alias, query) {
            return Some(70 + alias.len() as i32);
        }
    }
    let hint = entry.argument_hint.to_lowercase();
    if let Some(pos) = hint.find(query) {
        return Some(90 + pos as i32);
    }
    let description = entry.description.to_lowercase();
    if let Some(pos) = description.find(query) {
        return Some(120 + pos as i32);
    }
    entry
        .category
        .to_lowercase()
        .find(query)
        .map(|pos| 140 + pos as i32)
}

fn fuzzy_subsequence(haystack: &str, needle: &str) -> bool {
    let mut chars = needle.chars();
    let mut current = chars.next();
    if current.is_none() {
        return true;
    }
    for ch in haystack.chars() {
        if Some(ch) == current {
            current = chars.next();
            if current.is_none() {
                return true;
            }
        }
    }
    false
}

fn mcp_transport_label(config: &mossen_agent::mcp::types::McpServerConfig) -> &'static str {
    match config {
        mossen_agent::mcp::types::McpServerConfig::Stdio { .. } => "stdio",
        mossen_agent::mcp::types::McpServerConfig::Sse { .. } => "sse",
        mossen_agent::mcp::types::McpServerConfig::SseIde { .. } => "sse-ide",
        mossen_agent::mcp::types::McpServerConfig::Http { .. } => "http",
        mossen_agent::mcp::types::McpServerConfig::Ws { .. } => "ws",
        mossen_agent::mcp::types::McpServerConfig::WsIde { .. } => "ws-ide",
        mossen_agent::mcp::types::McpServerConfig::Sdk { .. } => "sdk",
        mossen_agent::mcp::types::McpServerConfig::HostedProxy { .. } => "hosted",
    }
}

fn mcp_runtime_state(
    state: mossen_agent::mcp::runtime_status::RuntimeMcpConnectionState,
) -> McpConnectionState {
    match state {
        mossen_agent::mcp::runtime_status::RuntimeMcpConnectionState::Connected => {
            McpConnectionState::Connected
        }
        mossen_agent::mcp::runtime_status::RuntimeMcpConnectionState::Pending => {
            McpConnectionState::Pending
        }
        mossen_agent::mcp::runtime_status::RuntimeMcpConnectionState::Failed => {
            McpConnectionState::Failed
        }
        mossen_agent::mcp::runtime_status::RuntimeMcpConnectionState::NeedsAuth => {
            McpConnectionState::NeedsAuth
        }
        mossen_agent::mcp::runtime_status::RuntimeMcpConnectionState::Disabled => {
            McpConnectionState::Disabled
        }
    }
}

fn render_render_error_frame(frame: &mut Frame, theme: &Theme, message: &str) {
    use ratatui::style::{Modifier, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};

    let area = frame.area();
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.error))
        .title(Span::styled(
            " Render error ",
            Style::default()
                .fg(theme.error)
                .add_modifier(Modifier::BOLD),
        ));
    let lines = vec![
        Line::from(Span::styled(
            "Rendering failed, but the session is still alive.",
            Style::default().fg(theme.text),
        )),
        Line::from(""),
        Line::from(Span::styled(
            message.to_string(),
            Style::default().fg(theme.error),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Resize or continue typing; this frame is isolated from the agent loop.",
            Style::default()
                .fg(theme.text_dim)
                .add_modifier(Modifier::ITALIC),
        )),
    ];
    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn panic_payload_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        return (*message).to_string();
    }
    if let Some(message) = payload.downcast_ref::<String>() {
        return message.clone();
    }
    "unknown render panic".to_string()
}

fn memory_entry_for_path(
    cwd: &std::path::Path,
    path: &std::path::Path,
    category: &str,
) -> crate::widgets::panels::MemoryEntry {
    let title = path
        .strip_prefix(cwd)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string();
    let bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    crate::widgets::panels::MemoryEntry {
        title,
        category: category.to_string(),
        preview: format!("{} bytes", bytes),
    }
}

fn truncate_plain(text: &str, max_chars: usize) -> String {
    mossen_utils::string_utils::truncate_chars(text, max_chars)
}

fn push_raw_line(lines: &mut Vec<String>, label: &str, value: &str) {
    lines.push(format!("  {label}: {}", raw_debug_preview(value)));
}

fn raw_debug_preview(value: &str) -> String {
    let escaped = value
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t");
    truncate_plain(&escaped, 900)
}

fn fallback_search_preview(message: &MessageData) -> String {
    if matches!(
        message.message_type,
        MessageType::ToolUse | MessageType::ToolResult
    ) {
        let tool_name = message
            .tool_name
            .as_deref()
            .map(display_tool_name)
            .unwrap_or_else(|| "Tool".to_string());
        return tool_name;
    }

    message
        .content
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !is_search_protocol_noise_line(line))
        .map(|line| truncate_plain(line, 160))
        .unwrap_or_else(|| "(empty)".to_string())
}

fn is_search_protocol_noise_line(line: &str) -> bool {
    matches!(
        line.trim(),
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

fn truncate_display_width(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut width = 0usize;
    for grapheme in text.graphemes(true) {
        let next = UnicodeWidthStr::width(grapheme);
        if width + next > max_width {
            break;
        }
        width += next;
        out.push_str(grapheme);
    }
    out
}

fn pad_display_width(text: &str, target_width: usize) -> String {
    let mut out = truncate_display_width(text, target_width);
    let width = UnicodeWidthStr::width(out.as_str());
    out.extend(std::iter::repeat(' ').take(target_width.saturating_sub(width)));
    out
}

fn approval_decision_kind_from_permission(decision: PermissionAction) -> ApprovalDecisionKind {
    match decision {
        PermissionAction::Allow => ApprovalDecisionKind::Allowed,
        PermissionAction::AllowAlways => ApprovalDecisionKind::AlwaysAllowed,
        PermissionAction::EditCommand => ApprovalDecisionKind::Cancelled,
        PermissionAction::Deny => ApprovalDecisionKind::Denied,
    }
}

fn render_approval_action_from_permission(decision: PermissionAction) -> RenderApprovalAction {
    match decision {
        PermissionAction::Allow => RenderApprovalAction::Allow,
        PermissionAction::AllowAlways => RenderApprovalAction::AlwaysAllow,
        PermissionAction::EditCommand => RenderApprovalAction::EditCommand,
        PermissionAction::Deny => RenderApprovalAction::Deny,
    }
}

fn approval_risk_from_permission_kind(kind: &PermissionKind) -> ApprovalRiskLevel {
    match kind {
        PermissionKind::FileEdit { .. }
        | PermissionKind::FileWrite { .. }
        | PermissionKind::Notebook { .. } => ApprovalRiskLevel::High,
        PermissionKind::Shell { .. }
        | PermissionKind::PowerShell { .. }
        | PermissionKind::WebFetch { .. }
        | PermissionKind::ComputerUse
        | PermissionKind::Filesystem { .. }
        | PermissionKind::ToolUse { .. } => ApprovalRiskLevel::Medium,
        PermissionKind::FileRead { .. }
        | PermissionKind::Skill { .. }
        | PermissionKind::UserQuestion { .. }
        | PermissionKind::PlanMode { .. } => ApprovalRiskLevel::Low,
    }
}

fn ui_stage_from_terminal(terminal: &str) -> UiStage {
    UiStage::from_terminal(terminal)
}

fn last_tool_anchor_block_id_in_messages(
    messages: &[MessageData],
    record_ids: &HashMap<usize, String>,
    parent_ids: &HashMap<usize, String>,
    tool_name: &str,
) -> Option<String> {
    let records = TranscriptRecords::from_messages_and_decisions_with_record_metadata(
        messages,
        &[],
        record_ids,
        parent_ids,
    );
    RenderTranscript::from_records(&records)
        .blocks
        .iter()
        .rev()
        .find(|block| {
            block
                .tool
                .as_ref()
                .is_some_and(|tool| tool.name == tool_name)
        })
        .map(|block| block.id.clone())
}

fn last_mcp_tool_anchor_block_id_in_messages(
    messages: &[MessageData],
    record_ids: &HashMap<usize, String>,
    parent_ids: &HashMap<usize, String>,
    server_name: &str,
) -> Option<String> {
    let records = TranscriptRecords::from_messages_and_decisions_with_record_metadata(
        messages,
        &[],
        record_ids,
        parent_ids,
    );
    RenderTranscript::from_records(&records)
        .blocks
        .iter()
        .rev()
        .find(|block| {
            block
                .tool
                .as_ref()
                .is_some_and(|tool| mcp_tool_name_matches_server(&tool.name, server_name))
        })
        .map(|block| block.id.clone())
}

fn mcp_tool_name_matches_server(tool_name: &str, server_name: &str) -> bool {
    let server_name = server_name.trim();
    if server_name.is_empty() {
        return false;
    }
    mcp_tool_server_name(tool_name).is_some_and(|server| server.eq_ignore_ascii_case(server_name))
}

fn mcp_tool_server_name(tool_name: &str) -> Option<&str> {
    let name = tool_name.strip_prefix("mcp__").unwrap_or(tool_name);
    let (server, tool) = name.split_once("__")?;
    (!server.is_empty() && !tool.is_empty()).then_some(server)
}

fn render_block_anchor_ids(block: crate::render_model::RenderBlock) -> Vec<String> {
    let mut ids = vec![block.id.clone()];
    if block.tool.is_some() && block.source_indices.len() == 2 {
        ids.push(format!(
            "tool-{}-{}",
            block.source_indices[0], block.source_indices[1]
        ));
    }
    ids
}

fn shift_render_record_overrides(overrides: &mut HashMap<usize, String>, removed_index: usize) {
    if overrides.is_empty() {
        return;
    }

    let mut shifted = HashMap::with_capacity(overrides.len());
    for (index, id) in overrides.drain() {
        if index < removed_index {
            shifted.insert(index, id);
        } else if index > removed_index {
            shifted.insert(index - 1, id);
        }
    }
    *overrides = shifted;
}

fn block_on_current_runtime<F: std::future::Future>(future: F) -> F::Output {
    if tokio::runtime::Handle::try_current().is_ok() {
        futures::executor::block_on(future)
    } else {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("create fallback tokio runtime for TUI command")
            .block_on(future)
    }
}

// Provide a small modal smoke-test entry point that exercises the
// permission-prompt overlay surface (used during integration testing).
#[doc(hidden)]
pub fn __debug_open_permission_modal(app: &mut App) {
    app.active_modal = ActiveModal::PermissionRequest(PermissionPromptState::new(
        PermissionKind::FileRead {
            path: "/tmp/debug".to_string(),
        },
        "DebugTool",
    ));
}

/// Peel `<think>...</think>` reasoning blocks out of a streamed assistant
/// text buffer, returning `(thinking, content)`. Multiple `<think>` blocks
/// are concatenated (newline-joined) into the same thinking buffer. An
/// unclosed `<think>` (still streaming) routes the trailing tail to
/// thinking so it can render live; once `</think>` arrives the split
/// resolves cleanly on the next chunk.
///
/// Returns `(None, buf)` when the buffer contains no `<think>` markers — so
/// non-reasoning backends pay no cost beyond a single `str::find`.
pub fn split_thinking_and_content(buf: &str) -> (Option<String>, String) {
    if !buf.contains("<think>") {
        return (None, buf.to_string());
    }

    let mut thinking_parts: Vec<&str> = Vec::new();
    let mut content_parts: Vec<&str> = Vec::new();
    let mut rest = buf;

    loop {
        match rest.find("<think>") {
            Some(open) => {
                if open > 0 {
                    content_parts.push(&rest[..open]);
                }
                let after_open = &rest[open + "<think>".len()..];
                match after_open.find("</think>") {
                    Some(close) => {
                        thinking_parts.push(&after_open[..close]);
                        rest = &after_open[close + "</think>".len()..];
                    }
                    None => {
                        // Open tag without close — still streaming. The
                        // remainder of the buffer is all thinking for now.
                        thinking_parts.push(after_open);
                        break;
                    }
                }
            }
            None => {
                content_parts.push(rest);
                break;
            }
        }
    }

    let thinking_joined = thinking_parts.join("\n").trim().to_string();
    let content_joined = content_parts.join("").trim_start().to_string();
    let thinking = if thinking_joined.is_empty() {
        None
    } else {
        Some(thinking_joined)
    };
    (thinking, content_joined)
}

#[cfg(test)]
mod engine_stream_tests {
    use super::{
        block_on_current_runtime, copy_response_index, ActiveModal, App,
        FooterConfigPersistenceStatus, HelpDialogState, McpRuntimeReloadResult, PickerKind,
        RenderSessionSnapshot, RenderSnapshotStartupRestoreStatus, SessionPermissionGate,
        SessionPermissionRules, RENDER_SESSION_SNAPSHOT_DIR, RENDER_STATUSLINE_CONFIG_PATH,
    };
    use crate::approval_state::{PermissionKind, PermissionPromptState, ToolUseConfirm};
    use crate::event::{AppEvent, EventBus};
    use crate::message_model::{MessageData, MessageType};
    use crate::render_events::{RenderEvent, RenderEventKind};
    use crate::render_model::{
        ActivityPanelSeverity, ApprovalAction as RenderApprovalAction, ApprovalDecisionKind,
        ApprovalDecisionModel, ApprovalRiskLevel, BlockingKind, DebugConfigRenderModel,
        ExternalStatusLineCommandConfig, FooterItem, FooterRenderConfig, RenderBlockKind,
        RenderNode, RenderTranscript, StatusRowLevel, StatusSectionRenderModel, ToolPhase,
    };
    use crate::state::{RenderActivity, SlashCommandInfo, SlashCommandKind, TurnState, UiStage};
    use crate::theme::Theme;
    use crate::widgets::idle_return::IdleReturnDialogState;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
    use mossen_agent::services::config::{facade, types::ConfigOverrideScope};
    use mossen_agent::types::EffortLevel;
    use mossen_agent::types::{ContentDelta, PermissionRequest, SdkMessage, StreamEventData};
    use mossen_types::{
        AssistantMessage, ContentBlock, Message, Role, TextBlock, ToolDefinition, ToolInputSchema,
        ToolUseBlock,
    };
    use ratatui::{
        backend::{Backend, ClearType, TestBackend, WindowSize},
        buffer::{Buffer, Cell},
        layout::{Position, Size},
        Terminal,
    };
    use std::collections::HashMap;
    use std::io;
    use std::sync::Arc;
    use std::time::Duration;

    const MODEL_ENV_KEYS: &[&str] = &[
        "MOSSEN_CODE_USE_CUSTOM_BACKEND",
        "MOSSEN_CODE_CUSTOM_BACKEND_PROTOCOL",
        "MOSSEN_CODE_CUSTOM_NAME",
        "MOSSEN_CODE_CUSTOM_BASE_URL",
        "MOSSEN_CODE_CUSTOM_API_KEY",
        "MOSSEN_CODE_CUSTOM_MODEL",
        "MOSSEN_API_BASE_URL",
        "MOSSEN_API_KEY",
    ];

    struct EnvGuard(Vec<(&'static str, Option<String>)>);

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (key, value) in self.0.drain(..) {
                match value {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }

    fn model_config_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
        LOCK.get_or_init(|| std::sync::Mutex::new(()))
            .lock()
            .expect("model config lock")
    }

    fn skill_reload_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
        LOCK.get_or_init(|| std::sync::Mutex::new(()))
            .lock()
            .expect("skill reload lock")
    }

    fn isolate_model_env() -> EnvGuard {
        let guard = EnvGuard(
            MODEL_ENV_KEYS
                .iter()
                .map(|key| (*key, std::env::var(key).ok()))
                .collect(),
        );
        for key in MODEL_ENV_KEYS {
            std::env::remove_var(key);
        }
        guard
    }

    fn seed_model_profiles() {
        facade::reset_facade_for_testing();
        facade::set_mossen_config_override(
            "mossen.profiles",
            serde_json::json!({
                "fast": {
                    "provider": "openai-compatible",
                    "baseURL": "https://api.example.com/v1",
                    "model": "example-fast",
                    "apiKey": "sk-test-fast-secret"
                },
                "large": {
                    "provider": "openai-responses",
                    "baseURL": "https://responses.example.com/v1",
                    "model": "example-large",
                    "apiKey": "sk-test-large-secret"
                }
            }),
            ConfigOverrideScope::Override,
        );
        facade::set_mossen_config_override(
            "mossen.activeProfile",
            serde_json::Value::String("large".to_string()),
            ConfigOverrideScope::Override,
        );
    }

    fn buffer_text(buf: &Buffer) -> String {
        let mut out = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                out.push_str(buf.content[buf.index_of(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    struct FailingDrawBackend {
        size: Size,
        cursor: Position,
    }

    impl FailingDrawBackend {
        fn new(width: u16, height: u16) -> Self {
            Self {
                size: Size::new(width, height),
                cursor: Position::ORIGIN,
            }
        }
    }

    impl Backend for FailingDrawBackend {
        fn draw<'a, I>(&mut self, _content: I) -> io::Result<()>
        where
            I: Iterator<Item = (u16, u16, &'a Cell)>,
        {
            Err(io::Error::other("synthetic terminal draw failure"))
        }

        fn hide_cursor(&mut self) -> io::Result<()> {
            Ok(())
        }

        fn show_cursor(&mut self) -> io::Result<()> {
            Ok(())
        }

        fn get_cursor_position(&mut self) -> io::Result<Position> {
            Ok(self.cursor)
        }

        fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> io::Result<()> {
            self.cursor = position.into();
            Ok(())
        }

        fn clear(&mut self) -> io::Result<()> {
            Ok(())
        }

        fn clear_region(&mut self, _clear_type: ClearType) -> io::Result<()> {
            Ok(())
        }

        fn size(&self) -> io::Result<Size> {
            Ok(self.size)
        }

        fn window_size(&mut self) -> io::Result<WindowSize> {
            Ok(WindowSize {
                columns_rows: self.size,
                pixels: Size::ZERO,
            })
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn mouse_event(kind: MouseEventKind) -> MouseEvent {
        MouseEvent {
            kind,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        }
    }

    #[test]
    fn extra_tool_definitions_are_forwarded_to_engine_params() {
        let mcp_tool = ToolDefinition {
            name: "mcp__dev__ping".to_string(),
            description: "Ping dev MCP server".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: None,
                required: None,
                extra: HashMap::new(),
            },
            cache_control: None,
        };
        let mut app = App::new().with_extra_tool_definitions(vec![mcp_tool.clone()]);

        app.handle_submit("call the MCP tool".to_string());

        let params = app
            .pending_submit
            .as_ref()
            .expect("submit should create engine params");
        assert!(
            params.tools.iter().any(|tool| tool.name == mcp_tool.name),
            "MCP definitions should be model-visible even though execution bypasses the built-in registry"
        );
    }

    #[test]
    fn reload_plugins_refreshes_mcp_tool_definitions_for_next_turn() {
        let mcp_tool = ToolDefinition {
            name: "mcp__dev__ping".to_string(),
            description: "Ping dev MCP server".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: None,
                required: None,
                extra: HashMap::new(),
            },
            cache_control: None,
        };
        let callback_tool = mcp_tool.clone();
        let mut app = App::new().with_mcp_reload_callback(Arc::new(move || {
            let callback_tool = callback_tool.clone();
            Box::pin(async move {
                Ok(McpRuntimeReloadResult {
                    tool_definitions: vec![callback_tool],
                    server_count: 1,
                    connected_count: 1,
                })
            })
        }));
        app.directives = Some(Arc::new(mossen_commands::all_directives()));

        app.handle_command("reload-plugins");

        assert!(app
            .extra_tool_definitions
            .iter()
            .any(|tool| tool.name == mcp_tool.name));
        app.handle_submit("call MCP".to_string());
        let params = app
            .pending_submit
            .as_ref()
            .expect("submit should create engine params");
        assert!(params.tools.iter().any(|tool| tool.name == mcp_tool.name));
        assert!(params
            .tools
            .iter()
            .any(|tool| tool.name == "ListMcpResources"));
        let transcript = app
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            transcript.contains("MCP runtime: 1/1 server(s), 1 tool(s) visible next turn."),
            "{transcript}"
        );
    }

    #[test]
    fn startup_hook_context_drains_into_first_prompt() {
        let mut app = App::new().with_startup_hook_messages(vec![
            mossen_utils::session_start::HookResultMessage {
                content: "repo startup context".to_string(),
                hook_name: "SessionStart".to_string(),
                message_type: "hook_additional_context".to_string(),
            },
        ]);

        assert_eq!(app.startup_additional_blocks.len(), 1);
        assert!(app.messages.iter().any(|message| {
            message.message_type == MessageType::System
                && message.content.contains("supplied startup context")
        }));

        app.handle_submit("run first task".to_string());

        let params = app
            .pending_submit
            .as_ref()
            .expect("submit should create engine params");
        assert_eq!(params.additional_blocks.len(), 1);
        match &params.additional_blocks[0] {
            ContentBlock::Text(block) => assert_eq!(block.text, "repo startup context"),
            _ => panic!("expected startup context text block"),
        }
        assert!(app.startup_additional_blocks.is_empty());
    }

    #[test]
    fn render_panic_boundary_shows_error_frame_instead_of_unwinding() {
        let theme = Theme::default();
        let backend = TestBackend::new(72, 8);
        let mut terminal = Terminal::new(backend).expect("test backend should initialize");
        let mut result = None;

        terminal
            .draw(|frame| {
                result = Some(App::render_with_panic_boundary(frame, &theme, |_| {
                    panic!("synthetic render failure");
                }));
            })
            .expect("panic boundary should keep terminal draw alive");

        let result = result.expect("render boundary should return a result");
        assert!(result.is_err());
        let rendered = buffer_text(terminal.backend().buffer());
        assert!(rendered.contains("Render error"), "{rendered}");
        assert!(rendered.contains("synthetic render failure"), "{rendered}");
        assert!(rendered.contains("session is still alive"), "{rendered}");
    }

    #[test]
    fn render_transcript_cache_reuses_model_until_transcript_changes() {
        let mut app = App::new();
        app.push_system_message("first render block", false);

        let first = app.render_transcript_model();
        assert_eq!(first.blocks.len(), 1);
        assert_eq!(
            app.render_transcript_cache_stats(),
            super::RenderTranscriptCacheStats {
                revision: 1,
                cached: true,
                hits: 0,
                misses: 1,
            }
        );

        let second = app.render_transcript_model();
        assert_eq!(second, first);
        assert_eq!(
            app.render_transcript_cache_stats(),
            super::RenderTranscriptCacheStats {
                revision: 1,
                cached: true,
                hits: 1,
                misses: 1,
            }
        );

        app.push_system_message("second render block", false);
        assert_eq!(
            app.render_transcript_cache_stats(),
            super::RenderTranscriptCacheStats {
                revision: 2,
                cached: false,
                hits: 1,
                misses: 1,
            }
        );

        let third = app.render_transcript_model();
        assert_eq!(third.blocks.len(), 2);
        assert_eq!(
            app.render_transcript_cache_stats(),
            super::RenderTranscriptCacheStats {
                revision: 2,
                cached: true,
                hits: 1,
                misses: 2,
            }
        );
    }

    #[test]
    fn repeated_long_session_frames_reuse_transcript_cache_for_visible_state_changes() {
        let mut app = App::new();
        app.messages = (0..900)
            .map(|index| MessageData {
                message_type: MessageType::Assistant,
                content: format!(
                    "long cache row {index:04}: repeated render frames must not rebuild the transcript model."
                ),
                timestamp: None,
                is_streaming: false,
                tool_name: None,
                is_error: false,
                thinking: None,
                thinking_completed_at: None,
                full_content: None,
                expanded: false,
            })
            .collect();

        let mut terminal =
            Terminal::new(TestBackend::new(120, 32)).expect("test backend should initialize");
        terminal
            .draw(|frame| app.render_for_test(frame))
            .expect("first long-session frame should draw");
        let first = app.render_transcript_cache_stats();
        assert!(first.cached);
        assert_eq!(first.misses, 1);
        assert_eq!(first.hits, 0);

        app.state.ui_stage = UiStage::RunningCommand;
        terminal
            .draw(|frame| app.render_for_test(frame))
            .expect("status-only long-session frame should draw");
        let status_only = app.render_transcript_cache_stats();
        assert_eq!(status_only.revision, first.revision);
        assert_eq!(
            status_only.misses, first.misses,
            "visible status changes must not rebuild the long transcript model"
        );
        assert_eq!(status_only.hits, first.hits + 1);

        app.prompt.input.insert_str("visible prompt edit");
        terminal
            .draw(|frame| app.render_for_test(frame))
            .expect("prompt-only long-session frame should draw");
        let prompt_only = app.render_transcript_cache_stats();
        assert_eq!(prompt_only.revision, first.revision);
        assert_eq!(
            prompt_only.misses, first.misses,
            "prompt repaint must keep using the cached long transcript model"
        );
        assert_eq!(prompt_only.hits, first.hits + 2);

        app.push_system_message("long cache transcript invalidator", false);
        terminal
            .draw(|frame| app.render_for_test(frame))
            .expect("changed long-session frame should draw");
        let changed = app.render_transcript_cache_stats();
        assert!(changed.revision > prompt_only.revision);
        assert_eq!(
            changed.misses,
            prompt_only.misses + 1,
            "real transcript changes must invalidate the cache exactly once"
        );
        assert!(changed.cached);
    }

    #[test]
    fn render_frame_scheduler_skips_idle_and_paces_active_animation() {
        let mut app = App::new();
        assert!(app.should_render_frame_for_run());

        app.note_render_frame_drawn();
        assert!(!app.should_render_frame_for_run());

        app.note_render_frame_skipped();
        assert_eq!(app.render_frame_scheduler_stats().skipped, 1);

        app.mark_render_dirty();
        assert!(app.should_render_frame_for_run());
        app.note_render_frame_drawn();
        assert!(!app.should_render_frame_for_run());

        app.state.is_streaming = true;
        assert!(!app.should_render_frame_for_run());
        app.render_last_frame_at = Some(
            std::time::Instant::now()
                .checked_sub(super::ACTIVE_RENDER_FRAME_INTERVAL + Duration::from_millis(1))
                .expect("test instant should allow subtraction"),
        );

        assert!(app.should_render_frame_for_run());
        let stats = app.render_frame_scheduler_stats();
        assert!(stats.active_animation);
        assert_eq!(stats.drawn, 2);
        assert_eq!(
            stats.active_frame_interval_ms,
            super::ACTIVE_RENDER_FRAME_INTERVAL.as_millis()
        );
    }

    #[test]
    fn render_frame_scheduler_adapts_after_slow_frame() {
        let mut app = App::new();
        app.note_render_frame_drawn_with_duration(Duration::from_millis(120));
        app.state.is_streaming = true;

        app.render_last_frame_at = Some(
            std::time::Instant::now()
                .checked_sub(super::ACTIVE_RENDER_FRAME_INTERVAL + Duration::from_millis(1))
                .expect("test instant should allow subtraction"),
        );
        assert!(
            !app.should_render_frame_for_run(),
            "slow frames should stretch the active animation interval before redrawing"
        );

        app.render_last_frame_at = Some(
            std::time::Instant::now()
                .checked_sub(Duration::from_millis(241))
                .expect("test instant should allow subtraction"),
        );
        assert!(app.should_render_frame_for_run());

        let stats = app.render_frame_scheduler_stats();
        assert_eq!(stats.last_frame_duration_ms, Some(120));
        assert_eq!(stats.max_frame_duration_ms, 120);
        assert_eq!(stats.avg_frame_duration_ms, Some(120));
        assert_eq!(stats.active_frame_interval_ms, 240);
    }

    #[test]
    fn next_render_frame_deadline_prefers_throttled_streaming_updates() {
        let mut app = App::new();
        app.handle_engine_message(SdkMessage::StreamEvent {
            event: StreamEventData::MessageStart,
            task_id: None,
        });
        app.note_render_frame_drawn();
        app.render_last_frame_at = Some(std::time::Instant::now());
        let (_tx, rx) = tokio::sync::mpsc::channel::<SdkMessage>(1);
        app.engine_rx = Some(rx);

        app.handle_engine_message(SdkMessage::StreamEvent {
            event: StreamEventData::ContentBlockDelta {
                index: 0,
                delta: ContentDelta::TextDelta {
                    text: "deadline delta".to_string(),
                },
            },
            task_id: None,
        });

        let next_due = app
            .next_render_frame_due_at()
            .expect("throttled streaming update should schedule a frame deadline");
        let remaining_ms = next_due
            .saturating_duration_since(std::time::Instant::now())
            .as_millis();
        assert!(
            remaining_ms <= crate::render_events::STREAM_THROTTLE_MS as u128,
            "next frame should use the stream throttle deadline, got {remaining_ms}ms"
        );
        assert!(
            remaining_ms < super::ACTIVE_RENDER_FRAME_INTERVAL.as_millis(),
            "throttled transcript updates should not wait for the active animation interval"
        );

        let stats = app.render_frame_scheduler_stats();
        assert!(stats.throttled_due_in_ms.is_some());
        assert!(stats.next_frame_due_in_ms.is_some());
    }

    #[test]
    fn streaming_text_delta_dirty_mark_is_throttled() {
        let mut app = App::new();
        app.handle_engine_message(SdkMessage::StreamEvent {
            event: StreamEventData::MessageStart,
            task_id: None,
        });
        assert!(app.render_frame_scheduler_stats().dirty);

        app.note_render_frame_drawn();
        app.render_last_frame_at = Some(std::time::Instant::now());
        app.handle_engine_message(SdkMessage::StreamEvent {
            event: StreamEventData::ContentBlockDelta {
                index: 0,
                delta: ContentDelta::TextDelta {
                    text: "fast delta".to_string(),
                },
            },
            task_id: None,
        });
        assert!(
            !app.render_frame_scheduler_stats().dirty,
            "a fast streaming text delta should update the transcript without scheduling an immediate redraw"
        );
        assert_eq!(app.messages.len(), 1);
        assert!(app.messages[0].content.contains("fast delta"));
        assert!(
            app.render_frame_scheduler_stats()
                .throttled_due_in_ms
                .is_some(),
            "a suppressed streaming redraw should leave a scheduled throttle deadline"
        );

        app.render_last_frame_at = Some(
            std::time::Instant::now()
                .checked_sub(Duration::from_millis(
                    crate::render_events::STREAM_THROTTLE_MS + 1,
                ))
                .expect("test instant should allow subtraction"),
        );
        app.handle_engine_message(SdkMessage::StreamEvent {
            event: StreamEventData::ContentBlockDelta {
                index: 0,
                delta: ContentDelta::TextDelta {
                    text: " after throttle".to_string(),
                },
            },
            task_id: None,
        });
        assert!(
            app.render_frame_scheduler_stats().dirty,
            "streaming text should schedule a redraw once the throttle interval has elapsed"
        );
        assert!(app.messages[0].content.contains("after throttle"));
    }

    #[test]
    fn no_visible_streaming_delta_does_not_schedule_frame() {
        let mut app = App::new();
        app.handle_engine_message(SdkMessage::StreamEvent {
            event: StreamEventData::MessageStart,
            task_id: None,
        });
        app.note_render_frame_drawn();
        app.render_last_frame_at = Some(std::time::Instant::now());

        app.handle_engine_message(SdkMessage::StreamEvent {
            event: StreamEventData::ContentBlockDelta {
                index: 0,
                delta: ContentDelta::TextDelta {
                    text: String::new(),
                },
            },
            task_id: None,
        });
        assert!(
            !app.render_frame_scheduler_stats().dirty,
            "empty text delta should not schedule a visible redraw"
        );
        assert!(
            app.render_frame_scheduler_stats()
                .throttled_due_in_ms
                .is_none(),
            "empty text delta should not leave a throttled redraw deadline"
        );

        app.handle_engine_message(SdkMessage::StreamEvent {
            event: StreamEventData::ContentBlockDelta {
                index: 0,
                delta: ContentDelta::ThinkingDelta {
                    thinking: String::new(),
                },
            },
            task_id: None,
        });
        assert!(
            !app.render_frame_scheduler_stats().dirty,
            "empty thinking delta should not schedule a visible redraw"
        );

        app.handle_engine_message(SdkMessage::StreamEvent {
            event: StreamEventData::ContentBlockDelta {
                index: 0,
                delta: ContentDelta::TextDelta {
                    text: "<think>".to_string(),
                },
            },
            task_id: None,
        });
        assert!(
            !app.render_frame_scheduler_stats().dirty,
            "streaming parser boundary with no visible text should not schedule a redraw"
        );
        assert!(
            app.render_frame_scheduler_stats()
                .throttled_due_in_ms
                .is_none(),
            "no-visible parser boundary should not leave a redraw deadline"
        );

        app.handle_engine_message(SdkMessage::StreamEvent {
            event: StreamEventData::ContentBlockDelta {
                index: 0,
                delta: ContentDelta::TextDelta {
                    text: "visible reasoning".to_string(),
                },
            },
            task_id: None,
        });
        assert_eq!(
            app.messages
                .first()
                .and_then(|message| message.thinking.as_deref()),
            Some("visible reasoning")
        );
        assert!(
            app.render_frame_scheduler_stats()
                .throttled_due_in_ms
                .is_some(),
            "first visible reasoning text should still schedule a paced redraw"
        );
    }

    #[test]
    fn throttled_streaming_delta_gets_paced_followup_frame() {
        let mut app = App::new();
        app.handle_engine_message(SdkMessage::StreamEvent {
            event: StreamEventData::MessageStart,
            task_id: None,
        });
        app.note_render_frame_drawn();
        app.render_last_frame_at = Some(std::time::Instant::now());

        let (_tx, rx) = tokio::sync::mpsc::channel::<SdkMessage>(1);
        app.engine_rx = Some(rx);
        app.handle_engine_message(SdkMessage::StreamEvent {
            event: StreamEventData::ContentBlockDelta {
                index: 0,
                delta: ContentDelta::TextDelta {
                    text: "paced delta".to_string(),
                },
            },
            task_id: None,
        });

        assert!(
            !app.render_frame_scheduler_stats().dirty,
            "fast streaming deltas should not force an immediate frame"
        );
        assert!(
            !app.should_render_frame_for_run(),
            "the same event-loop iteration should still skip the frame"
        );

        app.render_throttled_dirty_at = Some(
            std::time::Instant::now()
                .checked_sub(Duration::from_millis(1))
                .expect("test instant should allow subtraction"),
        );
        assert!(
            app.should_render_frame_for_run(),
            "throttle-deadline expiry should flush streaming transcript updates before the slower active animation interval"
        );
        app.note_render_frame_drawn();

        app.render_last_frame_at = Some(
            std::time::Instant::now()
                .checked_sub(super::ACTIVE_RENDER_FRAME_INTERVAL + Duration::from_millis(1))
                .expect("test instant should allow subtraction"),
        );
        assert!(
            app.should_render_frame_for_run(),
            "active stream scheduling should flush throttled transcript changes on a paced follow-up frame"
        );
    }

    #[test]
    fn external_statusline_inflight_does_not_drive_invisible_frames() {
        let mut app = App::new();
        app.note_render_frame_drawn();
        let idle_fingerprint = app.render_tick_fingerprint();

        app.external_statusline_in_flight = true;
        app.external_statusline_last_sequence = 42;
        assert_eq!(
            app.render_tick_fingerprint(),
            idle_fingerprint,
            "starting an invisible external statusline command should not dirty the main surface"
        );
        assert!(
            !app.render_frame_scheduler_stats().active_animation,
            "external statusline in-flight state should not drive the active render loop"
        );

        app.render_last_frame_at = Some(
            std::time::Instant::now()
                .checked_sub(super::ACTIVE_RENDER_FRAME_INTERVAL + Duration::from_millis(1))
                .expect("test instant should allow subtraction"),
        );
        assert!(
            !app.should_render_frame_for_run(),
            "an invisible background statusline process must not schedule animation frames"
        );

        app.external_statusline_output = Some("branch main".to_string());
        assert_ne!(
            app.render_tick_fingerprint(),
            idle_fingerprint,
            "visible statusline output changes should still repaint"
        );

        let mut debug_app = App::new();
        debug_app.active_modal = ActiveModal::DebugConfig(debug_app.build_debug_config_state());
        let debug_fingerprint = debug_app.render_tick_fingerprint();
        debug_app.external_statusline_in_flight = true;
        assert_ne!(
            debug_app.render_tick_fingerprint(),
            debug_fingerprint,
            "the debug modal should still reflect external statusline in-flight state"
        );
    }

    #[test]
    fn focus_change_updates_notification_latch_without_dirty_frame() {
        let mut app = App::new();
        app.services.set_focus(false);
        app.services.notification_fired = true;
        app.note_render_frame_drawn();
        let before = app.render_tick_fingerprint();

        app.handle_event(AppEvent::FocusChange(true));

        assert!(app.services.focus.is_focused());
        assert!(
            !app.services.notification_fired,
            "focus gained should still reset the notification latch"
        );
        assert_eq!(
            app.render_tick_fingerprint(),
            before,
            "focus and notification latch state are not visible in the main TUI"
        );
        assert!(
            !app.render_frame_scheduler_stats().dirty,
            "hidden focus-state changes should not schedule a redraw"
        );

        app.handle_event(AppEvent::FocusChange(false));

        assert!(!app.services.focus.is_focused());
        assert!(
            !app.render_frame_scheduler_stats().dirty,
            "focus lost without visible TUI state change should not schedule a redraw"
        );
    }

    #[test]
    fn mouse_events_dirty_only_when_visible_scroll_state_changes() {
        let mut app = App::new();
        app.note_render_frame_drawn();
        assert!(!app.render_frame_scheduler_stats().dirty);

        app.handle_event(AppEvent::Mouse(mouse_event(MouseEventKind::Moved)));
        assert!(
            !app.render_frame_scheduler_stats().dirty,
            "mouse movement without widget routing should not schedule a frame"
        );

        app.active_modal = ActiveModal::ConfirmClear;
        app.scroll.set_total_items(200);
        app.scroll.sticky = false;
        app.scroll.offset = 40;
        app.handle_event(AppEvent::Mouse(mouse_event(MouseEventKind::ScrollDown)));
        assert!(
            !app.render_frame_scheduler_stats().dirty,
            "static modals should consume wheel input without redrawing hidden transcript"
        );
        assert_eq!(app.scroll.offset, 40);

        app.active_modal = ActiveModal::None;
        app.handle_event(AppEvent::Mouse(mouse_event(MouseEventKind::ScrollDown)));
        assert!(app.render_frame_scheduler_stats().dirty);
        assert_eq!(app.scroll.offset, 43);

        app.note_render_frame_drawn();
        app.state.all_slash_commands = (0..35)
            .map(|index| SlashCommandInfo {
                name: format!("mouse-dirty-{index:02}"),
                description: "Mouse dirty smoke command".to_string(),
                category: "Smoke".to_string(),
                aliases: Vec::new(),
                argument_hint: String::new(),
                kind: SlashCommandKind::Command,
            })
            .collect();
        app.active_modal = ActiveModal::HelpDialog(HelpDialogState::new(""));
        app.handle_event(AppEvent::Mouse(mouse_event(MouseEventKind::ScrollDown)));
        assert!(
            app.render_frame_scheduler_stats().dirty,
            "scrollable modal wheel movement should schedule a frame"
        );
        let ActiveModal::HelpDialog(state) = &app.active_modal else {
            panic!("expected help dialog");
        };
        assert_eq!(state.scroll, 3);
    }

    #[test]
    fn mouse_wheel_down_refreshes_stale_transcript_height_after_manual_hold() {
        let mut app = App::new();
        app.message_content_area = Some(ratatui::layout::Rect::new(0, 0, 80, 10));
        app.messages.push(MessageData {
            message_type: MessageType::Assistant,
            content: (0..80)
                .map(|index| {
                    format!(
                        "matrix-row-{index:04}: long manual-scroll hold output keeps growing.\n"
                    )
                })
                .collect(),
            timestamp: None,
            is_streaming: false,
            tool_name: None,
            is_error: false,
            thinking: None,
            thinking_completed_at: None,
            full_content: None,
            expanded: false,
        });
        let surface = app.render_surface_model();
        let latest_total = app.message_total_rows(&surface, 80);
        assert!(
            latest_total > 20,
            "fixture must outgrow the stale scroll total"
        );
        app.scroll.set_viewport_height(10);
        app.scroll.set_total_items(20);
        app.scroll.sticky = false;
        app.scroll.offset = 10;
        app.note_render_frame_drawn();

        app.handle_event(AppEvent::Mouse(mouse_event(MouseEventKind::ScrollDown)));

        assert_eq!(
            app.scroll.total_items, latest_total,
            "wheel input should refresh transcript height from the latest model before scrolling"
        );
        assert!(
            app.scroll.offset > 10,
            "wheel down must be able to leave the stale old max offset"
        );
        assert!(
            app.render_frame_scheduler_stats().dirty,
            "refresh plus scroll movement must schedule a redraw"
        );
    }

    #[test]
    fn mouse_wheel_dismisses_idle_return_and_scrolls_transcript() {
        let mut app = App::new();
        app.message_content_area = Some(ratatui::layout::Rect::new(0, 0, 80, 10));
        app.messages.push(MessageData {
            message_type: MessageType::Assistant,
            content: (0..80)
                .map(|index| format!("idle-return-row-{index:04}: scroll should pass through.\n"))
                .collect(),
            timestamp: None,
            is_streaming: false,
            tool_name: None,
            is_error: false,
            thinking: None,
            thinking_completed_at: None,
            full_content: None,
            expanded: false,
        });
        let surface = app.render_surface_model();
        let total_rows = app.message_total_rows(&surface, 80);
        app.scroll.set_viewport_height(10);
        app.scroll.set_total_items(total_rows);
        app.scroll.sticky = false;
        app.scroll.offset = 20;
        app.active_modal = ActiveModal::IdleReturn("away 20m".to_string());
        app.services.idle_return_state =
            Some(IdleReturnDialogState::new(Duration::from_secs(20 * 60)));
        app.note_render_frame_drawn();

        app.handle_event(AppEvent::Mouse(mouse_event(MouseEventKind::ScrollDown)));

        assert!(matches!(app.active_modal, ActiveModal::None));
        assert!(app.services.idle_return_state.is_none());
        assert_eq!(app.scroll.offset, 23);
        assert!(
            app.render_frame_scheduler_stats().dirty,
            "dismissing idle return and scrolling transcript must redraw"
        );
    }

    #[test]
    fn debug_config_scroll_clamps_to_rendered_viewport() {
        let model = DebugConfigRenderModel::new("renderer diagnostics").section((0..12).fold(
            StatusSectionRenderModel::new("Renderer"),
            |section, index| {
                section.row(
                    format!("row {index}"),
                    format!("value {index}"),
                    StatusRowLevel::Info,
                )
            },
        ));
        let mut state = super::DebugConfigState::new(model);

        state.scroll_down(usize::MAX, 4);
        assert_eq!(
            state.scroll,
            state.model.row_count().saturating_sub(4),
            "debug-config scroll should stop at the last full viewport"
        );

        state.scroll_to_top();
        state.scroll_to_bottom(4);
        assert_eq!(
            state.scroll,
            state.model.row_count().saturating_sub(4),
            "End should not jump to a blank tail"
        );

        state.scroll = usize::MAX;
        assert_eq!(
            state.visible_scroll(4),
            state.model.row_count().saturating_sub(4),
            "render should clamp stale scroll after resize before drawing"
        );
        assert_eq!(
            state.visible_scroll(usize::MAX),
            0,
            "a large viewport should not preserve an unreachable old offset"
        );
    }

    #[test]
    fn non_scrollable_transcript_wheel_preserves_sticky_without_dirty_frame() {
        let mut app = App::new();
        app.scroll.set_viewport_height(20);
        app.scroll.set_total_items(3);
        app.note_render_frame_drawn();
        assert!(app.scroll.sticky);
        assert_eq!(app.scroll.offset, 0);
        assert!(!app.render_frame_scheduler_stats().dirty);

        app.handle_event(AppEvent::Mouse(mouse_event(MouseEventKind::ScrollUp)));

        assert_eq!(app.scroll.offset, 0);
        assert!(
            app.scroll.sticky,
            "wheel input on a short transcript must keep future output anchored to the live tail"
        );
        assert!(
            !app.render_frame_scheduler_stats().dirty,
            "no-op transcript wheel movement should not schedule a redraw"
        );
    }

    #[test]
    fn transcript_page_keys_use_rendered_message_viewport_height() {
        let mut app = App::new();
        app.scroll.set_viewport_height(17);
        app.scroll.set_total_items(100);
        app.message_content_area = Some(ratatui::layout::Rect::new(0, 0, 80, 17));
        assert!(app.scroll.sticky);
        assert_eq!(app.scroll.offset, 83);

        app.handle_event(AppEvent::Key(KeyEvent::new(
            KeyCode::PageUp,
            KeyModifiers::NONE,
        )));

        assert_eq!(
            app.scroll.offset, 66,
            "PageUp should use the last rendered transcript viewport height, not a fixed 10 rows"
        );
        assert!(!app.scroll.sticky);

        app.handle_event(AppEvent::Key(KeyEvent::new(
            KeyCode::PageDown,
            KeyModifiers::NONE,
        )));

        assert_eq!(
            app.scroll.offset, 83,
            "PageDown should use the same rendered transcript viewport height"
        );
        assert!(app.scroll.sticky);
    }

    #[test]
    fn transcript_scrollbar_does_not_create_its_own_overflow() {
        let content_width = 24u16;
        let content = "x".repeat(content_width.saturating_sub(3) as usize);
        let mut app = App::new();
        app.messages.push(MessageData {
            message_type: MessageType::System,
            content,
            timestamp: None,
            is_streaming: false,
            tool_name: None,
            is_error: false,
            thinking: None,
            thinking_completed_at: None,
            full_content: None,
            expanded: false,
        });
        let surface = app.render_surface_model();
        let full_rows = app.message_total_rows(&surface, content_width);
        let reduced_rows = app.message_total_rows(&surface, content_width.saturating_sub(1));
        let area = ratatui::layout::Rect::new(0, 0, content_width, full_rows as u16);

        let (message_area, scrollbar_area) = app.sync_message_scroll_with_scrollbar(area, &surface);

        assert!(
            reduced_rows > area.height as usize,
            "fixture must model the old self-induced rail overflow"
        );
        assert_eq!(
            message_area.width, area.width,
            "a transcript that fits at full width should keep the full content area"
        );
        assert!(
            scrollbar_area.is_none(),
            "the scrollbar rail must not appear only because reserving the rail would make text wrap"
        );
        assert_eq!(app.scroll.visible_count, full_rows);
        assert_eq!(app.scroll.total_items, full_rows);
        assert_eq!(app.scroll.offset, 0);
        assert!(app.scroll.sticky);
    }

    #[test]
    fn non_scrollable_transcript_page_key_preserves_sticky_without_dirty_frame() {
        let mut app = App::new();
        app.scroll.set_viewport_height(20);
        app.scroll.set_total_items(3);
        app.message_content_area = Some(ratatui::layout::Rect::new(0, 0, 80, 20));
        app.note_render_frame_drawn();
        assert!(app.scroll.sticky);
        assert_eq!(app.scroll.offset, 0);
        assert!(!app.render_frame_scheduler_stats().dirty);

        app.handle_event(AppEvent::Key(KeyEvent::new(
            KeyCode::PageUp,
            KeyModifiers::NONE,
        )));

        assert_eq!(app.scroll.offset, 0);
        assert!(
            app.scroll.sticky,
            "PageUp on a short transcript must keep future output anchored to the live tail"
        );
        assert!(
            !app.render_frame_scheduler_stats().dirty,
            "no-op transcript PageUp should not schedule a redraw"
        );

        app.handle_event(AppEvent::Key(KeyEvent::new(
            KeyCode::PageDown,
            KeyModifiers::NONE,
        )));

        assert_eq!(app.scroll.offset, 0);
        assert!(app.scroll.sticky);
        assert!(
            !app.render_frame_scheduler_stats().dirty,
            "no-op transcript PageDown should not schedule a redraw"
        );
    }

    #[test]
    fn transcript_arrow_keys_scroll_by_rendered_rows_and_restore_tail_while_streaming() {
        let mut app = App::new();
        app.state.is_streaming = true;
        app.scroll.set_viewport_height(17);
        app.scroll.set_total_items(100);
        app.message_content_area = Some(ratatui::layout::Rect::new(0, 0, 80, 17));
        assert!(app.scroll.sticky);
        assert_eq!(app.scroll.offset, 83);

        app.handle_event(AppEvent::Key(KeyEvent::new(
            KeyCode::Up,
            KeyModifiers::NONE,
        )));

        assert_eq!(
            app.scroll.offset, 82,
            "plain Up should scroll the transcript by one rendered row"
        );
        assert!(!app.scroll.sticky);

        app.handle_event(AppEvent::Key(KeyEvent::new(
            KeyCode::Down,
            KeyModifiers::NONE,
        )));

        assert_eq!(
            app.scroll.offset, 83,
            "plain Down should reach the live tail one rendered row at a time"
        );
        assert!(
            app.scroll.sticky,
            "returning to the max offset must re-enable live-tail sticky scroll"
        );
    }

    #[test]
    fn transcript_home_end_keys_jump_top_and_bottom_when_prompt_empty() {
        let mut app = App::new();
        app.scroll.set_viewport_height(10);
        app.scroll.set_total_items(50);
        app.message_content_area = Some(ratatui::layout::Rect::new(0, 0, 80, 10));
        assert!(app.scroll.sticky);
        assert_eq!(app.scroll.offset, 40);

        app.handle_event(AppEvent::Key(KeyEvent::new(
            KeyCode::Home,
            KeyModifiers::NONE,
        )));

        assert_eq!(app.scroll.offset, 0);
        assert!(
            !app.scroll.sticky,
            "jumping to the top of a long transcript should leave live-tail mode"
        );

        app.handle_event(AppEvent::Key(KeyEvent::new(
            KeyCode::End,
            KeyModifiers::NONE,
        )));

        assert_eq!(app.scroll.offset, 40);
        assert!(
            app.scroll.sticky,
            "End should provide a direct keyboard path back to the live tail"
        );
    }

    #[test]
    fn alt_arrow_keys_preserve_message_focus_navigation() {
        let mut app = App::new();
        app.push_system_message("first", false);
        app.push_system_message("second", false);
        app.message_content_area = Some(ratatui::layout::Rect::new(0, 0, 80, 10));

        app.handle_event(AppEvent::Key(KeyEvent::new(KeyCode::Up, KeyModifiers::ALT)));

        assert_eq!(app.focused_message_idx, Some(0));

        app.handle_event(AppEvent::Key(KeyEvent::new(
            KeyCode::Down,
            KeyModifiers::ALT,
        )));

        assert_eq!(app.focused_message_idx, Some(1));
    }

    #[test]
    fn no_op_transcript_arrow_keys_do_not_dirty_frame() {
        let mut empty_app = App::new();
        empty_app.note_render_frame_drawn();
        assert!(!empty_app.render_frame_scheduler_stats().dirty);

        empty_app.handle_event(AppEvent::Key(KeyEvent::new(
            KeyCode::Up,
            KeyModifiers::NONE,
        )));

        assert_eq!(empty_app.focused_message_idx, None);
        assert!(
            !empty_app.render_frame_scheduler_stats().dirty,
            "row Up on an empty transcript should not schedule a redraw"
        );

        let mut single_app = App::new();
        single_app.messages.push(MessageData {
            message_type: MessageType::System,
            content: "only message".to_string(),
            timestamp: None,
            is_streaming: false,
            tool_name: None,
            is_error: false,
            thinking: None,
            thinking_completed_at: None,
            full_content: None,
            expanded: false,
        });
        single_app.focused_message_idx = Some(0);
        single_app.scroll.set_viewport_height(20);
        single_app.scroll.set_total_items(1);
        single_app.message_content_area = Some(ratatui::layout::Rect::new(0, 0, 80, 20));
        single_app.note_render_frame_drawn();
        assert!(!single_app.render_frame_scheduler_stats().dirty);

        single_app.handle_event(AppEvent::Key(KeyEvent::new(
            KeyCode::Down,
            KeyModifiers::NONE,
        )));

        assert_eq!(single_app.focused_message_idx, Some(0));
        assert_eq!(single_app.scroll.offset, 0);
        assert!(single_app.scroll.sticky);
        assert!(
            !single_app.render_frame_scheduler_stats().dirty,
            "row Down that keeps the same visible transcript state should not schedule a redraw"
        );
    }

    #[test]
    fn resize_event_clamps_manual_scroll_without_restoring_sticky() {
        let mut app = App::new();
        app.scroll.set_viewport_height(5);
        app.scroll.set_total_items(100);
        app.scroll.scroll_up(20);
        assert!(!app.scroll.sticky);

        app.handle_event(AppEvent::Resize {
            width: 120,
            height: 100,
        });

        assert_eq!(app.scroll.visible_count, 96);
        assert_eq!(app.scroll.offset, 4);
        assert!(
            !app.scroll.sticky,
            "resize clamping must not silently re-enable sticky bottom"
        );
        assert!(app.render_frame_scheduler_stats().dirty);
    }

    #[test]
    fn same_size_resize_without_visible_state_change_does_not_dirty_frame() {
        let mut app = App::new();
        app.state.terminal_width = 120;
        app.state.terminal_height = 40;
        app.scroll.set_viewport_height(36);
        app.scroll.set_total_items(80);
        app.note_render_frame_drawn();
        assert!(!app.render_frame_scheduler_stats().dirty);

        app.handle_event(AppEvent::Resize {
            width: 120,
            height: 40,
        });

        assert_eq!(app.state.terminal_width, 120);
        assert_eq!(app.state.terminal_height, 40);
        assert_eq!(app.scroll.viewport_height, 36);
        assert!(
            !app.render_frame_scheduler_stats().dirty,
            "same-size resize with unchanged scroll state should not redraw"
        );

        let mut unsynced_app = App::new();
        unsynced_app.state.terminal_width = 80;
        unsynced_app.state.terminal_height = 24;
        unsynced_app.scroll.set_viewport_height(24);
        unsynced_app.note_render_frame_drawn();

        unsynced_app.handle_event(AppEvent::Resize {
            width: 80,
            height: 24,
        });

        assert_eq!(unsynced_app.scroll.viewport_height, 20);
        assert!(
            unsynced_app.render_frame_scheduler_stats().dirty,
            "same terminal dimensions must still redraw when resize synchronizes viewport state"
        );
    }

    #[test]
    fn tool_use_stop_and_empty_completed_do_not_render_as_answer() {
        let mut app = App::new();

        app.handle_engine_message(SdkMessage::StreamEvent {
            event: StreamEventData::MessageStart,
            task_id: None,
        });
        app.handle_engine_message(SdkMessage::StreamEvent {
            event: StreamEventData::MessageDelta {
                usage: None,
                stop_reason: Some("tool_use".to_string()),
            },
            task_id: None,
        });
        app.handle_engine_message(SdkMessage::Result {
            terminal: "Completed".to_string(),
            cost_usd: None,
            duration_ms: None,
            usage: None,
            task_id: None,
        });

        let transcript = app
            .messages
            .iter()
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!transcript.contains("stop: tool_use"));
        assert!(!transcript.contains("no content"));
    }

    #[test]
    fn structured_render_events_drive_footer_activity() {
        let mut app = App::new();

        app.handle_engine_message(SdkMessage::SystemInit {
            session_id: "session-render".to_string(),
            model: "test-model".to_string(),
            tools: Vec::new(),
            task_id: None,
        });
        assert_eq!(app.state.render_activity.status_line(), None);

        app.apply_render_event(&RenderEvent::new(
            RenderEventKind::CommandOutput {
                tool_id: Some("toolu-bash".to_string()),
                stream: "stdout".to_string(),
                bytes: 2048,
                preview_lines: 8,
                hidden_lines: 112,
                total_lines: Some(120),
                full_log_available: true,
            },
            crate::render_events::RenderEventScope::Main,
            UiStage::RunningCommand,
        ));
        assert_eq!(
            app.footer_render_model().activity.as_deref(),
            Some("cmd output: stdout 8 lines shown, 112 lines hidden, 120 lines total, full log")
        );

        app.handle_engine_message(SdkMessage::ToolUseSummary {
            tool_name: "TodoWrite".to_string(),
            tool_use_id: Some("toolu-plan".to_string()),
            summary: serde_json::json!({
                "new_todos": [
                    {"id": "1", "content": "Read renderer", "status": "completed"},
                    {"id": "2", "content": "Implement activity state", "status": "in_progress"}
                ]
            })
            .to_string(),
            full_content: None,
            task_id: None,
        });

        let footer = app.footer_render_model();
        assert_eq!(app.state.ui_stage, UiStage::Planning);
        assert_eq!(footer.turn_state.as_deref(), Some("planning"));
        assert_eq!(
            footer.activity.as_deref(),
            Some("plan: 2 steps, 1 done, 1 active, Implement activity state")
        );

        app.handle_engine_message(SdkMessage::ToolUseSummary {
            tool_name: "Edit".to_string(),
            tool_use_id: Some("toolu-edit".to_string()),
            summary: serde_json::json!({
                "file_path": "src/app.rs",
                "old_string": "old\n",
                "new_string": "new\nextra\n"
            })
            .to_string(),
            full_content: None,
            task_id: None,
        });

        let footer = app.footer_render_model();
        assert_eq!(app.state.ui_stage, UiStage::EditingFiles);
        assert_eq!(footer.activity.as_deref(), Some("diff: 1 +2 -1"));

        app.handle_engine_message(SdkMessage::Result {
            terminal: "Completed".to_string(),
            cost_usd: None,
            duration_ms: None,
            usage: None,
            task_id: None,
        });

        assert_eq!(
            app.state.render_activity.status_line().as_deref(),
            Some("final: done")
        );
    }

    #[test]
    fn footer_reports_estimated_context_usage_from_model_history() {
        let mut app = App::new();
        app.engine_history.push(Message {
            role: Role::User,
            content: vec![ContentBlock::Text(TextBlock {
                text: "x".repeat(8_000),
            })],
            uuid: None,
            is_meta: None,
            origin: None,
            timestamp: None,
            extra: HashMap::new(),
        });

        let footer = app.footer_render_model();
        let context = footer
            .context
            .expect("footer should expose estimated context usage");

        assert_eq!(context.used_tokens, 2_000);
        assert_eq!(
            context.window_tokens,
            mossen_utils::context::MODEL_CONTEXT_WINDOW_DEFAULT
        );
        assert_eq!(context.label(), "ctx 2k/200k");
    }

    #[test]
    fn footer_render_model_carries_session_statusline_config() {
        let mut app = App::new();
        app.state
            .footer_config
            .set_enabled(FooterItem::Project, false);
        app.state.footer_config.set_enabled(FooterItem::Cost, false);

        let footer = app.footer_render_model();

        assert!(!footer.config.is_enabled(FooterItem::Project));
        assert!(!footer.config.is_enabled(FooterItem::Cost));
        assert!(footer.config.is_enabled(FooterItem::Model));
    }

    #[test]
    fn footer_statusline_presets_have_distinct_render_shapes() {
        let minimal = FooterRenderConfig::minimal();
        let focused = FooterRenderConfig::focused();
        let standard = FooterRenderConfig::standard();
        let full = FooterRenderConfig::full();

        assert_eq!(minimal.preset_label(), "Minimal");
        assert_eq!(focused.preset_label(), "Focused");
        assert_eq!(standard.preset_label(), "Standard");
        assert_eq!(full.preset_label(), "Full");

        assert!(focused.is_enabled(FooterItem::AccessMode));
        assert!(focused.is_enabled(FooterItem::Reasoning));
        assert!(!focused.is_enabled(FooterItem::Project));
        assert!(!focused.is_enabled(FooterItem::Cost));
        assert!(full.is_enabled(FooterItem::ExternalStatus));
        assert_ne!(standard.right_items, full.right_items);
    }

    #[test]
    fn footer_statusline_codex_alias_applies_focused_preset() {
        let dir = tempfile::tempdir().expect("tempdir should be created");
        let mut app = App::new();
        app.engine_config.cwd = dir.path().to_string_lossy().to_string();

        app.handle_command("statusline codex");

        assert_eq!(app.state.footer_config.preset_label(), "Focused");
        assert!(app.state.footer_config.is_enabled(FooterItem::Model));
        assert!(app.state.footer_config.is_enabled(FooterItem::AccessMode));
        assert!(app.state.footer_config.is_enabled(FooterItem::Reasoning));
        assert!(app.state.footer_config.is_enabled(FooterItem::Activity));
        assert!(app.state.footer_config.is_enabled(FooterItem::Context));
        assert!(!app.state.footer_config.is_enabled(FooterItem::Project));
        assert!(!app.state.footer_config.is_enabled(FooterItem::Cost));
        assert_eq!(
            app.footer_config_persistence_status,
            FooterConfigPersistenceStatus::Saved
        );
    }

    #[test]
    fn slash_model_argument_updates_active_engine_model() {
        let mut app = App::new();

        app.handle_command("model mossen-max-4-6");

        assert_eq!(app.engine_config.model, "mossen-max-4-6");
        assert_eq!(app.state.current_model.as_deref(), Some("mossen-max-4-6"));
    }

    #[test]
    fn slash_model_status_does_not_set_literal_status_model() {
        let mut app = App::new();
        app.engine_config.model = "example-fast-highspeed".to_string();

        app.handle_command("model status");

        assert_eq!(app.engine_config.model, "example-fast-highspeed");
        let ActiveModal::CommandOutput {
            title,
            body,
            is_error,
        } = &app.active_modal
        else {
            panic!("model status should open command output");
        };
        assert_eq!(title, "Model");
        assert!(!*is_error);
        assert!(body.contains("Current model: example-fast-highspeed"));
    }

    #[test]
    fn exact_slash_command_enter_submits_instead_of_accepting_suggestion() {
        let mut app = App::new();
        app.state.all_slash_commands = vec![
            SlashCommandInfo {
                name: "exit".to_string(),
                description: "Exit".to_string(),
                category: "Session".to_string(),
                aliases: Vec::new(),
                argument_hint: String::new(),
                kind: SlashCommandKind::Command,
            },
            SlashCommandInfo {
                name: "quit".to_string(),
                description: "Exit".to_string(),
                category: "Session".to_string(),
                aliases: Vec::new(),
                argument_hint: String::new(),
                kind: SlashCommandKind::Command,
            },
        ];
        app.prompt.input.insert_str("/quit");
        app.update_suggestions();

        assert!(app.prompt.show_suggestions);
        app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert!(app.should_quit);
        assert!(!app.prompt.show_suggestions);
    }

    #[test]
    fn partial_slash_command_enter_still_accepts_suggestion() {
        let mut app = App::new();
        app.state.all_slash_commands = vec![SlashCommandInfo {
            name: "quit".to_string(),
            description: "Exit".to_string(),
            category: "Session".to_string(),
            aliases: Vec::new(),
            argument_hint: String::new(),
            kind: SlashCommandKind::Command,
        }];
        app.prompt.input.insert_str("/qui");
        app.update_suggestions();

        assert!(app.prompt.show_suggestions);
        app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert!(!app.should_quit);
        assert_eq!(app.prompt.input.value, "/quit ");
        assert!(!app.prompt.show_suggestions);
    }

    #[test]
    fn slash_rename_updates_live_tui_title_state() {
        let mut app = App::new();

        app.handle_command("rename project audit");

        assert_eq!(app.services.manual_title.as_deref(), Some("project audit"));
        assert!(app.services.visible_title().contains("project audit"));
        let ActiveModal::TitleConfig(state) = &app.active_modal else {
            panic!("rename should open title config modal");
        };
        assert_eq!(state.notice, "saved");
        assert_eq!(state.draft, "project audit");
    }

    #[test]
    fn slash_cost_uses_live_tui_cost_snapshot() {
        let mut app = App::new();
        app.total_cost_usd = 0.42;
        app.directives = Some(Arc::new(mossen_commands::all_directives()));

        app.handle_command("cost");

        let transcript = app
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            transcript.contains("Total cost:       $0.42"),
            "{transcript}"
        );
        assert!(
            !transcript.contains("No token usage has been recorded"),
            "{transcript}"
        );
    }

    #[test]
    fn slash_export_writes_live_transcript_file() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut app = App::new();
        app.engine_config.cwd = temp.path().to_string_lossy().to_string();
        app.messages.push(MessageData {
            message_type: MessageType::User,
            content: "please audit this".to_string(),
            timestamp: None,
            is_streaming: false,
            tool_name: None,
            is_error: false,
            thinking: None,
            thinking_completed_at: None,
            full_content: None,
            expanded: false,
        });
        app.messages.push(MessageData {
            message_type: MessageType::Assistant,
            content: "audit result".to_string(),
            timestamp: None,
            is_streaming: false,
            tool_name: None,
            is_error: false,
            thinking: None,
            thinking_completed_at: None,
            full_content: None,
            expanded: false,
        });

        app.handle_command("export md transcript");

        let path = temp.path().join("transcript.md");
        let exported = std::fs::read_to_string(&path).expect("export should write markdown");
        assert!(
            exported.contains("# Mossen Conversation Export"),
            "{exported}"
        );
        assert!(exported.contains("## User"), "{exported}");
        assert!(exported.contains("please audit this"), "{exported}");
        assert!(exported.contains("audit result"), "{exported}");
        let transcript = app
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            transcript.contains("Exported conversation transcript"),
            "{transcript}"
        );
    }

    #[test]
    fn slash_context_uses_live_tui_context_snapshot() {
        let mut app = App::new();
        app.engine_config.model = "example-fast-highspeed".to_string();
        app.engine_history.push(Message {
            role: Role::User,
            content: vec![ContentBlock::Text(TextBlock {
                text: "current live context".to_string(),
            })],
            uuid: None,
            is_meta: None,
            origin: None,
            timestamp: None,
            extra: HashMap::new(),
        });

        app.handle_command("context");

        let transcript = app
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(transcript.contains("Context Usage"), "{transcript}");
        assert!(transcript.contains("Estimated tokens:"), "{transcript}");
        assert!(
            transcript.contains("Engine history messages: 1"),
            "{transcript}"
        );
        assert!(!transcript.contains("not attached"), "{transcript}");
    }

    #[test]
    fn slash_model_list_opens_configured_profile_picker() {
        let _lock = model_config_lock();
        let _env = isolate_model_env();
        seed_model_profiles();
        let mut app = App::new();

        app.handle_command("model list");

        let ActiveModal::ModelPicker(state) = &app.active_modal else {
            panic!("model list should open picker");
        };
        assert!(state.models.iter().any(|model| model.id == "fast"
            && model.name == "example-fast"
            && model.provider == "profile: fast"));
        facade::reset_facade_for_testing();
    }

    #[test]
    fn slash_model_model_id_uses_matching_profile_backend() {
        let _lock = model_config_lock();
        let _env = isolate_model_env();
        seed_model_profiles();
        let mut app = App::new();

        app.handle_command("model example-fast");

        assert_eq!(app.engine_config.model, "example-fast");
        assert_eq!(
            app.engine_config.api_base_url.as_deref(),
            Some("https://api.example.com/v1")
        );
        assert_eq!(
            std::env::var("MOSSEN_CODE_CUSTOM_BASE_URL").as_deref(),
            Ok("https://api.example.com/v1")
        );
        assert_eq!(
            std::env::var("MOSSEN_CODE_CUSTOM_MODEL").as_deref(),
            Ok("example-fast")
        );
        assert_eq!(
            mossen_agent::services::config::profiles::get_active_profile_name().as_deref(),
            Some("fast")
        );
        assert!(!app.messages.iter().any(|message| {
            message.content.contains("sk-test-fast-secret")
                || message.content.contains("sk-test-large-secret")
        }));
        facade::reset_facade_for_testing();
    }

    #[test]
    fn slash_model_profile_uses_configured_provider_protocol() {
        let _lock = model_config_lock();
        let _env = isolate_model_env();
        seed_model_profiles();
        let mut app = App::new();

        app.handle_command("model large");

        assert_eq!(app.engine_config.model, "example-large");
        assert_eq!(
            std::env::var("MOSSEN_CODE_CUSTOM_BACKEND_PROTOCOL").as_deref(),
            Ok("openai-responses")
        );
        assert_eq!(
            std::env::var("MOSSEN_CODE_CUSTOM_BASE_URL").as_deref(),
            Ok("https://responses.example.com/v1")
        );
        facade::reset_facade_for_testing();
    }

    #[test]
    fn prompt_directive_slash_command_submits_to_engine() {
        let mut app = App::new();
        app.directives = Some(Arc::new(mossen_commands::all_directives()));
        app.refresh_slash_catalog();

        app.handle_command("review 123");

        let params = app
            .pending_submit
            .as_ref()
            .expect("prompt slash command should queue an engine submit");
        assert!(params.prompt.contains("PR number: 123"));
        assert!(app.state.is_streaming);
        assert!(app.state.is_waiting_for_response);
        let message = app
            .messages
            .last()
            .expect("command invocation should be visible");
        assert_eq!(message.message_type, MessageType::User);
        assert_eq!(message.content, "/review 123");
        assert!(!app.messages.iter().any(|message| {
            message.message_type == MessageType::CommandOutput
                && message.content.contains("PR number: 123")
        }));
    }

    #[test]
    fn slash_theme_arg_updates_live_renderer_theme() {
        let mut app = App::new();

        app.handle_command("theme light-high-contrast");

        assert_eq!(app.state.theme, crate::theme::ThemeName::LightHighContrast);
        assert_eq!(app.theme.name, crate::theme::ThemeName::LightHighContrast);
        assert!(matches!(app.active_modal, ActiveModal::None));
        let transcript = app
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            transcript.contains("Theme set to: light-high-contrast"),
            "{transcript}"
        );
    }

    #[test]
    fn slash_theme_without_arg_still_opens_picker() {
        let mut app = App::new();

        app.handle_command("theme");

        assert!(matches!(
            app.active_modal,
            ActiveModal::Picker {
                kind: PickerKind::Theme,
                ..
            }
        ));
    }

    #[test]
    fn slash_fast_arg_updates_next_engine_request() {
        let mut app = App::new();

        app.handle_command("fast on");
        app.handle_submit("hello".to_string());

        assert_eq!(app.engine_config.fast_mode, Some(true));
        assert!(app.state.fast_mode);
        assert_eq!(
            app.pending_submit
                .as_ref()
                .and_then(|params| params.fast_mode),
            Some(true)
        );
    }

    #[test]
    fn slash_effort_arg_updates_next_engine_request() {
        let mut app = App::new();

        app.handle_command("effort high");
        app.handle_submit("hello".to_string());

        assert_eq!(app.engine_config.effort, Some(EffortLevel::High));
        assert_eq!(
            app.pending_submit.as_ref().and_then(|params| params.effort),
            Some(EffortLevel::High)
        );
        assert_eq!(app.reasoning_status_label().as_deref(), Some("high"));
    }

    #[test]
    fn slash_output_style_arg_updates_live_engine_prompt() {
        let mut app = App::new();

        app.handle_command("output-style concise");

        assert_eq!(app.engine_config.output_style.as_deref(), Some("Concise"));
        assert!(app
            .engine_config
            .system_prompt
            .iter()
            .any(|block| block.text.starts_with("# Output style: Concise")));
        assert!(matches!(app.active_modal, ActiveModal::None));
    }

    #[test]
    fn slash_output_style_without_arg_still_opens_picker() {
        let mut app = App::new();

        app.handle_command("output-style");

        assert!(matches!(
            app.active_modal,
            ActiveModal::Picker {
                kind: PickerKind::OutputStyle,
                ..
            }
        ));
    }

    #[test]
    fn slash_proactive_arg_updates_live_engine_prompt() {
        let mut app = App::new();

        app.handle_command("proactive on");

        assert_eq!(
            app.command_context
                .env_vars
                .get("MOSSEN_PROACTIVE")
                .map(String::as_str),
            Some("1")
        );
        assert!(app
            .engine_config
            .system_prompt
            .iter()
            .any(|block| block.text.starts_with("# Proactive mode: Enabled")));
        assert!(matches!(app.active_modal, ActiveModal::None));
    }

    #[test]
    fn slash_reload_plugins_refreshes_skill_catalog() {
        let _lock = skill_reload_lock();
        mossen_skills::clear_dynamic_skills();
        let temp = tempfile::tempdir().expect("tempdir");
        let skill_dir = temp
            .path()
            .join(".mossen")
            .join("skills")
            .join("live-tui-skill");
        std::fs::create_dir_all(&skill_dir).expect("create skill dir");
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\ndescription: Live TUI reload skill\n---\nUse this skill after reload.\n",
        )
        .expect("write skill");

        let mut app = App::new();
        app.command_context.cwd = temp.path().to_path_buf();
        app.engine_config.cwd = temp.path().to_string_lossy().to_string();
        app.directives = Some(Arc::new(mossen_commands::all_directives()));
        app.refresh_slash_catalog();
        assert!(!app
            .state
            .all_slash_commands
            .iter()
            .any(|entry| entry.name == "live-tui-skill"));

        app.handle_command("reload-plugins");

        assert!(mossen_skills::get_dynamic_skills()
            .iter()
            .any(|skill| skill.name() == "live-tui-skill"));
        assert!(app
            .state
            .all_slash_commands
            .iter()
            .any(|entry| entry.name == "live-tui-skill"));
        let transcript = app
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(transcript.contains("runtime reloaded"), "{transcript}");
        mossen_skills::clear_dynamic_skills();
    }

    #[test]
    fn help_visible_directives_are_wired_through_tui_slash_router() {
        for (ctx_name, user_type) in [("standard", None), ("internal", Some("internal"))] {
            let mut ctx_app = App::new();
            ctx_app.command_context.user_type = user_type.map(str::to_string);
            let ctx = ctx_app.command_context.clone();
            let visible_directives = mossen_commands::all_directives();
            let visible = mossen_commands::visible_directives(&visible_directives, &ctx);
            assert!(
                visible.len() > 50,
                "{ctx_name}: expected populated help commands"
            );

            for directive in visible {
                let name = directive.name();
                let args = tui_smoke_args(name);
                let command_line = if args.is_empty() {
                    name.to_string()
                } else {
                    format!("{name} {}", args.join(" "))
                };
                let mut app = App::new();
                app.command_context = ctx.clone();
                app.directives = Some(Arc::new(mossen_commands::all_directives()));
                app.refresh_slash_catalog();

                app.handle_command(&command_line);

                let transcript = app
                    .messages
                    .iter()
                    .map(|message| message.content.as_str())
                    .collect::<Vec<_>>()
                    .join("\n");
                assert!(
                    slash_router_handled(&app),
                    "{ctx_name}: /{command_line} produced no TUI state change"
                );
                assert!(
                    !transcript.contains("Unknown command"),
                    "{ctx_name}: /{command_line} fell through to unknown command"
                );
                assert!(
                    !transcript.contains("no dedicated TUI panel is registered yet"),
                    "{ctx_name}: /{command_line} hit the widget placeholder"
                );
            }
        }
    }

    #[test]
    fn tui_slash_router_blocks_disabled_directives_before_execute() {
        let mut app = App::new();
        app.directives = Some(Arc::new(mossen_commands::all_directives()));
        app.refresh_slash_catalog();

        app.handle_command("remote-env help");

        let transcript = app
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            transcript.contains("/remote-env is not available in this session."),
            "{transcript}"
        );
        assert!(
            !transcript.contains("Variables forwarded to remote sessions"),
            "{transcript}"
        );
        assert!(
            !transcript.contains("Manage remote environment variables"),
            "{transcript}"
        );
    }

    #[test]
    fn tui_slash_router_blocks_hidden_directives_before_execute() {
        let mut app = App::new();
        app.directives = Some(Arc::new(mossen_commands::all_directives()));
        app.refresh_slash_catalog();

        app.handle_command("color blue");

        let transcript = app
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            transcript.contains("/color is not available in this session."),
            "{transcript}"
        );
        assert!(!transcript.contains("not wired"), "{transcript}");
        assert!(
            !transcript.contains("Cannot set session color"),
            "{transcript}"
        );
    }

    #[test]
    fn built_in_tui_debug_surfaces_do_not_expose_unfinished_wiring_text() {
        let mut app = App::new();

        app.handle_command("debug-config");

        let ActiveModal::DebugConfig(state) = &app.active_modal else {
            panic!("debug-config should open the debug config modal");
        };
        let mut text = state.model.summary.clone();
        for section in &state.model.sections {
            text.push('\n');
            text.push_str(&section.title);
            for row in &section.rows {
                text.push('\n');
                text.push_str(&row.label);
                text.push_str(": ");
                text.push_str(&row.value);
            }
        }
        text.push('\n');
        text.push_str(&app.snapshot_task_list());
        let lowered = text.to_ascii_lowercase();

        for forbidden in [
            "not wired",
            "placeholder",
            "stub",
            "not implemented",
            "unimplemented",
            "phase 5",
        ] {
            assert!(
                !lowered.contains(forbidden),
                "built-in TUI diagnostics surfaced unfinished text `{forbidden}`:\n{text}"
            );
        }
        assert!(lowered.contains("unavailable"), "{text}");
    }

    #[test]
    fn slash_add_dir_updates_future_tool_context() {
        let dir = tempfile::tempdir().expect("tempdir should be created");
        let cwd = dir.path().join("cwd");
        let extra = dir.path().join("extra");
        std::fs::create_dir_all(&cwd).expect("cwd should be created");
        std::fs::create_dir_all(&extra).expect("extra dir should be created");
        let extra_abs = extra
            .canonicalize()
            .expect("extra dir should canonicalize")
            .to_string_lossy()
            .to_string();

        let mut app = App::new();
        app.engine_config.cwd = cwd.to_string_lossy().to_string();
        app.handle_command(&format!("add-dir {}", extra.display()));

        assert_eq!(app.additional_working_directories, vec![extra_abs.clone()]);
        app.handle_submit("use the extra working directory".to_string());
        let params = app.pending_submit.expect("submit should be queued");
        assert_eq!(
            params.tool_use_context.additional_working_directories,
            Some(vec![extra_abs])
        );
    }

    #[test]
    fn copy_response_index_parses_latest_and_nth_response() {
        assert_eq!(copy_response_index("").unwrap(), 0);
        assert_eq!(copy_response_index("1").unwrap(), 0);
        assert_eq!(copy_response_index("3").unwrap(), 2);
        assert!(copy_response_index("latest").is_err());
        assert!(copy_response_index("0").is_err());
    }

    #[test]
    fn copy_command_payload_supports_latest_response_and_full_transcript() {
        let mut app = App::new();
        app.messages.push(MessageData {
            message_type: MessageType::User,
            content: "hello".to_string(),
            timestamp: None,
            is_streaming: false,
            tool_name: None,
            is_error: false,
            thinking: None,
            thinking_completed_at: None,
            full_content: None,
            expanded: false,
        });
        app.messages.push(MessageData {
            message_type: MessageType::Assistant,
            content: "short preview".to_string(),
            timestamp: None,
            is_streaming: false,
            tool_name: None,
            is_error: false,
            thinking: None,
            thinking_completed_at: None,
            full_content: Some("full assistant response".to_string()),
            expanded: false,
        });

        let (latest, latest_label) = app
            .copy_command_payload("")
            .expect("latest assistant response");
        assert_eq!(latest, "full assistant response");
        assert_eq!(latest_label, "latest assistant response");

        let (transcript, transcript_label) = app
            .copy_command_payload("transcript")
            .expect("full transcript");
        assert_eq!(transcript_label, "conversation transcript");
        assert!(transcript.contains("User:\nhello"), "{transcript}");
        assert!(
            transcript.contains("Assistant:\nfull assistant response"),
            "{transcript}"
        );

        let (all, all_label) = app.copy_command_payload("all").expect("all transcript");
        assert_eq!(all_label, "conversation transcript");
        assert_eq!(all, transcript);
    }

    fn tui_smoke_args(name: &str) -> &'static [&'static str] {
        match name {
            "access" | "bridges" | "crafts" | "delegates" | "metrics" | "passes" | "plugin"
            | "privacy" | "rate_limit" | "sandbox" | "usage" => &["help"],
            "changes" => &["summary"],
            "config" => &["list"],
            "deauth" => &["status"],
            "heapdump" => &["help"],
            "ide" => &["status"],
            "install" => &["status"],
            "output-style" => &["list"],
            "pr-comments" => &["help"],
            "project" => &["info"],
            "remote-env" => &["help"],
            "remote-setup" => &["status"],
            "stickers" => &["help"],
            "turbo" => &["status"],
            "vim" => &["status"],
            _ => &[],
        }
    }

    fn slash_router_handled(app: &App) -> bool {
        app.pending_submit.is_some()
            || app.state.is_streaming
            || !matches!(app.active_modal, ActiveModal::None)
            || app.should_quit
            || !app.messages.is_empty()
            || app.state.compact_in_progress
            || app.state.compact_progress.is_some()
    }

    #[test]
    fn footer_statusline_config_persists_to_project_file() {
        let dir = tempfile::tempdir().expect("tempdir should be created");
        let mut app = App::new();
        app.engine_config.cwd = dir.path().to_string_lossy().to_string();

        app.handle_command("statusline minimal");

        assert_eq!(
            app.footer_config_persistence_status,
            FooterConfigPersistenceStatus::Saved
        );
        let path = dir.path().join(RENDER_STATUSLINE_CONFIG_PATH);
        assert_eq!(app.footer_config_persistence_path.as_ref(), Some(&path));
        assert!(path.exists(), "statusline config file should be written");

        let mut restored = App::new();
        restored.engine_config.cwd = dir.path().to_string_lossy().to_string();
        restored
            .load_footer_render_config_on_startup()
            .expect("statusline config load should not error")
            .expect("statusline config should exist");

        assert_eq!(
            restored.footer_config_persistence_status,
            FooterConfigPersistenceStatus::Loaded
        );
        assert!(restored.state.footer_config.is_enabled(FooterItem::Model));
        assert!(restored.state.footer_config.is_enabled(FooterItem::Context));
        assert!(!restored.state.footer_config.is_enabled(FooterItem::Project));
        assert!(!restored
            .state
            .footer_config
            .is_enabled(FooterItem::MessageCount));
    }

    #[test]
    fn footer_statusline_startup_load_skips_non_default_runtime_config() {
        let dir = tempfile::tempdir().expect("tempdir should be created");
        let mut source = App::new();
        source.engine_config.cwd = dir.path().to_string_lossy().to_string();
        source.handle_command("statusline minimal");

        let mut app = App::new();
        app.engine_config.cwd = dir.path().to_string_lossy().to_string();
        app.state.footer_config.set_enabled(FooterItem::Cost, false);

        let loaded = app
            .load_footer_render_config_on_startup()
            .expect("startup skip should not error");

        assert!(loaded.is_none());
        assert_eq!(
            app.footer_config_persistence_status,
            FooterConfigPersistenceStatus::Skipped
        );
        assert!(!app.state.footer_config.is_enabled(FooterItem::Cost));
        assert!(app.state.footer_config.is_enabled(FooterItem::Project));
    }

    #[test]
    fn footer_statusline_loads_external_command_compat_shape() {
        let dir = tempfile::tempdir().expect("tempdir should be created");
        let path = dir.path().join(RENDER_STATUSLINE_CONFIG_PATH);
        std::fs::create_dir_all(path.parent().expect("config path should have parent"))
            .expect("config directory should be created");
        std::fs::write(
            &path,
            r#"{
  "statusLine": {
    "type": "command",
    "command": "printf external-ready",
    "timeout": 700,
    "interval_ms": 5000
  }
}"#,
        )
        .expect("statusline compat config should be written");

        let mut app = App::new();
        app.engine_config.cwd = dir.path().to_string_lossy().to_string();
        app.load_footer_render_config_on_startup()
            .expect("compat statusline config should load")
            .expect("compat config should exist");

        let external = app
            .state
            .footer_config
            .external_command
            .as_ref()
            .expect("external statusline command should be configured");
        assert_eq!(external.command, "printf external-ready");
        assert_eq!(external.timeout_ms, 700);
        assert_eq!(external.interval_ms, 5000);
        assert!(app
            .state
            .footer_config
            .is_enabled(FooterItem::ExternalStatus));
    }

    #[tokio::test]
    async fn external_statusline_command_tick_is_nonblocking_and_stable() {
        let dir = tempfile::tempdir().expect("tempdir should be created");
        let mut app = App::new();
        app.engine_config.cwd = dir.path().to_string_lossy().to_string();
        app.state.footer_config.external_command = Some(ExternalStatusLineCommandConfig {
            command: "sleep 0.15; printf external-ready".to_string(),
            timeout_ms: 1_000,
            interval_ms: 10_000,
        });
        app.state
            .footer_config
            .set_enabled(FooterItem::ExternalStatus, true);

        let start = std::time::Instant::now();
        app.dispatch_tick_for_test();
        assert!(
            start.elapsed() < Duration::from_millis(80),
            "tick should spawn the statusline command without waiting for it"
        );
        assert!(app.external_statusline_in_flight);
        assert!(app.external_statusline_output.is_none());

        tokio::time::sleep(Duration::from_millis(220)).await;
        app.dispatch_tick_for_test();

        assert!(!app.external_statusline_in_flight);
        assert_eq!(
            app.external_statusline_output.as_deref(),
            Some("external-ready")
        );
        assert!(app.external_statusline_error.is_none());
    }

    #[test]
    fn task_completion_notification_tick_adds_transcript_feedback() {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut app = App::new().with_task_notification_receiver(rx);

        tx.send(mossen_tools::task_store::TaskStoreEvent {
            id: "agent-1".to_string(),
            subject: "scan repo".to_string(),
            status: "completed".to_string(),
            task_type: Some("background_agent".to_string()),
            completed_at: Some(1),
            exit_code: Some(0),
        })
        .expect("send task event");

        app.dispatch_tick_for_test();

        let message = app.messages.last().expect("notification message");
        assert_eq!(message.message_type, MessageType::System);
        assert!(message.content.contains("Agent completed: scan repo"));
        assert!(message.content.contains("Task: agent-1"));
        assert!(message.content.contains("/agents logs agent-1"));
    }

    #[test]
    fn render_surface_carries_top_status_from_footer_facts() {
        let mut app = App::new();
        app.engine_config.model = "example-fast".to_string();
        app.engine_config
            .extra_body
            .insert("effort".to_string(), serde_json::json!("high"));
        app.state.ui_stage = UiStage::RunningCommand;
        app.state
            .render_activity
            .set(RenderActivity::CommandStarted {
                command: Some("cargo test".to_string()),
                cwd: None,
            });

        let surface = app.render_surface_model();

        assert_eq!(surface.top_status.stage.as_deref(), Some("running command"));
        assert_eq!(
            surface.top_status.activity.as_deref(),
            Some("cmd: cargo test")
        );
        assert_eq!(surface.top_status.model.as_deref(), Some("example-fast"));
        assert_eq!(surface.top_status.reasoning.as_deref(), Some("high"));
        assert_eq!(surface.top_status.blocking, surface.blocking);
    }

    #[test]
    fn render_surface_carries_active_panel_from_render_activity() {
        let mut app = App::new();
        app.state.ui_stage = UiStage::RunningCommand;
        app.state
            .render_activity
            .set(RenderActivity::CommandOutput {
                stream: "stdout".to_string(),
                bytes: 2048,
                preview_lines: 8,
                hidden_lines: 112,
                total_lines: Some(120),
                full_log_available: true,
            });

        let surface = app.render_surface_model();
        let panel = surface
            .activity_panel
            .expect("active command output should produce an activity panel");

        assert_eq!(panel.stage, "running command");
        assert_eq!(panel.title, "Command output");
        assert_eq!(
            panel.summary.as_deref(),
            Some("stdout: 8 shown · 112 hidden · 120 total · full log")
        );
        assert_eq!(panel.severity, ActivityPanelSeverity::Working);
    }

    #[test]
    fn render_surface_carries_plan_progress_counts_in_active_panel() {
        let mut app = App::new();
        app.state.ui_stage = UiStage::Planning;
        app.state.render_activity.set(RenderActivity::Plan {
            step_count: 4,
            completed_count: 1,
            active_count: 1,
            pending_count: 1,
            blocked_count: 1,
            active_step: Some("Verify terminal plan rendering".to_string()),
        });

        let surface = app.render_surface_model();
        let panel = surface
            .activity_panel
            .expect("active plan should produce an activity panel");

        assert_eq!(panel.stage, "planning");
        assert_eq!(panel.title, "Plan");
        assert_eq!(
            panel.summary.as_deref(),
            Some("4 steps · 1 done · 1 active · 1 pending · 1 blocked")
        );
        assert_eq!(panel.details[0].label, "active");
        assert_eq!(panel.details[0].value, "Verify terminal plan rendering");
        assert_eq!(
            app.footer_render_model().activity.as_deref(),
            Some(
                "plan: 4 steps, 1 done, 1 active, 1 pending, 1 blocked, Verify terminal plan rendering"
            )
        );
    }

    #[test]
    fn result_records_structured_final_summary_with_files_and_commands() {
        let mut app = App::new();
        app.handle_engine_message(SdkMessage::Assistant {
            message: AssistantMessage {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolUse(ToolUseBlock {
                    id: "toolu-bash".to_string(),
                    name: "Bash".to_string(),
                    input: serde_json::json!({ "command": "cargo test", "cwd": "/repo" }),
                })],
                uuid: Some("assistant-bash".to_string()),
                model: None,
                stop_reason: Some("tool_use".to_string()),
                extra: HashMap::new(),
            },
            usage: None,
            task_id: None,
        });
        app.handle_engine_message(SdkMessage::ToolUseSummary {
            tool_name: "Bash".to_string(),
            tool_use_id: Some("toolu-bash".to_string()),
            summary: serde_json::json!({
                "stdout": "ok",
                "stderr": "",
                "exit_code": 0,
                "duration_ms": 42
            })
            .to_string(),
            full_content: None,
            task_id: None,
        });
        app.handle_engine_message(SdkMessage::Assistant {
            message: AssistantMessage {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolUse(ToolUseBlock {
                    id: "toolu-write".to_string(),
                    name: "Write".to_string(),
                    input: serde_json::json!({
                        "file_path": "src/lib.rs",
                        "content": "new line\n"
                    }),
                })],
                uuid: Some("assistant-write".to_string()),
                model: None,
                stop_reason: Some("tool_use".to_string()),
                extra: HashMap::new(),
            },
            usage: None,
            task_id: None,
        });
        app.handle_engine_message(SdkMessage::ToolUseSummary {
            tool_name: "Write".to_string(),
            tool_use_id: Some("toolu-write".to_string()),
            summary: serde_json::json!({
                "file_path": "src/lib.rs",
                "old_string": "old line\n",
                "new_string": "new line\n"
            })
            .to_string(),
            full_content: None,
            task_id: None,
        });
        app.handle_engine_message(SdkMessage::Result {
            terminal: "Completed".to_string(),
            cost_usd: None,
            duration_ms: None,
            usage: None,
            task_id: None,
        });

        let surface = app.render_surface_model();
        let summary_block = surface
            .transcript
            .blocks
            .iter()
            .find(|block| block.kind == RenderBlockKind::FinalSummary)
            .expect("final summary should be a structured transcript block");
        let summary = summary_block
            .nodes
            .iter()
            .find_map(|node| match node {
                RenderNode::FinalSummary(summary) => Some(summary),
                _ => None,
            })
            .expect("final summary node should be present");

        assert!(summary.success);
        assert_eq!(summary.commands.len(), 1);
        assert_eq!(summary.commands[0].command, "cargo test");
        assert_eq!(summary.commands[0].exit_code, Some(0));
        assert_eq!(summary.verification_results.len(), 1);
        assert_eq!(summary.verification_results[0].command, "cargo test");
        assert!(summary.verification_results[0].passed);
        assert!(summary.residual_risks.is_empty());
        assert_eq!(summary.changed_files.len(), 1);
        assert_eq!(summary.changed_files[0].path, "src/lib.rs");
        assert_eq!(summary.changed_files[0].status, "M");
        assert_eq!(summary.changed_files[0].additions, 1);
        assert_eq!(summary.changed_files[0].deletions, 1);
        assert!(
            !summary_block
                .selector_summary()
                .contains("mossen-render:final-summary"),
            "raw sidecar payload should not leak into user-visible previews"
        );
    }

    #[test]
    fn successful_text_only_result_skips_default_final_summary_noise() {
        let mut app = App::new();
        app.messages.push(MessageData {
            message_type: MessageType::Assistant,
            content: "Done.".to_string(),
            timestamp: None,
            is_streaming: false,
            tool_name: None,
            is_error: false,
            thinking: None,
            thinking_completed_at: None,
            full_content: None,
            expanded: false,
        });

        app.handle_engine_message(SdkMessage::Result {
            terminal: "Completed".to_string(),
            cost_usd: None,
            duration_ms: None,
            usage: None,
            task_id: None,
        });

        let surface = app.render_surface_model();
        assert!(
            !surface
                .transcript
                .blocks
                .iter()
                .any(|block| block.kind == RenderBlockKind::FinalSummary),
            "text-only successful turns should not add a default Final Summary block"
        );
        assert!(
            !matches!(
                app.state.render_activity.current,
                Some(RenderActivity::Final { .. })
            ),
            "text-only successful turns should not switch the activity panel to Final summary"
        );
    }

    #[test]
    fn successful_command_only_result_skips_default_final_summary_noise() {
        let mut app = App::new();
        app.handle_engine_message(SdkMessage::Assistant {
            message: AssistantMessage {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolUse(ToolUseBlock {
                    id: "toolu-bash".to_string(),
                    name: "Bash".to_string(),
                    input: serde_json::json!({ "command": "cargo test", "cwd": "/repo" }),
                })],
                uuid: Some("assistant-bash".to_string()),
                model: None,
                stop_reason: Some("tool_use".to_string()),
                extra: HashMap::new(),
            },
            usage: None,
            task_id: None,
        });
        app.handle_engine_message(SdkMessage::ToolUseSummary {
            tool_name: "Bash".to_string(),
            tool_use_id: Some("toolu-bash".to_string()),
            summary: serde_json::json!({
                "stdout": "ok",
                "stderr": "",
                "exit_code": 0,
                "duration_ms": 42
            })
            .to_string(),
            full_content: None,
            task_id: None,
        });
        app.handle_engine_message(SdkMessage::Result {
            terminal: "Completed".to_string(),
            cost_usd: None,
            duration_ms: None,
            usage: None,
            task_id: None,
        });

        let surface = app.render_surface_model();
        assert!(
            !surface
                .transcript
                .blocks
                .iter()
                .any(|block| block.kind == RenderBlockKind::FinalSummary),
            "successful command-only turns should not add a default Final Summary block"
        );
    }

    #[test]
    fn completed_result_with_failed_verification_is_attention() {
        let mut app = App::new();
        app.handle_engine_message(SdkMessage::Assistant {
            message: AssistantMessage {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolUse(ToolUseBlock {
                    id: "toolu-bash".to_string(),
                    name: "Bash".to_string(),
                    input: serde_json::json!({ "command": "cargo check --workspace", "cwd": "/repo" }),
                })],
                uuid: Some("assistant-bash".to_string()),
                model: None,
                stop_reason: Some("tool_use".to_string()),
                extra: HashMap::new(),
            },
            usage: None,
            task_id: None,
        });
        app.handle_engine_message(SdkMessage::ToolUseSummary {
            tool_name: "Bash".to_string(),
            tool_use_id: Some("toolu-bash".to_string()),
            summary: serde_json::json!({
                "stdout": "",
                "stderr": "compile failed",
                "exit_code": 1,
                "duration_ms": 42
            })
            .to_string(),
            full_content: None,
            task_id: None,
        });
        app.handle_engine_message(SdkMessage::Result {
            terminal: "Completed".to_string(),
            cost_usd: None,
            duration_ms: None,
            usage: None,
            task_id: None,
        });

        let surface = app.render_surface_model();
        let summary = surface
            .transcript
            .blocks
            .iter()
            .find(|block| block.kind == RenderBlockKind::FinalSummary)
            .and_then(|block| {
                block.nodes.iter().find_map(|node| match node {
                    RenderNode::FinalSummary(summary) => Some(summary),
                    _ => None,
                })
            })
            .expect("failed verification should record a final summary");

        assert!(summary.success, "terminal result is still Completed");
        assert!(summary.needs_attention());
        assert_eq!(summary.title(), "Final Summary · Attention");
        assert!(
            summary
                .residual_risks
                .iter()
                .any(|risk| risk.contains("non-zero")),
            "{:?}",
            summary.residual_risks
        );
    }

    #[test]
    fn footer_state_reports_inline_approval() {
        let mut app = App::new();
        app.active_modal = ActiveModal::PermissionRequest(PermissionPromptState::new(
            PermissionKind::Shell {
                command: "cargo test".to_string(),
            },
            "Bash",
        ));

        assert_eq!(app.turn_state_label(), "waiting approval");
    }

    #[test]
    fn footer_render_model_uses_same_blocking_state_as_approval_surface() {
        let mut app = App::new();
        app.engine_config.model = "example-fast".to_string();
        app.engine_config
            .extra_body
            .insert("effort".to_string(), serde_json::json!("high"));
        app.active_modal = ActiveModal::PermissionRequest(PermissionPromptState::new(
            PermissionKind::Shell {
                command: "cargo test".to_string(),
            },
            "Bash",
        ));

        let approval = app
            .active_approval_render_model()
            .expect("approval model should exist");
        let footer = app.footer_render_model();

        assert_eq!(approval.detail, "cargo test");
        assert_eq!(footer.model.as_deref(), Some("example-fast"));
        assert_eq!(footer.reasoning.as_deref(), Some("high"));
        assert_eq!(footer.turn_state.as_deref(), Some("waiting approval"));
        assert_eq!(
            footer.blocking.as_ref().map(|blocking| blocking.kind),
            Some(BlockingKind::Approval)
        );
    }

    #[test]
    fn render_surface_unifies_transcript_approval_and_footer() {
        let mut app = App::new();
        app.messages.push(MessageData {
            message_type: MessageType::ToolUse,
            content: serde_json::json!({ "command": "cargo test" }).to_string(),
            timestamp: None,
            is_streaming: false,
            tool_name: Some("Bash".to_string()),
            is_error: false,
            thinking: None,
            thinking_completed_at: None,
            full_content: None,
            expanded: false,
        });
        app.messages.push(MessageData {
            message_type: MessageType::ToolResult,
            content: serde_json::json!({
                "stdout": "ok",
                "stderr": "",
                "exit_code": 0
            })
            .to_string(),
            timestamp: None,
            is_streaming: false,
            tool_name: Some("Bash".to_string()),
            is_error: false,
            thinking: None,
            thinking_completed_at: None,
            full_content: None,
            expanded: false,
        });
        app.active_modal = ActiveModal::ToolUseConfirm {
            confirm: ToolUseConfirm {
                tool_use_id: "tool-call-1".to_string(),
                tool_name: "Bash".to_string(),
                raw_input: serde_json::json!({ "command": "cargo test" }),
                input_summary: "cargo test".to_string(),
                risk_level: 0,
            },
            prompt: PermissionPromptState::new(
                PermissionKind::Shell {
                    command: "cargo test".to_string(),
                },
                "Bash",
            ),
        };

        let surface = app.render_surface_model();
        let tool_block = surface
            .transcript
            .blocks
            .iter()
            .find(|block| block.tool.is_some())
            .expect("tool block should be semantic transcript data");
        let approval = surface
            .approvals
            .first()
            .expect("approval should be part of the frame surface");

        assert_eq!(
            approval.anchor_block_id.as_deref(),
            Some(tool_block.id.as_str())
        );
        assert_eq!(approval.risk, ApprovalRiskLevel::Low);
        assert_eq!(surface.blocking, surface.footer.blocking);
        assert_eq!(
            surface.blocking.as_ref().map(|blocking| blocking.kind),
            Some(BlockingKind::Approval)
        );
    }

    #[test]
    fn shell_approval_surface_exposes_edit_command_action() {
        let mut app = App::new();
        app.active_modal = ActiveModal::PermissionRequest(PermissionPromptState::new(
            PermissionKind::Shell {
                command: "cargo test".to_string(),
            },
            "Bash",
        ));

        let surface = app.render_surface_model();
        let approval = surface.approvals.first().expect("approval should exist");

        assert!(approval
            .actions
            .contains(&RenderApprovalAction::EditCommand));
    }

    #[test]
    fn render_surface_keeps_cost_blocking_without_approval() {
        let mut app = App::new();
        app.active_modal = ActiveModal::CostThreshold("budget reached".to_string());

        let surface = app.render_surface_model();

        assert!(surface.approvals.is_empty());
        assert_eq!(
            surface.blocking.as_ref().map(|blocking| blocking.kind),
            Some(BlockingKind::CostLimit)
        );
        assert_eq!(surface.blocking, surface.footer.blocking);
    }

    #[test]
    fn engine_tool_use_id_flows_into_render_record_and_approval_anchor() {
        let mut app = App::new();
        app.handle_engine_message(SdkMessage::Assistant {
            message: AssistantMessage {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolUse(ToolUseBlock {
                    id: "toolu-stable-1".to_string(),
                    name: "Bash".to_string(),
                    input: serde_json::json!({ "command": "cargo test" }),
                })],
                uuid: Some("assistant-1".to_string()),
                model: None,
                stop_reason: Some("tool_use".to_string()),
                extra: HashMap::new(),
            },
            usage: None,
            task_id: None,
        });
        app.active_modal = ActiveModal::ToolUseConfirm {
            confirm: ToolUseConfirm {
                tool_use_id: "toolu-stable-1".to_string(),
                tool_name: "Bash".to_string(),
                raw_input: serde_json::json!({ "command": "cargo test" }),
                input_summary: "cargo test".to_string(),
                risk_level: 0,
            },
            prompt: PermissionPromptState::new(
                PermissionKind::Shell {
                    command: "cargo test".to_string(),
                },
                "Bash",
            ),
        };

        let surface = app.render_surface_model();
        let tool_block = surface
            .transcript
            .blocks
            .iter()
            .find(|block| block.tool.is_some())
            .expect("tool block should exist");
        let approval = surface
            .approvals
            .first()
            .expect("approval should be part of the frame surface");

        assert_eq!(tool_block.id, "toolu-stable-1");
        assert_eq!(approval.anchor_block_id.as_deref(), Some("toolu-stable-1"));

        app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(
            app.approval_decisions
                .last()
                .and_then(|decision| decision.anchor_block_id.as_deref()),
            Some("toolu-stable-1")
        );
    }

    #[test]
    fn engine_tool_result_keeps_parent_tool_use_id() {
        let mut app = App::new();
        app.handle_engine_message(SdkMessage::Assistant {
            message: AssistantMessage {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolUse(ToolUseBlock {
                    id: "toolu-stable-1".to_string(),
                    name: "Bash".to_string(),
                    input: serde_json::json!({ "command": "cargo test" }),
                })],
                uuid: Some("assistant-1".to_string()),
                model: None,
                stop_reason: Some("tool_use".to_string()),
                extra: HashMap::new(),
            },
            usage: None,
            task_id: None,
        });
        app.handle_engine_message(SdkMessage::ToolUseSummary {
            tool_name: "Bash".to_string(),
            tool_use_id: Some("toolu-stable-1".to_string()),
            summary: serde_json::json!({ "stdout": "ok", "exit_code": 0 }).to_string(),
            full_content: None,
            task_id: None,
        });

        assert_eq!(
            app.render_record_id_overrides.get(&1).map(String::as_str),
            Some("toolu-stable-1:result")
        );
        assert_eq!(
            app.render_record_parent_overrides
                .get(&1)
                .map(String::as_str),
            Some("toolu-stable-1")
        );

        let transcript = app.render_transcript_model();
        assert_eq!(transcript.blocks.len(), 1);
        assert_eq!(transcript.blocks[0].id, "toolu-stable-1");
        assert_eq!(transcript.blocks[0].source_indices, vec![0, 1]);
        assert_eq!(
            transcript.blocks[0].tool.as_ref().map(|tool| tool.phase),
            Some(ToolPhase::Succeeded)
        );
    }

    #[test]
    fn main_engine_turn_records_and_events_share_stable_turn_id() {
        let mut app = App::new();

        app.handle_submit("run tests".to_string());
        app.handle_engine_message(SdkMessage::SystemInit {
            session_id: "session-turn-id".to_string(),
            model: "test-model".to_string(),
            tools: Vec::new(),
            task_id: None,
        });
        app.handle_engine_message(SdkMessage::Assistant {
            message: AssistantMessage {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolUse(ToolUseBlock {
                    id: "toolu-turn-1".to_string(),
                    name: "Bash".to_string(),
                    input: serde_json::json!({ "command": "cargo test" }),
                })],
                uuid: Some("assistant-turn-1".to_string()),
                model: None,
                stop_reason: Some("tool_use".to_string()),
                extra: HashMap::new(),
            },
            usage: None,
            task_id: None,
        });
        app.handle_engine_message(SdkMessage::ToolUseSummary {
            tool_name: "Bash".to_string(),
            tool_use_id: Some("toolu-turn-1".to_string()),
            summary: serde_json::json!({ "stdout": "ok", "exit_code": 0 }).to_string(),
            full_content: None,
            task_id: None,
        });
        app.handle_engine_message(SdkMessage::Result {
            terminal: "Completed".to_string(),
            cost_usd: None,
            duration_ms: None,
            usage: None,
            task_id: None,
        });

        let turn_id = app
            .render_record_turn_overrides
            .get(&0)
            .expect("user message should be attached to a render turn")
            .clone();
        assert_eq!(turn_id, "turn-0001");
        assert!(app.current_render_turn_id.is_none());
        for index in 0..app.messages.len() {
            assert_eq!(
                app.render_record_turn_overrides
                    .get(&index)
                    .map(String::as_str),
                Some(turn_id.as_str()),
                "message {index} should carry the main render turn id"
            );
        }
        assert!(
            app.render_event_history
                .iter()
                .all(|event| event.turn_id.as_deref() == Some(turn_id.as_str())),
            "main render events should carry the same turn id"
        );
        assert_eq!(app.render_turn_id_label(), "turn-0001 (last)");

        app.handle_submit("next turn".to_string());
        assert_eq!(app.current_render_turn_id.as_deref(), Some("turn-0002"));
        assert_eq!(
            app.render_record_turn_overrides
                .get(&(app.messages.len() - 1))
                .map(String::as_str),
            Some("turn-0002")
        );
    }

    #[test]
    fn main_engine_messages_are_ingested_as_raw_layer1_events() {
        let mut app = App::new();

        app.handle_submit("inspect raw events".to_string());
        app.handle_engine_message(SdkMessage::SystemInit {
            session_id: "session-raw-events".to_string(),
            model: "test-model".to_string(),
            tools: vec!["Bash".to_string()],
            task_id: None,
        });
        app.handle_engine_message(SdkMessage::Assistant {
            message: AssistantMessage {
                role: Role::Assistant,
                content: vec![ContentBlock::Text(TextBlock {
                    text: "running".to_string(),
                })],
                uuid: Some("assistant-raw-1".to_string()),
                model: None,
                stop_reason: None,
                extra: HashMap::new(),
            },
            usage: None,
            task_id: None,
        });
        app.handle_engine_message(SdkMessage::Result {
            terminal: "Completed".to_string(),
            cost_usd: None,
            duration_ms: Some(42),
            usage: None,
            task_id: None,
        });

        assert_eq!(app.raw_engine_event_history.len(), 3);
        assert_eq!(app.raw_engine_event_history[0].sequence, 1);
        assert_eq!(
            app.raw_engine_event_history[0].turn_id.as_deref(),
            Some("turn-0001")
        );
        assert_eq!(app.raw_engine_event_history[0].scope_label(), "main");
        assert_eq!(app.raw_engine_event_history[0].kind.as_str(), "system_init");
        assert!(app.raw_engine_event_history[0]
            .payload_preview
            .contains("\"type\":\"system_init\""));
        assert!(app.current_render_turn_id.is_none());

        app.handle_engine_message(SdkMessage::ApiRetry {
            error: "temporary".to_string(),
            attempt: 1,
            max_retries: 3,
            retry_in_ms: 250,
            task_id: Some("agent-raw-1".to_string()),
        });
        let task_event = app
            .raw_engine_event_history
            .last()
            .expect("task raw event should be recorded");
        assert_eq!(task_event.sequence, 4);
        assert_eq!(task_event.turn_id, None);
        assert_eq!(task_event.scope_label(), "task:agent-raw-1");
        assert_eq!(task_event.kind.as_str(), "api_retry");
    }

    #[test]
    fn app_render_session_snapshot_roundtrips_current_layer1_state() {
        let mut app = App::new();

        app.handle_submit("serialize the render session".to_string());
        app.handle_engine_message(SdkMessage::SystemInit {
            session_id: "session-snapshot".to_string(),
            model: "test-model".to_string(),
            tools: vec!["Bash".to_string()],
            task_id: None,
        });
        app.handle_engine_message(SdkMessage::ToolUseSummary {
            tool_name: "Bash".to_string(),
            tool_use_id: Some("toolu-snapshot-1".to_string()),
            summary: "ok".to_string(),
            full_content: None,
            task_id: None,
        });

        let snapshot = app.render_session_snapshot();
        let payload = snapshot
            .to_json()
            .expect("app render session snapshot should serialize");
        let restored = RenderSessionSnapshot::from_json(&payload)
            .expect("app render session snapshot should deserialize");

        assert_eq!(restored.version, snapshot.version);
        assert_eq!(restored.session_id.as_deref(), Some("session-snapshot"));
        assert_eq!(restored.current_turn_id.as_deref(), Some("turn-0001"));
        assert_eq!(restored.record_count(), app.messages.len());
        assert_eq!(restored.raw_event_count(), 2);
        assert_eq!(
            restored.records.entries[0].turn_id.as_deref(),
            Some("turn-0001")
        );
        assert_eq!(
            restored.raw_engine_events[0].turn_id.as_deref(),
            Some("turn-0001")
        );
        assert!(
            payload.contains("\"toolu-snapshot-1\""),
            "snapshot payload should retain engine tool id: {payload}"
        );
    }

    #[test]
    fn app_saves_and_loads_current_render_session_snapshot_file() {
        let mut app = App::new();

        app.handle_submit("persist the render session".to_string());
        app.handle_engine_message(SdkMessage::SystemInit {
            session_id: "session-save-load".to_string(),
            model: "test-model".to_string(),
            tools: vec!["Bash".to_string()],
            task_id: None,
        });
        app.handle_engine_message(SdkMessage::ToolUseSummary {
            tool_name: "Bash".to_string(),
            tool_use_id: Some("toolu-save-load-1".to_string()),
            summary: "ok".to_string(),
            full_content: Some("full command log".to_string()),
            task_id: None,
        });

        let dir = tempfile::tempdir().expect("tempdir should be created");
        let path = dir.path().join("snapshots").join("session.json");
        let saved = app
            .save_render_session_snapshot(&path)
            .expect("app render session snapshot should save");
        let restored = App::load_render_session_snapshot(&path)
            .expect("app render session snapshot should load");

        assert_eq!(restored, saved);
        assert_eq!(restored.session_id.as_deref(), Some("session-save-load"));
        assert_eq!(restored.current_turn_id.as_deref(), Some("turn-0001"));
        assert_eq!(restored.raw_event_count(), 2);
        assert!(std::fs::read_to_string(&path)
            .expect("snapshot file should be readable")
            .contains("\"toolu-save-load-1\""));
    }

    #[test]
    fn render_snapshot_command_exports_current_session_to_explicit_path() {
        let mut app = App::new();

        app.handle_submit("export the render session".to_string());
        app.handle_engine_message(SdkMessage::SystemInit {
            session_id: "session-command-export".to_string(),
            model: "test-model".to_string(),
            tools: vec!["Bash".to_string()],
            task_id: None,
        });
        app.handle_engine_message(SdkMessage::ToolUseSummary {
            tool_name: "Bash".to_string(),
            tool_use_id: Some("toolu-command-export-1".to_string()),
            summary: "ok".to_string(),
            full_content: Some("full command log".to_string()),
            task_id: None,
        });
        let dir = tempfile::tempdir().expect("tempdir should be created");
        let path = dir.path().join("exports").join("render-session.json");

        app.handle_command(&format!("render-snapshot save {}", path.display()));

        let restored = App::load_render_session_snapshot(&path)
            .expect("render snapshot command should save a readable snapshot");
        assert_eq!(
            restored.session_id.as_deref(),
            Some("session-command-export")
        );
        assert_eq!(restored.current_turn_id.as_deref(), Some("turn-0001"));
        assert_eq!(restored.raw_event_count(), 2);
        assert!(std::fs::read_to_string(&path)
            .expect("snapshot file should be readable")
            .contains("\"toolu-command-export-1\""));
        let ActiveModal::CommandOutput {
            title,
            body,
            is_error,
        } = &app.active_modal
        else {
            panic!("render snapshot command should open a command-output modal");
        };
        assert_eq!(title, "Render Snapshot");
        assert!(!*is_error);
        assert!(body.contains("Saved render session snapshot"), "{body}");
        assert!(body.contains("records: 2"), "{body}");
        assert!(body.contains("raw events: 2"), "{body}");
    }

    #[test]
    fn render_snapshot_command_uses_sanitized_default_session_path() {
        let mut app = App::new();
        let dir = tempfile::tempdir().expect("tempdir should be created");
        app.engine_config.cwd = dir.path().to_string_lossy().to_string();

        app.handle_submit("export default render session".to_string());
        app.handle_engine_message(SdkMessage::SystemInit {
            session_id: "session/unsafe:name".to_string(),
            model: "test-model".to_string(),
            tools: vec!["Bash".to_string()],
            task_id: None,
        });

        app.handle_command("snapshot");

        let expected = dir
            .path()
            .join(RENDER_SESSION_SNAPSHOT_DIR)
            .join("session-unsafe-name.json");
        assert!(
            expected.exists(),
            "default render snapshot path should exist: {}",
            expected.display()
        );
        let restored = App::load_render_session_snapshot(&expected)
            .expect("default render snapshot should load");
        assert_eq!(restored.session_id.as_deref(), Some("session/unsafe:name"));
        let ActiveModal::CommandOutput { body, is_error, .. } = &app.active_modal else {
            panic!("snapshot command should show a result modal");
        };
        assert!(!*is_error);
        assert!(
            body.contains(".mossen/render-sessions/session-unsafe-name.json"),
            "{body}"
        );
    }

    #[test]
    fn render_snapshot_autosave_writes_default_snapshot_when_session_has_content() {
        let mut app = App::new();
        let dir = tempfile::tempdir().expect("tempdir should be created");
        app.engine_config.cwd = dir.path().to_string_lossy().to_string();

        app.handle_submit("autosave render session".to_string());
        app.handle_engine_message(SdkMessage::SystemInit {
            session_id: "session-autosave".to_string(),
            model: "test-model".to_string(),
            tools: vec!["Bash".to_string()],
            task_id: None,
        });
        app.handle_engine_message(SdkMessage::ToolUseSummary {
            tool_name: "Bash".to_string(),
            tool_use_id: Some("toolu-autosave-1".to_string()),
            summary: "ok".to_string(),
            full_content: Some("full command log".to_string()),
            task_id: None,
        });

        let path = app
            .autosave_render_session_snapshot()
            .expect("autosave should succeed")
            .expect("non-empty session should write a snapshot");

        let expected = dir
            .path()
            .join(RENDER_SESSION_SNAPSHOT_DIR)
            .join("session-autosave.json");
        assert_eq!(path, expected);
        assert_eq!(app.render_snapshot_autosave_path.as_ref(), Some(&expected));
        assert!(app.render_snapshot_autosave_error.is_none());
        let restored = App::load_render_session_snapshot(&expected)
            .expect("autosaved render snapshot should load");
        assert_eq!(restored.session_id.as_deref(), Some("session-autosave"));
        assert_eq!(restored.raw_event_count(), 2);
        assert!(std::fs::read_to_string(&expected)
            .expect("autosave file should be readable")
            .contains("\"toolu-autosave-1\""));
    }

    #[test]
    fn render_snapshot_autosave_skips_empty_session() {
        let mut app = App::new();
        let dir = tempfile::tempdir().expect("tempdir should be created");
        app.engine_config.cwd = dir.path().to_string_lossy().to_string();

        let path = app
            .autosave_render_session_snapshot()
            .expect("empty-session autosave should not error");

        assert_eq!(path, None);
        assert!(app.render_snapshot_autosave_path.is_none());
        assert!(app.render_snapshot_autosave_error.is_none());
        assert!(
            !dir.path().join(RENDER_SESSION_SNAPSHOT_DIR).exists(),
            "empty sessions should not create render snapshot directories"
        );
    }

    #[test]
    fn resume_command_restores_latest_render_snapshot() {
        let dir = tempfile::tempdir().expect("tempdir should be created");
        let mut source = App::new();
        source.engine_config.cwd = dir.path().to_string_lossy().to_string();
        source.handle_submit("resume latest render session".to_string());
        source.handle_engine_message(SdkMessage::SystemInit {
            session_id: "session-resume-latest".to_string(),
            model: "test-model".to_string(),
            tools: vec!["Bash".to_string()],
            task_id: None,
        });
        source.handle_engine_message(SdkMessage::ToolUseSummary {
            tool_name: "Bash".to_string(),
            tool_use_id: Some("toolu-resume-latest-1".to_string()),
            summary: "ok".to_string(),
            full_content: Some("full command log".to_string()),
            task_id: None,
        });
        let saved_path = source
            .autosave_render_session_snapshot()
            .expect("source autosave should succeed")
            .expect("source snapshot should be written");

        let mut app = App::new();
        app.engine_config.cwd = dir.path().to_string_lossy().to_string();
        app.messages.push(MessageData {
            message_type: MessageType::Assistant,
            content: "old transcript before resume".to_string(),
            timestamp: None,
            is_streaming: false,
            tool_name: None,
            is_error: false,
            thinking: None,
            thinking_completed_at: None,
            full_content: None,
            expanded: false,
        });

        app.handle_command("resume");

        assert_eq!(
            app.engine_session_id.as_deref(),
            Some("session-resume-latest")
        );
        assert!(app
            .messages
            .iter()
            .any(|message| message.content.contains("resume latest render session")));
        assert!(!app
            .messages
            .iter()
            .any(|message| message.content.contains("old transcript before resume")));
        assert_eq!(
            app.render_record_parent_overrides
                .get(&1)
                .map(String::as_str),
            Some("toolu-resume-latest-1")
        );
        let ActiveModal::CommandOutput { body, is_error, .. } = &app.active_modal else {
            panic!("resume should show a restore result modal");
        };
        assert!(!*is_error);
        assert!(body.contains("Restored render session snapshot"), "{body}");
        assert!(
            body.contains(".mossen/render-sessions/session-resume-latest.json"),
            "{body}"
        );
    }

    #[test]
    fn resume_command_reports_when_no_render_snapshot_exists() {
        let dir = tempfile::tempdir().expect("tempdir should be created");
        let mut app = App::new();
        app.engine_config.cwd = dir.path().to_string_lossy().to_string();

        app.handle_command("resume");

        let ActiveModal::CommandOutput {
            title,
            body,
            is_error,
        } = &app.active_modal
        else {
            panic!("resume without snapshots should show a result modal");
        };
        assert_eq!(title, "Resume");
        assert!(!*is_error);
        assert!(
            body.contains("No render session snapshot is available to resume"),
            "{body}"
        );
        assert!(body.contains(RENDER_SESSION_SNAPSHOT_DIR), "{body}");
    }

    #[test]
    fn startup_restore_hydrates_latest_render_snapshot_when_empty() {
        let dir = tempfile::tempdir().expect("tempdir should be created");
        let mut source = App::new();
        source.engine_config.cwd = dir.path().to_string_lossy().to_string();
        source.handle_submit("startup restore render session".to_string());
        source.handle_engine_message(SdkMessage::SystemInit {
            session_id: "session-startup-restore".to_string(),
            model: "test-model".to_string(),
            tools: vec!["Bash".to_string()],
            task_id: None,
        });
        source.handle_engine_message(SdkMessage::ToolUseSummary {
            tool_name: "Bash".to_string(),
            tool_use_id: Some("toolu-startup-restore-1".to_string()),
            summary: "ok".to_string(),
            full_content: Some("startup full log".to_string()),
            task_id: None,
        });
        let saved_path = source
            .autosave_render_session_snapshot()
            .expect("source autosave should succeed")
            .expect("source snapshot should be written");

        let mut app = App::new();
        app.engine_config.cwd = dir.path().to_string_lossy().to_string();

        let restored_path = app
            .restore_latest_render_session_snapshot_on_startup()
            .expect("startup restore should not error")
            .expect("startup restore should find the latest snapshot");

        assert_eq!(restored_path, saved_path);
        assert_eq!(
            app.render_snapshot_startup_restore_status,
            RenderSnapshotStartupRestoreStatus::Restored
        );
        assert_eq!(
            app.render_snapshot_startup_restore_path.as_ref(),
            Some(&saved_path)
        );
        assert!(app.render_snapshot_startup_restore_error.is_none());
        assert_eq!(
            app.engine_session_id.as_deref(),
            Some("session-startup-restore")
        );
        assert!(app
            .messages
            .iter()
            .any(|message| message.content.contains("startup restore render session")));
        assert!(app.engine_rx.is_none());
        assert!(app.pending_submit.is_none());
    }

    #[test]
    fn startup_restore_skips_when_app_already_has_render_content() {
        let dir = tempfile::tempdir().expect("tempdir should be created");
        let mut source = App::new();
        source.engine_config.cwd = dir.path().to_string_lossy().to_string();
        source.handle_submit("snapshot should not replace existing startup UI".to_string());
        source.handle_engine_message(SdkMessage::SystemInit {
            session_id: "session-startup-skip".to_string(),
            model: "test-model".to_string(),
            tools: vec![],
            task_id: None,
        });
        source
            .autosave_render_session_snapshot()
            .expect("source autosave should succeed")
            .expect("source snapshot should be written");

        let mut app = App::new();
        app.engine_config.cwd = dir.path().to_string_lossy().to_string();
        app.messages.push(MessageData {
            message_type: MessageType::Assistant,
            content: "existing startup transcript".to_string(),
            timestamp: None,
            is_streaming: false,
            tool_name: None,
            is_error: false,
            thinking: None,
            thinking_completed_at: None,
            full_content: None,
            expanded: false,
        });

        let restored_path = app
            .restore_latest_render_session_snapshot_on_startup()
            .expect("startup restore skip should not error");

        assert!(restored_path.is_none());
        assert_eq!(
            app.render_snapshot_startup_restore_status,
            RenderSnapshotStartupRestoreStatus::Skipped
        );
        assert!(app.render_snapshot_startup_restore_path.is_none());
        assert!(app.render_snapshot_startup_restore_error.is_none());
        assert!(app
            .messages
            .iter()
            .any(|message| message.content.contains("existing startup transcript")));
        assert!(!app.messages.iter().any(|message| message
            .content
            .contains("snapshot should not replace existing startup UI")));
    }

    #[tokio::test]
    async fn run_skips_latest_render_snapshot_by_default() {
        let dir = tempfile::tempdir().expect("tempdir should be created");
        let mut source = App::new();
        source.engine_config.cwd = dir.path().to_string_lossy().to_string();
        source.handle_submit("fresh startup must not inherit this transcript".to_string());
        source.handle_engine_message(SdkMessage::SystemInit {
            session_id: "session-run-default-skip".to_string(),
            model: "test-model".to_string(),
            tools: vec!["Bash".to_string()],
            task_id: None,
        });
        source
            .autosave_render_session_snapshot()
            .expect("source autosave should succeed")
            .expect("source snapshot should be written");

        let mut app = App::new();
        app.engine_config.cwd = dir.path().to_string_lossy().to_string();
        app.should_quit = true;
        let terminal =
            Terminal::new(TestBackend::new(80, 20)).expect("test terminal should initialize");

        app.run(terminal)
            .await
            .expect("run should skip startup restore and exit cleanly");

        assert_eq!(
            app.render_snapshot_startup_restore_status,
            RenderSnapshotStartupRestoreStatus::Skipped
        );
        assert!(app.engine_session_id.is_none());
        assert!(app.messages.is_empty());
    }

    #[tokio::test]
    async fn run_restores_latest_render_snapshot_before_first_loop() {
        let dir = tempfile::tempdir().expect("tempdir should be created");
        let mut source = App::new();
        source.engine_config.cwd = dir.path().to_string_lossy().to_string();
        source.handle_submit("run startup restore render transcript".to_string());
        source.handle_engine_message(SdkMessage::SystemInit {
            session_id: "session-run-startup-restore".to_string(),
            model: "test-model".to_string(),
            tools: vec!["Bash".to_string()],
            task_id: None,
        });
        source.handle_engine_message(SdkMessage::ToolUseSummary {
            tool_name: "Bash".to_string(),
            tool_use_id: Some("toolu-run-startup-restore-1".to_string()),
            summary: "ok".to_string(),
            full_content: Some("startup run full log".to_string()),
            task_id: None,
        });
        source
            .autosave_render_session_snapshot()
            .expect("source autosave should succeed")
            .expect("source snapshot should be written");

        let mut app = App::new().with_startup_render_session_restore(true);
        app.engine_config.cwd = dir.path().to_string_lossy().to_string();
        app.should_quit = true;
        let terminal =
            Terminal::new(TestBackend::new(80, 20)).expect("test terminal should initialize");

        app.run(terminal)
            .await
            .expect("run should restore and exit cleanly");

        assert_eq!(
            app.render_snapshot_startup_restore_status,
            RenderSnapshotStartupRestoreStatus::Restored
        );
        assert_eq!(
            app.engine_session_id.as_deref(),
            Some("session-run-startup-restore")
        );
        assert!(app.messages.iter().any(|message| message
            .content
            .contains("run startup restore render transcript")));
        assert!(app.engine_rx.is_none());
        assert!(app.pending_submit.is_none());
    }

    #[tokio::test]
    async fn run_autosaves_render_snapshot_when_terminal_draw_fails() {
        let dir = tempfile::tempdir().expect("tempdir should be created");
        let mut app = App::new();
        app.engine_config.cwd = dir.path().to_string_lossy().to_string();
        app.handle_engine_message(SdkMessage::SystemInit {
            session_id: "session-run-draw-fail-autosave".to_string(),
            model: "test-model".to_string(),
            tools: vec!["Bash".to_string()],
            task_id: None,
        });
        app.handle_engine_message(SdkMessage::ToolUseSummary {
            tool_name: "Bash".to_string(),
            tool_use_id: Some("toolu-run-draw-fail-autosave-1".to_string()),
            summary: "draw failure recoverable marker".to_string(),
            full_content: Some("draw failure full log".to_string()),
            task_id: None,
        });

        let terminal = Terminal::new(FailingDrawBackend::new(80, 20))
            .expect("failing backend terminal should initialize");
        let result = app.run_event_loop_with_bus(terminal, EventBus::new()).await;

        assert!(
            result.is_err(),
            "run should surface the terminal draw error"
        );
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("terminal draw failed after render snapshot autosave attempt"));
        let path = app
            .render_snapshot_autosave_path
            .clone()
            .expect("draw failure should still autosave render snapshot");
        assert!(path.exists(), "snapshot path should exist: {path:?}");
        assert!(app.render_snapshot_autosave_error.is_none());
        let saved = std::fs::read_to_string(path).expect("snapshot should be readable");
        assert!(saved.contains("session-run-draw-fail-autosave"), "{saved}");
        assert!(saved.contains("draw failure recoverable marker"), "{saved}");
        assert!(app.engine_rx.is_none());
        assert!(app.pending_submit.is_none());
    }

    #[test]
    fn render_snapshot_restore_command_hydrates_current_tui_state() {
        let mut source = App::new();
        source.handle_submit("restore the render session".to_string());
        source.handle_engine_message(SdkMessage::SystemInit {
            session_id: "session-command-restore".to_string(),
            model: "test-model".to_string(),
            tools: vec!["Bash".to_string()],
            task_id: None,
        });
        source.handle_engine_message(SdkMessage::ToolUseSummary {
            tool_name: "Bash".to_string(),
            tool_use_id: Some("toolu-command-restore-1".to_string()),
            summary: "ok".to_string(),
            full_content: Some("full command log".to_string()),
            task_id: None,
        });
        source.approval_decisions.push(ApprovalDecisionModel {
            id: "decision-restore-1".to_string(),
            tool_name: "Bash".to_string(),
            decision: ApprovalDecisionKind::Allowed,
            detail: "cargo test".to_string(),
            anchor_block_id: Some("toolu-command-restore-1".to_string()),
        });
        source.handle_engine_message(SdkMessage::Result {
            terminal: "Completed".to_string(),
            cost_usd: None,
            duration_ms: Some(25),
            usage: None,
            task_id: None,
        });
        let dir = tempfile::tempdir().expect("tempdir should be created");
        let path = dir.path().join("snapshots").join("restore.json");
        let saved = source
            .save_render_session_snapshot(&path)
            .expect("source snapshot should save");

        let mut app = App::new();
        app.messages.push(MessageData {
            message_type: MessageType::Assistant,
            content: "old live transcript".to_string(),
            timestamp: None,
            is_streaming: false,
            tool_name: None,
            is_error: false,
            thinking: None,
            thinking_completed_at: None,
            full_content: None,
            expanded: false,
        });
        app.handle_command(&format!("render-snapshot restore {}", path.display()));

        assert_eq!(
            app.engine_session_id.as_deref(),
            Some("session-command-restore")
        );
        assert_eq!(app.current_render_turn_id, saved.current_turn_id);
        assert_eq!(app.next_render_turn_seq, saved.next_render_turn_seq);
        assert_eq!(app.raw_engine_event_history.len(), saved.raw_event_count());
        assert_eq!(app.approval_decisions, saved.records.approval_decisions);
        assert!(!app
            .messages
            .iter()
            .any(|message| message.content.contains("old live transcript")));
        assert!(app
            .messages
            .iter()
            .any(|message| message.content.contains("restore the render session")));
        assert_eq!(
            app.render_record_id_overrides.get(&1).map(String::as_str),
            Some("toolu-command-restore-1:result")
        );
        assert_eq!(
            app.render_record_parent_overrides
                .get(&1)
                .map(String::as_str),
            Some("toolu-command-restore-1")
        );
        assert_eq!(app.state.ui_stage, UiStage::Done);
        assert_eq!(app.state.turn_state, TurnState::Idle);

        let restored_snapshot = app.render_session_snapshot();
        assert_eq!(
            restored_snapshot.records.entries.len(),
            saved.records.entries.len()
        );
        assert_eq!(
            restored_snapshot.records.final_summaries.len(),
            saved.records.final_summaries.len()
        );
        assert_eq!(restored_snapshot.raw_event_count(), saved.raw_event_count());

        let ActiveModal::CommandOutput {
            title,
            body,
            is_error,
        } = &app.active_modal
        else {
            panic!("restore command should show a result modal");
        };
        assert_eq!(title, "Render Snapshot");
        assert!(!*is_error);
        assert!(body.contains("Restored render session snapshot"), "{body}");
        assert!(
            body.contains("engine execution not resumed"),
            "restore modal should make execution boundary explicit: {body}"
        );
    }

    #[test]
    fn render_snapshot_restore_preserves_interrupted_turn_state() {
        let mut source = App::new();
        source.handle_submit("interrupted stream".to_string());
        let snapshot = source.render_session_snapshot();

        let mut app = App::new();
        app.restore_render_session_snapshot(&snapshot);

        assert_eq!(app.current_render_turn_id.as_deref(), Some("turn-0001"));
        assert!(app.state.is_streaming);
        assert!(app.state.is_waiting_for_response);
        assert_eq!(app.state.turn_state, TurnState::Streaming);
        assert_eq!(app.state.ui_stage, UiStage::Thinking);
        assert_eq!(
            app.render_record_turn_overrides.get(&0).map(String::as_str),
            Some("turn-0001")
        );
    }

    #[test]
    fn app_layers_subagent_records_under_task_parent() {
        let mut app = App::new();
        let task_id = "agent-task-1".to_string();

        app.handle_engine_message(SdkMessage::SystemInit {
            session_id: "task-render-session".to_string(),
            model: "test-model".to_string(),
            tools: vec!["Bash".to_string()],
            task_id: Some(task_id.clone()),
        });
        app.handle_engine_message(SdkMessage::Assistant {
            message: AssistantMessage {
                role: Role::Assistant,
                content: vec![
                    ContentBlock::Text(TextBlock {
                        text: "checking from subagent".to_string(),
                    }),
                    ContentBlock::ToolUse(ToolUseBlock {
                        id: "toolu-agent-1".to_string(),
                        name: "Bash".to_string(),
                        input: serde_json::json!({ "command": "cargo test" }),
                    }),
                ],
                uuid: Some("assistant-agent-1".to_string()),
                model: None,
                stop_reason: Some("tool_use".to_string()),
                extra: HashMap::new(),
            },
            usage: None,
            task_id: Some(task_id.clone()),
        });
        app.handle_engine_message(SdkMessage::ToolUseSummary {
            tool_name: "Bash".to_string(),
            tool_use_id: Some("toolu-agent-1".to_string()),
            summary: "ok".to_string(),
            full_content: None,
            task_id: Some(task_id.clone()),
        });
        app.handle_engine_message(SdkMessage::Result {
            terminal: "Completed".to_string(),
            cost_usd: None,
            duration_ms: Some(50),
            usage: None,
            task_id: Some(task_id.clone()),
        });

        let records = app.render_session_snapshot().records;
        let relations = records.relation_index();
        let task_root = "task:agent-task-1";
        let task_tool = "agent-task-1:toolu-agent-1";
        let task_tool_result = "agent-task-1:toolu-agent-1:result";
        let task_result = "task:agent-task-1:result";

        assert!(records.record_by_id(task_root).is_some());
        assert_eq!(
            records
                .record_by_id(task_tool)
                .and_then(|record| record.parent_id.as_deref()),
            Some(task_root)
        );
        assert_eq!(
            records
                .record_by_id(task_tool_result)
                .and_then(|record| record.parent_id.as_deref()),
            Some(task_tool)
        );
        assert_eq!(
            records
                .record_by_id(task_result)
                .and_then(|record| record.parent_id.as_deref()),
            Some(task_root)
        );
        assert_eq!(relations.orphan_count(), 0);
        assert!(
            relations
                .children_of(task_root)
                .is_some_and(|children| children.iter().any(|child| child == task_tool)
                    && children.iter().any(|child| child == task_result)),
            "{relations:?}"
        );
    }

    #[test]
    fn render_surface_models_file_permission_as_approval_blocking() {
        let mut app = App::new();
        app.active_modal = ActiveModal::PermissionRequest(PermissionPromptState::new(
            PermissionKind::FileWrite {
                path: "/tmp/output.md".to_string(),
            },
            "Write",
        ));

        let surface = app.render_surface_model();
        let approval = surface.approvals.first().expect("approval should exist");

        assert_eq!(approval.title, "File Write");
        assert_eq!(approval.detail_label, "Write Path");
        assert_eq!(approval.detail, "/tmp/output.md");
        assert_eq!(
            surface.blocking.as_ref().map(|blocking| blocking.kind),
            Some(BlockingKind::Approval)
        );
        assert_eq!(surface.blocking, surface.footer.blocking);
    }

    #[test]
    fn render_surface_models_mcp_channel_approval_as_approval_blocking() {
        let mut app = App::new();
        app.active_modal = ActiveModal::McpChannelApproval(
            mossen_agent::mcp::channel_approval::ChannelApprovalRequest {
                id: "mcp-approval-1".to_string(),
                server_name: "filesystem".to_string(),
                plugin: Some("local-plugin".to_string()),
                marketplace: Some("dev".to_string()),
                reason: "server wants a local channel".to_string(),
            },
        );

        let surface = app.render_surface_model();
        let approval = surface.approvals.first().expect("approval should exist");

        assert_eq!(approval.tool_name, "MCP Channel");
        assert_eq!(approval.detail_label, "Server");
        assert_eq!(approval.detail, "filesystem");
        assert_eq!(
            surface.blocking.as_ref().map(|blocking| blocking.kind),
            Some(BlockingKind::Approval)
        );
        assert_eq!(surface.blocking, surface.footer.blocking);
    }

    #[test]
    fn mcp_channel_approval_anchors_to_matching_mcp_tool() {
        let mut app = App::new();
        app.handle_engine_message(SdkMessage::Assistant {
            message: AssistantMessage {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolUse(ToolUseBlock {
                    id: "toolu-mcp-filesystem-1".to_string(),
                    name: "mcp__filesystem__read_file".to_string(),
                    input: serde_json::json!({ "path": "Cargo.toml" }),
                })],
                uuid: Some("assistant-mcp".to_string()),
                model: None,
                stop_reason: Some("tool_use".to_string()),
                extra: HashMap::new(),
            },
            usage: None,
            task_id: None,
        });
        app.active_modal = ActiveModal::McpChannelApproval(
            mossen_agent::mcp::channel_approval::ChannelApprovalRequest {
                id: "mcp-approval-anchored".to_string(),
                server_name: "filesystem".to_string(),
                plugin: Some("local-plugin".to_string()),
                marketplace: Some("dev".to_string()),
                reason: "server wants a local channel".to_string(),
            },
        );

        let surface = app.render_surface_model();
        let approval = surface.approvals.first().expect("approval should exist");
        assert_eq!(
            approval.anchor_block_id.as_deref(),
            Some("toolu-mcp-filesystem-1")
        );

        app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let decision = app
            .approval_decisions
            .last()
            .expect("MCP approval decision should be recorded");
        assert_eq!(
            decision.anchor_block_id.as_deref(),
            Some("toolu-mcp-filesystem-1")
        );

        let transcript = app.render_transcript_model();
        let tool_index = transcript
            .blocks
            .iter()
            .position(|block| block.id == "toolu-mcp-filesystem-1")
            .expect("MCP tool block should remain visible");
        let decision_index = transcript
            .blocks
            .iter()
            .position(|block| block.kind == RenderBlockKind::ApprovalDecision)
            .expect("decision should be inserted into transcript");
        assert_eq!(decision_index, tool_index + 1);
    }

    #[test]
    fn render_surface_models_idle_return_as_blocking_without_approval() {
        let mut app = App::new();
        app.active_modal = ActiveModal::IdleReturn("away for 48m".to_string());

        let surface = app.render_surface_model();

        assert!(surface.approvals.is_empty());
        assert_eq!(
            surface.blocking.as_ref().map(|blocking| blocking.kind),
            Some(BlockingKind::IdleReturn)
        );
        assert_eq!(surface.blocking, surface.footer.blocking);
    }

    #[test]
    fn render_surface_models_error_command_output_as_blocking() {
        let mut app = App::new();
        app.active_modal = ActiveModal::CommandOutput {
            title: "Render error".to_string(),
            body: "layout failed".to_string(),
            is_error: true,
        };

        let surface = app.render_surface_model();

        assert!(surface.approvals.is_empty());
        assert_eq!(
            surface.blocking.as_ref().map(|blocking| blocking.kind),
            Some(BlockingKind::Error)
        );
        assert_eq!(surface.blocking, surface.footer.blocking);
    }

    #[test]
    fn spinner_status_text_uses_surface_blocking_state() {
        let mut app = App::new();
        app.state.is_streaming = true;
        app.active_modal = ActiveModal::PermissionRequest(PermissionPromptState::new(
            PermissionKind::Shell {
                command: "cargo test".to_string(),
            },
            "Bash",
        ));

        let surface = app.render_surface_model();
        let text = app.spinner_status_text(&surface);

        assert!(text.starts_with("Waiting approval"), "{text}");
    }

    #[test]
    fn streaming_flag_drives_footer_and_spinner_state() {
        let mut app = App::new();
        app.state.is_streaming = true;
        app.messages.push(MessageData {
            message_type: MessageType::Assistant,
            content: "正在分析".to_string(),
            timestamp: None,
            is_streaming: true,
            tool_name: None,
            is_error: false,
            thinking: None,
            thinking_completed_at: None,
            full_content: None,
            expanded: false,
        });

        let surface = app.render_surface_model();
        let text = app.spinner_status_text(&surface);

        assert_eq!(surface.footer.turn_state.as_deref(), Some("streaming"));
        assert!(text.starts_with("Thinking"), "{text}");
    }

    #[test]
    fn engine_activity_clears_spinner_stalled_without_resetting_elapsed_time() {
        let mut app = App::new();
        app.state.is_streaming = true;
        app.spinner.reset();
        std::thread::sleep(Duration::from_millis(2));
        let elapsed_before = app.spinner.elapsed();
        app.spinner.set_stalled(true);

        app.handle_engine_message(SdkMessage::StreamEvent {
            event: StreamEventData::MessageStart,
            task_id: None,
        });

        assert!(!app.spinner.is_stalled());
        assert!(app.spinner.elapsed() >= elapsed_before);
        assert!(app.spinner.idle_for() < Duration::from_secs(1));
    }

    #[test]
    fn accepted_tool_permission_persists_as_semantic_decision_block() {
        let mut app = App::new();
        app.messages.push(MessageData {
            message_type: MessageType::ToolUse,
            content: serde_json::json!({ "command": "ls -la" }).to_string(),
            timestamp: None,
            is_streaming: false,
            tool_name: Some("Bash".to_string()),
            is_error: false,
            thinking: None,
            thinking_completed_at: None,
            full_content: None,
            expanded: false,
        });
        app.active_modal = ActiveModal::ToolUseConfirm {
            confirm: ToolUseConfirm {
                tool_use_id: "tool-call-1".to_string(),
                tool_name: "Bash".to_string(),
                raw_input: serde_json::json!({ "command": "ls -la" }),
                input_summary: "ls -la".to_string(),
                risk_level: 0,
            },
            prompt: PermissionPromptState::new(
                PermissionKind::Shell {
                    command: "ls -la".to_string(),
                },
                "Bash",
            ),
        };

        app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let transcript =
            RenderTranscript::from_messages_and_decisions(&app.messages, &app.approval_decisions);
        let decision = transcript.blocks.iter().find_map(|block| {
            block.nodes.iter().find_map(|node| match node {
                RenderNode::ApprovalDecision(decision) => Some(decision),
                _ => None,
            })
        });

        let decision = decision.expect("approval decision should remain in transcript");
        assert_eq!(decision.tool_name, "Bash");
        assert_eq!(decision.decision, ApprovalDecisionKind::Allowed);
        assert_eq!(decision.detail, "ls -la");
    }

    #[test]
    fn edit_command_approval_prefills_prompt_and_denies_pending_tool_request() {
        let mut app = App::new();
        app.messages.push(MessageData {
            message_type: MessageType::ToolUse,
            content: serde_json::json!({ "command": "cargo test" }).to_string(),
            timestamp: None,
            is_streaming: false,
            tool_name: Some("Bash".to_string()),
            is_error: false,
            thinking: None,
            thinking_completed_at: None,
            full_content: None,
            expanded: false,
        });
        let (reply_tx, mut reply_rx) = tokio::sync::oneshot::channel();
        app.active_permission_responder = Some(reply_tx);
        app.active_modal = ActiveModal::ToolUseConfirm {
            confirm: ToolUseConfirm {
                tool_use_id: "tool-call-1".to_string(),
                tool_name: "Bash".to_string(),
                raw_input: serde_json::json!({ "command": "cargo test" }),
                input_summary: "cargo test".to_string(),
                risk_level: 0,
            },
            prompt: PermissionPromptState::new(
                PermissionKind::Shell {
                    command: "cargo test".to_string(),
                },
                "Bash",
            ),
        };

        app.dispatch_key_for_test(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        app.dispatch_key_for_test(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert!(matches!(app.active_modal, ActiveModal::None));
        assert!(app.active_permission_responder.is_none());
        assert_eq!(
            app.prompt.input.value,
            "Edit command before running: cargo test"
        );
        assert!(matches!(
            reply_rx.try_recv(),
            Ok(mossen_agent::types::PermissionDecision::Deny)
        ));
        let decision = app
            .approval_decisions
            .last()
            .expect("approval decision should be recorded");
        assert_eq!(decision.decision, ApprovalDecisionKind::Cancelled);
        assert!(decision.detail.contains("edit command requested"));
    }

    #[test]
    fn footer_state_reports_product_stage_after_tool_use() {
        let mut app = App::new();
        app.state.is_streaming = true;
        app.state.ui_stage = UiStage::RunningCommand;
        app.messages.push(MessageData {
            message_type: MessageType::ToolUse,
            content: "command  cargo test".to_string(),
            timestamp: None,
            is_streaming: false,
            tool_name: Some("Bash".to_string()),
            is_error: false,
            thinking: None,
            thinking_completed_at: None,
            full_content: None,
            expanded: false,
        });

        assert_eq!(app.turn_state_label(), "running command");
    }

    #[test]
    fn pending_tool_approval_preempts_idle_return_dialog() {
        let mut app = App::new();
        app.active_modal = ActiveModal::IdleReturn("away 48m".to_string());
        app.services.idle_return_state =
            Some(IdleReturnDialogState::new(Duration::from_secs(48 * 60)));

        let (tx, rx) = tokio::sync::mpsc::channel(1);
        let (reply_tx, _reply_rx) = tokio::sync::oneshot::channel();
        tx.try_send(PermissionRequest {
            tool_id: "tool-1".to_string(),
            tool_name: "Bash".to_string(),
            input: serde_json::json!({ "command": "ls -la" }),
            responder: reply_tx,
        })
        .expect("permission request should enqueue");
        app.permission_rx = Some(rx);

        app.dispatch_tick_for_test();

        let ActiveModal::ToolUseConfirm { confirm, .. } = &app.active_modal else {
            panic!("expected tool approval modal");
        };
        assert_eq!(confirm.input_summary, "ls -la");
        assert!(app.active_permission_responder.is_some());
        assert!(app.services.idle_return_state.is_none());
        assert_eq!(app.turn_state_label(), "waiting approval");
        assert_eq!(app.state.ui_stage, UiStage::WaitingApproval);
    }

    #[test]
    fn permission_summary_truncates_multibyte_input_without_panic() {
        let mut app = App::new();
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        let (reply_tx, _reply_rx) = tokio::sync::oneshot::channel();
        let prompt = "逐行阅读代码，理解架构和实现细节。".repeat(30);

        tx.try_send(PermissionRequest {
            tool_id: "tool-1".to_string(),
            tool_name: "Task".to_string(),
            input: serde_json::json!({
                "description": "逐行阅读代码分析",
                "prompt": prompt,
            }),
            responder: reply_tx,
        })
        .expect("permission request should enqueue");
        app.permission_rx = Some(rx);

        app.dispatch_tick_for_test();

        let ActiveModal::ToolUseConfirm { confirm, .. } = &app.active_modal else {
            panic!("expected tool approval modal");
        };
        assert!(confirm.input_summary.ends_with('…'));
        assert!(confirm.input_summary.chars().count() <= 241);
    }

    #[test]
    fn session_permission_gate_applies_rules_before_fallback() {
        let gate = SessionPermissionGate::new(
            SessionPermissionRules {
                allow: vec!["Bash cargo test".to_string()],
                deny: vec!["Write".to_string()],
            },
            std::sync::Arc::new(mossen_agent::types::AllowAllGate),
        );

        let denied = block_on_current_runtime(mossen_agent::types::PermissionGate::check(
            &gate,
            "Write",
            "tool-1",
            &serde_json::json!({ "file_path": "src/lib.rs" }),
        ));
        assert_eq!(denied, mossen_agent::types::PermissionDecision::Deny);

        let allowed = block_on_current_runtime(mossen_agent::types::PermissionGate::check(
            &gate,
            "Bash",
            "tool-2",
            &serde_json::json!({ "command": "cargo test -q" }),
        ));
        assert_eq!(allowed, mossen_agent::types::PermissionDecision::Allow);

        let fallback = block_on_current_runtime(mossen_agent::types::PermissionGate::check(
            &gate,
            "Read",
            "tool-3",
            &serde_json::json!({ "file_path": "src/main.rs" }),
        ));
        assert_eq!(fallback, mossen_agent::types::PermissionDecision::Allow);
    }
}

#[cfg(test)]
mod think_split_tests {
    use super::split_thinking_and_content;

    #[test]
    fn passthrough_when_no_marker() {
        let (think, content) = split_thinking_and_content("hello world");
        assert!(think.is_none());
        assert_eq!(content, "hello world");
    }

    #[test]
    fn closed_block_splits_cleanly() {
        let (think, content) =
            split_thinking_and_content("<think>weighing options</think>\n\nthe answer is 42");
        assert_eq!(think.as_deref(), Some("weighing options"));
        assert_eq!(content, "the answer is 42");
    }

    #[test]
    fn unclosed_block_streams_to_thinking() {
        let (think, content) = split_thinking_and_content("<think>still reasoning…");
        assert_eq!(think.as_deref(), Some("still reasoning…"));
        assert_eq!(content, "");
    }

    #[test]
    fn multiple_blocks_concatenate() {
        let (think, content) = split_thinking_and_content(
            "<think>step one</think>partial<think>step two</think>final",
        );
        assert_eq!(think.as_deref(), Some("step one\nstep two"));
        assert_eq!(content, "partialfinal");
    }

    #[test]
    fn chinese_reasoning_and_markdown_remain_utf8_safe() {
        let (think, content) = split_thinking_and_content(
            "<think>逐行阅读代码，先理解入口。</think>\n\n```rust\nfn main() {}\n```\n结论",
        );
        assert_eq!(think.as_deref(), Some("逐行阅读代码，先理解入口。"));
        assert!(content.contains("```rust"));
        assert!(content.is_char_boundary(content.len()));
    }
}
