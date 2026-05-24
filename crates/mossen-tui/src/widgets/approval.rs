//! Inline approval renderer for the semantic rendering pipeline.
//!
//! This widget consumes `ApprovalRenderModel`; app state decides what is
//! pending, while this layer only lays out the decision surface.

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget, Wrap},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::render_glyphs::RenderGlyphs;
use crate::render_model::{ApprovalAction, ApprovalRenderModel};
use crate::theme::Theme;

pub struct ApprovalBlockWidget<'a> {
    model: &'a ApprovalRenderModel,
    theme: &'a Theme,
    glyphs: RenderGlyphs,
}

impl<'a> ApprovalBlockWidget<'a> {
    pub fn new(model: &'a ApprovalRenderModel, theme: &'a Theme) -> Self {
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
            return 0;
        }
        let inner_width = width.saturating_sub(2).max(1);
        let body_lines = approval_body_line_budget(self.model, inner_width);
        (7 + body_lines).min(13)
    }
}

impl<'a> Widget for ApprovalBlockWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let area = area.intersection(buf.area);
        if area.width < 20 || area.height < 5 {
            return;
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(self.glyphs.border)
            .border_style(Style::default().fg(self.theme.permission))
            .title(Span::styled(
                format!(" {} ", self.model.title),
                Style::default()
                    .fg(self.theme.permission)
                    .add_modifier(Modifier::BOLD),
            ));
        let inner = block.inner(area);
        block.render(area, buf);
        if inner.width == 0 || inner.height < 3 {
            return;
        }

        let body_height = inner.height.saturating_sub(5);
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(body_height),
                Constraint::Length(1),
            ])
            .split(inner);

        let waiting = Line::from(vec![
            Span::styled(
                "Waiting for approval",
                Style::default()
                    .fg(self.theme.permission)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  Enter confirm{}Esc deny", self.glyphs.separator()),
                Style::default().fg(self.theme.text_dim),
            ),
        ]);
        buf.set_line(chunks[0].x, chunks[0].y, &waiting, chunks[0].width);

        let tool_line = Line::from(vec![
            Span::styled("Tool: ", Style::default().fg(self.theme.text_dim)),
            Span::styled(
                self.model.tool_name.clone(),
                Style::default()
                    .fg(self.theme.text)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);
        buf.set_line(chunks[1].x, chunks[1].y, &tool_line, chunks[1].width);

        let detail_line = Line::from(vec![
            Span::styled(
                format!("{}: ", self.model.detail_label),
                Style::default().fg(self.theme.text_dim),
            ),
            Span::styled(
                self.model.detail.clone(),
                Style::default().fg(self.theme.primary),
            ),
        ]);
        buf.set_line(chunks[2].x, chunks[2].y, &detail_line, chunks[2].width);

        let risk_line = Line::from(vec![
            Span::styled("Risk: ", Style::default().fg(self.theme.text_dim)),
            Span::styled(
                self.model.risk.label().to_string(),
                Style::default()
                    .fg(risk_color(self.model.risk, self.theme))
                    .add_modifier(Modifier::BOLD),
            ),
        ]);
        buf.set_line(chunks[3].x, chunks[3].y, &risk_line, chunks[3].width);

        if body_height > 0 && !self.model.body.trim().is_empty() {
            let body = if self.model.expanded {
                self.model.body.clone()
            } else {
                truncate_display_width_with_suffix(
                    self.model.body.lines().next().unwrap_or_default(),
                    chunks[4].width as usize,
                    self.glyphs.ellipsis(),
                )
            };
            Paragraph::new(body)
                .style(Style::default().fg(self.theme.text_dim))
                .wrap(Wrap { trim: true })
                .render(chunks[4], buf);
        }

        let action_line = Line::from(
            self.model
                .actions
                .iter()
                .flat_map(|action| {
                    let selected = *action == self.model.selected_action;
                    let color = action_color(*action, self.theme);
                    let style = if selected {
                        Style::default()
                            .fg(self.theme.text)
                            .bg(color)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(color)
                    };
                    [
                        Span::styled(format!(" {} ", action.label()), style),
                        Span::raw("  "),
                    ]
                })
                .collect::<Vec<_>>(),
        );
        buf.set_line(chunks[5].x, chunks[5].y, &action_line, chunks[5].width);
    }
}

fn approval_body_line_budget(model: &ApprovalRenderModel, width: u16) -> u16 {
    if model.body.trim().is_empty() {
        return 0;
    }
    if !model.expanded {
        return 1;
    }
    crate::widgets::markdown::wrapped_line_count_for_text(&model.body, width)
        .min(6)
        .max(1) as u16
}

fn truncate_display_width_with_suffix(text: &str, max_cells: usize, suffix: &str) -> String {
    if max_cells == 0 {
        return String::new();
    }
    if UnicodeWidthStr::width(text) <= max_cells {
        return text.to_string();
    }

    let suffix_width = UnicodeWidthStr::width(suffix);
    if suffix_width >= max_cells {
        return truncate_display_width(suffix, max_cells);
    }

    let mut out = truncate_display_width(text, max_cells - suffix_width);
    out.push_str(suffix);
    out
}

fn truncate_display_width(text: &str, max_cells: usize) -> String {
    let mut out = String::new();
    let mut width = 0usize;
    for ch in text.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if width.saturating_add(ch_width) > max_cells {
            break;
        }
        out.push(ch);
        width += ch_width;
    }
    out
}

fn action_color(action: ApprovalAction, theme: &Theme) -> ratatui::style::Color {
    match action {
        ApprovalAction::Allow => theme.success,
        ApprovalAction::AlwaysAllow => theme.info,
        ApprovalAction::EditCommand => theme.warning,
        ApprovalAction::Deny => theme.error,
    }
}

fn risk_color(
    risk: crate::render_model::ApprovalRiskLevel,
    theme: &Theme,
) -> ratatui::style::Color {
    match risk {
        crate::render_model::ApprovalRiskLevel::Low => theme.success,
        crate::render_model::ApprovalRiskLevel::Medium => theme.warning,
        crate::render_model::ApprovalRiskLevel::High => theme.error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render_glyphs::RenderGlyphs;
    use crate::render_model::ApprovalRiskLevel;
    use ratatui::buffer::Buffer;

    #[test]
    fn renders_semantic_approval_without_permission_state() {
        let model = ApprovalRenderModel {
            id: "approval-1".to_string(),
            tool_name: "Bash".to_string(),
            title: "Shell Command".to_string(),
            detail_label: "Command".to_string(),
            detail: "cargo test".to_string(),
            risk: ApprovalRiskLevel::Medium,
            body: "Command requires shell execution in the workspace.".to_string(),
            actions: vec![
                ApprovalAction::Allow,
                ApprovalAction::AlwaysAllow,
                ApprovalAction::EditCommand,
                ApprovalAction::Deny,
            ],
            selected_action: ApprovalAction::EditCommand,
            anchor_block_id: Some("tool-1".to_string()),
            expanded: true,
        };
        let theme = Theme::default();
        let mut buf = Buffer::empty(Rect::new(0, 0, 88, 9));

        ApprovalBlockWidget::new(&model, &theme).render(buf.area, &mut buf);

        let rendered = buffer_text(&buf);
        assert!(rendered.contains("Shell Command"));
        assert!(rendered.contains("Waiting for approval"));
        assert!(rendered.contains("Tool: Bash"));
        assert!(rendered.contains("Command: cargo test"));
        assert!(rendered.contains("Risk: Medium"));
        assert!(rendered.contains("Edit command"));
        assert!(rendered.contains("Always"));
    }

    #[test]
    fn required_height_keeps_collapsed_body_visible() {
        let model = ApprovalRenderModel {
            id: "approval-body".to_string(),
            tool_name: "Bash".to_string(),
            title: "Shell Command".to_string(),
            detail_label: "Command".to_string(),
            detail: "ls -la".to_string(),
            risk: ApprovalRiskLevel::Low,
            body: "审批说明：这个命令需要在当前项目中执行，并且说明文字很长".to_string(),
            actions: vec![
                ApprovalAction::Allow,
                ApprovalAction::AlwaysAllow,
                ApprovalAction::Deny,
            ],
            selected_action: ApprovalAction::Allow,
            anchor_block_id: Some("tool-body".to_string()),
            expanded: false,
        };
        let theme = Theme::default();
        let height = ApprovalBlockWidget::new(&model, &theme).required_height(48);
        let mut buf = Buffer::empty(Rect::new(0, 0, 48, height));

        ApprovalBlockWidget::new(&model, &theme).render(buf.area, &mut buf);

        let rendered = buffer_text(&buf);
        assert!(rendered.contains('审') && rendered.contains('批'));
        assert!(rendered.contains("Allow"));
    }

    #[test]
    fn clips_partial_area_to_buffer_before_rendering() {
        let model = ApprovalRenderModel {
            id: "approval-clip".to_string(),
            tool_name: "Bash".to_string(),
            title: "Shell Command".to_string(),
            detail_label: "Command".to_string(),
            detail: "printf 逐行读代码".to_string(),
            risk: ApprovalRiskLevel::Medium,
            body: "Approval body can be longer than the remaining viewport.".to_string(),
            actions: vec![
                ApprovalAction::Allow,
                ApprovalAction::AlwaysAllow,
                ApprovalAction::Deny,
            ],
            selected_action: ApprovalAction::Allow,
            anchor_block_id: Some("tool-clip".to_string()),
            expanded: true,
        };
        let theme = Theme::default();
        let mut buf = Buffer::empty(Rect::new(0, 0, 48, 8));

        ApprovalBlockWidget::new(&model, &theme).render(Rect::new(12, 2, 80, 12), &mut buf);
        ApprovalBlockWidget::new(&model, &theme).render(Rect::new(90, 90, 20, 8), &mut buf);

        let rendered = buffer_text(&buf);
        assert!(rendered.contains("Shell Command"));
    }

    #[test]
    fn approval_can_render_with_ascii_border_profile() {
        let model = ApprovalRenderModel {
            id: "approval-ascii".to_string(),
            tool_name: "Bash".to_string(),
            title: "Shell Command".to_string(),
            detail_label: "Command".to_string(),
            detail: "cargo test".to_string(),
            risk: ApprovalRiskLevel::Medium,
            body: "Validate before final summary.".to_string(),
            actions: vec![ApprovalAction::Allow, ApprovalAction::Deny],
            selected_action: ApprovalAction::Allow,
            anchor_block_id: None,
            expanded: true,
        };
        let theme = Theme::default();
        let mut buf = Buffer::empty(Rect::new(0, 0, 80, 9));

        ApprovalBlockWidget::new(&model, &theme)
            .glyphs(RenderGlyphs::ascii())
            .render(buf.area, &mut buf);

        let rendered = buffer_text(&buf);
        assert!(rendered.contains("+ Shell Command"), "{rendered}");
        assert!(rendered.contains("Waiting for approval"), "{rendered}");
        assert!(rendered.contains('|'), "{rendered}");
        for forbidden in ["╭", "╰", "│", "─"] {
            assert!(
                !rendered.contains(forbidden),
                "ASCII approval leaked unicode glyph {forbidden:?}\n{rendered}"
            );
        }
    }

    fn buffer_text(buf: &Buffer) -> String {
        let mut out = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                out.push_str(buf.content[buf.index_of(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }
}
