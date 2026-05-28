//! # elicitation — MCP elicitation 请求处理
//!
//! 对应 TypeScript `services/mcp/elicitationHandler.ts`。
//! 提供 elicitation 请求事件、等待状态、注册与 hook 编排。Rust 端不持有
//! AppState，而是把队列交给调用方维护，仅暴露原始事件 + hook 调度。

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// `elicitationHandler.ts` `ElicitationWaitingState`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElicitationWaitingState {
    pub action_label: String,
    #[serde(default)]
    pub show_cancel: bool,
}

/// 简化的 ElicitResult 形态 — 与 TS `ElicitResult` 对应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElicitResult {
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<JsonValue>,
}

/// 简化的 ElicitRequestParams 形态。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ElicitRequestParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(default)]
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(
        default,
        rename = "elicitationId",
        skip_serializing_if = "Option::is_none"
    )]
    pub elicitation_id: Option<String>,
    #[serde(
        default,
        rename = "requestedSchema",
        skip_serializing_if = "Option::is_none"
    )]
    pub requested_schema: Option<JsonValue>,
}

/// `elicitationHandler.ts` `ElicitationRequestEvent` — 入队事件。
pub struct ElicitationRequestEvent {
    pub server_name: String,
    pub request_id: JsonValue,
    pub params: ElicitRequestParams,
    pub waiting_state: Option<ElicitationWaitingState>,
    pub respond: Arc<dyn Fn(ElicitResult) + Send + Sync>,
    pub on_waiting_dismiss: Option<Arc<dyn Fn(&str) + Send + Sync>>,
    pub completed: bool,
}

/// `elicitationHandler.ts` `getElicitationMode`。
pub fn get_elicitation_mode(params: &ElicitRequestParams) -> &'static str {
    match params.mode.as_deref() {
        Some("url") => "url",
        _ => "form",
    }
}

/// `elicitationHandler.ts` `findElicitationInQueue`。
pub fn find_elicitation_in_queue(
    queue: &[ElicitationRequestEvent],
    server_name: &str,
    elicitation_id: &str,
) -> Option<usize> {
    queue.iter().position(|e| {
        e.server_name == server_name
            && e.params.mode.as_deref() == Some("url")
            && e.params.elicitation_id.as_deref() == Some(elicitation_id)
    })
}

/// `elicitationHandler.ts` `registerElicitationHandler`。
///
/// Rust 端把 TS 中 SDK `client.setRequestHandler` 的副作用归还给调用方：
/// 我们只校验客户端 capabilities 是否包含 elicitation；返回值 true 表示
/// 调用方应当继续注册 (Rust 实际的 setRequestHandler 在 client.rs 里)。
pub fn register_elicitation_handler(client_capabilities: &JsonValue) -> bool {
    let elicit_cap = client_capabilities.get("elicitation").or_else(|| {
        client_capabilities
            .get("experimental")
            .and_then(|e| e.get("elicitation"))
    });
    matches!(elicit_cap, Some(v) if !v.is_null() && !matches!(v, JsonValue::Bool(false)))
}

/// `elicitationHandler.ts` `runElicitationHooks`。
///
/// Rust 端通过 `hook_runner` 注入 hook 调度，返回值可能给定提前响应或
/// 表明阻塞错误（导致 `decline`）。
pub async fn run_elicitation_hooks<F, Fut>(
    server_name: &str,
    params: &ElicitRequestParams,
    hook_runner: F,
) -> Option<ElicitResult>
where
    F: FnOnce(ElicitationHookInput) -> Fut,
    Fut: std::future::Future<Output = ElicitationHookOutput>,
{
    let mode = get_elicitation_mode(params).to_string();
    let result = hook_runner(ElicitationHookInput {
        server_name: server_name.to_string(),
        message: params.message.clone(),
        requested_schema: params.requested_schema.clone(),
        mode,
        url: params.url.clone(),
        elicitation_id: params.elicitation_id.clone(),
    })
    .await;
    if result.blocking_error {
        return Some(ElicitResult {
            action: "decline".into(),
            content: None,
        });
    }
    result.response.map(|r| ElicitResult {
        action: r.action,
        content: r.content,
    })
}

#[derive(Debug, Clone)]
pub struct ElicitationHookInput {
    pub server_name: String,
    pub message: String,
    pub requested_schema: Option<JsonValue>,
    pub mode: String,
    pub url: Option<String>,
    pub elicitation_id: Option<String>,
}

#[derive(Debug, Default, Clone)]
pub struct ElicitationHookOutput {
    pub response: Option<ElicitResult>,
    pub blocking_error: bool,
}

/// `elicitationHandler.ts` `runElicitationResultHooks`。
pub async fn run_elicitation_result_hooks<F, Fut>(
    server_name: &str,
    result: ElicitResult,
    mode: Option<&str>,
    elicitation_id: Option<&str>,
    hook_runner: F,
) -> ElicitResult
where
    F: FnOnce(ElicitationResultHookInput) -> Fut,
    Fut: std::future::Future<Output = ElicitationResultHookOutput>,
{
    let input = ElicitationResultHookInput {
        server_name: server_name.to_string(),
        action: result.action.clone(),
        content: result.content.clone(),
        mode: mode.map(|s| s.to_string()),
        elicitation_id: elicitation_id.map(|s| s.to_string()),
    };
    let hook = hook_runner(input).await;
    if hook.blocking_error {
        return ElicitResult {
            action: "decline".into(),
            content: None,
        };
    }
    if let Some(response) = hook.response {
        return ElicitResult {
            action: response.action,
            content: response.content.or(result.content),
        };
    }
    result
}

#[derive(Debug, Clone)]
pub struct ElicitationResultHookInput {
    pub server_name: String,
    pub action: String,
    pub content: Option<JsonValue>,
    pub mode: Option<String>,
    pub elicitation_id: Option<String>,
}

#[derive(Debug, Default, Clone)]
pub struct ElicitationResultHookOutput {
    pub response: Option<ElicitResult>,
    pub blocking_error: bool,
}
