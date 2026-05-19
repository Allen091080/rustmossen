//! Agent status panel components.
//!
//! Translates: components/agents/ (13 files + 1 subdirectory)
//! + AgentProgressLine.tsx + CoordinatorAgentStatus.tsx + BashModeProgress.tsx

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Widget, Wrap},
};

use crate::theme::Theme;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentStatus {
    Idle,
    Running,
    Waiting,
    Completed,
    Error,
}

impl AgentStatus {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Running => "Running",
            Self::Waiting => "Waiting",
            Self::Completed => "Completed",
            Self::Error => "Error",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            Self::Idle => "○",
            Self::Running => "●",
            Self::Waiting => "◌",
            Self::Completed => "✓",
            Self::Error => "✗",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AgentInfo {
    pub id: String,
    pub name: String,
    pub model: Option<String>,
    pub status: AgentStatus,
    pub color: Option<Color>,
    pub current_activity: Option<String>,
    pub tool_names: Vec<String>,
    pub message_count: usize,
}

impl AgentInfo {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            model: None,
            status: AgentStatus::Idle,
            color: None,
            current_activity: None,
            tool_names: Vec::new(),
            message_count: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// AgentsListWidget — list of agents (AgentsList.tsx)
// ---------------------------------------------------------------------------

pub struct AgentsListWidget<'a> {
    pub agents: &'a [AgentInfo],
    pub selected: usize,
    pub theme: &'a Theme,
}

impl<'a> AgentsListWidget<'a> {
    pub fn new(agents: &'a [AgentInfo], theme: &'a Theme) -> Self {
        Self {
            agents,
            selected: 0,
            theme,
        }
    }

    pub fn selected(mut self, idx: usize) -> Self {
        self.selected = idx;
        self
    }
}

impl<'a> Widget for AgentsListWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let items: Vec<ListItem> = self
            .agents
            .iter()
            .enumerate()
            .map(|(i, agent)| {
                let is_sel = i == self.selected;
                let color = agent.color.unwrap_or(self.theme.primary);
                let status_color = match agent.status {
                    AgentStatus::Running => self.theme.success,
                    AgentStatus::Error => self.theme.error,
                    AgentStatus::Waiting => self.theme.warning,
                    _ => self.theme.text_dim,
                };

                let style = if is_sel {
                    Style::default()
                        .fg(self.theme.text)
                        .bg(self.theme.selection)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(self.theme.text)
                };

                let line = Line::from(vec![
                    Span::styled(
                        format!("{} ", agent.status.icon()),
                        Style::default().fg(status_color),
                    ),
                    Span::styled("● ", Style::default().fg(color)),
                    Span::styled(&agent.name, style),
                    Span::styled(
                        agent
                            .current_activity
                            .as_deref()
                            .map(|a| format!("  {}", a))
                            .unwrap_or_default(),
                        Style::default().fg(self.theme.text_dim),
                    ),
                ]);
                ListItem::new(line)
            })
            .collect();

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.theme.style_border())
            .title(Span::styled(
                " Delegates ",
                Style::default()
                    .fg(self.theme.primary)
                    .add_modifier(Modifier::BOLD),
            ));

        List::new(items).block(block).render(area, buf);
    }
}

// ---------------------------------------------------------------------------
// AgentDetailWidget — agent detail view (AgentDetail.tsx)
// ---------------------------------------------------------------------------

pub struct AgentDetailWidget<'a> {
    pub agent: &'a AgentInfo,
    pub theme: &'a Theme,
}

impl<'a> AgentDetailWidget<'a> {
    pub fn new(agent: &'a AgentInfo, theme: &'a Theme) -> Self {
        Self { agent, theme }
    }
}

impl<'a> Widget for AgentDetailWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let color = self.agent.color.unwrap_or(self.theme.primary);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(color))
            .title(Span::styled(
                format!(" {} ", self.agent.name),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ));

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let mut lines = vec![
            Line::from(vec![
                Span::styled("Status: ", Style::default().fg(self.theme.text_dim)),
                Span::styled(
                    self.agent.status.label(),
                    Style::default().fg(match self.agent.status {
                        AgentStatus::Running => self.theme.success,
                        AgentStatus::Error => self.theme.error,
                        _ => self.theme.text,
                    }),
                ),
            ]),
            Line::from(vec![
                Span::styled("Model: ", Style::default().fg(self.theme.text_dim)),
                Span::styled(
                    self.agent.model.as_deref().unwrap_or("default"),
                    Style::default().fg(self.theme.text),
                ),
            ]),
            Line::from(vec![
                Span::styled("Messages: ", Style::default().fg(self.theme.text_dim)),
                Span::styled(
                    self.agent.message_count.to_string(),
                    Style::default().fg(self.theme.text),
                ),
            ]),
        ];

        if !self.agent.tool_names.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Instruments:",
                Style::default()
                    .fg(self.theme.text)
                    .add_modifier(Modifier::BOLD),
            )));
            for tool in &self.agent.tool_names {
                lines.push(Line::from(vec![
                    Span::styled("  • ", Style::default().fg(self.theme.text_dim)),
                    Span::styled(tool.as_str(), Style::default().fg(self.theme.text)),
                ]));
            }
        }

        Paragraph::new(lines).render(inner, buf);
    }
}

// ---------------------------------------------------------------------------
// AgentsMenuWidget — full agents management panel (AgentsMenu.tsx)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct AgentsMenuState {
    pub agents: Vec<AgentInfo>,
    pub selected: usize,
    pub show_detail: bool,
}

impl AgentsMenuState {
    pub fn new(agents: Vec<AgentInfo>) -> Self {
        Self {
            agents,
            selected: 0,
            show_detail: false,
        }
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected + 1 < self.agents.len() {
            self.selected += 1;
        }
    }

    pub fn selected_agent(&self) -> Option<&AgentInfo> {
        self.agents.get(self.selected)
    }
}

pub struct AgentsMenuWidget<'a> {
    pub state: &'a AgentsMenuState,
    pub theme: &'a Theme,
}

impl<'a> AgentsMenuWidget<'a> {
    pub fn new(state: &'a AgentsMenuState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for AgentsMenuWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 20 || area.height < 5 {
            return;
        }

        if self.state.show_detail {
            // Two-column: list | detail
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
                .split(area);

            let list_widget =
                AgentsListWidget::new(&self.state.agents, self.theme).selected(self.state.selected);
            list_widget.render(chunks[0], buf);

            if let Some(agent) = self.state.selected_agent() {
                let detail = AgentDetailWidget::new(agent, self.theme);
                detail.render(chunks[1], buf);
            }
        } else {
            let list_widget =
                AgentsListWidget::new(&self.state.agents, self.theme).selected(self.state.selected);
            list_widget.render(area, buf);
        }
    }
}

// ---------------------------------------------------------------------------
// AgentProgressLineWidget (AgentProgressLine.tsx)
// ---------------------------------------------------------------------------

pub struct AgentProgressLineWidget<'a> {
    pub agent_name: &'a str,
    pub activity: &'a str,
    pub color: Color,
    pub theme: &'a Theme,
}

impl<'a> AgentProgressLineWidget<'a> {
    pub fn new(agent_name: &'a str, activity: &'a str, theme: &'a Theme) -> Self {
        Self {
            agent_name,
            activity,
            color: theme.primary,
            theme,
        }
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }
}

impl<'a> Widget for AgentProgressLineWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let line = Line::from(vec![
            Span::styled("● ", Style::default().fg(self.color)),
            Span::styled(
                self.agent_name,
                Style::default()
                    .fg(self.theme.text)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" — ", Style::default().fg(self.theme.text_dim)),
            Span::styled(self.activity, Style::default().fg(self.theme.text_dim)),
        ]);
        buf.set_line(area.x, area.y, &line, area.width);
    }
}

// ---------------------------------------------------------------------------
// CoordinatorStatusWidget (CoordinatorAgentStatus.tsx)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct CoordinatorStatusState {
    pub agents: Vec<AgentInfo>,
    pub total_turns: usize,
    pub is_active: bool,
}

pub struct CoordinatorStatusWidget<'a> {
    pub state: &'a CoordinatorStatusState,
    pub theme: &'a Theme,
}

impl<'a> CoordinatorStatusWidget<'a> {
    pub fn new(state: &'a CoordinatorStatusState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for CoordinatorStatusWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let running = self
            .state
            .agents
            .iter()
            .filter(|a| a.status == AgentStatus::Running)
            .count();
        let completed = self
            .state
            .agents
            .iter()
            .filter(|a| a.status == AgentStatus::Completed)
            .count();

        let line = Line::from(vec![
            Span::styled(
                "Orchestrator",
                Style::default()
                    .fg(self.theme.secondary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    "  {} delegates ({} active, {} done)  {} turns",
                    self.state.agents.len(),
                    running,
                    completed,
                    self.state.total_turns
                ),
                Style::default().fg(self.theme.text_dim),
            ),
        ]);
        buf.set_line(area.x, area.y, &line, area.width);

        // Per-agent lines
        for (i, agent) in self.state.agents.iter().enumerate() {
            let y = area.y + 1 + i as u16;
            if y >= area.y + area.height {
                break;
            }
            let progress = AgentProgressLineWidget::new(
                &agent.name,
                agent.current_activity.as_deref().unwrap_or("idle"),
                self.theme,
            )
            .color(agent.color.unwrap_or(self.theme.primary));
            let line_area = Rect::new(area.x + 2, y, area.width.saturating_sub(2), 1);
            progress.render(line_area, buf);
        }
    }
}

// ---------------------------------------------------------------------------
// BashModeProgressWidget (BashModeProgress.tsx)
// ---------------------------------------------------------------------------

pub struct BashModeProgressWidget<'a> {
    pub command: &'a str,
    pub elapsed_ms: u64,
    pub theme: &'a Theme,
}

impl<'a> BashModeProgressWidget<'a> {
    pub fn new(command: &'a str, elapsed_ms: u64, theme: &'a Theme) -> Self {
        Self {
            command,
            elapsed_ms,
            theme,
        }
    }
}

impl<'a> Widget for BashModeProgressWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let elapsed = if self.elapsed_ms >= 1000 {
            format!("{:.1}s", self.elapsed_ms as f64 / 1000.0)
        } else {
            format!("{}ms", self.elapsed_ms)
        };

        let line = Line::from(vec![
            Span::styled("⏺ ", Style::default().fg(self.theme.spinner_primary)),
            Span::styled("$ ", Style::default().fg(self.theme.text_dim)),
            Span::styled(
                self.command,
                Style::default()
                    .fg(self.theme.text)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  ({})", elapsed),
                Style::default().fg(self.theme.text_dim),
            ),
        ]);
        buf.set_line(area.x, area.y, &line, area.width);
    }
}

// ===================================================================
// Agent menu / detail / selectors
// ===================================================================

#[derive(Debug, Clone, Default)]
pub struct AgentsMenu {
    pub agents: Vec<String>,
    pub selected: usize,
}

#[derive(Debug, Clone, Default)]
pub struct ToolSelector {
    pub available_tools: Vec<String>,
    pub selected: Vec<usize>,
    pub cursor: usize,
}

#[derive(Debug, Clone, Default)]
pub struct AgentDetail {
    pub name: String,
    pub description: String,
    pub model: String,
    pub tools: Vec<String>,
    pub color: String,
    pub prompt: String,
}

pub fn get_agent_source_display_name(source: &str) -> &'static str {
    match source {
        "bundled" => "Built-in",
        "user" => "User",
        "project" => "Project",
        "plugin" => "Plugin",
        _ => "Custom",
    }
}

#[derive(Debug, Clone, Default)]
pub struct ModelSelector {
    pub models: Vec<String>,
    pub current: String,
    pub cursor: usize,
}

// ===================================================================
// Agent types (agents/types.ts)
// ===================================================================

pub const AGENT_PATHS: &[&str] = &[
    "~/.mossen/agents",
    ".mossen/agents",
    "node_modules/.mossen-agents",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeState {
    Viewing,
    Editing,
    Creating,
    Confirming,
    Generating,
}

impl Default for ModeState {
    fn default() -> Self {
        Self::Viewing
    }
}

#[derive(Debug, Clone, Default)]
pub struct AgentValidationResult {
    pub ok: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct AgentNavigationFooter {
    pub hint_left: String,
    pub hint_right: String,
}

// ===================================================================
// validateAgent.ts
// ===================================================================

pub fn validate_agent_type(agent_type: &str) -> bool {
    matches!(
        agent_type,
        "single-turn" | "multi-turn" | "task" | "subagent" | "skill"
    )
}

pub fn validate_agent(
    name: &str,
    agent_type: &str,
    prompt: &str,
    tools: &[String],
) -> AgentValidationResult {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    if name.trim().is_empty() {
        errors.push("name is required".into());
    }
    if !validate_agent_type(agent_type) {
        errors.push(format!("unknown agent type: {}", agent_type));
    }
    if prompt.trim().is_empty() {
        warnings.push("prompt is empty".into());
    }
    if tools.is_empty() {
        warnings.push("no tools selected".into());
    }
    AgentValidationResult {
        ok: errors.is_empty(),
        errors,
        warnings,
    }
}

// ===================================================================
// agentFileUtils.ts
// ===================================================================

pub fn format_agent_as_markdown(
    name: &str,
    description: &str,
    model: &str,
    tools: &[String],
    color: &str,
    prompt: &str,
) -> String {
    let mut s = String::new();
    s.push_str("---\n");
    s.push_str(&format!("name: {}\n", name));
    s.push_str(&format!("description: {}\n", description));
    s.push_str(&format!("model: {}\n", model));
    s.push_str(&format!("color: {}\n", color));
    s.push_str(&format!("tools: [{}]\n", tools.join(", ")));
    s.push_str("---\n\n");
    s.push_str(prompt);
    if !prompt.ends_with('\n') {
        s.push('\n');
    }
    s
}

pub fn get_new_agent_file_path(scope: &str, name: &str, project_root: &str, home: &str) -> String {
    let base = match scope {
        "project" => format!("{}/.mossen/agents", project_root.trim_end_matches('/')),
        _ => format!("{}/.mossen/agents", home.trim_end_matches('/')),
    };
    format!("{}/{}.md", base, slugify_agent(name))
}

pub fn get_actual_agent_file_path(
    scope: &str,
    file_name: &str,
    project_root: &str,
    home: &str,
) -> String {
    let base = match scope {
        "project" => format!("{}/.mossen/agents", project_root.trim_end_matches('/')),
        _ => format!("{}/.mossen/agents", home.trim_end_matches('/')),
    };
    format!("{}/{}", base, file_name)
}

pub fn get_new_relative_agent_file_path(scope: &str, name: &str) -> String {
    match scope {
        "project" => format!(".mossen/agents/{}.md", slugify_agent(name)),
        _ => format!("~/.mossen/agents/{}.md", slugify_agent(name)),
    }
}

pub fn get_actual_relative_agent_file_path(scope: &str, file_name: &str) -> String {
    match scope {
        "project" => format!(".mossen/agents/{}", file_name),
        _ => format!("~/.mossen/agents/{}", file_name),
    }
}

pub fn save_agent_to_file(path: &str, markdown: &str) -> std::io::Result<()> {
    if let Some(parent) = std::path::Path::new(path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(path, markdown)
}

pub fn update_agent_file(path: &str, markdown: &str) -> std::io::Result<()> {
    save_agent_to_file(path, markdown)
}

pub fn delete_agent_from_file(path: &str) -> std::io::Result<()> {
    std::fs::remove_file(path)
}

fn slugify_agent(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

// ===================================================================
// generateAgent.ts
// ===================================================================

pub fn generate_agent(description: &str, model: &str) -> AgentDetail {
    AgentDetail {
        name: description
            .split_whitespace()
            .next()
            .unwrap_or("agent")
            .to_string(),
        description: description.to_string(),
        model: model.to_string(),
        tools: vec!["BashTool".into(), "FileReadTool".into(), "FileWriteTool".into()],
        color: "blue".into(),
        prompt: format!(
            "You are a specialised agent. Description:\n{}\n\nProceed step by step.",
            description
        ),
    }
}

// ===================================================================
// AgentEditor / AgentsList / ColorPicker
// ===================================================================

#[derive(Debug, Clone, Default)]
pub struct AgentEditor {
    pub agent: AgentDetail,
    pub focused_field: String,
    pub mode: ModeState,
}

#[derive(Debug, Clone, Default)]
pub struct AgentsList {
    pub agents: Vec<AgentDetail>,
    pub selected: usize,
}

#[derive(Debug, Clone, Default)]
pub struct ColorPicker {
    pub colors: Vec<&'static str>,
    pub selected: usize,
}

// ===================================================================
// Create-agent wizard
// ===================================================================

#[derive(Debug, Clone, Default)]
pub struct CreateAgentWizard {
    pub step: usize,
    pub agent: AgentDetail,
    pub scope: String,
}

#[derive(Debug, Clone, Default)]
pub struct MemoryStep {
    pub memory_text: String,
}

#[derive(Debug, Clone, Default)]
pub struct ConfirmStep {
    pub confirmed: bool,
}

#[derive(Debug, Clone, Default)]
pub struct TypeStep {
    pub agent_type: String,
}

#[derive(Debug, Clone, Default)]
pub struct ToolsStep {
    pub selected_tools: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ColorStep {
    pub color: String,
}

#[derive(Debug, Clone, Default)]
pub struct LocationStep {
    pub scope: String,
}

#[derive(Debug, Clone, Default)]
pub struct MethodStep {
    pub method: String,
}

#[derive(Debug, Clone, Default)]
pub struct ConfirmStepWrapper {
    pub inner: ConfirmStep,
}

#[derive(Debug, Clone, Default)]
pub struct PromptStep {
    pub prompt: String,
}

#[derive(Debug, Clone, Default)]
pub struct DescriptionStep {
    pub description: String,
}

#[derive(Debug, Clone, Default)]
pub struct ModelStep {
    pub model: String,
}

#[derive(Debug, Clone, Default)]
pub struct GenerateStep {
    pub prompt: String,
    pub generated: Option<AgentDetail>,
    pub generating: bool,
}
