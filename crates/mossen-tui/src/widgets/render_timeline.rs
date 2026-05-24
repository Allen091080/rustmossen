//! `/timeline` structured render-event timeline widget.

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
use crate::render_model::{RenderTimelineRenderModel, RenderTimelineRowRenderModel};
use crate::theme::Theme;

pub struct RenderTimelineWidget<'a> {
    model: &'a RenderTimelineRenderModel,
    theme: &'a Theme,
    glyphs: RenderGlyphs,
    selected: usize,
    scroll: usize,
}

impl<'a> RenderTimelineWidget<'a> {
    pub fn new(model: &'a RenderTimelineRenderModel, theme: &'a Theme) -> Self {
        Self {
            model,
            theme,
            glyphs: RenderGlyphs::default(),
            selected: 0,
            scroll: 0,
        }
    }

    pub fn glyphs(mut self, glyphs: RenderGlyphs) -> Self {
        self.glyphs = glyphs;
        self
    }

    pub fn selected(mut self, selected: usize) -> Self {
        self.selected = selected;
        self
    }

    pub fn scroll(mut self, scroll: usize) -> Self {
        self.scroll = scroll;
        self
    }
}

impl<'a> Widget for RenderTimelineWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 32 || area.height < 6 {
            return;
        }

        let block = Block::default()
            .title(Span::styled(
                " Render Timeline ",
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

        let summary = timeline_summary_line(self.model, self.glyphs);
        buf.set_stringn(
            inner.x,
            inner.y,
            clip_to_width(&summary, inner.width as usize),
            inner.width as usize,
            Style::default().fg(self.theme.text_dim),
        );

        if inner.height < 3 {
            return;
        }

        let footer_y = inner.y.saturating_add(inner.height.saturating_sub(1));
        let footer = format!(
            "Esc closes{}Up/Down selects{}Home/End jump",
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
        let rows_bottom = footer_y;
        if self.model.rows.is_empty() {
            buf.set_stringn(
                inner.x,
                y,
                "No render events recorded.",
                inner.width as usize,
                Style::default().fg(self.theme.text_dim),
            );
            return;
        }

        let selected = self.selected.min(self.model.rows.len().saturating_sub(1));
        for (index, row) in self.model.rows.iter().enumerate().skip(self.scroll) {
            if y >= rows_bottom {
                break;
            }
            let selected_row = index == selected;
            self.render_row_header(row, selected_row, inner.x, y, inner.width, buf);
            y = y.saturating_add(1);

            if selected_row {
                for line in selected_timeline_detail(row, self.glyphs) {
                    if y >= rows_bottom {
                        break;
                    }
                    buf.set_stringn(
                        inner.x,
                        y,
                        clip_to_width(&format!("  {line}"), inner.width as usize),
                        inner.width as usize,
                        Style::default().fg(self.theme.text_dim),
                    );
                    y = y.saturating_add(1);
                }
            }
        }
    }
}

impl RenderTimelineWidget<'_> {
    fn render_row_header(
        &self,
        row: &RenderTimelineRowRenderModel,
        selected: bool,
        x: u16,
        y: u16,
        width: u16,
        buf: &mut Buffer,
    ) {
        let marker = if selected { ">" } else { " " };
        let marker_style = if selected {
            Style::default()
                .fg(self.theme.text)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(self.theme.text_dim)
        };
        let event_style = if selected {
            Style::default()
                .fg(self.theme.text)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(self.theme.text)
        };
        let line = Line::from(vec![
            Span::styled(format!("{marker} "), marker_style),
            Span::styled(
                row.sequence_label(),
                Style::default().fg(self.theme.text_dim),
            ),
            Span::raw(" "),
            Span::styled(
                row.turn_id.as_deref().unwrap_or("-").to_string(),
                Style::default().fg(self.theme.text_subtle),
            ),
            Span::raw(" "),
            Span::styled(format!("[{}]", row.stage), self.stage_style(row)),
            Span::raw(" "),
            Span::styled(row.event.clone(), event_style),
            Span::styled(
                format!("  {}", row.summary),
                Style::default().fg(self.theme.text_dim),
            ),
        ]);
        buf.set_line(x, y, &line, width);
    }

    fn stage_style(&self, row: &RenderTimelineRowRenderModel) -> Style {
        let color = match row.stage.as_str() {
            "failed" | "cancelled" => self.theme.error,
            "waiting approval" => self.theme.warning,
            "done" => self.theme.success,
            "idle" => self.theme.text_dim,
            _ => self.theme.primary,
        };
        Style::default().fg(color)
    }
}

fn timeline_summary_line(model: &RenderTimelineRenderModel, glyphs: RenderGlyphs) -> String {
    let sep = glyphs.separator();
    format!(
        "events: {}{sep}turns: {}{sep}immediate: {}{sep}throttled: {}{sep}passive: {}{sep}append: {}{sep}active: {}{sep}frozen: {}",
        model.summary.total_count,
        model.summary.turn_count,
        model.summary.immediate_count,
        model.summary.throttled_count,
        model.summary.passive_count,
        model.summary.append_count,
        model.summary.update_active_count,
        model.summary.freeze_history_count
    )
}

fn selected_timeline_detail(
    row: &RenderTimelineRowRenderModel,
    glyphs: RenderGlyphs,
) -> Vec<String> {
    let sep = glyphs.separator();
    let mut lines = vec![format!(
        "turn: {}{sep}stage: {}{sep}scope: {}{sep}refresh: {}{sep}history: {}",
        row.turn_id.as_deref().unwrap_or("-"),
        row.stage,
        row.scope,
        row.refresh,
        row.history
    )];
    if let Some(detail) = row
        .detail
        .as_deref()
        .filter(|detail| !detail.trim().is_empty())
    {
        lines.push(format!("detail: {detail}"));
    }
    lines
}

fn clip_to_width(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    let width = UnicodeWidthStr::width(text);
    if width <= max_width {
        return text.to_string();
    }
    if max_width <= 1 {
        return ".".to_string();
    }

    let mut clipped = String::new();
    let mut used = 0usize;
    for grapheme in text.graphemes(true) {
        let grapheme_width = UnicodeWidthStr::width(grapheme);
        if used.saturating_add(grapheme_width).saturating_add(1) > max_width {
            break;
        }
        clipped.push_str(grapheme);
        used = used.saturating_add(grapheme_width);
    }
    clipped.push('.');
    clipped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render_events::{
        RenderEvent, RenderEventKind, RenderEventScope, RenderHistoryPolicy, RenderRefreshPolicy,
    };
    use crate::state::UiStage;
    use ratatui::buffer::Buffer;

    fn render_widget(model: &RenderTimelineRenderModel, width: u16, height: u16) -> String {
        let theme = Theme::default();
        let mut buf = Buffer::empty(Rect::new(0, 0, width, height));
        RenderTimelineWidget::new(model, &theme).render(Rect::new(0, 0, width, height), &mut buf);
        let mut out = String::new();
        for y in 0..height {
            let mut line = String::new();
            for x in 0..width {
                line.push_str(buf[(x, y)].symbol());
            }
            out.push_str(line.trim_end());
            if y + 1 < height {
                out.push('\n');
            }
        }
        out
    }

    #[test]
    fn timeline_renders_counts_and_selected_detail() {
        let events = vec![
            RenderEvent::new(
                RenderEventKind::CommandStarted {
                    tool_id: Some("toolu-bash".to_string()),
                    command: Some("cargo test -p mossen-tui timeline".to_string()),
                    cwd: Some("/repo".to_string()),
                },
                RenderEventScope::Main,
                UiStage::RunningCommand,
            )
            .with_turn_id("turn-0001"),
            RenderEvent::new(
                RenderEventKind::CommandOutput {
                    tool_id: Some("toolu-bash".to_string()),
                    stream: "stdout".to_string(),
                    bytes: 42,
                    preview_lines: 2,
                    hidden_lines: 0,
                    total_lines: Some(2),
                    full_log_available: false,
                },
                RenderEventScope::Main,
                UiStage::RunningCommand,
            )
            .with_turn_id("turn-0001"),
        ];
        let model = RenderTimelineRenderModel::from_events(&events);
        let rendered = render_widget(&model, 120, 14);

        assert!(rendered.contains("Render Timeline"), "{rendered}");
        assert!(rendered.contains("events: 2"), "{rendered}");
        assert!(rendered.contains("turns: 1"), "{rendered}");
        assert!(rendered.contains("turn-0001"), "{rendered}");
        assert!(rendered.contains("command_start"), "{rendered}");
        assert!(
            rendered.contains("cargo test -p mossen-tui timeline"),
            "{rendered}"
        );
        assert!(rendered.contains("scope: main"), "{rendered}");
        assert!(rendered.contains("history: freeze history"), "{rendered}");
    }

    #[test]
    fn timeline_clips_multibyte_rows_with_ascii_separator() {
        let event = RenderEvent {
            kind: RenderEventKind::ErrorRaised {
                source: "终端渲染".to_string(),
                summary: "中文路径和非常长的错误说明应该被安全裁剪".to_string(),
            },
            scope: RenderEventScope::Main,
            turn_id: Some("turn-utf8".to_string()),
            stage: UiStage::Failed,
            refresh: RenderRefreshPolicy::Passive,
            history: RenderHistoryPolicy::Append,
        };
        let model = RenderTimelineRenderModel::from_events(&[event]);
        let rendered = render_widget(&model, 44, 10);

        assert!(rendered.contains("Render Timeline"), "{rendered}");
        assert!(rendered.contains("events: 1"), "{rendered}");
        assert!(rendered.contains("error"), "{rendered}");
    }
}
