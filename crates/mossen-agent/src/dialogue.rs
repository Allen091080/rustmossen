//! # dialogue — Agent 对话循环
//!
//! 对应 TS `query.ts`，实现 Agent Loop 核心状态机：
//! 发送消息 → 接收响应 → 处理工具调用 → 继续循环。
//!
//! 核心函数：
//! - `initiate_dialogue()` — 顶层入口（对应 TS `query()`）
//! - `execute_turn_cycle()` — 循环体（对应 TS `queryLoop()`）

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use futures::stream::StreamExt;
use tracing::{debug, error, info, warn};

use crate::api_client::{self, ApiClientConfig, ApiError};
use crate::context::{
    auto_compact_if_needed, prepare_messages, should_recover_action_promise,
    yield_missing_tool_result_blocks, AutoCompactResult,
};
use crate::cost_tracker::{self, CostState};
use crate::hooks::post_sampling::{PostInferenceContext, PostSamplingHookRegistry};
use crate::retry::{self, RetryConfig, RetryError, SystemApiErrorNotification};
use crate::services::compact::compact::{
    compact_conversation_with_options, prepend_compact_boundary_to_messages,
    CompactConversationOptions,
};
use crate::services::compact::pending_compact_request::{
    dequeue_pending_compact_request, CompactMode, PendingCompactRequest, COMPACT_REQUEST_TIMEOUT,
};
use crate::services::compact::post_compact_cleanup;
use crate::services::root::pending_clear_request::{
    dequeue_pending_clear_request, PendingClearRequest, CLEAR_REQUEST_TIMEOUT,
};
use crate::services::root::runtime_status::{
    record_agent_dialogue_finish, record_agent_dialogue_start, record_tool_call_finish,
    record_tool_call_start, record_tool_permission_decision,
};
use crate::stop_hooks::{StopHookContext, StopHookManager, StopHookResult};
use crate::streaming::{StreamAccumulator, StreamEvent};
use crate::token_estimation::estimate_messages_tokens;
use crate::tool_registry::ToolRegistry;
use crate::types::*;
use mossen_types::{
    ContentBlock, Message, Role, TextBlock, ToolResultBlock, ToolResultContent, ToolUseBlock,
};

// ---------------------------------------------------------------------------
// 常量
// ---------------------------------------------------------------------------

/// 最大 action promise 恢复次数。
const MAX_ACTION_PROMISE_RECOVERY: u32 = 3;
/// 最大 max_output_tokens 恢复次数。
const MAX_OUTPUT_TOKENS_RECOVERY: u32 = 3;
/// 工具结果后模型返回空白 end_turn 时的恢复次数。
const MAX_EMPTY_RESPONSE_RECOVERY: u32 = 2;
/// 升档的 max_output_tokens 值。
const ESCALATED_MAX_OUTPUT_TOKENS: u32 = 64_000;
const PERMISSION_MODE_ENV: &str = "MOSSEN_PERMISSION_MODE";
const PERMISSION_ALLOW_RULES_ENV: &str = "MOSSEN_PERMISSION_ALLOW_RULES";
const PERMISSION_DENY_RULES_ENV: &str = "MOSSEN_PERMISSION_DENY_RULES";

#[derive(Debug, Clone, PartialEq, Eq)]
struct PreflightPermissionDecision {
    decision: PermissionDecision,
    deny_message: Option<&'static str>,
}

impl PreflightPermissionDecision {
    fn allow() -> Self {
        Self {
            decision: PermissionDecision::Allow,
            deny_message: None,
        }
    }

    fn deny(message: &'static str) -> Self {
        Self {
            decision: PermissionDecision::Deny,
            deny_message: Some(message),
        }
    }
}

fn permission_mode_decision(
    mode: PermissionMode,
    tool_name: &str,
    tool_is_read_only: bool,
) -> Option<PreflightPermissionDecision> {
    if tool_is_read_only {
        return Some(PreflightPermissionDecision::allow());
    }

    match mode {
        PermissionMode::Default => None,
        PermissionMode::AcceptEdits => {
            if is_edit_permission_tool(tool_name) {
                Some(PreflightPermissionDecision::allow())
            } else {
                None
            }
        }
        PermissionMode::BypassPermissions | PermissionMode::Auto | PermissionMode::Yolo => {
            Some(PreflightPermissionDecision::allow())
        }
        PermissionMode::Plan => {
            if tool_name == "ExitPlanMode" {
                Some(PreflightPermissionDecision::allow())
            } else {
                Some(PreflightPermissionDecision::deny(
                    "Plan mode allows read-only exploration only. Switch permission mode or approve the plan before running this tool.",
                ))
            }
        }
        PermissionMode::DontAsk => Some(PreflightPermissionDecision::deny(
            "Permission mode is set to dontAsk, so tool calls that require approval are blocked instead of prompting.",
        )),
    }
}

fn effective_permission_mode(default_mode: PermissionMode) -> PermissionMode {
    std::env::var(PERMISSION_MODE_ENV)
        .ok()
        .map(PermissionMode::parse)
        .unwrap_or(default_mode)
}

fn is_edit_permission_tool(tool_name: &str) -> bool {
    matches!(tool_name, "Edit" | "Write" | "NotebookEdit")
}

fn session_permission_rule_decision(
    tool_name: &str,
    input: &serde_json::Value,
) -> Option<PreflightPermissionDecision> {
    let deny_rules = permission_rule_env_lines(PERMISSION_DENY_RULES_ENV);
    if permission_rules_match(&deny_rules, tool_name, input) {
        return Some(PreflightPermissionDecision::deny(
            "Tool call denied by session permission rule.",
        ));
    }

    let allow_rules = permission_rule_env_lines(PERMISSION_ALLOW_RULES_ENV);
    if permission_rules_match(&allow_rules, tool_name, input) {
        return Some(PreflightPermissionDecision::allow());
    }

    None
}

fn permission_rule_env_lines(key: &str) -> Vec<String> {
    std::env::var(key)
        .ok()
        .map(|value| {
            value
                .lines()
                .map(normalize_permission_rule)
                .filter(|line| !line.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn permission_rules_match(rules: &[String], tool_name: &str, input: &serde_json::Value) -> bool {
    if rules.is_empty() {
        return false;
    }

    let candidates = permission_rule_candidates(tool_name, input);
    rules.iter().any(|rule| {
        !rule.is_empty()
            && candidates
                .iter()
                .any(|candidate| permission_rule_matches_candidate(rule, candidate))
    })
}

fn permission_rule_candidates(tool_name: &str, input: &serde_json::Value) -> Vec<String> {
    const INPUT_KEYS: &[&str] = &[
        "command",
        "file_path",
        "path",
        "url",
        "description",
        "prompt",
    ];

    let mut candidates = Vec::new();
    push_permission_rule_candidate(&mut candidates, tool_name.to_string());

    if let Some(object) = input.as_object() {
        for key in INPUT_KEYS {
            if let Some(value) = object.get(*key).and_then(serde_json::Value::as_str) {
                push_permission_rule_candidate(&mut candidates, value.to_string());
                push_permission_rule_candidate(&mut candidates, format!("{tool_name} {value}"));
                push_permission_rule_candidate(&mut candidates, format!("{tool_name}:{value}"));
            }
        }
    }

    candidates
}

fn push_permission_rule_candidate(candidates: &mut Vec<String>, candidate: String) {
    let candidate = normalize_permission_rule(&candidate);
    if !candidate.is_empty() && !candidates.iter().any(|existing| existing == &candidate) {
        candidates.push(candidate);
    }
}

fn normalize_permission_rule(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn permission_rule_matches_candidate(rule: &str, candidate: &str) -> bool {
    if rule == candidate {
        return true;
    }
    if rule.contains('*') && wildcard_permission_rule_matches(rule, candidate) {
        return true;
    }
    if let Some(tail) = candidate.strip_prefix(rule) {
        if tail.starts_with(' ') || tail.starts_with(':') {
            return true;
        }
    }
    if permission_rule_path_prefix_matches(rule, candidate) {
        return true;
    }
    false
}

fn wildcard_permission_rule_matches(pattern: &str, candidate: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    let parts: Vec<&str> = pattern.split('*').filter(|part| !part.is_empty()).collect();
    if parts.is_empty() {
        return true;
    }

    let mut position = 0;
    let mut index = 0;
    if !pattern.starts_with('*') {
        let first = parts[0];
        if !candidate.starts_with(first) {
            return false;
        }
        position = first.len();
        index = 1;
    }

    for part in parts.iter().skip(index) {
        let Some(found_at) = candidate[position..].find(part) else {
            return false;
        };
        position += found_at + part.len();
    }

    if !pattern.ends_with('*') {
        if let Some(last) = parts.last() {
            return candidate.ends_with(last);
        }
    }

    true
}

fn permission_rule_path_prefix_matches(rule: &str, candidate: &str) -> bool {
    if !rule.contains('/') && !rule.contains('\\') {
        return false;
    }
    let prefix = rule.trim_end_matches(|ch| ch == '/' || ch == '\\');
    if prefix.is_empty() {
        return false;
    }
    candidate == prefix
        || candidate
            .strip_prefix(prefix)
            .map(|tail| tail.starts_with('/') || tail.starts_with('\\'))
            .unwrap_or(false)
}

fn permission_decision_label(decision: &PermissionDecision) -> &'static str {
    match decision {
        PermissionDecision::Allow => "allow",
        PermissionDecision::AllowAlways => "allowAlways",
        PermissionDecision::AllowWithUpdatedInput { .. } => "allowWithUpdatedInput",
        PermissionDecision::Deny => "deny",
    }
}

// ---------------------------------------------------------------------------
// 查询错误
// ---------------------------------------------------------------------------

/// 查询错误。
#[derive(Debug, thiserror::Error)]
pub enum DialogueError {
    #[error("API error: {0}")]
    Api(#[from] ApiError),

    #[error("Retry exhausted: {0}")]
    RetryExhausted(String),

    #[error("User cancelled")]
    Cancelled,

    #[error("Internal error: {0}")]
    Internal(String),
}

// ---------------------------------------------------------------------------
// QueryYield — Stream 产出类型
// ---------------------------------------------------------------------------

/// 对话流的产出项。
pub enum QueryYield {
    /// SDK 消息事件。
    Message(SdkMessage),
    /// 终端结果。
    Terminal(TerminalReason),
}

// ---------------------------------------------------------------------------
// initiate_dialogue — 顶层入口
// ---------------------------------------------------------------------------

/// 启动对话——包装 execute_turn_cycle 并处理命令生命周期通知。
///
/// 对应 TS `query()` 函数。
pub async fn initiate_dialogue(
    spec: DialogueSpec,
    api_config: ApiClientConfig,
    tool_registry: Arc<ToolRegistry>,
    stop_hook_manager: Arc<StopHookManager>,
    post_sampling_manager: Arc<PostSamplingHookRegistry>,
    tx: tokio::sync::mpsc::Sender<SdkMessage>,
) -> Result<TerminalReason, DialogueError> {
    let session_id = uuid::Uuid::new_v4().to_string();

    info!(
        session_id = %session_id,
        model = %spec.model,
        origin = ?spec.origin_tag,
        "Initiating dialogue"
    );
    record_agent_dialogue_start(&session_id, &spec.model);

    // 发送系统初始化消息
    let _ = tx
        .send(SdkMessage::SystemInit {
            session_id: session_id.clone(),
            model: spec.model.clone(),
            tools: spec.tools.iter().map(|t| t.name.clone()).collect(),
            task_id: None,
        })
        .await;

    // 执行核心循环 — `execute_turn_cycle` returns the terminal reason along
    // with the accumulated `CostState` and total wall time so we can fill the
    // `Result` SDK message with live numbers instead of `None` placeholders.
    let session_start = Instant::now();
    let result = execute_turn_cycle(
        &spec,
        &api_config,
        &tool_registry,
        &stop_hook_manager,
        &post_sampling_manager,
        &tx,
        &session_id,
    )
    .await;

    // 发送结果消息 — live cost + duration. `usage` is still `None` here
    // because the per-call usage is already streamed via `MessageDelta`
    // events; downstream consumers re-aggregate from those.
    if let Ok((ref terminal, ref cost_state)) = result {
        let _ = tx
            .send(SdkMessage::Result {
                terminal: format!("{:?}", terminal),
                cost_usd: Some(cost_state.total_cost_usd),
                duration_ms: Some(session_start.elapsed().as_millis() as u64),
                usage: None,
                task_id: None,
            })
            .await;
    }
    let terminal_reason = result
        .as_ref()
        .ok()
        .map(|(terminal, _)| format!("{terminal:?}"));
    let error = result.as_ref().err().map(ToString::to_string);
    record_agent_dialogue_finish(terminal_reason.as_deref(), error.as_deref());

    result.map(|(terminal, _)| terminal)
}

// ---------------------------------------------------------------------------
// execute_turn_cycle — Agent 循环体
// ---------------------------------------------------------------------------

/// Agent Loop 核心循环。
///
/// 对应 TS `queryLoop()`（query.ts:273-1747）。
async fn execute_turn_cycle(
    spec: &DialogueSpec,
    api_config: &ApiClientConfig,
    tool_registry: &Arc<ToolRegistry>,
    stop_hook_manager: &Arc<StopHookManager>,
    post_sampling_manager: &Arc<PostSamplingHookRegistry>,
    tx: &tokio::sync::mpsc::Sender<SdkMessage>,
    session_id: &str,
) -> Result<(TerminalReason, CostState), DialogueError> {
    let mut state = TurnLedger::new(spec);
    let mut cost_state = CostState::default();
    let mut action_promise_recovery_count: u32 = 0;
    let mut empty_response_recovery_count: u32 = 0;

    let cancel = &spec.cancel;

    loop {
        // 0. 取消检查
        if cancel.is_cancelled() {
            return Ok((TerminalReason::AbortedStreaming, cost_state));
        }

        let _ = execute_pending_clear_request(&mut state, tx).await;
        let _ = execute_pending_compact_request(&mut state, tx).await;

        // 1. 消息预处理管线（含 microcompact）
        let env = snapshot_turn_env(spec, &state);
        let prepared = crate::context::prepare_messages_async(
            &state.messages,
            &env,
            None, // no per-query model override yet
        )
        .await;

        debug!(
            turn = state.turn_count,
            estimated_tokens = prepared.estimated_tokens,
            message_count = prepared.messages.len(),
            micro_compacted = prepared.micro_compacted,
            "Turn cycle: messages prepared"
        );

        // 2. 自动压缩
        let compact_result = auto_compact_if_needed(
            &prepared.messages,
            prepared.estimated_tokens,
            env.context_window,
            env.max_output_tokens,
            &mut state.auto_compact_tracking,
        )
        .await;

        let mut messages_for_query = match compact_result {
            AutoCompactResult::Compacted {
                before_tokens,
                after_tokens,
                messages: compacted_messages,
                summary: _,
            } => {
                let _ = tx
                    .send(SdkMessage::CompactBoundary {
                        before_token_count: before_tokens,
                        after_token_count: after_tokens,
                        task_id: None,
                    })
                    .await;
                state.messages = compacted_messages.clone();
                compacted_messages
            }
            AutoCompactResult::Failed { error } => {
                warn!(%error, "Auto-compact failed; continuing with uncompacted context");
                prepared.messages
            }
            AutoCompactResult::NotNeeded | AutoCompactResult::Skipped => prepared.messages,
        };

        // 3. 补齐孤立 tool_use 的 tool_result
        let missing_results = yield_missing_tool_result_blocks(&messages_for_query);
        for msg in missing_results {
            messages_for_query.push(msg);
        }

        // 4. 构建 API 请求参数
        let max_output = state
            .max_output_tokens_override
            .unwrap_or(env.max_output_tokens);

        let stream_params = api_client::build_stream_request(
            &env.model,
            max_output,
            &messages_for_query,
            &spec.system_prompt,
            &spec.tools,
            env.thinking_config.as_ref(),
            None, // tool_choice
            &spec.extra_body,
            &ApiMetadata { user_id: None },
        );

        // 5. 流式 API 调用 — 通过 retry::with_retry 拿到全局指数退避、
        //    Retry-After header 支持、429/529 区分跟踪、上下文溢出时的
        //    max_tokens 自动调整、以及取消 token 中断的完整重试机制。
        //
        //    UI 上每次重试发一条 `SdkMessage::ApiRetry` 让前端展示「retrying
        //    in N ms (attempt M/K)」横幅。
        let call_start = Instant::now();
        let retry_config = RetryConfig {
            max_retries: crate::api::with_retry::DEFAULT_MAX_RETRIES,
            model: env.model.clone(),
            fallback_model: None,
            thinking_config: env.thinking_config.clone(),
            fast_mode: spec.fast_mode,
        };
        let api_cfg = api_config.clone();
        let params = stream_params.clone();
        let retry_cancel = cancel.clone();
        let retry_tx = tx.clone();

        let stream_result = retry::with_retry(
            |_attempt, _ctx| {
                let cfg = api_cfg.clone();
                let params = params.clone();
                let cancel = retry_cancel.clone();
                async move { api_client::call_streaming(&cfg, &params, cancel).await }
            },
            &retry_config,
            &cancel,
            move |notif: SystemApiErrorNotification| {
                let tx = retry_tx.clone();
                let attempt = notif.attempt;
                let max_retries = notif.max_retries;
                let retry_in_ms = notif.retry_in_ms;
                let err_msg = notif.error.to_string();
                tokio::spawn(async move {
                    let _ = tx
                        .send(SdkMessage::ApiRetry {
                            error: err_msg,
                            attempt,
                            max_retries,
                            retry_in_ms,
                            task_id: None,
                        })
                        .await;
                });
            },
        )
        .await;

        let event_stream = match stream_result {
            Ok(s) => s,
            Err(RetryError::UserAbort) => {
                return Ok((TerminalReason::AbortedStreaming, cost_state));
            }
            Err(RetryError::FallbackTriggered { original, fallback }) => {
                warn!(
                    %original, %fallback,
                    "Fallback requested but no handler wired; will retry with the original model"
                );
                return Ok((TerminalReason::Retry, cost_state));
            }
            Err(RetryError::CannotRetry(e)) => {
                return Ok((TerminalReason::ModelError { error: e }, cost_state));
            }
        };

        // 6. 消费流式响应
        let mut accumulator = StreamAccumulator::new();
        let mut event_stream = event_stream;

        while let Some(event_result) = event_stream.next().await {
            if cancel.is_cancelled() {
                return Ok((TerminalReason::AbortedStreaming, cost_state));
            }

            match event_result {
                Ok(event) => {
                    // 转发流式事件到 UI
                    if let Some(sdk_event) = stream_event_to_sdk(&event) {
                        let _ = tx
                            .send(SdkMessage::StreamEvent {
                                event: sdk_event,
                                task_id: None,
                            })
                            .await;
                    }
                    accumulator.process_event(&event);
                }
                Err(ApiError::Cancelled) => {
                    return Ok((TerminalReason::AbortedStreaming, cost_state));
                }
                Err(ApiError::StreamTimeout) => {
                    warn!("Stream timeout during turn {}", state.turn_count);
                    return Ok((
                        TerminalReason::ModelError {
                            error: anyhow::anyhow!("Stream timeout"),
                        },
                        cost_state,
                    ));
                }
                Err(e) => {
                    error!(error = %e, "Stream error");
                    return Ok((
                        TerminalReason::ModelError {
                            error: anyhow::anyhow!("{}", e),
                        },
                        cost_state,
                    ));
                }
            }
        }

        // 8. Post-sampling hook：API 采样完成后通知注册的 watcher
        let psh_ctx = PostInferenceContext {
            messages_json: accumulator
                .content_blocks
                .iter()
                .map(|b| format!("{:?}", b))
                .collect::<Vec<_>>()
                .join("\n"),
            system_prompt: spec
                .system_prompt
                .iter()
                .map(|sb| sb.text.clone())
                .collect::<Vec<_>>()
                .join("\n"),
            user_context: std::collections::HashMap::new(),
            system_context: std::collections::HashMap::new(),
            query_source: Some(format!("{:?}", spec.origin_tag)),
        };
        post_sampling_manager.fire_post_inference_watchers(&psh_ctx);

        let call_duration = call_start.elapsed();

        // 记录用量和成本
        if let Some(ref usage) = accumulator.usage {
            let cost = cost_tracker::calculate_usd_cost(&env.model, usage);
            cost_tracker::add_to_total_session_cost(cost, usage, &env.model, &mut cost_state);
            cost_tracker::record_api_duration(
                &mut cost_state,
                call_duration.as_millis() as u64,
                false,
            );
        }

        // 构建助手消息
        let assistant_message = build_assistant_message(&accumulator);
        let visible_text = accumulator.visible_text();
        let user_visible_text = strip_synthetic_thinking_sections(&visible_text);
        let can_recover_empty_response = matches!(
            state.transition,
            Some(ContinueReason::NextTurn | ContinueReason::EmptyResponseRecovery { .. })
        ) || accumulator.stop_reason.as_deref()
            == Some("tool_use");
        let will_recover_empty_response = !accumulator.has_tool_use()
            && user_visible_text.trim().is_empty()
            && can_recover_empty_response
            && empty_response_recovery_count < MAX_EMPTY_RESPONSE_RECOVERY;

        // 发送助手消息到 UI。纯空响应没有用户价值，尤其是工具结果
        // 后的空白 end_turn；这类情况由下面的恢复分支继续推进。
        if !assistant_message.content.is_empty() && !will_recover_empty_response {
            let _ = tx
                .send(SdkMessage::Assistant {
                    message: mossen_types::AssistantMessage {
                        role: Role::Assistant,
                        content: assistant_message.content.clone(),
                        uuid: assistant_message.uuid.clone(),
                        model: accumulator.model.clone(),
                        stop_reason: accumulator.stop_reason.clone(),
                        extra: HashMap::new(),
                    },
                    usage: accumulator.usage.clone(),
                    task_id: None,
                })
                .await;
        }

        // 7. 恢复逻辑（8 个 continue 站点）

        // 站点 3: max_output_tokens 升档
        if accumulator.stop_reason.as_deref() == Some("max_tokens")
            && state.max_output_tokens_override.is_none()
        {
            debug!("Max output tokens escalation triggered");
            state.max_output_tokens_override = Some(ESCALATED_MAX_OUTPUT_TOKENS);
            state.messages.push(assistant_message);
            state.advance_turn(ContinueReason::MaxOutputTokensEscalate);
            continue;
        }

        // 站点 4: max_output_tokens 多轮恢复
        if accumulator.stop_reason.as_deref() == Some("max_tokens")
            && state.max_output_tokens_recovery_count < MAX_OUTPUT_TOKENS_RECOVERY
        {
            state.max_output_tokens_recovery_count += 1;
            debug!(
                attempt = state.max_output_tokens_recovery_count,
                "Max output tokens recovery"
            );
            state.messages.push(assistant_message);
            // 添加恢复提示
            state.messages.push(Message {
                role: Role::User,
                content: vec![ContentBlock::Text(TextBlock {
                    text: "Please continue from where you left off.".to_string(),
                })],
                uuid: Some(uuid::Uuid::new_v4().to_string()),
                is_meta: Some(true),
                origin: None,
                timestamp: Some(chrono::Utc::now().to_rfc3339()),
                extra: HashMap::new(),
            });
            state.advance_turn(ContinueReason::MaxOutputTokensRecovery {
                attempt: state.max_output_tokens_recovery_count,
            });
            continue;
        }

        // 站点 5: action promise 恢复
        if will_recover_empty_response {
            empty_response_recovery_count += 1;
            debug!(
                attempt = empty_response_recovery_count,
                "Empty response after tool results; asking model to continue"
            );
            let recovery_prompt = if accumulator.stop_reason.as_deref() == Some("tool_use") {
                "Your previous response indicated that you wanted to use a tool, but it did not include a complete structured tool call. Please continue by either issuing a valid tool call or answering the user's request visibly. Do not end the turn without visible progress."
            } else {
                "The previous tool calls have completed and produced results. Please continue and answer the user's request using those tool results. Do not end the turn without a visible response."
            };
            state.messages.push(Message {
                role: Role::User,
                content: vec![ContentBlock::Text(TextBlock {
                    text: recovery_prompt.to_string(),
                })],
                uuid: Some(uuid::Uuid::new_v4().to_string()),
                is_meta: Some(true),
                origin: None,
                timestamp: Some(chrono::Utc::now().to_rfc3339()),
                extra: HashMap::new(),
            });
            state.advance_turn(ContinueReason::EmptyResponseRecovery {
                attempt: empty_response_recovery_count,
            });
            continue;
        }
        if !accumulator.has_tool_use()
            && should_recover_action_promise(&user_visible_text)
            && action_promise_recovery_count < MAX_ACTION_PROMISE_RECOVERY
        {
            action_promise_recovery_count += 1;
            debug!(
                attempt = action_promise_recovery_count,
                "Action promise recovery triggered"
            );
            state.messages.push(assistant_message);
            state.messages.push(Message {
                role: Role::User,
                content: vec![ContentBlock::Text(TextBlock {
                    text: "You mentioned you would take action but didn't use any tools. Please proceed with the action.".to_string(),
                })],
                uuid: Some(uuid::Uuid::new_v4().to_string()),
                is_meta: Some(true),
                origin: None,
                timestamp: Some(chrono::Utc::now().to_rfc3339()),
                extra: HashMap::new(),
            });
            state.advance_turn(ContinueReason::ActionPromiseRecovery);
            continue;
        }

        // 8. 如果没有工具调用，检查 stop hooks 并结束
        if !accumulator.has_tool_use() {
            // Stop hooks 评估
            if !spec.skip_stop_hooks {
                let hook_ctx = StopHookContext {
                    session_id: session_id.to_string(),
                    origin_tag: spec.origin_tag.clone(),
                    auto_mode: spec.auto_mode,
                    turn_count: state.turn_count,
                };
                let hook_result = stop_hook_manager
                    .evaluate_halt_signals(&hook_ctx, cancel)
                    .await;

                match hook_result {
                    StopHookResult::Allow => {}
                    StopHookResult::Block { reason } => {
                        // 站点 6: stop hook 阻塞
                        debug!(reason = %reason, "Stop hook blocking, continuing");
                        state.messages.push(assistant_message);
                        state.advance_turn(ContinueReason::StopHookBlocking);
                        continue;
                    }
                    StopHookResult::Prevent { reason: _ } => {
                        state.messages.push(assistant_message);
                        return Ok((TerminalReason::StopHookPrevented, cost_state));
                    }
                }
            }

            state.messages.push(assistant_message);
            return Ok((TerminalReason::Completed, cost_state));
        }

        // 9. 工具执行
        let tool_uses = accumulator.tool_uses();
        state.messages.push(assistant_message);

        let mut tool_results: Vec<ContentBlock> = Vec::new();

        for (tool_id, tool_name, input_json) in &tool_uses {
            if cancel.is_cancelled() {
                return Ok((TerminalReason::AbortedTools, cost_state));
            }

            let mut input: serde_json::Value = serde_json::from_str(input_json)
                .unwrap_or_else(|_| serde_json::Value::Object(Default::default()));
            record_tool_call_start(tool_name);

            // ── Permission gate ───────────────────────────────────────
            // Before invoking the tool we consult `spec.permission_gate`.
            // Default is `AllowAllGate` (open). For supervised mode the TUI
            // installs an `InteractiveGate` whose `check()` posts a
            // `PermissionRequest` to the UI and waits on a oneshot reply.
            // A `Deny` short-circuits to a tool_result error block so the
            // model can react without us silently executing.
            let tool_is_read_only = if tool_name.starts_with("mcp__") {
                false
            } else {
                tool_registry
                    .get(tool_name)
                    .map(|tool| tool.is_read_only())
                    .unwrap_or(false)
            };
            let needs_permission = if tool_name.starts_with("mcp__") {
                true
            } else {
                tool_registry
                    .get(tool_name)
                    .map(|tool| tool.needs_permission())
                    .unwrap_or(true)
            };
            let rule_decision = if needs_permission {
                session_permission_rule_decision(tool_name, &input)
            } else {
                None
            };
            let mode_decision = if needs_permission && rule_decision.is_none() {
                permission_mode_decision(
                    effective_permission_mode(spec.permission_mode),
                    tool_name,
                    tool_is_read_only,
                )
            } else {
                None
            };
            let decision = if let Some(rule_decision) = rule_decision.as_ref() {
                rule_decision.decision.clone()
            } else if let Some(mode_decision) = mode_decision.as_ref() {
                mode_decision.decision.clone()
            } else if needs_permission {
                spec.permission_gate.check(tool_name, tool_id, &input).await
            } else {
                PermissionDecision::Allow
            };
            let permission_source = if rule_decision.is_some() {
                "session_permission_rules"
            } else if mode_decision.is_some() {
                "permission_mode"
            } else if needs_permission {
                "permission_gate"
            } else {
                "not_required"
            };
            record_tool_permission_decision(
                tool_name,
                permission_source,
                permission_decision_label(&decision),
            );
            if !decision.is_allowed() {
                debug!(tool = %tool_name, id = %tool_id, "Tool execution denied by permission gate");
                record_tool_call_finish(tool_name, "denied");
                tool_results.push(ContentBlock::ToolResult(ToolResultBlock {
                    tool_use_id: tool_id.clone(),
                    content: ToolResultContent::Text(
                        rule_decision
                            .as_ref()
                            .and_then(|decision| decision.deny_message)
                            .or_else(|| {
                                mode_decision
                                    .as_ref()
                                    .and_then(|decision| decision.deny_message)
                            })
                            .unwrap_or("User denied permission for this tool call.")
                            .to_string(),
                    ),
                    is_error: Some(true),
                }));
                continue;
            }
            if let Some(updated_input) = decision.updated_input().cloned() {
                input = updated_input;
            }

            debug!(tool = %tool_name, id = %tool_id, "Executing tool");
            let tool_start = Instant::now();

            // ── MCP fast-path ─────────────────────────────────────────
            // The model emits MCP tool calls under their fully-qualified
            // names (`mcp__<server>__<tool>`); routing them through the
            // tool_registry would require pre-registering one entry per
            // server-tool pair. Instead we recognise the prefix here and
            // dispatch straight to the live MCP client via the
            // process-global `McpServerManager`. Falls through to the
            // normal registry path when no MCP manager is installed or
            // the server isn't connected, so the model gets a structured
            // error instead of silent success.
            let exec_result = if tool_name.starts_with("mcp__") {
                tokio::select! {
                    result = execute_mcp_tool(tool_name, input.clone()) => result,
                    _ = cancel.cancelled() => {
                        record_tool_call_finish(tool_name, "cancelled");
                        return Ok((TerminalReason::AbortedTools, cost_state));
                    }
                }
            } else {
                match tool_registry
                    .execute_with_cancel(tool_name, input, &state.tool_use_context, cancel)
                    .await
                {
                    Ok(result) => Ok(result),
                    Err(_e) if cancel.is_cancelled() => {
                        record_tool_call_finish(tool_name, "cancelled");
                        return Ok((TerminalReason::AbortedTools, cost_state));
                    }
                    Err(e) => Err(e),
                }
            };

            let tool_duration = tool_start.elapsed().as_millis() as u64;
            cost_tracker::record_tool_duration(&mut cost_state, tool_duration);

            let result_block = match exec_result {
                Ok(result) => {
                    record_tool_call_finish(
                        tool_name,
                        if result.is_error {
                            "error"
                        } else {
                            "completed"
                        },
                    );
                    // 记录代码变更
                    if let Some(lines_added) = result.metadata.get("lines_added") {
                        if let Some(added) = lines_added.as_u64() {
                            let removed = result
                                .metadata
                                .get("lines_removed")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0);
                            cost_tracker::record_line_changes(&mut cost_state, added, removed);
                        }
                    }

                    ContentBlock::ToolResult(ToolResultBlock {
                        tool_use_id: tool_id.clone(),
                        content: ToolResultContent::Text(result.output),
                        is_error: if result.is_error { Some(true) } else { None },
                    })
                }
                Err(e) => {
                    record_tool_call_finish(tool_name, "error");
                    ContentBlock::ToolResult(ToolResultBlock {
                        tool_use_id: tool_id.clone(),
                        content: ToolResultContent::Text(format!("Error: {}", e)),
                        is_error: Some(true),
                    })
                }
            };

            // Surface a short summary to the UI so the user sees what the
            // tool actually returned. Without this the TUI shows the
            // assistant's tool_use block but no result line — the next
            // assistant turn appears out of nowhere and the run looks
            // hung even though the loop is working. We truncate long
            // outputs to ~600 chars so the message column doesn't
            // explode; the model still sees the full result in the
            // next turn's messages array.
            if let ContentBlock::ToolResult(ref tr) = result_block {
                let preview_text = match &tr.content {
                    ToolResultContent::Text(s) => s.clone(),
                    ToolResultContent::Blocks(blocks) => blocks
                        .iter()
                        .filter_map(|b| {
                            if let ContentBlock::Text(t) = b {
                                Some(t.text.as_str())
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("\n"),
                };
                const MAX_PREVIEW: usize = 600;
                // Char-boundary-safe truncation (preview_text may have CJK
                // multi-byte chars). The hint line is emitted by the TUI
                // widget from `full_content.len() - summary.len()` so the
                // preview here stays clean — no embedded `[truncated, …]`
                // sentinel that the renderer would otherwise echo verbatim.
                let (preview, full) = if preview_text.chars().count() > MAX_PREVIEW {
                    let head: String = preview_text.chars().take(MAX_PREVIEW).collect();
                    (format!("{}…", head), Some(preview_text.clone()))
                } else {
                    (preview_text, None)
                };
                let _ = tx
                    .send(SdkMessage::ToolUseSummary {
                        tool_name: tool_name.clone(),
                        tool_use_id: Some(tr.tool_use_id.clone()),
                        summary: preview,
                        full_content: full,
                        task_id: None,
                    })
                    .await;
            }

            tool_results.push(result_block);
        }

        // 添加工具结果消息
        if !tool_results.is_empty() {
            state.messages.push(Message {
                role: Role::User,
                content: tool_results,
                uuid: Some(uuid::Uuid::new_v4().to_string()),
                is_meta: Some(true),
                origin: None,
                timestamp: Some(chrono::Utc::now().to_rfc3339()),
                extra: HashMap::new(),
            });
        }

        // 10. 最大轮次检查
        state.turn_count += 1;
        if let Some(max_turns) = spec.max_turns {
            if state.turn_count >= max_turns {
                return Ok((
                    TerminalReason::MaxTurns {
                        turn_count: state.turn_count,
                    },
                    cost_state,
                ));
            }
        }

        // 站点 8: 正常下一轮
        state.transition = Some(ContinueReason::NextTurn);
        debug!(turn = state.turn_count, "Advancing to next turn");
    }
}

// ---------------------------------------------------------------------------
// 辅助函数
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
enum PendingCompactExecutionOutcome {
    NoRequest,
    TimedOut {
        request_id: String,
    },
    DryRun {
        request_id: String,
        pre_compact_token_count: u64,
        message_count: usize,
    },
    Completed {
        request_id: String,
        pre_compact_token_count: u64,
        post_compact_token_count: u64,
        compacted_message_count: usize,
        message_count_before: usize,
        message_count_after: usize,
    },
    Skipped {
        request_id: String,
        reason: String,
    },
    Failed {
        request_id: String,
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PendingClearExecutionOutcome {
    NoRequest,
    TimedOut {
        request_id: String,
    },
    DryRun {
        request_id: String,
        message_count: usize,
    },
    Completed {
        request_id: String,
        message_count_before: usize,
        message_count_after: usize,
    },
}

async fn emit_compact_request_status(
    tx: &tokio::sync::mpsc::Sender<SdkMessage>,
    request_id: String,
    status: CompactRequestStatus,
    dry_run: bool,
    before_token_count: Option<u64>,
    after_token_count: Option<u64>,
    message_count_before: Option<usize>,
    message_count_after: Option<usize>,
    compacted_message_count: Option<usize>,
    reason: Option<String>,
) {
    let _ = tx
        .send(SdkMessage::CompactRequestStatus {
            request_id,
            status,
            dry_run,
            before_token_count,
            after_token_count,
            message_count_before: message_count_before.map(|count| count as u64),
            message_count_after: message_count_after.map(|count| count as u64),
            compacted_message_count: compacted_message_count.map(|count| count as u64),
            reason: reason.and_then(compact_request_status_reason),
            task_id: None,
        })
        .await;
}

async fn emit_clear_request_status(
    tx: &tokio::sync::mpsc::Sender<SdkMessage>,
    request_id: String,
    status: ClearRequestStatus,
    dry_run: bool,
    message_count_before: Option<usize>,
    message_count_after: Option<usize>,
    reason: Option<String>,
) {
    let _ = tx
        .send(SdkMessage::ClearRequestStatus {
            request_id,
            status,
            dry_run,
            message_count_before: message_count_before.map(|count| count as u64),
            message_count_after: message_count_after.map(|count| count as u64),
            reason: reason.and_then(control_request_status_reason),
            task_id: None,
        })
        .await;
}

fn compact_request_status_reason(reason: String) -> Option<String> {
    control_request_status_reason(reason)
}

fn control_request_status_reason(reason: String) -> Option<String> {
    let line = reason
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .trim();
    if line.is_empty() {
        None
    } else {
        Some(line.chars().take(240).collect())
    }
}

async fn execute_pending_clear_request(
    state: &mut TurnLedger,
    tx: &tokio::sync::mpsc::Sender<SdkMessage>,
) -> PendingClearExecutionOutcome {
    let Some(request) = dequeue_pending_clear_request() else {
        return PendingClearExecutionOutcome::NoRequest;
    };

    execute_clear_request_at_safe_point(state, tx, request).await
}

async fn execute_clear_request_at_safe_point(
    state: &mut TurnLedger,
    tx: &tokio::sync::mpsc::Sender<SdkMessage>,
    request: PendingClearRequest,
) -> PendingClearExecutionOutcome {
    if request.enqueued_at.elapsed() > CLEAR_REQUEST_TIMEOUT {
        warn!(
            request_id = %request.request_id,
            "Dropping timed-out clear control request"
        );
        emit_clear_request_status(
            tx,
            request.request_id.clone(),
            ClearRequestStatus::TimedOut,
            request.dry_run,
            None,
            None,
            Some("clear request timed out before safe-point execution".to_string()),
        )
        .await;
        return PendingClearExecutionOutcome::TimedOut {
            request_id: request.request_id,
        };
    }

    let message_count_before = state.messages.len();
    if request.dry_run {
        info!(
            request_id = %request.request_id,
            message_count = message_count_before,
            "Dry-run clear control request observed"
        );
        emit_clear_request_status(
            tx,
            request.request_id.clone(),
            ClearRequestStatus::DryRun,
            true,
            Some(message_count_before),
            Some(message_count_before),
            Some("dry run only".to_string()),
        )
        .await;
        return PendingClearExecutionOutcome::DryRun {
            request_id: request.request_id,
            message_count: message_count_before,
        };
    }

    state.messages.clear();
    post_compact_cleanup::run_post_compact_cleanup(Some("sdk"));
    let message_count_after = state.messages.len();

    let _ = tx
        .send(SdkMessage::ConversationCleared {
            message_count_before: message_count_before as u64,
            message_count_after: message_count_after as u64,
            task_id: None,
        })
        .await;

    emit_clear_request_status(
        tx,
        request.request_id.clone(),
        ClearRequestStatus::Completed,
        false,
        Some(message_count_before),
        Some(message_count_after),
        None,
    )
    .await;

    PendingClearExecutionOutcome::Completed {
        request_id: request.request_id,
        message_count_before,
        message_count_after,
    }
}

async fn execute_pending_compact_request(
    state: &mut TurnLedger,
    tx: &tokio::sync::mpsc::Sender<SdkMessage>,
) -> PendingCompactExecutionOutcome {
    let Some(request) = dequeue_pending_compact_request() else {
        return PendingCompactExecutionOutcome::NoRequest;
    };

    execute_compact_request_at_safe_point(state, tx, request).await
}

async fn execute_compact_request_at_safe_point(
    state: &mut TurnLedger,
    tx: &tokio::sync::mpsc::Sender<SdkMessage>,
    request: PendingCompactRequest,
) -> PendingCompactExecutionOutcome {
    if request.enqueued_at.elapsed() > COMPACT_REQUEST_TIMEOUT {
        warn!(
            request_id = %request.request_id,
            "Dropping timed-out compact_conversation control request"
        );
        emit_compact_request_status(
            tx,
            request.request_id.clone(),
            CompactRequestStatus::TimedOut,
            request.dry_run,
            None,
            None,
            None,
            None,
            None,
            Some("compact request timed out before safe-point execution".to_string()),
        )
        .await;
        return PendingCompactExecutionOutcome::TimedOut {
            request_id: request.request_id,
        };
    }

    match request.mode {
        CompactMode::Manual => {}
    }

    let pre_compact_token_count = estimate_messages_tokens(&state.messages);
    let message_count_before = state.messages.len();

    if request.dry_run {
        info!(
            request_id = %request.request_id,
            pre_compact_token_count,
            message_count = message_count_before,
            "Dry-run compact_conversation control request observed"
        );
        emit_compact_request_status(
            tx,
            request.request_id.clone(),
            CompactRequestStatus::DryRun,
            true,
            Some(pre_compact_token_count),
            None,
            Some(message_count_before),
            Some(message_count_before),
            Some(0),
            Some("dry run only".to_string()),
        )
        .await;
        return PendingCompactExecutionOutcome::DryRun {
            request_id: request.request_id,
            pre_compact_token_count,
            message_count: message_count_before,
        };
    }

    let mut options = CompactConversationOptions::without_hooks();
    options.trigger = "manual";
    options.custom_instructions = request.custom_instructions.as_deref();

    let compact_result = compact_conversation_with_options(&state.messages, "Read", options).await;
    if !compact_result.success {
        let reason = compact_result
            .error
            .unwrap_or_else(|| "compact_conversation failed".to_string());
        warn!(
            request_id = %request.request_id,
            error = %reason,
            "compact_conversation control request failed"
        );
        emit_compact_request_status(
            tx,
            request.request_id.clone(),
            CompactRequestStatus::Failed,
            false,
            Some(pre_compact_token_count),
            None,
            Some(message_count_before),
            Some(message_count_before),
            None,
            Some(reason.clone()),
        )
        .await;
        return PendingCompactExecutionOutcome::Failed {
            request_id: request.request_id,
            reason,
        };
    }

    if compact_result.compacted_message_count == 0 {
        emit_compact_request_status(
            tx,
            request.request_id.clone(),
            CompactRequestStatus::Skipped,
            false,
            Some(pre_compact_token_count),
            None,
            Some(message_count_before),
            Some(message_count_before),
            Some(0),
            Some("not enough messages to compact".to_string()),
        )
        .await;
        return PendingCompactExecutionOutcome::Skipped {
            request_id: request.request_id,
            reason: "not enough messages to compact".to_string(),
        };
    }

    let compacted_message_count = compact_result.compacted_message_count;
    let (new_messages, post_compact_token_count) = prepend_compact_boundary_to_messages(
        compact_result.new_messages,
        "manual",
        compacted_message_count,
        pre_compact_token_count as usize,
    );
    let message_count_after = new_messages.len();
    state.messages = new_messages;
    post_compact_cleanup::run_post_compact_cleanup(Some("sdk"));

    let _ = tx
        .send(SdkMessage::CompactBoundary {
            before_token_count: pre_compact_token_count,
            after_token_count: post_compact_token_count as u64,
            task_id: None,
        })
        .await;

    emit_compact_request_status(
        tx,
        request.request_id.clone(),
        CompactRequestStatus::Completed,
        false,
        Some(pre_compact_token_count),
        Some(post_compact_token_count as u64),
        Some(message_count_before),
        Some(message_count_after),
        Some(compacted_message_count),
        None,
    )
    .await;

    PendingCompactExecutionOutcome::Completed {
        request_id: request.request_id,
        pre_compact_token_count,
        post_compact_token_count: post_compact_token_count as u64,
        compacted_message_count,
        message_count_before,
        message_count_after,
    }
}

fn strip_synthetic_thinking_sections(text: &str) -> String {
    let without_think = strip_ascii_tagged_section(text, "think");
    let without_thinking = strip_ascii_tagged_section(&without_think, "thinking");
    remove_ascii_tag(
        remove_ascii_tag(without_thinking, "<response>"),
        "</response>",
    )
    .trim()
    .to_string()
}

fn strip_ascii_tagged_section(input: &str, tag: &str) -> String {
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    let mut out = input.to_string();

    loop {
        let lower = out.to_ascii_lowercase();
        let Some(start) = lower.find(&open) else {
            break;
        };
        let content_start = start + open.len();
        let end = lower[content_start..]
            .find(&close)
            .map(|rel| content_start + rel + close.len())
            .unwrap_or(out.len());
        out.replace_range(start..end, "");
    }

    out
}

fn remove_ascii_tag(input: String, tag: &str) -> String {
    let mut out = input;
    loop {
        let lower = out.to_ascii_lowercase();
        let Some(start) = lower.find(tag) else {
            break;
        };
        out.replace_range(start..start + tag.len(), "");
    }
    out
}

/// 快照当前轮次的环境配置。
///
/// 对应 TS `buildQueryConfig()`。
fn snapshot_turn_env(spec: &DialogueSpec, _state: &TurnLedger) -> TurnEnvironment {
    TurnEnvironment {
        model: spec.model.clone(),
        context_window: 200_000, // 默认上下文窗口
        max_output_tokens: spec.max_output_tokens.unwrap_or(16_000),
        thinking_config: if spec.thinking_enabled {
            Some(ThinkingConfig {
                enabled: true,
                budget_tokens: spec.thinking_budget,
            })
        } else {
            None
        },
        fast_mode: spec.fast_mode.unwrap_or(false),
        effort: spec.effort,
        auto_mode: spec.auto_mode,
        betas: Vec::new(),
    }
}

/// 从流式累加器构建完整的 Message。
fn build_assistant_message(acc: &StreamAccumulator) -> Message {
    use crate::streaming::AccumulatedBlock;

    let content: Vec<ContentBlock> = acc
        .content_blocks
        .iter()
        .map(|block| match block {
            AccumulatedBlock::Text(text) => ContentBlock::Text(TextBlock { text: text.clone() }),
            AccumulatedBlock::ToolUse {
                id,
                name,
                input_json,
            } => {
                let input: serde_json::Value = serde_json::from_str(input_json)
                    .unwrap_or_else(|_| serde_json::Value::Object(Default::default()));
                ContentBlock::ToolUse(ToolUseBlock {
                    id: id.clone(),
                    name: name.clone(),
                    input,
                })
            }
            AccumulatedBlock::Thinking { thinking } => {
                ContentBlock::Thinking(mossen_types::ThinkingBlock {
                    thinking: thinking.clone(),
                    signature: None,
                })
            }
        })
        .collect();

    let mut extra = HashMap::new();
    if let Some(ref reason) = acc.stop_reason {
        extra.insert(
            "stop_reason".to_string(),
            serde_json::Value::String(reason.clone()),
        );
    }

    Message {
        role: Role::Assistant,
        content,
        uuid: Some(uuid::Uuid::new_v4().to_string()),
        is_meta: None,
        origin: None,
        timestamp: Some(chrono::Utc::now().to_rfc3339()),
        extra,
    }
}

/// 将 StreamEvent 转换为 SDK StreamEventData。
fn stream_event_to_sdk(event: &StreamEvent) -> Option<StreamEventData> {
    match event {
        StreamEvent::ContentBlockStart { index, .. } => {
            Some(StreamEventData::ContentBlockStart { index: *index })
        }
        StreamEvent::ContentBlockDelta { index, delta } => {
            Some(StreamEventData::ContentBlockDelta {
                index: *index,
                delta: delta.clone(),
            })
        }
        StreamEvent::ContentBlockStop { index } => {
            Some(StreamEventData::ContentBlockStop { index: *index })
        }
        StreamEvent::MessageStart { .. } => Some(StreamEventData::MessageStart),
        StreamEvent::MessageDelta { delta, usage } => Some(StreamEventData::MessageDelta {
            usage: usage.clone(),
            stop_reason: delta.stop_reason.clone(),
        }),
        StreamEvent::MessageStop => Some(StreamEventData::MessageStop),
        _ => None,
    }
}

/// Route an `mcp__<server>__<tool>` invocation to the live MCP client held
/// by the process-global `McpServerManager`. Builds a structured tool result
/// either way — success carries the concatenated text content, failure modes
/// (unknown server, disconnected, RPC error) return `is_error: true` so the
/// model can react instead of seeing an empty success.
async fn execute_mcp_tool(
    qualified_name: &str,
    input: serde_json::Value,
) -> anyhow::Result<crate::tool_registry::ToolResult> {
    use crate::tool_registry::ToolResult;
    let parsed = mossen_mcp::tools::parse_mcp_tool_name(qualified_name);
    let (server, tool) = match parsed {
        Some((server, Some(tool))) => (server, tool),
        _ => {
            return Ok(ToolResult {
                output: format!(
                    "Error: malformed MCP tool name '{}', expected mcp__<server>__<tool>",
                    qualified_name
                ),
                is_error: true,
                duration_ms: 0,
                metadata: HashMap::new(),
            });
        }
    };

    let Some(manager) = mossen_mcp::server::global_manager() else {
        return Ok(ToolResult {
            output: "Error: MCP server manager not installed — tool calls cannot be routed."
                .to_string(),
            is_error: true,
            duration_ms: 0,
            metadata: HashMap::new(),
        });
    };

    let Some((server_name, client)) = manager.get_client_by_normalized_name(&server) else {
        return Ok(ToolResult {
            output: format!(
                "Error: MCP server '{}' is not connected (or not present in the configured set).",
                server
            ),
            is_error: true,
            duration_ms: 0,
            metadata: HashMap::new(),
        });
    };

    let original_tool_name = match client.list_tools().await {
        Ok(result) => result
            .tools
            .into_iter()
            .find(|candidate| mossen_mcp::normalize_name_for_mcp(&candidate.name) == tool)
            .map(|candidate| candidate.name)
            .unwrap_or_else(|| tool.clone()),
        Err(_) => tool.clone(),
    };

    let arguments = if input.is_null() { None } else { Some(input) };
    match mossen_mcp::tools::execute_mcp_tool_call(&client, &original_tool_name, arguments).await {
        Ok(result) => Ok(ToolResult {
            output: result.text,
            is_error: result.is_error,
            duration_ms: 0,
            metadata: HashMap::new(),
        }),
        Err(e) => Ok(ToolResult {
            output: format!(
                "Error: MCP call to '{}/{}' failed: {}",
                server_name, original_tool_name, e
            ),
            is_error: true,
            duration_ms: 0,
            metadata: HashMap::new(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        effective_permission_mode, execute_pending_clear_request, execute_pending_compact_request,
        permission_mode_decision, session_permission_rule_decision,
        strip_synthetic_thinking_sections, PendingClearExecutionOutcome,
        PendingCompactExecutionOutcome,
    };
    use crate::services::compact::pending_compact_request::{
        clear_pending_compact_request, enqueue_pending_compact_request, CompactMode,
    };
    use crate::services::root::pending_clear_request::{
        clear_pending_clear_request, enqueue_pending_clear_request,
    };
    use crate::types::{
        ClearRequestStatus, CompactRequestStatus, ContinueReason, PermissionDecision,
        PermissionMode, ToolUseContext, TurnLedger,
    };
    use mossen_types::{ContentBlock, Message, Role, TextBlock};
    use std::collections::HashMap;

    fn permission_env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
        LOCK.get_or_init(|| std::sync::Mutex::new(()))
            .lock()
            .expect("permission env lock")
    }

    fn restore_permission_env(previous: Option<String>) {
        if let Some(previous) = previous {
            std::env::set_var(super::PERMISSION_MODE_ENV, previous);
        } else {
            std::env::remove_var(super::PERMISSION_MODE_ENV);
        }
    }

    fn restore_permission_env_vars(previous: Vec<(&'static str, Option<String>)>) {
        for (key, value) in previous {
            if let Some(value) = value {
                std::env::set_var(key, value);
            } else {
                std::env::remove_var(key);
            }
        }
    }

    fn test_message(role: Role, text: &str) -> Message {
        Message {
            role,
            content: vec![ContentBlock::Text(TextBlock {
                text: text.to_string(),
            })],
            uuid: None,
            is_meta: None,
            origin: None,
            timestamp: None,
            extra: HashMap::new(),
        }
    }

    fn test_turn_ledger(messages: Vec<Message>) -> TurnLedger {
        TurnLedger {
            messages,
            tool_use_context: ToolUseContext {
                cwd: ".".to_string(),
                additional_working_directories: None,
                extra: HashMap::new(),
            },
            auto_compact_tracking: None,
            max_output_tokens_recovery_count: 0,
            has_attempted_reactive_compact: false,
            max_output_tokens_override: None,
            pending_tool_use_summary: None,
            stop_hook_active: None,
            turn_count: 0,
            transition: None,
        }
    }

    #[test]
    fn thinking_only_text_is_not_user_visible() {
        let text = "<think>I should inspect files first.</think>";

        assert_eq!(strip_synthetic_thinking_sections(text), "");
    }

    #[test]
    fn visible_answer_after_thinking_is_preserved() {
        let text = "<think>tool result received</think>\n<response>Done.</response>";

        assert_eq!(strip_synthetic_thinking_sections(text), "Done.");
    }

    #[test]
    fn unclosed_thinking_is_treated_as_no_visible_answer() {
        let text = "<thinking>still reasoning";

        assert_eq!(strip_synthetic_thinking_sections(text), "");
    }

    #[test]
    fn permission_mode_parse_accepts_ui_and_sdk_spellings() {
        assert_eq!(
            PermissionMode::parse("accept-edits"),
            PermissionMode::AcceptEdits
        );
        assert_eq!(
            PermissionMode::parse("Full Auto"),
            PermissionMode::BypassPermissions
        );
        assert_eq!(PermissionMode::parse("supervised"), PermissionMode::Default);
        assert_eq!(PermissionMode::parse("suggest"), PermissionMode::Default);
        assert_eq!(PermissionMode::parse("ask"), PermissionMode::Default);
        assert_eq!(PermissionMode::parse("read-only"), PermissionMode::Plan);
        assert_eq!(PermissionMode::parse("readonly"), PermissionMode::Plan);
        assert_eq!(PermissionMode::parse("never-ask"), PermissionMode::DontAsk);
    }

    #[test]
    fn effective_permission_mode_prefers_session_env_override() {
        let _guard = permission_env_lock();
        let previous = std::env::var(super::PERMISSION_MODE_ENV).ok();
        std::env::set_var(super::PERMISSION_MODE_ENV, "plan");

        assert_eq!(
            effective_permission_mode(PermissionMode::BypassPermissions),
            PermissionMode::Plan
        );

        restore_permission_env(previous);
    }

    #[test]
    fn plan_mode_blocks_mutating_tools_but_allows_plan_release() {
        let edit = permission_mode_decision(PermissionMode::Plan, "Edit", false)
            .expect("plan mode should decide mutating tools");
        assert_eq!(edit.decision, PermissionDecision::Deny);
        assert!(edit.deny_message.is_some());

        let release = permission_mode_decision(PermissionMode::Plan, "ExitPlanMode", false)
            .expect("plan mode should allow releasing the plan");
        assert_eq!(release.decision, PermissionDecision::Allow);
    }

    #[test]
    fn accept_edits_only_short_circuits_edit_tools() {
        let edit = permission_mode_decision(PermissionMode::AcceptEdits, "Edit", false)
            .expect("acceptEdits should auto-allow edits");
        assert_eq!(edit.decision, PermissionDecision::Allow);

        assert!(permission_mode_decision(PermissionMode::AcceptEdits, "Bash", false).is_none());
    }

    #[test]
    fn bypass_and_dont_ask_modes_are_non_interactive() {
        let bypass = permission_mode_decision(PermissionMode::BypassPermissions, "Bash", false)
            .expect("bypassPermissions should decide every approval tool");
        assert_eq!(bypass.decision, PermissionDecision::Allow);

        let dont_ask = permission_mode_decision(PermissionMode::DontAsk, "Bash", false)
            .expect("dontAsk should decide every approval tool");
        assert_eq!(dont_ask.decision, PermissionDecision::Deny);
    }

    #[test]
    fn session_permission_rules_allow_matching_tool_inputs() {
        let _guard = permission_env_lock();
        let previous = vec![
            (
                super::PERMISSION_ALLOW_RULES_ENV,
                std::env::var(super::PERMISSION_ALLOW_RULES_ENV).ok(),
            ),
            (
                super::PERMISSION_DENY_RULES_ENV,
                std::env::var(super::PERMISSION_DENY_RULES_ENV).ok(),
            ),
        ];
        std::env::set_var(super::PERMISSION_ALLOW_RULES_ENV, "Bash cargo test");
        std::env::remove_var(super::PERMISSION_DENY_RULES_ENV);

        let decision = session_permission_rule_decision(
            "Bash",
            &serde_json::json!({ "command": "cargo test -q" }),
        )
        .expect("session allow rule should match command candidate");
        assert_eq!(decision.decision, PermissionDecision::Allow);
        assert!(decision.deny_message.is_none());

        restore_permission_env_vars(previous);
    }

    #[test]
    fn session_permission_rules_deny_precedes_allow() {
        let _guard = permission_env_lock();
        let previous = vec![
            (
                super::PERMISSION_ALLOW_RULES_ENV,
                std::env::var(super::PERMISSION_ALLOW_RULES_ENV).ok(),
            ),
            (
                super::PERMISSION_DENY_RULES_ENV,
                std::env::var(super::PERMISSION_DENY_RULES_ENV).ok(),
            ),
        ];
        std::env::set_var(super::PERMISSION_ALLOW_RULES_ENV, "*");
        std::env::set_var(super::PERMISSION_DENY_RULES_ENV, "Bash cargo test");

        let decision = session_permission_rule_decision(
            "Bash",
            &serde_json::json!({ "command": "cargo test -q" }),
        )
        .expect("session deny rule should match before allow wildcard");
        assert_eq!(decision.decision, PermissionDecision::Deny);
        assert_eq!(
            decision.deny_message,
            Some("Tool call denied by session permission rule.")
        );

        restore_permission_env_vars(previous);
    }

    #[test]
    fn session_permission_rules_match_file_path_prefixes() {
        let _guard = permission_env_lock();
        let previous = vec![
            (
                super::PERMISSION_ALLOW_RULES_ENV,
                std::env::var(super::PERMISSION_ALLOW_RULES_ENV).ok(),
            ),
            (
                super::PERMISSION_DENY_RULES_ENV,
                std::env::var(super::PERMISSION_DENY_RULES_ENV).ok(),
            ),
        ];
        std::env::remove_var(super::PERMISSION_ALLOW_RULES_ENV);
        std::env::set_var(super::PERMISSION_DENY_RULES_ENV, "src/generated/");

        let decision = session_permission_rule_decision(
            "Write",
            &serde_json::json!({ "file_path": "src/generated/output.rs" }),
        )
        .expect("path-prefix deny rule should match file path candidate");
        assert_eq!(decision.decision, PermissionDecision::Deny);

        restore_permission_env_vars(previous);
    }

    #[tokio::test]
    async fn pending_compact_request_compacts_state_and_emits_boundary() {
        clear_pending_compact_request();
        let mut state = test_turn_ledger(vec![
            test_message(Role::User, "one"),
            test_message(Role::Assistant, "two"),
            test_message(Role::User, "three"),
            test_message(Role::Assistant, "four"),
        ]);
        enqueue_pending_compact_request(
            "compact-request".to_string(),
            CompactMode::Manual,
            false,
            Some("keep decisions".to_string()),
        )
        .expect("enqueue compact request");
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);

        let outcome = execute_pending_compact_request(&mut state, &tx).await;

        match outcome {
            PendingCompactExecutionOutcome::Completed {
                request_id,
                compacted_message_count,
                message_count_before,
                message_count_after,
                ..
            } => {
                assert_eq!(request_id, "compact-request");
                assert_eq!(compacted_message_count, 2);
                assert_eq!(message_count_before, 4);
                assert_eq!(message_count_after, state.messages.len());
            }
            other => panic!("expected completed compact request, got {other:?}"),
        }

        let boundary = state.messages.first().expect("compact boundary");
        let metadata = boundary
            .extra
            .get("compact_metadata")
            .expect("compact metadata");
        assert_eq!(metadata["trigger"], "manual");
        assert_eq!(metadata["compacted_message_count"], 2);

        let summary_text = match state.messages.get(1).and_then(|m| m.content.first()) {
            Some(ContentBlock::Text(text)) => text.text.as_str(),
            other => panic!("expected summary text after boundary, got {other:?}"),
        };
        assert!(summary_text.contains("Compaction instructions applied: keep decisions"));

        match rx.recv().await.expect("compact boundary event") {
            crate::types::SdkMessage::CompactBoundary {
                before_token_count,
                after_token_count,
                ..
            } => {
                assert!(before_token_count > 0);
                assert!(after_token_count > 0);
            }
            other => panic!("expected compact boundary event, got {other:?}"),
        }
        match rx.recv().await.expect("compact status event") {
            crate::types::SdkMessage::CompactRequestStatus {
                request_id,
                status,
                dry_run,
                before_token_count,
                after_token_count,
                compacted_message_count,
                ..
            } => {
                assert_eq!(request_id, "compact-request");
                assert_eq!(status, CompactRequestStatus::Completed);
                assert!(!dry_run);
                assert!(before_token_count.unwrap_or(0) > 0);
                assert!(after_token_count.unwrap_or(0) > 0);
                assert_eq!(compacted_message_count, Some(2));
            }
            other => panic!("expected compact status event, got {other:?}"),
        }
        assert!(rx.try_recv().is_err());

        clear_pending_compact_request();
    }

    #[tokio::test]
    async fn pending_compact_request_dry_run_does_not_mutate_or_emit_boundary() {
        clear_pending_compact_request();
        let messages = vec![
            test_message(Role::User, "one"),
            test_message(Role::Assistant, "two"),
        ];
        let mut state = test_turn_ledger(messages.clone());
        enqueue_pending_compact_request(
            "dry-run-compact".to_string(),
            CompactMode::Manual,
            true,
            None,
        )
        .expect("enqueue compact request");
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);

        let outcome = execute_pending_compact_request(&mut state, &tx).await;

        match outcome {
            PendingCompactExecutionOutcome::DryRun {
                request_id,
                message_count,
                ..
            } => {
                assert_eq!(request_id, "dry-run-compact");
                assert_eq!(message_count, messages.len());
            }
            other => panic!("expected dry-run compact request, got {other:?}"),
        }
        assert_eq!(state.messages.len(), messages.len());
        match rx.recv().await.expect("compact dry-run status event") {
            crate::types::SdkMessage::CompactRequestStatus {
                request_id,
                status,
                dry_run,
                before_token_count,
                after_token_count,
                message_count_before,
                message_count_after,
                compacted_message_count,
                reason,
                ..
            } => {
                assert_eq!(request_id, "dry-run-compact");
                assert_eq!(status, CompactRequestStatus::DryRun);
                assert!(dry_run);
                assert!(before_token_count.unwrap_or(0) > 0);
                assert_eq!(after_token_count, None);
                assert_eq!(message_count_before, Some(messages.len() as u64));
                assert_eq!(message_count_after, Some(messages.len() as u64));
                assert_eq!(compacted_message_count, Some(0));
                assert_eq!(reason.as_deref(), Some("dry run only"));
            }
            other => panic!("expected compact dry-run status event, got {other:?}"),
        }
        assert!(rx.try_recv().is_err());

        clear_pending_compact_request();
    }

    #[tokio::test]
    async fn pending_compact_request_skipped_emits_status_event() {
        clear_pending_compact_request();
        let messages = vec![test_message(Role::User, "one")];
        let mut state = test_turn_ledger(messages.clone());
        enqueue_pending_compact_request(
            "skip-compact".to_string(),
            CompactMode::Manual,
            false,
            None,
        )
        .expect("enqueue compact request");
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);

        let outcome = execute_pending_compact_request(&mut state, &tx).await;

        match outcome {
            PendingCompactExecutionOutcome::Skipped { request_id, reason } => {
                assert_eq!(request_id, "skip-compact");
                assert_eq!(reason, "not enough messages to compact");
            }
            other => panic!("expected skipped compact request, got {other:?}"),
        }
        assert_eq!(state.messages.len(), messages.len());
        match rx.recv().await.expect("compact skipped status event") {
            crate::types::SdkMessage::CompactRequestStatus {
                request_id,
                status,
                dry_run,
                before_token_count,
                after_token_count,
                message_count_before,
                message_count_after,
                compacted_message_count,
                reason,
                ..
            } => {
                assert_eq!(request_id, "skip-compact");
                assert_eq!(status, CompactRequestStatus::Skipped);
                assert!(!dry_run);
                assert!(before_token_count.unwrap_or(0) > 0);
                assert_eq!(after_token_count, None);
                assert_eq!(message_count_before, Some(messages.len() as u64));
                assert_eq!(message_count_after, Some(messages.len() as u64));
                assert_eq!(compacted_message_count, Some(0));
                assert_eq!(reason.as_deref(), Some("not enough messages to compact"));
            }
            other => panic!("expected compact skipped status event, got {other:?}"),
        }
        assert!(rx.try_recv().is_err());

        clear_pending_compact_request();
    }

    #[tokio::test]
    async fn pending_clear_request_clears_state_and_emits_event() {
        clear_pending_clear_request();
        let mut state = test_turn_ledger(vec![
            test_message(Role::User, "one"),
            test_message(Role::Assistant, "two"),
        ]);
        enqueue_pending_clear_request("clear-request".to_string(), false)
            .expect("enqueue clear request");
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);

        let outcome = execute_pending_clear_request(&mut state, &tx).await;

        match outcome {
            PendingClearExecutionOutcome::Completed {
                request_id,
                message_count_before,
                message_count_after,
            } => {
                assert_eq!(request_id, "clear-request");
                assert_eq!(message_count_before, 2);
                assert_eq!(message_count_after, 0);
            }
            other => panic!("expected completed clear request, got {other:?}"),
        }
        assert!(state.messages.is_empty());

        match rx.recv().await.expect("conversation cleared event") {
            crate::types::SdkMessage::ConversationCleared {
                message_count_before,
                message_count_after,
                ..
            } => {
                assert_eq!(message_count_before, 2);
                assert_eq!(message_count_after, 0);
            }
            other => panic!("expected conversation cleared event, got {other:?}"),
        }
        match rx.recv().await.expect("clear status event") {
            crate::types::SdkMessage::ClearRequestStatus {
                request_id,
                status,
                dry_run,
                message_count_before,
                message_count_after,
                reason,
                ..
            } => {
                assert_eq!(request_id, "clear-request");
                assert_eq!(status, ClearRequestStatus::Completed);
                assert!(!dry_run);
                assert_eq!(message_count_before, Some(2));
                assert_eq!(message_count_after, Some(0));
                assert_eq!(reason, None);
            }
            other => panic!("expected clear status event, got {other:?}"),
        }
        assert!(rx.try_recv().is_err());

        clear_pending_clear_request();
    }

    #[tokio::test]
    async fn pending_clear_request_dry_run_emits_status_event() {
        clear_pending_clear_request();
        let messages = vec![
            test_message(Role::User, "one"),
            test_message(Role::Assistant, "two"),
        ];
        let mut state = test_turn_ledger(messages.clone());
        enqueue_pending_clear_request("dry-run-clear".to_string(), true)
            .expect("enqueue clear request");
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);

        let outcome = execute_pending_clear_request(&mut state, &tx).await;

        match outcome {
            PendingClearExecutionOutcome::DryRun {
                request_id,
                message_count,
            } => {
                assert_eq!(request_id, "dry-run-clear");
                assert_eq!(message_count, messages.len());
            }
            other => panic!("expected dry-run clear request, got {other:?}"),
        }
        assert_eq!(state.messages.len(), messages.len());
        match rx.recv().await.expect("clear dry-run status event") {
            crate::types::SdkMessage::ClearRequestStatus {
                request_id,
                status,
                dry_run,
                message_count_before,
                message_count_after,
                reason,
                ..
            } => {
                assert_eq!(request_id, "dry-run-clear");
                assert_eq!(status, ClearRequestStatus::DryRun);
                assert!(dry_run);
                assert_eq!(message_count_before, Some(messages.len() as u64));
                assert_eq!(message_count_after, Some(messages.len() as u64));
                assert_eq!(reason.as_deref(), Some("dry run only"));
            }
            other => panic!("expected clear dry-run status event, got {other:?}"),
        }
        assert!(rx.try_recv().is_err());

        clear_pending_clear_request();
    }
}
