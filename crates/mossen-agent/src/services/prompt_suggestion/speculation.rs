//! Speculation engine — speculative execution of predicted user intents.
//!
//! Runs a forked agent on the predicted next user message while the user
//! is still typing/thinking. If the prediction matches the actual input,
//! the pre-computed result is shown instantly.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, Notify};
use tracing::{debug, info, warn};

const MAX_SPECULATION_TURNS: usize = 20;
const MAX_SPECULATION_MESSAGES: usize = 100;

/// Tools that perform write operations (speculation must be careful with these).
fn is_write_tool(name: &str) -> bool {
    matches!(name, "Edit" | "Write" | "NotebookEdit")
}

/// Tools that are safe read-only operations.
fn is_safe_read_only_tool(name: &str) -> bool {
    matches!(name, "Read" | "Glob" | "Grep" | "ToolSearch" | "LSP" | "TaskGet" | "TaskList")
}

/// Speculation state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpeculationStatus {
    Idle,
    Running,
    Completed,
    Aborted,
    Failed,
}

/// Result of a speculation run.
#[derive(Debug, Clone)]
pub struct SpeculationResult {
    pub id: String,
    pub predicted_input: String,
    pub messages: Vec<serde_json::Value>,
    pub status: SpeculationStatus,
    pub turns_used: usize,
    pub duration_ms: u64,
    pub file_changes: Vec<FileChange>,
    pub overlay_path: Option<PathBuf>,
}

/// A file change produced during speculation.
#[derive(Debug, Clone)]
pub struct FileChange {
    pub path: PathBuf,
    pub kind: FileChangeKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileChangeKind {
    Created,
    Modified,
    Deleted,
}

/// Decision for whether speculation should proceed.
#[derive(Debug, Clone)]
pub enum SpeculationDecision {
    Allow {
        predicted_input: String,
    },
    Deny {
        message: String,
        reason: String,
    },
}

/// Check if speculation is enabled.
pub fn is_speculation_enabled() -> bool {
    if let Ok(val) = std::env::var("MOSSEN_CODE_ENABLE_SPECULATION") {
        if val == "0" || val.eq_ignore_ascii_case("false") {
            return false;
        }
    }
    // Default: disabled (opt-in feature)
    false
}

/// Configuration for a speculation run.
#[derive(Debug, Clone)]
pub struct SpeculationConfig {
    pub max_turns: usize,
    pub max_messages: usize,
    pub timeout: Duration,
    pub overlay_dir: PathBuf,
}

impl Default for SpeculationConfig {
    fn default() -> Self {
        Self {
            max_turns: MAX_SPECULATION_TURNS,
            max_messages: MAX_SPECULATION_MESSAGES,
            timeout: Duration::from_secs(60),
            overlay_dir: PathBuf::from("/tmp/mossen/speculation"),
        }
    }
}

/// Speculation engine that manages speculative execution.
pub struct SpeculationEngine {
    config: SpeculationConfig,
    current_speculation: Arc<Mutex<Option<SpeculationResult>>>,
    abort_notify: Arc<Notify>,
}

impl SpeculationEngine {
    pub fn new(config: SpeculationConfig) -> Self {
        Self {
            config,
            current_speculation: Arc::new(Mutex::new(None)),
            abort_notify: Arc::new(Notify::new()),
        }
    }

    /// Start a speculation run with the given predicted input.
    pub async fn start_speculation(
        &self,
        predicted_input: &str,
        messages: &[serde_json::Value],
    ) -> Result<String, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let overlay_path = self.config.overlay_dir.join(&id);

        // Create overlay directory
        tokio::fs::create_dir_all(&overlay_path)
            .await
            .map_err(|e| format!("Failed to create overlay dir: {}", e))?;

        let result = SpeculationResult {
            id: id.clone(),
            predicted_input: predicted_input.to_string(),
            messages: Vec::new(),
            status: SpeculationStatus::Running,
            turns_used: 0,
            duration_ms: 0,
            file_changes: Vec::new(),
            overlay_path: Some(overlay_path),
        };

        let mut current = self.current_speculation.lock().await;
        *current = Some(result);

        Ok(id)
    }

    /// Abort the current speculation.
    pub async fn abort(&self) {
        self.abort_notify.notify_one();
        let mut current = self.current_speculation.lock().await;
        if let Some(ref mut spec) = *current {
            spec.status = SpeculationStatus::Aborted;
            // Clean up overlay
            if let Some(ref path) = spec.overlay_path {
                let _ = tokio::fs::remove_dir_all(path).await;
            }
        }
    }

    /// Get the current speculation result.
    pub async fn get_current(&self) -> Option<SpeculationResult> {
        self.current_speculation.lock().await.clone()
    }

    /// Accept the speculation result if the actual input matches.
    pub async fn try_accept(
        &self,
        actual_input: &str,
        match_threshold: f64,
    ) -> Option<SpeculationResult> {
        let current = self.current_speculation.lock().await;
        if let Some(ref spec) = *current {
            if spec.status == SpeculationStatus::Completed {
                let similarity = compute_similarity(&spec.predicted_input, actual_input);
                if similarity >= match_threshold {
                    return Some(spec.clone());
                }
            }
        }
        None
    }

    /// Apply file changes from an accepted speculation to the main filesystem.
    pub async fn apply_overlay(&self, speculation_id: &str) -> Result<Vec<FileChange>, String> {
        let current = self.current_speculation.lock().await;
        if let Some(ref spec) = *current {
            if spec.id == speculation_id {
                if let Some(ref overlay_path) = spec.overlay_path {
                    return apply_overlay_files(overlay_path).await;
                }
            }
        }
        Err("Speculation not found or no overlay".to_string())
    }

    /// Clean up speculation state.
    pub async fn cleanup(&self) {
        let mut current = self.current_speculation.lock().await;
        if let Some(ref spec) = *current {
            if let Some(ref path) = spec.overlay_path {
                let _ = tokio::fs::remove_dir_all(path).await;
            }
        }
        *current = None;
    }
}

/// Compute string similarity (simple Jaccard on words).
fn compute_similarity(a: &str, b: &str) -> f64 {
    let words_a: HashSet<&str> = a.split_whitespace().collect();
    let words_b: HashSet<&str> = b.split_whitespace().collect();

    if words_a.is_empty() && words_b.is_empty() {
        return 1.0;
    }
    if words_a.is_empty() || words_b.is_empty() {
        return 0.0;
    }

    let intersection = words_a.intersection(&words_b).count();
    let union = words_a.union(&words_b).count();

    intersection as f64 / union as f64
}

/// Apply overlay files from the speculation directory to the main filesystem.
async fn apply_overlay_files(overlay_path: &Path) -> Result<Vec<FileChange>, String> {
    let mut changes = Vec::new();
    let mut read_dir = tokio::fs::read_dir(overlay_path)
        .await
        .map_err(|e| format!("Failed to read overlay dir: {}", e))?;

    while let Ok(Some(entry)) = read_dir.next_entry().await {
        let path = entry.path();
        if path.is_file() {
            let content = tokio::fs::read(&path)
                .await
                .map_err(|e| format!("Failed to read overlay file: {}", e))?;

            let file_name = path.file_name().unwrap_or_default().to_string_lossy();
            // Decode the original path from the overlay filename
            let original_path = PathBuf::from(
                urlencoding::decode(&file_name).unwrap_or_default().to_string(),
            );

            let kind = if original_path.exists() {
                FileChangeKind::Modified
            } else {
                FileChangeKind::Created
            };

            tokio::fs::write(&original_path, &content)
                .await
                .map_err(|e| format!("Failed to apply overlay: {}", e))?;

            changes.push(FileChange {
                path: original_path,
                kind,
            });
        }
    }

    // Clean up overlay directory
    let _ = tokio::fs::remove_dir_all(overlay_path).await;

    Ok(changes)
}

/// Validate whether a tool use is safe for speculation.
pub fn validate_speculation_tool_use(
    tool_name: &str,
    tool_input: &serde_json::Value,
) -> Result<(), String> {
    if is_safe_read_only_tool(tool_name) {
        return Ok(());
    }

    if is_write_tool(tool_name) {
        // Write tools are allowed in speculation (writes go to overlay)
        return Ok(());
    }

    if tool_name == "Bash" {
        // Only allow read-only bash commands in speculation
        let command = tool_input
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if command_is_read_only(command) {
            return Ok(());
        }
        return Err(format!(
            "Bash command '{}' is not safe for speculation",
            command
        ));
    }

    Err(format!("Tool '{}' is not allowed in speculation", tool_name))
}

/// Check if a bash command is read-only (heuristic).
fn command_is_read_only(command: &str) -> bool {
    let read_only_prefixes = [
        "ls", "cat", "head", "tail", "wc", "find", "grep", "stat", "file",
        "pwd", "echo", "date", "whoami", "uname", "which", "type",
    ];

    let first_word = command.split_whitespace().next().unwrap_or("");
    read_only_prefixes.iter().any(|&prefix| first_word == prefix)
}

// ---------------------------------------------------------------------------
// TS-mirror — `services/PromptSuggestion/speculation.ts` exports.
// ---------------------------------------------------------------------------

/// `speculation.ts` `prepareMessagesForInjection`.
pub fn prepare_messages_for_injection(
    messages: Vec<serde_json::Value>,
) -> Vec<serde_json::Value> {
    messages
        .into_iter()
        .filter(|m| {
            m.get("type").and_then(|v| v.as_str()) != Some("speculation_placeholder")
        })
        .collect()
}

/// `speculation.ts` `acceptSpeculation`.
pub async fn accept_speculation(state_id: &str) -> bool {
    let _ = state_id;
    true
}

/// `speculation.ts` `abortSpeculation`.
pub fn abort_speculation(state_id: &str) {
    let _ = state_id;
}

/// `speculation.ts` `handleSpeculationAccept`.
pub async fn handle_speculation_accept(state_id: &str) -> bool {
    accept_speculation(state_id).await
}

/// Active speculation tracking state. Mirrors TS `ActiveSpeculationState`.
#[derive(Debug, Clone)]
pub struct ActiveSpeculationState {
    pub speculation_id: String,
    pub started_at_ms: i64,
    pub status: SpeculationStatus,
    pub last_event_ms: Option<i64>,
    pub queued_inputs: Vec<String>,
}
