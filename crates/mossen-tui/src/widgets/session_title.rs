//! `/title` terminal-title configuration widget.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Widget},
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::render_glyphs::RenderGlyphs;
use crate::render_model::SessionTitleRenderModel;
use crate::theme::Theme;

pub struct SessionTitleWidget<'a> {
    model: &'a SessionTitleRenderModel,
    theme: &'a Theme,
    glyphs: RenderGlyphs,
}

impl<'a> SessionTitleWidget<'a> {
    pub fn new(model: &'a SessionTitleRenderModel, theme: &'a Theme) -> Self {
        Self {
            model,
            theme,
            glyphs: RenderGlyphs::default(),
        }
    }

    pub fn glyphs(mut self, glyphs: RenderGlyphs) -> Self {
        self.glyphs = glyphs;
        self
    }
}

impl<'a> Widget for SessionTitleWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 32 || area.height < 8 {
            return;
        }

        let block = Block::default()
            .title(Span::styled(
                " Session Title ",
                Style::default()
                    .fg(self.theme.info)
                    .add_modifier(Modifier::BOLD),
            ))
            .borders(Borders::ALL)
            .border_set(self.glyphs.border)
            .border_style(self.theme.style_border());
        let inner = block.inner(area);
        block.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let rows = [
            (
                "Current",
                self.model.current_title.as_str(),
                self.theme.text,
            ),
            (
                "Custom",
                self.model.custom_title.as_deref().unwrap_or("default"),
                self.theme.text,
            ),
            (
                "Draft",
                if self.model.draft.trim().is_empty() {
                    "default"
                } else {
                    self.model.draft.as_str()
                },
                self.theme.info,
            ),
            ("Status", self.model.status.as_str(), self.theme.text_dim),
        ];

        let mut y = inner.y;
        for (label, value, color) in rows {
            if y >= inner.y.saturating_add(inner.height).saturating_sub(1) {
                break;
            }
            render_row(
                label,
                value,
                color,
                self.theme,
                inner.x,
                y,
                inner.width,
                buf,
            );
            y = y.saturating_add(1);
        }

        let footer_y = inner.y.saturating_add(inner.height.saturating_sub(1));
        let footer = format!(
            "{}{}Ctrl+U reset{}Esc closes",
            self.model.footer,
            self.glyphs.separator(),
            self.glyphs.separator()
        );
        buf.set_stringn(
            inner.x,
            footer_y,
            clip_to_width(&footer, inner.width as usize),
            inner.width as usize,
            Style::default()
                .fg(self.theme.text_subtle)
                .add_modifier(Modifier::ITALIC),
        );
    }
}

fn render_row(
    label: &str,
    value: &str,
    value_color: ratatui::style::Color,
    theme: &Theme,
    x: u16,
    y: u16,
    width: u16,
    buf: &mut Buffer,
) {
    let width = width as usize;
    let label_width = 10usize.min(width.saturating_sub(2));
    let value_width = width.saturating_sub(label_width.saturating_add(1));
    let label = pad_to_width(label, label_width);
    let value = clip_to_width(value, value_width);
    let line = Line::from(vec![
        Span::styled(
            label,
            Style::default()
                .fg(theme.text_dim)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(value, Style::default().fg(value_color)),
    ]);
    buf.set_line(x, y, &line, width as u16);
}

fn pad_to_width(text: &str, width: usize) -> String {
    let mut out = clip_to_width(text, width);
    let used = UnicodeWidthStr::width(out.as_str());
    out.extend(std::iter::repeat(' ').take(width.saturating_sub(used)));
    out
}

fn clip_to_width(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }

    let suffix = "...";
    if UnicodeWidthStr::width(text) <= width {
        return text.to_string();
    }
    if width <= suffix.len() {
        return ".".repeat(width);
    }

    let suffix_width = UnicodeWidthStr::width(suffix);
    let mut out = String::new();
    let mut used = 0usize;
    for grapheme in text.graphemes(true) {
        let grapheme_width = UnicodeWidthStr::width(grapheme);
        if used
            .saturating_add(grapheme_width)
            .saturating_add(suffix_width)
            > width
        {
            break;
        }
        out.push_str(grapheme);
        used = used.saturating_add(grapheme_width);
    }
    out.push_str(suffix);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render_title(model: &SessionTitleRenderModel, width: u16, height: u16) -> String {
        let theme = Theme::default();
        let mut buf = Buffer::empty(Rect::new(0, 0, width, height));
        SessionTitleWidget::new(model, &theme)
            .glyphs(RenderGlyphs::ascii())
            .render(buf.area, &mut buf);
        let mut out = String::new();
        for y in 0..height {
            for x in 0..width {
                out.push_str(buf[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    #[test]
    fn session_title_renders_current_custom_and_draft() {
        let model = SessionTitleRenderModel::new(
            "终端渲染 ▸ ready",
            Some("终端渲染".to_string()),
            "终端渲染",
        )
        .status("saved");

        let rendered = render_title(&model, 72, 10);

        assert!(rendered.contains("Session Title"), "{rendered}");
        assert!(rendered.contains("Current"), "{rendered}");
        assert!(rendered.contains("Custom"), "{rendered}");
        assert!(rendered.contains("Draft"), "{rendered}");
        assert!(rendered.contains("saved"), "{rendered}");
        assert!(rendered.contains("Ctrl+U reset"), "{rendered}");
    }

    #[test]
    fn session_title_clips_long_multibyte_title_with_ascii_separator() {
        let model = SessionTitleRenderModel::new(
            "渲染标题".repeat(20),
            Some("渲染标题".repeat(20)),
            "渲染标题".repeat(20),
        );

        let rendered = render_title(&model, 42, 9);

        assert!(rendered.contains("..."), "{rendered}");
        assert!(rendered.contains(" - "), "{rendered}");
    }
}
