//! LSP server instance — manages lifecycle of a single LSP server.

use anyhow::{bail, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing::{debug, error};

use super::client::LspClient;
use super::config::{LspServerState, ScopedLspServerConfig};

/// LSP error code for "content modified" — transient error during indexing.
const LSP_ERROR_CONTENT_MODIFIED: i32 = -32801;
/// Maximum retries for transient LSP errors.
const MAX_RETRIES_FOR_TRANSIENT_ERRORS: u32 = 3;
/// Base delay in ms for exponential backoff.
const RETRY_BASE_DELAY_MS: u64 = 500;

/// Manages the lifecycle of a single LSP server with state tracking.
pub struct LspServerInstance {
    name: String,
    config: ScopedLspServerConfig,
    state: LspServerState,
    start_time: Option<Instant>,
    last_error: Option<String>,
    restart_count: u32,
    crash_recovery_count: u32,
    client: LspClient,
}

impl LspServerInstance {
    /// Create a new LSP server instance.
    pub fn new(name: &str, config: ScopedLspServerConfig) -> Result<Self> {
        if config.restart_on_crash.is_some() {
            bail!(
                "LSP server '{}': restart_on_crash is not yet implemented",
                name
            );
        }
        if config.shutdown_timeout.is_some() {
            bail!(
                "LSP server '{}': shutdown_timeout is not yet implemented",
                name
            );
        }

        Ok(Self {
            name: name.to_string(),
            config,
            state: LspServerState::Stopped,
            start_time: None,
            last_error: None,
            restart_count: 0,
            crash_recovery_count: 0,
            client: LspClient::new(name),
        })
    }

    /// Get the server name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the server config.
    pub fn config(&self) -> &ScopedLspServerConfig {
        &self.config
    }

    /// Get the current server state.
    pub fn state(&self) -> LspServerState {
        self.state
    }

    /// Get the last error, if any.
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// Get the restart count.
    pub fn restart_count(&self) -> u32 {
        self.restart_count
    }

    /// Start the LSP server and initialize it.
    pub async fn start(&mut self) -> Result<()> {
        if self.state == LspServerState::Running || self.state == LspServerState::Starting {
            return Ok(());
        }

        let max_restarts = self.config.max_restarts.unwrap_or(3);
        if self.state == LspServerState::Error && self.crash_recovery_count > max_restarts {
            let msg = format!(
                "LSP server '{}' exceeded max crash recovery attempts ({})",
                self.name, max_restarts
            );
            self.last_error = Some(msg.clone());
            bail!("{}", msg);
        }

        self.state = LspServerState::Starting;
        debug!("Starting LSP server instance: {}", self.name);

        let env = if self.config.env.is_empty() {
            None
        } else {
            Some(&self.config.env)
        };

        if let Err(e) = self
            .client
            .start(
                &self.config.command,
                &self.config.args,
                env,
                self.config.workspace_folder.as_deref(),
            )
            .await
        {
            self.state = LspServerState::Error;
            self.last_error = Some(e.to_string());
            return Err(e);
        }

        // Build initialize params
        let workspace_folder = self.config.workspace_folder.clone().unwrap_or_else(|| {
            std::env::current_dir()
                .unwrap()
                .to_string_lossy()
                .to_string()
        });

        let workspace_uri = format!("file://{}", workspace_folder);
        let folder_name = std::path::Path::new(&workspace_folder)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let init_params = serde_json::json!({
            "processId": std::process::id(),
            "initializationOptions": self.config.initialization_options.clone().unwrap_or(Value::Object(Default::default())),
            "workspaceFolders": [{
                "uri": workspace_uri,
                "name": folder_name
            }],
            "rootPath": workspace_folder,
            "rootUri": workspace_uri,
            "capabilities": {
                "workspace": {
                    "configuration": false,
                    "workspaceFolders": false
                },
                "textDocument": {
                    "synchronization": {
                        "dynamicRegistration": false,
                        "willSave": false,
                        "willSaveWaitUntil": false,
                        "didSave": true
                    },
                    "publishDiagnostics": {
                        "relatedInformation": true,
                        "tagSupport": { "valueSet": [1, 2] },
                        "versionSupport": false,
                        "codeDescriptionSupport": true,
                        "dataSupport": false
                    },
                    "hover": {
                        "dynamicRegistration": false,
                        "contentFormat": ["markdown", "plaintext"]
                    },
                    "definition": {
                        "dynamicRegistration": false,
                        "linkSupport": true
                    },
                    "references": { "dynamicRegistration": false },
                    "documentSymbol": {
                        "dynamicRegistration": false,
                        "hierarchicalDocumentSymbolSupport": true
                    },
                    "callHierarchy": { "dynamicRegistration": false }
                },
                "general": {
                    "positionEncodings": ["utf-16"]
                }
            }
        });

        let init_future = self.client.initialize(init_params);

        let result = if let Some(timeout_ms) = self.config.startup_timeout {
            tokio::time::timeout(Duration::from_millis(timeout_ms), init_future)
                .await
                .map_err(|_| {
                    anyhow::anyhow!(
                        "LSP server '{}' timed out after {}ms during initialization",
                        self.name,
                        timeout_ms
                    )
                })?
        } else {
            init_future.await
        };

        match result {
            Ok(_) => {
                self.state = LspServerState::Running;
                self.start_time = Some(Instant::now());
                self.crash_recovery_count = 0;
                debug!("LSP server instance started: {}", self.name);
                Ok(())
            }
            Err(e) => {
                let _ = self.client.stop().await;
                self.state = LspServerState::Error;
                self.last_error = Some(e.to_string());
                Err(e)
            }
        }
    }

    /// Stop the server gracefully.
    pub async fn stop(&mut self) -> Result<()> {
        if self.state == LspServerState::Stopped || self.state == LspServerState::Stopping {
            return Ok(());
        }

        self.state = LspServerState::Stopping;
        match self.client.stop().await {
            Ok(()) => {
                self.state = LspServerState::Stopped;
                debug!("LSP server instance stopped: {}", self.name);
                Ok(())
            }
            Err(e) => {
                self.state = LspServerState::Error;
                self.last_error = Some(e.to_string());
                Err(e)
            }
        }
    }

    /// Manually restart the server.
    pub async fn restart(&mut self) -> Result<()> {
        if let Err(e) = self.stop().await {
            error!(
                "Failed to stop LSP server '{}' during restart: {}",
                self.name, e
            );
            return Err(e);
        }

        self.restart_count += 1;
        let max_restarts = self.config.max_restarts.unwrap_or(3);
        if self.restart_count > max_restarts {
            bail!(
                "Max restart attempts ({}) exceeded for server '{}'",
                max_restarts,
                self.name
            );
        }

        self.start().await
    }

    /// Check if server is healthy and ready for requests.
    pub fn is_healthy(&self) -> bool {
        self.state == LspServerState::Running
    }

    /// Send an LSP request with retry logic for transient errors.
    pub async fn send_request(&self, method: &str, params: Value) -> Result<Value> {
        if !self.is_healthy() {
            bail!(
                "Cannot send request to LSP server '{}': server is {}{}",
                self.name,
                self.state,
                self.last_error
                    .as_ref()
                    .map(|e| format!(", last error: {}", e))
                    .unwrap_or_default()
            );
        }

        let mut last_error: Option<anyhow::Error> = None;

        for attempt in 0..=MAX_RETRIES_FOR_TRANSIENT_ERRORS {
            match self.client.send_request(method, params.clone()).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    last_error = Some(e);
                    // Check if content modified error
                    if attempt < MAX_RETRIES_FOR_TRANSIENT_ERRORS {
                        let delay = RETRY_BASE_DELAY_MS * 2u64.pow(attempt);
                        debug!(
                            "LSP request '{}' to '{}' got transient error, retrying in {}ms (attempt {}/{})",
                            method, self.name, delay, attempt + 1, MAX_RETRIES_FOR_TRANSIENT_ERRORS
                        );
                        sleep(Duration::from_millis(delay)).await;
                        continue;
                    }
                    break;
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("unknown error")))
    }

    /// Send an LSP notification.
    pub async fn send_notification(&self, method: &str, params: Value) -> Result<()> {
        if !self.is_healthy() {
            bail!(
                "Cannot send notification to LSP server '{}': server is {}",
                self.name,
                self.state
            );
        }
        self.client.send_notification(method, params).await
    }

    /// Register a handler for notifications from the server.
    pub async fn on_notification<F>(&self, method: &str, handler: F)
    where
        F: Fn(Value) + Send + Sync + 'static,
    {
        self.client.on_notification(method, handler).await;
    }

    /// Register a handler for requests from the server.
    pub async fn on_request<F>(&self, method: &str, handler: F)
    where
        F: Fn(Value) -> Value + Send + Sync + 'static,
    {
        self.client.on_request(method, handler).await;
    }
}

/// TS `createLSPServerInstance` — constructor entry-point.
pub fn create_lsp_server_instance(
    name: &str,
    config: ScopedLspServerConfig,
) -> anyhow::Result<LspServerInstance> {
    LspServerInstance::new(name, config)
}
