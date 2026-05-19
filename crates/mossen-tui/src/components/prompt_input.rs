//! Prompt input components — full input area with footer, help, notifications.
//!
//! Translates: components/PromptInput/ (21 files) including the main input,
//! footer, suggestions, modes, history search, voice indicator, queued commands.

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget, Wrap},
};

use crate::theme::Theme;

// ===================================================================
// Input modes (inputModes.ts)
// ===================================================================

/// Input mode configuration (inputModes.ts).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptInputMode {
    Normal,
    Bash,
    Plan,
    Search,
    MultiLine,
}

impl PromptInputMode {
    pub fn prefix(&self) -> &'static str {
        match self {
            Self::Normal => "❯",
            Self::Bash => "!",
            Self::Plan => "📋",
            Self::Search => "🔍",
            Self::MultiLine => "...",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Bash => "bash",
            Self::Plan => "plan",
            Self::Search => "search",
            Self::MultiLine => "multi-line",
        }
    }

    pub fn placeholder(&self) -> &'static str {
        match self {
            Self::Normal => "Ask anything...",
            Self::Bash => "Enter shell command...",
            Self::Plan => "Describe your plan...",
            Self::Search => "Search messages...",
            Self::MultiLine => "Continue typing (Ctrl+D to send)...",
        }
    }
}

// ===================================================================
// Input paste handling (inputPaste.ts)
// ===================================================================

/// Paste detection and processing (inputPaste.ts).
#[derive(Debug, Clone)]
pub struct PasteState {
    pub is_paste_pending: bool,
    pub paste_content: String,
    pub paste_lines: usize,
    pub should_bracket: bool,
}

impl PasteState {
    pub fn new() -> Self {
        Self {
            is_paste_pending: false,
            paste_content: String::new(),
            paste_lines: 0,
            should_bracket: false,
        }
    }

    pub fn receive_paste(&mut self, content: &str) {
        self.paste_content = content.to_string();
        self.paste_lines = content.lines().count();
        self.should_bracket = self.paste_lines > 1;
        self.is_paste_pending = true;
    }

    pub fn accept(&mut self) -> String {
        self.is_paste_pending = false;
        std::mem::take(&mut self.paste_content)
    }

    pub fn reject(&mut self) {
        self.is_paste_pending = false;
        self.paste_content.clear();
        self.paste_lines = 0;
    }
}

impl Default for PasteState {
    fn default() -> Self {
        Self::new()
    }
}

// ===================================================================
// Truncation (useMaybeTruncateInput.ts)
// ===================================================================

/// Input truncation state (useMaybeTruncateInput.ts).
pub struct InputTruncation {
    pub max_display_lines: usize,
    pub max_display_chars: usize,
}

impl InputTruncation {
    pub fn new(max_lines: usize, max_chars: usize) -> Self {
        Self {
            max_display_lines: max_lines,
            max_display_chars: max_chars,
        }
    }

    pub fn should_truncate(&self, input: &str) -> bool {
        input.lines().count() > self.max_display_lines
            || input.len() > self.max_display_chars
    }

    pub fn truncated_display<'a>(&self, input: &'a str) -> (&'a str, bool) {
        if !self.should_truncate(input) {
            return (input, false);
        }
        let byte_limit = input
            .char_indices()
            .take(self.max_display_chars)
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(input.len());
        (&input[..byte_limit.min(input.len())], true)
    }
}

impl Default for InputTruncation {
    fn default() -> Self {
        Self::new(10, 2000)
    }
}

// ===================================================================
// Placeholder (usePromptInputPlaceholder.ts)
// ===================================================================

/// Dynamic placeholder text logic (usePromptInputPlaceholder.ts).
pub fn compute_placeholder(
    mode: PromptInputMode,
    is_streaming: bool,
    has_tasks: bool,
    agent_name: Option<&str>,
) -> String {
    if is_streaming {
        return "Waiting for response...".to_string();
    }
    if let Some(agent) = agent_name {
        return format!("Ask @{} anything...", agent);
    }
    if has_tasks {
        return "Continue or ask about tasks...".to_string();
    }
    mode.placeholder().to_string()
}

// ===================================================================
// Fast icon hint (useShowFastIconHint.ts)
// ===================================================================

/// Fast icon hint display logic (useShowFastIconHint.ts).
#[derive(Debug, Clone)]
pub struct FastIconHintState {
    pub show_hint: bool,
    pub hint_shown_count: usize,
    pub max_shows: usize,
}

impl FastIconHintState {
    pub fn new(max_shows: usize) -> Self {
        Self {
            show_hint: false,
            hint_shown_count: 0,
            max_shows,
        }
    }

    pub fn should_show(&self, fast_mode_enabled: bool) -> bool {
        !fast_mode_enabled && self.hint_shown_count < self.max_shows
    }

    pub fn mark_shown(&mut self) {
        self.hint_shown_count += 1;
        if self.hint_shown_count >= self.max_shows {
            self.show_hint = false;
        }
    }
}

// ===================================================================
// Swarm banner (useSwarmBanner.ts)
// ===================================================================

/// Swarm banner state (useSwarmBanner.ts).
#[derive(Debug, Clone)]
pub struct SwarmBannerState {
    pub active_agents: Vec<String>,
    pub show_banner: bool,
}

impl SwarmBannerState {
    pub fn new() -> Self {
        Self {
            active_agents: Vec::new(),
            show_banner: false,
        }
    }

    pub fn update_agents(&mut self, agents: Vec<String>) {
        self.show_banner = !agents.is_empty();
        self.active_agents = agents;
    }

    pub fn banner_text(&self) -> String {
        if self.active_agents.is_empty() {
            return String::new();
        }
        if self.active_agents.len() == 1 {
            format!("@{} is working", self.active_agents[0])
        } else {
            format!("{} delegates active", self.active_agents.len())
        }
    }
}

impl Default for SwarmBannerState {
    fn default() -> Self {
        Self::new()
    }
}

// ===================================================================
// PromptInput utilities (utils.ts)
// ===================================================================

/// Detect if input is a slash command.
pub fn is_slash_command(input: &str) -> bool {
    input.starts_with('/') && !input.starts_with("//")
}

/// Detect if input is a bash command (! prefix).
pub fn is_bash_command(input: &str) -> bool {
    input.starts_with('!') && !input.starts_with("!!")
}

/// Extract command name from slash input.
pub fn extract_command_name(input: &str) -> Option<&str> {
    if !is_slash_command(input) {
        return None;
    }
    let without_slash = &input[1..];
    without_slash.split_whitespace().next()
}

/// Trim and normalize input before submission.
pub fn normalize_input(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    trimmed.to_string()
}

// ===================================================================
// Widget: PromptInputModeIndicator (PromptInputModeIndicator.tsx)
// ===================================================================

pub struct PromptInputModeIndicatorWidget<'a> {
    pub mode: PromptInputMode,
    pub theme: &'a Theme,
}

impl<'a> PromptInputModeIndicatorWidget<'a> {
    pub fn new(mode: PromptInputMode, theme: &'a Theme) -> Self {
        Self { mode, theme }
    }
}

impl<'a> Widget for PromptInputModeIndicatorWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        if self.mode == PromptInputMode::Normal {
            return;
        }
        let label = self.mode.label();
        let style = Style::default()
            .fg(self.theme.primary)
            .add_modifier(Modifier::BOLD);
        let text = format!("[{}]", label);
        buf.set_string(area.x, area.y, &text, style);
    }
}

// ===================================================================
// Widget: PromptInputFooter (PromptInputFooter.tsx)
// ===================================================================

#[derive(Debug, Clone)]
pub struct PromptInputFooterData {
    pub model_name: Option<String>,
    pub access_policy: String,
    pub fast_mode: bool,
    pub thinking: bool,
    pub message_count: usize,
    pub cost: Option<f64>,
}

impl PromptInputFooterData {
    pub fn new() -> Self {
        Self {
            model_name: None,
            access_policy: "Supervised".into(),
            fast_mode: false,
            thinking: true,
            message_count: 0,
            cost: None,
        }
    }
}

impl Default for PromptInputFooterData {
    fn default() -> Self {
        Self::new()
    }
}

pub struct PromptInputFooterWidget<'a> {
    pub data: &'a PromptInputFooterData,
    pub theme: &'a Theme,
}

impl<'a> PromptInputFooterWidget<'a> {
    pub fn new(data: &'a PromptInputFooterData, theme: &'a Theme) -> Self {
        Self { data, theme }
    }
}

impl<'a> Widget for PromptInputFooterWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let bg = Style::default().bg(self.theme.surface);
        for x in area.x..area.x + area.width {
            buf.set_string(x, area.y, " ", bg);
        }

        let mut left_spans: Vec<Span> = Vec::new();
        if let Some(ref model) = self.data.model_name {
            left_spans.push(Span::styled(
                format!(" {} ", model),
                Style::default().fg(self.theme.primary).bg(self.theme.surface),
            ));
        }
        left_spans.push(Span::styled(
            format!(" {} ", self.data.access_policy),
            Style::default().fg(self.theme.text_dim).bg(self.theme.surface),
        ));
        if self.data.fast_mode {
            left_spans.push(Span::styled(" ⚡ ", Style::default().fg(self.theme.warning).bg(self.theme.surface)));
        }
        if self.data.thinking {
            left_spans.push(Span::styled(" 💭 ", Style::default().fg(self.theme.secondary).bg(self.theme.surface)));
        }
        buf.set_line(area.x, area.y, &Line::from(left_spans), area.width / 2);

        let mut right_parts: Vec<String> = Vec::new();
        if let Some(cost) = self.data.cost {
            right_parts.push(format!("${:.2}", cost));
        }
        right_parts.push(format!("{} msgs", self.data.message_count));
        let right_text = right_parts.join("  ");
        let right_x = area.x + area.width.saturating_sub(right_text.len() as u16 + 1);
        buf.set_string(right_x, area.y, &right_text, Style::default().fg(self.theme.text_dim).bg(self.theme.surface));
    }
}

// ===================================================================
// Widget: PromptInputFooterLeftSide (PromptInputFooterLeftSide.tsx)
// ===================================================================

pub struct PromptInputFooterLeftSideWidget<'a> {
    pub mode: PromptInputMode,
    pub theme: &'a Theme,
}

impl<'a> PromptInputFooterLeftSideWidget<'a> {
    pub fn new(mode: PromptInputMode, theme: &'a Theme) -> Self {
        Self { mode, theme }
    }
}

impl<'a> Widget for PromptInputFooterLeftSideWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let hint = match self.mode {
            PromptInputMode::Normal => "Enter to send · /help for commands",
            PromptInputMode::Bash => "Enter to execute · Esc to cancel",
            PromptInputMode::Plan => "Enter to submit plan · Esc to cancel",
            PromptInputMode::Search => "Enter to search · Esc to cancel",
            PromptInputMode::MultiLine => "Ctrl+D to send · Esc to cancel",
        };
        buf.set_string(area.x, area.y, hint, Style::default().fg(self.theme.text_dim));
    }
}

// ===================================================================
// Widget: PromptInputFooterSuggestions (PromptInputFooterSuggestions.tsx)
// ===================================================================

#[derive(Debug, Clone)]
pub struct FooterSuggestion {
    pub label: String,
    pub shortcut: Option<String>,
}

pub struct PromptInputFooterSuggestionsWidget<'a> {
    pub suggestions: &'a [FooterSuggestion],
    pub theme: &'a Theme,
}

impl<'a> PromptInputFooterSuggestionsWidget<'a> {
    pub fn new(suggestions: &'a [FooterSuggestion], theme: &'a Theme) -> Self {
        Self { suggestions, theme }
    }
}

impl<'a> Widget for PromptInputFooterSuggestionsWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 || self.suggestions.is_empty() {
            return;
        }
        let mut x = area.x;
        for suggestion in self.suggestions {
            if x >= area.x + area.width {
                break;
            }
            if let Some(ref shortcut) = suggestion.shortcut {
                let shortcut_text = format!("[{}]", shortcut);
                buf.set_string(x, area.y, &shortcut_text, Style::default().fg(self.theme.text_dim).add_modifier(Modifier::BOLD));
                x += shortcut_text.len() as u16 + 1;
            }
            buf.set_string(x, area.y, &suggestion.label, Style::default().fg(self.theme.text_dim));
            x += suggestion.label.len() as u16 + 2;
        }
    }
}

// ===================================================================
// Widget: PromptInputHelpMenu (PromptInputHelpMenu.tsx)
// ===================================================================

#[derive(Debug, Clone)]
pub struct HelpMenuItem {
    pub command: String,
    pub description: String,
    pub shortcut: Option<String>,
}

pub struct PromptInputHelpMenuWidget<'a> {
    pub items: &'a [HelpMenuItem],
    pub selected: usize,
    pub theme: &'a Theme,
}

impl<'a> PromptInputHelpMenuWidget<'a> {
    pub fn new(items: &'a [HelpMenuItem], theme: &'a Theme) -> Self {
        Self { items, selected: 0, theme }
    }

    pub fn selected(mut self, idx: usize) -> Self {
        self.selected = idx;
        self
    }
}

impl<'a> Widget for PromptInputHelpMenuWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 || self.items.is_empty() {
            return;
        }
        for (i, item) in self.items.iter().enumerate() {
            let y = area.y + i as u16;
            if y >= area.y + area.height {
                break;
            }
            let is_sel = i == self.selected;
            let bg = if is_sel { self.theme.selection } else { Color::Reset };
            for x in area.x..area.x + area.width {
                buf.set_string(x, y, " ", Style::default().bg(bg));
            }
            let mut spans = vec![
                Span::styled(
                    if is_sel { "▸ " } else { "  " },
                    Style::default().fg(self.theme.primary).bg(bg),
                ),
                Span::styled(
                    format!("/{}", item.command),
                    Style::default().fg(self.theme.info).bg(bg).add_modifier(if is_sel { Modifier::BOLD } else { Modifier::empty() }),
                ),
            ];
            if let Some(ref shortcut) = item.shortcut {
                spans.push(Span::styled(format!("  [{}]", shortcut), Style::default().fg(self.theme.text_dim).bg(bg)));
            }
            spans.push(Span::styled(format!("  {}", item.description), Style::default().fg(self.theme.text_dim).bg(bg)));
            buf.set_line(area.x, y, &Line::from(spans), area.width);
        }
    }
}

// ===================================================================
// Widget: PromptInputQueuedCommands (PromptInputQueuedCommands.tsx)
// ===================================================================

pub struct PromptInputQueuedCommandsWidget<'a> {
    pub commands: &'a [String],
    pub theme: &'a Theme,
}

impl<'a> PromptInputQueuedCommandsWidget<'a> {
    pub fn new(commands: &'a [String], theme: &'a Theme) -> Self {
        Self { commands, theme }
    }
}

impl<'a> Widget for PromptInputQueuedCommandsWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 || self.commands.is_empty() {
            return;
        }
        let header = Line::from(vec![
            Span::styled("📋 ", Style::default().fg(self.theme.info)),
            Span::styled(
                format!("{} queued command{}", self.commands.len(), if self.commands.len() != 1 { "s" } else { "" }),
                Style::default().fg(self.theme.info),
            ),
        ]);
        buf.set_line(area.x, area.y, &header, area.width);

        for (i, cmd) in self.commands.iter().enumerate() {
            let y = area.y + 1 + i as u16;
            if y >= area.y + area.height {
                break;
            }
            let line = Line::from(vec![
                Span::styled(format!("  {}. ", i + 1), Style::default().fg(self.theme.text_dim)),
                Span::styled(cmd.as_str(), Style::default().fg(self.theme.text)),
            ]);
            buf.set_line(area.x, y, &line, area.width);
        }
    }
}

// ===================================================================
// Widget: PromptInputStashNotice (PromptInputStashNotice.tsx)
// ===================================================================

pub struct PromptInputStashNoticeWidget<'a> {
    pub stashed_input: &'a str,
    pub theme: &'a Theme,
}

impl<'a> PromptInputStashNoticeWidget<'a> {
    pub fn new(stashed_input: &'a str, theme: &'a Theme) -> Self {
        Self { stashed_input, theme }
    }
}

impl<'a> Widget for PromptInputStashNoticeWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 || self.stashed_input.is_empty() {
            return;
        }
        let preview: String = self.stashed_input.chars().take(30).collect();
        let line = Line::from(vec![
            Span::styled("📌 ", Style::default().fg(self.theme.text_dim)),
            Span::styled("Stashed: ", Style::default().fg(self.theme.text_dim)),
            Span::styled(
                if self.stashed_input.len() > 30 { format!("{}...", preview) } else { preview },
                Style::default().fg(self.theme.text),
            ),
        ]);
        buf.set_line(area.x, area.y, &line, area.width);
    }
}

// ===================================================================
// Widget: IssueFlagBanner (IssueFlagBanner.tsx)
// ===================================================================

pub struct IssueFlagBannerWidget<'a> {
    pub issue_text: &'a str,
    pub theme: &'a Theme,
}

impl<'a> IssueFlagBannerWidget<'a> {
    pub fn new(issue_text: &'a str, theme: &'a Theme) -> Self {
        Self { issue_text, theme }
    }
}

impl<'a> Widget for IssueFlagBannerWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let line = Line::from(vec![
            Span::styled("⚠ ", Style::default().fg(self.theme.warning)),
            Span::styled(self.issue_text, Style::default().fg(self.theme.warning)),
        ]);
        buf.set_line(area.x, area.y, &line, area.width);
    }
}

// ===================================================================
// Widget: Notifications (Notifications.tsx in PromptInput dir)
// ===================================================================

#[derive(Debug, Clone)]
pub struct PromptNotification {
    pub message: String,
    pub level: PromptNotificationLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptNotificationLevel {
    Info,
    Warning,
    Error,
}

pub struct PromptNotificationsWidget<'a> {
    pub notifications: &'a [PromptNotification],
    pub theme: &'a Theme,
}

impl<'a> PromptNotificationsWidget<'a> {
    pub fn new(notifications: &'a [PromptNotification], theme: &'a Theme) -> Self {
        Self { notifications, theme }
    }
}

impl<'a> Widget for PromptNotificationsWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 || self.notifications.is_empty() {
            return;
        }
        for (i, notif) in self.notifications.iter().enumerate() {
            let y = area.y + i as u16;
            if y >= area.y + area.height {
                break;
            }
            let (icon, color) = match notif.level {
                PromptNotificationLevel::Info => ("ℹ", self.theme.info),
                PromptNotificationLevel::Warning => ("⚠", self.theme.warning),
                PromptNotificationLevel::Error => ("✗", self.theme.error),
            };
            let line = Line::from(vec![
                Span::styled(format!("{} ", icon), Style::default().fg(color)),
                Span::styled(&notif.message, Style::default().fg(color)),
            ]);
            buf.set_line(area.x, y, &line, area.width);
        }
    }
}

// ===================================================================
// Widget: SandboxPromptFooterHint (SandboxPromptFooterHint.tsx)
// ===================================================================

pub struct SandboxPromptFooterHintWidget<'a> {
    pub sandbox_enabled: bool,
    pub theme: &'a Theme,
}

impl<'a> SandboxPromptFooterHintWidget<'a> {
    pub fn new(sandbox_enabled: bool, theme: &'a Theme) -> Self {
        Self { sandbox_enabled, theme }
    }
}

impl<'a> Widget for SandboxPromptFooterHintWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        if self.sandbox_enabled {
            let line = Line::from(vec![
                Span::styled("🔒 ", Style::default().fg(self.theme.success)),
                Span::styled("Sandbox active", Style::default().fg(self.theme.success)),
            ]);
            buf.set_line(area.x, area.y, &line, area.width);
        }
    }
}

// ===================================================================
// Widget: ShimmeredInput (ShimmeredInput.tsx)
// ===================================================================

pub struct ShimmeredInputWidget<'a> {
    pub text: &'a str,
    pub shimmer_offset: usize,
    pub theme: &'a Theme,
}

impl<'a> ShimmeredInputWidget<'a> {
    pub fn new(text: &'a str, shimmer_offset: usize, theme: &'a Theme) -> Self {
        Self { text, shimmer_offset, theme }
    }
}

impl<'a> Widget for ShimmeredInputWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 || self.text.is_empty() {
            return;
        }
        let shimmer_colors: &[Color] = &[
            Color::Rgb(60, 60, 90),
            Color::Rgb(80, 80, 120),
            Color::Rgb(100, 110, 160),
            Color::Rgb(130, 140, 200),
            Color::Rgb(160, 170, 230),
            Color::Rgb(130, 140, 200),
            Color::Rgb(100, 110, 160),
            Color::Rgb(80, 80, 120),
        ];
        let chars: Vec<char> = self.text.chars().collect();
        let mut x = area.x;
        for (i, ch) in chars.iter().enumerate() {
            if x >= area.x + area.width {
                break;
            }
            let color_idx = (i + self.shimmer_offset) % shimmer_colors.len();
            let style = Style::default().fg(shimmer_colors[color_idx]);
            buf.set_string(x, area.y, &ch.to_string(), style);
            x += 1;
        }
    }
}

// ===================================================================
// Widget: VoiceIndicator (VoiceIndicator.tsx)
// ===================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceState {
    Idle,
    Listening,
    Processing,
    Error,
}

pub struct VoiceIndicatorWidget<'a> {
    pub state: VoiceState,
    pub theme: &'a Theme,
}

impl<'a> VoiceIndicatorWidget<'a> {
    pub fn new(state: VoiceState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for VoiceIndicatorWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let (icon, label, color) = match self.state {
            VoiceState::Idle => ("🎤", "Voice", self.theme.text_dim),
            VoiceState::Listening => ("🔴", "Listening...", self.theme.error),
            VoiceState::Processing => ("⏺", "Processing...", self.theme.warning),
            VoiceState::Error => ("✗", "Voice error", self.theme.error),
        };
        let line = Line::from(vec![
            Span::styled(format!("{} ", icon), Style::default().fg(color)),
            Span::styled(label, Style::default().fg(color)),
        ]);
        buf.set_line(area.x, area.y, &line, area.width);
    }
}

// ===================================================================
// Widget: HistorySearchInput (HistorySearchInput.tsx)
// ===================================================================

#[derive(Debug, Clone)]
pub struct HistorySearchInputState {
    pub query: String,
    pub results: Vec<HistorySearchResult>,
    pub selected: usize,
    pub is_active: bool,
}

#[derive(Debug, Clone)]
pub struct HistorySearchResult {
    pub content: String,
    pub timestamp: String,
    pub session_id: String,
}

impl HistorySearchInputState {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            results: Vec::new(),
            selected: 0,
            is_active: false,
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

    pub fn selected_result(&self) -> Option<&HistorySearchResult> {
        self.results.get(self.selected)
    }
}

impl Default for HistorySearchInputState {
    fn default() -> Self {
        Self::new()
    }
}

pub struct HistorySearchInputWidget<'a> {
    pub state: &'a HistorySearchInputState,
    pub theme: &'a Theme,
}

impl<'a> HistorySearchInputWidget<'a> {
    pub fn new(state: &'a HistorySearchInputState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for HistorySearchInputWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height < 2 {
            return;
        }
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .split(area);

        let input_line = Line::from(vec![
            Span::styled("🔍 ", Style::default().fg(self.theme.info)),
            Span::styled(
                if self.state.query.is_empty() { "Search history..." } else { &self.state.query },
                Style::default().fg(if self.state.query.is_empty() { self.theme.text_dim } else { self.theme.text }),
            ),
        ]);
        buf.set_line(chunks[0].x, chunks[0].y, &input_line, chunks[0].width);

        for (i, result) in self.state.results.iter().enumerate() {
            let y = chunks[1].y + i as u16;
            if y >= chunks[1].y + chunks[1].height {
                break;
            }
            let is_sel = i == self.state.selected;
            let bg = if is_sel { self.theme.selection } else { Color::Reset };
            for x in chunks[1].x..chunks[1].x + chunks[1].width {
                buf.set_string(x, y, " ", Style::default().bg(bg));
            }
            let preview: String = result.content.chars().take(50).collect();
            let line = Line::from(vec![
                Span::styled(
                    if is_sel { "▸ " } else { "  " },
                    Style::default().fg(self.theme.primary).bg(bg),
                ),
                Span::styled(
                    preview,
                    Style::default().fg(self.theme.text).bg(bg).add_modifier(if is_sel { Modifier::BOLD } else { Modifier::empty() }),
                ),
                Span::styled(format!("  {}", result.timestamp), Style::default().fg(self.theme.text_dim).bg(bg)),
            ]);
            buf.set_line(chunks[1].x, y, &line, chunks[1].width);
        }
    }
}

// ===================================================================
// PromptInput supplementary widgets
// ===================================================================

/// Highlighted input — same source text with positional highlight runs.
#[derive(Debug, Clone, Default)]
pub struct HighlightedInput {
    pub source: String,
    pub highlight_runs: Vec<(usize, usize)>, // (start, end) byte offsets
}

/// Swarm banner hook state — toggles banner on group-chat sessions.
#[derive(Debug, Clone, Default)]
pub struct SwarmBannerInfo {
    pub visible: bool,
    pub members: Vec<String>,
}

/// Hook-equivalent useSwarmBanner.
pub fn use_swarm_banner(state: &mut SwarmBannerInfo) -> &mut SwarmBannerInfo {
    state
}

#[derive(Debug, Clone, Default)]
pub struct PromptInputFooterLeftSide {
    pub status: String,
    pub mode: String,
}

#[derive(Debug, Clone, Default)]
pub struct PromptInputStashNotice {
    pub stash_count: usize,
}

// ===================================================================
// inputModes.ts
// ===================================================================

/// Map a mode character to mode name.
pub fn get_mode_from_input(input: &str) -> Option<&'static str> {
    let first = input.chars().next()?;
    match first {
        '/' => Some("command"),
        '!' => Some("shell"),
        '@' => Some("teammate"),
        '#' => Some("memory"),
        '?' => Some("help"),
        _ => None,
    }
}

/// Strip the mode prefix character from the input.
pub fn get_value_from_input(input: &str) -> String {
    if get_mode_from_input(input).is_some() {
        input.chars().skip(1).collect()
    } else {
        input.to_string()
    }
}

/// Whether the leading character of input is a mode prefix.
pub fn is_input_mode_character(c: char) -> bool {
    matches!(c, '/' | '!' | '@' | '#' | '?')
}

/// Prepend the mode-character to the input for the given mode.
pub fn prepend_mode_character_to_input(mode: &str, value: &str) -> String {
    let c = match mode {
        "command" => '/',
        "shell" => '!',
        "teammate" => '@',
        "memory" => '#',
        "help" => '?',
        _ => return value.to_string(),
    };
    let mut s = String::with_capacity(value.len() + 1);
    s.push(c);
    s.push_str(value);
    s
}

// ===================================================================
// usePromptInputPlaceholder.ts / useShowFastIconHint.ts
// ===================================================================

/// Return the placeholder for the current mode.
pub fn use_prompt_input_placeholder(mode: &str, idle_seconds: u64) -> String {
    match mode {
        "command" => "Type a command".into(),
        "shell" => "Run a shell command".into(),
        "teammate" => "Message a teammate".into(),
        "memory" => "Save to memory".into(),
        "help" => "Ask a help question".into(),
        _ if idle_seconds > 60 => "Pick up where you left off…".into(),
        _ => "Try \"build a calculator app\"…".into(),
    }
}

/// Whether to show the fast-icon hint right now.
pub fn use_show_fast_icon_hint(seen_count: u32, fast_session: bool) -> bool {
    fast_session && seen_count < 3
}

#[derive(Debug, Clone, Default)]
pub struct SandboxPromptFooterHint {
    pub message: String,
}

// ===================================================================
// PromptInput utils.ts
// ===================================================================

/// Whether vim mode is enabled.
pub fn is_vim_mode_enabled(settings: &serde_json::Value) -> bool {
    settings
        .get("vimMode")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Help text shown next to newline keybindings.
pub fn get_newline_instructions(vim_mode: bool) -> String {
    if vim_mode {
        "j: line, Esc: command".into()
    } else {
        "↵: send · Shift+↵: newline".into()
    }
}

/// Whether a character is a non-space printable character.
pub fn is_non_space_printable(c: char) -> bool {
    !c.is_whitespace() && !c.is_control()
}

#[derive(Debug, Clone, Default)]
pub struct MaybeTruncateInputState {
    pub original_len: usize,
    pub truncated: bool,
    pub displayed: String,
}

/// Hook-equivalent useMaybeTruncateInput — truncate excessively long input.
pub fn use_maybe_truncate_input(input: &str, max_len: usize) -> MaybeTruncateInputState {
    if input.len() <= max_len {
        MaybeTruncateInputState {
            original_len: input.len(),
            truncated: false,
            displayed: input.to_string(),
        }
    } else {
        let mut displayed: String = input.chars().take(max_len.saturating_sub(3)).collect();
        displayed.push_str("...");
        MaybeTruncateInputState {
            original_len: input.len(),
            truncated: true,
            displayed,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct IssueFlagBanner {
    pub flag_name: String,
    pub url: String,
}

#[derive(Debug, Clone, Default)]
pub struct PromptInputQueuedCommands {
    pub queue: Vec<String>,
    pub cursor: usize,
}

#[derive(Debug, Clone, Default)]
pub struct PromptInputHelpMenu {
    pub items: Vec<String>,
    pub selected: usize,
}

// ===================================================================
// Notifications.tsx
// ===================================================================

/// Default timeout (ms) for the temporary footer status.
pub const FOOTER_TEMPORARY_STATUS_TIMEOUT: u64 = 4000;

/// Build the auth-status notice for the footer.
pub fn get_auth_status_notice(authed: bool, source: &str) -> String {
    if authed {
        format!("Authenticated ({})", source)
    } else {
        "Not authenticated — run `mossen login`".to_string()
    }
}

#[derive(Debug, Clone, Default)]
pub struct Notifications {
    pub items: Vec<String>,
    pub temporary_until_ms: u64,
}

// ===================================================================
// PromptInputFooterSuggestions.tsx
// ===================================================================

/// Maximum suggestions shown in the overlay.
pub const OVERLAY_MAX_ITEMS: usize = 8;

/// One suggestion item in the overlay.
#[derive(Debug, Clone, Default)]
pub struct SuggestionItem {
    pub label: String,
    pub kind: String, // "command" | "file" | "teammate" | "skill"
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct PromptInputFooterSuggestions {
    pub items: Vec<SuggestionItem>,
    pub selected: usize,
}

#[derive(Debug, Clone, Default)]
pub struct PromptInputModeIndicator {
    pub mode: String,
    pub label: String,
}

// ===================================================================
// inputPaste.ts
// ===================================================================

/// Truncate a pasted message for display in the prompt input.
pub fn maybe_truncate_message_for_input(message: &str, max_len: usize) -> (String, bool) {
    if message.len() <= max_len {
        (message.to_string(), false)
    } else {
        let mut s: String = message.chars().take(max_len.saturating_sub(20)).collect();
        s.push_str(&format!("…[+{} chars]", message.len() - s.len()));
        (s, true)
    }
}

/// Wrapper used by paste handlers — same shape as the hook version.
pub fn maybe_truncate_input(input: &str, max_len: usize) -> String {
    maybe_truncate_message_for_input(input, max_len).0
}

#[derive(Debug, Clone, Default)]
pub struct VoiceIndicator {
    pub state: String, // "idle" | "listening" | "transcribing" | "error"
    pub level: f32,
}

#[derive(Debug, Clone, Default)]
pub struct VoiceWarmupHint {
    pub visible: bool,
    pub message: String,
}

/// Suggestion category for the prompt-input footer — mirrors TS
/// `type SuggestionType = 'command' | 'file' | 'directory' | 'agent' | 'shell'
/// | 'custom-title' | 'slack-channel' | 'none'`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestionType {
    Command,
    File,
    Directory,
    Agent,
    Shell,
    CustomTitle,
    SlackChannel,
    None,
}
