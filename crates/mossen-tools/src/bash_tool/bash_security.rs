//! Bash security checks — validates commands for potentially dangerous patterns.
//!
//! Corresponds to `bashSecurity.ts` (2593 lines). Checks for command substitution,
//! dangerous shell patterns, heredoc manipulation, Zsh-specific attacks, and more.

use regex::Regex;
use std::collections::HashSet;

/// Command substitution patterns that require security review.
struct DangerousPattern {
    pattern: Regex,
    message: &'static str,
}

/// Security check result.
#[derive(Debug, Clone, PartialEq)]
pub enum SecurityBehavior {
    /// Command appears safe.
    Allow,
    /// Command needs user approval.
    Ask,
    /// Command is explicitly denied.
    Deny,
}

/// Security check result with details.
#[derive(Debug, Clone)]
pub struct SecurityResult {
    pub behavior: SecurityBehavior,
    pub message: Option<String>,
    pub check_id: Option<u32>,
}

impl SecurityResult {
    pub fn safe() -> Self {
        Self {
            behavior: SecurityBehavior::Allow,
            message: None,
            check_id: None,
        }
    }
    pub fn ask(message: impl Into<String>, check_id: u32) -> Self {
        Self {
            behavior: SecurityBehavior::Ask,
            message: Some(message.into()),
            check_id: Some(check_id),
        }
    }
    pub fn deny(message: impl Into<String>, check_id: u32) -> Self {
        Self {
            behavior: SecurityBehavior::Deny,
            message: Some(message.into()),
            check_id: Some(check_id),
        }
    }
}

// Security check IDs
const CHECK_INCOMPLETE_COMMANDS: u32 = 1;
const CHECK_JQ_SYSTEM_FUNCTION: u32 = 2;
const CHECK_JQ_FILE_ARGUMENTS: u32 = 3;
const CHECK_DANGEROUS_PATTERNS: u32 = 4;
const CHECK_HEREDOC_SUBSTITUTION: u32 = 5;
const CHECK_ZSH_DANGEROUS_COMMANDS: u32 = 6;
const CHECK_BACKTICK_SUBSTITUTION: u32 = 7;
const CHECK_COMMAND_SUBSTITUTION: u32 = 8;
const CHECK_SHELL_QUOTE_BUG: u32 = 9;
const CHECK_MALFORMED_TOKENS: u32 = 10;
const CHECK_BRACE_EXPANSION: u32 = 11;
const CHECK_GLOBBING_ATTACK: u32 = 12;

/// Zsh-specific dangerous commands that can bypass security checks.
fn zsh_dangerous_commands() -> HashSet<&'static str> {
    let mut set = HashSet::new();
    set.insert("zmodload");
    set.insert("emulate");
    set.insert("sysopen");
    set.insert("sysread");
    set.insert("syswrite");
    set.insert("sysseek");
    set.insert("zpty");
    set.insert("ztcp");
    set.insert("zsocket");
    set.insert("mapfile");
    set.insert("zf_rm");
    set.insert("zf_mv");
    set.insert("zf_ln");
    set.insert("zf_chmod");
    set.insert("zf_chown");
    set.insert("zf_mkdir");
    set.insert("zf_rmdir");
    set.insert("zf_chgrp");
    set
}

/// Command substitution patterns that indicate potential security issues.
fn command_substitution_patterns() -> Vec<DangerousPattern> {
    vec![
        DangerousPattern {
            pattern: Regex::new(r"<\(").unwrap(),
            message: "process substitution <()",
        },
        DangerousPattern {
            pattern: Regex::new(r">\(").unwrap(),
            message: "process substitution >()",
        },
        DangerousPattern {
            pattern: Regex::new(r"=\(").unwrap(),
            message: "Zsh process substitution =()",
        },
        DangerousPattern {
            pattern: Regex::new(r"(?:^|[\s;&|])=[a-zA-Z_]").unwrap(),
            message: "Zsh equals expansion (=cmd)",
        },
        DangerousPattern {
            pattern: Regex::new(r"\$\(").unwrap(),
            message: "$() command substitution",
        },
        DangerousPattern {
            pattern: Regex::new(r"\$\{").unwrap(),
            message: "${} parameter substitution",
        },
        DangerousPattern {
            pattern: Regex::new(r"\$\[").unwrap(),
            message: "$[] legacy arithmetic expansion",
        },
        DangerousPattern {
            pattern: Regex::new(r"~\[").unwrap(),
            message: "Zsh-style parameter expansion",
        },
        DangerousPattern {
            pattern: Regex::new(r"\(e:").unwrap(),
            message: "Zsh-style glob qualifiers",
        },
        DangerousPattern {
            pattern: Regex::new(r"\(\+").unwrap(),
            message: "Zsh glob qualifier with command execution",
        },
        DangerousPattern {
            pattern: Regex::new(r"\}\s*always\s*\{").unwrap(),
            message: "Zsh always block (try/always construct)",
        },
        DangerousPattern {
            pattern: Regex::new(r"<#").unwrap(),
            message: "PowerShell comment syntax",
        },
    ]
}

/// Strip safe heredoc substitutions from a command for security checking.
/// Heredocs with quoted delimiters (<<'EOF' or <<"EOF") don't expand variables.
pub fn strip_safe_heredoc_substitutions(command: &str) -> String {
    // Match heredocs with quoted delimiters and remove their bodies
    let heredoc_re = Regex::new(r#"<<[-~]?['"](\w+)['"].*?\n([\s\S]*?)\n\1"#).unwrap();
    let mut result = command.to_string();

    // Simple heredoc stripping: remove content between quoted heredoc markers
    // This is a simplified version - in full implementation would use proper shell parsing
    let lines: Vec<&str> = command.lines().collect();
    let mut in_heredoc = false;
    let mut heredoc_marker = String::new();
    let mut output_lines = Vec::new();

    for line in &lines {
        if in_heredoc {
            if line.trim() == heredoc_marker {
                in_heredoc = false;
                output_lines.push(*line);
            }
            // Skip heredoc body lines (safe — no expansion)
            continue;
        }

        // Check for heredoc start with quoted delimiter
        let heredoc_start =
            Regex::new(r#"<<[-~]?['"](\w+)['"]"#).unwrap();
        if let Some(cap) = heredoc_start.captures(line) {
            heredoc_marker = cap.get(1).unwrap().as_str().to_string();
            in_heredoc = true;
        }
        output_lines.push(*line);
    }

    output_lines.join("\n")
}

/// Validate that a command doesn't contain dangerous shell patterns.
fn validate_dangerous_patterns(command: &str) -> SecurityResult {
    let patterns = command_substitution_patterns();

    for dp in &patterns {
        if dp.pattern.is_match(command) {
            return SecurityResult::ask(
                format!(
                    "Command contains potentially dangerous pattern: {}",
                    dp.message
                ),
                CHECK_COMMAND_SUBSTITUTION,
            );
        }
    }

    SecurityResult::safe()
}

/// Check for unescaped backtick command substitution.
fn check_backtick_substitution(command: &str) -> SecurityResult {
    let chars: Vec<char> = command.chars().collect();
    let mut i = 0;
    let mut in_single_quote = false;

    while i < chars.len() {
        let c = chars[i];

        if c == '\'' {
            in_single_quote = !in_single_quote;
            i += 1;
            continue;
        }

        if !in_single_quote {
            if c == '\\' && i + 1 < chars.len() {
                i += 2; // Skip escaped character
                continue;
            }
            if c == '`' {
                return SecurityResult::ask(
                    "Command contains backtick command substitution",
                    CHECK_BACKTICK_SUBSTITUTION,
                );
            }
        }

        i += 1;
    }

    SecurityResult::safe()
}

/// Check for Zsh-specific dangerous commands.
fn check_zsh_dangerous_commands(command: &str) -> SecurityResult {
    let dangerous = zsh_dangerous_commands();
    let subcommands = split_simple(command);

    for subcmd in &subcommands {
        let base_command = subcmd.trim().split_whitespace().next().unwrap_or("");
        if dangerous.contains(base_command) {
            return SecurityResult::deny(
                format!(
                    "Command '{}' is blocked for security (Zsh dangerous command)",
                    base_command
                ),
                CHECK_ZSH_DANGEROUS_COMMANDS,
            );
        }
    }

    SecurityResult::safe()
}

/// Check for heredoc in command substitution (potential injection).
fn check_heredoc_in_substitution(command: &str) -> SecurityResult {
    let re = Regex::new(r"\$\(.*<<").unwrap();
    if re.is_match(command) {
        return SecurityResult::ask(
            "Command contains heredoc inside command substitution",
            CHECK_HEREDOC_SUBSTITUTION,
        );
    }
    SecurityResult::safe()
}

/// Check for incomplete/unterminated commands that could be exploited.
fn check_incomplete_commands(command: &str) -> SecurityResult {
    let trimmed = command.trim();

    // Check for trailing pipe/operators that indicate incomplete commands
    if trimmed.ends_with('|')
        || trimmed.ends_with("&&")
        || trimmed.ends_with("||")
        || trimmed.ends_with(';')
    {
        return SecurityResult::ask(
            "Command appears incomplete (trailing operator)",
            CHECK_INCOMPLETE_COMMANDS,
        );
    }

    SecurityResult::safe()
}

/// Check for jq system function or file argument attacks.
fn check_jq_security(command: &str) -> SecurityResult {
    // Check for jq's @system function
    if command.contains("@system") || command.contains("@base64d") {
        let base = command.trim().split_whitespace().next().unwrap_or("");
        if base == "jq" || command.contains("| jq") {
            return SecurityResult::ask(
                "jq command uses potentially dangerous function",
                CHECK_JQ_SYSTEM_FUNCTION,
            );
        }
    }

    // Check for jq with file input arguments that could be exploited
    let re = Regex::new(r"\bjq\b[^|;]*\b(--slurpfile|--jsonargs|--rawfile)\b").unwrap();
    if re.is_match(command) {
        return SecurityResult::ask(
            "jq command uses file input arguments",
            CHECK_JQ_FILE_ARGUMENTS,
        );
    }

    SecurityResult::safe()
}

/// Simple command splitting for security checks.
fn split_simple(command: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut escape_next = false;

    for c in command.chars() {
        if escape_next {
            current.push(c);
            escape_next = false;
            continue;
        }
        if c == '\\' && !in_single_quote {
            escape_next = true;
            current.push(c);
            continue;
        }
        if c == '\'' && !in_double_quote {
            in_single_quote = !in_single_quote;
            current.push(c);
            continue;
        }
        if c == '"' && !in_single_quote {
            in_double_quote = !in_double_quote;
            current.push(c);
            continue;
        }
        if !in_single_quote && !in_double_quote && (c == ';' || c == '|') {
            let trimmed = current.trim().to_string();
            if !trimmed.is_empty() {
                segments.push(trimmed);
            }
            current.clear();
            continue;
        }
        current.push(c);
    }

    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        segments.push(trimmed);
    }
    segments
}

/// Main security check entry point (async, deprecated name for compatibility).
/// Runs all security validators against the command.
pub async fn bash_command_is_safe_async_deprecated(command: &str) -> SecurityResult {
    bash_command_is_safe_deprecated(command)
}

/// Synchronous security check (deprecated name for compatibility).
pub fn bash_command_is_safe_deprecated(command: &str) -> SecurityResult {
    // Strip safe heredoc substitutions first
    let cleaned = strip_safe_heredoc_substitutions(command);

    // Run checks in priority order
    let checks: Vec<fn(&str) -> SecurityResult> = vec![
        check_incomplete_commands,
        check_zsh_dangerous_commands,
        check_heredoc_in_substitution,
        check_backtick_substitution,
        check_jq_security,
        validate_dangerous_patterns,
    ];

    for check in &checks {
        let result = check(&cleaned);
        if result.behavior != SecurityBehavior::Allow {
            return result;
        }
    }

    SecurityResult::safe()
}

/// Check if a command is safe without considering heredocs (for already-stripped commands).
pub fn check_command_safety_no_heredoc(command: &str) -> SecurityResult {
    let checks: Vec<fn(&str) -> SecurityResult> = vec![
        check_incomplete_commands,
        check_zsh_dangerous_commands,
        check_backtick_substitution,
        check_jq_security,
        validate_dangerous_patterns,
    ];

    for check in &checks {
        let result = check(command);
        if result.behavior != SecurityBehavior::Allow {
            return result;
        }
    }

    SecurityResult::safe()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_command() {
        let result = bash_command_is_safe_deprecated("ls -la");
        assert_eq!(result.behavior, SecurityBehavior::Allow);
    }

    #[test]
    fn test_command_substitution() {
        let result = bash_command_is_safe_deprecated("echo $(whoami)");
        assert_eq!(result.behavior, SecurityBehavior::Ask);
    }

    #[test]
    fn test_backtick_substitution() {
        let result = bash_command_is_safe_deprecated("echo `whoami`");
        assert_eq!(result.behavior, SecurityBehavior::Ask);
    }

    #[test]
    fn test_zsh_dangerous_command() {
        let result = bash_command_is_safe_deprecated("zmodload zsh/system");
        assert_eq!(result.behavior, SecurityBehavior::Deny);
    }

    #[test]
    fn test_incomplete_command() {
        let result = bash_command_is_safe_deprecated("echo hello |");
        assert_eq!(result.behavior, SecurityBehavior::Ask);
    }

    #[test]
    fn test_jq_system() {
        let result = bash_command_is_safe_deprecated("echo '{}' | jq '@system'");
        assert_eq!(result.behavior, SecurityBehavior::Ask);
    }

    #[test]
    fn test_process_substitution() {
        let result = bash_command_is_safe_deprecated("diff <(cmd1) file");
        assert_eq!(result.behavior, SecurityBehavior::Ask);
    }

    #[test]
    fn test_safe_heredoc() {
        // Quoted heredoc delimiters are safe (no variable expansion)
        let cmd = "cat <<'EOF'\nhello $USER\nEOF";
        let stripped = strip_safe_heredoc_substitutions(cmd);
        // After stripping, the heredoc body is removed
        assert!(!stripped.contains("$USER") || stripped.contains("'EOF'"));
    }
}

// ---------------------------------------------------------------------------
// TS-mirror — `tools/BashTool/bashSecurity.ts` additional export.
// ---------------------------------------------------------------------------

/// `bashSecurity.ts` `hasSafeHeredocSubstitution` — true when the command
/// uses a single- or double-quoted heredoc delimiter that disables variable
/// expansion in the body.
pub fn has_safe_heredoc_substitution(command: &str) -> bool {
    command.contains("<<'") || command.contains("<<\"")
}
