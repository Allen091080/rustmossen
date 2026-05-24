//! Frontmatter parser for markdown files.
//! Extracts and parses YAML frontmatter between `---` delimiters.

use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;
use tracing::warn;

/// Frontmatter data parsed from YAML header.
#[derive(Debug, Clone, Default)]
pub struct FrontmatterData {
    pub allowed_tools: Option<Vec<String>>,
    pub description: Option<String>,
    pub type_field: Option<String>,
    pub argument_hint: Option<String>,
    pub when_to_use: Option<String>,
    pub version: Option<String>,
    pub hide_from_slash_command_tool: Option<String>,
    pub model: Option<String>,
    pub skills: Option<String>,
    pub user_invocable: Option<String>,
    pub hooks: Option<serde_json::Value>,
    pub effort: Option<String>,
    pub context: Option<String>,
    pub agent: Option<String>,
    pub paths: Option<Vec<String>>,
    pub shell: Option<String>,
    pub extra: HashMap<String, serde_yaml::Value>,
}

/// Parsed markdown with frontmatter extracted.
#[derive(Debug, Clone)]
pub struct ParsedMarkdown {
    pub frontmatter: FrontmatterData,
    pub content: String,
}

/// Characters that require quoting in YAML values.
static YAML_SPECIAL_CHARS: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"[{}\[\]*&#!|>%@`]|: "#).unwrap());

/// Frontmatter regex pattern.
static FRONTMATTER_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?s)^---\s*\n([\s\S]*?)---\s*\n?").unwrap());

/// Pre-processes frontmatter text to quote values that contain special YAML characters.
fn quote_problematic_values(frontmatter_text: &str) -> String {
    let key_value_re = Regex::new(r"^([a-zA-Z_-]+):\s+(.+)$").unwrap();
    let lines: Vec<&str> = frontmatter_text.split('\n').collect();
    let mut result: Vec<String> = Vec::with_capacity(lines.len());

    for line in lines {
        if let Some(caps) = key_value_re.captures(line) {
            let key = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let value = caps.get(2).map(|m| m.as_str()).unwrap_or("");

            if key.is_empty() || value.is_empty() {
                result.push(line.to_string());
                continue;
            }

            // Skip if already quoted
            if (value.starts_with('"') && value.ends_with('"'))
                || (value.starts_with('\'') && value.ends_with('\''))
            {
                result.push(line.to_string());
                continue;
            }

            // Quote if contains special YAML characters
            if YAML_SPECIAL_CHARS.is_match(value) {
                let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
                result.push(format!("{}: \"{}\"", key, escaped));
                continue;
            }
        }

        result.push(line.to_string());
    }

    result.join("\n")
}

/// Parses markdown content to extract frontmatter and content.
pub fn parse_frontmatter(markdown: &str, source_path: Option<&str>) -> ParsedMarkdown {
    let Some(caps) = FRONTMATTER_REGEX.captures(markdown) else {
        return ParsedMarkdown {
            frontmatter: FrontmatterData::default(),
            content: markdown.to_string(),
        };
    };

    let full_match = caps.get(0).unwrap();
    let frontmatter_text = caps.get(1).map(|m| m.as_str()).unwrap_or("");
    let content = markdown[full_match.end()..].to_string();

    let frontmatter = parse_yaml_frontmatter(frontmatter_text, source_path);

    ParsedMarkdown {
        frontmatter,
        content,
    }
}

fn parse_yaml_frontmatter(text: &str, source_path: Option<&str>) -> FrontmatterData {
    // First attempt: parse as-is
    match try_parse_yaml(text) {
        Some(data) => return data,
        None => {}
    }

    // Second attempt: quote problematic values
    let quoted = quote_problematic_values(text);
    match try_parse_yaml(&quoted) {
        Some(data) => data,
        None => {
            let location = source_path
                .map(|p| format!(" in {}", p))
                .unwrap_or_default();
            warn!("Failed to parse YAML frontmatter{}", location);
            FrontmatterData::default()
        }
    }
}

fn try_parse_yaml(text: &str) -> Option<FrontmatterData> {
    let parsed: serde_yaml::Value = serde_yaml::from_str(text).ok()?;
    let mapping = parsed.as_mapping()?;

    let mut data = FrontmatterData::default();

    for (key, value) in mapping {
        let key_str = key.as_str().unwrap_or("");
        match key_str {
            "allowed-tools" => {
                data.allowed_tools = Some(yaml_to_string_vec(value));
            }
            "description" => {
                data.description = value.as_str().map(|s| s.to_string());
            }
            "type" => {
                data.type_field = value.as_str().map(|s| s.to_string());
            }
            "argument-hint" => {
                data.argument_hint = value.as_str().map(|s| s.to_string());
            }
            "when_to_use" => {
                data.when_to_use = value.as_str().map(|s| s.to_string());
            }
            "version" => {
                data.version = value.as_str().map(|s| s.to_string());
            }
            "hide-from-slash-command-tool" => {
                data.hide_from_slash_command_tool = value.as_str().map(|s| s.to_string());
            }
            "model" => {
                data.model = value.as_str().map(|s| s.to_string());
            }
            "skills" => {
                data.skills = value.as_str().map(|s| s.to_string());
            }
            "user-invocable" => {
                data.user_invocable = value.as_str().map(|s| s.to_string());
            }
            "hooks" => {
                if let Ok(json_val) = serde_json::to_value(value) {
                    data.hooks = Some(json_val);
                }
            }
            "effort" => {
                data.effort = value.as_str().map(|s| s.to_string());
            }
            "context" => {
                data.context = value.as_str().map(|s| s.to_string());
            }
            "agent" => {
                data.agent = value.as_str().map(|s| s.to_string());
            }
            "paths" => {
                data.paths = Some(yaml_to_string_vec(value));
            }
            "shell" => {
                data.shell = value.as_str().map(|s| s.to_string());
            }
            _ => {
                data.extra.insert(key_str.to_string(), value.clone());
            }
        }
    }

    Some(data)
}

fn yaml_to_string_vec(value: &serde_yaml::Value) -> Vec<String> {
    match value {
        serde_yaml::Value::String(s) => vec![s.clone()],
        serde_yaml::Value::Sequence(seq) => seq
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect(),
        _ => vec![],
    }
}

/// Splits a comma-separated string and expands brace patterns.
/// Commas inside braces are not treated as separators.
/// Also accepts a vector of strings for ergonomic frontmatter.
pub fn split_path_in_frontmatter(input: &[String]) -> Vec<String> {
    input
        .iter()
        .flat_map(|s| split_single_path_in_frontmatter(s))
        .collect()
}

/// Splits a single comma-separated string with brace expansion.
pub fn split_single_path_in_frontmatter(input: &str) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut brace_depth = 0i32;

    for ch in input.chars() {
        match ch {
            '{' => {
                brace_depth += 1;
                current.push(ch);
            }
            '}' => {
                brace_depth -= 1;
                current.push(ch);
            }
            ',' if brace_depth == 0 => {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    parts.push(trimmed);
                }
                current.clear();
            }
            _ => {
                current.push(ch);
            }
        }
    }

    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        parts.push(trimmed);
    }

    parts
        .into_iter()
        .filter(|p| !p.is_empty())
        .flat_map(|pattern| expand_braces(&pattern))
        .collect()
}

/// Expands brace patterns in a glob string.
fn expand_braces(pattern: &str) -> Vec<String> {
    let brace_re = Regex::new(r"^([^\{]*)\{([^\}]+)\}(.*)$").unwrap();

    let Some(caps) = brace_re.captures(pattern) else {
        return vec![pattern.to_string()];
    };

    let prefix = caps.get(1).map(|m| m.as_str()).unwrap_or("");
    let alternatives = caps.get(2).map(|m| m.as_str()).unwrap_or("");
    let suffix = caps.get(3).map(|m| m.as_str()).unwrap_or("");

    let parts: Vec<&str> = alternatives.split(',').map(|s| s.trim()).collect();

    let mut expanded: Vec<String> = Vec::new();
    for part in parts {
        let combined = format!("{}{}{}", prefix, part, suffix);
        let further = expand_braces(&combined);
        expanded.extend(further);
    }

    expanded
}

/// Parses a positive integer value from frontmatter.
pub fn parse_positive_int_from_frontmatter(value: &serde_yaml::Value) -> Option<u64> {
    let parsed = match value {
        serde_yaml::Value::Number(n) => n.as_u64(),
        serde_yaml::Value::String(s) => s.parse::<u64>().ok(),
        _ => None,
    };

    parsed.filter(|&v| v > 0)
}

/// Validate and coerce a description value from frontmatter.
pub fn coerce_description_to_string(
    value: &serde_yaml::Value,
    component_name: Option<&str>,
    plugin_name: Option<&str>,
) -> Option<String> {
    match value {
        serde_yaml::Value::Null => None,
        serde_yaml::Value::String(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        serde_yaml::Value::Number(n) => Some(n.to_string()),
        serde_yaml::Value::Bool(b) => Some(b.to_string()),
        _ => {
            let source = match (plugin_name, component_name) {
                (Some(p), Some(c)) => format!("{}:{}", p, c),
                (None, Some(c)) => c.to_string(),
                _ => "unknown".to_string(),
            };
            warn!("Description invalid for {} - omitting", source);
            None
        }
    }
}

/// Parse a boolean frontmatter value.
/// Only returns true for literal true or "true" string.
pub fn parse_boolean_frontmatter(value: &serde_yaml::Value) -> bool {
    match value {
        serde_yaml::Value::Bool(true) => true,
        serde_yaml::Value::String(s) => s == "true",
        _ => false,
    }
}

/// Shell values accepted in `shell:` frontmatter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontmatterShell {
    Bash,
    Powershell,
}

const FRONTMATTER_SHELLS: &[(&str, FrontmatterShell)] = &[
    ("bash", FrontmatterShell::Bash),
    ("powershell", FrontmatterShell::Powershell),
];

/// Parse and validate the `shell:` frontmatter field.
pub fn parse_shell_frontmatter(
    value: &serde_yaml::Value,
    source: &str,
) -> Option<FrontmatterShell> {
    let s = match value {
        serde_yaml::Value::Null => return None,
        serde_yaml::Value::String(s) => s.trim().to_lowercase(),
        other => other.as_str().unwrap_or("").trim().to_lowercase(),
    };

    if s.is_empty() {
        return None;
    }

    for &(name, shell) in FRONTMATTER_SHELLS {
        if s == name {
            return Some(shell);
        }
    }

    let valid: Vec<&str> = FRONTMATTER_SHELLS.iter().map(|(n, _)| *n).collect();
    warn!(
        "Frontmatter 'shell: {}' in {} is not recognized. Valid values: {}. Falling back to bash.",
        s,
        source,
        valid.join(", ")
    );
    None
}
