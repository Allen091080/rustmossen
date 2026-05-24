//! `/ps` process/status inspection widget.

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
use crate::render_model::{ProcessListRenderModel, ProcessRowRenderModel, ProcessStatus};
use crate::theme::Theme;

pub struct ProcessListWidget<'a> {
    model: &'a ProcessListRenderModel,
    theme: &'a Theme,
    glyphs: RenderGlyphs,
    selected: usize,
    scroll: usize,
}

impl<'a> ProcessListWidget<'a> {
    pub fn new(model: &'a ProcessListRenderModel, theme: &'a Theme) -> Self {
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

impl<'a> Widget for ProcessListWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 24 || area.height < 5 {
            return;
        }

        let block = Block::default()
            .title(Span::styled(
                " Process Status ",
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

        let summary = process_summary_line(self.model, self.glyphs);
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
                "No active processes.",
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

            if selected_row && y < rows_bottom {
                if let Some(detail) = selected_process_detail(row, self.glyphs) {
                    buf.set_stringn(
                        inner.x,
                        y,
                        clip_to_width(&format!("  {}", detail), inner.width as usize),
                        inner.width as usize,
                        Style::default().fg(self.theme.text_dim),
                    );
                    y = y.saturating_add(1);
                }
            }
        }
    }
}

impl ProcessListWidget<'_> {
    fn render_row_header(
        &self,
        row: &ProcessRowRenderModel,
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
        let title_style = if selected {
            Style::default()
                .fg(self.theme.text)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(self.theme.text)
        };

        let status_label = format!("[{}]", row.status.label());
        let kind_label = format!("{:<8}", row.kind.label());
        let line = Line::from(vec![
            Span::styled(format!("{marker} "), marker_style),
            Span::styled(status_label, self.status_style(row.status)),
            Span::raw(" "),
            Span::styled(kind_label, Style::default().fg(self.theme.text_dim)),
            Span::raw(" "),
            Span::styled(row.title.clone(), title_style),
        ]);
        buf.set_line(x, y, &line, width);
    }

    fn status_style(&self, status: ProcessStatus) -> Style {
        let color = match status {
            ProcessStatus::Idle => self.theme.text_dim,
            ProcessStatus::Running => self.theme.primary,
            ProcessStatus::Waiting => self.theme.warning,
            ProcessStatus::Completed => self.theme.success,
            ProcessStatus::Failed => self.theme.error,
            ProcessStatus::Info => self.theme.info,
        };
        Style::default().fg(color)
    }
}

fn process_summary_line(model: &ProcessListRenderModel, glyphs: RenderGlyphs) -> String {
    let sep = glyphs.separator();
    format!(
        "stage: {}{sep}turn: {}{sep}active: {}{sep}waiting: {}{sep}failed: {}",
        model.summary.stage,
        model.summary.turn_state,
        model.summary.active_count,
        model.summary.waiting_count,
        model.summary.failed_count
    )
}

fn selected_process_detail(row: &ProcessRowRenderModel, glyphs: RenderGlyphs) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(detail) = row.detail.as_deref() {
        parts.push(detail.to_string());
    }
    for fact in &row.facts {
        parts.push(format!("{}: {}", fact.label, fact.value));
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(glyphs.separator()))
    }
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
    use crate::render_glyphs::RenderGlyphs;
    use crate::render_model::{
        ProcessListRenderModel, ProcessRowKind, ProcessRowRenderModel, ProcessStatus,
        ProcessSummaryRenderModel,
    };

    fn render(widget: impl Widget, width: u16, height: u16) -> String {
        let mut buffer = Buffer::empty(Rect::new(0, 0, width, height));
        widget.render(Rect::new(0, 0, width, height), &mut buffer);
        let mut out = String::new();
        for y in 0..height {
            for x in 0..width {
                out.push_str(buffer.content[buffer.index_of(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    #[test]
    fn process_list_renders_summary_and_selected_detail() {
        let theme = Theme::default();
        let mut summary = ProcessSummaryRenderModel::new("running command", "streaming");
        summary.active_count = 1;
        let model = ProcessListRenderModel::new(
            summary,
            vec![ProcessRowRenderModel::new(
                "cmd",
                ProcessRowKind::Activity,
                ProcessStatus::Running,
                "Command running",
            )
            .detail("cargo test -p mossen-tui")
            .fact("cwd", "/repo")],
        );

        let rendered = render(ProcessListWidget::new(&model, &theme).selected(0), 72, 8);

        assert!(rendered.contains("Process Status"));
        assert!(rendered.contains("running command"));
        assert!(rendered.contains("[running]"));
        assert!(rendered.contains("cargo test -p mossen-tui"));
        assert!(rendered.contains("cwd: /repo"));
    }

    #[test]
    fn process_list_uses_ascii_separator_and_clips_detail() {
        let theme = Theme::default();
        let mut summary = ProcessSummaryRenderModel::new("thinking", "streaming");
        summary.waiting_count = 1;
        let model = ProcessListRenderModel::new(
            summary,
            vec![ProcessRowRenderModel::new(
                "todo",
                ProcessRowKind::Todo,
                ProcessStatus::Waiting,
                "完整渲染红线：不能把旧组件当成果",
            )
            .detail("等待实现一个很长很长的终端检查面板")],
        );

        let rendered = render(
            ProcessListWidget::new(&model, &theme)
                .glyphs(RenderGlyphs::ascii())
                .selected(0),
            38,
            7,
        );

        assert!(rendered.contains(" - "), "{rendered}");
        assert!(rendered.contains("..."), "{rendered}");
        assert!(!rendered.contains('…'), "{rendered}");
        assert!(!rendered.contains('·'), "{rendered}");
    }
}
