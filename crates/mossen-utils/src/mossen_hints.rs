//! Mossen hints protocol — parser and pending-hint store.
//!
//! CLIs and SDKs running under Mossen can emit a self-closing `<mossen-hint />`
//! tag to stderr. The harness scans tool output for these tags, strips them before
//! the output reaches the model, and surfaces an install prompt to the user.

use std::collections::HashSet;
use std::sync::Mutex;

use once_cell::sync::Lazy;
use regex::Regex;

/// Hint type discriminator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MossenHintType {
    Plugin,
}

/// A parsed Mossen hint.
#[derive(Debug, Clone)]
pub struct MossenHint {
    /// Spec version declared by the emitter.
    pub v: u32,
    /// Hint discriminator.
    pub hint_type: MossenHintType,
    /// Hint payload (e.g., plugin name@marketplace slug).
    pub value: String,
    /// First token of the shell command that produced this hint.
    pub source_command: String,
}

/// Supported spec versions.
static SUPPORTED_VERSIONS: Lazy<HashSet<u32>> = Lazy::new(|| {
    let mut s = HashSet::new();
    s.insert(1);
    s
});

/// Supported hint types.
static SUPPORTED_TYPES: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    let mut s = HashSet::new();
    s.insert("plugin");
    s
});

/// Outer tag match regex.
static HINT_TAG_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?m)^[ \t]*<mossen-hint\s+([^>]*?)\s*/>[ \t]*$").unwrap());

/// Attribute matcher regex.
static ATTR_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(\w+)=(?:"([^"]*)"|([^\s/>]+))"#).unwrap());

/// Result of extracting hints from output.
pub struct ExtractResult {
    pub hints: Vec<MossenHint>,
    pub stripped: String,
}

/// Scan shell tool output for hint tags, returning the parsed hints and
/// the output with hint lines removed.
pub fn extract_mossen_hints(output: &str, command: &str) -> ExtractResult {
    // Fast path: no tag open sequence -> no work
    if !output.contains("<mossen-hint") {
        return ExtractResult {
            hints: Vec::new(),
            stripped: output.to_string(),
        };
    }

    let source_command = first_command_token(command);
    let mut hints = Vec::new();

    let stripped = HINT_TAG_RE
        .replace_all(output, |caps: &regex::Captures| {
            let raw_line = caps.get(0).map(|m| m.as_str()).unwrap_or("");
            let attrs = parse_attrs(raw_line);

            let v: u32 = attrs.get("v").and_then(|s| s.parse().ok()).unwrap_or(0);
            let hint_type_str = attrs.get("type").map(|s| s.as_str()).unwrap_or("");
            let value = attrs.get("value").map(|s| s.as_str()).unwrap_or("");

            if !SUPPORTED_VERSIONS.contains(&v) {
                tracing::debug!("[mossenHints] dropped hint with unsupported v={}", v);
                return String::new();
            }
            if hint_type_str.is_empty() || !SUPPORTED_TYPES.contains(hint_type_str) {
                tracing::debug!(
                    "[mossenHints] dropped hint with unsupported type={}",
                    hint_type_str
                );
                return String::new();
            }
            if value.is_empty() {
                tracing::debug!("[mossenHints] dropped hint with empty value");
                return String::new();
            }

            let hint_type = match hint_type_str {
                "plugin" => MossenHintType::Plugin,
                _ => return String::new(),
            };

            hints.push(MossenHint {
                v,
                hint_type,
                value: value.to_string(),
                source_command: source_command.clone(),
            });
            String::new()
        })
        .to_string();

    // Collapse runs of blank lines introduced by the replace
    let collapsed = if !hints.is_empty() {
        let re = Regex::new(r"\n{3,}").unwrap();
        re.replace_all(&stripped, "\n\n").to_string()
    } else {
        stripped
    };

    ExtractResult {
        hints,
        stripped: collapsed,
    }
}

/// Parse attributes from a tag body string.
fn parse_attrs(tag_body: &str) -> std::collections::HashMap<String, String> {
    let mut attrs = std::collections::HashMap::new();
    for caps in ATTR_RE.captures_iter(tag_body) {
        if let Some(key) = caps.get(1) {
            let value = caps
                .get(2)
                .or_else(|| caps.get(3))
                .map(|m| m.as_str())
                .unwrap_or("");
            attrs.insert(key.as_str().to_string(), value.to_string());
        }
    }
    attrs
}

/// Get the first whitespace-separated token from a command string.
fn first_command_token(command: &str) -> String {
    let trimmed = command.trim();
    match trimmed.find(char::is_whitespace) {
        Some(idx) => trimmed[..idx].to_string(),
        None => trimmed.to_string(),
    }
}

// ============================================================================
// Pending-hint store
// ============================================================================

struct HintStore {
    pending_hint: Option<MossenHint>,
    shown_this_session: bool,
}

static HINT_STORE: Lazy<Mutex<HintStore>> = Lazy::new(|| {
    Mutex::new(HintStore {
        pending_hint: None,
        shown_this_session: false,
    })
});

/// Raw store write. Callers should gate first.
pub fn set_pending_hint(hint: MossenHint) {
    let mut store = HINT_STORE.lock().unwrap();
    if store.shown_this_session {
        return;
    }
    store.pending_hint = Some(hint);
}

/// Clear the slot without flipping the session flag — for rejected hints.
pub fn clear_pending_hint() {
    let mut store = HINT_STORE.lock().unwrap();
    store.pending_hint = None;
}

/// Flip the once-per-session flag. Call only when a dialog is actually shown.
pub fn mark_shown_this_session() {
    let mut store = HINT_STORE.lock().unwrap();
    store.shown_this_session = true;
}

/// Get the current pending hint snapshot.
pub fn get_pending_hint_snapshot() -> Option<MossenHint> {
    let store = HINT_STORE.lock().unwrap();
    store.pending_hint.clone()
}

/// Check if a hint dialog has been shown this session.
pub fn has_shown_hint_this_session() -> bool {
    let store = HINT_STORE.lock().unwrap();
    store.shown_this_session
}

/// Test-only reset.
pub fn reset_mossen_hint_store() {
    let mut store = HINT_STORE.lock().unwrap();
    store.pending_hint = None;
    store.shown_this_session = false;
}

/// 对应 TS `export const _test = {...}`：仅测试用的命名空间集合。
#[doc(hidden)]
#[allow(non_upper_case_globals)]
pub const _test: &str = "_test";
