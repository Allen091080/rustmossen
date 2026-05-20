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
use crate::retry::{self, RetryConfig, RetryError, SystemApiErrorNotification};
use crate::hooks::post_sampling::{PostInferenceContext, PostSamplingHookRegistry};
use crate::stop_hooks::{StopHookContext, StopHookManager, StopHookResult};
use crate::streaming::{StreamAccumulator, StreamEvent};
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
/// 升档的 max_output_tokens 值。
const ESCALATED_MAX_OUTPUT_TOKENS: u32 = 64_000;

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

    // 发送系统初始化消息
    let _ = tx
        .send(SdkMessage::SystemInit {
            session_id: session_id.clone(),
            model: spec.model.clone(),
            tools: spec.tools.iter().map(|t| t.name.clone()).collect(),
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
            })
            .await;
    }

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

    let cancel = &spec.cancel;

    loop {
        // 0. 取消检查
        if cancel.is_cancelled() {
            return Ok((TerminalReason::AbortedStreaming, cost_state));
        }

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

        if let AutoCompactResult::Compacted {
            before_tokens,
            after_tokens,
            summary: _,
        } = compact_result
        {
            let _ = tx
                .send(SdkMessage::CompactBoundary {
                    before_token_count: before_tokens,
                    after_token_count: after_tokens,
                })
                .await;
        }

        // 3. 补齐孤立 tool_use 的 tool_result
        let missing_results = yield_missing_tool_result_blocks(&prepared.messages);
        let mut messages_for_query = prepared.messages;
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
                        let _ = tx.send(SdkMessage::StreamEvent { event: sdk_event }).await;
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

        // 发送助手消息到 UI
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
            })
            .await;

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
        let visible_text = accumulator.visible_text();
        if !accumulator.has_tool_use()
            && should_recover_action_promise(&visible_text)
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

            let input: serde_json::Value = serde_json::from_str(input_json).unwrap_or_default();

            // ── Permission gate ───────────────────────────────────────
            // Before invoking the tool we consult `spec.permission_gate`.
            // Default is `AllowAllGate` (open). For supervised mode the TUI
            // installs an `InteractiveGate` whose `check()` posts a
            // `PermissionRequest` to the UI and waits on a oneshot reply.
            // A `Deny` short-circuits to a tool_result error block so the
            // model can react without us silently executing.
            let decision = spec.permission_gate.check(tool_name, tool_id, &input).await;
            if !decision.is_allowed() {
                debug!(tool = %tool_name, id = %tool_id, "Tool execution denied by permission gate");
                tool_results.push(ContentBlock::ToolResult(ToolResultBlock {
                    tool_use_id: tool_id.clone(),
                    content: ToolResultContent::Text(
                        "User denied permission for this tool call.".to_string(),
                    ),
                    is_error: Some(true),
                }));
                continue;
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
                execute_mcp_tool(tool_name, input.clone()).await
            } else {
                tool_registry
                    .execute(tool_name, input, &state.tool_use_context)
                    .await
            };

            let tool_duration = tool_start.elapsed().as_millis() as u64;
            cost_tracker::record_tool_duration(&mut cost_state, tool_duration);

            let result_block = match exec_result {
                Ok(result) => {
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
                Err(e) => ContentBlock::ToolResult(ToolResultBlock {
                    tool_use_id: tool_id.clone(),
                    content: ToolResultContent::Text(format!("Error: {}", e)),
                    is_error: Some(true),
                }),
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
                        summary: preview,
                        full_content: full,
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
                let input: serde_json::Value = serde_json::from_str(input_json).unwrap_or_default();
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
            output:
                "Error: MCP server manager not installed — tool calls cannot be routed.".to_string(),
            is_error: true,
            duration_ms: 0,
            metadata: HashMap::new(),
        });
    };

    let Some(client) = manager.get_client(&server) else {
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

    let arguments = if input.is_null() { None } else { Some(input) };
    match mossen_mcp::tools::execute_mcp_tool_call(&client, &tool, arguments).await {
        Ok(result) => Ok(ToolResult {
            output: result.text,
            is_error: result.is_error,
            duration_ms: 0,
            metadata: HashMap::new(),
        }),
        Err(e) => Ok(ToolResult {
            output: format!("Error: MCP call to '{}/{}' failed: {}", server, tool, e),
            is_error: true,
            duration_ms: 0,
            metadata: HashMap::new(),
        }),
    }
}
