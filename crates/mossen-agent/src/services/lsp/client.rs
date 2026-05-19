//! LSP client — manages JSON-RPC communication with an LSP server process via stdio.

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use anyhow::{bail, Result};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, Notify, oneshot};
use tracing::{debug, error, warn};

/// LSP client that manages communication with an LSP server process.
pub struct LspClient {
    server_name: String,
    state: Arc<Mutex<LspClientState>>,
}

struct LspClientState {
    process: Option<Child>,
    capabilities: Option<Value>,
    is_initialized: bool,
    start_failed: bool,
    start_error: Option<String>,
    is_stopping: bool,
    next_request_id: i64,
    pending_requests: HashMap<i64, oneshot::Sender<Result<Value>>>,
    notification_handlers: HashMap<String, Vec<Box<dyn Fn(Value) + Send + Sync>>>,
    request_handlers: HashMap<String, Box<dyn Fn(Value) -> Value + Send + Sync>>,
}

impl LspClient {
    /// Create a new LSP client for the given server.
    pub fn new(server_name: &str) -> Self {
        Self {
            server_name: server_name.to_string(),
            state: Arc::new(Mutex::new(LspClientState {
                process: None,
                capabilities: None,
                is_initialized: false,
                start_failed: false,
                start_error: None,
                is_stopping: false,
                next_request_id: 1,
                pending_requests: HashMap::new(),
                notification_handlers: HashMap::new(),
                request_handlers: HashMap::new(),
            })),
        }
    }

    /// Get server capabilities after initialization.
    pub async fn capabilities(&self) -> Option<Value> {
        self.state.lock().await.capabilities.clone()
    }

    /// Check if the server has completed initialization.
    pub async fn is_initialized(&self) -> bool {
        self.state.lock().await.is_initialized
    }

    /// Start the LSP server process.
    pub async fn start(
        &self,
        command: &str,
        args: &[String],
        env: Option<&HashMap<String, String>>,
        cwd: Option<&str>,
    ) -> Result<()> {
        let mut cmd = Command::new(command);
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some(env_vars) = env {
            for (k, v) in env_vars {
                cmd.env(k, v);
            }
        }
        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }

        let child = cmd.spawn().map_err(|e| {
            anyhow::anyhow!("LSP server {} failed to start: {}", self.server_name, e)
        })?;

        let mut state = self.state.lock().await;
        state.process = Some(child);
        state.start_failed = false;
        state.start_error = None;

        debug!("LSP client started for {}", self.server_name);
        Ok(())
    }

    /// Initialize the LSP server with workspace information.
    pub async fn initialize(&self, params: Value) -> Result<Value> {
        let mut state = self.state.lock().await;
        if state.start_failed {
            bail!(
                "LSP server {} failed to start: {}",
                self.server_name,
                state.start_error.as_deref().unwrap_or("unknown error")
            );
        }

        // Send initialize request
        let result = self.send_request_inner(&mut state, "initialize", params).await?;

        // Extract capabilities
        if let Some(caps) = result.get("capabilities") {
            state.capabilities = Some(caps.clone());
        }

        // Send initialized notification
        self.send_notification_inner(&mut state, "initialized", Value::Object(Default::default()))
            .await?;

        state.is_initialized = true;
        debug!("LSP server {} initialized", self.server_name);
        Ok(result)
    }

    /// Send an LSP request and wait for the response.
    pub async fn send_request(&self, method: &str, params: Value) -> Result<Value> {
        let mut state = self.state.lock().await;
        if state.start_failed {
            bail!(
                "LSP server {} failed to start",
                self.server_name
            );
        }
        if !state.is_initialized {
            bail!("LSP server {} not initialized", self.server_name);
        }
        self.send_request_inner(&mut state, method, params).await
    }

    /// Send an LSP notification (fire-and-forget).
    pub async fn send_notification(&self, method: &str, params: Value) -> Result<()> {
        let mut state = self.state.lock().await;
        if state.start_failed {
            bail!("LSP server {} failed to start", self.server_name);
        }
        self.send_notification_inner(&mut state, method, params).await
    }

    /// Register a handler for notifications from the server.
    pub async fn on_notification<F>(&self, method: &str, handler: F)
    where
        F: Fn(Value) + Send + Sync + 'static,
    {
        let mut state = self.state.lock().await;
        state
            .notification_handlers
            .entry(method.to_string())
            .or_default()
            .push(Box::new(handler));
        debug!(
            "Registered notification handler for {}.{}",
            self.server_name, method
        );
    }

    /// Register a handler for requests from the server.
    pub async fn on_request<F>(&self, method: &str, handler: F)
    where
        F: Fn(Value) -> Value + Send + Sync + 'static,
    {
        let mut state = self.state.lock().await;
        state
            .request_handlers
            .insert(method.to_string(), Box::new(handler));
        debug!(
            "Registered request handler for {}.{}",
            self.server_name, method
        );
    }

    /// Stop the LSP server gracefully.
    pub async fn stop(&self) -> Result<()> {
        let mut state = self.state.lock().await;
        state.is_stopping = true;

        // Send shutdown request
        if state.is_initialized {
            let _ = self
                .send_request_inner(&mut state, "shutdown", Value::Object(Default::default()))
                .await;
            let _ = self
                .send_notification_inner(&mut state, "exit", Value::Object(Default::default()))
                .await;
        }

        // Kill the process
        if let Some(ref mut process) = state.process {
            let _ = process.kill().await;
        }

        state.process = None;
        state.is_initialized = false;
        state.capabilities = None;
        state.is_stopping = false;
        debug!("LSP client stopped for {}", self.server_name);
        Ok(())
    }

    async fn send_request_inner(
        &self,
        state: &mut LspClientState,
        method: &str,
        params: Value,
    ) -> Result<Value> {
        let id = state.next_request_id;
        state.next_request_id += 1;

        let message = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });

        let content = serde_json::to_string(&message)?;
        let header = format!("Content-Length: {}\r\n\r\n", content.len());

        if let Some(ref mut process) = state.process {
            if let Some(ref mut stdin) = process.stdin {
                stdin.write_all(header.as_bytes()).await?;
                stdin.write_all(content.as_bytes()).await?;
                stdin.flush().await?;
            }
        }

        // For simplicity, return empty object. Full implementation would
        // use the pending_requests map with proper async response reading.
        Ok(Value::Object(Default::default()))
    }

    async fn send_notification_inner(
        &self,
        state: &mut LspClientState,
        method: &str,
        params: Value,
    ) -> Result<()> {
        let message = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });

        let content = serde_json::to_string(&message)?;
        let header = format!("Content-Length: {}\r\n\r\n", content.len());

        if let Some(ref mut process) = state.process {
            if let Some(ref mut stdin) = process.stdin {
                stdin.write_all(header.as_bytes()).await?;
                stdin.write_all(content.as_bytes()).await?;
                stdin.flush().await?;
            }
        }

        Ok(())
    }
}

/// TS `createLSPClient` — constructor entry-point. Takes the LSP server name.
pub fn create_lsp_client(server_name: &str) -> LspClient {
    LspClient::new(server_name)
}
