//! Static (one-shot) rendering helpers — extract first frame, strip ANSI.
//!
//! Mirrors TS `utils/staticRender.tsx`. The TS module wires React/Ink to
//! produce a one-shot render of a component tree to a string. The Rust
//! port keeps the two non-React helpers — first-frame extraction and ANSI
//! stripping — as pure string functions, and exposes a renderer hook
//! that takes a closure producing the ANSI text (mirroring the role of
//! the React tree).

/// DEC synchronized-update markers used by terminals (and emitted by Ink).
pub const SYNC_START: &str = "\x1B[?2026h";
pub const SYNC_END: &str = "\x1B[?2026l";

/// Extracts content from the first complete frame in Ink's output.
///
/// Ink with non-TTY stdout outputs multiple frames, each wrapped in DEC
/// synchronized update sequences ([?2026h ... [?2026l). We only want the
/// first frame's content.
pub fn extract_first_frame(output: &str) -> String {
    let Some(start_idx) = output.find(SYNC_START) else {
        return output.to_string();
    };
    let content_start = start_idx + SYNC_START.len();
    let Some(end_offset) = output[content_start..].find(SYNC_END) else {
        return output.to_string();
    };
    output[content_start..content_start + end_offset].to_string()
}

/// Strip ANSI escape sequences from a string. Implements a minimal subset
/// sufficient for color/cursor/sync sequences emitted by Ink and chalk:
/// `ESC [ ... letter`, `ESC ] ... BEL`, `ESC ( c`, `ESC ) c`, and `ESC`-
/// terminated single-char sequences.
pub fn strip_ansi(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == 0x1B && i + 1 < bytes.len() {
            // ESC sequence
            let next = bytes[i + 1];
            match next {
                b'[' => {
                    // CSI: ESC [ ... <letter @-~>
                    let mut j = i + 2;
                    while j < bytes.len() {
                        let c = bytes[j];
                        // CSI is terminated by a byte in 0x40..=0x7E
                        if (0x40..=0x7E).contains(&c) {
                            j += 1;
                            break;
                        }
                        j += 1;
                    }
                    i = j;
                    continue;
                }
                b']' => {
                    // OSC: ESC ] ... BEL or ESC \
                    let mut j = i + 2;
                    while j < bytes.len() {
                        let c = bytes[j];
                        if c == 0x07 {
                            j += 1;
                            break;
                        }
                        if c == 0x1B && j + 1 < bytes.len() && bytes[j + 1] == b'\\' {
                            j += 2;
                            break;
                        }
                        j += 1;
                    }
                    i = j;
                    continue;
                }
                b'(' | b')' => {
                    // Character-set selection: ESC ( c — skip two bytes
                    i += 3;
                    continue;
                }
                _ => {
                    // Single-byte ESC sequence — skip ESC and the next byte
                    i += 2;
                    continue;
                }
            }
        }
        // Push raw byte. We can rely on UTF-8 boundaries here because every
        // ANSI escape we accept above begins with a single-byte ESC (0x1B);
        // non-ASCII bytes (>= 0x80) always belong to the body of a UTF-8
        // codepoint and are passed through verbatim.
        if let Some(ch) = std::str::from_utf8(&bytes[i..i + 1]).ok().and_then(|s| s.chars().next()) {
            out.push(ch);
            i += 1;
        } else {
            // Multibyte UTF-8 char — find the end of this code point.
            let cp_end = utf8_codepoint_end(bytes, i);
            if let Ok(s) = std::str::from_utf8(&bytes[i..cp_end]) {
                out.push_str(s);
            }
            i = cp_end;
        }
    }
    out
}

/// Find the end index (exclusive) of the UTF-8 code point starting at `i`.
fn utf8_codepoint_end(bytes: &[u8], i: usize) -> usize {
    let b = bytes[i];
    let len = if b < 0x80 {
        1
    } else if b < 0xC0 {
        1 // continuation byte mid-stream — skip 1 to make progress
    } else if b < 0xE0 {
        2
    } else if b < 0xF0 {
        3
    } else {
        4
    };
    (i + len).min(bytes.len())
}

/// Renders a producer closure to an ANSI string with optional column width.
///
/// In TS this wraps `render(<RenderOnceAndExit>{node}</RenderOnceAndExit>)`
/// and waits for exit. In Rust we let the caller produce the ANSI text
/// (typically from a TUI tree they own); this function then runs the same
/// first-frame extraction the TS code does.
///
/// `columns` is forwarded to the producer so callers that own width-aware
/// rendering (Ratatui, etc.) can use it.
pub async fn render_to_ansi_string<F, Fut>(producer: F, columns: Option<u16>) -> String
where
    F: FnOnce(Option<u16>) -> Fut,
    Fut: std::future::Future<Output = String>,
{
    let raw = producer(columns).await;
    extract_first_frame(&raw)
}

/// Renders a producer closure to a plain-text string (ANSI stripped).
pub async fn render_to_string<F, Fut>(producer: F, columns: Option<u16>) -> String
where
    F: FnOnce(Option<u16>) -> Fut,
    Fut: std::future::Future<Output = String>,
{
    let ansi = render_to_ansi_string(producer, columns).await;
    strip_ansi(&ansi)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_first_frame_passthrough_when_no_markers() {
        assert_eq!(extract_first_frame("hello"), "hello");
    }

    #[test]
    fn extract_first_frame_isolates_first_frame() {
        let s = format!(
            "{}frame1{}{}frame2{}",
            SYNC_START, SYNC_END, SYNC_START, SYNC_END
        );
        assert_eq!(extract_first_frame(&s), "frame1");
    }

    #[test]
    fn strip_ansi_basic() {
        let s = "\x1B[31mred\x1B[0m text";
        assert_eq!(strip_ansi(s), "red text");
    }

    #[test]
    fn strip_ansi_handles_sync_markers() {
        let s = format!("{}hi{}", SYNC_START, SYNC_END);
        assert_eq!(strip_ansi(&s), "hi");
    }
}
