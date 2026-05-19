//! Root-level medium components (100-300 lines each in TS source).
//! Covers: Onboarding, CoordinatorAgentStatus, QuickOpenDialog, Markdown,
//! WorktreeExitDialog, EffortCallout, ShowInIDEPrompt, MCPServerDesktopImportDialog,
//! SessionPreview, IdeOnboardingDialog, AutoUpdater, TokenWarning, NativeAutoUpdater,
//! HighlightedCode, StructuredDiff, FileEditToolDiff, ThinkingToggle, ExportDialog,
//! FileEditToolUseRejectedMessage, TeleportResumeWrapper, TeleportStash,
//! IdeAutoConnectDialog, InvalidConfigDialog, ValidationErrorsList,
//! SkillImprovementSurvey, MCPServerMultiselectDialog, MossenMdExternalIncludesDialog,
//! VimTextInput, TeleportProgress, TagTabs, ApproveApiKey, HistorySearchDialog,
//! WorkflowMultiselectDialog, BaseTextInput, TeleportError, DevChannelsDialog,
//! TextInput, OutputStylePicker, FileEditToolUpdatedMessage, IdleReturnDialog,
//! AgentProgressLine, MCPServerApprovalDialog, MossenErrorBoundary, CompactSummary,
//! TeleportRepoMismatchDialog, ChannelDowngradeDialog

use std::collections::HashMap;
use std::time::{Duration, Instant};

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Widget, Wrap};

use crate::theme::Theme;

// ─── Onboarding ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OnboardingStep {
    Welcome,
    SelectTheme,
    SelectModel,
    SetApiKey,
    Complete,
}

pub struct OnboardingState {
    pub step: OnboardingStep,
    pub selected_theme: Option<String>,
    pub selected_model: Option<String>,
    pub api_key: String,
    pub cursor_offset: usize,
    pub error: Option<String>,
    pub show_trust_dialog: bool,
}

impl OnboardingState {
    pub fn new() -> Self {
        Self {
            step: OnboardingStep::Welcome,
            selected_theme: None,
            selected_model: None,
            api_key: String::new(),
            cursor_offset: 0,
            error: None,
            show_trust_dialog: false,
        }
    }

    pub fn advance(&mut self) {
        self.step = match self.step {
            OnboardingStep::Welcome => OnboardingStep::SelectTheme,
            OnboardingStep::SelectTheme => OnboardingStep::SelectModel,
            OnboardingStep::SelectModel => OnboardingStep::SetApiKey,
            OnboardingStep::SetApiKey => OnboardingStep::Complete,
            OnboardingStep::Complete => OnboardingStep::Complete,
        };
    }

    pub fn go_back(&mut self) {
        self.step = match self.step {
            OnboardingStep::Welcome => OnboardingStep::Welcome,
            OnboardingStep::SelectTheme => OnboardingStep::Welcome,
            OnboardingStep::SelectModel => OnboardingStep::SelectTheme,
            OnboardingStep::SetApiKey => OnboardingStep::SelectModel,
            OnboardingStep::Complete => OnboardingStep::SetApiKey,
        };
    }

    pub fn set_theme(&mut self, theme: String) {
        self.selected_theme = Some(theme);
    }

    pub fn set_model(&mut self, model: String) {
        self.selected_model = Some(model);
    }

    pub fn set_api_key(&mut self, key: String) {
        self.cursor_offset = key.len();
        self.api_key = key;
    }

    pub fn is_complete(&self) -> bool {
        matches!(self.step, OnboardingStep::Complete)
    }
}

pub struct OnboardingWidget<'a> {
    pub state: &'a OnboardingState,
    pub theme: &'a Theme,
}

impl<'a> Widget for OnboardingWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default().title(" Welcome ").borders(Borders::ALL);
        let inner = block.inner(area);
        block.render(area, buf);

        match self.state.step {
            OnboardingStep::Welcome => {
                let lines = vec![
                    "Welcome to Mossen!",
                    "",
                    "Let's set up your environment.",
                    "",
                    "Press Enter to continue...",
                ];
                for (i, line) in lines.iter().enumerate() {
                    let y = inner.y + i as u16;
                    if y < inner.y + inner.height {
                        buf.set_string(inner.x, y, line, Style::default().fg(Color::White));
                    }
                }
            }
            OnboardingStep::SelectTheme => {
                buf.set_string(inner.x, inner.y, "Select a theme:", Style::default().fg(Color::White));
            }
            OnboardingStep::SelectModel => {
                buf.set_string(inner.x, inner.y, "Select a model:", Style::default().fg(Color::White));
            }
            OnboardingStep::SetApiKey => {
                buf.set_string(inner.x, inner.y, "Enter your API key:", Style::default().fg(Color::White));
                let masked = if self.state.api_key.is_empty() {
                    "".to_string()
                } else {
                    format!("{}...{}", &self.state.api_key[..4.min(self.state.api_key.len())], "****")
                };
                buf.set_string(inner.x, inner.y + 2, &masked, Style::default().fg(Color::Cyan));
            }
            OnboardingStep::Complete => {
                buf.set_string(inner.x, inner.y, "✓ Setup complete!", Style::default().fg(Color::Green));
                buf.set_string(inner.x, inner.y + 2, "Press Enter to start...", Style::default().fg(Color::DarkGray));
            }
        }
    }
}

// ─── CoordinatorAgentStatus ────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentPhase {
    Idle,
    Thinking,
    Acting,
    WaitingForUser,
    Complete,
    Error,
}

#[derive(Debug, Clone)]
pub struct CoordinatorAgent {
    pub id: String,
    pub name: String,
    pub phase: AgentPhase,
    pub current_action: Option<String>,
    pub progress_message: Option<String>,
    pub color: Color,
}

pub struct CoordinatorAgentStatusState {
    pub agents: Vec<CoordinatorAgent>,
    pub show_details: bool,
}

impl CoordinatorAgentStatusState {
    pub fn new() -> Self {
        Self {
            agents: Vec::new(),
            show_details: false,
        }
    }

    pub fn set_agents(&mut self, agents: Vec<CoordinatorAgent>) {
        self.agents = agents;
    }

    pub fn toggle_details(&mut self) {
        self.show_details = !self.show_details;
    }
}

pub struct CoordinatorAgentStatusWidget<'a> {
    pub state: &'a CoordinatorAgentStatusState,
    pub theme: &'a Theme,
}

impl<'a> Widget for CoordinatorAgentStatusWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        for (i, agent) in self.state.agents.iter().enumerate() {
            let y = area.y + i as u16;
            if y >= area.y + area.height {
                break;
            }
            let phase_icon = match agent.phase {
                AgentPhase::Idle => "○",
                AgentPhase::Thinking => "◐",
                AgentPhase::Acting => "●",
                AgentPhase::WaitingForUser => "◑",
                AgentPhase::Complete => "✓",
                AgentPhase::Error => "✗",
            };
            buf.set_string(area.x, y, phase_icon, Style::default().fg(agent.color));
            buf.set_string(area.x + 2, y, &agent.name, Style::default().fg(Color::White));
            if self.state.show_details {
                if let Some(ref action) = agent.current_action {
                    let action_x = area.x + 2 + agent.name.len() as u16 + 2;
                    let max_w = (area.width as usize).saturating_sub(action_x as usize - area.x as usize);
                    let display = if action.len() > max_w { &action[..max_w] } else { action.as_str() };
                    buf.set_string(action_x, y, display, Style::default().fg(Color::DarkGray));
                }
            }
        }
    }
}

// ─── QuickOpenDialog ───────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct QuickOpenItem {
    pub path: String,
    pub label: String,
    pub score: f64,
}

pub struct QuickOpenDialogState {
    pub query: String,
    pub cursor_offset: usize,
    pub items: Vec<QuickOpenItem>,
    pub focused_index: usize,
    pub is_loading: bool,
}

impl QuickOpenDialogState {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            cursor_offset: 0,
            items: Vec::new(),
            focused_index: 0,
            is_loading: false,
        }
    }

    pub fn set_query(&mut self, query: String) {
        self.cursor_offset = query.len();
        self.query = query;
        self.is_loading = true;
        self.focused_index = 0;
    }

    pub fn set_results(&mut self, items: Vec<QuickOpenItem>) {
        self.items = items;
        self.is_loading = false;
    }

    pub fn focus_next(&mut self) {
        if !self.items.is_empty() {
            self.focused_index = (self.focused_index + 1).min(self.items.len() - 1);
        }
    }

    pub fn focus_prev(&mut self) {
        self.focused_index = self.focused_index.saturating_sub(1);
    }

    pub fn selected_item(&self) -> Option<&QuickOpenItem> {
        self.items.get(self.focused_index)
    }
}

pub struct QuickOpenDialogWidget<'a> {
    pub state: &'a QuickOpenDialogState,
    pub theme: &'a Theme,
}

impl<'a> Widget for QuickOpenDialogWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default().title(" Quick Open ").borders(Borders::ALL);
        let inner = block.inner(area);
        block.render(area, buf);

        let input_text = if self.state.query.is_empty() { "Type to search files..." } else { &self.state.query };
        buf.set_string(inner.x, inner.y, input_text, Style::default().fg(Color::White));

        let list_start = inner.y + 2;
        for (i, item) in self.state.items.iter().enumerate() {
            let y = list_start + i as u16;
            if y >= inner.y + inner.height {
                break;
            }
            let is_focused = i == self.state.focused_index;
            let prefix = if is_focused { "▸ " } else { "  " };
            let style = if is_focused { Style::default().fg(Color::Cyan) } else { Style::default().fg(Color::White) };
            let max_w = (inner.width as usize).saturating_sub(prefix.len());
            let label = if item.label.len() > max_w { &item.label[..max_w] } else { &item.label };
            buf.set_string(inner.x, y, &format!("{}{}", prefix, label), style);
        }
    }
}

// ─── Markdown (streaming) ──────────────────────────────────────────────────

pub struct StreamingMarkdownState {
    pub content: String,
    pub is_streaming: bool,
    pub scroll_offset: usize,
    pub highlight_enabled: bool,
}

impl StreamingMarkdownState {
    pub fn new() -> Self {
        Self {
            content: String::new(),
            is_streaming: false,
            scroll_offset: 0,
            highlight_enabled: true,
        }
    }

    pub fn append_content(&mut self, text: &str) {
        self.content.push_str(text);
    }

    pub fn set_content(&mut self, text: String) {
        self.content = text;
    }

    pub fn set_streaming(&mut self, streaming: bool) {
        self.is_streaming = streaming;
    }

    pub fn scroll_up(&mut self, amount: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
    }

    pub fn scroll_down(&mut self, amount: usize) {
        self.scroll_offset += amount;
    }
}

pub struct StreamingMarkdownWidget<'a> {
    pub state: &'a StreamingMarkdownState,
    pub theme: &'a Theme,
}

impl<'a> Widget for StreamingMarkdownWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let lines: Vec<&str> = self.state.content.lines().collect();
        let start = self.state.scroll_offset.min(lines.len());
        let end = (start + area.height as usize).min(lines.len());
        for (i, line) in lines[start..end].iter().enumerate() {
            let y = area.y + i as u16;
            if y >= area.y + area.height { break; }
            let display = if line.len() > area.width as usize { &line[..area.width as usize] } else { line };
            buf.set_string(area.x, y, display, Style::default().fg(Color::White));
        }
        if self.state.is_streaming {
            let cursor_y = area.y + (end - start) as u16;
            if cursor_y < area.y + area.height {
                buf.set_string(area.x, cursor_y, "▍", Style::default().fg(Color::Cyan));
            }
        }
    }
}

// ─── WorktreeExitDialog ────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorktreeExitAction {
    KeepRunning,
    StopAll,
    StopAndClean,
}

impl WorktreeExitAction {
    pub fn label(&self) -> &'static str {
        match self {
            Self::KeepRunning => "Keep agents running in background",
            Self::StopAll => "Stop all agents",
            Self::StopAndClean => "Stop all and clean up worktree",
        }
    }
}

pub struct WorktreeExitDialogState {
    pub options: Vec<WorktreeExitAction>,
    pub focused_index: usize,
    pub worktree_path: String,
    pub agent_count: usize,
}

impl WorktreeExitDialogState {
    pub fn new(worktree_path: String, agent_count: usize) -> Self {
        Self {
            options: vec![
                WorktreeExitAction::KeepRunning,
                WorktreeExitAction::StopAll,
                WorktreeExitAction::StopAndClean,
            ],
            focused_index: 0,
            worktree_path,
            agent_count,
        }
    }

    pub fn focus_next(&mut self) {
        self.focused_index = (self.focused_index + 1) % self.options.len();
    }

    pub fn focus_prev(&mut self) {
        self.focused_index = if self.focused_index == 0 { self.options.len() - 1 } else { self.focused_index - 1 };
    }

    pub fn select_current(&self) -> &WorktreeExitAction {
        &self.options[self.focused_index]
    }
}

pub struct WorktreeExitDialogWidget<'a> {
    pub state: &'a WorktreeExitDialogState,
    pub theme: &'a Theme,
}

impl<'a> Widget for WorktreeExitDialogWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default().title(" Exit Worktree ").borders(Borders::ALL);
        let inner = block.inner(area);
        block.render(area, buf);

        let header = format!("{} agents running in {}", self.state.agent_count, self.state.worktree_path);
        buf.set_string(inner.x, inner.y, &header, Style::default().fg(Color::White));

        for (i, opt) in self.state.options.iter().enumerate() {
            let y = inner.y + 2 + i as u16;
            if y >= inner.y + inner.height { break; }
            let is_focused = i == self.state.focused_index;
            let prefix = if is_focused { "▸ " } else { "  " };
            let style = if is_focused { Style::default().fg(Color::Cyan) } else { Style::default().fg(Color::White) };
            buf.set_string(inner.x, y, &format!("{}{}", prefix, opt.label()), style);
        }
    }
}

// ─── EffortCallout ─────────────────────────────────────────────────────────

pub struct EffortCalloutState {
    pub visible: bool,
    pub model_name: String,
    pub effort_level: String,
    pub auto_dismissed: bool,
    pub show_time: Option<Instant>,
}

impl EffortCalloutState {
    pub fn new() -> Self {
        Self {
            visible: false,
            model_name: String::new(),
            effort_level: String::new(),
            auto_dismissed: false,
            show_time: None,
        }
    }

    pub fn show(&mut self, model: String, effort: String) {
        self.model_name = model;
        self.effort_level = effort;
        self.visible = true;
        self.auto_dismissed = false;
        self.show_time = Some(Instant::now());
    }

    pub fn dismiss(&mut self) {
        self.visible = false;
        self.auto_dismissed = true;
    }

    pub fn should_auto_dismiss(&self, timeout: Duration) -> bool {
        if let Some(show_time) = self.show_time {
            show_time.elapsed() > timeout
        } else {
            false
        }
    }
}

pub struct EffortCalloutWidget<'a> {
    pub state: &'a EffortCalloutState,
    pub theme: &'a Theme,
}

impl<'a> Widget for EffortCalloutWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.state.visible { return; }
        let text = format!("Using {} at {} effort", self.state.model_name, self.state.effort_level);
        buf.set_string(area.x, area.y, &text, Style::default().fg(Color::DarkGray));
    }
}

// ─── ShowInIDEPrompt ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShowInIDEAction {
    Open,
    Copy,
    Cancel,
}

pub struct ShowInIDEPromptState {
    pub file_path: String,
    pub line_number: Option<usize>,
    pub selected_action: ShowInIDEAction,
    pub ide_name: String,
}

impl ShowInIDEPromptState {
    pub fn new(file_path: String, line_number: Option<usize>, ide_name: String) -> Self {
        Self {
            file_path,
            line_number,
            selected_action: ShowInIDEAction::Open,
            ide_name,
        }
    }

    pub fn cycle_action(&mut self) {
        self.selected_action = match self.selected_action {
            ShowInIDEAction::Open => ShowInIDEAction::Copy,
            ShowInIDEAction::Copy => ShowInIDEAction::Cancel,
            ShowInIDEAction::Cancel => ShowInIDEAction::Open,
        };
    }
}

pub struct ShowInIDEPromptWidget<'a> {
    pub state: &'a ShowInIDEPromptState,
    pub theme: &'a Theme,
}

impl<'a> Widget for ShowInIDEPromptWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let location = if let Some(line) = self.state.line_number {
            format!("{}:{}", self.state.file_path, line)
        } else {
            self.state.file_path.clone()
        };
        buf.set_string(area.x, area.y, &format!("Open {} in {}?", location, self.state.ide_name), Style::default().fg(Color::White));
        let options = vec![
            ("o", "Open", ShowInIDEAction::Open),
            ("c", "Copy path", ShowInIDEAction::Copy),
            ("Esc", "Cancel", ShowInIDEAction::Cancel),
        ];
        let mut x = area.x;
        for (key, label, action) in &options {
            let is_selected = *action == self.state.selected_action;
            let style = if is_selected { Style::default().fg(Color::Cyan) } else { Style::default().fg(Color::DarkGray) };
            let text = format!("[{}] {} ", key, label);
            buf.set_string(x, area.y + 1, &text, style);
            x += text.len() as u16;
        }
    }
}

// ─── SessionPreview ────────────────────────────────────────────────────────

pub struct SessionPreviewState {
    pub session_id: String,
    pub title: String,
    pub messages_preview: Vec<String>,
    pub loading: bool,
    pub total_messages: usize,
}

impl SessionPreviewState {
    pub fn new(session_id: String) -> Self {
        Self { session_id, title: String::new(), messages_preview: Vec::new(), loading: true, total_messages: 0 }
    }

    pub fn set_preview(&mut self, title: String, messages: Vec<String>, total: usize) {
        self.title = title;
        self.messages_preview = messages;
        self.total_messages = total;
        self.loading = false;
    }
}

pub struct SessionPreviewWidget<'a> {
    pub state: &'a SessionPreviewState,
    pub theme: &'a Theme,
}

impl<'a> Widget for SessionPreviewWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if self.state.loading {
            buf.set_string(area.x, area.y, "Loading preview...", Style::default().fg(Color::Yellow));
            return;
        }
        buf.set_string(area.x, area.y, &self.state.title, Style::default().fg(Color::White).add_modifier(Modifier::BOLD));
        let count_text = format!("{} messages", self.state.total_messages);
        buf.set_string(area.x, area.y + 1, &count_text, Style::default().fg(Color::DarkGray));
        for (i, msg) in self.state.messages_preview.iter().enumerate() {
            let y = area.y + 3 + i as u16;
            if y >= area.y + area.height { break; }
            let display = if msg.len() > area.width as usize { &msg[..area.width as usize] } else { msg.as_str() };
            buf.set_string(area.x, y, display, Style::default().fg(Color::White));
        }
    }
}

// ─── AutoUpdater ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateStatus {
    CheckingForUpdate,
    UpdateAvailable { version: String },
    Downloading { progress: u8 },
    ReadyToInstall { version: String },
    UpToDate,
    Error(String),
}

pub struct AutoUpdaterState {
    pub status: UpdateStatus,
    pub auto_install: bool,
    pub dismissed: bool,
}

impl AutoUpdaterState {
    pub fn new() -> Self {
        Self { status: UpdateStatus::CheckingForUpdate, auto_install: false, dismissed: false }
    }

    pub fn set_status(&mut self, status: UpdateStatus) {
        self.status = status;
    }

    pub fn dismiss(&mut self) {
        self.dismissed = true;
    }
}

pub struct AutoUpdaterWidget<'a> {
    pub state: &'a AutoUpdaterState,
    pub theme: &'a Theme,
}

impl<'a> Widget for AutoUpdaterWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if self.state.dismissed { return; }
        let (text, color) = match &self.state.status {
            UpdateStatus::CheckingForUpdate => ("Checking for updates...".to_string(), Color::DarkGray),
            UpdateStatus::UpdateAvailable { version } => (format!("Update available: v{}", version), Color::Yellow),
            UpdateStatus::Downloading { progress } => (format!("Downloading... {}%", progress), Color::Cyan),
            UpdateStatus::ReadyToInstall { version } => (format!("v{} ready to install", version), Color::Green),
            UpdateStatus::UpToDate => ("Up to date".to_string(), Color::Green),
            UpdateStatus::Error(msg) => (format!("Update error: {}", msg), Color::Red),
        };
        buf.set_string(area.x, area.y, &text, Style::default().fg(color));
    }
}

// ─── TokenWarning ──────────────────────────────────────────────────────────

pub struct TokenWarningState {
    pub current_tokens: u64,
    pub max_tokens: u64,
    pub percentage: f64,
    pub show_details: bool,
    pub model_name: String,
}

impl TokenWarningState {
    pub fn new(current: u64, max: u64, model: String) -> Self {
        let percentage = if max > 0 { (current as f64 / max as f64) * 100.0 } else { 0.0 };
        Self { current_tokens: current, max_tokens: max, percentage, show_details: false, model_name: model }
    }

    pub fn update(&mut self, current: u64) {
        self.current_tokens = current;
        self.percentage = if self.max_tokens > 0 { (current as f64 / self.max_tokens as f64) * 100.0 } else { 0.0 };
    }

    pub fn should_warn(&self) -> bool {
        self.percentage >= 80.0
    }

    pub fn is_critical(&self) -> bool {
        self.percentage >= 95.0
    }
}

pub struct TokenWarningWidget<'a> {
    pub state: &'a TokenWarningState,
    pub theme: &'a Theme,
}

impl<'a> Widget for TokenWarningWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.state.should_warn() { return; }
        let color = if self.state.is_critical() { Color::Red } else { Color::Yellow };
        let text = format!("⚠ Context {:.0}% full ({}/{})", self.state.percentage, self.state.current_tokens, self.state.max_tokens);
        buf.set_string(area.x, area.y, &text, Style::default().fg(color));
        if self.state.is_critical() {
            buf.set_string(area.x, area.y + 1, "Consider using /compact to free context", Style::default().fg(Color::DarkGray));
        }
    }
}

// ─── HighlightedCode ───────────────────────────────────────────────────────

pub struct HighlightedCodeState {
    pub code: String,
    pub language: Option<String>,
    pub start_line: usize,
    pub highlight_lines: Vec<usize>,
    pub show_line_numbers: bool,
}

impl HighlightedCodeState {
    pub fn new(code: String, language: Option<String>) -> Self {
        Self { code, language, start_line: 1, highlight_lines: Vec::new(), show_line_numbers: true }
    }

    pub fn set_highlight_lines(&mut self, lines: Vec<usize>) {
        self.highlight_lines = lines;
    }
}

pub struct HighlightedCodeWidget<'a> {
    pub state: &'a HighlightedCodeState,
    pub theme: &'a Theme,
}

impl<'a> Widget for HighlightedCodeWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let lines: Vec<&str> = self.state.code.lines().collect();
        let line_num_width = if self.state.show_line_numbers {
            format!("{}", self.state.start_line + lines.len()).len() + 1
        } else {
            0
        };
        for (i, line) in lines.iter().enumerate() {
            let y = area.y + i as u16;
            if y >= area.y + area.height { break; }
            let line_num = self.state.start_line + i;
            let is_highlighted = self.state.highlight_lines.contains(&line_num);
            let bg = if is_highlighted { Color::DarkGray } else { Color::Reset };
            if self.state.show_line_numbers {
                let num_str = format!("{:>width$} ", line_num, width = line_num_width - 1);
                buf.set_string(area.x, y, &num_str, Style::default().fg(Color::DarkGray).bg(bg));
            }
            let code_x = area.x + line_num_width as u16;
            let max_w = (area.width as usize).saturating_sub(line_num_width);
            let display = if line.len() > max_w { &line[..max_w] } else { line };
            buf.set_string(code_x, y, display, Style::default().fg(Color::White).bg(bg));
        }
    }
}

// ─── StructuredDiff ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffLineType {
    Added,
    Removed,
    Context,
    Header,
}

#[derive(Debug, Clone)]
pub struct DiffLine {
    pub line_type: DiffLineType,
    pub content: String,
    pub old_line_num: Option<usize>,
    pub new_line_num: Option<usize>,
}

pub struct StructuredDiffState {
    pub lines: Vec<DiffLine>,
    pub file_path: String,
    pub scroll_offset: usize,
    pub syntax_highlight: bool,
}

impl StructuredDiffState {
    pub fn new(file_path: String, lines: Vec<DiffLine>) -> Self {
        Self { lines, file_path, scroll_offset: 0, syntax_highlight: true }
    }

    pub fn scroll_up(&mut self, amount: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
    }

    pub fn scroll_down(&mut self, amount: usize) {
        self.scroll_offset = (self.scroll_offset + amount).min(self.lines.len().saturating_sub(1));
    }
}

pub struct StructuredDiffWidget<'a> {
    pub state: &'a StructuredDiffState,
    pub theme: &'a Theme,
}

impl<'a> Widget for StructuredDiffWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // File path header
        buf.set_string(area.x, area.y, &self.state.file_path, Style::default().fg(Color::White).add_modifier(Modifier::BOLD));

        let start = self.state.scroll_offset;
        let end = (start + (area.height as usize).saturating_sub(1)).min(self.state.lines.len());
        for (i, line) in self.state.lines[start..end].iter().enumerate() {
            let y = area.y + 1 + i as u16;
            if y >= area.y + area.height { break; }
            let (prefix, style) = match line.line_type {
                DiffLineType::Added => ("+", Style::default().fg(Color::Green)),
                DiffLineType::Removed => ("-", Style::default().fg(Color::Red)),
                DiffLineType::Context => (" ", Style::default().fg(Color::White)),
                DiffLineType::Header => ("@", Style::default().fg(Color::Cyan)),
            };
            buf.set_string(area.x, y, prefix, style);
            let max_w = (area.width as usize).saturating_sub(2);
            let content = if line.content.len() > max_w { &line.content[..max_w] } else { &line.content };
            buf.set_string(area.x + 2, y, content, style);
        }
    }
}

// ─── FileEditToolDiff ──────────────────────────────────────────────────────

pub struct FileEditToolDiffState {
    pub file_path: String,
    pub old_content: String,
    pub new_content: String,
    pub diff_lines: Vec<DiffLine>,
    pub is_new_file: bool,
}

impl FileEditToolDiffState {
    pub fn new(file_path: String, old_content: String, new_content: String) -> Self {
        let is_new_file = old_content.is_empty();
        let diff_lines = compute_diff_lines(&old_content, &new_content);
        Self { file_path, old_content, new_content, diff_lines, is_new_file }
    }
}

fn compute_diff_lines(old: &str, new: &str) -> Vec<DiffLine> {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    let mut result = Vec::new();
    let max_len = old_lines.len().max(new_lines.len());
    let mut old_idx = 0;
    let mut new_idx = 0;
    while old_idx < old_lines.len() || new_idx < new_lines.len() {
        if old_idx < old_lines.len() && new_idx < new_lines.len() {
            if old_lines[old_idx] == new_lines[new_idx] {
                result.push(DiffLine {
                    line_type: DiffLineType::Context,
                    content: old_lines[old_idx].to_string(),
                    old_line_num: Some(old_idx + 1),
                    new_line_num: Some(new_idx + 1),
                });
                old_idx += 1;
                new_idx += 1;
            } else {
                result.push(DiffLine {
                    line_type: DiffLineType::Removed,
                    content: old_lines[old_idx].to_string(),
                    old_line_num: Some(old_idx + 1),
                    new_line_num: None,
                });
                old_idx += 1;
                result.push(DiffLine {
                    line_type: DiffLineType::Added,
                    content: new_lines[new_idx].to_string(),
                    old_line_num: None,
                    new_line_num: Some(new_idx + 1),
                });
                new_idx += 1;
            }
        } else if old_idx < old_lines.len() {
            result.push(DiffLine {
                line_type: DiffLineType::Removed,
                content: old_lines[old_idx].to_string(),
                old_line_num: Some(old_idx + 1),
                new_line_num: None,
            });
            old_idx += 1;
        } else {
            result.push(DiffLine {
                line_type: DiffLineType::Added,
                content: new_lines[new_idx].to_string(),
                old_line_num: None,
                new_line_num: Some(new_idx + 1),
            });
            new_idx += 1;
        }
    }
    result
}

pub struct FileEditToolDiffWidget<'a> {
    pub state: &'a FileEditToolDiffState,
    pub theme: &'a Theme,
}

impl<'a> Widget for FileEditToolDiffWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let header = if self.state.is_new_file {
            format!("New file: {}", self.state.file_path)
        } else {
            format!("Edit: {}", self.state.file_path)
        };
        buf.set_string(area.x, area.y, &header, Style::default().fg(Color::White).add_modifier(Modifier::BOLD));
        for (i, line) in self.state.diff_lines.iter().enumerate() {
            let y = area.y + 1 + i as u16;
            if y >= area.y + area.height { break; }
            let (prefix, style) = match line.line_type {
                DiffLineType::Added => ("+", Style::default().fg(Color::Green)),
                DiffLineType::Removed => ("-", Style::default().fg(Color::Red)),
                DiffLineType::Context => (" ", Style::default().fg(Color::DarkGray)),
                DiffLineType::Header => ("@", Style::default().fg(Color::Cyan)),
            };
            buf.set_string(area.x, y, prefix, style);
            let max_w = (area.width as usize).saturating_sub(2);
            let content = if line.content.len() > max_w { &line.content[..max_w] } else { &line.content };
            buf.set_string(area.x + 2, y, content, style);
        }
    }
}

// ─── ThinkingToggle ────────────────────────────────────────────────────────

pub struct ThinkingToggleState {
    pub expanded: bool,
    pub thinking_text: String,
    pub block_id: String,
    pub max_preview_lines: usize,
}

impl ThinkingToggleState {
    pub fn new(block_id: String, thinking_text: String) -> Self {
        Self { expanded: false, thinking_text, block_id, max_preview_lines: 3 }
    }

    pub fn toggle(&mut self) {
        self.expanded = !self.expanded;
    }
}

pub struct ThinkingToggleWidget<'a> {
    pub state: &'a ThinkingToggleState,
    pub theme: &'a Theme,
}

impl<'a> Widget for ThinkingToggleWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let icon = if self.state.expanded { "▼" } else { "▶" };
        let header = format!("{} Thinking", icon);
        buf.set_string(area.x, area.y, &header, Style::default().fg(Color::Blue));

        if self.state.expanded {
            let lines: Vec<&str> = self.state.thinking_text.lines().collect();
            for (i, line) in lines.iter().enumerate() {
                let y = area.y + 1 + i as u16;
                if y >= area.y + area.height { break; }
                let max_w = (area.width as usize).saturating_sub(2);
                let display = if line.len() > max_w { &line[..max_w] } else { line };
                buf.set_string(area.x + 2, y, display, Style::default().fg(Color::DarkGray));
            }
        } else {
            let preview_lines: Vec<&str> = self.state.thinking_text.lines().take(self.state.max_preview_lines).collect();
            for (i, line) in preview_lines.iter().enumerate() {
                let y = area.y + 1 + i as u16;
                if y >= area.y + area.height { break; }
                let max_w = (area.width as usize).saturating_sub(2);
                let display = if line.len() > max_w { &line[..max_w] } else { line };
                buf.set_string(area.x + 2, y, display, Style::default().fg(Color::DarkGray));
            }
        }
    }
}

// ─── ExportDialog ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportFormat {
    Markdown,
    Json,
    PlainText,
}

impl ExportFormat {
    pub fn label(&self) -> &'static str {
        match self { Self::Markdown => "Markdown", Self::Json => "JSON", Self::PlainText => "Plain Text" }
    }
    pub fn extension(&self) -> &'static str {
        match self { Self::Markdown => "md", Self::Json => "json", Self::PlainText => "txt" }
    }
}

pub struct ExportDialogState {
    pub format: ExportFormat,
    pub output_path: String,
    pub include_thinking: bool,
    pub include_tool_output: bool,
    pub exporting: bool,
    pub error: Option<String>,
    pub done: bool,
}

impl ExportDialogState {
    pub fn new() -> Self {
        Self {
            format: ExportFormat::Markdown,
            output_path: String::new(),
            include_thinking: false,
            include_tool_output: true,
            exporting: false,
            error: None,
            done: false,
        }
    }

    pub fn cycle_format(&mut self) {
        self.format = match self.format {
            ExportFormat::Markdown => ExportFormat::Json,
            ExportFormat::Json => ExportFormat::PlainText,
            ExportFormat::PlainText => ExportFormat::Markdown,
        };
    }

    pub fn toggle_thinking(&mut self) { self.include_thinking = !self.include_thinking; }
    pub fn toggle_tool_output(&mut self) { self.include_tool_output = !self.include_tool_output; }

    pub fn start_export(&mut self) { self.exporting = true; self.error = None; }
    pub fn complete_export(&mut self, path: String) { self.exporting = false; self.output_path = path; self.done = true; }
    pub fn set_error(&mut self, err: String) { self.exporting = false; self.error = Some(err); }
}

pub struct ExportDialogWidget<'a> {
    pub state: &'a ExportDialogState,
    pub theme: &'a Theme,
}

impl<'a> Widget for ExportDialogWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default().title(" Export ").borders(Borders::ALL);
        let inner = block.inner(area);
        block.render(area, buf);

        if self.state.done {
            buf.set_string(inner.x, inner.y, &format!("✓ Exported to: {}", self.state.output_path), Style::default().fg(Color::Green));
            return;
        }
        if self.state.exporting {
            buf.set_string(inner.x, inner.y, "Exporting...", Style::default().fg(Color::Yellow));
            return;
        }

        buf.set_string(inner.x, inner.y, &format!("Format: {} [f to cycle]", self.state.format.label()), Style::default().fg(Color::White));
        let thinking_check = if self.state.include_thinking { "[x]" } else { "[ ]" };
        let tool_check = if self.state.include_tool_output { "[x]" } else { "[ ]" };
        buf.set_string(inner.x, inner.y + 2, &format!("{} Include thinking [t]", thinking_check), Style::default().fg(Color::White));
        buf.set_string(inner.x, inner.y + 3, &format!("{} Include tool output [o]", tool_check), Style::default().fg(Color::White));
        if let Some(ref err) = self.state.error {
            buf.set_string(inner.x, inner.y + 5, err, Style::default().fg(Color::Red));
        }
    }
}

// ─── TagTabs ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct TagTab {
    pub label: String,
    pub count: usize,
}

pub struct TagTabsState {
    pub tabs: Vec<TagTab>,
    pub selected_index: usize,
}

impl TagTabsState {
    pub fn new(tabs: Vec<TagTab>) -> Self {
        Self { tabs, selected_index: 0 }
    }

    pub fn select_next(&mut self) {
        if !self.tabs.is_empty() {
            self.selected_index = (self.selected_index + 1) % self.tabs.len();
        }
    }

    pub fn select_prev(&mut self) {
        if !self.tabs.is_empty() {
            self.selected_index = if self.selected_index == 0 { self.tabs.len() - 1 } else { self.selected_index - 1 };
        }
    }

    pub fn selected_label(&self) -> Option<&str> {
        self.tabs.get(self.selected_index).map(|t| t.label.as_str())
    }
}

pub struct TagTabsWidget<'a> {
    pub state: &'a TagTabsState,
    pub theme: &'a Theme,
}

impl<'a> Widget for TagTabsWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut x = area.x;
        for (i, tab) in self.state.tabs.iter().enumerate() {
            let is_selected = i == self.state.selected_index;
            let label = if tab.count > 0 { format!(" {} ({}) ", tab.label, tab.count) } else { format!(" {} ", tab.label) };
            if x + label.len() as u16 > area.x + area.width { break; }
            let style = if is_selected {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            buf.set_string(x, area.y, &label, style);
            x += label.len() as u16;
        }
    }
}

// ─── TextInput / BaseTextInput / VimTextInput ──────────────────────────────

pub struct TextInputState {
    pub value: String,
    pub cursor_offset: usize,
    pub placeholder: String,
    pub is_focused: bool,
    pub mask: Option<char>,
}

impl TextInputState {
    pub fn new(placeholder: &str) -> Self {
        Self { value: String::new(), cursor_offset: 0, placeholder: placeholder.to_string(), is_focused: true, mask: None }
    }

    pub fn set_value(&mut self, value: String) {
        self.cursor_offset = value.len();
        self.value = value;
    }

    pub fn insert_char(&mut self, c: char) {
        self.value.insert(self.cursor_offset, c);
        self.cursor_offset += c.len_utf8();
    }

    pub fn delete_back(&mut self) {
        if self.cursor_offset > 0 {
            let prev = self.value[..self.cursor_offset].chars().last().map(|c| c.len_utf8()).unwrap_or(0);
            self.cursor_offset -= prev;
            self.value.remove(self.cursor_offset);
        }
    }

    pub fn delete_forward(&mut self) {
        if self.cursor_offset < self.value.len() {
            self.value.remove(self.cursor_offset);
        }
    }

    pub fn move_left(&mut self) {
        if self.cursor_offset > 0 {
            let prev = self.value[..self.cursor_offset].chars().last().map(|c| c.len_utf8()).unwrap_or(0);
            self.cursor_offset -= prev;
        }
    }

    pub fn move_right(&mut self) {
        if self.cursor_offset < self.value.len() {
            let next = self.value[self.cursor_offset..].chars().next().map(|c| c.len_utf8()).unwrap_or(0);
            self.cursor_offset += next;
        }
    }

    pub fn move_to_start(&mut self) { self.cursor_offset = 0; }
    pub fn move_to_end(&mut self) { self.cursor_offset = self.value.len(); }

    pub fn delete_word_back(&mut self) {
        let before = &self.value[..self.cursor_offset];
        let trimmed = before.trim_end();
        let word_start = trimmed.rfind(char::is_whitespace).map(|i| i + 1).unwrap_or(0);
        let removed = self.cursor_offset - word_start;
        self.value = format!("{}{}", &self.value[..word_start], &self.value[self.cursor_offset..]);
        self.cursor_offset = word_start;
    }

    pub fn clear(&mut self) {
        self.value.clear();
        self.cursor_offset = 0;
    }
}

pub struct TextInputWidget<'a> {
    pub state: &'a TextInputState,
    pub theme: &'a Theme,
}

impl<'a> Widget for TextInputWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let display = if self.state.value.is_empty() {
            &self.state.placeholder
        } else if let Some(mask) = self.state.mask {
            // Can't return reference to local, use value for non-masked
            &self.state.value
        } else {
            &self.state.value
        };
        let style = if self.state.value.is_empty() {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default().fg(Color::White)
        };
        let max_w = area.width as usize;
        let text = if display.len() > max_w { &display[..max_w] } else { display };
        buf.set_string(area.x, area.y, text, style);
        // Cursor
        if self.state.is_focused {
            let cursor_x = area.x + self.state.cursor_offset.min(max_w) as u16;
            if cursor_x < area.x + area.width {
                buf.set_string(cursor_x, area.y, "▍", Style::default().fg(Color::Cyan));
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VimMode {
    Normal,
    Insert,
    Visual,
    Command,
}

pub struct VimTextInputState {
    pub inner: TextInputState,
    pub mode: VimMode,
    pub command_buffer: String,
    pub register: Option<String>,
}

impl VimTextInputState {
    pub fn new(placeholder: &str) -> Self {
        Self {
            inner: TextInputState::new(placeholder),
            mode: VimMode::Insert,
            command_buffer: String::new(),
            register: None,
        }
    }

    pub fn enter_normal(&mut self) { self.mode = VimMode::Normal; }
    pub fn enter_insert(&mut self) { self.mode = VimMode::Insert; }
    pub fn enter_visual(&mut self) { self.mode = VimMode::Visual; }

    pub fn handle_normal_key(&mut self, key: char) {
        match key {
            'i' => self.enter_insert(),
            'a' => { self.inner.move_right(); self.enter_insert(); }
            'A' => { self.inner.move_to_end(); self.enter_insert(); }
            'I' => { self.inner.move_to_start(); self.enter_insert(); }
            'h' => self.inner.move_left(),
            'l' => self.inner.move_right(),
            '0' => self.inner.move_to_start(),
            '$' => self.inner.move_to_end(),
            'x' => self.inner.delete_forward(),
            'X' => self.inner.delete_back(),
            'v' => self.enter_visual(),
            _ => {}
        }
    }
}

// ─── Remaining small dialogs ───────────────────────────────────────────────

pub struct ApproveApiKeyState {
    pub key_preview: String,
    pub source: String,
    pub approved: Option<bool>,
}

impl ApproveApiKeyState {
    pub fn new(key_preview: String, source: String) -> Self {
        Self { key_preview, source, approved: None }
    }
    pub fn approve(&mut self) { self.approved = Some(true); }
    pub fn deny(&mut self) { self.approved = Some(false); }
}

pub struct ApproveApiKeyWidget<'a> {
    pub state: &'a ApproveApiKeyState,
    pub theme: &'a Theme,
}

impl<'a> Widget for ApproveApiKeyWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default().title(" Approve API Key ").borders(Borders::ALL);
        let inner = block.inner(area);
        block.render(area, buf);
        buf.set_string(inner.x, inner.y, &format!("Source: {}", self.state.source), Style::default().fg(Color::White));
        buf.set_string(inner.x, inner.y + 1, &format!("Key: {}", self.state.key_preview), Style::default().fg(Color::Cyan));
        buf.set_string(inner.x, inner.y + 3, "Approve? (y/n)", Style::default().fg(Color::Yellow));
    }
}

pub struct HistorySearchDialogState {
    pub query: String,
    pub results: Vec<String>,
    pub focused_index: usize,
    pub is_searching: bool,
}

impl HistorySearchDialogState {
    pub fn new() -> Self {
        Self { query: String::new(), results: Vec::new(), focused_index: 0, is_searching: false }
    }

    pub fn set_query(&mut self, q: String) { self.query = q; self.is_searching = true; }
    pub fn set_results(&mut self, r: Vec<String>) { self.results = r; self.is_searching = false; self.focused_index = 0; }
    pub fn focus_next(&mut self) { if !self.results.is_empty() { self.focused_index = (self.focused_index + 1).min(self.results.len() - 1); } }
    pub fn focus_prev(&mut self) { self.focused_index = self.focused_index.saturating_sub(1); }
}

pub struct HistorySearchDialogWidget<'a> {
    pub state: &'a HistorySearchDialogState,
    pub theme: &'a Theme,
}

impl<'a> Widget for HistorySearchDialogWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default().title(" History ").borders(Borders::ALL);
        let inner = block.inner(area);
        block.render(area, buf);
        let input = if self.state.query.is_empty() { "Search history..." } else { &self.state.query };
        buf.set_string(inner.x, inner.y, input, Style::default().fg(Color::White));
        for (i, result) in self.state.results.iter().enumerate() {
            let y = inner.y + 2 + i as u16;
            if y >= inner.y + inner.height { break; }
            let is_focused = i == self.state.focused_index;
            let prefix = if is_focused { "▸ " } else { "  " };
            let style = if is_focused { Style::default().fg(Color::Cyan) } else { Style::default().fg(Color::White) };
            let max_w = (inner.width as usize).saturating_sub(prefix.len());
            let display = if result.len() > max_w { &result[..max_w] } else { result.as_str() };
            buf.set_string(inner.x, y, &format!("{}{}", prefix, display), style);
        }
    }
}

// ─── Additional dialogs (MCP-related, Teleport, Validation, etc.) ──────────

pub struct MCPServerApprovalDialogState {
    pub server_name: String,
    pub server_uri: String,
    pub tools_count: usize,
    pub approved: Option<bool>,
}

impl MCPServerApprovalDialogState {
    pub fn new(name: String, uri: String, tools: usize) -> Self {
        Self { server_name: name, server_uri: uri, tools_count: tools, approved: None }
    }
    pub fn approve(&mut self) { self.approved = Some(true); }
    pub fn deny(&mut self) { self.approved = Some(false); }
}

pub struct MCPServerApprovalDialogWidget<'a> {
    pub state: &'a MCPServerApprovalDialogState,
    pub theme: &'a Theme,
}

impl<'a> Widget for MCPServerApprovalDialogWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default().title(" MCP Server Approval ").borders(Borders::ALL);
        let inner = block.inner(area);
        block.render(area, buf);
        buf.set_string(inner.x, inner.y, &format!("Server: {}", self.state.server_name), Style::default().fg(Color::White));
        buf.set_string(inner.x, inner.y + 1, &format!("URI: {}", self.state.server_uri), Style::default().fg(Color::DarkGray));
        buf.set_string(inner.x, inner.y + 2, &format!("Tools: {}", self.state.tools_count), Style::default().fg(Color::DarkGray));
        buf.set_string(inner.x, inner.y + 4, "Allow this server? (y/n)", Style::default().fg(Color::Yellow));
    }
}

pub struct ValidationErrorsListState {
    pub errors: Vec<String>,
    pub title: String,
}

impl ValidationErrorsListState {
    pub fn new(title: String, errors: Vec<String>) -> Self {
        Self { errors, title }
    }
}

pub struct ValidationErrorsListWidget<'a> {
    pub state: &'a ValidationErrorsListState,
    pub theme: &'a Theme,
}

impl<'a> Widget for ValidationErrorsListWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        buf.set_string(area.x, area.y, &self.state.title, Style::default().fg(Color::Red).add_modifier(Modifier::BOLD));
        for (i, err) in self.state.errors.iter().enumerate() {
            let y = area.y + 1 + i as u16;
            if y >= area.y + area.height { break; }
            let max_w = (area.width as usize).saturating_sub(4);
            let display = if err.len() > max_w { &err[..max_w] } else { err.as_str() };
            buf.set_string(area.x + 2, y, &format!("• {}", display), Style::default().fg(Color::Red));
        }
    }
}

pub struct TeleportProgressState {
    pub phase: String,
    pub progress_percent: u8,
    pub message: String,
    pub error: Option<String>,
}

impl TeleportProgressState {
    pub fn new() -> Self {
        Self { phase: "Connecting".to_string(), progress_percent: 0, message: String::new(), error: None }
    }
    pub fn set_phase(&mut self, phase: String, progress: u8) { self.phase = phase; self.progress_percent = progress; }
    pub fn set_message(&mut self, msg: String) { self.message = msg; }
    pub fn set_error(&mut self, err: String) { self.error = Some(err); }
}

pub struct TeleportProgressWidget<'a> {
    pub state: &'a TeleportProgressState,
    pub theme: &'a Theme,
}

impl<'a> Widget for TeleportProgressWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if let Some(ref err) = self.state.error {
            buf.set_string(area.x, area.y, &format!("Error: {}", err), Style::default().fg(Color::Red));
            return;
        }
        buf.set_string(area.x, area.y, &self.state.phase, Style::default().fg(Color::White));
        let bar_width = (area.width as usize).saturating_sub(2);
        let filled = (self.state.progress_percent as usize * bar_width) / 100;
        let bar = format!("[{}{}] {}%", "█".repeat(filled), "░".repeat(bar_width.saturating_sub(filled)), self.state.progress_percent);
        buf.set_string(area.x, area.y + 1, &bar, Style::default().fg(Color::Cyan));
        if !self.state.message.is_empty() {
            buf.set_string(area.x, area.y + 2, &self.state.message, Style::default().fg(Color::DarkGray));
        }
    }
}

pub struct OutputStylePickerState {
    pub styles: Vec<String>,
    pub focused_index: usize,
    pub current_style: String,
}

impl OutputStylePickerState {
    pub fn new(styles: Vec<String>, current: String) -> Self {
        let focused = styles.iter().position(|s| *s == current).unwrap_or(0);
        Self { styles, focused_index: focused, current_style: current }
    }
    pub fn focus_next(&mut self) { self.focused_index = (self.focused_index + 1) % self.styles.len(); }
    pub fn focus_prev(&mut self) { self.focused_index = if self.focused_index == 0 { self.styles.len() - 1 } else { self.focused_index - 1 }; }
    pub fn select_current(&self) -> &str { &self.styles[self.focused_index] }
}

pub struct OutputStylePickerWidget<'a> {
    pub state: &'a OutputStylePickerState,
    pub theme: &'a Theme,
}

impl<'a> Widget for OutputStylePickerWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default().title(" Output Style ").borders(Borders::ALL);
        let inner = block.inner(area);
        block.render(area, buf);
        for (i, style_name) in self.state.styles.iter().enumerate() {
            let y = inner.y + i as u16;
            if y >= inner.y + inner.height { break; }
            let is_focused = i == self.state.focused_index;
            let is_current = *style_name == self.state.current_style;
            let prefix = if is_focused { "▸ " } else { "  " };
            let suffix = if is_current { " (current)" } else { "" };
            let style = if is_focused { Style::default().fg(Color::Cyan) } else { Style::default().fg(Color::White) };
            buf.set_string(inner.x, y, &format!("{}{}{}", prefix, style_name, suffix), style);
        }
    }
}

pub struct CompactSummaryState {
    pub summary_text: String,
    pub messages_compacted: usize,
    pub tokens_saved: u64,
}

impl CompactSummaryState {
    pub fn new(summary: String, messages: usize, tokens: u64) -> Self {
        Self { summary_text: summary, messages_compacted: messages, tokens_saved: tokens }
    }
}

pub struct CompactSummaryWidget<'a> {
    pub state: &'a CompactSummaryState,
    pub theme: &'a Theme,
}

impl<'a> Widget for CompactSummaryWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let header = format!("Compacted {} messages (saved {} tokens)", self.state.messages_compacted, self.state.tokens_saved);
        buf.set_string(area.x, area.y, &header, Style::default().fg(Color::Green));
        let lines: Vec<&str> = self.state.summary_text.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            let y = area.y + 2 + i as u16;
            if y >= area.y + area.height { break; }
            let max_w = area.width as usize;
            let display = if line.len() > max_w { &line[..max_w] } else { line };
            buf.set_string(area.x, y, display, Style::default().fg(Color::DarkGray));
        }
    }
}

#[derive(Debug)]
pub struct IdleReturnDialogState {
    pub idle_duration: Duration,
    pub message: String,
    pub dismissed: bool,
}

impl IdleReturnDialogState {
    pub fn new(idle_duration: Duration) -> Self {
        let mins = idle_duration.as_secs() / 60;
        let message = if mins > 60 {
            format!("Welcome back! You were away for {}h {}m.", mins / 60, mins % 60)
        } else {
            format!("Welcome back! You were away for {}m.", mins)
        };
        Self { idle_duration, message, dismissed: false }
    }
    pub fn dismiss(&mut self) { self.dismissed = true; }
}

pub struct IdleReturnDialogWidget<'a> {
    pub state: &'a IdleReturnDialogState,
    pub theme: &'a Theme,
}

impl<'a> Widget for IdleReturnDialogWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if self.state.dismissed { return; }
        buf.set_string(area.x, area.y, &self.state.message, Style::default().fg(Color::White));
        buf.set_string(area.x, area.y + 1, "Press any key to continue...", Style::default().fg(Color::DarkGray));
    }
}

pub struct AgentProgressLineState {
    pub agent_name: String,
    pub action: String,
    pub elapsed: Duration,
    pub color: Color,
}

impl AgentProgressLineState {
    pub fn new(name: String, action: String, color: Color) -> Self {
        Self { agent_name: name, action, elapsed: Duration::ZERO, color }
    }
    pub fn update_elapsed(&mut self, elapsed: Duration) { self.elapsed = elapsed; }
}

pub struct AgentProgressLineWidget<'a> {
    pub state: &'a AgentProgressLineState,
    pub theme: &'a Theme,
}

impl<'a> Widget for AgentProgressLineWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let elapsed_str = format!("{:.1}s", self.state.elapsed.as_secs_f64());
        let text = format!("◐ {} {} ({})", self.state.agent_name, self.state.action, elapsed_str);
        let max_w = area.width as usize;
        let display = if text.len() > max_w { &text[..max_w] } else { &text };
        buf.set_string(area.x, area.y, display, Style::default().fg(self.state.color));
    }
}
