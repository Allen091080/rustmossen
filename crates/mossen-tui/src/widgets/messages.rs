//! Messages list widget — renders a scrollable list of messages.
//!
//! Manages the row-based virtual scroll of transcript entries.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::Line,
    widgets::{Paragraph, Widget, Wrap},
};
use std::collections::HashSet;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::render_block::{tool_card_lines_for_virtual_scroll, RenderBlockWidget};
use crate::layout::VirtualScroll;
use crate::message_model::MessageData;
use crate::render_cache::{RenderHeightCache, RenderHeightFlags};
use crate::render_glyphs::RenderGlyphs;
use crate::render_model::{RenderBlock, RenderBlockKind, RenderNode, RenderTranscript, ToolPhase};
use crate::render_profile::RendererProfile;
use crate::theme::Theme;

/// Messages list widget — renders visible messages in a scrollable viewport.
pub struct MessagesWidget<'a> {
    pub messages: &'a [MessageData],
    pub transcript: Option<&'a RenderTranscript>,
    pub theme: &'a Theme,
    pub scroll: &'a VirtualScroll,
    /// Index where "unseen" divider should appear (None = no divider).
    pub unseen_divider_index: Option<usize>,
    /// When `true`, every row pins its thinking block visible
    /// regardless of the 30s fade timer. Set from `App.show_all_thinking`.
    pub show_all_thinking: bool,
    /// Indices of `ToolUse` rows whose result is currently collapsed.
    /// Rows immediately after a collapsed ToolUse are skipped from
    /// rendering. The ToolUse row itself shows the profile's disclosure glyphs.
    pub collapsed_tool_groups: &'a HashSet<usize>,
    /// Index of the currently keyboard-focused message, drawn with a
    /// highlight band on the prefix column.
    pub focused_idx: Option<usize>,
    /// Optional Layer 3 height cache shared by scroll accounting and render.
    pub height_cache: Option<&'a RenderHeightCache>,
    /// Unicode or ASCII terminal glyph profile.
    pub glyphs: RenderGlyphs,
}

impl<'a> MessagesWidget<'a> {
    pub fn new(messages: &'a [MessageData], theme: &'a Theme, scroll: &'a VirtualScroll) -> Self {
        // Static empty fallback so the no-arg constructor can satisfy
        // the `&HashSet` field without allocating on each call.
        static EMPTY: once_cell::sync::Lazy<std::collections::HashSet<usize>> =
            once_cell::sync::Lazy::new(std::collections::HashSet::new);
        Self {
            messages,
            transcript: None,
            theme,
            scroll,
            unseen_divider_index: None,
            show_all_thinking: false,
            collapsed_tool_groups: &EMPTY,
            focused_idx: None,
            height_cache: None,
            glyphs: RenderGlyphs::default(),
        }
    }

    pub fn transcript(mut self, transcript: &'a RenderTranscript) -> Self {
        self.transcript = Some(transcript);
        self
    }

    pub fn unseen_divider(mut self, index: Option<usize>) -> Self {
        self.unseen_divider_index = index;
        self
    }

    pub fn show_all_thinking(mut self, on: bool) -> Self {
        self.show_all_thinking = on;
        self
    }

    pub fn collapsed_tool_groups(mut self, groups: &'a HashSet<usize>) -> Self {
        self.collapsed_tool_groups = groups;
        self
    }

    pub fn focused_idx(mut self, idx: Option<usize>) -> Self {
        self.focused_idx = idx;
        self
    }

    pub fn height_cache(mut self, cache: &'a RenderHeightCache) -> Self {
        self.height_cache = Some(cache);
        self
    }

    pub fn glyphs(mut self, glyphs: RenderGlyphs) -> Self {
        self.glyphs = glyphs;
        self
    }

    pub fn required_content_height(
        messages: &'a [MessageData],
        theme: &'a Theme,
        width: u16,
        show_all_thinking: bool,
        collapsed_tool_groups: &'a HashSet<usize>,
    ) -> usize {
        let transcript = RenderTranscript::from_messages(messages);
        Self::required_content_height_from_transcript(
            messages.len(),
            &transcript,
            theme,
            width,
            show_all_thinking,
            collapsed_tool_groups,
        )
    }

    pub fn required_content_height_from_transcript(
        source_record_count: usize,
        transcript: &RenderTranscript,
        theme: &'a Theme,
        width: u16,
        show_all_thinking: bool,
        collapsed_tool_groups: &'a HashSet<usize>,
    ) -> usize {
        Self::required_content_height_from_transcript_with_cache(
            source_record_count,
            transcript,
            theme,
            width,
            show_all_thinking,
            collapsed_tool_groups,
            None,
        )
    }

    pub fn required_content_height_from_transcript_with_cache(
        source_record_count: usize,
        transcript: &RenderTranscript,
        theme: &'a Theme,
        width: u16,
        show_all_thinking: bool,
        collapsed_tool_groups: &'a HashSet<usize>,
        height_cache: Option<&RenderHeightCache>,
    ) -> usize {
        Self::required_content_height_from_transcript_with_cache_and_glyphs(
            source_record_count,
            transcript,
            theme,
            width,
            show_all_thinking,
            collapsed_tool_groups,
            height_cache,
            RenderGlyphs::default(),
        )
    }

    pub fn required_content_height_from_transcript_with_cache_and_glyphs(
        source_record_count: usize,
        transcript: &RenderTranscript,
        theme: &'a Theme,
        width: u16,
        show_all_thinking: bool,
        collapsed_tool_groups: &'a HashSet<usize>,
        height_cache: Option<&RenderHeightCache>,
        glyphs: RenderGlyphs,
    ) -> usize {
        build_entries(
            source_record_count,
            transcript,
            theme,
            width,
            show_all_thinking,
            collapsed_tool_groups,
            None,
            None,
            height_cache,
            glyphs,
        )
        .iter()
        .map(RenderEntry::height)
        .sum()
    }

    pub fn content_row_range_for_source_index_from_transcript_with_cache_and_glyphs(
        source_index: usize,
        source_record_count: usize,
        transcript: &RenderTranscript,
        theme: &'a Theme,
        width: u16,
        show_all_thinking: bool,
        collapsed_tool_groups: &'a HashSet<usize>,
        height_cache: Option<&RenderHeightCache>,
        glyphs: RenderGlyphs,
    ) -> Option<(usize, usize)> {
        let entries = build_entries(
            source_record_count,
            transcript,
            theme,
            width,
            show_all_thinking,
            collapsed_tool_groups,
            None,
            Some(source_index),
            height_cache,
            glyphs,
        );
        let mut row = 0usize;
        for entry in entries {
            let height = entry.height();
            if let RenderEntry::Message { block_index, .. } = &entry {
                if transcript
                    .blocks
                    .get(*block_index)
                    .is_some_and(|block| block.source_indices.contains(&source_index))
                {
                    return Some((row, row.saturating_add(height)));
                }
            }
            row = row.saturating_add(height);
        }
        None
    }
}

impl<'a> Widget for MessagesWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let area = area.intersection(buf.area);
        if area.width == 0 || area.height == 0 {
            return;
        }

        let owned_transcript;
        let transcript = match self.transcript {
            Some(transcript) => transcript,
            None => {
                owned_transcript = RenderTranscript::from_messages(self.messages);
                &owned_transcript
            }
        };
        if transcript.is_empty() {
            return;
        }

        let source_record_count = self.messages.len().max(transcript.source_record_count());
        let entries = build_entries(
            source_record_count,
            transcript,
            self.theme,
            area.width,
            self.show_all_thinking,
            self.collapsed_tool_groups,
            self.unseen_divider_index,
            self.focused_idx,
            self.height_cache,
            self.glyphs,
        );
        let total_height: usize = entries.iter().map(RenderEntry::height).sum();
        let viewport_height = area.height as usize;
        let start_row = if self.scroll.sticky {
            total_height.saturating_sub(viewport_height)
        } else {
            self.scroll
                .offset
                .min(total_height.saturating_sub(viewport_height))
        };
        let end_row = start_row.saturating_add(viewport_height);
        let mut row_cursor = 0usize;
        let mut y = area.y;
        let area_bottom = area.bottom();

        for entry in entries {
            if y >= area_bottom {
                break;
            }

            let entry_height = entry.height();
            let entry_start = row_cursor;
            let entry_end = row_cursor.saturating_add(entry_height);
            row_cursor = entry_end;

            if entry_end <= start_row {
                continue;
            }
            if entry_start >= end_row {
                break;
            }

            let skip = start_row.saturating_sub(entry_start).min(entry_height);
            let visible_height = entry_end
                .min(end_row)
                .saturating_sub(entry_start.max(start_row));
            if visible_height == 0 {
                continue;
            }
            let visible_height = visible_height.min(area_bottom.saturating_sub(y) as usize);
            if visible_height == 0 {
                continue;
            }

            render_entry_clipped(
                &entry,
                transcript,
                self.theme,
                self.show_all_thinking,
                area,
                y,
                skip,
                visible_height as u16,
                self.glyphs,
                buf,
            );
            y = y.saturating_add(visible_height as u16);
        }
    }
}

#[derive(Clone)]
enum RenderEntry {
    Divider {
        text: String,
    },
    Message {
        block_index: usize,
        height: usize,
        add_margin: bool,
        collapsed_group: bool,
        focused: bool,
    },
}

impl RenderEntry {
    fn height(&self) -> usize {
        match self {
            Self::Divider { .. } => 1,
            Self::Message { height, .. } => *height,
        }
    }
}

fn build_entries<'a>(
    source_record_count: usize,
    transcript: &'a RenderTranscript,
    theme: &'a Theme,
    width: u16,
    show_all_thinking: bool,
    collapsed_tool_groups: &'a HashSet<usize>,
    unseen_divider_index: Option<usize>,
    focused_idx: Option<usize>,
    height_cache: Option<&RenderHeightCache>,
    glyphs: RenderGlyphs,
) -> Vec<RenderEntry> {
    let mut entries = Vec::new();
    let mut last_source_index = None;
    let profile = RendererProfile::from_width(width);
    for (block_index, block) in transcript.blocks.iter().enumerate() {
        let source_index = block.source_indices.first().copied().unwrap_or(block_index);
        if unseen_divider_index == Some(source_index) {
            let unseen_count = source_record_count.saturating_sub(source_index);
            entries.push(RenderEntry::Divider {
                text: unseen_divider_text(unseen_count, glyphs),
            });
        }

        if hidden_by_collapsed_tool_result(block, collapsed_tool_groups) {
            continue;
        }

        let collapsed_group = collapsed_tool_groups.contains(&source_index)
            && block
                .tool
                .as_ref()
                .is_some_and(|tool| !matches!(tool.phase, ToolPhase::Running));
        let add_margin = last_source_index.is_some();
        let focused = focused_idx.is_some_and(|idx| block.source_indices.contains(&idx));
        let flags = RenderHeightFlags {
            add_margin,
            show_all_thinking,
            focused,
            collapsed: collapsed_group,
        };
        let measure = || {
            RenderBlockWidget::new(block, theme)
                .profile(profile)
                .glyphs(glyphs)
                .add_margin(add_margin)
                .show_all_thinking(show_all_thinking)
                .focused(focused)
                .collapsed(collapsed_group)
                .required_height(width)
                .max(1)
        };
        let height = match height_cache {
            Some(cache) => {
                cache.height_for_block(block, theme.name, width, profile, flags, measure)
            }
            None => measure(),
        };

        entries.push(RenderEntry::Message {
            block_index,
            height,
            add_margin,
            collapsed_group,
            focused,
        });
        last_source_index = Some(source_index);
    }
    entries
}

fn unseen_divider_text(unseen_count: usize, glyphs: RenderGlyphs) -> String {
    let rule = glyphs.border.horizontal_top.repeat(3);
    let label = if unseen_count > 1 {
        "messages"
    } else {
        "message"
    };
    format!("{rule} new {label} {rule}")
}

#[allow(clippy::too_many_arguments)]
fn render_entry_clipped(
    entry: &RenderEntry,
    transcript: &RenderTranscript,
    theme: &Theme,
    show_all_thinking: bool,
    area: Rect,
    dest_y: u16,
    skip: usize,
    visible_height: u16,
    glyphs: RenderGlyphs,
    buf: &mut Buffer,
) {
    let profile = RendererProfile::from_width(area.width);
    match entry {
        RenderEntry::Divider { text } => {
            if skip == 0 && visible_height > 0 {
                let width = area.right().saturating_sub(area.x) as usize;
                buf.set_stringn(area.x, dest_y, text, width, Style::default().fg(theme.info));
            }
        }
        RenderEntry::Message {
            block_index,
            height,
            add_margin,
            collapsed_group,
            focused,
        } => {
            let Some(block) = transcript.blocks.get(*block_index) else {
                return;
            };
            let row = RenderBlockWidget::new(block, theme)
                .profile(profile)
                .glyphs(glyphs)
                .add_margin(*add_margin)
                .show_all_thinking(show_all_thinking)
                .focused(*focused)
                .collapsed(*collapsed_group);

            let full_height = (*height).min(u16::MAX as usize) as u16;
            // `Rect::new` preserves aspect ratio when width * height exceeds
            // u16::MAX, which can silently shrink the scratch width. Keep
            // the real terminal width and cap only the height.
            let max_scratch_height = (u16::MAX / area.width.max(1)).max(1);
            let scratch_height = full_height.min(max_scratch_height);
            if skip.saturating_add(visible_height as usize) > scratch_height as usize
                && (render_tall_tool_block_slice(
                    block,
                    theme,
                    area,
                    dest_y,
                    skip,
                    visible_height,
                    *add_margin,
                    *focused,
                    *collapsed_group,
                    profile,
                    glyphs,
                    buf,
                ) || render_tall_text_block_slice(
                    block,
                    theme,
                    show_all_thinking,
                    area,
                    dest_y,
                    skip,
                    visible_height,
                    *add_margin,
                    *focused,
                    glyphs,
                    buf,
                ))
            {
                return;
            }

            let mut scratch = Buffer::empty(Rect::new(0, 0, area.width, scratch_height));
            row.render(Rect::new(0, 0, area.width, scratch_height), &mut scratch);

            if skip >= scratch.area.height as usize {
                if render_tall_tool_block_slice(
                    block,
                    theme,
                    area,
                    dest_y,
                    skip,
                    visible_height,
                    *add_margin,
                    *focused,
                    *collapsed_group,
                    profile,
                    glyphs,
                    buf,
                ) || render_tall_text_block_slice(
                    block,
                    theme,
                    show_all_thinking,
                    area,
                    dest_y,
                    skip,
                    visible_height,
                    *add_margin,
                    *focused,
                    glyphs,
                    buf,
                ) {
                    return;
                }

                // For extremely tall rows, the backing scratch buffer cannot
                // represent the full virtual height. Copy the deepest
                // rendered slice rather than returning a blank viewport. Tool
                // cards should normally stay bounded by RendererProfile; this
                // fallback is only for unexpected oversized widgets.
                let copy_height = visible_height.min(scratch.area.height);
                let copy_width = area.width.min(scratch.area.width);
                let copy_skip = scratch.area.height.saturating_sub(copy_height);
                copy_scratch_rows(
                    &scratch,
                    copy_skip,
                    copy_height,
                    copy_width,
                    area,
                    dest_y,
                    buf,
                );
                return;
            }

            let skip = skip.min(scratch.area.height as usize) as u16;
            let copy_height = visible_height.min(scratch.area.height.saturating_sub(skip));
            let copy_width = area.width.min(scratch.area.width);
            copy_scratch_rows(&scratch, skip, copy_height, copy_width, area, dest_y, buf);
        }
    }
}

fn copy_scratch_rows(
    scratch: &Buffer,
    skip: u16,
    copy_height: u16,
    copy_width: u16,
    area: Rect,
    dest_y: u16,
    buf: &mut Buffer,
) {
    let area_bottom = area.bottom();
    for dy in 0..copy_height {
        let src_y = skip.saturating_add(dy);
        let dst_y = dest_y.saturating_add(dy);
        if src_y >= scratch.area.height || dst_y >= area_bottom {
            break;
        }
        for x in 0..copy_width {
            let dst_x = area.x.saturating_add(x);
            if !buf.area.contains((dst_x, dst_y).into()) {
                continue;
            }
            let src_idx = scratch.index_of(x, src_y);
            let dst_idx = buf.index_of(dst_x, dst_y);
            buf.content[dst_idx] = scratch.content[src_idx].clone();
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn render_tall_tool_block_slice(
    block: &RenderBlock,
    theme: &Theme,
    area: Rect,
    dest_y: u16,
    skip: usize,
    visible_height: u16,
    add_margin: bool,
    focused: bool,
    collapsed: bool,
    profile: RendererProfile,
    glyphs: RenderGlyphs,
    buf: &mut Buffer,
) -> bool {
    let rows = virtual_tool_block_rows(
        block, theme, area.width, add_margin, focused, collapsed, profile, glyphs,
    );
    if rows.is_empty() {
        return false;
    }

    let visible = visible_height as usize;
    let start = skip.min(rows.len().saturating_sub(visible));
    let max_width = area.width as usize;
    let style = Style::default().fg(theme.text);
    for dy in 0..visible_height {
        let dst_y = dest_y.saturating_add(dy);
        if dst_y >= area.bottom() {
            break;
        }
        let Some(row) = rows.get(start.saturating_add(dy as usize)) else {
            break;
        };
        buf.set_stringn(area.x, dst_y, row, max_width, style);
    }

    true
}

fn virtual_tool_block_rows(
    block: &RenderBlock,
    theme: &Theme,
    width: u16,
    add_margin: bool,
    focused: bool,
    collapsed: bool,
    profile: RendererProfile,
    glyphs: RenderGlyphs,
) -> Vec<String> {
    let Some(tool) = block.tool.as_ref() else {
        return Vec::new();
    };
    if width == 0 {
        return Vec::new();
    }

    let focus_bar_width = usize::from(focused);
    let content_width = (width as usize).saturating_sub(3 + focus_bar_width).max(1);
    let inner_width = content_width.saturating_sub(2).max(1);
    let mut content_rows = Vec::new();
    for line in tool_card_lines_for_virtual_scroll(
        block,
        tool,
        inner_width.min(u16::MAX as usize) as u16,
        profile,
        theme,
        collapsed,
        glyphs,
    ) {
        push_wrapped_virtual_rows(&mut content_rows, &line_plain_text(&line), inner_width);
    }
    if content_rows.is_empty() {
        content_rows.push(String::new());
    }

    let mut rows = Vec::new();
    if add_margin {
        rows.push(String::new());
    }

    let mut card_rows = Vec::with_capacity(content_rows.len().saturating_add(2));
    card_rows.push(virtual_tool_border_row(
        &tool_card_title(tool, collapsed, glyphs),
        content_width,
        true,
        glyphs,
    ));
    for row in content_rows {
        card_rows.push(virtual_tool_inner_row(&row, content_width, glyphs));
    }
    card_rows.push(virtual_tool_border_row("", content_width, false, glyphs));

    let prefix = virtual_prefix_for_block(block.kind, glyphs);
    for (index, card_row) in card_rows.into_iter().enumerate() {
        let mut row = String::new();
        if focused {
            row.push(' ');
        }
        if index == 0 {
            row.push_str(prefix);
            row.push_str("  ");
        } else {
            row.push_str("   ");
        }
        row.push_str(&card_row);
        rows.push(truncate_virtual_row(&row, width as usize));
    }

    rows
}

fn tool_card_title(
    tool: &crate::render_model::ToolCardModel,
    collapsed: bool,
    glyphs: RenderGlyphs,
) -> String {
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

fn virtual_tool_inner_row(text: &str, width: usize, glyphs: RenderGlyphs) -> String {
    if width < 2 {
        return truncate_virtual_row(text, width);
    }
    let inner_width = width.saturating_sub(2);
    format!(
        "{}{}{}",
        glyphs.border.vertical_left,
        pad_virtual_row(text, inner_width),
        glyphs.border.vertical_right
    )
}

fn virtual_tool_border_row(title: &str, width: usize, top: bool, glyphs: RenderGlyphs) -> String {
    if width < 2 {
        return truncate_virtual_row(title, width);
    }
    let inner_width = width.saturating_sub(2);
    let body = if top && !title.is_empty() {
        let title = truncate_virtual_row(title, inner_width);
        let title_width = UnicodeWidthStr::width(title.as_str());
        let fill = glyphs
            .border
            .horizontal_top
            .repeat(inner_width.saturating_sub(title_width));
        format!("{title}{fill}")
    } else {
        glyphs.border.horizontal_bottom.repeat(inner_width)
    };
    if top {
        format!(
            "{}{}{}",
            glyphs.border.top_left, body, glyphs.border.top_right
        )
    } else {
        format!(
            "{}{}{}",
            glyphs.border.bottom_left, body, glyphs.border.bottom_right
        )
    }
}

fn pad_virtual_row(text: &str, width: usize) -> String {
    let mut out = truncate_virtual_row(text, width);
    let used = UnicodeWidthStr::width(out.as_str());
    out.extend(std::iter::repeat(' ').take(width.saturating_sub(used)));
    out
}

#[allow(clippy::too_many_arguments)]
fn render_tall_text_block_slice(
    block: &RenderBlock,
    theme: &Theme,
    show_all_thinking: bool,
    area: Rect,
    dest_y: u16,
    skip: usize,
    visible_height: u16,
    add_margin: bool,
    focused: bool,
    glyphs: RenderGlyphs,
    buf: &mut Buffer,
) -> bool {
    if block.tool.is_some() {
        return false;
    }

    if !block.nodes.iter().all(supports_scrolled_text_node_slice) {
        return render_virtual_text_block_slice(
            block,
            theme,
            show_all_thinking,
            area,
            dest_y,
            skip,
            visible_height,
            add_margin,
            focused,
            glyphs,
            buf,
        );
    }

    let focus_bar_width = u16::from(focused);
    let prefix_x = area.x.saturating_add(focus_bar_width);
    let content_x = prefix_x.saturating_add(3);
    let content_width = area
        .right()
        .saturating_sub(content_x)
        .min(area.width.saturating_sub(3 + focus_bar_width));
    if content_width == 0 || visible_height == 0 {
        return false;
    }

    let max_paragraph_scroll = (u16::MAX as usize)
        .saturating_sub(visible_height as usize)
        .saturating_sub(1);
    if skip > max_paragraph_scroll {
        return render_virtual_text_block_slice(
            block,
            theme,
            show_all_thinking,
            area,
            dest_y,
            skip,
            visible_height,
            add_margin,
            focused,
            glyphs,
            buf,
        );
    }

    let mut state = TallTextSliceState {
        skip,
        dst_y: dest_y,
        remaining: visible_height as usize,
        prefix_on_first_content_row: false,
        prefix_drawn: false,
    };

    if add_margin {
        if state.skip > 0 {
            state.skip -= 1;
        } else {
            state.dst_y = state.dst_y.saturating_add(1);
            state.remaining = state.remaining.saturating_sub(1);
        }
    }
    state.prefix_on_first_content_row = state.skip == 0;

    let chrome = TallTextChrome {
        area,
        prefix_x,
        content_x,
        content_width,
        focused,
        block_kind: block.kind,
        glyphs,
    };

    for node in &block.nodes {
        if state.remaining == 0 || state.dst_y >= area.bottom() {
            break;
        }

        match node {
            RenderNode::Thinking(text) => {
                if show_all_thinking || block.state.streaming {
                    let body = format!("{} {text}", glyphs.thinking);
                    let node_height =
                        crate::widgets::markdown::wrapped_line_count_for_text(&body, content_width);
                    draw_tall_paragraph_node_slice(
                        Paragraph::new(body).style(
                            Style::default()
                                .fg(theme.text_dim)
                                .add_modifier(Modifier::ITALIC),
                        ),
                        node_height,
                        &mut state,
                        chrome,
                        theme,
                        buf,
                    );
                }
            }
            RenderNode::Markdown(text) => {
                let widget = crate::widgets::markdown::MarkdownWidget::new(text)
                    .theme(theme)
                    .base_style(Style::default().fg(theme.text))
                    .glyphs(glyphs)
                    .max_width(content_width);
                let lines = widget.parse_to_lines();
                let node_height =
                    crate::widgets::markdown::wrapped_line_count_for_lines(&lines, content_width);
                draw_tall_paragraph_node_slice(
                    Paragraph::new(lines).wrap(Wrap { trim: false }),
                    node_height,
                    &mut state,
                    chrome,
                    theme,
                    buf,
                );
            }
            RenderNode::PlainText(text) => {
                let node_height =
                    crate::widgets::markdown::wrapped_line_count_for_text(text, content_width);
                draw_tall_paragraph_node_slice(
                    Paragraph::new(text.as_str())
                        .style(Style::default().fg(theme.text))
                        .wrap(Wrap { trim: false }),
                    node_height,
                    &mut state,
                    chrome,
                    theme,
                    buf,
                );
            }
            RenderNode::ApprovalDecision(decision) => {
                let line = decision.line();
                let node_height =
                    crate::widgets::markdown::wrapped_line_count_for_text(&line, content_width);
                draw_tall_paragraph_node_slice(
                    Paragraph::new(line)
                        .style(Style::default().fg(theme.text))
                        .wrap(Wrap { trim: false }),
                    node_height,
                    &mut state,
                    chrome,
                    theme,
                    buf,
                );
            }
            RenderNode::Error(_)
            | RenderNode::FileChangeSummary(_)
            | RenderNode::FinalSummary(_)
            | RenderNode::ToolCard(_) => {}
        }
    }

    true
}

fn supports_scrolled_text_node_slice(node: &RenderNode) -> bool {
    matches!(
        node,
        RenderNode::Thinking(_)
            | RenderNode::Markdown(_)
            | RenderNode::PlainText(_)
            | RenderNode::ApprovalDecision(_)
    )
}

#[derive(Clone, Copy)]
struct TallTextChrome {
    area: Rect,
    prefix_x: u16,
    content_x: u16,
    content_width: u16,
    focused: bool,
    block_kind: RenderBlockKind,
    glyphs: RenderGlyphs,
}

struct TallTextSliceState {
    skip: usize,
    dst_y: u16,
    remaining: usize,
    prefix_on_first_content_row: bool,
    prefix_drawn: bool,
}

fn draw_tall_paragraph_node_slice<'a>(
    paragraph: Paragraph<'a>,
    node_height: usize,
    state: &mut TallTextSliceState,
    chrome: TallTextChrome,
    theme: &Theme,
    buf: &mut Buffer,
) {
    if node_height == 0 || state.remaining == 0 || state.dst_y >= chrome.area.bottom() {
        return;
    }
    if state.skip >= node_height {
        state.skip -= node_height;
        return;
    }

    let node_skip = state.skip;
    state.skip = 0;

    let available_height = state
        .remaining
        .min(node_height.saturating_sub(node_skip))
        .min(chrome.area.bottom().saturating_sub(state.dst_y) as usize);
    if available_height == 0 {
        return;
    }

    let draw_height = available_height.min(u16::MAX as usize) as u16;
    paint_tall_text_chrome(state, chrome, theme, buf, draw_height);
    paragraph
        .scroll((node_skip.min(u16::MAX as usize) as u16, 0))
        .wrap(Wrap { trim: false })
        .render(
            Rect::new(
                chrome.content_x,
                state.dst_y,
                chrome.content_width,
                draw_height,
            ),
            buf,
        );

    state.dst_y = state.dst_y.saturating_add(draw_height);
    state.remaining = state.remaining.saturating_sub(draw_height as usize);
}

fn paint_tall_text_chrome(
    state: &mut TallTextSliceState,
    chrome: TallTextChrome,
    theme: &Theme,
    buf: &mut Buffer,
    height: u16,
) {
    let gutter_width = chrome.content_x.saturating_sub(chrome.area.x);
    let gutter = " ".repeat(gutter_width as usize);
    let gutter_style = if chrome.focused {
        Style::default().bg(theme.warning)
    } else {
        Style::default()
    };

    for dy in 0..height {
        let y = state.dst_y.saturating_add(dy);
        if y >= chrome.area.bottom() {
            break;
        }
        if gutter_width > 0 {
            buf.set_stringn(
                chrome.area.x,
                y,
                &gutter,
                gutter_width as usize,
                gutter_style,
            );
        }
    }

    if state.prefix_on_first_content_row && !state.prefix_drawn {
        buf.set_string(
            chrome.prefix_x,
            state.dst_y,
            virtual_prefix_for_block(chrome.block_kind, chrome.glyphs),
            Style::default().fg(theme.text_dim),
        );
        state.prefix_drawn = true;
    }
}

#[allow(clippy::too_many_arguments)]
fn render_virtual_text_block_slice(
    block: &RenderBlock,
    theme: &Theme,
    show_all_thinking: bool,
    area: Rect,
    dest_y: u16,
    skip: usize,
    visible_height: u16,
    add_margin: bool,
    focused: bool,
    glyphs: RenderGlyphs,
    buf: &mut Buffer,
) -> bool {
    let rows = virtual_text_block_rows(
        block,
        show_all_thinking,
        area.width,
        add_margin,
        focused,
        glyphs,
    );
    if rows.is_empty() {
        return false;
    }

    let visible = visible_height as usize;
    let start = skip.min(rows.len().saturating_sub(visible));
    let max_width = area.width as usize;
    let style = Style::default().fg(theme.text);
    for dy in 0..visible_height {
        let dst_y = dest_y.saturating_add(dy);
        if dst_y >= area.bottom() {
            break;
        }
        let Some(row) = rows.get(start.saturating_add(dy as usize)) else {
            break;
        };
        buf.set_stringn(area.x, dst_y, row, max_width, style);
    }

    true
}

fn virtual_text_block_rows(
    block: &RenderBlock,
    show_all_thinking: bool,
    width: u16,
    add_margin: bool,
    focused: bool,
    glyphs: RenderGlyphs,
) -> Vec<String> {
    let focus_bar_width = usize::from(focused);
    let content_width = (width as usize).saturating_sub(3 + focus_bar_width).max(1);
    let mut content_rows = Vec::new();

    for node in &block.nodes {
        match node {
            RenderNode::Thinking(text) => {
                if show_all_thinking || block.state.streaming {
                    push_wrapped_virtual_rows(
                        &mut content_rows,
                        &format!("{} {text}", glyphs.thinking),
                        content_width,
                    );
                }
            }
            RenderNode::Markdown(text) => {
                let lines = crate::widgets::markdown::MarkdownWidget::new(text)
                    .glyphs(glyphs)
                    .max_width(content_width.min(u16::MAX as usize) as u16)
                    .parse_to_lines();
                for line in &lines {
                    push_wrapped_virtual_rows(
                        &mut content_rows,
                        &line_plain_text(line),
                        content_width,
                    );
                }
            }
            RenderNode::PlainText(text) => {
                push_wrapped_virtual_rows(&mut content_rows, text, content_width);
            }
            RenderNode::Error(error) => {
                push_wrapped_virtual_rows(
                    &mut content_rows,
                    &format!("{}: {}", error.title, error.summary),
                    content_width,
                );
                if let Some(key_detail) = error.key_detail.as_deref() {
                    push_wrapped_virtual_rows(
                        &mut content_rows,
                        &format!("Key detail: {key_detail}"),
                        content_width,
                    );
                }
                if let Some(details) = error.details.as_deref() {
                    for line in details.lines().take(8) {
                        push_wrapped_virtual_rows(&mut content_rows, line, content_width);
                    }
                }
                if error.detail_hidden_line_count > 0 {
                    push_wrapped_virtual_rows(
                        &mut content_rows,
                        &format!(
                            "Full log: {} more lines hidden",
                            error.detail_hidden_line_count
                        ),
                        content_width,
                    );
                }
                if let Some(retry) = error.retry_hint.as_deref() {
                    push_wrapped_virtual_rows(
                        &mut content_rows,
                        &format!("Retry: {retry}"),
                        content_width,
                    );
                }
            }
            RenderNode::FileChangeSummary(summary) => {
                push_wrapped_virtual_rows(
                    &mut content_rows,
                    &format!(
                        "{}: +{} -{}",
                        summary.title(),
                        summary.total_additions(),
                        summary.total_deletions()
                    ),
                    content_width,
                );
                for file in summary.files.iter().take(10) {
                    push_wrapped_virtual_rows(
                        &mut content_rows,
                        &format!(
                            "{} {} +{} -{}",
                            file.status, file.path, file.additions, file.deletions
                        ),
                        content_width,
                    );
                }
                if summary.files.len() > 10 {
                    push_wrapped_virtual_rows(
                        &mut content_rows,
                        &format!("... {} more files", summary.files.len() - 10),
                        content_width,
                    );
                }
            }
            RenderNode::FinalSummary(summary) => {
                let status = if summary.needs_attention() {
                    "Needs attention"
                } else {
                    "Completed"
                };
                push_wrapped_virtual_rows(
                    &mut content_rows,
                    &format!("Final Summary: {status}"),
                    content_width,
                );
                if !summary.changed_files.is_empty() {
                    push_wrapped_virtual_rows(
                        &mut content_rows,
                        &format!("Files: {} changed", summary.changed_files.len()),
                        content_width,
                    );
                    for file in summary.changed_files.iter().take(8) {
                        push_wrapped_virtual_rows(
                            &mut content_rows,
                            &format!(
                                "{} {} +{} -{}",
                                file.status, file.path, file.additions, file.deletions
                            ),
                            content_width,
                        );
                    }
                }
                if !summary.commands.is_empty() {
                    push_wrapped_virtual_rows(
                        &mut content_rows,
                        &format!("Commands: {} run", summary.commands.len()),
                        content_width,
                    );
                    for command in summary.commands.iter().take(6) {
                        push_wrapped_virtual_rows(
                            &mut content_rows,
                            &format!("$ {} {}", command.command, command.status),
                            content_width,
                        );
                    }
                }
                if !summary.verification_results.is_empty() {
                    push_wrapped_virtual_rows(
                        &mut content_rows,
                        &format!(
                            "Verification: {} checks",
                            summary.verification_results.len()
                        ),
                        content_width,
                    );
                    for result in summary.verification_results.iter().take(6) {
                        push_wrapped_virtual_rows(
                            &mut content_rows,
                            &format!("$ {} {}", result.command, result.status),
                            content_width,
                        );
                    }
                }
                if !summary.residual_risks.is_empty() {
                    push_wrapped_virtual_rows(
                        &mut content_rows,
                        &format!("Risks: {}", summary.residual_risks.len()),
                        content_width,
                    );
                    for risk in summary.residual_risks.iter().take(4) {
                        push_wrapped_virtual_rows(
                            &mut content_rows,
                            &format!("! {risk}"),
                            content_width,
                        );
                    }
                }
            }
            RenderNode::ApprovalDecision(decision) => {
                push_wrapped_virtual_rows(&mut content_rows, &decision.line(), content_width);
            }
            RenderNode::ToolCard(_) => return Vec::new(),
        }
    }

    if content_rows.is_empty() {
        content_rows.push(String::new());
    }

    let mut rows = Vec::new();
    if add_margin {
        rows.push(String::new());
    }

    let prefix = virtual_prefix_for_block(block.kind, glyphs);
    for (index, content) in content_rows.into_iter().enumerate() {
        let mut row = String::new();
        if focused {
            row.push(' ');
        }
        if index == 0 {
            row.push_str(prefix);
            row.push_str("  ");
        } else {
            row.push_str("   ");
        }
        row.push_str(&content);
        rows.push(truncate_virtual_row(&row, width as usize));
    }

    rows
}

fn virtual_prefix_for_block(kind: RenderBlockKind, glyphs: RenderGlyphs) -> &'static str {
    match kind {
        RenderBlockKind::User => glyphs.user,
        RenderBlockKind::Assistant => glyphs.assistant,
        RenderBlockKind::System => glyphs.system,
        RenderBlockKind::Error => glyphs.error,
        RenderBlockKind::FileChangeSummary => glyphs.file_change,
        RenderBlockKind::CommandOutput => glyphs.command_output,
        RenderBlockKind::Progress => glyphs.progress,
        RenderBlockKind::Attachment => glyphs.attachment,
        RenderBlockKind::Tool => glyphs.tool,
        RenderBlockKind::ApprovalDecision => glyphs.approval_decision,
        RenderBlockKind::FinalSummary => glyphs.final_summary,
        RenderBlockKind::SkillInvocation => glyphs.skill,
    }
}

fn line_plain_text(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

fn push_wrapped_virtual_rows(rows: &mut Vec<String>, text: &str, width: usize) {
    let width = width.max(1);
    for segment in text.split('\n') {
        if segment.is_empty() {
            rows.push(String::new());
        } else {
            let expected =
                crate::widgets::markdown::wrapped_line_count_for_text(segment, width as u16);
            if expected > u16::MAX as usize || width > u16::MAX as usize {
                push_hard_wrapped_virtual_rows(rows, segment, width);
                continue;
            }
            let area = Rect::new(0, 0, width as u16, expected as u16);
            let mut scratch = Buffer::empty(area);
            Paragraph::new(segment.to_string())
                .wrap(Wrap { trim: false })
                .render(area, &mut scratch);
            for y in 0..area.height {
                rows.push(buffer_row_string(&scratch, y, area.width));
            }
        }
    }
}

fn push_hard_wrapped_virtual_rows(rows: &mut Vec<String>, segment: &str, width: usize) {
    let mut row = String::new();
    let mut used = 0usize;
    for grapheme in segment.graphemes(true) {
        let grapheme_width = UnicodeWidthStr::width(grapheme);
        if grapheme_width > width {
            continue;
        }
        if used > 0 && used.saturating_add(grapheme_width) > width {
            rows.push(std::mem::take(&mut row));
            used = 0;
        }
        row.push_str(grapheme);
        used = used.saturating_add(grapheme_width);
    }
    rows.push(row);
}

fn buffer_row_string(buf: &Buffer, y: u16, width: u16) -> String {
    let mut row = String::new();
    for x in 0..width {
        row.push_str(buf[(x, y)].symbol());
    }
    row.trim_end().to_string()
}

fn truncate_virtual_row(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut used = 0usize;
    for grapheme in text.graphemes(true) {
        let width = UnicodeWidthStr::width(grapheme);
        if used.saturating_add(width) > max_width {
            break;
        }
        out.push_str(grapheme);
        used = used.saturating_add(width);
    }
    out
}

fn hidden_by_collapsed_tool_result(
    block: &RenderBlock,
    collapsed_tool_groups: &HashSet<usize>,
) -> bool {
    if block.source_indices.len() != 1 {
        return false;
    }

    let source_index = block.source_indices[0];
    if !matches!(block.kind, RenderBlockKind::Tool) {
        return false;
    }
    if block
        .tool
        .as_ref()
        .is_some_and(|tool| matches!(tool.phase, ToolPhase::Requested | ToolPhase::Running))
    {
        return false;
    }

    source_index > 0 && collapsed_tool_groups.contains(&(source_index - 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message_model::MessageType;
    use crate::theme::Theme;

    fn message(content: String) -> MessageData {
        MessageData {
            message_type: MessageType::System,
            content,
            timestamp: None,
            is_streaming: false,
            tool_name: None,
            is_error: false,
            thinking: None,
            thinking_completed_at: None,
            full_content: None,
            expanded: false,
        }
    }

    fn assistant_message(content: String) -> MessageData {
        MessageData {
            message_type: MessageType::Assistant,
            content,
            timestamp: None,
            is_streaming: false,
            tool_name: None,
            is_error: false,
            thinking: None,
            thinking_completed_at: None,
            full_content: None,
            expanded: false,
        }
    }

    fn tool_message(message_type: MessageType, content: String, tool_name: &str) -> MessageData {
        MessageData {
            message_type,
            content,
            timestamp: None,
            is_streaming: false,
            tool_name: Some(tool_name.to_string()),
            is_error: false,
            thinking: None,
            thinking_completed_at: None,
            full_content: None,
            expanded: false,
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

    #[test]
    fn sticky_scroll_renders_bottom_of_single_tall_message() {
        let lines = (0..20)
            .map(|n| format!("line {n:02}"))
            .collect::<Vec<_>>()
            .join("\n");
        let messages = vec![message(lines)];
        let theme = Theme::default();
        let mut scroll = VirtualScroll::new(5);
        let total_rows =
            MessagesWidget::required_content_height(&messages, &theme, 40, false, &HashSet::new());
        scroll.set_total_items(total_rows);
        scroll.scroll_to_bottom();

        let mut buf = Buffer::empty(Rect::new(0, 0, 40, 5));
        MessagesWidget::new(&messages, &theme, &scroll).render(buf.area, &mut buf);

        let rendered = buffer_text(&buf);
        assert!(rendered.contains("line 19"));
        assert!(!rendered.contains("line 00"));
    }

    #[test]
    fn render_area_outside_buffer_is_clipped_before_painting() {
        let messages = vec![message("hello".to_string())];
        let theme = Theme::default();
        let mut scroll = VirtualScroll::new(10);
        let total_rows =
            MessagesWidget::required_content_height(&messages, &theme, 20, false, &HashSet::new());
        scroll.set_total_items(total_rows);
        scroll.scroll_to_bottom();

        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 5));
        MessagesWidget::new(&messages, &theme, &scroll).render(Rect::new(0, 8, 20, 10), &mut buf);
    }

    #[test]
    fn supplied_transcript_renders_without_message_slice() {
        let theme = Theme::default();
        let scroll = VirtualScroll::new(8);
        let transcript = crate::render_model::RenderTranscript {
            blocks: vec![crate::render_model::RenderBlock {
                id: "message-10".to_string(),
                source_indices: vec![10],
                kind: crate::render_model::RenderBlockKind::Assistant,
                state: crate::render_model::RenderBlockState::default(),
                nodes: vec![crate::render_model::RenderNode::Markdown(
                    "hello **world**".to_string(),
                )],
                tool: None,
            }],
        };
        let mut buf = Buffer::empty(Rect::new(0, 0, 40, 8));

        MessagesWidget::new(&[], &theme, &scroll)
            .transcript(&transcript)
            .render(buf.area, &mut buf);

        let rendered = buffer_text(&buf);
        assert!(rendered.contains("hello"), "{rendered}");
        assert!(transcript.source_record_count() > 10);
    }

    #[test]
    fn long_bash_result_clips_without_buffer_panic() {
        let stdout = (0..18_000)
            .map(|n| format!("line {n:05} with enough text to wrap"))
            .collect::<Vec<_>>()
            .join("\n");
        let raw = serde_json::json!({
            "stdout": stdout,
            "stderr": "",
            "exit_code": 0,
        })
        .to_string();
        let messages = vec![MessageData {
            message_type: MessageType::ToolResult,
            content: raw.clone(),
            timestamp: None,
            is_streaming: false,
            tool_name: Some("Bash".to_string()),
            is_error: false,
            thinking: None,
            thinking_completed_at: None,
            full_content: Some(raw),
            expanded: true,
        }];
        let theme = Theme::default();
        let mut scroll = VirtualScroll::new(12);
        let total_rows =
            MessagesWidget::required_content_height(&messages, &theme, 22, false, &HashSet::new());
        scroll.set_total_items(total_rows);
        scroll.scroll_to_bottom();

        let mut buf = Buffer::empty(Rect::new(0, 0, 22, 12));
        MessagesWidget::new(&messages, &theme, &scroll).render(buf.area, &mut buf);

        let rendered = buffer_text(&buf);
        assert!(
            rendered.contains("exit") || rendered.trim().contains('0'),
            "{rendered}"
        );
    }

    #[test]
    fn collapsed_long_bash_result_uses_bounded_preview_height() {
        let stdout = (0..18_000)
            .map(|n| format!("line {n:05} with enough text to wrap"))
            .collect::<Vec<_>>()
            .join("\n");
        let raw = serde_json::json!({
            "stdout": stdout,
            "stderr": "",
            "exit_code": 0,
        })
        .to_string();
        let mut summary: String = raw.chars().take(600).collect();
        summary.push('…');
        let messages = vec![MessageData {
            message_type: MessageType::ToolResult,
            content: summary,
            timestamp: None,
            is_streaming: false,
            tool_name: Some("Bash".to_string()),
            is_error: false,
            thinking: None,
            thinking_completed_at: None,
            full_content: Some(raw),
            expanded: false,
        }];
        let theme = Theme::default();

        let total_rows =
            MessagesWidget::required_content_height(&messages, &theme, 40, false, &HashSet::new());

        assert!(
            total_rows < 120,
            "collapsed preview should stay bounded, got {total_rows} rows"
        );

        let mut scroll = VirtualScroll::new(18);
        scroll.set_total_items(total_rows);
        scroll.scroll_to_bottom();
        let mut buf = Buffer::empty(Rect::new(0, 0, 40, 18));
        MessagesWidget::new(&messages, &theme, &scroll).render(buf.area, &mut buf);
        let rendered = buffer_text(&buf);

        assert!(rendered.contains("Output:"), "{rendered}");
        assert!(rendered.contains("full log"), "{rendered}");
        assert!(!rendered.contains("line 17999"), "{rendered}");
    }

    #[test]
    fn virtual_text_rows_match_measured_text_block_height() {
        let theme = Theme::default();
        let blocks = vec![
            crate::render_model::RenderBlock {
                id: "assistant-markdown".to_string(),
                source_indices: vec![0],
                kind: crate::render_model::RenderBlockKind::Assistant,
                state: crate::render_model::RenderBlockState::default(),
                nodes: vec![crate::render_model::RenderNode::Markdown(
                    concat!(
                        "## 标题\n\n",
                        "这是一段包含中文、emoji ✅、很长英文 token ",
                        "render_pipeline_height_contract_long_token_without_spaces ",
                        "以及普通空格换行的 Markdown。\n\n",
                        "```rust\nfn main() {\n    println!(\"height contract\");\n}\n```\n\n",
                        "| Layer | Responsibility |\n",
                        "| --- | --- |\n",
                        "| L2 | semantics |\n",
                        "| L3 | terminal |\n"
                    )
                    .to_string(),
                )],
                tool: None,
            },
            crate::render_model::RenderBlock {
                id: "assistant-plain".to_string(),
                source_indices: vec![1],
                kind: crate::render_model::RenderBlockKind::Assistant,
                state: crate::render_model::RenderBlockState::default(),
                nodes: vec![crate::render_model::RenderNode::PlainText(
                    "plain text row with 中文宽字符 and long_unbroken_segment_for_wrapping"
                        .to_string(),
                )],
                tool: None,
            },
            crate::render_model::RenderBlock {
                id: "assistant-thinking".to_string(),
                source_indices: vec![2],
                kind: crate::render_model::RenderBlockKind::Assistant,
                state: crate::render_model::RenderBlockState {
                    streaming: true,
                    ..crate::render_model::RenderBlockState::default()
                },
                nodes: vec![crate::render_model::RenderNode::Thinking(
                    "正在推理：宽字符、emoji ✅、wrapped rows 都必须和高度统计一致。".to_string(),
                )],
                tool: None,
            },
        ];

        for block in &blocks {
            for width in [12, 16, 24, 48, 80] {
                for add_margin in [false, true] {
                    for focused in [false, true] {
                        let measured = RenderBlockWidget::new(block, &theme)
                            .add_margin(add_margin)
                            .show_all_thinking(false)
                            .focused(focused)
                            .required_height(width);
                        let glyphs = RenderGlyphs::default();
                        let virtual_rows = virtual_text_block_rows(
                            block, false, width, add_margin, focused, glyphs,
                        );
                        assert_eq!(
                            virtual_rows.len(),
                            measured,
                            "virtual rows must match measured height for block={} width={width} margin={add_margin} focused={focused}\nrows={virtual_rows:#?}",
                            block.id
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn sticky_bottom_reaches_tail_of_long_markdown_stream() {
        let theme = Theme::default();
        let mut content = String::from("PTY_LONG_MATRIX_HEAD_W107\n");
        for index in 0..903 {
            content.push_str(&format!(
                "matrix-row-{index:04}: long external PTY soak keeps streaming, resize, and scroll stable.\n"
            ));
        }
        content.push_str("PTY_LONG_MATRIX_TAIL_W107\n");
        let messages = vec![assistant_message(content)];
        let width = 96;
        let height = 24;
        let total_rows = MessagesWidget::required_content_height(
            &messages,
            &theme,
            width,
            false,
            &HashSet::new(),
        );
        let mut scroll = VirtualScroll::new(height);
        scroll.set_total_items(total_rows);
        scroll.scroll_to_bottom();
        let mut buf = Buffer::empty(Rect::new(0, 0, width, height));

        MessagesWidget::new(&messages, &theme, &scroll).render(buf.area, &mut buf);

        let rendered = buffer_text(&buf);
        assert!(
            rendered.contains("PTY_LONG_MATRIX_TAIL_W107"),
            "sticky bottom must render the real tail, not stop above it\n{rendered}"
        );
    }

    #[test]
    fn large_transcript_height_cache_reuses_layouts() {
        let mut messages = Vec::new();
        for index in 0..1_050 {
            messages.push(assistant_message(format!(
                "assistant block {index}\n\n```rust\nfn sample_{index}() {{}}\n```"
            )));
            if index % 21 == 0 {
                messages.push(tool_message(
                    MessageType::ToolUse,
                    serde_json::json!({ "command": format!("echo {index}") }).to_string(),
                    "Bash",
                ));
                messages.push(tool_message(
                    MessageType::ToolResult,
                    serde_json::json!({
                        "stdout": format!("line {index}\nline {}", index + 1),
                        "stderr": "",
                        "exit_code": 0,
                    })
                    .to_string(),
                    "Bash",
                ));
            }
        }

        let theme = Theme::default();
        let transcript = RenderTranscript::from_messages(&messages);
        assert!(
            transcript.blocks.len() >= 1_000,
            "fixture should exercise 1000+ render blocks, got {}",
            transcript.blocks.len()
        );

        let cache = RenderHeightCache::default();
        let started = std::time::Instant::now();
        let first = MessagesWidget::required_content_height_from_transcript_with_cache(
            messages.len(),
            &transcript,
            &theme,
            80,
            false,
            &HashSet::new(),
            Some(&cache),
        );
        let after_first = cache.stats();
        let second = MessagesWidget::required_content_height_from_transcript_with_cache(
            messages.len(),
            &transcript,
            &theme,
            80,
            false,
            &HashSet::new(),
            Some(&cache),
        );
        let after_second = cache.stats();

        assert_eq!(first, second);
        assert!(after_first.misses >= transcript.blocks.len());
        assert!(after_second.hits >= transcript.blocks.len());
        assert_eq!(after_second.entries, after_first.entries);

        let mut scroll = VirtualScroll::new(24);
        scroll.set_total_items(second);
        scroll.scroll_to_bottom();
        let mut buf = Buffer::empty(Rect::new(0, 0, 80, 24));
        MessagesWidget::new(&messages, &theme, &scroll)
            .transcript(&transcript)
            .height_cache(&cache)
            .render(buf.area, &mut buf);
        let rendered = buffer_text(&buf);
        assert!(rendered.contains("assistant block 1049"), "{rendered}");

        let elapsed = started.elapsed();
        eprintln!(
            "large transcript layout: blocks={}, rows={}, first_misses={}, second_hits={}, elapsed_ms={}",
            transcript.blocks.len(),
            second,
            after_first.misses,
            after_second.hits,
            elapsed.as_millis()
        );
        assert!(
            elapsed < std::time::Duration::from_secs(10),
            "large transcript layout took {:?}",
            elapsed
        );
    }
}
