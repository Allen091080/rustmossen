//! Early input capture — port of `utils/earlyInput.ts`.
//!
//! Users who type `mossen` and immediately keep typing would otherwise lose
//! the head of their prompt during the (sub-second) startup window. The TS
//! entrypoint puts the terminal in raw mode, attaches a `readable` listener
//! on stdin, and buffers each keystroke until the REPL takes over via
//! `consumeEarlyInput`.
//!
//! In Rust we reproduce that contract end-to-end:
//!   * `start_capturing_early_input` enables raw mode (crossterm), spawns a
//!     blocking reader task that polls keyboard events, and feeds each chunk
//!     through `process_input_chunk` which applies the same control-character
//!     rules as the TS implementation (with grapheme-aware backspace via
//!     `unicode-segmentation`).
//!   * `stop_capturing_early_input` flips the capture flag, signals the reader
//!     task to drain and exit, and disables raw mode (idempotent — the REPL
//!     will re-enable it via its own setup).
//!   * `consume_early_input` returns the trimmed buffer and stops capture.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use once_cell::sync::Lazy;
use unicode_segmentation::UnicodeSegmentation;

/// Shared buffer and capture flag. Held behind a `Mutex` so the reader thread
/// and the consumer (REPL bootstrap) can both touch it.
struct EarlyInputState {
    buffer: String,
}

static STATE: Lazy<Mutex<EarlyInputState>> = Lazy::new(|| {
    Mutex::new(EarlyInputState {
        buffer: String::new(),
    })
});

/// `true` between `start_capturing_early_input` returning successfully and
/// `stop_capturing_early_input` being called. Atomic so the reader thread can
/// poll it without holding the buffer lock.
static IS_CAPTURING: AtomicBool = AtomicBool::new(false);

/// Records whether we successfully enabled raw mode so `stop_capturing` only
/// disables it when we own that state.
static RAW_MODE_ENABLED: AtomicBool = AtomicBool::new(false);

/// Returns `true` if `fd` refers to a terminal (`libc::isatty`).
fn fd_is_tty(fd: i32) -> bool {
    #[cfg(unix)]
    unsafe {
        libc::isatty(fd) == 1
    }
    #[cfg(not(unix))]
    {
        let _ = fd;
        false
    }
}

/// Start capturing stdin keystrokes early, before the REPL is initialised.
///
/// No-ops (and returns `false`) when:
///   * we're already capturing,
///   * stdin is not a TTY (piped / redirected input),
///   * argv contains `-p` / `--print` (raw mode would break `Ctrl+C` SIGINT
///     in non-interactive runs).
///
/// Returns `true` if capture actually started.
pub fn start_capturing_early_input() -> bool {
    if IS_CAPTURING.load(Ordering::SeqCst) {
        return false;
    }
    if !fd_is_tty(0) {
        return false;
    }

    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "-p" || a == "--print") {
        return false;
    }

    // Clear any seed and enable raw mode. If raw-mode setup fails (e.g.
    // running under a stripped-down PTY) silently bail without capture — same
    // failure semantics as the TS `try/catch` around `setRawMode(true)`.
    {
        let mut state = STATE.lock().unwrap();
        state.buffer.clear();
    }
    if enable_raw_mode().is_err() {
        return false;
    }
    RAW_MODE_ENABLED.store(true, Ordering::SeqCst);
    IS_CAPTURING.store(true, Ordering::SeqCst);

    thread::Builder::new()
        .name("mossen-early-input".into())
        .spawn(read_loop)
        .ok();

    true
}

/// Reader loop: poll the terminal for events at 25 ms granularity, translate
/// keyboard input into the same byte sequence the TS reader sees, and pump it
/// through `process_input_chunk`. Exits as soon as `IS_CAPTURING` is cleared.
fn read_loop() {
    while IS_CAPTURING.load(Ordering::SeqCst) {
        match event::poll(Duration::from_millis(25)) {
            Ok(true) => {}
            Ok(false) => continue,
            Err(_) => break,
        }
        match event::read() {
            Ok(Event::Key(key)) => {
                if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
                    continue;
                }
                if let Some(chunk) = key_event_to_chunk(&key) {
                    process_input_chunk(&chunk);
                }
            }
            Ok(_) => continue,
            Err(_) => break,
        }
    }
}

/// Translate a crossterm `KeyEvent` into the equivalent raw byte chunk a
/// terminal in raw mode would deliver — the same sequences `processChunk` in
/// `utils/earlyInput.ts` decodes.
fn key_event_to_chunk(key: &KeyEvent) -> Option<String> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Char('c') if ctrl => Some("\x03".to_string()),
        KeyCode::Char('d') if ctrl => Some("\x04".to_string()),
        KeyCode::Char(c) if ctrl => {
            // Map Ctrl+<letter> to the corresponding control byte (Ctrl+A=1, …).
            let lower = c.to_ascii_lowercase();
            if ('a'..='z').contains(&lower) {
                Some(((lower as u8 - b'a' + 1) as char).to_string())
            } else {
                None
            }
        }
        KeyCode::Char(c) => Some(c.to_string()),
        KeyCode::Enter => Some("\r".to_string()),
        KeyCode::Tab => Some("\t".to_string()),
        KeyCode::Backspace => Some("\x7f".to_string()),
        KeyCode::Esc => Some("\x1b".to_string()),
        _ => None,
    }
}

/// Apply the TS `processChunk` rules to a chunk of raw input bytes.
///
/// Behaviour kept point-for-point compatible:
///   * Ctrl+C (`\x03`) → stop capturing **and** exit with status 130 (the
///     shutdown machinery is not yet initialised this early, matching TS's
///     `process.exit(130)`).
///   * Ctrl+D (`\x04`) → stop capturing.
///   * Backspace / DEL → pop the trailing grapheme cluster (CJK-safe).
///   * `ESC` followed by `0x40..=0x7e` → ANSI escape sequence, discarded.
///   * Carriage return → store as `\n`.
///   * Other control chars (< 0x20, not TAB/LF/CR) → dropped.
///   * Everything else appended verbatim.
pub fn process_input_chunk(data: &str) {
    if !IS_CAPTURING.load(Ordering::SeqCst) {
        return;
    }

    let chars: Vec<char> = data.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];
        let code = ch as u32;

        if code == 3 {
            stop_capturing_early_input();
            std::process::exit(130);
        }

        if code == 4 {
            stop_capturing_early_input();
            return;
        }

        if code == 127 || code == 8 {
            let mut state = STATE.lock().unwrap();
            if let Some(last) = state.buffer.graphemes(true).next_back() {
                let take = last.len();
                let new_len = state.buffer.len() - take;
                state.buffer.truncate(new_len);
            }
            i += 1;
            continue;
        }

        if code == 27 {
            // ESC — skip the entire escape sequence.
            // CSI: ESC [ (0x5b) + optional params (0x20-0x2f) + final byte (0x40-0x7e).
            i += 1;
            if i < chars.len() && chars[i] as u32 == 0x5b {
                i += 1;
                while i < chars.len() {
                    let c = chars[i] as u32;
                    if (0x20..=0x2f).contains(&c) {
                        i += 1;
                    } else {
                        break;
                    }
                }
            }
            if i < chars.len() {
                let c = chars[i] as u32;
                if (64..=126).contains(&c) {
                    i += 1;
                }
            }
            continue;
        }

        if code < 32 && code != 9 && code != 10 && code != 13 {
            i += 1;
            continue;
        }

        if code == 13 {
            let mut state = STATE.lock().unwrap();
            state.buffer.push('\n');
            i += 1;
            continue;
        }

        let mut state = STATE.lock().unwrap();
        state.buffer.push(ch);
        i += 1;
    }
}

/// Stop capturing. Idempotent — safe to call from the reader thread, the
/// consumer, or both. Disables raw mode iff we enabled it ourselves.
pub fn stop_capturing_early_input() {
    if !IS_CAPTURING.swap(false, Ordering::SeqCst) {
        return;
    }
    if RAW_MODE_ENABLED.swap(false, Ordering::SeqCst) {
        // Best-effort: disable raw mode. The REPL will re-enable it via its
        // own setup; calling it twice is harmless.
        let _ = disable_raw_mode();
    }
}

/// Stop capture and return the trimmed buffer.
pub fn consume_early_input() -> String {
    stop_capturing_early_input();
    let mut state = STATE.lock().unwrap();
    let input = state.buffer.trim().to_string();
    state.buffer.clear();
    input
}

/// Whether the buffer currently has any non-whitespace text.
pub fn has_early_input() -> bool {
    let state = STATE.lock().unwrap();
    !state.buffer.trim().is_empty()
}

/// Pre-seed the buffer (used when the REPL is launched with an initial prompt
/// to make the input box show it before the first render).
pub fn seed_early_input(text: &str) {
    let mut state = STATE.lock().unwrap();
    state.buffer = text.to_string();
}

/// Whether capture is currently active.
pub fn is_capturing_early_input() -> bool {
    IS_CAPTURING.load(Ordering::SeqCst)
}

#[cfg(test)]
mod tests {
    use super::*;
    use once_cell::sync::Lazy;
    use std::sync::Mutex;

    static TEST_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    /// Drives `process_input_chunk` directly (without the reader thread or raw
    /// mode) to exercise the chunk-decoding rules.
    fn force_capture() {
        IS_CAPTURING.store(true, Ordering::SeqCst);
        let mut state = STATE.lock().unwrap();
        state.buffer.clear();
    }

    fn read_buf() -> String {
        STATE.lock().unwrap().buffer.clone()
    }

    #[test]
    fn appends_printable_chars() {
        let _guard = TEST_LOCK.lock().unwrap();
        force_capture();
        process_input_chunk("hi");
        assert_eq!(read_buf(), "hi");
        stop_capturing_early_input();
    }

    #[test]
    fn backspace_removes_grapheme() {
        let _guard = TEST_LOCK.lock().unwrap();
        force_capture();
        process_input_chunk("你好");
        process_input_chunk("\x7f");
        assert_eq!(read_buf(), "你");
        stop_capturing_early_input();
    }

    #[test]
    fn carriage_return_becomes_newline() {
        let _guard = TEST_LOCK.lock().unwrap();
        force_capture();
        process_input_chunk("a\rb");
        assert_eq!(read_buf(), "a\nb");
        stop_capturing_early_input();
    }

    #[test]
    fn escape_sequence_dropped() {
        let _guard = TEST_LOCK.lock().unwrap();
        force_capture();
        // ESC [ A → up-arrow, should be skipped entirely
        process_input_chunk("a\x1b[Ab");
        assert_eq!(read_buf(), "ab");
        stop_capturing_early_input();
    }
}
