//! Parse keypress from raw terminal input (parse-keypress.ts).

/// A parsed key event.
#[derive(Debug, Clone)]
pub struct ParsedKey {
    pub name: String,
    pub sequence: String,
    pub ctrl: bool, pub meta: bool, pub shift: bool,
}

/// Status payload for a DECRPM (private-mode report) response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecrpmStatus {
    NotRecognised,
    Set,
    Reset,
    PermanentlySet,
    PermanentlyReset,
}

/// Status returned for an unrecognised DECRPM reply.
pub const DECRPM_STATUS: DecrpmStatus = DecrpmStatus::NotRecognised;

/// Inbound parse-keypress state machine constants.
pub const INITIAL_STATE: &str = "ground";

/// Keys that are not alphanumeric — useful for input filters.
pub const NON_ALPHANUMERIC_KEYS: &[&str] = &[
    "return", "escape", "tab", "backspace", "delete",
    "up", "down", "left", "right", "home", "end",
    "pageup", "pagedown",
    "f1", "f2", "f3", "f4", "f5", "f6", "f7", "f8", "f9", "f10", "f11", "f12",
];

/// Out-of-band response received from the terminal in addition to keypress events.
#[derive(Debug, Clone)]
pub enum TerminalResponse {
    /// DECRPM private-mode report (CSI ? mode ; status $ y).
    Decrpm { mode: u32, status: DecrpmStatus },
    /// Primary Device Attributes (CSI ? Pn ; ... c).
    Da1 { attrs: Vec<u32> },
    /// Secondary Device Attributes (CSI > Pn ; ... c).
    Da2 { attrs: Vec<u32> },
    /// Kitty keyboard protocol flags (CSI ? flags u).
    KittyKeyboard { flags: u32 },
    /// Cursor position report (CSI row ; col R).
    CursorPosition { row: u32, col: u32 },
    /// OSC reply (ESC ] code ; body BEL/ST).
    Osc { code: u32, body: String },
    /// XTVERSION reply (CSI > | name ST).
    Xtversion { name: String },
    /// Generic — unrecognised escape sequence.
    Unknown(String),
}

/// State machine for the streaming keypress parser.
#[derive(Debug, Clone, Default)]
pub struct KeyParseState {
    pub buffer: Vec<u8>,
    pub in_csi: bool,
    pub in_osc: bool,
    pub started_at_ms: u64,
}

/// One inbound response parsed off the wire.
#[derive(Debug, Clone)]
pub enum ParsedResponse {
    Key(ParsedKey),
    Mouse(ParsedMouse),
    Terminal(TerminalResponse),
}

/// Parsed mouse event.
#[derive(Debug, Clone)]
pub struct ParsedMouse {
    pub x: u16,
    pub y: u16,
    pub button: u8,
    pub pressed: bool,
    pub motion: bool,
}

/// Wrapper covering all parsed-input kinds.
#[derive(Debug, Clone)]
pub enum ParsedInput {
    Key(ParsedKey),
    Mouse(ParsedMouse),
    Paste(String),
    Terminal(TerminalResponse),
}

/// Parse multiple keypresses from a single chunk of terminal input.
pub fn parse_multiple_keypresses(input: &str) -> Vec<ParsedKey> {
    let mut out = Vec::new();
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1B {
            // ESC starts a sequence. Greedily match CSI / SS3 / meta+char.
            if i + 2 < bytes.len() && bytes[i + 1] == b'[' {
                // Find the final byte of CSI sequence.
                let mut j = i + 2;
                while j < bytes.len() && !(0x40..=0x7E).contains(&bytes[j]) {
                    j += 1;
                }
                if j < bytes.len() {
                    let seq = &input[i..=j];
                    out.push(parse_keypress(seq));
                    i = j + 1;
                    continue;
                }
            }
            if i + 1 < bytes.len() {
                let seq = &input[i..=i + 1];
                out.push(parse_keypress(seq));
                i += 2;
                continue;
            }
        }
        let end = i + 1;
        out.push(parse_keypress(&input[i..end]));
        i = end;
    }
    out
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
