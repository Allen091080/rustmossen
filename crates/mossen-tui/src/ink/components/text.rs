//! Text component — styled text rendering (Text.tsx).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextWrap { Wrap, Truncate, TruncateStart, TruncateMiddle, TruncateEnd }

#[derive(Debug, Clone)]
pub struct TextStyle {
    pub bold: bool, pub italic: bool, pub underline: bool, pub strikethrough: bool,
    pub dim: bool, pub inverse: bool, pub color: Option<String>, pub bg_color: Option<String>,
    pub wrap: TextWrap,
}

impl Default for TextStyle {
    fn default() -> Self { Self { bold: false, italic: false, underline: false, strikethrough: false, dim: false, inverse: false, color: None, bg_color: None, wrap: TextWrap::Wrap } }
}

#[derive(Debug, Clone)]
pub struct TextComponentState {
    pub content: String,
    pub style: TextStyle,
}

impl TextComponentState {
    pub fn new(content: &str) -> Self { Self { content: content.to_string(), style: TextStyle::default() } }
    pub fn with_style(mut self, style: TextStyle) -> Self { self.style = style; self }
    pub fn set_content(&mut self, content: &str) { self.content = content.to_string(); }
    pub fn rendered_width(&self) -> usize { unicode_width::UnicodeWidthStr::width(self.content.as_str()) }
}
impl Default for TextComponentState { fn default() -> Self { Self::new("") } }

/// TS `Text` exports `type Props`. Mirrors the same shape as `TextStyle` + content.
pub type Props = TextComponentState;
