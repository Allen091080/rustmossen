//! Typeahead hook (useTypeahead.ts).
//! Provides typeahead/autocomplete for the input.

#[derive(Debug, Clone)]
pub struct TypeaheadState {
    pub active: bool,
    pub initialized: bool,
}

impl TypeaheadState {
    pub fn new() -> Self {
        Self {
            active: false,
            initialized: false,
        }
    }
    pub fn initialize(&mut self) {
        self.initialized = true;
    }
    pub fn activate(&mut self) {
        self.active = true;
    }
    pub fn deactivate(&mut self) {
        self.active = false;
    }
    pub fn is_active(&self) -> bool {
        self.active
    }
}
impl Default for TypeaheadState {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Helper functions for typeahead matching.
// ============================================================================

/// A completion token result from cursor scanning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionToken {
    pub token: String,
    pub start_pos: usize,
    pub is_quoted: bool,
}

/// Result of applying a directory/file suggestion.
#[derive(Debug, Clone)]
pub struct ApplySuggestionResult {
    pub new_input: String,
    pub cursor_pos: usize,
}

/// Options for `format_replacement_value`.
#[derive(Debug, Clone)]
pub struct ReplacementValueOptions<'a> {
    pub display_text: &'a str,
    pub mode: &'a str,
    pub has_at_prefix: bool,
    pub needs_quotes: bool,
    pub is_quoted: bool,
    pub is_complete: bool,
}

/// Shell completion type passed to `apply_shell_suggestion`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellCompletionType {
    Variable,
    Command,
    Path,
}

/// Extract search token from a completion token by removing @ prefix and
/// quotes.
///
/// TS source: `extractSearchToken(completionToken)`.
pub fn extract_search_token(completion_token: &CompletionToken) -> String {
    if completion_token.is_quoted {
        // Remove @" prefix and optional closing "
        let s = if completion_token.token.len() >= 2 {
            &completion_token.token[2..]
        } else {
            ""
        };
        if let Some(stripped) = s.strip_suffix('"') {
            stripped.to_string()
        } else {
            s.to_string()
        }
    } else if completion_token.token.starts_with('@') {
        completion_token.token[1..].to_string()
    } else {
        completion_token.token.clone()
    }
}

/// Format a replacement value with proper @ prefix and quotes based on
/// context.
///
/// TS source: `formatReplacementValue(options)`.
pub fn format_replacement_value(opts: &ReplacementValueOptions<'_>) -> String {
    let space = if opts.is_complete { " " } else { "" };
    if opts.is_quoted || opts.needs_quotes {
        if opts.mode == "bash" {
            format!("\"{}\"{}", opts.display_text, space)
        } else {
            format!("@\"{}\"{}", opts.display_text, space)
        }
    } else if opts.has_at_prefix {
        if opts.mode == "bash" {
            format!("{}{}", opts.display_text, space)
        } else {
            format!("@{}{}", opts.display_text, space)
        }
    } else {
        opts.display_text.to_string()
    }
}

/// Apply a shell completion suggestion by replacing the current word.
///
/// TS source: `applyShellSuggestion(...)`. Returns the new input and new
/// cursor offset (since Rust can't easily mutate via callbacks like the TS
/// version does).
pub fn apply_shell_suggestion(
    suggestion_display_text: &str,
    input: &str,
    cursor_offset: usize,
    completion_type: Option<ShellCompletionType>,
) -> ApplySuggestionResult {
    let cursor_offset = cursor_offset.min(input.len());
    let before_cursor = &input[..cursor_offset];
    let last_space = before_cursor.rfind(' ');
    let word_start = last_space.map(|i| i + 1).unwrap_or(0);

    let replacement_text = match completion_type {
        Some(ShellCompletionType::Variable) => format!("${} ", suggestion_display_text),
        Some(ShellCompletionType::Command) => format!("{} ", suggestion_display_text),
        _ => suggestion_display_text.to_string(),
    };

    let mut new_input = String::with_capacity(input.len() + replacement_text.len());
    new_input.push_str(&input[..word_start]);
    new_input.push_str(&replacement_text);
    new_input.push_str(&input[cursor_offset..]);

    let cursor_pos = word_start + replacement_text.len();
    ApplySuggestionResult {
        new_input,
        cursor_pos,
    }
}

/// Apply a directory/file suggestion to the input.
///
/// TS source: `applyDirectorySuggestion(...)`. Always prepends `@` and
/// appends `/` for directories or a space for files.
pub fn apply_directory_suggestion(
    input: &str,
    suggestion_id: &str,
    token_start_pos: usize,
    token_length: usize,
    is_directory: bool,
) -> ApplySuggestionResult {
    let suffix = if is_directory { '/' } else { ' ' };
    let token_end = (token_start_pos + token_length).min(input.len());
    let before = &input[..token_start_pos.min(input.len())];
    let after = &input[token_end..];
    let replacement = format!("@{}{}", suggestion_id, suffix);
    let mut new_input = String::with_capacity(before.len() + replacement.len() + after.len());
    new_input.push_str(before);
    new_input.push_str(&replacement);
    new_input.push_str(after);
    let cursor_pos = before.len() + replacement.len();
    ApplySuggestionResult {
        new_input,
        cursor_pos,
    }
}

/// True if this character is one of the path-token characters used by the
/// TS regex `[\p{L}\p{N}\p{M}_\-./\\()[\]~:]`. The Rust port treats unicode
/// letters/digits/marks the same way: `is_alphanumeric()` covers L+N for
/// the typical CJK/Latin/Cyrillic cases and the explicit punctuation set
/// matches the regex literal.
fn is_path_token_char(c: char) -> bool {
    if c.is_alphanumeric() {
        return true;
    }
    matches!(
        c,
        '_' | '-' | '.' | '/' | '\\' | '(' | ')' | '[' | ']' | '~' | ':'
    )
}

/// Extract a completable token at the cursor position.
///
/// TS source: `extractCompletionToken(text, cursorPos, includeAtSymbol)`.
pub fn extract_completion_token(
    text: &str,
    cursor_pos: usize,
    include_at_symbol: bool,
) -> Option<CompletionToken> {
    if text.is_empty() {
        return None;
    }
    let cursor_pos = cursor_pos.min(text.len());
    let text_before: &str = &text[..cursor_pos];
    let text_after: &str = &text[cursor_pos..];

    // Quoted @"..." mention path.
    if include_at_symbol {
        if let Some(at_idx) = text_before.rfind("@\"") {
            // Make sure no `"` exists between at_idx+2 and end of before:
            // the regex /@"([^"]*)"?$/ requires the content to be free of
            // closing quotes.
            let after_open = &text_before[at_idx + 2..];
            if !after_open.contains('"') {
                // Try to capture content after cursor up to closing quote (or
                // end).
                let mut suffix_end = 0;
                for (i, c) in text_after.char_indices() {
                    if c == '"' {
                        suffix_end = i + c.len_utf8();
                        break;
                    }
                    suffix_end = i + c.len_utf8();
                }
                let quoted_suffix = &text_after[..suffix_end];
                let token = format!("{}{}", &text_before[at_idx..], quoted_suffix);
                return Some(CompletionToken {
                    token,
                    start_pos: at_idx,
                    is_quoted: true,
                });
            }
        }
    }

    // Fast path for @ tokens.
    if include_at_symbol {
        if let Some(at_idx) = text_before.rfind('@') {
            // Ensure @ is at start of input or preceded by whitespace.
            let prev_ok = at_idx == 0
                || text_before[..at_idx]
                    .chars()
                    .next_back()
                    .map(|c| c.is_whitespace())
                    .unwrap_or(false);
            if prev_ok {
                let from_at = &text_before[at_idx..];
                // From_at must start with '@' followed by path token chars.
                // Equivalent to AT_TOKEN_HEAD_RE matching the entire from_at.
                let mut idx = '@'.len_utf8();
                let bytes_after_at = &from_at[idx..];
                for c in bytes_after_at.chars() {
                    if !is_path_token_char(c) {
                        break;
                    }
                    idx += c.len_utf8();
                }
                if idx == from_at.len() {
                    // Consume trailing path-char run after the cursor.
                    let mut suffix_end = 0;
                    for c in text_after.chars() {
                        if !is_path_token_char(c) {
                            break;
                        }
                        suffix_end += c.len_utf8();
                    }
                    let token = format!("{}{}", from_at, &text_after[..suffix_end]);
                    return Some(CompletionToken {
                        token,
                        start_pos: at_idx,
                        is_quoted: false,
                    });
                }
            }
        }
    }

    // Plain token. Equivalent of /[\p{L}...]+$/ — find the longest trailing
    // run of path token chars (and optionally a leading `@`).
    let mut start_pos = cursor_pos;
    // Walk backward over path token chars.
    for (i, c) in text_before.char_indices().rev() {
        if is_path_token_char(c) {
            start_pos = i;
        } else {
            break;
        }
    }
    if include_at_symbol && start_pos > 0 {
        // Allow a leading '@' just before the token (matches TS regex
        // alternation).
        let prev_byte_end = start_pos;
        let prev_char = text_before[..prev_byte_end].chars().next_back();
        if let Some('@') = prev_char {
            start_pos -= '@'.len_utf8();
        }
    }
    if start_pos == cursor_pos {
        return None;
    }
    let head = &text_before[start_pos..];
    let mut suffix_end = 0;
    for c in text_after.chars() {
        if !is_path_token_char(c) {
            break;
        }
        suffix_end += c.len_utf8();
    }
    let tail = &text_after[..suffix_end];
    let token = format!("{}{}", head, tail);
    Some(CompletionToken {
        token,
        start_pos,
        is_quoted: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_search_token_unquoted_at() {
        let t = CompletionToken {
            token: "@foo".to_string(),
            start_pos: 0,
            is_quoted: false,
        };
        assert_eq!(extract_search_token(&t), "foo");
    }

    #[test]
    fn extract_search_token_quoted() {
        let t = CompletionToken {
            token: "@\"hello world\"".to_string(),
            start_pos: 0,
            is_quoted: true,
        };
        assert_eq!(extract_search_token(&t), "hello world");
    }

    #[test]
    fn extract_search_token_quoted_open() {
        let t = CompletionToken {
            token: "@\"abc".to_string(),
            start_pos: 0,
            is_quoted: true,
        };
        assert_eq!(extract_search_token(&t), "abc");
    }

    #[test]
    fn format_replacement_value_at_complete() {
        let v = format_replacement_value(&ReplacementValueOptions {
            display_text: "foo.txt",
            mode: "prompt",
            has_at_prefix: true,
            needs_quotes: false,
            is_quoted: false,
            is_complete: true,
        });
        assert_eq!(v, "@foo.txt ");
    }

    #[test]
    fn format_replacement_value_bash_quoted() {
        let v = format_replacement_value(&ReplacementValueOptions {
            display_text: "my file",
            mode: "bash",
            has_at_prefix: false,
            needs_quotes: true,
            is_quoted: false,
            is_complete: false,
        });
        assert_eq!(v, "\"my file\"");
    }

    #[test]
    fn apply_shell_suggestion_variable() {
        let r = apply_shell_suggestion("HOME", "echo $", 6, Some(ShellCompletionType::Variable));
        assert_eq!(r.new_input, "echo $HOME ");
        assert_eq!(r.cursor_pos, 11);
    }

    #[test]
    fn apply_directory_suggestion_dir() {
        let r = apply_directory_suggestion("see @src", "src/components", 4, 4, true);
        assert_eq!(r.new_input, "see @src/components/");
        assert_eq!(r.cursor_pos, 20);
    }

    #[test]
    fn extract_completion_token_at() {
        let r = extract_completion_token("look @foo/b", 11, true).unwrap();
        assert_eq!(r.token, "@foo/b");
        assert_eq!(r.start_pos, 5);
        assert!(!r.is_quoted);
    }

    #[test]
    fn extract_completion_token_quoted() {
        let r = extract_completion_token("@\"hello", 7, true).unwrap();
        assert_eq!(r.is_quoted, true);
        assert_eq!(r.start_pos, 0);
    }

    #[test]
    fn extract_completion_token_plain() {
        let r = extract_completion_token("hello world.txt", 15, false).unwrap();
        assert_eq!(r.token, "world.txt");
        assert_eq!(r.start_pos, 6);
    }
}
