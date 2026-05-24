//! `/debug-config` redacted semantic configuration widget.

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
use crate::render_model::{DebugConfigRenderModel, StatusRowLevel, StatusRowRenderModel};
use crate::theme::Theme;

pub struct DebugConfigWidget<'a> {
    model: &'a DebugConfigRenderModel,
    theme: &'a Theme,
    glyphs: RenderGlyphs,
    scroll: usize,
}

impl<'a> DebugConfigWidget<'a> {
    pub fn new(model: &'a DebugConfigRenderModel, theme: &'a Theme) -> Self {
        Self {
            model,
            theme,
            glyphs: RenderGlyphs::default(),
            scroll: 0,
        }
    }

    pub fn glyphs(mut self, glyphs: RenderGlyphs) -> Self {
        self.glyphs = glyphs;
        self
    }

    pub fn scroll(mut self, scroll: usize) -> Self {
        self.scroll = scroll;
        self
    }
}

impl<'a> Widget for DebugConfigWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 30 || area.height < 6 {
            return;
        }

        let block = Block::default()
            .title(Span::styled(
                " Debug Config ",
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

        buf.set_stringn(
            inner.x,
            inner.y,
            clip_to_width(&self.model.summary, inner.width as usize),
            inner.width as usize,
            Style::default().fg(self.theme.text_dim),
        );

        let footer_y = inner.y.saturating_add(inner.height.saturating_sub(1));
        let footer = format!(
            "{}{}Up/Down scroll{}redacted",
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

        let mut y = inner.y.saturating_add(2);
        if self.model.is_empty() {
            if y < footer_y {
                buf.set_stringn(
                    inner.x,
                    y,
                    "No debug configuration facts available.",
                    inner.width as usize,
                    Style::default().fg(self.theme.text_dim),
                );
            }
            return;
        }

        let mut row_index = 0usize;
        for section in &self.model.sections {
            if row_index >= self.scroll {
                if y >= footer_y {
                    break;
                }
                buf.set_stringn(
                    inner.x,
                    y,
                    clip_to_width(&section.title, inner.width as usize),
                    inner.width as usize,
                    Style::default()
                        .fg(self.theme.info)
                        .add_modifier(Modifier::BOLD),
                );
                y = y.saturating_add(1);
            }
            row_index = row_index.saturating_add(1);

            for row in &section.rows {
                if row_index >= self.scroll {
                    if y >= footer_y {
                        break;
                    }
                    self.render_row(row, inner.x, y, inner.width, buf);
                    y = y.saturating_add(1);
                }
                row_index = row_index.saturating_add(1);
            }
        }
    }
}

impl DebugConfigWidget<'_> {
    fn render_row(&self, row: &StatusRowRenderModel, x: u16, y: u16, width: u16, buf: &mut Buffer) {
        let width = width as usize;
        let label_width = (width / 3).clamp(11, 20).min(width.saturating_sub(2));
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
    use crate::render_model::StatusSectionRenderModel;
    use ratatui::buffer::Buffer;

    fn render_debug_config(
        model: &DebugConfigRenderModel,
        width: u16,
        height: u16,
        scroll: usize,
        glyphs: RenderGlyphs,
    ) -> String {
        let theme = Theme::default();
        let mut buf = Buffer::empty(Rect::new(0, 0, width, height));
        DebugConfigWidget::new(model, &theme)
            .glyphs(glyphs)
            .scroll(scroll)
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
    fn debug_config_renders_redacted_config_rows() {
        let model = DebugConfigRenderModel::new("renderer unicode | secrets redacted")
            .section(
                StatusSectionRenderModel::new("Engine")
                    .row("Model", "MiniMax-M2.7", StatusRowLevel::Good)
                    .row("API Key", "configured", StatusRowLevel::Good)
                    .row(
                        "Extra Body",
                        "keys: effort, temperature",
                        StatusRowLevel::Info,
                    ),
            )
            .section(StatusSectionRenderModel::new("Renderer").row(
                "Glyphs",
                "unicode",
                StatusRowLevel::Info,
            ));

        let rendered = render_debug_config(&model, 82, 12, 0, RenderGlyphs::unicode());

        assert!(rendered.contains("Debug Config"), "{rendered}");
        assert!(rendered.contains("secrets redacted"), "{rendered}");
        assert!(rendered.contains("API Key"), "{rendered}");
        assert!(rendered.contains("configured"), "{rendered}");
        assert!(!rendered.contains("sk-"), "{rendered}");
    }

    #[test]
    fn debug_config_scrolls_and_clips_multibyte_rows_with_ascii_separator() {
        let model = DebugConfigRenderModel::new(
            "渲染配置摘要非常长，需要在窄终端里安全裁剪并保持 UTF-8 边界",
        )
        .section(
            StatusSectionRenderModel::new("Session")
                .row("Product", "Mossen Code", StatusRowLevel::Info)
                .row("Version", "dev", StatusRowLevel::Info),
        )
        .section(StatusSectionRenderModel::new("Renderer").row(
            "Workspace",
            "/Users/allen/Documents/rustmossen/包含中文路径/非常长的目录名称",
            StatusRowLevel::Normal,
        ));

        let rendered = render_debug_config(&model, 46, 8, 2, RenderGlyphs::ascii());

        assert!(rendered.contains("Version"), "{rendered}");
        assert!(rendered.contains("Renderer"), "{rendered}");
        assert!(rendered.contains("Esc closes - Up/Down"), "{rendered}");
        assert!(
            !rendered.contains("�"),
            "multibyte clipping must stay on UTF-8 boundaries\n{rendered}"
        );
    }
}
