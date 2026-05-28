//! # tungsten — InternalProbe 工具
//!
//! 对应 TS `TungstenTool`（61 行）。内部探针 — 禁用的 tmux 风格终端会话助手。

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// 内部探针 — 调试用存根工具。
pub struct InternalProbe;

/// 工具输入。
#[derive(Debug, Clone, Deserialize)]
pub struct InternalProbeInput {
    #[serde(default)]
    pub command: Option<String>,
}

/// 工具输出。
#[derive(Debug, Clone, Serialize)]
pub struct InternalProbeOutput {
    pub success: bool,
    pub message: String,
}

fn build_input_schema() -> ToolInputSchema {
    let mut properties = HashMap::new();
    properties.insert(
        "command".to_string(),
        serde_json::json!({
            "type": "string",
            "description": "Reserved stub field for Tungsten sessions."
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
impl Tool for InternalProbe {
    fn name(&self) -> &str {
        "Tungsten"
    }

    fn description(&self) -> &str {
        "Unavailable in this reconstructed source build."
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

    async fn execute(
        &self,
        _input: Value,
        _context: &ToolUseContext,
    ) -> anyhow::Result<ToolResult> {
        let output = InternalProbeOutput {
            success: false,
            message: "TungstenTool is unavailable in this reconstructed source build.".to_string(),
        };

        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error: false,
            duration_ms: 0,
            metadata: HashMap::new(),
        })
    }
}

/// `TungstenTool/TungstenLiveMonitor.tsx` `TungstenLiveMonitor` — React node
/// that renders nothing in the reconstructed source build. The Rust port
/// returns an empty string for the same "no UI" semantics.
pub fn tungsten_live_monitor() -> String {
    String::new()
}

/// Alias matching the TS export name.
#[allow(non_snake_case)]
pub fn TungstenLiveMonitor() -> String {
    tungsten_live_monitor()
}
