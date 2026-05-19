//! Prompt input widget — the main user input area.
//!
//! Translates PromptInput/ directory (21 files) including the input box,
//! footer, mode indicator, help menu, and notifications.

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};

use super::text_input::{TextInputState, TextInputWidget};
use crate::state::InputMode;
use crate::theme::Theme;

/// Prompt input state — combines text input with mode and submission logic.
///
/// Translates: PromptInput.tsx (95.8KB) state management.
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
    pub fn suggestion_up(&mut self) {
        if self.suggestions.is_empty() {
            return;
        }
        self.selected_suggestion = Some(match self.selected_suggestion {
            None => self.suggestions.len() - 1,
            Some(0) => self.suggestions.len() - 1,
            Some(i) => i - 1,
        });
    }

    /// Navigate suggestions down.
    pub fn suggestion_down(&mut self) {
        if self.suggestions.is_empty() {
            return;
        }
        self.selected_suggestion = Some(match self.selected_suggestion {
            None => 0,
            Some(i) if i >= self.suggestions.len() - 1 => 0,
            Some(i) => i + 1,
        });
    }

    /// Accept current suggestion.
    pub fn accept_suggestion(&mut self) {
        if let Some(idx) = self.selected_suggestion {
            if let Some(suggestion) = self.suggestions.get(idx) {
                self.input.clear();
                self.input.insert_str(&suggestion.label);
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
}

impl<'a> PromptInputWidget<'a> {
    pub fn new(state: &'a PromptInputState, theme: &'a Theme) -> Self {
        Self {
            state,
            theme,
            show_footer: true,
        }
    }

    /// Calculate total height needed for the prompt area.
    pub fn required_height(&self) -> u16 {
        let input_height = 1u16;
        let footer_height = if self.show_footer { 1 } else { 0 };
        let suggestions_height = if self.state.show_suggestions {
            self.state.suggestions.len().min(8) as u16
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
        let input_h = 1u16;
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

        // Render input line with mode indicator
        render_input_line(input_area, buf, self.state, self.theme);

        // Render footer
        if self.show_footer && footer_h > 0 {
            render_footer(footer_area, buf, self.state, self.theme);
        }
    }
}

fn render_input_line(area: Rect, buf: &mut Buffer, state: &PromptInputState, theme: &Theme) {
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
        buf.set_string(x, area.y, &indicator, mode_style);
        x += indicator.len() as u16;
    }

    // Prompt prefix
    let prefix = if state.is_command_input() {
        "/"
    } else if state.is_bash_input() {
        "!"
    } else {
        "❯ "
    };
    let prefix_style = Style::default().fg(theme.primary);
    buf.set_string(x, area.y, prefix, prefix_style);
    x += prefix.len() as u16;

    // Text input
    let input_area = Rect::new(x, area.y, area.width.saturating_sub(x - area.x), 1);
    let input_widget = TextInputWidget::new(&state.input);
    input_widget.render(input_area, buf);
}

fn render_footer(area: Rect, buf: &mut Buffer, state: &PromptInputState, theme: &Theme) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let dim_style = Style::default().fg(theme.text_dim);

    // Left side: mode/status info
    let left_text = match state.mode {
        InputMode::Normal => "Enter to send · /help for commands",
        InputMode::Bash => "Enter to execute · Esc to cancel",
        InputMode::Vim => "i insert · : command · Esc normal",
        InputMode::Search => "Enter to search · Esc to cancel",
        InputMode::Command => "Tab to complete · Enter to execute",
    };

    buf.set_string(area.x, area.y, left_text, dim_style);
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

    let visible_count = area.height as usize;
    let display_suggestions = &suggestions[..suggestions.len().min(visible_count)];

    for (i, suggestion) in display_suggestions.iter().enumerate() {
        let y = area.y + i as u16;
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
            SuggestionKind::Skill => "#",
        };

        let text = format!("{}{}", prefix, suggestion.label);
        buf.set_string(area.x, y, &text, style);

        // Description on the right
        if let Some(ref desc) = suggestion.description {
            let desc_x = area.x + 20;
            if desc_x < area.x + area.width {
                let desc_style = if is_selected {
                    Style::default().fg(theme.text_dim).bg(theme.selection)
                } else {
                    Style::default().fg(theme.text_subtle)
                };
                let avail = (area.x + area.width - desc_x) as usize;
                let truncated: String = desc.chars().take(avail).collect();
                buf.set_string(desc_x, y, &truncated, desc_style);
            }
        }
    }
}
