//! Shell quoting utilities.
//!
//! Translated from `shellQuoting.ts` (129 lines).

use regex::Regex;

/// Detects if a command contains a heredoc pattern.
pub fn contains_heredoc(command: &str) -> bool {
    let digit_shift = Regex::new(r"\d\s*<<\s*\d").unwrap();
    let bracket_shift = Regex::new(r"\[\[\s*\d+\s*<<\s*\d+\s*\]\]").unwrap();
    let arith_shift = Regex::new(r"\$\(\(.*<<.*\)\)").unwrap();
    if digit_shift.is_match(command)
        || bracket_shift.is_match(command)
        || arith_shift.is_match(command)
    {
        return false;
    }
    let heredoc_regex = Regex::new(r#"<<-?\s*(?:(['"])(\w+)\1|\\(\w+))"#).unwrap();
    heredoc_regex.is_match(command)
}

/// Detects if a command contains multiline strings in quotes.
pub fn contains_multiline_string(command: &str) -> bool {
    let single_quote_ml = Regex::new(r"'(?:[^'\\]|\\.)*\n(?:[^'\\]|\\.)*'").unwrap();
    let double_quote_ml = Regex::new(r#""(?:[^"\\]|\\.)*\n(?:[^"\\]|\\.)*""#).unwrap();
    single_quote_ml.is_match(command) || double_quote_ml.is_match(command)
}

/// Quotes a shell command appropriately, preserving heredocs and multiline strings.
pub fn quote_shell_command(command: &str, add_stdin_redirect: bool) -> String {
    if contains_heredoc(command) || contains_multiline_string(command) {
        let escaped = command.replace('\'', "'\"'\"'");
        let quoted = format!("'{}'", escaped);
        if contains_heredoc(command) {
            return quoted;
        }
        if add_stdin_redirect {
            return format!("{} < /dev/null", quoted);
        }
        return quoted;
    }
    if add_stdin_redirect {
        return crate::bash::shell_quote::quote(&[command, "<", "/dev/null"]);
    }
    crate::bash::shell_quote::quote(&[command])
}

/// Detects if a command already has a stdin redirect.
pub fn has_stdin_redirect(command: &str) -> bool {
    let re = Regex::new(r"(?:^|[\s;&|])<(?![<(])\s*\S+").unwrap();
    re.is_match(command)
}

/// Checks if stdin redirect should be added to a command.
pub fn should_add_stdin_redirect(command: &str) -> bool {
    if contains_heredoc(command) {
        return false;
    }
    if has_stdin_redirect(command) {
        return false;
    }
    true
}

/// Rewrites Windows CMD-style `>nul` redirects to POSIX `/dev/null`.
pub fn rewrite_windows_null_redirect(command: &str) -> String {
    let re = Regex::new(r"(\d?&?>+\s*)[Nn][Uu][Ll](?=\s|$|[|&;)\n])").unwrap();
    re.replace_all(command, "${1}/dev/null").to_string()
}
