//! Scroll box — scrollable container (ScrollBox.tsx).

#[derive(Debug, Clone)]
pub struct ScrollBoxState {
    pub scroll_offset: u32,
    pub viewport_height: u32,
    pub content_height: u32,
    pub auto_scroll_to_bottom: bool,
    pub is_scrolled_to_bottom: bool,
}

impl ScrollBoxState {
    pub fn new(viewport_height: u32) -> Self {
        Self { scroll_offset: 0, viewport_height, content_height: 0, auto_scroll_to_bottom: true, is_scrolled_to_bottom: true }
    }
    pub fn set_content_height(&mut self, height: u32) {
        self.content_height = height;
        if self.auto_scroll_to_bottom { self.scroll_to_bottom(); }
    }
    pub fn scroll_up(&mut self, lines: u32) { self.scroll_offset = self.scroll_offset.saturating_sub(lines); self.is_scrolled_to_bottom = false; }
    pub fn scroll_down(&mut self, lines: u32) {
        self.scroll_offset = (self.scroll_offset + lines).min(self.max_scroll());
        self.is_scrolled_to_bottom = self.scroll_offset >= self.max_scroll();
    }
    pub fn scroll_to_bottom(&mut self) { self.scroll_offset = self.max_scroll(); self.is_scrolled_to_bottom = true; }
    pub fn scroll_to_top(&mut self) { self.scroll_offset = 0; self.is_scrolled_to_bottom = false; }
    pub fn max_scroll(&self) -> u32 { self.content_height.saturating_sub(self.viewport_height) }
    pub fn visible_range(&self) -> (u32, u32) { (self.scroll_offset, self.scroll_offset + self.viewport_height) }
}
impl Default for ScrollBoxState { fn default() -> Self { Self::new(24) } }

/// TS `ScrollBox` exports `type ScrollBoxProps`.
pub type ScrollBoxProps = ScrollBoxState;
/// TS `ScrollBox` also exports `type Props`.
pub type Props = ScrollBoxState;
