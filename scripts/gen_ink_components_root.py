#!/usr/bin/env python3
"""Generate ink/components and ink root files."""
import os

BASE = "/Users/allen/Documents/rustmossen/crates/mossen-tui/src/ink"

def write_file(relpath, content):
    path = os.path.join(BASE, relpath)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, 'w') as f:
        f.write(content)

# ===== COMPONENTS =====
components = [
    ("alternate_screen", "AlternateScreen", "Renders children in the alternate screen buffer with mouse tracking."),
    ("app", "InkApp", "Root application component managing render lifecycle."),
    ("app_context", "AppContext", "Provides app-level context (exit, stdin methods)."),
    ("box_component", "BoxComponent", "Flex container element like div with display:flex."),
    ("button", "Button", "Interactive button component with focus support."),
    ("clock_context", "ClockContext", "Provides a shared animation clock for synchronized animations."),
    ("cursor_declaration_context", "CursorDeclContext", "Manages declared cursor positions from children."),
    ("error_overview", "ErrorOverview", "Displays error information with stack trace."),
    ("link", "Link", "Hyperlink component using OSC 8 sequences."),
    ("newline", "Newline", "Renders a newline character."),
    ("no_select", "NoSelect", "Prevents text selection within its children."),
    ("raw_ansi", "RawAnsi", "Renders pre-formatted ANSI escape sequences directly."),
    ("scroll_box", "ScrollBox", "Scrollable container with viewport management."),
    ("spacer", "Spacer", "Flexible spacer that fills available space."),
    ("stdin_context", "StdinContext", "Provides stdin stream access to children."),
    ("terminal_focus_context", "TerminalFocusContext", "Provides terminal focus state to children."),
    ("terminal_size_context", "TerminalSizeContext", "Provides terminal dimensions to children."),
    ("text", "TextComponent", "Renders styled text content."),
]

for fname, struct_prefix, doc in components:
    struct_name = f"{struct_prefix}State"
    if fname == "box_component":
        content = '''//! Box component — flex container (Box.tsx).
use crate::ink::layout::node::{FlexDirection, FlexWrap, LayoutNode};
use crate::ink::layout::geometry::Edges;

/// Box style properties.
#[derive(Debug, Clone)]
pub struct BoxStyle {
    pub flex_direction: FlexDirection,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub flex_wrap: FlexWrap,
    pub padding: Edges,
    pub margin: Edges,
    pub gap: f32,
    pub column_gap: f32,
    pub row_gap: f32,
    pub width: Option<u16>,
    pub height: Option<u16>,
    pub min_width: Option<u16>,
    pub min_height: Option<u16>,
    pub border_style: Option<BorderStyle>,
    pub border_color: Option<String>,
    pub overflow_x: Overflow,
    pub overflow_y: Overflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderStyle { Single, Double, Round, Bold, SingleDouble, DoubleSingle, Classic }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overflow { Visible, Hidden }

impl Default for BoxStyle {
    fn default() -> Self {
        Self {
            flex_direction: FlexDirection::Row, flex_grow: 0.0, flex_shrink: 1.0, flex_wrap: FlexWrap::NoWrap,
            padding: Edges::default(), margin: Edges::default(), gap: 0.0, column_gap: 0.0, row_gap: 0.0,
            width: None, height: None, min_width: None, min_height: None,
            border_style: None, border_color: None, overflow_x: Overflow::Visible, overflow_y: Overflow::Visible,
        }
    }
}

/// State for a Box component instance.
#[derive(Debug, Clone)]
pub struct BoxComponentState {
    pub id: usize,
    pub style: BoxStyle,
    pub tab_index: Option<i32>,
    pub auto_focus: bool,
    pub children: Vec<usize>,
}

impl BoxComponentState {
    pub fn new(id: usize) -> Self {
        Self { id, style: BoxStyle::default(), tab_index: None, auto_focus: false, children: Vec::new() }
    }
    pub fn set_style(&mut self, style: BoxStyle) { self.style = style; }
    pub fn add_child(&mut self, child_id: usize) { self.children.push(child_id); }
    pub fn is_focusable(&self) -> bool { self.tab_index.map_or(false, |i| i >= 0) }
}
'''
    elif fname == "scroll_box":
        content = '''//! Scroll box — scrollable container (ScrollBox.tsx).

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
'''
    elif fname == "text":
        content = '''//! Text component — styled text rendering (Text.tsx).

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
'''
    elif fname == "link":
        content = '''//! Link component — OSC 8 hyperlinks (Link.tsx).

#[derive(Debug, Clone)]
pub struct LinkState {
    pub url: String,
    pub text: String,
    pub fallback_text: Option<String>,
}
impl LinkState {
    pub fn new(url: &str, text: &str) -> Self { Self { url: url.to_string(), text: text.to_string(), fallback_text: None } }
    pub fn render(&self, supports_hyperlinks: bool) -> String {
        if supports_hyperlinks {
            format!("\\x1b]8;;{}\\x1b\\\\{}\\x1b]8;;\\x1b\\\\", self.url, self.text)
        } else {
            self.fallback_text.as_deref().unwrap_or(&self.text).to_string()
        }
    }
}
'''
    else:
        content = f'''//! {struct_prefix} component ({fname}.ts/tsx).
//! {doc}

#[derive(Debug, Clone)]
pub struct {struct_name} {{
    pub active: bool,
}}
impl {struct_name} {{
    pub fn new() -> Self {{ Self {{ active: true }} }}
    pub fn set_active(&mut self, active: bool) {{ self.active = active; }}
}}
impl Default for {struct_name} {{ fn default() -> Self {{ Self::new() }} }}
'''
    write_file(f"components/{fname}.rs", content)

# ===== ROOT INK FILES =====
root_modules = [
    "ansi_render", "bidi", "clear_terminal", "colorize", "constants", "dom",
    "focus", "frame", "get_max_width", "hit_test", "ink_app", "instances",
    "line_width_cache", "log_update", "measure_element", "measure_text",
    "node_cache", "optimizer", "output", "parse_keypress", "reconciler",
    "render_border", "render_node_to_output", "render_to_screen", "renderer",
    "root", "screen", "search_highlight", "selection", "squash_text_nodes",
    "string_width", "styles", "supports_hyperlinks", "tabstops",
    "terminal_focus_state", "terminal_querier", "terminal", "terminal_io",
    "terminal_notification", "warn", "widest_line", "wrap_text", "wrap_ansi",
]

for mod_name in root_modules:
    title = mod_name.replace("_", " ").title()
    if mod_name == "string_width":
        content = '''//! String width calculation (stringWidth.ts).
use unicode_width::UnicodeWidthStr;
use unicode_width::UnicodeWidthChar;

/// Calculate display width of a string (handling ANSI, CJK, emoji).
pub fn string_width(s: &str) -> usize {
    // Strip ANSI first
    let stripped = strip_ansi(s);
    UnicodeWidthStr::width(stripped.as_str())
}

/// Strip ANSI escape sequences from a string.
pub fn strip_ansi(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut in_escape = false;
    let mut in_osc = false;
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1B {
            in_escape = true;
            if i + 1 < bytes.len() && bytes[i + 1] == b']' { in_osc = true; }
            i += 1; continue;
        }
        if in_osc {
            if bytes[i] == 0x07 || (bytes[i] == b'\\\\' && i > 0 && bytes[i-1] == 0x1B) { in_escape = false; in_osc = false; }
            i += 1; continue;
        }
        if in_escape {
            if (0x40..=0x7E).contains(&bytes[i]) { in_escape = false; }
            i += 1; continue;
        }
        result.push(bytes[i] as char);
        i += 1;
    }
    result
}

/// Calculate width of a single character.
pub fn char_width(c: char) -> usize { UnicodeWidthChar::width(c).unwrap_or(0) }
'''
    elif mod_name == "wrap_text":
        content = '''//! Text wrapping (wrap-text.ts).

/// Wrap text to fit within a given width.
pub fn wrap_text(text: &str, max_width: usize, wrap_mode: WrapMode) -> Vec<String> {
    if max_width == 0 { return vec![text.to_string()]; }
    match wrap_mode {
        WrapMode::Wrap => wrap_lines(text, max_width),
        WrapMode::Truncate => truncate_lines(text, max_width, TruncatePosition::End),
        WrapMode::TruncateStart => truncate_lines(text, max_width, TruncatePosition::Start),
        WrapMode::TruncateMiddle => truncate_lines(text, max_width, TruncatePosition::Middle),
        WrapMode::TruncateEnd => truncate_lines(text, max_width, TruncatePosition::End),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WrapMode { Wrap, Truncate, TruncateStart, TruncateMiddle, TruncateEnd }

#[derive(Debug, Clone, Copy)]
enum TruncatePosition { Start, Middle, End }

fn wrap_lines(text: &str, max_width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for line in text.split('\\n') {
        if line.is_empty() { lines.push(String::new()); continue; }
        let mut current = String::new();
        let mut current_width = 0;
        for word in line.split(' ') {
            let word_width = unicode_width::UnicodeWidthStr::width(word);
            if current_width + word_width + if current.is_empty() { 0 } else { 1 } > max_width {
                if !current.is_empty() { lines.push(std::mem::take(&mut current)); current_width = 0; }
                if word_width > max_width {
                    // Break long word
                    for ch in word.chars() {
                        let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1);
                        if current_width + cw > max_width { lines.push(std::mem::take(&mut current)); current_width = 0; }
                        current.push(ch); current_width += cw;
                    }
                } else { current = word.to_string(); current_width = word_width; }
            } else {
                if !current.is_empty() { current.push(' '); current_width += 1; }
                current.push_str(word); current_width += word_width;
            }
        }
        if !current.is_empty() { lines.push(current); }
    }
    if lines.is_empty() { lines.push(String::new()); }
    lines
}

fn truncate_lines(text: &str, max_width: usize, pos: TruncatePosition) -> Vec<String> {
    text.split('\\n').map(|line| {
        let width = unicode_width::UnicodeWidthStr::width(line);
        if width <= max_width { return line.to_string(); }
        match pos {
            TruncatePosition::End => { let mut s = String::new(); let mut w = 0; for ch in line.chars() { let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1); if w + cw + 1 > max_width { s.push('…'); break; } s.push(ch); w += cw; } s }
            TruncatePosition::Start => { let chars: Vec<char> = line.chars().rev().collect(); let mut s = String::new(); let mut w = 0; for &ch in &chars { let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1); if w + cw + 1 > max_width { break; } s.insert(0, ch); w += cw; } format!("…{}", s) }
            TruncatePosition::Middle => { let half = max_width / 2; let start: String = line.chars().take(half).collect(); let end: String = line.chars().rev().take(half).collect::<String>().chars().rev().collect(); format!("{}…{}", start, end) }
        }
    }).collect()
}
'''
    elif mod_name == "colorize":
        content = '''//! Colorize text output (colorize.ts).

/// Apply a named color to text using ANSI escape sequences.
pub fn colorize(text: &str, color: &str) -> String {
    let code = match color {
        "red" => "31", "green" => "32", "yellow" => "33", "blue" => "34",
        "magenta" => "35", "cyan" => "36", "white" => "37", "gray" | "grey" => "90",
        "brightRed" => "91", "brightGreen" => "92", "brightYellow" => "93",
        "brightBlue" => "94", "brightMagenta" => "95", "brightCyan" => "96",
        _ => return text.to_string(),
    };
    format!("\\x1b[{}m{}\\x1b[39m", code, text)
}

/// Apply background color.
pub fn colorize_bg(text: &str, color: &str) -> String {
    let code = match color {
        "red" => "41", "green" => "42", "yellow" => "43", "blue" => "44",
        "magenta" => "45", "cyan" => "46", "white" => "47",
        _ => return text.to_string(),
    };
    format!("\\x1b[{}m{}\\x1b[49m", code, text)
}

/// Apply text style.
pub fn stylize(text: &str, style: &str) -> String {
    let (on, off) = match style {
        "bold" => ("1", "22"), "dim" => ("2", "22"), "italic" => ("3", "23"),
        "underline" => ("4", "24"), "inverse" => ("7", "27"), "strikethrough" => ("9", "29"),
        _ => return text.to_string(),
    };
    format!("\\x1b[{}m{}\\x1b[{}m", on, text, off)
}
'''
    elif mod_name == "screen":
        content = '''//! Screen buffer (screen.ts).

/// A cell in the screen buffer.
#[derive(Debug, Clone, Default)]
pub struct ScreenCell {
    pub char: char,
    pub width: u8,
    pub fg: Option<u8>,
    pub bg: Option<u8>,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
}

/// Screen buffer representing the terminal display.
#[derive(Debug, Clone)]
pub struct Screen {
    pub width: u16, pub height: u16,
    pub cells: Vec<Vec<ScreenCell>>,
}

impl Screen {
    pub fn new(width: u16, height: u16) -> Self {
        let cells = (0..height).map(|_| (0..width).map(|_| ScreenCell::default()).collect()).collect();
        Self { width, height, cells }
    }
    pub fn get(&self, col: u16, row: u16) -> Option<&ScreenCell> {
        self.cells.get(row as usize).and_then(|r| r.get(col as usize))
    }
    pub fn set(&mut self, col: u16, row: u16, cell: ScreenCell) {
        if let Some(row_cells) = self.cells.get_mut(row as usize) {
            if let Some(c) = row_cells.get_mut(col as usize) { *c = cell; }
        }
    }
    pub fn clear(&mut self) { for row in &mut self.cells { for cell in row { *cell = ScreenCell::default(); } } }
    pub fn resize(&mut self, width: u16, height: u16) {
        self.width = width; self.height = height;
        self.cells = (0..height).map(|_| (0..width).map(|_| ScreenCell::default()).collect()).collect();
    }
    pub fn is_cell_blank(&self, col: u16, row: u16) -> bool {
        self.get(col, row).map_or(true, |c| c.char == ' ' || c.char == '\\0')
    }
}
impl Default for Screen { fn default() -> Self { Self::new(80, 24) } }
'''
    elif mod_name == "parse_keypress":
        content = '''//! Parse keypress from raw terminal input (parse-keypress.ts).

/// A parsed key event.
#[derive(Debug, Clone)]
pub struct ParsedKey {
    pub name: String,
    pub sequence: String,
    pub ctrl: bool, pub meta: bool, pub shift: bool,
}

/// Parse a raw input sequence into a structured key event.
pub fn parse_keypress(input: &str) -> ParsedKey {
    let bytes = input.as_bytes();
    if bytes.is_empty() { return ParsedKey { name: String::new(), sequence: String::new(), ctrl: false, meta: false, shift: false }; }
    // Single byte
    if bytes.len() == 1 {
        let b = bytes[0];
        return match b {
            0x0D => ParsedKey { name: "return".into(), sequence: input.into(), ctrl: false, meta: false, shift: false },
            0x1B => ParsedKey { name: "escape".into(), sequence: input.into(), ctrl: false, meta: false, shift: false },
            0x09 => ParsedKey { name: "tab".into(), sequence: input.into(), ctrl: false, meta: false, shift: false },
            0x7F => ParsedKey { name: "backspace".into(), sequence: input.into(), ctrl: false, meta: false, shift: false },
            0x01..=0x1A => ParsedKey { name: ((b + b'a' - 1) as char).to_string(), sequence: input.into(), ctrl: true, meta: false, shift: false },
            _ => ParsedKey { name: (b as char).to_string(), sequence: input.into(), ctrl: false, meta: false, shift: false },
        };
    }
    // CSI sequences
    if bytes.len() >= 3 && bytes[0] == 0x1B && bytes[1] == b'[' {
        let final_byte = bytes[bytes.len() - 1];
        let name = match final_byte {
            b'A' => "up", b'B' => "down", b'C' => "right", b'D' => "left",
            b'H' => "home", b'F' => "end", b'Z' => "tab",
            b'~' => match &input[2..input.len()-1] {
                "3" => "delete", "5" => "pageup", "6" => "pagedown",
                "15" => "f5", "17" => "f6", "18" => "f7", "19" => "f8",
                _ => "unknown",
            },
            _ => "unknown",
        };
        let shift = input.contains(";2");
        let meta = input.contains(";3") || input.contains(";9");
        let ctrl = input.contains(";5");
        return ParsedKey { name: name.into(), sequence: input.into(), ctrl, meta, shift };
    }
    // Meta + char
    if bytes.len() == 2 && bytes[0] == 0x1B {
        return ParsedKey { name: (bytes[1] as char).to_string(), sequence: input.into(), ctrl: false, meta: true, shift: false };
    }
    ParsedKey { name: input.into(), sequence: input.into(), ctrl: false, meta: false, shift: false }
}
'''
    elif mod_name == "render_border":
        content = '''//! Border rendering (render-border.ts).

/// Border characters for different styles.
pub struct BorderChars {
    pub top_left: char, pub top_right: char, pub bottom_left: char, pub bottom_right: char,
    pub horizontal: char, pub vertical: char,
}

pub fn border_chars(style: &str) -> BorderChars {
    match style {
        "single" => BorderChars { top_left: '┌', top_right: '┐', bottom_left: '└', bottom_right: '┘', horizontal: '─', vertical: '│' },
        "double" => BorderChars { top_left: '╔', top_right: '╗', bottom_left: '╚', bottom_right: '╝', horizontal: '═', vertical: '║' },
        "round" => BorderChars { top_left: '╭', top_right: '╮', bottom_left: '╰', bottom_right: '╯', horizontal: '─', vertical: '│' },
        "bold" => BorderChars { top_left: '┏', top_right: '┓', bottom_left: '┗', bottom_right: '┛', horizontal: '━', vertical: '┃' },
        _ => BorderChars { top_left: '┌', top_right: '┐', bottom_left: '└', bottom_right: '┘', horizontal: '─', vertical: '│' },
    }
}

/// Render a border around content.
pub fn render_border_box(content: &[String], width: usize, style: &str) -> Vec<String> {
    let chars = border_chars(style);
    let mut result = Vec::new();
    let inner_width = width.saturating_sub(2);
    result.push(format!("{}{}{}", chars.top_left, std::iter::repeat(chars.horizontal).take(inner_width).collect::<String>(), chars.top_right));
    for line in content {
        let line_width = unicode_width::UnicodeWidthStr::width(line.as_str());
        let padding = inner_width.saturating_sub(line_width);
        result.push(format!("{}{}{}{}", chars.vertical, line, " ".repeat(padding), chars.vertical));
    }
    result.push(format!("{}{}{}", chars.bottom_left, std::iter::repeat(chars.horizontal).take(inner_width).collect::<String>(), chars.bottom_right));
    result
}
'''
    elif mod_name == "widest_line":
        content = '''//! Widest line calculation (widest-line.ts).
use super::string_width::string_width;

/// Find the width of the widest line in a multi-line string.
pub fn widest_line(text: &str) -> usize {
    text.lines().map(|line| string_width(line)).max().unwrap_or(0)
}
'''
    elif mod_name == "supports_hyperlinks":
        content = '''//! Check if terminal supports hyperlinks (supports-hyperlinks.ts).

/// Check if the current terminal supports OSC 8 hyperlinks.
pub fn supports_hyperlinks() -> bool {
    // Check common terminal emulators that support hyperlinks
    if let Ok(term) = std::env::var("TERM_PROGRAM") {
        return matches!(term.as_str(), "iTerm.app" | "WezTerm" | "vscode" | "Hyper" | "Alacritty");
    }
    if std::env::var("FORCE_HYPERLINK").ok().as_deref() == Some("1") { return true; }
    if std::env::var("VTE_VERSION").is_ok() { return true; }
    false
}
'''
    else:
        content = f'''//! {title} ({mod_name.replace("_", "-")}.ts).

#[derive(Debug, Clone, Default)]
pub struct {mod_name.replace("_", " ").title().replace(" ", "")}State {{
    pub initialized: bool,
}}

impl {mod_name.replace("_", " ").title().replace(" ", "")}State {{
    pub fn new() -> Self {{ Self {{ initialized: false }} }}
    pub fn initialize(&mut self) {{ self.initialized = true; }}
}}
'''
    write_file(f"{mod_name}.rs", content)

print("All ink files created successfully")
