//! Live active-turn panel for the semantic rendering pipeline.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::Widget;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::render_glyphs::RenderGlyphs;
use crate::render_model::{ActivityPanelRenderModel, ActivityPanelSeverity};
use crate::theme::Theme;

pub struct ActivityPanelWidget<'a> {
    model: &'a ActivityPanelRenderModel,
    theme: &'a Theme,
    glyphs: RenderGlyphs,
}

impl<'a> ActivityPanelWidget<'a> {
    pub fn new(model: &'a ActivityPanelRenderModel, theme: &'a Theme) -> Self {
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

    pub fn required_height(&self, width: u16) -> u16 {
        if width < 24 {
            0
        } else if width < 44 {
            1
        } else {
            3
        }
    }
}

impl<'a> Widget for ActivityPanelWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 24 || area.height == 0 {
            return;
        }

        if area.height < 3 || area.width < 44 {
            self.render_compact(area, buf);
        } else {
            self.render_panel(area, buf);
        }
    }
}

impl ActivityPanelWidget<'_> {
    fn render_compact(&self, area: Rect, buf: &mut Buffer) {
        let style = self.severity_style();
        let text = activity_panel_compact_line(self.model, self.glyphs);
        let line = pad_to_width(&text, area.width as usize);
        buf.set_stringn(area.x, area.y, line, area.width as usize, style);
    }

    fn render_panel(&self, area: Rect, buf: &mut Buffer) {
        let title_style = self.severity_style().add_modifier(Modifier::BOLD);
        let body_style = Style::default().fg(self.theme.text);
        let border_style = Style::default().fg(self.theme.border);
        let border = self.glyphs.border;

        let inner_width = area.width.saturating_sub(2) as usize;
        let title = format!(
            " {}{}{} ",
            self.model.title,
            self.glyphs.separator(),
            self.model.stage
        );
        let title = clip_to_width(&title, inner_width);
        let title_width = UnicodeWidthStr::width(title.as_str());
        let fill = border
            .horizontal_top
            .repeat(inner_width.saturating_sub(title_width));
        let top = format!("{}{}{}{}", border.top_left, title, fill, border.top_right);
        buf.set_stringn(area.x, area.y, top, area.width as usize, border_style);
        if !title.is_empty() {
            buf.set_stringn(
                area.x.saturating_add(1),
                area.y,
                title,
                inner_width,
                title_style,
            );
        }

        let detail = activity_panel_detail_line(self.model, self.glyphs);
        let detail = pad_to_width(&detail, inner_width);
        let middle = format!(
            "{}{}{}",
            border.vertical_left, detail, border.vertical_right
        );
        buf.set_stringn(
            area.x,
            area.y.saturating_add(1),
            middle,
            area.width as usize,
            body_style,
        );
        buf.set_stringn(
            area.x,
            area.y.saturating_add(1),
            border.vertical_left,
            1,
            border_style,
        );
        buf.set_stringn(
            area.x.saturating_add(area.width.saturating_sub(1)),
            area.y.saturating_add(1),
            border.vertical_right,
            1,
            border_style,
        );

        let bottom_body = border.horizontal_bottom.repeat(inner_width);
        let bottom = format!(
            "{}{}{}",
            border.bottom_left, bottom_body, border.bottom_right
        );
        buf.set_stringn(
            area.x,
            area.y.saturating_add(2),
            bottom,
            area.width as usize,
            border_style,
        );
    }

    fn severity_style(&self) -> Style {
        let color = match self.model.severity {
            ActivityPanelSeverity::Info => self.theme.info,
            ActivityPanelSeverity::Working => self.theme.primary,
            ActivityPanelSeverity::Waiting => self.theme.warning,
            ActivityPanelSeverity::Success => self.theme.success,
            ActivityPanelSeverity::Warning => self.theme.warning,
            ActivityPanelSeverity::Error => self.theme.error,
        };
        Style::default().fg(color)
    }
}

fn activity_panel_compact_line(model: &ActivityPanelRenderModel, glyphs: RenderGlyphs) -> String {
    let mut parts = vec![model.title.clone(), model.stage.clone()];
    if let Some(summary) = model.summary.as_deref() {
        parts.push(summary.to_string());
    }
    for detail in &model.details {
        parts.push(format!("{}: {}", detail.label, detail.value));
    }
    parts.retain(|part| !part.trim().is_empty());
    parts.join(glyphs.separator())
}

fn activity_panel_detail_line(model: &ActivityPanelRenderModel, glyphs: RenderGlyphs) -> String {
    let mut parts = Vec::new();
    if let Some(summary) = model.summary.as_deref() {
        parts.push(summary.to_string());
    }
    for detail in &model.details {
        parts.push(format!("{}: {}", detail.label, detail.value));
    }
    if parts.is_empty() {
        model.stage.clone()
    } else {
        parts.join(glyphs.separator())
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
    let text_width = UnicodeWidthStr::width(text);
    if text_width <= width {
        return text.to_string();
    }
    if width <= suffix.len() {
        return ".".repeat(width);
    }

    let mut out = String::new();
    let mut used = 0usize;
    let suffix_width = UnicodeWidthStr::width(suffix);
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
    use ratatui::buffer::Buffer;

    fn render_panel(model: &ActivityPanelRenderModel, width: u16, glyphs: RenderGlyphs) -> String {
        let theme = Theme::default();
        let height = ActivityPanelWidget::new(model, &theme)
            .glyphs(glyphs)
            .required_height(width);
        let mut buf = Buffer::empty(Rect::new(0, 0, width, height.max(1)));
        ActivityPanelWidget::new(model, &theme)
            .glyphs(glyphs)
            .render(buf.area, &mut buf);
        let mut out = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                out.push_str(buf.content[buf.index_of(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    #[test]
    fn activity_panel_renders_command_activity_details() {
        let model = ActivityPanelRenderModel::new(
            "running command",
            "Command output",
            ActivityPanelSeverity::Working,
        )
        .summary("stdout 8 lines shown, 112 hidden")
        .detail("command", "cargo test -p mossen-tui")
        .detail("cwd", "/repo");

        let rendered = render_panel(&model, 96, RenderGlyphs::unicode());

        assert!(rendered.contains("Command output"), "{rendered}");
        assert!(rendered.contains("running command"), "{rendered}");
        assert!(rendered.contains("stdout 8 lines shown"), "{rendered}");
        assert!(rendered.contains("cargo test -p mossen-tui"), "{rendered}");
    }

    #[test]
    fn activity_panel_clips_compact_width() {
        let model = ActivityPanelRenderModel::new(
            "running command",
            "Command output",
            ActivityPanelSeverity::Working,
        )
        .summary("stdout with a very long detail line that must be clipped");

        let rendered = render_panel(&model, 32, RenderGlyphs::ascii());

        assert!(rendered.contains("Command output"), "{rendered}");
        assert!(rendered.contains("..."), "{rendered}");
        for line in rendered.lines() {
            assert_eq!(UnicodeWidthStr::width(line), 32, "{rendered}");
        }
    }

    #[test]
    fn activity_panel_uses_ascii_border_profile() {
        let model = ActivityPanelRenderModel::new(
            "waiting approval",
            "Shell Command",
            ActivityPanelSeverity::Waiting,
        )
        .summary("Command: cargo test");

        let rendered = render_panel(&model, 72, RenderGlyphs::ascii());

        assert!(rendered.contains("+ Shell Command"), "{rendered}");
        assert!(rendered.contains("Command: cargo test"), "{rendered}");
        assert!(!rendered.contains("╭"), "{rendered}");
    }
}
