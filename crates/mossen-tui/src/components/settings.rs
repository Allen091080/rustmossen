//! Settings panel, MCP UI, Sandbox UI, Wizard components.
//!
//! Translates: components/Settings/ (4 files), components/mcp/ (12 files),
//! components/sandbox/ (5 files), components/wizard/ (5 files)

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Gauge, List, ListItem, Paragraph, Widget, Wrap},
};

use crate::components::design_system::DialogWidget;
use crate::theme::Theme;

// ===================================================================
// Settings panel (Settings/)
// ===================================================================

#[derive(Debug, Clone)]
pub struct SettingEntry {
    pub key: String,
    pub label: String,
    pub value: SettingValue,
    pub description: String,
    pub category: String,
}

#[derive(Debug, Clone)]
pub enum SettingValue {
    Bool(bool),
    Text(String),
    Choice {
        options: Vec<String>,
        selected: usize,
    },
    Number(f64),
}

impl SettingValue {
    pub fn display(&self) -> String {
        match self {
            Self::Bool(b) => if *b { "✓ On" } else { "✗ Off" }.to_string(),
            Self::Text(s) => s.clone(),
            Self::Choice { options, selected } => {
                options.get(*selected).cloned().unwrap_or_default()
            }
            Self::Number(n) => format!("{}", n),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SettingsPanelState {
    pub entries: Vec<SettingEntry>,
    pub selected: usize,
    pub filter: String,
    pub editing: bool,
}

impl SettingsPanelState {
    pub fn new(entries: Vec<SettingEntry>) -> Self {
        Self {
            entries,
            selected: 0,
            filter: String::new(),
            editing: false,
        }
    }

    pub fn filtered_entries(&self) -> Vec<(usize, &SettingEntry)> {
        if self.filter.is_empty() {
            self.entries.iter().enumerate().collect()
        } else {
            let q = self.filter.to_lowercase();
            self.entries
                .iter()
                .enumerate()
                .filter(|(_, e)| {
                    e.label.to_lowercase().contains(&q)
                        || e.key.to_lowercase().contains(&q)
                        || e.category.to_lowercase().contains(&q)
                })
                .collect()
        }
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        let max = self.filtered_entries().len();
        if self.selected + 1 < max {
            self.selected += 1;
        }
    }
}

pub struct SettingsPanelWidget<'a> {
    pub state: &'a SettingsPanelState,
    pub theme: &'a Theme,
}

impl<'a> SettingsPanelWidget<'a> {
    pub fn new(state: &'a SettingsPanelState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for SettingsPanelWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.theme.style_border())
            .title(" Settings ");

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.width == 0 || inner.height < 2 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .split(inner);

        // Filter
        let filter_line = if self.state.filter.is_empty() {
            Line::from(Span::styled(
                "Type to filter...",
                Style::default().fg(self.theme.text_dim),
            ))
        } else {
            Line::from(vec![
                Span::styled("Filter: ", Style::default().fg(self.theme.text_dim)),
                Span::styled(&self.state.filter, Style::default().fg(self.theme.text)),
            ])
        };
        buf.set_line(chunks[0].x, chunks[0].y, &filter_line, chunks[0].width);

        // Entries
        let filtered = self.state.filtered_entries();
        for (vi, (_, entry)) in filtered.iter().enumerate() {
            let y = chunks[1].y + vi as u16;
            if y >= chunks[1].y + chunks[1].height {
                break;
            }

            let is_sel = vi == self.state.selected;
            let bg = if is_sel {
                self.theme.selection
            } else {
                Color::Reset
            };
            for x in chunks[1].x..chunks[1].x + chunks[1].width {
                buf.set_string(x, y, " ", Style::default().bg(bg));
            }

            let line = Line::from(vec![
                Span::styled(
                    if is_sel { "▸ " } else { "  " },
                    Style::default().fg(self.theme.primary).bg(bg),
                ),
                Span::styled(
                    &entry.label,
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
                    format!("  {}", entry.value.display()),
                    Style::default().fg(self.theme.info).bg(bg),
                ),
            ]);
            buf.set_line(chunks[1].x, y, &line, chunks[1].width);
        }
    }
}

// ===================================================================
// MCP UI (components/mcp/)
// ===================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpServerStatus {
    Connected,
    Connecting,
    Disconnected,
    Error,
}

impl McpServerStatus {
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Connected => "●",
            Self::Connecting => "◌",
            Self::Disconnected => "○",
            Self::Error => "✗",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Connected => "Connected",
            Self::Connecting => "Connecting",
            Self::Disconnected => "Disconnected",
            Self::Error => "Error",
        }
    }
}

#[derive(Debug, Clone)]
pub struct McpServerInfo {
    pub name: String,
    pub status: McpServerStatus,
    pub tool_count: usize,
    pub resource_count: usize,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct McpPanelState {
    pub servers: Vec<McpServerInfo>,
    pub selected: usize,
}

impl McpPanelState {
    pub fn new(servers: Vec<McpServerInfo>) -> Self {
        Self {
            servers,
            selected: 0,
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

pub struct McpPanelWidget<'a> {
    pub state: &'a McpPanelState,
    pub theme: &'a Theme,
}

impl<'a> McpPanelWidget<'a> {
    pub fn new(state: &'a McpPanelState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for McpPanelWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.theme.style_border())
            .title(Span::styled(
                " Bridge Servers ",
                Style::default()
                    .fg(self.theme.primary)
                    .add_modifier(Modifier::BOLD),
            ));
        let inner = block.inner(area);
        block.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        for (i, server) in self.state.servers.iter().enumerate() {
            let y = inner.y + i as u16;
            if y >= inner.y + inner.height {
                break;
            }

            let is_sel = i == self.state.selected;
            let status_color = match server.status {
                McpServerStatus::Connected => self.theme.success,
                McpServerStatus::Connecting => self.theme.warning,
                McpServerStatus::Disconnected => self.theme.text_dim,
                McpServerStatus::Error => self.theme.error,
            };
            let bg = if is_sel {
                self.theme.selection
            } else {
                Color::Reset
            };

            for x in inner.x..inner.x + inner.width {
                buf.set_string(x, y, " ", Style::default().bg(bg));
            }

            let line = Line::from(vec![
                Span::styled(
                    format!("{} ", server.status.icon()),
                    Style::default().fg(status_color).bg(bg),
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
                    format!(
                        "  {} tools, {} resources",
                        server.tool_count, server.resource_count
                    ),
                    Style::default().fg(self.theme.text_dim).bg(bg),
                ),
            ]);
            buf.set_line(inner.x, y, &line, inner.width);
        }
    }
}

// ===================================================================
// Sandbox UI (components/sandbox/)
// ===================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxMode {
    Enabled,
    Disabled,
    Partial,
}

#[derive(Debug, Clone)]
pub struct SandboxPanelState {
    pub mode: SandboxMode,
    pub violations: Vec<SandboxViolation>,
    pub allow_list: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SandboxViolation {
    pub path: String,
    pub operation: String,
    pub timestamp: String,
}

impl SandboxPanelState {
    pub fn new(mode: SandboxMode) -> Self {
        Self {
            mode,
            violations: Vec::new(),
            allow_list: Vec::new(),
        }
    }
}

pub struct SandboxPanelWidget<'a> {
    pub state: &'a SandboxPanelState,
    pub theme: &'a Theme,
}

impl<'a> SandboxPanelWidget<'a> {
    pub fn new(state: &'a SandboxPanelState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for SandboxPanelWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mode_label = match self.state.mode {
            SandboxMode::Enabled => "Enabled",
            SandboxMode::Disabled => "Disabled",
            SandboxMode::Partial => "Partial",
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.theme.style_border())
            .title(format!(" Sandbox [{}] ", mode_label));

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        if self.state.violations.is_empty() {
            let msg = Paragraph::new("No violations recorded")
                .style(Style::default().fg(self.theme.text_dim));
            msg.render(inner, buf);
        } else {
            let items: Vec<ListItem> = self
                .state
                .violations
                .iter()
                .map(|v| {
                    let line = Line::from(vec![
                        Span::styled("✗ ", Style::default().fg(self.theme.error)),
                        Span::styled(&v.operation, Style::default().fg(self.theme.text)),
                        Span::styled(
                            format!("  {}", v.path),
                            Style::default().fg(self.theme.text_dim),
                        ),
                    ]);
                    ListItem::new(line)
                })
                .collect();
            List::new(items).render(inner, buf);
        }
    }
}

// ===================================================================
// Wizard (components/wizard/)
// ===================================================================

#[derive(Debug, Clone)]
pub struct WizardStep {
    pub title: String,
    pub description: String,
    pub completed: bool,
}

#[derive(Debug, Clone)]
pub struct WizardState {
    pub title: String,
    pub steps: Vec<WizardStep>,
    pub current_step: usize,
    pub completed: bool,
}

impl WizardState {
    pub fn new(title: impl Into<String>, steps: Vec<WizardStep>) -> Self {
        Self {
            title: title.into(),
            steps,
            current_step: 0,
            completed: false,
        }
    }

    pub fn advance(&mut self) {
        if let Some(step) = self.steps.get_mut(self.current_step) {
            step.completed = true;
        }
        if self.current_step + 1 < self.steps.len() {
            self.current_step += 1;
        } else {
            self.completed = true;
        }
    }

    pub fn progress(&self) -> f64 {
        if self.steps.is_empty() {
            return 0.0;
        }
        self.steps.iter().filter(|s| s.completed).count() as f64 / self.steps.len() as f64
    }
}

pub struct WizardWidget<'a> {
    pub state: &'a WizardState,
    pub theme: &'a Theme,
}

impl<'a> WizardWidget<'a> {
    pub fn new(state: &'a WizardState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for WizardWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let dialog = DialogWidget::new(&self.state.title, self.theme).size(60, 16);
        let inner = dialog.inner_area(area);
        dialog.render(area, buf);

        if inner.width == 0 || inner.height < 4 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // progress
                Constraint::Length(1), // space
                Constraint::Min(1),    // steps
            ])
            .split(inner);

        // Progress bar
        let progress = self.state.progress();
        let gauge = Gauge::default()
            .gauge_style(
                Style::default()
                    .fg(self.theme.primary)
                    .bg(self.theme.surface),
            )
            .ratio(progress)
            .label(format!(
                "Step {} of {}",
                self.state.current_step + 1,
                self.state.steps.len()
            ));
        gauge.render(chunks[0], buf);

        // Steps
        for (i, step) in self.state.steps.iter().enumerate() {
            let y = chunks[2].y + i as u16;
            if y >= chunks[2].y + chunks[2].height {
                break;
            }

            let is_current = i == self.state.current_step;
            let icon = if step.completed {
                "✓"
            } else if is_current {
                "▸"
            } else {
                "○"
            };
            let icon_color = if step.completed {
                self.theme.success
            } else if is_current {
                self.theme.primary
            } else {
                self.theme.text_dim
            };

            let line = Line::from(vec![
                Span::styled(format!(" {} ", icon), Style::default().fg(icon_color)),
                Span::styled(
                    &step.title,
                    if is_current {
                        Style::default()
                            .fg(self.theme.text)
                            .add_modifier(Modifier::BOLD)
                    } else if step.completed {
                        Style::default().fg(self.theme.text_dim)
                    } else {
                        Style::default().fg(self.theme.text)
                    },
                ),
            ]);
            buf.set_line(chunks[2].x, y, &line, chunks[2].width);
        }
    }
}

// ===================================================================
// Settings top-level surfaces
// ===================================================================

/// Top-level settings dialog state — tabs + selection.
#[derive(Debug, Clone, Default)]
pub struct Settings {
    pub tabs: Vec<String>,
    pub selected_tab: usize,
    pub dirty: bool,
}

/// Config tab state.
#[derive(Debug, Clone, Default)]
pub struct Config {
    pub key_values: Vec<(String, String)>,
    pub selected: usize,
}

/// Usage tab state — token usage + cost summary.
#[derive(Debug, Clone, Default)]
pub struct Usage {
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_read_tokens: u64,
    pub total_cache_write_tokens: u64,
    pub cost_cents: u64,
}

impl Usage {
    pub fn formatted_cost(&self) -> String {
        format!("${}.{:02}", self.cost_cents / 100, self.cost_cents % 100)
    }
}

/// Build diagnostics list for the Status tab.
pub fn build_diagnostics(
    has_network: bool,
    has_credentials: bool,
    plugin_count: u32,
    mcp_count: u32,
) -> Vec<(String, bool)> {
    vec![
        ("Network".into(), has_network),
        ("Credentials".into(), has_credentials),
        (format!("Plugins ({})", plugin_count), plugin_count > 0),
        (format!("MCP servers ({})", mcp_count), mcp_count > 0),
    ]
}
