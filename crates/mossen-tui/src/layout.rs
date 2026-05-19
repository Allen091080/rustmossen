//! Layout system for the TUI layer.
//!
//! Translates Ink's Flexbox layout (yoga.ts) into ratatui Layout utilities.
//! Provides the FullscreenLayout equivalent and common layout helpers.

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

impl AppLayout {
    /// Compute the standard fullscreen layout from terminal area.
    ///
    /// Equivalent to FullscreenLayout.tsx's fullscreen path:
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
    let width = area.width.saturating_sub(left + right);
    let height = area.height.saturating_sub(top + bottom);
    Rect::new(x, y, width, height)
}

/// Center a block of given size within an area.
pub fn center(area: Rect, width: u16, height: u16) -> Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
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
        let content_y = divider_y + 1;
        let content_height = area.height.saturating_sub(content_y);

        Self {
            peek: Rect::new(area.x, area.y, area.width, divider_y),
            divider: Rect::new(area.x, area.y + divider_y, area.width, 1),
            content: Rect::new(
                area.x + padding_x,
                area.y + content_y,
                area.width.saturating_sub(padding_x * 2),
                content_height,
            ),
        }
    }
}

/// Virtual scroll state for large message lists.
///
/// Translates useVirtualScroll.ts — manages which items are visible
/// in the viewport and handles scroll position.
pub struct VirtualScroll {
    /// Total number of items
    pub total_items: usize,
    /// Index of first visible item
    pub offset: usize,
    /// Number of visible items
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
            visible_count: 0,
            sticky: true,
            viewport_height,
        }
    }

    /// Update total items and auto-scroll if sticky.
    pub fn set_total_items(&mut self, count: usize) {
        self.total_items = count;
        if self.sticky {
            self.scroll_to_bottom();
        }
    }

    /// Scroll to bottom and re-enable sticky.
    pub fn scroll_to_bottom(&mut self) {
        self.sticky = true;
        self.offset = self.total_items.saturating_sub(self.visible_count);
    }

    /// Scroll up by n items, disabling sticky.
    pub fn scroll_up(&mut self, n: usize) {
        self.sticky = false;
        self.offset = self.offset.saturating_sub(n);
    }

    /// Scroll down by n items, re-enabling sticky if at bottom.
    pub fn scroll_down(&mut self, n: usize) {
        self.offset = (self.offset + n).min(self.total_items.saturating_sub(self.visible_count));
        if self.offset >= self.total_items.saturating_sub(self.visible_count) {
            self.sticky = true;
        }
    }

    /// Update viewport height (e.g., on terminal resize).
    pub fn set_viewport_height(&mut self, height: u16) {
        self.viewport_height = height;
        self.visible_count = height as usize;
        if self.sticky {
            self.scroll_to_bottom();
        }
    }
}
