//! # exit_worktree — BranchRejoin 工具
//!
//! 对应 TS `ExitWorktreeTool`（330 行）。退出 worktree 并可选保留或删除。

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// 分支回归器 — 退出 worktree 并返回原始工作目录。
pub struct BranchRejoin;

#[derive(Debug, Clone, Deserialize)]
pub struct BranchRejoinInput {
    /// "keep" 保留 worktree，"remove" 删除 worktree 和分支。
    pub action: String,
    /// 当 action="remove" 且有未提交改动时必须为 true。
    #[serde(default)]
    pub discard_changes: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BranchRejoinOutput {
    pub action: String,
    #[serde(rename = "originalCwd")]
    pub original_cwd: String,
    #[serde(rename = "worktreePath")]
    pub worktree_path: String,
    #[serde(skip_serializing_if = "Option::is_none", rename = "worktreeBranch")]
    pub worktree_branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "discardedFiles")]
    pub discarded_files: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "discardedCommits")]
    pub discarded_commits: Option<u64>,
    pub message: String,
}

fn build_input_schema() -> ToolInputSchema {
    let mut properties = HashMap::new();
    properties.insert("action".to_string(), serde_json::json!({
        "type": "string",
        "enum": ["keep", "remove"],
        "description": "\"keep\" leaves the worktree and branch on disk; \"remove\" deletes both."
    }));
    properties.insert("discard_changes".to_string(), serde_json::json!({
        "type": "boolean",
        "description": "Required true when action is \"remove\" and the worktree has uncommitted files or unmerged commits."
    }));
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(properties),
        required: Some(vec!["action".to_string()]),
        extra: HashMap::new(),
    }
}

#[async_trait]
impl Tool for BranchRejoin {
    fn name(&self) -> &str {
        "ExitWorktree"
    }
    fn description(&self) -> &str {
        "Exits the current worktree session and returns to the original directory"
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
        let inp: BranchRejoinInput = serde_json::from_value(input)?;
        let output = BranchRejoinOutput {
            action: inp.action,
            original_cwd: String::new(),
            worktree_path: String::new(),
            worktree_branch: None,
            discarded_files: None,
            discarded_commits: None,
            message: "Worktree exit is a stub in this build.".to_string(),
        };
        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error: false,
            duration_ms: 0,
            metadata: HashMap::new(),
        })
    }
}
