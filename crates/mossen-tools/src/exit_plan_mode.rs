//! # exit_plan_mode — PlanRelease 工具
//!
//! 对应 TS `ExitPlanModeV2Tool`（494 行）。退出计划模式，可选附带允许的操作提示。

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// 计划释放 — 退出计划模式。
pub struct PlanRelease;

/// 允许的操作提示。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AllowedPrompt {
    /// 适用的工具名称。
    pub tool: String,
    /// 语义描述，如 "run tests"。
    pub prompt: String,
}

/// 工具输入。
#[derive(Debug, Clone, Deserialize)]
pub struct PlanReleaseInput {
    /// 计划所需的提示级权限列表。
    #[serde(default)]
    pub allowed_prompts: Option<Vec<AllowedPrompt>>,
}

/// 工具输出。
#[derive(Debug, Clone, Serialize)]
pub struct PlanReleaseOutput {
    pub message: String,
}

fn build_input_schema() -> ToolInputSchema {
    let mut properties = HashMap::new();
    properties.insert(
        "allowedPrompts".to_string(),
        serde_json::json!({
            "type": "array",
            "description": "Prompt-based permissions needed to implement the plan",
            "items": {
                "type": "object",
                "properties": {
                    "tool": { "type": "string", "description": "The tool this prompt applies to" },
                    "prompt": { "type": "string", "description": "Semantic description of the action" }
                },
                "required": ["tool", "prompt"]
            }
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
impl Tool for PlanRelease {
    fn name(&self) -> &str {
        "ExitPlanMode"
    }

    fn description(&self) -> &str {
        "Exit plan mode and present the implementation plan for approval"
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
        let _inp: PlanReleaseInput = serde_json::from_value(input)?;

        let output = PlanReleaseOutput {
            message: "Exited plan mode. The plan has been presented for approval.".to_string(),
        };

        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error: false,
            duration_ms: 0,
            metadata: HashMap::new(),
        })
    }
}
