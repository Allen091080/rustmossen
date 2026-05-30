//! Top status renderer for the semantic render surface.
//!
//! The footer remains configurable and detailed; this widget keeps the current
//! turn state visible at the top of the terminal on every normal frame.

use ratatui::{buffer::Buffer, layout::Rect, style::Style, widgets::Widget};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::render_glyphs::RenderGlyphs;
use crate::render_model::{BlockingKind, TopStatusRenderModel};
use crate::theme::Theme;

pub struct StatusHeaderWidget<'a> {
    status: &'a TopStatusRenderModel,
    theme: &'a Theme,
    glyphs: RenderGlyphs,
}

impl<'a> StatusHeaderWidget<'a> {
    pub fn new(status: &'a TopStatusRenderModel, theme: &'a Theme) -> Self {
        Self {
            status,
            theme,
            glyphs: RenderGlyphs::default(),
        }
    }

    pub fn glyphs(mut self, glyphs: RenderGlyphs) -> Self {
        self.glyphs = glyphs;
        self
    }
}

impl<'a> Widget for StatusHeaderWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let area = area.intersection(buf.area);
        if area.width == 0 || area.height == 0 {
            return;
        }

        let bg = Style::default().bg(self.theme.surface);
        for x in area.x..area.right() {
            buf.set_string(x, area.y, " ", bg);
        }

        let text = status_header_text(self.status, self.glyphs);
        let text = truncate_display_width(&text, area.width as usize, self.glyphs);
        let style = Style::default().fg(self.theme.text).bg(self.theme.surface);
        buf.set_string(area.x, area.y, text, style);
    }
}

fn status_header_text(status: &TopStatusRenderModel, glyphs: RenderGlyphs) -> String {
    let sep = glyphs.separator();
    let mut parts = Vec::new();

    if let Some(blocking) = status.blocking.as_ref() {
        parts.push(format!(
            "{}: {}",
            blocking_header_label(blocking.kind),
            blocking.title
        ));
        if !blocking.detail.trim().is_empty() {
            parts.push(blocking.detail.clone());
        }
    } else {
        let stage = status.stage.as_deref().unwrap_or("idle");
        parts.push(format!("status: {stage}"));
    }

    if let Some(activity) = status.activity.as_deref().filter(|value| !value.is_empty()) {
        parts.push(activity.to_string());
    }
    if let Some(model) = status.model.as_deref().filter(|value| !value.is_empty()) {
        parts.push(format!("model {model}"));
    }
    if let Some(access_mode) = status
        .access_mode
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        parts.push(format!("mode {access_mode}"));
    }
    if let Some(reasoning) = status
        .reasoning
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        parts.push(format!("reasoning:{reasoning}"));
    }
    if let Some(context) = status.context {
        parts.push(context.label());
    }
    if let Some(message_count) = status.message_count {
        parts.push(format!("{message_count} msgs"));
    }

    format!(" {}", parts.join(sep))
}

fn blocking_header_label(kind: BlockingKind) -> &'static str {
    match kind {
        BlockingKind::Approval => "approval required",
        BlockingKind::Error => "blocked by error",
        BlockingKind::CostLimit => "cost threshold",
        BlockingKind::IdleReturn => "idle return",
        BlockingKind::Info => "notice",
    }
}

fn truncate_display_width(text: &str, max_width: usize, glyphs: RenderGlyphs) -> String {
    if max_width == 0 || text.is_empty() {
        return String::new();
    }
    if UnicodeWidthStr::width(text) <= max_width {
        return text.to_string();
    }

    let suffix = glyphs.ellipsis();
    let suffix_width = UnicodeWidthStr::width(suffix);
    if suffix_width >= max_width {
        return truncate_exact_width(suffix, max_width);
    }

    let mut out = String::new();
    let mut used = 0usize;
    let max_body_width = max_width.saturating_sub(suffix_width);
    for grapheme in text.graphemes(true) {
        let width = UnicodeWidthStr::width(grapheme);
        if used + width > max_body_width {
            break;
        }
        out.push_str(grapheme);
        used += width;
    }
    out.push_str(suffix);
    out
}

fn truncate_exact_width(text: &str, max_width: usize) -> String {
    let mut out = String::new();
    let mut used = 0usize;
    for grapheme in text.graphemes(true) {
        let width = UnicodeWidthStr::width(grapheme);
        if used + width > max_width {
            break;
        }
        out.push_str(grapheme);
        used += width;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render_model::{BlockingRenderModel, ContextUsageRenderModel, TopStatusRenderModel};
    use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};

    fn render_header(status: &TopStatusRenderModel, width: u16) -> String {
        render_header_with_glyphs(status, width, RenderGlyphs::unicode())
    }

    fn render_header_with_glyphs(
        status: &TopStatusRenderModel,
        width: u16,
        glyphs: RenderGlyphs,
    ) -> String {
        let theme = Theme::default();
        let mut buf = Buffer::empty(Rect::new(0, 0, width, 1));
        StatusHeaderWidget::new(status, &theme)
            .glyphs(glyphs)
            .render(Rect::new(0, 0, width, 1), &mut buf);
        let mut out = String::new();
        for x in 0..width {
            out.push_str(buf[(x, 0)].symbol());
        }
        out
    }

    #[test]
    fn status_header_renders_core_session_facts() {
        let status = TopStatusRenderModel {
            stage: Some("running command".to_string()),
            activity: Some("cmd: cargo test".to_string()),
            model: Some("example-fast".to_string()),
            access_mode: Some("Supervised".to_string()),
            reasoning: Some("high".to_string()),
            context: Some(ContextUsageRenderModel {
                used_tokens: 24_000,
                window_tokens: 200_000,
            }),
            goal_status: None,
            message_count: Some(17),
            blocking: None,
        };

        let rendered = render_header(&status, 128);

        assert!(rendered.contains("status: running command"), "{rendered}");
        assert!(rendered.contains("cmd: cargo test"), "{rendered}");
        assert!(rendered.contains("model example-fast"), "{rendered}");
        assert!(rendered.contains("mode Supervised"), "{rendered}");
        assert!(rendered.contains("reasoning:high"), "{rendered}");
        assert!(rendered.contains("ctx 24k/200k"), "{rendered}");
        assert!(rendered.contains("17 msgs"), "{rendered}");
    }

    #[test]
    fn status_header_prioritizes_blocking_and_clips_narrow_width() {
        let status = TopStatusRenderModel {
            stage: Some("running command".to_string()),
            model: Some("example-fast".to_string()),
            blocking: Some(BlockingRenderModel::approval(
                "Shell Command",
                "Risk: Medium · Command: cargo test --all-targets",
            )),
            ..TopStatusRenderModel::default()
        };

        let rendered = render_header_with_glyphs(&status, 32, RenderGlyphs::ascii());

        assert!(rendered.contains("approval required"), "{rendered}");
        assert!(rendered.contains("..."), "{rendered}");
        assert_eq!(UnicodeWidthStr::width(rendered.as_str()), 32);
    }

    #[test]
    fn status_header_uses_ascii_separator_when_requested() {
        let status = TopStatusRenderModel {
            stage: Some("thinking".to_string()),
            model: Some("gpt-test".to_string()),
            reasoning: Some("low".to_string()),
            ..TopStatusRenderModel::default()
        };

        let rendered = render_header_with_glyphs(&status, 80, RenderGlyphs::ascii());

        assert!(
            rendered.contains("status: thinking - model gpt-test"),
            "{rendered}"
        );
        assert!(!rendered.contains("·"), "{rendered}");
    }
}
