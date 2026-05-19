//! Pure utility functions for MCP name normalization.
//! This file has no dependencies to avoid circular imports.

use regex::Regex;

/// Hosted server names are prefixed with this string.
const HOSTED_SERVER_PREFIX: &str = "hosted ";

/// Normalize server names to be compatible with the API pattern `^[a-zA-Z0-9_-]{1,64}$`.
/// Replaces any invalid characters (including dots and spaces) with underscores.
///
/// For hosted servers (names starting with "hosted "), also collapses
/// consecutive underscores and strips leading/trailing underscores to prevent
/// interference with the `__` delimiter used in MCP tool names.
pub fn normalize_name_for_mcp(name: &str) -> String {
    let re_invalid = Regex::new(r"[^a-zA-Z0-9_\-]").unwrap();
    let mut normalized = re_invalid.replace_all(name, "_").to_string();

    if name.starts_with(HOSTED_SERVER_PREFIX) {
        let re_multi_underscore = Regex::new(r"_+").unwrap();
        normalized = re_multi_underscore.replace_all(&normalized, "_").to_string();
        let re_leading_trailing = Regex::new(r"^_|_$").unwrap();
        normalized = re_leading_trailing.replace_all(&normalized, "").to_string();
    }

    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_normalization() {
        assert_eq!(normalize_name_for_mcp("hello.world"), "hello_world");
        assert_eq!(normalize_name_for_mcp("my server"), "my_server");
    }

    #[test]
    fn test_hosted_normalization() {
        assert_eq!(normalize_name_for_mcp("hosted Example Server"), "hosted_Example_Server");
        assert_eq!(normalize_name_for_mcp("hosted  double  space"), "hosted_double_space");
    }
}
