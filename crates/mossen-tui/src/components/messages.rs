//! Message rendering components — all message type variants.
//!
//! Translates: components/messages/ (41 files) covering every message type:
//! user, assistant, system, tool use/result, thinking, attachments, etc.

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget, Wrap},
};

use crate::theme::Theme;

// ===================================================================
// Message content types (nullRenderingAttachments.ts, teamMemSaved.ts)
// ===================================================================

/// Attachment type in a message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachmentKind {
    File { path: String, mime_type: Option<String> },
    Image { url: String, alt: Option<String> },
    Url { href: String, title: Option<String> },
    Code { language: String, content: String },
}

/// Whether an attachment should be rendered or is null/hidden.
pub fn should_render_attachment(kind: &AttachmentKind) -> bool {
    match kind {
        AttachmentKind::File { path, .. } => !path.is_empty(),
        AttachmentKind::Image { url, .. } => !url.is_empty(),
        AttachmentKind::Url { href, .. } => !href.is_empty(),
        AttachmentKind::Code { content, .. } => !content.is_empty(),
    }
}

/// Team memory saved entry (teamMemSaved.ts).
#[derive(Debug, Clone)]
pub struct TeamMemSavedEntry {
    pub key: String,
    pub value: String,
    pub saved_by: Option<String>,
}

/// Team memory collapsed state (teamMemCollapsed.tsx).
#[derive(Debug, Clone)]
pub struct TeamMemCollapsedState {
    pub entries: Vec<TeamMemSavedEntry>,
    pub is_expanded: bool,
    pub total_count: usize,
}

impl TeamMemCollapsedState {
    pub fn new(entries: Vec<TeamMemSavedEntry>) -> Self {
        let total = entries.len();
        Self {
            entries,
            is_expanded: false,
            total_count: total,
        }
    }

    pub fn toggle_expand(&mut self) {
        self.is_expanded = !self.is_expanded;
    }

    pub fn visible_entries(&self) -> &[TeamMemSavedEntry] {
        if self.is_expanded {
            &self.entries
        } else {
            &self.entries[..self.entries.len().min(3)]
        }
    }
}

// ===================================================================
// User message variants
// ===================================================================

/// User prompt message (UserPromptMessage.tsx / UserTextMessage.tsx).
pub struct UserTextMessageWidget<'a> {
    pub content: &'a str,
    pub is_from_paste: bool,
    pub theme: &'a Theme,
}

impl<'a> UserTextMessageWidget<'a> {
    pub fn new(content: &'a str, theme: &'a Theme) -> Self {
        Self { content, is_from_paste: false, theme }
    }

    pub fn from_paste(mut self, v: bool) -> Self {
        self.is_from_paste = v;
        self
    }
}

impl<'a> Widget for UserTextMessageWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 4 || area.height == 0 {
            return;
        }

        let prefix_style = Style::default()
            .fg(self.theme.primary)
            .add_modifier(Modifier::BOLD);
        buf.set_string(area.x, area.y, "❯", prefix_style);

        let content_area = Rect::new(
            area.x + 3,
            area.y,
            area.width.saturating_sub(3),
            area.height,
        );

        let style = Style::default().fg(self.theme.text);
        let p = Paragraph::new(self.content).style(style).wrap(Wrap { trim: false });
        p.render(content_area, buf);
    }
}

/// User bash input message (UserBashInputMessage.tsx).
pub struct UserBashInputMessageWidget<'a> {
    pub command: &'a str,
    pub theme: &'a Theme,
}

impl<'a> UserBashInputMessageWidget<'a> {
    pub fn new(command: &'a str, theme: &'a Theme) -> Self {
        Self { command, theme }
    }
}

impl<'a> Widget for UserBashInputMessageWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 4 || area.height == 0 {
            return;
        }
        let line = Line::from(vec![
            Span::styled("! ", Style::default().fg(self.theme.warning).add_modifier(Modifier::BOLD)),
            Span::styled("$ ", Style::default().fg(self.theme.text_dim)),
            Span::styled(self.command, Style::default().fg(self.theme.text).add_modifier(Modifier::BOLD)),
        ]);
        buf.set_line(area.x, area.y, &line, area.width);
    }
}

/// User bash output message (UserBashOutputMessage.tsx).
pub struct UserBashOutputMessageWidget<'a> {
    pub output: &'a str,
    pub exit_code: Option<i32>,
    pub is_stderr: bool,
    pub theme: &'a Theme,
}

impl<'a> UserBashOutputMessageWidget<'a> {
    pub fn new(output: &'a str, theme: &'a Theme) -> Self {
        Self { output, exit_code: None, is_stderr: false, theme }
    }

    pub fn exit_code(mut self, code: i32) -> Self {
        self.exit_code = Some(code);
        self
    }

    pub fn stderr(mut self, v: bool) -> Self {
        self.is_stderr = v;
        self
    }
}

impl<'a> Widget for UserBashOutputMessageWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let style = if self.is_stderr {
            Style::default().fg(self.theme.error)
        } else {
            Style::default().fg(self.theme.text_dim)
        };

        let mut y = area.y;
        if let Some(code) = self.exit_code {
            let exit_style = if code == 0 {
                Style::default().fg(self.theme.success)
            } else {
                Style::default().fg(self.theme.error)
            };
            let line = Line::from(vec![
                Span::styled("  exit: ", Style::default().fg(self.theme.text_dim)),
                Span::styled(code.to_string(), exit_style),
            ]);
            buf.set_line(area.x, y, &line, area.width);
            y += 1;
        }

        if y < area.y + area.height {
            let content_area = Rect::new(area.x + 2, y, area.width.saturating_sub(2), area.y + area.height - y);
            let p = Paragraph::new(self.output).style(style).wrap(Wrap { trim: false });
            p.render(content_area, buf);
        }
    }
}

/// User command message (UserCommandMessage.tsx).
pub struct UserCommandMessageWidget<'a> {
    pub command_name: &'a str,
    pub args: &'a str,
    pub theme: &'a Theme,
}

impl<'a> UserCommandMessageWidget<'a> {
    pub fn new(command_name: &'a str, args: &'a str, theme: &'a Theme) -> Self {
        Self { command_name, args, theme }
    }
}

impl<'a> Widget for UserCommandMessageWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let line = Line::from(vec![
            Span::styled("/ ", Style::default().fg(self.theme.info)),
            Span::styled(self.command_name, Style::default().fg(self.theme.info).add_modifier(Modifier::BOLD)),
            Span::styled(format!(" {}", self.args), Style::default().fg(self.theme.text_dim)),
        ]);
        buf.set_line(area.x, area.y, &line, area.width);
    }
}

/// User image message (UserImageMessage.tsx).
pub struct UserImageMessageWidget<'a> {
    pub image_path: &'a str,
    pub alt_text: Option<&'a str>,
    pub theme: &'a Theme,
}

impl<'a> UserImageMessageWidget<'a> {
    pub fn new(image_path: &'a str, theme: &'a Theme) -> Self {
        Self { image_path, alt_text: None, theme }
    }

    pub fn alt_text(mut self, alt: &'a str) -> Self {
        self.alt_text = Some(alt);
        self
    }
}

impl<'a> Widget for UserImageMessageWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let alt = self.alt_text.unwrap_or("[image]");
        let line = Line::from(vec![
            Span::styled("📷 ", Style::default().fg(self.theme.text_dim)),
            Span::styled(alt, Style::default().fg(self.theme.text)),
            Span::styled(format!("  ({})", self.image_path), Style::default().fg(self.theme.text_dim)),
        ]);
        buf.set_line(area.x, area.y, &line, area.width);
    }
}

/// User plan message (UserPlanMessage.tsx).
pub struct UserPlanMessageWidget<'a> {
    pub plan_content: &'a str,
    pub theme: &'a Theme,
}

impl<'a> UserPlanMessageWidget<'a> {
    pub fn new(plan_content: &'a str, theme: &'a Theme) -> Self {
        Self { plan_content, theme }
    }
}

impl<'a> Widget for UserPlanMessageWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let header = Line::from(vec![
            Span::styled("📋 ", Style::default().fg(self.theme.info)),
            Span::styled("Plan Mode", Style::default().fg(self.theme.info).add_modifier(Modifier::BOLD)),
        ]);
        buf.set_line(area.x, area.y, &header, area.width);

        if area.height > 1 {
            let content_area = Rect::new(area.x + 2, area.y + 1, area.width.saturating_sub(2), area.height - 1);
            let p = Paragraph::new(self.plan_content)
                .style(Style::default().fg(self.theme.text))
                .wrap(Wrap { trim: false });
            p.render(content_area, buf);
        }
    }
}

/// User channel message (UserChannelMessage.tsx).
pub struct UserChannelMessageWidget<'a> {
    pub channel: &'a str,
    pub content: &'a str,
    pub theme: &'a Theme,
}

impl<'a> UserChannelMessageWidget<'a> {
    pub fn new(channel: &'a str, content: &'a str, theme: &'a Theme) -> Self {
        Self { channel, content, theme }
    }
}

impl<'a> Widget for UserChannelMessageWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let line = Line::from(vec![
            Span::styled("#", Style::default().fg(self.theme.secondary)),
            Span::styled(self.channel, Style::default().fg(self.theme.secondary).add_modifier(Modifier::BOLD)),
            Span::styled(format!("  {}", self.content), Style::default().fg(self.theme.text)),
        ]);
        buf.set_line(area.x, area.y, &line, area.width);
    }
}

/// User memory input message (UserMemoryInputMessage.tsx).
pub struct UserMemoryInputMessageWidget<'a> {
    pub memory_key: &'a str,
    pub content: &'a str,
    pub theme: &'a Theme,
}

impl<'a> UserMemoryInputMessageWidget<'a> {
    pub fn new(memory_key: &'a str, content: &'a str, theme: &'a Theme) -> Self {
        Self { memory_key, content, theme }
    }
}

impl<'a> Widget for UserMemoryInputMessageWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let line = Line::from(vec![
            Span::styled("🧠 ", Style::default().fg(self.theme.secondary)),
            Span::styled("Memory: ", Style::default().fg(self.theme.text_dim)),
            Span::styled(self.memory_key, Style::default().fg(self.theme.secondary).add_modifier(Modifier::BOLD)),
        ]);
        buf.set_line(area.x, area.y, &line, area.width);

        if area.height > 1 {
            let content_area = Rect::new(area.x + 2, area.y + 1, area.width.saturating_sub(2), area.height - 1);
            let p = Paragraph::new(self.content)
                .style(Style::default().fg(self.theme.text_dim))
                .wrap(Wrap { trim: false });
            p.render(content_area, buf);
        }
    }
}

/// User resource update message (UserResourceUpdateMessage.tsx).
pub struct UserResourceUpdateMessageWidget<'a> {
    pub resource_path: &'a str,
    pub change_type: &'a str,
    pub theme: &'a Theme,
}

impl<'a> UserResourceUpdateMessageWidget<'a> {
    pub fn new(resource_path: &'a str, change_type: &'a str, theme: &'a Theme) -> Self {
        Self { resource_path, change_type, theme }
    }
}

impl<'a> Widget for UserResourceUpdateMessageWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let line = Line::from(vec![
            Span::styled("📄 ", Style::default().fg(self.theme.text_dim)),
            Span::styled(self.change_type, Style::default().fg(self.theme.info)),
            Span::styled(format!(": {}", self.resource_path), Style::default().fg(self.theme.text)),
        ]);
        buf.set_line(area.x, area.y, &line, area.width);
    }
}

/// User teammate message (UserTeammateMessage.tsx).
pub struct UserTeammateMessageWidget<'a> {
    pub teammate_name: &'a str,
    pub content: &'a str,
    pub color: Color,
    pub theme: &'a Theme,
}

impl<'a> UserTeammateMessageWidget<'a> {
    pub fn new(teammate_name: &'a str, content: &'a str, theme: &'a Theme) -> Self {
        Self { teammate_name, content, color: theme.secondary, theme }
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }
}

impl<'a> Widget for UserTeammateMessageWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let line = Line::from(vec![
            Span::styled("● ", Style::default().fg(self.color)),
            Span::styled(self.teammate_name, Style::default().fg(self.color).add_modifier(Modifier::BOLD)),
            Span::styled(": ", Style::default().fg(self.theme.text_dim)),
            Span::styled(self.content, Style::default().fg(self.theme.text)),
        ]);
        buf.set_line(area.x, area.y, &line, area.width);
    }
}

/// User local command output message (UserLocalCommandOutputMessage.tsx).
pub struct UserLocalCommandOutputMessageWidget<'a> {
    pub command: &'a str,
    pub output: &'a str,
    pub success: bool,
    pub theme: &'a Theme,
}

impl<'a> UserLocalCommandOutputMessageWidget<'a> {
    pub fn new(command: &'a str, output: &'a str, success: bool, theme: &'a Theme) -> Self {
        Self { command, output, success, theme }
    }
}

impl<'a> Widget for UserLocalCommandOutputMessageWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let status_color = if self.success { self.theme.success } else { self.theme.error };
        let status_icon = if self.success { "✓" } else { "✗" };
        let line = Line::from(vec![
            Span::styled(format!("{} ", status_icon), Style::default().fg(status_color)),
            Span::styled("$ ", Style::default().fg(self.theme.text_dim)),
            Span::styled(self.command, Style::default().fg(self.theme.text)),
        ]);
        buf.set_line(area.x, area.y, &line, area.width);

        if area.height > 1 && !self.output.is_empty() {
            let content_area = Rect::new(area.x + 2, area.y + 1, area.width.saturating_sub(2), area.height - 1);
            let p = Paragraph::new(self.output)
                .style(Style::default().fg(self.theme.text_dim))
                .wrap(Wrap { trim: false });
            p.render(content_area, buf);
        }
    }
}

/// User agent notification message (UserAgentNotificationMessage.tsx).
pub struct UserAgentNotificationMessageWidget<'a> {
    pub agent_name: &'a str,
    pub notification: &'a str,
    pub color: Color,
    pub theme: &'a Theme,
}

impl<'a> UserAgentNotificationMessageWidget<'a> {
    pub fn new(agent_name: &'a str, notification: &'a str, theme: &'a Theme) -> Self {
        Self { agent_name, notification, color: theme.secondary, theme }
    }
}

impl<'a> Widget for UserAgentNotificationMessageWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let line = Line::from(vec![
            Span::styled("● ", Style::default().fg(self.color)),
            Span::styled(self.agent_name, Style::default().fg(self.color).add_modifier(Modifier::BOLD)),
            Span::styled(": ", Style::default().fg(self.theme.text_dim)),
            Span::styled(self.notification, Style::default().fg(self.theme.text)),
        ]);
        buf.set_line(area.x, area.y, &line, area.width);
    }
}

// ===================================================================
// User tool result messages (UserToolResultMessage/)
// ===================================================================

/// Tool result status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolResultStatus {
    Success,
    Error,
    Rejected,
    Canceled,
}

/// User tool result message (UserToolResultMessage.tsx).
pub struct UserToolResultMessageWidget<'a> {
    pub tool_name: &'a str,
    pub status: ToolResultStatus,
    pub content: &'a str,
    pub theme: &'a Theme,
}

impl<'a> UserToolResultMessageWidget<'a> {
    pub fn new(tool_name: &'a str, status: ToolResultStatus, content: &'a str, theme: &'a Theme) -> Self {
        Self { tool_name, status, content, theme }
    }
}

impl<'a> Widget for UserToolResultMessageWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let (icon, color) = match self.status {
            ToolResultStatus::Success => ("✓", self.theme.success),
            ToolResultStatus::Error => ("✗", self.theme.error),
            ToolResultStatus::Rejected => ("⊘", self.theme.warning),
            ToolResultStatus::Canceled => ("⊘", self.theme.text_dim),
        };

        let line = Line::from(vec![
            Span::styled(format!("{} ", icon), Style::default().fg(color)),
            Span::styled(self.tool_name, Style::default().fg(self.theme.info).add_modifier(Modifier::BOLD)),
            Span::styled(
                match self.status {
                    ToolResultStatus::Success => " completed",
                    ToolResultStatus::Error => " failed",
                    ToolResultStatus::Rejected => " rejected",
                    ToolResultStatus::Canceled => " canceled",
                },
                Style::default().fg(color),
            ),
        ]);
        buf.set_line(area.x, area.y, &line, area.width);

        if area.height > 1 && !self.content.is_empty() {
            let content_area = Rect::new(area.x + 2, area.y + 1, area.width.saturating_sub(2), area.height - 1);
            let style = if self.status == ToolResultStatus::Error {
                Style::default().fg(self.theme.error)
            } else {
                Style::default().fg(self.theme.text_dim)
            };
            let p = Paragraph::new(self.content).style(style).wrap(Wrap { trim: false });
            p.render(content_area, buf);
        }
    }
}

/// Rejected plan message (RejectedPlanMessage.tsx).
pub struct RejectedPlanMessageWidget<'a> {
    pub reason: &'a str,
    pub theme: &'a Theme,
}

impl<'a> RejectedPlanMessageWidget<'a> {
    pub fn new(reason: &'a str, theme: &'a Theme) -> Self {
        Self { reason, theme }
    }
}

impl<'a> Widget for RejectedPlanMessageWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let line = Line::from(vec![
            Span::styled("⊘ ", Style::default().fg(self.theme.warning)),
            Span::styled("Plan rejected: ", Style::default().fg(self.theme.warning).add_modifier(Modifier::BOLD)),
            Span::styled(self.reason, Style::default().fg(self.theme.text)),
        ]);
        buf.set_line(area.x, area.y, &line, area.width);
    }
}

/// Rejected tool use message (RejectedToolUseMessage.tsx).
pub struct RejectedToolUseMessageWidget<'a> {
    pub tool_name: &'a str,
    pub reason: &'a str,
    pub theme: &'a Theme,
}

impl<'a> RejectedToolUseMessageWidget<'a> {
    pub fn new(tool_name: &'a str, reason: &'a str, theme: &'a Theme) -> Self {
        Self { tool_name, reason, theme }
    }
}

impl<'a> Widget for RejectedToolUseMessageWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let line = Line::from(vec![
            Span::styled("⊘ ", Style::default().fg(self.theme.warning)),
            Span::styled(self.tool_name, Style::default().fg(self.theme.info).add_modifier(Modifier::BOLD)),
            Span::styled(" rejected: ", Style::default().fg(self.theme.warning)),
            Span::styled(self.reason, Style::default().fg(self.theme.text)),
        ]);
        buf.set_line(area.x, area.y, &line, area.width);
    }
}

// ===================================================================
// Assistant message variants
// ===================================================================

/// Assistant text message (AssistantTextMessage.tsx).
pub struct AssistantTextMessageWidget<'a> {
    pub content: &'a str,
    pub is_streaming: bool,
    pub theme: &'a Theme,
}

impl<'a> AssistantTextMessageWidget<'a> {
    pub fn new(content: &'a str, theme: &'a Theme) -> Self {
        Self { content, is_streaming: false, theme }
    }

    pub fn streaming(mut self, v: bool) -> Self {
        self.is_streaming = v;
        self
    }
}

impl<'a> Widget for AssistantTextMessageWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 4 || area.height == 0 {
            return;
        }

        let prefix_style = Style::default().fg(self.theme.secondary);
        buf.set_string(area.x, area.y, "⏺", prefix_style);

        let content_area = Rect::new(
            area.x + 3,
            area.y,
            area.width.saturating_sub(3),
            area.height,
        );
        let style = Style::default().fg(self.theme.text);
        let p = Paragraph::new(self.content).style(style).wrap(Wrap { trim: false });
        p.render(content_area, buf);
    }
}

/// Assistant thinking message (AssistantThinkingMessage.tsx).
pub struct AssistantThinkingMessageWidget<'a> {
    pub thinking_content: &'a str,
    pub is_expanded: bool,
    pub duration_ms: Option<u64>,
    pub theme: &'a Theme,
}

impl<'a> AssistantThinkingMessageWidget<'a> {
    pub fn new(thinking_content: &'a str, theme: &'a Theme) -> Self {
        Self { thinking_content, is_expanded: false, duration_ms: None, theme }
    }

    pub fn expanded(mut self, v: bool) -> Self {
        self.is_expanded = v;
        self
    }

    pub fn duration_ms(mut self, ms: u64) -> Self {
        self.duration_ms = Some(ms);
        self
    }
}

impl<'a> Widget for AssistantThinkingMessageWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let duration_str = self.duration_ms.map(|ms| {
            if ms >= 1000 { format!("{:.1}s", ms as f64 / 1000.0) }
            else { format!("{}ms", ms) }
        });

        let mut header_spans = vec![
            Span::styled("💭 ", Style::default().fg(self.theme.secondary)),
            Span::styled("Reasoning", Style::default().fg(self.theme.secondary).add_modifier(Modifier::BOLD)),
        ];
        if let Some(ref dur) = duration_str {
            header_spans.push(Span::styled(format!("  ({})", dur), Style::default().fg(self.theme.text_dim)));
        }
        if !self.is_expanded {
            header_spans.push(Span::styled("  ▸", Style::default().fg(self.theme.text_dim)));
        }
        buf.set_line(area.x, area.y, &Line::from(header_spans), area.width);

        if self.is_expanded && area.height > 1 {
            let content_area = Rect::new(area.x + 2, area.y + 1, area.width.saturating_sub(2), area.height - 1);
            let p = Paragraph::new(self.thinking_content)
                .style(Style::default().fg(self.theme.text_dim))
                .wrap(Wrap { trim: false });
            p.render(content_area, buf);
        }
    }
}

/// Assistant redacted thinking message (AssistantRedactedThinkingMessage.tsx).
pub struct AssistantRedactedThinkingMessageWidget<'a> {
    pub theme: &'a Theme,
}

impl<'a> AssistantRedactedThinkingMessageWidget<'a> {
    pub fn new(theme: &'a Theme) -> Self {
        Self { theme }
    }
}

impl<'a> Widget for AssistantRedactedThinkingMessageWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let line = Line::from(vec![
            Span::styled("💭 ", Style::default().fg(self.theme.text_dim)),
            Span::styled("Reasoning (redacted)", Style::default().fg(self.theme.text_dim)),
        ]);
        buf.set_line(area.x, area.y, &line, area.width);
    }
}

/// Highlighted thinking text renderer (HighlightedThinkingText.tsx).
pub struct HighlightedThinkingTextWidget<'a> {
    pub text: &'a str,
    pub highlight_pattern: Option<&'a str>,
    pub theme: &'a Theme,
}

impl<'a> HighlightedThinkingTextWidget<'a> {
    pub fn new(text: &'a str, theme: &'a Theme) -> Self {
        Self { text, highlight_pattern: None, theme }
    }

    pub fn highlight(mut self, pattern: &'a str) -> Self {
        self.highlight_pattern = Some(pattern);
        self
    }
}

impl<'a> Widget for HighlightedThinkingTextWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        if let Some(pattern) = self.highlight_pattern {
            let parts: Vec<&str> = self.text.splitn(3, pattern).collect();
            if parts.len() >= 2 {
                let mut spans = Vec::new();
                spans.push(Span::styled(parts[0], Style::default().fg(self.theme.text_dim)));
                spans.push(Span::styled(
                    pattern,
                    Style::default().fg(self.theme.warning).add_modifier(Modifier::BOLD),
                ));
                if parts.len() > 2 {
                    spans.push(Span::styled(parts[2], Style::default().fg(self.theme.text_dim)));
                }
                buf.set_line(area.x, area.y, &Line::from(spans), area.width);
                return;
            }
        }

        let p = Paragraph::new(self.text)
            .style(Style::default().fg(self.theme.text_dim))
            .wrap(Wrap { trim: false });
        p.render(area, buf);
    }
}

/// Assistant tool use message (AssistantToolUseMessage.tsx).
pub struct AssistantToolUseMessageWidget<'a> {
    pub tool_name: &'a str,
    pub input_preview: &'a str,
    pub is_streaming: bool,
    pub theme: &'a Theme,
}

impl<'a> AssistantToolUseMessageWidget<'a> {
    pub fn new(tool_name: &'a str, input_preview: &'a str, theme: &'a Theme) -> Self {
        Self { tool_name, input_preview, is_streaming: false, theme }
    }

    pub fn streaming(mut self, v: bool) -> Self {
        self.is_streaming = v;
        self
    }
}

impl<'a> Widget for AssistantToolUseMessageWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let icon = if self.is_streaming { "⋯" } else { "⚡" };
        let line = Line::from(vec![
            Span::styled(format!("{} ", icon), Style::default().fg(self.theme.info)),
            Span::styled(self.tool_name, Style::default().fg(self.theme.info).add_modifier(Modifier::BOLD)),
        ]);
        buf.set_line(area.x, area.y, &line, area.width);

        if area.height > 1 && !self.input_preview.is_empty() {
            let content_area = Rect::new(area.x + 2, area.y + 1, area.width.saturating_sub(2), area.height - 1);
            let p = Paragraph::new(self.input_preview)
                .style(Style::default().fg(self.theme.text_dim))
                .wrap(Wrap { trim: false });
            p.render(content_area, buf);
        }
    }
}

// ===================================================================
// System/special messages
// ===================================================================

/// System text message (SystemTextMessage.tsx).
pub struct SystemTextMessageWidget<'a> {
    pub content: &'a str,
    pub theme: &'a Theme,
}

impl<'a> SystemTextMessageWidget<'a> {
    pub fn new(content: &'a str, theme: &'a Theme) -> Self {
        Self { content, theme }
    }
}

impl<'a> Widget for SystemTextMessageWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let line = Line::from(vec![
            Span::styled("ℹ ", Style::default().fg(self.theme.system_message_fg)),
            Span::styled(self.content, Style::default().fg(self.theme.system_message_fg)),
        ]);
        buf.set_line(area.x, area.y, &line, area.width);
    }
}

/// System API error message (SystemAPIErrorMessage.tsx).
pub struct SystemApiErrorMessageWidget<'a> {
    pub error_message: &'a str,
    pub status_code: Option<u16>,
    pub is_retryable: bool,
    pub theme: &'a Theme,
}

impl<'a> SystemApiErrorMessageWidget<'a> {
    pub fn new(error_message: &'a str, theme: &'a Theme) -> Self {
        Self { error_message, status_code: None, is_retryable: false, theme }
    }

    pub fn status_code(mut self, code: u16) -> Self {
        self.status_code = Some(code);
        self
    }

    pub fn retryable(mut self, v: bool) -> Self {
        self.is_retryable = v;
        self
    }
}

impl<'a> Widget for SystemApiErrorMessageWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let mut spans = vec![
            Span::styled("✗ ", Style::default().fg(self.theme.error)),
            Span::styled("API Error", Style::default().fg(self.theme.error).add_modifier(Modifier::BOLD)),
        ];
        if let Some(code) = self.status_code {
            spans.push(Span::styled(format!(" ({})", code), Style::default().fg(self.theme.error)));
        }
        if self.is_retryable {
            spans.push(Span::styled(" [retrying]", Style::default().fg(self.theme.warning)));
        }
        buf.set_line(area.x, area.y, &Line::from(spans), area.width);

        if area.height > 1 {
            let content_area = Rect::new(area.x + 2, area.y + 1, area.width.saturating_sub(2), area.height - 1);
            let p = Paragraph::new(self.error_message)
                .style(Style::default().fg(self.theme.error))
                .wrap(Wrap { trim: false });
            p.render(content_area, buf);
        }
    }
}

/// Rate limit message (RateLimitMessage.tsx).
pub struct RateLimitMessageWidget<'a> {
    pub wait_seconds: u64,
    pub request_type: &'a str,
    pub theme: &'a Theme,
}

impl<'a> RateLimitMessageWidget<'a> {
    pub fn new(wait_seconds: u64, request_type: &'a str, theme: &'a Theme) -> Self {
        Self { wait_seconds, request_type, theme }
    }
}

impl<'a> Widget for RateLimitMessageWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let line = Line::from(vec![
            Span::styled("⏳ ", Style::default().fg(self.theme.warning)),
            Span::styled("Rate limited", Style::default().fg(self.theme.warning).add_modifier(Modifier::BOLD)),
            Span::styled(
                format!(" — waiting {}s for {}", self.wait_seconds, self.request_type),
                Style::default().fg(self.theme.text_dim),
            ),
        ]);
        buf.set_line(area.x, area.y, &line, area.width);
    }
}

/// Shutdown message (ShutdownMessage.tsx).
pub struct ShutdownMessageWidget<'a> {
    pub reason: &'a str,
    pub theme: &'a Theme,
}

impl<'a> ShutdownMessageWidget<'a> {
    pub fn new(reason: &'a str, theme: &'a Theme) -> Self {
        Self { reason, theme }
    }
}

impl<'a> Widget for ShutdownMessageWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let line = Line::from(vec![
            Span::styled("⏻ ", Style::default().fg(self.theme.text_dim)),
            Span::styled("Session ended", Style::default().fg(self.theme.text_dim).add_modifier(Modifier::BOLD)),
            Span::styled(format!(": {}", self.reason), Style::default().fg(self.theme.text_dim)),
        ]);
        buf.set_line(area.x, area.y, &line, area.width);
    }
}

/// Attachment message (AttachmentMessage.tsx).
pub struct AttachmentMessageWidget<'a> {
    pub attachments: &'a [AttachmentKind],
    pub theme: &'a Theme,
}

impl<'a> AttachmentMessageWidget<'a> {
    pub fn new(attachments: &'a [AttachmentKind], theme: &'a Theme) -> Self {
        Self { attachments, theme }
    }
}

impl<'a> Widget for AttachmentMessageWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 || self.attachments.is_empty() {
            return;
        }
        let mut y = area.y;
        for attachment in self.attachments {
            if y >= area.y + area.height {
                break;
            }
            if !should_render_attachment(attachment) {
                continue;
            }
            let (icon, label) = match attachment {
                AttachmentKind::File { path, .. } => ("📄", path.as_str()),
                AttachmentKind::Image { url, alt } => ("📷", alt.as_deref().unwrap_or(url.as_str())),
                AttachmentKind::Url { href, title } => ("🔗", title.as_deref().unwrap_or(href.as_str())),
                AttachmentKind::Code { language, .. } => ("💻", language.as_str()),
            };
            let line = Line::from(vec![
                Span::styled(format!("{} ", icon), Style::default().fg(self.theme.text_dim)),
                Span::styled(label, Style::default().fg(self.theme.primary).add_modifier(Modifier::UNDERLINED)),
            ]);
            buf.set_line(area.x, y, &line, area.width);
            y += 1;
        }
    }
}

/// Hook progress message (HookProgressMessage.tsx).
pub struct HookProgressMessageWidget<'a> {
    pub hook_name: &'a str,
    pub status: &'a str,
    pub theme: &'a Theme,
}

impl<'a> HookProgressMessageWidget<'a> {
    pub fn new(hook_name: &'a str, status: &'a str, theme: &'a Theme) -> Self {
        Self { hook_name, status, theme }
    }
}

impl<'a> Widget for HookProgressMessageWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let line = Line::from(vec![
            Span::styled("⚙ ", Style::default().fg(self.theme.text_dim)),
            Span::styled(self.hook_name, Style::default().fg(self.theme.text)),
            Span::styled(format!(": {}", self.status), Style::default().fg(self.theme.text_dim)),
        ]);
        buf.set_line(area.x, area.y, &line, area.width);
    }
}

/// Plan approval message (PlanApprovalMessage.tsx).
pub struct PlanApprovalMessageWidget<'a> {
    pub plan_summary: &'a str,
    pub approved: bool,
    pub theme: &'a Theme,
}

impl<'a> PlanApprovalMessageWidget<'a> {
    pub fn new(plan_summary: &'a str, approved: bool, theme: &'a Theme) -> Self {
        Self { plan_summary, approved, theme }
    }
}

impl<'a> Widget for PlanApprovalMessageWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let (icon, status, color) = if self.approved {
            ("✓", "approved", self.theme.success)
        } else {
            ("✗", "rejected", self.theme.error)
        };
        let line = Line::from(vec![
            Span::styled(format!("{} ", icon), Style::default().fg(color)),
            Span::styled("Plan ", Style::default().fg(self.theme.text)),
            Span::styled(status, Style::default().fg(color).add_modifier(Modifier::BOLD)),
        ]);
        buf.set_line(area.x, area.y, &line, area.width);

        if area.height > 1 {
            let content_area = Rect::new(area.x + 2, area.y + 1, area.width.saturating_sub(2), area.height - 1);
            let p = Paragraph::new(self.plan_summary)
                .style(Style::default().fg(self.theme.text_dim))
                .wrap(Wrap { trim: false });
            p.render(content_area, buf);
        }
    }
}

/// Task assignment message (TaskAssignmentMessage.tsx).
pub struct TaskAssignmentMessageWidget<'a> {
    pub task_id: &'a str,
    pub agent_name: &'a str,
    pub subject: &'a str,
    pub theme: &'a Theme,
}

impl<'a> TaskAssignmentMessageWidget<'a> {
    pub fn new(task_id: &'a str, agent_name: &'a str, subject: &'a str, theme: &'a Theme) -> Self {
        Self { task_id, agent_name, subject, theme }
    }
}

impl<'a> Widget for TaskAssignmentMessageWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let line = Line::from(vec![
            Span::styled("📋 ", Style::default().fg(self.theme.info)),
            Span::styled(format!("#{} ", self.task_id), Style::default().fg(self.theme.text_dim)),
            Span::styled("→ ", Style::default().fg(self.theme.text_dim)),
            Span::styled(format!("@{}", self.agent_name), Style::default().fg(self.theme.secondary)),
            Span::styled(format!(": {}", self.subject), Style::default().fg(self.theme.text)),
        ]);
        buf.set_line(area.x, area.y, &line, area.width);
    }
}

/// Advisor message (AdvisorMessage.tsx).
pub struct AdvisorMessageWidget<'a> {
    pub content: &'a str,
    pub theme: &'a Theme,
}

impl<'a> AdvisorMessageWidget<'a> {
    pub fn new(content: &'a str, theme: &'a Theme) -> Self {
        Self { content, theme }
    }
}

impl<'a> Widget for AdvisorMessageWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let line = Line::from(vec![
            Span::styled("💡 ", Style::default().fg(self.theme.info)),
            Span::styled("Advisor: ", Style::default().fg(self.theme.info).add_modifier(Modifier::BOLD)),
            Span::styled(self.content, Style::default().fg(self.theme.text)),
        ]);
        buf.set_line(area.x, area.y, &line, area.width);
    }
}

/// Compact boundary message (CompactBoundaryMessage.tsx).
pub struct CompactBoundaryMessageWidget<'a> {
    pub label: &'a str,
    pub theme: &'a Theme,
}

impl<'a> CompactBoundaryMessageWidget<'a> {
    pub fn new(label: &'a str, theme: &'a Theme) -> Self {
        Self { label, theme }
    }
}

impl<'a> Widget for CompactBoundaryMessageWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let label_len = self.label.len() as u16;
        let left = (area.width.saturating_sub(label_len + 2)) / 2;
        let right = area.width.saturating_sub(left + label_len + 2);
        let line = Line::from(vec![
            Span::styled("─".repeat(left as usize), Style::default().fg(self.theme.text_dim)),
            Span::styled(format!(" {} ", self.label), Style::default().fg(self.theme.text_dim)),
            Span::styled("─".repeat(right as usize), Style::default().fg(self.theme.text_dim)),
        ]);
        buf.set_line(area.x, area.y, &line, area.width);
    }
}

/// Grouped tool use content (GroupedToolUseContent.tsx).
pub struct GroupedToolUseContentWidget<'a> {
    pub tool_name: &'a str,
    pub count: usize,
    pub is_expanded: bool,
    pub entries: &'a [(&'a str, &'a str)],
    pub theme: &'a Theme,
}

impl<'a> GroupedToolUseContentWidget<'a> {
    pub fn new(tool_name: &'a str, count: usize, entries: &'a [(&'a str, &'a str)], theme: &'a Theme) -> Self {
        Self { tool_name, count, is_expanded: false, entries, theme }
    }

    pub fn expanded(mut self, v: bool) -> Self {
        self.is_expanded = v;
        self
    }
}

impl<'a> Widget for GroupedToolUseContentWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let toggle = if self.is_expanded { "▾" } else { "▸" };
        let header = Line::from(vec![
            Span::styled(format!("{} ", toggle), Style::default().fg(self.theme.text_dim)),
            Span::styled(self.tool_name, Style::default().fg(self.theme.info).add_modifier(Modifier::BOLD)),
            Span::styled(format!(" (×{})", self.count), Style::default().fg(self.theme.text_dim)),
        ]);
        buf.set_line(area.x, area.y, &header, area.width);

        if self.is_expanded {
            for (i, (label, detail)) in self.entries.iter().enumerate() {
                let y = area.y + 1 + i as u16;
                if y >= area.y + area.height {
                    break;
                }
                let line = Line::from(vec![
                    Span::styled("  • ", Style::default().fg(self.theme.text_dim)),
                    Span::styled(*label, Style::default().fg(self.theme.text)),
                    Span::styled(format!("  {}", detail), Style::default().fg(self.theme.text_dim)),
                ]);
                buf.set_line(area.x, y, &line, area.width);
            }
        }
    }
}

/// Collapsed read/search content (CollapsedReadSearchContent.tsx).
pub struct CollapsedReadSearchContentWidget<'a> {
    pub label: &'a str,
    pub file_count: usize,
    pub is_expanded: bool,
    pub files: &'a [&'a str],
    pub theme: &'a Theme,
}

impl<'a> CollapsedReadSearchContentWidget<'a> {
    pub fn new(label: &'a str, file_count: usize, files: &'a [&'a str], theme: &'a Theme) -> Self {
        Self { label, file_count, is_expanded: false, files, theme }
    }

    pub fn expanded(mut self, v: bool) -> Self {
        self.is_expanded = v;
        self
    }
}

impl<'a> Widget for CollapsedReadSearchContentWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let toggle = if self.is_expanded { "▾" } else { "▸" };
        let header = Line::from(vec![
            Span::styled(format!("{} ", toggle), Style::default().fg(self.theme.text_dim)),
            Span::styled(self.label, Style::default().fg(self.theme.text)),
            Span::styled(format!(" ({} files)", self.file_count), Style::default().fg(self.theme.text_dim)),
        ]);
        buf.set_line(area.x, area.y, &header, area.width);

        if self.is_expanded {
            for (i, file) in self.files.iter().enumerate() {
                let y = area.y + 1 + i as u16;
                if y >= area.y + area.height {
                    break;
                }
                let line = Line::from(vec![
                    Span::styled("  📄 ", Style::default().fg(self.theme.text_dim)),
                    Span::styled(
                        *file,
                        Style::default().fg(self.theme.primary).add_modifier(Modifier::UNDERLINED),
                    ),
                ]);
                buf.set_line(area.x, y, &line, area.width);
            }
        }
    }
}

// ===================================================================
// messageActions.tsx — keyboard actions on selected messages
// ===================================================================

/// One actionable command available on a focused message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageActionKind {
    Copy,
    CopyAsMarkdown,
    Edit,
    Reply,
    Delete,
    Quote,
    OpenInIde,
    OpenLink,
}

/// Static list of all known message actions in their display order.
pub const MESSAGE_ACTIONS: &[MessageActionKind] = &[
    MessageActionKind::Copy,
    MessageActionKind::CopyAsMarkdown,
    MessageActionKind::Edit,
    MessageActionKind::Reply,
    MessageActionKind::Quote,
    MessageActionKind::Delete,
    MessageActionKind::OpenInIde,
    MessageActionKind::OpenLink,
];

/// Per-message capabilities — which actions are valid for this message.
#[derive(Debug, Clone, Copy, Default)]
pub struct MessageActionCaps {
    pub can_copy: bool,
    pub can_edit: bool,
    pub can_reply: bool,
    pub can_delete: bool,
    pub can_quote: bool,
    pub can_open_in_ide: bool,
    pub can_open_link: bool,
}

/// Container for what makes a message navigable in the action bar.
#[derive(Debug, Clone)]
pub struct NavigableOf {
    pub id: String,
    pub role: String,
    pub text: String,
}

/// Specifically navigable message — kept distinct from a tool/system marker.
#[derive(Debug, Clone)]
pub struct NavigableMessage {
    pub inner: NavigableOf,
    pub caps: MessageActionCaps,
}

/// Selected-message context: which message is selected and its caps.
#[derive(Debug, Clone, Default)]
pub struct MessageActionsSelectedContext {
    pub selected_id: Option<String>,
    pub caps: MessageActionCaps,
    pub focused: bool,
}

/// In-virtual-list flag — whether the parent is a VirtualMessageList.
#[derive(Debug, Clone, Copy, Default)]
pub struct InVirtualListContext {
    pub is_virtual: bool,
}

/// Copy textual representation of a message to a clipboard buffer (returned).
pub fn copy_text_of(msg: &NavigableOf, as_markdown: bool) -> String {
    if as_markdown {
        format!("**{}**: {}", msg.role, msg.text)
    } else {
        msg.text.clone()
    }
}

/// Background style for the currently-selected message.
pub fn use_selected_message_bg(theme: &Theme, focused: bool) -> Color {
    if focused {
        theme.selection
    } else {
        theme.surface
    }
}

/// MessageActions state machine — selection + dispatch.
#[derive(Debug, Clone, Default)]
pub struct MessageActions {
    pub context: MessageActionsSelectedContext,
    pub last_action: Option<MessageActionKind>,
}

impl MessageActions {
    pub fn dispatch(&mut self, action: MessageActionKind) {
        self.last_action = Some(action);
    }
}

/// Hook-equivalent for useMessageActions — returns a mutable handle.
pub fn use_message_actions(state: &mut MessageActions) -> &mut MessageActions {
    state
}

/// Keybindings → action mapping for message actions.
#[derive(Debug, Clone)]
pub struct MessageActionsKeybindings;

impl MessageActionsKeybindings {
    pub fn map(key: &str) -> Option<MessageActionKind> {
        match key {
            "c" => Some(MessageActionKind::Copy),
            "C" => Some(MessageActionKind::CopyAsMarkdown),
            "e" => Some(MessageActionKind::Edit),
            "r" => Some(MessageActionKind::Reply),
            "d" => Some(MessageActionKind::Delete),
            "q" => Some(MessageActionKind::Quote),
            "i" => Some(MessageActionKind::OpenInIde),
            "o" => Some(MessageActionKind::OpenLink),
            _ => None,
        }
    }
}

/// Bottom action bar — a row of action chips for the selected message.
#[derive(Debug, Clone, Default)]
pub struct MessageActionsBar {
    pub caps: MessageActionCaps,
    pub hover: Option<MessageActionKind>,
}

impl MessageActionsBar {
    pub fn visible_actions(&self) -> Vec<MessageActionKind> {
        MESSAGE_ACTIONS
            .iter()
            .copied()
            .filter(|a| match a {
                MessageActionKind::Copy => self.caps.can_copy,
                MessageActionKind::CopyAsMarkdown => self.caps.can_copy,
                MessageActionKind::Edit => self.caps.can_edit,
                MessageActionKind::Reply => self.caps.can_reply,
                MessageActionKind::Delete => self.caps.can_delete,
                MessageActionKind::Quote => self.caps.can_quote,
                MessageActionKind::OpenInIde => self.caps.can_open_in_ide,
                MessageActionKind::OpenLink => self.caps.can_open_link,
            })
            .collect()
    }
}

// ===================================================================
// MessageSelector.tsx — filter helpers
// ===================================================================

/// Filter for user messages that can be re-selected by ↑.
pub fn selectable_user_messages_filter(role: &str, is_synthetic: bool) -> bool {
    role == "user" && !is_synthetic
}

/// Whether messages after the given index are only synthetic noise.
pub fn messages_after_are_only_synthetic(
    roles: &[(&str, bool)],
    after_idx: usize,
) -> bool {
    roles
        .iter()
        .skip(after_idx + 1)
        .all(|(_, is_synth)| *is_synth)
}

/// MessageSelector — circular cursor over selectable messages.
#[derive(Debug, Clone, Default)]
pub struct MessageSelector {
    pub message_ids: Vec<String>,
    pub selected_index: Option<usize>,
}

impl MessageSelector {
    pub fn new(message_ids: Vec<String>) -> Self {
        Self {
            message_ids,
            selected_index: None,
        }
    }
    pub fn prev(&mut self) {
        if self.message_ids.is_empty() {
            return;
        }
        self.selected_index = Some(match self.selected_index {
            None => self.message_ids.len() - 1,
            Some(0) => self.message_ids.len() - 1,
            Some(i) => i - 1,
        });
    }
    pub fn next(&mut self) {
        if self.message_ids.is_empty() {
            return;
        }
        self.selected_index = Some(match self.selected_index {
            None => 0,
            Some(i) if i + 1 >= self.message_ids.len() => 0,
            Some(i) => i + 1,
        });
    }
}

// ===================================================================
// Message.tsx / MessageRow.tsx — props equality memoization
// ===================================================================

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MessageProps {
    pub id: String,
    pub role: String,
    pub text_hash: u64,
    pub is_streaming: bool,
    pub is_selected: bool,
    pub theme_name: String,
}

pub fn are_message_props_equal(a: &MessageProps, b: &MessageProps) -> bool {
    a == b
}

/// TS Message.tsx exports `type Props = { ... }`. This module-level alias keeps
/// parity with the TS surface area (`pub type Props = MessageProps;` lives in a
/// sub-module so it can co-exist with `MessageRow`'s `Props` alias below).
pub mod message_props_alias {
    pub type Props = super::MessageProps;
}

#[derive(Debug, Clone, Default)]
pub struct Message {
    pub props: MessageProps,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MessageRowProps {
    pub id: String,
    pub width: u16,
    pub is_streaming: bool,
    pub tools_all_resolved: bool,
    pub theme_name: String,
}

pub fn are_message_row_props_equal(a: &MessageRowProps, b: &MessageRowProps) -> bool {
    a == b
}

/// TS MessageRow.tsx exports `type Props = { ... }`. Mirrored alias.
pub mod message_row_props_alias {
    pub type Props = super::MessageRowProps;
}

#[derive(Debug, Clone, Default)]
pub struct MessageRow {
    pub props: Option<MessageRowProps>,
}

pub fn is_message_streaming(msg_id: &str, currently_streaming: Option<&str>) -> bool {
    currently_streaming.map(|s| s == msg_id).unwrap_or(false)
}

pub fn all_tools_resolved(tool_ids: &[&str], resolved_ids: &[&str]) -> bool {
    tool_ids.iter().all(|id| resolved_ids.contains(id))
}

// ===================================================================
// Messages.tsx — global filters & slicing
// ===================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SliceAnchor {
    Tail,
    Cursor,
    MessageId,
}

pub fn filter_for_brief_tool(role: &str, is_brief_capable: bool) -> bool {
    role == "tool_use" || role == "tool_result" || (role == "assistant" && is_brief_capable)
}

pub fn drop_text_in_brief_turns(roles: &[&str], briefs: &[bool]) -> Vec<bool> {
    roles
        .iter()
        .zip(briefs.iter())
        .map(|(r, b)| !(*b && *r == "assistant"))
        .collect()
}

pub fn compute_slice_start(
    total: usize,
    viewport_rows: usize,
    anchor: SliceAnchor,
    cursor: Option<usize>,
) -> usize {
    match anchor {
        SliceAnchor::Tail => total.saturating_sub(viewport_rows),
        SliceAnchor::Cursor => {
            let c = cursor.unwrap_or(total.saturating_sub(1));
            c.saturating_sub(viewport_rows / 2)
        }
        SliceAnchor::MessageId => cursor.unwrap_or(0),
    }
}

#[derive(Debug, Clone, Default)]
pub struct Messages {
    pub anchor: Option<SliceAnchor>,
    pub cursor: Option<usize>,
    pub viewport_rows: usize,
}

impl Messages {
    pub fn slice_start(&self, total: usize) -> usize {
        compute_slice_start(
            total,
            self.viewport_rows,
            self.anchor.unwrap_or(SliceAnchor::Tail),
            self.cursor,
        )
    }
}

// ===================================================================
// Individual message-type widgets (state structs + lines())
// ===================================================================

#[derive(Debug, Clone, Default)]
pub struct UserPromptMessage { pub text: String }
#[derive(Debug, Clone, Default)]
pub struct AssistantRedactedThinkingMessage { pub redacted_count: usize }
#[derive(Debug, Clone, Default)]
pub struct HookProgressMessage { pub hook_name: String, pub label: String, pub progress: f32 }
#[derive(Debug, Clone, Default)]
pub struct UserLocalCommandOutputMessage { pub command: String, pub output: String, pub exit_code: i32 }
#[derive(Debug, Clone, Default)]
pub struct UserPlanMessage { pub plan: String }
#[derive(Debug, Clone, Default)]
pub struct UserTeammateMessage { pub teammate: String, pub text: String }
#[derive(Debug, Clone, Default)]
pub struct SystemTextMessage { pub text: String }
#[derive(Debug, Clone, Default)]
pub struct CollapsedReadSearchContent { pub search_term: String, pub match_count: usize, pub is_expanded: bool }
#[derive(Debug, Clone, Default)]
pub struct UserTextMessage { pub text: String }
#[derive(Debug, Clone, Default)]
pub struct UserBashInputMessage { pub command: String }
#[derive(Debug, Clone, Default)]
pub struct UserBashOutputMessage { pub output: String, pub exit_code: i32, pub truncated: bool }
#[derive(Debug, Clone, Default)]
pub struct UserResourceUpdateMessage { pub resource_uri: String, pub summary: String }
#[derive(Debug, Clone, Default)]
pub struct SystemAPIErrorMessage { pub error: String, pub request_id: Option<String> }
#[derive(Debug, Clone, Default)]
pub struct AssistantToolUseMessage { pub tool_name: String, pub input_summary: String }
#[derive(Debug, Clone, Default)]
pub struct CompactBoundaryMessage { pub before_count: usize, pub after_count: usize, pub savings_pct: u32 }
#[derive(Debug, Clone, Default)]
pub struct AssistantThinkingMessage { pub thinking: String, pub is_expanded: bool }
#[derive(Debug, Clone, Default)]
pub struct GroupedToolUseContent { pub tool_uses: Vec<String> }
#[derive(Debug, Clone, Default)]
pub struct AssistantTextMessage { pub text: String }
#[derive(Debug, Clone, Default)]
pub struct HighlightedThinkingText { pub fragments: Vec<String> }
#[derive(Debug, Clone, Default)]
pub struct AttachmentMessage { pub name: String, pub kind: String }
#[derive(Debug, Clone, Default)]
pub struct AdvisorMessage { pub advice: String }
#[derive(Debug, Clone, Default)]
pub struct UserCommandMessage { pub command: String, pub args: Vec<String> }
#[derive(Debug, Clone, Default)]
pub struct UserChannelMessage { pub channel: String, pub payload: String }
#[derive(Debug, Clone, Default)]
pub struct UserImageMessage { pub image_url: String, pub alt: String }
#[derive(Debug, Clone, Default)]
pub struct UserMemoryInputMessage { pub memory_key: String, pub text: String }
#[derive(Debug, Clone, Default)]
pub struct UserAgentNotificationMessage { pub agent: String, pub text: String }

pub fn format_teammate_message_content(teammate: &str, body: &str) -> String {
    format!("[from {}] {}", teammate, body)
}

pub fn teammate_message_content(teammate: &str, body: &str) -> Vec<String> {
    vec![format_teammate_message_content(teammate, body)]
}

// ===================================================================
// ShutdownMessage.tsx
// ===================================================================

#[derive(Debug, Clone, Default)]
pub struct ShutdownRequestDisplay {
    pub reason: String,
}

#[derive(Debug, Clone, Default)]
pub struct ShutdownRejectedDisplay {
    pub reason: String,
    pub blocker: String,
}

pub fn try_render_shutdown_message(
    role: &str,
    kind: &str,
    reason: &str,
) -> Option<ShutdownRequestDisplay> {
    if role == "system" && kind == "shutdown" {
        Some(ShutdownRequestDisplay {
            reason: reason.to_string(),
        })
    } else {
        None
    }
}

pub fn get_shutdown_message_summary(reason: &str) -> String {
    if reason.is_empty() {
        "shutdown".to_string()
    } else {
        format!("shutdown: {}", reason)
    }
}

// ===================================================================
// PlanApprovalMessage.tsx
// ===================================================================

#[derive(Debug, Clone, Default)]
pub struct PlanApprovalRequestDisplay {
    pub plan_text: String,
    pub approver: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct PlanApprovalResponseDisplay {
    pub approved: bool,
    pub feedback: String,
}

pub fn try_render_plan_approval_message(
    role: &str,
    kind: &str,
    plan_text: &str,
) -> Option<PlanApprovalRequestDisplay> {
    if role == "user" && kind == "plan_approval_request" {
        Some(PlanApprovalRequestDisplay {
            plan_text: plan_text.to_string(),
            approver: None,
        })
    } else {
        None
    }
}

// ===================================================================
// RateLimitMessage.tsx
// ===================================================================

#[derive(Debug, Clone, Default)]
pub struct RateLimitMessage {
    pub used_pct: u32,
    pub reset_seconds: u64,
    pub tier: String,
}

pub fn get_upsell_message(used_pct: u32, tier: &str) -> String {
    if tier == "free" {
        "Upgrade to Pro for higher limits and faster responses.".to_string()
    } else if used_pct >= 95 {
        "Approaching your monthly limit — consider upgrading.".to_string()
    } else if used_pct >= 80 {
        "Heavy usage detected.".to_string()
    } else {
        String::new()
    }
}

// ===================================================================
// teamMemSaved.ts / teamMemCollapsed.tsx
// ===================================================================

pub fn team_mem_saved_part(entry: &TeamMemSavedEntry) -> String {
    if let Some(by) = &entry.saved_by {
        format!("[memory] {} = {} (by {})", entry.key, entry.value, by)
    } else {
        format!("[memory] {} = {}", entry.key, entry.value)
    }
}

pub fn check_has_team_mem_ops(entries: &[TeamMemSavedEntry]) -> bool {
    !entries.is_empty()
}

pub fn team_mem_count_parts(n: usize) -> String {
    if n == 1 {
        "1 team memory saved".to_string()
    } else {
        format!("{} team memories saved", n)
    }
}

// ===================================================================
// nullRenderingAttachments.ts
// ===================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NullRenderingAttachmentType {
    Hidden,
    Synthetic,
    NoOp,
    SystemNotice,
}

pub fn is_null_rendering_attachment(kind: NullRenderingAttachmentType) -> bool {
    matches!(
        kind,
        NullRenderingAttachmentType::Hidden
            | NullRenderingAttachmentType::Synthetic
            | NullRenderingAttachmentType::NoOp
    )
}

// ===================================================================
// TaskAssignmentMessage.tsx
// ===================================================================

#[derive(Debug, Clone, Default)]
pub struct TaskAssignmentDisplay {
    pub task_id: String,
    pub assignee: String,
    pub summary: String,
}

pub fn try_render_task_assignment_message(
    role: &str,
    kind: &str,
    task_id: &str,
    assignee: &str,
    summary: &str,
) -> Option<TaskAssignmentDisplay> {
    if role == "system" && kind == "task_assignment" {
        Some(TaskAssignmentDisplay {
            task_id: task_id.to_string(),
            assignee: assignee.to_string(),
            summary: summary.to_string(),
        })
    } else {
        None
    }
}

pub fn get_task_assignment_summary(task_id: &str, assignee: &str) -> String {
    format!("→ {} → {}", task_id, assignee)
}

// ===================================================================
// UserToolResultMessage/* — tool result variants
// ===================================================================

#[derive(Debug, Clone, Default)]
pub struct UserToolResultMessage { pub tool_name: String, pub content: String, pub succeeded: bool }
#[derive(Debug, Clone, Default)]
pub struct UserToolSuccessMessage { pub tool_name: String, pub summary: String }
#[derive(Debug, Clone, Default)]
pub struct UserToolRejectMessage { pub tool_name: String, pub reason: String }
#[derive(Debug, Clone, Default)]
pub struct RejectedPlanMessage { pub plan_text: String, pub reason: String }
#[derive(Debug, Clone, Default)]
pub struct UserToolErrorMessage { pub tool_name: String, pub error: String }
#[derive(Debug, Clone, Default)]
pub struct RejectedToolUseMessage { pub tool_name: String, pub reason: String }
#[derive(Debug, Clone, Default)]
pub struct UserToolCanceledMessage { pub tool_name: String }

pub fn use_get_tool_from_messages<'a>(
    messages: &'a [(String, String, String)],
    target_tool_use_id: &str,
) -> Option<&'a (String, String, String)> {
    messages
        .iter()
        .find(|(_, role, id)| role == "tool_use" && id == target_tool_use_id)
}

// ===================================================================
// StructuredDiff/colorDiff.ts — syntax-aware diff highlighting
// ===================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorModuleUnavailableReason {
    NotInstalled,
    LoadFailed,
    DisabledByEnv,
    UnsupportedLanguage,
}

pub fn get_color_module_unavailable_reason(
    has_module: bool,
    env_disabled: bool,
    language: Option<&str>,
) -> Option<ColorModuleUnavailableReason> {
    if env_disabled {
        return Some(ColorModuleUnavailableReason::DisabledByEnv);
    }
    if !has_module {
        return Some(ColorModuleUnavailableReason::NotInstalled);
    }
    if language.map(|l| l.is_empty()).unwrap_or(true) {
        return Some(ColorModuleUnavailableReason::UnsupportedLanguage);
    }
    None
}

pub fn expect_color_diff(left: &str, right: &str) -> bool {
    left == right
}

pub fn expect_color_file(left: &[String], right: &[String]) -> bool {
    left == right
}

pub fn get_syntax_theme(theme_name: &str) -> &'static str {
    match theme_name {
        "dark" | "dark-high-contrast" => "github-dark",
        "light" | "light-high-contrast" => "github-light",
        _ => "github-dark",
    }
}

// ===================================================================
// StructuredDiff/Fallback.tsx — fallback diff renderer
// ===================================================================

#[derive(Debug, Clone)]
pub struct LineObject {
    pub kind: &'static str,
    pub text: String,
    pub line_no_old: Option<u32>,
    pub line_no_new: Option<u32>,
}

pub fn transform_lines_to_objects(raw_lines: &[&str]) -> Vec<LineObject> {
    let mut out = Vec::with_capacity(raw_lines.len());
    let mut old_no: u32 = 0;
    let mut new_no: u32 = 0;
    for raw in raw_lines {
        let s = *raw;
        if s.starts_with("@@") {
            out.push(LineObject {
                kind: "header",
                text: s.to_string(),
                line_no_old: None,
                line_no_new: None,
            });
        } else if let Some(rest) = s.strip_prefix('+') {
            new_no += 1;
            out.push(LineObject {
                kind: "add",
                text: rest.to_string(),
                line_no_old: None,
                line_no_new: Some(new_no),
            });
        } else if let Some(rest) = s.strip_prefix('-') {
            old_no += 1;
            out.push(LineObject {
                kind: "remove",
                text: rest.to_string(),
                line_no_old: Some(old_no),
                line_no_new: None,
            });
        } else {
            old_no += 1;
            new_no += 1;
            out.push(LineObject {
                kind: "context",
                text: s.to_string(),
                line_no_old: Some(old_no),
                line_no_new: Some(new_no),
            });
        }
    }
    out
}

pub fn process_adjacent_lines(
    lines: &[LineObject],
) -> Vec<(Option<LineObject>, Option<LineObject>)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].kind == "remove" && i + 1 < lines.len() && lines[i + 1].kind == "add" {
            out.push((Some(lines[i].clone()), Some(lines[i + 1].clone())));
            i += 2;
        } else {
            let l = lines[i].clone();
            match l.kind {
                "add" => out.push((None, Some(l))),
                "remove" => out.push((Some(l), None)),
                _ => out.push((Some(l.clone()), Some(l))),
            }
            i += 1;
        }
    }
    out
}

pub fn calculate_word_diffs(old: &str, new: &str) -> Vec<(usize, usize, &'static str)> {
    let old_words: Vec<&str> = old.split_whitespace().collect();
    let new_words: Vec<&str> = new.split_whitespace().collect();
    let mut out = Vec::new();
    let n = old_words.len().max(new_words.len());
    for i in 0..n {
        let same = old_words.get(i) == new_words.get(i);
        if !same {
            out.push((i, i + 1, "diff"));
        }
    }
    out
}

pub fn number_diff_lines(lines: &mut [LineObject]) {
    let mut old_no: u32 = 0;
    let mut new_no: u32 = 0;
    for l in lines.iter_mut() {
        match l.kind {
            "add" => {
                new_no += 1;
                l.line_no_new = Some(new_no);
                l.line_no_old = None;
            }
            "remove" => {
                old_no += 1;
                l.line_no_old = Some(old_no);
                l.line_no_new = None;
            }
            "context" => {
                old_no += 1;
                new_no += 1;
                l.line_no_old = Some(old_no);
                l.line_no_new = Some(new_no);
            }
            _ => {}
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct StructuredDiffFallback {
    pub raw_lines: Vec<String>,
    pub objects: Vec<LineObject>,
}

impl StructuredDiffFallback {
    pub fn new(raw_lines: Vec<String>) -> Self {
        let refs: Vec<&str> = raw_lines.iter().map(|s| s.as_str()).collect();
        let objects = transform_lines_to_objects(&refs);
        Self {
            raw_lines,
            objects,
        }
    }
}
