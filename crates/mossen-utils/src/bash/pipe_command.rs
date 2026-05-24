//! Pipe command rearrangement for stdin redirect handling.
//!
//! Translated from `bashPipeCommand.ts` (295 lines).

use regex::Regex;

use crate::bash::shell_quote::{
    has_malformed_tokens, has_shell_quote_single_quote_bug, quote, try_parse_shell_command,
};

/// Rearranges a command with pipes to place stdin redirect after the first command.
pub fn rearrange_pipe_command(command: &str) -> String {
    // Skip if command has backticks
    if command.contains('`') {
        return quote_with_eval_stdin_redirect(command);
    }

    // Skip if command has command substitution
    if command.contains("$(") {
        return quote_with_eval_stdin_redirect(command);
    }

    // Skip if command references shell variables
    let var_re = Regex::new(r"\$[A-Za-z_\{]").unwrap();
    if var_re.is_match(command) {
        return quote_with_eval_stdin_redirect(command);
    }

    // Skip if command contains bash control structures
    if contains_control_structure(command) {
        return quote_with_eval_stdin_redirect(command);
    }

    // Join continuation lines before parsing
    let joined = join_continuation_lines(command);

    // Bail if remaining newlines
    if joined.contains('\n') {
        return quote_with_eval_stdin_redirect(command);
    }

    // Check for shell-quote single-quote bug
    if has_shell_quote_single_quote_bug(&joined) {
        return quote_with_eval_stdin_redirect(command);
    }

    let parsed = match try_parse_shell_command(&joined) {
        Some(tokens) => tokens,
        None => return quote_with_eval_stdin_redirect(command),
    };

    // Check for malformed tokens
    if has_malformed_tokens(&joined) {
        return quote_with_eval_stdin_redirect(command);
    }

    let first_pipe_index = find_first_pipe_operator(&parsed);

    if first_pipe_index <= 0 {
        return quote_with_eval_stdin_redirect(command);
    }

    // Rebuild: first_command < /dev/null | rest_of_pipeline
    let mut parts: Vec<String> = Vec::new();
    parts.extend(build_command_parts(&parsed, 0, first_pipe_index as usize));
    parts.push("< /dev/null".to_string());
    parts.extend(build_command_parts(
        &parsed,
        first_pipe_index as usize,
        parsed.len(),
    ));

    single_quote_for_eval(&parts.join(" "))
}

/// Finds the index of the first pipe operator in parsed shell command.
fn find_first_pipe_operator(parsed: &[String]) -> i32 {
    for (i, entry) in parsed.iter().enumerate() {
        if entry == "|" {
            return i as i32;
        }
    }
    -1
}

/// Builds command parts from parsed entries.
fn build_command_parts(parsed: &[String], start: usize, end: usize) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut seen_non_env_var = false;
    let env_var_re = Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*=").unwrap();

    let mut i = start;
    while i < end {
        let entry = &parsed[i];

        // Check for file descriptor redirections (e.g., 2>&1, 2>/dev/null)
        let fd_re = Regex::new(r"^[012]$").unwrap();
        if fd_re.is_match(entry) && i + 2 < end {
            let op = &parsed[i + 1];
            let target = &parsed[i + 2];

            // Handle 2>&1 style
            if op == ">&" && fd_re.is_match(target) {
                parts.push(format!("{}>&{}", entry, target));
                i += 3;
                continue;
            }
            // Handle 2>/dev/null style
            if op == ">" && target == "/dev/null" {
                parts.push(format!("{}>/dev/null", entry));
                i += 3;
                continue;
            }
            // Handle 2> &1 style
            if op == ">" && target.starts_with('&') {
                let fd = &target[1..];
                if fd_re.is_match(fd) {
                    parts.push(format!("{}>&{}", entry, fd));
                    i += 3;
                    continue;
                }
            }
        }

        // Handle regular entries
        if is_operator_token(entry) {
            parts.push(entry.clone());
            if is_command_separator(entry) {
                seen_non_env_var = false;
            }
        } else {
            let is_env_var = !seen_non_env_var && env_var_re.is_match(entry);
            if is_env_var {
                let eq_index = entry.find('=').unwrap();
                let name = &entry[..eq_index];
                let value = &entry[eq_index + 1..];
                let quoted_value = quote(&[value]);
                parts.push(format!("{}={}", name, quoted_value));
            } else {
                seen_non_env_var = true;
                parts.push(quote(&[entry]));
            }
        }
        i += 1;
    }

    parts
}

/// Check if a token is a shell operator.
fn is_operator_token(token: &str) -> bool {
    matches!(
        token,
        "|" | "||" | "&&" | ";" | ">" | ">>" | "<" | "<<" | ">&" | "<&"
    )
}

/// Checks if an operator is a command separator.
fn is_command_separator(op: &str) -> bool {
    op == "&&" || op == "||" || op == ";"
}

/// Checks if a command contains bash control structures.
fn contains_control_structure(command: &str) -> bool {
    let re = Regex::new(r"\b(for|while|until|if|case|select)\s").unwrap();
    re.is_match(command)
}

/// Quotes a command and adds `< /dev/null` as a shell redirect on eval.
fn quote_with_eval_stdin_redirect(command: &str) -> String {
    format!("{} < /dev/null", single_quote_for_eval(command))
}

/// Single-quote a string for use as an eval argument.
fn single_quote_for_eval(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\"'\"'"))
}

/// Joins shell continuation lines.
fn join_continuation_lines(command: &str) -> String {
    let re = Regex::new(r"\\+\n").unwrap();
    re.replace_all(command, |caps: &regex::Captures| {
        let m = caps.get(0).unwrap().as_str();
        let backslash_count = m.len() - 1;
        if backslash_count % 2 == 1 {
            "\\".repeat(backslash_count - 1)
        } else {
            m.to_string()
        }
    })
    .to_string()
}
