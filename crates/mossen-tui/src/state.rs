//! Global TUI state management.
//!
//! Translates the React Context + AppStateStore pattern into a shared state
//! struct with watch-based change notification.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{watch, RwLock};

use mossen_tools::todo::TodoItem;
use mossen_utils::string_utils::truncate_chars_with_suffix;

use crate::render_model::FooterRenderConfig;
use crate::theme::ThemeName;

/// Expanded view mode for the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExpandedView {
    #[default]
    None,
    Tasks,
    Teammates,
}

/// View selection mode for footer navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewSelectionMode {
    #[default]
    Normal,
    Footer,
    Expanded,
}

/// Connection status for remote sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConnectionStatus {
    #[default]
    Disconnected,
    Connecting,
    Connected,
    Error,
}

/// Input mode for the prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputMode {
    #[default]
    Normal,
    Bash,
    Vim,
    Search,
    Command,
}

/// Turn lifecycle state for Ctrl+C interrupt protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TurnState {
    #[default]
    Idle,
    Streaming,
    Cancelling,
    Cancelled,
}

/// Product-level agent stage shown by the terminal render surface.
///
/// This is intentionally broader than low-level stream state: it models the
/// engineering workflow users care about while a turn is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UiStage {
    #[default]
    Idle,
    Thinking,
    Planning,
    ReadingRepo,
    EditingFiles,
    WaitingApproval,
    RunningCommand,
    ReviewingResult,
    Retrying,
    Done,
    Failed,
    Cancelled,
}

impl UiStage {
    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Thinking => "thinking",
            Self::Planning => "planning",
            Self::ReadingRepo => "reading repo",
            Self::EditingFiles => "editing files",
            Self::WaitingApproval => "waiting approval",
            Self::RunningCommand => "running command",
            Self::ReviewingResult => "reviewing result",
            Self::Retrying => "retrying",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn is_blocking(self) -> bool {
        matches!(self, Self::WaitingApproval | Self::Failed)
    }

    pub fn allows_streaming(self) -> bool {
        !matches!(
            self,
            Self::Idle | Self::Done | Self::Failed | Self::Cancelled
        )
    }

    pub fn from_tool_name(tool_name: &str) -> Self {
        match tool_name.to_ascii_lowercase().as_str() {
            "bash" | "powershell" => Self::RunningCommand,
            "read"
            | "grep"
            | "glob"
            | "webfetch"
            | "websearch"
            | "readmcpresource"
            | "listmcpresources"
            | "listmcpresourcestool"
            | "toolsearch" => Self::ReadingRepo,
            "write" | "edit" | "multiedit" | "notebookedit" => Self::EditingFiles,
            "todowrite" | "exitplanmode" | "taskcreate" | "tasklist" | "taskget" | "taskupdate" => {
                Self::Planning
            }
            "task" | "agent" | "taskoutput" | "taskstop" | "skill" => Self::Planning,
            _ if tool_name.to_ascii_lowercase().starts_with("mcp__")
                || tool_name.contains("__") =>
            {
                Self::ReadingRepo
            }
            _ => Self::ReviewingResult,
        }
    }

    pub fn from_terminal(terminal: &str) -> Self {
        let normalized = terminal.to_ascii_lowercase();
        if normalized.contains("abort") || normalized.contains("cancel") {
            Self::Cancelled
        } else if normalized.contains("modelerror")
            || normalized.contains("error")
            || normalized.contains("failed")
            || normalized.contains("prevented")
        {
            Self::Failed
        } else if normalized.contains("retry") {
            Self::Retrying
        } else {
            Self::Done
        }
    }
}

/// Latest high-signal rendering activity derived from structured render events.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RenderActivityState {
    pub current: Option<RenderActivity>,
}

impl RenderActivityState {
    pub fn set(&mut self, activity: RenderActivity) {
        self.current = Some(activity);
    }

    pub fn clear(&mut self) {
        self.current = None;
    }

    pub fn status_line(&self) -> Option<String> {
        self.current.as_ref().map(RenderActivity::status_line)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderActivity {
    AssistantMessage {
        bytes: usize,
    },
    Thinking {
        bytes: usize,
    },
    ToolInput {
        bytes: usize,
    },
    Tool {
        name: String,
    },
    Plan {
        step_count: usize,
        completed_count: usize,
        active_count: usize,
        pending_count: usize,
        blocked_count: usize,
        active_step: Option<String>,
    },
    FileChange {
        file_count: usize,
        additions: usize,
        deletions: usize,
    },
    Diff {
        file_count: usize,
        additions: usize,
        deletions: usize,
    },
    CommandStarted {
        command: Option<String>,
        cwd: Option<String>,
    },
    CommandOutput {
        stream: String,
        bytes: usize,
        preview_lines: usize,
        hidden_lines: usize,
        total_lines: Option<usize>,
        full_log_available: bool,
    },
    CommandFinished {
        exit_code: Option<i64>,
        duration_ms: Option<u64>,
    },
    BackgroundTask {
        task_id: String,
        status: String,
        command: Option<String>,
        preview_lines: usize,
        hidden_lines: usize,
        exit_code: Option<i64>,
    },
    Approval {
        tool_name: String,
    },
    Error {
        source: String,
        summary: String,
    },
    Retry {
        attempt: u32,
        max_retries: u32,
        retry_in_ms: u64,
    },
    Compact {
        before_token_count: u64,
        after_token_count: u64,
    },
    CompactStatus {
        status: String,
        dry_run: bool,
        reason: Option<String>,
    },
    ClearStatus {
        status: String,
        dry_run: bool,
        reason: Option<String>,
    },
    SlashCommand {
        command: String,
        status: String,
        summary: String,
        error: Option<String>,
    },
    Final {
        success: bool,
    },
}

impl RenderActivity {
    pub fn status_line(&self) -> String {
        match self {
            Self::AssistantMessage { bytes } => format!("message: {bytes} bytes"),
            Self::Thinking { bytes } => format!("thinking: {bytes} bytes"),
            Self::ToolInput { bytes } => format!("tool input: {bytes} bytes"),
            Self::Tool { name } => format!("tool: {}", truncate_activity(name)),
            Self::Plan {
                step_count,
                completed_count,
                active_count,
                pending_count,
                blocked_count,
                active_step,
            } => {
                let mut parts = vec![format!("plan: {step_count} steps")];
                if *completed_count > 0 {
                    parts.push(format!("{completed_count} done"));
                }
                if *active_count > 0 {
                    parts.push(format!("{active_count} active"));
                }
                if *pending_count > 0 {
                    parts.push(format!("{pending_count} pending"));
                }
                if *blocked_count > 0 {
                    parts.push(format!("{blocked_count} blocked"));
                }
                if let Some(active) = active_step.as_deref() {
                    parts.push(truncate_activity(active));
                }
                parts.join(", ")
            }
            Self::FileChange {
                file_count,
                additions,
                deletions,
            } => format!("files: {file_count} +{additions} -{deletions}"),
            Self::Diff {
                file_count,
                additions,
                deletions,
            } => format!("diff: {file_count} +{additions} -{deletions}"),
            Self::CommandStarted { command, cwd } => {
                let command = command
                    .as_deref()
                    .map(truncate_activity)
                    .unwrap_or_else(|| "<command>".to_string());
                match cwd.as_deref().filter(|cwd| !cwd.trim().is_empty()) {
                    Some(cwd) => format!("cmd: {command} @ {}", truncate_activity(cwd)),
                    None => format!("cmd: {command}"),
                }
            }
            Self::CommandOutput {
                stream,
                bytes,
                preview_lines,
                hidden_lines,
                total_lines,
                full_log_available,
            } => command_output_activity_line(
                stream,
                *bytes,
                *preview_lines,
                *hidden_lines,
                *total_lines,
                *full_log_available,
            ),
            Self::CommandFinished {
                exit_code,
                duration_ms,
            } => {
                let status = exit_code
                    .map(|code| format!("exit {code}"))
                    .unwrap_or_else(|| "finished".to_string());
                match duration_ms {
                    Some(ms) => format!("cmd: {status} in {ms}ms"),
                    None => format!("cmd: {status}"),
                }
            }
            Self::BackgroundTask {
                task_id,
                status,
                command,
                preview_lines,
                hidden_lines,
                exit_code,
            } => {
                let mut parts = vec![
                    format!("background task {status}: {}", truncate_activity(task_id)),
                    format!("output {preview_lines}/{hidden_lines}"),
                ];
                if let Some(command) = command.as_deref() {
                    parts.push(truncate_activity(command));
                }
                if let Some(code) = exit_code {
                    parts.push(format!("exit {code}"));
                }
                parts.join(", ")
            }
            Self::Approval { tool_name } => {
                format!("approval: {}", truncate_activity(tool_name))
            }
            Self::Error { source, summary } => {
                format!(
                    "error: {} {}",
                    truncate_activity(source),
                    truncate_activity(summary)
                )
            }
            Self::Retry {
                attempt,
                max_retries,
                retry_in_ms,
            } => format!("retry: {attempt}/{max_retries} in {retry_in_ms}ms"),
            Self::Compact {
                before_token_count,
                after_token_count,
            } => format!("compact: {before_token_count}->{after_token_count}"),
            Self::CompactStatus {
                status,
                dry_run,
                reason,
            } => {
                let mut parts = vec![format!("compact: {status}")];
                if *dry_run {
                    parts.push("dry run".to_string());
                }
                if let Some(reason) = reason.as_deref().filter(|reason| !reason.trim().is_empty()) {
                    parts.push(truncate_activity(reason));
                }
                parts.join(", ")
            }
            Self::ClearStatus {
                status,
                dry_run,
                reason,
            } => {
                let mut parts = vec![format!("clear: {status}")];
                if *dry_run {
                    parts.push("dry run".to_string());
                }
                if let Some(reason) = reason.as_deref().filter(|reason| !reason.trim().is_empty()) {
                    parts.push(truncate_activity(reason));
                }
                parts.join(", ")
            }
            Self::SlashCommand {
                command,
                status,
                error,
                ..
            } => {
                let mut parts = vec![format!(
                    "/{}: {}",
                    truncate_activity(command),
                    truncate_activity(status)
                )];
                if let Some(error) = error.as_deref().filter(|error| !error.trim().is_empty()) {
                    parts.push(truncate_activity(error));
                }
                parts.join(", ")
            }
            Self::Final { success } => {
                if *success {
                    "final: done".to_string()
                } else {
                    "final: failed".to_string()
                }
            }
        }
    }
}

fn truncate_activity(text: &str) -> String {
    truncate_chars_with_suffix(text.trim(), 64, "...")
}

fn command_output_activity_line(
    stream: &str,
    bytes: usize,
    preview_lines: usize,
    hidden_lines: usize,
    total_lines: Option<usize>,
    full_log_available: bool,
) -> String {
    let stream = truncate_activity(stream);
    if preview_lines == 0 && hidden_lines == 0 && total_lines.is_none() {
        return format!("cmd output: {stream} {bytes} bytes");
    }

    let mut parts = vec![format!(
        "{} shown",
        pluralize_activity(preview_lines, "line")
    )];
    if hidden_lines > 0 {
        parts.push(format!(
            "{} hidden",
            pluralize_activity(hidden_lines, "line")
        ));
    }
    if let Some(total) = total_lines {
        parts.push(format!("{} total", pluralize_activity(total, "line")));
    }
    if full_log_available {
        parts.push("full log".to_string());
    }
    format!("cmd output: {stream} {}", parts.join(", "))
}

fn pluralize_activity(count: usize, unit: &str) -> String {
    if count == 1 {
        format!("1 {unit}")
    } else {
        format!("{count} {unit}s")
    }
}

/// Teammate (sub-agent) lifecycle state.
#[derive(Debug, Clone)]
pub enum TeammateState {
    Running,
    Completed(String),
    Failed(String),
}

/// User-facing slash entry surfaced by `/` typeahead and `/help`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlashCommandInfo {
    pub name: String,
    pub description: String,
    pub category: String,
    pub aliases: Vec<String>,
    pub argument_hint: String,
    pub kind: SlashCommandKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlashCommandKind {
    Command,
    Skill,
}

/// Lightweight MCP status used by the TUI status bar and `/mcp` panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServerStatus {
    pub name: String,
    pub state: McpConnectionState,
    pub transport: String,
    pub tools_count: usize,
    pub prompts_count: usize,
    pub resources_count: usize,
    pub scope: String,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpConnectionState {
    Configured,
    Connected,
    Pending,
    Failed,
    NeedsAuth,
    Disabled,
}

impl McpConnectionState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Configured => "configured",
            Self::Connected => "connected",
            Self::Pending => "pending",
            Self::Failed => "failed",
            Self::NeedsAuth => "needs-auth",
            Self::Disabled => "disabled",
        }
    }
}

/// Task list state, driven by TodoWrite tool results.
#[derive(Debug, Clone, Default)]
pub struct TaskListState {
    pub tasks: Vec<TodoItem>,
    pub last_update: Option<std::time::Instant>,
}

/// Global application state — mirrors the TS AppState store fields.
#[derive(Debug, Clone)]
pub struct AppState {
    // --- Basic settings ---
    pub verbose: bool,
    pub theme: ThemeName,

    // --- UI state ---
    pub expanded_view: ExpandedView,
    pub is_brief_only: bool,
    pub view_selection_mode: ViewSelectionMode,
    pub input_mode: InputMode,

    // --- Session ---
    pub remote_connection_status: ConnectionStatus,
    pub remote_session_url: Option<String>,

    // --- Model ---
    pub current_model: Option<String>,
    pub fast_mode: bool,
    pub thinking_enabled: bool,

    // --- Agent ---
    pub agent_name: Option<String>,

    // --- Notifications ---
    pub notification_count: usize,
    pub active_overlays: HashSet<String>,

    // --- Task state ---
    pub foreground_task_id: Option<String>,
    pub background_task_count: usize,
    pub task_list: TaskListState,

    // --- Teammate (sub-agent) state ---
    pub teammate_messages: HashMap<String, Vec<String>>,
    pub teammate_states: HashMap<String, TeammateState>,

    // --- Slash commands / skills ---
    pub all_slash_commands: Vec<SlashCommandInfo>,

    // --- Compact UI ---
    pub compact_in_progress: bool,
    pub compact_progress: Option<String>,
    pub compact_notice_until: Option<std::time::Instant>,

    // --- MCP UI ---
    pub mcp_servers: Vec<McpServerStatus>,

    // --- Footer/status-line UI ---
    pub footer_config: FooterRenderConfig,

    // --- Turn state (Ctrl+C protocol) ---
    pub turn_state: TurnState,
    pub ui_stage: UiStage,
    pub render_activity: RenderActivityState,

    // --- Messages ---
    pub message_count: usize,
    pub is_streaming: bool,
    pub is_waiting_for_response: bool,

    // --- Terminal ---
    pub terminal_width: u16,
    pub terminal_height: u16,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            verbose: false,
            theme: ThemeName::default(),
            expanded_view: ExpandedView::default(),
            is_brief_only: false,
            view_selection_mode: ViewSelectionMode::default(),
            input_mode: InputMode::default(),
            remote_connection_status: ConnectionStatus::default(),
            remote_session_url: None,
            current_model: None,
            fast_mode: false,
            thinking_enabled: true,
            agent_name: None,
            notification_count: 0,
            active_overlays: HashSet::new(),
            foreground_task_id: None,
            background_task_count: 0,
            task_list: TaskListState::default(),
            teammate_messages: HashMap::new(),
            teammate_states: HashMap::new(),
            all_slash_commands: Vec::new(),
            compact_in_progress: false,
            compact_progress: None,
            compact_notice_until: None,
            mcp_servers: Vec::new(),
            footer_config: FooterRenderConfig::default(),
            turn_state: TurnState::default(),
            ui_stage: UiStage::default(),
            render_activity: RenderActivityState::default(),
            message_count: 0,
            is_streaming: false,
            is_waiting_for_response: false,
            terminal_width: 80,
            terminal_height: 24,
        }
    }
}

/// Thread-safe state store with change notification.
///
/// Translates the TS `Store<T>` pattern (getState/setState/subscribe).
#[derive(Clone)]
pub struct AppStore {
    state: Arc<RwLock<AppState>>,
    notify_tx: Arc<watch::Sender<u64>>,
    notify_rx: watch::Receiver<u64>,
    version: Arc<std::sync::atomic::AtomicU64>,
}

impl AppStore {
    pub fn new(initial: AppState) -> Self {
        let (notify_tx, notify_rx) = watch::channel(0u64);
        Self {
            state: Arc::new(RwLock::new(initial)),
            notify_tx: Arc::new(notify_tx),
            notify_rx,
            version: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    /// Read current state.
    pub async fn get_state(&self) -> AppState {
        self.state.read().await.clone()
    }

    /// Update state with a closure and notify subscribers.
    pub async fn set_state(&self, f: impl FnOnce(&mut AppState)) {
        let mut state = self.state.write().await;
        f(&mut state);
        let v = self
            .version
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let _ = self.notify_tx.send(v + 1);
    }

    /// Subscribe to state changes. Returns a receiver that wakes on each update.
    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.notify_rx.clone()
    }
}

impl Default for AppStore {
    fn default() -> Self {
        Self::new(AppState::default())
    }
}

#[cfg(test)]
mod tests {
    use super::UiStage;

    #[test]
    fn task_workitem_tools_keep_footer_in_active_planning_stage() {
        for tool_name in [
            "TaskCreate",
            "TaskList",
            "TaskGet",
            "TaskUpdate",
            "TaskOutput",
            "TaskStop",
        ] {
            assert_eq!(
                UiStage::from_tool_name(tool_name),
                UiStage::Planning,
                "{tool_name} should not fall back to reviewing result while active"
            );
        }
    }
}
