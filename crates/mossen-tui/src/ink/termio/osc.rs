//! OSC (Operating System Command) parsing (osc.ts).

use super::types::{Action, Color, LinkAction, NamedColor, TabStatusAction, TitleAction};

/// OSC sequence prefix (ESC + ']').
pub const OSC_PREFIX: &str = "\x1b]";

/// String terminator (ESC + '\\').
pub const OSC_ST: &str = "\x1b\\";

/// String terminator (alias matching the TS name).
pub const ST: &str = "\x1b\\";

/// BEL terminator.
pub const OSC_BEL: char = '\x07';

/// OSC command numbers.
pub struct OSC;
impl OSC {
    pub const SET_TITLE_AND_ICON: u32 = 0;
    pub const SET_ICON: u32 = 1;
    pub const SET_TITLE: u32 = 2;
    pub const SET_COLOR: u32 = 4;
    pub const SET_CWD: u32 = 7;
    pub const HYPERLINK: u32 = 8;
    pub const ITERM2: u32 = 9;
    pub const SET_FG_COLOR: u32 = 10;
    pub const SET_BG_COLOR: u32 = 11;
    pub const SET_CURSOR_COLOR: u32 = 12;
    pub const CLIPBOARD: u32 = 52;
    pub const KITTY: u32 = 99;
    pub const RESET_COLOR: u32 = 104;
    pub const RESET_FG_COLOR: u32 = 110;
    pub const RESET_BG_COLOR: u32 = 111;
    pub const RESET_CURSOR_COLOR: u32 = 112;
    pub const SEMANTIC_PROMPT: u32 = 133;
    pub const GHOSTTY: u32 = 777;
    pub const TAB_STATUS: u32 = 21337;
}

/// iTerm2 OSC 9 subcommand numbers.
pub struct ITERM2;
impl ITERM2 {
    pub const NOTIFY: u32 = 0;
    pub const BADGE: u32 = 2;
    pub const PROGRESS: u32 = 4;
}

/// Progress sub-codes for iTerm2.
pub struct PROGRESS;
impl PROGRESS {
    pub const CLEAR: u32 = 0;
    pub const SET: u32 = 1;
    pub const ERROR: u32 = 2;
    pub const INDETERMINATE: u32 = 3;
}

/// Which path setClipboard will take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardPath { Native, TmuxBuffer, Osc52 }

/// Decide clipboard write path based on env. Returns Native on macOS without
/// SSH_CONNECTION, TmuxBuffer when $TMUX is set, otherwise Osc52.
pub fn get_clipboard_path() -> ClipboardPath {
    let in_ssh = std::env::var("SSH_CONNECTION").is_ok();
    let is_darwin = std::env::consts::OS == "macos";
    if is_darwin && !in_ssh {
        return ClipboardPath::Native;
    }
    if std::env::var("TMUX").is_ok() {
        return ClipboardPath::TmuxBuffer;
    }
    ClipboardPath::Osc52
}

/// Build an OSC sequence with the BEL terminator (safe everywhere). Use
/// `osc_st` for an ST-terminated form when callers need that.
pub fn osc(parts: &[&str]) -> String {
    let joined = parts.join(";");
    // Kitty prefers ST to avoid bells. We use BEL by default; callers wanting
    // ST can switch via TERM/env probe later. The TS code consulted env.terminal
    // which is set during init; we keep behavior simple and BEL-only here, then
    // expose `osc_st` for explicit ST emission.
    let term = std::env::var("TERM").unwrap_or_default();
    let terminator: &str = if term.contains("kitty") { OSC_ST } else { "\x07" };
    format!("{}{}{}", OSC_PREFIX, joined, terminator)
}

/// Force-ST variant of [`osc`].
pub fn osc_st(parts: &[&str]) -> String {
    let joined = parts.join(";");
    format!("{}{}{}", OSC_PREFIX, joined, OSC_ST)
}

/// Wrap an escape sequence for tmux/screen DCS-passthrough, leaving it
/// unchanged when not inside a multiplexer.
pub fn wrap_for_multiplexer(sequence: &str) -> String {
    if std::env::var("TMUX").is_ok() {
        // Double inner ESCs.
        let escaped = sequence.replace('\x1b', "\x1b\x1b");
        return format!("\x1bPtmux;{}\x1b\\", escaped);
    }
    if std::env::var("STY").is_ok() {
        return format!("\x1bP{}\x1b\\", sequence);
    }
    sequence.to_string()
}

/// Wrap a payload in tmux's DCS passthrough: `ESC P tmux ; <payload> ESC \`.
fn tmux_passthrough(payload: &str) -> String {
    let doubled = payload.replace('\x1b', "\x1b\x1b");
    format!("\x1bPtmux;{}{}", doubled, OSC_ST)
}

/// Load text into tmux's paste buffer via `tmux load-buffer`. Returns
/// Ok(true) on success, Ok(false) when not inside tmux or tmux exited non-zero.
pub fn tmux_load_buffer(text: &str) -> std::io::Result<bool> {
    if std::env::var("TMUX").is_err() {
        return Ok(false);
    }
    use std::io::Write;
    use std::process::{Command, Stdio};
    let lc = std::env::var("LC_TERMINAL").unwrap_or_default();
    let args: &[&str] = if lc == "iTerm2" {
        &["load-buffer", "-"]
    } else {
        &["load-buffer", "-w", "-"]
    };
    let mut child = Command::new("tmux")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(text.as_bytes());
    }
    let status = child.wait()?;
    Ok(status.success())
}

/// Compute an OSC 8 `id=` value derived from the URL (deterministic per URL).
fn osc8_id(url: &str) -> String {
    let mut h: i32 = 0;
    for c in url.chars() {
        h = h.wrapping_shl(5).wrapping_sub(h).wrapping_add(c as i32);
    }
    let u = h as u32;
    // Base36 like JS .toString(36)
    if u == 0 { return "0".to_string(); }
    let mut n = u;
    let mut out = Vec::new();
    while n > 0 {
        let d = (n % 36) as u8;
        let c = if d < 10 { b'0' + d } else { b'a' + (d - 10) };
        out.push(c);
        n /= 36;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// Start a hyperlink (OSC 8) with auto-assigned `id` param. Empty URL closes.
pub fn link(url: &str, extra_params: &[(&str, &str)]) -> String {
    if url.is_empty() {
        return link_end();
    }
    let id = osc8_id(url);
    let mut params: Vec<(String, String)> = vec![("id".to_string(), id)];
    for (k, v) in extra_params {
        params.push((k.to_string(), v.to_string()));
    }
    let pairs: Vec<String> = params.iter().map(|(k, v)| format!("{}={}", k, v)).collect();
    let param_str = pairs.join(":");
    osc(&["8", &param_str, url])
}

/// Close a hyperlink (OSC 8 with empty params and empty URL).
pub fn link_end() -> String {
    osc(&["8", "", ""])
}

/// Clear iTerm2 progress bar (OSC 9 ; 4 ; 0 ; BEL).
pub fn clear_iterm2_progress() -> String {
    format!("{}{};{};{};{}",
        OSC_PREFIX, OSC::ITERM2, ITERM2::PROGRESS, PROGRESS::CLEAR, OSC_BEL)
}

/// Clear terminal title (OSC 0 ; BEL).
pub fn clear_terminal_title() -> String {
    format!("{}{};{}", OSC_PREFIX, OSC::SET_TITLE_AND_ICON, OSC_BEL)
}

/// Clear all three OSC 21337 tab-status fields.
pub fn clear_tab_status() -> String {
    osc(&["21337", "indicator=;status=;status-color="])
}

/// Whether OSC 21337 tab-status emission is enabled (ant gating).
pub fn supports_tab_status() -> bool {
    std::env::var("USER_TYPE").map(|v| v == "ant").unwrap_or(false)
}

fn color_to_hex(c: &Color) -> String {
    match c {
        Color::Rgb(r, g, b) => format!("#{:02x}{:02x}{:02x}", r, g, b),
        _ => String::new(),
    }
}

/// Emit an OSC 21337 tab-status sequence. `Some(None)` clears; `None` leaves
/// the field untouched.
pub fn tab_status(fields: &TabStatusAction) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(opt) = &fields.indicator {
        let v = opt.as_ref().map(color_to_hex).unwrap_or_default();
        parts.push(format!("indicator={}", v));
    }
    if let Some(opt) = &fields.status {
        let v = opt
            .as_ref()
            .map(|s| s.replace('\\', "\\\\").replace(';', "\\;"))
            .unwrap_or_default();
        parts.push(format!("status={}", v));
    }
    if let Some(opt) = &fields.status_color {
        let v = opt.as_ref().map(color_to_hex).unwrap_or_default();
        parts.push(format!("status-color={}", v));
    }
    let joined = parts.join(";");
    osc(&["21337", &joined])
}

/// Compute OSC 52 clipboard payload as base64 (no terminators).
fn base64_encode(input: &[u8]) -> String {
    const ALPHA: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((input.len() + 2) / 3 * 4);
    let mut i = 0;
    while i + 3 <= input.len() {
        let n = ((input[i] as u32) << 16) | ((input[i + 1] as u32) << 8) | (input[i + 2] as u32);
        out.push(ALPHA[((n >> 18) & 0x3F) as usize] as char);
        out.push(ALPHA[((n >> 12) & 0x3F) as usize] as char);
        out.push(ALPHA[((n >> 6) & 0x3F) as usize] as char);
        out.push(ALPHA[(n & 0x3F) as usize] as char);
        i += 3;
    }
    let rem = input.len() - i;
    if rem == 1 {
        let n = (input[i] as u32) << 16;
        out.push(ALPHA[((n >> 18) & 0x3F) as usize] as char);
        out.push(ALPHA[((n >> 12) & 0x3F) as usize] as char);
        out.push('=');
        out.push('=');
    } else if rem == 2 {
        let n = ((input[i] as u32) << 16) | ((input[i + 1] as u32) << 8);
        out.push(ALPHA[((n >> 18) & 0x3F) as usize] as char);
        out.push(ALPHA[((n >> 12) & 0x3F) as usize] as char);
        out.push(ALPHA[((n >> 6) & 0x3F) as usize] as char);
        out.push('=');
    }
    out
}

/// Set the system clipboard via OSC 52, with tmux passthrough wrapping when
/// inside tmux. Returns the sequence callers should write to stdout.
pub fn set_clipboard(text: &str) -> String {
    let b64 = base64_encode(text.as_bytes());
    let raw = format!("{}{};c;{}{}", OSC_PREFIX, OSC::CLIPBOARD, b64, OSC_BEL);
    if std::env::var("TMUX").is_ok() {
        // Try to load tmux buffer; ignore failures (we still return passthrough).
        let _ = tmux_load_buffer(text);
        return tmux_passthrough(&format!(
            "\x1b]{};c;{}{}", OSC::CLIPBOARD, b64, OSC_BEL
        ));
    }
    raw
}

/// Reset cached Linux clipboard probe (test-only). No-op in the Rust port
/// because clipboard probing happens per-call; the function exists to mirror
/// the TS API surface for callers reaching into internals from tests.
pub fn reset_linux_copy_cache() {}

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

/// Public XParseColor parser (`#RRGGBB` and `rgb:R/G/B`). Returns None on
/// failure. Mirrors the TS `parseOscColor` export so callers/tests can reuse.
pub fn parse_osc_color(spec: &str) -> Option<Color> {
    // #RRGGBB
    if spec.len() == 7 && spec.starts_with('#') {
        let r = u8::from_str_radix(&spec[1..3], 16).ok()?;
        let g = u8::from_str_radix(&spec[3..5], 16).ok()?;
        let b = u8::from_str_radix(&spec[5..7], 16).ok()?;
        return Some(Color::Rgb(r, g, b));
    }
    // rgb:R/G/B with 1..4 hex digits per component
    if let Some(rest) = spec.strip_prefix("rgb:") {
        let parts: Vec<&str> = rest.split('/').collect();
        if parts.len() == 3 {
            let scale = |s: &str| -> Option<u8> {
                if s.is_empty() || s.len() > 4 { return None; }
                let n = u32::from_str_radix(s, 16).ok()?;
                let max = 16u32.pow(s.len() as u32) - 1;
                Some(((n as f64 / max as f64) * 255.0).round() as u8)
            };
            let r = scale(parts[0])?;
            let g = scale(parts[1])?;
            let b = scale(parts[2])?;
            return Some(Color::Rgb(r, g, b));
        }
    }
    None
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
