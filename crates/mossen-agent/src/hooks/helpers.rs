//! # helpers — Hook 辅助函数
//!
//! 对应 TS `utils/hooks/hookHelpers.ts`。
//! 提供 Hook 系统共用的辅助函数。

use mossen_types::hooks::{AggregatedHookResult, HookBlockingError, HookOutcome, HookResult};

/// 聚合多个 Hook 结果。
///
/// 将多个 HookResult 合并为一个 AggregatedHookResult。
pub fn aggregate_hook_results(results: &[HookResult]) -> AggregatedHookResult {
    let mut blocking_errors = Vec::new();
    let mut additional_contexts = Vec::new();
    let mut prevent_continuation = false;
    let mut stop_reason = None;
    let mut permission_behavior = None;
    let mut hook_permission_decision_reason = None;
    let mut initial_user_message = None;
    let mut updated_input = None;
    let mut updated_mcp_tool_output = None;
    let mut permission_request_result = None;
    let mut retry = None;

    for result in results {
        // 收集阻塞错误
        if let Some(ref error) = result.blocking_error {
            blocking_errors.push(error.clone());
        }

        // 收集附加上下文
        if let Some(ref ctx) = result.additional_context {
            additional_contexts.push(ctx.clone());
        }

        // 合并阻止继续标志
        if result.prevent_continuation == Some(true) {
            prevent_continuation = true;
        }

        // 取第一个非空的停止原因
        if stop_reason.is_none() {
            if let Some(ref reason) = result.stop_reason {
                stop_reason = Some(reason.clone());
            }
        }

        // 取第一个权限行为
        if permission_behavior.is_none() {
            permission_behavior = result.permission_behavior;
        }

        // 取第一个权限决策原因
        if hook_permission_decision_reason.is_none() {
            hook_permission_decision_reason = result.hook_permission_decision_reason.clone();
        }

        // 取第一个初始用户消息
        if initial_user_message.is_none() {
            initial_user_message = result.initial_user_message.clone();
        }

        // 取最后的 updated_input
        if result.updated_input.is_some() {
            updated_input = result.updated_input.clone();
        }

        // 取最后的 updated_mcp_tool_output
        if result.updated_mcp_tool_output.is_some() {
            updated_mcp_tool_output = result.updated_mcp_tool_output.clone();
        }

        // 取第一个权限请求结果
        if permission_request_result.is_none() {
            permission_request_result = result.permission_request_result.clone();
        }

        // 取第一个 retry 标志
        if retry.is_none() {
            retry = result.retry;
        }
    }

    AggregatedHookResult {
        message: results.iter().find_map(|r| r.message.clone()),
        blocking_errors: if blocking_errors.is_empty() {
            None
        } else {
            Some(blocking_errors)
        },
        prevent_continuation: if prevent_continuation {
            Some(true)
        } else {
            None
        },
        stop_reason,
        hook_permission_decision_reason,
        permission_behavior,
        additional_contexts: if additional_contexts.is_empty() {
            None
        } else {
            Some(additional_contexts)
        },
        initial_user_message,
        updated_input,
        updated_mcp_tool_output,
        permission_request_result,
        retry,
    }
}

/// Hook 响应 JSON schema 定义。
///
/// 对应 TS `hookResponseSchema()`。
pub fn hook_response_json_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "ok": {
                "type": "boolean",
                "description": "Whether the condition was met"
            },
            "reason": {
                "type": "string",
                "description": "Reason, if the condition was not met"
            }
        },
        "required": ["ok"],
        "additionalProperties": false
    })
}

/// 注册 frontmatter hooks 到会话。
///
/// 对应 TS `registerFrontmatterHooks()`。
pub fn register_frontmatter_hooks(
    session_hooks: &super::session_hooks::SessionHooksManager,
    session_id: &str,
    hooks_settings: &super::settings::HooksSettings,
    _source_name: &str,
    is_agent: bool,
) {
    use mossen_types::hooks::{HookEvent, HOOK_EVENTS};

    for &event in HOOK_EVENTS {
        let matchers = match hooks_settings.get(&event) {
            Some(m) => m,
            None => continue,
        };

        // 对 Agent 来源，将 Stop 转换为 SubagentStop
        let target_event = if is_agent && event == HookEvent::Stop {
            HookEvent::SubagentStop
        } else {
            event
        };

        for matcher_config in matchers {
            let matcher = matcher_config.matcher.as_deref().unwrap_or("");
            for hook in &matcher_config.hooks {
                session_hooks.add_session_hook(
                    session_id,
                    target_event,
                    matcher,
                    hook.clone(),
                    None,
                );
            }
        }
    }
}

/// 注册 skill hooks 到会话。
///
/// 对应 TS `registerSkillHooks()`。
pub fn register_skill_hooks(
    session_hooks: &super::session_hooks::SessionHooksManager,
    session_id: &str,
    hooks_settings: &super::settings::HooksSettings,
    _skill_name: &str,
    skill_root: Option<String>,
) {
    use mossen_types::hooks::HOOK_EVENTS;

    for &event in HOOK_EVENTS {
        let matchers = match hooks_settings.get(&event) {
            Some(m) => m,
            None => continue,
        };

        for matcher_config in matchers {
            let matcher = matcher_config.matcher.as_deref().unwrap_or("");
            for hook in &matcher_config.hooks {
                session_hooks.add_session_hook(
                    session_id,
                    event,
                    matcher,
                    hook.clone(),
                    skill_root.clone(),
                );
            }
        }
    }
}
