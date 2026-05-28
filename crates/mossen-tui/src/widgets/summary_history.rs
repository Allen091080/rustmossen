//! `/results` semantic final-summary history widget.

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
use crate::render_lifecycle::{CommandSummaryModel, VerificationSummaryModel};
use crate::render_model::{FinalSummaryHistoryRenderModel, FinalSummaryHistoryRowRenderModel};
use crate::theme::Theme;

pub struct SummaryHistoryWidget<'a> {
    model: &'a FinalSummaryHistoryRenderModel,
    theme: &'a Theme,
    glyphs: RenderGlyphs,
    selected: usize,
    scroll: usize,
    expanded: bool,
    detail_scroll: usize,
}

impl<'a> SummaryHistoryWidget<'a> {
    pub fn new(model: &'a FinalSummaryHistoryRenderModel, theme: &'a Theme) -> Self {
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

impl<'a> Widget for SummaryHistoryWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 30 || area.height < 6 {
            return;
        }

        let block = Block::default()
            .title(Span::styled(
                " Final Summaries ",
                Style::default()
                    .fg(self.theme.success)
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

        let summary = summary_history_line(self.model, self.glyphs);
        buf.set_stringn(
            inner.x,
            inner.y,
            clip_to_width(&summary, inner.width as usize),
            inner.width as usize,
            Style::default().fg(self.theme.text_dim),
        );

        let footer_y = inner.y.saturating_add(inner.height.saturating_sub(1));
        let footer = summary_footer_line(self.selected_row(), self.expanded, self.glyphs);
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
                "No final summaries recorded.",
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
                    selected_summary_detail(row, self.glyphs, self.expanded, self.detail_scroll)
                {
                    if y >= rows_bottom {
                        break;
                    }
                    let style = if line.starts_with("risk:") || !row.success {
                        Style::default().fg(self.theme.warning)
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

impl SummaryHistoryWidget<'_> {
    fn selected_row(&self) -> Option<&FinalSummaryHistoryRowRenderModel> {
        self.model.rows.get(self.selected)
    }

    fn render_row_header(
        &self,
        row: &FinalSummaryHistoryRowRenderModel,
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
        let status_label = format!("[{}]", row.status_label());
        let line = Line::from(vec![
            Span::styled(format!("{marker} "), marker_style),
            Span::styled(status_label, self.status_style(row)),
            Span::raw(" "),
            Span::styled(row.title.clone(), title_style),
            Span::styled(
                format!(
                    "  files {} checks {} risks {}  {}",
                    row.changed_files.len(),
                    row.verification_results.len(),
                    row.residual_risks.len(),
                    final_summary_row_preview(row)
                ),
                Style::default().fg(self.theme.text_dim),
            ),
        ]);
        buf.set_line(x, y, &line, width);
    }

    fn status_style(&self, row: &FinalSummaryHistoryRowRenderModel) -> Style {
        if row.success {
            Style::default().fg(self.theme.success)
        } else {
            Style::default().fg(self.theme.warning)
        }
    }
}

fn final_summary_row_preview(row: &FinalSummaryHistoryRowRenderModel) -> String {
    row.changed_files
        .first()
        .map(|file| display_path_tail(&file.path))
        .or_else(|| row.commands.first().map(|command| command.command.clone()))
        .or_else(|| (!row.terminal.trim().is_empty()).then(|| row.terminal.clone()))
        .unwrap_or_else(|| "no details recorded".to_string())
}

fn display_path_tail(path: &str) -> String {
    path.rsplit(['/', '\\'])
        .find(|part| !part.is_empty())
        .unwrap_or(path)
        .to_string()
}

fn summary_history_line(model: &FinalSummaryHistoryRenderModel, glyphs: RenderGlyphs) -> String {
    let sep = glyphs.separator();
    format!(
        "summaries: {}{sep}completed: {}{sep}attention: {}{sep}files: {}{sep}commands: {}{sep}checks: {}{sep}risks: {}",
        model.summary.total_count,
        model.summary.completed_count,
        model.summary.attention_count,
        model.summary.changed_file_count,
        model.summary.command_count,
        model.summary.verification_count,
        model.summary.risk_count
    )
}

fn summary_footer_line(
    selected: Option<&FinalSummaryHistoryRowRenderModel>,
    expanded: bool,
    glyphs: RenderGlyphs,
) -> String {
    let sep = glyphs.separator();
    if selected.is_some_and(FinalSummaryHistoryRowRenderModel::has_details) {
        if expanded {
            format!("Esc closes{sep}Space collapses details{sep}PageUp/PageDown scroll details")
        } else {
            format!("Esc closes{sep}Space/Enter expands details{sep}Up/Down selects")
        }
    } else {
        format!("Esc closes{sep}Up/Down selects{sep}Home/End jump")
    }
}

fn selected_summary_detail(
    row: &FinalSummaryHistoryRowRenderModel,
    glyphs: RenderGlyphs,
    expanded: bool,
    detail_scroll: usize,
) -> Vec<String> {
    let mut lines = vec![
        format!("status: {}", row.status_label()),
        format!(
            "counts: {} files{}{} commands{}{} checks{}{} risks",
            row.changed_files.len(),
            glyphs.separator(),
            row.commands.len(),
            glyphs.separator(),
            row.verification_results.len(),
            glyphs.separator(),
            row.residual_risks.len()
        ),
    ];
    if !row.terminal.trim().is_empty() {
        lines.push(format!("terminal: {}", row.terminal));
    }
    if !row.has_details() {
        return lines;
    }
    if !expanded {
        lines.push("details: collapsed; press Space/Enter to expand".to_string());
        return lines;
    }
    lines.push("details: expanded from semantic final summary".to_string());
    append_summary_detail_lines(&mut lines, row, glyphs, detail_scroll);
    lines
}

fn append_summary_detail_lines(
    lines: &mut Vec<String>,
    row: &FinalSummaryHistoryRowRenderModel,
    glyphs: RenderGlyphs,
    detail_scroll: usize,
) {
    let mut detail_lines = Vec::new();
    for file in &row.changed_files {
        detail_lines.push(format!(
            "file: {} {} +{} -{}",
            file.status, file.path, file.additions, file.deletions
        ));
    }
    for command in &row.commands {
        detail_lines.push(format_command_summary(command, glyphs));
    }
    for verification in &row.verification_results {
        detail_lines.push(format_verification_summary(verification, glyphs));
    }
    for risk in &row.residual_risks {
        detail_lines.push(format!("risk: {risk}"));
    }
    for note in &row.notes {
        detail_lines.push(format!("note: {note}"));
    }

    let start = detail_scroll.min(detail_lines.len().saturating_sub(1));
    if start > 0 {
        lines.push(format!(
            "{} skipped {start} final-summary detail lines",
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
            "{} {remaining} more final-summary detail lines",
            glyphs.ellipsis()
        ));
    }
}

fn format_command_summary(command: &CommandSummaryModel, glyphs: RenderGlyphs) -> String {
    let sep = glyphs.separator();
    let mut parts = vec![
        format!("command: {}", command.command),
        format!("status {}", command.status),
    ];
    if let Some(exit_code) = command.exit_code {
        parts.push(format!("exit {exit_code}"));
    }
    if let Some(duration_ms) = command.duration_ms {
        parts.push(format!("{duration_ms}ms"));
    }
    if let Some(cwd) = command.cwd.as_deref().filter(|cwd| !cwd.trim().is_empty()) {
        parts.push(format!("cwd {cwd}"));
    }
    parts.join(sep)
}

fn format_verification_summary(
    verification: &VerificationSummaryModel,
    glyphs: RenderGlyphs,
) -> String {
    let sep = glyphs.separator();
    let mut parts = vec![
        format!("check: {}", verification.command),
        verification.status.clone(),
    ];
    if let Some(exit_code) = verification.exit_code {
        parts.push(format!("exit {exit_code}"));
    }
    if let Some(duration_ms) = verification.duration_ms {
        parts.push(format!("{duration_ms}ms"));
    }
    parts.join(sep)
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
    use crate::render_lifecycle::FileChangeSummaryModel;
    use ratatui::buffer::Buffer;

    fn render_widget(
        model: &FinalSummaryHistoryRenderModel,
        width: u16,
        height: u16,
        expanded: bool,
    ) -> String {
        let theme = Theme::default();
        let mut buf = Buffer::empty(Rect::new(0, 0, width, height));
        SummaryHistoryWidget::new(model, &theme)
            .expanded(expanded)
            .render(buf.area, &mut buf);
        buffer_to_string(&buf, width, height)
    }

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

    fn summary_row() -> FinalSummaryHistoryRowRenderModel {
        FinalSummaryHistoryRowRenderModel {
            id: "summary-1".to_string(),
            title: "Final Summary".to_string(),
            success: true,
            terminal: "Completed".to_string(),
            changed_files: vec![FileChangeSummaryModel {
                path: "crates/mossen-tui/src/app.rs".to_string(),
                status: "M".to_string(),
                additions: 12,
                deletions: 3,
            }],
            commands: vec![CommandSummaryModel {
                command: "cargo test -p mossen-tui".to_string(),
                cwd: Some("/repo".to_string()),
                exit_code: Some(0),
                duration_ms: Some(420),
                status: "passed".to_string(),
            }],
            verification_results: vec![VerificationSummaryModel {
                command: "cargo check -p mossen-tui".to_string(),
                status: "passed".to_string(),
                passed: true,
                exit_code: Some(0),
                duration_ms: Some(240),
            }],
            residual_risks: vec!["Snapshot drift requires review".to_string()],
            notes: vec!["No task execution code changed".to_string()],
            source_block_id: Some("summary-1".to_string()),
        }
    }

    #[test]
    fn summary_history_renders_selected_summary_detail() {
        let model = FinalSummaryHistoryRenderModel::from_rows(vec![summary_row()]);

        let rendered = render_widget(&model, 104, 14, false);

        assert!(rendered.contains("Final Summaries"));
        assert!(rendered.contains("summaries: 1"));
        assert!(rendered.contains("[Completed]"));
        assert!(rendered.contains("status: Completed"));
        assert!(rendered.contains("counts: 1 files"));
        assert!(rendered.contains("details: collapsed"));
        assert!(rendered.contains("Esc closes"));
    }

    #[test]
    fn summary_history_expands_semantic_final_summary_details() {
        let model = FinalSummaryHistoryRenderModel::from_rows(vec![summary_row()]);

        let rendered = render_widget(&model, 112, 18, true);

        assert!(rendered.contains("details: expanded"));
        assert!(rendered.contains("file: M crates/mossen-tui/src/app.rs +12 -3"));
        assert!(rendered.contains("command: cargo test -p mossen-tui"));
        assert!(rendered.contains("check: cargo check -p mossen-tui"));
        assert!(rendered.contains("risk: Snapshot drift requires review"));
        assert!(rendered.contains("Space collapses details"));
    }

    #[test]
    fn summary_history_clips_multibyte_rows_without_panic() {
        let mut row = summary_row();
        row.terminal = "完成了很长很长的终端渲染总结，需要在窄终端里安全裁剪".to_string();
        let model = FinalSummaryHistoryRenderModel::from_rows(vec![row]);

        let rendered = render_widget(&model, 44, 10, false);

        assert!(rendered.contains("Final Summaries"));
        assert!(rendered.contains("details:"));
    }
}
