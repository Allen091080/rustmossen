//! Virtual Scroll hook (useVirtualScroll.ts).
//! Manages virtual scrolling for long message lists.

#[derive(Debug, Clone)]
pub struct VirtualScrollState {
    pub total_items: usize,
    pub viewport_height: usize,
    pub scroll_offset: usize,
    pub item_heights: Vec<u16>,
    pub total_height: u64,
    pub anchor_index: Option<usize>,
}

impl VirtualScrollState {
    pub fn new(viewport_height: usize) -> Self {
        Self {
            total_items: 0,
            viewport_height,
            scroll_offset: 0,
            item_heights: Vec::new(),
            total_height: 0,
            anchor_index: None,
        }
    }
    pub fn set_items(&mut self, count: usize, heights: Vec<u16>) {
        self.total_items = count;
        self.total_height = heights.iter().map(|h| *h as u64).sum();
        self.item_heights = heights;
    }
    pub fn scroll_to(&mut self, offset: usize) {
        self.scroll_offset = offset.min(self.max_scroll());
    }
    pub fn scroll_by(&mut self, delta: i32) {
        let new = (self.scroll_offset as i32 + delta).max(0) as usize;
        self.scroll_to(new);
    }
    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = self.max_scroll();
    }
    pub fn scroll_to_top(&mut self) {
        self.scroll_offset = 0;
    }
    pub fn max_scroll(&self) -> usize {
        self.total_height
            .saturating_sub(self.viewport_height as u64) as usize
    }
    pub fn visible_range(&self) -> (usize, usize) {
        let mut acc = 0u64;
        let mut start = 0;
        let mut end = self.total_items;
        for (i, h) in self.item_heights.iter().enumerate() {
            if acc + *h as u64 > self.scroll_offset as u64 && start == 0 && i > 0 {
                start = i;
            }
            if acc > (self.scroll_offset + self.viewport_height) as u64 {
                end = i;
                break;
            }
            acc += *h as u64;
        }
        (start, end.min(self.total_items))
    }
    pub fn is_at_bottom(&self) -> bool {
        self.scroll_offset >= self.max_scroll().saturating_sub(5)
    }
}
impl Default for VirtualScrollState {
    fn default() -> Self {
        Self::new(24)
    }
}

/// Result returned by the virtual scroll computation.
///
/// TS source: `export type VirtualScrollResult`. Models the same shape used
/// by the React hook: the mounted-range slice indices, spacer heights,
/// cumulative offsets, and per-item layout queries.
#[derive(Debug, Clone, Default)]
pub struct VirtualScrollResult {
    /// `[start, end)` half-open slice of items to render.
    pub range: (usize, usize),
    /// Spacer height (rows) before the first rendered item.
    pub top_spacer: u32,
    /// Spacer height (rows) after the last rendered item.
    pub bottom_spacer: u32,
    /// Cumulative y-offset of each item in list-wrapper coords; the
    /// trailing entry is the total list height.
    /// `offsets[i]` = rows above item i, `offsets[n]` = totalHeight.
    pub offsets: Vec<u32>,
    /// Measured Yoga heights, keyed by index. `None` = not measured yet.
    pub item_heights: Vec<Option<u16>>,
    /// Index of the most recent `scrollToIndex` target, if any.
    pub scroll_target: Option<usize>,
}

impl VirtualScrollResult {
    pub fn new(range: (usize, usize), top_spacer: u32, bottom_spacer: u32) -> Self {
        Self {
            range,
            top_spacer,
            bottom_spacer,
            offsets: Vec::new(),
            item_heights: Vec::new(),
            scroll_target: None,
        }
    }

    /// Read offset for item at `index`, returning total height past the end.
    pub fn get_item_top(&self, index: usize) -> i32 {
        self.offsets
            .get(index)
            .copied()
            .map(|v| v as i32)
            .unwrap_or(-1)
    }

    /// Measured Yoga height for item; `None` if not measured.
    pub fn get_item_height(&self, index: usize) -> Option<u16> {
        self.item_heights.get(index).copied().flatten()
    }
}
