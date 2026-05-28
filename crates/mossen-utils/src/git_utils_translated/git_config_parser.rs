//! Lightweight parser for .git/config files.
//!
//! Verified against git's config.c:
//!   - Section names: case-insensitive, alphanumeric + hyphen
//!   - Subsection names (quoted): case-sensitive, backslash escapes (\\ and \")
//!   - Key names: case-insensitive, alphanumeric + hyphen
//!   - Values: optional quoting, inline comments (# or ;), backslash escapes

use std::path::Path;
use anyhow::Result;

/// Parse a single value from .git/config.
/// Finds the first matching key under the given section/subsection.
pub async fn parse_git_config_value(
    git_dir: &str,
    section: &str,
    subsection: Option<&str>,
    key: &str,
) -> Result<Option<String>> {
    let config_path = Path::new(git_dir).join("config");
    match tokio::fs::read_to_string(&config_path).await {
        Ok(config) => Ok(Some(parse_config_string(&config, section, subsection, key))),
        Err(_) => Ok(None),
    }
}

/// Parse a config value from an in-memory config string.
/// Exported for testing.
pub fn parse_config_string(
    config: &str,
    section: &str,
    subsection: Option<&str>,
    key: &str,
) -> Option<String> {
    let lines: Vec<&str> = config.split('\n').collect();
    let section_lower = section.to_lowercase();
    let key_lower = key.to_lowercase();

    let mut in_section = false;
    for line in lines {
        let trimmed = line.trim();

        // Skip empty lines and comment-only lines
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
            continue;
        }

        // Section header
        if trimmed.starts_with('[') {
            in_section = matches_section_header(trimmed, &section_lower, subsection);
            continue;
        }

        if !in_section {
            continue;
        }

        // Key-value line: find the key name
        if let Some((parsed_key, parsed_value)) = parse_key_value(trimmed) {
            if parsed_key.to_lowercase() == key_lower {
                return Some(parsed_value);
            }
        }
    }

    None
}

/// Parse a key = value line. Returns None if the line doesn't contain a valid key.
fn parse_key_value(line: &str) -> Option<(String, String)> {
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();

    // Read key: alphanumeric + hyphen, starting with alpha
    let mut i = 0;
    while i < len && is_key_char(chars[i]) {
        i += 1;
    }
    if i == 0 {
        return None;
    }
    let key: String = chars[0..i].iter().collect();

    // Skip whitespace
    while i < len && (chars[i] == ' ' || chars[i] == '\t') {
        i += 1;
    }

    // Must have '='
    if i >= len || chars[i] != '=' {
        return None;
    }
    i += 1; // skip '='

    // Skip whitespace after '='
    while i < len && (chars[i] == ' ' || chars[i] == '\t') {
        i += 1;
    }

    let value = parse_value(&chars, i);
    Some((key, value))
}

/// Parse a config value starting at position i.
/// Handles quoted strings, escape sequences, and inline comments.
fn parse_value(chars: &[char], start: usize) -> String {
    let mut result = String::new();
    let mut in_quote = false;
    let mut i = start;

    while i < chars.len() {
        let ch = chars[i];

        // Inline comments outside quotes end the value
        if !in_quote && (ch == '#' || ch == ';') {
            break;
        }

        if ch == '"' {
            in_quote = !in_quote;
            i += 1;
            continue;
        }

        if ch == '\\' && i + 1 < chars.len() {
            let next = chars[i + 1];
            if in_quote {
                // Inside quotes: recognize escape sequences
                match next {
                    'n' => result.push('\n'),
                    't' => result.push('\t'),
                    'b' => result.push('\u{0008}'),
                    '"' => result.push('"'),
                    '\\' => result.push('\\'),
                    _ => {
                        // Git silently drops the backslash for unknown escapes
                        result.push(next);
                    }
                }
                i += 2;
                continue;
            }
            // Outside quotes: handle backslash escape
            if next == '\\' {
                result.push('\\');
                i += 2;
                continue;
            }
        }

        result.push(ch);
        i += 1;
    }

    // Trim trailing whitespace from unquoted portions
    if !in_quote {
        result = trim_trailing_whitespace(&result);
    }

    result
}

fn trim_trailing_whitespace(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut end = chars.len();
    while end > 0 && (chars[end - 1] == ' ' || chars[end - 1] == '\t') {
        end -= 1;
    }
    chars[0..end].iter().collect()
}

/// Check if a config line like `[remote "origin"]` matches the given section/subsection.
/// Section matching is case-insensitive; subsection matching is case-sensitive.
fn matches_section_header(
    line: &str,
    section_lower: &str,
    subsection: Option<&str>,
) -> bool {
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();

    if chars.is_empty() || chars[0] != '[' {
        return false;
    }

    let mut i = 1;

    // Read section name
    while i < len
        && chars[i] != ']'
        && chars[i] != ' '
        && chars[i] != '\t'
        && chars[i] != '"'
    {
        i += 1;
    }

    let found_section: String = chars[1..i].iter().collect();
    let found_section_lower = found_section.to_lowercase();

    if found_section_lower != section_lower {
        return false;
    }

    match subsection {
        None => {
            // Simple section: must end with ']'
            i < len && chars[i] == ']'
        }
        Some(sub) => {
            // Skip whitespace before subsection quote
            while i < len && (chars[i] == ' ' || chars[i] == '\t') {
                i += 1;
            }

            // Must have opening quote
            if i >= len || chars[i] != '"' {
                return false;
            }
            i += 1; // skip opening quote

            // Read subsection — case-sensitive, handle \\ and \" escapes
            let mut found_subsection = String::new();
            while i < len && chars[i] != '"' {
                if chars[i] == '\\' && i + 1 < len {
                    let next = chars[i + 1];
                    if next == '\\' || next == '"' {
                        found_subsection.push(next);
                        i += 2;
                        continue;
                    }
                    // Git drops the backslash for other escapes in subsections
                    found_subsection.push(next);
                    i += 2;
                    continue;
                }
                found_subsection.push(chars[i]);
                i += 1;
            }

            // Must have closing quote followed by ']'
            if i >= len || chars[i] != '"' {
                return false;
            }
            i += 1; // skip closing quote

            if i >= len || chars[i] != ']' {
                return false;
            }

            found_subsection == sub
        }
    }
}

fn is_key_char(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch.is_ascii_digit() || ch == '-'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_section() {
        let config = r#"
[core]
    repositoryformatversion = 0
    filemode = true
"#;
        assert_eq!(
            parse_config_string(config, "core", None, "repositoryformatversion"),
            Some("0".to_string())
        );
    }

    #[test]
    fn test_parse_subsection() {
        let config = r#"
[remote "origin"]
    url = https://github.com/example/repo.git
    fetch = +refs/heads/*:refs/remotes/origin/*
"#;
        assert_eq!(
            parse_config_string(config, "remote", Some("origin"), "url"),
            Some("https://github.com/example/repo.git".to_string())
        );
    }
}
