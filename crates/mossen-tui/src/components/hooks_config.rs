//! Hooks configuration UI components.
//!
//! Translates: hooks/HooksConfigMenu.tsx, hooks/PromptDialog.tsx,
//! hooks/SelectEventMode.tsx, hooks/SelectHookMode.tsx,
//! hooks/SelectMatcherMode.tsx, hooks/ViewHookMode.tsx

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
// Types
// ===================================================================

/// Hook event types.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HookEvent {
    PreToolUse,
    PostToolUse,
    Notification,
    Stop,
    SubagentStop,
}

impl HookEvent {
    pub fn label(&self) -> &'static str {
        match self {
            Self::PreToolUse => "Pre-tool use",
            Self::PostToolUse => "Post-tool use",
            Self::Notification => "Notification",
            Self::Stop => "Stop",
            Self::SubagentStop => "Subagent stop",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::PreToolUse => "Runs before a tool is executed",
            Self::PostToolUse => "Runs after a tool finishes",
            Self::Notification => "Runs when a notification is sent",
            Self::Stop => "Runs when the agent stops",
            Self::SubagentStop => "Runs when a subagent stops",
        }
    }

    pub fn all() -> &'static [HookEvent] {
        &[
            Self::PreToolUse,
            Self::PostToolUse,
            Self::Notification,
            Self::Stop,
            Self::SubagentStop,
        ]
    }
}

/// Hook configuration type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookType {
    Command,
    Prompt,
    Agent,
    Http,
}

impl HookType {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Command => "Command",
            Self::Prompt => "Prompt",
            Self::Agent => "Agent",
            Self::Http => "HTTP",
        }
    }

    pub fn content_field_label(&self) -> &'static str {
        match self {
            Self::Command => "Command",
            Self::Prompt => "Prompt",
            Self::Agent => "Prompt",
            Self::Http => "URL",
        }
    }
}

/// Individual hook configuration.
#[derive(Debug, Clone)]
pub struct IndividualHookConfig {
    pub hook_type: HookType,
    pub event: HookEvent,
    pub matcher: String,
    pub content: String,
    pub status_message: Option<String>,
    pub enabled: bool,
}

impl IndividualHookConfig {
    pub fn new(hook_type: HookType, event: HookEvent, content: impl Into<String>) -> Self {
        Self {
            hook_type,
            event,
            matcher: String::new(),
            content: content.into(),
            status_message: None,
            enabled: true,
        }
    }

    pub fn with_matcher(mut self, matcher: impl Into<String>) -> Self {
        self.matcher = matcher.into();
        self
    }

    pub fn with_status_message(mut self, msg: impl Into<String>) -> Self {
        self.status_message = Some(msg.into());
        self
    }
}

/// Hook event metadata for display.
#[derive(Debug, Clone)]
pub struct HookEventMetadata {
    pub event: HookEvent,
    pub hook_count: usize,
    pub restricted_by_policy: bool,
}

/// Matcher metadata for tool-based events.
#[derive(Debug, Clone)]
pub struct MatcherMetadata {
    pub label: String,
    pub description: String,
}

// ===================================================================
// Mode states for the hooks config menu
// ===================================================================

/// Navigation mode for the hooks config menu.
#[derive(Debug, Clone)]
pub enum HooksConfigMode {
    SelectEvent,
    SelectMatcher { event: HookEvent },
    SelectHook { event: HookEvent, matcher: String },
    ViewHook { event: HookEvent, matcher: String, hook_index: usize },
    PromptDialog { event: HookEvent, matcher: String },
    Disabled,
}

// ===================================================================
// HooksConfigMenuState — from HooksConfigMenu.tsx
// ===================================================================

/// Top-level hooks configuration state.
#[derive(Debug, Clone)]
pub struct HooksConfigMenuState {
    pub mode: HooksConfigMode,
    pub hooks: Vec<IndividualHookConfig>,
    pub event_metadata: Vec<HookEventMetadata>,
    pub tool_names: Vec<String>,
    pub selected_index: usize,
    pub hooks_enabled: bool,
    pub restricted_by_policy: bool,
}

impl HooksConfigMenuState {
    pub fn new(hooks: Vec<IndividualHookConfig>, tool_names: Vec<String>) -> Self {
        let event_metadata = Self::compute_event_metadata(&hooks);
        let total_count = hooks.len();
        Self {
            mode: HooksConfigMode::SelectEvent,
            hooks,
            event_metadata,
            tool_names,
            selected_index: 0,
            hooks_enabled: true,
            restricted_by_policy: false,
        }
    }

    fn compute_event_metadata(hooks: &[IndividualHookConfig]) -> Vec<HookEventMetadata> {
        HookEvent::all()
            .iter()
            .map(|event| {
                let count = hooks.iter().filter(|h| h.event == *event).count();
                HookEventMetadata {
                    event: event.clone(),
                    hook_count: count,
                    restricted_by_policy: false,
                }
            })
            .collect()
    }

    pub fn total_hooks_count(&self) -> usize {
        self.hooks.len()
    }

    pub fn hooks_for_event(&self, event: &HookEvent) -> Vec<&IndividualHookConfig> {
        self.hooks.iter().filter(|h| &h.event == event).collect()
    }

    pub fn hooks_for_event_and_matcher(&self, event: &HookEvent, matcher: &str) -> Vec<&IndividualHookConfig> {
        self.hooks
            .iter()
            .filter(|h| &h.event == event && h.matcher == matcher)
            .collect()
    }

    pub fn select_event(&mut self, event: HookEvent) {
        // Check if event has matcher metadata
        let has_matcher = matches!(event, HookEvent::PreToolUse | HookEvent::PostToolUse);
        if has_matcher && !self.tool_names.is_empty() {
            self.mode = HooksConfigMode::SelectMatcher { event };
        } else {
            self.mode = HooksConfigMode::SelectHook {
                event,
                matcher: String::new(),
            };
        }
        self.selected_index = 0;
    }

    pub fn select_matcher(&mut self, event: HookEvent, matcher: String) {
        self.mode = HooksConfigMode::SelectHook { event, matcher };
        self.selected_index = 0;
    }

    pub fn select_hook(&mut self, event: HookEvent, matcher: String, hook_index: usize) {
        self.mode = HooksConfigMode::ViewHook {
            event,
            matcher,
            hook_index,
        };
    }

    pub fn go_back(&mut self) {
        match &self.mode {
            HooksConfigMode::ViewHook { event, matcher, .. } => {
                let e = event.clone();
                let m = matcher.clone();
                self.mode = HooksConfigMode::SelectHook {
                    event: e,
                    matcher: m,
                };
                self.selected_index = 0;
            }
            HooksConfigMode::SelectHook { event, .. } => {
                let has_matcher = matches!(event, HookEvent::PreToolUse | HookEvent::PostToolUse);
                if has_matcher && !self.tool_names.is_empty() {
                    let e = event.clone();
                    self.mode = HooksConfigMode::SelectMatcher { event: e };
                } else {
                    self.mode = HooksConfigMode::SelectEvent;
                }
                self.selected_index = 0;
            }
            HooksConfigMode::SelectMatcher { .. } => {
                self.mode = HooksConfigMode::SelectEvent;
                self.selected_index = 0;
            }
            HooksConfigMode::PromptDialog { event, matcher } => {
                let e = event.clone();
                let m = matcher.clone();
                self.mode = HooksConfigMode::SelectHook {
                    event: e,
                    matcher: m,
                };
                self.selected_index = 0;
            }
            _ => {}
        }
    }

    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    pub fn move_down(&mut self, max: usize) {
        if self.selected_index + 1 < max {
            self.selected_index += 1;
        }
    }
}

// ===================================================================
// SelectEventMode Widget — from SelectEventMode.tsx
// ===================================================================

/// Widget for the event selection mode.
pub struct SelectEventModeWidget<'a> {
    pub state: &'a HooksConfigMenuState,
    pub theme: &'a Theme,
}

impl<'a> SelectEventModeWidget<'a> {
    pub fn new(state: &'a HooksConfigMenuState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for SelectEventModeWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let title = format!("Hook Configuration ({} hooks)", self.state.total_hooks_count());
        let dialog = DialogWidget::new(&title, self.theme).size(60, 16);
        let inner = dialog.inner_area(area);
        dialog.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let items: Vec<ListItem> = self
            .state
            .event_metadata
            .iter()
            .enumerate()
            .map(|(i, meta)| {
                let is_selected = i == self.state.selected_index;
                let prefix = if is_selected { "❯ " } else { "  " };
                let count_label = if meta.hook_count > 0 {
                    format!(" ({})", meta.hook_count)
                } else {
                    String::new()
                };
                let restricted = if meta.restricted_by_policy { " 🔒" } else { "" };

                let style = if is_selected {
                    Style::default()
                        .fg(self.theme.primary)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                let line = Line::from(vec![
                    Span::raw(prefix.to_string()),
                    Span::styled(meta.event.label().to_string(), style),
                    Span::styled(count_label, Style::default().fg(Color::DarkGray)),
                    Span::raw(restricted.to_string()),
                ]);
                ListItem::new(line)
            })
            .collect();

        let list = List::new(items);
        Widget::render(list, inner, buf);
    }
}

// ===================================================================
// SelectMatcherMode Widget — from SelectMatcherMode.tsx
// ===================================================================

/// Widget for the matcher selection mode.
pub struct SelectMatcherModeWidget<'a> {
    pub state: &'a HooksConfigMenuState,
    pub event: &'a HookEvent,
    pub theme: &'a Theme,
}

impl<'a> SelectMatcherModeWidget<'a> {
    pub fn new(state: &'a HooksConfigMenuState, event: &'a HookEvent, theme: &'a Theme) -> Self {
        Self { state, event, theme }
    }
}

impl<'a> Widget for SelectMatcherModeWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let title = format!("Select Matcher - {}", self.event.label());
        let dialog = DialogWidget::new(&title, self.theme).size(60, 16);
        let inner = dialog.inner_area(area);
        dialog.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        // Build options: "All tools" + each tool name
        let mut options = vec![("", "All tools".to_string())];
        for name in &self.state.tool_names {
            options.push((name.as_str(), name.clone()));
        }

        let items: Vec<ListItem> = options
            .iter()
            .enumerate()
            .map(|(i, (_, label))| {
                let is_selected = i == self.state.selected_index;
                let prefix = if is_selected { "❯ " } else { "  " };
                let style = if is_selected {
                    Style::default()
                        .fg(self.theme.primary)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                ListItem::new(Line::from(vec![
                    Span::raw(prefix.to_string()),
                    Span::styled(label.clone(), style),
                ]))
            })
            .collect();

        let list = List::new(items);
        Widget::render(list, inner, buf);
    }
}

// ===================================================================
// SelectHookMode Widget — from SelectHookMode.tsx
// ===================================================================

/// Widget for the hook selection mode.
pub struct SelectHookModeWidget<'a> {
    pub state: &'a HooksConfigMenuState,
    pub event: &'a HookEvent,
    pub matcher: &'a str,
    pub theme: &'a Theme,
}

impl<'a> SelectHookModeWidget<'a> {
    pub fn new(
        state: &'a HooksConfigMenuState,
        event: &'a HookEvent,
        matcher: &'a str,
        theme: &'a Theme,
    ) -> Self {
        Self {
            state,
            event,
            matcher,
            theme,
        }
    }
}

impl<'a> Widget for SelectHookModeWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let hooks = self.state.hooks_for_event_and_matcher(self.event, self.matcher);

        let subtitle = if self.matcher.is_empty() {
            self.event.label().to_string()
        } else {
            format!("{} · {}", self.event.label(), self.matcher)
        };
        let title = format!("Hooks - {}", subtitle);
        let dialog = DialogWidget::new(&title, self.theme).size(60, 16);
        let inner = dialog.inner_area(area);
        dialog.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        if hooks.is_empty() {
            Paragraph::new("No hooks configured for this event")
                .style(Style::default().fg(Color::DarkGray))
                .render(inner, buf);
            return;
        }

        let items: Vec<ListItem> = hooks
            .iter()
            .enumerate()
            .map(|(i, hook)| {
                let is_selected = i == self.state.selected_index;
                let prefix = if is_selected { "❯ " } else { "  " };
                let type_label = hook.hook_type.label();
                let content_preview: String = hook.content.chars().take(30).collect();
                let enabled_indicator = if hook.enabled { "" } else { " (disabled)" };

                let style = if is_selected {
                    Style::default()
                        .fg(self.theme.primary)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                let line = Line::from(vec![
                    Span::raw(prefix.to_string()),
                    Span::styled(format!("[{}] ", type_label), Style::default().fg(Color::DarkGray)),
                    Span::styled(content_preview, style),
                    Span::styled(
                        enabled_indicator.to_string(),
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
// ViewHookMode Widget — from ViewHookMode.tsx
// ===================================================================

/// Widget for viewing a single hook's details.
pub struct ViewHookModeWidget<'a> {
    pub hook: &'a IndividualHookConfig,
    pub theme: &'a Theme,
}

impl<'a> ViewHookModeWidget<'a> {
    pub fn new(hook: &'a IndividualHookConfig, theme: &'a Theme) -> Self {
        Self { hook, theme }
    }
}

impl<'a> Widget for ViewHookModeWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let dialog = DialogWidget::new("Hook details", self.theme).size(65, 18);
        let inner = dialog.inner_area(area);
        dialog.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let mut lines: Vec<Line> = Vec::new();

        // Type
        lines.push(Line::from(vec![
            Span::styled("Type: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(self.hook.hook_type.label()),
        ]));

        // Event
        lines.push(Line::from(vec![
            Span::styled("Event: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(self.hook.event.label()),
        ]));

        // Matcher (if set)
        if !self.hook.matcher.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("Matcher: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(&self.hook.matcher),
            ]));
        }

        // Enabled
        lines.push(Line::from(vec![
            Span::styled("Enabled: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(if self.hook.enabled { "Yes" } else { "No" }),
        ]));

        lines.push(Line::from(""));

        // Content field
        let field_label = self.hook.hook_type.content_field_label();
        lines.push(Line::from(Span::styled(
            format!("{}:", field_label),
            Style::default().fg(Color::DarkGray),
        )));

        // Content value in a bordered box
        let content_display: String = self.hook.content.chars().take(inner.width as usize - 4).collect();
        lines.push(Line::from(format!("  {}", content_display)));

        // Status message
        if let Some(ref status) = self.hook.status_message {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::raw("Status message: "),
                Span::styled(status.as_str(), Style::default().fg(Color::DarkGray)),
            ]));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "To modify or remove this hook, edit settings.json directly or ask Mossen to help.",
            Style::default().fg(Color::DarkGray),
        )));

        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .render(inner, buf);
    }
}

// ===================================================================
// PromptDialog Widget — from PromptDialog.tsx
// ===================================================================

/// State for the prompt dialog (hook with prompt type).
#[derive(Debug, Clone)]
pub struct HookPromptDialogState {
    pub prompt_text: String,
    pub cursor_pos: usize,
}

impl HookPromptDialogState {
    pub fn new() -> Self {
        Self {
            prompt_text: String::new(),
            cursor_pos: 0,
        }
    }

    pub fn insert_char(&mut self, c: char) {
        self.prompt_text.insert(self.cursor_pos, c);
        self.cursor_pos += 1;
    }

    pub fn delete_char(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
            self.prompt_text.remove(self.cursor_pos);
        }
    }

    pub fn move_left(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
        }
    }

    pub fn move_right(&mut self) {
        if self.cursor_pos < self.prompt_text.len() {
            self.cursor_pos += 1;
        }
    }
}

impl Default for HookPromptDialogState {
    fn default() -> Self {
        Self::new()
    }
}

/// Widget for the hook prompt dialog.
pub struct HookPromptDialogWidget<'a> {
    pub state: &'a HookPromptDialogState,
    pub theme: &'a Theme,
}

impl<'a> HookPromptDialogWidget<'a> {
    pub fn new(state: &'a HookPromptDialogState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for HookPromptDialogWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let dialog = DialogWidget::new("Enter prompt for hook", self.theme).size(60, 8);
        let inner = dialog.inner_area(area);
        dialog.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(1), Constraint::Min(0)])
            .split(inner);

        // Instructions
        Paragraph::new("Type your prompt and press Enter to confirm:")
            .style(Style::default().fg(Color::DarkGray))
            .render(chunks[0], buf);

        // Input
        let display: String = self.state.prompt_text.chars().take(inner.width as usize).collect();
        if display.is_empty() {
            buf.set_string(
                chunks[1].x,
                chunks[1].y,
                "Enter prompt...",
                Style::default().fg(Color::DarkGray),
            );
        } else {
            buf.set_string(
                chunks[1].x,
                chunks[1].y,
                &display,
                Style::default().fg(self.theme.text),
            );
        }
    }
}

// ===================================================================
// HooksConfigMenu Widget — from HooksConfigMenu.tsx (dispatcher)
// ===================================================================

/// Top-level widget for hooks configuration menu.
pub struct HooksConfigMenuWidget<'a> {
    pub state: &'a HooksConfigMenuState,
    pub theme: &'a Theme,
}

impl<'a> HooksConfigMenuWidget<'a> {
    pub fn new(state: &'a HooksConfigMenuState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for HooksConfigMenuWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        match &self.state.mode {
            HooksConfigMode::Disabled => {
                let dialog = DialogWidget::new("Hook Configuration - Disabled", self.theme).size(50, 8);
                let inner = dialog.inner_area(area);
                dialog.render(area, buf);
                if inner.width > 0 && inner.height > 0 {
                    Paragraph::new("Hooks are disabled by policy.")
                        .style(Style::default().fg(Color::DarkGray))
                        .render(inner, buf);
                }
            }
            HooksConfigMode::SelectEvent => {
                SelectEventModeWidget::new(self.state, self.theme).render(area, buf);
            }
            HooksConfigMode::SelectMatcher { event } => {
                let event_ref = event;
                SelectMatcherModeWidget::new(self.state, event_ref, self.theme).render(area, buf);
            }
            HooksConfigMode::SelectHook { event, matcher } => {
                let event_ref = event;
                SelectHookModeWidget::new(self.state, event_ref, matcher, self.theme)
                    .render(area, buf);
            }
            HooksConfigMode::ViewHook {
                event,
                matcher,
                hook_index,
            } => {
                let hooks = self.state.hooks_for_event_and_matcher(event, matcher);
                if let Some(hook) = hooks.get(*hook_index) {
                    ViewHookModeWidget::new(hook, self.theme).render(area, buf);
                }
            }
            HooksConfigMode::PromptDialog { .. } => {
                let prompt_state = HookPromptDialogState::new();
                HookPromptDialogWidget::new(&prompt_state, self.theme).render(area, buf);
            }
        }
    }
}

// ===================================================================
// Hooks config navigation widgets
// ===================================================================

#[derive(Debug, Clone, Default)]
pub struct HooksConfigMenu {
    pub events: Vec<String>,
    pub selected: usize,
}

#[derive(Debug, Clone, Default)]
pub struct ViewHookMode {
    pub event: String,
    pub matcher: String,
    pub hook_id: String,
}

pub fn get_hooks_docs_url() -> &'static str {
    "https://docs.mossen.dev/hooks"
}

/// Read-only copy shown when a hook config is sourced from managed settings.
pub fn get_hooks_readonly_copy(source: &str) -> String {
    format!(
        "Hooks are managed by {} and cannot be edited here.",
        source
    )
}

#[derive(Debug, Clone, Default)]
pub struct SelectEventMode {
    pub events: Vec<String>,
    pub selected: usize,
}

#[derive(Debug, Clone, Default)]
pub struct SelectMatcherMode {
    pub matchers: Vec<String>,
    pub selected: usize,
}

#[derive(Debug, Clone, Default)]
pub struct SelectHookMode {
    pub hooks: Vec<String>,
    pub selected: usize,
}
