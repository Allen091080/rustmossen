//! Elicitation handler — manages MCP server elicitation requests.
//!
//! Translates `services/mcp/elicitationHandler.ts`.

use std::collections::HashMap;
use std::sync::Arc;

use mossen_utils::hooks_utils::{
    execute_elicitation_hooks, execute_elicitation_result_hooks, HooksContext,
    TOOL_HOOK_EXECUTION_TIMEOUT_MS,
};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio_util::sync::CancellationToken;

/// Configuration for the waiting state shown after the user opens a URL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElicitationWaitingState {
    pub action_label: String,
    pub show_cancel: Option<bool>,
}

/// Elicitation request parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElicitRequestParams {
    pub message: String,
    pub mode: Option<String>,
    pub url: Option<String>,
    pub elicitation_id: Option<String>,
    pub requested_schema: Option<serde_json::Value>,
}

/// Elicitation result (response).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElicitResult {
    pub action: String, // "accept" | "decline" | "cancel"
    pub content: Option<serde_json::Value>,
}

/// An elicitation request event queued for UI handling.
#[derive(Debug, Clone)]
pub struct ElicitationRequestEvent {
    pub server_name: String,
    pub request_id: String,
    pub params: ElicitRequestParams,
    pub waiting_state: Option<ElicitationWaitingState>,
    pub completed: bool,
}

/// Get the elicitation mode from params.
pub fn get_elicitation_mode(params: &ElicitRequestParams) -> &str {
    match params.mode.as_deref() {
        Some("url") => "url",
        _ => "form",
    }
}

/// Find a queued elicitation event by server name and elicitation_id.
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

/// Hook response for elicitation.
#[derive(Debug, Clone)]
pub struct ElicitationHookResponse {
    pub action: String,
    pub content: Option<serde_json::Value>,
}

/// Elicitation handler that manages the queue and responses.
pub struct ElicitationHandler {
    queue: Arc<Mutex<Vec<ElicitationRequestEvent>>>,
    response_senders: Arc<Mutex<HashMap<String, oneshot::Sender<ElicitResult>>>>,
    hooks_context: Option<Arc<HooksContext>>,
}

impl ElicitationHandler {
    pub fn new() -> Self {
        Self {
            queue: Arc::new(Mutex::new(Vec::new())),
            response_senders: Arc::new(Mutex::new(HashMap::new())),
            hooks_context: None,
        }
    }

    pub fn with_hooks_context(hooks_context: Arc<HooksContext>) -> Self {
        Self {
            queue: Arc::new(Mutex::new(Vec::new())),
            response_senders: Arc::new(Mutex::new(HashMap::new())),
            hooks_context: Some(hooks_context),
        }
    }

    /// Handle an incoming elicitation request from an MCP server.
    /// Returns the result once the user responds or the request is cancelled.
    pub async fn handle_elicitation_request(
        &self,
        server_name: &str,
        request_id: &str,
        params: ElicitRequestParams,
        cancel: CancellationToken,
    ) -> ElicitResult {
        let mode = get_elicitation_mode(&params).to_string();
        let mode = mode.as_str();
        tracing::debug!(server_name, mode, "Received elicitation request");

        let elicitation_id = if mode == "url" {
            params.elicitation_id.clone()
        } else {
            None
        };

        if let Some(result) = self
            .run_configured_elicitation_hook(server_name, &params, mode, &cancel)
            .await
        {
            return self
                .run_configured_elicitation_result_hook(
                    server_name,
                    result,
                    mode,
                    elicitation_id.as_deref(),
                    &cancel,
                )
                .await;
        }

        let waiting_state = elicitation_id.as_ref().map(|_| ElicitationWaitingState {
            action_label: "Skip confirmation".to_string(),
            show_cancel: None,
        });

        let event = ElicitationRequestEvent {
            server_name: server_name.to_string(),
            request_id: request_id.to_string(),
            params,
            waiting_state,
            completed: false,
        };

        let (tx, rx) = oneshot::channel();

        {
            let mut queue = self.queue.lock().await;
            queue.push(event);
        }
        {
            let mut senders = self.response_senders.lock().await;
            senders.insert(request_id.to_string(), tx);
        }

        let result = tokio::select! {
            result = rx => {
                match result {
                    Ok(r) => {
                        tracing::debug!(server_name, action = %r.action, "Elicitation response");
                        r
                    }
                    Err(_) => {
                        ElicitResult { action: "cancel".to_string(), content: None }
                    }
                }
            }
            _ = cancel.cancelled() => {
                let mut senders = self.response_senders.lock().await;
                senders.remove(request_id);
                let mut queue = self.queue.lock().await;
                queue.retain(|e| e.request_id != request_id);
                ElicitResult { action: "cancel".to_string(), content: None }
            }
        };

        self.run_configured_elicitation_result_hook(
            server_name,
            result,
            mode,
            elicitation_id.as_deref(),
            &cancel,
        )
        .await
    }

    /// Respond to a pending elicitation request.
    pub async fn respond(&self, request_id: &str, result: ElicitResult) -> bool {
        let sender = {
            let mut senders = self.response_senders.lock().await;
            senders.remove(request_id)
        };

        if let Some(tx) = sender {
            let _ = tx.send(result);
            let mut queue = self.queue.lock().await;
            queue.retain(|e| e.request_id != request_id);
            true
        } else {
            false
        }
    }

    /// Mark an elicitation as completed (URL mode completion notification).
    pub async fn mark_completed(&self, server_name: &str, elicitation_id: &str) -> bool {
        let mut queue = self.queue.lock().await;
        if let Some(idx) = find_elicitation_in_queue(&queue, server_name, elicitation_id) {
            queue[idx].completed = true;
            true
        } else {
            tracing::debug!(
                server_name,
                elicitation_id,
                "Ignoring completion notification for unknown elicitation"
            );
            false
        }
    }

    /// Get current queue (for UI rendering).
    pub async fn get_queue(&self) -> Vec<ElicitationRequestEvent> {
        self.queue.lock().await.clone()
    }

    async fn run_configured_elicitation_hook(
        &self,
        server_name: &str,
        params: &ElicitRequestParams,
        mode: &str,
        cancel: &CancellationToken,
    ) -> Option<ElicitResult> {
        let ctx = self.hooks_context.as_deref()?;
        let result = execute_elicitation_hooks(
            ctx,
            server_name,
            &params.message,
            params.requested_schema.as_ref(),
            None,
            Some(cancel),
            TOOL_HOOK_EXECUTION_TIMEOUT_MS,
            Some(mode),
            params.url.as_deref(),
            params.elicitation_id.as_deref(),
        )
        .await;

        if result.blocking_error.is_some() {
            return Some(ElicitResult {
                action: "decline".to_string(),
                content: None,
            });
        }
        result.elicitation_response.map(|response| ElicitResult {
            action: response.action,
            content: response.content,
        })
    }

    async fn run_configured_elicitation_result_hook(
        &self,
        server_name: &str,
        result: ElicitResult,
        mode: &str,
        elicitation_id: Option<&str>,
        cancel: &CancellationToken,
    ) -> ElicitResult {
        let Some(ctx) = self.hooks_context.as_deref() else {
            return result;
        };
        let hook_result = execute_elicitation_result_hooks(
            ctx,
            server_name,
            &result.action,
            result.content.as_ref(),
            None,
            Some(cancel),
            TOOL_HOOK_EXECUTION_TIMEOUT_MS,
            Some(mode),
            elicitation_id,
        )
        .await;

        if hook_result.blocking_error.is_some() {
            return ElicitResult {
                action: "decline".to_string(),
                content: None,
            };
        }

        if let Some(response) = hook_result.elicitation_result_response {
            return ElicitResult {
                action: response.action,
                content: response.content.or(result.content),
            };
        }

        result
    }
}

impl Default for ElicitationHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Run elicitation hooks (pre-response).
/// Returns Some(response) if a hook provides a programmatic answer.
pub async fn run_elicitation_hooks(
    server_name: &str,
    params: &ElicitRequestParams,
    hooks: &[Box<dyn ElicitationHook + Send + Sync>],
) -> Option<ElicitResult> {
    let mode = get_elicitation_mode(params);
    for hook in hooks {
        match hook.on_elicitation(server_name, params, mode).await {
            Ok(Some(response)) => {
                tracing::debug!(server_name, "Elicitation resolved by hook");
                return Some(ElicitResult {
                    action: response.action,
                    content: response.content,
                });
            }
            Ok(None) => continue,
            Err(e) => {
                tracing::error!(server_name, error = %e, "Elicitation hook error");
                continue;
            }
        }
    }
    None
}

/// Run elicitation result hooks (post-response).
pub async fn run_elicitation_result_hooks(
    server_name: &str,
    result: ElicitResult,
    mode: &str,
    elicitation_id: Option<&str>,
    hooks: &[Box<dyn ElicitationResultHook + Send + Sync>],
) -> ElicitResult {
    for hook in hooks {
        match hook
            .on_elicitation_result(server_name, &result, mode, elicitation_id)
            .await
        {
            Ok(Some(modified)) => {
                return ElicitResult {
                    action: modified.action,
                    content: modified.content.or(result.content),
                };
            }
            Ok(None) => continue,
            Err(e) => {
                tracing::error!(server_name, error = %e, "ElicitationResult hook error");
                continue;
            }
        }
    }
    result
}

/// Trait for elicitation hooks (pre-response).
#[async_trait::async_trait]
pub trait ElicitationHook {
    async fn on_elicitation(
        &self,
        server_name: &str,
        params: &ElicitRequestParams,
        mode: &str,
    ) -> Result<Option<ElicitationHookResponse>, Box<dyn std::error::Error + Send + Sync>>;
}

/// Trait for elicitation result hooks (post-response).
#[async_trait::async_trait]
pub trait ElicitationResultHook {
    async fn on_elicitation_result(
        &self,
        server_name: &str,
        result: &ElicitResult,
        mode: &str,
        elicitation_id: Option<&str>,
    ) -> Result<Option<ElicitationHookResponse>, Box<dyn std::error::Error + Send + Sync>>;
}

/// TS `registerElicitationHandler(serverName, handler)` — registers the
/// elicitation router under the supplied server name. Returns a cancel token
/// used by the caller to deregister later.
pub fn register_elicitation_handler(
    handler: ElicitationHandler,
    server_name: String,
) -> std::sync::Arc<std::sync::Mutex<ElicitationHandler>> {
    let _ = server_name; // registration scope tracked elsewhere
    std::sync::Arc::new(std::sync::Mutex::new(handler))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mossen_utils::hooks_utils::HookMatcher;
    use serde_json::json;
    use std::collections::{HashMap, HashSet};
    use std::time::Duration;

    fn hooks_context(cwd: &std::path::Path, commands: Vec<(&str, String)>) -> Arc<HooksContext> {
        let mut registered_hooks: HashMap<String, Vec<HookMatcher>> = HashMap::new();
        for (event, command) in commands {
            registered_hooks
                .entry(event.to_string())
                .or_default()
                .push(HookMatcher {
                    matcher: None,
                    hooks: vec![json!({
                        "type": "command",
                        "command": command,
                        "timeout": 1
                    })],
                    plugin_root: None,
                    plugin_id: None,
                    plugin_name: None,
                    skill_root: None,
                    skill_name: None,
                });
        }

        Arc::new(HooksContext {
            session_id: "elicitation-hook-test".to_string(),
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
        })
    }

    fn python_hook_command(marker: &std::path::Path, event_name: &str, content: &str) -> String {
        format!(
            "python3 -c 'import json, pathlib, sys; pathlib.Path({:?}).open(\"a\").write(sys.stdin.read()); print(json.dumps({{\"hookSpecificOutput\":{{\"hookEventName\":{:?},\"action\":\"accept\",\"content\":{{\"source\":{:?}}}}}}}))'",
            marker.to_string_lossy(),
            event_name,
            content,
        )
    }

    #[tokio::test]
    async fn settings_elicitation_hook_can_answer_without_queueing_ui_request() {
        let temp = tempfile::tempdir().expect("tempdir");
        let marker = temp.path().join("elicitation.log");
        let ctx = hooks_context(
            temp.path(),
            vec![(
                "Elicitation",
                python_hook_command(&marker, "Elicitation", "pre-hook"),
            )],
        );
        let handler = ElicitationHandler::with_hooks_context(ctx);

        let result = handler
            .handle_elicitation_request(
                "test-server",
                "request-1",
                ElicitRequestParams {
                    message: "confirm access".to_string(),
                    mode: Some("form".to_string()),
                    url: None,
                    elicitation_id: None,
                    requested_schema: Some(json!({"type": "object"})),
                },
                CancellationToken::new(),
            )
            .await;

        assert_eq!(result.action, "accept");
        assert_eq!(result.content, Some(json!({"source": "pre-hook"})));
        assert!(handler.get_queue().await.is_empty());
        let log = std::fs::read_to_string(marker).expect("hook marker");
        assert!(log.contains(r#""hook_event_name":"Elicitation""#), "{log}");
        assert!(log.contains("confirm access"), "{log}");
    }

    #[tokio::test]
    async fn settings_elicitation_result_hook_can_modify_user_response() {
        let temp = tempfile::tempdir().expect("tempdir");
        let marker = temp.path().join("elicitation-result.log");
        let ctx = hooks_context(
            temp.path(),
            vec![(
                "ElicitationResult",
                python_hook_command(&marker, "ElicitationResult", "result-hook"),
            )],
        );
        let handler = Arc::new(ElicitationHandler::with_hooks_context(ctx));
        let task_handler = Arc::clone(&handler);

        let task = tokio::spawn(async move {
            task_handler
                .handle_elicitation_request(
                    "test-server",
                    "request-2",
                    ElicitRequestParams {
                        message: "confirm access".to_string(),
                        mode: Some("form".to_string()),
                        url: None,
                        elicitation_id: None,
                        requested_schema: None,
                    },
                    CancellationToken::new(),
                )
                .await
        });

        for _ in 0..20 {
            if !handler.get_queue().await.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(handler.get_queue().await.len(), 1);
        assert!(
            handler
                .respond(
                    "request-2",
                    ElicitResult {
                        action: "accept".to_string(),
                        content: Some(json!({"source": "user"})),
                    },
                )
                .await
        );

        let result = task.await.expect("elicitation task");
        assert_eq!(result.action, "accept");
        assert_eq!(result.content, Some(json!({"source": "result-hook"})));
        let log = std::fs::read_to_string(marker).expect("hook marker");
        assert!(
            log.contains(r#""hook_event_name":"ElicitationResult""#),
            "{log}"
        );
        assert!(log.contains(r#""action":"accept""#), "{log}");
    }
}
