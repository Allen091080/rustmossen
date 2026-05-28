//! Mode-based permission validation for bash commands.
//!
//! Checks if commands should be handled differently based on the current permission mode.
//! Currently handles Accept Edits mode for filesystem commands.

use crate::bash_tool::command_semantics;

/// Permission behavior outcome.
#[derive(Debug, Clone, PartialEq)]
pub enum PermissionBehavior {
    /// Command is allowed by mode-specific rules.
    Allow,
    /// Command needs explicit user approval.
    Ask,
    /// No mode-specific handling; pass through to normal flow.
    Passthrough,
    /// Command is denied.
    Deny,
}

/// Result of a permission check.
#[derive(Debug, Clone)]
pub struct PermissionResult {
    pub behavior: PermissionBehavior,
    pub message: Option<String>,
    pub decision_reason: Option<DecisionReason>,
}

/// Why a permission decision was made.
#[derive(Debug, Clone)]
pub enum DecisionReason {
    Mode { mode: String },
    Other { reason: String },
    SubcommandResults,
}

/// The current tool permission context.
#[derive(Debug, Clone)]
pub struct ToolPermissionContext {
    pub mode: String,
}

/// Commands automatically allowed in "acceptEdits" mode.
const ACCEPT_EDITS_ALLOWED_COMMANDS: &[&str] =
    &["mkdir", "touch", "rm", "rmdir", "mv", "cp", "sed"];

/// Check if a command is a filesystem command allowed in acceptEdits mode.
fn is_filesystem_command(command: &str) -> bool {
    ACCEPT_EDITS_ALLOWED_COMMANDS.contains(&command)
}

/// Validate a single command against the current mode.
fn validate_command_for_mode(
    cmd: &str,
    tool_permission_context: &ToolPermissionContext,
) -> PermissionResult {
    let trimmed = cmd.trim();
    let base_cmd = trimmed.split_whitespace().next().unwrap_or("");

    if base_cmd.is_empty() {
        return PermissionResult {
            behavior: PermissionBehavior::Passthrough,
            message: Some("Base command not found".to_string()),
            decision_reason: None,
        };
    }

    // In Accept Edits mode, auto-allow filesystem operations
    if tool_permission_context.mode == "acceptEdits" && is_filesystem_command(base_cmd) {
        return PermissionResult {
            behavior: PermissionBehavior::Allow,
            message: None,
            decision_reason: Some(DecisionReason::Mode {
                mode: "acceptEdits".to_string(),
            }),
        };
    }

    PermissionResult {
        behavior: PermissionBehavior::Passthrough,
        message: Some(format!(
            "No mode-specific handling for '{}' in {} mode",
            base_cmd, tool_permission_context.mode
        )),
        decision_reason: None,
    }
}

/// Split a command string by `&&`, `||`, and `;` (deprecated simple split).
/// This is a simplified version used for mode validation.
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

/// Main entry point for mode-based permission logic.
///
/// Returns:
/// - `Allow` if the current mode permits auto-approval
/// - `Ask` if the command needs approval in current mode
/// - `Passthrough` if no mode-specific handling applies
pub fn check_permission_mode(
    command: &str,
    tool_permission_context: &ToolPermissionContext,
) -> PermissionResult {
    // Skip if in bypass mode (handled elsewhere)
    if tool_permission_context.mode == "bypassPermissions" {
        return PermissionResult {
            behavior: PermissionBehavior::Passthrough,
            message: Some("Bypass mode is handled in main permission flow".to_string()),
            decision_reason: None,
        };
    }

    // Skip if in dontAsk mode (handled in main permission flow)
    if tool_permission_context.mode == "dontAsk" {
        return PermissionResult {
            behavior: PermissionBehavior::Passthrough,
            message: Some("DontAsk mode is handled in main permission flow".to_string()),
            decision_reason: None,
        };
    }

    let commands = split_command_deprecated(command);

    // Check each subcommand
    for cmd in &commands {
        let result = validate_command_for_mode(cmd, tool_permission_context);
        if result.behavior != PermissionBehavior::Passthrough {
            return result;
        }
    }

    // No mode-specific handling needed
    PermissionResult {
        behavior: PermissionBehavior::Passthrough,
        message: Some("No mode-specific validation required".to_string()),
        decision_reason: None,
    }
}

/// Get the list of commands auto-allowed in the given mode.
pub fn get_auto_allowed_commands(mode: &str) -> &'static [&'static str] {
    match mode {
        "acceptEdits" => ACCEPT_EDITS_ALLOWED_COMMANDS,
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_accept_edits_allows_mkdir() {
        let ctx = ToolPermissionContext {
            mode: "acceptEdits".to_string(),
        };
        let result = check_permission_mode("mkdir -p /tmp/foo", &ctx);
        assert_eq!(result.behavior, PermissionBehavior::Allow);
    }

    #[test]
    fn test_accept_edits_passthrough_for_git() {
        let ctx = ToolPermissionContext {
            mode: "acceptEdits".to_string(),
        };
        let result = check_permission_mode("git push", &ctx);
        assert_eq!(result.behavior, PermissionBehavior::Passthrough);
    }

    #[test]
    fn test_bypass_mode_passthrough() {
        let ctx = ToolPermissionContext {
            mode: "bypassPermissions".to_string(),
        };
        let result = check_permission_mode("rm -rf /", &ctx);
        assert_eq!(result.behavior, PermissionBehavior::Passthrough);
    }
}
