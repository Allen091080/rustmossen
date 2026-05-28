//! # exit_worktree — BranchRejoin 工具
//!
//! 对应 TS `ExitWorktreeTool`（330 行）。退出 worktree 并可选保留或删除。

use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::process::Command;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};
use mossen_utils::hooks_utils::execute_worktree_remove_hook;

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

async fn git_output(cwd: &Path, args: &[&str]) -> anyhow::Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .await?;
    if !output.status.success() {
        anyhow::bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

async fn git_status(cwd: &Path, args: &[&str]) -> anyhow::Result<()> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .await?;
    if !output.status.success() {
        anyhow::bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
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

    async fn execute(&self, input: Value, context: &ToolUseContext) -> anyhow::Result<ToolResult> {
        let start = Instant::now();
        let inp: BranchRejoinInput = serde_json::from_value(input)?;
        let Some(session) = crate::enter_worktree::active_worktree_session() else {
            let output = BranchRejoinOutput {
                action: inp.action,
                original_cwd: String::new(),
                worktree_path: String::new(),
                worktree_branch: None,
                discarded_files: None,
                discarded_commits: None,
                message: "No EnterWorktree session is active; nothing to exit.".to_string(),
            };
            return Ok(ToolResult {
                output: serde_json::to_string(&output)?,
                is_error: false,
                duration_ms: start.elapsed().as_millis() as u64,
                metadata: HashMap::new(),
            });
        };

        if !matches!(inp.action.as_str(), "keep" | "remove") {
            anyhow::bail!("action must be \"keep\" or \"remove\"");
        }

        if session.hook_managed {
            let mut is_error = false;
            let message = if inp.action == "remove" {
                match crate::task_hooks::runtime_hook_context(context) {
                    Some(hooks_context) => {
                        if execute_worktree_remove_hook(
                            hooks_context.as_ref(),
                            &session.worktree_path,
                        )
                        .await
                        {
                            crate::enter_worktree::clear_active_worktree_session();
                            "Removed hook-managed worktree and returned subsequent tool calls to the original cwd.".to_string()
                        } else {
                            is_error = true;
                            "WorktreeRemove hook did not remove the hook-managed worktree."
                                .to_string()
                        }
                    }
                    None => {
                        is_error = true;
                        "Cannot remove hook-managed worktree because hook context is unavailable."
                            .to_string()
                    }
                }
            } else {
                crate::enter_worktree::clear_active_worktree_session();
                "Kept hook-managed worktree on disk and returned subsequent tool calls to the original cwd.".to_string()
            };

            let output = BranchRejoinOutput {
                action: inp.action,
                original_cwd: session.original_cwd.clone(),
                worktree_path: session.worktree_path.clone(),
                worktree_branch: None,
                discarded_files: None,
                discarded_commits: None,
                message,
            };
            let mut metadata = HashMap::new();
            if !is_error {
                metadata.insert("set_cwd".to_string(), Value::String(session.original_cwd));
            }
            return Ok(ToolResult {
                output: serde_json::to_string(&output)?,
                is_error,
                duration_ms: start.elapsed().as_millis() as u64,
                metadata,
            });
        }

        let repo_root = Path::new(&session.repo_root);
        let worktree_path = Path::new(&session.worktree_path);
        let dirty_count = git_output(worktree_path, &["status", "--porcelain"])
            .await
            .unwrap_or_default()
            .lines()
            .count() as u64;
        let commit_count = git_output(
            worktree_path,
            &[
                "rev-list",
                "--count",
                &format!("{}..HEAD", session.base_head),
            ],
        )
        .await
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

        let mut is_error = false;
        let message = if inp.action == "remove" {
            let discard = inp.discard_changes.unwrap_or(false);
            if !discard && (dirty_count > 0 || commit_count > 0) {
                is_error = true;
                format!(
                    "Refusing to remove worktree with {dirty_count} changed files and {commit_count} commits. Re-run with discard_changes=true after user confirmation."
                )
            } else {
                git_status(
                    repo_root,
                    &["worktree", "remove", "--force", &session.worktree_path],
                )
                .await?;
                let _ = git_status(repo_root, &["branch", "-D", &session.worktree_branch]).await;
                crate::enter_worktree::clear_active_worktree_session();
                "Removed worktree and returned subsequent tool calls to the original cwd."
                    .to_string()
            }
        } else {
            crate::enter_worktree::clear_active_worktree_session();
            "Kept worktree on disk and returned subsequent tool calls to the original cwd."
                .to_string()
        };

        let output = BranchRejoinOutput {
            action: inp.action,
            original_cwd: session.original_cwd.clone(),
            worktree_path: session.worktree_path.clone(),
            worktree_branch: Some(session.worktree_branch.clone()),
            discarded_files: Some(dirty_count),
            discarded_commits: Some(commit_count),
            message,
        };
        let mut metadata = HashMap::new();
        if !is_error {
            metadata.insert("set_cwd".to_string(), Value::String(session.original_cwd));
        }
        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error,
            duration_ms: start.elapsed().as_millis() as u64,
            metadata,
        })
    }
}
