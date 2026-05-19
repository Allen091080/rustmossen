use std::sync::Mutex;

use once_cell::sync::Lazy;

/// Sentinel written to stderr ahead of any diverted non-JSON line
pub const STDOUT_GUARD_MARKER: &str = "[stdout-guard]";

struct GuardState {
    installed: bool,
    buffer: String,
}

static STATE: Lazy<Mutex<GuardState>> = Lazy::new(|| {
    Mutex::new(GuardState {
        installed: false,
        buffer: String::new(),
    })
});

/// Check if a line is valid JSON (empty lines are tolerated in NDJSON streams)
fn is_json_line(line: &str) -> bool {
    if line.is_empty() {
        return true;
    }
    serde_json::from_str::<serde_json::Value>(line).is_ok()
}

/// Process a chunk of text through the stdout guard.
/// Returns lines that are valid JSON (to be written to stdout).
/// Non-JSON lines are returned separately for diversion to stderr.
pub fn process_stdout_chunk(text: &str) -> GuardOutput {
    let mut state = STATE.lock().unwrap();
    if !state.installed {
        // Guard not installed, pass through
        return GuardOutput {
            stdout_lines: vec![text.to_string()],
            stderr_lines: Vec::new(),
        };
    }

    state.buffer.push_str(text);
    let mut stdout_lines = Vec::new();
    let mut stderr_lines = Vec::new();

    while let Some(newline_idx) = state.buffer.find('\n') {
        let line = state.buffer[..newline_idx].to_string();
        state.buffer = state.buffer[newline_idx + 1..].to_string();

        if is_json_line(&line) {
            stdout_lines.push(format!("{}\n", line));
        } else {
            stderr_lines.push(format!("{} {}\n", STDOUT_GUARD_MARKER, line));
        }
    }

    GuardOutput {
        stdout_lines,
        stderr_lines,
    }
}

/// Output from the guard processing
#[derive(Debug, Clone)]
pub struct GuardOutput {
    /// Lines that should go to stdout (valid JSON)
    pub stdout_lines: Vec<String>,
    /// Lines that should go to stderr (non-JSON, tagged with marker)
    pub stderr_lines: Vec<String>,
}

/// Install the stream-json stdout guard.
/// After installation, all writes should go through `process_stdout_chunk`.
pub fn install_stream_json_stdout_guard() {
    let mut state = STATE.lock().unwrap();
    if state.installed {
        return;
    }
    state.installed = true;
    state.buffer.clear();
}

/// Flush any remaining buffer content at shutdown.
/// Returns any final output that needs to be written.
pub fn flush_stream_json_stdout_guard() -> GuardOutput {
    let mut state = STATE.lock().unwrap();
    if !state.installed {
        return GuardOutput {
            stdout_lines: Vec::new(),
            stderr_lines: Vec::new(),
        };
    }

    let mut stdout_lines = Vec::new();
    let mut stderr_lines = Vec::new();

    if !state.buffer.is_empty() {
        let remaining = std::mem::take(&mut state.buffer);
        if is_json_line(&remaining) {
            stdout_lines.push(format!("{}\n", remaining));
        } else {
            stderr_lines.push(format!("{} {}\n", STDOUT_GUARD_MARKER, remaining));
        }
    }

    state.installed = false;

    GuardOutput {
        stdout_lines,
        stderr_lines,
    }
}

/// Testing-only reset. Clears state so subsequent tests start clean.
pub fn reset_stream_json_stdout_guard_for_testing() {
    let mut state = STATE.lock().unwrap();
    state.buffer.clear();
    state.installed = false;
}

/// Check if the guard is currently installed
pub fn is_stream_json_stdout_guard_installed() -> bool {
    let state = STATE.lock().unwrap();
    state.installed
}
