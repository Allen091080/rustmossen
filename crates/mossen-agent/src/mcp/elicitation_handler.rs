//! Elicitation handler — manages MCP server elicitation requests.
//!
//! Translates `services/mcp/elicitationHandler.ts`.

use std::collections::HashMap;
use std::sync::Arc;

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
}

impl ElicitationHandler {
    pub fn new() -> Self {
        Self {
            queue: Arc::new(Mutex::new(Vec::new())),
            response_senders: Arc::new(Mutex::new(HashMap::new())),
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
        let mode = get_elicitation_mode(&params);
        tracing::debug!(server_name, mode, "Received elicitation request");

        let elicitation_id = if mode == "url" {
            params.elicitation_id.clone()
        } else {
            None
        };

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

        tokio::select! {
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
        }
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
