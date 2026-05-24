//! Semantic message selector modal.
//!
//! This is an active App modal renderer. It consumes summaries that already
//! came through `RenderTranscript`, so selector rows do not inspect raw
//! message/tool payloads.

use std::time::Instant;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    widgets::{Block, Borders, Widget},
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::render_glyphs::RenderGlyphs;
use crate::theme::Theme;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderableMessageType {
    User,
    Assistant,
    System,
    Progress,
    ToolUse,
    ToolResult,
    CollapsedReadSearch,
    Thinking,
    Meta,
}

#[derive(Debug, Clone)]
pub struct RenderableMessage {
    pub uuid: String,
    pub message_type: RenderableMessageType,
    pub content: String,
    pub tool_use_id: Option<String>,
    pub is_meta: bool,
    pub is_api_error: bool,
    pub timestamp: Option<Instant>,
    pub model: Option<String>,
    pub thinking_content: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreOption {
    Both,
    Conversation,
    Code,
    Summarize,
    SummarizeUpTo,
    Nevermind,
}

impl RestoreOption {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Both => "Restore code and conversation",
            Self::Conversation => "Restore conversation",
            Self::Code => "Restore code",
            Self::Summarize => "Summarize from here",
            Self::SummarizeUpTo => "Summarize up to here",
            Self::Nevermind => "Never mind",
        }
    }

    pub fn is_summarize(&self) -> bool {
        matches!(self, Self::Summarize | Self::SummarizeUpTo)
    }
}

#[derive(Debug, Clone)]
pub struct DiffStats {
    pub files_changed: usize,
    pub lines_added: usize,
    pub lines_removed: usize,
}

#[derive(Debug)]
pub struct MessageSelectorState {
    pub messages: Vec<RenderableMessage>,
    pub selected_index: usize,
    pub message_to_restore: Option<usize>,
    pub diff_stats: Option<DiffStats>,
    pub is_restoring: bool,
    pub restoring_option: Option<RestoreOption>,
    pub selected_restore_option: RestoreOption,
    pub summarize_feedback: String,
    pub error: Option<String>,
    pub file_history_enabled: bool,
}

impl MessageSelectorState {
    pub fn new(messages: Vec<RenderableMessage>, file_history_enabled: bool) -> Self {
        let selected_index = messages.len().saturating_sub(1);
        Self {
            messages,
            selected_index,
            message_to_restore: None,
            diff_stats: None,
            is_restoring: false,
            restoring_option: None,
            selected_restore_option: RestoreOption::Both,
            summarize_feedback: String::new(),
            error: None,
            file_history_enabled,
        }
    }

    pub fn focus_next(&mut self) {
        if self.selected_index < self.messages.len().saturating_sub(1) {
            self.selected_index += 1;
        }
    }

    pub fn focus_prev(&mut self) {
        self.selected_index = self.selected_index.saturating_sub(1);
    }

    pub fn confirm_selection(&mut self) {
        self.message_to_restore = Some(self.selected_index);
    }

    pub fn set_restore_option(&mut self, option: RestoreOption) {
        self.selected_restore_option = option;
    }

    pub fn start_restore(&mut self) {
        self.is_restoring = true;
        self.restoring_option = Some(self.selected_restore_option.clone());
    }

    pub fn back(&mut self) {
        if self.message_to_restore.is_some() {
            self.message_to_restore = None;
        }
    }

    pub fn get_restore_options(&self, can_restore_code: bool) -> Vec<RestoreOption> {
        let mut options = if can_restore_code {
            vec![
                RestoreOption::Both,
                RestoreOption::Conversation,
                RestoreOption::Code,
            ]
        } else {
            vec![RestoreOption::Conversation]
        };
        options.push(RestoreOption::Summarize);
        options.push(RestoreOption::SummarizeUpTo);
        options.push(RestoreOption::Nevermind);
        options
    }
}

pub struct MessageSelectorWidget<'a> {
    pub state: &'a MessageSelectorState,
    pub theme: &'a Theme,
    pub glyphs: RenderGlyphs,
}

impl<'a> Widget for MessageSelectorWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let area = area.intersection(buf.area);
        if area.width == 0 || area.height == 0 {
            return;
        }

        let block = Block::default()
            .title(" Select Message ")
            .borders(Borders::ALL)
            .border_set(self.glyphs.border)
            .border_style(Style::default().fg(self.theme.border));
        let inner = block.inner(area);
        block.render(area, buf);
        if inner.width == 0 || inner.height == 0 {
            return;
        }

        if self.state.message_to_restore.is_some() {
            render_restore_options(self.state, inner, buf, self.theme, self.glyphs);
        } else {
            render_message_rows(self.state, inner, buf, self.theme, self.glyphs);
        }
    }
}

fn render_restore_options(
    state: &MessageSelectorState,
    inner: Rect,
    buf: &mut Buffer,
    theme: &Theme,
    glyphs: RenderGlyphs,
) {
    let options = state.get_restore_options(state.file_history_enabled);
    for (i, opt) in options.iter().enumerate() {
        let y = inner.y.saturating_add(i as u16);
        if y >= inner.bottom() {
            break;
        }
        let is_selected = *opt == state.selected_restore_option;
        let prefix = if is_selected {
            format!("{} ", glyphs.selected_indicator())
        } else {
            "  ".to_string()
        };
        let style = selector_style(is_selected, theme);
        let line =
            truncate_to_display_width(&format!("{}{}", prefix, opt.label()), inner.width, glyphs);
        buf.set_string(inner.x, y, &line, style);
    }
}

fn render_message_rows(
    state: &MessageSelectorState,
    inner: Rect,
    buf: &mut Buffer,
    theme: &Theme,
    glyphs: RenderGlyphs,
) {
    let max_visible = inner.height.min(7) as usize;
    if max_visible == 0 {
        return;
    }
    let first_visible = state
        .selected_index
        .saturating_sub(max_visible / 2)
        .min(state.messages.len().saturating_sub(max_visible));
    let end_visible = (first_visible + max_visible).min(state.messages.len());

    for (i, msg) in state.messages[first_visible..end_visible]
        .iter()
        .enumerate()
    {
        let y = inner.y.saturating_add(i as u16);
        if y >= inner.bottom() {
            break;
        }
        let is_selected = first_visible + i == state.selected_index;
        let prefix = if is_selected {
            format!("{} ", glyphs.selected_indicator())
        } else {
            "  ".to_string()
        };
        let prefix_width = UnicodeWidthStr::width(prefix.as_str());
        let content_width = inner.width as usize;
        let content_budget = content_width.saturating_sub(prefix_width);
        let content = truncate_to_display_width(&msg.content, content_budget as u16, glyphs);
        let line =
            truncate_to_display_width(&format!("{}{}", prefix, content), inner.width, glyphs);
        buf.set_string(inner.x, y, &line, selector_style(is_selected, theme));
    }
}

fn selector_style(is_selected: bool, theme: &Theme) -> Style {
    if is_selected {
        Style::default()
            .fg(theme.primary)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text)
    }
}

fn truncate_to_display_width(text: &str, max_width: u16, glyphs: RenderGlyphs) -> String {
    let max_width = max_width as usize;
    if max_width == 0 {
        return String::new();
    }
    if UnicodeWidthStr::width(text) <= max_width {
        return text.to_string();
    }

    let suffix = glyphs.ellipsis();
    let suffix_width = UnicodeWidthStr::width(suffix);
    if suffix_width >= max_width {
        return String::new();
    }
    let budget = max_width.saturating_sub(suffix_width);
    let mut out = String::new();
    let mut used = 0usize;
    for grapheme in text.graphemes(true) {
        let width = UnicodeWidthStr::width(grapheme);
        if used.saturating_add(width) > budget {
            break;
        }
        out.push_str(grapheme);
        used = used.saturating_add(width);
    }
    out.push_str(suffix);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buffer_text(buf: &Buffer) -> String {
        let mut out = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                out.push_str(buf[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    fn sample_selector_state(content: &str) -> MessageSelectorState {
        MessageSelectorState::new(
            vec![RenderableMessage {
                uuid: "tool-1".to_string(),
                message_type: RenderableMessageType::ToolResult,
                content: content.to_string(),
                tool_use_id: Some("Bash".to_string()),
                is_meta: false,
                is_api_error: false,
                timestamp: None,
                model: None,
                thinking_content: None,
            }],
            true,
        )
    }

    #[test]
    fn message_selector_clips_multibyte_rows_without_raw_payloads() {
        let state = sample_selector_state("Bash · exit 0 · 逐行阅读真实渲染路径而不是原始 JSON");
        let theme = Theme::default();
        let mut buf = Buffer::empty(Rect::new(0, 0, 32, 5));

        MessageSelectorWidget {
            state: &state,
            theme: &theme,
            glyphs: RenderGlyphs::unicode(),
        }
        .render(buf.area, &mut buf);

        let rendered = buffer_text(&buf);
        assert!(rendered.contains("Bash"), "{rendered}");
        assert!(rendered.contains('逐') || rendered.contains('渲') || rendered.contains('…'));
        assert!(!rendered.contains('{'));
        assert!(!rendered.contains("\"stdout\""));
    }

    #[test]
    fn message_selector_can_render_ascii_indicator_and_ellipsis() {
        let state = sample_selector_state("Bash completed with a very long output line");
        let theme = Theme::default();
        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 5));

        MessageSelectorWidget {
            state: &state,
            theme: &theme,
            glyphs: RenderGlyphs::ascii(),
        }
        .render(buf.area, &mut buf);

        let rendered = buffer_text(&buf);
        assert!(rendered.contains("> Bash"), "{rendered}");
        assert!(rendered.contains("..."), "{rendered}");
        assert!(!rendered.contains('▸'), "{rendered}");
        assert!(!rendered.contains('…'), "{rendered}");
    }
}
