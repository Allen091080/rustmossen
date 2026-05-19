//! # effort_control — EffortTuner 工具
//!
//! 对应 TS `commands/effort`。调节推理 effort 级别。

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// Effort 调节器 — 设置模型推理力度。
pub struct EffortTuner;

/// 合法的 effort 级别。
const VALID_LEVELS: &[&str] = &["low", "medium", "high", "max", "auto"];

/// 工具输入。
#[derive(Debug, Clone, Deserialize)]
pub struct EffortTunerInput {
    /// effort 级别：low | medium | high | max | auto。
    #[serde(default)]
    pub level: Option<String>,
}

/// 工具输出。
#[derive(Debug, Clone, Serialize)]
pub struct EffortTunerOutput {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
}

fn build_input_schema() -> ToolInputSchema {
    let mut properties = HashMap::new();
    properties.insert(
        "level".to_string(),
        serde_json::json!({
            "type": "string",
            "enum": ["low", "medium", "high", "max", "auto"],
            "description": "Effort level for model usage."
        }),
    );

    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(properties),
        required: None,
        extra: HashMap::new(),
    }
}

#[async_trait]
impl Tool for EffortTuner {
    fn name(&self) -> &str {
        "EffortControl"
    }

    fn description(&self) -> &str {
        "Set effort level for model usage"
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
        let inp: EffortTunerInput = serde_json::from_value(input)?;

        let had_level = inp.level.is_some();
        let output = match inp.level {
            Some(level) => {
                let normalized = level.to_lowercase();
                if VALID_LEVELS.contains(&normalized.as_str()) {
                    EffortTunerOutput {
                        message: format!("Set effort level to {normalized}"),
                        level: Some(normalized),
                    }
                } else {
                    EffortTunerOutput {
                        message: format!(
                            "Invalid effort level: {level}. Valid options: {}",
                            VALID_LEVELS.join(", ")
                        ),
                        level: None,
                    }
                }
            }
            None => EffortTunerOutput {
                message: "Current effort level: auto".to_string(),
                level: Some("auto".to_string()),
            },
        };

        let is_error = output.level.is_none() && had_level;

        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error,
            duration_ms: 0,
            metadata: HashMap::new(),
        })
    }
}
