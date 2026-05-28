//! # permissions — PowerShell permission checking
//!
//! Translates `tools/PowerShellTool/powershellPermissions.ts`.
//! Implements permission rules (prefix/exact/wildcard) for PowerShell commands.

use std::collections::HashSet;
use std::sync::LazyLock;

/// Permission rule types for PowerShell commands.
#[derive(Debug, Clone)]
pub enum PermissionRule {
    /// Matches if command starts with the given prefix.
    Prefix { prefix: String },
    /// Matches only the exact command string.
    Exact { command: String },
    /// Matches using glob-like wildcard pattern.
    Wildcard { pattern: String },
}

/// Result of a permission check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionResult {
    /// Command is allowed by an allow rule.
    Allowed,
    /// Command is denied by a deny rule.
    Denied,
    /// No matching rule found — needs user approval.
    NeedsApproval,
}

/// Parse a permission rule pattern string into a PermissionRule.
pub fn parse_permission_rule(pattern: &str) -> PermissionRule {
    if pattern.contains('*') || pattern.contains('?') {
        PermissionRule::Wildcard {
            pattern: pattern.to_string(),
        }
    } else if pattern.ends_with("**") {
        PermissionRule::Prefix {
            prefix: pattern[..pattern.len() - 2].to_string(),
        }
    } else if pattern.ends_with(' ') || pattern.ends_with('/') {
        PermissionRule::Prefix {
            prefix: pattern.to_string(),
        }
    } else {
        PermissionRule::Exact {
            command: pattern.to_string(),
        }
    }
}

/// Match a wildcard pattern against a command string.
pub fn match_wildcard_pattern(pattern: &str, command: &str) -> bool {
    let pattern_lower = pattern.to_lowercase();
    let command_lower = command.to_lowercase();

    let pattern_parts: Vec<&str> = pattern_lower.split('*').collect();
    if pattern_parts.is_empty() {
        return true;
    }

    let mut pos = 0;
    for (i, part) in pattern_parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if let Some(found) = command_lower[pos..].find(part) {
            if i == 0 && found != 0 {
                // First part must match at start
                return false;
            }
            pos += found + part.len();
        } else {
            return false;
        }
    }

    // If pattern doesn't end with *, command must end here
    if !pattern_lower.ends_with('*') && pos != command_lower.len() {
        return false;
    }

    true
}

/// Strip leading environment variable assignments from a command.
/// e.g., `$env:FOO='bar'; git status` → `git status`
pub fn strip_leading_env_vars(command: &str) -> String {
    let trimmed = command.trim();

    // PowerShell env vars: $env:NAME = "value";
    let re = regex::Regex::new(r"(?i)^\s*\$env:\w+\s*=\s*[^;]*;\s*").unwrap();
    let mut result = trimmed.to_string();

    while let Some(m) = re.find(&result) {
        result = result[m.end()..].to_string();
    }

    result
}

/// Check a single rule against a command.
fn matches_rule(rule: &PermissionRule, command: &str) -> bool {
    let cmd_lower = command.to_lowercase();
    match rule {
        PermissionRule::Prefix { prefix } => {
            let prefix_lower = prefix.to_lowercase();
            cmd_lower.starts_with(&prefix_lower)
        }
        PermissionRule::Exact { command: exact } => {
            let exact_lower = exact.to_lowercase();
            cmd_lower == exact_lower
        }
        PermissionRule::Wildcard { pattern } => match_wildcard_pattern(pattern, command),
    }
}

/// Check if a PowerShell command has permission based on allow/deny rules.
pub fn powershell_has_permission(
    command: &str,
    allow_rules: &[PermissionRule],
    deny_rules: &[PermissionRule],
) -> PermissionResult {
    let stripped = strip_leading_env_vars(command);
    let check_cmd = if stripped.is_empty() {
        command
    } else {
        &stripped
    };

    // Check deny rules first (deny takes precedence)
    for rule in deny_rules {
        if matches_rule(rule, check_cmd) {
            return PermissionResult::Denied;
        }
    }

    // Check allow rules
    for rule in allow_rules {
        if matches_rule(rule, check_cmd) {
            return PermissionResult::Allowed;
        }
    }

    PermissionResult::NeedsApproval
}

/// PowerShell-specific safe wrappers to strip before permission checking.
static PS_SAFE_WRAPPERS: LazyLock<Vec<regex::Regex>> = LazyLock::new(|| {
    vec![
        // & operator with quoted command
        regex::Regex::new(r#"^&\s+['"]([^'"]+)['"]\s*"#).unwrap(),
        // Invoke-Expression
        regex::Regex::new(r"(?i)^Invoke-Expression\s+").unwrap(),
        // Start-Process
        regex::Regex::new(r"(?i)^Start-Process\s+(-FilePath\s+)?").unwrap(),
    ]
});

/// Strip safe wrapper constructs from a PowerShell command.
pub fn strip_safe_wrappers(command: &str) -> String {
    let mut result = command.trim().to_string();

    for wrapper in PS_SAFE_WRAPPERS.iter() {
        if let Some(m) = wrapper.find(&result) {
            result = result[m.end()..].trim().to_string();
        }
    }

    result
}

// ---------------------------------------------------------------------------
// TS-mirror — `tools/PowerShellTool/powershellPermissions.ts` exports.
// ---------------------------------------------------------------------------

/// `powershellPermissions.ts` `powershellPermissionRule` — alias.
pub fn powershell_permission_rule(rule: &str) -> PermissionRule {
    parse_permission_rule(rule)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PsPermissionBehavior {
    Allow,
    Ask,
    Deny,
    Passthrough,
}

#[derive(Debug, Clone)]
pub struct PsPermissionResult {
    pub behavior: PsPermissionBehavior,
    pub message: Option<String>,
}

/// `powershellPermissions.ts` `powershellToolCheckExactMatchPermission`.
pub fn powershell_tool_check_exact_match_permission(
    command: &str,
    allow_rules: &[String],
    deny_rules: &[String],
) -> PsPermissionResult {
    let normalized = command.trim();
    for rule in deny_rules {
        if let PermissionRule::Exact { command: pattern } = parse_permission_rule(rule) {
            if pattern == normalized {
                return PsPermissionResult {
                    behavior: PsPermissionBehavior::Deny,
                    message: Some(format!("Denied by rule: {}", rule)),
                };
            }
        }
    }
    for rule in allow_rules {
        if let PermissionRule::Exact { command: pattern } = parse_permission_rule(rule) {
            if pattern == normalized {
                return PsPermissionResult {
                    behavior: PsPermissionBehavior::Allow,
                    message: None,
                };
            }
        }
    }
    PsPermissionResult {
        behavior: PsPermissionBehavior::Passthrough,
        message: None,
    }
}

/// `powershellPermissions.ts` `powershellToolCheckPermission`.
pub fn powershell_tool_check_permission(
    command: &str,
    allow_rules: &[String],
    deny_rules: &[String],
) -> PsPermissionResult {
    let exact = powershell_tool_check_exact_match_permission(command, allow_rules, deny_rules);
    if exact.behavior != PsPermissionBehavior::Passthrough {
        return exact;
    }
    for rule in deny_rules {
        if match_wildcard_pattern(rule, command) {
            return PsPermissionResult {
                behavior: PsPermissionBehavior::Deny,
                message: Some(format!("Denied by rule: {}", rule)),
            };
        }
    }
    for rule in allow_rules {
        if match_wildcard_pattern(rule, command) {
            return PsPermissionResult {
                behavior: PsPermissionBehavior::Allow,
                message: None,
            };
        }
    }
    PsPermissionResult {
        behavior: PsPermissionBehavior::Ask,
        message: None,
    }
}

/// `powershellPermissions.ts` `powershellToolHasPermission`.
pub fn powershell_tool_has_permission(
    command: &str,
    allow_rules: &[String],
    deny_rules: &[String],
) -> PsPermissionResult {
    powershell_tool_check_permission(command, allow_rules, deny_rules)
}
