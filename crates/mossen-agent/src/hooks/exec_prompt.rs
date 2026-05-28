//! # exec_prompt — Prompt Hook 执行器
//!
//! 对应 TS `utils/hooks/execPromptHook.ts`。
//! 使用单次 LLM 查询执行 Prompt Hook。
//! 返回 JSON schema 输出（ok/reason）。

use std::time::Duration;

use mossen_types::hooks::HookOutcome;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use crate::api::mossen_api::{query_with_model, SystemPrompt};

/// Prompt Hook 响应 schema。
///
/// 对应 TS `hookResponseSchema()`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookResponseData {
    /// 条件是否满足。
    pub ok: bool,
    /// 原因（条件未满足时）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Prompt Hook 配置。
#[derive(Debug, Clone)]
pub struct PromptHookConfig {
    /// Prompt 文本。
    pub prompt: String,
    /// 模型名称。
    pub model: Option<String>,
    /// 超时时间（秒）。
    pub timeout_secs: Option<f64>,
}

/// Prompt Hook 执行结果。
#[derive(Debug, Clone)]
pub struct PromptHookResult {
    /// 执行结果状态。
    pub outcome: HookOutcome,
    /// 阻塞错误消息。
    pub blocking_error: Option<String>,
    /// 是否阻止继续。
    pub prevent_continuation: bool,
    /// 停止原因。
    pub stop_reason: Option<String>,
    /// 原始响应文本。
    pub response_text: Option<String>,
}

/// 执行 Prompt Hook。
///
/// 对应 TS `execPromptHook()`。使用 Fast（或 hook.model 指定的模型）发起
/// 单次非流式查询，要求模型按 `hookResponseSchema` 返回 JSON：
/// `{"ok": true}` 或 `{"ok": false, "reason": "..."}`。
///
/// 翻译参考 `utils/hooks/execPromptHook.ts`。
pub async fn exec_prompt_hook(
    config: &PromptHookConfig,
    json_input: &str,
    _hook_name: &str,
) -> PromptHookResult {
    // 替换 $ARGUMENTS 占位符。
    let processed_prompt = substitute_arguments(&config.prompt, json_input);
    debug!(prompt = %processed_prompt, "Executing prompt hook");

    // 系统提示词：要求模型严格输出 hookResponseSchema 形状的 JSON。
    let system: SystemPrompt = vec![
        "You are evaluating a hook in Mossen.\n\n\
        Your response must be a JSON object matching one of the following schemas:\n\
        1. If the condition is met, return: {\"ok\": true}\n\
        2. If the condition is not met, return: {\"ok\": false, \"reason\": \"Reason for why it is not met\"}"
            .to_string(),
    ];

    // 输出 schema —— 通过 query_with_model 的 output_format 参数发送，
    // 让模型严格按 hookResponseSchema 输出。
    let output_format = json!({
        "type": "json_schema",
        "schema": {
            "type": "object",
            "properties": {
                "ok": { "type": "boolean" },
                "reason": { "type": "string" }
            },
            "required": ["ok"],
            "additionalProperties": false
        }
    });

    // 模型选择：优先用 hook 显式指定；否则用全局 small/fast model。
    let model = config.model.clone().unwrap_or_else(small_fast_model);

    // 超时控制——超时 / 取消由 cancellation token 触发；query_with_model
    // 内部读取 API_TIMEOUT_MS 但 hook 自身可指定更短超时。
    let timeout_ms = config
        .timeout_secs
        .map(|s| (s * 1000.0) as u64)
        .unwrap_or(30_000);

    let cancel = CancellationToken::new();
    let cancel_for_timeout = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(timeout_ms)).await;
        cancel_for_timeout.cancel();
    });

    let response = match query_with_model(
        &system,
        &processed_prompt,
        &model,
        Some(&output_format),
        &cancel,
        false,
    )
    .await
    {
        Ok(resp) => resp,
        Err(err) => {
            warn!(error = %err, "Prompt hook LLM query failed");
            if cancel.is_cancelled() {
                return PromptHookResult {
                    outcome: HookOutcome::Cancelled,
                    blocking_error: None,
                    prevent_continuation: false,
                    stop_reason: None,
                    response_text: None,
                };
            }
            return PromptHookResult {
                outcome: HookOutcome::NonBlockingError,
                blocking_error: None,
                prevent_continuation: false,
                stop_reason: None,
                response_text: Some(format!("Error executing prompt hook: {err}")),
            };
        }
    };

    let text = extract_text_content_from_value(&response.message.content);
    let trimmed = text.trim().to_string();
    debug!(response = %trimmed, "Prompt hook model response");

    match parse_hook_response(&trimmed) {
        Ok(parsed) => {
            if parsed.ok {
                PromptHookResult {
                    outcome: HookOutcome::Success,
                    blocking_error: None,
                    prevent_continuation: false,
                    stop_reason: None,
                    response_text: Some(trimmed),
                }
            } else {
                PromptHookResult {
                    outcome: HookOutcome::Blocking,
                    blocking_error: Some(format!(
                        "Prompt hook condition was not met: {}",
                        parsed.reason.as_deref().unwrap_or("(no reason given)")
                    )),
                    prevent_continuation: true,
                    stop_reason: parsed.reason,
                    response_text: Some(trimmed),
                }
            }
        }
        Err(err) => {
            warn!(error = %err, response = %trimmed, "Prompt hook response failed schema validation");
            PromptHookResult {
                outcome: HookOutcome::NonBlockingError,
                blocking_error: None,
                prevent_continuation: false,
                stop_reason: None,
                response_text: Some(format!("Schema validation failed: {err}")),
            }
        }
    }
}

/// 默认 small/fast 模型——与 `mossen_api::get_small_fast_model` 一致。
fn small_fast_model() -> String {
    std::env::var("MOSSEN_SMALL_FAST_MODEL")
        .unwrap_or_else(|_| "mossen-3-5-fast-latest".to_string())
}

/// 提取 assistant 消息中的文本内容（拼接所有 text 块）。
///
/// `mossen_api::AssistantMessage.message.content` 是 `serde_json::Value`
/// （未经反序列化的原始 ContentBlock 数组）。对应 TS `extractTextContent`。
fn extract_text_content_from_value(content: &serde_json::Value) -> String {
    let Some(arr) = content.as_array() else {
        return content.as_str().unwrap_or("").to_string();
    };
    arr.iter()
        .filter_map(|block| {
            if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                block.get("text").and_then(|t| t.as_str())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("")
}

/// 替换 prompt 中的 $ARGUMENTS 占位符。
///
/// 对应 TS `addArgumentsToPrompt()` / `substituteArguments()`。
pub fn substitute_arguments(prompt: &str, json_input: &str) -> String {
    if prompt.contains("$ARGUMENTS") {
        prompt.replace("$ARGUMENTS", json_input)
    } else {
        format!("{prompt}\n\nArguments:\n{json_input}")
    }
}

/// 解析 Prompt Hook 响应。
///
/// 对应 TS 中解析 `hookResponseSchema` 的逻辑。
pub fn parse_hook_response(text: &str) -> Result<HookResponseData, String> {
    serde_json::from_str(text).map_err(|e| format!("Failed to parse hook response: {e}"))
}
