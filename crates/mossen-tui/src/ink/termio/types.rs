//! Semantic types for ANSI escape sequences (types.ts).

/// Compare two colour specs for value equality.
pub fn colors_equal(a: &Color, b: &Color) -> bool {
    a == b
}

/// Named colors from the 16-color palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamedColor {
    Black, Red, Green, Yellow, Blue, Magenta, Cyan, White,
    BrightBlack, BrightRed, BrightGreen, BrightYellow,
    BrightBlue, BrightMagenta, BrightCyan, BrightWhite,
}

/// Color specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    Named(NamedColor),
    Indexed(u8),
    Rgb(u8, u8, u8),
    Default,
}

/// Underline style variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnderlineStyle { None, Single, Double, Curly, Dotted, Dashed }

/// Text style attributes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextStyle {
    pub bold: bool, pub dim: bool, pub italic: bool,
    pub underline: UnderlineStyle, pub blink: bool,
    pub inverse: bool, pub hidden: bool, pub strikethrough: bool,
    pub overline: bool, pub fg: Color, pub bg: Color,
    pub underline_color: Color,
}

impl TextStyle {
    pub fn default_style() -> Self {
        Self {
            bold: false, dim: false, italic: false, underline: UnderlineStyle::None,
            blink: false, inverse: false, hidden: false, strikethrough: false,
            overline: false, fg: Color::Default, bg: Color::Default,
            underline_color: Color::Default,
        }
    }
    pub fn styles_equal(a: &Self, b: &Self) -> bool { a == b }
}

impl Default for TextStyle { fn default() -> Self { Self::default_style() } }

/// Cursor direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorDirection { Up, Down, Forward, Back }

/// Cursor action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CursorAction {
    Move { direction: CursorDirection, count: u32 },
    Position { row: u32, col: u32 },
    Column { col: u32 },
    Row { row: u32 },
    Save, Restore, Show, Hide,
    Style { style: CursorStyle, blinking: bool },
    NextLine { count: u32 },
    PrevLine { count: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorStyle { Block, Underline, Bar }

/// Erase action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EraseRegion { ToEnd, ToStart, All, Scrollback }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EraseLineRegion { ToEnd, ToStart, All }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EraseAction {
    Display(EraseRegion),
    Line(EraseLineRegion),
    Chars(u32),
}

/// Scroll action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollAction {
    Up(u32), Down(u32), SetRegion { top: u32, bottom: u32 },
}

/// Mode action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseTrackingMode { Off, Normal, Button, Any }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeAction {
    AlternateScreen(bool), BracketedPaste(bool),
    MouseTracking(MouseTrackingMode), FocusEvents(bool),
}

/// Link action (OSC 8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkAction {
    Start { url: String, params: Vec<(String, String)> },
    End,
}

/// Title action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TitleAction {
    WindowTitle(String), IconName(String), Both(String),
}

/// Tab status action.
#[derive(Debug, Clone, PartialEq)]
pub struct TabStatusAction {
    pub indicator: Option<Option<Color>>,
    pub status: Option<Option<String>>,
    pub status_color: Option<Option<Color>>,
}

/// A grapheme with display width.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grapheme {
    pub value: String,
    pub width: u8,
}

/// Text segment with styling.
#[derive(Debug, Clone, PartialEq)]
pub struct TextSegment {
    pub text: String,
    pub style: TextStyle,
}

/// All possible parsed actions.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Text { graphemes: Vec<Grapheme>, style: TextStyle },
    Cursor(CursorAction),
    Erase(EraseAction),
    Scroll(ScrollAction),
    Mode(ModeAction),
    Link(LinkAction),
    Title(TitleAction),
    TabStatus(TabStatusAction),
    Sgr(String),
    Bell,
    Reset,
    Unknown(String),
}
