//! Command splitting and redirection extraction utilities.
//!
//! Translated from `commands.ts` (1340 lines).

use rand::Rng;
use regex::Regex;
use std::collections::HashSet;

use crate::bash::heredoc::{extract_heredocs, restore_heredocs};
use crate::bash::shell_quote::try_parse_shell_command;

// ─── Placeholders ───

struct Placeholders {
    single_quote: String,
    double_quote: String,
    new_line: String,
    escaped_open_paren: String,
    escaped_close_paren: String,
}

fn generate_placeholders() -> Placeholders {
    let mut rng = rand::thread_rng();
    let bytes: [u8; 8] = rng.gen();
    let salt: String = bytes.iter().map(|b| format!("{:02x}", b)).collect();
    Placeholders {
        single_quote: format!("__SINGLE_QUOTE_{}__", salt),
        double_quote: format!("__DOUBLE_QUOTE_{}__", salt),
        new_line: format!("__NEW_LINE_{}__", salt),
        escaped_open_paren: format!("__ESCAPED_OPEN_PAREN_{}__", salt),
        escaped_close_paren: format!("__ESCAPED_CLOSE_PAREN_{}__", salt),
    }
}

lazy_static::lazy_static! {
    static ref ALLOWED_FILE_DESCRIPTORS: HashSet<&'static str> = {
        let mut s = HashSet::new();
        s.insert("0");
        s.insert("1");
        s.insert("2");
        s
    };

    static ref ALL_SUPPORTED_CONTROL_OPERATORS: HashSet<&'static str> = {
        let mut s = HashSet::new();
        s.insert("&&");
        s.insert("||");
        s.insert(";");
        s.insert(";;");
        s.insert("|");
        s.insert(">&");
        s.insert(">");
        s.insert(">>");
        s
    };

    static ref COMMAND_LIST_SEPARATORS: HashSet<&'static str> = {
        let mut s = HashSet::new();
        s.insert("&&");
        s.insert("||");
        s.insert(";");
        s.insert(";;");
        s.insert("|");
        s
    };
}

/// Checks if a redirection target is a simple static file path.
fn is_static_redirect_target(target: &str) -> bool {
    if target.contains(char::is_whitespace) || target.contains('\'') || target.contains('"') {
        return false;
    }
    if target.is_empty() {
        return false;
    }
    if target.starts_with('#') {
        return false;
    }
    !target.starts_with('!')
        && !target.starts_with('=')
        && !target.contains('$')
        && !target.contains('`')
        && !target.contains('*')
        && !target.contains('?')
        && !target.contains('[')
        && !target.contains('{')
        && !target.contains('~')
        && !target.contains('(')
        && !target.contains('<')
        && !target.starts_with('&')
}

/// Join continuation lines (backslash-newline).
fn join_continuation_lines(cmd: &str) -> String {
    let re = Regex::new(r"\\+\n").unwrap();
    re.replace_all(cmd, |caps: &regex::Captures| {
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

/// Splits a command string into individual commands and operators.
pub fn split_command_with_operators(command: &str) -> Vec<String> {
    let placeholders = generate_placeholders();

    let extraction = extract_heredocs(command, None);
    let processed_command = &extraction.processed_command;

    let command_with_continuations_joined = join_continuation_lines(processed_command);
    let command_original_joined = join_continuation_lines(command);

    // Prepare for parsing
    let parse_input = command_with_continuations_joined
        .replace('"', &format!("\"{}", placeholders.double_quote))
        .replace('\'', &format!("'{}", placeholders.single_quote))
        .replace('\n', &format!("\n{}\n", placeholders.new_line))
        .replace("\\(", &placeholders.escaped_open_paren)
        .replace("\\)", &placeholders.escaped_close_paren);

    let parse_result = try_parse_shell_command(&parse_input);

    let parsed = match parse_result {
        Some(tokens) if !tokens.is_empty() => tokens,
        Some(tokens) if tokens.is_empty() => return Vec::new(),
        _ => return vec![command_original_joined],
    };

    // 1. Collapse adjacent strings
    let mut parts: Vec<Option<String>> = Vec::new();
    for part in &parsed {
        if let Some(last) = parts.last_mut() {
            if let Some(last_str) = last {
                if *part == placeholders.new_line {
                    parts.push(None);
                    continue;
                }
                last_str.push(' ');
                last_str.push_str(part);
                continue;
            }
        }
        parts.push(Some(part.clone()));
    }

    // 2. Map back from placeholders
    let quoted_parts: Vec<String> = parts
        .into_iter()
        .filter_map(|p| p)
        .map(|part| {
            part.replace(&placeholders.single_quote, "'")
                .replace(&placeholders.double_quote, "\"")
                .replace(&format!("\n{}\n", placeholders.new_line), "\n")
                .replace(&placeholders.escaped_open_paren, "\\(")
                .replace(&placeholders.escaped_close_paren, "\\)")
        })
        .collect();

    // 3. Restore heredocs
    restore_heredocs(&quoted_parts, &extraction.heredocs)
}

/// Filter control operators from a list of commands and operators.
pub fn filter_control_operators(commands_and_operators: &[String]) -> Vec<String> {
    commands_and_operators
        .iter()
        .filter(|part| !ALL_SUPPORTED_CONTROL_OPERATORS.contains(part.as_str()))
        .cloned()
        .collect()
}

/// Legacy regex/shell-quote path for splitting commands.
pub fn split_command_deprecated(command: &str) -> Vec<String> {
    let mut parts: Vec<Option<String>> = split_command_with_operators(command)
        .into_iter()
        .map(Some)
        .collect();

    for i in 0..parts.len() {
        let part = match &parts[i] {
            Some(p) => p.clone(),
            None => continue,
        };

        if part == ">&" || part == ">" || part == ">>" {
            let prev_part = if i > 0 {
                parts[i - 1].as_ref().map(|s| s.trim().to_string())
            } else {
                None
            };
            let next_part = parts
                .get(i + 1)
                .and_then(|p| p.as_ref().map(|s| s.trim().to_string()));
            let after_next_part = parts
                .get(i + 2)
                .and_then(|p| p.as_ref().map(|s| s.trim().to_string()));

            let next_part_str = match &next_part {
                Some(s) => s.as_str(),
                None => continue,
            };

            let mut should_strip = false;
            let mut strip_third_token = false;

            // Handle FD suffix detection
            let mut effective_next_part = next_part_str.to_string();
            if (part == ">" || part == ">>")
                && next_part_str.len() >= 3
                && next_part_str.chars().nth(next_part_str.len() - 2) == Some(' ')
                && ALLOWED_FILE_DESCRIPTORS.contains(&next_part_str[next_part_str.len() - 1..])
                && after_next_part
                    .as_ref()
                    .map_or(false, |a| a == ">" || a == ">>" || a == ">&")
            {
                effective_next_part = next_part_str[..next_part_str.len() - 2].to_string();
            }

            if part == ">&" && ALLOWED_FILE_DESCRIPTORS.contains(next_part_str) {
                should_strip = true;
            } else if part == ">"
                && next_part_str == "&"
                && after_next_part
                    .as_ref()
                    .map_or(false, |a| ALLOWED_FILE_DESCRIPTORS.contains(a.as_str()))
            {
                should_strip = true;
                strip_third_token = true;
            } else if part == ">"
                && next_part_str.starts_with('&')
                && next_part_str.len() > 1
                && ALLOWED_FILE_DESCRIPTORS.contains(&next_part_str[1..])
            {
                should_strip = true;
            } else if (part == ">" || part == ">>")
                && is_static_redirect_target(&effective_next_part)
            {
                should_strip = true;
            }

            if should_strip {
                if let Some(ref prev) = prev_part {
                    if prev.len() >= 3
                        && ALLOWED_FILE_DESCRIPTORS.contains(&prev[prev.len() - 1..])
                        && prev.chars().nth(prev.len() - 2) == Some(' ')
                    {
                        parts[i - 1] = Some(prev[..prev.len() - 2].to_string());
                    }
                }
                parts[i] = None;
                if i + 1 < parts.len() {
                    parts[i + 1] = None;
                }
                if strip_third_token && i + 2 < parts.len() {
                    parts[i + 2] = None;
                }
            }
        }
    }

    let string_parts: Vec<String> = parts
        .into_iter()
        .filter_map(|p| p)
        .filter(|p| !p.is_empty())
        .collect();
    filter_control_operators(&string_parts)
}

/// Checks if a command is a help command.
pub fn is_help_command(command: &str) -> bool {
    let trimmed = command.trim();
    if !trimmed.ends_with("--help") {
        return false;
    }
    if trimmed.contains('"') || trimmed.contains('\'') {
        return false;
    }

    let parse_result = try_parse_shell_command(trimmed);
    let tokens = match parse_result {
        Some(t) => t,
        None => return false,
    };

    let alphanumeric_pattern = Regex::new(r"^[a-zA-Z0-9]+$").unwrap();
    let mut found_help = false;

    for token in &tokens {
        if token.starts_with('-') {
            if token == "--help" {
                found_help = true;
            } else {
                return false;
            }
        } else if !alphanumeric_pattern.is_match(token) {
            return false;
        }
    }

    found_help
}

/// Checks if a command is a command list (safe compound).
fn is_command_list(command: &str) -> bool {
    let placeholders = generate_placeholders();
    let extraction = extract_heredocs(command, None);

    let parse_input = extraction
        .processed_command
        .replace('"', &format!("\"{}", placeholders.double_quote))
        .replace('\'', &format!("'{}", placeholders.single_quote));

    let parts = match try_parse_shell_command(&parse_input) {
        Some(t) => t,
        None => return false,
    };

    for part in &parts {
        // Check if it's an operator
        if COMMAND_LIST_SEPARATORS.contains(part.as_str()) {
            continue;
        }
        if part == ">" || part == ">>" || part == ">&" {
            continue;
        }
        // All other parts are strings (safe)
    }
    true
}

/// Checks if a compound command is unsafe (legacy).
pub fn is_unsafe_compound_command_deprecated(command: &str) -> bool {
    let extraction = extract_heredocs(command, None);
    let parse_result = try_parse_shell_command(&extraction.processed_command);
    if parse_result.is_none() {
        return true;
    }
    split_command_deprecated(command).len() > 1 && !is_command_list(command)
}

/// Output redirection info.
#[derive(Debug, Clone)]
pub struct OutputRedirection {
    pub target: String,
    pub operator: String, // ">" or ">>"
}

/// Result of extracting output redirections.
#[derive(Debug, Clone)]
pub struct ExtractRedirectionsResult {
    pub command_without_redirections: String,
    pub redirections: Vec<OutputRedirection>,
    pub has_dangerous_redirection: bool,
}

/// Extracts output redirections from a command.
pub fn extract_output_redirections(cmd: &str) -> ExtractRedirectionsResult {
    let mut redirections: Vec<OutputRedirection> = Vec::new();
    let mut has_dangerous_redirection = false;

    let extraction = extract_heredocs(cmd, None);
    let processed_command = join_continuation_lines(&extraction.processed_command);

    let parse_result = try_parse_shell_command(&processed_command);

    if parse_result.is_none() {
        return ExtractRedirectionsResult {
            command_without_redirections: cmd.to_string(),
            redirections: Vec::new(),
            has_dangerous_redirection: true,
        };
    }

    let parsed = parse_result.unwrap();
    if parsed.is_empty() {
        return ExtractRedirectionsResult {
            command_without_redirections: cmd.to_string(),
            redirections,
            has_dangerous_redirection: false,
        };
    }

    // Simple approach: scan for > and >> operators
    let mut kept: Vec<String> = Vec::new();
    let mut i = 0;
    while i < parsed.len() {
        let part = &parsed[i];

        if (part == ">" || part == ">>") && i + 1 < parsed.len() {
            let target = &parsed[i + 1];
            if is_simple_target(target) {
                redirections.push(OutputRedirection {
                    target: target.clone(),
                    operator: part.clone(),
                });
                i += 2;
                continue;
            } else if has_dangerous_expansion(target) {
                has_dangerous_redirection = true;
            }
        } else if part == ">&" && i + 1 < parsed.len() {
            let target = &parsed[i + 1];
            if is_file_descriptor(target) {
                // fd-to-fd redirect, keep as is
                kept.push(part.clone());
                kept.push(target.clone());
                i += 2;
                continue;
            } else if is_simple_target(target) {
                redirections.push(OutputRedirection {
                    target: target.clone(),
                    operator: ">".to_string(),
                });
                i += 2;
                continue;
            } else if has_dangerous_expansion(target) {
                has_dangerous_redirection = true;
            }
        }

        kept.push(part.clone());
        i += 1;
    }

    let reconstructed = kept.join(" ");
    let restored = restore_heredocs(&[reconstructed], &extraction.heredocs);

    ExtractRedirectionsResult {
        command_without_redirections: restored
            .into_iter()
            .next()
            .unwrap_or_else(|| cmd.to_string()),
        redirections,
        has_dangerous_redirection,
    }
}

fn is_simple_target(target: &str) -> bool {
    if target.is_empty() {
        return false;
    }
    !target.starts_with('!')
        && !target.starts_with('=')
        && !target.starts_with('~')
        && !target.contains('$')
        && !target.contains('`')
        && !target.contains('*')
        && !target.contains('?')
        && !target.contains('[')
        && !target.contains('{')
}

fn has_dangerous_expansion(target: &str) -> bool {
    if target.is_empty() {
        return false;
    }
    target.contains('$')
        || target.contains('%')
        || target.contains('`')
        || target.contains('*')
        || target.contains('?')
        || target.contains('[')
        || target.contains('{')
        || target.starts_with('!')
        || target.starts_with('=')
        || target.starts_with('~')
}

fn is_file_descriptor(s: &str) -> bool {
    let trimmed = s.trim();
    trimmed.chars().all(|c| c.is_ascii_digit()) && !trimmed.is_empty()
}

/// Clear command prefix caches (no-op in Rust static implementation).
pub fn clear_command_prefix_caches() {
    // No memoization cache in Rust implementation
}

/// 对应 TS `getCommandSubcommandPrefix`：返回 `cmd subcmd` 前缀（去掉参数）。
pub fn get_command_subcommand_prefix(command: &str) -> String {
    let mut iter = command.split_whitespace();
    let cmd = iter.next().unwrap_or("");
    let sub = iter.next().unwrap_or("");
    if sub.is_empty() || sub.starts_with('-') {
        cmd.to_string()
    } else {
        format!("{} {}", cmd, sub)
    }
}
