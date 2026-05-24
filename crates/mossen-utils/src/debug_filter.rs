use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashSet;

/// Parsed debug filter configuration.
#[derive(Debug, Clone)]
pub struct DebugFilter {
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub is_exclusive: bool,
}

/// Parse debug filter string into a filter configuration.
///
/// Examples:
/// - "api,hooks" -> include only api and hooks categories
/// - "!1p,!file" -> exclude logging and file categories
/// - undefined/empty -> no filtering (show all)
pub fn parse_debug_filter(filter_string: Option<&str>) -> Option<DebugFilter> {
    let filter_string = filter_string?;
    let trimmed = filter_string.trim();
    if trimmed.is_empty() {
        return None;
    }

    let filters: Vec<&str> = trimmed
        .split(',')
        .map(|f| f.trim())
        .filter(|f| !f.is_empty())
        .collect();

    if filters.is_empty() {
        return None;
    }

    let has_exclusive = filters.iter().any(|f| f.starts_with('!'));
    let has_inclusive = filters.iter().any(|f| !f.starts_with('!'));

    // Mixed inclusive/exclusive filters: return None
    if has_exclusive && has_inclusive {
        return None;
    }

    let clean_filters: Vec<String> = filters
        .iter()
        .map(|f| f.trim_start_matches('!').to_lowercase())
        .collect();

    Some(DebugFilter {
        include: if has_exclusive {
            vec![]
        } else {
            clean_filters.clone()
        },
        exclude: if has_exclusive { clean_filters } else { vec![] },
        is_exclusive: has_exclusive,
    })
}

static MCP_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"^MCP server ["']([^"']+)["']"#).unwrap());
static PREFIX_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"^([^:\[]+):").unwrap());
static BRACKET_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\[([^\]]+)\]").unwrap());
static SECONDARY_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r":\s*([^:]+?)(?:\s+(?:type|mode|status|event))?:").unwrap());

/// Extract debug categories from a message.
///
/// Supports multiple patterns:
/// - "category: message" -> ["category"]
/// - "[CATEGORY] message" -> ["category"]
/// - "MCP server \"name\": message" -> ["mcp", "name"]
/// - "[INTERNAL-ONLY] 1P event: mossen_timer" -> ["internal-only", "1p"]
pub fn extract_debug_categories(message: &str) -> Vec<String> {
    let mut categories: Vec<String> = Vec::new();

    // Pattern 3: MCP server "servername"
    if let Some(caps) = MCP_REGEX.captures(message) {
        categories.push("mcp".to_string());
        if let Some(m) = caps.get(1) {
            categories.push(m.as_str().to_lowercase());
        }
    } else {
        // Pattern 1: "category: message"
        if let Some(caps) = PREFIX_REGEX.captures(message) {
            if let Some(m) = caps.get(1) {
                categories.push(m.as_str().trim().to_lowercase());
            }
        }
    }

    // Pattern 2: [CATEGORY] at the start
    if let Some(caps) = BRACKET_REGEX.captures(message) {
        if let Some(m) = caps.get(1) {
            categories.push(m.as_str().trim().to_lowercase());
        }
    }

    // Pattern 4: Check for 1p event
    if message.to_lowercase().contains("1p event:") {
        categories.push("1p".to_string());
    }

    // Pattern 5: Secondary categories
    if let Some(caps) = SECONDARY_REGEX.captures(message) {
        if let Some(m) = caps.get(1) {
            let secondary = m.as_str().trim().to_lowercase();
            if secondary.len() < 30 && !secondary.contains(' ') {
                categories.push(secondary);
            }
        }
    }

    // Remove duplicates
    let mut seen = HashSet::new();
    categories.retain(|c| seen.insert(c.clone()));
    categories
}

/// Check if debug message should be shown based on filter.
pub fn should_show_debug_categories(categories: &[String], filter: Option<&DebugFilter>) -> bool {
    let filter = match filter {
        None => return true,
        Some(f) => f,
    };

    if categories.is_empty() {
        return false;
    }

    if filter.is_exclusive {
        !categories.iter().any(|cat| filter.exclude.contains(cat))
    } else {
        categories.iter().any(|cat| filter.include.contains(cat))
    }
}

/// Main function to check if a debug message should be shown.
pub fn should_show_debug_message(message: &str, filter: Option<&DebugFilter>) -> bool {
    if filter.is_none() {
        return true;
    }

    let categories = extract_debug_categories(message);
    should_show_debug_categories(&categories, filter)
}
