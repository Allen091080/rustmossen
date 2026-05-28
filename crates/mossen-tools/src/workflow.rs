//! # workflow — PipelineRunner 工具
//!
//! 对应 TS `WorkflowTool`（feature-gated，仅 constants.ts）。编排多步骤工作流。

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// 管线运行器 — 编排并执行多步骤工作流。
pub struct PipelineRunner;

#[derive(Debug, Clone, Deserialize)]
pub struct PipelineRunnerInput {
    /// 工作流定义或标识符。
    pub workflow: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct PipelineRunnerOutput {
    pub success: bool,
    pub message: String,
}

fn build_input_schema() -> ToolInputSchema {
    let mut properties = HashMap::new();
    properties.insert(
        "workflow".to_string(),
        serde_json::json!({
            "type": "object",
            "description": "Workflow definition or identifier"
        }),
    );
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(properties),
        required: Some(vec!["workflow".to_string()]),
        extra: HashMap::new(),
    }
}

#[async_trait]
impl Tool for PipelineRunner {
    fn name(&self) -> &str {
        "Workflow"
    }
    fn description(&self) -> &str {
        "Orchestrate and execute multi-step workflows"
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

    async fn execute(
        &self,
        _input: Value,
        _context: &ToolUseContext,
    ) -> anyhow::Result<ToolResult> {
        let output = PipelineRunnerOutput {
            success: false,
            message: "Workflow tool is a stub in this build.".to_string(),
        };
        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error: true,
            duration_ms: 0,
            metadata: HashMap::new(),
        })
    }
}
