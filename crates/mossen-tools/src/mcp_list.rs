//! # mcp_list — BridgeInventory 工具
//!
//! 对应 TS `ListMcpResourcesTool`（124 行）。列出 MCP 服务器资源。

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// 桥接资源清单 — 列出 MCP 服务器资源。
pub struct BridgeInventory;

#[derive(Debug, Clone, Deserialize)]
pub struct BridgeInventoryInput {
    #[serde(default)]
    pub server: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct McpResource {
    pub uri: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none", rename = "mimeType")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub server: String,
}

fn build_input_schema() -> ToolInputSchema {
    let mut properties = HashMap::new();
    properties.insert(
        "server".to_string(),
        serde_json::json!({
            "type": "string", "description": "Optional server name to filter resources by"
        }),
    );
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(properties),
        required: Some(vec![]),
        extra: HashMap::new(),
    }
}

#[async_trait]
impl Tool for BridgeInventory {
    fn name(&self) -> &str {
        "ListMcpResources"
    }
    fn description(&self) -> &str {
        "List resources from connected MCP servers"
    }
    fn tool_type(&self) -> ToolType {
        ToolType::Builtin
    }
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name().to_string(),
            description: self.description().to_string(),
            input_schema: build_input_schema(),
            cache_control: None,
        }
    }
    fn is_read_only(&self) -> bool {
        true
    }

    async fn execute(&self, input: Value, _context: &ToolUseContext) -> anyhow::Result<ToolResult> {
        let inp: BridgeInventoryInput = serde_json::from_value(input)?;
        let Some(manager) = mossen_mcp::server::global_manager() else {
            return Ok(ToolResult {
                output: "Error: MCP server manager not installed — resources cannot be listed."
                    .to_string(),
                is_error: true,
                duration_ms: 0,
                metadata: HashMap::new(),
            });
        };

        let requested_server = inp.server.as_deref();
        let requested_normalized = requested_server.map(mossen_mcp::normalize_name_for_mcp);
        let mut resources = Vec::new();

        for (server_name, server_resources) in manager.get_all_resources().await {
            let server_matches = match (requested_server, requested_normalized.as_deref()) {
                (Some(raw), Some(normalized)) => {
                    raw == server_name
                        || normalized == mossen_mcp::normalize_name_for_mcp(&server_name)
                }
                _ => true,
            };
            if !server_matches {
                continue;
            }

            for entry in server_resources {
                resources.push(McpResource {
                    uri: entry.resource.uri,
                    name: entry.resource.name,
                    mime_type: entry.resource.mime_type,
                    description: entry.resource.description,
                    server: entry.server,
                });
            }
        }

        resources.sort_by(|left, right| {
            left.server
                .cmp(&right.server)
                .then_with(|| left.uri.cmp(&right.uri))
        });

        Ok(ToolResult {
            output: serde_json::to_string(&resources)?,
            is_error: false,
            duration_ms: 0,
            metadata: HashMap::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp_read::BridgeReader;
    use mossen_mcp::config::{
        ConfigScope, McpServerConfig, ScopedMcpServerConfig, StdioServerConfig,
    };
    use mossen_mcp::protocol::Implementation;
    use mossen_mcp::McpServerManager;
    use serde_json::json;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex, OnceLock};

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("crate is under crates/mossen-tools")
            .to_path_buf()
    }

    fn test_context() -> ToolUseContext {
        ToolUseContext {
            cwd: repo_root().to_string_lossy().to_string(),
            additional_working_directories: None,
            extra: HashMap::new(),
        }
    }

    fn global_mcp_test_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .expect("global MCP test lock poisoned")
    }

    #[tokio::test]
    async fn resource_tools_list_and_read_live_global_mcp_manager() {
        let _guard = global_mcp_test_lock();
        mossen_mcp::server::clear_global_manager();
        let mock_server = repo_root().join("scripts/harness_mock_mcp_server.py");
        assert!(mock_server.exists(), "mock MCP server fixture exists");

        let manager = Arc::new(McpServerManager::new(Implementation {
            name: "mossen-tools-test".to_string(),
            version: "0.0.0".to_string(),
        }));
        manager
            .update_configs(HashMap::from([(
                "mcp_resource_server".to_string(),
                ScopedMcpServerConfig {
                    config: McpServerConfig::Stdio(StdioServerConfig {
                        transport_type: Some("stdio".to_string()),
                        command: "python3".to_string(),
                        args: vec![mock_server.to_string_lossy().to_string()],
                        env: None,
                    }),
                    scope: ConfigScope::Local,
                    plugin_source: None,
                },
            )]))
            .await;
        manager.connect_all().await;
        mossen_mcp::server::set_global_manager(manager.clone());

        let context = test_context();
        let inventory = BridgeInventory
            .execute(json!({ "server": "mcp_resource_server" }), &context)
            .await
            .expect("list resources");
        assert!(!inventory.is_error, "{}", inventory.output);
        let resources: serde_json::Value =
            serde_json::from_str(&inventory.output).expect("resource json");
        assert_eq!(resources[0]["uri"].as_str(), Some("mcp://fixture/doc"));
        assert_eq!(resources[0]["server"].as_str(), Some("mcp_resource_server"));

        let read = BridgeReader
            .execute(
                json!({ "server": "mcp_resource_server", "uri": "mcp://fixture/doc" }),
                &context,
            )
            .await
            .expect("read resource");
        assert!(!read.is_error, "{}", read.output);
        assert!(read.output.contains("RESOURCE_BODY_M3"), "{}", read.output);

        manager.disconnect_all().await;
        mossen_mcp::server::clear_global_manager();
    }
}
