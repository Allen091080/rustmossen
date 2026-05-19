//! Render placeholder (renderPlaceholder.ts).
//! Provides placeholder text rendering for the input when empty.

#[derive(Debug, Clone)]
pub struct RenderPlaceholderState {
    pub placeholder_text: String,
    pub is_visible: bool,
    pub style: PlaceholderStyle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceholderStyle { Dim, Italic, DimItalic }

impl RenderPlaceholderState {
    pub fn new(text: &str) -> Self {
        Self { placeholder_text: text.to_string(), is_visible: true, style: PlaceholderStyle::DimItalic }
    }
    pub fn should_show(&self, input_value: &str, is_focused: bool) -> bool {
        self.is_visible && input_value.is_empty() && is_focused
    }
    pub fn set_text(&mut self, text: String) { self.placeholder_text = text; }
    pub fn set_visible(&mut self, visible: bool) { self.is_visible = visible; }
    pub fn get_text(&self) -> &str { &self.placeholder_text }
}
impl Default for RenderPlaceholderState { fn default() -> Self { Self::new("Type a message...") } }
