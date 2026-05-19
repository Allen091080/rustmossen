//! Message row widget — wraps a message with margin and layout.
//!
//! Translates MessageRow.tsx — adds consistent spacing and alignment.

use ratatui::{buffer::Buffer, layout::Rect, style::Style, widgets::Widget};
use unicode_width::UnicodeWidthStr;

use super::message::{MessageData, MessageWidget};
use crate::theme::Theme;

/// A single message row with margin control.
pub struct MessageRowWidget<'a> {
    pub data: &'a MessageData,
    pub theme: &'a Theme,
    pub add_margin: bool,
    pub is_last: bool,
    pub show_all_thinking: bool,
    /// When `true`, the row prepends an `└─ ` indent glyph to make a
    /// tool_result visibly subordinate to the tool_use directly above it.
    /// Set by `MessagesWidget` after a `ToolUse → ToolResult` pairing.
    pub grouped_tool_result: bool,
    /// `true` when this row is a `ToolUse` whose paired `ToolResult` is
    /// hidden by the user — drawn with `▶` instead of `▼` and a "(N
    /// chars hidden)" hint so the user knows it's expandable.
    pub collapsed_group: bool,
    /// `true` when this row currently has keyboard focus. Renders a
    /// reverse-video band on the prefix column.
    pub focused: bool,
}

impl<'a> MessageRowWidget<'a> {
    pub fn new(data: &'a MessageData, theme: &'a Theme) -> Self {
        Self {
            data,
            theme,
            add_margin: true,
            is_last: false,
            show_all_thinking: false,
            grouped_tool_result: false,
            collapsed_group: false,
            focused: false,
        }
    }

    pub fn grouped_tool_result(mut self, on: bool) -> Self {
        self.grouped_tool_result = on;
        self
    }

    pub fn collapsed_group(mut self, on: bool) -> Self {
        self.collapsed_group = on;
        self
    }

    pub fn focused(mut self, on: bool) -> Self {
        self.focused = on;
        self
    }

    pub fn add_margin(mut self, add: bool) -> Self {
        self.add_margin = add;
        self
    }

    pub fn is_last(mut self, is_last: bool) -> Self {
        self.is_last = is_last;
        self
    }

    pub fn show_all_thinking(mut self, on: bool) -> Self {
        self.show_all_thinking = on;
        self
    }

    /// Mirror the renderer's fade logic so `required_height` reserves
    /// zero lines once the thinking block has faded out.
    fn thinking_visible(&self) -> bool {
        if self.show_all_thinking {
            return true;
        }
        match self.data.thinking_completed_at {
            None => true, // still streaming
            Some(t) => t.elapsed() < std::time::Duration::from_secs(30),
        }
    }

    /// Calculate the height needed for this message row.
    ///
    /// Counts hard line breaks and visual wrapping based on terminal cell
    /// width (CJK characters occupy two cells). When the message carries a
    /// `thinking` block, its `💭 …` lines are reserved above the content,
    /// plus a one-row spacer between the two. The tool-name header adds
    /// one extra row when present, and a margin row is appended when set.
    pub fn required_height(&self, width: u16) -> u16 {
        let content_width = width.saturating_sub(3) as usize; // prefix + space
        if content_width == 0 {
            return 1;
        }
        let mut lines: usize = 0;
        for segment in self.data.content.split('\n') {
            let seg_width = UnicodeWidthStr::width(segment).max(1);
            lines += (seg_width + content_width - 1) / content_width;
        }
        if lines == 0 {
            lines = 1;
        }
        let mut thinking_lines: usize = 0;
        if let Some(ref thinking) = self.data.thinking {
            if !thinking.is_empty() && self.thinking_visible() {
                // Match the `💭 ` prefix the renderer prepends so the
                // reservation matches the actual rendered output.
                let body = format!("💭 {}", thinking);
                for segment in body.split('\n') {
                    let seg_width = UnicodeWidthStr::width(segment).max(1);
                    thinking_lines += (seg_width + content_width - 1) / content_width;
                }
                // Spacer row between the reasoning block and the answer.
                if !self.data.content.is_empty() {
                    thinking_lines += 1;
                }
            }
        }
        // Tool rows are drawn inside a rounded box (top + bottom border
        // = 2 rows). Non-tool rows still get a 1-row tool-name header
        // when a tool_name is set (rare — used by misc system rows).
        let is_tool_kind = matches!(
            self.data.message_type,
            super::message::MessageType::ToolUse | super::message::MessageType::ToolResult
        );
        let header = if is_tool_kind {
            2
        } else if self.data.tool_name.is_some() {
            1
        } else {
            0
        };
        let margin = if self.add_margin { 1 } else { 0 };
        (lines + thinking_lines + header + margin).min(u16::MAX as usize) as u16
    }
}

impl<'a> Widget for MessageRowWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let y_offset = if self.add_margin { 1 } else { 0 };
        let msg_area = Rect::new(
            area.x,
            area.y + y_offset,
            area.width,
            area.height.saturating_sub(y_offset),
        );

        // Focus highlight — a single yellow bar in column 0 of every
        // row of the focused message. Keeps the visual cue simple but
        // unambiguous regardless of theme.
        if self.focused {
            use ratatui::style::{Color, Style};
            let focus_style = Style::default().bg(Color::Yellow);
            for row in 0..msg_area.height {
                buf.set_string(msg_area.x, msg_area.y + row, " ", focus_style);
            }
        }
        let bar_offset = if self.focused { 1 } else { 0 };

        // Box-render: ToolUse and ToolResult rows are wrapped in a
        // rounded-corner box so the call/response pair reads as a
        // single, visually-contained unit instead of two free-floating
        // lines. The box uses dim grey for resting state and accent
        // colour when focused; the title carries the tool name.
        let after_bar_x = msg_area.x + bar_offset;
        let after_bar_w = msg_area.width.saturating_sub(bar_offset);
        let is_tool = matches!(
            self.data.message_type,
            super::message::MessageType::ToolUse | super::message::MessageType::ToolResult
        );
        let render_area = if is_tool && after_bar_w > 6 && msg_area.height >= 2 {
            use ratatui::style::Style;
            use ratatui::widgets::{Block, BorderType, Borders};
            let border_color = if self.focused {
                ratatui::style::Color::Rgb(200, 180, 90)
            } else {
                self.theme.text_dim
            };
            let title = match (&self.data.message_type, &self.data.tool_name) {
                (super::message::MessageType::ToolUse, Some(n)) => {
                    let icon = if self.collapsed_group { "▶" } else { "▼" };
                    format!(" {} {} ", icon, n)
                }
                (super::message::MessageType::ToolResult, Some(n)) => {
                    format!(" ↳ {} ", n)
                }
                _ => " tool ".to_string(),
            };
            let block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(border_color))
                .title(title);
            let inner = block.inner(Rect::new(
                after_bar_x,
                msg_area.y,
                after_bar_w,
                msg_area.height,
            ));
            block.render(
                Rect::new(after_bar_x, msg_area.y, after_bar_w, msg_area.height),
                buf,
            );
            inner
        } else if self.grouped_tool_result && after_bar_w > 4 {
            use ratatui::style::Style;
            let indent_style = Style::default().fg(self.theme.text_dim);
            buf.set_string(after_bar_x, msg_area.y, "└─ ", indent_style);
            Rect::new(
                after_bar_x + 3,
                msg_area.y,
                after_bar_w.saturating_sub(3),
                msg_area.height,
            )
        } else {
            Rect::new(after_bar_x, msg_area.y, after_bar_w, msg_area.height)
        };

        // The ▶/▼ glyph and tool-name are folded into the box title above
        // when `is_tool` is true; no separate in-body indicator needed.

        // Decide which body to render: when the user expanded a
        // ToolResult, swap in `full_content` so they see the entire
        // tool output rather than the 600-char preview.
        let mut effective_data = self.data.clone();
        if matches!(self.data.message_type, super::message::MessageType::ToolResult)
            && self.data.expanded
        {
            if let Some(full) = &self.data.full_content {
                effective_data.content = full.clone();
            }
        }

        let widget = MessageWidget::new(&effective_data, self.theme)
            .width(render_area.width)
            .show_all_thinking(self.show_all_thinking)
            .suppress_tool_header(is_tool);
        widget.render(render_area, buf);

        // Tool result truncation hint — derived from `full_content`
        // length vs `content` length so the message body itself stays
        // clean of sentinel strings.
        if matches!(self.data.message_type, super::message::MessageType::ToolResult)
            && render_area.height > 1
        {
            if let Some(full) = &self.data.full_content {
                let hidden = full.chars().count().saturating_sub(self.data.content.chars().count());
                if hidden > 0 {
                    use ratatui::style::{Modifier, Style};
                    let hint_y = render_area.y + render_area.height - 1;
                    let hint_style = Style::default()
                        .fg(self.theme.text_dim)
                        .add_modifier(Modifier::ITALIC);
                    let hint = if self.data.expanded {
                        "← collapse".to_string()
                    } else {
                        format!("… {} more chars — → to expand", hidden)
                    };
                    // Trim hint to area width so it doesn't bleed off-screen.
                    let max_w = render_area.width as usize;
                    let cropped: String = hint.chars().take(max_w).collect();
                    buf.set_string(render_area.x, hint_y, &cropped, hint_style);
                }
            }
        }
    }
}
