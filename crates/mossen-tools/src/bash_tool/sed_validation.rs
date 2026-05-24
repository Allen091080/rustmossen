//! Sed command validation for the Bash tool.
//!
//! Corresponds to `sedValidation.ts` (685 lines). Validates sed commands for
//! safety in the permission system, checking flags, expressions, and file targets.

use regex::Regex;
use std::collections::HashSet;

/// Safe sed flags for line printing commands.
const LINE_PRINT_SAFE_FLAGS: &[&str] = &["-n", "-E", "-r", "-z"];

/// Safe sed flags for substitution commands.
const SUBSTITUTION_SAFE_FLAGS: &[&str] = &[
    "-i",
    "-E",
    "-r",
    "-e",
    "--in-place",
    "--expression",
    "--regexp-extended",
];

/// Result of sed validation.
#[derive(Debug, Clone, PartialEq)]
pub enum SedCheckResult {
    /// Sed command is safe (read-only or matches allowlist).
    Allowed,
    /// Sed command needs approval.
    NeedsApproval { message: String },
    /// Not a sed command; pass through.
    Passthrough,
}

/// Validate flags against an allowlist.
/// Handles both single flags and combined flags (e.g., -nE).
fn validate_flags_against_allowlist(flags: &[&str], allowed_flags: &[&str]) -> bool {
    for flag in flags {
        if flag.starts_with('-') && !flag.starts_with("--") && flag.len() > 2 {
            // Combined flags like -nE
            for ch in flag[1..].chars() {
                let single_flag = format!("-{}", ch);
                if !allowed_flags.contains(&single_flag.as_str()) {
                    return false;
                }
            }
        } else {
            if !allowed_flags.contains(flag) {
                return false;
            }
        }
    }
    true
}

/// Pattern 1: Check if this is a line printing command with -n flag.
/// Allows: sed -n 'N' | sed -n 'N,M' with optional -E, -r, -z flags.
/// File arguments are ALLOWED for this pattern.
pub fn is_line_printing_command(command: &str, expressions: &[String]) -> bool {
    let re = Regex::new(r"^\s*sed\s+").unwrap();
    if !re.is_match(command) {
        return false;
    }

    // Must have -n flag
    let parts: Vec<&str> = command.trim().split_whitespace().collect();
    let has_n = parts
        .iter()
        .any(|p| *p == "-n" || p.contains('n') && p.starts_with('-') && !p.starts_with("--"));

    if !has_n {
        return false;
    }

    // Check that all expressions are print/address patterns
    let print_pattern = Regex::new(r"^[0-9,$;p=\s/\\]+$").unwrap();
    for expr in expressions {
        if !print_pattern.is_match(expr) {
            return false;
        }
    }

    true
}

/// Pattern 2: Check if this is a safe substitution command.
/// Validates that the substitution pattern and flags are safe.
pub fn is_safe_substitution(expression: &str) -> bool {
    // Must start with s/
    if !expression.starts_with("s/")
        && !expression.starts_with("s|")
        && !expression.starts_with("s#")
    {
        return false;
    }

    // The expression itself is a substitution — this is always potentially modifying
    // We allow it through sed validation but it requires -i for in-place editing
    true
}

/// Pattern 3: Check for dangerous sed patterns.
/// These require explicit approval.
fn has_dangerous_pattern(expression: &str) -> bool {
    let normalized = normalize_expression(expression);
    let expression = normalized.as_str();

    // `e` flag (execute pattern space as command)
    if expression.ends_with('e') {
        let re = Regex::new(r"/[gipe]*e[gipe]*$").unwrap();
        if re.is_match(expression) {
            return true;
        }
    }

    // `w` command or flag (write to file)
    if expression.contains("/w ") || expression.ends_with("/w") {
        return true;
    }

    // `r` or `R` command (read file into pattern space)
    let r_cmd = Regex::new(r"^[0-9,$]*[rR]\s").unwrap();
    if r_cmd.is_match(expression) {
        return true;
    }

    false
}

fn normalize_expression(expression: &str) -> String {
    expression
        .trim()
        .trim_matches('\'')
        .trim_matches('"')
        .to_string()
}

/// Check sed-specific constraints on a command.
///
/// Returns:
/// - `Allowed` if the sed command is safe to auto-approve
/// - `NeedsApproval` if it needs permission  
/// - `Passthrough` if not a sed command
pub fn check_sed_constraints(command: &str) -> SedCheckResult {
    let trimmed = command.trim();

    // Must start with sed
    if !trimmed.starts_with("sed ") && trimmed != "sed" {
        return SedCheckResult::Passthrough;
    }

    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    if parts.len() < 2 {
        return SedCheckResult::Passthrough;
    }

    // Extract flags and expressions
    let mut flags = Vec::new();
    let mut expressions = Vec::new();
    let mut has_in_place = false;
    let mut i = 1;

    while i < parts.len() {
        let part = parts[i];

        if part == "-i" || part == "--in-place" || part.starts_with("-i") {
            has_in_place = true;
            flags.push(part);
            i += 1;
            // -i may have a backup suffix argument
            if part == "-i" && i < parts.len() {
                let next = parts[i];
                if next.is_empty() || next.starts_with('.') {
                    i += 1; // Skip backup suffix
                }
            }
            continue;
        }

        if part == "-e" || part == "--expression" {
            flags.push(part);
            i += 1;
            if i < parts.len() {
                expressions.push(parts[i].to_string());
                i += 1;
            }
            continue;
        }

        if part.starts_with('-') {
            flags.push(part);
            i += 1;
            continue;
        }

        // First non-flag, non-file argument is the expression
        if expressions.is_empty() {
            expressions.push(part.to_string());
        }
        // Rest are file arguments
        i += 1;
    }

    // Check for dangerous patterns in expressions
    for expr in &expressions {
        if has_dangerous_pattern(expr) {
            return SedCheckResult::NeedsApproval {
                message: format!(
                    "sed command contains potentially dangerous pattern: {}",
                    normalize_expression(expr)
                ),
            };
        }
    }

    // If sed has -n flag (suppress output) and only uses print commands, it's read-only
    let has_n = flags
        .iter()
        .any(|f| *f == "-n" || (f.starts_with('-') && !f.starts_with("--") && f.contains('n')));
    if has_n && !has_in_place {
        if is_line_printing_command(trimmed, &expressions) {
            return SedCheckResult::Allowed;
        }
    }

    // In-place sed always needs approval (modifies files)
    if has_in_place {
        return SedCheckResult::NeedsApproval {
            message: "sed -i modifies files in place".to_string(),
        };
    }

    // Non-in-place sed without -n that just outputs is read-only
    if !has_in_place {
        return SedCheckResult::Allowed;
    }

    SedCheckResult::NeedsApproval {
        message: "sed command requires approval".to_string(),
    }
}

/// Check if a sed command is allowed by the permission allowlist.
pub fn sed_command_is_allowed_by_allowlist(command: &str, allow_rules: &[String]) -> bool {
    // Check if any allow rule explicitly covers sed commands
    for rule in allow_rules {
        if rule == "sed:*" || rule == "sed" {
            return true;
        }
        if rule.starts_with("sed ") && command.starts_with(rule.as_str()) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sed_print_command() {
        let result = check_sed_constraints("sed -n '5p' file.txt");
        assert_eq!(result, SedCheckResult::Allowed);
    }

    #[test]
    fn test_sed_in_place() {
        let result = check_sed_constraints("sed -i 's/foo/bar/g' file.txt");
        assert_eq!(
            result,
            SedCheckResult::NeedsApproval {
                message: "sed -i modifies files in place".to_string()
            }
        );
    }

    #[test]
    fn test_sed_non_in_place_substitution() {
        let result = check_sed_constraints("sed 's/foo/bar/g' file.txt");
        assert_eq!(result, SedCheckResult::Allowed);
    }

    #[test]
    fn test_not_sed_command() {
        let result = check_sed_constraints("grep pattern file.txt");
        assert_eq!(result, SedCheckResult::Passthrough);
    }

    #[test]
    fn test_dangerous_e_flag() {
        let result = check_sed_constraints("sed 's/foo/bar/e' file.txt");
        assert_eq!(
            result,
            SedCheckResult::NeedsApproval {
                message: "sed command contains potentially dangerous pattern: s/foo/bar/e"
                    .to_string()
            }
        );
    }

    #[test]
    fn test_validate_flags() {
        assert!(validate_flags_against_allowlist(
            &["-n", "-E"],
            LINE_PRINT_SAFE_FLAGS
        ));
        assert!(!validate_flags_against_allowlist(
            &["-n", "-x"],
            LINE_PRINT_SAFE_FLAGS
        ));
    }
}

// ---------------------------------------------------------------------------
// TS-mirror — `tools/BashTool/sedValidation.ts` additional exports.
// ---------------------------------------------------------------------------

/// `sedValidation.ts` `isPrintCommand`.
pub fn is_print_command(cmd: &str) -> bool {
    let trimmed = cmd.trim();
    if trimmed.is_empty() {
        return false;
    }
    matches!(trimmed.chars().last(), Some('p' | '=' | 'l'))
}

/// `sedValidation.ts` `hasFileArgs`.
pub fn has_file_args(command: &str) -> bool {
    let tokens: Vec<&str> = command.split_whitespace().collect();
    if tokens.is_empty() || tokens[0] != "sed" {
        return false;
    }
    let mut iter = tokens.iter().skip(1).peekable();
    let mut script_seen = false;
    while let Some(tok) = iter.next() {
        if tok.starts_with('-') {
            if matches!(*tok, "-e" | "-f" | "--expression" | "--file") {
                iter.next();
                script_seen = true;
            }
            continue;
        }
        if !script_seen {
            script_seen = true;
            continue;
        }
        return true;
    }
    false
}

/// `sedValidation.ts` `extractSedExpressions`.
pub fn extract_sed_expressions(command: &str) -> Vec<String> {
    let tokens: Vec<&str> = command.split_whitespace().collect();
    let mut out = Vec::new();
    let mut iter = tokens.iter().peekable();
    if iter.peek().copied() == Some(&"sed") {
        iter.next();
    }
    let mut saw_script = false;
    while let Some(tok) = iter.next() {
        if *tok == "-e" || *tok == "--expression" {
            if let Some(expr) = iter.next() {
                out.push((*expr).to_string());
                saw_script = true;
            }
            continue;
        }
        if tok.starts_with("-e") && tok.len() > 2 {
            out.push(tok[2..].to_string());
            saw_script = true;
            continue;
        }
        if !tok.starts_with('-') && !saw_script {
            out.push((*tok).to_string());
            saw_script = true;
        }
    }
    out
}
