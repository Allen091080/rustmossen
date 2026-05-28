//! Permission rule parser.
//!
//! Translates `utils/permissions/permissionRuleParser.ts`.
//! Handles parsing permission rule strings, escape/unescape, and legacy tool name aliasing.

use once_cell::sync::Lazy;
use std::collections::HashMap;

use super::permission_result::PermissionRuleValue;

// ─── Legacy Tool Name Aliases ───────────────────────────────────────────────

/// Tool name constants
pub const AGENT_TOOL_NAME: &str = "Agent";
pub const TASK_STOP_TOOL_NAME: &str = "TaskStop";
pub const TASK_OUTPUT_TOOL_NAME: &str = "TaskOutput";

static LEGACY_TOOL_NAME_ALIASES: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert("Task", AGENT_TOOL_NAME);
    m.insert("KillShell", TASK_STOP_TOOL_NAME);
    m.insert("AgentOutputTool", TASK_OUTPUT_TOOL_NAME);
    m.insert("BashOutputTool", TASK_OUTPUT_TOOL_NAME);
    m
});

/// Normalize a legacy tool name to its canonical form.
pub fn normalize_legacy_tool_name(name: &str) -> &str {
    LEGACY_TOOL_NAME_ALIASES.get(name).copied().unwrap_or(name)
}

/// Get all legacy names that map to a canonical name.
pub fn get_legacy_tool_names(canonical_name: &str) -> Vec<&'static str> {
    LEGACY_TOOL_NAME_ALIASES
        .iter()
        .filter(|(_, v)| **v == canonical_name)
        .map(|(k, _)| *k)
        .collect()
}

// ─── Escape/Unescape ────────────────────────────────────────────────────────

/// Escapes special characters in rule content for safe storage in permission rules.
/// Permission rules use the format "Tool(content)", so parentheses in content must be escaped.
pub fn escape_rule_content(content: &str) -> String {
    content
        .replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
}

/// Unescapes special characters in rule content after parsing from permission rules.
pub fn unescape_rule_content(content: &str) -> String {
    content
        .replace("\\(", "(")
        .replace("\\)", ")")
        .replace("\\\\", "\\")
}

// ─── Parser ─────────────────────────────────────────────────────────────────

/// Find the index of the first unescaped occurrence of a character.
fn find_first_unescaped_char(s: &str, target: char) -> Option<usize> {
    let bytes = s.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == target as u8 {
            let mut backslash_count = 0usize;
            let mut j = i as isize - 1;
            while j >= 0 && bytes[j as usize] == b'\\' {
                backslash_count += 1;
                j -= 1;
            }
            if backslash_count % 2 == 0 {
                return Some(i);
            }
        }
    }
    None
}

/// Find the index of the last unescaped occurrence of a character.
fn find_last_unescaped_char(s: &str, target: char) -> Option<usize> {
    let bytes = s.as_bytes();
    for i in (0..bytes.len()).rev() {
        if bytes[i] == target as u8 {
            let mut backslash_count = 0usize;
            let mut j = i as isize - 1;
            while j >= 0 && bytes[j as usize] == b'\\' {
                backslash_count += 1;
                j -= 1;
            }
            if backslash_count % 2 == 0 {
                return Some(i);
            }
        }
    }
    None
}

/// Parses a permission rule string into its components.
/// Handles escaped parentheses in the content portion.
///
/// Format: "ToolName" or "ToolName(content)"
pub fn permission_rule_value_from_string(rule_string: &str) -> PermissionRuleValue {
    // Find the first unescaped opening parenthesis
    let open_paren_index = match find_first_unescaped_char(rule_string, '(') {
        Some(i) => i,
        None => {
            return PermissionRuleValue {
                tool_name: normalize_legacy_tool_name(rule_string).to_string(),
                rule_content: None,
            };
        }
    };

    // Find the last unescaped closing parenthesis
    let close_paren_index = match find_last_unescaped_char(rule_string, ')') {
        Some(i) if i > open_paren_index => i,
        _ => {
            return PermissionRuleValue {
                tool_name: normalize_legacy_tool_name(rule_string).to_string(),
                rule_content: None,
            };
        }
    };

    // Ensure the closing paren is at the end
    if close_paren_index != rule_string.len() - 1 {
        return PermissionRuleValue {
            tool_name: normalize_legacy_tool_name(rule_string).to_string(),
            rule_content: None,
        };
    }

    let tool_name = &rule_string[..open_paren_index];
    let raw_content = &rule_string[open_paren_index + 1..close_paren_index];

    // Missing toolName (e.g., "(foo)") is malformed
    if tool_name.is_empty() {
        return PermissionRuleValue {
            tool_name: normalize_legacy_tool_name(rule_string).to_string(),
            rule_content: None,
        };
    }

    // Empty content or standalone wildcard → tool-wide rule
    if raw_content.is_empty() || raw_content == "*" {
        return PermissionRuleValue {
            tool_name: normalize_legacy_tool_name(tool_name).to_string(),
            rule_content: None,
        };
    }

    let rule_content = unescape_rule_content(raw_content);
    PermissionRuleValue {
        tool_name: normalize_legacy_tool_name(tool_name).to_string(),
        rule_content: Some(rule_content),
    }
}

/// Converts a permission rule value to its string representation.
pub fn permission_rule_value_to_string(rule_value: &PermissionRuleValue) -> String {
    match &rule_value.rule_content {
        None => rule_value.tool_name.clone(),
        Some(content) => {
            let escaped_content = escape_rule_content(content);
            format!("{}({})", rule_value.tool_name, escaped_content)
        }
    }
}
