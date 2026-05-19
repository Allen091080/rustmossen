//! Messages list widget — renders a scrollable list of messages.
//!
//! Translates Messages.tsx — manages the virtual scroll of message entries.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

use super::message::{MessageData, MessageType};
use super::message_row::MessageRowWidget;
use crate::layout::VirtualScroll;
use crate::theme::Theme;

/// Messages list widget — renders visible messages in a scrollable viewport.
pub struct MessagesWidget<'a> {
    pub messages: &'a [MessageData],
    pub theme: &'a Theme,
    pub scroll: &'a VirtualScroll,
    /// Index where "unseen" divider should appear (None = no divider).
    pub unseen_divider_index: Option<usize>,
    /// When `true`, every row pins its thinking block visible
    /// regardless of the 30s fade timer. Set from `App.show_all_thinking`.
    pub show_all_thinking: bool,
    /// Indices of `ToolUse` rows whose result is currently collapsed.
    /// Rows immediately after a collapsed ToolUse are skipped from
    /// rendering. The ToolUse row itself shows `▶`; expanded shows `▼`.
    pub collapsed_tool_groups: &'a std::collections::HashSet<usize>,
    /// Index of the currently keyboard-focused message, drawn with a
    /// highlight band on the prefix column.
    pub focused_idx: Option<usize>,
}

impl<'a> MessagesWidget<'a> {
    pub fn new(messages: &'a [MessageData], theme: &'a Theme, scroll: &'a VirtualScroll) -> Self {
        // Static empty fallback so the no-arg constructor can satisfy
        // the `&HashSet` field without allocating on each call.
        static EMPTY: once_cell::sync::Lazy<std::collections::HashSet<usize>> =
            once_cell::sync::Lazy::new(std::collections::HashSet::new);
        Self {
            messages,
            theme,
            scroll,
            unseen_divider_index: None,
            show_all_thinking: false,
            collapsed_tool_groups: &EMPTY,
            focused_idx: None,
        }
    }

    pub fn unseen_divider(mut self, index: Option<usize>) -> Self {
        self.unseen_divider_index = index;
        self
    }

    pub fn show_all_thinking(mut self, on: bool) -> Self {
        self.show_all_thinking = on;
        self
    }

    pub fn collapsed_tool_groups(
        mut self,
        groups: &'a std::collections::HashSet<usize>,
    ) -> Self {
        self.collapsed_tool_groups = groups;
        self
    }

    pub fn focused_idx(mut self, idx: Option<usize>) -> Self {
        self.focused_idx = idx;
        self
    }
}

impl<'a> Widget for MessagesWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 || self.messages.is_empty() {
            return;
        }

        let start = self.scroll.offset;
        let end = (start + area.height as usize).min(self.messages.len());
        let mut y = area.y;

        for i in start..end {
            if y >= area.y + area.height {
                break;
            }

            // Render unseen divider if needed
            if let Some(div_idx) = self.unseen_divider_index {
                if i == div_idx && y < area.y + area.height {
                    let divider_style = Style::default().fg(self.theme.info);
                    let divider_text = format!(
                        "─── new {} ───",
                        if end - div_idx > 1 {
                            "messages"
                        } else {
                            "message"
                        }
                    );
                    buf.set_string(area.x, y, &divider_text, divider_style);
                    y += 1;
                    if y >= area.y + area.height {
                        break;
                    }
                }
            }

            let msg = &self.messages[i];
            // Group rendering: a ToolResult that follows a ToolUse is
            // visually subordinate to the call above it. Drop the
            // top-margin and let MessageRowWidget render an `└─` indent
            // (via is_grouped_tool_result) so the user reads them as a
            // single call/response unit instead of two unrelated lines.
            let prev_is_tool_use = i > 0
                && matches!(self.messages[i - 1].message_type, MessageType::ToolUse);
            let is_grouped_tool_result =
                matches!(msg.message_type, MessageType::ToolResult) && prev_is_tool_use;
            // Hide ToolResult rows whose preceding ToolUse is collapsed.
            if is_grouped_tool_result && self.collapsed_tool_groups.contains(&(i - 1)) {
                continue;
            }
            let collapsed_group = matches!(msg.message_type, MessageType::ToolUse)
                && self.collapsed_tool_groups.contains(&i);
            let focused = self.focused_idx == Some(i);
            let add_margin = i > start && !is_grouped_tool_result;
            let available_height = area.y + area.height - y;

            let row = MessageRowWidget::new(msg, self.theme)
                .add_margin(add_margin)
                .is_last(i == self.messages.len() - 1)
                .show_all_thinking(self.show_all_thinking)
                .grouped_tool_result(is_grouped_tool_result)
                .collapsed_group(collapsed_group)
                .focused(focused);
            let needed = row.required_height(area.width).max(1);
            let row_height = needed.min(available_height);

            let row_area = Rect::new(area.x, y, area.width, row_height);
            row.render(row_area, buf);

            y = y.saturating_add(row_height);
        }
    }
}
