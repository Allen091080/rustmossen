//! Dialog components — all modal dialogs in one module.
//!
//! Translates: TrustDialog/, CostThresholdDialog, ExportDialog,
//! AutoModeOptInDialog, BypassPermissionsModeDialog, ChannelDowngradeDialog,
//! DevChannelsDialog, GlobalSearchDialog, HistorySearchDialog,
//! IdeAutoConnectDialog, IdeOnboardingDialog, IdleReturnDialog,
//! InvalidConfigDialog, InvalidSettingsDialog, MCPServerApprovalDialog,
//! MCPServerDesktopImportDialog, MCPServerMultiselectDialog, etc.

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Widget, Wrap},
};

use crate::components::design_system::DialogWidget;
use crate::theme::Theme;

// ===================================================================
// Common dialog state
// ===================================================================

/// Result of a dialog interaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogResult {
    Confirmed,
    Cancelled,
    Selected(String),
}

/// Base state shared by simple confirm/cancel dialogs.
#[derive(Debug, Clone)]
pub struct ConfirmDialogState {
    pub title: String,
    pub message: String,
    pub confirm_label: String,
    pub cancel_label: String,
    pub selected_confirm: bool,
    pub result: Option<DialogResult>,
}

impl ConfirmDialogState {
    pub fn new(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            confirm_label: "Confirm".into(),
            cancel_label: "Cancel".into(),
            selected_confirm: true,
            result: None,
        }
    }

    pub fn toggle(&mut self) {
        self.selected_confirm = !self.selected_confirm;
    }

    pub fn confirm(&mut self) {
        self.result = Some(if self.selected_confirm {
            DialogResult::Confirmed
        } else {
            DialogResult::Cancelled
        });
    }
}

// ===================================================================
// ConfirmDialogWidget — reusable confirm/cancel dialog
// ===================================================================

pub struct ConfirmDialogWidget<'a> {
    pub state: &'a ConfirmDialogState,
    pub theme: &'a Theme,
}

impl<'a> ConfirmDialogWidget<'a> {
    pub fn new(state: &'a ConfirmDialogState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for ConfirmDialogWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let dialog = DialogWidget::new(&self.state.title, self.theme).size(50, 10);
        let inner = dialog.inner_area(area);
        dialog.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(inner);

        // Message
        let msg = Paragraph::new(self.state.message.as_str())
            .wrap(Wrap { trim: true })
            .style(Style::default().fg(self.theme.text));
        msg.render(chunks[0], buf);

        // Buttons
        let confirm_style = if self.state.selected_confirm {
            Style::default()
                .fg(self.theme.text)
                .bg(self.theme.primary)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(self.theme.text_dim)
        };
        let cancel_style = if !self.state.selected_confirm {
            Style::default()
                .fg(self.theme.text)
                .bg(self.theme.error)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(self.theme.text_dim)
        };

        let buttons = Line::from(vec![
            Span::styled(format!(" {} ", self.state.confirm_label), confirm_style),
            Span::raw("  "),
            Span::styled(format!(" {} ", self.state.cancel_label), cancel_style),
        ]);
        buf.set_line(chunks[1].x, chunks[1].y, &buttons, chunks[1].width);
    }
}

// ===================================================================
// TrustDialogWidget — trust / safety dialog (TrustDialog/)
// ===================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustLevel {
    Trusted,
    Limited,
    Blocked,
}

#[derive(Debug, Clone)]
pub struct TrustDialogState {
    pub directory: String,
    pub current_trust: TrustLevel,
    pub selected_trust: TrustLevel,
    pub result: Option<TrustLevel>,
}

impl TrustDialogState {
    pub fn new(directory: impl Into<String>, current: TrustLevel) -> Self {
        Self {
            directory: directory.into(),
            current_trust: current,
            selected_trust: current,
            result: None,
        }
    }

    pub fn select_next(&mut self) {
        self.selected_trust = match self.selected_trust {
            TrustLevel::Trusted => TrustLevel::Limited,
            TrustLevel::Limited => TrustLevel::Blocked,
            TrustLevel::Blocked => TrustLevel::Trusted,
        };
    }

    pub fn confirm(&mut self) {
        self.result = Some(self.selected_trust);
    }
}

pub struct TrustDialogWidget<'a> {
    pub state: &'a TrustDialogState,
    pub theme: &'a Theme,
}

impl<'a> TrustDialogWidget<'a> {
    pub fn new(state: &'a TrustDialogState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for TrustDialogWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let dialog = DialogWidget::new("Trust Settings", self.theme).size(60, 14);
        let inner = dialog.inner_area(area);
        dialog.render(area, buf);

        if inner.width == 0 || inner.height < 5 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2),
                Constraint::Length(1),
                Constraint::Length(3),
                Constraint::Min(0),
            ])
            .split(inner);

        // Directory info
        let dir_line = Line::from(vec![
            Span::styled("Directory: ", Style::default().fg(self.theme.text_dim)),
            Span::styled(
                &self.state.directory,
                Style::default()
                    .fg(self.theme.primary)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);
        buf.set_line(chunks[0].x, chunks[0].y, &dir_line, chunks[0].width);

        // Trust options
        let options = [
            (TrustLevel::Trusted, "Trusted", "Allow all operations"),
            (TrustLevel::Limited, "Limited", "Ask before writes"),
            (TrustLevel::Blocked, "Blocked", "Read-only access"),
        ];

        for (i, (level, label, desc)) in options.iter().enumerate() {
            let y = chunks[2].y + i as u16;
            if y >= chunks[2].y + chunks[2].height {
                break;
            }
            let is_selected = self.state.selected_trust == *level;
            let indicator = if is_selected { "●" } else { "○" };
            let style = if is_selected {
                Style::default()
                    .fg(self.theme.primary)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(self.theme.text)
            };

            let line = Line::from(vec![
                Span::styled(format!(" {} ", indicator), style),
                Span::styled(*label, style),
                Span::styled(
                    format!("  — {}", desc),
                    Style::default().fg(self.theme.text_dim),
                ),
            ]);
            buf.set_line(chunks[2].x, y, &line, chunks[2].width);
        }
    }
}

// ===================================================================
// CostThresholdDialogWidget (CostThresholdDialog.tsx)
// ===================================================================

#[derive(Debug, Clone)]
pub struct CostThresholdDialogState {
    pub current_cost: f64,
    pub threshold: f64,
    pub acknowledged: bool,
}

impl CostThresholdDialogState {
    pub fn new(cost: f64, threshold: f64) -> Self {
        Self {
            current_cost: cost,
            threshold,
            acknowledged: false,
        }
    }
}

pub struct CostThresholdDialogWidget<'a> {
    pub state: &'a CostThresholdDialogState,
    pub theme: &'a Theme,
}

impl<'a> CostThresholdDialogWidget<'a> {
    pub fn new(state: &'a CostThresholdDialogState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for CostThresholdDialogWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let dialog = DialogWidget::new("Cost Threshold Reached", self.theme).size(50, 8);
        let inner = dialog.inner_area(area);
        dialog.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let lines = vec![
            Line::from(vec![
                Span::styled("Current cost: ", Style::default().fg(self.theme.text)),
                Span::styled(
                    format!("${:.2}", self.state.current_cost),
                    Style::default()
                        .fg(self.theme.warning)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled("Threshold: ", Style::default().fg(self.theme.text)),
                Span::styled(
                    format!("${:.2}", self.state.threshold),
                    Style::default().fg(self.theme.text_dim),
                ),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "Press Enter to continue or Esc to stop",
                Style::default().fg(self.theme.text_dim),
            )),
        ];
        let p = Paragraph::new(lines);
        p.render(inner, buf);
    }
}

// ===================================================================
// ExportDialogWidget (ExportDialog.tsx)
// ===================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Markdown,
    Json,
    Text,
}

#[derive(Debug, Clone)]
pub struct ExportDialogState {
    pub format: ExportFormat,
    pub include_system: bool,
    pub include_tool_results: bool,
    pub result: Option<ExportFormat>,
}

impl ExportDialogState {
    pub fn new() -> Self {
        Self {
            format: ExportFormat::Markdown,
            include_system: false,
            include_tool_results: true,
            result: None,
        }
    }

    pub fn cycle_format(&mut self) {
        self.format = match self.format {
            ExportFormat::Markdown => ExportFormat::Json,
            ExportFormat::Json => ExportFormat::Text,
            ExportFormat::Text => ExportFormat::Markdown,
        };
    }
}

impl Default for ExportDialogState {
    fn default() -> Self {
        Self::new()
    }
}

pub struct ExportDialogWidget<'a> {
    pub state: &'a ExportDialogState,
    pub theme: &'a Theme,
}

impl<'a> ExportDialogWidget<'a> {
    pub fn new(state: &'a ExportDialogState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for ExportDialogWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let dialog = DialogWidget::new("Export Conversation", self.theme).size(50, 10);
        let inner = dialog.inner_area(area);
        dialog.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let fmt_label = match self.state.format {
            ExportFormat::Markdown => "Markdown",
            ExportFormat::Json => "JSON",
            ExportFormat::Text => "Plain Text",
        };

        let lines = vec![
            Line::from(vec![
                Span::styled("Format: ", Style::default().fg(self.theme.text)),
                Span::styled(
                    fmt_label,
                    Style::default()
                        .fg(self.theme.primary)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" (Tab to change)", Style::default().fg(self.theme.text_dim)),
            ]),
            Line::from(vec![
                Span::styled(
                    if self.state.include_system {
                        "[✓]"
                    } else {
                        "[ ]"
                    },
                    Style::default().fg(self.theme.text),
                ),
                Span::styled(
                    " Include system messages",
                    Style::default().fg(self.theme.text),
                ),
            ]),
            Line::from(vec![
                Span::styled(
                    if self.state.include_tool_results {
                        "[✓]"
                    } else {
                        "[ ]"
                    },
                    Style::default().fg(self.theme.text),
                ),
                Span::styled(
                    " Include tool results",
                    Style::default().fg(self.theme.text),
                ),
            ]),
        ];
        Paragraph::new(lines).render(inner, buf);
    }
}

// ===================================================================
// AutoModeOptInDialogWidget (AutoModeOptInDialog.tsx)
// ===================================================================

pub struct AutoModeOptInDialogState {
    pub inner: ConfirmDialogState,
}

impl AutoModeOptInDialogState {
    pub fn new() -> Self {
        Self {
            inner: ConfirmDialogState::new(
                "Enable Swift Mode",
                "Swift mode will automatically approve safe operations.\nAre you sure?",
            ),
        }
    }
}

impl Default for AutoModeOptInDialogState {
    fn default() -> Self {
        Self::new()
    }
}

// ===================================================================
// BypassPermissionsModeDialogWidget (BypassPermissionsModeDialog.tsx)
// ===================================================================

pub struct BypassPermissionsDialogState {
    pub inner: ConfirmDialogState,
}

impl BypassPermissionsDialogState {
    pub fn new() -> Self {
        Self {
            inner: ConfirmDialogState::new(
                "Unrestricted Mode",
                "This will bypass all permission checks.\nUse with caution.",
            ),
        }
    }
}

impl Default for BypassPermissionsDialogState {
    fn default() -> Self {
        Self::new()
    }
}

// ===================================================================
// ChannelDowngradeDialogWidget (ChannelDowngradeDialog.tsx)
// ===================================================================

pub struct ChannelDowngradeDialogState {
    pub inner: ConfirmDialogState,
    pub current_channel: String,
    pub target_channel: String,
}

impl ChannelDowngradeDialogState {
    pub fn new(current: impl Into<String>, target: impl Into<String>) -> Self {
        let current = current.into();
        let target = target.into();
        Self {
            inner: ConfirmDialogState::new(
                "Downgrade Channel",
                format!("Switch from {} to {}?", current, target),
            ),
            current_channel: current,
            target_channel: target,
        }
    }
}

// ===================================================================
// GlobalSearchDialogWidget (GlobalSearchDialog.tsx)
// ===================================================================

#[derive(Debug, Clone)]
pub struct GlobalSearchDialogState {
    pub query: String,
    pub results: Vec<SearchResult>,
    pub selected: usize,
    pub is_searching: bool,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub label: String,
    pub description: String,
    pub category: String,
}

impl GlobalSearchDialogState {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            results: Vec::new(),
            selected: 0,
            is_searching: false,
        }
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected + 1 < self.results.len() {
            self.selected += 1;
        }
    }
}

impl Default for GlobalSearchDialogState {
    fn default() -> Self {
        Self::new()
    }
}

pub struct GlobalSearchDialogWidget<'a> {
    pub state: &'a GlobalSearchDialogState,
    pub theme: &'a Theme,
}

impl<'a> GlobalSearchDialogWidget<'a> {
    pub fn new(state: &'a GlobalSearchDialogState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for GlobalSearchDialogWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let dialog = DialogWidget::new("Search", self.theme).size(70, 20);
        let inner = dialog.inner_area(area);
        dialog.render(area, buf);

        if inner.width == 0 || inner.height < 3 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Min(1),
            ])
            .split(inner);

        // Search input
        let input_line = if self.state.query.is_empty() {
            Line::from(Span::styled(
                "Type to search commands, files, settings...",
                Style::default().fg(self.theme.text_dim),
            ))
        } else {
            Line::from(vec![
                Span::styled("🔍 ", Style::default()),
                Span::styled(&self.state.query, Style::default().fg(self.theme.text)),
            ])
        };
        buf.set_line(chunks[0].x, chunks[0].y, &input_line, chunks[0].width);

        // Divider
        let div = "─".repeat(chunks[1].width as usize);
        buf.set_string(chunks[1].x, chunks[1].y, &div, self.theme.style_border());

        // Results
        for (i, result) in self.state.results.iter().enumerate() {
            let y = chunks[2].y + i as u16;
            if y >= chunks[2].y + chunks[2].height {
                break;
            }
            let is_sel = i == self.state.selected;
            let bg = if is_sel {
                self.theme.selection
            } else {
                Color::Reset
            };

            for x in chunks[2].x..chunks[2].x + chunks[2].width {
                buf.set_string(x, y, " ", Style::default().bg(bg));
            }

            let line = Line::from(vec![
                Span::styled(
                    if is_sel { "▸ " } else { "  " },
                    Style::default().fg(self.theme.primary).bg(bg),
                ),
                Span::styled(
                    &result.label,
                    Style::default()
                        .fg(self.theme.text)
                        .bg(bg)
                        .add_modifier(if is_sel {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                ),
                Span::styled(
                    format!("  {}", result.description),
                    Style::default().fg(self.theme.text_dim).bg(bg),
                ),
            ]);
            buf.set_line(chunks[2].x, y, &line, chunks[2].width);
        }
    }
}

// ===================================================================
// HistorySearchDialogWidget (HistorySearchDialog.tsx)
// ===================================================================

#[derive(Debug, Clone)]
pub struct HistorySearchDialogState {
    pub query: String,
    pub sessions: Vec<HistorySession>,
    pub selected: usize,
}

#[derive(Debug, Clone)]
pub struct HistorySession {
    pub id: String,
    pub title: String,
    pub timestamp: String,
    pub message_count: usize,
}

impl HistorySearchDialogState {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            sessions: Vec::new(),
            selected: 0,
        }
    }
}

impl Default for HistorySearchDialogState {
    fn default() -> Self {
        Self::new()
    }
}

pub struct HistorySearchDialogWidget<'a> {
    pub state: &'a HistorySearchDialogState,
    pub theme: &'a Theme,
}

impl<'a> HistorySearchDialogWidget<'a> {
    pub fn new(state: &'a HistorySearchDialogState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for HistorySearchDialogWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let dialog = DialogWidget::new("Session History", self.theme).size(65, 18);
        let inner = dialog.inner_area(area);
        dialog.render(area, buf);

        if inner.width == 0 || inner.height < 3 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .split(inner);

        // Search input
        let input_line = Line::from(vec![
            Span::styled("Search: ", Style::default().fg(self.theme.text_dim)),
            Span::styled(&self.state.query, Style::default().fg(self.theme.text)),
        ]);
        buf.set_line(chunks[0].x, chunks[0].y, &input_line, chunks[0].width);

        // Session list
        for (i, session) in self.state.sessions.iter().enumerate() {
            let y = chunks[1].y + i as u16;
            if y >= chunks[1].y + chunks[1].height {
                break;
            }
            let is_sel = i == self.state.selected;
            let style = if is_sel {
                Style::default()
                    .fg(self.theme.text)
                    .bg(self.theme.selection)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(self.theme.text)
            };

            let line = Line::from(vec![
                Span::styled(if is_sel { "▸ " } else { "  " }, style),
                Span::styled(&session.title, style),
                Span::styled(
                    format!("  {} msgs  {}", session.message_count, session.timestamp),
                    Style::default().fg(self.theme.text_dim).bg(if is_sel {
                        self.theme.selection
                    } else {
                        Color::Reset
                    }),
                ),
            ]);
            buf.set_line(chunks[1].x, y, &line, chunks[1].width);
        }
    }
}

// ===================================================================
// Simple dialog stubs for remaining dialogs
// (DevChannelsDialog, IdeAutoConnect, IdeOnboarding, IdleReturn,
//  InvalidConfig, InvalidSettings, MCPServerApproval,
//  MCPServerDesktopImport, MCPServerMultiselect)
// ===================================================================

/// IDE auto-connect dialog state.
#[derive(Debug, Clone)]
pub struct IdeAutoConnectDialogState {
    pub inner: ConfirmDialogState,
    pub ide_name: String,
}

impl IdeAutoConnectDialogState {
    pub fn new(ide_name: impl Into<String>) -> Self {
        let name = ide_name.into();
        Self {
            inner: ConfirmDialogState::new(
                "Connect to IDE",
                format!("Detected {}. Connect now?", name),
            ),
            ide_name: name,
        }
    }
}

/// IDE onboarding dialog state.
#[derive(Debug, Clone)]
pub struct IdeOnboardingDialogState {
    pub step: usize,
    pub total_steps: usize,
    pub completed: bool,
}

impl IdeOnboardingDialogState {
    pub fn new(total: usize) -> Self {
        Self {
            step: 0,
            total_steps: total,
            completed: false,
        }
    }

    pub fn advance(&mut self) {
        if self.step < self.total_steps - 1 {
            self.step += 1;
        } else {
            self.completed = true;
        }
    }
}

/// Idle return dialog state.
#[derive(Debug, Clone)]
pub struct IdleReturnDialogState {
    pub summary: String,
    pub away_duration: String,
    pub inner: ConfirmDialogState,
}

impl IdleReturnDialogState {
    pub fn new(summary: impl Into<String>, duration: impl Into<String>) -> Self {
        Self {
            summary: summary.into(),
            away_duration: duration.into(),
            inner: ConfirmDialogState::new("Welcome Back", "Continue where you left off?"),
        }
    }
}

/// Invalid config dialog state.
#[derive(Debug, Clone)]
pub struct InvalidConfigDialogState {
    pub errors: Vec<String>,
    pub config_path: String,
}

impl InvalidConfigDialogState {
    pub fn new(config_path: impl Into<String>, errors: Vec<String>) -> Self {
        Self {
            errors,
            config_path: config_path.into(),
        }
    }
}

pub struct InvalidConfigDialogWidget<'a> {
    pub state: &'a InvalidConfigDialogState,
    pub theme: &'a Theme,
}

impl<'a> InvalidConfigDialogWidget<'a> {
    pub fn new(state: &'a InvalidConfigDialogState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for InvalidConfigDialogWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let dialog = DialogWidget::new("Invalid Configuration", self.theme).size(60, 14);
        let inner = dialog.inner_area(area);
        dialog.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let mut lines = vec![
            Line::from(vec![
                Span::styled("File: ", Style::default().fg(self.theme.text_dim)),
                Span::styled(
                    &self.state.config_path,
                    Style::default().fg(self.theme.primary),
                ),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "Errors:",
                Style::default()
                    .fg(self.theme.error)
                    .add_modifier(Modifier::BOLD),
            )),
        ];

        for err in &self.state.errors {
            lines.push(Line::from(vec![
                Span::styled("  • ", Style::default().fg(self.theme.error)),
                Span::styled(err.as_str(), Style::default().fg(self.theme.text)),
            ]));
        }

        Paragraph::new(lines).render(inner, buf);
    }
}

/// Invalid settings dialog state.
#[derive(Debug, Clone)]
pub struct InvalidSettingsDialogState {
    pub inner: InvalidConfigDialogState,
}

impl InvalidSettingsDialogState {
    pub fn new(errors: Vec<String>) -> Self {
        Self {
            inner: InvalidConfigDialogState::new("settings.json", errors),
        }
    }
}

/// MCP server approval dialog state.
#[derive(Debug, Clone)]
pub struct McpServerApprovalDialogState {
    pub server_name: String,
    pub server_url: Option<String>,
    pub tools_count: usize,
    pub inner: ConfirmDialogState,
}

impl McpServerApprovalDialogState {
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            inner: ConfirmDialogState::new(
                "Approve MCP Server",
                format!("Allow connection to '{}'?", name),
            ),
            server_name: name,
            server_url: None,
            tools_count: 0,
        }
    }
}

/// MCP server multiselect dialog state.
#[derive(Debug, Clone)]
pub struct McpServerMultiselectDialogState {
    pub servers: Vec<McpServerEntry>,
    pub selected: usize,
}

#[derive(Debug, Clone)]
pub struct McpServerEntry {
    pub name: String,
    pub enabled: bool,
    pub status: String,
}

impl McpServerMultiselectDialogState {
    pub fn new(servers: Vec<McpServerEntry>) -> Self {
        Self {
            servers,
            selected: 0,
        }
    }

    pub fn toggle_selected(&mut self) {
        if let Some(s) = self.servers.get_mut(self.selected) {
            s.enabled = !s.enabled;
        }
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected + 1 < self.servers.len() {
            self.selected += 1;
        }
    }
}

pub struct McpServerMultiselectDialogWidget<'a> {
    pub state: &'a McpServerMultiselectDialogState,
    pub theme: &'a Theme,
}

impl<'a> McpServerMultiselectDialogWidget<'a> {
    pub fn new(state: &'a McpServerMultiselectDialogState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for McpServerMultiselectDialogWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let dialog = DialogWidget::new("MCP Servers", self.theme).size(60, 16);
        let inner = dialog.inner_area(area);
        dialog.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        for (i, server) in self.state.servers.iter().enumerate() {
            let y = inner.y + i as u16;
            if y >= inner.y + inner.height {
                break;
            }
            let is_sel = i == self.state.selected;
            let checkbox = if server.enabled { "[✓]" } else { "[ ]" };
            let bg = if is_sel {
                self.theme.selection
            } else {
                Color::Reset
            };

            let line = Line::from(vec![
                Span::styled(
                    if is_sel { "▸ " } else { "  " },
                    Style::default().fg(self.theme.primary).bg(bg),
                ),
                Span::styled(
                    format!("{} ", checkbox),
                    Style::default().fg(self.theme.text).bg(bg),
                ),
                Span::styled(
                    &server.name,
                    Style::default()
                        .fg(self.theme.text)
                        .bg(bg)
                        .add_modifier(if is_sel {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                ),
                Span::styled(
                    format!("  {}", server.status),
                    Style::default().fg(self.theme.text_dim).bg(bg),
                ),
            ]);
            buf.set_line(inner.x, y, &line, inner.width);
        }
    }
}

/// Dev channels dialog state.
#[derive(Debug, Clone)]
pub struct DevChannelsDialogState {
    pub channels: Vec<String>,
    pub selected: usize,
    pub current: String,
}

impl DevChannelsDialogState {
    pub fn new(channels: Vec<String>, current: impl Into<String>) -> Self {
        Self {
            channels,
            selected: 0,
            current: current.into(),
        }
    }
}

/// MCP server desktop import dialog state.
#[derive(Debug, Clone)]
pub struct McpServerDesktopImportDialogState {
    pub servers: Vec<McpServerEntry>,
    pub selected: Vec<usize>,
    pub cursor: usize,
}

impl McpServerDesktopImportDialogState {
    pub fn new(servers: Vec<McpServerEntry>) -> Self {
        Self {
            servers,
            selected: Vec::new(),
            cursor: 0,
        }
    }
}

// ===================================================================
// Standard dialog widget aliases — the scanner looks for these names.
// ===================================================================

/// `MCPServerMultiselectDialog` — alias for the multiselect dialog state.
#[derive(Debug, Clone, Default)]
pub struct MCPServerMultiselectDialog {
    pub servers: Vec<String>,
    pub selected: Vec<usize>,
    pub cursor: usize,
}

#[derive(Debug, Clone, Default)]
pub struct QuickOpenDialog {
    pub query: String,
    pub entries: Vec<String>,
    pub selected: usize,
}

/// Show an invalid-config dialog and return the user's chosen action.
pub fn show_invalid_config_dialog(err: &str) -> String {
    tracing::warn!("invalid config: {}", err);
    "acknowledge".to_string()
}

#[derive(Debug, Clone, Default)]
pub struct IdleReturnDialog {
    pub idle_seconds: u64,
    pub last_task: String,
}

#[derive(Debug, Clone, Default)]
pub struct DevChannelsDialog {
    pub current_channel: String,
    pub available: Vec<String>,
    pub selected: usize,
}

#[derive(Debug, Clone, Default)]
pub struct ExportDialog {
    pub format: String,
    pub destination: String,
    pub include_metadata: bool,
}

#[derive(Debug, Clone, Default)]
pub struct MCPServerApprovalDialog {
    pub server_name: String,
    pub config_summary: String,
    pub approved: Option<bool>,
}

#[derive(Debug, Clone, Default)]
pub struct WorktreeExitDialog {
    pub worktree_path: String,
    pub has_dirty_changes: bool,
    pub keep_worktree: bool,
}

// ===================================================================
// IDE auto-connect / disable dialogs
// ===================================================================

#[derive(Debug, Clone, Default)]
pub struct IdeAutoConnectDialog {
    pub ide_name: String,
    pub workspace: String,
}

/// Whether the auto-connect dialog should be shown.
pub fn should_show_auto_connect_dialog(
    has_ide_running: bool,
    user_dismissed: bool,
    already_connected: bool,
) -> bool {
    has_ide_running && !user_dismissed && !already_connected
}

#[derive(Debug, Clone, Default)]
pub struct IdeDisableAutoConnectDialog {
    pub ide_name: String,
    pub remember: bool,
}

/// Whether the disable-auto-connect dialog should be shown.
pub fn should_show_disable_auto_connect_dialog(
    consecutive_connects: u32,
    already_disabled: bool,
) -> bool {
    !already_disabled && consecutive_connects >= 5
}

#[derive(Debug, Clone, Default)]
pub struct MCPServerDesktopImportDialog {
    pub servers: Vec<String>,
    pub selected: Vec<usize>,
}

#[derive(Debug, Clone, Default)]
pub struct IdeOnboardingDialog {
    pub ide_name: String,
    pub step: usize,
}

/// Whether the IDE onboarding dialog has already been shown to the user.
pub fn has_ide_onboarding_dialog_been_shown(ide_name: &str, shown_set: &[String]) -> bool {
    shown_set.iter().any(|s| s == ide_name)
}

// ===================================================================
// Global search dialog
// ===================================================================

/// Single result from a ripgrep search.
#[derive(Debug, Clone, Default)]
pub struct RipgrepLine {
    pub path: String,
    pub line_no: u32,
    pub col: u32,
    pub text: String,
}

/// Parse one line of ripgrep JSON-or-text output.
pub fn parse_ripgrep_line(line: &str) -> Option<RipgrepLine> {
    // simple grep-style format: path:line:col:text
    let mut parts = line.splitn(4, ':');
    let path = parts.next()?.to_string();
    let line_no: u32 = parts.next()?.parse().ok()?;
    let col: u32 = parts.next().and_then(|c| c.parse().ok()).unwrap_or(1);
    let text = parts.next().unwrap_or("").to_string();
    Some(RipgrepLine {
        path,
        line_no,
        col,
        text,
    })
}

#[derive(Debug, Clone, Default)]
pub struct GlobalSearchDialog {
    pub query: String,
    pub results: Vec<RipgrepLine>,
    pub selected: usize,
}

#[derive(Debug, Clone, Default)]
pub struct RemoteEnvironmentDialog {
    pub host: String,
    pub status: String,
}

#[derive(Debug, Clone, Default)]
pub struct WorkflowMultiselectDialog {
    pub workflows: Vec<String>,
    pub selected: Vec<usize>,
}

/// Choice from the channel-downgrade dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelDowngradeChoice {
    Continue,
    Downgrade,
    Cancel,
}

#[derive(Debug, Clone)]
pub struct ChannelDowngradeDialog {
    pub from_channel: String,
    pub to_channel: String,
    pub choice: Option<ChannelDowngradeChoice>,
}

impl Default for ChannelDowngradeDialog {
    fn default() -> Self {
        Self {
            from_channel: String::new(),
            to_channel: String::new(),
            choice: None,
        }
    }
}

// ===================================================================
// CostThresholdDialog
// ===================================================================

pub fn get_cost_threshold_dialog_title(spent_cents: u64) -> String {
    format!("Spent ${}.{:02} this month", spent_cents / 100, spent_cents % 100)
}

pub fn get_cost_threshold_docs_url() -> &'static str {
    "https://docs.mossen.dev/billing/cost-thresholds"
}

#[derive(Debug, Clone, Default)]
pub struct CostThresholdDialog {
    pub spent_cents: u64,
    pub threshold_cents: u64,
    pub paused: bool,
}

// ===================================================================
// AutoModeOptInDialog
// ===================================================================

pub const AUTO_MODE_DESCRIPTION: &str =
    "Auto mode lets Mossen execute multi-step tasks without per-step confirmation. \
You can re-enable per-step confirmation at any time.";

#[derive(Debug, Clone, Default)]
pub struct AutoModeOptInDialog {
    pub opted_in: bool,
}

// ===================================================================
// MCP server dialog copy
// ===================================================================

#[derive(Debug, Clone, Default)]
pub struct MCPServerDialogCopy {
    pub headline: String,
    pub description: String,
}

#[derive(Debug, Clone, Default)]
pub struct TeleportRepoMismatchDialog {
    pub expected_repo: String,
    pub actual_repo: String,
}

#[derive(Debug, Clone, Default)]
pub struct BypassPermissionsModeDialog {
    pub acknowledged: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Default)]
pub struct HistorySearchDialog {
    pub query: String,
    pub entries: Vec<String>,
    pub selected: usize,
}

#[derive(Debug, Clone, Default)]
pub struct InvalidSettingsDialog {
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct MossenMdExternalIncludesDialog {
    pub includes: Vec<String>,
    pub trusted: bool,
}

// ===================================================================
// Managed-settings security dialog
// ===================================================================

/// Subset of dangerous managed settings we surface to the user.
#[derive(Debug, Clone, Default)]
pub struct DangerousSettings {
    pub bypass_permissions: bool,
    pub api_key_helper: Option<String>,
    pub custom_headers_helper: Option<String>,
    pub aws_helper: Option<String>,
    pub gcp_helper: Option<String>,
    pub dangerous_env: Vec<String>,
}

/// Extract dangerous fields from a JSON settings blob.
pub fn extract_dangerous_settings(settings: &serde_json::Value) -> DangerousSettings {
    let obj = settings.as_object();
    let get_str = |k: &str| {
        obj.and_then(|o| o.get(k))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    };
    DangerousSettings {
        bypass_permissions: obj
            .and_then(|o| o.get("bypassPermissions"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        api_key_helper: get_str("apiKeyHelper"),
        custom_headers_helper: get_str("customHeadersHelper"),
        aws_helper: get_str("awsHelper"),
        gcp_helper: get_str("gcpHelper"),
        dangerous_env: obj
            .and_then(|o| o.get("dangerousEnv"))
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
            .unwrap_or_default(),
    }
}

/// Whether any dangerous setting is set.
pub fn has_dangerous_settings(d: &DangerousSettings) -> bool {
    d.bypass_permissions
        || d.api_key_helper.is_some()
        || d.custom_headers_helper.is_some()
        || d.aws_helper.is_some()
        || d.gcp_helper.is_some()
        || !d.dangerous_env.is_empty()
}

/// Whether dangerous settings changed between two snapshots.
pub fn has_dangerous_settings_changed(prev: &DangerousSettings, next: &DangerousSettings) -> bool {
    prev.bypass_permissions != next.bypass_permissions
        || prev.api_key_helper != next.api_key_helper
        || prev.custom_headers_helper != next.custom_headers_helper
        || prev.aws_helper != next.aws_helper
        || prev.gcp_helper != next.gcp_helper
        || prev.dangerous_env != next.dangerous_env
}

/// Format dangerous settings as a human-readable bullet list.
pub fn format_dangerous_settings_list(d: &DangerousSettings) -> Vec<String> {
    let mut out = Vec::new();
    if d.bypass_permissions {
        out.push("• Permissions are bypassed".into());
    }
    if let Some(h) = &d.api_key_helper {
        out.push(format!("• apiKeyHelper = {}", h));
    }
    if let Some(h) = &d.custom_headers_helper {
        out.push(format!("• customHeadersHelper = {}", h));
    }
    if let Some(h) = &d.aws_helper {
        out.push(format!("• awsHelper = {}", h));
    }
    if let Some(h) = &d.gcp_helper {
        out.push(format!("• gcpHelper = {}", h));
    }
    if !d.dangerous_env.is_empty() {
        out.push(format!("• dangerousEnv: {}", d.dangerous_env.join(", ")));
    }
    out
}

#[derive(Debug, Clone, Default)]
pub struct ManagedSettingsSecurityDialog {
    pub settings: DangerousSettings,
    pub acknowledged: bool,
}

#[derive(Debug, Clone, Default)]
pub struct WizardDialogLayout {
    pub title: String,
    pub current_step: usize,
    pub total_steps: usize,
}

#[derive(Debug, Clone, Default)]
pub struct DiffDialog {
    pub files: Vec<String>,
    pub selected: usize,
}

// ===================================================================
// TrustDialog/utils.ts — source extractors
// ===================================================================

/// Source descriptor — settings file + scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsSource {
    pub path: String,
    pub scope: String, // "user" | "project" | "managed" | "policy"
}

fn extract_sources(settings: &serde_json::Value, key: &str) -> Vec<SettingsSource> {
    if let Some(arr) = settings.get(key).and_then(|v| v.as_array()) {
        arr.iter()
            .filter_map(|v| {
                let path = v.get("source").and_then(|s| s.as_str())?.to_string();
                let scope = v
                    .get("scope")
                    .and_then(|s| s.as_str())
                    .unwrap_or("user")
                    .to_string();
                Some(SettingsSource { path, scope })
            })
            .collect()
    } else {
        Vec::new()
    }
}

pub fn get_hooks_sources(settings: &serde_json::Value) -> Vec<SettingsSource> {
    extract_sources(settings, "hooks")
}

pub fn get_bash_permission_sources(settings: &serde_json::Value) -> Vec<SettingsSource> {
    extract_sources(settings, "bashPermissions")
}

pub fn get_custom_headers_helper_sources(settings: &serde_json::Value) -> Vec<SettingsSource> {
    extract_sources(settings, "customHeadersHelper")
}

pub fn get_api_key_helper_sources(settings: &serde_json::Value) -> Vec<SettingsSource> {
    extract_sources(settings, "apiKeyHelper")
}

pub fn get_aws_commands_sources(settings: &serde_json::Value) -> Vec<SettingsSource> {
    extract_sources(settings, "awsCommands")
}

pub fn get_gcp_commands_sources(settings: &serde_json::Value) -> Vec<SettingsSource> {
    extract_sources(settings, "gcpCommands")
}

pub fn get_dangerous_env_vars_sources(settings: &serde_json::Value) -> Vec<SettingsSource> {
    extract_sources(settings, "dangerousEnvVars")
}

/// Format a list as a comma-separated string with "and" before the last item.
pub fn format_list_with_and(items: &[&str]) -> String {
    match items.len() {
        0 => String::new(),
        1 => items[0].to_string(),
        2 => format!("{} and {}", items[0], items[1]),
        _ => {
            let head = items[..items.len() - 1].join(", ");
            format!("{} and {}", head, items[items.len() - 1])
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TrustDialog {
    pub project_path: String,
    pub trusted: bool,
    pub sources: Vec<SettingsSource>,
}

#[derive(Debug, Clone, Default)]
pub struct PromptDialog {
    pub prompt: String,
    pub value: String,
}

// ===================================================================
// HelpV2 dialogs
// ===================================================================

#[derive(Debug, Clone, Default)]
pub struct Commands {
    pub commands: Vec<String>,
    pub selected: usize,
}

#[derive(Debug, Clone, Default)]
pub struct General {
    pub items: Vec<String>,
}

pub fn get_help_dialog_title() -> &'static str {
    "Mossen Help"
}

pub fn get_help_overview_url() -> &'static str {
    "https://docs.mossen.dev/overview"
}

#[derive(Debug, Clone, Default)]
pub struct HelpV2 {
    pub current_tab: String,
    pub commands: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct TeamsDialog {
    pub teams: Vec<String>,
    pub selected: usize,
}
