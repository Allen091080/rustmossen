//! Minor directory components.
//!
//! Translates: grove/Grove.tsx, memory/MemoryFileSelector.tsx, memory/MemoryUpdateNotification.tsx,
//! sandbox/SandboxConfigTab.tsx, sandbox/SandboxDependenciesTab.tsx, sandbox/SandboxDoctorSection.tsx,
//! sandbox/SandboxOverridesTab.tsx, sandbox/SandboxSettings.tsx, shell/ExpandShellOutputContext.tsx,
//! shell/OutputLine.tsx, shell/ShellProgressMessage.tsx, shell/ShellTimeDisplay.tsx,
//! skills/SkillsMenu.tsx, teams/TeamStatus.tsx, teams/TeamsDialog.tsx,
//! ui/OrderedList.tsx, ui/OrderedListItem.tsx, ui/TreeSelect.tsx,
//! wizard/WizardDialogLayout.tsx, wizard/WizardNavigationFooter.tsx, wizard/WizardProvider.tsx,
//! wizard/useWizard.ts, wizard/index.ts,
//! DesktopUpsell/DesktopUpsell.tsx, HelpV2/HelpCommands.tsx, HelpV2/HelpKeybindings.tsx,
//! HelpV2/HelpV2.tsx, HighlightedCode/HighlightedCode.tsx, LspRecommendation/LspRecommendation.tsx,
//! ManagedSettingsSecurityDialog/..., MossenHint/MossenHint.tsx, Passes/Passes.tsx,
//! StructuredDiff/..., TrustDialog/..., diff/...

use std::collections::HashMap;

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Widget, Wrap},
};

use crate::components::design_system::DialogWidget;
use crate::theme::Theme;

// ===================================================================
// Grove — from grove/Grove.tsx
// ===================================================================

/// Decision from the Grove terms dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroveDecision {
    AcceptOptIn,
    AcceptOptOut,
    Defer,
    Escape,
    SkipRendering,
}

/// Display location for the Grove dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroveLocation {
    Settings,
    PolicyUpdateModal,
    Onboarding,
}

/// State for the Grove terms dialog.
#[derive(Debug, Clone)]
pub struct GroveDialogState {
    pub location: GroveLocation,
    pub show_if_already_viewed: bool,
    pub selected_index: usize,
    pub is_grace_period: bool,
    pub decision: Option<GroveDecision>,
}

impl GroveDialogState {
    pub fn new(location: GroveLocation) -> Self {
        Self {
            location,
            show_if_already_viewed: false,
            selected_index: 0,
            is_grace_period: true,
            decision: None,
        }
    }

    pub fn options(&self) -> Vec<(&'static str, GroveDecision)> {
        vec![
            ("Accept & opt in to training", GroveDecision::AcceptOptIn),
            ("Accept & opt out of training", GroveDecision::AcceptOptOut),
            ("Remind me later", GroveDecision::Defer),
        ]
    }

    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    pub fn move_down(&mut self) {
        let max = self.options().len().saturating_sub(1);
        if self.selected_index < max {
            self.selected_index += 1;
        }
    }

    pub fn confirm(&mut self) {
        let options = self.options();
        if let Some((_, decision)) = options.get(self.selected_index) {
            self.decision = Some(*decision);
        }
    }
}

/// Widget for the Grove terms dialog.
pub struct GroveDialogWidget<'a> {
    pub state: &'a GroveDialogState,
    pub theme: &'a Theme,
}

impl<'a> GroveDialogWidget<'a> {
    pub fn new(state: &'a GroveDialogState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for GroveDialogWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let dialog = DialogWidget::new("Platform Terms Update", self.theme).size(65, 18);
        let inner = dialog.inner_area(area);
        dialog.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Message
                Constraint::Min(1),    // Options
            ])
            .split(inner);

        // Message
        let msg = "An update to our Platform Terms and Privacy Policy is available. Please review and accept.";
        Paragraph::new(msg)
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(self.theme.text))
            .render(chunks[0], buf);

        // Options
        let options = self.state.options();
        let items: Vec<ListItem> = options
            .iter()
            .enumerate()
            .map(|(i, (label, _))| {
                let is_selected = i == self.state.selected_index;
                let prefix = if is_selected { "❯ " } else { "  " };
                let style = if is_selected {
                    Style::default().fg(self.theme.primary).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                ListItem::new(Line::from(Span::styled(
                    format!("{}{}", prefix, label),
                    style,
                )))
            })
            .collect();
        let list = List::new(items);
        Widget::render(list, chunks[1], buf);
    }
}

// ===================================================================
// Memory — from memory/MemoryFileSelector.tsx, memory/MemoryUpdateNotification.tsx
// ===================================================================

/// State for the memory file selector.
#[derive(Debug, Clone)]
pub struct MemoryFileSelectorState {
    pub files: Vec<String>,
    pub selected_index: usize,
}

impl MemoryFileSelectorState {
    pub fn new(files: Vec<String>) -> Self {
        Self {
            files,
            selected_index: 0,
        }
    }

    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected_index + 1 < self.files.len() {
            self.selected_index += 1;
        }
    }

    pub fn selected_file(&self) -> Option<&str> {
        self.files.get(self.selected_index).map(|s| s.as_str())
    }
}

/// Widget for the memory file selector.
pub struct MemoryFileSelectorWidget<'a> {
    pub state: &'a MemoryFileSelectorState,
    pub theme: &'a Theme,
}

impl<'a> MemoryFileSelectorWidget<'a> {
    pub fn new(state: &'a MemoryFileSelectorState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for MemoryFileSelectorWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let dialog = DialogWidget::new("Select Memory File", self.theme).size(50, 12);
        let inner = dialog.inner_area(area);
        dialog.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let items: Vec<ListItem> = self
            .state
            .files
            .iter()
            .enumerate()
            .map(|(i, file)| {
                let is_selected = i == self.state.selected_index;
                let prefix = if is_selected { "❯ " } else { "  " };
                let style = if is_selected {
                    Style::default().fg(self.theme.primary).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                ListItem::new(Line::from(Span::styled(format!("{}{}", prefix, file), style)))
            })
            .collect();
        let list = List::new(items);
        Widget::render(list, inner, buf);
    }
}

/// Notification about a memory update.
#[derive(Debug, Clone)]
pub struct MemoryUpdateNotification {
    pub message: String,
    pub file_path: String,
    pub visible: bool,
}

impl MemoryUpdateNotification {
    pub fn new(message: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            file_path: path.into(),
            visible: true,
        }
    }

    pub fn dismiss(&mut self) {
        self.visible = false;
    }
}

/// Widget for memory update notification.
pub struct MemoryUpdateNotificationWidget<'a> {
    pub notification: &'a MemoryUpdateNotification,
    pub theme: &'a Theme,
}

impl<'a> MemoryUpdateNotificationWidget<'a> {
    pub fn new(notification: &'a MemoryUpdateNotification, theme: &'a Theme) -> Self {
        Self { notification, theme }
    }
}

impl<'a> Widget for MemoryUpdateNotificationWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.notification.visible || area.height == 0 {
            return;
        }
        let line = Line::from(vec![
            Span::styled("💾 ", Style::default()),
            Span::styled(&self.notification.message, Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!(" ({})", self.notification.file_path),
                Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
            ),
        ]);
        Paragraph::new(line).render(area, buf);
    }
}

// ===================================================================
// Sandbox — from sandbox/*.tsx
// ===================================================================

/// Tab in the sandbox settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxTab {
    Config,
    Dependencies,
    Doctor,
    Overrides,
}

impl SandboxTab {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Config => "Config",
            Self::Dependencies => "Dependencies",
            Self::Doctor => "Doctor",
            Self::Overrides => "Overrides",
        }
    }

    pub fn all() -> &'static [SandboxTab] {
        &[Self::Config, Self::Dependencies, Self::Doctor, Self::Overrides]
    }
}

/// Sandbox configuration entry.
#[derive(Debug, Clone)]
pub struct SandboxConfigEntry {
    pub key: String,
    pub value: String,
    pub description: Option<String>,
    pub is_default: bool,
}

/// Sandbox dependency entry.
#[derive(Debug, Clone)]
pub struct SandboxDependency {
    pub name: String,
    pub version: String,
    pub status: SandboxDepStatus,
}

/// Dependency status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxDepStatus {
    Installed,
    Missing,
    Outdated,
}

/// Doctor check result.
#[derive(Debug, Clone)]
pub struct DoctorCheck {
    pub name: String,
    pub passed: bool,
    pub message: String,
}

/// Override entry.
#[derive(Debug, Clone)]
pub struct SandboxOverride {
    pub path_pattern: String,
    pub permission: String,
    pub source: String,
}

/// Full sandbox settings state.
#[derive(Debug, Clone)]
pub struct SandboxSettingsState {
    pub active_tab: SandboxTab,
    pub config_entries: Vec<SandboxConfigEntry>,
    pub dependencies: Vec<SandboxDependency>,
    pub doctor_checks: Vec<DoctorCheck>,
    pub overrides: Vec<SandboxOverride>,
    pub selected_index: usize,
}

impl SandboxSettingsState {
    pub fn new() -> Self {
        Self {
            active_tab: SandboxTab::Config,
            config_entries: Vec::new(),
            dependencies: Vec::new(),
            doctor_checks: Vec::new(),
            overrides: Vec::new(),
            selected_index: 0,
        }
    }

    pub fn switch_tab(&mut self, tab: SandboxTab) {
        self.active_tab = tab;
        self.selected_index = 0;
    }

    pub fn next_tab(&mut self) {
        let tabs = SandboxTab::all();
        let idx = tabs.iter().position(|t| *t == self.active_tab).unwrap_or(0);
        let new_idx = (idx + 1) % tabs.len();
        self.active_tab = tabs[new_idx];
        self.selected_index = 0;
    }

    pub fn prev_tab(&mut self) {
        let tabs = SandboxTab::all();
        let idx = tabs.iter().position(|t| *t == self.active_tab).unwrap_or(0);
        let new_idx = if idx == 0 { tabs.len() - 1 } else { idx - 1 };
        self.active_tab = tabs[new_idx];
        self.selected_index = 0;
    }

    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    pub fn move_down(&mut self) {
        let max = match self.active_tab {
            SandboxTab::Config => self.config_entries.len(),
            SandboxTab::Dependencies => self.dependencies.len(),
            SandboxTab::Doctor => self.doctor_checks.len(),
            SandboxTab::Overrides => self.overrides.len(),
        };
        if self.selected_index + 1 < max {
            self.selected_index += 1;
        }
    }
}

impl Default for SandboxSettingsState {
    fn default() -> Self {
        Self::new()
    }
}

/// Widget for sandbox settings panel.
pub struct SandboxSettingsWidget<'a> {
    pub state: &'a SandboxSettingsState,
    pub theme: &'a Theme,
}

impl<'a> SandboxSettingsWidget<'a> {
    pub fn new(state: &'a SandboxSettingsState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for SandboxSettingsWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let dialog = DialogWidget::new("Sandbox Settings", self.theme).size(70, 22);
        let inner = dialog.inner_area(area);
        dialog.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .split(inner);

        // Tab bar
        let tabs: Vec<Span> = SandboxTab::all()
            .iter()
            .map(|tab| {
                let style = if *tab == self.state.active_tab {
                    Style::default().fg(self.theme.primary).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                Span::styled(format!(" {} ", tab.label()), style)
            })
            .collect();
        let tab_line = Line::from(tabs);
        buf.set_line(chunks[0].x, chunks[0].y, &tab_line, chunks[0].width);

        // Content based on active tab
        match self.state.active_tab {
            SandboxTab::Config => {
                let items: Vec<ListItem> = self
                    .state
                    .config_entries
                    .iter()
                    .enumerate()
                    .map(|(i, entry)| {
                        let is_sel = i == self.state.selected_index;
                        let style = if is_sel {
                            Style::default().fg(self.theme.primary)
                        } else {
                            Style::default()
                        };
                        let default_mark = if entry.is_default { " (default)" } else { "" };
                        ListItem::new(Line::from(vec![
                            Span::styled(&entry.key, style),
                            Span::raw(": "),
                            Span::styled(&entry.value, Style::default().fg(Color::DarkGray)),
                            Span::styled(default_mark, Style::default().fg(Color::DarkGray)),
                        ]))
                    })
                    .collect();
                let list = List::new(items);
                Widget::render(list, chunks[1], buf);
            }
            SandboxTab::Dependencies => {
                let items: Vec<ListItem> = self
                    .state
                    .dependencies
                    .iter()
                    .enumerate()
                    .map(|(i, dep)| {
                        let is_sel = i == self.state.selected_index;
                        let (icon, color) = match dep.status {
                            SandboxDepStatus::Installed => ("✓", Color::Green),
                            SandboxDepStatus::Missing => ("✗", Color::Red),
                            SandboxDepStatus::Outdated => ("△", Color::Yellow),
                        };
                        let style = if is_sel {
                            Style::default().fg(self.theme.primary)
                        } else {
                            Style::default()
                        };
                        ListItem::new(Line::from(vec![
                            Span::styled(format!("{} ", icon), Style::default().fg(color)),
                            Span::styled(&dep.name, style),
                            Span::styled(
                                format!(" v{}", dep.version),
                                Style::default().fg(Color::DarkGray),
                            ),
                        ]))
                    })
                    .collect();
                let list = List::new(items);
                Widget::render(list, chunks[1], buf);
            }
            SandboxTab::Doctor => {
                let items: Vec<ListItem> = self
                    .state
                    .doctor_checks
                    .iter()
                    .map(|check| {
                        let (icon, color) = if check.passed {
                            ("✓", Color::Green)
                        } else {
                            ("✗", Color::Red)
                        };
                        ListItem::new(Line::from(vec![
                            Span::styled(format!("{} ", icon), Style::default().fg(color)),
                            Span::raw(&check.name),
                            Span::styled(
                                format!(" — {}", check.message),
                                Style::default().fg(Color::DarkGray),
                            ),
                        ]))
                    })
                    .collect();
                let list = List::new(items);
                Widget::render(list, chunks[1], buf);
            }
            SandboxTab::Overrides => {
                let items: Vec<ListItem> = self
                    .state
                    .overrides
                    .iter()
                    .enumerate()
                    .map(|(i, ovr)| {
                        let is_sel = i == self.state.selected_index;
                        let style = if is_sel {
                            Style::default().fg(self.theme.primary)
                        } else {
                            Style::default()
                        };
                        ListItem::new(Line::from(vec![
                            Span::styled(&ovr.path_pattern, style),
                            Span::raw(" → "),
                            Span::styled(&ovr.permission, Style::default().fg(Color::DarkGray)),
                            Span::styled(
                                format!(" ({})", ovr.source),
                                Style::default().fg(Color::DarkGray),
                            ),
                        ]))
                    })
                    .collect();
                let list = List::new(items);
                Widget::render(list, chunks[1], buf);
            }
        }
    }
}

// ===================================================================
// Shell — from shell/*.tsx
// ===================================================================

/// Shell output line data.
#[derive(Debug, Clone)]
pub struct ShellOutputLine {
    pub content: String,
    pub is_stderr: bool,
    pub line_number: usize,
}

/// Shell progress state.
#[derive(Debug, Clone)]
pub struct ShellProgressState {
    pub command: String,
    pub elapsed_ms: u64,
    pub exit_code: Option<i32>,
    pub is_running: bool,
}

impl ShellProgressState {
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            elapsed_ms: 0,
            exit_code: None,
            is_running: true,
        }
    }

    pub fn finish(&mut self, code: i32) {
        self.exit_code = Some(code);
        self.is_running = false;
    }

    pub fn elapsed_display(&self) -> String {
        let secs = self.elapsed_ms / 1000;
        if secs < 60 {
            format!("{}s", secs)
        } else {
            format!("{}m{}s", secs / 60, secs % 60)
        }
    }
}

/// Widget for shell progress message.
pub struct ShellProgressWidget<'a> {
    pub state: &'a ShellProgressState,
    pub theme: &'a Theme,
}

impl<'a> ShellProgressWidget<'a> {
    pub fn new(state: &'a ShellProgressState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for ShellProgressWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width < 10 {
            return;
        }

        let status = if self.state.is_running {
            Span::styled("⟳ running", Style::default().fg(Color::Yellow))
        } else {
            match self.state.exit_code {
                Some(0) => Span::styled("✓ done", Style::default().fg(Color::Green)),
                Some(code) => Span::styled(format!("✗ exit {}", code), Style::default().fg(Color::Red)),
                None => Span::styled("? unknown", Style::default().fg(Color::DarkGray)),
            }
        };

        let line = Line::from(vec![
            Span::styled("$ ", Style::default().fg(Color::DarkGray)),
            Span::raw(&self.state.command),
            Span::raw("  "),
            status,
            Span::styled(
                format!("  {}", self.state.elapsed_display()),
                Style::default().fg(Color::DarkGray),
            ),
        ]);
        Paragraph::new(line).render(area, buf);
    }
}

/// Whether expanded shell output context is enabled.
#[derive(Debug, Clone)]
pub struct ExpandShellOutputContext {
    pub expanded: bool,
    pub max_lines: usize,
}

impl ExpandShellOutputContext {
    pub fn new() -> Self {
        Self {
            expanded: false,
            max_lines: 20,
        }
    }

    pub fn toggle(&mut self) {
        self.expanded = !self.expanded;
    }

    pub fn visible_lines(&self, total: usize) -> usize {
        if self.expanded {
            total
        } else {
            self.max_lines.min(total)
        }
    }
}

impl Default for ExpandShellOutputContext {
    fn default() -> Self {
        Self::new()
    }
}

// ===================================================================
// Skills — from skills/SkillsMenu.tsx
// ===================================================================

/// A skill in the skills menu.
#[derive(Debug, Clone)]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
    pub command: String,
    pub enabled: bool,
}

/// State for the skills menu.
#[derive(Debug, Clone)]
pub struct SkillsMenuState {
    pub skills: Vec<SkillInfo>,
    pub selected_index: usize,
}

impl SkillsMenuState {
    pub fn new(skills: Vec<SkillInfo>) -> Self {
        Self {
            skills,
            selected_index: 0,
        }
    }

    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected_index + 1 < self.skills.len() {
            self.selected_index += 1;
        }
    }

    pub fn selected_skill(&self) -> Option<&SkillInfo> {
        self.skills.get(self.selected_index)
    }
}

/// Widget for the skills menu.
pub struct SkillsMenuWidget<'a> {
    pub state: &'a SkillsMenuState,
    pub theme: &'a Theme,
}

impl<'a> SkillsMenuWidget<'a> {
    pub fn new(state: &'a SkillsMenuState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for SkillsMenuWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let dialog = DialogWidget::new("Skills", self.theme).size(55, 16);
        let inner = dialog.inner_area(area);
        dialog.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let items: Vec<ListItem> = self
            .state
            .skills
            .iter()
            .enumerate()
            .map(|(i, skill)| {
                let is_sel = i == self.state.selected_index;
                let prefix = if is_sel { "❯ " } else { "  " };
                let style = if is_sel {
                    Style::default().fg(self.theme.primary).add_modifier(Modifier::BOLD)
                } else if !skill.enabled {
                    Style::default().fg(Color::DarkGray)
                } else {
                    Style::default()
                };
                let line = Line::from(vec![
                    Span::raw(prefix.to_string()),
                    Span::styled(format!("/{}", skill.command), style),
                    Span::styled(
                        format!(" — {}", skill.description),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]);
                ListItem::new(line)
            })
            .collect();
        let list = List::new(items);
        Widget::render(list, inner, buf);
    }
}

// ===================================================================
// Teams — from teams/TeamStatus.tsx, teams/TeamsDialog.tsx
// ===================================================================

/// Status of a team member.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeamMemberStatus {
    Active,
    Idle,
    Offline,
}

/// Team member info.
#[derive(Debug, Clone)]
pub struct TeamMember {
    pub name: String,
    pub role: String,
    pub status: TeamMemberStatus,
    pub current_task: Option<String>,
}

/// State for the teams dialog.
#[derive(Debug, Clone)]
pub struct TeamsDialogState {
    pub members: Vec<TeamMember>,
    pub selected_index: usize,
}

impl TeamsDialogState {
    pub fn new(members: Vec<TeamMember>) -> Self {
        Self {
            members,
            selected_index: 0,
        }
    }

    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected_index + 1 < self.members.len() {
            self.selected_index += 1;
        }
    }

    pub fn active_count(&self) -> usize {
        self.members.iter().filter(|m| m.status == TeamMemberStatus::Active).count()
    }
}

/// Widget for team status display.
pub struct TeamStatusWidget<'a> {
    pub state: &'a TeamsDialogState,
    pub theme: &'a Theme,
}

impl<'a> TeamStatusWidget<'a> {
    pub fn new(state: &'a TeamsDialogState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for TeamStatusWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let dialog = DialogWidget::new("Team Members", self.theme).size(60, 16);
        let inner = dialog.inner_area(area);
        dialog.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let items: Vec<ListItem> = self
            .state
            .members
            .iter()
            .enumerate()
            .map(|(i, member)| {
                let is_sel = i == self.state.selected_index;
                let (icon, color) = match member.status {
                    TeamMemberStatus::Active => ("●", Color::Green),
                    TeamMemberStatus::Idle => ("○", Color::Yellow),
                    TeamMemberStatus::Offline => ("○", Color::DarkGray),
                };
                let prefix = if is_sel { "❯ " } else { "  " };
                let style = if is_sel {
                    Style::default().fg(self.theme.primary).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                let mut spans = vec![
                    Span::raw(prefix.to_string()),
                    Span::styled(format!("{} ", icon), Style::default().fg(color)),
                    Span::styled(&member.name, style),
                    Span::styled(format!(" ({})", member.role), Style::default().fg(Color::DarkGray)),
                ];
                if let Some(ref task) = member.current_task {
                    spans.push(Span::styled(
                        format!(" — {}", task),
                        Style::default().fg(Color::DarkGray),
                    ));
                }
                ListItem::new(Line::from(spans))
            })
            .collect();
        let list = List::new(items);
        Widget::render(list, inner, buf);
    }
}

// ===================================================================
// UI — from ui/OrderedList.tsx, ui/OrderedListItem.tsx, ui/TreeSelect.tsx
// ===================================================================

/// An item in an ordered list.
#[derive(Debug, Clone)]
pub struct OrderedListItem {
    pub content: String,
    pub sub_items: Vec<String>,
}

/// Widget for an ordered list.
pub struct OrderedListWidget<'a> {
    pub items: &'a [OrderedListItem],
    pub theme: &'a Theme,
}

impl<'a> OrderedListWidget<'a> {
    pub fn new(items: &'a [OrderedListItem], theme: &'a Theme) -> Self {
        Self { items, theme }
    }
}

impl<'a> Widget for OrderedListWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || self.items.is_empty() {
            return;
        }
        let mut y = area.y;
        for (i, item) in self.items.iter().enumerate() {
            if y >= area.y + area.height {
                break;
            }
            let prefix = format!("{}. ", i + 1);
            let line = Line::from(vec![
                Span::styled(&prefix, Style::default().fg(Color::DarkGray)),
                Span::raw(&item.content),
            ]);
            buf.set_line(area.x, y, &line, area.width);
            y += 1;

            for sub in &item.sub_items {
                if y >= area.y + area.height {
                    break;
                }
                let sub_line = Line::from(vec![
                    Span::raw("   • "),
                    Span::styled(sub.as_str(), Style::default().fg(Color::DarkGray)),
                ]);
                buf.set_line(area.x, y, &sub_line, area.width);
                y += 1;
            }
        }
    }
}

/// A node in a tree select.
#[derive(Debug, Clone)]
pub struct TreeNode {
    pub label: String,
    pub value: String,
    pub children: Vec<TreeNode>,
    pub expanded: bool,
}

impl TreeNode {
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            children: Vec::new(),
            expanded: false,
        }
    }

    pub fn with_children(mut self, children: Vec<TreeNode>) -> Self {
        self.children = children;
        self
    }

    pub fn toggle_expand(&mut self) {
        self.expanded = !self.expanded;
    }

    /// Flatten tree into visible items with depth.
    pub fn flatten(&self, depth: usize) -> Vec<(usize, &TreeNode)> {
        let mut result = vec![(depth, self)];
        if self.expanded {
            for child in &self.children {
                result.extend(child.flatten(depth + 1));
            }
        }
        result
    }
}

/// State for tree select.
#[derive(Debug, Clone)]
pub struct TreeSelectState {
    pub roots: Vec<TreeNode>,
    pub selected_index: usize,
}

impl TreeSelectState {
    pub fn new(roots: Vec<TreeNode>) -> Self {
        Self {
            roots,
            selected_index: 0,
        }
    }

    pub fn visible_items(&self) -> Vec<(usize, &TreeNode)> {
        let mut items = Vec::new();
        for root in &self.roots {
            items.extend(root.flatten(0));
        }
        items
    }

    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    pub fn move_down(&mut self) {
        let max = self.visible_items().len().saturating_sub(1);
        if self.selected_index < max {
            self.selected_index += 1;
        }
    }
}

/// Widget for tree select.
pub struct TreeSelectWidget<'a> {
    pub state: &'a TreeSelectState,
    pub theme: &'a Theme,
}

impl<'a> TreeSelectWidget<'a> {
    pub fn new(state: &'a TreeSelectState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for TreeSelectWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width < 5 {
            return;
        }
        let items = self.state.visible_items();
        for (vi, (depth, node)) in items.iter().enumerate() {
            let y = area.y + vi as u16;
            if y >= area.y + area.height {
                break;
            }
            let indent = (*depth as u16) * 2;
            let is_sel = vi == self.state.selected_index;
            let prefix = if is_sel { "❯ " } else { "  " };
            let expand_icon = if !node.children.is_empty() {
                if node.expanded { "▼ " } else { "▶ " }
            } else {
                "  "
            };
            let style = if is_sel {
                Style::default().fg(self.theme.primary).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let x = area.x + indent;
            buf.set_string(x, y, prefix, style);
            buf.set_string(x + 2, y, expand_icon, Style::default().fg(Color::DarkGray));
            let label_x = x + 4;
            let avail = area.width.saturating_sub(indent + 4) as usize;
            let label: String = node.label.chars().take(avail).collect();
            buf.set_string(label_x, y, &label, style);
        }
    }
}

// ===================================================================
// Wizard — from wizard/*.tsx
// ===================================================================

/// Wizard step definition.
#[derive(Debug, Clone)]
pub struct WizardStep {
    pub id: String,
    pub title: String,
    pub description: String,
}

/// State for wizard navigation.
#[derive(Debug, Clone)]
pub struct WizardState {
    pub steps: Vec<WizardStep>,
    pub current_step: usize,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub is_last_step: bool,
}

impl WizardState {
    pub fn new(steps: Vec<WizardStep>) -> Self {
        let is_last = steps.len() <= 1;
        Self {
            steps,
            current_step: 0,
            can_go_back: false,
            can_go_forward: !is_last,
            is_last_step: is_last,
        }
    }

    pub fn next(&mut self) {
        if self.current_step + 1 < self.steps.len() {
            self.current_step += 1;
            self.can_go_back = true;
            self.is_last_step = self.current_step + 1 >= self.steps.len();
            self.can_go_forward = !self.is_last_step;
        }
    }

    pub fn prev(&mut self) {
        if self.current_step > 0 {
            self.current_step -= 1;
            self.can_go_back = self.current_step > 0;
            self.is_last_step = false;
            self.can_go_forward = true;
        }
    }

    pub fn current(&self) -> Option<&WizardStep> {
        self.steps.get(self.current_step)
    }

    pub fn progress_label(&self) -> String {
        format!("{}/{}", self.current_step + 1, self.steps.len())
    }
}

/// Widget for wizard navigation footer.
pub struct WizardNavigationFooterWidget<'a> {
    pub state: &'a WizardState,
    pub theme: &'a Theme,
}

impl<'a> WizardNavigationFooterWidget<'a> {
    pub fn new(state: &'a WizardState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for WizardNavigationFooterWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width < 20 {
            return;
        }

        let mut spans = Vec::new();
        if self.state.can_go_back {
            spans.push(Span::styled("← Back", Style::default().fg(Color::DarkGray)));
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(
            self.state.progress_label(),
            Style::default().fg(Color::DarkGray),
        ));
        if self.state.can_go_forward {
            spans.push(Span::raw("  "));
            spans.push(Span::styled("Next →", Style::default().fg(self.theme.primary)));
        } else if self.state.is_last_step {
            spans.push(Span::raw("  "));
            spans.push(Span::styled("Done ✓", Style::default().fg(Color::Green)));
        }
        let line = Line::from(spans);
        buf.set_line(area.x, area.y, &line, area.width);
    }
}

/// Widget for wizard dialog layout.
pub struct WizardDialogLayoutWidget<'a> {
    pub state: &'a WizardState,
    pub theme: &'a Theme,
}

impl<'a> WizardDialogLayoutWidget<'a> {
    pub fn new(state: &'a WizardState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for WizardDialogLayoutWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if let Some(step) = self.state.current() {
            let dialog = DialogWidget::new(&step.title, self.theme).size(60, 16);
            let inner = dialog.inner_area(area);
            dialog.render(area, buf);

            if inner.width == 0 || inner.height == 0 {
                return;
            }

            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(1)])
                .split(inner);

            // Step content
            Paragraph::new(step.description.as_str())
                .wrap(Wrap { trim: false })
                .render(chunks[0], buf);

            // Footer
            WizardNavigationFooterWidget::new(self.state, self.theme).render(chunks[1], buf);
        }
    }
}

// ===================================================================
// Misc small components — DesktopUpsell, HelpV2, HighlightedCode, etc.
// ===================================================================

/// Desktop upsell state.
#[derive(Debug, Clone)]
pub struct DesktopUpsellState {
    pub visible: bool,
    pub message: String,
}

impl DesktopUpsellState {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            visible: true,
            message: message.into(),
        }
    }

    pub fn dismiss(&mut self) {
        self.visible = false;
    }
}

/// Help V2 sections.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpSection {
    Commands,
    Keybindings,
}

/// State for the help V2 panel.
#[derive(Debug, Clone)]
pub struct HelpV2State {
    pub section: HelpSection,
    pub commands: Vec<(String, String)>,
    pub keybindings: Vec<(String, String)>,
    pub selected_index: usize,
}

impl HelpV2State {
    pub fn new() -> Self {
        Self {
            section: HelpSection::Commands,
            commands: Vec::new(),
            keybindings: Vec::new(),
            selected_index: 0,
        }
    }

    pub fn switch_section(&mut self, section: HelpSection) {
        self.section = section;
        self.selected_index = 0;
    }

    pub fn current_items(&self) -> &[(String, String)] {
        match self.section {
            HelpSection::Commands => &self.commands,
            HelpSection::Keybindings => &self.keybindings,
        }
    }
}

impl Default for HelpV2State {
    fn default() -> Self {
        Self::new()
    }
}

/// Widget for Help V2 panel.
pub struct HelpV2Widget<'a> {
    pub state: &'a HelpV2State,
    pub theme: &'a Theme,
}

impl<'a> HelpV2Widget<'a> {
    pub fn new(state: &'a HelpV2State, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for HelpV2Widget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let dialog = DialogWidget::new("Help", self.theme).size(60, 20);
        let inner = dialog.inner_area(area);
        dialog.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .split(inner);

        // Section tabs
        let cmd_style = if self.state.section == HelpSection::Commands {
            Style::default().fg(self.theme.primary).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let kb_style = if self.state.section == HelpSection::Keybindings {
            Style::default().fg(self.theme.primary).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let tab_line = Line::from(vec![
            Span::styled(" Commands ", cmd_style),
            Span::raw(" | "),
            Span::styled(" Keybindings ", kb_style),
        ]);
        buf.set_line(chunks[0].x, chunks[0].y, &tab_line, chunks[0].width);

        // Items
        let items = self.state.current_items();
        for (i, (key, desc)) in items.iter().enumerate() {
            let y = chunks[1].y + i as u16;
            if y >= chunks[1].y + chunks[1].height {
                break;
            }
            let line = Line::from(vec![
                Span::styled(
                    format!("{:>12}", key),
                    Style::default().fg(self.theme.primary),
                ),
                Span::raw("  "),
                Span::styled(desc.as_str(), Style::default().fg(self.theme.text)),
            ]);
            buf.set_line(chunks[1].x, y, &line, chunks[1].width);
        }
    }
}

/// Passes state (usage/credit passes).
#[derive(Debug, Clone)]
pub struct PassesState {
    pub total: u32,
    pub used: u32,
    pub expires_at: Option<String>,
}

impl PassesState {
    pub fn new(total: u32, used: u32) -> Self {
        Self {
            total,
            used,
            expires_at: None,
        }
    }

    pub fn remaining(&self) -> u32 {
        self.total.saturating_sub(self.used)
    }

    pub fn usage_percentage(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            self.used as f64 / self.total as f64
        }
    }
}

/// Mossen hint state.
#[derive(Debug, Clone)]
pub struct MossenHintState {
    pub hint_text: String,
    pub visible: bool,
}

impl MossenHintState {
    pub fn new(hint: impl Into<String>) -> Self {
        Self {
            hint_text: hint.into(),
            visible: true,
        }
    }

    pub fn dismiss(&mut self) {
        self.visible = false;
    }
}

/// Widget for Mossen hint.
pub struct MossenHintWidget<'a> {
    pub state: &'a MossenHintState,
    pub theme: &'a Theme,
}

impl<'a> MossenHintWidget<'a> {
    pub fn new(state: &'a MossenHintState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for MossenHintWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.state.visible || area.height == 0 {
            return;
        }
        let line = Line::from(vec![
            Span::styled("💡 ", Style::default()),
            Span::styled(
                &self.state.hint_text,
                Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
            ),
        ]);
        Paragraph::new(line).render(area, buf);
    }
}

/// LSP recommendation state.
#[derive(Debug, Clone)]
pub struct LspRecommendationState {
    pub language: String,
    pub server_name: String,
    pub install_command: Option<String>,
    pub visible: bool,
}

impl LspRecommendationState {
    pub fn new(language: impl Into<String>, server: impl Into<String>) -> Self {
        Self {
            language: language.into(),
            server_name: server.into(),
            install_command: None,
            visible: true,
        }
    }

    pub fn dismiss(&mut self) {
        self.visible = false;
    }
}

// ===================================================================
// Scroll keybinding handler — components/ScrollKeybindingHandler.tsx
// ===================================================================

/// Subset of input key state used by scroll/selection decisions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScrollKey {
    pub left_arrow: bool,
    pub right_arrow: bool,
    pub up_arrow: bool,
    pub down_arrow: bool,
    pub home: bool,
    pub end: bool,
    pub page_up: bool,
    pub page_down: bool,
    pub wheel_up: bool,
    pub wheel_down: bool,
    pub shift: bool,
    pub meta: bool,
    pub super_key: bool,
    pub ctrl: bool,
}

/// Direction of a focus extension move for selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusMove {
    Left,
    Right,
    Up,
    Down,
    LineStart,
    LineEnd,
}

/// Whether a keypress should clear active text selection.
/// Bare arrows clear; shift/meta/super modified nav keeps selection;
/// wheel events do not clear here (handled by scroll keybinding path).
pub fn should_clear_selection_on_key(key: ScrollKey) -> bool {
    if key.wheel_up || key.wheel_down {
        return false;
    }
    let is_nav = key.left_arrow
        || key.right_arrow
        || key.up_arrow
        || key.down_arrow
        || key.home
        || key.end
        || key.page_up
        || key.page_down;
    if is_nav && (key.shift || key.meta || key.super_key) {
        return false;
    }
    true
}

/// Map a keypress to a focus-extension move (keyboard text selection).
pub fn selection_focus_move_for_key(key: ScrollKey) -> Option<FocusMove> {
    if !key.shift || key.meta {
        return None;
    }
    if key.left_arrow {
        return Some(FocusMove::Left);
    }
    if key.right_arrow {
        return Some(FocusMove::Right);
    }
    if key.up_arrow {
        return Some(FocusMove::Up);
    }
    if key.down_arrow {
        return Some(FocusMove::Down);
    }
    if key.home {
        return Some(FocusMove::LineStart);
    }
    if key.end {
        return Some(FocusMove::LineEnd);
    }
    None
}

/// Wheel accel state for scroll velocity computation (per ScrollKeybindingHandler.tsx).
#[derive(Debug, Clone)]
pub struct WheelAccelState {
    pub time_ms: u64,
    pub mult: f64,
    pub dir: i8,
    pub xterm_js: bool,
    pub frac: f64,
    pub base: f64,
    pub pending_flip: bool,
    pub wheel_mode: bool,
    pub burst_count: u32,
}

const WHEEL_MODE_IDLE_DISENGAGE_MS: u64 = 1500;
const WHEEL_BOUNCE_GAP_MAX_MS: u64 = 80;
const WHEEL_BURST_MS: u64 = 5;
const WHEEL_DECAY_HALFLIFE_MS: f64 = 80.0;
const WHEEL_MODE_CAP: f64 = 30.0;
const WHEEL_MODE_STEP: f64 = 15.0;
const WHEEL_MODE_RAMP: f64 = 4.0;
const WHEEL_ACCEL_WINDOW_MS: u64 = 40;
const WHEEL_ACCEL_MAX: f64 = 6.0;
const WHEEL_ACCEL_STEP: f64 = 1.0;
const WHEEL_DECAY_GAP_MS: u64 = 80;
const WHEEL_DECAY_IDLE_MS: u64 = 250;
const WHEEL_DECAY_CAP_FAST: f64 = 8.0;
const WHEEL_DECAY_CAP_SLOW: f64 = 4.0;
const WHEEL_DECAY_STEP: f64 = 5.0;

/// Read MOSSEN_CODE_SCROLL_SPEED env, default 1, clamp (0,20].
pub fn read_scroll_speed_base() -> f64 {
    match std::env::var("MOSSEN_CODE_SCROLL_SPEED") {
        Ok(raw) => match raw.parse::<f64>() {
            Ok(n) if n > 0.0 => n.min(20.0),
            _ => 1.0,
        },
        Err(_) => 1.0,
    }
}

/// Build an initial wheel accel state.
pub fn init_wheel_accel(xterm_js: bool, base: f64) -> WheelAccelState {
    WheelAccelState {
        time_ms: 0,
        mult: base,
        dir: 0,
        xterm_js,
        frac: 0.0,
        base,
        pending_flip: false,
        wheel_mode: false,
        burst_count: 0,
    }
}

/// Compute number of rows to scroll for a wheel event.
pub fn compute_wheel_step(state: &mut WheelAccelState, dir: i8, now_ms: u64) -> i64 {
    if !state.xterm_js {
        if state.wheel_mode && now_ms.saturating_sub(state.time_ms) > WHEEL_MODE_IDLE_DISENGAGE_MS {
            state.wheel_mode = false;
            state.burst_count = 0;
            state.mult = state.base;
        }
        if state.pending_flip {
            state.pending_flip = false;
            if dir != state.dir
                || now_ms.saturating_sub(state.time_ms) > WHEEL_BOUNCE_GAP_MAX_MS
            {
                state.dir = dir;
                state.time_ms = now_ms;
                state.mult = state.base;
                return state.mult.floor() as i64;
            }
            state.wheel_mode = true;
        }
        let gap = now_ms.saturating_sub(state.time_ms);
        if dir != state.dir && state.dir != 0 {
            state.pending_flip = true;
            state.time_ms = now_ms;
            return 0;
        }
        state.dir = dir;
        state.time_ms = now_ms;

        if state.wheel_mode {
            if gap < WHEEL_BURST_MS {
                state.burst_count += 1;
                if state.burst_count >= 5 {
                    state.wheel_mode = false;
                    state.burst_count = 0;
                    state.mult = state.base;
                } else {
                    return 1;
                }
            } else {
                state.burst_count = 0;
            }
        }
        if state.wheel_mode {
            let m = 0.5f64.powf(gap as f64 / WHEEL_DECAY_HALFLIFE_MS);
            let cap = WHEEL_MODE_CAP.max(state.base * 2.0);
            let next = 1.0 + (state.mult - 1.0) * m + WHEEL_MODE_STEP * m;
            state.mult = cap.min(next).min(state.mult + WHEEL_MODE_RAMP);
            return state.mult.floor() as i64;
        }
        if gap > WHEEL_ACCEL_WINDOW_MS {
            state.mult = state.base;
        } else {
            let cap = WHEEL_ACCEL_MAX.max(state.base * 2.0);
            state.mult = cap.min(state.mult + WHEEL_ACCEL_STEP);
        }
        return state.mult.floor() as i64;
    }

    // xterm.js path
    let gap = now_ms.saturating_sub(state.time_ms);
    let same_dir = dir == state.dir;
    state.time_ms = now_ms;
    state.dir = dir;
    if same_dir && gap < WHEEL_BURST_MS {
        return 1;
    }
    if !same_dir || gap > WHEEL_DECAY_IDLE_MS {
        state.mult = 2.0;
        state.frac = 0.0;
    } else {
        let m = 0.5f64.powf(gap as f64 / WHEEL_DECAY_HALFLIFE_MS);
        let cap = if gap >= WHEEL_DECAY_GAP_MS {
            WHEEL_DECAY_CAP_SLOW
        } else {
            WHEEL_DECAY_CAP_FAST
        };
        state.mult = cap.min(1.0 + (state.mult - 1.0) * m + WHEEL_DECAY_STEP * m);
    }
    let total = state.mult + state.frac;
    let rows = total.floor();
    state.frac = total - rows;
    rows as i64
}

/// Modal pager actions for scrollable views.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalPagerAction {
    LineUp,
    LineDown,
    HalfPageUp,
    HalfPageDown,
    FullPageUp,
    FullPageDown,
    Top,
    Bottom,
}

/// Map a keystroke to a modal pager action (vi-style).
pub fn modal_pager_action(input: &str, key: ScrollKey) -> Option<ModalPagerAction> {
    if key.up_arrow && !key.shift && !key.meta {
        return Some(ModalPagerAction::LineUp);
    }
    if key.down_arrow && !key.shift && !key.meta {
        return Some(ModalPagerAction::LineDown);
    }
    if key.home {
        return Some(ModalPagerAction::Top);
    }
    if key.end {
        return Some(ModalPagerAction::Bottom);
    }
    if key.ctrl {
        match input {
            "u" => return Some(ModalPagerAction::HalfPageUp),
            "d" => return Some(ModalPagerAction::HalfPageDown),
            "b" => return Some(ModalPagerAction::FullPageUp),
            "f" => return Some(ModalPagerAction::FullPageDown),
            _ => {}
        }
    }
    match input {
        "k" => Some(ModalPagerAction::LineUp),
        "j" => Some(ModalPagerAction::LineDown),
        "g" => Some(ModalPagerAction::Top),
        "G" => Some(ModalPagerAction::Bottom),
        _ => None,
    }
}

/// Scroll box handle abstraction (subset of TS ScrollBoxHandle).
#[derive(Debug, Clone)]
pub struct ScrollBoxHandle {
    pub offset: i64,
    pub viewport_rows: i64,
    pub content_rows: i64,
}

impl ScrollBoxHandle {
    pub fn new(viewport_rows: i64, content_rows: i64) -> Self {
        Self {
            offset: 0,
            viewport_rows,
            content_rows,
        }
    }

    pub fn max_offset(&self) -> i64 {
        (self.content_rows - self.viewport_rows).max(0)
    }
}

/// Jump offset by delta rows, clamped. Returns true if offset changed.
pub fn jump_by(s: &mut ScrollBoxHandle, delta: i64) -> bool {
    let prev = s.offset;
    s.offset = (s.offset + delta).clamp(0, s.max_offset());
    s.offset != prev
}

/// Scroll up by amount rows.
pub fn scroll_up(s: &mut ScrollBoxHandle, amount: i64) {
    jump_by(s, -amount);
}

/// Apply a modal pager action; returns Some(true) if action changed offset,
/// Some(false) if action recognised but offset unchanged, None if no action.
pub fn apply_modal_pager_action(
    s: &mut ScrollBoxHandle,
    act: Option<ModalPagerAction>,
    on_before_jump: &mut dyn FnMut(i64),
) -> Option<bool> {
    let act = act?;
    let delta = match act {
        ModalPagerAction::LineUp => -1,
        ModalPagerAction::LineDown => 1,
        ModalPagerAction::HalfPageUp => -(s.viewport_rows / 2).max(1),
        ModalPagerAction::HalfPageDown => (s.viewport_rows / 2).max(1),
        ModalPagerAction::FullPageUp => -s.viewport_rows.max(1),
        ModalPagerAction::FullPageDown => s.viewport_rows.max(1),
        ModalPagerAction::Top => -s.offset,
        ModalPagerAction::Bottom => s.max_offset() - s.offset,
    };
    on_before_jump(delta);
    Some(jump_by(s, delta))
}

/// Selection state used by drag-scroll direction logic.
#[derive(Debug, Clone, Copy)]
pub struct SelectionRange {
    pub anchor_row: i64,
    pub focus_row: i64,
}

/// Determine direction (-1/0/1) to drag-scroll when selection extends offscreen.
pub fn drag_scroll_direction(
    sel: Option<SelectionRange>,
    top: i64,
    bottom: i64,
    already_scrolling_dir: i8,
) -> i8 {
    let Some(s) = sel else {
        return 0;
    };
    let focus = s.focus_row;
    if focus < top {
        return -1;
    }
    if focus >= bottom {
        return 1;
    }
    already_scrolling_dir
}

/// ScrollKeybindingHandler state — owns the wheel accel + key handling.
#[derive(Debug, Clone)]
pub struct ScrollKeybindingHandler {
    pub accel: WheelAccelState,
    pub handle: ScrollBoxHandle,
    pub selection: Option<SelectionRange>,
}

impl ScrollKeybindingHandler {
    pub fn new(viewport_rows: i64, content_rows: i64, xterm_js: bool) -> Self {
        Self {
            accel: init_wheel_accel(xterm_js, read_scroll_speed_base()),
            handle: ScrollBoxHandle::new(viewport_rows, content_rows),
            selection: None,
        }
    }

    pub fn on_wheel(&mut self, dir: i8, now_ms: u64) -> i64 {
        let rows = compute_wheel_step(&mut self.accel, dir, now_ms);
        if rows > 0 {
            jump_by(&mut self.handle, rows * dir as i64);
        }
        rows
    }

    pub fn on_key(&mut self, input: &str, key: ScrollKey) -> bool {
        if should_clear_selection_on_key(key) {
            self.selection = None;
        }
        let action = modal_pager_action(input, key);
        apply_modal_pager_action(&mut self.handle, action, &mut |_| {}).unwrap_or(false)
    }
}

// ===================================================================
// FullscreenLayout — components/FullscreenLayout.tsx
// ===================================================================

/// Position of the unseen-message divider in the message list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnseenDivider {
    pub after_message_index: usize,
    pub unseen_count: usize,
}

/// Count consecutive unseen assistant turns at the tail of the messages list.
pub fn count_unseen_assistant_turns(role_seq: &[&str], last_seen_index: Option<usize>) -> usize {
    let start = last_seen_index.map(|i| i + 1).unwrap_or(0);
    role_seq
        .iter()
        .skip(start)
        .filter(|r| **r == "assistant")
        .count()
}

/// Compute the unseen divider given the role sequence and last-seen index.
pub fn compute_unseen_divider(
    role_seq: &[&str],
    last_seen_index: Option<usize>,
) -> Option<UnseenDivider> {
    let count = count_unseen_assistant_turns(role_seq, last_seen_index);
    if count == 0 {
        return None;
    }
    let after = last_seen_index.unwrap_or(0);
    Some(UnseenDivider {
        after_message_index: after,
        unseen_count: count,
    })
}

/// FullscreenLayout state machine. Tracks unseen divider + scroll chrome.
#[derive(Debug, Clone, Default)]
pub struct FullscreenLayoutState {
    pub last_seen_index: Option<usize>,
    pub divider: Option<UnseenDivider>,
    pub scroll_chrome_visible: bool,
}

impl FullscreenLayoutState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn refresh(&mut self, role_seq: &[&str]) {
        self.divider = compute_unseen_divider(role_seq, self.last_seen_index);
    }

    pub fn mark_seen(&mut self, idx: usize) {
        self.last_seen_index = Some(idx);
        self.divider = None;
    }
}

/// Hook-equivalent: track unseen divider derived from messages.
pub fn use_unseen_divider(
    state: &mut FullscreenLayoutState,
    role_seq: &[&str],
) -> Option<UnseenDivider> {
    state.refresh(role_seq);
    state.divider
}

/// Alias: FullscreenLayout component handle.
pub type FullscreenLayout = FullscreenLayoutState;

/// Render-equivalent for FullscreenLayout — paints a header divider if any.
pub fn fullscreen_layout_render(
    f: &mut ratatui::Frame<'_>,
    area: Rect,
    state: &FullscreenLayoutState,
    theme: &Theme,
) {
    if let Some(div) = state.divider {
        let label = format!(" {} new assistant message(s) ", div.unseen_count);
        let para = Paragraph::new(Line::from(Span::styled(
            label,
            Style::default().fg(theme.primary).add_modifier(Modifier::BOLD),
        )));
        f.render_widget(para, area);
    }
}

/// Context-equivalent for ScrollChromeContext. Toggles scroll bar/chrome.
#[derive(Debug, Clone, Default)]
pub struct ScrollChromeContext {
    pub visible: bool,
    pub auto_hide_ms: u64,
    pub last_activity_ms: u64,
}

impl ScrollChromeContext {
    pub fn new(auto_hide_ms: u64) -> Self {
        Self {
            visible: false,
            auto_hide_ms,
            last_activity_ms: 0,
        }
    }
    pub fn show(&mut self, now_ms: u64) {
        self.visible = true;
        self.last_activity_ms = now_ms;
    }
    pub fn tick(&mut self, now_ms: u64) {
        if self.visible && now_ms.saturating_sub(self.last_activity_ms) > self.auto_hide_ms {
            self.visible = false;
        }
    }
}

// ===================================================================
// StatusLine — components/StatusLine.tsx
// ===================================================================

/// Whether the status line should be rendered.
pub fn status_line_should_display(
    has_messages: bool,
    is_loading: bool,
    has_error: bool,
) -> bool {
    has_messages || is_loading || has_error
}

/// Find id of the last assistant message (helper for status line updates).
pub fn get_last_assistant_message_id<'a>(
    messages: &'a [(String, String)], // (id, role)
) -> Option<&'a str> {
    messages
        .iter()
        .rev()
        .find(|(_, role)| role == "assistant")
        .map(|(id, _)| id.as_str())
}

/// Stable cache key for the status line update so React doesn't thrash.
pub fn get_status_line_update_key(
    last_assistant_id: Option<&str>,
    cost_cents: u64,
    tokens: u64,
) -> String {
    format!(
        "{}::{}::{}",
        last_assistant_id.unwrap_or("none"),
        cost_cents,
        tokens
    )
}

// ===================================================================
// Fallback tool-use messages — minor fallback widgets
// ===================================================================

/// Fallback view for a rejected tool use (no specific renderer found).
#[derive(Debug, Clone)]
pub struct FallbackToolUseRejectedMessage {
    pub tool_name: String,
    pub reason: Option<String>,
}

impl FallbackToolUseRejectedMessage {
    pub fn new(tool_name: impl Into<String>) -> Self {
        Self {
            tool_name: tool_name.into(),
            reason: None,
        }
    }
    pub fn lines(&self, theme: &Theme) -> Vec<Line<'static>> {
        let mut out = vec![Line::from(Span::styled(
            format!("✗ {} rejected", self.tool_name),
            Style::default().fg(theme.error),
        ))];
        if let Some(r) = &self.reason {
            out.push(Line::from(Span::styled(
                format!("  reason: {}", r),
                Style::default().fg(theme.text_dim),
            )));
        }
        out
    }
}

/// Fallback view for tool use errors.
#[derive(Debug, Clone)]
pub struct FallbackToolUseErrorMessage {
    pub tool_name: String,
    pub error: String,
}

impl FallbackToolUseErrorMessage {
    pub fn new(tool_name: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            tool_name: tool_name.into(),
            error: error.into(),
        }
    }
    pub fn lines(&self, theme: &Theme) -> Vec<Line<'static>> {
        vec![
            Line::from(Span::styled(
                format!("⚠ {} error", self.tool_name),
                Style::default().fg(theme.error),
            )),
            Line::from(Span::styled(
                format!("  {}", self.error),
                Style::default().fg(theme.text_dim),
            )),
        ]
    }
}

/// FileEdit rejected fallback.
#[derive(Debug, Clone)]
pub struct FileEditToolUseRejectedMessage {
    pub path: String,
}

impl FileEditToolUseRejectedMessage {
    pub fn new(path: impl Into<String>) -> Self {
        Self { path: path.into() }
    }
    pub fn line(&self, theme: &Theme) -> Line<'static> {
        Line::from(Span::styled(
            format!("✗ edit rejected: {}", self.path),
            Style::default().fg(theme.error),
        ))
    }
}

/// Notebook-edit rejected fallback.
#[derive(Debug, Clone)]
pub struct NotebookEditToolUseRejectedMessage {
    pub notebook: String,
}

impl NotebookEditToolUseRejectedMessage {
    pub fn new(notebook: impl Into<String>) -> Self {
        Self {
            notebook: notebook.into(),
        }
    }
    pub fn line(&self, theme: &Theme) -> Line<'static> {
        Line::from(Span::styled(
            format!("✗ notebook edit rejected: {}", self.notebook),
            Style::default().fg(theme.error),
        ))
    }
}

// ===================================================================
// MessageResponse / TagTabs / SandboxViolationExpandedView etc.
// ===================================================================

/// A canned response surfaced as a chip.
#[derive(Debug, Clone)]
pub struct MessageResponse {
    pub label: String,
    pub value: String,
    pub selected: bool,
}

impl MessageResponse {
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            selected: false,
        }
    }
}

/// Tag tabs (chips) state.
#[derive(Debug, Clone, Default)]
pub struct TagTabs {
    pub tags: Vec<String>,
    pub selected_index: usize,
}

impl TagTabs {
    pub fn new(tags: Vec<String>) -> Self {
        Self {
            tags,
            selected_index: 0,
        }
    }
    pub fn next(&mut self) {
        if !self.tags.is_empty() {
            self.selected_index = (self.selected_index + 1) % self.tags.len();
        }
    }
    pub fn prev(&mut self) {
        if !self.tags.is_empty() {
            self.selected_index =
                (self.selected_index + self.tags.len() - 1) % self.tags.len();
        }
    }
    pub fn current(&self) -> Option<&str> {
        self.tags.get(self.selected_index).map(|s| s.as_str())
    }
}

/// Sandbox violation expanded view.
#[derive(Debug, Clone)]
pub struct SandboxViolationExpandedView {
    pub violations: Vec<String>,
    pub is_expanded: bool,
}

impl SandboxViolationExpandedView {
    pub fn new(violations: Vec<String>) -> Self {
        Self {
            violations,
            is_expanded: false,
        }
    }
    pub fn toggle(&mut self) {
        self.is_expanded = !self.is_expanded;
    }
}

// ===================================================================
// PackageManagerAutoUpdater — package manager helpers
// ===================================================================

/// Detected package manager kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageManagerKind {
    Npm,
    Pnpm,
    Yarn,
    Bun,
    Brew,
    None,
}

/// Get the update prefix (e.g. "npm install -g").
pub fn get_package_manager_update_prefix(pm: PackageManagerKind) -> &'static str {
    match pm {
        PackageManagerKind::Npm => "npm install -g",
        PackageManagerKind::Pnpm => "pnpm add -g",
        PackageManagerKind::Yarn => "yarn global add",
        PackageManagerKind::Bun => "bun add -g",
        PackageManagerKind::Brew => "brew upgrade",
        PackageManagerKind::None => "",
    }
}

/// Full update command for a package and pm.
pub fn get_package_manager_update_command(pm: PackageManagerKind, pkg: &str) -> String {
    let prefix = get_package_manager_update_prefix(pm);
    if prefix.is_empty() {
        String::new()
    } else {
        format!("{} {}", prefix, pkg)
    }
}

/// PackageManagerAutoUpdater state.
#[derive(Debug, Clone)]
pub struct PackageManagerAutoUpdater {
    pub pm: PackageManagerKind,
    pub package: String,
    pub current_version: String,
    pub latest_version: Option<String>,
    pub updating: bool,
}

impl PackageManagerAutoUpdater {
    pub fn new(pm: PackageManagerKind, package: impl Into<String>, current: impl Into<String>) -> Self {
        Self {
            pm,
            package: package.into(),
            current_version: current.into(),
            latest_version: None,
            updating: false,
        }
    }
    pub fn needs_update(&self) -> bool {
        self.latest_version
            .as_deref()
            .map(|v| v != self.current_version)
            .unwrap_or(false)
    }
    pub fn update_command(&self) -> String {
        get_package_manager_update_command(self.pm, &self.package)
    }
}

// ===================================================================
// CompactSummary
// ===================================================================

/// State for the compact summary message.
#[derive(Debug, Clone)]
pub struct CompactSummary {
    pub turns_compacted: usize,
    pub tokens_before: u64,
    pub tokens_after: u64,
    pub trigger: String,
}

impl CompactSummary {
    pub fn new(trigger: impl Into<String>) -> Self {
        Self {
            turns_compacted: 0,
            tokens_before: 0,
            tokens_after: 0,
            trigger: trigger.into(),
        }
    }
    pub fn savings_pct(&self) -> u64 {
        if self.tokens_before == 0 {
            0
        } else {
            (100 * (self.tokens_before - self.tokens_after)) / self.tokens_before
        }
    }
}

// ===================================================================
// TextInput / Markdown / Streaming markdown
// ===================================================================

/// TextInput Props.
#[derive(Debug, Clone, Default)]
pub struct TextInputProps {
    pub value: String,
    pub placeholder: String,
    pub disabled: bool,
    pub mask: Option<char>,
    pub width: u16,
}

/// Markdown widget (line-broken render).
#[derive(Debug, Clone)]
pub struct Markdown {
    pub source: String,
    pub width: u16,
}

impl Markdown {
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            width: 80,
        }
    }
    pub fn lines(&self) -> Vec<String> {
        self.source.lines().map(|l| l.to_string()).collect()
    }
}

/// Streaming markdown — accumulates partial text.
#[derive(Debug, Clone, Default)]
pub struct StreamingMarkdown {
    pub buffer: String,
    pub is_complete: bool,
}

impl StreamingMarkdown {
    pub fn append(&mut self, chunk: &str) {
        self.buffer.push_str(chunk);
    }
    pub fn finish(&mut self) {
        self.is_complete = true;
    }
}

// ===================================================================
// BashModeProgress / ApproveApiKey / ShowInIDEPrompt etc.
// ===================================================================

#[derive(Debug, Clone, Default)]
pub struct BashModeProgress {
    pub command: String,
    pub bytes_out: u64,
    pub elapsed_ms: u64,
}

impl BashModeProgress {
    pub fn new(cmd: impl Into<String>) -> Self {
        Self {
            command: cmd.into(),
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone)]
pub struct ApproveApiKey {
    pub key_preview: String,
    pub source: String,
    pub approved: bool,
}

impl ApproveApiKey {
    pub fn new(preview: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            key_preview: preview.into(),
            source: source.into(),
            approved: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ShowInIDEPrompt {
    pub path: String,
    pub line: Option<u32>,
}

impl ShowInIDEPrompt {
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            line: None,
        }
    }
}

// ===================================================================
// Output style / language pickers / status notices / interrupts
// ===================================================================

#[derive(Debug, Clone, Default)]
pub struct OutputStylePickerProps {
    pub current: String,
    pub available: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct OutputStylePicker {
    pub props: OutputStylePickerProps,
    pub selected_index: usize,
}

impl OutputStylePicker {
    pub fn new(props: OutputStylePickerProps) -> Self {
        Self {
            props,
            selected_index: 0,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct LanguagePicker {
    pub languages: Vec<String>,
    pub selected_index: usize,
}

impl LanguagePicker {
    pub fn new(languages: Vec<String>) -> Self {
        Self {
            languages,
            selected_index: 0,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct StatusNotices {
    pub notices: Vec<String>,
}

impl StatusNotices {
    pub fn push(&mut self, n: impl Into<String>) {
        self.notices.push(n.into());
    }
}

#[derive(Debug, Clone)]
pub struct InterruptedByUser {
    pub at_ms: u64,
}

impl InterruptedByUser {
    pub fn new(at_ms: u64) -> Self {
        Self { at_ms }
    }
}

// ===================================================================
// FileEditToolDiff / FileEditToolUpdatedMessage
// ===================================================================

#[derive(Debug, Clone)]
pub struct FileEditToolDiff {
    pub path: String,
    pub old_text: String,
    pub new_text: String,
}

impl FileEditToolDiff {
    pub fn new(path: impl Into<String>, old: impl Into<String>, new: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            old_text: old.into(),
            new_text: new.into(),
        }
    }
    pub fn changed_lines(&self) -> usize {
        let a = self.old_text.lines().count();
        let b = self.new_text.lines().count();
        a.max(b)
    }
}

#[derive(Debug, Clone)]
pub struct FileEditToolUpdatedMessage {
    pub path: String,
    pub lines_added: usize,
    pub lines_removed: usize,
}

impl FileEditToolUpdatedMessage {
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            lines_added: 0,
            lines_removed: 0,
        }
    }
}

// ===================================================================
// DesktopHandoff / AwsAuthStatusBox / ClickableImageRef / etc.
// ===================================================================

pub fn get_download_url(platform: &str, arch: &str) -> String {
    format!(
        "https://mossen.dev/download/{}-{}.tar.gz",
        platform.to_lowercase(),
        arch.to_lowercase()
    )
}

#[derive(Debug, Clone)]
pub struct DesktopHandoff {
    pub session_id: String,
    pub url: String,
}

impl DesktopHandoff {
    pub fn new(session_id: impl Into<String>) -> Self {
        let session_id = session_id.into();
        Self {
            url: format!("mossen://handoff/{}", session_id),
            session_id,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct AwsAuthStatusBox {
    pub profile: String,
    pub region: String,
    pub authenticated: bool,
}

#[derive(Debug, Clone)]
pub struct ClickableImageRef {
    pub url: String,
    pub alt: String,
}

impl ClickableImageRef {
    pub fn new(url: impl Into<String>, alt: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            alt: alt.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PrBadge {
    pub number: u32,
    pub state: String,
}

impl PrBadge {
    pub fn new(number: u32, state: impl Into<String>) -> Self {
        Self {
            number,
            state: state.into(),
        }
    }
}

/// HighlightedCode element (passthrough — actual highlighting in utils).
#[derive(Debug, Clone)]
pub struct HighlightedCode {
    pub code: String,
    pub language: String,
}

impl HighlightedCode {
    pub fn new(code: impl Into<String>, language: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            language: language.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TeleportStash {
    pub stash_id: String,
    pub label: String,
}

impl TeleportStash {
    pub fn new(stash_id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            stash_id: stash_id.into(),
            label: label.into(),
        }
    }
}

/// ExitFlow phases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitFlowPhase {
    Confirming,
    Persisting,
    Done,
}

#[derive(Debug, Clone)]
pub struct ExitFlow {
    pub phase: ExitFlowPhase,
    pub reason: Option<String>,
}

impl ExitFlow {
    pub fn new() -> Self {
        Self {
            phase: ExitFlowPhase::Confirming,
            reason: None,
        }
    }
    pub fn advance(&mut self) {
        self.phase = match self.phase {
            ExitFlowPhase::Confirming => ExitFlowPhase::Persisting,
            ExitFlowPhase::Persisting => ExitFlowPhase::Done,
            ExitFlowPhase::Done => ExitFlowPhase::Done,
        };
    }
}

impl Default for ExitFlow {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Default)]
pub struct StructuredDiffList {
    pub files: Vec<String>,
    pub selected_index: usize,
}

impl StructuredDiffList {
    pub fn new(files: Vec<String>) -> Self {
        Self {
            files,
            selected_index: 0,
        }
    }
}

/// Sentry-style error boundary (state container).
#[derive(Debug, Clone, Default)]
pub struct SentryErrorBoundary {
    pub error: Option<String>,
    pub captured_at_ms: u64,
}

impl SentryErrorBoundary {
    pub fn capture(&mut self, e: impl Into<String>, at_ms: u64) {
        self.error = Some(e.into());
        self.captured_at_ms = at_ms;
    }
}

/// Toggle for thinking content visibility.
#[derive(Debug, Clone, Default)]
pub struct ThinkingToggle {
    pub expanded: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ThinkingToggleProps {
    pub initial_expanded: bool,
}

impl ThinkingToggle {
    pub fn new(props: ThinkingToggleProps) -> Self {
        Self {
            expanded: props.initial_expanded,
        }
    }
    pub fn toggle(&mut self) {
        self.expanded = !self.expanded;
    }
}

#[derive(Debug, Clone, Default)]
pub struct MemoryUsageIndicator {
    pub rss_bytes: u64,
    pub heap_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct TeleportResumeWrapper {
    pub session_id: String,
    pub status: String,
}

impl TeleportResumeWrapper {
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            status: "pending".to_string(),
        }
    }
}

// ===================================================================
// Coordinator agent status
// ===================================================================

/// Visible agent task (subset of full task struct).
#[derive(Debug, Clone)]
pub struct VisibleAgentTask {
    pub id: String,
    pub label: String,
    pub status: String,
    pub progress_percent: u32,
}

/// Filter coordinator tasks down to those visible (running or recently finished).
pub fn get_visible_agent_tasks(tasks: &[VisibleAgentTask]) -> Vec<VisibleAgentTask> {
    tasks
        .iter()
        .filter(|t| t.status != "hidden" && t.status != "deleted")
        .cloned()
        .collect()
}

/// Coordinator panel state.
#[derive(Debug, Clone, Default)]
pub struct CoordinatorTaskPanel {
    pub tasks: Vec<VisibleAgentTask>,
    pub selected_index: usize,
}

impl CoordinatorTaskPanel {
    pub fn count(&self) -> usize {
        get_visible_agent_tasks(&self.tasks).len()
    }
}

/// Hook-equivalent: count visible coordinator tasks.
pub fn use_coordinator_task_count(panel: &CoordinatorTaskPanel) -> usize {
    panel.count()
}

// ===================================================================
// ModelPicker / CtrlOToExpand / SearchBox / FilePathLink / LogSelector
// ===================================================================

/// Default header text for the model picker, based on current model name.
pub fn get_default_model_picker_header_text(current_model: &str) -> String {
    if current_model.is_empty() {
        "Select a model".to_string()
    } else {
        format!("Switch from {}", current_model)
    }
}

#[derive(Debug, Clone, Default)]
pub struct ModelPickerProps {
    pub current_model: String,
    pub available: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ModelPicker {
    pub props: ModelPickerProps,
    pub selected_index: usize,
}

impl ModelPicker {
    pub fn new(props: ModelPickerProps) -> Self {
        Self {
            props,
            selected_index: 0,
        }
    }
    pub fn header(&self) -> String {
        get_default_model_picker_header_text(&self.props.current_model)
    }
}

/// SubAgent provider state (parent context for sub-agent UI).
#[derive(Debug, Clone, Default)]
pub struct SubAgentProvider {
    pub agents: Vec<String>,
    pub active_index: Option<usize>,
}

/// Ctrl-O ("Outline") expand toggle.
#[derive(Debug, Clone, Default)]
pub struct CtrlOToExpand {
    pub expanded: bool,
    pub hint_visible: bool,
}

/// Trigger the Ctrl-O action (toggle expansion).
pub fn ctrl_o_to_expand(state: &mut CtrlOToExpand) -> bool {
    state.expanded = !state.expanded;
    state.expanded
}

#[derive(Debug, Clone, Default)]
pub struct SearchBox {
    pub query: String,
    pub focused: bool,
}

impl SearchBox {
    pub fn set_query(&mut self, q: impl Into<String>) {
        self.query = q.into();
    }
}

#[derive(Debug, Clone)]
pub struct FilePathLink {
    pub path: String,
    pub display: String,
}

impl FilePathLink {
    pub fn new(path: impl Into<String>) -> Self {
        let path = path.into();
        Self {
            display: path.clone(),
            path,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct LogSelectorProps {
    pub entries: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct LogSelector {
    pub props: LogSelectorProps,
    pub selected: usize,
}

impl LogSelector {
    pub fn new(props: LogSelectorProps) -> Self {
        Self { props, selected: 0 }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ContextSuggestions {
    pub suggestions: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct AutoUpdater {
    pub channel: String,
    pub available: bool,
    pub installing: bool,
}

#[derive(Debug, Clone, Default)]
pub struct VimTextInputProps {
    pub value: String,
    pub mode: String, // "normal" | "insert"
}

#[derive(Debug, Clone)]
pub struct AutoUpdaterWrapper {
    pub inner: AutoUpdater,
    pub min_check_interval_ms: u64,
}

impl AutoUpdaterWrapper {
    pub fn new() -> Self {
        Self {
            inner: AutoUpdater::default(),
            min_check_interval_ms: 60_000,
        }
    }
}

impl Default for AutoUpdaterWrapper {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Default)]
pub struct TaskListV2 {
    pub tasks: Vec<String>,
    pub selected_index: usize,
}

#[derive(Debug, Clone, Default)]
pub struct TeammateViewHeader {
    pub teammate: String,
    pub role: String,
}

#[derive(Debug, Clone, Default)]
pub struct BaseTextInput {
    pub value: String,
    pub cursor_offset: usize,
}

impl BaseTextInput {
    pub fn insert(&mut self, s: &str) {
        self.value.insert_str(self.cursor_offset, s);
        self.cursor_offset += s.len();
    }
}

#[derive(Debug, Clone)]
pub struct ResumeTask {
    pub task_id: String,
    pub status: String,
}

impl ResumeTask {
    pub fn new(task_id: impl Into<String>) -> Self {
        Self {
            task_id: task_id.into(),
            status: "pending".to_string(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct MossenInChromeOnboarding {
    pub step: usize,
    pub done: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ContextVisualization {
    pub tokens: u64,
    pub limit: u64,
}

impl ContextVisualization {
    pub fn percent(&self) -> u32 {
        if self.limit == 0 {
            0
        } else {
            ((100 * self.tokens) / self.limit) as u32
        }
    }
}

// ===================================================================
// MossenErrorBoundary — error handling & injection
// ===================================================================

/// React-style error boundary container.
#[derive(Debug, Clone, Default)]
pub struct MossenErrorBoundary {
    pub error: Option<String>,
    pub component: Option<String>,
}

impl MossenErrorBoundary {
    pub fn catch(&mut self, err: impl Into<String>, component: impl Into<String>) {
        self.error = Some(err.into());
        self.component = Some(component.into());
    }
    pub fn reset(&mut self) {
        self.error = None;
        self.component = None;
    }
}

/// Wrap a render closure with an error boundary container.
pub fn with_error_boundary<F: FnOnce() -> Result<(), String>>(
    boundary: &mut MossenErrorBoundary,
    component: &str,
    f: F,
) {
    if let Err(e) = f() {
        boundary.catch(e, component);
    }
}

/// Whether to inject a thrown error (test/debug helper).
pub fn should_inject_throw(env_var: &str) -> bool {
    std::env::var(env_var).map(|v| v == "1" || v.eq_ignore_ascii_case("true")).unwrap_or(false)
}

/// Component that throws when invoked — used in test paths.
#[derive(Debug, Clone)]
pub struct InjectionThrower {
    pub message: String,
}

impl InjectionThrower {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
    pub fn throw(self) -> ! {
        panic!("InjectionThrower: {}", self.message);
    }
}

// ===================================================================
// Various small dialogs
// ===================================================================

#[derive(Debug, Clone, Default)]
pub struct MarkdownTable {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Default)]
pub struct Stats {
    pub turns: u64,
    pub tokens: u64,
    pub cost_cents: u64,
}

#[derive(Debug, Clone, Default)]
pub struct SessionPreview {
    pub session_id: String,
    pub summary: String,
}

#[derive(Debug, Clone)]
pub struct TeleportProgress {
    pub label: String,
    pub progress: f32,
    pub eta_seconds: Option<u64>,
}

impl TeleportProgress {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            progress: 0.0,
            eta_seconds: None,
        }
    }
}

/// Wrap a future-equivalent with progress reporting (helper).
pub async fn teleport_with_progress<F, T>(
    state: &mut TeleportProgress,
    fut: F,
) -> T
where
    F: std::future::Future<Output = T>,
{
    state.progress = 0.0;
    let r = fut.await;
    state.progress = 1.0;
    r
}

#[derive(Debug, Clone, Default)]
pub struct Onboarding {
    pub step: usize,
    pub completed: bool,
}

/// StructuredDiff top-level component (delegates to row renderer).
#[derive(Debug, Clone)]
pub struct StructuredDiff {
    pub files: Vec<FileEditToolDiff>,
}

impl StructuredDiff {
    pub fn new(files: Vec<FileEditToolDiff>) -> Self {
        Self { files }
    }
}

#[derive(Debug, Clone)]
pub struct ConfigurableShortcutHint {
    pub binding_name: String,
    pub display: String,
}

impl ConfigurableShortcutHint {
    pub fn new(binding_name: impl Into<String>, display: impl Into<String>) -> Self {
        Self {
            binding_name: binding_name.into(),
            display: display.into(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PressEnterToContinue {
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct OffscreenFreeze {
    pub frozen: bool,
}

#[derive(Debug, Clone, Default)]
pub struct MessageTimestamp {
    pub epoch_ms: u64,
    pub format: String, // "relative" | "absolute"
}

#[derive(Debug, Clone, Default)]
pub struct MessageModel {
    pub model_name: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Default)]
pub struct NativeAutoUpdater {
    pub channel: String,
    pub current_version: String,
    pub installing: bool,
}

#[derive(Debug, Clone, Default)]
pub struct SessionBackgroundHint {
    pub session_id: String,
    pub visible: bool,
}

#[derive(Debug, Clone, Default)]
pub struct AgentProgressLine {
    pub agent: String,
    pub label: String,
    pub progress: f32,
}

// ===================================================================
// Feedback URL builders + state
// ===================================================================

pub fn get_feedback_issue_draft_base_url() -> &'static str {
    "https://github.com/anthropics/claude-code/issues/new"
}

pub fn get_feedback_submission_url() -> &'static str {
    "https://api.mossen.dev/feedback/submit"
}

/// Build a GitHub issue URL with title and body pre-populated.
pub fn create_git_hub_issue_url(title: &str, body: &str) -> String {
    let q = |s: &str| {
        s.replace('%', "%25")
            .replace(' ', "%20")
            .replace('\n', "%0A")
            .replace('&', "%26")
            .replace('#', "%23")
    };
    format!(
        "{}?title={}&body={}",
        get_feedback_issue_draft_base_url(),
        q(title),
        q(body)
    )
}

#[derive(Debug, Clone, Default)]
pub struct Feedback {
    pub draft: String,
    pub sent: bool,
}

impl Feedback {
    pub fn issue_url(&self) -> String {
        create_git_hub_issue_url("Feedback", &self.draft)
    }
}

#[derive(Debug, Clone, Default)]
pub struct IdeStatusIndicator {
    pub ide_name: String,
    pub connected: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ValidationErrorsList {
    pub errors: Vec<String>,
}

// ===================================================================
// Teleport errors
// ===================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeleportLocalErrorType {
    NetworkUnreachable,
    AuthExpired,
    StashCorrupt,
    SessionNotFound,
    PermissionDenied,
}

#[derive(Debug, Clone)]
pub struct TeleportError {
    pub kind: TeleportLocalErrorType,
    pub message: String,
}

impl TeleportError {
    pub fn new(kind: TeleportLocalErrorType, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

/// Collect teleport errors from a list of optional error sources.
pub fn get_teleport_errors(sources: &[Option<TeleportError>]) -> Vec<TeleportError> {
    sources.iter().filter_map(|e| e.clone()).collect()
}

// ===================================================================
// DevBar / EffortCallout / ToolUseLoader / TokenWarning / Spinner etc.
// ===================================================================

#[derive(Debug, Clone, Default)]
pub struct DevBar {
    pub flags: HashMap<String, String>,
    pub visible: bool,
}

#[derive(Debug, Clone, Default)]
pub struct EffortCallout {
    pub effort_label: String,
    pub turns: u64,
    pub tokens: u64,
}

/// Whether the effort callout should be shown for a given turn count.
pub fn should_show_effort_callout(turns: u64, threshold: u64) -> bool {
    turns >= threshold && turns % threshold == 0
}

#[derive(Debug, Clone, Default)]
pub struct ToolUseLoader {
    pub tool_name: String,
    pub started_at_ms: u64,
}

#[derive(Debug, Clone, Default)]
pub struct TokenWarning {
    pub used: u64,
    pub limit: u64,
}

impl TokenWarning {
    pub fn pct(&self) -> u32 {
        if self.limit == 0 {
            0
        } else {
            ((100 * self.used) / self.limit) as u32
        }
    }
    pub fn severity(&self) -> u8 {
        let p = self.pct();
        if p >= 90 {
            2
        } else if p >= 75 {
            1
        } else {
            0
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SpinnerWithVerb {
    pub verb: String,
    pub frame: u32,
}

#[derive(Debug, Clone, Default)]
pub struct BriefIdleStatus {
    pub last_active_ms: u64,
    pub now_ms: u64,
}

impl BriefIdleStatus {
    pub fn idle_seconds(&self) -> u64 {
        self.now_ms.saturating_sub(self.last_active_ms) / 1000
    }
}

#[derive(Debug, Clone, Default)]
pub struct Spinner {
    pub frames: Vec<String>,
    pub frame_index: usize,
}

impl Spinner {
    pub fn tick(&mut self) {
        if !self.frames.is_empty() {
            self.frame_index = (self.frame_index + 1) % self.frames.len();
        }
    }
    pub fn current(&self) -> &str {
        self.frames.get(self.frame_index).map(|s| s.as_str()).unwrap_or("")
    }
}

/// Handle used to imperatively jump to a row in VirtualMessageList.
#[derive(Debug, Clone, Default)]
pub struct JumpHandle {
    pub target_row: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct VirtualMessageList {
    pub rows: Vec<String>,
    pub viewport_rows: i64,
    pub offset: i64,
    pub jump: JumpHandle,
}

impl VirtualMessageList {
    pub fn jump_to(&mut self, row: i64) {
        self.jump.target_row = Some(row);
        self.offset = row;
    }
}

#[derive(Debug, Clone, Default)]
pub struct SkillImprovementSurvey {
    pub skill_id: String,
    pub rating: Option<u8>,
    pub comment: String,
}

#[derive(Debug, Clone, Default)]
pub struct DiagnosticsDisplay {
    pub items: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct KeybindingWarnings {
    pub conflicts: Vec<String>,
}

// ===================================================================
// Icons / Effort indicator
// ===================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FastIconKind {
    Sparkle,
    Bolt,
    Check,
    Cross,
    Question,
    Triangle,
}

/// Return the unicode string for a fast icon.
pub fn get_fast_icon_string(kind: FastIconKind) -> &'static str {
    match kind {
        FastIconKind::Sparkle => "✦",
        FastIconKind::Bolt => "⚡",
        FastIconKind::Check => "✓",
        FastIconKind::Cross => "✗",
        FastIconKind::Question => "?",
        FastIconKind::Triangle => "▲",
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FastIcon(pub FastIconKind);

impl FastIcon {
    pub fn glyph(&self) -> &'static str {
        get_fast_icon_string(self.0)
    }
}

/// Build a textual notification for the current effort level.
pub fn get_effort_notification_text(effort: &str, threshold_turns: u64) -> String {
    format!("{} effort threshold: {} turns", effort, threshold_turns)
}

// ===================================================================
// ThemePicker / ConsoleOAuthFlow / TreeSelect / OrderedList / etc.
// ===================================================================

#[derive(Debug, Clone, Default)]
pub struct ThemePickerProps {
    pub current: String,
    pub themes: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ThemePicker {
    pub props: ThemePickerProps,
    pub selected_index: usize,
}

#[derive(Debug, Clone, Default)]
pub struct ConsoleOAuthFlow {
    pub url: String,
    pub state_code: String,
    pub completed: bool,
}

#[derive(Debug, Clone, Default)]
pub struct TreeSelectProps {
    pub nodes: Vec<String>,
    pub indent_levels: Vec<u8>,
}

#[derive(Debug, Clone, Default)]
pub struct TreeSelect {
    pub props: TreeSelectProps,
    pub selected: usize,
    pub expanded: Vec<bool>,
}

impl TreeSelect {
    pub fn new(props: TreeSelectProps) -> Self {
        let n = props.nodes.len();
        Self {
            props,
            selected: 0,
            expanded: vec![true; n],
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct OrderedListItemContext {
    pub index: usize,
    pub marker: String,
}

#[derive(Debug, Clone, Default)]
pub struct OrderedList {
    pub items: Vec<String>,
    pub start: usize,
}

#[derive(Debug, Clone, Default)]
pub struct LspRecommendationMenu {
    pub recommendations: Vec<LspRecommendationState>,
    pub selected: usize,
}

#[derive(Debug, Clone, Default)]
pub struct PluginHintMenu {
    pub plugins: Vec<String>,
    pub selected: usize,
}

#[derive(Debug, Clone, Default)]
pub struct MemoryFileSelector {
    pub files: Vec<String>,
    pub selected: usize,
}

/// Resolve a memory file path relative to the project root.
pub fn get_relative_memory_path(path: &str, project_root: &str) -> String {
    if let Some(rest) = path.strip_prefix(project_root) {
        rest.trim_start_matches('/').to_string()
    } else {
        path.to_string()
    }
}

#[derive(Debug, Clone, Default)]
pub struct Passes {
    pub passes_remaining: u64,
    pub renew_at_ms: u64,
}

// ===================================================================
// Wizard infrastructure
// ===================================================================

#[derive(Debug, Clone, Default)]
pub struct WizardContext {
    pub steps: Vec<String>,
    pub index: usize,
    pub data: HashMap<String, String>,
}

#[derive(Debug, Clone, Default)]
pub struct WizardProvider {
    pub context: WizardContext,
}

impl WizardProvider {
    pub fn next(&mut self) {
        if self.context.index + 1 < self.context.steps.len() {
            self.context.index += 1;
        }
    }
    pub fn prev(&mut self) {
        if self.context.index > 0 {
            self.context.index -= 1;
        }
    }
}

/// Hook-equivalent returning a mutable handle to the wizard context.
pub fn use_wizard(provider: &mut WizardProvider) -> &mut WizardContext {
    &mut provider.context
}

#[derive(Debug, Clone, Default)]
pub struct WizardNavigationFooter {
    pub current: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Default)]
pub struct DiffFileList {
    pub files: Vec<String>,
    pub selected: usize,
}

#[derive(Debug, Clone, Default)]
pub struct DiffDetailView {
    pub file: String,
    pub diff_lines: Vec<String>,
}

// ===================================================================
// Desktop upsell
// ===================================================================

#[derive(Debug, Clone, Default)]
pub struct DesktopUpsellConfig {
    pub headline: String,
    pub bullets: Vec<String>,
    pub cta: String,
}

/// Build the desktop upsell config (static today).
pub fn get_desktop_upsell_config() -> DesktopUpsellConfig {
    DesktopUpsellConfig {
        headline: "Mossen Desktop".into(),
        bullets: vec![
            "Native window".into(),
            "Faster startup".into(),
            "Background sessions".into(),
        ],
        cta: "Download".into(),
    }
}

/// Whether the startup desktop upsell should be shown.
pub fn should_show_desktop_upsell_startup(
    session_count: u64,
    dismissed: bool,
    is_desktop: bool,
) -> bool {
    !is_desktop && !dismissed && session_count >= 3
}

/// Build copy variants (A/B test).
pub fn get_desktop_upsell_copy(variant: u8) -> (&'static str, &'static str) {
    match variant {
        0 => ("Try Mossen Desktop", "Get the native experience"),
        1 => ("New: Mossen Desktop", "Stay in flow — no terminal needed"),
        _ => ("Mossen Desktop", "Available now"),
    }
}

#[derive(Debug, Clone)]
pub struct DesktopUpsellStartup {
    pub config: DesktopUpsellConfig,
    pub variant: u8,
    pub shown: bool,
}

impl DesktopUpsellStartup {
    pub fn new(variant: u8) -> Self {
        Self {
            config: get_desktop_upsell_config(),
            variant,
            shown: false,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TeamStatus {
    pub team_name: String,
    pub members_online: u32,
}

#[derive(Debug, Clone, Default)]
pub struct SkillsMenu {
    pub skills: Vec<String>,
    pub selected: usize,
}

#[derive(Debug, Clone, Default)]
pub struct HighlightedCodeFallback {
    pub code: String,
}

/// Grove dialog with privacy settings.
#[derive(Debug, Clone, Default)]
pub struct GroveDialog {
    pub state: GroveDialogStateLite,
}

#[derive(Debug, Clone, Default)]
pub struct GroveDialogStateLite {
    pub accepted: bool,
    pub opted_in: bool,
}

#[derive(Debug, Clone, Default)]
pub struct PrivacySettingsDialog {
    pub training_opt_in: bool,
    pub error_reporting: bool,
}


// === TS Props aliases (per-component sub-modules to mirror TS `type Props`) ===
pub mod thinking_toggle_props_alias {
    pub type Props = super::ThinkingToggleProps;
}
pub mod model_picker_props_alias {
    pub type Props = super::ModelPickerProps;
}
pub mod vim_text_input_props_alias {
    pub type Props = super::VimTextInputProps;
}
pub mod text_input_props_alias {
    pub type Props = super::TextInputProps;
}
