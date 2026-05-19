//! Design system components — base widget library.
//!
//! Translates components/design-system/ (16 files) into reusable Rust widgets:
//! Dialog, FuzzyPicker, Tabs, ThemedBox, ThemedText, Divider, ProgressBar,
//! ListItem, StatusIcon, KeyboardShortcutHint, LoadingState, etc.

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Gauge, Paragraph, Widget, Wrap},
};

use crate::theme::Theme;

// --- Dialog widget (Dialog.tsx) ---

/// A modal dialog container with border and title.
pub struct DialogWidget<'a> {
    pub title: &'a str,
    pub theme: &'a Theme,
    pub width: u16,
    pub height: u16,
}

impl<'a> DialogWidget<'a> {
    pub fn new(title: &'a str, theme: &'a Theme) -> Self {
        Self {
            title,
            theme,
            width: 60,
            height: 20,
        }
    }

    pub fn size(mut self, width: u16, height: u16) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    /// Get the inner content area (inside borders).
    pub fn inner_area(&self, area: Rect) -> Rect {
        let dialog_area = crate::layout::center(area, self.width, self.height);
        Rect::new(
            dialog_area.x + 2,
            dialog_area.y + 1,
            dialog_area.width.saturating_sub(4),
            dialog_area.height.saturating_sub(2),
        )
    }
}

impl<'a> Widget for DialogWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let dialog_area = crate::layout::center(area, self.width, self.height);

        // Clear background
        Clear.render(dialog_area, buf);

        // Render bordered block
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.theme.style_border_focused())
            .title(Span::styled(
                self.title,
                Style::default()
                    .fg(self.theme.primary)
                    .add_modifier(Modifier::BOLD),
            ));
        block.render(dialog_area, buf);
    }
}

// --- Divider widget (Divider.tsx) ---

/// A horizontal divider line.
pub struct DividerWidget<'a> {
    pub label: Option<&'a str>,
    pub style: Style,
}

impl<'a> DividerWidget<'a> {
    pub fn new() -> Self {
        Self {
            label: None,
            style: Style::default().fg(Color::DarkGray),
        }
    }

    pub fn label(mut self, label: &'a str) -> Self {
        self.label = Some(label);
        self
    }

    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }
}

impl<'a> Default for DividerWidget<'a> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> Widget for DividerWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        if let Some(label) = self.label {
            let label_len = label.len() as u16;
            let left_len = (area.width.saturating_sub(label_len + 2)) / 2;
            let right_len = area.width.saturating_sub(left_len + label_len + 2);

            let line = Line::from(vec![
                Span::styled("─".repeat(left_len as usize), self.style),
                Span::styled(format!(" {} ", label), self.style),
                Span::styled("─".repeat(right_len as usize), self.style),
            ]);
            buf.set_line(area.x, area.y, &line, area.width);
        } else {
            let divider = "─".repeat(area.width as usize);
            buf.set_string(area.x, area.y, &divider, self.style);
        }
    }
}

// --- ProgressBar widget (ProgressBar.tsx) ---

/// A progress bar widget.
pub struct ProgressBarWidget<'a> {
    pub progress: f64,
    pub label: Option<&'a str>,
    pub theme: &'a Theme,
}

impl<'a> ProgressBarWidget<'a> {
    pub fn new(progress: f64, theme: &'a Theme) -> Self {
        Self {
            progress: progress.clamp(0.0, 1.0),
            label: None,
            theme,
        }
    }

    pub fn label(mut self, label: &'a str) -> Self {
        self.label = Some(label);
        self
    }
}

impl<'a> Widget for ProgressBarWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let gauge = Gauge::default()
            .gauge_style(
                Style::default()
                    .fg(self.theme.primary)
                    .bg(self.theme.surface),
            )
            .ratio(self.progress);

        gauge.render(area, buf);

        // Overlay label if provided
        if let Some(label) = self.label {
            let label_x = area.x + (area.width.saturating_sub(label.len() as u16)) / 2;
            buf.set_string(label_x, area.y, label, Style::default().fg(self.theme.text));
        }
    }
}

// --- ThemedBox widget (ThemedBox.tsx) ---

/// A themed bordered container.
pub struct ThemedBoxWidget<'a> {
    pub title: Option<&'a str>,
    pub theme: &'a Theme,
    pub focused: bool,
}

impl<'a> ThemedBoxWidget<'a> {
    pub fn new(theme: &'a Theme) -> Self {
        Self {
            title: None,
            theme,
            focused: false,
        }
    }

    pub fn title(mut self, title: &'a str) -> Self {
        self.title = Some(title);
        self
    }

    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    pub fn block(&self) -> Block<'a> {
        let border_style = if self.focused {
            self.theme.style_border_focused()
        } else {
            self.theme.style_border()
        };

        let mut block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style);

        if let Some(title) = self.title {
            block = block.title(Span::styled(title, Style::default().fg(self.theme.primary)));
        }

        block
    }
}

// --- StatusIcon widget (StatusIcon.tsx) ---

/// Status indicator icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusLevel {
    Success,
    Warning,
    Error,
    Info,
    Loading,
}

pub struct StatusIconWidget {
    pub level: StatusLevel,
    pub style: Style,
}

impl StatusIconWidget {
    pub fn new(level: StatusLevel, theme: &Theme) -> Self {
        let style = match level {
            StatusLevel::Success => Style::default().fg(theme.success),
            StatusLevel::Warning => Style::default().fg(theme.warning),
            StatusLevel::Error => Style::default().fg(theme.error),
            StatusLevel::Info => Style::default().fg(theme.info),
            StatusLevel::Loading => Style::default().fg(theme.spinner_primary),
        };
        Self { level, style }
    }

    pub fn icon(&self) -> &'static str {
        match self.level {
            StatusLevel::Success => "✓",
            StatusLevel::Warning => "⚠",
            StatusLevel::Error => "✗",
            StatusLevel::Info => "ℹ",
            StatusLevel::Loading => "⏺",
        }
    }
}

impl Widget for StatusIconWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width > 0 && area.height > 0 {
            buf.set_string(area.x, area.y, self.icon(), self.style);
        }
    }
}

// --- KeyboardShortcutHint widget (KeyboardShortcutHint.tsx) ---

/// Renders a keyboard shortcut hint like "[Ctrl+C] cancel".
pub struct ShortcutHintWidget<'a> {
    pub key: &'a str,
    pub description: &'a str,
    pub theme: &'a Theme,
}

impl<'a> ShortcutHintWidget<'a> {
    pub fn new(key: &'a str, description: &'a str, theme: &'a Theme) -> Self {
        Self {
            key,
            description,
            theme,
        }
    }
}

impl<'a> Widget for ShortcutHintWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let line = Line::from(vec![
            Span::styled(
                format!("[{}]", self.key),
                Style::default()
                    .fg(self.theme.text_dim)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(self.description, Style::default().fg(self.theme.text_dim)),
        ]);
        buf.set_line(area.x, area.y, &line, area.width);
    }
}

// --- ListItem widget (ListItem.tsx) ---

/// A selectable list item with icon and description.
pub struct ListItemWidget<'a> {
    pub label: &'a str,
    pub description: Option<&'a str>,
    pub icon: Option<&'a str>,
    pub selected: bool,
    pub theme: &'a Theme,
}

impl<'a> ListItemWidget<'a> {
    pub fn new(label: &'a str, theme: &'a Theme) -> Self {
        Self {
            label,
            description: None,
            icon: None,
            selected: false,
            theme,
        }
    }

    pub fn description(mut self, desc: &'a str) -> Self {
        self.description = Some(desc);
        self
    }

    pub fn icon(mut self, icon: &'a str) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
}

impl<'a> Widget for ListItemWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let (fg, bg) = if self.selected {
            (self.theme.text, self.theme.selection)
        } else {
            (self.theme.text, Color::Reset)
        };

        // Clear line with background
        let bg_style = Style::default().bg(bg);
        for x in area.x..area.x + area.width {
            buf.set_string(x, area.y, " ", bg_style);
        }

        let mut x = area.x;

        // Selection indicator
        if self.selected {
            buf.set_string(
                x,
                area.y,
                "▸",
                Style::default().fg(self.theme.primary).bg(bg),
            );
        }
        x += 2;

        // Icon
        if let Some(icon) = self.icon {
            buf.set_string(x, area.y, icon, Style::default().fg(fg).bg(bg));
            x += 2;
        }

        // Label
        let label_style = Style::default().fg(fg).bg(bg);
        if self.selected {
            buf.set_string(
                x,
                area.y,
                self.label,
                label_style.add_modifier(Modifier::BOLD),
            );
        } else {
            buf.set_string(x, area.y, self.label, label_style);
        }
        x += self.label.len() as u16 + 1;

        // Description
        if let Some(desc) = self.description {
            if x < area.x + area.width {
                let desc_style = Style::default().fg(self.theme.text_dim).bg(bg);
                let avail = (area.x + area.width - x) as usize;
                let truncated: String = desc.chars().take(avail).collect();
                buf.set_string(x, area.y, &truncated, desc_style);
            }
        }
    }
}

// ===================================================================
// Design system primitives
// ===================================================================

/// Context propagating the hover colour for ThemedText.
#[derive(Debug, Clone, Copy, Default)]
pub struct TextHoverColorContext {
    pub hover_color: Option<ratatui::style::Color>,
}

/// FuzzyPicker — fuzzy-search a list of items.
#[derive(Debug, Clone, Default)]
pub struct FuzzyPicker {
    pub items: Vec<String>,
    pub query: String,
    pub selected_index: usize,
    pub filtered: Vec<usize>,
}

impl FuzzyPicker {
    pub fn new(items: Vec<String>) -> Self {
        let n = items.len();
        Self {
            items,
            query: String::new(),
            selected_index: 0,
            filtered: (0..n).collect(),
        }
    }
    pub fn set_query(&mut self, q: &str) {
        self.query = q.to_string();
        self.filtered = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, s)| fuzzy_match(s, q))
            .map(|(i, _)| i)
            .collect();
        self.selected_index = 0;
    }
}

fn fuzzy_match(s: &str, q: &str) -> bool {
    if q.is_empty() {
        return true;
    }
    let mut si = s.chars();
    for c in q.chars() {
        let target = c.to_ascii_lowercase();
        let found = si.any(|sc| sc.to_ascii_lowercase() == target);
        if !found {
            return false;
        }
    }
    true
}

// ===================================================================
// Tabs
// ===================================================================

#[derive(Debug, Clone)]
pub struct Tab {
    pub key: String,
    pub label: String,
    pub badge: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Tabs {
    pub tabs: Vec<Tab>,
    pub selected_index: usize,
    pub header_focused: bool,
}

impl Tabs {
    pub fn new(tabs: Vec<Tab>) -> Self {
        Self {
            tabs,
            selected_index: 0,
            header_focused: false,
        }
    }
    pub fn next(&mut self) {
        if !self.tabs.is_empty() {
            self.selected_index = (self.selected_index + 1) % self.tabs.len();
        }
    }
    pub fn prev(&mut self) {
        if !self.tabs.is_empty() {
            self.selected_index =
                (self.selected_index + self.tabs.len() - 1) % self.tabs.len();
        }
    }
}

/// Hook-equivalent: total width consumed by tab headers.
pub fn use_tabs_width(tabs: &Tabs) -> u16 {
    tabs.tabs
        .iter()
        .map(|t| (t.label.len() as u16) + 4 + t.badge.as_ref().map(|b| b.len() as u16 + 2).unwrap_or(0))
        .sum()
}

/// Hook-equivalent: whether the tab header has focus right now.
pub fn use_tab_header_focus(tabs: &Tabs) -> bool {
    tabs.header_focused
}

// ===================================================================
// Pane / Divider / ProgressBar
// ===================================================================

#[derive(Debug, Clone)]
pub struct Pane {
    pub title: String,
    pub border: bool,
    pub padded: bool,
}

impl Default for Pane {
    fn default() -> Self {
        Self {
            title: String::new(),
            border: true,
            padded: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Divider {
    pub style: char,
    pub width: u16,
}

impl Default for Divider {
    fn default() -> Self {
        Self {
            style: '─',
            width: 80,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ProgressBar {
    pub progress: f32, // 0..=1
    pub width: u16,
    pub show_pct: bool,
}

impl ProgressBar {
    pub fn render_line(&self) -> String {
        let total = self.width as usize;
        let done = (self.progress.clamp(0.0, 1.0) * total as f32).round() as usize;
        let bar: String = std::iter::repeat('█')
            .take(done)
            .chain(std::iter::repeat('░').take(total - done))
            .collect();
        if self.show_pct {
            format!("{} {:>3}%", bar, (self.progress.clamp(0.0, 1.0) * 100.0) as u32)
        } else {
            bar
        }
    }
}

// ===================================================================
// Theme provider / hooks
// ===================================================================

#[derive(Debug, Clone, Default)]
pub struct ThemeProvider {
    pub current: crate::theme::Theme,
    pub preview: Option<crate::theme::Theme>,
}

/// Hook-equivalent: read the current theme.
pub fn use_theme(p: &ThemeProvider) -> &crate::theme::Theme {
    p.preview.as_ref().unwrap_or(&p.current)
}

/// Hook-equivalent: read the user's theme setting.
pub fn use_theme_setting() -> crate::theme::ThemeSetting {
    crate::theme::ThemeSetting::default()
}

/// Hook-equivalent: temporarily preview a theme.
pub fn use_preview_theme(provider: &mut ThemeProvider, preview: Option<crate::theme::Theme>) {
    provider.preview = preview;
}

// ===================================================================
// Dialog / ListItem / Ratchet / Byline / Keyboard hint / LoadingState
// ===================================================================

#[derive(Debug, Clone)]
pub struct Dialog {
    pub title: String,
    pub width: u16,
    pub height: u16,
}

impl Default for Dialog {
    fn default() -> Self {
        Self {
            title: String::new(),
            width: 70,
            height: 16,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ListItem {
    pub label: String,
    pub description: Option<String>,
    pub selected: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Ratchet {
    pub current: u32,
    pub target: u32,
    pub step: u32,
}

impl Ratchet {
    pub fn tick(&mut self) {
        if self.current < self.target {
            self.current = (self.current + self.step).min(self.target);
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Byline {
    pub author: String,
    pub timestamp_ms: u64,
}

#[derive(Debug, Clone, Default)]
pub struct KeyboardShortcutHint {
    pub key_display: String,
    pub label: String,
}

#[derive(Debug, Clone, Default)]
pub struct LoadingState {
    pub label: String,
    pub spinner_frame: u32,
}

// === ThemedBox / ThemedText props ===

/// Props for `ThemedBox` (mirrors TS `components/design-system/ThemedBox.tsx`).
/// Theme keys are stored as plain strings (resolved at render time via `Theme`).
#[derive(Debug, Clone, Default)]
pub struct ThemedBoxProps {
    pub border_color: Option<String>,
    pub border_top_color: Option<String>,
    pub border_bottom_color: Option<String>,
    pub border_left_color: Option<String>,
    pub border_right_color: Option<String>,
    pub background_color: Option<String>,
    pub tab_index: Option<i32>,
    pub auto_focus: bool,
}

/// Props for `ThemedText` (mirrors TS `components/design-system/ThemedText.tsx`).
#[derive(Debug, Clone, Default)]
pub struct ThemedTextProps {
    pub color: Option<String>,
    pub background_color: Option<String>,
    pub dim_color: bool,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub inverse: bool,
    pub wrap: Option<String>,
}

// TS export aliases — each in its own sub-module so they can coexist.
pub mod themed_box_props_alias {
    pub type Props = super::ThemedBoxProps;
}
pub mod themed_text_props_alias {
    pub type Props = super::ThemedTextProps;
}
