#!/usr/bin/env python3
"""Generate ink/termio/*.rs files - ANSI terminal I/O."""
import os

BASE = "/Users/allen/Documents/rustmossen/crates/mossen-tui/src/ink/termio"
files = []

files.append(("types.rs", '''//! Semantic types for ANSI escape sequences (types.ts).

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
'''))

files.append(("ansi.rs", '''//! ANSI control characters and constants (ansi.ts).

/// C0 control characters.
pub struct C0;
impl C0 {
    pub const NUL: u8 = 0x00;
    pub const SOH: u8 = 0x01;
    pub const STX: u8 = 0x02;
    pub const ETX: u8 = 0x03;
    pub const EOT: u8 = 0x04;
    pub const ENQ: u8 = 0x05;
    pub const ACK: u8 = 0x06;
    pub const BEL: u8 = 0x07;
    pub const BS: u8 = 0x08;
    pub const HT: u8 = 0x09;
    pub const LF: u8 = 0x0A;
    pub const VT: u8 = 0x0B;
    pub const FF: u8 = 0x0C;
    pub const CR: u8 = 0x0D;
    pub const SO: u8 = 0x0E;
    pub const SI: u8 = 0x0F;
    pub const ESC: u8 = 0x1B;
    pub const DEL: u8 = 0x7F;
}

/// ESC sequence type classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscType { Csi, Osc, Dcs, Apc, Ss3, Other }

/// Classify the byte after ESC.
pub fn esc_type(byte: u8) -> EscType {
    match byte {
        b'[' => EscType::Csi,
        b']' => EscType::Osc,
        b'P' => EscType::Dcs,
        b'_' => EscType::Apc,
        b'O' => EscType::Ss3,
        _ => EscType::Other,
    }
}

/// Check if byte is valid ESC sequence final byte.
pub fn is_esc_final(byte: u8) -> bool {
    (0x40..=0x7E).contains(&byte)
}

/// Check if byte is a C0 control character.
pub fn is_c0(byte: u8) -> bool {
    byte < 0x20 || byte == C0::DEL
}
'''))

files.append(("csi.rs", '''//! CSI (Control Sequence Introducer) parsing (csi.ts).

/// CSI final byte constants.
pub struct CSI;
impl CSI {
    pub const CUU: u8 = b'A'; // Cursor Up
    pub const CUD: u8 = b'B'; // Cursor Down
    pub const CUF: u8 = b'C'; // Cursor Forward
    pub const CUB: u8 = b'D'; // Cursor Back
    pub const CNL: u8 = b'E'; // Cursor Next Line
    pub const CPL: u8 = b'F'; // Cursor Previous Line
    pub const CHA: u8 = b'G'; // Cursor Horizontal Absolute
    pub const CUP: u8 = b'H'; // Cursor Position
    pub const ED: u8 = b'J';  // Erase in Display
    pub const EL: u8 = b'K';  // Erase in Line
    pub const SU: u8 = b'S';  // Scroll Up
    pub const SD: u8 = b'T';  // Scroll Down
    pub const SGR: u8 = b'm'; // Select Graphic Rendition
    pub const DSR: u8 = b'n'; // Device Status Report
    pub const DECSTBM: u8 = b'r'; // Set Top and Bottom Margins
    pub const SM: u8 = b'h';  // Set Mode
    pub const RM: u8 = b'l';  // Reset Mode
    pub const DECSET: u8 = b'h'; // DEC Private Mode Set
    pub const DECRST: u8 = b'l'; // DEC Private Mode Reset
}

/// Cursor style constants for DECSCUSR.
pub const CURSOR_STYLES: [(u8, &str, bool); 6] = [
    (1, "block", true), (2, "block", false),
    (3, "underline", true), (4, "underline", false),
    (5, "bar", true), (6, "bar", false),
];

/// Erase display region mapping.
pub const ERASE_DISPLAY: [(u8, &str); 4] = [
    (0, "toEnd"), (1, "toStart"), (2, "all"), (3, "scrollback"),
];

/// Erase line region mapping.
pub const ERASE_LINE_REGION: [(u8, &str); 3] = [
    (0, "toEnd"), (1, "toStart"), (2, "all"),
];

/// Check if byte is a valid CSI parameter byte.
pub fn is_csi_param(byte: u8) -> bool {
    (0x30..=0x3F).contains(&byte)
}

/// Check if byte is a valid CSI intermediate byte.
pub fn is_csi_intermediate(byte: u8) -> bool {
    (0x20..=0x2F).contains(&byte)
}

/// Check if byte is a valid CSI final byte.
pub fn is_csi_final(byte: u8) -> bool {
    (0x40..=0x7E).contains(&byte)
}

/// Parse CSI parameter string into numbers.
pub fn parse_csi_params(param_str: &str) -> Vec<u32> {
    if param_str.is_empty() { return Vec::new(); }
    param_str.split(|c| c == ';' || c == ':')
        .map(|s| if s.is_empty() { 0 } else { s.parse().unwrap_or(0) })
        .collect()
}
'''))

files.append(("dec.rs", '''//! DEC private mode constants (dec.ts).

/// DEC private mode numbers.
pub struct DEC;
impl DEC {
    pub const CURSOR_KEYS: u16 = 1;
    pub const ORIGIN: u16 = 6;
    pub const AUTO_WRAP: u16 = 7;
    pub const CURSOR_VISIBLE: u16 = 25;
    pub const MOUSE_NORMAL: u16 = 1000;
    pub const MOUSE_BUTTON: u16 = 1002;
    pub const MOUSE_ANY: u16 = 1003;
    pub const FOCUS_EVENT: u16 = 1004;
    pub const MOUSE_SGR: u16 = 1006;
    pub const ALTERNATE_SCREEN: u16 = 1049;
    pub const BRACKETED_PASTE: u16 = 2004;
}

/// Map DEC mode number to semantic action.
pub fn dec_mode_to_action(mode: u16, enabled: bool) -> Option<super::types::ModeAction> {
    use super::types::{ModeAction, MouseTrackingMode};
    match mode {
        DEC::ALTERNATE_SCREEN => Some(ModeAction::AlternateScreen(enabled)),
        DEC::BRACKETED_PASTE => Some(ModeAction::BracketedPaste(enabled)),
        DEC::FOCUS_EVENT => Some(ModeAction::FocusEvents(enabled)),
        DEC::MOUSE_NORMAL => Some(ModeAction::MouseTracking(if enabled { MouseTrackingMode::Normal } else { MouseTrackingMode::Off })),
        DEC::MOUSE_BUTTON => Some(ModeAction::MouseTracking(if enabled { MouseTrackingMode::Button } else { MouseTrackingMode::Off })),
        DEC::MOUSE_ANY => Some(ModeAction::MouseTracking(if enabled { MouseTrackingMode::Any } else { MouseTrackingMode::Off })),
        _ => None,
    }
}
'''))

files.append(("esc.rs", '''//! ESC sequence parsing (esc.ts).

use super::types::Action;

/// Parse a simple ESC sequence (ESC + one byte).
pub fn parse_esc(final_byte: u8) -> Option<Action> {
    match final_byte {
        b'c' => Some(Action::Reset),
        b'7' => Some(Action::Cursor(super::types::CursorAction::Save)),
        b'8' => Some(Action::Cursor(super::types::CursorAction::Restore)),
        b'D' => Some(Action::Scroll(super::types::ScrollAction::Up(1))),
        b'M' => Some(Action::Scroll(super::types::ScrollAction::Down(1))),
        b'E' => Some(Action::Cursor(super::types::CursorAction::NextLine { count: 1 })),
        _ => None,
    }
}
'''))

files.append(("osc.rs", '''//! OSC (Operating System Command) parsing (osc.ts).

use super::types::{Action, Color, LinkAction, NamedColor, TabStatusAction, TitleAction};

/// Parse an OSC sequence payload.
pub fn parse_osc(payload: &str) -> Option<Action> {
    let (cmd_str, rest) = payload.split_once(';').unwrap_or((payload, ""));
    let cmd: u32 = cmd_str.parse().unwrap_or(0);

    match cmd {
        0 => Some(Action::Title(TitleAction::Both(rest.to_string()))),
        1 => Some(Action::Title(TitleAction::IconName(rest.to_string()))),
        2 => Some(Action::Title(TitleAction::WindowTitle(rest.to_string()))),
        8 => parse_osc8_link(rest),
        21337 => parse_osc_tab_status(rest),
        _ => None,
    }
}

/// Parse OSC 8 hyperlink.
fn parse_osc8_link(payload: &str) -> Option<Action> {
    let (params_str, url) = payload.split_once(';').unwrap_or(("", payload));
    if url.is_empty() {
        Some(Action::Link(LinkAction::End))
    } else {
        let params: Vec<(String, String)> = params_str.split(':')
            .filter_map(|p| p.split_once('=').map(|(k, v)| (k.to_string(), v.to_string())))
            .collect();
        Some(Action::Link(LinkAction::Start { url: url.to_string(), params }))
    }
}

/// Parse OSC 21337 tab status.
fn parse_osc_tab_status(payload: &str) -> Option<Action> {
    let mut action = TabStatusAction { indicator: None, status: None, status_color: None };
    for part in payload.split(';') {
        if let Some((key, value)) = part.split_once('=') {
            match key {
                "indicator" => action.indicator = Some(parse_color_value(value)),
                "status" => action.status = Some(if value.is_empty() { None } else { Some(value.to_string()) }),
                "statusColor" => action.status_color = Some(parse_color_value(value)),
                _ => {}
            }
        } else {
            // Bare key = clear
            match part {
                "indicator" => action.indicator = Some(None),
                "status" => action.status = Some(None),
                "statusColor" => action.status_color = Some(None),
                _ => {}
            }
        }
    }
    Some(Action::TabStatus(action))
}

fn parse_color_value(value: &str) -> Option<Color> {
    if value.is_empty() { return None; }
    if value.starts_with('#') && value.len() == 7 {
        let r = u8::from_str_radix(&value[1..3], 16).unwrap_or(0);
        let g = u8::from_str_radix(&value[3..5], 16).unwrap_or(0);
        let b = u8::from_str_radix(&value[5..7], 16).unwrap_or(0);
        return Some(Color::Rgb(r, g, b));
    }
    match value {
        "red" => Some(Color::Named(NamedColor::Red)),
        "green" => Some(Color::Named(NamedColor::Green)),
        "yellow" => Some(Color::Named(NamedColor::Yellow)),
        "blue" => Some(Color::Named(NamedColor::Blue)),
        _ => None,
    }
}
'''))

files.append(("sgr.rs", '''//! SGR (Select Graphic Rendition) parsing (sgr.ts).

use super::types::{Color, NamedColor, TextStyle, UnderlineStyle};

/// Apply SGR parameters to a text style.
pub fn apply_sgr(style: &mut TextStyle, params: &[u32]) {
    let mut i = 0;
    while i < params.len() {
        let p = params[i];
        match p {
            0 => *style = TextStyle::default_style(),
            1 => style.bold = true,
            2 => style.dim = true,
            3 => style.italic = true,
            4 => {
                // Check for extended underline (4:x)
                if i + 1 < params.len() && params[i + 1] <= 5 {
                    style.underline = match params[i + 1] {
                        0 => UnderlineStyle::None,
                        1 => UnderlineStyle::Single,
                        2 => UnderlineStyle::Double,
                        3 => UnderlineStyle::Curly,
                        4 => UnderlineStyle::Dotted,
                        5 => UnderlineStyle::Dashed,
                        _ => UnderlineStyle::Single,
                    };
                    i += 1;
                } else {
                    style.underline = UnderlineStyle::Single;
                }
            }
            5 => style.blink = true,
            7 => style.inverse = true,
            8 => style.hidden = true,
            9 => style.strikethrough = true,
            21 => style.underline = UnderlineStyle::Double,
            22 => { style.bold = false; style.dim = false; }
            23 => style.italic = false,
            24 => style.underline = UnderlineStyle::None,
            25 => style.blink = false,
            27 => style.inverse = false,
            28 => style.hidden = false,
            29 => style.strikethrough = false,
            30..=37 => style.fg = Color::Named(named_from_sgr(p - 30)),
            38 => { i += parse_extended_color(params, i, &mut style.fg); }
            39 => style.fg = Color::Default,
            40..=47 => style.bg = Color::Named(named_from_sgr(p - 40)),
            48 => { i += parse_extended_color(params, i, &mut style.bg); }
            49 => style.bg = Color::Default,
            53 => style.overline = true,
            55 => style.overline = false,
            58 => { i += parse_extended_color(params, i, &mut style.underline_color); }
            59 => style.underline_color = Color::Default,
            90..=97 => style.fg = Color::Named(bright_named_from_sgr(p - 90)),
            100..=107 => style.bg = Color::Named(bright_named_from_sgr(p - 100)),
            _ => {}
        }
        i += 1;
    }
}

fn named_from_sgr(idx: u32) -> NamedColor {
    match idx {
        0 => NamedColor::Black, 1 => NamedColor::Red, 2 => NamedColor::Green,
        3 => NamedColor::Yellow, 4 => NamedColor::Blue, 5 => NamedColor::Magenta,
        6 => NamedColor::Cyan, _ => NamedColor::White,
    }
}

fn bright_named_from_sgr(idx: u32) -> NamedColor {
    match idx {
        0 => NamedColor::BrightBlack, 1 => NamedColor::BrightRed, 2 => NamedColor::BrightGreen,
        3 => NamedColor::BrightYellow, 4 => NamedColor::BrightBlue, 5 => NamedColor::BrightMagenta,
        6 => NamedColor::BrightCyan, _ => NamedColor::BrightWhite,
    }
}

fn parse_extended_color(params: &[u32], base: usize, color: &mut Color) -> usize {
    if base + 1 >= params.len() { return 0; }
    match params[base + 1] {
        5 => {
            // 256-color: 38;5;n
            if base + 2 < params.len() {
                *color = Color::Indexed(params[base + 2] as u8);
                return 2;
            }
            1
        }
        2 => {
            // RGB: 38;2;r;g;b
            if base + 4 < params.len() {
                *color = Color::Rgb(params[base + 2] as u8, params[base + 3] as u8, params[base + 4] as u8);
                return 4;
            }
            1
        }
        _ => 1,
    }
}
'''))

files.append(("tokenize.rs", '''//! Input tokenizer — escape sequence boundary detection (tokenize.ts).

/// A token from the tokenizer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    Text(String),
    Sequence(String),
}

/// Tokenizer state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State { Ground, Escape, EscapeIntermediate, Csi, Ss3, Osc, Dcs, Apc }

/// Streaming tokenizer for terminal input.
#[derive(Debug, Clone)]
pub struct Tokenizer {
    state: State,
    buffer: String,
    x10_mouse: bool,
}

impl Tokenizer {
    pub fn new(x10_mouse: bool) -> Self {
        Self { state: State::Ground, buffer: String::new(), x10_mouse }
    }

    /// Feed input and get resulting tokens.
    pub fn feed(&mut self, input: &str) -> Vec<Token> {
        let mut tokens = Vec::new();
        let mut text_acc = String::new();
        let bytes = input.as_bytes();
        let mut i = 0;

        while i < bytes.len() {
            let byte = bytes[i];
            match self.state {
                State::Ground => {
                    if byte == super::ansi::C0::ESC {
                        if !text_acc.is_empty() {
                            tokens.push(Token::Text(std::mem::take(&mut text_acc)));
                        }
                        self.buffer.clear();
                        self.buffer.push(byte as char);
                        self.state = State::Escape;
                    } else if byte < 0x20 && byte != super::ansi::C0::HT && byte != super::ansi::C0::LF && byte != super::ansi::C0::CR {
                        if !text_acc.is_empty() {
                            tokens.push(Token::Text(std::mem::take(&mut text_acc)));
                        }
                        tokens.push(Token::Sequence(String::from(byte as char)));
                    } else {
                        text_acc.push(byte as char);
                    }
                }
                State::Escape => {
                    self.buffer.push(byte as char);
                    match byte {
                        b'[' => self.state = State::Csi,
                        b']' => self.state = State::Osc,
                        b'O' => self.state = State::Ss3,
                        b'P' => self.state = State::Dcs,
                        b'_' => self.state = State::Apc,
                        0x20..=0x2F => self.state = State::EscapeIntermediate,
                        _ => {
                            tokens.push(Token::Sequence(std::mem::take(&mut self.buffer)));
                            self.state = State::Ground;
                        }
                    }
                }
                State::EscapeIntermediate => {
                    self.buffer.push(byte as char);
                    if (0x30..=0x7E).contains(&byte) {
                        tokens.push(Token::Sequence(std::mem::take(&mut self.buffer)));
                        self.state = State::Ground;
                    }
                }
                State::Csi => {
                    self.buffer.push(byte as char);
                    if super::csi::is_csi_final(byte) {
                        tokens.push(Token::Sequence(std::mem::take(&mut self.buffer)));
                        self.state = State::Ground;
                    }
                }
                State::Ss3 => {
                    self.buffer.push(byte as char);
                    tokens.push(Token::Sequence(std::mem::take(&mut self.buffer)));
                    self.state = State::Ground;
                }
                State::Osc => {
                    if byte == super::ansi::C0::BEL || (byte == b'\\\\' && self.buffer.ends_with('\\x1b')) {
                        if byte != super::ansi::C0::BEL { self.buffer.push(byte as char); }
                        tokens.push(Token::Sequence(std::mem::take(&mut self.buffer)));
                        self.state = State::Ground;
                    } else {
                        self.buffer.push(byte as char);
                    }
                }
                State::Dcs | State::Apc => {
                    if byte == b'\\\\' && self.buffer.ends_with('\\x1b') {
                        self.buffer.push(byte as char);
                        tokens.push(Token::Sequence(std::mem::take(&mut self.buffer)));
                        self.state = State::Ground;
                    } else {
                        self.buffer.push(byte as char);
                    }
                }
            }
            i += 1;
        }

        if !text_acc.is_empty() {
            tokens.push(Token::Text(text_acc));
        }
        tokens
    }

    /// Flush buffered incomplete sequences.
    pub fn flush(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        if !self.buffer.is_empty() {
            tokens.push(Token::Sequence(std::mem::take(&mut self.buffer)));
            self.state = State::Ground;
        }
        tokens
    }

    /// Reset tokenizer state.
    pub fn reset(&mut self) {
        self.state = State::Ground;
        self.buffer.clear();
    }

    /// Get buffered content.
    pub fn buffer(&self) -> &str { &self.buffer }
}

impl Default for Tokenizer { fn default() -> Self { Self::new(false) } }
'''))

files.append(("parser.rs", '''//! Semantic ANSI parser — produces structured actions from input (parser.ts).

use super::ansi::C0;
use super::csi::{self, CSI};
use super::dec::dec_mode_to_action;
use super::esc::parse_esc;
use super::osc::parse_osc;
use super::sgr::apply_sgr;
use super::tokenize::{Token, Tokenizer};
use super::types::*;

/// Streaming ANSI parser that produces semantic actions.
#[derive(Debug, Clone)]
pub struct Parser {
    tokenizer: Tokenizer,
    current_style: TextStyle,
}

impl Parser {
    pub fn new() -> Self {
        Self { tokenizer: Tokenizer::new(false), current_style: TextStyle::default_style() }
    }

    /// Parse input into semantic actions.
    pub fn parse(&mut self, input: &str) -> Vec<Action> {
        let tokens = self.tokenizer.feed(input);
        let mut actions = Vec::new();

        for token in tokens {
            match token {
                Token::Text(text) => {
                    let graphemes = self.segment_graphemes(&text);
                    if !graphemes.is_empty() {
                        actions.push(Action::Text { graphemes, style: self.current_style });
                    }
                }
                Token::Sequence(seq) => {
                    if let Some(action) = self.parse_sequence(&seq) {
                        actions.push(action);
                    }
                }
            }
        }
        actions
    }

    /// Parse a single escape sequence into an action.
    fn parse_sequence(&mut self, seq: &str) -> Option<Action> {
        let bytes = seq.as_bytes();
        if bytes.is_empty() { return None; }

        // Single control character
        if bytes.len() == 1 {
            return match bytes[0] {
                C0::BEL => Some(Action::Bell),
                _ => None,
            };
        }

        // Must start with ESC
        if bytes[0] != C0::ESC { return None; }
        if bytes.len() < 2 { return None; }

        match bytes[1] {
            b'[' => self.parse_csi(seq),
            b']' => self.parse_osc_seq(seq),
            b'O' => self.parse_ss3(seq),
            _ => parse_esc(bytes[1]),
        }
    }

    fn parse_csi(&mut self, seq: &str) -> Option<Action> {
        let inner = &seq[2..];
        if inner.is_empty() { return None; }

        let final_byte = inner.as_bytes()[inner.len() - 1];
        let before_final = &inner[..inner.len() - 1];

        let (private_mode, param_str) = if !before_final.is_empty() && "?>=".contains(before_final.chars().next().unwrap_or(' ')) {
            (Some(before_final.chars().next().unwrap()), &before_final[1..])
        } else {
            (None, before_final)
        };

        let params = csi::parse_csi_params(param_str);

        // Private mode sequences (? prefix)
        if let Some('?') = private_mode {
            let mode = params.first().copied().unwrap_or(0) as u16;
            let enabled = final_byte == CSI::SM;
            if let Some(action) = dec_mode_to_action(mode, enabled) {
                return Some(Action::Mode(action));
            }
            // Cursor style (DECSCUSR)
            if final_byte == b'q' {
                let style_num = params.first().copied().unwrap_or(0);
                let (cursor_style, blinking) = match style_num {
                    0 | 1 => (CursorStyle::Block, true),
                    2 => (CursorStyle::Block, false),
                    3 => (CursorStyle::Underline, true),
                    4 => (CursorStyle::Underline, false),
                    5 => (CursorStyle::Bar, true),
                    6 => (CursorStyle::Bar, false),
                    _ => (CursorStyle::Block, true),
                };
                return Some(Action::Cursor(CursorAction::Style { style: cursor_style, blinking }));
            }
            return Some(Action::Unknown(seq.to_string()));
        }

        // Standard CSI sequences
        let count = params.first().copied().unwrap_or(1).max(1);
        match final_byte {
            CSI::CUU => Some(Action::Cursor(CursorAction::Move { direction: CursorDirection::Up, count })),
            CSI::CUD => Some(Action::Cursor(CursorAction::Move { direction: CursorDirection::Down, count })),
            CSI::CUF => Some(Action::Cursor(CursorAction::Move { direction: CursorDirection::Forward, count })),
            CSI::CUB => Some(Action::Cursor(CursorAction::Move { direction: CursorDirection::Back, count })),
            CSI::CNL => Some(Action::Cursor(CursorAction::NextLine { count })),
            CSI::CPL => Some(Action::Cursor(CursorAction::PrevLine { count })),
            CSI::CHA => Some(Action::Cursor(CursorAction::Column { col: count })),
            CSI::CUP => {
                let row = params.first().copied().unwrap_or(1);
                let col = params.get(1).copied().unwrap_or(1);
                Some(Action::Cursor(CursorAction::Position { row, col }))
            }
            CSI::ED => {
                let region = match params.first().copied().unwrap_or(0) {
                    0 => EraseRegion::ToEnd, 1 => EraseRegion::ToStart,
                    2 => EraseRegion::All, 3 => EraseRegion::Scrollback,
                    _ => EraseRegion::ToEnd,
                };
                Some(Action::Erase(EraseAction::Display(region)))
            }
            CSI::EL => {
                let region = match params.first().copied().unwrap_or(0) {
                    0 => EraseLineRegion::ToEnd, 1 => EraseLineRegion::ToStart,
                    _ => EraseLineRegion::All,
                };
                Some(Action::Erase(EraseAction::Line(region)))
            }
            CSI::SU => Some(Action::Scroll(ScrollAction::Up(count))),
            CSI::SD => Some(Action::Scroll(ScrollAction::Down(count))),
            CSI::SGR => {
                let sgr_params = if params.is_empty() { vec![0] } else { params };
                apply_sgr(&mut self.current_style, &sgr_params);
                None // Style change is tracked internally
            }
            _ => Some(Action::Unknown(seq.to_string())),
        }
    }

    fn parse_osc_seq(&self, seq: &str) -> Option<Action> {
        // Strip ESC ] prefix and BEL/ST suffix
        let payload = if seq.starts_with("\x1b]") {
            let end = if seq.ends_with("\x07") { seq.len() - 1 }
                else if seq.ends_with("\x1b\\\\") { seq.len() - 2 }
                else { seq.len() };
            &seq[2..end]
        } else {
            return None;
        };
        parse_osc(payload)
    }

    fn parse_ss3(&self, seq: &str) -> Option<Action> {
        if seq.len() < 3 { return None; }
        let final_byte = seq.as_bytes()[2];
        match final_byte {
            b'A' => Some(Action::Cursor(CursorAction::Move { direction: CursorDirection::Up, count: 1 })),
            b'B' => Some(Action::Cursor(CursorAction::Move { direction: CursorDirection::Down, count: 1 })),
            b'C' => Some(Action::Cursor(CursorAction::Move { direction: CursorDirection::Forward, count: 1 })),
            b'D' => Some(Action::Cursor(CursorAction::Move { direction: CursorDirection::Back, count: 1 })),
            _ => None,
        }
    }

    fn segment_graphemes(&self, text: &str) -> Vec<Grapheme> {
        use unicode_segmentation::UnicodeSegmentation;
        text.graphemes(true).map(|g| {
            let width = if g.len() > 4 || g.chars().any(|c| {
                let cp = c as u32;
                (0x1100..=0x115F).contains(&cp) || (0x2E80..=0x9FFF).contains(&cp) ||
                (0xAC00..=0xD7A3).contains(&cp) || (0xF900..=0xFAFF).contains(&cp) ||
                (0x1F300..=0x1FAFF).contains(&cp)
            }) { 2 } else { 1 };
            Grapheme { value: g.to_string(), width }
        }).collect()
    }

    /// Reset parser state.
    pub fn reset(&mut self) {
        self.tokenizer.reset();
        self.current_style = TextStyle::default_style();
    }

    /// Get current style.
    pub fn current_style(&self) -> TextStyle { self.current_style }
}

impl Default for Parser { fn default() -> Self { Self::new() } }
'''))

for fname, content in files:
    path = os.path.join(BASE, fname)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, 'w') as f:
        f.write(content)
    print(f"Created {fname}")

print(f"\nTotal: {len(files)} files created")
