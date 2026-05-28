//! `/errors` semantic error history widget.

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
use crate::render_model::{ErrorHistoryRenderModel, ErrorHistoryRowRenderModel};
use crate::theme::Theme;

pub struct ErrorHistoryWidget<'a> {
    model: &'a ErrorHistoryRenderModel,
    theme: &'a Theme,
    glyphs: RenderGlyphs,
    selected: usize,
    scroll: usize,
    expanded: bool,
    detail_scroll: usize,
}

impl<'a> ErrorHistoryWidget<'a> {
    pub fn new(model: &'a ErrorHistoryRenderModel, theme: &'a Theme) -> Self {
        Self {
            model,
            theme,
            glyphs: RenderGlyphs::default(),
            selected: 0,
            scroll: 0,
            expanded: false,
            detail_scroll: 0,
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

    pub fn expanded(mut self, expanded: bool) -> Self {
        self.expanded = expanded;
        self
    }

    pub fn detail_scroll(mut self, detail_scroll: usize) -> Self {
        self.detail_scroll = detail_scroll;
        self
    }
}

impl<'a> Widget for ErrorHistoryWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 28 || area.height < 6 {
            return;
        }

        let block = Block::default()
            .title(Span::styled(
                " Error History ",
                Style::default()
                    .fg(self.theme.error)
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

        let summary = error_summary_line(self.model, self.glyphs);
        buf.set_stringn(
            inner.x,
            inner.y,
            clip_to_width(&summary, inner.width as usize),
            inner.width as usize,
            Style::default().fg(self.theme.text_dim),
        );

        let footer_y = inner.y.saturating_add(inner.height.saturating_sub(1));
        let footer = error_footer_line(self.selected_row(), self.expanded, self.glyphs);
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
                "No semantic errors recorded.",
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
                for line in
                    selected_error_detail(row, self.glyphs, self.expanded, self.detail_scroll)
                {
                    if y >= rows_bottom {
                        break;
                    }
                    let style = if line.starts_with("key detail:")
                        || line.starts_with("details:")
                        || row.command_failure
                    {
                        Style::default().fg(self.theme.error)
                    } else {
                        Style::default().fg(self.theme.text_dim)
                    };
                    buf.set_stringn(
                        inner.x,
                        y,
                        clip_to_width(&format!("  {line}"), inner.width as usize),
                        inner.width as usize,
                        style,
                    );
                    y = y.saturating_add(1);
                }
            }
        }
    }
}

impl ErrorHistoryWidget<'_> {
    fn selected_row(&self) -> Option<&ErrorHistoryRowRenderModel> {
        self.model.rows.get(self.selected)
    }

    fn render_row_header(
        &self,
        row: &ErrorHistoryRowRenderModel,
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
        let status = if row.retrying {
            "[Retrying]"
        } else if row.command_failure {
            "[Command]"
        } else {
            "[Error]"
        };
        let line = Line::from(vec![
            Span::styled(format!("{marker} "), marker_style),
            Span::styled(status, self.status_style(row)),
            Span::raw(" "),
            Span::styled(row.title.clone(), title_style),
            Span::styled(
                format!("  {}", row.summary),
                Style::default().fg(self.theme.text_dim),
            ),
        ]);
        buf.set_line(x, y, &line, width);
    }

    fn status_style(&self, row: &ErrorHistoryRowRenderModel) -> Style {
        if row.retrying {
            Style::default().fg(self.theme.warning)
        } else {
            Style::default().fg(self.theme.error)
        }
    }
}

fn error_summary_line(model: &ErrorHistoryRenderModel, glyphs: RenderGlyphs) -> String {
    let sep = glyphs.separator();
    format!(
        "errors: {}{sep}command failures: {}{sep}retrying: {}{sep}hidden details: {}",
        model.summary.total_count,
        model.summary.command_failure_count,
        model.summary.retrying_count,
        model.summary.hidden_detail_count
    )
}

fn error_footer_line(
    selected: Option<&ErrorHistoryRowRenderModel>,
    expanded: bool,
    glyphs: RenderGlyphs,
) -> String {
    let sep = glyphs.separator();
    if selected.is_some_and(ErrorHistoryRowRenderModel::has_details) {
        if expanded {
            format!("Esc closes{sep}Space collapses details{sep}PageUp/PageDown scroll details")
        } else {
            format!("Esc closes{sep}Space/Enter expands details{sep}Up/Down selects")
        }
    } else {
        format!("Esc closes{sep}Up/Down selects{sep}Home/End jump")
    }
}

fn selected_error_detail(
    row: &ErrorHistoryRowRenderModel,
    glyphs: RenderGlyphs,
    expanded: bool,
    detail_scroll: usize,
) -> Vec<String> {
    let mut lines = vec![
        format!("source: {}", row.source),
        format!("summary: {}", row.summary),
    ];
    if !row.has_details() {
        return lines;
    }
    if !expanded {
        lines.push("details: collapsed; press Space/Enter to expand".to_string());
        return lines;
    }
    lines.push("details: expanded from semantic error data".to_string());
    append_error_detail_lines(&mut lines, row, glyphs, detail_scroll);
    lines
}

fn append_error_detail_lines(
    lines: &mut Vec<String>,
    row: &ErrorHistoryRowRenderModel,
    glyphs: RenderGlyphs,
    detail_scroll: usize,
) {
    let mut detail_lines = Vec::new();
    if let Some(key_detail) = row
        .key_detail
        .as_deref()
        .filter(|detail| !detail.trim().is_empty())
    {
        detail_lines.push(format!("key detail: {key_detail}"));
    }
    if let Some(details) = row
        .details
        .as_deref()
        .filter(|details| !details.trim().is_empty())
    {
        for (index, line) in details.lines().enumerate() {
            if index == 0 {
                detail_lines.push(format!("details: {line}"));
            } else {
                detail_lines.push(format!("{} {line}", glyphs.disclosure_collapsed));
            }
        }
    }
    if row.detail_hidden_line_count > 0 {
        detail_lines.push(format!(
            "full log: {} more detail lines hidden",
            row.detail_hidden_line_count
        ));
    }
    if let Some(retry) = row
        .retry_hint
        .as_deref()
        .filter(|retry| !retry.trim().is_empty())
    {
        detail_lines.push(format!("retry: {retry}"));
    }

    let start = detail_scroll.min(detail_lines.len().saturating_sub(1));
    if start > 0 {
        lines.push(format!(
            "{} skipped {start} detail lines",
            glyphs.ellipsis()
        ));
    }
    let mut emitted = 0usize;
    for line in detail_lines.into_iter().skip(start).take(200) {
        lines.push(line);
        emitted += 1;
    }
    let remaining = row
        .detail_line_count()
        .saturating_sub(start.saturating_add(emitted));
    if remaining > 0 {
        lines.push(format!(
            "{} {remaining} more detail lines",
            glyphs.ellipsis()
        ));
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
        ErrorHistoryRenderModel, ErrorHistoryRowRenderModel, ErrorRenderModel,
    };
    use crate::theme::Theme;
    use ratatui::buffer::Buffer;

    fn buffer_to_string(buf: &Buffer, width: u16, height: u16) -> String {
        let mut out = String::new();
        for y in 0..height {
            for x in 0..width {
                out.push_str(buf.content[buf.index_of(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    fn render_widget(
        model: &ErrorHistoryRenderModel,
        width: u16,
        height: u16,
        expanded: bool,
        detail_scroll: usize,
        glyphs: RenderGlyphs,
    ) -> String {
        let theme = Theme::default();
        let mut buf = Buffer::empty(Rect::new(0, 0, width, height));
        ErrorHistoryWidget::new(model, &theme)
            .glyphs(glyphs)
            .expanded(expanded)
            .detail_scroll(detail_scroll)
            .render(buf.area, &mut buf);
        buffer_to_string(&buf, width, height)
    }

    fn error_row() -> ErrorHistoryRowRenderModel {
        let error = ErrorRenderModel {
            title: "Command error".to_string(),
            summary: "Build failed".to_string(),
            key_detail: Some("error[E0425]: cannot find value `missing`".to_string()),
            details: Some(
                "command: cargo test\nstderr: assertion failed\nstdout: running tests".to_string(),
            ),
            detail_hidden_line_count: 12,
            retry_hint: Some("Automatic retry is scheduled.".to_string()),
            retrying: true,
        };
        let mut row = ErrorHistoryRowRenderModel::from_error("err-1", "Bash", error);
        row.command_failure = true;
        row
    }

    #[test]
    fn error_history_renders_selected_error_detail() {
        let model = ErrorHistoryRenderModel::from_rows(vec![error_row()]);

        let rendered = render_widget(&model, 96, 14, false, 0, RenderGlyphs::unicode());

        assert!(rendered.contains("Error History"));
        assert!(rendered.contains("errors: 1"));
        assert!(rendered.contains("command failures: 1"));
        assert!(rendered.contains("[Retrying]"));
        assert!(rendered.contains("source: Bash"));
        assert!(rendered.contains("details: collapsed"));
        assert!(rendered.contains("Space/Enter expands details"));
    }

    #[test]
    fn error_history_expands_semantic_details() {
        let model = ErrorHistoryRenderModel::from_rows(vec![error_row()]);

        let rendered = render_widget(&model, 96, 18, true, 0, RenderGlyphs::unicode());

        assert!(rendered.contains("details: expanded"));
        assert!(rendered.contains("error[E0425]"));
        assert!(rendered.contains("stderr: assertion failed"));
        assert!(rendered.contains("full log: 12 more detail lines hidden"));
        assert!(rendered.contains("Space collapses details"));
    }

    #[test]
    fn error_history_scrolls_expanded_details() {
        let model = ErrorHistoryRenderModel::from_rows(vec![error_row()]);

        let rendered = render_widget(&model, 96, 14, true, 2, RenderGlyphs::unicode());

        assert!(rendered.contains("skipped 2 detail lines"));
        assert!(rendered.contains("stdout: running tests"));
        assert!(!rendered.contains("key detail:"));
    }

    #[test]
    fn error_history_clips_multibyte_with_ascii_footer() {
        let mut row = error_row();
        row.summary = "错误信息非常长，需要在窄终端里安全裁剪并保持列宽".to_string();
        let model = ErrorHistoryRenderModel::from_rows(vec![row]);

        let rendered = render_widget(&model, 42, 10, false, 0, RenderGlyphs::ascii());

        assert!(rendered.contains("Esc closes - Space/Enter"));
        assert!(rendered.contains("..."));
        assert!(!rendered.contains('\u{fffd}'));
    }
}
