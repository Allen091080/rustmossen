use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio::fs;

/// Supported platforms for desktop companion.
pub const SUPPORTED_PLATFORMS: &[&str] = &["macos", "windows"];

/// MCP server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
}

/// Get the Mossen Desktop config path.
pub async fn get_mossen_desktop_config_path(platform: &str) -> Result<PathBuf, String> {
    if !SUPPORTED_PLATFORMS.contains(&platform) {
        return Err(format!(
            "Unsupported platform: {} - desktop companion integration only works on macOS and WSL.",
            platform
        ));
    }

    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));

    if platform == "macos" {
        return Ok(home
            .join("Library")
            .join("Application Support")
            .join("Mossen")
            .join("mossen_desktop_config.json"));
    }

    // WSL/Windows path detection
    if let Ok(user_profile) = std::env::var("USERPROFILE") {
        let windows_home = user_profile.replace('\\', "/");
        let wsl_path = windows_home
            .trim_start_matches(|c: char| c.is_ascii_alphabetic() || c == ':');
        let config_path = PathBuf::from(format!(
            "/mnt/c{}/AppData/Roaming/Mossen/mossen_desktop_config.json",
            wsl_path
        ));

        if fs::metadata(&config_path).await.is_ok() {
            return Ok(config_path);
        }
    }

    // Try to find in /mnt/c/Users
    let users_dir = Path::new("/mnt/c/Users");
    if let Ok(mut entries) = fs::read_dir(users_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name().to_string_lossy().to_string();
            if name == "Public" || name == "Default" || name == "Default User" || name == "All Users"
            {
                continue;
            }

            let potential_config = users_dir
                .join(&name)
                .join("AppData")
                .join("Roaming")
                .join("Mossen")
                .join("mossen_desktop_config.json");

            if fs::metadata(&potential_config).await.is_ok() {
                return Ok(potential_config);
            }
        }
    }

    Err("Could not find desktop companion config file in Windows. Make sure Mossen Desktop is installed on Windows.".to_string())
}

/// Read MCP servers from Mossen Desktop config.
pub async fn read_mossen_desktop_mcp_servers(
    platform: &str,
) -> std::collections::HashMap<String, McpServerConfig> {
    if !SUPPORTED_PLATFORMS.contains(&platform) {
        return std::collections::HashMap::new();
    }

    let config_path = match get_mossen_desktop_config_path(platform).await {
        Ok(p) => p,
        Err(_) => return std::collections::HashMap::new(),
    };

    let config_content = match fs::read_to_string(&config_path).await {
        Ok(c) => c,
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                return std::collections::HashMap::new();
            }
            return std::collections::HashMap::new();
        }
    };

    let config: serde_json::Value = match serde_json::from_str(&config_content) {
        Ok(v) => v,
        Err(_) => return std::collections::HashMap::new(),
    };

    let mcp_servers = match config.get("mcpServers") {
        Some(v) if v.is_object() => v.as_object().unwrap(),
        _ => return std::collections::HashMap::new(),
    };

    let mut servers = std::collections::HashMap::new();

    for (name, server_config) in mcp_servers {
        if !server_config.is_object() {
            continue;
        }

        if let Ok(parsed) = serde_json::from_value::<McpServerConfig>(server_config.clone()) {
            servers.insert(name.clone(), parsed);
        }
    }

    servers
}
