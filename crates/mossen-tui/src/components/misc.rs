//! Feedback, status bar, model picker, notification, shell, memory,
//! skills, teams, and miscellaneous UI components.
//!
//! Translates: FeedbackSurvey/, StatusLine.tsx, ModelPicker.tsx,
//! Notifications.tsx, shell/, memory/, skills/, teams/, ui/,
//! HelpV2/, and standalone component files.

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Gauge, List, ListItem, Paragraph, Tabs, Widget, Wrap},
};

use crate::components::design_system::DialogWidget;
use crate::theme::Theme;

// ===================================================================
// Feedback / Survey (FeedbackSurvey/)
// ===================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedbackRating {
    Positive,
    Negative,
    Neutral,
}

#[derive(Debug, Clone)]
pub struct FeedbackState {
    pub rating: Option<FeedbackRating>,
    pub comment: String,
    pub submitted: bool,
    pub tags: Vec<String>,
    pub selected_tag: usize,
}

impl FeedbackState {
    pub fn new() -> Self {
        Self {
            rating: None,
            comment: String::new(),
            submitted: false,
            tags: vec![
                "Helpful".into(),
                "Accurate".into(),
                "Fast".into(),
                "Needs improvement".into(),
            ],
            selected_tag: 0,
        }
    }
}

impl Default for FeedbackState {
    fn default() -> Self {
        Self::new()
    }
}

pub struct FeedbackWidget<'a> {
    pub state: &'a FeedbackState,
    pub theme: &'a Theme,
}

impl<'a> FeedbackWidget<'a> {
    pub fn new(state: &'a FeedbackState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for FeedbackWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let dialog = DialogWidget::new("Feedback", self.theme).size(50, 12);
        let inner = dialog.inner_area(area);
        dialog.render(area, buf);

        if inner.width == 0 || inner.height < 4 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Min(1),
            ])
            .split(inner);

        // Rating
        let thumbs_up = if self.state.rating == Some(FeedbackRating::Positive) {
            Style::default()
                .fg(self.theme.success)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(self.theme.text_dim)
        };
        let thumbs_down = if self.state.rating == Some(FeedbackRating::Negative) {
            Style::default()
                .fg(self.theme.error)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(self.theme.text_dim)
        };

        let rating_line = Line::from(vec![
            Span::styled("Rate: ", Style::default().fg(self.theme.text)),
            Span::styled(" 👍 ", thumbs_up),
            Span::raw("  "),
            Span::styled(" 👎 ", thumbs_down),
        ]);
        buf.set_line(chunks[0].x, chunks[0].y, &rating_line, chunks[0].width);

        // Tags
        let mut tag_spans: Vec<Span> = vec![Span::styled(
            "Tags: ",
            Style::default().fg(self.theme.text_dim),
        )];
        for (i, tag) in self.state.tags.iter().enumerate() {
            let style = if i == self.state.selected_tag {
                Style::default()
                    .fg(self.theme.primary)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(self.theme.text_dim)
            };
            tag_spans.push(Span::styled(format!("[{}] ", tag), style));
        }
        buf.set_line(
            chunks[1].x,
            chunks[1].y,
            &Line::from(tag_spans),
            chunks[1].width,
        );

        // Comment
        let comment_line = Line::from(vec![
            Span::styled("Comment: ", Style::default().fg(self.theme.text_dim)),
            Span::styled(
                if self.state.comment.is_empty() {
                    "(optional)"
                } else {
                    &self.state.comment
                },
                Style::default().fg(self.theme.text),
            ),
        ]);
        buf.set_line(chunks[2].x, chunks[2].y, &comment_line, chunks[2].width);
    }
}

// ===================================================================
// StatusBar (StatusLine.tsx)
// ===================================================================

#[derive(Debug, Clone)]
pub struct StatusBarState {
    pub model: Option<String>,
    pub access_policy: String,
    pub fast_mode: bool,
    pub thinking: bool,
    pub message_count: usize,
    pub cost: Option<f64>,
    pub left_text: Option<String>,
}

impl StatusBarState {
    pub fn new() -> Self {
        Self {
            model: None,
            access_policy: "Supervised".into(),
            fast_mode: false,
            thinking: true,
            message_count: 0,
            cost: None,
            left_text: None,
        }
    }
}

impl Default for StatusBarState {
    fn default() -> Self {
        Self::new()
    }
}

pub struct StatusBarWidget<'a> {
    pub state: &'a StatusBarState,
    pub theme: &'a Theme,
}

impl<'a> StatusBarWidget<'a> {
    pub fn new(state: &'a StatusBarState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for StatusBarWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        // Background
        let bg = Style::default().bg(self.theme.surface);
        for x in area.x..area.x + area.width {
            buf.set_string(x, area.y, " ", bg);
        }

        // Left: cwd + model + mode + flags. cwd shows just the trailing
        // directory name so long absolute paths don't crowd out the rest of
        // the bar; users who need the full path can `/pwd`.
        let mut left_spans: Vec<Span> = Vec::new();
        if let Some(ref text) = self.state.left_text {
            let short = std::path::Path::new(text)
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| text.clone());
            left_spans.push(Span::styled(
                format!(" 📁 {} ", short),
                Style::default()
                    .fg(self.theme.text_dim)
                    .bg(self.theme.surface),
            ));
        }
        if let Some(ref model) = self.state.model {
            left_spans.push(Span::styled(
                format!(" {} ", model),
                Style::default()
                    .fg(self.theme.primary)
                    .bg(self.theme.surface),
            ));
        }
        left_spans.push(Span::styled(
            format!(" {} ", self.state.access_policy),
            Style::default()
                .fg(self.theme.text_dim)
                .bg(self.theme.surface),
        ));
        if self.state.fast_mode {
            left_spans.push(Span::styled(
                " ⚡ ",
                Style::default()
                    .fg(self.theme.warning)
                    .bg(self.theme.surface),
            ));
        }
        if self.state.thinking {
            left_spans.push(Span::styled(
                " 💭 ",
                Style::default()
                    .fg(self.theme.secondary)
                    .bg(self.theme.surface),
            ));
        }
        buf.set_line(area.x, area.y, &Line::from(left_spans), area.width / 2);

        // Right: cost + msgs
        let mut right_parts: Vec<String> = Vec::new();
        if let Some(cost) = self.state.cost {
            right_parts.push(format!("${:.2}", cost));
        }
        right_parts.push(format!("{} msgs", self.state.message_count));
        let right_text = right_parts.join("  ");
        let right_x = area.x + area.width.saturating_sub(right_text.len() as u16 + 1);
        buf.set_string(
            right_x,
            area.y,
            &right_text,
            Style::default()
                .fg(self.theme.text_dim)
                .bg(self.theme.surface),
        );
    }
}

// ===================================================================
// ModelPicker (ModelPicker.tsx)
// ===================================================================

#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub supports_thinking: bool,
    pub supports_streaming: bool,
    pub is_current: bool,
}

#[derive(Debug, Clone)]
pub struct ModelPickerState {
    pub models: Vec<ModelInfo>,
    pub selected: usize,
    pub filter: String,
}

impl ModelPickerState {
    pub fn new(models: Vec<ModelInfo>) -> Self {
        Self {
            models,
            selected: 0,
            filter: String::new(),
        }
    }

    pub fn filtered(&self) -> Vec<(usize, &ModelInfo)> {
        if self.filter.is_empty() {
            self.models.iter().enumerate().collect()
        } else {
            let q = self.filter.to_lowercase();
            self.models
                .iter()
                .enumerate()
                .filter(|(_, m)| {
                    m.name.to_lowercase().contains(&q) || m.id.to_lowercase().contains(&q)
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
        let max = self.filtered().len();
        if self.selected + 1 < max {
            self.selected += 1;
        }
    }
}

pub struct ModelPickerWidget<'a> {
    pub state: &'a ModelPickerState,
    pub theme: &'a Theme,
}

impl<'a> ModelPickerWidget<'a> {
    pub fn new(state: &'a ModelPickerState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for ModelPickerWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let dialog = DialogWidget::new("Select Model", self.theme).size(60, 18);
        let inner = dialog.inner_area(area);
        dialog.render(area, buf);

        if inner.width == 0 || inner.height < 3 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .split(inner);

        // Filter input
        let filter_line = if self.state.filter.is_empty() {
            Line::from(Span::styled(
                "Type to filter models...",
                Style::default().fg(self.theme.text_dim),
            ))
        } else {
            Line::from(vec![
                Span::styled("🔍 ", Style::default()),
                Span::styled(&self.state.filter, Style::default().fg(self.theme.text)),
            ])
        };
        buf.set_line(chunks[0].x, chunks[0].y, &filter_line, chunks[0].width);

        // Model list
        let filtered = self.state.filtered();
        for (vi, (_, model)) in filtered.iter().enumerate() {
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

            let mut spans = vec![Span::styled(
                if is_sel { "▸ " } else { "  " },
                Style::default().fg(self.theme.primary).bg(bg),
            )];
            if model.is_current {
                spans.push(Span::styled(
                    "● ",
                    Style::default().fg(self.theme.success).bg(bg),
                ));
            }
            spans.push(Span::styled(
                &model.name,
                Style::default()
                    .fg(self.theme.text)
                    .bg(bg)
                    .add_modifier(if is_sel {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            ));
            spans.push(Span::styled(
                format!("  ({})", model.provider),
                Style::default().fg(self.theme.text_dim).bg(bg),
            ));
            if model.supports_thinking {
                spans.push(Span::styled(
                    " 💭",
                    Style::default().fg(self.theme.secondary).bg(bg),
                ));
            }

            buf.set_line(chunks[1].x, y, &Line::from(spans), chunks[1].width);
        }
    }
}

// ===================================================================
// Notification (Notifications.tsx)
// ===================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationLevel {
    Info,
    Warning,
    Error,
    Success,
}

#[derive(Debug, Clone)]
pub struct Notification {
    pub id: String,
    pub level: NotificationLevel,
    pub message: String,
    pub dismissable: bool,
    pub timestamp_ms: u64,
}

#[derive(Debug, Clone)]
pub struct NotificationState {
    pub notifications: Vec<Notification>,
    pub max_visible: usize,
}

impl NotificationState {
    pub fn new() -> Self {
        Self {
            notifications: Vec::new(),
            max_visible: 5,
        }
    }

    pub fn push(&mut self, notif: Notification) {
        self.notifications.push(notif);
    }

    pub fn dismiss(&mut self, id: &str) {
        self.notifications.retain(|n| n.id != id);
    }
}

impl Default for NotificationState {
    fn default() -> Self {
        Self::new()
    }
}

pub struct NotificationWidget<'a> {
    pub state: &'a NotificationState,
    pub theme: &'a Theme,
}

impl<'a> NotificationWidget<'a> {
    pub fn new(state: &'a NotificationState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for NotificationWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 || self.state.notifications.is_empty() {
            return;
        }

        let visible = self
            .state
            .notifications
            .iter()
            .rev()
            .take(self.state.max_visible);
        for (i, notif) in visible.enumerate() {
            let y = area.y + i as u16;
            if y >= area.y + area.height {
                break;
            }

            let (icon, color) = match notif.level {
                NotificationLevel::Info => ("ℹ", self.theme.info),
                NotificationLevel::Warning => ("⚠", self.theme.warning),
                NotificationLevel::Error => ("✗", self.theme.error),
                NotificationLevel::Success => ("✓", self.theme.success),
            };

            let line = Line::from(vec![
                Span::styled(format!("{} ", icon), Style::default().fg(color)),
                Span::styled(&notif.message, Style::default().fg(self.theme.text)),
            ]);
            buf.set_line(area.x, y, &line, area.width);
        }
    }
}

// ===================================================================
// Shell UI (components/shell/)
// ===================================================================

#[derive(Debug, Clone)]
pub struct ShellOutputState {
    pub lines: Vec<ShellOutputLine>,
    pub scroll_offset: usize,
    pub is_running: bool,
}

#[derive(Debug, Clone)]
pub struct ShellOutputLine {
    pub content: String,
    pub is_stderr: bool,
}

impl ShellOutputState {
    pub fn new() -> Self {
        Self {
            lines: Vec::new(),
            scroll_offset: 0,
            is_running: false,
        }
    }
}

impl Default for ShellOutputState {
    fn default() -> Self {
        Self::new()
    }
}

pub struct ShellOutputWidget<'a> {
    pub state: &'a ShellOutputState,
    pub theme: &'a Theme,
}

impl<'a> ShellOutputWidget<'a> {
    pub fn new(state: &'a ShellOutputState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for ShellOutputWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let visible = self
            .state
            .lines
            .iter()
            .skip(self.state.scroll_offset)
            .take(area.height as usize);

        for (i, line) in visible.enumerate() {
            let y = area.y + i as u16;
            let style = if line.is_stderr {
                Style::default().fg(self.theme.error)
            } else {
                Style::default().fg(self.theme.text)
            };
            let avail = area.width as usize;
            let truncated: String = line.content.chars().take(avail).collect();
            buf.set_string(area.x, y, &truncated, style);
        }
    }
}

// ===================================================================
// Memory UI (components/memory/)
// ===================================================================

#[derive(Debug, Clone)]
pub struct MemoryEntry {
    pub title: String,
    pub category: String,
    pub preview: String,
}

#[derive(Debug, Clone)]
pub struct MemoryPanelState {
    pub entries: Vec<MemoryEntry>,
    pub selected: usize,
}

impl MemoryPanelState {
    pub fn new(entries: Vec<MemoryEntry>) -> Self {
        Self {
            entries,
            selected: 0,
        }
    }
}

pub struct MemoryPanelWidget<'a> {
    pub state: &'a MemoryPanelState,
    pub theme: &'a Theme,
}

impl<'a> MemoryPanelWidget<'a> {
    pub fn new(state: &'a MemoryPanelState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for MemoryPanelWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.theme.style_border())
            .title(" Recall ");
        let inner = block.inner(area);
        block.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let items: Vec<ListItem> = self
            .state
            .entries
            .iter()
            .enumerate()
            .map(|(i, entry)| {
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
                    Span::styled(&entry.title, style),
                    Span::styled(
                        format!("  [{}]", entry.category),
                        Style::default().fg(self.theme.text_dim).bg(if is_sel {
                            self.theme.selection
                        } else {
                            Color::Reset
                        }),
                    ),
                ]);
                ListItem::new(line)
            })
            .collect();

        List::new(items).render(inner, buf);
    }
}

// ===================================================================
// Skills UI (components/skills/)
// ===================================================================

#[derive(Debug, Clone)]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
    pub enabled: bool,
}

#[derive(Debug, Clone)]
pub struct SkillsPanelState {
    pub skills: Vec<SkillInfo>,
    pub selected: usize,
}

impl SkillsPanelState {
    pub fn new(skills: Vec<SkillInfo>) -> Self {
        Self {
            skills,
            selected: 0,
        }
    }
}

pub struct SkillsPanelWidget<'a> {
    pub state: &'a SkillsPanelState,
    pub theme: &'a Theme,
}

impl<'a> SkillsPanelWidget<'a> {
    pub fn new(state: &'a SkillsPanelState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for SkillsPanelWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.theme.style_border())
            .title(" Crafts ");
        let inner = block.inner(area);
        block.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        for (i, skill) in self.state.skills.iter().enumerate() {
            let y = inner.y + i as u16;
            if y >= inner.y + inner.height {
                break;
            }

            let is_sel = i == self.state.selected;
            let checkbox = if skill.enabled { "[✓]" } else { "[ ]" };
            let bg = if is_sel {
                self.theme.selection
            } else {
                Color::Reset
            };

            let line = Line::from(vec![
                Span::styled(
                    format!("{} {} ", if is_sel { "▸" } else { " " }, checkbox),
                    Style::default().fg(self.theme.text).bg(bg),
                ),
                Span::styled(
                    &skill.name,
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
                    format!("  {}", skill.description),
                    Style::default().fg(self.theme.text_dim).bg(bg),
                ),
            ]);
            buf.set_line(inner.x, y, &line, inner.width);
        }
    }
}

// ===================================================================
// Teams UI (components/teams/)
// ===================================================================

#[derive(Debug, Clone)]
pub struct TeamMember {
    pub name: String,
    pub role: String,
    pub status: String,
    pub color: Option<Color>,
}

#[derive(Debug, Clone)]
pub struct TeamsPanelState {
    pub members: Vec<TeamMember>,
    pub selected: usize,
}

impl TeamsPanelState {
    pub fn new(members: Vec<TeamMember>) -> Self {
        Self {
            members,
            selected: 0,
        }
    }
}

pub struct TeamsPanelWidget<'a> {
    pub state: &'a TeamsPanelState,
    pub theme: &'a Theme,
}

impl<'a> TeamsPanelWidget<'a> {
    pub fn new(state: &'a TeamsPanelState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for TeamsPanelWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.theme.style_border())
            .title(" Team ");
        let inner = block.inner(area);
        block.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        for (i, member) in self.state.members.iter().enumerate() {
            let y = inner.y + i as u16;
            if y >= inner.y + inner.height {
                break;
            }

            let color = member.color.unwrap_or(self.theme.primary);
            let is_sel = i == self.state.selected;
            let bg = if is_sel {
                self.theme.selection
            } else {
                Color::Reset
            };

            let line = Line::from(vec![
                Span::styled("● ", Style::default().fg(color).bg(bg)),
                Span::styled(
                    &member.name,
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
                    format!("  {} — {}", member.role, member.status),
                    Style::default().fg(self.theme.text_dim).bg(bg),
                ),
            ]);
            buf.set_line(inner.x, y, &line, inner.width);
        }
    }
}

// ===================================================================
// HelpV2 (components/HelpV2/)
// ===================================================================

#[derive(Debug, Clone)]
pub struct HelpSection {
    pub title: String,
    pub items: Vec<HelpItem>,
}

#[derive(Debug, Clone)]
pub struct HelpItem {
    pub key: String,
    pub description: String,
}

pub struct HelpWidget<'a> {
    pub sections: &'a [HelpSection],
    pub theme: &'a Theme,
}

impl<'a> HelpWidget<'a> {
    pub fn new(sections: &'a [HelpSection], theme: &'a Theme) -> Self {
        Self { sections, theme }
    }
}

impl<'a> Widget for HelpWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let dialog = DialogWidget::new("Help", self.theme).size(70, 20);
        let inner = dialog.inner_area(area);
        dialog.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let mut y = inner.y;
        for section in self.sections {
            if y >= inner.y + inner.height {
                break;
            }

            // Section title
            buf.set_line(
                inner.x,
                y,
                &Line::from(Span::styled(
                    &section.title,
                    Style::default()
                        .fg(self.theme.primary)
                        .add_modifier(Modifier::BOLD),
                )),
                inner.width,
            );
            y += 1;

            for item in &section.items {
                if y >= inner.y + inner.height {
                    break;
                }

                let line = Line::from(vec![
                    Span::styled(
                        format!("  {:>12}  ", item.key),
                        Style::default()
                            .fg(self.theme.info)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(&item.description, Style::default().fg(self.theme.text)),
                ]);
                buf.set_line(inner.x, y, &line, inner.width);
                y += 1;
            }
            y += 1; // gap between sections
        }
    }
}

// ===================================================================
// Miscellaneous standalone components
// ===================================================================

/// Token warning widget (TokenWarning.tsx).
pub struct TokenWarningWidget<'a> {
    pub usage_ratio: f64,
    pub current_tokens: usize,
    pub max_tokens: usize,
    pub theme: &'a Theme,
}

impl<'a> TokenWarningWidget<'a> {
    pub fn new(current: usize, max: usize, theme: &'a Theme) -> Self {
        Self {
            usage_ratio: if max > 0 {
                current as f64 / max as f64
            } else {
                0.0
            },
            current_tokens: current,
            max_tokens: max,
            theme,
        }
    }
}

impl<'a> Widget for TokenWarningWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let color = if self.usage_ratio > 0.9 {
            self.theme.error
        } else if self.usage_ratio > 0.7 {
            self.theme.warning
        } else {
            self.theme.text_dim
        };

        let line = Line::from(vec![
            Span::styled("⚠ ", Style::default().fg(color)),
            Span::styled(
                format!(
                    "Context: {}/{} tokens ({:.0}%)",
                    self.current_tokens,
                    self.max_tokens,
                    self.usage_ratio * 100.0
                ),
                Style::default().fg(color),
            ),
        ]);
        buf.set_line(area.x, area.y, &line, area.width);
    }
}

/// Compact summary widget (CompactSummary.tsx).
pub struct CondenseNoticeWidget<'a> {
    pub summary: &'a str,
    pub removed_messages: usize,
    pub theme: &'a Theme,
}

impl<'a> CondenseNoticeWidget<'a> {
    pub fn new(summary: &'a str, removed: usize, theme: &'a Theme) -> Self {
        Self {
            summary,
            removed_messages: removed,
            theme,
        }
    }
}

impl<'a> Widget for CondenseNoticeWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let line = Line::from(vec![
            Span::styled("📝 Condensed ", Style::default().fg(self.theme.info)),
            Span::styled(
                format!("({} messages removed)", self.removed_messages),
                Style::default().fg(self.theme.text_dim),
            ),
        ]);
        buf.set_line(area.x, area.y, &line, area.width);

        if area.height > 1 {
            let avail = area.width as usize;
            let summary: String = self.summary.chars().take(avail).collect();
            buf.set_string(
                area.x,
                area.y + 1,
                &summary,
                Style::default().fg(self.theme.text_dim),
            );
        }
    }
}

/// Thinking toggle widget (ThinkingToggle.tsx).
pub struct ThinkingToggleWidget<'a> {
    pub enabled: bool,
    pub theme: &'a Theme,
}

impl<'a> ThinkingToggleWidget<'a> {
    pub fn new(enabled: bool, theme: &'a Theme) -> Self {
        Self { enabled, theme }
    }
}

impl<'a> Widget for ThinkingToggleWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let (icon, label, color) = if self.enabled {
            ("💭", "Reasoning ON", self.theme.secondary)
        } else {
            ("💤", "Reasoning OFF", self.theme.text_dim)
        };

        let line = Line::from(vec![
            Span::styled(format!("{} ", icon), Style::default().fg(color)),
            Span::styled(label, Style::default().fg(color)),
        ]);
        buf.set_line(area.x, area.y, &line, area.width);
    }
}

/// Effort callout widget (EffortCallout.tsx).
pub struct EffortCalloutWidget<'a> {
    pub level: &'a str,
    pub theme: &'a Theme,
}

impl<'a> EffortCalloutWidget<'a> {
    pub fn new(level: &'a str, theme: &'a Theme) -> Self {
        Self { level, theme }
    }
}

impl<'a> Widget for EffortCalloutWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let line = Line::from(vec![
            Span::styled("Effort: ", Style::default().fg(self.theme.text_dim)),
            Span::styled(
                self.level,
                Style::default()
                    .fg(self.theme.primary)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);
        buf.set_line(area.x, area.y, &line, area.width);
    }
}

// ===================================================================
// Shell components
// ===================================================================

#[derive(Debug, Clone, Default)]
pub struct ShellTimeDisplay {
    pub elapsed_ms: u64,
    pub started_at_ms: u64,
}

impl ShellTimeDisplay {
    pub fn formatted(&self) -> String {
        let s = self.elapsed_ms / 1000;
        if s < 60 {
            format!("{}s", s)
        } else if s < 3600 {
            format!("{}m{:02}s", s / 60, s % 60)
        } else {
            format!("{}h{:02}m", s / 3600, (s % 3600) / 60)
        }
    }
}

/// Context provider for expand-shell-output visibility.
#[derive(Debug, Clone, Default)]
pub struct ExpandShellOutputProvider {
    pub expanded: std::collections::HashMap<String, bool>,
}

/// Hook-equivalent: read & toggle expansion for a given shell output id.
pub fn use_expand_shell_output(provider: &mut ExpandShellOutputProvider, id: &str) -> bool {
    let cur = provider.expanded.get(id).copied().unwrap_or(false);
    provider.expanded.insert(id.to_string(), !cur);
    !cur
}

/// Attempt to parse a string as JSON and pretty-format it.
pub fn try_format_json(s: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(s).ok()?;
    serde_json::to_string_pretty(&v).ok()
}

/// Try to JSON-format a content payload if it parses as JSON; else return None.
pub fn try_json_format_content(content: &str) -> Option<String> {
    if content.trim_start().starts_with('{') || content.trim_start().starts_with('[') {
        try_format_json(content)
    } else {
        None
    }
}

/// Replace URLs in text with display tuples (offset, length).
pub fn linkify_urls_in_text(text: &str) -> Vec<(usize, usize, String)> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i..].starts_with(b"http://") || bytes[i..].starts_with(b"https://") {
            let mut end = i;
            while end < bytes.len() && !bytes[end].is_ascii_whitespace() {
                end += 1;
            }
            out.push((i, end - i, text[i..end].to_string()));
            i = end;
        } else {
            i += 1;
        }
    }
    out
}

/// Strip ANSI underline-on/off escape sequences from input.
pub fn strip_underline_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            // ESC [ ... m
            chars.next();
            let mut buf = String::new();
            while let Some(&n) = chars.peek() {
                chars.next();
                if n == 'm' {
                    break;
                }
                buf.push(n);
            }
            if buf != "4" && buf != "24" {
                out.push('\x1b');
                out.push('[');
                out.push_str(&buf);
                out.push('m');
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[derive(Debug, Clone, Default)]
pub struct OutputLine {
    pub text: String,
    pub is_stderr: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ShellProgressMessage {
    pub command: String,
    pub elapsed_ms: u64,
    pub bytes_out: u64,
}

// ===================================================================
// Sandbox components
// ===================================================================

#[derive(Debug, Clone, Default)]
pub struct SandboxDoctorSection {
    pub findings: Vec<String>,
    pub overall_ok: bool,
}

pub fn get_sandbox_override_docs_url() -> &'static str {
    "https://docs.mossen.dev/sandbox/overrides"
}

/// Build the copy shown when the sandbox falls back to unsandboxed mode.
pub fn get_unsandboxed_fallback_copy(reason: &str) -> String {
    format!(
        "Sandbox unavailable: {}. Falling back to direct execution — be careful.",
        reason
    )
}

#[derive(Debug, Clone, Default)]
pub struct SandboxOverridesTab {
    pub overrides: Vec<String>,
    pub selected: usize,
}

pub fn get_sandbox_docs_url() -> &'static str {
    "https://docs.mossen.dev/sandbox"
}

#[derive(Debug, Clone, Default)]
pub struct SandboxSettings {
    pub enabled: bool,
    pub current_profile: String,
}

#[derive(Debug, Clone, Default)]
pub struct SandboxDependenciesTab {
    pub items: Vec<String>,
    pub satisfied: Vec<bool>,
}

#[derive(Debug, Clone, Default)]
pub struct SandboxConfigTab {
    pub key_values: Vec<(String, String)>,
    pub selected: usize,
}
