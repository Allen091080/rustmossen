//! `/status` semantic session overview widget.

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
use crate::render_model::{StatusOverviewRenderModel, StatusRowLevel, StatusRowRenderModel};
use crate::theme::Theme;

pub struct StatusOverviewWidget<'a> {
    model: &'a StatusOverviewRenderModel,
    theme: &'a Theme,
    glyphs: RenderGlyphs,
}

impl<'a> StatusOverviewWidget<'a> {
    pub fn new(model: &'a StatusOverviewRenderModel, theme: &'a Theme) -> Self {
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

impl<'a> Widget for StatusOverviewWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 24 || area.height < 5 {
            return;
        }

        let block = Block::default()
            .title(Span::styled(
                " Status ",
                Style::default()
                    .fg(self.theme.primary)
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

        let summary = clip_to_width(&self.model.summary, inner.width as usize);
        buf.set_stringn(
            inner.x,
            inner.y,
            summary,
            inner.width as usize,
            Style::default().fg(self.theme.text_dim),
        );

        let footer_y = inner.y.saturating_add(inner.height.saturating_sub(1));
        let footer = format!(
            "{}{}/status semantic overview",
            self.model.footer,
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

        let mut y = inner.y.saturating_add(2);
        if self.model.is_empty() {
            if y < footer_y {
                buf.set_stringn(
                    inner.x,
                    y,
                    "No status facts available.",
                    inner.width as usize,
                    Style::default().fg(self.theme.text_dim),
                );
            }
            return;
        }

        for section in &self.model.sections {
            if y >= footer_y {
                break;
            }
            buf.set_stringn(
                inner.x,
                y,
                clip_to_width(&section.title, inner.width as usize),
                inner.width as usize,
                Style::default()
                    .fg(self.theme.primary)
                    .add_modifier(Modifier::BOLD),
            );
            y = y.saturating_add(1);

            for row in &section.rows {
                if y >= footer_y {
                    break;
                }
                self.render_row(row, inner.x, y, inner.width, buf);
                y = y.saturating_add(1);
            }
        }
    }
}

impl StatusOverviewWidget<'_> {
    fn render_row(&self, row: &StatusRowRenderModel, x: u16, y: u16, width: u16, buf: &mut Buffer) {
        let width = width as usize;
        let label_width = (width / 3).clamp(10, 18).min(width.saturating_sub(2));
        let value_width = width.saturating_sub(label_width.saturating_add(1));
        let label = pad_to_width(&row.label, label_width);
        let value = clip_to_width(&row.value, value_width);
        let line = Line::from(vec![
            Span::styled(
                label,
                Style::default()
                    .fg(self.theme.text_dim)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(value, self.level_style(row.level)),
        ]);
        buf.set_line(x, y, &line, width as u16);
    }

    fn level_style(&self, level: StatusRowLevel) -> Style {
        let color = match level {
            StatusRowLevel::Normal => self.theme.text,
            StatusRowLevel::Good => self.theme.success,
            StatusRowLevel::Warning => self.theme.warning,
            StatusRowLevel::Error => self.theme.error,
            StatusRowLevel::Info => self.theme.info,
        };
        Style::default().fg(color)
    }
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
    use crate::render_model::{StatusRowLevel, StatusSectionRenderModel};

    fn render_status(
        model: &StatusOverviewRenderModel,
        width: u16,
        height: u16,
        glyphs: RenderGlyphs,
    ) -> String {
        let theme = Theme::default();
        let mut buf = Buffer::empty(Rect::new(0, 0, width, height));
        StatusOverviewWidget::new(model, &theme)
            .glyphs(glyphs)
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
    fn status_overview_renders_sections_and_rows() {
        let model =
            StatusOverviewRenderModel::new("MiniMax-M2.7 | turn running command | mode Supervised")
                .section(
                    StatusSectionRenderModel::new("Session")
                        .row("Model", "MiniMax-M2.7", StatusRowLevel::Good)
                        .row("Session", "session-123", StatusRowLevel::Info),
                )
                .section(
                    StatusSectionRenderModel::new("Policy")
                        .row("Access Mode", "Supervised", StatusRowLevel::Normal)
                        .row("API Key", "configured", StatusRowLevel::Good),
                );

        let rendered = render_status(&model, 82, 16, RenderGlyphs::unicode());

        assert!(rendered.contains("Status"), "{rendered}");
        assert!(rendered.contains("Session"), "{rendered}");
        assert!(rendered.contains("Model"), "{rendered}");
        assert!(rendered.contains("MiniMax-M2.7"), "{rendered}");
        assert!(rendered.contains("Policy"), "{rendered}");
        assert!(rendered.contains("API Key"), "{rendered}");
        assert!(rendered.contains("configured"), "{rendered}");
        assert!(rendered.contains("Esc closes"), "{rendered}");
    }

    #[test]
    fn status_overview_clips_multibyte_and_uses_ascii_separator() {
        let model = StatusOverviewRenderModel::new(
            "模型 MiniMax-M2.7 正在执行一个很长很长的状态摘要用于测试裁剪",
        )
        .section(StatusSectionRenderModel::new("Workspace").row(
            "CWD",
            "/Users/allen/Documents/rustmossen/包含中文路径/非常长的目录名称",
            StatusRowLevel::Info,
        ));

        let rendered = render_status(&model, 48, 9, RenderGlyphs::ascii());

        assert!(rendered.contains("Status"), "{rendered}");
        assert!(rendered.contains("Workspace"), "{rendered}");
        assert!(rendered.contains("CWD"), "{rendered}");
        assert!(rendered.contains("Esc closes - /status"), "{rendered}");
        assert!(
            !rendered.contains("�"),
            "multibyte clipping must stay on UTF-8 boundaries\n{rendered}"
        );
    }
}
