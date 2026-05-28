//! `/commands` command execution history widget.

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
use crate::render_model::{
    CommandHistoryRenderModel, CommandHistoryRowRenderModel, CommandRunStatus,
};
use crate::theme::Theme;

pub struct CommandHistoryWidget<'a> {
    model: &'a CommandHistoryRenderModel,
    theme: &'a Theme,
    glyphs: RenderGlyphs,
    selected: usize,
    scroll: usize,
    expanded: bool,
    detail_scroll: usize,
}

impl<'a> CommandHistoryWidget<'a> {
    pub fn new(model: &'a CommandHistoryRenderModel, theme: &'a Theme) -> Self {
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

impl<'a> Widget for CommandHistoryWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 28 || area.height < 6 {
            return;
        }

        let block = Block::default()
            .title(Span::styled(
                " Command History ",
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

        let summary = command_summary_line(self.model, self.glyphs);
        buf.set_stringn(
            inner.x,
            inner.y,
            clip_to_width(&summary, inner.width as usize),
            inner.width as usize,
            Style::default().fg(self.theme.text_dim),
        );

        let footer_y = inner.y.saturating_add(inner.height.saturating_sub(1));
        let footer = command_footer_line(self.selected_row(), self.expanded, self.glyphs);
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
                "No command executions recorded.",
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
                    selected_command_detail(row, self.glyphs, self.expanded, self.detail_scroll)
                {
                    if y >= rows_bottom {
                        break;
                    }
                    let style = if line.starts_with("stderr:")
                        || line.contains(" stderr")
                        || line.starts_with("! ")
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

impl CommandHistoryWidget<'_> {
    fn selected_row(&self) -> Option<&CommandHistoryRowRenderModel> {
        self.model.rows.get(self.selected)
    }

    fn render_row_header(
        &self,
        row: &CommandHistoryRowRenderModel,
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
        let status_label = format!("[{}]", row.run.status.label());
        let line = Line::from(vec![
            Span::styled(format!("{marker} "), marker_style),
            Span::styled(status_label, self.status_style(row.run.status)),
            Span::raw(" "),
            Span::styled(row.title.clone(), title_style),
            Span::styled(
                format!("  {}", row.run.output_summary_line()),
                Style::default().fg(self.theme.text_dim),
            ),
        ]);
        buf.set_line(x, y, &line, width);
    }

    fn status_style(&self, status: CommandRunStatus) -> Style {
        let color = match status {
            CommandRunStatus::Requested | CommandRunStatus::Running => self.theme.primary,
            CommandRunStatus::WaitingApproval => self.theme.warning,
            CommandRunStatus::Succeeded => self.theme.success,
            CommandRunStatus::Failed | CommandRunStatus::Rejected => self.theme.error,
        };
        Style::default().fg(color)
    }
}

fn command_summary_line(model: &CommandHistoryRenderModel, glyphs: RenderGlyphs) -> String {
    let sep = glyphs.separator();
    format!(
        "commands: {}{sep}running: {}{sep}failed: {}{sep}full logs: {}",
        model.summary.total_count,
        model.summary.running_count,
        model.summary.failed_count,
        model.summary.full_log_count
    )
}

fn command_footer_line(
    selected: Option<&CommandHistoryRowRenderModel>,
    expanded: bool,
    glyphs: RenderGlyphs,
) -> String {
    let sep = glyphs.separator();
    if selected.is_some_and(|row| row.run.has_embedded_full_log()) {
        if expanded {
            format!("Esc closes{sep}Space collapses log{sep}PageUp/PageDown scroll log")
        } else {
            format!("Esc closes{sep}Space/Enter expands log{sep}Up/Down selects")
        }
    } else {
        format!("Esc closes{sep}Up/Down selects{sep}Home/End jump")
    }
}

fn selected_command_detail(
    row: &CommandHistoryRowRenderModel,
    glyphs: RenderGlyphs,
    expanded: bool,
    detail_scroll: usize,
) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(command) = row.run.command.as_deref() {
        lines.push(format!("command: {command}"));
    }
    if let Some(cwd) = row.run.cwd.as_deref() {
        lines.push(format!("cwd: {cwd}"));
    }
    lines.push(format!("status: {}", row.run.status_line()));
    lines.push(format!("output: {}", row.run.output_summary_line()));
    if row.run.has_embedded_full_log() {
        if expanded {
            lines.push("full log: expanded from stored command output".to_string());
            append_full_log_lines(&mut lines, row, glyphs, detail_scroll);
            return lines;
        }
        lines.push("full log: collapsed; press Space/Enter to expand".to_string());
    } else if row.run.full_log_available {
        lines.push("full log: available from active command output".to_string());
    }
    append_preview_lines(&mut lines, "stdout", row.stdout_preview.as_deref(), glyphs);
    append_preview_lines(&mut lines, "stderr", row.stderr_preview.as_deref(), glyphs);
    lines
}

fn append_full_log_lines(
    lines: &mut Vec<String>,
    row: &CommandHistoryRowRenderModel,
    glyphs: RenderGlyphs,
    detail_scroll: usize,
) {
    let mut log_lines = Vec::new();
    append_stream_lines(
        &mut log_lines,
        "stdout",
        row.run.stdout.full_text.as_deref(),
        glyphs,
    );
    append_stream_lines(
        &mut log_lines,
        "stderr",
        row.run.stderr.full_text.as_deref(),
        glyphs,
    );
    if log_lines.is_empty() {
        append_preview_lines(lines, "stdout", row.stdout_preview.as_deref(), glyphs);
        append_preview_lines(lines, "stderr", row.stderr_preview.as_deref(), glyphs);
        return;
    }

    let start = detail_scroll.min(log_lines.len().saturating_sub(1));
    if start > 0 {
        lines.push(format!(
            "{} skipped {start} full-log lines",
            glyphs.ellipsis()
        ));
    }
    let mut emitted = 0usize;
    for line in log_lines.into_iter().skip(start).take(200) {
        lines.push(line);
        emitted += 1;
    }
    let remaining = row
        .run
        .full_log_line_count()
        .saturating_sub(start.saturating_add(emitted));
    if remaining > 0 {
        lines.push(format!(
            "{} {remaining} more full-log lines",
            glyphs.ellipsis()
        ));
    }
}

fn append_stream_lines(
    lines: &mut Vec<String>,
    label: &str,
    text: Option<&str>,
    glyphs: RenderGlyphs,
) {
    let Some(text) = text else {
        return;
    };
    for (index, line) in text.lines().enumerate() {
        if index == 0 {
            lines.push(format!("{label}: {line}"));
        } else {
            lines.push(format!("{} {line}", glyphs.disclosure_collapsed));
        }
    }
}

fn append_preview_lines(
    lines: &mut Vec<String>,
    label: &str,
    preview: Option<&str>,
    glyphs: RenderGlyphs,
) {
    let Some(preview) = preview else {
        return;
    };
    for (index, line) in preview.lines().take(4).enumerate() {
        if index == 0 {
            lines.push(format!("{label}: {line}"));
        } else {
            lines.push(format!("{} {line}", glyphs.disclosure_collapsed));
        }
    }
    let remaining = preview.lines().count().saturating_sub(4);
    if remaining > 0 {
        lines.push(format!(
            "{} {remaining} more {label} lines",
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
        CommandHistoryRenderModel, CommandHistoryRowRenderModel, CommandRunRenderModel,
        CommandRunStatus, CommandStreamRenderModel,
    };
    use crate::theme::Theme;
    use ratatui::buffer::Buffer;

    fn render_widget(model: &CommandHistoryRenderModel, width: u16, height: u16) -> String {
        let theme = Theme::default();
        let mut buf = Buffer::empty(Rect::new(0, 0, width, height));
        CommandHistoryWidget::new(model, &theme).render(buf.area, &mut buf);
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

    fn command_run(status: CommandRunStatus) -> CommandRunRenderModel {
        CommandRunRenderModel {
            command: Some("cargo test -p mossen-tui".to_string()),
            cwd: Some("/repo".to_string()),
            status,
            exit_code: Some(1),
            duration_ms: Some(42),
            timed_out: false,
            interrupted: false,
            signal: None,
            error_summary: Some("test failure".to_string()),
            stdout: CommandStreamRenderModel {
                name: "stdout".to_string(),
                preview_line_count: 2,
                hidden_line_count: 5,
                total_line_count: Some(7),
                has_content: true,
                full_log_available: true,
                full_text: Some("running tests\n1 failed\nfull log tail\n".to_string()),
            },
            stderr: CommandStreamRenderModel {
                name: "stderr".to_string(),
                preview_line_count: 1,
                hidden_line_count: 0,
                total_line_count: Some(1),
                has_content: true,
                full_log_available: true,
                full_text: Some("error: assertion failed\n".to_string()),
            },
            full_log_available: true,
        }
    }

    #[test]
    fn command_history_renders_selected_command_detail() {
        let model =
            CommandHistoryRenderModel::from_rows(vec![CommandHistoryRowRenderModel::from_run(
                "cmd-1",
                command_run(CommandRunStatus::Failed),
            )
            .stdout_preview("running tests\n1 failed\n")
            .stderr_preview("error: assertion failed\n")]);

        let rendered = render_widget(&model, 96, 14);

        assert!(rendered.contains("Command History"));
        assert!(rendered.contains("commands: 1"));
        assert!(rendered.contains("[Failed]"));
        assert!(rendered.contains("cargo test -p mossen-tui"));
        assert!(rendered.contains("cwd: /repo"));
        assert!(rendered.contains("stdout: running tests"));
        assert!(rendered.contains("stderr: error: assertion failed"));
        assert!(rendered.contains("full log: collapsed"));
        assert!(rendered.contains("Esc closes"));
    }

    #[test]
    fn command_history_expands_embedded_full_log() {
        let model =
            CommandHistoryRenderModel::from_rows(vec![CommandHistoryRowRenderModel::from_run(
                "cmd-1",
                command_run(CommandRunStatus::Succeeded),
            )
            .stdout_preview("running tests\n1 failed\n")
            .stderr_preview("error: assertion failed\n")]);
        let theme = Theme::default();
        let mut buf = Buffer::empty(Rect::new(0, 0, 96, 16));
        CommandHistoryWidget::new(&model, &theme)
            .expanded(true)
            .render(buf.area, &mut buf);
        let rendered = buffer_to_string(&buf, 96, 16);

        assert!(rendered.contains("full log: expanded"));
        assert!(rendered.contains("full log tail"));
        assert!(rendered.contains("Space collapses log"));
    }

    #[test]
    fn command_history_scrolls_expanded_full_log() {
        let mut run = command_run(CommandRunStatus::Succeeded);
        run.stdout.full_text = Some("line 1\nline 2\nline 3\nline 4\nline 5\nline 6\n".to_string());
        run.stderr.full_text = None;
        let model =
            CommandHistoryRenderModel::from_rows(vec![CommandHistoryRowRenderModel::from_run(
                "cmd-1", run,
            )
            .stdout_preview("line 1\nline 2\n")]);
        let theme = Theme::default();
        let mut buf = Buffer::empty(Rect::new(0, 0, 96, 14));
        CommandHistoryWidget::new(&model, &theme)
            .expanded(true)
            .detail_scroll(3)
            .render(buf.area, &mut buf);
        let rendered = buffer_to_string(&buf, 96, 14);

        assert!(rendered.contains("skipped 3 full-log lines"));
        assert!(rendered.contains("line 4"));
        assert!(!rendered.contains("stdout: line 1"));
    }

    #[test]
    fn command_history_clips_multibyte_with_ascii_footer() {
        let mut run = command_run(CommandRunStatus::Running);
        run.command = Some("cargo test --package 渲染模块 -- 超长中文命令需要安全裁剪".to_string());
        let model =
            CommandHistoryRenderModel::from_rows(vec![CommandHistoryRowRenderModel::from_run(
                "cmd-cjk", run,
            )
            .stdout_preview("逐行读取终端渲染输出并保持列宽\n")]);
        let theme = Theme::default();
        let mut buf = Buffer::empty(Rect::new(0, 0, 42, 10));
        CommandHistoryWidget::new(&model, &theme)
            .glyphs(RenderGlyphs::ascii())
            .render(buf.area, &mut buf);
        let rendered = buffer_to_string(&buf, 42, 10);

        assert!(rendered.contains("Esc closes - Space/Enter expands log"));
        assert!(!rendered.contains('\u{fffd}'));
    }
}
