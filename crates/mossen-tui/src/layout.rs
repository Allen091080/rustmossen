//! Layout system for the TUI layer.
//!
//! Provides the fullscreen layout and common ratatui layout helpers.

use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// Main application layout regions.
#[derive(Debug, Clone, Copy)]
pub struct AppLayout {
    /// Header area (sticky prompt, breadcrumbs)
    pub header: Rect,
    /// Main scrollable content area (messages)
    pub content: Rect,
    /// Bottom pinned area (prompt input, spinner, permissions)
    pub bottom: Rect,
}

/// Deterministic layout for auxiliary live panels (TodoWrite task list and
/// sub-agent activity). These panels must never be painted over messages; when
/// the terminal is too small to reserve space, the areas are `None` and callers
/// should skip the live panel rather than covering transcript content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuxiliaryPanelLayout {
    pub messages: Rect,
    pub task_list: Option<Rect>,
    pub teammates: Option<Rect>,
}

impl AppLayout {
    /// Compute the standard fullscreen layout from terminal area.
    ///
    /// Fullscreen path:
    /// - Header: 0-1 rows (sticky prompt when scrolled up)
    /// - Content: flex-grow (scrollable messages)
    /// - Bottom: up to 50% height (prompt + suggestions)
    pub fn fullscreen(area: Rect, header_height: u16, bottom_height: u16) -> Self {
        let bottom_h = bottom_height.min(area.height / 2);
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(header_height),
                Constraint::Min(1),
                Constraint::Length(bottom_h),
            ])
            .split(area);

        Self {
            header: chunks[0],
            content: chunks[1],
            bottom: chunks[2],
        }
    }

    /// Simple inline layout (non-fullscreen mode).
    /// Content and bottom rendered sequentially.
    pub fn inline(area: Rect, bottom_height: u16) -> Self {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(bottom_height)])
            .split(area);

        Self {
            header: Rect::new(area.x, area.y, area.width, 0),
            content: chunks[0],
            bottom: chunks[1],
        }
    }
}

/// Split a content region into transcript and live auxiliary panel areas.
pub fn split_auxiliary_panels(
    content: Rect,
    task_count: usize,
    teammate_count: usize,
) -> AuxiliaryPanelLayout {
    let task_height = panel_height(task_count, 2, 10, 3);
    let teammate_height = panel_height(teammate_count, 1, 10, 2);
    let has_task = task_height > 0;
    let has_teammate = teammate_height > 0;
    let gap = u16::from(has_task && has_teammate);
    let total_panel_height = task_height
        .saturating_add(teammate_height)
        .saturating_add(gap);

    if total_panel_height == 0 || content.width < 24 || content.height < 8 {
        return AuxiliaryPanelLayout {
            messages: content,
            task_list: None,
            teammates: None,
        };
    }

    // Wide terminals get a right rail. This preserves vertical scrollback while
    // still keeping live task/sub-agent state visible.
    if content.width >= 112 {
        let rail_width = (content.width / 3).clamp(30, 44);
        if content.width > rail_width.saturating_add(48) {
            let messages_width = content.width - rail_width - 1;
            let rail_x = content.x.saturating_add(messages_width).saturating_add(1);
            let messages = Rect::new(content.x, content.y, messages_width, content.height);
            let mut y = content.y;
            let task_list = if has_task {
                let h = task_height.min(content.bottom().saturating_sub(y));
                let area = Rect::new(rail_x, y, rail_width, h);
                let after_task = y.saturating_add(h);
                y = after_task.saturating_add(gap.min(content.bottom().saturating_sub(after_task)));
                Some(area)
            } else {
                None
            };
            let teammates = if has_teammate && y < content.bottom() {
                let h = teammate_height.min(content.bottom().saturating_sub(y));
                Some(Rect::new(rail_x, y, rail_width, h))
            } else {
                None
            };

            return AuxiliaryPanelLayout {
                messages,
                task_list,
                teammates,
            };
        }
    }

    // Narrow terminals reserve rows below the transcript, provided enough
    // transcript height remains. Otherwise skip panels instead of overlaying.
    if content.height > total_panel_height.saturating_add(6) {
        let messages_height = content.height - total_panel_height;
        let messages = Rect::new(content.x, content.y, content.width, messages_height);
        let mut y = content.y.saturating_add(messages_height);
        let task_list = if has_task {
            let area = Rect::new(content.x, y, content.width, task_height);
            y = y.saturating_add(task_height.saturating_add(gap));
            Some(area)
        } else {
            None
        };
        let teammates = if has_teammate {
            Some(Rect::new(content.x, y, content.width, teammate_height))
        } else {
            None
        };
        return AuxiliaryPanelLayout {
            messages,
            task_list,
            teammates,
        };
    }

    AuxiliaryPanelLayout {
        messages: content,
        task_list: None,
        teammates: None,
    }
}

fn panel_height(count: usize, chrome_rows: u16, max_rows: u16, min_rows: u16) -> u16 {
    if count == 0 {
        0
    } else {
        let count_rows = u16::try_from(count).unwrap_or(u16::MAX);
        count_rows
            .saturating_add(chrome_rows)
            .clamp(min_rows, max_rows)
    }
}

/// Split an area into columns with given proportions.
pub fn split_horizontal(area: Rect, constraints: &[Constraint]) -> Vec<Rect> {
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area)
        .to_vec()
}

/// Split an area into rows with given proportions.
pub fn split_vertical(area: Rect, constraints: &[Constraint]) -> Vec<Rect> {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area)
        .to_vec()
}

/// Apply padding to a Rect (shrink inward).
pub fn pad(area: Rect, top: u16, right: u16, bottom: u16, left: u16) -> Rect {
    let x = area.x.saturating_add(left);
    let y = area.y.saturating_add(top);
    let horizontal = left.saturating_add(right);
    let vertical = top.saturating_add(bottom);
    let width = area.width.saturating_sub(horizontal);
    let height = area.height.saturating_sub(vertical);
    Rect::new(x, y, width, height)
}

/// Center a block of given size within an area.
pub fn center(area: Rect, width: u16, height: u16) -> Rect {
    let x = area
        .x
        .saturating_add((area.width.saturating_sub(width)) / 2);
    let y = area
        .y
        .saturating_add((area.height.saturating_sub(height)) / 2);
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}

/// Modal overlay layout — covers most of the screen with a peek above.
pub struct ModalLayout {
    pub peek: Rect,
    pub divider: Rect,
    pub content: Rect,
}

impl ModalLayout {
    /// Create modal layout with given peek rows visible above.
    pub fn new(area: Rect, peek_rows: u16, padding_x: u16) -> Self {
        let divider_y = peek_rows.min(area.height.saturating_sub(1));
        let content_y = divider_y.saturating_add(1);
        let content_height = area.height.saturating_sub(content_y);
        let horizontal_padding = padding_x.saturating_mul(2);

        Self {
            peek: Rect::new(area.x, area.y, area.width, divider_y),
            divider: Rect::new(area.x, area.y.saturating_add(divider_y), area.width, 1),
            content: Rect::new(
                area.x.saturating_add(padding_x),
                area.y.saturating_add(content_y),
                area.width.saturating_sub(horizontal_padding),
                content_height,
            ),
        }
    }
}

/// Virtual scroll state for large message lists.
pub struct VirtualScroll {
    /// Total number of rendered rows
    pub total_items: usize,
    /// First visible rendered row
    pub offset: usize,
    /// Number of visible rows
    pub visible_count: usize,
    /// Whether auto-scroll to bottom is active (sticky scroll)
    pub sticky: bool,
    /// Viewport height in rows
    pub viewport_height: u16,
}

impl VirtualScroll {
    pub fn new(viewport_height: u16) -> Self {
        Self {
            total_items: 0,
            offset: 0,
            visible_count: viewport_height as usize,
            sticky: true,
            viewport_height,
        }
    }

    /// Update total rows and auto-scroll if sticky.
    pub fn set_total_items(&mut self, count: usize) {
        self.total_items = count;
        if self.sticky {
            self.scroll_to_bottom();
        } else {
            self.offset = self
                .offset
                .min(self.total_items.saturating_sub(self.visible_count));
        }
    }

    /// Scroll to bottom and re-enable sticky.
    pub fn scroll_to_bottom(&mut self) {
        self.sticky = true;
        self.offset = self.max_scroll_offset();
    }

    /// Scroll to the first row. Sticky stays enabled only when there is no overflow.
    pub fn scroll_to_top(&mut self) {
        let max_offset = self.max_scroll_offset();
        self.offset = 0;
        self.sticky = max_offset == 0;
    }

    /// Scroll up by n items, disabling sticky.
    pub fn scroll_up(&mut self, n: usize) {
        let max_offset = self.max_scroll_offset();
        if n == 0 || max_offset == 0 {
            self.offset = self.offset.min(max_offset);
            if max_offset == 0 {
                self.sticky = true;
            }
            return;
        }
        let current_offset = if self.sticky {
            max_offset
        } else {
            self.offset.min(max_offset)
        };
        self.sticky = false;
        self.offset = current_offset.saturating_sub(n);
    }

    /// Scroll down by n items, re-enabling sticky if at bottom.
    pub fn scroll_down(&mut self, n: usize) {
        let max_offset = self.max_scroll_offset();
        if n == 0 || max_offset == 0 {
            self.offset = self.offset.min(max_offset);
            if max_offset == 0 || self.offset >= max_offset {
                self.sticky = true;
            }
            return;
        }
        let current_offset = if self.sticky {
            max_offset
        } else {
            self.offset.min(max_offset)
        };
        self.offset = current_offset.saturating_add(n).min(max_offset);
        if self.offset >= max_offset {
            self.sticky = true;
        }
    }

    /// Update viewport height (e.g., on terminal resize).
    pub fn set_viewport_height(&mut self, height: u16) {
        self.viewport_height = height;
        self.visible_count = height as usize;
        if self.sticky {
            self.scroll_to_bottom();
        } else {
            self.offset = self.offset.min(self.max_scroll_offset());
        }
    }

    fn max_scroll_offset(&self) -> usize {
        self.total_items.saturating_sub(self.visible_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overlaps(a: Rect, b: Rect) -> bool {
        a.x < b.x + b.width && a.x + a.width > b.x && a.y < b.y + b.height && a.y + a.height > b.y
    }

    #[test]
    fn auxiliary_panels_use_right_rail_on_wide_terminals() {
        let layout = split_auxiliary_panels(Rect::new(0, 0, 120, 30), 3, 2);
        let task = layout.task_list.expect("task panel");
        let teammates = layout.teammates.expect("teammate panel");

        assert!(layout.messages.width < 120);
        assert!(!overlaps(layout.messages, task));
        assert!(!overlaps(layout.messages, teammates));
        assert!(!overlaps(task, teammates));
    }

    #[test]
    fn auxiliary_panels_stack_below_messages_on_narrow_terminals() {
        let layout = split_auxiliary_panels(Rect::new(0, 0, 80, 28), 4, 1);
        let task = layout.task_list.expect("task panel");
        let teammates = layout.teammates.expect("teammate panel");

        assert_eq!(layout.messages.width, 80);
        assert!(layout.messages.height < 28);
        assert_eq!(task.y, layout.messages.bottom());
        assert!(teammates.y >= task.bottom());
        assert!(!overlaps(layout.messages, task));
        assert!(!overlaps(layout.messages, teammates));
        assert!(!overlaps(task, teammates));
    }

    #[test]
    fn auxiliary_panels_are_skipped_in_tiny_content_area() {
        let layout = split_auxiliary_panels(Rect::new(0, 0, 70, 9), 6, 3);

        assert_eq!(layout.messages, Rect::new(0, 0, 70, 9));
        assert!(layout.task_list.is_none());
        assert!(layout.teammates.is_none());
    }

    #[test]
    fn padding_and_scroll_math_saturate_instead_of_overflowing() {
        let padded = pad(Rect::new(10, 20, 8, 4), u16::MAX, u16::MAX, 12, 9);
        assert_eq!(padded.width, 0);
        assert_eq!(padded.height, 0);

        let modal = ModalLayout::new(Rect::new(u16::MAX - 2, u16::MAX - 2, 4, 4), 3, u16::MAX);
        assert_eq!(modal.content.width, 0);

        let mut scroll = VirtualScroll::new(5);
        scroll.set_total_items(20);
        scroll.sticky = false;
        scroll.offset = usize::MAX - 1;
        scroll.scroll_down(usize::MAX);
        assert_eq!(scroll.offset, 15);
    }

    #[test]
    fn viewport_resize_clamps_manual_scroll_without_restoring_sticky() {
        let mut scroll = VirtualScroll::new(5);
        scroll.set_total_items(100);
        scroll.scroll_up(20);
        assert!(!scroll.sticky);

        scroll.set_viewport_height(96);

        assert_eq!(scroll.visible_count, 96);
        assert_eq!(scroll.offset, 4);
        assert!(!scroll.sticky);
    }

    #[test]
    fn non_scrollable_scroll_up_preserves_sticky_bottom() {
        let mut scroll = VirtualScroll::new(20);
        scroll.set_total_items(3);
        assert!(scroll.sticky);
        assert_eq!(scroll.offset, 0);

        scroll.scroll_up(5);

        assert_eq!(scroll.offset, 0);
        assert!(
            scroll.sticky,
            "wheel/PageUp on short transcript must not break sticky-bottom"
        );
    }

    #[test]
    fn sticky_scroll_up_uses_live_bottom_offset() {
        let mut scroll = VirtualScroll::new(20);
        scroll.set_total_items(100);
        scroll.sticky = true;
        scroll.offset = 0;

        scroll.scroll_up(5);

        assert_eq!(scroll.offset, 75);
        assert!(!scroll.sticky);
    }
}
