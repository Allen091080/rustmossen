//! Bash command helpers — compound command permission checking.
//!
//! Corresponds to `bashCommandHelpers.ts` (266 lines). Handles piped commands,
//! segmented permission checks, and operator-level security validation.

use crate::bash_tool::bash_permissions::{
    self, BashPermissionRule, PermissionBehavior, PermissionResult, PermissionUpdate,
    bash_tool_has_permission, is_normalized_cd_command, is_normalized_git_command,
};

/// Command identity checkers for security validation.
pub struct CommandIdentityCheckers;

impl CommandIdentityCheckers {
    pub fn is_cd_command(command: &str) -> bool {
        is_normalized_cd_command(command)
    }

    pub fn is_git_command(command: &str) -> bool {
        is_normalized_git_command(command)
    }
}

/// Check segmented command permissions.
///
/// When a command is split into pipe segments, each segment is checked
/// individually through the permission system. Returns the most restrictive result.
pub async fn segmented_command_permission_result(
    full_command: &str,
    segments: &[String],
    allow_rules: &[String],
    deny_rules: &[String],
) -> PermissionResult {
    // Check for multiple cd commands across all segments
    let cd_commands: Vec<&String> = segments
        .iter()
        .filter(|seg| CommandIdentityCheckers::is_cd_command(seg.trim()))
        .collect();

    if cd_commands.len() > 1 {
        return PermissionResult::ask(
            "Multiple directory changes in one command require approval for clarity".to_string(),
        );
    }

    // SECURITY: Check for cd+git across pipe segments to prevent bare repo fsmonitor bypass
    {
        let mut has_cd = false;
        let mut has_git = false;

        for segment in segments {
            let subcommands = split_command_deprecated(segment);
            for sub in &subcommands {
                let trimmed = sub.trim();
                if CommandIdentityCheckers::is_cd_command(trimmed) {
                    has_cd = true;
                }
                if CommandIdentityCheckers::is_git_command(trimmed) {
                    has_git = true;
                }
            }
        }

        if has_cd && has_git {
            return PermissionResult::ask(
                "Compound commands with cd and git require approval to prevent bare repository attacks".to_string(),
            );
        }
    }

    // Check each segment through the full permission system
    let mut all_allowed = true;
    let mut any_denied = false;
    let mut denied_message = String::new();
    let mut suggestions: Vec<PermissionUpdate> = Vec::new();

    for segment in segments {
        let trimmed = segment.trim();
        if trimmed.is_empty() {
            continue;
        }

        let result = bash_tool_has_permission(trimmed, allow_rules, deny_rules);

        match result.behavior {
            PermissionBehavior::Deny => {
                any_denied = true;
                denied_message = result
                    .message
                    .unwrap_or_else(|| format!("Permission denied for: {}", trimmed));
                break;
            }
            PermissionBehavior::Ask => {
                all_allowed = false;
                suggestions.extend(result.suggestions);
            }
            PermissionBehavior::Allow => {}
            PermissionBehavior::Passthrough => {
                all_allowed = false;
            }
        }
    }

    if any_denied {
        return PermissionResult::deny(denied_message);
    }

    if all_allowed {
        return PermissionResult::allow(Some(full_command.to_string()));
    }

    // Collect suggestions from segments that need approval
    let mut result = PermissionResult::ask(format!(
        "Command requires approval: {}",
        truncate(full_command, 80)
    ));
    result.suggestions = suggestions;
    result
}

/// Build a command segment without output redirections.
/// Strips `>`, `>>`, `2>`, etc. to avoid treating filenames as commands.
pub fn build_segment_without_redirections(segment: &str) -> String {
    // Fast path: skip parsing if no redirection operators present
    if !segment.contains('>') {
        return segment.to_string();
    }

    // Simple redirection stripping: remove > filename, >> filename, 2> filename patterns
    let re = regex::Regex::new(r"\s*[12]?>>?\s*\S+").unwrap();
    re.replace_all(segment, "").trim().to_string()
}

/// Check command operator permissions.
///
/// Validates compound commands with pipes, checking each segment independently.
pub async fn check_command_operator_permissions(
    command: &str,
    allow_rules: &[String],
    deny_rules: &[String],
) -> PermissionResult {
    // Split by pipe
    let pipe_segments = split_by_pipe(command);

    // If no pipes (single segment), let normal flow handle it
    if pipe_segments.len() <= 1 {
        return PermissionResult::passthrough("No pipes found in command");
    }

    // Strip output redirections from each segment
    let segments: Vec<String> = pipe_segments
        .iter()
        .map(|seg| build_segment_without_redirections(seg))
        .collect();

    segmented_command_permission_result(command, &segments, allow_rules, deny_rules).await
}

/// Split a command by pipes while respecting quotes.
fn split_by_pipe(command: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut escape_next = false;

    let chars: Vec<char> = command.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        if escape_next {
            current.push(c);
            escape_next = false;
            i += 1;
            continue;
        }

        if c == '\\' && !in_single_quote {
            escape_next = true;
            current.push(c);
            i += 1;
            continue;
        }

        if c == '\'' && !in_double_quote {
            in_single_quote = !in_single_quote;
            current.push(c);
            i += 1;
            continue;
        }

        if c == '"' && !in_single_quote {
            in_double_quote = !in_double_quote;
            current.push(c);
            i += 1;
            continue;
        }

        if c == '|' && !in_single_quote && !in_double_quote {
            // Check it's not || (logical OR)
            if i + 1 < chars.len() && chars[i + 1] == '|' {
                current.push(c);
                current.push(chars[i + 1]);
                i += 2;
                continue;
            }
            let trimmed = current.trim().to_string();
            if !trimmed.is_empty() {
                segments.push(trimmed);
            }
            current.clear();
            i += 1;
            continue;
        }

        current.push(c);
        i += 1;
    }

    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        segments.push(trimmed);
    }
    segments
}

/// Split command by && || ; (simple deprecated version).
fn split_command_deprecated(command: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut escape_next = false;
    let chars: Vec<char> = command.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        if escape_next {
            current.push(c);
            escape_next = false;
            i += 1;
            continue;
        }
        if c == '\\' && !in_single_quote {
            escape_next = true;
            current.push(c);
            i += 1;
            continue;
        }
        if c == '\'' && !in_double_quote {
            in_single_quote = !in_single_quote;
            current.push(c);
            i += 1;
            continue;
        }
        if c == '"' && !in_single_quote {
            in_double_quote = !in_double_quote;
            current.push(c);
            i += 1;
            continue;
        }
        if !in_single_quote && !in_double_quote {
            if i + 1 < chars.len() {
                let next = chars[i + 1];
                if (c == '&' && next == '&') || (c == '|' && next == '|') {
                    let trimmed = current.trim().to_string();
                    if !trimmed.is_empty() {
                        segments.push(trimmed);
                    }
                    current.clear();
                    i += 2;
                    continue;
                }
            }
            if c == ';' {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    segments.push(trimmed);
                }
                current.clear();
                i += 1;
                continue;
            }
        }
        current.push(c);
        i += 1;
    }

    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        segments.push(trimmed);
    }
    segments
}

/// Truncate a string for display.
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_by_pipe() {
        let segments = split_by_pipe("cat file | grep pattern | wc -l");
        assert_eq!(segments, vec!["cat file", "grep pattern", "wc -l"]);
    }

    #[test]
    fn test_split_by_pipe_with_or() {
        let segments = split_by_pipe("cmd1 || cmd2 | cmd3");
        assert_eq!(segments, vec!["cmd1 || cmd2", "cmd3"]);
    }

    #[test]
    fn test_build_segment_without_redirections() {
        assert_eq!(
            build_segment_without_redirections("echo hello > file.txt"),
            "echo hello"
        );
        assert_eq!(
            build_segment_without_redirections("cat file"),
            "cat file"
        );
    }
}
