//! Agent summary — periodic background summarization for coordinator mode sub-agents.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

const SUMMARY_INTERVAL_MS: u64 = 30_000;

/// Build the summary prompt for the forked agent.
fn build_summary_prompt(previous_summary: Option<&str>) -> String {
    let prev_line = match previous_summary {
        Some(s) => format!("\nPrevious: \"{}\" — say something NEW.\n", s),
        None => String::new(),
    };

    format!(
        r#"Describe your most recent action in 3-5 words using present tense (-ing). Name the file or function, not the branch. Do not use tools.
{}
Good: "Reading runAgent.ts"
Good: "Fixing null check in validate.ts"
Good: "Running auth module tests"
Good: "Adding retry logic to fetchUser"

Bad (past tense): "Analyzed the branch diff"
Bad (too vague): "Investigating the issue"
Bad (too long): "Reviewing full branch diff and AgentTool.tsx integration"
Bad (branch name): "Analyzed adam/background-summary branch diff""#,
        prev_line
    )
}

/// Callback trait for running the forked agent summary.
#[async_trait::async_trait]
pub trait AgentSummaryRunner: Send + Sync {
    /// Get the current transcript message count for the agent.
    async fn get_transcript_message_count(&self, agent_id: &str) -> usize;

    /// Run a forked agent to generate a summary. Returns the summary text if successful.
    async fn run_forked_summary(
        &self,
        agent_id: &str,
        prompt: &str,
    ) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>>;

    /// Update the agent summary in the UI/state.
    fn update_summary(&self, task_id: &str, summary: &str);
}

/// Handle for the running summarization loop.
pub struct AgentSummarizationHandle {
    stopped: Arc<AtomicBool>,
    task: Option<JoinHandle<()>>,
}

impl AgentSummarizationHandle {
    /// Stop the summarization loop.
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

impl Drop for AgentSummarizationHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Start periodic agent summarization.
pub fn start_agent_summarization(
    task_id: String,
    agent_id: String,
    runner: Arc<dyn AgentSummaryRunner>,
) -> AgentSummarizationHandle {
    let stopped = Arc::new(AtomicBool::new(false));
    let stopped_clone = Arc::clone(&stopped);
    let previous_summary: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    let task = tokio::spawn(async move {
        loop {
            if stopped_clone.load(Ordering::SeqCst) {
                break;
            }

            tokio::time::sleep(Duration::from_millis(SUMMARY_INTERVAL_MS)).await;

            if stopped_clone.load(Ordering::SeqCst) {
                break;
            }

            tracing::debug!("[AgentSummary] Timer fired for agent {}", agent_id);

            let msg_count = runner.get_transcript_message_count(&agent_id).await;
            if msg_count < 3 {
                tracing::debug!(
                    "[AgentSummary] Skipping summary for {}: not enough messages ({})",
                    task_id,
                    msg_count
                );
                continue;
            }

            let prev = previous_summary.lock().await.clone();
            let prompt = build_summary_prompt(prev.as_deref());

            match runner.run_forked_summary(&agent_id, &prompt).await {
                Ok(Some(summary_text)) => {
                    tracing::debug!(
                        "[AgentSummary] Summary result for {}: {}",
                        task_id,
                        summary_text
                    );
                    *previous_summary.lock().await = Some(summary_text.clone());
                    runner.update_summary(&task_id, &summary_text);
                }
                Ok(None) => {
                    tracing::debug!("[AgentSummary] No summary text for {}", task_id);
                }
                Err(e) => {
                    if !stopped_clone.load(Ordering::SeqCst) {
                        tracing::error!("[AgentSummary] Error for {}: {}", task_id, e);
                    }
                }
            }
        }
    });

    AgentSummarizationHandle {
        stopped,
        task: Some(task),
    }
}
