//! CSI (Control Sequence Introducer) parsing (csi.ts).

/// CSI sequence prefix: ESC + '['.
pub const CSI_PREFIX: &str = "\x1b[";

/// Parameter separator inside CSI sequences.
pub const CSI_SEP: char = ';';

/// CSI byte range bounds.
pub struct CsiRange;
impl CsiRange {
    pub const PARAM_START: u8 = 0x30;
    pub const PARAM_END: u8 = 0x3f;
    pub const INTERMEDIATE_START: u8 = 0x20;
    pub const INTERMEDIATE_END: u8 = 0x2f;
    pub const FINAL_START: u8 = 0x40;
    pub const FINAL_END: u8 = 0x7e;
}

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
    pub const CHT: u8 = b'I'; // Cursor Horizontal Tab
    pub const VPA: u8 = b'd'; // Vertical Position Absolute
    pub const HVP: u8 = b'f'; // Horizontal Vertical Position
    pub const ED: u8 = b'J';  // Erase in Display
    pub const EL: u8 = b'K';  // Erase in Line
    pub const ECH: u8 = b'X'; // Erase Character
    pub const IL: u8 = b'L';  // Insert Lines
    pub const DL: u8 = b'M';  // Delete Lines
    pub const ICH: u8 = b'@'; // Insert Characters
    pub const DCH: u8 = b'P'; // Delete Characters
    pub const SU: u8 = b'S';  // Scroll Up
    pub const SD: u8 = b'T';  // Scroll Down
    pub const SGR: u8 = b'm'; // Select Graphic Rendition
    pub const DSR: u8 = b'n'; // Device Status Report
    pub const DECSCUSR: u8 = b'q'; // Set Cursor Style
    pub const DECSTBM: u8 = b'r'; // Set Top and Bottom Margins
    pub const SCOSC: u8 = b's'; // Save Cursor Position
    pub const SCORC: u8 = b'u'; // Restore Cursor Position
    pub const CBT: u8 = b'Z';   // Cursor Backward Tabulation
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

/// Generate a CSI sequence with params joined by ';' and a final byte.
/// Mirrors TS `csi(...args)`: when no params, returns just CSI_PREFIX + final.
pub fn csi_with_params(params: &[i64], final_byte: char) -> String {
    if params.is_empty() {
        return format!("{}{}", CSI_PREFIX, final_byte);
    }
    let parts: Vec<String> = params.iter().map(|p| p.to_string()).collect();
    format!("{}{}{}", CSI_PREFIX, parts.join(";"), final_byte)
}

/// Generate a CSI sequence from a raw body (e.g. "200~" or ">1u").
pub fn csi_raw(body: &str) -> String {
    format!("{}{}", CSI_PREFIX, body)
}

/// Move cursor up n lines (CSI n A).
pub fn cursor_up(n: i64) -> String {
    if n == 0 { return String::new(); }
    csi_with_params(&[n], 'A')
}

/// Move cursor down n lines (CSI n B).
pub fn cursor_down(n: i64) -> String {
    if n == 0 { return String::new(); }
    csi_with_params(&[n], 'B')
}

/// Move cursor forward n columns (CSI n C).
pub fn cursor_forward(n: i64) -> String {
    if n == 0 { return String::new(); }
    csi_with_params(&[n], 'C')
}

/// Move cursor back n columns (CSI n D).
pub fn cursor_back(n: i64) -> String {
    if n == 0 { return String::new(); }
    csi_with_params(&[n], 'D')
}

/// Move cursor to absolute column (1-indexed) (CSI col G).
pub fn cursor_to(col: i64) -> String {
    csi_with_params(&[col], 'G')
}

/// CSI cursor home column (CSI G).
pub fn cursor_left() -> String {
    csi_raw("G")
}

/// Move cursor to row;col (1-indexed) (CSI row;col H).
pub fn cursor_position(row: i64, col: i64) -> String {
    csi_with_params(&[row, col], 'H')
}

/// Home cursor (CSI H).
pub fn cursor_home() -> String {
    csi_raw("H")
}

/// Move cursor by relative (x, y); positive x right, positive y down.
pub fn cursor_move(x: i64, y: i64) -> String {
    let mut out = String::new();
    if x < 0 {
        out.push_str(&cursor_back(-x));
    } else if x > 0 {
        out.push_str(&cursor_forward(x));
    }
    if y < 0 {
        out.push_str(&cursor_up(-y));
    } else if y > 0 {
        out.push_str(&cursor_down(y));
    }
    out
}

/// Save cursor position (CSI s).
pub fn cursor_save() -> String { csi_raw("s") }

/// Restore cursor position (CSI u).
pub fn cursor_restore() -> String { csi_raw("u") }

/// Erase from cursor to end of line (CSI K).
pub fn erase_to_end_of_line() -> String { csi_raw("K") }

/// Erase from cursor to start of line (CSI 1 K).
pub fn erase_to_start_of_line() -> String { csi_with_params(&[1], 'K') }

/// Erase entire line (CSI 2 K).
pub fn erase_line() -> String { csi_with_params(&[2], 'K') }

/// Erase from cursor to end of screen (CSI J).
pub fn erase_to_end_of_screen() -> String { csi_raw("J") }

/// Erase from cursor to start of screen (CSI 1 J).
pub fn erase_to_start_of_screen() -> String { csi_with_params(&[1], 'J') }

/// Erase entire screen (CSI 2 J).
pub fn erase_screen() -> String { csi_with_params(&[2], 'J') }

/// Erase scrollback buffer (CSI 3 J).
pub fn erase_scrollback() -> String { csi_with_params(&[3], 'J') }

/// Erase N lines starting from cursor line, moving cursor up.
pub fn erase_lines(n: i64) -> String {
    if n <= 0 { return String::new(); }
    let mut out = String::new();
    for i in 0..n {
        out.push_str(&erase_line());
        if i < n - 1 {
            out.push_str(&cursor_up(1));
        }
    }
    out.push_str(&cursor_left());
    out
}

/// Scroll up n lines (CSI n S).
pub fn scroll_up(n: i64) -> String {
    if n == 0 { return String::new(); }
    csi_with_params(&[n], 'S')
}

/// Scroll down n lines (CSI n T).
pub fn scroll_down(n: i64) -> String {
    if n == 0 { return String::new(); }
    csi_with_params(&[n], 'T')
}

/// Set scroll region (DECSTBM, CSI top;bottom r), 1-indexed inclusive.
pub fn set_scroll_region(top: i64, bottom: i64) -> String {
    csi_with_params(&[top, bottom], 'r')
}

/// Reset scroll region to full screen (CSI r). Homes the cursor.
pub fn reset_scroll_region() -> String { csi_raw("r") }

/// Bracketed paste: terminal sends CSI 200 ~ before pasted content.
pub fn paste_start() -> String { csi_raw("200~") }

/// Bracketed paste: terminal sends CSI 201 ~ after pasted content.
pub fn paste_end() -> String { csi_raw("201~") }

/// Focus events: terminal gained focus (CSI I).
pub fn focus_in() -> String { csi_raw("I") }

/// Focus events: terminal lost focus (CSI O).
pub fn focus_out() -> String { csi_raw("O") }

/// Enable Kitty keyboard protocol with disambiguate escape codes flag.
pub fn enable_kitty_keyboard() -> String { csi_raw(">1u") }

/// Disable Kitty keyboard protocol.
pub fn disable_kitty_keyboard() -> String { csi_raw("<u") }

/// Enable xterm modifyOtherKeys level 2.
pub fn enable_modify_other_keys() -> String { csi_raw(">4;2m") }

/// Disable xterm modifyOtherKeys (reset to default).
pub fn disable_modify_other_keys() -> String { csi_raw(">4m") }

/// Cursor style classification produced by DECSCUSR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorStyle { Block, Underline, Bar }

/// Decode DECSCUSR param into a (style, blinking) tuple, mirroring TS table.
pub fn cursor_style_for_param(n: u8) -> (CursorStyle, bool) {
    match n {
        0 | 1 => (CursorStyle::Block, true),
        2 => (CursorStyle::Block, false),
        3 => (CursorStyle::Underline, true),
        4 => (CursorStyle::Underline, false),
        5 => (CursorStyle::Bar, true),
        6 => (CursorStyle::Bar, false),
        _ => (CursorStyle::Block, true),
    }
}
