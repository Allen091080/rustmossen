//! Footer/status-line renderer for the semantic render surface.
//!
//! This is the Layer 3 renderer for `FooterRenderModel`: App builds footer
//! facts once in Layer 2, and this widget only decides how to fit them into
//! terminal cells.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Widget,
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::render_glyphs::RenderGlyphs;
use crate::render_model::{BlockingKind, FooterItem, FooterRenderModel};
use crate::render_profile::RendererProfile;
use crate::theme::Theme;

pub struct FooterWidget<'a> {
    footer: &'a FooterRenderModel,
    theme: &'a Theme,
    glyphs: RenderGlyphs,
}

impl<'a> FooterWidget<'a> {
    pub fn new(footer: &'a FooterRenderModel, theme: &'a Theme) -> Self {
        Self {
            footer,
            theme,
            glyphs: RenderGlyphs::default(),
        }
    }

    pub fn glyphs(mut self, glyphs: RenderGlyphs) -> Self {
        self.glyphs = glyphs;
        self
    }
}

impl<'a> Widget for FooterWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let area = area.intersection(buf.area);
        if area.width == 0 || area.height == 0 {
            return;
        }

        let bg = Style::default().bg(self.theme.surface);
        for x in area.x..area.right() {
            buf.set_string(x, area.y, " ", bg);
        }

        let profile = RendererProfile::from_width(area.width);
        let mut left_spans = Vec::new();

        if matches!(profile, RendererProfile::Small) {
            push_status_span(&mut left_spans, self.footer, self.theme);
            if self.footer.config.is_enabled(FooterItem::Model) {
                push_left_item_span(
                    &mut left_spans,
                    FooterItem::Model,
                    self.footer,
                    self.theme,
                    self.glyphs,
                );
            }
        } else {
            for item in [
                FooterItem::Project,
                FooterItem::Model,
                FooterItem::AccessMode,
                FooterItem::Reasoning,
            ] {
                if self.footer.config.is_enabled(item) {
                    push_left_item_span(
                        &mut left_spans,
                        item,
                        self.footer,
                        self.theme,
                        self.glyphs,
                    );
                }
            }
            push_status_span(&mut left_spans, self.footer, self.theme);
            for item in [
                FooterItem::Activity,
                FooterItem::TurnState,
                FooterItem::McpSummary,
            ] {
                if self.footer.config.is_enabled(item) {
                    push_left_item_span(
                        &mut left_spans,
                        item,
                        self.footer,
                        self.theme,
                        self.glyphs,
                    );
                }
            }
        }

        let right_text = right_footer_text(self.footer);
        let right_display =
            truncate_start_display_width(&right_text, area.width as usize, self.glyphs);
        let right_width = display_width_u16(&right_display);
        let left_width = area.width.saturating_sub(right_width.saturating_add(1));
        if left_width > 0 {
            buf.set_line(area.x, area.y, &Line::from(left_spans), left_width);
        }
        let right_x = area.x + area.width.saturating_sub(right_width);
        if right_width > 0 {
            buf.set_string(
                right_x,
                area.y,
                &right_display,
                Style::default()
                    .fg(self.theme.text_dim)
                    .bg(self.theme.surface),
            );
        }
    }
}

fn push_left_item_span<'a>(
    spans: &mut Vec<Span<'a>>,
    item: FooterItem,
    footer: &FooterRenderModel,
    theme: &Theme,
    glyphs: RenderGlyphs,
) {
    match item {
        FooterItem::Project => {
            if let Some(project) = footer.project.as_deref() {
                let short = project_tail(project);
                spans.push(Span::styled(
                    format!(" {} {short} ", glyphs.project),
                    Style::default().fg(theme.text_dim).bg(theme.surface),
                ));
            }
        }
        FooterItem::Model => {
            if let Some(model) = footer.model.as_deref() {
                spans.push(Span::styled(
                    format!(" {model} "),
                    Style::default().fg(theme.primary).bg(theme.surface),
                ));
            }
        }
        FooterItem::AccessMode => {
            let access_mode = footer.access_mode.as_deref().unwrap_or("Supervised");
            spans.push(Span::styled(
                format!(" {access_mode} "),
                Style::default().fg(theme.text_dim).bg(theme.surface),
            ));
        }
        FooterItem::Reasoning => {
            if let Some(reasoning) = footer.reasoning.as_deref() {
                spans.push(Span::styled(
                    format!(" reasoning:{reasoning} "),
                    Style::default().fg(theme.secondary).bg(theme.surface),
                ));
            }
        }
        FooterItem::Activity => {
            if let Some(activity) = footer.activity.as_deref() {
                spans.push(Span::styled(
                    format!(" {activity} "),
                    Style::default().fg(theme.text_dim).bg(theme.surface),
                ));
            }
        }
        FooterItem::TurnState => {
            if is_thinking_state(footer.turn_state.as_deref()) {
                spans.push(Span::styled(
                    format!(" {} ", glyphs.thinking),
                    Style::default().fg(theme.secondary).bg(theme.surface),
                ));
            }
        }
        FooterItem::McpSummary => {
            if let Some(mcp) = footer.mcp_summary.as_deref() {
                spans.push(Span::styled(
                    format!(" {mcp} "),
                    Style::default().fg(theme.info).bg(theme.surface),
                ));
            }
        }
        FooterItem::Context
        | FooterItem::Cost
        | FooterItem::MessageCount
        | FooterItem::ExternalStatus => {}
    }
}

fn push_status_span<'a>(spans: &mut Vec<Span<'a>>, footer: &FooterRenderModel, theme: &Theme) {
    if let Some(blocking) = footer.blocking.as_ref() {
        let (label, style) = blocking_badge(blocking.kind, theme);
        spans.push(Span::styled(label, style));
    } else if let Some(turn_state) = footer.turn_state.as_deref() {
        spans.push(Span::styled(
            format!(" {turn_state} "),
            Style::default().fg(theme.text_dim).bg(theme.surface),
        ));
    }
}

fn project_tail(project: &str) -> String {
    std::path::Path::new(project)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| project.to_string())
}

fn blocking_badge(kind: BlockingKind, theme: &Theme) -> (&'static str, Style) {
    match kind {
        BlockingKind::Approval => (
            " approval required",
            Style::default()
                .fg(theme.background)
                .bg(theme.warning)
                .add_modifier(Modifier::BOLD),
        ),
        BlockingKind::Error => (
            " error ",
            Style::default()
                .fg(theme.background)
                .bg(theme.error)
                .add_modifier(Modifier::BOLD),
        ),
        BlockingKind::CostLimit => (
            " cost limit ",
            Style::default()
                .fg(theme.background)
                .bg(theme.warning)
                .add_modifier(Modifier::BOLD),
        ),
        BlockingKind::IdleReturn => (
            " idle ",
            Style::default()
                .fg(theme.text_dim)
                .bg(theme.surface)
                .add_modifier(Modifier::ITALIC),
        ),
        BlockingKind::Info => (
            " notice ",
            Style::default().fg(theme.info).bg(theme.surface),
        ),
    }
}

fn is_thinking_state(turn_state: Option<&str>) -> bool {
    matches!(
        turn_state,
        Some(
            "thinking"
                | "planning"
                | "reading repo"
                | "editing files"
                | "waiting approval"
                | "running command"
                | "reviewing result"
                | "retrying"
                | "streaming"
                | "running tool"
        )
    )
}

fn right_footer_text(footer: &FooterRenderModel) -> String {
    let mut parts = Vec::new();
    for item in &footer.config.right_items {
        match item {
            FooterItem::Context => {
                if let Some(context) = footer.context {
                    parts.push(context.label());
                }
            }
            FooterItem::Cost => {
                if let Some(cost) = footer.cost.as_deref() {
                    parts.push(cost.to_string());
                }
            }
            FooterItem::MessageCount => {
                if let Some(count) = footer.message_count {
                    parts.push(format!("{count} msgs"));
                }
            }
            FooterItem::ExternalStatus => {
                if let Some(status) = footer.external_status.as_deref() {
                    parts.push(status.to_string());
                }
            }
            FooterItem::Project
            | FooterItem::Model
            | FooterItem::AccessMode
            | FooterItem::Reasoning
            | FooterItem::Activity
            | FooterItem::TurnState
            | FooterItem::McpSummary => {}
        }
    }
    parts.join("  ")
}

fn display_width_u16(text: &str) -> u16 {
    UnicodeWidthStr::width(text).min(u16::MAX as usize) as u16
}

fn truncate_start_display_width(text: &str, max_width: usize, glyphs: RenderGlyphs) -> String {
    if max_width == 0 || text.is_empty() {
        return String::new();
    }
    if UnicodeWidthStr::width(text) <= max_width {
        return text.to_string();
    }
    let suffix = glyphs.ellipsis();
    let suffix_width = UnicodeWidthStr::width(suffix);
    if suffix_width >= max_width {
        return truncate_display_width(suffix, max_width);
    }

    let mut tail = Vec::new();
    let mut used = suffix_width;
    for grapheme in text.graphemes(true).rev() {
        let width = UnicodeWidthStr::width(grapheme);
        if used + width > max_width {
            break;
        }
        used += width;
        tail.push(grapheme);
    }
    tail.reverse();
    format!("{}{}", suffix, tail.concat())
}

fn truncate_display_width(text: &str, max_width: usize) -> String {
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
    use crate::render_glyphs::RenderGlyphs;
    use crate::render_model::{BlockingRenderModel, ContextUsageRenderModel, FooterRenderConfig};
    use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};

    fn render_footer(footer: &FooterRenderModel, width: u16) -> String {
        let theme = Theme::default();
        let mut buf = Buffer::empty(Rect::new(0, 0, width, 1));
        FooterWidget::new(footer, &theme).render(Rect::new(0, 0, width, 1), &mut buf);
        let mut out = String::new();
        for x in 0..width {
            out.push_str(buf[(x, 0)].symbol());
        }
        out
    }

    fn render_footer_with_glyphs(
        footer: &FooterRenderModel,
        width: u16,
        glyphs: RenderGlyphs,
    ) -> String {
        let theme = Theme::default();
        let mut buf = Buffer::empty(Rect::new(0, 0, width, 1));
        FooterWidget::new(footer, &theme)
            .glyphs(glyphs)
            .render(Rect::new(0, 0, width, 1), &mut buf);
        let mut out = String::new();
        for x in 0..width {
            out.push_str(buf[(x, 0)].symbol());
        }
        out
    }

    #[test]
    fn footer_widget_renders_semantic_footer_model_directly() {
        let footer = FooterRenderModel {
            project: Some("/Users/allen/Documents/rustmossen/逐行阅读项目".to_string()),
            model: Some("MiniMax-M2.7".to_string()),
            access_mode: Some("Supervised".to_string()),
            reasoning: Some("low".to_string()),
            context: Some(ContextUsageRenderModel {
                used_tokens: 24_000,
                window_tokens: 200_000,
            }),
            turn_state: Some("waiting approval".to_string()),
            activity: Some("approval: Bash".to_string()),
            cost: Some("$0.15".to_string()),
            message_count: Some(17),
            mcp_summary: Some("2 MCP".to_string()),
            external_status: None,
            blocking: Some(BlockingRenderModel::approval(
                "Shell Command",
                "Command: ls -la",
            )),
            config: FooterRenderConfig::default(),
        };

        let rendered = render_footer(&footer, 120);

        for ch in ['逐', '行', '阅', '读', '项', '目'] {
            assert!(rendered.contains(ch), "{rendered}");
        }
        assert!(rendered.contains("MiniMax-M2.7"), "{rendered}");
        assert!(rendered.contains("reasoning:low"), "{rendered}");
        assert!(rendered.contains("approval required"), "{rendered}");
        assert!(rendered.contains("approval: Bash"), "{rendered}");
        assert!(rendered.contains("ctx 24k/200k"), "{rendered}");
        assert!(rendered.contains("$0.15"), "{rendered}");
        assert!(rendered.contains("17 msgs"), "{rendered}");
    }

    #[test]
    fn footer_widget_renders_external_status_when_configured() {
        let mut config = FooterRenderConfig::default();
        config.set_enabled(FooterItem::ExternalStatus, true);
        let footer = FooterRenderModel {
            model: Some("MiniMax-M2.7".to_string()),
            external_status: Some("branch main".to_string()),
            config,
            ..FooterRenderModel::default()
        };

        let rendered = render_footer(&footer, 96);

        assert!(rendered.contains("MiniMax-M2.7"), "{rendered}");
        assert!(rendered.contains("branch main"), "{rendered}");
    }

    #[test]
    fn footer_widget_keeps_right_metrics_visible_when_tiny() {
        let footer = FooterRenderModel {
            cost: Some("$999.99".to_string()),
            message_count: Some(123456),
            ..FooterRenderModel::default()
        };

        let rendered = render_footer(&footer, 8);

        assert!(rendered.contains("msgs"), "{rendered}");
        assert_eq!(UnicodeWidthStr::width(rendered.as_str()), 8);
    }

    #[test]
    fn footer_small_profile_keeps_core_status_and_hides_secondary_fields() {
        let footer = FooterRenderModel {
            project: Some("/repo/mossen".to_string()),
            model: Some("gpt-test".to_string()),
            access_mode: Some("Full Auto".to_string()),
            reasoning: Some("high".to_string()),
            turn_state: Some("running command".to_string()),
            activity: Some("cmd: cargo test".to_string()),
            cost: Some("$0.01".to_string()),
            message_count: Some(8),
            mcp_summary: Some("3 MCP".to_string()),
            ..FooterRenderModel::default()
        };

        let rendered = render_footer(&footer, 72);

        assert!(rendered.contains("running command"), "{rendered}");
        assert!(rendered.contains("gpt-test"), "{rendered}");
        assert!(
            rendered.contains("$0.01") || rendered.contains("8 msgs"),
            "{rendered}"
        );
        assert!(!rendered.contains("mossen"), "{rendered}");
        assert!(!rendered.contains("Full Auto"), "{rendered}");
        assert!(!rendered.contains("reasoning:high"), "{rendered}");
        assert!(!rendered.contains("cargo test"), "{rendered}");
        assert!(!rendered.contains("3 MCP"), "{rendered}");
    }

    #[test]
    fn footer_medium_profile_keeps_model_when_project_is_absent() {
        let footer = FooterRenderModel {
            model: Some("gpt-test".to_string()),
            access_mode: Some("Supervised".to_string()),
            reasoning: Some("low".to_string()),
            turn_state: Some("thinking".to_string()),
            ..FooterRenderModel::default()
        };

        let rendered = render_footer(&footer, 100);

        assert!(rendered.contains("gpt-test"), "{rendered}");
        assert!(rendered.contains("Supervised"), "{rendered}");
        assert!(rendered.contains("reasoning:low"), "{rendered}");
        assert!(rendered.contains("thinking"), "{rendered}");
    }

    #[test]
    fn footer_can_render_ascii_status_glyphs() {
        let footer = FooterRenderModel {
            project: Some("/repo/mossen".to_string()),
            model: Some("gpt-test".to_string()),
            turn_state: Some("thinking".to_string()),
            message_count: Some(3),
            ..FooterRenderModel::default()
        };

        let rendered = render_footer_with_glyphs(&footer, 80, RenderGlyphs::ascii());

        assert!(rendered.contains("dir mossen"), "{rendered}");
        assert!(rendered.contains("?"), "{rendered}");
        assert!(!rendered.contains("📁"), "{rendered}");
        assert!(!rendered.contains("💭"), "{rendered}");
    }

    #[test]
    fn footer_widget_honors_item_config_but_keeps_core_status() {
        let mut config = FooterRenderConfig::default();
        config.set_enabled(FooterItem::Project, false);
        config.set_enabled(FooterItem::Model, false);
        config.set_enabled(FooterItem::Cost, false);
        config.set_enabled(FooterItem::MessageCount, false);

        let footer = FooterRenderModel {
            project: Some("/repo/mossen".to_string()),
            model: Some("gpt-test".to_string()),
            turn_state: Some("running command".to_string()),
            activity: Some("cmd: cargo test".to_string()),
            context: Some(ContextUsageRenderModel {
                used_tokens: 1_000,
                window_tokens: 10_000,
            }),
            cost: Some("$0.42".to_string()),
            message_count: Some(9),
            config,
            ..FooterRenderModel::default()
        };

        let rendered = render_footer(&footer, 120);

        assert!(rendered.contains("running command"), "{rendered}");
        assert!(rendered.contains("cmd: cargo test"), "{rendered}");
        assert!(rendered.contains("ctx 1k/10k"), "{rendered}");
        assert!(!rendered.contains("mossen"), "{rendered}");
        assert!(!rendered.contains("gpt-test"), "{rendered}");
        assert!(!rendered.contains("$0.42"), "{rendered}");
        assert!(!rendered.contains("9 msgs"), "{rendered}");
    }
}
