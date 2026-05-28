//! Shell rule matching utilities for permission checking.
//!
//! Provides parsing of permission rules (exact, prefix, wildcard) and
//! matching commands against those rules.

use regex::Regex;

use super::permission_result::{
    PermissionBehavior, PermissionRuleValue, PermissionUpdate, PermissionUpdateDestination,
};

// Sentinel placeholders for wildcard pattern escaping.
const ESCAPED_STAR_PLACEHOLDER: &str = "\x00ESCAPED_STAR\x00";
const ESCAPED_BACKSLASH_PLACEHOLDER: &str = "\x00ESCAPED_BACKSLASH\x00";

/// Parsed permission rule discriminated union.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellPermissionRule {
    Exact { command: String },
    Prefix { prefix: String },
    Wildcard { pattern: String },
}

/// Extract prefix from legacy `:*` syntax (e.g., "npm:*" -> "npm").
/// Maintained for backwards compatibility.
pub fn permission_rule_extract_prefix(permission_rule: &str) -> Option<&str> {
    if permission_rule.ends_with(":*") && permission_rule.len() > 2 {
        Some(&permission_rule[..permission_rule.len() - 2])
    } else {
        None
    }
}

/// Check if a pattern contains unescaped wildcards (not legacy `:*` syntax).
/// Returns true if the pattern contains `*` that are not escaped with `\` or part of `:*` at the end.
pub fn has_wildcards(pattern: &str) -> bool {
    // If it ends with :*, it's legacy prefix syntax, not wildcard
    if pattern.ends_with(":*") {
        return false;
    }
    // Check for unescaped * anywhere in the pattern
    let chars: Vec<char> = pattern.chars().collect();
    for i in 0..chars.len() {
        if chars[i] == '*' {
            // Count backslashes before this asterisk
            let mut backslash_count = 0;
            let mut j = i as isize - 1;
            while j >= 0 && chars[j as usize] == '\\' {
                backslash_count += 1;
                j -= 1;
            }
            // If even number of backslashes (including 0), the asterisk is unescaped
            if backslash_count % 2 == 0 {
                return true;
            }
        }
    }
    false
}

/// Match a command against a wildcard pattern.
/// Wildcards (`*`) match any sequence of characters.
/// Use `\*` to match a literal asterisk character.
/// Use `\\` to match a literal backslash.
pub fn match_wildcard_pattern(pattern: &str, command: &str, case_insensitive: bool) -> bool {
    let trimmed_pattern = pattern.trim();

    // Process the pattern to handle escape sequences: \* and \\
    let mut processed = String::new();
    let chars: Vec<char> = trimmed_pattern.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];
        // Handle escape sequences
        if ch == '\\' && i + 1 < chars.len() {
            let next_char = chars[i + 1];
            if next_char == '*' {
                processed.push_str(ESCAPED_STAR_PLACEHOLDER);
                i += 2;
                continue;
            } else if next_char == '\\' {
                processed.push_str(ESCAPED_BACKSLASH_PLACEHOLDER);
                i += 2;
                continue;
            }
        }
        processed.push(ch);
        i += 1;
    }

    // Escape regex special characters except *
    let _escaped = regex::escape(&processed.replace('*', "\x01"))
        .replace("\\x01", "\x01")
        .replace('\x01', ".*");

    // Actually, let's do it correctly:
    // First escape everything in processed except *, then convert * to .*
    let mut escaped_str = String::new();
    for ch in processed.chars() {
        if ch == '*' {
            escaped_str.push_str(".*");
        } else if ".+?^${}()|[]\\'\"".chars().any(|special| special == ch) {
            escaped_str.push('\\');
            escaped_str.push(ch);
        } else {
            escaped_str.push(ch);
        }
    }

    // Convert placeholders back to escaped regex literals
    let with_placeholders = escaped_str
        .replace(ESCAPED_STAR_PLACEHOLDER, "\\*")
        .replace(ESCAPED_BACKSLASH_PLACEHOLDER, "\\\\");

    let mut regex_pattern = with_placeholders;

    // When a pattern ends with ' .*' (space + unescaped wildcard) AND the trailing
    // wildcard is the ONLY unescaped wildcard, make the trailing space-and-args
    // optional so 'git *' matches both 'git add' and bare 'git'.
    let unescaped_star_count = processed.chars().filter(|&c| c == '*').count();
    if regex_pattern.ends_with(" .*") && unescaped_star_count == 1 {
        let len = regex_pattern.len();
        regex_pattern = format!("{}( .*)?", &regex_pattern[..len - 3]);
    }

    // Create regex that matches the entire string.
    // The 's' (dotAll) flag makes '.' match newlines.
    let flags = if case_insensitive { "(?si)" } else { "(?s)" };
    let full_pattern = format!("{}^{}$", flags, regex_pattern);

    match Regex::new(&full_pattern) {
        Ok(re) => re.is_match(command),
        Err(_) => false,
    }
}

/// Parse a permission rule string into a structured rule object.
pub fn parse_permission_rule(permission_rule: &str) -> ShellPermissionRule {
    // Check for legacy :* prefix syntax first (backwards compatibility)
    if let Some(prefix) = permission_rule_extract_prefix(permission_rule) {
        return ShellPermissionRule::Prefix {
            prefix: prefix.to_string(),
        };
    }

    // Check for new wildcard syntax (contains * but not :* at end)
    if has_wildcards(permission_rule) {
        return ShellPermissionRule::Wildcard {
            pattern: permission_rule.to_string(),
        };
    }

    // Otherwise, it's an exact match
    ShellPermissionRule::Exact {
        command: permission_rule.to_string(),
    }
}

/// Generate permission update suggestion for an exact command match.
pub fn suggestion_for_exact_command(tool_name: &str, command: &str) -> Vec<PermissionUpdate> {
    vec![PermissionUpdate::AddRules {
        destination: PermissionUpdateDestination::LocalSettings,
        rules: vec![PermissionRuleValue {
            tool_name: tool_name.to_string(),
            rule_content: Some(command.to_string()),
        }],
        behavior: PermissionBehavior::Allow,
    }]
}

/// Generate permission update suggestion for a prefix match.
pub fn suggestion_for_prefix(tool_name: &str, prefix: &str) -> Vec<PermissionUpdate> {
    vec![PermissionUpdate::AddRules {
        destination: PermissionUpdateDestination::LocalSettings,
        rules: vec![PermissionRuleValue {
            tool_name: tool_name.to_string(),
            rule_content: Some(format!("{}:*", prefix)),
        }],
        behavior: PermissionBehavior::Allow,
    }]
}
