//! # enter_worktree — BranchIsolator 工具
//!
//! 对应 TS `EnterWorktreeTool`（128 行）。创建隔离的 git worktree 并切换。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::process::Command;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};
use mossen_utils::hooks_utils::{execute_worktree_create_hook, has_worktree_create_hook};

/// 分支隔离器 — 创建 git worktree 并切入。
pub struct BranchIsolator;

#[derive(Debug, Clone, Deserialize)]
pub struct BranchIsolatorInput {
    #[serde(default)]
    pub name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{clear_active_worktree_session, BranchIsolator};
    use crate::exit_worktree::BranchRejoin;
    use mossen_agent::tool_registry::Tool;
    use mossen_types::ToolUseContext;
    use mossen_utils::hooks_utils::{
        register_runtime_hooks_context, unregister_runtime_hooks_context, HookMatcher, HooksContext,
    };
    use serde_json::Value;
    use std::collections::HashMap;
    use std::collections::HashSet;
    use std::path::Path;
    use std::sync::Arc;
    use tokio::process::Command;

    async fn git(cwd: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .output()
            .await
            .expect("git command");
        assert!(
            output.status.success(),
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn context(cwd: &Path) -> ToolUseContext {
        ToolUseContext {
            cwd: cwd.to_string_lossy().to_string(),
            additional_working_directories: None,
            extra: HashMap::new(),
        }
    }

    struct HookRegistration {
        id: String,
    }

    impl Drop for HookRegistration {
        fn drop(&mut self) {
            unregister_runtime_hooks_context(&self.id);
        }
    }

    fn hooked_context(
        cwd: &Path,
        hooks: Vec<(&str, String)>,
    ) -> (ToolUseContext, HookRegistration) {
        let mut registered_hooks = HashMap::new();
        for (event, command) in hooks {
            registered_hooks.insert(
                event.to_string(),
                vec![HookMatcher {
                    matcher: None,
                    hooks: vec![serde_json::json!({
                        "type": "command",
                        "command": command,
                        "timeout": 1
                    })],
                    plugin_root: None,
                    plugin_id: None,
                    plugin_name: None,
                    skill_root: None,
                    skill_name: None,
                }],
            );
        }
        let hooks_context = Arc::new(HooksContext {
            session_id: "worktree-hook-test".to_string(),
            original_cwd: cwd.to_string_lossy().to_string(),
            project_root: cwd.to_string_lossy().to_string(),
            is_non_interactive: true,
            trust_accepted: true,
            hooks_config_snapshot: None,
            registered_hooks: Some(registered_hooks),
            disable_all_hooks: false,
            managed_hooks_only: false,
            main_thread_agent_type: Some("main".to_string()),
            custom_backend_enabled: false,
            simple_mode: false,
            get_transcript_path: Arc::new(|session_id| format!("/tmp/{session_id}.jsonl")),
            get_agent_transcript_path: Arc::new(|agent_id| format!("/tmp/agent-{agent_id}.jsonl")),
            log_debug: Arc::new(|_| {}),
            log_error: Arc::new(|_| {}),
            log_event: Arc::new(|_, _| {}),
            get_settings: Arc::new(|| None),
            get_settings_for_source: Arc::new(|_| None),
            invalidate_session_env_cache: Arc::new(|| {}),
            dynamic_hook_executor: None,
            subprocess_env: std::env::vars().collect(),
            allowed_official_marketplace_names: HashSet::new(),
        });
        let id = register_runtime_hooks_context(hooks_context);
        let mut context = context(cwd);
        context.extra.insert(
            crate::task_hooks::HOOK_CONTEXT_ID_EXTRA_KEY.to_string(),
            serde_json::json!(id.clone()),
        );
        (context, HookRegistration { id })
    }

    #[tokio::test]
    async fn enter_and_exit_worktree_create_switch_metadata_and_remove() {
        clear_active_worktree_session();
        let temp = tempfile::tempdir().expect("tempdir");
        git(temp.path(), &["init"]).await;
        git(
            temp.path(),
            &["config", "user.email", "mossen@example.invalid"],
        )
        .await;
        git(temp.path(), &["config", "user.name", "Mossen Test"]).await;
        tokio::fs::write(temp.path().join("README.md"), "seed\n")
            .await
            .expect("seed");
        git(temp.path(), &["add", "README.md"]).await;
        git(temp.path(), &["commit", "-m", "seed"]).await;

        let enter = BranchIsolator
            .execute(
                serde_json::json!({ "name": "chain-smoke" }),
                &context(temp.path()),
            )
            .await
            .expect("enter worktree");
        assert!(!enter.is_error);
        let set_cwd = enter
            .metadata
            .get("set_cwd")
            .and_then(Value::as_str)
            .expect("set_cwd metadata");
        assert!(Path::new(set_cwd).exists());

        let exit = BranchRejoin
            .execute(
                serde_json::json!({
                    "action": "remove",
                    "discard_changes": true
                }),
                &context(Path::new(set_cwd)),
            )
            .await
            .expect("exit worktree");
        assert!(!exit.is_error);
        assert_eq!(
            exit.metadata.get("set_cwd").and_then(Value::as_str),
            Some(temp.path().to_string_lossy().as_ref())
        );
        assert!(!Path::new(set_cwd).exists());
        clear_active_worktree_session();
    }

    #[tokio::test]
    async fn enter_and_exit_worktree_use_hooks_outside_git_repo() {
        clear_active_worktree_session();
        let temp = tempfile::tempdir().expect("tempdir");
        let hook_worktree = temp.path().join("hook-worktree");
        let remove_marker = temp.path().join("worktree_remove_marker");
        let hook_worktree_arg = hook_worktree.to_string_lossy().replace('\'', "'\\''");
        let remove_marker_arg = remove_marker.to_string_lossy().replace('\'', "'\\''");
        let (context, _registration) = hooked_context(
            temp.path(),
            vec![
                (
                    "WorktreeCreate",
                    format!("mkdir -p '{hook_worktree_arg}'; printf '%s' '{hook_worktree_arg}'"),
                ),
                (
                    "WorktreeRemove",
                    format!("rm -rf '{hook_worktree_arg}'; printf removed > '{remove_marker_arg}'"),
                ),
            ],
        );

        let enter = BranchIsolator
            .execute(serde_json::json!({ "name": "hooked" }), &context)
            .await
            .expect("enter hook worktree");
        assert!(!enter.is_error, "{}", enter.output);
        assert_eq!(
            enter.metadata.get("set_cwd").and_then(Value::as_str),
            Some(hook_worktree.to_string_lossy().as_ref())
        );
        assert!(hook_worktree.exists());

        let exit = BranchRejoin
            .execute(
                serde_json::json!({
                    "action": "remove",
                    "discard_changes": true
                }),
                &context,
            )
            .await
            .expect("exit hook worktree");
        assert!(!exit.is_error, "{}", exit.output);
        assert_eq!(
            exit.metadata.get("set_cwd").and_then(Value::as_str),
            Some(temp.path().to_string_lossy().as_ref())
        );
        assert!(!hook_worktree.exists());
        assert_eq!(
            tokio::fs::read_to_string(remove_marker)
                .await
                .expect("WorktreeRemove marker"),
            "removed"
        );
        clear_active_worktree_session();
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BranchIsolatorOutput {
    #[serde(rename = "worktreePath")]
    pub worktree_path: String,
    #[serde(skip_serializing_if = "Option::is_none", rename = "worktreeBranch")]
    pub worktree_branch: Option<String>,
    #[serde(rename = "originalCwd")]
    pub original_cwd: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ActiveWorktreeSession {
    pub original_cwd: String,
    pub repo_root: String,
    pub worktree_path: String,
    pub worktree_branch: String,
    pub base_head: String,
    pub hook_managed: bool,
}

fn active_session_store() -> &'static Mutex<Option<ActiveWorktreeSession>> {
    static STORE: OnceLock<Mutex<Option<ActiveWorktreeSession>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(None))
}

pub(crate) fn active_worktree_session() -> Option<ActiveWorktreeSession> {
    active_session_store().lock().unwrap().clone()
}

pub(crate) fn clear_active_worktree_session() {
    *active_session_store().lock().unwrap() = None;
}

fn set_active_worktree_session(session: ActiveWorktreeSession) {
    *active_session_store().lock().unwrap() = Some(session);
}

fn sanitize_name(name: &str) -> String {
    let mut out: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '-'
            }
        })
        .collect();
    out.truncate(64);
    let out = out.trim_matches(['-', '.', '_']).to_string();
    if out.is_empty() {
        format!("wt-{}", uuid::Uuid::new_v4().simple())
    } else {
        out
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

    async fn execute(&self, input: Value, context: &ToolUseContext) -> anyhow::Result<ToolResult> {
        let start = Instant::now();
        let inp: BranchIsolatorInput = serde_json::from_value(input)?;
        if let Some(active) = active_worktree_session() {
            let output = BranchIsolatorOutput {
                worktree_path: active.worktree_path,
                worktree_branch: Some(active.worktree_branch),
                original_cwd: active.original_cwd,
                message: "A worktree session is already active.".to_string(),
            };
            return Ok(ToolResult {
                output: serde_json::to_string(&output)?,
                is_error: true,
                duration_ms: start.elapsed().as_millis() as u64,
                metadata: HashMap::new(),
            });
        }

        let name = sanitize_name(
            inp.name
                .as_deref()
                .filter(|name| !name.trim().is_empty())
                .unwrap_or("mossen-worktree"),
        );
        let original_cwd = PathBuf::from(&context.cwd);
        let repo_root_result = git_output(&original_cwd, &["rev-parse", "--show-toplevel"]).await;
        let repo_root = match repo_root_result {
            Ok(repo_root) => PathBuf::from(repo_root),
            Err(git_error) => {
                if let Some(hooks_context) = crate::task_hooks::runtime_hook_context(context) {
                    if has_worktree_create_hook(hooks_context.as_ref()) {
                        let worktree_path =
                            execute_worktree_create_hook(hooks_context.as_ref(), &name).await?;
                        let session = ActiveWorktreeSession {
                            original_cwd: context.cwd.clone(),
                            repo_root: context.cwd.clone(),
                            worktree_path: worktree_path.clone(),
                            worktree_branch: String::new(),
                            base_head: String::new(),
                            hook_managed: true,
                        };
                        set_active_worktree_session(session.clone());
                        let output = BranchIsolatorOutput {
                            worktree_path: session.worktree_path.clone(),
                            worktree_branch: None,
                            original_cwd: context.cwd.clone(),
                            message: "Created hook-managed worktree and switched subsequent tool calls to it.".to_string(),
                        };
                        let mut metadata = HashMap::new();
                        metadata
                            .insert("set_cwd".to_string(), Value::String(session.worktree_path));
                        return Ok(ToolResult {
                            output: serde_json::to_string(&output)?,
                            is_error: false,
                            duration_ms: start.elapsed().as_millis() as u64,
                            metadata,
                        });
                    }
                }
                return Err(git_error);
            }
        };
        let base_head = git_output(&repo_root, &["rev-parse", "HEAD"]).await?;
        let suffix = &uuid::Uuid::new_v4().simple().to_string()[..8];
        let worktree_name = format!("{name}-{suffix}");
        let branch = format!("mossen/{worktree_name}");
        let worktree_path = repo_root
            .join(".mossen")
            .join("worktrees")
            .join(&worktree_name);
        if let Some(parent) = worktree_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        git_status(
            &repo_root,
            &[
                "worktree",
                "add",
                "-b",
                &branch,
                worktree_path
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("non-utf8 worktree path"))?,
                "HEAD",
            ],
        )
        .await?;

        let session = ActiveWorktreeSession {
            original_cwd: context.cwd.clone(),
            repo_root: repo_root.to_string_lossy().to_string(),
            worktree_path: worktree_path.to_string_lossy().to_string(),
            worktree_branch: branch.clone(),
            base_head,
            hook_managed: false,
        };
        set_active_worktree_session(session.clone());
        let output = BranchIsolatorOutput {
            worktree_path: session.worktree_path.clone(),
            worktree_branch: Some(branch),
            original_cwd: context.cwd.clone(),
            message: "Created git worktree and switched subsequent tool calls to it.".to_string(),
        };
        let mut metadata = HashMap::new();
        metadata.insert("set_cwd".to_string(), Value::String(session.worktree_path));
        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error: false,
            duration_ms: start.elapsed().as_millis() as u64,
            metadata,
        })
    }
}
