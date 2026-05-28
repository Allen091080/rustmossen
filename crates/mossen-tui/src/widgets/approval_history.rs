//! `/approvals` semantic approval history widget.

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
    ApprovalAction, ApprovalHistoryRenderModel, ApprovalHistoryRowRenderModel,
    ApprovalHistoryStatus,
};
use crate::theme::Theme;

pub struct ApprovalHistoryWidget<'a> {
    model: &'a ApprovalHistoryRenderModel,
    theme: &'a Theme,
    glyphs: RenderGlyphs,
    selected: usize,
    scroll: usize,
    expanded: bool,
    detail_scroll: usize,
}

impl<'a> ApprovalHistoryWidget<'a> {
    pub fn new(model: &'a ApprovalHistoryRenderModel, theme: &'a Theme) -> Self {
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

impl<'a> Widget for ApprovalHistoryWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 30 || area.height < 6 {
            return;
        }

        let block = Block::default()
            .title(Span::styled(
                " Approval History ",
                Style::default()
                    .fg(self.theme.permission)
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

        let summary = approval_summary_line(self.model, self.glyphs);
        buf.set_stringn(
            inner.x,
            inner.y,
            clip_to_width(&summary, inner.width as usize),
            inner.width as usize,
            Style::default().fg(self.theme.text_dim),
        );

        let footer_y = inner.y.saturating_add(inner.height.saturating_sub(1));
        let footer = approval_footer_line(self.selected_row(), self.expanded, self.glyphs);
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
                "No approval decisions recorded.",
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
                    selected_approval_detail(row, self.glyphs, self.expanded, self.detail_scroll)
                {
                    if y >= rows_bottom {
                        break;
                    }
                    let style = if row.status.is_negative() {
                        Style::default().fg(self.theme.permission_denied)
                    } else if row.status == ApprovalHistoryStatus::Pending {
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

impl ApprovalHistoryWidget<'_> {
    fn selected_row(&self) -> Option<&ApprovalHistoryRowRenderModel> {
        self.model.rows.get(self.selected)
    }

    fn render_row_header(
        &self,
        row: &ApprovalHistoryRowRenderModel,
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
        let preview = approval_row_preview(row);
        let line = Line::from(vec![
            Span::styled(format!("{marker} "), marker_style),
            Span::styled(status_label, self.status_style(row)),
            Span::raw(" "),
            Span::styled(row.title.clone(), title_style),
            Span::styled(
                format!("  {}  {}", row.tool_name, preview),
                Style::default().fg(self.theme.text_dim),
            ),
        ]);
        buf.set_line(x, y, &line, width);
    }

    fn status_style(&self, row: &ApprovalHistoryRowRenderModel) -> Style {
        match row.status {
            ApprovalHistoryStatus::Pending => Style::default().fg(self.theme.warning),
            ApprovalHistoryStatus::Allowed | ApprovalHistoryStatus::AlwaysAllowed => {
                Style::default().fg(self.theme.success)
            }
            ApprovalHistoryStatus::Denied | ApprovalHistoryStatus::Cancelled => {
                Style::default().fg(self.theme.permission_denied)
            }
        }
    }
}

fn approval_summary_line(model: &ApprovalHistoryRenderModel, glyphs: RenderGlyphs) -> String {
    let sep = glyphs.separator();
    format!(
        "approvals: {}{sep}pending: {}{sep}allowed: {}{sep}denied: {}{sep}cancelled: {}{sep}high risk: {}",
        model.summary.total_count,
        model.summary.pending_count,
        model.summary.allowed_count,
        model.summary.denied_count,
        model.summary.cancelled_count,
        model.summary.high_risk_count
    )
}

fn approval_footer_line(
    selected: Option<&ApprovalHistoryRowRenderModel>,
    expanded: bool,
    glyphs: RenderGlyphs,
) -> String {
    let sep = glyphs.separator();
    if selected.is_some_and(ApprovalHistoryRowRenderModel::has_details) {
        if expanded {
            format!("Esc closes{sep}Space collapses details{sep}PageUp/PageDown scroll details")
        } else {
            format!("Esc closes{sep}Space/Enter expands details{sep}Up/Down selects")
        }
    } else {
        format!("Esc closes{sep}Up/Down selects{sep}Home/End jump")
    }
}

fn approval_row_preview(row: &ApprovalHistoryRowRenderModel) -> String {
    if !row.detail.trim().is_empty() {
        format!("{} {}", row.detail_label, row.detail)
    } else if let Some(risk) = row.risk {
        format!("risk {}", risk.label())
    } else {
        row.status_label().to_string()
    }
}

fn selected_approval_detail(
    row: &ApprovalHistoryRowRenderModel,
    glyphs: RenderGlyphs,
    expanded: bool,
    detail_scroll: usize,
) -> Vec<String> {
    let mut lines = vec![
        format!("status: {}", row.status_label()),
        format!("tool: {}", row.tool_name),
    ];
    if !row.detail.trim().is_empty() {
        lines.push(format!("{}: {}", row.detail_label, row.detail));
    }
    if let Some(risk) = row.risk {
        lines.push(format!("risk: {}", risk.label()));
    }
    if !row.has_details() {
        return lines;
    }
    if !expanded {
        lines.push("details: collapsed; press Space/Enter to expand".to_string());
        return lines;
    }
    lines.push("details: expanded from semantic approval data".to_string());
    append_approval_detail_lines(&mut lines, row, glyphs, detail_scroll);
    lines
}

fn append_approval_detail_lines(
    lines: &mut Vec<String>,
    row: &ApprovalHistoryRowRenderModel,
    glyphs: RenderGlyphs,
    detail_scroll: usize,
) {
    let mut detail_lines = Vec::new();
    if let Some(selected) = row.selected_action {
        detail_lines.push(format!("selected action: {}", selected.label()));
    }
    if !row.actions.is_empty() {
        detail_lines.push(format!(
            "actions: {}",
            row.actions
                .iter()
                .map(|action| format_action(*action, row.selected_action))
                .collect::<Vec<_>>()
                .join(" | ")
        ));
    }
    if let Some(body) = row.body.as_deref().filter(|body| !body.trim().is_empty()) {
        for (index, line) in body.lines().enumerate() {
            if index == 0 {
                detail_lines.push(format!("body: {line}"));
            } else {
                detail_lines.push(format!("{} {line}", glyphs.disclosure_collapsed));
            }
        }
    }
    if let Some(anchor) = row.anchor_block_id.as_deref() {
        detail_lines.push(format!("anchor: {anchor}"));
    }
    if let Some(source) = row.source_block_id.as_deref() {
        detail_lines.push(format!("source block: {source}"));
    }

    let start = detail_scroll.min(detail_lines.len().saturating_sub(1));
    if start > 0 {
        lines.push(format!(
            "{} skipped {start} approval detail lines",
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
            "{} {remaining} more approval detail lines",
            glyphs.ellipsis()
        ));
    }
}

fn format_action(action: ApprovalAction, selected: Option<ApprovalAction>) -> String {
    if Some(action) == selected {
        format!("*{}", action.label())
    } else {
        action.label().to_string()
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
    use crate::render_model::{ApprovalHistoryRenderModel, ApprovalRiskLevel};
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
        model: &ApprovalHistoryRenderModel,
        width: u16,
        height: u16,
        expanded: bool,
        detail_scroll: usize,
        glyphs: RenderGlyphs,
    ) -> String {
        let theme = Theme::default();
        let mut buf = Buffer::empty(Rect::new(0, 0, width, height));
        ApprovalHistoryWidget::new(model, &theme)
            .glyphs(glyphs)
            .expanded(expanded)
            .detail_scroll(detail_scroll)
            .render(buf.area, &mut buf);
        buffer_to_string(&buf, width, height)
    }

    fn pending_row() -> ApprovalHistoryRowRenderModel {
        ApprovalHistoryRowRenderModel {
            id: "approval-1".to_string(),
            status: ApprovalHistoryStatus::Pending,
            tool_name: "Bash".to_string(),
            title: "Shell Command".to_string(),
            detail_label: "Command".to_string(),
            detail: "cargo test -p mossen-tui".to_string(),
            risk: Some(ApprovalRiskLevel::High),
            body: Some("Run test suite\nWrites target artifacts".to_string()),
            actions: vec![
                ApprovalAction::Allow,
                ApprovalAction::AlwaysAllow,
                ApprovalAction::Deny,
            ],
            selected_action: Some(ApprovalAction::Allow),
            anchor_block_id: Some("tool-1".to_string()),
            source_block_id: None,
        }
    }

    #[test]
    fn approval_history_renders_pending_detail() {
        let model = ApprovalHistoryRenderModel::from_rows(vec![pending_row()]);

        let rendered = render_widget(&model, 96, 14, false, 0, RenderGlyphs::unicode());

        assert!(rendered.contains("Approval History"));
        assert!(rendered.contains("approvals: 1"));
        assert!(rendered.contains("pending: 1"));
        assert!(rendered.contains("[Pending]"));
        assert!(rendered.contains("Shell Command"));
        assert!(rendered.contains("risk: High"));
        assert!(rendered.contains("details: collapsed"));
        assert!(rendered.contains("Space/Enter expands details"));
    }

    #[test]
    fn approval_history_expands_semantic_details() {
        let model = ApprovalHistoryRenderModel::from_rows(vec![pending_row()]);

        let rendered = render_widget(&model, 96, 18, true, 0, RenderGlyphs::unicode());

        assert!(rendered.contains("details: expanded"));
        assert!(rendered.contains("selected action: Allow"));
        assert!(rendered.contains("actions: *Allow | Always | Deny"));
        assert!(rendered.contains("body: Run test suite"));
        assert!(rendered.contains("anchor: tool-1"));
        assert!(rendered.contains("Space collapses details"));
    }

    #[test]
    fn approval_history_scrolls_expanded_details() {
        let model = ApprovalHistoryRenderModel::from_rows(vec![pending_row()]);

        let rendered = render_widget(&model, 96, 14, true, 2, RenderGlyphs::unicode());

        assert!(rendered.contains("skipped 2 approval detail lines"));
        assert!(rendered.contains("Writes target artifacts"));
        assert!(!rendered.contains("selected action:"));
    }

    #[test]
    fn approval_history_clips_multibyte_with_ascii_footer() {
        let mut row = pending_row();
        row.detail = "需要审批的一条非常长的命令，会在窄终端里安全裁剪".to_string();
        let model = ApprovalHistoryRenderModel::from_rows(vec![row]);

        let rendered = render_widget(&model, 44, 10, false, 0, RenderGlyphs::ascii());

        assert!(rendered.contains("Esc closes - Space/Enter"));
        assert!(rendered.contains("..."));
        assert!(!rendered.contains('\u{fffd}'));
    }
}
