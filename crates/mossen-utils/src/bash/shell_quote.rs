//! Shell quoting — equivalent of the `shell-quote` npm package.
//!
//! Translated from `shellQuote.ts` (304 lines).

use regex::Regex;

/// Quote an array of arguments into a shell-safe string.
pub fn quote(args: &[&str]) -> String {
    args.iter()
        .map(|a| quote_single(a))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Quote a single argument for shell safety.
fn quote_single(s: &str) -> String {
    if s.is_empty() {
        return "''".to_string();
    }
    // If the string contains only safe chars, return as-is
    let safe_re = Regex::new(r"^[a-zA-Z0-9@%+=:,./-]+$").unwrap();
    if safe_re.is_match(s) {
        return s.to_string();
    }
    // Single-quote the string, escaping embedded single quotes
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Try to parse a shell command string into tokens (simplified shell-quote parse).
pub fn try_parse_shell_command(cmd: &str) -> Option<Vec<String>> {
    parse_shell_tokens(cmd)
}

/// Try to quote shell arguments with proper escaping.
pub fn try_quote_shell_args(args: &[&str]) -> Option<String> {
    Some(quote(args))
}

/// Parse a shell command into tokens, handling quotes and escapes.
fn parse_shell_tokens(input: &str) -> Option<Vec<String>> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        let c = chars[i];
        match c {
            ' ' | '\t' => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
                i += 1;
            }
            '\'' => {
                // Single-quoted string — no escaping inside
                i += 1;
                while i < len && chars[i] != '\'' {
                    current.push(chars[i]);
                    i += 1;
                }
                if i >= len {
                    return None; // Unterminated quote
                }
                i += 1; // Skip closing quote
            }
            '"' => {
                // Double-quoted string — only backslash escaping
                i += 1;
                while i < len && chars[i] != '"' {
                    if chars[i] == '\\' && i + 1 < len {
                        let next = chars[i + 1];
                        if matches!(next, '$' | '`' | '"' | '\\' | '\n') {
                            current.push(next);
                            i += 2;
                            continue;
                        }
                    }
                    current.push(chars[i]);
                    i += 1;
                }
                if i >= len {
                    return None; // Unterminated quote
                }
                i += 1; // Skip closing quote
            }
            '\\' => {
                // Backslash escape
                if i + 1 < len {
                    if chars[i + 1] == '\n' {
                        // Line continuation — skip
                        i += 2;
                    } else {
                        current.push(chars[i + 1]);
                        i += 2;
                    }
                } else {
                    current.push('\\');
                    i += 1;
                }
            }
            // Shell operators — return as separate tokens
            '|' | '&' | ';' | '<' | '>' | '(' | ')' => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
                let mut op = c.to_string();
                // Check for multi-char operators
                if i + 1 < len {
                    let next = chars[i + 1];
                    if (c == '|' && next == '|')
                        || (c == '&' && next == '&')
                        || (c == ';' && next == ';')
                        || (c == '>' && next == '>')
                        || (c == '<' && next == '<')
                        || (c == '>' && next == '&')
                        || (c == '<' && next == '&')
                    {
                        op.push(next);
                        i += 1;
                    }
                }
                tokens.push(op);
                i += 1;
            }
            '#'
                // Comment — rest of line is ignored
                if current.is_empty()
                    && (tokens.is_empty() || tokens.last().map(|s| s.as_str()) == Some("\n"))
                => {
                    while i < len && chars[i] != '\n' {
                        i += 1;
                    }
                }
            '\n' => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
                i += 1;
            }
            _ => {
                current.push(c);
                i += 1;
            }
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    Some(tokens)
}

/// Check if a command has malformed tokens (unbalanced brackets/braces/quotes).
pub fn has_malformed_tokens(cmd: &str) -> bool {
    let mut paren_depth: i32 = 0;
    let mut bracket_depth: i32 = 0;
    let mut brace_depth: i32 = 0;
    let chars: Vec<char> = cmd.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        let c = chars[i];
        match c {
            '\'' => {
                i += 1;
                while i < len && chars[i] != '\'' {
                    i += 1;
                }
                if i >= len {
                    return true;
                }
                i += 1;
            }
            '"' => {
                i += 1;
                while i < len && chars[i] != '"' {
                    if chars[i] == '\\' && i + 1 < len {
                        i += 1;
                    }
                    i += 1;
                }
                if i >= len {
                    return true;
                }
                i += 1;
            }
            '(' => {
                paren_depth += 1;
                i += 1;
            }
            ')' => {
                paren_depth -= 1;
                i += 1;
            }
            '[' => {
                bracket_depth += 1;
                i += 1;
            }
            ']' => {
                bracket_depth -= 1;
                i += 1;
            }
            '{' => {
                brace_depth += 1;
                i += 1;
            }
            '}' => {
                brace_depth -= 1;
                i += 1;
            }
            '\\' => {
                i += 2;
            }
            _ => {
                i += 1;
            }
        }
        if paren_depth < 0 || bracket_depth < 0 || brace_depth < 0 {
            return true;
        }
    }
    paren_depth != 0 || bracket_depth != 0 || brace_depth != 0
}

/// Detect shell-quote's single-quote bug differential between bash and the library.
pub fn has_shell_quote_single_quote_bug(cmd: &str) -> bool {
    // The TS version detects cases where shell-quote produces different output
    // than bash for single-quoted strings containing backslash sequences.
    // In Rust we implement our own parser so this differential doesn't apply.
    let _ = cmd;
    false
}

/// 对应 TS `ShellParseResult`：shell-quote parse 结果。
#[derive(Debug, Clone, Default)]
pub struct ShellParseResult {
    pub tokens: Vec<String>,
    pub success: bool,
}

/// 对应 TS `ShellQuoteResult`：shell-quote quote 结果。
#[derive(Debug, Clone, Default)]
pub struct ShellQuoteResult {
    pub quoted: String,
}
