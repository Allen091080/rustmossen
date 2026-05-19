//! Message widget — renders a single message entry.
//!
//! Translates Message.tsx (626 lines) — dispatches to the appropriate
//! sub-widget based on message type (user, assistant, system, etc.)

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget, Wrap},
};

use crate::theme::Theme;

/// Message role/type discriminant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageType {
    User,
    Assistant,
    System,
    Progress,
    Attachment,
    ToolUse,
    ToolResult,
}

/// A renderable message entry.
#[derive(Debug, Clone)]
pub struct MessageData {
    pub message_type: MessageType,
    pub content: String,
    pub timestamp: Option<String>,
    pub is_streaming: bool,
    pub tool_name: Option<String>,
    pub is_error: bool,
    /// Reasoning/`<think>` content peeled out of the model's streamed text.
    /// Rendered above `content` in a dim italic block so users can watch the
    /// model think token-by-token while the final answer assembles below.
    /// `None` when the model never emitted a `<think>` segment.
    pub thinking: Option<String>,
    /// `Instant` at which the message stream finished. Used to drive the
    /// 30-second auto-fade of the `thinking` block — once the model is
    /// done reasoning, the dim italic preview hangs around for half a
    /// minute so the user can still read it, then disappears to declutter
    /// the scrollback. `None` until streaming completes for this entry.
    pub thinking_completed_at: Option<std::time::Instant>,
    /// Full untruncated tool output. `None` when this row is not a
    /// ToolResult, or when the preview already is the full content.
    /// When `Some` and the row is `expanded`, the renderer swaps
    /// `content` for this string so the user sees the entire output.
    pub full_content: Option<String>,
    /// User-controlled expand state for ToolResult rows. Toggled by
    /// pressing → / Enter while the row is focused. Has no effect when
    /// `full_content` is None.
    pub expanded: bool,
}

/// Widget to render a single message.
pub struct MessageWidget<'a> {
    pub data: &'a MessageData,
    pub theme: &'a Theme,
    pub width: u16,
    pub show_timestamp: bool,
    /// When `true`, thinking blocks render regardless of the 30s fade
    /// timer. Driven by App.show_all_thinking (Ctrl+E toggle).
    pub show_all_thinking: bool,
    /// When `true`, suppress the in-body tool-name header. Set by
    /// `MessageRowWidget` once it has drawn a containing box whose
    /// title already carries the tool name.
    pub suppress_tool_header: bool,
    pub add_margin: bool,
}

impl<'a> MessageWidget<'a> {
    pub fn new(data: &'a MessageData, theme: &'a Theme) -> Self {
        Self {
            data,
            theme,
            width: 80,
            show_timestamp: false,
            add_margin: true,
            show_all_thinking: false,
            suppress_tool_header: false,
        }
    }

    /// Forwarded by `MessagesWidget` so the App's Ctrl+E toggle pins
    /// thinking blocks against the 30s fade.
    pub fn show_all_thinking(mut self, on: bool) -> Self {
        self.show_all_thinking = on;
        self
    }

    pub fn suppress_tool_header(mut self, on: bool) -> Self {
        self.suppress_tool_header = on;
        self
    }

    pub fn width(mut self, width: u16) -> Self {
        self.width = width;
        self
    }

    pub fn show_timestamp(mut self, show: bool) -> Self {
        self.show_timestamp = show;
        self
    }

    pub fn add_margin(mut self, add: bool) -> Self {
        self.add_margin = add;
        self
    }

    /// Get the style for the message prefix/indicator.
    fn prefix_style(&self) -> (Style, &'static str) {
        match self.data.message_type {
            MessageType::User => (
                Style::default()
                    .fg(self.theme.primary)
                    .add_modifier(Modifier::BOLD),
                "❯",
            ),
            // ✻ (TEARDROP_ASTERISK) is the TS REPL's canonical assistant
            // marker — themed `success` so it stays legible across both
            // dark and light palettes and tracks `/theme` changes.
            MessageType::Assistant => (
                Style::default()
                    .fg(self.theme.success)
                    .add_modifier(Modifier::BOLD),
                "✻",
            ),
            MessageType::System => (Style::default().fg(self.theme.system_message_fg), "ℹ"),
            MessageType::Progress => (Style::default().fg(self.theme.text_dim), "⋯"),
            MessageType::ToolUse => (Style::default().fg(self.theme.info), "⚡"),
            MessageType::ToolResult => (Style::default().fg(self.theme.success), "✓"),
            MessageType::Attachment => (Style::default().fg(self.theme.text_dim), "📎"),
        }
    }

    /// Get content style.
    fn content_style(&self) -> Style {
        if self.data.is_error {
            return self.theme.style_error();
        }
        match self.data.message_type {
            MessageType::User => Style::default().fg(self.theme.text),
            MessageType::Assistant => Style::default().fg(self.theme.text),
            MessageType::System => Style::default().fg(self.theme.system_message_fg),
            MessageType::Progress => Style::default().fg(self.theme.text_dim),
            MessageType::ToolUse => Style::default().fg(self.theme.text),
            MessageType::ToolResult => Style::default().fg(self.theme.text),
            MessageType::Attachment => Style::default().fg(self.theme.text_dim),
        }
    }
}

impl<'a> Widget for MessageWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 4 || area.height == 0 {
            return;
        }

        let (prefix_style, prefix_char) = self.prefix_style();
        let content_style = self.content_style();

        // Layout: [prefix 2 cols] [content rest]
        let prefix_width = 2u16;
        let content_area = Rect::new(
            area.x + prefix_width + 1,
            area.y,
            area.width.saturating_sub(prefix_width + 1),
            area.height,
        );

        // Render prefix
        buf.set_string(area.x, area.y, prefix_char, prefix_style);

        // ── Reasoning / thinking block ──────────────────────────────────
        // When the assistant message carries `<think>` content (peeled out
        // of the streamed text upstream) render it above the answer in a
        // dim italic style with a 💭 prefix on the first line. The block
        // wraps inside the content column so long reasoning fits the row.
        let mut content_y = content_area.y;
        let mut content_height = content_area.height;
        // 30-second auto-fade for thinking: once streaming has finished
        // for this message, hide the reasoning block after 30s unless
        // the user pressed Ctrl+E to pin it visible. While streaming
        // (thinking_completed_at = None) we always show it.
        let fade_thinking = if self.show_all_thinking {
            false
        } else if let Some(t) = self.data.thinking_completed_at {
            t.elapsed() >= std::time::Duration::from_secs(30)
        } else {
            false
        };
        if let Some(ref thinking) = self.data.thinking {
            if !thinking.is_empty() && content_height > 0 && !fade_thinking {
                // While still streaming (no completed_at timestamp) we
                // render thinking with a moving shimmer band so the user
                // sees the model is "alive". Once streaming is done the
                // block falls back to a static dim italic style — the
                // 30-second fade timer then takes over.
                let still_streaming = self.data.thinking_completed_at.is_none();
                let static_style = Style::default()
                    .fg(self.theme.text_dim)
                    .add_modifier(Modifier::ITALIC);
                let think_body = format!("💭 {}", thinking);
                let think_lines = wrapped_line_count(&think_body, content_area.width);
                let take = think_lines.min(content_height);
                let think_area =
                    Rect::new(content_area.x, content_y, content_area.width, take);
                if still_streaming {
                    // Per-char styling: a 4-character bright window
                    // sweeps across the body. We collapse `\n` into the
                    // span list as line breaks; ratatui wraps on hard
                    // breaks even inside a Line collection.
                    use std::time::SystemTime;
                    let now_ms = SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .map(|d| d.as_millis())
                        .unwrap_or(0) as f64;
                    let total_chars = think_body.chars().count().max(1) as i64;
                    // Window completes a sweep every ~5 seconds.
                    let offset = ((now_ms / 50.0) as i64) % (total_chars + 12);
                    let mut shimmer_lines: Vec<ratatui::text::Line> = Vec::new();
                    let mut spans: Vec<ratatui::text::Span> = Vec::new();
                    for (i, ch) in think_body.chars().enumerate() {
                        if ch == '\n' {
                            shimmer_lines.push(ratatui::text::Line::from(std::mem::take(&mut spans)));
                            continue;
                        }
                        let dist = (offset - i as i64).abs();
                        let style = if dist <= 3 {
                            let t = 1.0 - (dist as f32) / 4.0;
                            let g = (170.0 + 70.0 * t) as u8;
                            let b = (170.0 + 70.0 * t) as u8;
                            Style::default()
                                .fg(ratatui::style::Color::Rgb(150, g, b))
                                .add_modifier(Modifier::ITALIC | Modifier::BOLD)
                        } else {
                            static_style
                        };
                        spans.push(ratatui::text::Span::styled(ch.to_string(), style));
                    }
                    if !spans.is_empty() {
                        shimmer_lines.push(ratatui::text::Line::from(spans));
                    }
                    Paragraph::new(shimmer_lines)
                        .wrap(Wrap { trim: false })
                        .render(think_area, buf);
                } else {
                    Paragraph::new(think_body.as_str())
                        .style(static_style)
                        .wrap(Wrap { trim: false })
                        .render(think_area, buf);
                }
                content_y = content_y.saturating_add(take);
                content_height = content_height.saturating_sub(take);
                // Spacer row between thinking and answer when there's room.
                if content_height > 1 && !self.data.content.is_empty() {
                    content_y = content_y.saturating_add(1);
                    content_height = content_height.saturating_sub(1);
                }
            }
        }

        // ToolResult bodies are often raw JSON strings where `\n` and
        // `\t` appear as literal backslash-n / backslash-t escapes —
        // Paragraph::wrap then sees a single never-breaking line that
        // overruns the area. Unescape them here so wrap can do its job.
        // Cheap to do per-render; the alternative is to mutate the
        // upstream content which would lose the original for hash/log.
        let rendered_content: String =
            if matches!(self.data.message_type, MessageType::ToolResult) {
                unescape_for_display(&self.data.content)
            } else {
                self.data.content.clone()
            };
        let rendered_body: &str = &rendered_content;

        // Render tool name if present and not suppressed by parent.
        let show_tool_header = !self.suppress_tool_header;
        if show_tool_header {
        if let Some(ref tool_name) = self.data.tool_name {
            let tool_style = Style::default()
                .fg(self.theme.info)
                .add_modifier(Modifier::BOLD);
            let line = Line::from(vec![
                Span::styled(tool_name.clone(), tool_style),
                Span::raw(" "),
            ]);
            let p = Paragraph::new(line);
            p.render(
                Rect::new(content_area.x, content_y, content_area.width, 1),
                buf,
            );
            // Content starts on next line
            if content_height > 1 {
                let text_area = Rect::new(
                    content_area.x,
                    content_y + 1,
                    content_area.width,
                    content_height - 1,
                );
                self.render_body(rendered_body, content_style, text_area, buf);
            }
        } else if content_height > 0 {
            let text_area =
                Rect::new(content_area.x, content_y, content_area.width, content_height);
            self.render_body(rendered_body, content_style, text_area, buf);
        }
        } else if content_height > 0 {
            // Header suppressed — paint the body straight into the
            // content area (the box title above already carries the name).
            let text_area =
                Rect::new(content_area.x, content_y, content_area.width, content_height);
            self.render_body(rendered_body, content_style, text_area, buf);
        }
    }
}

impl<'a> MessageWidget<'a> {
    /// Body renderer — picks the right path for each message kind:
    ///   * Assistant → pulldown-cmark + syntect via MarkdownWidget so
    ///     code fences, lists, headings, bold/italic etc. actually
    ///     render as prose instead of literal `**bold**`.
    ///   * ToolResult for Edit/Write/NotebookEdit → diff highlighter:
    ///     `+ ` lines green, `- ` lines red, `@@` lines blue.
    ///   * Everything else → plain Paragraph with the row's content_style.
    fn render_body(
        &self,
        text: &str,
        content_style: Style,
        text_area: Rect,
        buf: &mut Buffer,
    ) {
        if matches!(self.data.message_type, MessageType::Assistant) {
            let widget = crate::widgets::markdown::MarkdownWidget::new(text)
                .base_style(content_style);
            let lines = widget.parse_to_lines();
            Paragraph::new(lines)
                .style(content_style)
                .wrap(Wrap { trim: false })
                .render(text_area, buf);
            return;
        }

        // Diff-style colouring for Edit/Write/NotebookEdit tool results.
        // The Bash tool's output never starts a line with `+ ` so we
        // gate by tool_name rather than content sniffing.
        let is_diff_tool = matches!(self.data.message_type, MessageType::ToolResult)
            && self
                .data
                .tool_name
                .as_deref()
                .map(|n| matches!(n, "Edit" | "Write" | "NotebookEdit" | "MultiEdit"))
                .unwrap_or(false);
        if is_diff_tool {
            let lines: Vec<Line> = text
                .lines()
                .map(|line| {
                    let style = if line.starts_with("+++") || line.starts_with("---") {
                        Style::default().fg(ratatui::style::Color::Rgb(120, 160, 220))
                    } else if line.starts_with("@@") {
                        Style::default()
                            .fg(ratatui::style::Color::Rgb(180, 130, 220))
                            .add_modifier(Modifier::BOLD)
                    } else if line.starts_with('+') {
                        Style::default().fg(ratatui::style::Color::Rgb(120, 200, 130))
                    } else if line.starts_with('-') {
                        Style::default().fg(ratatui::style::Color::Rgb(220, 130, 130))
                    } else {
                        content_style
                    };
                    Line::from(Span::styled(line.to_string(), style))
                })
                .collect();
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .render(text_area, buf);
            return;
        }

        // Bash tool results arrive as `{"stdout":"…","stderr":"…","exit_code":0}`
        // JSON. Unpack into colour-coded sections so the user reads
        // stdout (white) above stderr (red) above exit-code, instead of
        // a literal JSON blob. Falls through to plain Paragraph when
        // parsing fails or the tool isn't Bash.
        let is_bash = matches!(self.data.message_type, MessageType::ToolResult)
            && self.data.tool_name.as_deref() == Some("Bash");
        if is_bash {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(text) {
                if v.is_object() {
                    let mut lines: Vec<Line> = Vec::new();
                    let stdout = v
                        .get("stdout")
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .trim_end_matches('\n');
                    let stderr = v
                        .get("stderr")
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .trim_end_matches('\n');
                    let exit_code = v.get("exit_code").and_then(|c| c.as_i64());
                    if !stdout.is_empty() {
                        for ln in stdout.split('\n') {
                            lines.push(Line::from(Span::styled(
                                ln.to_string(),
                                content_style,
                            )));
                        }
                    }
                    if !stderr.is_empty() {
                        if !lines.is_empty() {
                            lines.push(Line::from(""));
                        }
                        lines.push(Line::from(Span::styled(
                            "stderr:".to_string(),
                            Style::default()
                                .fg(ratatui::style::Color::Rgb(220, 130, 130))
                                .add_modifier(Modifier::BOLD),
                        )));
                        for ln in stderr.split('\n') {
                            lines.push(Line::from(Span::styled(
                                ln.to_string(),
                                Style::default().fg(ratatui::style::Color::Rgb(220, 130, 130)),
                            )));
                        }
                    }
                    if let Some(code) = exit_code {
                        let exit_style = if code == 0 {
                            Style::default()
                                .fg(ratatui::style::Color::Rgb(140, 150, 160))
                                .add_modifier(Modifier::DIM)
                        } else {
                            Style::default()
                                .fg(ratatui::style::Color::Rgb(220, 130, 130))
                                .add_modifier(Modifier::BOLD)
                        };
                        lines.push(Line::from(Span::styled(
                            format!("exit {}", code),
                            exit_style,
                        )));
                    }
                    if !lines.is_empty() {
                        Paragraph::new(lines)
                            .wrap(Wrap { trim: false })
                            .render(text_area, buf);
                        return;
                    }
                }
            }
        }

        // User messages may contain `[Image #N]` markers from the
        // image-paste handler. Render them as a tinted attachment chip
        // inline so the user sees their paste landed instead of a raw
        // bracket string. Other user content is plain Paragraph.
        if matches!(self.data.message_type, MessageType::User)
            && text.contains("[Image #")
        {
            let lines = render_with_image_markers(text, content_style);
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .render(text_area, buf);
            return;
        }

        Paragraph::new(text)
            .style(content_style)
            .wrap(Wrap { trim: false })
            .render(text_area, buf);
    }
}

/// Split `text` on `[Image #N]` markers and emit a `Vec<Line>` where
/// each marker becomes a styled `📎 Image #N` chip while surrounding
/// prose keeps `content_style`. Markers must match `[Image #<digits>]`;
/// anything else passes through unchanged.
fn render_with_image_markers<'a>(text: &str, content_style: Style) -> Vec<Line<'a>> {
    use ratatui::style::Color;
    let chip_style = Style::default()
        .fg(Color::Rgb(255, 255, 255))
        .bg(Color::Rgb(60, 90, 140))
        .add_modifier(Modifier::BOLD);
    let mut lines: Vec<Line> = Vec::new();
    for input_line in text.split('\n') {
        let mut spans: Vec<Span> = Vec::new();
        let mut cursor = 0usize;
        let bytes = input_line.as_bytes();
        while cursor < bytes.len() {
            if let Some(start) = input_line[cursor..].find("[Image #") {
                let abs_start = cursor + start;
                if abs_start > cursor {
                    spans.push(Span::styled(
                        input_line[cursor..abs_start].to_string(),
                        content_style,
                    ));
                }
                if let Some(end_rel) = input_line[abs_start..].find(']') {
                    let abs_end = abs_start + end_rel + 1;
                    let inner = &input_line[abs_start + 1..abs_end - 1]; // "Image #N"
                    spans.push(Span::styled(format!(" 📎 {} ", inner), chip_style));
                    cursor = abs_end;
                } else {
                    spans.push(Span::styled(
                        input_line[abs_start..].to_string(),
                        content_style,
                    ));
                    cursor = input_line.len();
                }
            } else {
                spans.push(Span::styled(
                    input_line[cursor..].to_string(),
                    content_style,
                ));
                cursor = input_line.len();
            }
        }
        lines.push(Line::from(spans));
    }
    lines
}

/// Translate the literal `\n` / `\t` / `\r` escapes that show up when a
/// ToolResult is a raw JSON string into the actual control characters so
/// `Paragraph::wrap` can break lines correctly. Without this a single
/// long stdout payload renders as one un-wrappable row that overflows
/// the message column horizontally — visible in the screenshot bug where
/// `{"stdout":"total 2720\n…"}` ran off the right edge of the screen.
fn unescape_for_display(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.peek() {
                Some('n') => {
                    out.push('\n');
                    chars.next();
                }
                Some('t') => {
                    out.push('\t');
                    chars.next();
                }
                Some('r') => {
                    out.push('\r');
                    chars.next();
                }
                Some('\\') => {
                    out.push('\\');
                    chars.next();
                }
                Some('"') => {
                    out.push('"');
                    chars.next();
                }
                _ => out.push(c),
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Approximate the line count `Paragraph::wrap` will produce — counts hard
/// `\n` breaks and visual wrapping based on CJK-aware cell width. Used by
/// `MessageWidget::render` and `MessageRowWidget::required_height` so the
/// row reserves enough vertical space for the rendered output.
pub fn wrapped_line_count(text: &str, width: u16) -> u16 {
    let w = width.max(1) as usize;
    let mut lines: usize = 0;
    for segment in text.split('\n') {
        let cells =
            unicode_width::UnicodeWidthStr::width(segment).max(1);
        lines += (cells + w - 1) / w;
    }
    if lines == 0 {
        lines = 1;
    }
    lines.min(u16::MAX as usize) as u16
}
