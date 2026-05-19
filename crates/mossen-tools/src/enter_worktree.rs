//! # enter_worktree — BranchIsolator 工具
//!
//! 对应 TS `EnterWorktreeTool`（128 行）。创建隔离的 git worktree 并切换。

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// 分支隔离器 — 创建 git worktree 并切入。
pub struct BranchIsolator;

#[derive(Debug, Clone, Deserialize)]
pub struct BranchIsolatorInput {
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BranchIsolatorOutput {
    #[serde(rename = "worktreePath")]
    pub worktree_path: String,
    #[serde(skip_serializing_if = "Option::is_none", rename = "worktreeBranch")]
    pub worktree_branch: Option<String>,
    pub message: String,
}

fn build_input_schema() -> ToolInputSchema {
    let mut properties = HashMap::new();
    properties.insert("name".to_string(), serde_json::json!({
        "type": "string",
        "description": "Optional name for the worktree (letters, digits, dots, underscores, dashes; max 64 chars)"
    }));
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(properties),
        required: Some(vec![]),
        extra: HashMap::new(),
    }
}

#[async_trait]
impl Tool for BranchIsolator {
    fn name(&self) -> &str {
        "EnterWorktree"
    }
    fn description(&self) -> &str {
        "Creates an isolated worktree and switches the session into it"
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
        let _inp: BranchIsolatorInput = serde_json::from_value(input)?;
        let output = BranchIsolatorOutput {
            worktree_path: String::new(),
            worktree_branch: None,
            message: "Worktree creation is a stub in this build.".to_string(),
        };
        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error: false,
            duration_ms: 0,
            metadata: HashMap::new(),
        })
    }
}
