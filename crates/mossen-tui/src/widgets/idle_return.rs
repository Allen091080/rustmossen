//! Idle-return modal renderer.
//!
//! This active modal is intentionally small and terminal-native. It lives in
//! widgets rather than a translated root component island.

use std::time::Duration;

use ratatui::{buffer::Buffer, layout::Rect, style::Style, widgets::Widget};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::render_glyphs::RenderGlyphs;
use crate::theme::Theme;

#[derive(Debug)]
pub struct IdleReturnDialogState {
    pub idle_duration: Duration,
    pub message: String,
    pub dismissed: bool,
}

impl IdleReturnDialogState {
    pub fn new(idle_duration: Duration) -> Self {
        let mins = idle_duration.as_secs() / 60;
        let message = if mins > 60 {
            format!(
                "Welcome back! You were away for {}h {}m.",
                mins / 60,
                mins % 60
            )
        } else {
            format!("Welcome back! You were away for {}m.", mins)
        };
        Self {
            idle_duration,
            message,
            dismissed: false,
        }
    }

    pub fn dismiss(&mut self) {
        self.dismissed = true;
    }
}

pub struct IdleReturnDialogWidget<'a> {
    pub state: &'a IdleReturnDialogState,
    pub theme: &'a Theme,
    glyphs: RenderGlyphs,
}

impl<'a> IdleReturnDialogWidget<'a> {
    pub fn new(state: &'a IdleReturnDialogState, theme: &'a Theme) -> Self {
        Self {
            state,
            theme,
            glyphs: RenderGlyphs::default(),
        }
    }

    pub fn glyphs(mut self, glyphs: RenderGlyphs) -> Self {
        self.glyphs = glyphs;
        self
    }
}

impl<'a> Widget for IdleReturnDialogWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let area = area.intersection(buf.area);
        if self.state.dismissed || area.width == 0 || area.height == 0 {
            return;
        }

        let message = truncate_to_display_width(&self.state.message, area.width, self.glyphs);
        buf.set_string(
            area.x,
            area.y,
            &message,
            Style::default().fg(self.theme.text),
        );
        if area.height > 1 {
            let hint =
                truncate_to_display_width("Press any key to continue...", area.width, self.glyphs);
            buf.set_string(
                area.x,
                area.y.saturating_add(1),
                &hint,
                Style::default().fg(self.theme.text_dim),
            );
        }
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
    let budget = max_width.saturating_sub(suffix_width);
    let mut out = String::new();
    let mut width = 0usize;
    for grapheme in text.graphemes(true) {
        let grapheme_width = UnicodeWidthStr::width(grapheme);
        if width.saturating_add(grapheme_width) > budget {
            break;
        }
        out.push_str(grapheme);
        width = width.saturating_add(grapheme_width);
    }
    out.push_str(suffix);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_return_clips_to_modal_width() {
        let state = IdleReturnDialogState::new(Duration::from_secs(72 * 60));
        let theme = Theme::default();
        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 2));

        IdleReturnDialogWidget::new(&state, &theme).render(buf.area, &mut buf);

        let line = (0..buf.area.width)
            .map(|x| buf[(x, 0)].symbol())
            .collect::<String>();
        assert!(line.contains("Welcome"));
        assert!(UnicodeWidthStr::width(line.trim_end()) <= 20);
    }

    #[test]
    fn idle_return_can_use_ascii_truncation() {
        let state = IdleReturnDialogState::new(Duration::from_secs(72 * 60));
        let theme = Theme::default();
        let mut buf = Buffer::empty(Rect::new(0, 0, 18, 2));

        IdleReturnDialogWidget::new(&state, &theme)
            .glyphs(RenderGlyphs::ascii())
            .render(buf.area, &mut buf);

        let line = (0..buf.area.width)
            .map(|x| buf[(x, 0)].symbol())
            .collect::<String>();
        assert!(line.contains("..."), "{line}");
        assert!(!line.contains('…'), "{line}");
    }
}
