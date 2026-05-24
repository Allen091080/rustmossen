//! Semantic transcript block renderer.
//!
//! This widget is the terminal-facing side of the three-layer rendering
//! pipeline. It consumes `RenderBlock` values rather than raw agent messages,
//! so protocol cleanup and tool result parsing stay out of the viewport code.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget, Wrap},
};

use crate::render_glyphs::{RenderGlyphMode, RenderGlyphs};
use crate::render_model::{
    ApprovalDecisionKind, ApprovalDecisionModel, CodeSectionRenderModel, CommandRunRenderModel,
    ErrorRenderModel, FileChangeSummaryRenderModel, FinalSummaryModel, PlanRenderModel,
    PlanStepStatus, RenderBlock, RenderBlockKind, RenderNode, ToolCardModel, ToolPhase,
    ToolSection, ToolSectionKind,
};
use crate::render_profile::RendererProfile;
use crate::theme::Theme;
use crate::widgets::highlighted_code::HighlightedCodeWidget;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub struct RenderBlockWidget<'a> {
    block: &'a RenderBlock,
    theme: &'a Theme,
    add_margin: bool,
    show_all_thinking: bool,
    focused: bool,
    collapsed: bool,
    profile: Option<RendererProfile>,
    glyphs: RenderGlyphs,
}

impl<'a> RenderBlockWidget<'a> {
    pub fn new(block: &'a RenderBlock, theme: &'a Theme) -> Self {
        Self {
            block,
            theme,
            add_margin: true,
            show_all_thinking: false,
            focused: false,
            collapsed: false,
            profile: None,
            glyphs: RenderGlyphs::default(),
        }
    }

    pub fn add_margin(mut self, add: bool) -> Self {
        self.add_margin = add;
        self
    }

    pub fn show_all_thinking(mut self, on: bool) -> Self {
        self.show_all_thinking = on;
        self
    }

    pub fn focused(mut self, on: bool) -> Self {
        self.focused = on;
        self
    }

    pub fn collapsed(mut self, on: bool) -> Self {
        self.collapsed = on;
        self
    }

    pub fn profile(mut self, profile: RendererProfile) -> Self {
        self.profile = Some(profile);
        self
    }

    pub fn glyphs(mut self, glyphs: RenderGlyphs) -> Self {
        self.glyphs = glyphs;
        self
    }

    pub fn required_height(&self, width: u16) -> usize {
        let profile = self.profile_for_width(width);
        let margin = usize::from(self.add_margin);
        let focus_bar_width = u16::from(self.focused);
        let body_width = width.saturating_sub(3 + focus_bar_width).max(1);
        let body_height = if let Some(tool) = self.block.tool.as_ref() {
            self.tool_card_height(tool, body_width, profile)
        } else {
            self.nodes_height(body_width, profile)
        };
        body_height.saturating_add(margin).max(1)
    }

    fn profile_for_width(&self, width: u16) -> RendererProfile {
        self.profile
            .unwrap_or_else(|| RendererProfile::from_width(width))
    }

    fn nodes_height(&self, width: u16, profile: RendererProfile) -> usize {
        let mut height = 0usize;
        for node in &self.block.nodes {
            match node {
                RenderNode::Thinking(text) => {
                    if self.show_all_thinking || self.block.state.streaming {
                        height +=
                            wrapped_line_count(&format!("{} {text}", self.glyphs.thinking), width);
                    }
                }
                RenderNode::Markdown(text) => {
                    height += crate::widgets::markdown::MarkdownWidget::new(text)
                        .theme(self.theme)
                        .glyphs(self.glyphs)
                        .max_width(width)
                        .rendered_height(width) as usize;
                }
                RenderNode::PlainText(text) => {
                    height += wrapped_line_count(text, width);
                }
                RenderNode::Error(error) => {
                    height += card_lines_height(&error_card_lines(error, self.theme), width);
                }
                RenderNode::FileChangeSummary(summary) => {
                    height += card_lines_height(
                        &file_change_summary_lines(summary, self.theme, self.glyphs),
                        width,
                    );
                }
                RenderNode::FinalSummary(summary) => {
                    height += card_lines_height(
                        &final_summary_lines(summary, self.theme, self.glyphs),
                        width,
                    );
                }
                RenderNode::ToolCard(tool) => {
                    height += self.tool_card_height(tool, width, profile);
                }
                RenderNode::ApprovalDecision(decision) => {
                    height += wrapped_line_count(&decision.line(), width);
                }
            }
        }
        height.max(1)
    }

    fn tool_card_height(
        &self,
        tool: &ToolCardModel,
        width: u16,
        profile: RendererProfile,
    ) -> usize {
        if width < 4 {
            return 1;
        }
        let inner_width = width.saturating_sub(2).max(1);
        let mut height = 2usize; // border
        let mut content_lines = 0usize;

        if let Some(plan) = tool.plan.as_ref() {
            content_lines += plan_plain_lines(plan, self.collapsed, profile, self.glyphs)
                .iter()
                .map(|line| wrapped_line_count(line, inner_width))
                .sum::<usize>();
        }

        if tool.command_run.is_none() && tool.plan.is_none() {
            if let Some(summary) = tool.summary.as_deref() {
                content_lines += wrapped_line_count(summary, inner_width);
            }
        }

        if let Some(command_run) = tool.command_run.as_ref() {
            content_lines += command_run_plain_lines(command_run, self.collapsed)
                .iter()
                .map(|line| wrapped_line_count(line, inner_width))
                .sum::<usize>();
        }

        if !self.collapsed {
            for section in &tool.sections {
                if tool.command_run.is_some() && !command_section_is_terminal_visible(section) {
                    continue;
                }
                if tool.plan.is_some() && plan_section_is_terminal_replaced(section) {
                    continue;
                }
                content_lines += self.section_height(section, inner_width, profile);
            }
        }

        height += content_lines.max(1);
        height
    }

    fn section_height(&self, section: &ToolSection, width: u16, profile: RendererProfile) -> usize {
        if section.code.is_none()
            && (section.kind == ToolSectionKind::Diff || body_looks_diff(&section.body))
        {
            return 1 + diff_display_lines(
                &section.body,
                section_line_limit_for_block(self.block, profile),
                profile.tool_section_line_chars(),
                diff_display_mode_for_block(self.block),
                self.glyphs,
            )
            .iter()
            .map(|line| wrapped_line_count(&line.text, width))
            .sum::<usize>();
        }

        let (body, clipped, hidden_lines) = bounded_section_body(
            &section.body,
            section_line_limit_for_block(self.block, profile),
            profile.tool_section_line_chars(),
            self.glyphs,
        );
        let mut height = 1usize; // section label
        if let Some(code) = section.code.as_ref() {
            height += code_section_lines(&body, code, self.theme, self.glyphs)
                .iter()
                .map(|line| wrapped_line_count(&line_plain_text(line), width))
                .sum::<usize>();
        } else if body_looks_markdown(&body) {
            height += crate::widgets::markdown::MarkdownWidget::new(&body)
                .theme(self.theme)
                .glyphs(self.glyphs)
                .max_width(width)
                .rendered_height(width) as usize;
        } else {
            height += wrapped_line_count(&body, width);
        }
        if clipped {
            height += wrapped_line_count(&clipping_hint_text(hidden_lines, self.glyphs), width);
        }
        height
    }
}

impl<'a> Widget for RenderBlockWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let area = area.intersection(buf.area);
        if area.width == 0 || area.height == 0 {
            return;
        }

        let y_offset = if self.add_margin { 1 } else { 0 };
        if y_offset >= area.height {
            return;
        }
        let msg_area = Rect::new(
            area.x,
            area.y + y_offset,
            area.width,
            area.height.saturating_sub(y_offset),
        );

        if self.focused {
            let focus_style = Style::default().bg(self.theme.warning);
            for row in 0..msg_area.height {
                buf.set_string(msg_area.x, msg_area.y + row, " ", focus_style);
            }
        }

        let bar_offset = if self.focused { 1 } else { 0 };
        let (prefix_style, prefix) = prefix_for_block(self.block.kind, self.theme, self.glyphs);
        let prefix_x = msg_area.x.saturating_add(bar_offset);
        if msg_area.width > bar_offset {
            buf.set_string(prefix_x, msg_area.y, prefix, prefix_style);
        }

        let content_x = prefix_x.saturating_add(3);
        let content_width = msg_area
            .right()
            .saturating_sub(content_x)
            .min(msg_area.width.saturating_sub(3 + bar_offset));
        if content_width == 0 {
            return;
        }
        let content_area = Rect::new(content_x, msg_area.y, content_width, msg_area.height);
        let profile = self.profile_for_width(content_width);

        if let Some(tool) = self.block.tool.as_ref() {
            self.render_tool_card(tool, content_area, profile, buf);
        } else {
            self.render_nodes(content_area, profile, buf);
        }
    }
}

impl<'a> RenderBlockWidget<'a> {
    fn render_nodes(&self, area: Rect, profile: RendererProfile, buf: &mut Buffer) {
        let area = area.intersection(buf.area);
        if area.width == 0 || area.height == 0 {
            return;
        }

        let mut y = area.y;
        let bottom = area.bottom();
        for node in &self.block.nodes {
            if y >= bottom {
                break;
            }
            match node {
                RenderNode::Thinking(text) => {
                    if !self.show_all_thinking && !self.block.state.streaming {
                        continue;
                    }
                    let body = format!("{} {text}", self.glyphs.thinking);
                    let height = wrapped_line_count(&body, area.width)
                        .min(bottom.saturating_sub(y) as usize)
                        as u16;
                    let node_area = Rect::new(area.x, y, area.width, height);
                    Paragraph::new(body)
                        .style(
                            Style::default()
                                .fg(self.theme.text_dim)
                                .add_modifier(Modifier::ITALIC),
                        )
                        .wrap(Wrap { trim: false })
                        .render(node_area, buf);
                    y = y.saturating_add(height);
                }
                RenderNode::Markdown(text) => {
                    let widget = crate::widgets::markdown::MarkdownWidget::new(text)
                        .theme(self.theme)
                        .base_style(Style::default().fg(self.theme.text))
                        .glyphs(self.glyphs)
                        .max_width(area.width);
                    let lines = widget.parse_to_lines();
                    let height = widget
                        .rendered_height(area.width)
                        .min(bottom.saturating_sub(y) as usize)
                        .max(1) as u16;
                    let node_area = Rect::new(area.x, y, area.width, height);
                    Paragraph::new(lines)
                        .wrap(Wrap { trim: false })
                        .render(node_area, buf);
                    y = y.saturating_add(height);
                }
                RenderNode::PlainText(text) => {
                    let height = wrapped_line_count(text, area.width)
                        .min(bottom.saturating_sub(y) as usize)
                        as u16;
                    let node_area = Rect::new(area.x, y, area.width, height);
                    Paragraph::new(text.as_str())
                        .style(style_for_block(self.block.kind, self.theme))
                        .wrap(Wrap { trim: false })
                        .render(node_area, buf);
                    y = y.saturating_add(height);
                }
                RenderNode::Error(error) => {
                    let lines = error_card_lines(error, self.theme);
                    let height = card_lines_height(&lines, area.width)
                        .min(bottom.saturating_sub(y) as usize)
                        .max(1) as u16;
                    render_semantic_card(
                        error.title.as_str(),
                        lines,
                        Style::default().fg(self.theme.error),
                        self.glyphs,
                        Rect::new(area.x, y, area.width, height),
                        buf,
                    );
                    y = y.saturating_add(height);
                }
                RenderNode::FileChangeSummary(summary) => {
                    let lines = file_change_summary_lines(summary, self.theme, self.glyphs);
                    let height = card_lines_height(&lines, area.width)
                        .min(bottom.saturating_sub(y) as usize)
                        .max(1) as u16;
                    render_semantic_card(
                        &summary.title(),
                        lines,
                        Style::default().fg(self.theme.info),
                        self.glyphs,
                        Rect::new(area.x, y, area.width, height),
                        buf,
                    );
                    y = y.saturating_add(height);
                }
                RenderNode::FinalSummary(summary) => {
                    let lines = final_summary_lines(summary, self.theme, self.glyphs);
                    let height = card_lines_height(&lines, area.width)
                        .min(bottom.saturating_sub(y) as usize)
                        .max(1) as u16;
                    let border = if summary.success {
                        Style::default().fg(self.theme.success)
                    } else {
                        Style::default().fg(self.theme.error)
                    };
                    render_semantic_card(
                        summary.title(),
                        lines,
                        border,
                        self.glyphs,
                        Rect::new(area.x, y, area.width, height),
                        buf,
                    );
                    y = y.saturating_add(height);
                }
                RenderNode::ToolCard(tool) => {
                    self.render_tool_card(
                        tool,
                        Rect::new(area.x, y, area.width, bottom - y),
                        profile,
                        buf,
                    );
                    break;
                }
                RenderNode::ApprovalDecision(decision) => {
                    let line = decision.line();
                    let height = wrapped_line_count(&line, area.width)
                        .min(bottom.saturating_sub(y) as usize)
                        as u16;
                    let node_area = Rect::new(area.x, y, area.width, height);
                    Paragraph::new(line)
                        .style(style_for_approval_decision(decision, self.theme))
                        .wrap(Wrap { trim: false })
                        .render(node_area, buf);
                    y = y.saturating_add(height);
                }
            }
        }
    }

    fn render_tool_card(
        &self,
        tool: &ToolCardModel,
        area: Rect,
        profile: RendererProfile,
        buf: &mut Buffer,
    ) {
        let area = area.intersection(buf.area);
        if area.width < 4 || area.height == 0 {
            return;
        }

        let title = tool_title(tool, self.collapsed, self.glyphs);
        let border_style = match tool.phase {
            ToolPhase::Failed | ToolPhase::Rejected => Style::default().fg(self.theme.error),
            ToolPhase::WaitingApproval => Style::default().fg(self.theme.permission),
            ToolPhase::Running | ToolPhase::Requested => Style::default().fg(self.theme.info),
            ToolPhase::Succeeded => Style::default().fg(self.theme.border),
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(self.glyphs.border)
            .border_style(border_style)
            .title(Span::styled(
                title,
                border_style.add_modifier(Modifier::BOLD),
            ));
        let inner = block.inner(area);
        block.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let lines = self.tool_lines(tool, inner.width, profile);
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .render(inner, buf);
    }

    fn tool_lines(
        &self,
        tool: &ToolCardModel,
        width: u16,
        profile: RendererProfile,
    ) -> Vec<Line<'static>> {
        tool_card_lines_for_virtual_scroll(
            self.block,
            tool,
            width,
            profile,
            self.theme,
            self.collapsed,
            self.glyphs,
        )
    }
}

fn render_semantic_card(
    title: &str,
    lines: Vec<Line<'static>>,
    border_style: Style,
    glyphs: RenderGlyphs,
    area: Rect,
    buf: &mut Buffer,
) {
    let area = area.intersection(buf.area);
    if area.width < 4 || area.height == 0 {
        return;
    }
    let block = Block::default()
        .borders(Borders::ALL)
        .border_set(glyphs.border)
        .border_style(border_style)
        .title(Span::styled(
            format!(" {title} "),
            border_style.add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    block.render(area, buf);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .render(inner, buf);
}

fn card_lines_height(lines: &[Line<'_>], width: u16) -> usize {
    if width < 4 {
        return 1;
    }
    let inner_width = width.saturating_sub(2).max(1);
    let body = lines
        .iter()
        .map(line_plain_text)
        .map(|line| wrapped_line_count(&line, inner_width))
        .sum::<usize>()
        .max(1);
    body.saturating_add(2)
}

fn line_plain_text(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

fn error_card_lines(error: &ErrorRenderModel, theme: &Theme) -> Vec<Line<'static>> {
    let label = Style::default()
        .fg(theme.error)
        .add_modifier(Modifier::BOLD);
    let text = Style::default().fg(theme.text);
    let dim = Style::default().fg(theme.text_dim);
    let mut lines = vec![Line::from(vec![
        Span::styled("Reason: ", label),
        Span::styled(error.summary.clone(), text),
    ])];
    if let Some(key_detail) = error
        .key_detail
        .as_deref()
        .filter(|detail| !detail.trim().is_empty())
    {
        lines.push(Line::from(vec![
            Span::styled("Key detail: ", label),
            Span::styled(key_detail.to_string(), text),
        ]));
    }
    if let Some(details) = error
        .details
        .as_deref()
        .filter(|details| !details.trim().is_empty())
    {
        lines.push(Line::from(Span::styled("Details:", label)));
        for line in details.lines().take(8) {
            lines.push(Line::from(Span::styled(line.to_string(), text)));
        }
    }
    if error.detail_hidden_line_count > 0 {
        lines.push(Line::from(vec![
            Span::styled("Full log: ", label),
            Span::styled(
                format!(
                    "{} more lines hidden; expand or inspect command output for full details.",
                    error.detail_hidden_line_count
                ),
                dim,
            ),
        ]));
    }
    if let Some(retry) = error.retry_hint.as_deref() {
        lines.push(Line::from(vec![
            Span::styled("Retry: ", label),
            Span::styled(retry.to_string(), dim),
        ]));
    }
    lines
}

fn file_change_summary_lines(
    summary: &FileChangeSummaryRenderModel,
    theme: &Theme,
    glyphs: RenderGlyphs,
) -> Vec<Line<'static>> {
    let label = Style::default().fg(theme.info).add_modifier(Modifier::BOLD);
    let text = Style::default().fg(theme.text);
    let dim = Style::default().fg(theme.text_dim);
    let mut lines = Vec::new();

    lines.push(Line::from(vec![
        Span::styled("Files: ", label),
        Span::styled(
            format!(
                "{} changed  +{} -{}",
                summary.files.len(),
                summary.total_additions(),
                summary.total_deletions()
            ),
            text,
        ),
    ]));

    let modified = summary.count_with_status("M");
    let added = summary.count_with_status("A");
    let deleted = summary.count_with_status("D");
    let mut counts = Vec::new();
    if modified > 0 {
        counts.push(format!("{modified} modified"));
    }
    if added > 0 {
        counts.push(format!("{added} added"));
    }
    if deleted > 0 {
        counts.push(format!("{deleted} deleted"));
    }
    if !counts.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("Kinds: ", label),
            Span::styled(counts.join(glyphs.separator()), dim),
        ]));
    }

    for file in summary.files.iter().take(10) {
        lines.push(Line::from(vec![
            Span::styled(format!("  {} ", file.status), dim),
            Span::styled(file.path.clone(), text),
            Span::styled(format!("  +{} -{}", file.additions, file.deletions), dim),
        ]));
    }
    if summary.files.len() > 10 {
        lines.push(Line::from(Span::styled(
            format!(
                "  {} {} more files",
                glyphs.ellipsis(),
                summary.files.len() - 10
            ),
            dim,
        )));
    }

    lines
}

fn final_summary_lines(
    summary: &FinalSummaryModel,
    theme: &Theme,
    glyphs: RenderGlyphs,
) -> Vec<Line<'static>> {
    let label = Style::default()
        .fg(if summary.success {
            theme.success
        } else {
            theme.error
        })
        .add_modifier(Modifier::BOLD);
    let text = Style::default().fg(theme.text);
    let dim = Style::default().fg(theme.text_dim);
    let mut lines = Vec::new();
    let status = if summary.success {
        "Completed"
    } else {
        "Needs attention"
    };
    lines.push(Line::from(vec![
        Span::styled("Status: ", label),
        Span::styled(status.to_string(), text),
    ]));
    if !summary.terminal.trim().is_empty() && summary.terminal != "Completed" {
        lines.push(Line::from(vec![
            Span::styled("Terminal: ", label),
            Span::styled(summary.terminal.clone(), text),
        ]));
    }

    if summary.changed_files.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("Files: ", label),
            Span::styled("no file changes recorded", dim),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled("Files: ", label),
            Span::styled(format!("{} changed", summary.changed_files.len()), text),
        ]));
        for file in summary.changed_files.iter().take(8) {
            lines.push(Line::from(vec![
                Span::styled(format!("  {} ", file.status), dim),
                Span::styled(file.path.clone(), text),
                Span::styled(format!("  +{} -{}", file.additions, file.deletions), dim),
            ]));
        }
        if summary.changed_files.len() > 8 {
            lines.push(Line::from(Span::styled(
                format!(
                    "  {} {} more files",
                    glyphs.ellipsis(),
                    summary.changed_files.len() - 8
                ),
                dim,
            )));
        }
    }

    if summary.commands.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("Commands: ", label),
            Span::styled("none recorded", dim),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled("Commands: ", label),
            Span::styled(format!("{} run", summary.commands.len()), text),
        ]));
        for command in summary.commands.iter().take(6) {
            let mut suffix = command.status.clone();
            if let Some(exit) = command.exit_code {
                suffix.push_str(&format!("{}exit {exit}", glyphs.separator()));
            }
            if let Some(duration) = command.duration_ms {
                suffix.push_str(&format!("{}{duration}ms", glyphs.separator()));
            }
            lines.push(Line::from(vec![
                Span::styled("  $ ", dim),
                Span::styled(command.command.clone(), text),
                Span::styled(format!("  {suffix}"), dim),
            ]));
        }
        if summary.commands.len() > 6 {
            lines.push(Line::from(Span::styled(
                format!(
                    "  {} {} more commands",
                    glyphs.ellipsis(),
                    summary.commands.len() - 6
                ),
                dim,
            )));
        }
    }

    if summary.verification_results.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("Verification: ", label),
            Span::styled("none recorded", dim),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled("Verification: ", label),
            Span::styled(
                format!("{} checks", summary.verification_results.len()),
                text,
            ),
        ]));
        for result in summary.verification_results.iter().take(6) {
            let status_style = if result.passed {
                Style::default().fg(theme.success)
            } else {
                Style::default().fg(theme.error)
            };
            let mut suffix = result.status.clone();
            if let Some(exit) = result.exit_code {
                suffix.push_str(&format!("{}exit {exit}", glyphs.separator()));
            }
            if let Some(duration) = result.duration_ms {
                suffix.push_str(&format!("{}{duration}ms", glyphs.separator()));
            }
            lines.push(Line::from(vec![
                Span::styled("  $ ", dim),
                Span::styled(result.command.clone(), text),
                Span::styled(format!("  {suffix}"), status_style),
            ]));
        }
        if summary.verification_results.len() > 6 {
            lines.push(Line::from(Span::styled(
                format!(
                    "  {} {} more checks",
                    glyphs.ellipsis(),
                    summary.verification_results.len() - 6
                ),
                dim,
            )));
        }
    }

    if summary.residual_risks.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("Risks: ", label),
            Span::styled("none recorded", dim),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled("Risks: ", label),
            Span::styled(format!("{}", summary.residual_risks.len()), text),
        ]));
        for risk in summary.residual_risks.iter().take(4) {
            lines.push(Line::from(vec![
                Span::styled("  ! ", Style::default().fg(theme.warning)),
                Span::styled(risk.clone(), text),
            ]));
        }
        if summary.residual_risks.len() > 4 {
            lines.push(Line::from(Span::styled(
                format!(
                    "  {} {} more risks",
                    glyphs.ellipsis(),
                    summary.residual_risks.len() - 4
                ),
                dim,
            )));
        }
    }

    for note in summary.notes.iter().take(4) {
        lines.push(Line::from(vec![
            Span::styled("Note: ", label),
            Span::styled(note.clone(), text),
        ]));
    }

    lines
}

pub(crate) fn tool_card_lines_for_virtual_scroll(
    block: &RenderBlock,
    tool: &ToolCardModel,
    width: u16,
    profile: RendererProfile,
    theme: &Theme,
    collapsed: bool,
    glyphs: RenderGlyphs,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if let Some(plan) = tool.plan.as_ref() {
        append_plan_lines(&mut lines, plan, collapsed, profile, theme, glyphs);
    }
    if let Some(command_run) = tool.command_run.as_ref() {
        append_command_run_lines(&mut lines, command_run, collapsed, theme);
    }
    if tool.command_run.is_none() && tool.plan.is_none() {
        if let Some(summary) = tool.summary.as_deref() {
            lines.push(Line::from(Span::styled(
                summary.to_string(),
                Style::default().fg(theme.text_dim),
            )));
        }
    }

    if collapsed {
        if lines.is_empty() {
            lines.push(Line::from(Span::styled(
                "collapsed",
                Style::default().fg(theme.text_dim),
            )));
        }
        return lines;
    }

    for section in &tool.sections {
        if tool.command_run.is_some() && !command_section_is_terminal_visible(section) {
            continue;
        }
        if tool.plan.is_some() && plan_section_is_terminal_replaced(section) {
            continue;
        }
        append_section_lines(
            &mut lines,
            section,
            width,
            section_line_limit_for_block(block, profile),
            profile.tool_section_line_chars(),
            diff_display_mode_for_block(block),
            glyphs,
            theme,
        );
    }

    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "(no content)",
            Style::default().fg(theme.text_dim),
        )));
    }
    lines
}

fn append_plan_lines(
    lines: &mut Vec<Line<'static>>,
    plan: &PlanRenderModel,
    collapsed: bool,
    profile: RendererProfile,
    theme: &Theme,
    glyphs: RenderGlyphs,
) {
    let label = Style::default().fg(theme.info).add_modifier(Modifier::BOLD);
    let text = Style::default().fg(theme.text);
    let dim = Style::default().fg(theme.text_dim);

    lines.push(Line::from(vec![
        Span::styled("Plan: ", label),
        Span::styled(plan.summary_line(), text),
    ]));
    if let Some(active) = plan.active_step() {
        lines.push(Line::from(vec![
            Span::styled("Active: ", label),
            Span::styled(active.content.clone(), text),
        ]));
    }
    if collapsed {
        lines.push(Line::from(Span::styled(
            "Plan folded; expand to inspect all steps.",
            dim.add_modifier(Modifier::ITALIC),
        )));
        return;
    }

    let limit = plan_step_limit(profile);
    for step in plan.steps.iter().take(limit) {
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {} ", plan_status_marker(step.status, glyphs)),
                plan_status_style(step.status, theme),
            ),
            Span::styled(format!("{}  ", step.label), dim),
            Span::styled(step.content.clone(), text),
        ]));
    }
    if plan.steps.len() > limit {
        lines.push(Line::from(Span::styled(
            format!(
                "  {} {} more plan steps hidden",
                glyphs.ellipsis(),
                plan.steps.len() - limit
            ),
            dim,
        )));
    }
}

fn plan_plain_lines(
    plan: &PlanRenderModel,
    collapsed: bool,
    profile: RendererProfile,
    glyphs: RenderGlyphs,
) -> Vec<String> {
    let mut lines = vec![format!("Plan: {}", plan.summary_line())];
    if let Some(active) = plan.active_step() {
        lines.push(format!("Active: {}", active.content));
    }
    if collapsed {
        lines.push("Plan folded; expand to inspect all steps.".to_string());
        return lines;
    }

    let limit = plan_step_limit(profile);
    lines.extend(plan.steps.iter().take(limit).map(|step| {
        format!(
            "  {} {} {}",
            plan_status_marker(step.status, glyphs),
            step.label,
            step.content
        )
    }));
    if plan.steps.len() > limit {
        lines.push(format!(
            "  {} {} more plan steps hidden",
            glyphs.ellipsis(),
            plan.steps.len() - limit
        ));
    }
    lines
}

fn plan_step_limit(profile: RendererProfile) -> usize {
    match profile {
        RendererProfile::Small => 3,
        RendererProfile::Medium => 6,
        RendererProfile::Large => 12,
    }
}

fn plan_status_marker(status: PlanStepStatus, glyphs: RenderGlyphs) -> &'static str {
    match (glyphs.mode, status) {
        (RenderGlyphMode::Unicode, PlanStepStatus::Completed) => "✓",
        (RenderGlyphMode::Unicode, PlanStepStatus::InProgress) => "→",
        (RenderGlyphMode::Unicode, PlanStepStatus::Pending) => "·",
        (RenderGlyphMode::Unicode, PlanStepStatus::Blocked) => "!",
        (RenderGlyphMode::Unicode, PlanStepStatus::Cancelled) => "×",
        (RenderGlyphMode::Unicode, PlanStepStatus::Other) => "-",
        (RenderGlyphMode::Ascii, PlanStepStatus::Completed) => "x",
        (RenderGlyphMode::Ascii, PlanStepStatus::InProgress) => ">",
        (RenderGlyphMode::Ascii, PlanStepStatus::Pending) => "-",
        (RenderGlyphMode::Ascii, PlanStepStatus::Blocked) => "!",
        (RenderGlyphMode::Ascii, PlanStepStatus::Cancelled) => "!",
        (RenderGlyphMode::Ascii, PlanStepStatus::Other) => "-",
    }
}

fn plan_status_style(status: PlanStepStatus, theme: &Theme) -> Style {
    match status {
        PlanStepStatus::Completed => Style::default().fg(theme.success),
        PlanStepStatus::InProgress => Style::default()
            .fg(theme.warning)
            .add_modifier(Modifier::BOLD),
        PlanStepStatus::Blocked | PlanStepStatus::Cancelled => Style::default().fg(theme.error),
        PlanStepStatus::Pending | PlanStepStatus::Other => Style::default().fg(theme.text_dim),
    }
}

fn append_command_run_lines(
    lines: &mut Vec<Line<'static>>,
    command: &CommandRunRenderModel,
    collapsed: bool,
    theme: &Theme,
) {
    let label = Style::default().fg(theme.info).add_modifier(Modifier::BOLD);
    let text = Style::default().fg(theme.text);
    let dim = Style::default().fg(theme.text_dim);

    if let Some(command_text) = command.command.as_deref() {
        lines.push(Line::from(vec![
            Span::styled("command: ", label),
            Span::styled(command_text.to_string(), text),
        ]));
    }
    if let Some(cwd) = command.cwd.as_deref() {
        lines.push(Line::from(vec![
            Span::styled("cwd: ", label),
            Span::styled(cwd.to_string(), dim),
        ]));
    }
    lines.push(Line::from(vec![
        Span::styled("Status: ", label),
        Span::styled(command.status_line(), text),
    ]));
    lines.push(Line::from(vec![
        Span::styled("Output: ", label),
        Span::styled(command.output_summary_line(), dim),
    ]));
    if collapsed {
        lines.push(Line::from(Span::styled(
            "Output folded; expand to inspect command details.",
            dim.add_modifier(Modifier::ITALIC),
        )));
    }
}

fn command_run_plain_lines(command: &CommandRunRenderModel, collapsed: bool) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(command_text) = command.command.as_deref() {
        lines.push(format!("command: {command_text}"));
    }
    if let Some(cwd) = command.cwd.as_deref() {
        lines.push(format!("cwd: {cwd}"));
    }
    lines.push(format!("Status: {}", command.status_line()));
    lines.push(format!("Output: {}", command.output_summary_line()));
    if collapsed {
        lines.push("Output folded; expand to inspect command details.".to_string());
    }
    lines
}

fn command_section_is_terminal_visible(section: &ToolSection) -> bool {
    !matches!(
        section.kind,
        ToolSectionKind::Input | ToolSectionKind::Metadata
    )
}

fn plan_section_is_terminal_replaced(section: &ToolSection) -> bool {
    matches!(section.title.as_str(), "todos" | "input")
}

fn section_line_limit_for_block(block: &RenderBlock, profile: RendererProfile) -> usize {
    if block.state.expanded {
        profile.tool_expanded_lines()
    } else {
        profile.tool_preview_lines()
    }
}

fn diff_display_mode_for_block(block: &RenderBlock) -> DiffDisplayMode {
    if block.state.expanded {
        DiffDisplayMode::Expanded
    } else {
        DiffDisplayMode::Compact
    }
}

fn prefix_for_block(
    kind: RenderBlockKind,
    theme: &Theme,
    glyphs: RenderGlyphs,
) -> (Style, &'static str) {
    match kind {
        RenderBlockKind::User => (
            Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD),
            glyphs.user,
        ),
        RenderBlockKind::Assistant => (
            Style::default()
                .fg(theme.success)
                .add_modifier(Modifier::BOLD),
            glyphs.assistant,
        ),
        RenderBlockKind::System => (Style::default().fg(theme.system_message_fg), glyphs.system),
        RenderBlockKind::Error => (
            Style::default()
                .fg(theme.error)
                .add_modifier(Modifier::BOLD),
            glyphs.error,
        ),
        RenderBlockKind::FileChangeSummary => (
            Style::default().fg(theme.info).add_modifier(Modifier::BOLD),
            glyphs.file_change,
        ),
        RenderBlockKind::CommandOutput => (Style::default().fg(theme.info), glyphs.command_output),
        RenderBlockKind::Progress => (Style::default().fg(theme.text_dim), glyphs.progress),
        RenderBlockKind::Attachment => (Style::default().fg(theme.text_dim), glyphs.attachment),
        RenderBlockKind::Tool => (Style::default().fg(theme.info), glyphs.tool),
        RenderBlockKind::ApprovalDecision => (
            Style::default().fg(theme.text_dim),
            glyphs.approval_decision,
        ),
        RenderBlockKind::FinalSummary => (
            Style::default()
                .fg(theme.success)
                .add_modifier(Modifier::BOLD),
            glyphs.final_summary,
        ),
        RenderBlockKind::SkillInvocation => (Style::default().fg(theme.info), glyphs.skill),
    }
}

fn style_for_block(kind: RenderBlockKind, theme: &Theme) -> Style {
    match kind {
        RenderBlockKind::System => Style::default().fg(theme.system_message_fg),
        RenderBlockKind::Error => Style::default().fg(theme.error),
        RenderBlockKind::FileChangeSummary => Style::default().fg(theme.info),
        RenderBlockKind::Progress
        | RenderBlockKind::Attachment
        | RenderBlockKind::FinalSummary
        | RenderBlockKind::ApprovalDecision => Style::default().fg(theme.text_dim),
        _ => Style::default().fg(theme.text),
    }
}

fn style_for_approval_decision(decision: &ApprovalDecisionModel, theme: &Theme) -> Style {
    match decision.decision {
        ApprovalDecisionKind::Allowed | ApprovalDecisionKind::AlwaysAllowed => {
            Style::default().fg(theme.success)
        }
        ApprovalDecisionKind::Denied | ApprovalDecisionKind::Cancelled => {
            Style::default().fg(theme.error)
        }
    }
}

fn tool_title(tool: &ToolCardModel, collapsed: bool, glyphs: RenderGlyphs) -> String {
    let disclosure = if collapsed {
        glyphs.disclosure_collapsed
    } else {
        glyphs.disclosure_expanded
    };
    let phase = match tool.phase {
        ToolPhase::Requested => "requested",
        ToolPhase::Running => "running",
        ToolPhase::Succeeded => "done",
        ToolPhase::Failed => "failed",
        ToolPhase::WaitingApproval => "approval",
        ToolPhase::Rejected => "rejected",
    };
    format!(
        " {disclosure} {}{}{phase} ",
        tool.product_title(),
        glyphs.separator()
    )
}

fn append_section_lines(
    lines: &mut Vec<Line<'static>>,
    section: &ToolSection,
    width: u16,
    max_lines: usize,
    max_chars_per_line: usize,
    diff_mode: DiffDisplayMode,
    glyphs: RenderGlyphs,
    theme: &Theme,
) {
    let section_style = section_style(section.kind, theme);
    let label_style = section_style.add_modifier(Modifier::BOLD);
    lines.push(Line::from(Span::styled(
        format!("{}:", section.title),
        label_style,
    )));

    let (body, clipped, hidden_lines) =
        bounded_section_body(&section.body, max_lines, max_chars_per_line, glyphs);
    if let Some(code) = section.code.as_ref() {
        lines.extend(code_section_lines(&body, code, theme, glyphs));
    } else if section.kind == ToolSectionKind::Diff || body_looks_diff(&section.body) {
        append_diff_lines(
            lines,
            &section.body,
            max_lines,
            max_chars_per_line,
            diff_mode,
            section_style,
            theme,
            glyphs,
        );
    } else if body_looks_markdown(&body) {
        let markdown_lines = crate::widgets::markdown::MarkdownWidget::new(&body)
            .theme(theme)
            .base_style(section_style)
            .glyphs(glyphs)
            .max_width(width)
            .parse_to_lines();
        lines.extend(markdown_lines);
    } else {
        for line in body.lines() {
            lines.push(Line::from(Span::styled(line.to_string(), section_style)));
        }
    }

    if clipped
        && section.kind != ToolSectionKind::Diff
        && !body_looks_diff(&section.body)
        && section.code.is_none()
    {
        let hint = clipping_hint_text(hidden_lines, glyphs);
        lines.push(Line::from(Span::styled(
            hint,
            Style::default()
                .fg(theme.text_dim)
                .add_modifier(Modifier::ITALIC),
        )));
    }
    if clipped && section.code.is_some() {
        let hint = clipping_hint_text(hidden_lines, glyphs);
        lines.push(Line::from(Span::styled(
            hint,
            Style::default()
                .fg(theme.text_dim)
                .add_modifier(Modifier::ITALIC),
        )));
    }
}

fn code_section_lines(
    body: &str,
    code: &CodeSectionRenderModel,
    theme: &Theme,
    glyphs: RenderGlyphs,
) -> Vec<Line<'static>> {
    let mut widget = HighlightedCodeWidget::new(body, theme)
        .line_numbers(code.line_numbers)
        .line_number_separator(line_number_separator(glyphs))
        .start_line(code.start_line.max(1));
    if let Some(file_path) = code.file_path.as_deref() {
        widget = widget.file_path(file_path);
    }

    let mut lines = widget.build_lines();
    if code.hidden_lines > 0 {
        lines.push(Line::from(Span::styled(
            format!(
                "{} {} not shown",
                glyphs.ellipsis(),
                pluralize(code.hidden_lines, "line")
            ),
            Style::default()
                .fg(theme.text_dim)
                .add_modifier(Modifier::ITALIC),
        )));
    }
    lines
}

fn line_number_separator(glyphs: RenderGlyphs) -> &'static str {
    match glyphs.mode {
        RenderGlyphMode::Unicode => "│",
        RenderGlyphMode::Ascii => "|",
    }
}

fn clipping_hint_text(hidden_lines: usize, glyphs: RenderGlyphs) -> String {
    if hidden_lines > 0 {
        format!(
            "{} {hidden_lines} more lines clipped in TUI preview",
            glyphs.ellipsis()
        )
    } else {
        format!(
            "{} output clipped in TUI preview; expand or inspect tool output for the full body",
            glyphs.ellipsis()
        )
    }
}

fn append_diff_lines(
    lines: &mut Vec<Line<'static>>,
    body: &str,
    max_lines: usize,
    max_chars_per_line: usize,
    mode: DiffDisplayMode,
    base_style: Style,
    theme: &Theme,
    glyphs: RenderGlyphs,
) {
    for line in diff_display_lines(body, max_lines, max_chars_per_line, mode, glyphs) {
        lines.push(Line::from(Span::styled(
            line.text,
            diff_line_style(line.kind, base_style, theme),
        )));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiffDisplayMode {
    Compact,
    Expanded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiffDisplayLineKind {
    Summary,
    File,
    Header,
    Hunk,
    Added,
    Removed,
    Context,
    Hint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DiffDisplayLine {
    text: String,
    kind: DiffDisplayLineKind,
}

fn diff_display_lines(
    body: &str,
    max_lines: usize,
    max_chars_per_line: usize,
    mode: DiffDisplayMode,
    glyphs: RenderGlyphs,
) -> Vec<DiffDisplayLine> {
    let stats = diff_stats(body);
    let target_rows = diff_target_rows(max_lines, &stats, mode);
    let mut rows = Vec::new();

    rows.push(DiffDisplayLine {
        text: format!(
            "summary: {}{}{}{}+{} -{}",
            pluralize(stats.files.len().max(1), "file"),
            glyphs.separator(),
            pluralize(stats.hunks, "hunk"),
            glyphs.separator(),
            stats.additions,
            stats.deletions
        ),
        kind: DiffDisplayLineKind::Summary,
    });

    let file_limit = diff_file_summary_limit(mode);
    for file in stats.files.iter().take(file_limit) {
        if rows.len() >= target_rows {
            break;
        }
        rows.push(DiffDisplayLine {
            text: format!(
                "file: {}  +{} -{}",
                file.path, file.additions, file.deletions
            ),
            kind: DiffDisplayLineKind::File,
        });
    }
    if stats.files.len() > file_limit && rows.len() < target_rows {
        rows.push(DiffDisplayLine {
            text: format!(
                "{} {} more files",
                glyphs.ellipsis(),
                stats.files.len() - file_limit
            ),
            kind: DiffDisplayLineKind::Hint,
        });
    }

    let mut raw_budget = target_rows.saturating_sub(rows.len());
    if mode == DiffDisplayMode::Compact && !compact_diff_can_show_full_raw(&stats, max_lines) {
        raw_budget = raw_budget.min(compact_diff_raw_budget(raw_budget));
    }
    let candidates = diff_raw_candidates(body, max_chars_per_line, glyphs);
    let initial_selection = select_diff_raw_candidates(&candidates, raw_budget, mode);
    let important_total = candidates
        .iter()
        .filter(|candidate| candidate.important)
        .count();
    let initially_selected_important = initial_selection
        .iter()
        .filter(|candidate| candidate.important)
        .count();
    let raw_lines_hidden = initial_selection.len() < candidates.len();
    let needs_hint = raw_budget > 1
        && ((raw_lines_hidden && mode == DiffDisplayMode::Expanded)
            || initially_selected_important < important_total
            || (raw_lines_hidden && !compact_diff_can_show_full_raw(&stats, max_lines))
            || initial_selection
                .iter()
                .any(|candidate| candidate.truncated));
    if needs_hint {
        raw_budget = raw_budget.saturating_sub(1);
    }

    let selected = select_diff_raw_candidates(&candidates, raw_budget, mode);
    for candidate in &selected {
        rows.push(DiffDisplayLine {
            text: candidate.text.clone(),
            kind: candidate.kind,
        });
    }

    if needs_hint && rows.len() < target_rows {
        let hidden_lines = candidates.len().saturating_sub(selected.len());
        rows.push(DiffDisplayLine {
            text: diff_fold_hint_text(hidden_lines, mode, glyphs),
            kind: DiffDisplayLineKind::Hint,
        });
    }

    rows
}

fn diff_target_rows(max_lines: usize, stats: &DiffStats, mode: DiffDisplayMode) -> usize {
    let max_lines = max_lines.max(1);
    match mode {
        DiffDisplayMode::Compact => max_lines.min(stats.total_lines.max(1)),
        DiffDisplayMode::Expanded => max_lines,
    }
}

fn diff_file_summary_limit(mode: DiffDisplayMode) -> usize {
    match mode {
        DiffDisplayMode::Compact => 4,
        DiffDisplayMode::Expanded => 8,
    }
}

fn compact_diff_can_show_full_raw(stats: &DiffStats, max_lines: usize) -> bool {
    stats.files.len() <= 1 && stats.total_lines <= max_lines
}

fn compact_diff_raw_budget(available: usize) -> usize {
    available.min(3)
}

fn diff_fold_hint_text(hidden_lines: usize, mode: DiffDisplayMode, glyphs: RenderGlyphs) -> String {
    let action = match mode {
        DiffDisplayMode::Compact => "expand for full context",
        DiffDisplayMode::Expanded => "inspect full output for remaining context",
    };
    if hidden_lines > 0 {
        format!(
            "{} {hidden_lines} more diff lines folded; {action}",
            glyphs.ellipsis()
        )
    } else {
        format!("{} long diff lines clipped; {action}", glyphs.ellipsis())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DiffRawCandidate {
    index: usize,
    text: String,
    kind: DiffDisplayLineKind,
    priority: u8,
    important: bool,
    truncated: bool,
}

fn diff_raw_candidates(
    body: &str,
    max_chars_per_line: usize,
    glyphs: RenderGlyphs,
) -> Vec<DiffRawCandidate> {
    body.lines()
        .enumerate()
        .map(|(index, line)| {
            let clean = terminal_output_line(line);
            let text =
                truncate_display_width_with_suffix(&clean, max_chars_per_line, glyphs.ellipsis());
            let kind = diff_display_line_kind(line);
            let priority = diff_line_priority(line, kind);
            DiffRawCandidate {
                index,
                truncated: text != clean,
                text,
                kind,
                priority,
                important: matches!(
                    kind,
                    DiffDisplayLineKind::Header
                        | DiffDisplayLineKind::Hunk
                        | DiffDisplayLineKind::Added
                        | DiffDisplayLineKind::Removed
                ) && priority < 4,
            }
        })
        .collect()
}

fn select_diff_raw_candidates(
    candidates: &[DiffRawCandidate],
    budget: usize,
    mode: DiffDisplayMode,
) -> Vec<&DiffRawCandidate> {
    if budget == 0 {
        return Vec::new();
    }

    match mode {
        DiffDisplayMode::Compact => {
            let mut selected = candidates.iter().collect::<Vec<_>>();
            selected.sort_by_key(|candidate| (candidate.priority, candidate.index));
            selected.truncate(budget);
            selected.sort_by_key(|candidate| candidate.index);
            selected
        }
        DiffDisplayMode::Expanded => candidates.iter().take(budget).collect(),
    }
}

fn diff_line_priority(line: &str, kind: DiffDisplayLineKind) -> u8 {
    let trimmed = line.trim_start();
    match kind {
        DiffDisplayLineKind::Header if trimmed.starts_with("diff --git") => 0,
        DiffDisplayLineKind::Hunk => 1,
        DiffDisplayLineKind::Added | DiffDisplayLineKind::Removed => 2,
        DiffDisplayLineKind::Context => 3,
        DiffDisplayLineKind::Header => 4,
        DiffDisplayLineKind::Summary | DiffDisplayLineKind::File | DiffDisplayLineKind::Hint => 5,
    }
}

fn diff_line_style(kind: DiffDisplayLineKind, base_style: Style, theme: &Theme) -> Style {
    match kind {
        DiffDisplayLineKind::Summary | DiffDisplayLineKind::File => Style::default().fg(theme.info),
        DiffDisplayLineKind::Header => Style::default().fg(theme.info),
        DiffDisplayLineKind::Hunk => Style::default().fg(theme.info).add_modifier(Modifier::BOLD),
        DiffDisplayLineKind::Added => Style::default().fg(theme.success),
        DiffDisplayLineKind::Removed => Style::default().fg(theme.error),
        DiffDisplayLineKind::Hint => Style::default()
            .fg(theme.text_dim)
            .add_modifier(Modifier::ITALIC),
        DiffDisplayLineKind::Context => base_style,
    }
}

fn diff_display_line_kind(line: &str) -> DiffDisplayLineKind {
    let trimmed = line.trim_start();
    if trimmed.starts_with("diff --git")
        || trimmed.starts_with("index ")
        || trimmed.starts_with("--- ")
        || trimmed.starts_with("+++ ")
    {
        DiffDisplayLineKind::Header
    } else if trimmed.starts_with("@@") {
        DiffDisplayLineKind::Hunk
    } else if trimmed.starts_with('+') {
        DiffDisplayLineKind::Added
    } else if trimmed.starts_with('-') {
        DiffDisplayLineKind::Removed
    } else {
        DiffDisplayLineKind::Context
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct DiffStats {
    files: Vec<DiffFileStats>,
    additions: usize,
    deletions: usize,
    hunks: usize,
    total_lines: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct DiffFileStats {
    path: String,
    additions: usize,
    deletions: usize,
    hunks: usize,
}

fn diff_stats(body: &str) -> DiffStats {
    let mut stats = DiffStats {
        total_lines: body.lines().count(),
        ..DiffStats::default()
    };
    let mut current_file: Option<usize> = None;
    let mut pending_old_path: Option<String> = None;

    for line in body.lines() {
        let trimmed = line.trim_start();
        if let Some(path) = diff_git_path(trimmed) {
            current_file = Some(push_diff_file(&mut stats, path));
            pending_old_path = None;
            continue;
        }

        if let Some(path) = trimmed.strip_prefix("--- ") {
            pending_old_path = Some(clean_diff_path(path));
            continue;
        }

        if let Some(path) = trimmed.strip_prefix("+++ ") {
            if current_file.is_none() {
                let new_path = clean_diff_path(path);
                let old_path = pending_old_path.take().unwrap_or_default();
                let label = if !old_path.is_empty() && old_path != new_path {
                    format!("{old_path} -> {new_path}")
                } else if new_path.is_empty() {
                    "diff".to_string()
                } else {
                    new_path
                };
                current_file = Some(push_diff_file(&mut stats, label));
            }
            continue;
        }

        if trimmed.starts_with("@@") {
            let index = ensure_diff_file(&mut stats, &mut current_file);
            stats.hunks += 1;
            stats.files[index].hunks += 1;
            continue;
        }

        if trimmed.starts_with('+') && !trimmed.starts_with("+++") {
            let index = ensure_diff_file(&mut stats, &mut current_file);
            stats.additions += 1;
            stats.files[index].additions += 1;
        } else if trimmed.starts_with('-') && !trimmed.starts_with("---") {
            let index = ensure_diff_file(&mut stats, &mut current_file);
            stats.deletions += 1;
            stats.files[index].deletions += 1;
        }
    }

    if stats.files.is_empty() && stats.total_lines > 0 {
        stats.files.push(DiffFileStats {
            path: "diff".to_string(),
            additions: stats.additions,
            deletions: stats.deletions,
            hunks: stats.hunks,
        });
    }

    stats
}

fn diff_git_path(line: &str) -> Option<String> {
    let rest = line.strip_prefix("diff --git ")?;
    let mut parts = rest.split_whitespace();
    let old = parts.next().map(clean_diff_path).unwrap_or_default();
    let new = parts.next().map(clean_diff_path).unwrap_or_default();
    Some(if !new.is_empty() {
        new
    } else if !old.is_empty() {
        old
    } else {
        "diff".to_string()
    })
}

fn clean_diff_path(path: &str) -> String {
    path.trim()
        .trim_matches('"')
        .strip_prefix("a/")
        .or_else(|| path.trim().trim_matches('"').strip_prefix("b/"))
        .unwrap_or_else(|| path.trim().trim_matches('"'))
        .to_string()
}

fn push_diff_file(stats: &mut DiffStats, path: String) -> usize {
    stats.files.push(DiffFileStats {
        path,
        ..DiffFileStats::default()
    });
    stats.files.len() - 1
}

fn ensure_diff_file(stats: &mut DiffStats, current_file: &mut Option<usize>) -> usize {
    if let Some(index) = *current_file {
        return index;
    }

    let index = push_diff_file(stats, "diff".to_string());
    *current_file = Some(index);
    index
}

fn pluralize(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

fn section_style(kind: ToolSectionKind, theme: &Theme) -> Style {
    match kind {
        ToolSectionKind::Input => Style::default().fg(theme.info),
        ToolSectionKind::Output => Style::default().fg(theme.text),
        ToolSectionKind::Diff => Style::default().fg(theme.text),
        ToolSectionKind::Error => Style::default().fg(theme.error),
        ToolSectionKind::Metadata => Style::default().fg(theme.text_dim),
    }
}

fn bounded_section_body(
    text: &str,
    max_lines: usize,
    max_cells_per_line: usize,
    glyphs: RenderGlyphs,
) -> (String, bool, usize) {
    let mut clipped = false;
    let total_lines = text.lines().count();
    let mut hidden_lines = 0usize;
    let mut out = Vec::new();

    for (index, line) in text.lines().enumerate() {
        if index >= max_lines {
            clipped = true;
            hidden_lines = total_lines.saturating_sub(max_lines);
            break;
        }
        let clean = terminal_output_line(line);
        let truncated =
            truncate_display_width_with_suffix(&clean, max_cells_per_line, glyphs.ellipsis());
        if truncated != clean {
            clipped = true;
        }
        out.push(truncated);
    }

    if out.is_empty() && !text.is_empty() {
        let clean = terminal_output_line(text);
        out.push(truncate_display_width_with_suffix(
            &clean,
            max_cells_per_line,
            glyphs.ellipsis(),
        ));
    }

    (out.join("\n"), clipped, hidden_lines)
}

fn terminal_output_line(line: &str) -> String {
    strip_ansi_escapes::strip_str(line).replace('\t', "    ")
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

    let content_budget = max_cells.saturating_sub(suffix_width);
    let mut out = truncate_display_width(text, content_budget);
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

fn body_looks_markdown(text: &str) -> bool {
    text.contains("```")
        || text.lines().any(|line| {
            let trimmed = line.trim_start();
            trimmed.starts_with("# ")
                || trimmed.starts_with("## ")
                || trimmed.starts_with("- ")
                || trimmed.starts_with("* ")
                || trimmed.starts_with("> ")
                || trimmed.starts_with("1. ")
        })
}

fn body_looks_diff(text: &str) -> bool {
    let mut has_diff_header = false;
    let mut has_change_line = false;

    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("diff --git")
            || trimmed.starts_with("@@")
            || trimmed.starts_with("--- ")
            || trimmed.starts_with("+++ ")
        {
            has_diff_header = true;
        }
        if (trimmed.starts_with('+') && !trimmed.starts_with("+++"))
            || (trimmed.starts_with('-') && !trimmed.starts_with("---"))
        {
            has_change_line = true;
        }
    }

    has_diff_header && has_change_line
}

fn wrapped_line_count(text: &str, width: u16) -> usize {
    crate::widgets::markdown::wrapped_line_count_for_text(text, width)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render_glyphs::RenderGlyphs;
    use crate::render_model::{
        ErrorRenderModel, PlanRenderModel, PlanStepRenderModel, PlanStepStatus, RenderBlockState,
        ToolCardModel, ToolPhase, ToolSection, ToolSectionKind,
    };
    use crate::render_profile::RendererProfile;
    use ratatui::buffer::Buffer;

    #[test]
    fn tool_card_height_is_bounded_for_large_output() {
        let output = (0..10_000)
            .map(|n| format!("line {n:05}"))
            .collect::<Vec<_>>()
            .join("\n");
        let block = RenderBlock {
            id: "tool-0-1".to_string(),
            source_indices: vec![0, 1],
            kind: RenderBlockKind::Tool,
            state: RenderBlockState::default(),
            nodes: Vec::new(),
            tool: Some(ToolCardModel {
                name: "Bash".to_string(),
                phase: ToolPhase::Succeeded,
                summary: Some("exit 0".to_string()),
                sections: vec![ToolSection::new("stdout", output, ToolSectionKind::Output)],
                command_run: None,
                plan: None,
            }),
        };
        let theme = Theme::default();
        let height = RenderBlockWidget::new(&block, &theme).required_height(80);

        assert!(height < 100, "height should be bounded, got {height}");
    }

    #[test]
    fn bounded_section_body_strips_ansi_and_clips_by_display_width() {
        let (body, clipped, hidden) = bounded_section_body(
            "\u{1b}[31m错误错误错误错误\u{1b}[0m\tok",
            8,
            9,
            RenderGlyphs::unicode(),
        );

        assert!(!body.contains('\u{1b}'), "ANSI escapes leaked: {body:?}");
        assert!(body.contains("错误错误"));
        assert!(
            clipped,
            "wide CJK output should be clipped to the cell budget"
        );
        assert_eq!(hidden, 0);
        assert!(
            unicode_width::UnicodeWidthStr::width(body.as_str()) <= 9,
            "body exceeded display budget: {body:?}"
        );
    }

    #[test]
    fn focused_rows_reserve_the_focus_bar_width() {
        let block = RenderBlock {
            id: "assistant-0".to_string(),
            source_indices: vec![0],
            kind: RenderBlockKind::Assistant,
            state: RenderBlockState::default(),
            nodes: vec![RenderNode::PlainText("abcdefg".to_string())],
            tool: None,
        };
        let theme = Theme::default();

        let height = RenderBlockWidget::new(&block, &theme)
            .add_margin(false)
            .focused(true)
            .required_height(10);

        assert_eq!(
            height, 2,
            "focused rows reserve one extra cell for the focus band before measuring wraps"
        );
    }

    #[test]
    fn renders_tool_sections_without_raw_json_keys() {
        let block = RenderBlock {
            id: "tool-0-1".to_string(),
            source_indices: vec![0, 1],
            kind: RenderBlockKind::Tool,
            state: RenderBlockState::default(),
            nodes: Vec::new(),
            tool: Some(ToolCardModel {
                name: "Bash".to_string(),
                phase: ToolPhase::Succeeded,
                summary: Some("exit 0".to_string()),
                sections: vec![ToolSection::new("stdout", "ok", ToolSectionKind::Output)],
                command_run: None,
                plan: None,
            }),
        };
        let theme = Theme::default();
        let widget = RenderBlockWidget::new(&block, &theme).add_margin(false);
        let mut buf = Buffer::empty(Rect::new(0, 0, 60, 8));

        widget.render(buf.area, &mut buf);

        let rendered = buffer_text(&buf);
        assert!(rendered.contains("stdout"));
        assert!(rendered.contains("ok"));
        assert!(!rendered.contains("\"stdout\""));
    }

    #[test]
    fn renders_todowrite_as_structured_plan_steps() {
        let block = RenderBlock {
            id: "tool-plan".to_string(),
            source_indices: vec![0],
            kind: RenderBlockKind::Tool,
            state: RenderBlockState::default(),
            nodes: Vec::new(),
            tool: Some(ToolCardModel {
                name: "TodoWrite".to_string(),
                phase: ToolPhase::Succeeded,
                summary: Some("3 todos".to_string()),
                sections: vec![ToolSection::new(
                    "todos",
                    "completed: 读渲染文档\nin_progress: 建结构化计划\npending: 跑回归",
                    ToolSectionKind::Output,
                )],
                command_run: None,
                plan: Some(PlanRenderModel {
                    title: "Plan".to_string(),
                    steps: vec![
                        PlanStepRenderModel {
                            status: PlanStepStatus::Completed,
                            label: "completed".to_string(),
                            content: "读渲染文档".to_string(),
                        },
                        PlanStepRenderModel {
                            status: PlanStepStatus::InProgress,
                            label: "in_progress".to_string(),
                            content: "建结构化计划".to_string(),
                        },
                        PlanStepRenderModel {
                            status: PlanStepStatus::Pending,
                            label: "pending".to_string(),
                            content: "跑回归".to_string(),
                        },
                    ],
                }),
            }),
        };
        let theme = Theme::default();
        let widget = RenderBlockWidget::new(&block, &theme).add_margin(false);
        let mut buf = Buffer::empty(Rect::new(0, 0, 82, 12));

        widget.render(buf.area, &mut buf);

        let rendered = buffer_text(&buf);
        assert!(rendered.contains("Plan: 3 steps"), "{rendered}");
        assert!(rendered.contains("Active:"), "{rendered}");
        assert!(rendered.contains('建'), "{rendered}");
        assert!(rendered.contains("completed"), "{rendered}");
        assert!(rendered.contains("in_progress"), "{rendered}");
        assert!(rendered.contains("pending"), "{rendered}");
        assert!(
            !rendered.contains("todos:"),
            "structured plan should replace duplicate raw todos section\n{rendered}"
        );
    }

    #[test]
    fn renders_error_cards_as_layered_failure_objects() {
        let block = RenderBlock {
            id: "error-layered".to_string(),
            source_indices: vec![0],
            kind: RenderBlockKind::Error,
            state: RenderBlockState {
                streaming: false,
                error: true,
                expanded: false,
            },
            nodes: vec![RenderNode::Error(ErrorRenderModel {
                title: "Command error".to_string(),
                summary: "Build failed".to_string(),
                key_detail: Some("thread 'main' panicked at src/main.rs:10".to_string()),
                details: Some(
                    (1..=10)
                        .map(|line| format!("detail line {line:02}"))
                        .collect::<Vec<_>>()
                        .join("\n"),
                ),
                detail_hidden_line_count: 2,
                retry_hint: Some("Automatic retry is scheduled.".to_string()),
                retrying: true,
            })],
            tool: None,
        };
        let theme = Theme::default();
        let mut buf = Buffer::empty(Rect::new(0, 0, 80, 17));

        RenderBlockWidget::new(&block, &theme)
            .add_margin(false)
            .render(buf.area, &mut buf);

        let rendered = buffer_text(&buf);
        assert!(rendered.contains("Command error"), "{rendered}");
        assert!(rendered.contains("Reason: Build failed"), "{rendered}");
        assert!(rendered.contains("Key detail:"), "{rendered}");
        assert!(
            rendered.contains("panicked at src/main.rs:10"),
            "{rendered}"
        );
        assert!(
            rendered.contains("Full log: 2 more lines hidden"),
            "{rendered}"
        );
        assert!(
            rendered.contains("Retry: Automatic retry is scheduled."),
            "{rendered}"
        );
    }

    #[test]
    fn clips_partial_area_to_buffer_before_rendering() {
        let block = RenderBlock {
            id: "assistant-clip".to_string(),
            source_indices: vec![0],
            kind: RenderBlockKind::Assistant,
            state: RenderBlockState::default(),
            nodes: vec![RenderNode::Markdown(
                "## 观察\n\n逐行读代码，检查 `Buffer` 边界。\n".to_string(),
            )],
            tool: None,
        };
        let theme = Theme::default();
        let mut buf = Buffer::empty(Rect::new(0, 0, 32, 5));

        RenderBlockWidget::new(&block, &theme)
            .add_margin(false)
            .render(Rect::new(8, 2, 60, 8), &mut buf);
        RenderBlockWidget::new(&block, &theme)
            .add_margin(false)
            .render(Rect::new(90, 90, 20, 4), &mut buf);

        let rendered = buffer_text(&buf);
        assert!(
            rendered.contains('观') || rendered.contains("Buffer"),
            "partial render should keep visible assistant content\n{rendered}"
        );
    }

    #[test]
    fn renderer_profile_controls_tool_preview_budget() {
        let output = (0..50)
            .map(|n| format!("line {n:02}"))
            .collect::<Vec<_>>()
            .join("\n");
        let block = RenderBlock {
            id: "tool-0-1".to_string(),
            source_indices: vec![0, 1],
            kind: RenderBlockKind::Tool,
            state: RenderBlockState::default(),
            nodes: Vec::new(),
            tool: Some(ToolCardModel {
                name: "Bash".to_string(),
                phase: ToolPhase::Succeeded,
                summary: Some("exit 0".to_string()),
                sections: vec![ToolSection::new("stdout", output, ToolSectionKind::Output)],
                command_run: None,
                plan: None,
            }),
        };
        let theme = Theme::default();

        let small = RenderBlockWidget::new(&block, &theme)
            .profile(RendererProfile::Small)
            .required_height(80);
        let large = RenderBlockWidget::new(&block, &theme)
            .profile(RendererProfile::Large)
            .required_height(80);

        assert!(small < large, "small={small}, large={large}");
    }

    #[test]
    fn renders_unified_diff_sections_with_diff_semantics() {
        let theme = Theme::default();
        let section = ToolSection::new(
            "stdout",
            concat!(
                "diff --git a/src/demo.rs b/src/demo.rs\n",
                "@@ -1,3 +1,4 @@\n",
                " fn main() {\n",
                "-    println!(\"old\");\n",
                "+    println!(\"new\");\n",
                "+    println!(\"extra\");\n",
                " }\n",
            ),
            ToolSectionKind::Output,
        );
        let mut lines = Vec::new();

        append_section_lines(
            &mut lines,
            &section,
            80,
            20,
            120,
            DiffDisplayMode::Compact,
            RenderGlyphs::unicode(),
            &theme,
        );

        let added = lines
            .iter()
            .flat_map(|line| line.spans.iter())
            .find(|span| span.content.as_ref().starts_with("+    println"))
            .expect("added diff line should render");
        let removed = lines
            .iter()
            .flat_map(|line| line.spans.iter())
            .find(|span| span.content.as_ref().starts_with("-    println"))
            .expect("removed diff line should render");
        let hunk = lines
            .iter()
            .flat_map(|line| line.spans.iter())
            .find(|span| span.content.as_ref().starts_with("@@"))
            .expect("hunk header should render");

        assert_eq!(added.style.fg, Some(theme.success));
        assert_eq!(removed.style.fg, Some(theme.error));
        assert_eq!(hunk.style.fg, Some(theme.info));
        assert!(hunk.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn diff_display_lines_add_summary_file_stats_and_fold_hint() {
        let body = concat!(
            "diff --git a/src/demo.rs b/src/demo.rs\n",
            "index 111..222 100644\n",
            "--- a/src/demo.rs\n",
            "+++ b/src/demo.rs\n",
            "@@ -1,6 +1,7 @@\n",
            " line 1\n",
            "-old 2\n",
            "+new 2\n",
            "+new 3\n",
            " line 4\n",
        );

        let lines = diff_display_lines(
            body,
            4,
            120,
            DiffDisplayMode::Compact,
            RenderGlyphs::unicode(),
        );
        let text = lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(text.contains("summary: 1 file · 1 hunk · +2 -1"), "{text}");
        assert!(text.contains("file: src/demo.rs  +2 -1"), "{text}");
        assert!(
            text.contains("diff --git a/src/demo.rs b/src/demo.rs"),
            "{text}"
        );
        assert!(text.contains("more diff lines folded"), "{text}");
    }

    #[test]
    fn compact_diff_stays_summary_first_while_expanded_keeps_more_context() {
        let mut body = concat!(
            "diff --git a/src/large.rs b/src/large.rs\n",
            "index 111..222 100644\n",
            "--- a/src/large.rs\n",
            "+++ b/src/large.rs\n",
            "@@ -1,32 +1,32 @@\n",
        )
        .to_string();
        for index in 0..32 {
            body.push_str(&format!("-old line {index:02}\n"));
            body.push_str(&format!("+new line {index:02}\n"));
        }

        let compact = diff_display_lines(
            &body,
            24,
            120,
            DiffDisplayMode::Compact,
            RenderGlyphs::unicode(),
        );
        let expanded = diff_display_lines(
            &body,
            24,
            120,
            DiffDisplayMode::Expanded,
            RenderGlyphs::unicode(),
        );
        let compact_text = compact
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let expanded_text = expanded
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(
            compact_text.contains("summary: 1 file · 1 hunk · +32 -32"),
            "{compact_text}"
        );
        assert!(compact.len() < expanded.len(), "{compact_text}");
        assert!(
            !compact_text.contains("+new line 10"),
            "compact diff should not become a long log wall\n{compact_text}"
        );
        assert!(
            expanded_text.contains("+new line 07"),
            "expanded diff should preserve ordered context\n{expanded_text}"
        );
        assert!(
            expanded_text.contains("more diff lines folded"),
            "expanded view should still disclose clipped tail context\n{expanded_text}"
        );
    }

    #[test]
    fn ascii_glyph_profile_renders_tool_cards_without_unicode_chrome() {
        let block = RenderBlock {
            id: "tool-ascii".to_string(),
            source_indices: vec![0],
            kind: RenderBlockKind::Tool,
            state: RenderBlockState::default(),
            nodes: Vec::new(),
            tool: Some(ToolCardModel {
                name: "Bash".to_string(),
                phase: ToolPhase::Succeeded,
                summary: Some("exit 0".to_string()),
                sections: vec![ToolSection::new("stdout", "ok", ToolSectionKind::Output)],
                command_run: None,
                plan: None,
            }),
        };
        let theme = Theme::default();
        let mut buf = Buffer::empty(Rect::new(0, 0, 72, 8));

        RenderBlockWidget::new(&block, &theme)
            .add_margin(false)
            .glyphs(RenderGlyphs::ascii())
            .render(buf.area, &mut buf);

        let rendered = buffer_text(&buf);
        assert!(rendered.contains("$  + v Bash"), "{rendered}");
        assert!(rendered.contains("|stdout:"), "{rendered}");
        for forbidden in ["╭", "╰", "│", "─", "▼", "⚡"] {
            assert!(
                !rendered.contains(forbidden),
                "ASCII render leaked unicode glyph {forbidden:?}\n{rendered}"
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
