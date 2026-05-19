//! Shared utilities for expanding environment variables in MCP server configurations.

use regex::Regex;
use std::env;

/// Result of expanding environment variables in a string.
pub struct EnvExpansionResult {
    /// The expanded string with env variables replaced.
    pub expanded: String,
    /// List of variable names that were referenced but not found in the environment.
    pub missing_vars: Vec<String>,
}

/// Expand environment variables in a string value.
/// Handles `${VAR}` and `${VAR:-default}` syntax.
///
/// Returns an `EnvExpansionResult` with the expanded string and list of missing variables.
pub fn expand_env_vars_in_string(value: &str) -> EnvExpansionResult {
    let mut missing_vars: Vec<String> = Vec::new();

    let re = Regex::new(r"\$\{([^}]+)\}").unwrap();
    let expanded = re.replace_all(value, |caps: &regex::Captures| {
        let var_content = &caps[1];
        // Split on :- to support default values (limit to 2 parts to preserve :- in defaults)
        let parts: Vec<&str> = var_content.splitn(2, ":-").collect();
        let var_name = parts[0];
        let default_value = parts.get(1).copied();

        match env::var(var_name) {
            Ok(env_value) => env_value,
            Err(_) => {
                if let Some(default) = default_value {
                    default.to_string()
                } else {
                    // Track missing variable for error reporting
                    missing_vars.push(var_name.to_string());
                    // Return original if not found (allows debugging but will be reported as error)
                    caps[0].to_string()
                }
            }
        }
    });

    EnvExpansionResult {
        expanded: expanded.to_string(),
        missing_vars,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_with_existing_var() {
        env::set_var("TEST_MCP_VAR_123", "hello");
        let result = expand_env_vars_in_string("prefix_${TEST_MCP_VAR_123}_suffix");
        assert_eq!(result.expanded, "prefix_hello_suffix");
        assert!(result.missing_vars.is_empty());
        env::remove_var("TEST_MCP_VAR_123");
    }

    #[test]
    fn test_expand_with_default() {
        let result = expand_env_vars_in_string("${NONEXISTENT_VAR_XYZ:-fallback}");
        assert_eq!(result.expanded, "fallback");
        assert!(result.missing_vars.is_empty());
    }

    #[test]
    fn test_expand_missing_var() {
        let result = expand_env_vars_in_string("${NONEXISTENT_VAR_ABCDEF}");
        assert_eq!(result.expanded, "${NONEXISTENT_VAR_ABCDEF}");
        assert_eq!(result.missing_vars, vec!["NONEXISTENT_VAR_ABCDEF"]);
    }
}
