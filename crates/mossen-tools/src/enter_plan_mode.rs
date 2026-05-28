//! # enter_plan_mode — PlanGate 工具
//!
//! 对应 TS `EnterPlanModeTool`（173 行）。请求进入计划模式，切换为只读探索。

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// 计划门 — 请求进入计划模式。
pub struct PlanGate;

/// 工具输入（无参数）。
#[derive(Debug, Clone, Deserialize)]
pub struct PlanGateInput {}

/// 工具输出。
#[derive(Debug, Clone, Serialize)]
pub struct PlanGateOutput {
    pub message: String,
}

fn build_input_schema() -> ToolInputSchema {
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(HashMap::new()),
        required: Some(vec![]),
        extra: HashMap::new(),
    }
}

#[async_trait]
impl Tool for PlanGate {
    fn name(&self) -> &str {
        "EnterPlanMode"
    }

    fn description(&self) -> &str {
        "Requests permission to enter plan mode for complex tasks requiring exploration and design"
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
        let output = PlanGateOutput {
            message: "Entered plan mode. You should now focus on exploring the codebase and designing an implementation approach.".to_string(),
        };

        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error: false,
            duration_ms: 0,
            metadata: HashMap::new(),
        })
    }
}
