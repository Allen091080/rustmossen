//! # remote_trigger — EventRelay 工具
//!
//! 对应 TS `RemoteTriggerTool`（162 行）。管理远程触发器（CRUD + run）。

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// 事件中继器 — 管理远程计划触发器。
pub struct EventRelay;

#[derive(Debug, Clone, Deserialize)]
pub struct EventRelayInput {
    /// 操作类型：list, get, create, update, run。
    pub action: String,
    /// 触发器 ID（get/update/run 必需）。
    #[serde(default)]
    pub trigger_id: Option<String>,
    /// JSON body（create/update 必需）。
    #[serde(default)]
    pub body: Option<HashMap<String, Value>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventRelayOutput {
    pub status: u16,
    pub json: String,
}

fn build_input_schema() -> ToolInputSchema {
    let mut properties = HashMap::new();
    properties.insert(
        "action".to_string(),
        serde_json::json!({
            "type": "string",
            "enum": ["list", "get", "create", "update", "run"]
        }),
    );
    properties.insert(
        "trigger_id".to_string(),
        serde_json::json!({
            "type": "string",
            "pattern": "^[\\w-]+$",
            "description": "Required for get, update, and run"
        }),
    );
    properties.insert(
        "body".to_string(),
        serde_json::json!({
            "type": "object",
            "additionalProperties": true,
            "description": "JSON body for create and update"
        }),
    );
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(properties),
        required: Some(vec!["action".to_string()]),
        extra: HashMap::new(),
    }
}

#[async_trait]
impl Tool for EventRelay {
    fn name(&self) -> &str {
        "RemoteTrigger"
    }
    fn description(&self) -> &str {
        "Manage scheduled remote agent triggers (list, get, create, update, run)"
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
        false
    }

    async fn execute(&self, input: Value, _context: &ToolUseContext) -> anyhow::Result<ToolResult> {
        let inp: EventRelayInput = serde_json::from_value(input)?;
        let output = EventRelayOutput {
            status: 501,
            json: serde_json::json!({
                "error": format!("RemoteTrigger '{}' is a stub in this build.", inp.action)
            })
            .to_string(),
        };
        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error: true,
            duration_ms: 0,
            metadata: HashMap::new(),
        })
    }
}
