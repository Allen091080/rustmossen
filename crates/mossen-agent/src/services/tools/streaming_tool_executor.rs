//! Streaming tool executor — executes tools during streaming responses.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::VecDeque;
use tokio::sync::mpsc;
use tracing::debug;

use super::tool_execution::{
    execute_tool, ToolExecutionContext, ToolExecutionRequest, ToolExecutionResult,
    ToolPermissionChecker,
};

/// A tool use block from a streaming response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamingToolUse {
    pub id: String,
    pub name: String,
    pub input: Value,
}

/// Event emitted by the streaming tool executor.
#[derive(Debug, Clone)]
pub enum StreamingToolEvent {
    /// Tool execution started.
    Started {
        tool_use_id: String,
        tool_name: String,
    },
    /// Tool execution completed.
    Completed(ToolExecutionResult),
    /// All queued tools have been executed.
    AllComplete,
}

/// Streaming tool executor that processes tool uses as they arrive from the API stream.
pub struct StreamingToolExecutor {
    context: ToolExecutionContext,
    queue: VecDeque<StreamingToolUse>,
    results: Vec<ToolExecutionResult>,
    event_tx: mpsc::UnboundedSender<StreamingToolEvent>,
}

impl StreamingToolExecutor {
    pub fn new(
        context: ToolExecutionContext,
        event_tx: mpsc::UnboundedSender<StreamingToolEvent>,
    ) -> Self {
        Self {
            context,
            queue: VecDeque::new(),
            results: Vec::new(),
            event_tx,
        }
    }

    /// Enqueue a tool use for execution.
    pub fn enqueue(&mut self, tool_use: StreamingToolUse) {
        self.queue.push_back(tool_use);
    }

    /// Execute all queued tools sequentially.
    pub async fn execute_all(
        &mut self,
        permission_checker: &dyn ToolPermissionChecker,
    ) -> Vec<ToolExecutionResult> {
        while let Some(tool_use) = self.queue.pop_front() {
            let request = ToolExecutionRequest {
                tool_name: tool_use.name.clone(),
                tool_use_id: tool_use.id.clone(),
                input: tool_use.input.clone(),
                is_from_streaming: true,
            };

            let _ = self.event_tx.send(StreamingToolEvent::Started {
                tool_use_id: tool_use.id.clone(),
                tool_name: tool_use.name.clone(),
            });

            let result = execute_tool(&request, &self.context, permission_checker).await;
            match result {
                Ok(r) => {
                    let _ = self.event_tx.send(StreamingToolEvent::Completed(r.clone()));
                    self.results.push(r);
                }
                Err(e) => {
                    let error_result = ToolExecutionResult {
                        tool_use_id: tool_use.id.clone(),
                        content: format!("Error: {}", e),
                        is_error: true,
                        duration_ms: 0,
                        metadata: std::collections::HashMap::new(),
                    };
                    let _ = self
                        .event_tx
                        .send(StreamingToolEvent::Completed(error_result.clone()));
                    self.results.push(error_result);
                }
            }
        }

        let _ = self.event_tx.send(StreamingToolEvent::AllComplete);
        std::mem::take(&mut self.results)
    }

    /// Get results collected so far.
    pub fn results(&self) -> &[ToolExecutionResult] {
        &self.results
    }

    /// Check if there are pending tools in the queue.
    pub fn has_pending(&self) -> bool {
        !self.queue.is_empty()
    }
}
