//! # config — SettingsTuner 工具
//!
//! 对应 TS `ConfigTool`（441 行）。读取或修改 Mossen 设置。

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// 设置调节器 — 获取/设置 Mossen 配置项。
pub struct SettingsTuner;

#[derive(Debug, Clone, Deserialize)]
pub struct SettingsTunerInput {
    /// 设置键（如 "theme", "model", "permissions.defaultMode"）。
    pub setting: String,
    /// 新值。省略则返回当前值。
    #[serde(default)]
    pub value: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SettingsTunerOutput {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub setting: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "previousValue")]
    pub previous_value: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "newValue")]
    pub new_value: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn build_input_schema() -> ToolInputSchema {
    let mut properties = HashMap::new();
    properties.insert("setting".to_string(), serde_json::json!({
        "type": "string",
        "description": "The setting key (e.g., \"theme\", \"model\", \"permissions.defaultMode\")"
    }));
    properties.insert(
        "value".to_string(),
        serde_json::json!({
            "oneOf": [
                { "type": "string" },
                { "type": "boolean" },
                { "type": "number" }
            ],
            "description": "The new value. Omit to get current value."
        }),
    );
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(properties),
        required: Some(vec!["setting".to_string()]),
        extra: HashMap::new(),
    }
}

#[async_trait]
impl Tool for SettingsTuner {
    fn name(&self) -> &str {
        "Config"
    }
    fn description(&self) -> &str {
        "Get or set Mossen settings (theme, model, permissions)"
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
        let inp: SettingsTunerInput = serde_json::from_value(input)?;
        let is_get = inp.value.is_none();
        let output = SettingsTunerOutput {
            success: false,
            operation: Some(if is_get {
                "get".to_string()
            } else {
                "set".to_string()
            }),
            setting: Some(inp.setting),
            value: None,
            previous_value: None,
            new_value: inp.value,
            error: Some("Config tool is a stub in this build.".to_string()),
        };
        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error: true,
            duration_ms: 0,
            metadata: HashMap::new(),
        })
    }
}
