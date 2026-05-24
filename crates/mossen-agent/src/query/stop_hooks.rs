/// Stop hooks — post-turn lifecycle hooks that run after the model stops generating.
///
/// Handles:
/// - Unfinished task guard (re-prompts model if tasks remain unfinished)
/// - Stop hooks (user-defined scripts that run after each turn)
/// - TeammateIdle / TaskCompleted hooks (for teammate agents)
/// - Background tasks: prompt suggestion, memory extraction, auto-dream
use std::collections::HashSet;

/// Result of running stop hooks
#[derive(Debug, Clone)]
pub struct StopHookResult {
    pub blocking_errors: Vec<serde_json::Value>,
    pub prevent_continuation: bool,
}

/// Trait for external dependencies
#[async_trait::async_trait]
pub trait StopHooksContext: Send + Sync {
    fn get_agent_id(&self) -> Option<String>;
    fn get_agent_name(&self) -> Option<String>;
    fn get_team_name(&self) -> Option<String>;
    fn is_teammate(&self) -> bool;
    fn is_bare_mode(&self) -> bool;
    fn is_extract_mode_active(&self) -> bool;
    fn is_non_interactive_session(&self) -> bool;
    fn is_todo_v2_enabled(&self) -> bool;
    fn is_custom_backend_enabled(&self) -> bool;
    fn is_env_defined_falsy(&self, key: &str) -> bool;
    fn is_aborted(&self) -> bool;

    fn get_task_list_id(&self) -> String;
    async fn list_tasks(&self, list_id: &str) -> Result<Vec<Task>, String>;
    fn prioritize_tasks_for_display(&self, tasks: Vec<Task>) -> Vec<Task>;

    async fn execute_stop_hooks(&self, messages: &[serde_json::Value]) -> Vec<HookResult>;
    async fn execute_teammate_idle_hooks(&self) -> Vec<HookResult>;
    async fn execute_task_completed_hooks(&self, task: &Task) -> Vec<HookResult>;

    fn execute_prompt_suggestion(&self, context: &serde_json::Value);
    fn execute_extract_memories(&self, context: &serde_json::Value);
    fn execute_auto_dream(&self, context: &serde_json::Value);

    fn save_cache_safe_params(&self, params: &serde_json::Value);
    fn log_event(&self, event: &str, metadata: serde_json::Value);
}

#[derive(Debug, Clone)]
pub struct Task {
    pub id: String,
    pub subject: String,
    pub description: Option<String>,
    pub status: String,
    pub owner: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HookResult {
    pub blocking_error: Option<String>,
    pub prevent_continuation: bool,
    pub stop_reason: Option<String>,
}

const UNFINISHED_TASK_GUARD_MARKER: &str = "MOSSEN_UNFINISHED_TASK_GUARD";
const MAX_UNFINISHED_TASK_GUARD_ATTEMPTS: usize = 3;

fn get_task_owner_aliases(ctx: &dyn StopHooksContext) -> HashSet<String> {
    let mut aliases = HashSet::new();
    if let Some(agent_id) = ctx.get_agent_id() {
        aliases.insert(agent_id);
    }
    if let Some(agent_name) = ctx.get_agent_name() {
        if let Some(team_name) = ctx.get_team_name() {
            aliases.insert(format!("{}@{}", agent_name, team_name));
        }
        aliases.insert(agent_name);
    }
    aliases
}

fn is_relevant_unfinished_task(
    task: &Task,
    agent_id: Option<&str>,
    owner_aliases: &HashSet<String>,
) -> bool {
    if task.status == "completed" {
        return false;
    }
    if agent_id.is_some() {
        return task
            .owner
            .as_ref()
            .map_or(false, |o| owner_aliases.contains(o));
    }
    task.owner.is_none()
        || task
            .owner
            .as_ref()
            .map_or(false, |o| owner_aliases.contains(o))
}

fn create_unfinished_task_digest(tasks: &[Task]) -> String {
    tasks
        .iter()
        .map(|t| {
            format!(
                "{}:{}:{}",
                t.id,
                t.status,
                t.owner.as_deref().unwrap_or("main")
            )
        })
        .collect::<Vec<_>>()
        .join("|")
}

fn count_unfinished_task_guard_messages(messages: &[serde_json::Value], digest: &str) -> usize {
    let marker = format!("{}:{}", UNFINISHED_TASK_GUARD_MARKER, digest);
    messages
        .iter()
        .filter(|m| get_message_content_text(m).contains(&marker))
        .count()
}

fn get_message_content_text(message: &serde_json::Value) -> String {
    if let Some(content) = message.get("message").and_then(|m| m.get("content")) {
        if let Some(s) = content.as_str() {
            return s.to_string();
        }
        if let Some(arr) = content.as_array() {
            return arr
                .iter()
                .filter_map(|block| block.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join("\n");
        }
    }
    String::new()
}

fn format_unfinished_task_for_guard(task: &Task) -> String {
    let owner = task
        .owner
        .as_ref()
        .map(|o| format!(" owner={}", o))
        .unwrap_or_default();
    format!("- #{} [{}{}] {}", task.id, task.status, owner, task.subject)
}

/// Main stop hooks handler
pub async fn handle_stop_hooks(
    ctx: &dyn StopHooksContext,
    messages_for_query: &[serde_json::Value],
    assistant_messages: &[serde_json::Value],
    query_source: &str,
) -> StopHookResult {
    // Save cache params for main session queries
    if query_source == "repl_main_thread" || query_source == "sdk" {
        ctx.save_cache_safe_params(&serde_json::json!({}));
    }

    // Background tasks (fire-and-forget style in non-bare mode)
    if !ctx.is_bare_mode() {
        if !ctx.is_env_defined_falsy("MOSSEN_CODE_ENABLE_PROMPT_SUGGESTION") {
            ctx.execute_prompt_suggestion(&serde_json::json!({}));
        }
        if ctx.get_agent_id().is_none() && ctx.is_extract_mode_active() {
            ctx.execute_extract_memories(&serde_json::json!({}));
        }
        if ctx.get_agent_id().is_none() {
            ctx.execute_auto_dream(&serde_json::json!({}));
        }
    }

    // Unfinished task guard
    if let Some(blocking_msg) = create_unfinished_task_blocking_message(
        ctx,
        messages_for_query,
        assistant_messages,
        query_source,
    )
    .await
    {
        return StopHookResult {
            blocking_errors: vec![blocking_msg],
            prevent_continuation: false,
        };
    }

    // Custom backend skip
    if ctx.is_custom_backend_enabled()
        && std::env::var("MOSSEN_CODE_ENABLE_CUSTOM_BACKEND_STOP_HOOKS").as_deref() != Ok("1")
    {
        return StopHookResult {
            blocking_errors: Vec::new(),
            prevent_continuation: false,
        };
    }

    // Execute stop hooks
    let all_messages: Vec<serde_json::Value> = messages_for_query
        .iter()
        .chain(assistant_messages.iter())
        .cloned()
        .collect();

    let results = ctx.execute_stop_hooks(&all_messages).await;
    let mut blocking_errors = Vec::new();
    let mut prevented = false;

    for result in &results {
        if let Some(ref err) = result.blocking_error {
            blocking_errors.push(serde_json::json!({
                "type": "user",
                "message": {"content": err},
                "isMeta": true,
            }));
        }
        if result.prevent_continuation {
            prevented = true;
        }
    }

    if ctx.is_aborted() {
        return StopHookResult {
            blocking_errors: Vec::new(),
            prevent_continuation: true,
        };
    }

    if prevented {
        return StopHookResult {
            blocking_errors: Vec::new(),
            prevent_continuation: true,
        };
    }

    if !blocking_errors.is_empty() {
        return StopHookResult {
            blocking_errors,
            prevent_continuation: false,
        };
    }

    // Teammate hooks
    if ctx.is_teammate() {
        let agent_name = ctx.get_agent_name().unwrap_or_default();
        let task_list_id = ctx.get_task_list_id();

        if let Ok(tasks) = ctx.list_tasks(&task_list_id).await {
            let in_progress: Vec<&Task> = tasks
                .iter()
                .filter(|t| t.status == "in_progress" && t.owner.as_deref() == Some(&agent_name))
                .collect();

            for task in in_progress {
                let results = ctx.execute_task_completed_hooks(task).await;
                for result in &results {
                    if let Some(ref err) = result.blocking_error {
                        blocking_errors.push(serde_json::json!({
                            "type": "user",
                            "message": {"content": err},
                            "isMeta": true,
                        }));
                    }
                    if result.prevent_continuation {
                        return StopHookResult {
                            blocking_errors: Vec::new(),
                            prevent_continuation: true,
                        };
                    }
                }
            }
        }

        // TeammateIdle hooks
        let idle_results = ctx.execute_teammate_idle_hooks().await;
        for result in &idle_results {
            if let Some(ref err) = result.blocking_error {
                blocking_errors.push(serde_json::json!({
                    "type": "user",
                    "message": {"content": err},
                    "isMeta": true,
                }));
            }
            if result.prevent_continuation {
                return StopHookResult {
                    blocking_errors: Vec::new(),
                    prevent_continuation: true,
                };
            }
        }

        if !blocking_errors.is_empty() {
            return StopHookResult {
                blocking_errors,
                prevent_continuation: false,
            };
        }
    }

    StopHookResult {
        blocking_errors: Vec::new(),
        prevent_continuation: false,
    }
}

async fn create_unfinished_task_blocking_message(
    ctx: &dyn StopHooksContext,
    messages_for_query: &[serde_json::Value],
    assistant_messages: &[serde_json::Value],
    query_source: &str,
) -> Option<serde_json::Value> {
    if (!query_source.starts_with("repl_main_thread") && !query_source.starts_with("agent:"))
        || ctx.is_non_interactive_session()
        || !ctx.is_todo_v2_enabled()
    {
        return None;
    }

    let task_list_id = ctx.get_task_list_id();
    let tasks = ctx.list_tasks(&task_list_id).await.ok()?;

    let agent_id = ctx.get_agent_id();
    let owner_aliases = get_task_owner_aliases(ctx);
    let prioritized = ctx.prioritize_tasks_for_display(tasks);
    let unresolved: Vec<&Task> = prioritized
        .iter()
        .filter(|t| is_relevant_unfinished_task(t, agent_id.as_deref(), &owner_aliases))
        .collect();

    if unresolved.is_empty() {
        return None;
    }

    let digest =
        create_unfinished_task_digest(&unresolved.iter().map(|t| (*t).clone()).collect::<Vec<_>>());

    let all_messages: Vec<serde_json::Value> = messages_for_query
        .iter()
        .chain(assistant_messages.iter())
        .cloned()
        .collect();

    let guard_attempts = count_unfinished_task_guard_messages(&all_messages, &digest);
    if guard_attempts >= MAX_UNFINISHED_TASK_GUARD_ATTEMPTS {
        return None;
    }

    let visible = &unresolved[..std::cmp::min(8, unresolved.len())];
    let overflow = unresolved.len().saturating_sub(8);
    let mut task_lines: Vec<String> = visible
        .iter()
        .map(|t| format_unfinished_task_for_guard(t))
        .collect();
    if overflow > 0 {
        task_lines.push(format!("- ... and {} more", overflow));
    }

    let content = format!(
        "<system-reminder>\n\
         {}:{}\n\
         Attempt {}/{}.\n\
         You are about to end the turn while the task list still has unresolved items:\n\
         {}\n\n\
         Do not end silently. Continue by calling tools for the unfinished work, or give the user a clear partial/failure report that explains what remains blocked. If the task list is stale or intentionally abandoned, say that explicitly instead of leaving the user at an idle prompt.\n\
         </system-reminder>",
        UNFINISHED_TASK_GUARD_MARKER,
        digest,
        guard_attempts + 1,
        MAX_UNFINISHED_TASK_GUARD_ATTEMPTS,
        task_lines.join("\n")
    );

    Some(serde_json::json!({
        "type": "user",
        "message": {"content": content},
        "isMeta": true,
    }))
}
