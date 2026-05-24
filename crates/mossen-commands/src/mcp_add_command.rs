//! MCP add command module
//!
//! This module provides the `mcp add` subcommand functionality.

use std::collections::HashMap;

/// Transport type for MCP server
#[derive(Debug, Clone)]
pub enum TransportType {
    Stdio,
    Sse,
    Http,
}

impl TransportType {
    /// Parse transport type from string
    pub fn from_str(s: Option<&str>) -> Self {
        match s {
            Some("sse") => TransportType::Sse,
            Some("http") => TransportType::Http,
            _ => TransportType::Stdio,
        }
    }
}

/// MCP server configuration
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub name: String,
    pub transport: TransportType,
    pub command_or_url: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub headers: Option<HashMap<String, String>>,
    pub scope: String,
}

impl ServerConfig {
    /// Create a new server config
    pub fn new(name: String, command_or_url: String, transport: TransportType) -> Self {
        Self {
            name,
            transport,
            command_or_url,
            args: Vec::new(),
            env: HashMap::new(),
            headers: None,
            scope: "local".to_string(),
        }
    }

    /// Check if this looks like a URL
    pub fn looks_like_url(&self) -> bool {
        self.command_or_url.starts_with("http://")
            || self.command_or_url.starts_with("https://")
            || self.command_or_url.starts_with("localhost")
            || self.command_or_url.ends_with("/sse")
            || self.command_or_url.ends_with("/mcp")
    }
}

/// Parse headers from string array
pub fn parse_headers(headers: Option<Vec<&str>>) -> Option<HashMap<String, String>> {
    headers.map(|h| {
        h.iter()
            .filter_map(|s| {
                let parts: Vec<&str> = s.splitn(2, ':').collect();
                if parts.len() == 2 {
                    Some((parts[0].trim().to_string(), parts[1].trim().to_string()))
                } else {
                    None
                }
            })
            .collect()
    })
}

/// Ensure config scope is valid
pub fn ensure_config_scope(scope: Option<&str>) -> String {
    match scope {
        Some("local" | "user" | "project") => scope.unwrap().to_string(),
        _ => "local".to_string(),
    }
}

/// Parse environment variables from string array
pub fn parse_env_vars(env: Option<Vec<&str>>) -> HashMap<String, String> {
    env.map(|e| {
        e.iter()
            .filter_map(|s| {
                let parts: Vec<&str> = s.splitn(2, '=').collect();
                if parts.len() == 2 {
                    Some((parts[0].to_string(), parts[1].to_string()))
                } else {
                    None
                }
            })
            .collect()
    })
    .unwrap_or_default()
}

/// Describe MCP config file path
pub fn describe_mcp_config_file_path(scope: &str) -> String {
    format!("~/.mossen/mcp-{}.json", scope)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transport_default() {
        assert!(matches!(
            TransportType::from_str(None),
            TransportType::Stdio
        ));
    }

    #[test]
    fn test_transport_sse() {
        assert!(matches!(
            TransportType::from_str(Some("sse")),
            TransportType::Sse
        ));
    }

    #[test]
    fn test_transport_http() {
        assert!(matches!(
            TransportType::from_str(Some("http")),
            TransportType::Http
        ));
    }

    #[test]
    fn test_parse_headers() {
        let headers = parse_headers(Some(vec!["Authorization: Bearer xxx", "X-Custom: value"]));
        assert!(headers.is_some());
        let h = headers.unwrap();
        assert_eq!(h.get("Authorization"), Some(&"Bearer xxx".to_string()));
        assert_eq!(h.get("X-Custom"), Some(&"value".to_string()));
    }

    #[test]
    fn test_ensure_config_scope() {
        assert_eq!(ensure_config_scope(Some("user")), "user");
        assert_eq!(ensure_config_scope(Some("project")), "project");
        assert_eq!(ensure_config_scope(Some("invalid")), "local");
        assert_eq!(ensure_config_scope(None), "local");
    }

    #[test]
    fn test_parse_env_vars() {
        let env = parse_env_vars(Some(vec!["API_KEY=xxx", "DEBUG=true"]));
        assert_eq!(env.get("API_KEY"), Some(&"xxx".to_string()));
        assert_eq!(env.get("DEBUG"), Some(&"true".to_string()));
    }

    #[test]
    fn test_looks_like_url() {
        let mut config = ServerConfig::new(
            "test".to_string(),
            "https://example.com/mcp".to_string(),
            TransportType::Http,
        );
        assert!(config.looks_like_url());

        config.command_or_url = "npx my-server".to_string();
        assert!(!config.looks_like_url());
    }
}
