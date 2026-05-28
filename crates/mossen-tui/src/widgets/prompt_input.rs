//! Prompt input widget — the main user input area.
//!
//! Translates PromptInput/ directory (21 files) including the input box,
//! footer, mode indicator, help menu, and notifications.

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Widget},
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::text_input::{TextInputState, TextInputWidget};
use crate::render_glyphs::RenderGlyphs;
use crate::state::InputMode;
use crate::theme::Theme;

const PROMPT_COMPOSER_HEIGHT: u16 = 3;
const MAX_SUGGESTION_ROWS: usize = 12;

/// Prompt input state — combines text input with mode and submission logic.
///
/// State management for the active prompt input widget.
pub struct PromptInputState {
    /// The underlying text input
    pub input: TextInputState,
    /// Current input mode
    pub mode: InputMode,
    /// Whether input is currently accepting characters
    pub active: bool,
    /// Queued commands waiting to execute
    pub queued_commands: Vec<String>,
    /// Whether to show the help menu
    pub show_help: bool,
    /// Whether to show suggestions
    pub show_suggestions: bool,
    /// Current suggestions list
    pub suggestions: Vec<Suggestion>,
    /// Selected suggestion index
    pub selected_suggestion: Option<usize>,
}

/// A command/file suggestion entry.
#[derive(Debug, Clone)]
pub struct Suggestion {
    pub label: String,
    pub description: Option<String>,
    pub kind: SuggestionKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestionKind {
    Command,
    File,
    Agent,
    Skill,
}

impl PromptInputState {
    pub fn new() -> Self {
        Self {
            input: TextInputState::new().with_placeholder("Ask anything..."),
            mode: InputMode::Normal,
            active: true,
            queued_commands: Vec::new(),
            show_help: false,
            show_suggestions: false,
            suggestions: Vec::new(),
            selected_suggestion: None,
        }
    }

    /// Check if input starts with command prefix '/'.
    pub fn is_command_input(&self) -> bool {
        self.input.value.starts_with('/')
    }

    /// Check if input starts with bash prefix '!'.
    pub fn is_bash_input(&self) -> bool {
        self.input.value.starts_with('!')
    }

    /// Get the mode indicator text.
    pub fn mode_indicator(&self) -> Option<&'static str> {
        match self.mode {
            InputMode::Normal => None,
            InputMode::Bash => Some("bash"),
            InputMode::Vim => Some("vim"),
            InputMode::Search => Some("search"),
            InputMode::Command => Some("cmd"),
        }
    }

    /// Submit the current input, returning the value.
    pub fn submit(&mut self) -> Option<String> {
        if !self.active {
            return None;
        }
        let val = self.input.submit();
        if val.is_empty() {
            return None;
        }
        self.show_suggestions = false;
        self.selected_suggestion = None;
        Some(val)
    }

    /// Toggle help menu visibility.
    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }

    /// Navigate suggestions up.
    pub fn suggestion_up(&mut self) -> bool {
        if self.suggestions.is_empty() {
            return false;
        }
        let before = self.selected_suggestion;
        self.selected_suggestion = Some(match self.selected_suggestion {
            None => self.suggestions.len() - 1,
            Some(0) => self.suggestions.len() - 1,
            Some(i) => i - 1,
        });
        before != self.selected_suggestion
    }

    /// Navigate suggestions down.
    pub fn suggestion_down(&mut self) -> bool {
        if self.suggestions.is_empty() {
            return false;
        }
        let before = self.selected_suggestion;
        self.selected_suggestion = Some(match self.selected_suggestion {
            None => 0,
            Some(i) if i >= self.suggestions.len() - 1 => 0,
            Some(i) => i + 1,
        });
        before != self.selected_suggestion
    }

    /// Move down by a visible page of suggestions.
    pub fn suggestion_page_down(&mut self, page_size: usize) -> bool {
        if self.suggestions.is_empty() {
            return false;
        }
        let before = self.selected_suggestion;
        let current = self.selected_suggestion.unwrap_or(0);
        let last = self.suggestions.len().saturating_sub(1);
        self.selected_suggestion = Some(current.saturating_add(page_size.max(1)).min(last));
        before != self.selected_suggestion
    }

    /// Move up by a visible page of suggestions.
    pub fn suggestion_page_up(&mut self, page_size: usize) -> bool {
        if self.suggestions.is_empty() {
            return false;
        }
        let before = self.selected_suggestion;
        let current = self.selected_suggestion.unwrap_or(0);
        self.selected_suggestion = Some(current.saturating_sub(page_size.max(1)));
        before != self.selected_suggestion
    }

    /// Accept current suggestion.
    pub fn accept_suggestion(&mut self) {
        if let Some(idx) = self.selected_suggestion {
            if let Some(suggestion) = self.suggestions.get(idx) {
                self.input.clear();
                let replacement = match suggestion.kind {
                    SuggestionKind::Command | SuggestionKind::Skill => {
                        format!("/{} ", suggestion.label.trim_start_matches('/'))
                    }
                    SuggestionKind::Agent => {
                        format!("@{} ", suggestion.label.trim_start_matches('@'))
                    }
                    SuggestionKind::File => suggestion.label.clone(),
                };
                self.input.insert_str(&replacement);
            }
        }
        self.show_suggestions = false;
        self.selected_suggestion = None;
    }
}

impl Default for PromptInputState {
    fn default() -> Self {
        Self::new()
    }
}

/// Prompt input widget for rendering.
pub struct PromptInputWidget<'a> {
    pub state: &'a PromptInputState,
    pub theme: &'a Theme,
    pub show_footer: bool,
    pub glyphs: RenderGlyphs,
}

impl<'a> PromptInputWidget<'a> {
    pub fn new(state: &'a PromptInputState, theme: &'a Theme) -> Self {
        Self {
            state,
            theme,
            show_footer: true,
            glyphs: RenderGlyphs::default(),
        }
    }

    pub fn glyphs(mut self, glyphs: RenderGlyphs) -> Self {
        self.glyphs = glyphs;
        self
    }

    /// Calculate total height needed for the prompt area.
    pub fn required_height(&self) -> u16 {
        let input_height = PROMPT_COMPOSER_HEIGHT;
        let footer_height = if self.show_footer { 1 } else { 0 };
        let suggestions_height = if self.state.show_suggestions {
            self.state.suggestions.len().min(MAX_SUGGESTION_ROWS) as u16
        } else {
            0
        };
        input_height + footer_height + suggestions_height
    }
}

impl<'a> Widget for PromptInputWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        // Layout: suggestions (above), input line, footer
        let footer_h = if self.show_footer { 1u16 } else { 0 };
        let input_h = prompt_composer_height_for_area(area.height, footer_h);
        let suggestions_h = area.height.saturating_sub(input_h + footer_h);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(suggestions_h),
                Constraint::Length(input_h),
                Constraint::Length(footer_h),
            ])
            .split(area);

        let suggestions_area = chunks[0];
        let input_area = chunks[1];
        let footer_area = chunks[2];

        // Render suggestions
        if self.state.show_suggestions && !self.state.suggestions.is_empty() {
            render_suggestions(
                suggestions_area,
                buf,
                &self.state.suggestions,
                self.state.selected_suggestion,
                self.theme,
            );
        }

        render_input_box(input_area, buf, self.state, self.theme, self.glyphs);

        // Render footer
        if self.show_footer && footer_h > 0 {
            render_footer(footer_area, buf, self.state, self.theme, self.glyphs);
        }
    }
}

fn prompt_composer_height_for_area(area_height: u16, footer_height: u16) -> u16 {
    if area_height.saturating_sub(footer_height) >= PROMPT_COMPOSER_HEIGHT {
        PROMPT_COMPOSER_HEIGHT
    } else {
        1
    }
}

fn render_input_box(
    area: Rect,
    buf: &mut Buffer,
    state: &PromptInputState,
    theme: &Theme,
    glyphs: RenderGlyphs,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    if area.height < PROMPT_COMPOSER_HEIGHT || area.width < 4 {
        render_input_line(area, buf, state, theme, glyphs);
        return;
    }

    let border_style = if state.active {
        Style::default().fg(theme.border_focused).bg(theme.surface)
    } else {
        Style::default().fg(theme.border).bg(theme.surface)
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_set(glyphs.border)
        .border_style(border_style)
        .style(Style::default().bg(theme.surface));
    block.render(area, buf);

    let inner = Rect::new(
        area.x.saturating_add(1),
        area.y.saturating_add(1),
        area.width.saturating_sub(2),
        1,
    );
    render_input_line(inner, buf, state, theme, glyphs);
}

fn render_input_line(
    area: Rect,
    buf: &mut Buffer,
    state: &PromptInputState,
    theme: &Theme,
    glyphs: RenderGlyphs,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let mut x = area.x;

    // Mode indicator
    if let Some(mode_text) = state.mode_indicator() {
        let mode_style = Style::default()
            .fg(theme.primary)
            .add_modifier(Modifier::BOLD);
        let indicator = format!("[{}] ", mode_text);
        let display = truncate_to_display_width(&indicator, (area.right() - x) as usize);
        buf.set_string(x, area.y, &display, mode_style);
        x = x.saturating_add(display_width_u16(&display));
        if x >= area.right() {
            return;
        }
    }

    // Prompt prefix
    let prefix = format!("{} ", glyphs.prompt);
    let prefix_style = Style::default().fg(theme.primary);
    let prefix_display = truncate_to_display_width(&prefix, (area.right() - x) as usize);
    buf.set_string(x, area.y, &prefix_display, prefix_style);
    x = x.saturating_add(display_width_u16(&prefix_display));
    if x >= area.right() {
        return;
    }

    // Text input
    let input_area = Rect::new(x, area.y, area.width.saturating_sub(x - area.x), 1);
    let input_widget = TextInputWidget::new(&state.input).style(Style::default().fg(theme.text));
    input_widget.render(input_area, buf);
}

fn render_footer(
    area: Rect,
    buf: &mut Buffer,
    state: &PromptInputState,
    theme: &Theme,
    glyphs: RenderGlyphs,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let dim_style = Style::default().fg(theme.text_dim);

    // Left side: mode/status info
    let left_text = match state.mode {
        InputMode::Normal => format!(
            "Enter to send{sep}/help for commands",
            sep = glyphs.separator()
        ),
        InputMode::Bash => format!(
            "Enter to execute{sep}Esc to cancel",
            sep = glyphs.separator()
        ),
        InputMode::Vim => format!(
            "i insert{sep}: command{sep}Esc normal",
            sep = glyphs.separator()
        ),
        InputMode::Search => format!(
            "Enter to search{sep}Esc to cancel",
            sep = glyphs.separator()
        ),
        InputMode::Command => format!(
            "Tab to complete{sep}Enter to execute",
            sep = glyphs.separator()
        ),
    };

    buf.set_line(
        area.x,
        area.y,
        &Line::from(Span::styled(left_text, dim_style)),
        area.width,
    );
}

fn render_suggestions(
    area: Rect,
    buf: &mut Buffer,
    suggestions: &[Suggestion],
    selected: Option<usize>,
    theme: &Theme,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let visible_count = (area.height as usize)
        .min(MAX_SUGGESTION_ROWS)
        .min(suggestions.len());
    if visible_count == 0 {
        return;
    }
    let start = suggestion_window_start(suggestions.len(), visible_count, selected);

    for (row, (i, suggestion)) in suggestions
        .iter()
        .enumerate()
        .skip(start)
        .take(visible_count)
        .enumerate()
    {
        let y = area.y + row as u16;
        if y >= area.y + area.height {
            break;
        }

        let is_selected = selected == Some(i);
        let style = if is_selected {
            Style::default()
                .fg(theme.text)
                .bg(theme.selection)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text_dim)
        };

        let prefix = match suggestion.kind {
            SuggestionKind::Command => "/",
            SuggestionKind::File => " ",
            SuggestionKind::Agent => "@",
            SuggestionKind::Skill => "/",
        };

        let text = format!("{}{}", prefix, suggestion.label);
        let width = area.width as usize;
        let (label_budget, desc_gap) = if width >= 36 {
            (width.min(28), 2usize)
        } else {
            (width, 0usize)
        };
        let label_display = truncate_to_display_width(&text, label_budget);
        buf.set_string(area.x, y, &label_display, style);

        // Description on the right
        if let Some(ref desc) = suggestion.description {
            let desc_x = area
                .x
                .saturating_add(label_budget as u16)
                .saturating_add(desc_gap as u16);
            if desc_x < area.x + area.width {
                let desc_style = if is_selected {
                    Style::default().fg(theme.text_dim).bg(theme.selection)
                } else {
                    Style::default().fg(theme.text_subtle)
                };
                let avail = (area.x + area.width - desc_x) as usize;
                let truncated = truncate_to_display_width(desc, avail);
                buf.set_string(desc_x, y, &truncated, desc_style);
            }
        }
    }
}

fn suggestion_window_start(len: usize, visible_count: usize, selected: Option<usize>) -> usize {
    if len <= visible_count {
        return 0;
    }
    let selected = selected.unwrap_or(0).min(len.saturating_sub(1));
    selected
        .saturating_add(1)
        .saturating_sub(visible_count)
        .min(len.saturating_sub(visible_count))
}

fn display_width_u16(text: &str) -> u16 {
    UnicodeWidthStr::width(text).min(u16::MAX as usize) as u16
}

fn truncate_to_display_width(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut width = 0usize;
    for grapheme in text.graphemes(true) {
        let w = UnicodeWidthStr::width(grapheme);
        if width + w > max_width {
            break;
        }
        width += w;
        out.push_str(grapheme);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{PromptInputState, PromptInputWidget, Suggestion, SuggestionKind};
    use crate::render_glyphs::RenderGlyphs;
    use crate::theme::Theme;
    use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};

    fn render_prompt(state: &PromptInputState, width: u16, height: u16) -> String {
        render_prompt_with_glyphs(state, width, height, RenderGlyphs::unicode())
    }

    fn render_prompt_with_glyphs(
        state: &PromptInputState,
        width: u16,
        height: u16,
        glyphs: RenderGlyphs,
    ) -> String {
        let theme = Theme::default();
        let mut buf = Buffer::empty(Rect::new(0, 0, width, height));
        PromptInputWidget::new(state, &theme)
            .glyphs(glyphs)
            .render(Rect::new(0, 0, width, height), &mut buf);
        let mut out = String::new();
        for y in 0..height {
            for x in 0..width {
                out.push_str(buf[(x, y)].symbol());
            }
            if y + 1 < height {
                out.push('\n');
            }
        }
        out
    }

    #[test]
    fn active_prompt_renders_placeholder_without_hiding_first_character() {
        let state = PromptInputState::new();

        let rendered = render_prompt(&state, 32, 4);

        assert!(
            rendered.contains("Ask anything"),
            "active prompt should keep placeholder legible:\n{rendered}"
        );
    }

    #[test]
    fn active_prompt_renders_as_visible_composer_box() {
        let state = PromptInputState::new();

        let rendered = render_prompt(&state, 40, 4);

        assert!(
            rendered
                .lines()
                .next()
                .is_some_and(|line| line.contains('─')),
            "composer should have a visible top border:\n{rendered}"
        );
        assert!(
            rendered.contains("Ask anything"),
            "composer should keep the placeholder legible:\n{rendered}"
        );
    }

    #[test]
    fn prompt_can_render_ascii_prefix() {
        let state = PromptInputState::new();

        let rendered = render_prompt_with_glyphs(&state, 32, 4, RenderGlyphs::ascii());

        assert!(rendered.contains("> "), "{rendered}");
        assert!(!rendered.contains('❯'), "{rendered}");
    }

    #[test]
    fn active_prompt_suggestions_do_not_overlap_multibyte_labels() {
        let mut state = PromptInputState::new();
        state.show_suggestions = true;
        state.suggestions = vec![Suggestion {
            label: "逐行阅读代码并分析架构".to_string(),
            description: Some("说明文字需要保持可见".to_string()),
            kind: SuggestionKind::Command,
        }];

        let rendered = render_prompt(&state, 48, 5);

        assert!(
            rendered.contains('说')
                && rendered.contains('明')
                && rendered.contains('文')
                && rendered.contains('字'),
            "description should remain visible beside wide labels:\n{rendered}"
        );
    }

    #[test]
    fn prompt_suggestion_window_follows_selected_item() {
        let mut state = PromptInputState::new();
        state.show_suggestions = true;
        state.suggestions = (0..10)
            .map(|index| Suggestion {
                label: format!("cmd{index:02}"),
                description: Some("command".to_string()),
                kind: SuggestionKind::Command,
            })
            .collect();
        state.selected_suggestion = Some(7);

        let rendered = render_prompt(&state, 48, 9);

        assert!(rendered.contains("/cmd07"), "{rendered}");
        assert!(rendered.contains("/cmd03"), "{rendered}");
        assert!(
            !rendered.contains("/cmd02") && !rendered.contains("/cmd00"),
            "suggestion viewport should move past the first commands:\n{rendered}"
        );
    }

    #[test]
    fn prompt_suggestions_can_show_more_than_five_rows() {
        let mut state = PromptInputState::new();
        state.show_suggestions = true;
        state.suggestions = (0..13)
            .map(|index| Suggestion {
                label: format!("cmd{index:02}"),
                description: None,
                kind: SuggestionKind::Command,
            })
            .collect();

        let rendered = render_prompt(&state, 48, 16);

        assert!(rendered.contains("/cmd00"), "{rendered}");
        assert!(rendered.contains("/cmd11"), "{rendered}");
        assert!(!rendered.contains("/cmd12"), "{rendered}");
    }

    #[test]
    fn prompt_suggestion_page_navigation_clamps_to_bounds() {
        let mut state = PromptInputState::new();
        state.suggestions = (0..10)
            .map(|index| Suggestion {
                label: format!("cmd{index:02}"),
                description: None,
                kind: SuggestionKind::Command,
            })
            .collect();

        assert!(state.suggestion_page_down(5));
        assert_eq!(state.selected_suggestion, Some(5));
        assert!(state.suggestion_page_down(5));
        assert_eq!(state.selected_suggestion, Some(9));
        assert!(state.suggestion_page_up(5));
        assert_eq!(state.selected_suggestion, Some(4));
        assert!(state.suggestion_page_up(5));
        assert_eq!(state.selected_suggestion, Some(0));
    }
}
