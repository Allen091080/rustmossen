//! LSP server manager — manages multiple LSP server instances and routes requests by file extension.

use anyhow::{bail, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error};
use url::Url;

use super::config::{get_all_lsp_servers, LspServerState, ScopedLspServerConfig};
use super::server_instance::LspServerInstance;

/// LSP Server Manager — manages multiple LSP server instances
/// and routes requests based on file extensions.
pub struct LspServerManager {
    servers: Arc<RwLock<HashMap<String, LspServerInstance>>>,
    extension_map: Arc<RwLock<HashMap<String, Vec<String>>>>,
    opened_files: Arc<RwLock<HashMap<String, String>>>,
}

impl LspServerManager {
    /// Create a new LSP server manager.
    pub fn new() -> Self {
        Self {
            servers: Arc::new(RwLock::new(HashMap::new())),
            extension_map: Arc::new(RwLock::new(HashMap::new())),
            opened_files: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Initialize the manager by loading all configured LSP servers.
    pub async fn initialize(&self) -> Result<()> {
        let server_configs = get_all_lsp_servers().await?;
        debug!(
            "[LSP SERVER MANAGER] get_all_lsp_servers returned {} server(s)",
            server_configs.len()
        );

        let mut servers = self.servers.write().await;
        let mut extension_map = self.extension_map.write().await;

        for (server_name, config) in server_configs {
            if config.command.is_empty() {
                error!("Server {} missing required 'command' field", server_name);
                continue;
            }
            if config.extension_to_language.is_empty() {
                error!(
                    "Server {} missing required 'extension_to_language' field",
                    server_name
                );
                continue;
            }

            // Check if binary is available
            if !is_binary_installed(&config.command).await {
                debug!(
                    "Skipping LSP server {}: binary '{}' not found in PATH",
                    server_name, config.command
                );
                continue;
            }

            // Map file extensions to this server
            for ext in config.extension_to_language.keys() {
                let normalized = ext.to_lowercase();
                extension_map
                    .entry(normalized)
                    .or_default()
                    .push(server_name.clone());
            }

            // Create server instance
            match LspServerInstance::new(&server_name, config) {
                Ok(instance) => {
                    servers.insert(server_name.clone(), instance);
                }
                Err(e) => {
                    error!("Failed to initialize LSP server {}: {}", server_name, e);
                }
            }
        }

        debug!("LSP manager initialized with {} servers", servers.len());
        Ok(())
    }

    /// Shutdown all running servers and clear state.
    pub async fn shutdown(&self) -> Result<()> {
        let mut servers = self.servers.write().await;
        let mut errors = Vec::new();

        for (name, server) in servers.iter_mut() {
            let state = server.state();
            if state == LspServerState::Running || state == LspServerState::Error {
                if let Err(e) = server.stop().await {
                    errors.push(format!("{}: {}", name, e));
                }
            }
        }

        servers.clear();
        self.extension_map.write().await.clear();
        self.opened_files.write().await.clear();

        if !errors.is_empty() {
            bail!(
                "Failed to stop {} LSP server(s): {}",
                errors.len(),
                errors.join("; ")
            );
        }
        Ok(())
    }

    /// Get the server name for a given file path based on extension.
    fn get_server_name_for_file(
        &self,
        file_path: &str,
        extension_map: &HashMap<String, Vec<String>>,
    ) -> Option<String> {
        let ext = Path::new(file_path)
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))
            .unwrap_or_default();

        extension_map
            .get(&ext)
            .and_then(|names| names.first().cloned())
    }

    /// Ensure the appropriate LSP server is started for the given file.
    pub async fn ensure_server_started(&self, file_path: &str) -> Result<()> {
        let extension_map = self.extension_map.read().await;
        let server_name = match self.get_server_name_for_file(file_path, &extension_map) {
            Some(name) => name,
            None => return Ok(()),
        };
        drop(extension_map);

        let mut servers = self.servers.write().await;
        if let Some(server) = servers.get_mut(&server_name) {
            let state = server.state();
            if state == LspServerState::Stopped || state == LspServerState::Error {
                server.start().await?;
            }
        }
        Ok(())
    }

    /// Send a request to the appropriate LSP server for the given file.
    pub async fn send_request(
        &self,
        file_path: &str,
        method: &str,
        params: Value,
    ) -> Result<Option<Value>> {
        self.ensure_server_started(file_path).await?;

        let extension_map = self.extension_map.read().await;
        let server_name = match self.get_server_name_for_file(file_path, &extension_map) {
            Some(name) => name,
            None => return Ok(None),
        };
        drop(extension_map);

        let servers = self.servers.read().await;
        if let Some(server) = servers.get(&server_name) {
            let result = server.send_request(method, params).await?;
            Ok(Some(result))
        } else {
            Ok(None)
        }
    }

    /// Get all server names and their states.
    pub async fn get_all_servers(&self) -> Vec<(String, LspServerState)> {
        let servers = self.servers.read().await;
        servers
            .iter()
            .map(|(name, server)| (name.clone(), server.state()))
            .collect()
    }

    /// Synchronize file open to LSP server.
    pub async fn open_file(&self, file_path: &str, content: &str) -> Result<()> {
        self.ensure_server_started(file_path).await?;

        let extension_map = self.extension_map.read().await;
        let server_name = match self.get_server_name_for_file(file_path, &extension_map) {
            Some(name) => name,
            None => return Ok(()),
        };
        drop(extension_map);

        let file_uri = path_to_file_url(file_path);

        // Skip if already opened
        {
            let opened = self.opened_files.read().await;
            if opened.get(&file_uri) == Some(&server_name) {
                debug!("LSP: File already open, skipping didOpen for {}", file_path);
                return Ok(());
            }
        }

        // Get language ID
        let ext = Path::new(file_path)
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))
            .unwrap_or_default();

        let servers = self.servers.read().await;
        let language_id = servers
            .get(&server_name)
            .and_then(|s| s.config().extension_to_language.get(&ext))
            .cloned()
            .unwrap_or_else(|| "plaintext".to_string());

        if let Some(server) = servers.get(&server_name) {
            server
                .send_notification(
                    "textDocument/didOpen",
                    serde_json::json!({
                        "textDocument": {
                            "uri": file_uri,
                            "languageId": language_id,
                            "version": 1,
                            "text": content
                        }
                    }),
                )
                .await?;
        }
        drop(servers);

        self.opened_files
            .write()
            .await
            .insert(file_uri, server_name);
        debug!("LSP: Sent didOpen for {}", file_path);
        Ok(())
    }

    /// Synchronize file change to LSP server.
    pub async fn change_file(&self, file_path: &str, content: &str) -> Result<()> {
        let extension_map = self.extension_map.read().await;
        let server_name = match self.get_server_name_for_file(file_path, &extension_map) {
            Some(name) => name,
            None => return Ok(()),
        };
        drop(extension_map);

        let file_uri = path_to_file_url(file_path);

        // If not opened yet, open it
        {
            let opened = self.opened_files.read().await;
            if opened.get(&file_uri) != Some(&server_name) {
                drop(opened);
                return self.open_file(file_path, content).await;
            }
        }

        let servers = self.servers.read().await;
        if let Some(server) = servers.get(&server_name) {
            if server.state() == LspServerState::Running {
                server
                    .send_notification(
                        "textDocument/didChange",
                        serde_json::json!({
                            "textDocument": { "uri": file_uri, "version": 1 },
                            "contentChanges": [{ "text": content }]
                        }),
                    )
                    .await?;
                debug!("LSP: Sent didChange for {}", file_path);
            }
        }
        Ok(())
    }

    /// Synchronize file save to LSP server.
    pub async fn save_file(&self, file_path: &str) -> Result<()> {
        let extension_map = self.extension_map.read().await;
        let server_name = match self.get_server_name_for_file(file_path, &extension_map) {
            Some(name) => name,
            None => return Ok(()),
        };
        drop(extension_map);

        let servers = self.servers.read().await;
        if let Some(server) = servers.get(&server_name) {
            if server.state() == LspServerState::Running {
                let file_uri = path_to_file_url(file_path);
                server
                    .send_notification(
                        "textDocument/didSave",
                        serde_json::json!({
                            "textDocument": { "uri": file_uri }
                        }),
                    )
                    .await?;
                debug!("LSP: Sent didSave for {}", file_path);
            }
        }
        Ok(())
    }

    /// Synchronize file close to LSP server.
    pub async fn close_file(&self, file_path: &str) -> Result<()> {
        let extension_map = self.extension_map.read().await;
        let server_name = match self.get_server_name_for_file(file_path, &extension_map) {
            Some(name) => name,
            None => return Ok(()),
        };
        drop(extension_map);

        let file_uri = path_to_file_url(file_path);

        let servers = self.servers.read().await;
        if let Some(server) = servers.get(&server_name) {
            if server.state() == LspServerState::Running {
                server
                    .send_notification(
                        "textDocument/didClose",
                        serde_json::json!({
                            "textDocument": { "uri": file_uri }
                        }),
                    )
                    .await?;
            }
        }
        drop(servers);

        self.opened_files.write().await.remove(&file_uri);
        debug!("LSP: Sent didClose for {}", file_path);
        Ok(())
    }

    /// Check if a file is already open on a compatible LSP server.
    pub async fn is_file_open(&self, file_path: &str) -> bool {
        let file_uri = path_to_file_url(file_path);
        self.opened_files.read().await.contains_key(&file_uri)
    }
}

impl Default for LspServerManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert a file path to a file:// URL.
fn path_to_file_url(file_path: &str) -> String {
    let abs_path = std::path::Path::new(file_path);
    let abs_path = if abs_path.is_absolute() {
        abs_path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(abs_path)
    };
    Url::from_file_path(&abs_path)
        .map(|u| u.to_string())
        .unwrap_or_else(|_| format!("file://{}", abs_path.display()))
}

/// Check if a binary is installed and available in PATH.
async fn is_binary_installed(command: &str) -> bool {
    tokio::process::Command::new("which")
        .arg(command)
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// TS `createLSPServerManager` — constructor entry-point.
pub fn create_lsp_server_manager() -> LspServerManager {
    LspServerManager::default()
}
