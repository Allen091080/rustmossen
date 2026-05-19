//! Terminal (terminal.ts).

use std::sync::OnceLock;

#[derive(Debug, Clone, Default)]
pub struct TerminalState {
    pub initialized: bool,
}

impl TerminalState {
    pub fn new() -> Self { Self { initialized: false } }
    pub fn initialize(&mut self) { self.initialized = true; }
}

/// Progress reporting state used by [`emit_progress`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressState {
    Running,
    Completed,
    Error,
    Indeterminate,
}

/// Progress reporting payload mirroring the TS `Progress` type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Progress {
    pub state: ProgressState,
    pub percentage: Option<f32>,
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

fn semver_at_least(version: &str, want: &str) -> bool {
    // Compare dotted numeric segments left-to-right. Treat missing/garbage as 0.
    let parse = |s: &str| -> Vec<u32> {
        s.split('.').take(3).map(|p| {
            p.chars().take_while(|c| c.is_ascii_digit()).collect::<String>()
                .parse().unwrap_or(0)
        }).collect()
    };
    let a = parse(version);
    let b = parse(want);
    for i in 0..3 {
        let av = *a.get(i).unwrap_or(&0);
        let bv = *b.get(i).unwrap_or(&0);
        if av != bv { return av > bv; }
    }
    true
}

/// True when the terminal supports OSC 9;4 progress reporting (ConEmu,
/// Ghostty 1.2+, iTerm2 3.6.6+). Returns false when not on a TTY.
pub fn is_progress_reporting_available() -> bool {
    // We approximate "process.stdout.isTTY" by checking the IS_TTY env hint
    // populated by the runtime. Most callers in this crate set it via the
    // terminal init path.
    if !std::io::IsTerminal::is_terminal(&std::io::stdout()) {
        return false;
    }
    if env("WT_SESSION").is_some() {
        return false;
    }
    if env("ConEmuANSI").is_some() || env("ConEmuPID").is_some() || env("ConEmuTask").is_some() {
        return true;
    }
    let term_program = env("TERM_PROGRAM").unwrap_or_default();
    let version = match env("TERM_PROGRAM_VERSION") {
        Some(v) if !v.is_empty() => v,
        _ => return false,
    };
    if term_program == "ghostty" {
        return semver_at_least(&version, "1.2.0");
    }
    if term_program == "iTerm.app" {
        return semver_at_least(&version, "3.6.6");
    }
    false
}

/// True when the terminal supports DEC mode 2026 (synchronized output).
pub fn is_synchronized_output_supported() -> bool {
    if env("TMUX").is_some() {
        return false;
    }
    let term_program = env("TERM_PROGRAM").unwrap_or_default();
    let term = env("TERM").unwrap_or_default();
    let modern = ["iTerm.app", "WezTerm", "WarpTerminal", "ghostty",
                  "contour", "vscode", "alacritty"];
    if modern.contains(&term_program.as_str()) {
        return true;
    }
    if term.contains("kitty") || env("KITTY_WINDOW_ID").is_some() {
        return true;
    }
    if term == "xterm-ghostty" {
        return true;
    }
    if term.starts_with("foot") {
        return true;
    }
    if term.contains("alacritty") {
        return true;
    }
    if env("ZED_TERM").is_some() {
        return true;
    }
    if env("WT_SESSION").is_some() {
        return true;
    }
    if let Some(v) = env("VTE_VERSION") {
        if let Ok(n) = v.parse::<u32>() {
            if n >= 6800 {
                return true;
            }
        }
    }
    false
}

static XTVERSION_NAME: OnceLock<String> = OnceLock::new();

/// Record the XTVERSION response (called once when the reply arrives on stdin).
pub fn set_xtversion_name(name: impl Into<String>) {
    let _ = XTVERSION_NAME.set(name.into());
}

/// True when the terminal is xterm.js-based (VS Code, Cursor, Windsurf).
pub fn is_xterm_js() -> bool {
    if env("TERM_PROGRAM").as_deref() == Some("vscode") {
        return true;
    }
    XTVERSION_NAME.get().map(|n| n.starts_with("xterm.js")).unwrap_or(false)
}

const EXTENDED_KEYS_TERMINALS: &[&str] =
    &["iTerm.app", "kitty", "WezTerm", "ghostty", "tmux", "windows-terminal"];

/// True when the current terminal correctly handles extended key reporting.
pub fn supports_extended_keys() -> bool {
    let t = env("TERM_PROGRAM").unwrap_or_default();
    EXTENDED_KEYS_TERMINALS.contains(&t.as_str())
}

/// True when the terminal exhibits the conhost cursor-up viewport-yank bug.
pub fn has_cursor_up_viewport_yank_bug() -> bool {
    if std::env::consts::OS == "windows" {
        return true;
    }
    env("WT_SESSION").is_some()
}

/// Cached result of [`is_synchronized_output_supported`], lazily computed once.
pub fn sync_output_supported() -> bool {
    static CACHE: OnceLock<bool> = OnceLock::new();
    *CACHE.get_or_init(is_synchronized_output_supported)
}

/// Terminal abstraction — minimal handle covering cells the writer needs.
#[derive(Debug, Clone, Default)]
pub struct Terminal {
    pub cols: u16,
    pub rows: u16,
    pub cursor_x: u16,
    pub cursor_y: u16,
    pub buffer: Vec<String>,
}

/// Compute the cell-diff between previous and next frames, emitting only
/// changed cells as escape sequences to `out`.
pub fn write_diff_to_terminal(
    prev: &Terminal,
    next: &Terminal,
    out: &mut Vec<u8>,
) -> usize {
    let before = out.len();
    let rows = prev.buffer.len().max(next.buffer.len());
    for row in 0..rows {
        let p = prev.buffer.get(row).map(|s| s.as_str()).unwrap_or("");
        let n = next.buffer.get(row).map(|s| s.as_str()).unwrap_or("");
        if p == n {
            continue;
        }
        // Move cursor to (1, row+1)
        out.extend_from_slice(format!("\x1b[{};{}H", row + 1, 1).as_bytes());
        out.extend_from_slice(n.as_bytes());
    }
    out.len() - before
}
