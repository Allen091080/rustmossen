//! Task panel components.
//!
//! Translates: components/tasks/ (10 files) + TaskListV2.tsx

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Widget, Wrap},
};

use crate::theme::Theme;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Cancelled,
}

impl TaskStatus {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Pending => "Pending",
            Self::InProgress => "In Progress",
            Self::Completed => "Completed",
            Self::Failed => "Failed",
            Self::Cancelled => "Cancelled",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            Self::Pending => "○",
            Self::InProgress => "◐",
            Self::Completed => "✓",
            Self::Failed => "✗",
            Self::Cancelled => "⊘",
        }
    }
}

#[derive(Debug, Clone)]
pub struct TaskInfo {
    pub id: String,
    pub subject: String,
    pub status: TaskStatus,
    pub agent_name: Option<String>,
    pub progress: Option<f64>,
    pub active_form: Option<String>,
    pub blocked_by: Vec<String>,
    pub blocks: Vec<String>,
}

impl TaskInfo {
    pub fn new(id: impl Into<String>, subject: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            subject: subject.into(),
            status: TaskStatus::Pending,
            agent_name: None,
            progress: None,
            active_form: None,
            blocked_by: Vec::new(),
            blocks: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// TaskListWidget — task list panel (BackgroundTasksDialog.tsx)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct TaskListState {
    pub tasks: Vec<TaskInfo>,
    pub selected: usize,
    pub show_completed: bool,
}

impl TaskListState {
    pub fn new(tasks: Vec<TaskInfo>) -> Self {
        Self {
            tasks,
            selected: 0,
            show_completed: true,
        }
    }

    pub fn visible_tasks(&self) -> Vec<&TaskInfo> {
        self.tasks
            .iter()
            .filter(|t| {
                self.show_completed
                    || !matches!(t.status, TaskStatus::Completed | TaskStatus::Cancelled)
            })
            .collect()
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        let max = self.visible_tasks().len();
        if self.selected + 1 < max {
            self.selected += 1;
        }
    }
}

pub struct TaskListWidget<'a> {
    pub state: &'a TaskListState,
    pub theme: &'a Theme,
}

impl<'a> TaskListWidget<'a> {
    pub fn new(state: &'a TaskListState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for TaskListWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let tasks = self.state.visible_tasks();
        let items: Vec<ListItem> = tasks
            .iter()
            .enumerate()
            .map(|(i, task)| {
                let is_sel = i == self.state.selected;
                let status_color = match task.status {
                    TaskStatus::InProgress => self.theme.spinner_primary,
                    TaskStatus::Completed => self.theme.success,
                    TaskStatus::Failed => self.theme.error,
                    TaskStatus::Cancelled => self.theme.text_dim,
                    TaskStatus::Pending => self.theme.text_subtle,
                };

                let bg = if is_sel {
                    self.theme.selection
                } else {
                    Color::Reset
                };
                let style = Style::default().fg(self.theme.text).bg(bg);

                let mut spans = vec![
                    Span::styled(
                        format!("{} ", task.status.icon()),
                        Style::default().fg(status_color).bg(bg),
                    ),
                    Span::styled(
                        format!("#{} ", task.id),
                        Style::default().fg(self.theme.text_dim).bg(bg),
                    ),
                    Span::styled(
                        &task.subject,
                        if is_sel {
                            style.add_modifier(Modifier::BOLD)
                        } else {
                            style
                        },
                    ),
                ];

                if let Some(ref agent) = task.agent_name {
                    spans.push(Span::styled(
                        format!("  @{}", agent),
                        Style::default().fg(self.theme.secondary).bg(bg),
                    ));
                }

                if let Some(ref form) = task.active_form {
                    if task.status == TaskStatus::InProgress {
                        spans.push(Span::styled(
                            format!("  {}", form),
                            Style::default().fg(self.theme.text_dim).bg(bg),
                        ));
                    }
                }

                ListItem::new(Line::from(spans))
            })
            .collect();

        let pending = tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Pending)
            .count();
        let in_progress = tasks
            .iter()
            .filter(|t| t.status == TaskStatus::InProgress)
            .count();
        let completed = tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Completed)
            .count();

        let title = format!(
            " Work Items ({} pending, {} active, {} done) ",
            pending, in_progress, completed
        );

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.theme.style_border())
            .title(Span::styled(
                title,
                Style::default()
                    .fg(self.theme.primary)
                    .add_modifier(Modifier::BOLD),
            ));

        List::new(items).block(block).render(area, buf);
    }
}

// ---------------------------------------------------------------------------
// TaskDetailWidget — task detail view
// ---------------------------------------------------------------------------

pub struct TaskDetailWidget<'a> {
    pub task: &'a TaskInfo,
    pub theme: &'a Theme,
}

impl<'a> TaskDetailWidget<'a> {
    pub fn new(task: &'a TaskInfo, theme: &'a Theme) -> Self {
        Self { task, theme }
    }
}

impl<'a> Widget for TaskDetailWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.theme.style_border())
            .title(Span::styled(
                format!(" Task #{} ", self.task.id),
                Style::default()
                    .fg(self.theme.primary)
                    .add_modifier(Modifier::BOLD),
            ));

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let mut lines = vec![
            Line::from(vec![
                Span::styled("Subject: ", Style::default().fg(self.theme.text_dim)),
                Span::styled(
                    &self.task.subject,
                    Style::default()
                        .fg(self.theme.text)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled("Status: ", Style::default().fg(self.theme.text_dim)),
                Span::styled(
                    self.task.status.label(),
                    Style::default().fg(match self.task.status {
                        TaskStatus::InProgress => self.theme.spinner_primary,
                        TaskStatus::Completed => self.theme.success,
                        TaskStatus::Failed => self.theme.error,
                        _ => self.theme.text,
                    }),
                ),
            ]),
        ];

        if let Some(ref agent) = self.task.agent_name {
            lines.push(Line::from(vec![
                Span::styled("Agent: ", Style::default().fg(self.theme.text_dim)),
                Span::styled(agent.as_str(), Style::default().fg(self.theme.secondary)),
            ]));
        }

        if !self.task.blocked_by.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("Blocked by: ", Style::default().fg(self.theme.text_dim)),
                Span::styled(
                    self.task.blocked_by.join(", "),
                    Style::default().fg(self.theme.warning),
                ),
            ]));
        }

        if !self.task.blocks.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("Blocks: ", Style::default().fg(self.theme.text_dim)),
                Span::styled(
                    self.task.blocks.join(", "),
                    Style::default().fg(self.theme.info),
                ),
            ]));
        }

        Paragraph::new(lines).render(inner, buf);
    }
}

// ---------------------------------------------------------------------------
// BackgroundTasksDialogWidget — full tasks dialog
// ---------------------------------------------------------------------------

pub struct BackgroundTasksDialogWidget<'a> {
    pub state: &'a TaskListState,
    pub theme: &'a Theme,
    pub show_detail: bool,
}

impl<'a> BackgroundTasksDialogWidget<'a> {
    pub fn new(state: &'a TaskListState, theme: &'a Theme) -> Self {
        Self {
            state,
            theme,
            show_detail: false,
        }
    }
}

impl<'a> Widget for BackgroundTasksDialogWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 20 || area.height < 5 {
            return;
        }

        if self.show_detail {
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);

            let list = TaskListWidget::new(self.state, self.theme);
            list.render(chunks[0], buf);

            let visible = self.state.visible_tasks();
            if let Some(task) = visible.get(self.state.selected) {
                let detail = TaskDetailWidget::new(task, self.theme);
                detail.render(chunks[1], buf);
            }
        } else {
            let list = TaskListWidget::new(self.state, self.theme);
            list.render(area, buf);
        }
    }
}

// ===================================================================
// Task dialogs and progress widgets
// ===================================================================

#[derive(Debug, Clone, Default)]
pub struct AsyncAgentDetailDialog {
    pub task_id: String,
    pub messages: Vec<String>,
    pub selected_message: usize,
}

/// Render tool activity for a teammate or sub-agent.
pub fn render_tool_activity(tool_name: &str, status: &str) -> String {
    format!("[{}] {}", status, tool_name)
}

#[derive(Debug, Clone, Default)]
pub struct ShellProgress {
    pub command: String,
    pub elapsed_ms: u64,
    pub status: String,
}

/// One-line status text for the task list.
pub fn task_status_text(status: &str, elapsed_ms: u64) -> String {
    let secs = elapsed_ms / 1000;
    format!("{} · {}s", status, secs)
}

#[derive(Debug, Clone, Default)]
pub struct BackgroundTaskStatus {
    pub id: String,
    pub status: String,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Default)]
pub struct DreamDetailDialog {
    pub turn_count: u64,
    pub summary: String,
}

// ===================================================================
// taskStatusUtils.tsx
// ===================================================================

/// Whether the status is terminal (no further updates expected).
pub fn is_terminal_status(status: &str) -> bool {
    matches!(
        status,
        "completed" | "failed" | "canceled" | "stopped" | "error"
    )
}

/// Single-glyph icon for a task status.
pub fn get_task_status_icon(status: &str) -> &'static str {
    match status {
        "pending" => "○",
        "running" | "in_progress" => "●",
        "completed" => "✓",
        "failed" | "error" => "✗",
        "canceled" | "stopped" => "■",
        _ => "·",
    }
}

/// Theme colour for a task status icon.
pub fn get_task_status_color(status: &str, theme: &crate::theme::Theme) -> ratatui::style::Color {
    match status {
        "completed" => theme.success,
        "failed" | "error" => theme.error,
        "canceled" | "stopped" => theme.text_dim,
        "running" | "in_progress" => theme.primary,
        _ => theme.text_dim,
    }
}

/// One-line description of a teammate's current activity.
pub fn describe_teammate_activity(teammate: &str, last_tool: Option<&str>) -> String {
    match last_tool {
        Some(t) => format!("{} · {}", teammate, t),
        None => teammate.to_string(),
    }
}

/// Whether the tasks footer should be hidden (no tasks visible).
pub fn should_hide_tasks_footer(visible_count: usize, in_dialog: bool) -> bool {
    in_dialog || visible_count == 0
}

#[derive(Debug, Clone, Default)]
pub struct ShellDetailDialog {
    pub task_id: String,
    pub command: String,
    pub output: String,
}

#[derive(Debug, Clone, Default)]
pub struct BackgroundTasksDialog {
    pub tasks: Vec<BackgroundTaskStatus>,
    pub selected: usize,
}

#[derive(Debug, Clone, Default)]
pub struct InProcessTeammateDetailDialog {
    pub teammate: String,
    pub turns: u64,
    pub last_message: String,
}

#[derive(Debug, Clone, Default)]
pub struct BackgroundTask {
    pub id: String,
    pub kind: String,
    pub started_at_ms: u64,
    pub status: String,
}

