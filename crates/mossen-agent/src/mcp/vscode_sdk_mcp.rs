//! VSCode SDK MCP — bidirectional communication with VSCode extension.
//!
//! Translates `services/mcp/vscodeSdkMcp.ts`.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// Auto mode state for VSCode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoModeEnabledState {
    Enabled,
    Disabled,
    OptIn,
}

/// Read auto mode enabled state from feature config.
pub fn read_auto_mode_enabled_state(config: Option<&serde_json::Value>) -> Option<AutoModeEnabledState> {
    let v = config?
        .as_object()?
        .get("enabled")?
        .as_str()?;
    match v {
        "enabled" => Some(AutoModeEnabledState::Enabled),
        "disabled" => Some(AutoModeEnabledState::Disabled),
        "opt-in" => Some(AutoModeEnabledState::OptIn),
        _ => None,
    }
}

/// Log event notification params.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEventParams {
    pub event_name: String,
    pub event_data: serde_json::Value,
}

/// Interface for the MCP client used by VSCode SDK.
#[async_trait::async_trait]
pub trait VscodeMcpClient: Send + Sync {
    async fn send_notification(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

/// VSCode SDK MCP state.
pub struct VscodeSdkMcp {
    client: Arc<RwLock<Option<Arc<dyn VscodeMcpClient>>>>,
}

impl VscodeSdkMcp {
    pub fn new() -> Self {
        Self {
            client: Arc::new(RwLock::new(None)),
        }
    }

    /// Send a file_updated notification to the VSCode MCP server.
    pub async fn notify_vscode_file_updated(
        &self,
        file_path: &str,
        old_content: Option<&str>,
        new_content: Option<&str>,
        is_ant_user: bool,
    ) {
        if !is_ant_user {
            return;
        }

        let client = self.client.read().await;
        if let Some(c) = client.as_ref() {
            let params = serde_json::json!({
                "filePath": file_path,
                "oldContent": old_content,
                "newContent": new_content,
            });
            if let Err(e) = c.send_notification("file_updated", params).await {
                tracing::debug!(
                    "[VSCode] Failed to send file_updated notification: {}",
                    e
                );
            }
        }
    }

    /// Set up the VSCode MCP connection.
    pub async fn setup(
        &self,
        client: Arc<dyn VscodeMcpClient>,
        feature_gates: HashMap<String, serde_json::Value>,
        auto_mode_state: Option<AutoModeEnabledState>,
    ) {
        // Store the client reference
        {
            let mut guard = self.client.write().await;
            *guard = Some(client.clone());
        }

        // Send experiment gates to VSCode
        let mut gates: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
        for (key, value) in &feature_gates {
            gates.insert(key.clone(), value.clone());
        }

        if let Some(state) = auto_mode_state {
            let state_str = match state {
                AutoModeEnabledState::Enabled => "enabled",
                AutoModeEnabledState::Disabled => "disabled",
                AutoModeEnabledState::OptIn => "opt-in",
            };
            gates.insert(
                "tengu_auto_mode_state".to_string(),
                serde_json::Value::String(state_str.to_string()),
            );
        }

        let params = serde_json::json!({ "gates": gates });
        if let Err(e) = client.send_notification("experiment_gates", params).await {
            tracing::debug!(
                "[VSCode] Failed to send experiment_gates notification: {}",
                e
            );
        }
    }

    /// Clear the stored client reference.
    pub async fn clear(&self) {
        let mut guard = self.client.write().await;
        *guard = None;
    }
}

impl Default for VscodeSdkMcp {
    fn default() -> Self {
        Self::new()
    }
}

/// `LogEventNotificationSchema` validator. Mirrors TS Zod schema that
/// requires `method == "logging/setLevel" | "logging/message"` and a
/// `params` object.
pub struct LogEventNotificationSchema;

impl LogEventNotificationSchema {
    pub fn parse(value: &serde_json::Value) -> Result<serde_json::Value, String> {
        let obj = value.as_object().ok_or("expected object")?;
        let method = obj
            .get("method")
            .and_then(|v| v.as_str())
            .ok_or("missing method")?;
        if !matches!(method, "logging/setLevel" | "logging/message") {
            return Err(format!("unexpected logging method: {method}"));
        }
        if !obj.get("params").map(|v| v.is_object()).unwrap_or(false) {
            return Err("params must be object".to_string());
        }
        Ok(value.clone())
    }
}

/// Setup the VS Code SDK MCP — wires the provided SDK clients into the
/// `VscodeSdkMcp` singleton. Mirrors TS `setupVscodeSdkMcp(sdkClients)`.
pub async fn setup_vscode_sdk_mcp(
    target: &VscodeSdkMcp,
    sdk_clients: Vec<std::sync::Arc<dyn VscodeMcpClient>>,
    feature_gates: HashMap<String, serde_json::Value>,
    auto_mode_state: Option<AutoModeEnabledState>,
) {
    // Only the first stdio/sdk-typed client is retained — matches the TS
    // behaviour where the IDE provides exactly one VSCode SDK connection.
    if let Some(client) = sdk_clients.into_iter().next() {
        target.setup(client, feature_gates, auto_mode_state).await;
    }
}
