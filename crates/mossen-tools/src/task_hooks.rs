use std::sync::Arc;

use mossen_types::ToolUseContext;
use mossen_utils::hooks_utils::{
    execute_file_changed_hooks, execute_task_completed_hooks, execute_task_created_hooks,
    get_runtime_hooks_context, get_task_completed_hook_message, get_task_created_hook_message,
    AggregatedHookResult, HooksContext, TOOL_HOOK_EXECUTION_TIMEOUT_MS,
};

pub(crate) const HOOK_CONTEXT_ID_EXTRA_KEY: &str = "mossen_hook_context_id";
pub(crate) const PERMISSION_MODE_EXTRA_KEY: &str = "mossen_permission_mode";

#[derive(Debug, Clone, Default)]
pub(crate) struct TaskHookOutcome {
    pub block_message: Option<String>,
    pub additional_contexts: Vec<String>,
}

pub(crate) fn runtime_hook_context(context: &ToolUseContext) -> Option<Arc<HooksContext>> {
    let id = context
        .extra
        .get(HOOK_CONTEXT_ID_EXTRA_KEY)
        .and_then(|value| value.as_str())?;
    get_runtime_hooks_context(id)
}

pub(crate) fn permission_mode(context: &ToolUseContext) -> Option<&str> {
    context
        .extra
        .get(PERMISSION_MODE_EXTRA_KEY)
        .and_then(|value| value.as_str())
}

pub(crate) async fn task_created(
    context: &ToolUseContext,
    task_id: &str,
    subject: &str,
    description: Option<&str>,
) -> TaskHookOutcome {
    let Some(ctx) = runtime_hook_context(context) else {
        return TaskHookOutcome::default();
    };
    task_created_with_context(
        Some(ctx.as_ref()),
        permission_mode(context),
        task_id,
        subject,
        description,
    )
    .await
}

pub(crate) async fn task_completed(
    context: &ToolUseContext,
    task_id: &str,
    subject: &str,
    description: Option<&str>,
) -> TaskHookOutcome {
    let Some(ctx) = runtime_hook_context(context) else {
        return TaskHookOutcome::default();
    };
    task_completed_with_context(
        Some(ctx.as_ref()),
        permission_mode(context),
        task_id,
        subject,
        description,
    )
    .await
}

pub(crate) async fn file_changed(context: &ToolUseContext, file_path: &str, event: &str) {
    let Some(ctx) = runtime_hook_context(context) else {
        return;
    };
    let (_results, env_exports, watch_paths) =
        execute_file_changed_hooks(&ctx, file_path, event, TOOL_HOOK_EXECUTION_TIMEOUT_MS).await;
    if !env_exports.is_empty() || !watch_paths.is_empty() {
        tracing::debug!(
            env_export_count = env_exports.len(),
            watch_path_count = watch_paths.len(),
            "FileChanged hooks produced environment/watch updates"
        );
    }
}

pub(crate) async fn task_created_with_context(
    ctx: Option<&HooksContext>,
    permission_mode: Option<&str>,
    task_id: &str,
    subject: &str,
    description: Option<&str>,
) -> TaskHookOutcome {
    let Some(ctx) = ctx else {
        return TaskHookOutcome::default();
    };
    let results = execute_task_created_hooks(
        ctx,
        task_id,
        subject,
        description,
        None,
        None,
        permission_mode,
        None,
        TOOL_HOOK_EXECUTION_TIMEOUT_MS,
    )
    .await;
    lifecycle_outcome(results, true)
}

pub(crate) async fn task_completed_with_context(
    ctx: Option<&HooksContext>,
    permission_mode: Option<&str>,
    task_id: &str,
    subject: &str,
    description: Option<&str>,
) -> TaskHookOutcome {
    let Some(ctx) = ctx else {
        return TaskHookOutcome::default();
    };
    let results = execute_task_completed_hooks(
        ctx,
        task_id,
        subject,
        description,
        None,
        None,
        permission_mode,
        None,
        TOOL_HOOK_EXECUTION_TIMEOUT_MS,
    )
    .await;
    lifecycle_outcome(results, false)
}

pub(crate) fn append_additional_contexts(output: &mut String, label: &str, contexts: &[String]) {
    if contexts.is_empty() {
        return;
    }
    if !output.is_empty() {
        output.push_str("\n\n");
    }
    output.push_str(label);
    output.push_str(":\n");
    output.push_str(&contexts.join("\n"));
}

fn lifecycle_outcome(results: Vec<AggregatedHookResult>, created: bool) -> TaskHookOutcome {
    let mut outcome = TaskHookOutcome {
        block_message: None,
        additional_contexts: results
            .iter()
            .flat_map(|result| result.additional_contexts.clone().unwrap_or_default())
            .filter(|context| !context.trim().is_empty())
            .collect(),
    };

    for result in results {
        if let Some(blocking_error) = result.blocking_error {
            outcome.block_message = Some(if created {
                get_task_created_hook_message(&blocking_error)
            } else {
                get_task_completed_hook_message(&blocking_error)
            });
        }
    }

    outcome
}
