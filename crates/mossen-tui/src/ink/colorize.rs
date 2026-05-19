//! Colorize text output (colorize.ts).

/// Apply a named color to text using ANSI escape sequences.
pub fn colorize(text: &str, color: &str) -> String {
    let code = match color {
        "red" => "31", "green" => "32", "yellow" => "33", "blue" => "34",
        "magenta" => "35", "cyan" => "36", "white" => "37", "gray" | "grey" => "90",
        "brightRed" => "91", "brightGreen" => "92", "brightYellow" => "93",
        "brightBlue" => "94", "brightMagenta" => "95", "brightCyan" => "96",
        _ => return text.to_string(),
    };
    format!("\x1b[{}m{}\x1b[39m", code, text)
}

/// Apply background color.
pub fn colorize_bg(text: &str, color: &str) -> String {
    let code = match color {
        "red" => "41", "green" => "42", "yellow" => "43", "blue" => "44",
        "magenta" => "45", "cyan" => "46", "white" => "47",
        _ => return text.to_string(),
    };
    format!("\x1b[{}m{}\x1b[49m", code, text)
}

/// Apply text style.
pub fn stylize(text: &str, style: &str) -> String {
    let (on, off) = match style {
        "bold" => ("1", "22"), "dim" => ("2", "22"), "italic" => ("3", "23"),
        "underline" => ("4", "24"), "inverse" => ("7", "27"), "strikethrough" => ("9", "29"),
        _ => return text.to_string(),
    };
    format!("\x1b[{}m{}\x1b[{}m", on, text, off)
}

/// Whether chalk has been "boosted" for xterm.js (deeper colours).
pub const CHALK_BOOSTED_FOR_XTERMJS: bool = true;

/// Whether chalk has been "clamped" for tmux (no 24-bit colours).
pub const CHALK_CLAMPED_FOR_TMUX: bool = false;

/// Apply a list of text style flags onto text.
pub fn apply_text_styles(text: &str, styles: &[&str]) -> String {
    let mut out = text.to_string();
    for s in styles {
        out = stylize(&out, s);
    }
    out
}

/// Apply both fg and optional bg colour onto text.
pub fn apply_color(text: &str, fg: Option<&str>, bg: Option<&str>) -> String {
    let mut out = text.to_string();
    if let Some(c) = fg {
        out = colorize(&out, c);
    }
    if let Some(c) = bg {
        out = colorize_bg(&out, c);
    }
    out
}

/// Foreground vs. background colour channel — mirrors TS `ColorType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorType {
    Foreground,
    Background,
}
