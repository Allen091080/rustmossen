//! # config — SettingsTuner 工具
//!
//! 对应 TS `ConfigTool`（441 行）。读取或修改 Mossen 设置。

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use mossen_agent::services::config::facade::{
    clear_mossen_config_overrides, get_all_mossen_config_values, resolve_mossen_config,
    set_mossen_config_override,
};
use mossen_agent::services::config::types::ConfigOverrideScope;
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
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
        let start = std::time::Instant::now();
        let setting = inp.setting.trim().to_string();
        let output = if setting == "*" || setting.eq_ignore_ascii_case("all") {
            SettingsTunerOutput {
                success: true,
                operation: Some("list".to_string()),
                setting: None,
                value: Some(serde_json::to_value(get_all_mossen_config_values())?),
                previous_value: None,
                new_value: None,
                error: None,
                source: Some("resolved".to_string()),
            }
        } else if let Some(value) = inp.value {
            let previous = resolve_mossen_config(&setting, Value::Null);
            if value.is_null() {
                clear_mossen_config_overrides(ConfigOverrideScope::Override, Some(&setting));
                let resolved = resolve_mossen_config(&setting, Value::Null);
                SettingsTunerOutput {
                    success: true,
                    operation: Some("clear".to_string()),
                    setting: Some(setting),
                    value: Some(resolved.value),
                    previous_value: Some(previous.value),
                    new_value: None,
                    error: None,
                    source: Some(format!("{:?}", resolved.source).to_ascii_lowercase()),
                }
            } else {
                set_mossen_config_override(&setting, value.clone(), ConfigOverrideScope::Override);
                let resolved = resolve_mossen_config(&setting, Value::Null);
                SettingsTunerOutput {
                    success: true,
                    operation: Some("set".to_string()),
                    setting: Some(setting),
                    value: Some(resolved.value),
                    previous_value: Some(previous.value),
                    new_value: Some(value),
                    error: None,
                    source: Some(format!("{:?}", resolved.source).to_ascii_lowercase()),
                }
            }
        } else {
            let resolved = resolve_mossen_config(&setting, Value::Null);
            SettingsTunerOutput {
                success: true,
                operation: Some("get".to_string()),
                setting: Some(setting),
                value: Some(resolved.value),
                previous_value: None,
                new_value: None,
                error: None,
                source: Some(format!("{:?}", resolved.source).to_ascii_lowercase()),
            }
        };
        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error: !output.success,
            duration_ms: start.elapsed().as_millis() as u64,
            metadata: HashMap::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::SettingsTuner;
    use mossen_agent::services::config::facade::reset_facade_for_testing;
    use mossen_agent::tool_registry::Tool;
    use mossen_types::ToolUseContext;
    use serde_json::Value;
    use std::collections::HashMap;

    fn context() -> ToolUseContext {
        ToolUseContext {
            cwd: ".".to_string(),
            additional_working_directories: None,
            extra: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn config_tool_sets_gets_and_clears_runtime_override() {
        reset_facade_for_testing();
        let tool = SettingsTuner;
        let set = tool
            .execute(
                serde_json::json!({
                    "setting": "mossen.tool.autoBackgroundAgentsEnabled",
                    "value": true
                }),
                &context(),
            )
            .await
            .expect("set");
        assert!(!set.is_error);
        let get = tool
            .execute(
                serde_json::json!({
                    "setting": "mossen.tool.autoBackgroundAgentsEnabled"
                }),
                &context(),
            )
            .await
            .expect("get");
        let output: Value = serde_json::from_str(&get.output).expect("json");
        assert_eq!(output["value"], true);
        assert_eq!(output["source"], "override");

        let clear = tool
            .execute(
                serde_json::json!({
                    "setting": "mossen.tool.autoBackgroundAgentsEnabled",
                    "value": null
                }),
                &context(),
            )
            .await
            .expect("clear");
        assert!(!clear.is_error);
    }
}
