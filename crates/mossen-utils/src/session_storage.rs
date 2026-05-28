//! # session_storage — 会话持久化工具库
//!
//! 对应 TypeScript `utils/sessionStorage.ts`。
//! 提供 transcript 读写、去重、会话管理等功能。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, Result};
use chrono::Utc;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::Mutex as AsyncMutex;
use tracing;

use crate::string_utils::truncate_chars;
use mossen_types::logs::{
    AttributionSnapshotMessage, ContextCollapseCommitEntry, ContextCollapseSnapshotEntry,
    FileHistorySnapshotMessage, LogOption, PersistedWorktreeSession, SessionMode,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// 50 MB — prevents OOM in the tombstone slow path which reads + rewrites the
/// entire session file.
const MAX_TOMBSTONE_REWRITE_BYTES: u64 = 50 * 1024 * 1024;

/// 50 MB — session JSONL can grow to multiple GB. Callers that read the raw
/// transcript must bail out above this threshold to avoid OOM.
pub const MAX_TRANSCRIPT_READ_BYTES: u64 = 50 * 1024 * 1024;

/// Remote flush interval in milliseconds.
const REMOTE_FLUSH_INTERVAL_MS: u64 = 10;

/// Number of sessions to enrich on the initial load of the resume picker.
const INITIAL_ENRICH_COUNT: usize = 50;

/// Size of the lite read buffer (64KB).
const LITE_READ_BUF_SIZE: usize = 64 * 1024;

/// Threshold for skipping pre-compact content (5MB).
const SKIP_PRECOMPACT_THRESHOLD: u64 = 5 * 1024 * 1024;

/// Pre-compiled regex to skip non-meaningful messages when extracting first prompt.
static SKIP_FIRST_PROMPT_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(?:\s*<[a-z][\w-]*[\s>]|\[Request interrupted by user[^\]]*\])").unwrap()
});

/// Ephemeral progress types that are UI-only.
static EPHEMERAL_PROGRESS_TYPES: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    let mut s = HashSet::new();
    s.insert("bash_progress");
    s.insert("powershell_progress");
    s.insert("mcp_progress");
    s
});

/// Metadata entry types that appear before a compact boundary.
const METADATA_TYPE_MARKERS: &[&str] = &[
    "\"type\":\"summary\"",
    "\"type\":\"custom-title\"",
    "\"type\":\"tag\"",
    "\"type\":\"agent-name\"",
    "\"type\":\"agent-color\"",
    "\"type\":\"agent-setting\"",
    "\"type\":\"mode\"",
    "\"type\":\"worktree-state\"",
    "\"type\":\"pr-link\"",
];

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Transcript type alias.
pub type Transcript = Vec<Value>;

/// Agent metadata persisted as a sidecar file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentMetadata {
    pub agent_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Remote agent metadata for CCR tasks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteAgentMetadata {
    pub task_id: String,
    pub remote_task_type: String,
    pub session_id: String,
    pub title: String,
    pub command: String,
    pub spawned_at: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_use_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_long_running: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_ultraplan: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_remote_review: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_task_metadata: Option<HashMap<String, Value>>,
}

/// Team information for message chains.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
}

/// Result of loading session logs with progressive enrichment support.
#[derive(Debug, Clone)]
pub struct SessionLogResult {
    pub logs: Vec<LogOption>,
    pub all_stat_logs: Vec<LogOption>,
    pub next_index: usize,
}

/// Lite metadata extracted from head/tail of JSONL file.
#[derive(Debug, Clone, Default)]
pub struct LiteMetadata {
    pub first_prompt: String,
    pub git_branch: Option<String>,
    pub is_sidechain: bool,
    pub project_path: Option<String>,
    pub team_name: Option<String>,
    pub custom_title: Option<String>,
    pub summary: Option<String>,
    pub tag: Option<String>,
    pub agent_setting: Option<String>,
    pub pr_number: Option<u64>,
    pub pr_url: Option<String>,
    pub pr_repository: Option<String>,
}

/// Internal event writer function type.
pub type InternalEventWriter = Arc<
    dyn Fn(
            String,
            HashMap<String, Value>,
            Option<WriteOptions>,
        ) -> futures::future::BoxFuture<'static, Result<()>>
        + Send
        + Sync,
>;

/// Write options for internal event writer.
#[derive(Debug, Clone, Default)]
pub struct WriteOptions {
    pub is_compaction: Option<bool>,
    pub agent_id: Option<String>,
}

/// Internal event reader function type.
pub type InternalEventReader = Arc<
    dyn Fn() -> futures::future::BoxFuture<'static, Result<Option<Vec<InternalEvent>>>>
        + Send
        + Sync,
>;

/// An internal event from CCR v2.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InternalEvent {
    pub payload: HashMap<String, Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
}

/// Context for session operations — injected dependencies.
#[derive(Clone)]
pub struct SessionContext {
    pub config_home_dir: PathBuf,
    pub original_cwd: PathBuf,
    pub session_id: String,
    pub session_project_dir: Option<PathBuf>,
    pub user_type: String,
    pub entrypoint: Option<String>,
    pub version: String,
    pub is_persistence_disabled: bool,
    pub node_env: String,
    pub enable_session_persistence: bool,
    pub skip_prompt_history: bool,
    pub test_enable_persistence: bool,
    pub cleanup_period_days: Option<i32>,
    pub plan_slug_cache: HashMap<String, String>,
    pub built_in_command_names: HashSet<String>,
    pub is_shutting_down: bool,
    pub feature_proactive: bool,
    pub feature_kairos: bool,
    pub feature_pebble_leaf_prune: bool,
    pub disable_precompact_skip: bool,
    pub save_hook_additional_context: bool,
}

/// Result of loading a transcript file.
#[derive(Debug, Clone)]
pub struct LoadTranscriptResult {
    pub messages: HashMap<String, Value>,
    pub summaries: HashMap<String, String>,
    pub custom_titles: HashMap<String, String>,
    pub tags: HashMap<String, String>,
    pub agent_names: HashMap<String, String>,
    pub agent_colors: HashMap<String, String>,
    pub agent_settings: HashMap<String, String>,
    pub pr_numbers: HashMap<String, u64>,
    pub pr_urls: HashMap<String, String>,
    pub pr_repositories: HashMap<String, String>,
    pub modes: HashMap<String, String>,
    pub worktree_states: HashMap<String, Option<PersistedWorktreeSession>>,
    pub file_history_snapshots: HashMap<String, FileHistorySnapshotMessage>,
    pub attribution_snapshots: HashMap<String, AttributionSnapshotMessage>,
    pub content_replacements: HashMap<String, Vec<Value>>,
    pub agent_content_replacements: HashMap<String, Vec<Value>>,
    pub context_collapse_commits: Vec<ContextCollapseCommitEntry>,
    pub context_collapse_snapshot: Option<ContextCollapseSnapshotEntry>,
    pub leaf_uuids: HashSet<String>,
}

// ---------------------------------------------------------------------------
// Project struct (session file management)
// ---------------------------------------------------------------------------

/// Project manages session file writes, buffering, and metadata caching.
pub struct Project {
    pub current_session_tag: Option<String>,
    pub current_session_title: Option<String>,
    pub current_session_agent_name: Option<String>,
    pub current_session_agent_color: Option<String>,
    pub current_session_last_prompt: Option<String>,
    pub current_session_agent_setting: Option<String>,
    pub current_session_mode: Option<String>,
    /// Tri-state: None = never touched, Some(None) = exited worktree, Some(Some(..)) = in worktree.
    pub current_session_worktree: Option<Option<PersistedWorktreeSession>>,
    pub current_session_pr_number: Option<u64>,
    pub current_session_pr_url: Option<String>,
    pub current_session_pr_repository: Option<String>,
    pub session_file: Option<PathBuf>,
    pending_entries: Vec<Value>,
    remote_ingress_url: Option<String>,
    internal_event_writer: Option<InternalEventWriter>,
    internal_event_reader: Option<InternalEventReader>,
    internal_subagent_event_reader: Option<InternalEventReader>,
    pending_write_count: i64,
    flush_resolvers: Vec<tokio::sync::oneshot::Sender<()>>,
    write_queues: HashMap<PathBuf, Vec<(Value, Option<tokio::sync::oneshot::Sender<()>>)>>,
    flush_interval_ms: u64,
    max_chunk_bytes: usize,
    existing_session_files: HashMap<String, PathBuf>,
}

impl Default for Project {
    fn default() -> Self {
        Self::new()
    }
}

impl Project {
    pub fn new() -> Self {
        Self {
            current_session_tag: None,
            current_session_title: None,
            current_session_agent_name: None,
            current_session_agent_color: None,
            current_session_last_prompt: None,
            current_session_agent_setting: None,
            current_session_mode: None,
            current_session_worktree: None,
            current_session_pr_number: None,
            current_session_pr_url: None,
            current_session_pr_repository: None,
            session_file: None,
            pending_entries: Vec::new(),
            remote_ingress_url: None,
            internal_event_writer: None,
            internal_event_reader: None,
            internal_subagent_event_reader: None,
            pending_write_count: 0,
            flush_resolvers: Vec::new(),
            write_queues: HashMap::new(),
            flush_interval_ms: 100,
            max_chunk_bytes: 100 * 1024 * 1024,
            existing_session_files: HashMap::new(),
        }
    }

    /// Reset flush/queue state for testing.
    pub fn reset_flush_state(&mut self) {
        self.pending_write_count = 0;
        self.flush_resolvers.clear();
        self.write_queues.clear();
    }

    /// Reset the session file pointer.
    pub fn reset_session_file(&mut self) {
        self.session_file = None;
        self.pending_entries.clear();
    }

    fn increment_pending_writes(&mut self) {
        self.pending_write_count += 1;
    }

    fn decrement_pending_writes(&mut self) {
        self.pending_write_count -= 1;
        if self.pending_write_count == 0 {
            let resolvers = std::mem::take(&mut self.flush_resolvers);
            for resolve in resolvers {
                let _ = resolve.send(());
            }
        }
    }

    /// Append data to file, creating parent directory if needed.
    async fn append_to_file(file_path: &Path, data: &str) -> Result<()> {
        match tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(file_path)
            .await
        {
            Ok(mut file) => {
                use tokio::io::AsyncWriteExt;
                file.write_all(data.as_bytes()).await?;
                Ok(())
            }
            Err(_) => {
                if let Some(parent) = file_path.parent() {
                    tokio::fs::create_dir_all(parent).await?;
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        let _ = tokio::fs::set_permissions(
                            parent,
                            std::fs::Permissions::from_mode(0o700),
                        )
                        .await;
                    }
                }
                let mut file = tokio::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .mode(0o600)
                    .open(file_path)
                    .await?;
                use tokio::io::AsyncWriteExt;
                file.write_all(data.as_bytes()).await?;
                Ok(())
            }
        }
    }

    /// Drain all write queues to disk.
    async fn drain_write_queue(&mut self) -> Result<()> {
        let queue_keys: Vec<PathBuf> = self.write_queues.keys().cloned().collect();
        for file_path in queue_keys {
            if let Some(queue) = self.write_queues.get_mut(&file_path) {
                if queue.is_empty() {
                    continue;
                }
                let batch: Vec<(Value, Option<tokio::sync::oneshot::Sender<()>>)> =
                    std::mem::take(queue);

                let mut content = String::new();
                let mut resolvers: Vec<Option<tokio::sync::oneshot::Sender<()>>> = Vec::new();

                for (entry, resolve) in batch {
                    let line = match serde_json::to_string(&entry) {
                        Ok(s) => s + "\n",
                        Err(_) => continue,
                    };

                    if content.len() + line.len() >= self.max_chunk_bytes {
                        let _ = Self::append_to_file(&file_path, &content).await;
                        for r in resolvers.drain(..).flatten() {
                            let _ = r.send(());
                        }
                        content.clear();
                    }

                    content.push_str(&line);
                    resolvers.push(resolve);
                }

                if !content.is_empty() {
                    let _ = Self::append_to_file(&file_path, &content).await;
                    for r in resolvers.into_iter().flatten() {
                        let _ = r.send(());
                    }
                }
            }
        }

        // Clean up empty queues
        self.write_queues.retain(|_, q| !q.is_empty());
        Ok(())
    }

    /// Enqueue a write to a file.
    fn enqueue_write(&mut self, file_path: PathBuf, entry: Value) {
        let queue = self.write_queues.entry(file_path).or_default();
        queue.push((entry, None));
    }

    /// Flush all pending writes.
    pub async fn flush(&mut self) -> Result<()> {
        self.drain_write_queue().await?;
        Ok(())
    }

    /// Remove a message from the transcript by UUID.
    pub async fn remove_message_by_uuid(&mut self, target_uuid: &str) -> Result<()> {
        self.increment_pending_writes();
        let result = self.remove_message_by_uuid_inner(target_uuid).await;
        self.decrement_pending_writes();
        result
    }

    async fn remove_message_by_uuid_inner(&self, target_uuid: &str) -> Result<()> {
        let session_file = match &self.session_file {
            Some(f) => f.clone(),
            None => return Ok(()),
        };

        let metadata = match tokio::fs::metadata(&session_file).await {
            Ok(m) => m,
            Err(_) => return Ok(()),
        };
        let file_size = metadata.len();
        if file_size == 0 {
            return Ok(());
        }

        // Try fast path: read tail and locate the line
        let chunk_len = std::cmp::min(file_size, LITE_READ_BUF_SIZE as u64);
        let tail_start = file_size - chunk_len;

        let content = tokio::fs::read(&session_file).await?;
        let tail = &content[tail_start as usize..];

        let needle = format!("\"uuid\":\"{}\"", target_uuid);
        if let Some(match_idx) = tail
            .windows(needle.len())
            .rposition(|w| w == needle.as_bytes())
        {
            let prev_nl = tail[..match_idx].iter().rposition(|&b| b == b'\n');
            if prev_nl.is_some() || tail_start == 0 {
                let line_start = prev_nl.map(|p| p + 1).unwrap_or(0);
                let next_nl = tail[match_idx + needle.len()..]
                    .iter()
                    .position(|&b| b == b'\n');
                let line_end = match next_nl {
                    Some(pos) => match_idx + needle.len() + pos + 1,
                    None => tail.len(),
                };

                let abs_line_start = tail_start as usize + line_start;
                let after = &tail[line_end..];

                let mut new_content = content[..abs_line_start].to_vec();
                new_content.extend_from_slice(after);
                tokio::fs::write(&session_file, &new_content).await?;
                return Ok(());
            }
        }

        // Slow path: target not in tail
        if file_size > MAX_TOMBSTONE_REWRITE_BYTES {
            tracing::warn!(
                "Skipping tombstone removal: session file too large ({})",
                file_size
            );
            return Ok(());
        }

        let content_str = String::from_utf8_lossy(&content);
        let lines: Vec<&str> = content_str
            .split('\n')
            .filter(|line| {
                if line.trim().is_empty() {
                    return true;
                }
                match serde_json::from_str::<Value>(line) {
                    Ok(entry) => entry.get("uuid").and_then(|v| v.as_str()) != Some(target_uuid),
                    Err(_) => true,
                }
            })
            .collect();

        tokio::fs::write(&session_file, lines.join("\n")).await?;
        Ok(())
    }

    /// Check whether persistence should be skipped.
    fn should_skip_persistence(&self, ctx: &SessionContext) -> bool {
        (ctx.node_env == "test" && !ctx.test_enable_persistence)
            || ctx.cleanup_period_days == Some(0)
            || ctx.is_persistence_disabled
            || ctx.skip_prompt_history
    }

    /// Create the session file and flush buffered entries.
    async fn materialize_session_file(&mut self, ctx: &SessionContext) -> Result<()> {
        if self.should_skip_persistence(ctx) {
            return Ok(());
        }
        self.ensure_current_session_file(ctx);
        self.re_append_session_metadata(ctx, false);
        if !self.pending_entries.is_empty() {
            let buffered = std::mem::take(&mut self.pending_entries);
            for entry in buffered {
                self.append_entry(entry, &ctx.session_id, ctx).await?;
            }
        }
        Ok(())
    }

    fn ensure_current_session_file(&mut self, ctx: &SessionContext) -> PathBuf {
        if self.session_file.is_none() {
            self.session_file = Some(get_transcript_path(ctx));
        }
        self.session_file.clone().unwrap()
    }

    async fn get_existing_session_file(
        &mut self,
        session_id: &str,
        ctx: &SessionContext,
    ) -> Option<PathBuf> {
        if let Some(cached) = self.existing_session_files.get(session_id) {
            return Some(cached.clone());
        }
        let target_file = get_transcript_path_for_session(session_id, ctx);
        match tokio::fs::metadata(&target_file).await {
            Ok(_) => {
                self.existing_session_files
                    .insert(session_id.to_string(), target_file.clone());
                Some(target_file)
            }
            Err(_) => None,
        }
    }

    /// Re-append cached session metadata to the end of the transcript file.
    pub fn re_append_session_metadata(&self, ctx: &SessionContext, skip_title_refresh: bool) {
        let session_file = match &self.session_file {
            Some(f) => f.clone(),
            None => return,
        };
        if ctx.session_id.is_empty() {
            return;
        }

        let tail = read_file_tail_sync(&session_file);
        let _tail_lines: Vec<&str> = tail.split('\n').collect();

        // Refresh SDK-mutable fields from tail (title, tag)
        // We skip this for now in the interest of a simpler implementation
        // and handle it via the context's cached values

        if !skip_title_refresh {
            // Title refresh from tail is handled by caller via ctx
        }

        let session_id = &ctx.session_id;

        if let Some(ref last_prompt) = self.current_session_last_prompt {
            let entry = serde_json::json!({
                "type": "last-prompt",
                "lastPrompt": last_prompt,
                "sessionId": session_id,
            });
            append_entry_to_file_sync(&session_file, &entry);
        }
        if let Some(ref title) = self.current_session_title {
            let entry = serde_json::json!({
                "type": "custom-title",
                "customTitle": title,
                "sessionId": session_id,
            });
            append_entry_to_file_sync(&session_file, &entry);
        }
        if let Some(ref tag) = self.current_session_tag {
            let entry = serde_json::json!({
                "type": "tag",
                "tag": tag,
                "sessionId": session_id,
            });
            append_entry_to_file_sync(&session_file, &entry);
        }
        if let Some(ref agent_name) = self.current_session_agent_name {
            let entry = serde_json::json!({
                "type": "agent-name",
                "agentName": agent_name,
                "sessionId": session_id,
            });
            append_entry_to_file_sync(&session_file, &entry);
        }
        if let Some(ref agent_color) = self.current_session_agent_color {
            let entry = serde_json::json!({
                "type": "agent-color",
                "agentColor": agent_color,
                "sessionId": session_id,
            });
            append_entry_to_file_sync(&session_file, &entry);
        }
        if let Some(ref agent_setting) = self.current_session_agent_setting {
            let entry = serde_json::json!({
                "type": "agent-setting",
                "agentSetting": agent_setting,
                "sessionId": session_id,
            });
            append_entry_to_file_sync(&session_file, &entry);
        }
        if let Some(ref mode) = self.current_session_mode {
            let entry = serde_json::json!({
                "type": "mode",
                "mode": mode,
                "sessionId": session_id,
            });
            append_entry_to_file_sync(&session_file, &entry);
        }
        if let Some(ref worktree_opt) = self.current_session_worktree {
            let entry = serde_json::json!({
                "type": "worktree-state",
                "worktreeSession": worktree_opt,
                "sessionId": session_id,
            });
            append_entry_to_file_sync(&session_file, &entry);
        }
        if let (Some(pr_number), Some(ref pr_url), Some(ref pr_repo)) = (
            self.current_session_pr_number,
            &self.current_session_pr_url,
            &self.current_session_pr_repository,
        ) {
            let entry = serde_json::json!({
                "type": "pr-link",
                "sessionId": session_id,
                "prNumber": pr_number,
                "prUrl": pr_url,
                "prRepository": pr_repo,
                "timestamp": Utc::now().to_rfc3339(),
            });
            append_entry_to_file_sync(&session_file, &entry);
        }
    }

    /// Insert a chain of messages into the transcript.
    pub async fn insert_message_chain(
        &mut self,
        messages: &[Value],
        is_sidechain: bool,
        agent_id: Option<&str>,
        starting_parent_uuid: Option<&str>,
        team_info: Option<&TeamInfo>,
        ctx: &SessionContext,
    ) -> Result<()> {
        self.increment_pending_writes();
        let result = self
            .insert_message_chain_inner(
                messages,
                is_sidechain,
                agent_id,
                starting_parent_uuid,
                team_info,
                ctx,
            )
            .await;
        self.decrement_pending_writes();
        result
    }

    async fn insert_message_chain_inner(
        &mut self,
        messages: &[Value],
        is_sidechain: bool,
        agent_id: Option<&str>,
        starting_parent_uuid: Option<&str>,
        team_info: Option<&TeamInfo>,
        ctx: &SessionContext,
    ) -> Result<()> {
        let mut parent_uuid: Option<String> = starting_parent_uuid.map(|s| s.to_string());

        // First user/assistant message materializes the session file.
        if self.session_file.is_none()
            && messages.iter().any(|m| {
                let t = m.get("type").and_then(|v| v.as_str()).unwrap_or("");
                t == "user" || t == "assistant"
            })
        {
            self.materialize_session_file(ctx).await?;
        }

        let git_branch: Option<String> = None; // Would be obtained from git
        let slug = ctx.plan_slug_cache.get(&ctx.session_id).cloned();

        for message in messages {
            let msg_type = message.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let is_compact_boundary = is_compact_boundary_message(message);

            // For tool_result messages, use the assistant message UUID if available
            let mut effective_parent_uuid = parent_uuid.clone();
            if msg_type == "user" {
                if let Some(src_uuid) = message
                    .get("sourceToolAssistantUUID")
                    .and_then(|v| v.as_str())
                {
                    effective_parent_uuid = Some(src_uuid.to_string());
                }
            }

            let mut transcript_message = message.clone();
            if let Some(obj) = transcript_message.as_object_mut() {
                obj.insert(
                    "parentUuid".to_string(),
                    if is_compact_boundary {
                        Value::Null
                    } else {
                        match &effective_parent_uuid {
                            Some(u) => Value::String(u.clone()),
                            None => Value::Null,
                        }
                    },
                );
                if is_compact_boundary {
                    if let Some(ref pu) = parent_uuid {
                        obj.insert("logicalParentUuid".to_string(), Value::String(pu.clone()));
                    }
                }
                obj.insert("isSidechain".to_string(), Value::Bool(is_sidechain));
                if let Some(ti) = team_info {
                    if let Some(ref tn) = ti.team_name {
                        obj.insert("teamName".to_string(), Value::String(tn.clone()));
                    }
                    if let Some(ref an) = ti.agent_name {
                        obj.insert("agentName".to_string(), Value::String(an.clone()));
                    }
                }
                if msg_type == "user" {
                    if let Some(_pid) = ctx.entrypoint.as_ref() {
                        // promptId would come from context
                    }
                }
                if let Some(aid) = agent_id {
                    obj.insert("agentId".to_string(), Value::String(aid.to_string()));
                }
                obj.insert("userType".to_string(), Value::String(ctx.user_type.clone()));
                if let Some(ref ep) = ctx.entrypoint {
                    obj.insert("entrypoint".to_string(), Value::String(ep.clone()));
                }
                obj.insert(
                    "cwd".to_string(),
                    Value::String(ctx.original_cwd.to_string_lossy().to_string()),
                );
                obj.insert(
                    "sessionId".to_string(),
                    Value::String(ctx.session_id.clone()),
                );
                obj.insert("version".to_string(), Value::String(ctx.version.clone()));
                if let Some(ref gb) = git_branch {
                    obj.insert("gitBranch".to_string(), Value::String(gb.clone()));
                }
                if let Some(ref s) = slug {
                    obj.insert("slug".to_string(), Value::String(s.clone()));
                }
            }

            self.append_entry(transcript_message, &ctx.session_id, ctx)
                .await?;

            if is_chain_participant(message) {
                parent_uuid = message
                    .get("uuid")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
            }
        }

        // Cache last user prompt
        if !is_sidechain {
            let text = get_first_meaningful_user_message_text_content(messages);
            if let Some(text) = text {
                let flat = text.replace('\n', " ").trim().to_string();
                self.current_session_last_prompt = Some(if flat.len() > 200 {
                    format!("{}…", flat[..200].trim())
                } else {
                    flat
                });
            }
        }

        Ok(())
    }

    /// Insert a file history snapshot.
    pub async fn insert_file_history_snapshot(
        &mut self,
        message_id: &str,
        snapshot: &Value,
        is_snapshot_update: bool,
        ctx: &SessionContext,
    ) -> Result<()> {
        self.increment_pending_writes();
        let entry = serde_json::json!({
            "type": "file-history-snapshot",
            "messageId": message_id,
            "snapshot": snapshot,
            "isSnapshotUpdate": is_snapshot_update,
        });
        let result = self.append_entry(entry, &ctx.session_id, ctx).await;
        self.decrement_pending_writes();
        result
    }

    /// Insert a queue operation.
    pub async fn insert_queue_operation(
        &mut self,
        queue_op: Value,
        ctx: &SessionContext,
    ) -> Result<()> {
        self.increment_pending_writes();
        let result = self.append_entry(queue_op, &ctx.session_id, ctx).await;
        self.decrement_pending_writes();
        result
    }

    /// Insert an attribution snapshot.
    pub async fn insert_attribution_snapshot(
        &mut self,
        snapshot: Value,
        ctx: &SessionContext,
    ) -> Result<()> {
        self.increment_pending_writes();
        let result = self.append_entry(snapshot, &ctx.session_id, ctx).await;
        self.decrement_pending_writes();
        result
    }

    /// Insert content replacements.
    pub async fn insert_content_replacement(
        &mut self,
        replacements: &[Value],
        agent_id: Option<&str>,
        ctx: &SessionContext,
    ) -> Result<()> {
        self.increment_pending_writes();
        let entry = serde_json::json!({
            "type": "content-replacement",
            "sessionId": ctx.session_id,
            "agentId": agent_id,
            "replacements": replacements,
        });
        let result = self.append_entry(entry, &ctx.session_id, ctx).await;
        self.decrement_pending_writes();
        result
    }

    /// Append an entry, routing to the correct file and queue.
    pub async fn append_entry(
        &mut self,
        entry: Value,
        session_id: &str,
        ctx: &SessionContext,
    ) -> Result<()> {
        if self.should_skip_persistence(ctx) {
            return Ok(());
        }

        let is_current_session = session_id == ctx.session_id;

        let session_file = if is_current_session {
            if self.session_file.is_none() {
                self.pending_entries.push(entry);
                return Ok(());
            }
            self.session_file.clone().unwrap()
        } else {
            match self.get_existing_session_file(session_id, ctx).await {
                Some(f) => f,
                None => {
                    tracing::error!(
                        "appendEntry: session file not found for other session {}",
                        session_id
                    );
                    return Ok(());
                }
            }
        };

        let entry_type = entry.get("type").and_then(|v| v.as_str()).unwrap_or("");

        match entry_type {
            "summary"
            | "custom-title"
            | "ai-title"
            | "last-prompt"
            | "task-summary"
            | "tag"
            | "agent-name"
            | "agent-color"
            | "agent-setting"
            | "pr-link"
            | "file-history-snapshot"
            | "attribution-snapshot"
            | "speculation-accept"
            | "mode"
            | "worktree-state"
            | "marble-origami-commit"
            | "marble-origami-snapshot"
            | "queue-operation" => {
                self.enqueue_write(session_file, entry);
            }
            "content-replacement" => {
                let target_file = if entry.get("agentId").and_then(|v| v.as_str()).is_some() {
                    let aid = entry["agentId"].as_str().unwrap();
                    get_agent_transcript_path(aid, ctx)
                } else {
                    session_file
                };
                self.enqueue_write(target_file, entry);
            }
            _ => {
                // Transcript message (user/assistant/attachment/system)
                let is_sidechain = entry
                    .get("isSidechain")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let entry_agent_id = entry.get("agentId").and_then(|v| v.as_str());
                let is_agent_sidechain = is_sidechain && entry_agent_id.is_some();

                let target_file = if is_agent_sidechain {
                    get_agent_transcript_path(entry_agent_id.unwrap(), ctx)
                } else {
                    session_file
                };

                self.enqueue_write(target_file, entry);
            }
        }

        Ok(())
    }

    pub fn set_remote_ingress_url(&mut self, url: String) {
        self.remote_ingress_url = Some(url.clone());
        tracing::debug!("Remote persistence enabled with URL: {}", url);
        if !url.is_empty() {
            self.flush_interval_ms = REMOTE_FLUSH_INTERVAL_MS;
        }
    }

    pub fn set_internal_event_writer(&mut self, writer: InternalEventWriter) {
        self.internal_event_writer = Some(writer);
        tracing::debug!("CCR v2 internal event writer registered for transcript persistence");
        self.flush_interval_ms = REMOTE_FLUSH_INTERVAL_MS;
    }

    pub fn set_internal_event_reader(&mut self, reader: InternalEventReader) {
        self.internal_event_reader = Some(reader);
        tracing::debug!("CCR v2 internal event reader registered for session resume");
    }

    pub fn set_internal_subagent_event_reader(&mut self, reader: InternalEventReader) {
        self.internal_subagent_event_reader = Some(reader);
        tracing::debug!("CCR v2 subagent event reader registered for session resume");
    }

    pub fn get_internal_event_reader(&self) -> Option<&InternalEventReader> {
        self.internal_event_reader.as_ref()
    }

    pub fn get_internal_subagent_event_reader(&self) -> Option<&InternalEventReader> {
        self.internal_subagent_event_reader.as_ref()
    }
}

// ---------------------------------------------------------------------------
// Module-level state
// ---------------------------------------------------------------------------

static AGENT_TRANSCRIPT_SUBDIRS: Lazy<Mutex<HashMap<String, String>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

/// Get the projects directory.
pub fn get_projects_dir(ctx: &SessionContext) -> PathBuf {
    ctx.config_home_dir.join("projects")
}

/// Sanitize a path for use as a directory name.
pub fn sanitize_path(path: &str) -> String {
    path.replace(['/', '\\', ':'], "-")
        .trim_matches('-')
        .to_string()
}

/// Get the project directory for a given cwd.
pub fn get_project_dir(cwd: &Path, ctx: &SessionContext) -> PathBuf {
    get_projects_dir(ctx).join(sanitize_path(&cwd.to_string_lossy()))
}

/// Get the transcript path for the current session.
pub fn get_transcript_path(ctx: &SessionContext) -> PathBuf {
    let project_dir = ctx
        .session_project_dir
        .clone()
        .unwrap_or_else(|| get_project_dir(&ctx.original_cwd, ctx));
    project_dir.join(format!("{}.jsonl", ctx.session_id))
}

/// Get the transcript path for a specific session.
pub fn get_transcript_path_for_session(session_id: &str, ctx: &SessionContext) -> PathBuf {
    if session_id == ctx.session_id {
        return get_transcript_path(ctx);
    }
    let project_dir = get_project_dir(&ctx.original_cwd, ctx);
    project_dir.join(format!("{}.jsonl", session_id))
}

/// Get the transcript path for an agent.
pub fn get_agent_transcript_path(agent_id: &str, ctx: &SessionContext) -> PathBuf {
    let project_dir = ctx
        .session_project_dir
        .clone()
        .unwrap_or_else(|| get_project_dir(&ctx.original_cwd, ctx));
    let subdirs = AGENT_TRANSCRIPT_SUBDIRS.lock();
    let base = if let Some(subdir) = subdirs.get(agent_id) {
        project_dir
            .join(&ctx.session_id)
            .join("subagents")
            .join(subdir)
    } else {
        project_dir.join(&ctx.session_id).join("subagents")
    };
    base.join(format!("agent-{}.jsonl", agent_id))
}

fn get_agent_metadata_path(agent_id: &str, ctx: &SessionContext) -> PathBuf {
    let transcript_path = get_agent_transcript_path(agent_id, ctx);
    transcript_path.with_extension("meta.json")
}

fn get_remote_agents_dir(ctx: &SessionContext) -> PathBuf {
    let project_dir = ctx
        .session_project_dir
        .clone()
        .unwrap_or_else(|| get_project_dir(&ctx.original_cwd, ctx));
    project_dir.join(&ctx.session_id).join("remote-agents")
}

fn get_remote_agent_metadata_path(task_id: &str, ctx: &SessionContext) -> PathBuf {
    get_remote_agents_dir(ctx).join(format!("remote-agent-{}.meta.json", task_id))
}
// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Check if a message is a compact boundary message.
pub fn is_compact_boundary_message(message: &Value) -> bool {
    message.get("type").and_then(|v| v.as_str()) == Some("system")
        && message.get("subtype").and_then(|v| v.as_str()) == Some("compact_boundary")
}

/// Check if an entry is a transcript message.
pub fn is_transcript_message(entry: &Value) -> bool {
    let entry_type = entry.get("type").and_then(|v| v.as_str()).unwrap_or("");
    matches!(entry_type, "user" | "assistant" | "attachment" | "system")
}

/// Entries that participate in the parentUuid chain.
pub fn is_chain_participant(m: &Value) -> bool {
    m.get("type").and_then(|v| v.as_str()) != Some("progress")
}

/// Check if a data type is ephemeral tool progress.
pub fn is_ephemeral_tool_progress(data_type: &str) -> bool {
    EPHEMERAL_PROGRESS_TYPES.contains(data_type)
}

/// Check if entry is a legacy progress entry.
fn is_legacy_progress_entry(entry: &Value) -> bool {
    entry.get("type").and_then(|v| v.as_str()) == Some("progress")
        && entry.get("uuid").and_then(|v| v.as_str()).is_some()
}

/// Append an entry to file synchronously.
fn append_entry_to_file_sync(full_path: &Path, entry: &Value) {
    let line = match serde_json::to_string(entry) {
        Ok(s) => s + "\n",
        Err(_) => return,
    };
    if let Some(parent) = full_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    use std::io::Write;
    let mut file = match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(full_path)
    {
        Ok(f) => f,
        Err(_) => return,
    };
    let _ = file.write_all(line.as_bytes());
}

/// Sync tail read for reAppendSessionMetadata's external-writer check.
fn read_file_tail_sync(full_path: &Path) -> String {
    use std::io::Read;
    let file = match std::fs::File::open(full_path) {
        Ok(f) => f,
        Err(_) => return String::new(),
    };
    let metadata = match file.metadata() {
        Ok(m) => m,
        Err(_) => return String::new(),
    };
    let size = metadata.len();
    let tail_offset = size.saturating_sub(LITE_READ_BUF_SIZE as u64);
    let buf_size = std::cmp::min(LITE_READ_BUF_SIZE as u64, size - tail_offset) as usize;
    let mut buf = vec![0u8; buf_size];
    use std::io::Seek;
    let mut file = file;
    if file.seek(std::io::SeekFrom::Start(tail_offset)).is_err() {
        return String::new();
    }
    match file.read(&mut buf) {
        Ok(n) => String::from_utf8_lossy(&buf[..n]).to_string(),
        Err(_) => String::new(),
    }
}

/// Parse a JSONL buffer into a list of Values.
fn parse_jsonl(data: &[u8]) -> Vec<Value> {
    let text = String::from_utf8_lossy(data);
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

/// Extract a JSON string field from text.
pub fn extract_json_string_field(text: &str, key: &str) -> Option<String> {
    let patterns = [format!("\"{}\":\"", key), format!("\"{}\":\" \"", key)];
    for pattern in &patterns {
        if let Some(idx) = text.find(pattern.as_str()) {
            let value_start = idx + pattern.len();
            let rest = &text[value_start..];
            if let Some(end) = find_unescaped_quote(rest) {
                return Some(rest[..end].replace("\\n", "\n").replace("\\t", "\t"));
            }
        }
    }
    None
}

/// Extract the last occurrence of a JSON string field.
pub fn extract_last_json_string_field(text: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{}\":\"", key);
    let mut last_val = None;
    let mut search_from = 0;
    while let Some(idx) = text[search_from..].find(&pattern) {
        let abs_idx = search_from + idx;
        let value_start = abs_idx + pattern.len();
        let rest = &text[value_start..];
        if let Some(end) = find_unescaped_quote(rest) {
            last_val = Some(rest[..end].replace("\\n", "\n").replace("\\t", "\t"));
        }
        search_from = value_start;
    }
    last_val
}

/// Find the position of the first unescaped quote in a string.
fn find_unescaped_quote(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == b'"' {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Like extract_json_string_field but returns first maxLen characters.
fn extract_json_string_field_prefix(text: &str, key: &str, max_len: usize) -> String {
    let patterns = [format!("\"{}\":\"", key), format!("\"{}\":\" \"", key)];
    for pattern in &patterns {
        if let Some(idx) = text.find(pattern.as_str()) {
            let value_start = idx + pattern.len();
            let rest = &text[value_start..];
            let mut collected = 0;
            let mut end_pos = 0;
            let bytes = rest.as_bytes();
            while end_pos < bytes.len() && collected < max_len {
                if bytes[end_pos] == b'\\' {
                    end_pos += 2;
                    collected += 1;
                    continue;
                }
                if bytes[end_pos] == b'"' {
                    break;
                }
                end_pos += 1;
                collected += 1;
            }
            let raw = &rest[..end_pos];
            return raw
                .replace("\\n", " ")
                .replace("\\t", " ")
                .trim()
                .to_string();
        }
    }
    String::new()
}

/// Extract a tag from text content.
pub fn extract_tag(text: &str, tag_name: &str) -> Option<String> {
    let open = format!("<{}", tag_name);
    let close = format!("</{}>", tag_name);
    if let Some(start_idx) = text.find(&open) {
        let after_open = &text[start_idx + open.len()..];
        // Find the end of the opening tag
        if let Some(gt_idx) = after_open.find('>') {
            let content_start = gt_idx + 1;
            let rest = &after_open[content_start..];
            if let Some(end_idx) = rest.find(&close) {
                return Some(rest[..end_idx].to_string());
            }
        }
    }
    None
}

/// O(n) single-pass: find the message with the latest timestamp matching a predicate.
fn find_latest_message<'a, I, F>(messages: I, predicate: F) -> Option<&'a Value>
where
    I: Iterator<Item = &'a Value>,
    F: Fn(&Value) -> bool,
{
    let mut latest: Option<&Value> = None;
    let mut max_time: Option<String> = None;
    for m in messages {
        if !predicate(m) {
            continue;
        }
        let t = m.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
        if max_time.as_deref().map_or(true, |mt| t > mt) {
            max_time = Some(t.to_string());
            latest = Some(m);
        }
    }
    latest
}

// ---------------------------------------------------------------------------
// Agent metadata CRUD
// ---------------------------------------------------------------------------

/// Set an agent transcript subdirectory.
pub fn set_agent_transcript_subdir(agent_id: &str, subdir: &str) {
    AGENT_TRANSCRIPT_SUBDIRS
        .lock()
        .insert(agent_id.to_string(), subdir.to_string());
}

/// Clear an agent transcript subdirectory.
pub fn clear_agent_transcript_subdir(agent_id: &str) {
    AGENT_TRANSCRIPT_SUBDIRS.lock().remove(agent_id);
}

/// Write agent metadata to a sidecar file.
pub async fn write_agent_metadata(
    agent_id: &str,
    metadata: &AgentMetadata,
    ctx: &SessionContext,
) -> Result<()> {
    let path = get_agent_metadata_path(agent_id, ctx);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let content = serde_json::to_string(metadata)?;
    tokio::fs::write(&path, content).await?;
    Ok(())
}

/// Read agent metadata from a sidecar file.
pub async fn read_agent_metadata(
    agent_id: &str,
    ctx: &SessionContext,
) -> Result<Option<AgentMetadata>> {
    let path = get_agent_metadata_path(agent_id, ctx);
    match tokio::fs::read_to_string(&path).await {
        Ok(raw) => {
            let metadata: AgentMetadata = serde_json::from_str(&raw)?;
            Ok(Some(metadata))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Write remote agent metadata.
pub async fn write_remote_agent_metadata(
    task_id: &str,
    metadata: &RemoteAgentMetadata,
    ctx: &SessionContext,
) -> Result<()> {
    let path = get_remote_agent_metadata_path(task_id, ctx);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let content = serde_json::to_string(metadata)?;
    tokio::fs::write(&path, content).await?;
    Ok(())
}

/// Read remote agent metadata.
pub async fn read_remote_agent_metadata(
    task_id: &str,
    ctx: &SessionContext,
) -> Result<Option<RemoteAgentMetadata>> {
    let path = get_remote_agent_metadata_path(task_id, ctx);
    match tokio::fs::read_to_string(&path).await {
        Ok(raw) => {
            let metadata: RemoteAgentMetadata = serde_json::from_str(&raw)?;
            Ok(Some(metadata))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Delete remote agent metadata.
pub async fn delete_remote_agent_metadata(task_id: &str, ctx: &SessionContext) -> Result<()> {
    let path = get_remote_agent_metadata_path(task_id, ctx);
    match tokio::fs::remove_file(&path).await {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// List all remote agent metadata files.
pub async fn list_remote_agent_metadata(ctx: &SessionContext) -> Result<Vec<RemoteAgentMetadata>> {
    let dir = get_remote_agents_dir(ctx);
    let mut entries = match tokio::fs::read_dir(&dir).await {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut results = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.ends_with(".meta.json") {
            continue;
        }
        let path = entry.path();
        match tokio::fs::read_to_string(&path).await {
            Ok(raw) => {
                if let Ok(metadata) = serde_json::from_str::<RemoteAgentMetadata>(&raw) {
                    results.push(metadata);
                }
            }
            Err(_) => {
                tracing::debug!("listRemoteAgentMetadata: skipping {}", name);
            }
        }
    }
    Ok(results)
}

/// Check if a session ID exists on disk.
pub fn session_id_exists(session_id: &str, ctx: &SessionContext) -> bool {
    let project_dir = get_project_dir(&ctx.original_cwd, ctx);
    let session_file = project_dir.join(format!("{}.jsonl", session_id));
    session_file.exists()
}

// ---------------------------------------------------------------------------
// Transcript recording
// ---------------------------------------------------------------------------

/// Record a transcript, deduplicating already-recorded messages.
pub async fn record_transcript(
    project: &mut Project,
    messages: &[Value],
    team_info: Option<&TeamInfo>,
    starting_parent_uuid_hint: Option<&str>,
    all_messages: Option<&[Value]>,
    ctx: &SessionContext,
) -> Result<Option<String>> {
    let cleaned = clean_messages_for_logging(messages, all_messages, ctx);
    let message_set = get_session_messages(&ctx.session_id, ctx).await?;

    let mut new_messages: Vec<Value> = Vec::new();
    let mut starting_parent_uuid = starting_parent_uuid_hint.map(|s| s.to_string());
    let mut seen_new_message = false;

    for m in &cleaned {
        let uuid = m.get("uuid").and_then(|v| v.as_str()).unwrap_or("");
        if message_set.contains(uuid) {
            if !seen_new_message && is_chain_participant(m) {
                starting_parent_uuid = Some(uuid.to_string());
            }
        } else {
            new_messages.push(m.clone());
            seen_new_message = true;
        }
    }

    if !new_messages.is_empty() {
        project
            .insert_message_chain(
                &new_messages,
                false,
                None,
                starting_parent_uuid.as_deref(),
                team_info,
                ctx,
            )
            .await?;
    }

    let last_recorded = new_messages.iter().rev().find(|m| is_chain_participant(m));
    let result = last_recorded
        .and_then(|m| {
            m.get("uuid")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .or(starting_parent_uuid);
    Ok(result)
}

/// Record sidechain transcript.
pub async fn record_sidechain_transcript(
    project: &mut Project,
    messages: &[Value],
    agent_id: Option<&str>,
    starting_parent_uuid: Option<&str>,
    ctx: &SessionContext,
) -> Result<()> {
    let cleaned = clean_messages_for_logging(messages, None, ctx);
    project
        .insert_message_chain(&cleaned, true, agent_id, starting_parent_uuid, None, ctx)
        .await
}

/// Record a queue operation.
pub async fn record_queue_operation(
    project: &mut Project,
    queue_op: Value,
    ctx: &SessionContext,
) -> Result<()> {
    project.insert_queue_operation(queue_op, ctx).await
}

/// Remove a transcript message by UUID.
pub async fn remove_transcript_message(project: &mut Project, target_uuid: &str) -> Result<()> {
    project.remove_message_by_uuid(target_uuid).await
}

/// Record a file history snapshot.
pub async fn record_file_history_snapshot(
    project: &mut Project,
    message_id: &str,
    snapshot: &Value,
    is_snapshot_update: bool,
    ctx: &SessionContext,
) -> Result<()> {
    project
        .insert_file_history_snapshot(message_id, snapshot, is_snapshot_update, ctx)
        .await
}

/// Record an attribution snapshot.
pub async fn record_attribution_snapshot(
    project: &mut Project,
    snapshot: Value,
    ctx: &SessionContext,
) -> Result<()> {
    project.insert_attribution_snapshot(snapshot, ctx).await
}

/// Record content replacements.
pub async fn record_content_replacement(
    project: &mut Project,
    replacements: &[Value],
    agent_id: Option<&str>,
    ctx: &SessionContext,
) -> Result<()> {
    project
        .insert_content_replacement(replacements, agent_id, ctx)
        .await
}

/// Record a context collapse commit.
pub async fn record_context_collapse_commit(
    project: &mut Project,
    commit: Value,
    ctx: &SessionContext,
) -> Result<()> {
    if ctx.session_id.is_empty() {
        return Ok(());
    }
    let mut entry = commit;
    if let Some(obj) = entry.as_object_mut() {
        obj.insert(
            "type".to_string(),
            Value::String("marble-origami-commit".to_string()),
        );
        obj.insert(
            "sessionId".to_string(),
            Value::String(ctx.session_id.clone()),
        );
    }
    project.append_entry(entry, &ctx.session_id, ctx).await
}

/// Record a context collapse snapshot.
pub async fn record_context_collapse_snapshot(
    project: &mut Project,
    snapshot: Value,
    ctx: &SessionContext,
) -> Result<()> {
    if ctx.session_id.is_empty() {
        return Ok(());
    }
    let mut entry = snapshot;
    if let Some(obj) = entry.as_object_mut() {
        obj.insert(
            "type".to_string(),
            Value::String("marble-origami-snapshot".to_string()),
        );
        obj.insert(
            "sessionId".to_string(),
            Value::String(ctx.session_id.clone()),
        );
    }
    project.append_entry(entry, &ctx.session_id, ctx).await
}

/// Flush session storage.
pub async fn flush_session_storage(project: &mut Project) -> Result<()> {
    project.flush().await
}

/// Reset session file pointer.
pub fn reset_session_file_pointer(project: &mut Project) {
    project.reset_session_file();
}

/// Adopt the existing session file after --continue/--resume.
pub fn adopt_resumed_session_file(project: &mut Project, ctx: &SessionContext) {
    project.session_file = Some(get_transcript_path(ctx));
    project.re_append_session_metadata(ctx, true);
}

// ---------------------------------------------------------------------------
// Session metadata
// ---------------------------------------------------------------------------

/// Save a custom title for a session.
pub fn save_custom_title(
    project: &mut Project,
    session_id: &str,
    custom_title: &str,
    full_path: Option<&Path>,
    ctx: &SessionContext,
) {
    let resolved_path = full_path
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| get_transcript_path_for_session(session_id, ctx));
    let entry = serde_json::json!({
        "type": "custom-title",
        "customTitle": custom_title,
        "sessionId": session_id,
    });
    append_entry_to_file_sync(&resolved_path, &entry);
    if session_id == ctx.session_id {
        project.current_session_title = Some(custom_title.to_string());
    }
}

/// Save an AI-generated title.
pub fn save_ai_generated_title(session_id: &str, ai_title: &str, ctx: &SessionContext) {
    let path = get_transcript_path_for_session(session_id, ctx);
    let entry = serde_json::json!({
        "type": "ai-title",
        "aiTitle": ai_title,
        "sessionId": session_id,
    });
    append_entry_to_file_sync(&path, &entry);
}

/// Save a task summary.
pub fn save_task_summary(session_id: &str, summary: &str, ctx: &SessionContext) {
    let path = get_transcript_path_for_session(session_id, ctx);
    let entry = serde_json::json!({
        "type": "task-summary",
        "summary": summary,
        "sessionId": session_id,
        "timestamp": Utc::now().to_rfc3339(),
    });
    append_entry_to_file_sync(&path, &entry);
}

/// Save a tag for a session.
pub fn save_tag(
    project: &mut Project,
    session_id: &str,
    tag: &str,
    full_path: Option<&Path>,
    ctx: &SessionContext,
) {
    let resolved_path = full_path
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| get_transcript_path_for_session(session_id, ctx));
    let entry = serde_json::json!({
        "type": "tag",
        "tag": tag,
        "sessionId": session_id,
    });
    append_entry_to_file_sync(&resolved_path, &entry);
    if session_id == ctx.session_id {
        project.current_session_tag = Some(tag.to_string());
    }
}

/// Link a session to a GitHub pull request.
pub fn link_session_to_pr(
    project: &mut Project,
    session_id: &str,
    pr_number: u64,
    pr_url: &str,
    pr_repository: &str,
    full_path: Option<&Path>,
    ctx: &SessionContext,
) {
    let resolved_path = full_path
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| get_transcript_path_for_session(session_id, ctx));
    let entry = serde_json::json!({
        "type": "pr-link",
        "sessionId": session_id,
        "prNumber": pr_number,
        "prUrl": pr_url,
        "prRepository": pr_repository,
        "timestamp": Utc::now().to_rfc3339(),
    });
    append_entry_to_file_sync(&resolved_path, &entry);
    if session_id == ctx.session_id {
        project.current_session_pr_number = Some(pr_number);
        project.current_session_pr_url = Some(pr_url.to_string());
        project.current_session_pr_repository = Some(pr_repository.to_string());
    }
}

/// Get the current session tag.
pub fn get_current_session_tag(
    project: &Project,
    session_id: &str,
    ctx: &SessionContext,
) -> Option<String> {
    if session_id == ctx.session_id {
        return project.current_session_tag.clone();
    }
    None
}

/// Get the current session title.
pub fn get_current_session_title(
    project: &Project,
    session_id: &str,
    ctx: &SessionContext,
) -> Option<String> {
    if session_id == ctx.session_id {
        return project.current_session_title.clone();
    }
    None
}

/// Get the current session agent color.
pub fn get_current_session_agent_color(project: &Project) -> Option<String> {
    project.current_session_agent_color.clone()
}

/// Restore session metadata into in-memory cache on resume.
pub fn restore_session_metadata(project: &mut Project, meta: &RestoreSessionMeta) {
    if let Some(ref title) = meta.custom_title {
        if project.current_session_title.is_none() {
            project.current_session_title = Some(title.clone());
        }
    }
    if let Some(ref tag) = meta.tag {
        project.current_session_tag = if tag.is_empty() {
            None
        } else {
            Some(tag.clone())
        };
    }
    if let Some(ref name) = meta.agent_name {
        project.current_session_agent_name = Some(name.clone());
    }
    if let Some(ref color) = meta.agent_color {
        project.current_session_agent_color = Some(color.clone());
    }
    if let Some(ref setting) = meta.agent_setting {
        project.current_session_agent_setting = Some(setting.clone());
    }
    if let Some(ref mode) = meta.mode {
        project.current_session_mode = Some(mode.clone());
    }
    if let Some(ref wt) = meta.worktree_session {
        project.current_session_worktree = Some(wt.clone());
    }
    if let Some(pr) = meta.pr_number {
        project.current_session_pr_number = Some(pr);
    }
    if let Some(ref url) = meta.pr_url {
        project.current_session_pr_url = Some(url.clone());
    }
    if let Some(ref repo) = meta.pr_repository {
        project.current_session_pr_repository = Some(repo.clone());
    }
}

/// Metadata for restoring a session.
#[derive(Debug, Clone, Default)]
pub struct RestoreSessionMeta {
    pub custom_title: Option<String>,
    pub tag: Option<String>,
    pub agent_name: Option<String>,
    pub agent_color: Option<String>,
    pub agent_setting: Option<String>,
    pub mode: Option<String>,
    pub worktree_session: Option<Option<PersistedWorktreeSession>>,
    pub pr_number: Option<u64>,
    pub pr_url: Option<String>,
    pub pr_repository: Option<String>,
}

/// Clear all cached session metadata.
pub fn clear_session_metadata(project: &mut Project) {
    project.current_session_title = None;
    project.current_session_tag = None;
    project.current_session_agent_name = None;
    project.current_session_agent_color = None;
    project.current_session_last_prompt = None;
    project.current_session_agent_setting = None;
    project.current_session_mode = None;
    project.current_session_worktree = None;
    project.current_session_pr_number = None;
    project.current_session_pr_url = None;
    project.current_session_pr_repository = None;
}

/// Re-append cached session metadata.
pub fn re_append_session_metadata(project: &Project, ctx: &SessionContext) {
    project.re_append_session_metadata(ctx, false);
}

/// Save agent name.
pub fn save_agent_name(
    project: &mut Project,
    session_id: &str,
    agent_name: &str,
    full_path: Option<&Path>,
    ctx: &SessionContext,
) {
    let resolved_path = full_path
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| get_transcript_path_for_session(session_id, ctx));
    let entry = serde_json::json!({
        "type": "agent-name",
        "agentName": agent_name,
        "sessionId": session_id,
    });
    append_entry_to_file_sync(&resolved_path, &entry);
    if session_id == ctx.session_id {
        project.current_session_agent_name = Some(agent_name.to_string());
    }
}

/// Save agent color.
pub fn save_agent_color(
    project: &mut Project,
    session_id: &str,
    agent_color: &str,
    full_path: Option<&Path>,
    ctx: &SessionContext,
) {
    let resolved_path = full_path
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| get_transcript_path_for_session(session_id, ctx));
    let entry = serde_json::json!({
        "type": "agent-color",
        "agentColor": agent_color,
        "sessionId": session_id,
    });
    append_entry_to_file_sync(&resolved_path, &entry);
    if session_id == ctx.session_id {
        project.current_session_agent_color = Some(agent_color.to_string());
    }
}

/// Save agent setting (cache only).
pub fn save_agent_setting(project: &mut Project, agent_setting: &str) {
    project.current_session_agent_setting = Some(agent_setting.to_string());
}

/// Cache a session title.
pub fn cache_session_title(project: &mut Project, custom_title: &str) {
    project.current_session_title = Some(custom_title.to_string());
}

/// Save the session mode (cache only).
pub fn save_mode(project: &mut Project, mode: &str) {
    project.current_session_mode = Some(mode.to_string());
}

/// Save worktree state.
pub fn save_worktree_state(
    project: &mut Project,
    worktree_session: Option<PersistedWorktreeSession>,
    ctx: &SessionContext,
) {
    let stripped = worktree_session
        .as_ref()
        .map(|wt| PersistedWorktreeSession {
            original_cwd: wt.original_cwd.clone(),
            worktree_path: wt.worktree_path.clone(),
            worktree_name: wt.worktree_name.clone(),
            worktree_branch: wt.worktree_branch.clone(),
            original_branch: wt.original_branch.clone(),
            original_head_commit: wt.original_head_commit.clone(),
            session_id: wt.session_id.clone(),
            tmux_session_name: wt.tmux_session_name.clone(),
            hook_based: wt.hook_based,
        });
    project.current_session_worktree = Some(stripped.clone());
    if let Some(ref session_file) = project.session_file {
        let entry = serde_json::json!({
            "type": "worktree-state",
            "worktreeSession": stripped,
            "sessionId": ctx.session_id,
        });
        append_entry_to_file_sync(session_file, &entry);
    }
}

// ---------------------------------------------------------------------------
// Session ID and log helpers
// ---------------------------------------------------------------------------

/// Extract session ID from a log.
pub fn get_session_id_from_log(log: &LogOption) -> Option<String> {
    if let Some(ref sid) = log.session_id {
        return Some(sid.clone());
    }
    log.messages
        .first()
        .and_then(|m| m.extra.get("sessionId"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Check if a log is a lite log.
pub fn is_lite_log(log: &LogOption) -> bool {
    log.messages.is_empty() && log.session_id.is_some()
}

/// Check if a message is loggable.
pub fn is_loggable_message(m: &Value, user_type: &str, save_hook_context: bool) -> bool {
    let msg_type = m.get("type").and_then(|v| v.as_str()).unwrap_or("");
    if msg_type == "progress" {
        return false;
    }
    if msg_type == "attachment" && user_type != "internal" {
        if save_hook_context {
            if let Some(attachment) = m.get("attachment") {
                if attachment.get("type").and_then(|v| v.as_str())
                    == Some("hook_additional_context")
                {
                    return true;
                }
            }
        }
        return false;
    }
    true
}

/// Clean messages for logging.
pub fn clean_messages_for_logging(
    messages: &[Value],
    all_messages: Option<&[Value]>,
    ctx: &SessionContext,
) -> Vec<Value> {
    let filtered: Vec<Value> = messages
        .iter()
        .filter(|m| is_loggable_message(m, &ctx.user_type, ctx.save_hook_additional_context))
        .cloned()
        .collect();

    if ctx.user_type != "internal" {
        let all = all_messages.unwrap_or(messages);
        transform_messages_for_external_transcript(&filtered, all)
    } else {
        filtered
    }
}

/// Collect REPL tool use IDs from messages.
fn collect_repl_ids(messages: &[Value]) -> HashSet<String> {
    let mut ids = HashSet::new();
    for m in messages {
        if m.get("type").and_then(|v| v.as_str()) == Some("assistant") {
            if let Some(content) = m
                .get("message")
                .and_then(|msg| msg.get("content"))
                .and_then(|c| c.as_array())
            {
                for b in content {
                    if b.get("type").and_then(|v| v.as_str()) == Some("tool_use")
                        && b.get("name").and_then(|v| v.as_str()) == Some("repl")
                    {
                        if let Some(id) = b.get("id").and_then(|v| v.as_str()) {
                            ids.insert(id.to_string());
                        }
                    }
                }
            }
        }
    }
    ids
}

/// Transform messages for external transcript (strip REPL wrapper).
fn transform_messages_for_external_transcript(
    messages: &[Value],
    all_messages: &[Value],
) -> Vec<Value> {
    let repl_ids = collect_repl_ids(all_messages);
    if repl_ids.is_empty() {
        return messages.to_vec();
    }

    let mut result = Vec::new();
    for m in messages {
        let msg_type = m.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match msg_type {
            "assistant" => {
                if let Some(content) = m
                    .get("message")
                    .and_then(|msg| msg.get("content"))
                    .and_then(|c| c.as_array())
                {
                    let has_repl = content.iter().any(|b| {
                        b.get("type").and_then(|v| v.as_str()) == Some("tool_use")
                            && b.get("name").and_then(|v| v.as_str()) == Some("repl")
                    });
                    let filtered: Vec<&Value> = if has_repl {
                        content
                            .iter()
                            .filter(|b| {
                                !(b.get("type").and_then(|v| v.as_str()) == Some("tool_use")
                                    && b.get("name").and_then(|v| v.as_str()) == Some("repl"))
                            })
                            .collect()
                    } else {
                        content.iter().collect()
                    };
                    if filtered.is_empty() {
                        continue;
                    }
                    let mut msg = m.clone();
                    if let Some(obj) = msg.as_object_mut() {
                        obj.remove("isVirtual");
                        if has_repl {
                            if let Some(message) = obj.get_mut("message") {
                                if let Some(msg_obj) = message.as_object_mut() {
                                    msg_obj.insert(
                                        "content".to_string(),
                                        Value::Array(filtered.into_iter().cloned().collect()),
                                    );
                                }
                            }
                        }
                    }
                    result.push(msg);
                } else {
                    result.push(m.clone());
                }
            }
            "user" => {
                if let Some(content) = m
                    .get("message")
                    .and_then(|msg| msg.get("content"))
                    .and_then(|c| c.as_array())
                {
                    let has_repl = content.iter().any(|b| {
                        b.get("type").and_then(|v| v.as_str()) == Some("tool_result")
                            && b.get("tool_use_id")
                                .and_then(|v| v.as_str())
                                .is_some_and(|id| repl_ids.contains(id))
                    });
                    let filtered: Vec<&Value> = if has_repl {
                        content
                            .iter()
                            .filter(|b| {
                                !(b.get("type").and_then(|v| v.as_str()) == Some("tool_result")
                                    && b.get("tool_use_id")
                                        .and_then(|v| v.as_str())
                                        .is_some_and(|id| repl_ids.contains(id)))
                            })
                            .collect()
                    } else {
                        content.iter().collect()
                    };
                    if filtered.is_empty() {
                        continue;
                    }
                    let mut msg = m.clone();
                    if let Some(obj) = msg.as_object_mut() {
                        obj.remove("isVirtual");
                        if has_repl {
                            if let Some(message) = obj.get_mut("message") {
                                if let Some(msg_obj) = message.as_object_mut() {
                                    msg_obj.insert(
                                        "content".to_string(),
                                        Value::Array(filtered.into_iter().cloned().collect()),
                                    );
                                }
                            }
                        }
                    }
                    result.push(msg);
                } else {
                    let mut msg = m.clone();
                    if let Some(obj) = msg.as_object_mut() {
                        obj.remove("isVirtual");
                    }
                    result.push(msg);
                }
            }
            _ => {
                let mut msg = m.clone();
                if let Some(obj) = msg.as_object_mut() {
                    obj.remove("isVirtual");
                }
                result.push(msg);
            }
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Conversation chain building
// ---------------------------------------------------------------------------

/// Build a conversation chain from a leaf message to root.
pub fn build_conversation_chain(
    messages: &HashMap<String, Value>,
    leaf_message: &Value,
) -> Vec<Value> {
    let mut transcript: Vec<Value> = Vec::new();
    let mut seen = HashSet::new();
    let mut current_msg: Option<&Value> = Some(leaf_message);

    while let Some(msg) = current_msg {
        let uuid = msg
            .get("uuid")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if seen.contains(&uuid) {
            tracing::error!(
                "Cycle detected in parentUuid chain at message {}. Returning partial transcript.",
                uuid
            );
            break;
        }
        seen.insert(uuid.clone());
        transcript.push(msg.clone());

        current_msg = msg
            .get("parentUuid")
            .and_then(|v| v.as_str())
            .and_then(|parent_uuid| messages.get(parent_uuid));
    }

    transcript.reverse();
    recover_orphaned_parallel_tool_results(messages, transcript, &seen)
}

/// Post-pass for buildConversationChain: recover sibling assistant blocks and tool_results.
fn recover_orphaned_parallel_tool_results(
    messages: &HashMap<String, Value>,
    chain: Vec<Value>,
    seen: &HashSet<String>,
) -> Vec<Value> {
    let chain_assistants: Vec<&Value> = chain
        .iter()
        .filter(|m| m.get("type").and_then(|v| v.as_str()) == Some("assistant"))
        .collect();
    if chain_assistants.is_empty() {
        return chain;
    }

    // Build anchor map by message.id
    let mut anchor_by_msg_id: HashMap<String, usize> = HashMap::new();
    for (idx, a) in chain.iter().enumerate() {
        if a.get("type").and_then(|v| v.as_str()) == Some("assistant") {
            if let Some(msg_id) = a
                .get("message")
                .and_then(|m| m.get("id"))
                .and_then(|v| v.as_str())
            {
                anchor_by_msg_id.insert(msg_id.to_string(), idx);
            }
        }
    }

    // Build sibling groups and TR index
    let mut siblings_by_msg_id: HashMap<String, Vec<&Value>> = HashMap::new();
    let mut tool_results_by_asst: HashMap<String, Vec<&Value>> = HashMap::new();
    for m in messages.values() {
        let msg_type = m.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if msg_type == "assistant" {
            if let Some(msg_id) = m
                .get("message")
                .and_then(|msg| msg.get("id"))
                .and_then(|v| v.as_str())
            {
                siblings_by_msg_id
                    .entry(msg_id.to_string())
                    .or_default()
                    .push(m);
            }
        } else if msg_type == "user" {
            if let Some(parent_uuid) = m.get("parentUuid").and_then(|v| v.as_str()) {
                if let Some(content) = m
                    .get("message")
                    .and_then(|msg| msg.get("content"))
                    .and_then(|c| c.as_array())
                {
                    if content
                        .iter()
                        .any(|b| b.get("type").and_then(|v| v.as_str()) == Some("tool_result"))
                    {
                        tool_results_by_asst
                            .entry(parent_uuid.to_string())
                            .or_default()
                            .push(m);
                    }
                }
            }
        }
    }

    // Collect inserts
    let mut processed_groups = HashSet::new();
    let mut inserts: HashMap<usize, Vec<Value>> = HashMap::new();
    let mut recovered_count = 0;

    for a in &chain_assistants {
        let msg_id = match a
            .get("message")
            .and_then(|m| m.get("id"))
            .and_then(|v| v.as_str())
        {
            Some(id) => id.to_string(),
            None => continue,
        };
        if processed_groups.contains(&msg_id) {
            continue;
        }
        processed_groups.insert(msg_id.clone());

        let group = siblings_by_msg_id.get(&msg_id);
        let orphaned_siblings: Vec<Value> = group
            .map(|g| {
                g.iter()
                    .filter(|s| {
                        let uuid = s.get("uuid").and_then(|v| v.as_str()).unwrap_or("");
                        !seen.contains(uuid)
                    })
                    .cloned()
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();

        let mut orphaned_trs: Vec<Value> = Vec::new();
        if let Some(group) = group {
            for member in group {
                let member_uuid = member.get("uuid").and_then(|v| v.as_str()).unwrap_or("");
                if let Some(trs) = tool_results_by_asst.get(member_uuid) {
                    for tr in trs {
                        let tr_uuid = tr.get("uuid").and_then(|v| v.as_str()).unwrap_or("");
                        if !seen.contains(tr_uuid) {
                            orphaned_trs.push((*tr).clone());
                        }
                    }
                }
            }
        }

        if orphaned_siblings.is_empty() && orphaned_trs.is_empty() {
            continue;
        }

        if let Some(&anchor_idx) = anchor_by_msg_id.get(&msg_id) {
            let mut recovered: Vec<Value> = Vec::new();
            recovered.extend(orphaned_siblings);
            recovered.extend(orphaned_trs);
            recovered_count += recovered.len();
            inserts.insert(anchor_idx, recovered);
        }
    }

    if recovered_count == 0 {
        return chain;
    }

    let mut result: Vec<Value> = Vec::new();
    for (i, m) in chain.into_iter().enumerate() {
        result.push(m);
        if let Some(to_insert) = inserts.remove(&i) {
            result.extend(to_insert);
        }
    }
    result
}

/// Get session messages (UUID set) for deduplication.
async fn get_session_messages(session_id: &str, ctx: &SessionContext) -> Result<HashSet<String>> {
    let session_file = get_transcript_path_for_session(session_id, ctx);
    match tokio::fs::read(&session_file).await {
        Ok(data) => {
            let entries = parse_jsonl(&data);
            let mut uuids = HashSet::new();
            for entry in &entries {
                if is_transcript_message(entry) {
                    if let Some(uuid) = entry.get("uuid").and_then(|v| v.as_str()) {
                        uuids.insert(uuid.to_string());
                    }
                }
            }
            Ok(uuids)
        }
        Err(_) => Ok(HashSet::new()),
    }
}

// ---------------------------------------------------------------------------
// Transcript loading
// ---------------------------------------------------------------------------

/// Load a transcript file, extracting all messages, metadata, and snapshots.
pub async fn load_transcript_file(
    file_path: &Path,
    _keep_all_leaves: bool,
) -> Result<LoadTranscriptResult> {
    let mut messages: HashMap<String, Value> = HashMap::new();
    let mut summaries: HashMap<String, String> = HashMap::new();
    let mut custom_titles: HashMap<String, String> = HashMap::new();
    let mut tags: HashMap<String, String> = HashMap::new();
    let mut agent_names: HashMap<String, String> = HashMap::new();
    let mut agent_colors: HashMap<String, String> = HashMap::new();
    let mut agent_settings: HashMap<String, String> = HashMap::new();
    let mut pr_numbers: HashMap<String, u64> = HashMap::new();
    let mut pr_urls: HashMap<String, String> = HashMap::new();
    let mut pr_repositories: HashMap<String, String> = HashMap::new();
    let mut modes: HashMap<String, String> = HashMap::new();
    let mut worktree_states: HashMap<String, Option<PersistedWorktreeSession>> = HashMap::new();
    let mut file_history_snapshots: HashMap<String, FileHistorySnapshotMessage> = HashMap::new();
    let mut attribution_snapshots: HashMap<String, AttributionSnapshotMessage> = HashMap::new();
    let mut content_replacements: HashMap<String, Vec<Value>> = HashMap::new();
    let mut agent_content_replacements: HashMap<String, Vec<Value>> = HashMap::new();
    let mut context_collapse_commits: Vec<ContextCollapseCommitEntry> = Vec::new();
    let mut context_collapse_snapshot: Option<ContextCollapseSnapshotEntry> = None;

    let data = match tokio::fs::read(file_path).await {
        Ok(d) => d,
        Err(_) => {
            return Ok(LoadTranscriptResult {
                messages,
                summaries,
                custom_titles,
                tags,
                agent_names,
                agent_colors,
                agent_settings,
                pr_numbers,
                pr_urls,
                pr_repositories,
                modes,
                worktree_states,
                file_history_snapshots,
                attribution_snapshots,
                content_replacements,
                agent_content_replacements,
                context_collapse_commits,
                context_collapse_snapshot,
                leaf_uuids: HashSet::new(),
            });
        }
    };

    // Progress bridge for legacy progress entries
    let mut progress_bridge: HashMap<String, Option<String>> = HashMap::new();

    let entries = parse_jsonl(&data);
    for entry in entries {
        let entry_type = entry.get("type").and_then(|v| v.as_str()).unwrap_or("");

        // Check for legacy progress entries first
        if is_legacy_progress_entry(&entry) {
            let uuid = entry
                .get("uuid")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let parent = entry
                .get("parentUuid")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let resolved = if let Some(ref p) = parent {
                if progress_bridge.contains_key(p) {
                    progress_bridge.get(p).cloned().flatten()
                } else {
                    parent.clone()
                }
            } else {
                None
            };
            progress_bridge.insert(uuid, resolved);
            continue;
        }

        if is_transcript_message(&entry) {
            let mut entry = entry;
            // Bridge progress parents
            if let Some(parent_uuid) = entry.get("parentUuid").and_then(|v| v.as_str()) {
                if progress_bridge.contains_key(parent_uuid) {
                    let bridged = progress_bridge.get(parent_uuid).cloned().flatten();
                    if let Some(obj) = entry.as_object_mut() {
                        obj.insert(
                            "parentUuid".to_string(),
                            match bridged {
                                Some(u) => Value::String(u),
                                None => Value::Null,
                            },
                        );
                    }
                }
            }
            let uuid = entry
                .get("uuid")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if !uuid.is_empty() {
                messages.insert(uuid, entry.clone());
            }
            // Compact boundary: clear collapse data
            if is_compact_boundary_message(&entry) {
                context_collapse_commits.clear();
                context_collapse_snapshot = None;
            }
        } else {
            let session_id = entry
                .get("sessionId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            match entry_type {
                "summary" => {
                    if let Some(leaf_uuid) = entry.get("leafUuid").and_then(|v| v.as_str()) {
                        if let Some(summary) = entry.get("summary").and_then(|v| v.as_str()) {
                            summaries.insert(leaf_uuid.to_string(), summary.to_string());
                        }
                    }
                }
                "custom-title" => {
                    if let Some(title) = entry.get("customTitle").and_then(|v| v.as_str()) {
                        custom_titles.insert(session_id, title.to_string());
                    }
                }
                "tag" => {
                    if let Some(tag) = entry.get("tag").and_then(|v| v.as_str()) {
                        tags.insert(session_id, tag.to_string());
                    }
                }
                "agent-name" => {
                    if let Some(name) = entry.get("agentName").and_then(|v| v.as_str()) {
                        agent_names.insert(session_id, name.to_string());
                    }
                }
                "agent-color" => {
                    if let Some(color) = entry.get("agentColor").and_then(|v| v.as_str()) {
                        agent_colors.insert(session_id, color.to_string());
                    }
                }
                "agent-setting" => {
                    if let Some(setting) = entry.get("agentSetting").and_then(|v| v.as_str()) {
                        agent_settings.insert(session_id, setting.to_string());
                    }
                }
                "mode" => {
                    if let Some(mode) = entry.get("mode").and_then(|v| v.as_str()) {
                        modes.insert(session_id, mode.to_string());
                    }
                }
                "worktree-state" => {
                    let wt = entry
                        .get("worktreeSession")
                        .and_then(|v| serde_json::from_value(v.clone()).ok());
                    worktree_states.insert(session_id, wt);
                }
                "pr-link" => {
                    if let Some(num) = entry.get("prNumber").and_then(|v| v.as_u64()) {
                        pr_numbers.insert(session_id.clone(), num);
                    }
                    if let Some(url) = entry.get("prUrl").and_then(|v| v.as_str()) {
                        pr_urls.insert(session_id.clone(), url.to_string());
                    }
                    if let Some(repo) = entry.get("prRepository").and_then(|v| v.as_str()) {
                        pr_repositories.insert(session_id, repo.to_string());
                    }
                }
                "file-history-snapshot" => {
                    if let Some(msg_id) = entry.get("messageId").and_then(|v| v.as_str()) {
                        if let Ok(snapshot) =
                            serde_json::from_value::<FileHistorySnapshotMessage>(entry.clone())
                        {
                            file_history_snapshots.insert(msg_id.to_string(), snapshot);
                        }
                    }
                }
                "attribution-snapshot" => {
                    if let Some(msg_id) = entry.get("messageId").and_then(|v| v.as_str()) {
                        if let Ok(snapshot) =
                            serde_json::from_value::<AttributionSnapshotMessage>(entry.clone())
                        {
                            attribution_snapshots.insert(msg_id.to_string(), snapshot);
                        }
                    }
                }
                "content-replacement" => {
                    if let Some(replacements) = entry.get("replacements").and_then(|v| v.as_array())
                    {
                        if let Some(agent_id) = entry.get("agentId").and_then(|v| v.as_str()) {
                            agent_content_replacements
                                .entry(agent_id.to_string())
                                .or_default()
                                .extend(replacements.iter().cloned());
                        } else {
                            content_replacements
                                .entry(session_id)
                                .or_default()
                                .extend(replacements.iter().cloned());
                        }
                    }
                }
                "marble-origami-commit" => {
                    if let Ok(commit) =
                        serde_json::from_value::<ContextCollapseCommitEntry>(entry.clone())
                    {
                        context_collapse_commits.push(commit);
                    }
                }
                "marble-origami-snapshot" => {
                    context_collapse_snapshot =
                        serde_json::from_value::<ContextCollapseSnapshotEntry>(entry.clone()).ok();
                }
                _ => {}
            }
        }
    }

    // Apply preserved segment relinks
    apply_preserved_segment_relinks(&mut messages);
    // Apply snip removals
    apply_snip_removals(&mut messages);

    // Compute leaf UUIDs
    let all_messages: Vec<&Value> = messages.values().collect();
    let parent_uuids: HashSet<String> = all_messages
        .iter()
        .filter_map(|msg| msg.get("parentUuid").and_then(|v| v.as_str()))
        .map(|s| s.to_string())
        .collect();

    let terminal_messages: Vec<&Value> = all_messages
        .iter()
        .filter(|msg| {
            let uuid = msg.get("uuid").and_then(|v| v.as_str()).unwrap_or("");
            !parent_uuids.contains(uuid)
        })
        .copied()
        .collect();

    let mut leaf_uuids = HashSet::new();
    for terminal in &terminal_messages {
        let mut seen = HashSet::new();
        let mut current: Option<&Value> = Some(terminal);
        while let Some(msg) = current {
            let uuid = msg
                .get("uuid")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if seen.contains(&uuid) {
                break;
            }
            seen.insert(uuid.clone());
            let msg_type = msg.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if msg_type == "user" || msg_type == "assistant" {
                leaf_uuids.insert(uuid);
                break;
            }
            current = msg
                .get("parentUuid")
                .and_then(|v| v.as_str())
                .and_then(|parent_uuid| messages.get(parent_uuid));
        }
    }

    Ok(LoadTranscriptResult {
        messages,
        summaries,
        custom_titles,
        tags,
        agent_names,
        agent_colors,
        agent_settings,
        pr_numbers,
        pr_urls,
        pr_repositories,
        modes,
        worktree_states,
        file_history_snapshots,
        attribution_snapshots,
        content_replacements,
        agent_content_replacements,
        context_collapse_commits,
        context_collapse_snapshot,
        leaf_uuids,
    })
}

/// Apply preserved segment relinks. Mutates the map in place.
fn apply_preserved_segment_relinks(messages: &mut HashMap<String, Value>) {
    // Find the absolute-last boundary and the last seg-boundary
    let mut last_seg_metadata: Option<Value> = None;
    let mut last_seg_boundary_idx: i64 = -1;
    let mut absolute_last_boundary_idx: i64 = -1;
    let mut entry_index: HashMap<String, usize> = HashMap::new();

    let entries: Vec<(String, Value)> = messages
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    for (i, (uuid, entry)) in entries.iter().enumerate() {
        entry_index.insert(uuid.clone(), i);
        if is_compact_boundary_message(entry) {
            absolute_last_boundary_idx = i as i64;
            if let Some(seg) = entry
                .get("compactMetadata")
                .and_then(|m| m.get("preservedSegment"))
            {
                last_seg_metadata = Some(seg.clone());
                last_seg_boundary_idx = i as i64;
            }
        }
    }

    let last_seg = match last_seg_metadata {
        Some(seg) => seg,
        None => return,
    };

    let seg_is_live = last_seg_boundary_idx == absolute_last_boundary_idx;

    let head_uuid = last_seg
        .get("headUuid")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let tail_uuid = last_seg
        .get("tailUuid")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let anchor_uuid = last_seg
        .get("anchorUuid")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let mut preserved_uuids = HashSet::new();

    if seg_is_live {
        let mut walk_seen = HashSet::new();
        let mut cur_uuid = tail_uuid.clone();
        let mut reached_head = false;

        loop {
            if walk_seen.contains(&cur_uuid) {
                break;
            }
            walk_seen.insert(cur_uuid.clone());
            preserved_uuids.insert(cur_uuid.clone());

            if cur_uuid == head_uuid {
                reached_head = true;
                break;
            }

            match messages.get(&cur_uuid) {
                Some(msg) => match msg.get("parentUuid").and_then(|v| v.as_str()) {
                    Some(parent) => cur_uuid = parent.to_string(),
                    None => break,
                },
                None => break,
            }
        }

        if !reached_head {
            return;
        }

        // Relink head → anchor
        if let Some(head) = messages.get(&head_uuid).cloned() {
            let mut new_head = head;
            if let Some(obj) = new_head.as_object_mut() {
                obj.insert("parentUuid".to_string(), Value::String(anchor_uuid.clone()));
            }
            messages.insert(head_uuid.clone(), new_head);
        }

        // Tail-splice: anchor's other children → tail
        let keys: Vec<String> = messages.keys().cloned().collect();
        for uuid in &keys {
            if uuid == &head_uuid {
                continue;
            }
            if let Some(msg) = messages.get(uuid) {
                if msg.get("parentUuid").and_then(|v| v.as_str()) == Some(&anchor_uuid) {
                    let mut new_msg = msg.clone();
                    if let Some(obj) = new_msg.as_object_mut() {
                        obj.insert("parentUuid".to_string(), Value::String(tail_uuid.clone()));
                    }
                    messages.insert(uuid.clone(), new_msg);
                }
            }
        }

        // Zero stale usage for preserved messages
        for uuid in &preserved_uuids {
            if let Some(msg) = messages.get(uuid) {
                if msg.get("type").and_then(|v| v.as_str()) == Some("assistant") {
                    let mut new_msg = msg.clone();
                    if let Some(obj) = new_msg.as_object_mut() {
                        if let Some(message) = obj.get_mut("message") {
                            if let Some(msg_obj) = message.as_object_mut() {
                                msg_obj.insert(
                                    "usage".to_string(),
                                    serde_json::json!({
                                        "input_tokens": 0,
                                        "output_tokens": 0,
                                        "cache_creation_input_tokens": 0,
                                        "cache_read_input_tokens": 0,
                                    }),
                                );
                            }
                        }
                    }
                    messages.insert(uuid.clone(), new_msg);
                }
            }
        }
    }

    // Prune everything before the absolute-last boundary that isn't preserved
    let to_delete: Vec<String> = messages
        .keys()
        .filter(|uuid| {
            if let Some(&idx) = entry_index.get(*uuid) {
                (idx as i64) < absolute_last_boundary_idx && !preserved_uuids.contains(*uuid)
            } else {
                false
            }
        })
        .cloned()
        .collect();
    for uuid in to_delete {
        messages.remove(&uuid);
    }
}

/// Apply snip removals. Mutates the map in place.
fn apply_snip_removals(messages: &mut HashMap<String, Value>) {
    let mut to_delete = HashSet::new();
    for entry in messages.values() {
        if let Some(removed_uuids) = entry
            .get("snipMetadata")
            .and_then(|m| m.get("removedUuids"))
            .and_then(|v| v.as_array())
        {
            for uuid in removed_uuids {
                if let Some(u) = uuid.as_str() {
                    to_delete.insert(u.to_string());
                }
            }
        }
    }
    if to_delete.is_empty() {
        return;
    }

    // Capture parent links before deleting
    let mut deleted_parent: HashMap<String, Option<String>> = HashMap::new();
    for uuid in &to_delete {
        if let Some(entry) = messages.get(uuid) {
            let parent = entry
                .get("parentUuid")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            deleted_parent.insert(uuid.clone(), parent);
        }
        messages.remove(uuid);
    }

    // Resolve function with path compression
    fn resolve(
        start: &str,
        to_delete: &HashSet<String>,
        deleted_parent: &mut HashMap<String, Option<String>>,
    ) -> Option<String> {
        let mut path: Vec<String> = Vec::new();
        let mut cur: Option<String> = Some(start.to_string());
        while let Some(ref c) = cur {
            if !to_delete.contains(c) {
                break;
            }
            path.push(c.clone());
            cur = match deleted_parent.get(c) {
                Some(Some(p)) => Some(p.clone()),
                _ => None,
            };
        }
        for p in &path {
            deleted_parent.insert(p.clone(), cur.clone());
        }
        cur
    }

    // Relink survivors
    let keys: Vec<String> = messages.keys().cloned().collect();
    for uuid in keys {
        let parent_uuid = messages
            .get(&uuid)
            .and_then(|m| m.get("parentUuid"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        if let Some(ref pu) = parent_uuid {
            if to_delete.contains(pu) {
                let resolved = resolve(pu, &to_delete, &mut deleted_parent);
                if let Some(msg) = messages.get_mut(&uuid) {
                    if let Some(obj) = msg.as_object_mut() {
                        obj.insert(
                            "parentUuid".to_string(),
                            match resolved {
                                Some(u) => Value::String(u),
                                None => Value::Null,
                            },
                        );
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Message content helpers
// ---------------------------------------------------------------------------

/// Get the first meaningful user message text content.
pub fn get_first_meaningful_user_message_text_content(transcript: &[Value]) -> Option<String> {
    for msg in transcript {
        let msg_type = msg.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if msg_type != "user" {
            continue;
        }
        if msg.get("isMeta").and_then(|v| v.as_bool()).unwrap_or(false) {
            continue;
        }
        if msg
            .get("isCompactSummary")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            continue;
        }

        let content = msg.get("message").and_then(|m| m.get("content"));
        let content = match content {
            Some(c) => c,
            None => continue,
        };

        let mut texts: Vec<String> = Vec::new();
        if let Some(s) = content.as_str() {
            texts.push(s.to_string());
        } else if let Some(arr) = content.as_array() {
            for block in arr {
                if block.get("type").and_then(|v| v.as_str()) == Some("text") {
                    if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                        texts.push(text.to_string());
                    }
                }
            }
        }

        for text_content in &texts {
            if text_content.is_empty() {
                continue;
            }
            // Check for command name tag
            if let Some(command_name) = extract_tag(text_content, "command-name") {
                let _clean_name = command_name.trim_start_matches('/');
                // Built-in commands are not meaningful
                // For custom commands, check for args
                let command_args = extract_tag(text_content, "command-args")
                    .map(|a| a.trim().to_string())
                    .filter(|a| !a.is_empty());
                if let Some(args) = command_args {
                    return Some(format!("{} {}", command_name, args));
                }
                continue;
            }

            // Check for bash input
            if let Some(bash_input) = extract_tag(text_content, "bash-input") {
                return Some(format!("! {}", bash_input));
            }

            // Skip non-meaningful messages
            if SKIP_FIRST_PROMPT_PATTERN.is_match(text_content) {
                continue;
            }

            return Some(text_content.clone());
        }
    }
    None
}

/// Extract first prompt from transcript messages.
fn extract_first_prompt(transcript: &[Value]) -> String {
    if let Some(text) = get_first_meaningful_user_message_text_content(transcript) {
        let mut result = text.replace('\n', " ").trim().to_string();
        if result.chars().count() > 200 {
            result = truncate_chars(result.trim(), 200);
        }
        return result;
    }
    "No prompt".to_string()
}

/// Remove extra fields from transcript messages.
pub fn remove_extra_fields(transcript: &[Value]) -> Vec<Value> {
    transcript
        .iter()
        .map(|m| {
            let mut msg = m.clone();
            if let Some(obj) = msg.as_object_mut() {
                obj.remove("isSidechain");
                obj.remove("parentUuid");
            }
            msg
        })
        .collect()
}

/// Check if a user message has visible content.
fn has_visible_user_content(message: &Value) -> bool {
    if message.get("type").and_then(|v| v.as_str()) != Some("user") {
        return false;
    }
    if message
        .get("isMeta")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return false;
    }
    let content = match message.get("message").and_then(|m| m.get("content")) {
        Some(c) => c,
        None => return false,
    };
    if let Some(s) = content.as_str() {
        return !s.trim().is_empty();
    }
    if let Some(arr) = content.as_array() {
        return arr.iter().any(|block| {
            let bt = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
            bt == "text" || bt == "image" || bt == "document"
        });
    }
    false
}

/// Check if an assistant message has visible text content.
fn has_visible_assistant_content(message: &Value) -> bool {
    if message.get("type").and_then(|v| v.as_str()) != Some("assistant") {
        return false;
    }
    let content = match message
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_array())
    {
        Some(c) => c,
        None => return false,
    };
    content.iter().any(|block| {
        block.get("type").and_then(|v| v.as_str()) == Some("text")
            && block
                .get("text")
                .and_then(|v| v.as_str())
                .is_some_and(|t| !t.trim().is_empty())
    })
}

/// Count visible messages that would appear as conversation turns.
fn count_visible_messages(transcript: &[Value]) -> usize {
    let mut count = 0;
    for message in transcript {
        let msg_type = message.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match msg_type {
            "user" if has_visible_user_content(message) => {
                count += 1;
            }
            "assistant" if has_visible_assistant_content(message) => {
                count += 1;
            }
            _ => {}
        }
    }
    count
}

/// Check resume consistency.
pub fn check_resume_consistency(chain: &[Value]) {
    for i in (0..chain.len()).rev() {
        let m = &chain[i];
        if m.get("type").and_then(|v| v.as_str()) != Some("system") {
            continue;
        }
        if m.get("subtype").and_then(|v| v.as_str()) != Some("turn_duration") {
            continue;
        }
        let expected = match m.get("messageCount").and_then(|v| v.as_u64()) {
            Some(e) => e as usize,
            None => return,
        };
        let actual = i;
        tracing::info!(
            expected = expected,
            actual = actual,
            delta = (actual as i64) - (expected as i64),
            chain_length = chain.len(),
            "Resume consistency check"
        );
        return;
    }
}

// ---------------------------------------------------------------------------
// Session file listing and enrichment
// ---------------------------------------------------------------------------

/// Get all session JSONL files in a project directory with their stats.
pub async fn get_session_files_with_mtime(
    project_dir: &Path,
) -> Result<HashMap<String, SessionFileInfo>> {
    let mut session_files = HashMap::new();
    let mut entries = match tokio::fs::read_dir(project_dir).await {
        Ok(e) => e,
        Err(_) => return Ok(session_files),
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.ends_with(".jsonl") {
            continue;
        }
        let session_id = name.trim_end_matches(".jsonl");
        // Basic UUID validation
        if session_id.len() != 36 {
            continue;
        }
        let path = entry.path();
        if let Ok(metadata) = tokio::fs::metadata(&path).await {
            let mtime = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            let ctime = metadata
                .created()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            session_files.insert(
                session_id.to_string(),
                SessionFileInfo {
                    path,
                    mtime,
                    ctime,
                    size: metadata.len(),
                },
            );
        }
    }

    Ok(session_files)
}

/// Session file information.
#[derive(Debug, Clone)]
pub struct SessionFileInfo {
    pub path: PathBuf,
    pub mtime: u64,
    pub ctime: u64,
    pub size: u64,
}

/// Get session files in lite format (stat only, no file reads).
pub async fn get_session_files_lite(
    project_dir: &Path,
    limit: Option<usize>,
    project_path: Option<&str>,
) -> Result<Vec<LogOption>> {
    let session_files_map = get_session_files_with_mtime(project_dir).await?;

    let mut entries: Vec<(String, SessionFileInfo)> = session_files_map.into_iter().collect();
    entries.sort_by(|a, b| b.1.mtime.cmp(&a.1.mtime));

    if let Some(limit) = limit {
        entries.truncate(limit);
    }

    let mut logs = Vec::new();
    for (session_id, file_info) in &entries {
        let mtime_str = chrono::DateTime::from_timestamp_millis(file_info.mtime as i64)
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_default();
        let created = chrono::DateTime::from_timestamp_millis(file_info.ctime as i64)
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_default();

        let log = LogOption {
            date: mtime_str.clone(),
            messages: Vec::new(),
            full_path: Some(file_info.path.to_string_lossy().to_string()),
            value: 0,
            created,
            modified: mtime_str,
            first_prompt: String::new(),
            message_count: 0,
            file_size: Some(file_info.size),
            is_sidechain: false,
            is_lite: Some(true),
            session_id: Some(session_id.clone()),
            project_path: project_path.map(|p| p.to_string()),
            team_name: None,
            agent_name: None,
            agent_color: None,
            agent_setting: None,
            is_teammate: None,
            leaf_uuid: None,
            summary: None,
            custom_title: None,
            tag: None,
            file_history_snapshots: None,
            attribution_snapshots: None,
            context_collapse_commits: None,
            context_collapse_snapshot: None,
            pr_number: None,
            pr_url: None,
            pr_repository: None,
            mode: None,
            worktree_session: None,
            content_replacements: None,
        };
        logs.push(log);
    }

    // Sort and assign indices
    mossen_types::logs::sort_logs(&mut logs);
    for (i, log) in logs.iter_mut().enumerate() {
        log.value = i as i64;
    }

    Ok(logs)
}

/// Fetch logs for the current project.
pub async fn fetch_logs(limit: Option<usize>, ctx: &SessionContext) -> Result<Vec<LogOption>> {
    let project_dir = get_project_dir(&ctx.original_cwd, ctx);
    let logs = get_session_files_lite(
        &project_dir,
        limit,
        Some(&ctx.original_cwd.to_string_lossy()),
    )
    .await?;
    Ok(logs)
}

/// Load full messages for a lite log.
pub async fn load_full_log(log: &LogOption) -> Result<LogOption> {
    if !is_lite_log(log) {
        return Ok(log.clone());
    }
    let session_file = match &log.full_path {
        Some(p) => PathBuf::from(p),
        None => return Ok(log.clone()),
    };

    match load_transcript_file(&session_file, false).await {
        Ok(result) => {
            if result.messages.is_empty() {
                return Ok(log.clone());
            }
            let most_recent_leaf = find_latest_message(result.messages.values(), |msg| {
                let uuid = msg.get("uuid").and_then(|v| v.as_str()).unwrap_or("");
                let msg_type = msg.get("type").and_then(|v| v.as_str()).unwrap_or("");
                result.leaf_uuids.contains(uuid) && (msg_type == "user" || msg_type == "assistant")
            });
            match most_recent_leaf {
                Some(leaf) => {
                    let transcript = build_conversation_chain(&result.messages, leaf);
                    let session_id = leaf
                        .get("sessionId")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let mut enriched = log.clone();
                    enriched.messages = Vec::new(); // Would need proper conversion
                    enriched.first_prompt = extract_first_prompt(&transcript);
                    enriched.message_count = count_visible_messages(&transcript);
                    if let Some(title) = result.custom_titles.get(&session_id) {
                        enriched.custom_title = Some(title.clone());
                    }
                    if let Some(tag) = result.tags.get(&session_id) {
                        enriched.tag = Some(tag.clone());
                    }
                    if let Some(name) = result.agent_names.get(&session_id) {
                        enriched.agent_name = Some(name.clone());
                    }
                    if let Some(color) = result.agent_colors.get(&session_id) {
                        enriched.agent_color = Some(color.clone());
                    }
                    if let Some(setting) = result.agent_settings.get(&session_id) {
                        enriched.agent_setting = Some(setting.clone());
                    }
                    if let Some(mode) = result.modes.get(&session_id) {
                        enriched.mode = match mode.as_str() {
                            "coordinator" => Some(SessionMode::Coordinator),
                            _ => Some(SessionMode::Normal),
                        };
                    }
                    Ok(enriched)
                }
                None => Ok(log.clone()),
            }
        }
        Err(_) => Ok(log.clone()),
    }
}

/// Load transcript from a file (JSON or JSONL).
pub async fn load_transcript_from_file(file_path: &Path) -> Result<LogOption> {
    if file_path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
        let result = load_transcript_file(file_path, false).await?;
        if result.messages.is_empty() {
            return Err(anyhow!("No messages found in JSONL file"));
        }
        let leaf_message = find_latest_message(result.messages.values(), |msg| {
            let uuid = msg.get("uuid").and_then(|v| v.as_str()).unwrap_or("");
            result.leaf_uuids.contains(uuid)
        });
        match leaf_message {
            Some(leaf) => {
                let transcript = build_conversation_chain(&result.messages, leaf);
                let first_prompt = extract_first_prompt(&transcript);
                let message_count = count_visible_messages(&transcript);
                let session_id = leaf
                    .get("sessionId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let summary = result
                    .summaries
                    .get(leaf.get("uuid").and_then(|v| v.as_str()).unwrap_or(""));
                let custom_title = result.custom_titles.get(&session_id);
                let tag = result.tags.get(&session_id);

                Ok(LogOption {
                    date: leaf
                        .get("timestamp")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    messages: Vec::new(), // Would need conversion
                    full_path: Some(file_path.to_string_lossy().to_string()),
                    value: 0,
                    created: transcript
                        .first()
                        .and_then(|m| m.get("timestamp"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    modified: leaf
                        .get("timestamp")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    first_prompt,
                    message_count,
                    is_sidechain: transcript
                        .first()
                        .and_then(|m| m.get("isSidechain"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
                    summary: summary.cloned(),
                    custom_title: custom_title.cloned(),
                    tag: tag.cloned(),
                    leaf_uuid: leaf
                        .get("uuid")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    file_size: None,
                    is_lite: None,
                    session_id: None,
                    project_path: None,
                    team_name: None,
                    agent_name: None,
                    agent_color: None,
                    agent_setting: None,
                    is_teammate: None,
                    file_history_snapshots: None,
                    attribution_snapshots: None,
                    context_collapse_commits: None,
                    context_collapse_snapshot: None,
                    pr_number: None,
                    pr_url: None,
                    pr_repository: None,
                    mode: None,
                    worktree_session: None,
                    content_replacements: None,
                })
            }
            None => Err(anyhow!("No valid conversation chain found in JSONL file")),
        }
    } else {
        // JSON log files
        let content = tokio::fs::read_to_string(file_path).await?;
        let parsed: Value = serde_json::from_str(&content)
            .map_err(|e| anyhow!("Invalid JSON in transcript file: {}", e))?;

        let messages: Vec<Value> = if let Some(arr) = parsed.as_array() {
            arr.clone()
        } else if let Some(obj) = parsed.as_object() {
            if let Some(msgs) = obj.get("messages").and_then(|v| v.as_array()) {
                msgs.clone()
            } else {
                return Err(anyhow!(
                    "Transcript must be an array or object with messages array"
                ));
            }
        } else {
            return Err(anyhow!(
                "Transcript must be an array or object with messages array"
            ));
        };

        let first_prompt = extract_first_prompt(&messages);
        let message_count = count_visible_messages(&messages);

        Ok(LogOption {
            date: messages
                .last()
                .and_then(|m| m.get("timestamp"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            messages: Vec::new(),
            full_path: Some(file_path.to_string_lossy().to_string()),
            value: 0,
            created: messages
                .first()
                .and_then(|m| m.get("timestamp"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            modified: messages
                .last()
                .and_then(|m| m.get("timestamp"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            first_prompt,
            message_count,
            is_sidechain: false,
            is_lite: None,
            session_id: None,
            project_path: None,
            team_name: None,
            agent_name: None,
            agent_color: None,
            agent_setting: None,
            is_teammate: None,
            leaf_uuid: None,
            summary: None,
            custom_title: None,
            tag: None,
            file_size: None,
            file_history_snapshots: None,
            attribution_snapshots: None,
            context_collapse_commits: None,
            context_collapse_snapshot: None,
            pr_number: None,
            pr_url: None,
            pr_repository: None,
            mode: None,
            worktree_session: None,
            content_replacements: None,
        })
    }
}

/// Hydrate a remote session.
pub async fn hydrate_remote_session(
    project: &mut Project,
    session_id: &str,
    ingress_url: &str,
    ctx: &SessionContext,
) -> Result<bool> {
    let project_dir = get_project_dir(&ctx.original_cwd, ctx);
    tokio::fs::create_dir_all(&project_dir).await?;
    let session_file = get_transcript_path_for_session(session_id, ctx);
    // Note: actual remote fetching would be done via the injected ingress service
    // For now, create an empty file
    tokio::fs::write(&session_file, "").await?;
    project.set_remote_ingress_url(ingress_url.to_string());
    Ok(false)
}

/// Search sessions by custom title.
pub async fn search_sessions_by_custom_title(
    query: &str,
    limit: Option<usize>,
    exact: bool,
    ctx: &SessionContext,
) -> Result<Vec<LogOption>> {
    let logs = fetch_logs(None, ctx).await?;
    let normalized_query = query.to_lowercase().trim().to_string();

    let matching: Vec<LogOption> = logs
        .into_iter()
        .filter(|log| {
            if let Some(ref title) = log.custom_title {
                let t = title.to_lowercase().trim().to_string();
                if exact {
                    t == normalized_query
                } else {
                    t.contains(&normalized_query)
                }
            } else {
                false
            }
        })
        .collect();

    // Deduplicate by sessionId
    let mut session_to_log: HashMap<String, LogOption> = HashMap::new();
    for log in matching {
        if let Some(ref sid) = log.session_id {
            let existing = session_to_log.get(sid);
            if existing.is_none() || log.modified > existing.unwrap().modified {
                session_to_log.insert(sid.clone(), log);
            }
        }
    }

    let mut result: Vec<LogOption> = session_to_log.into_values().collect();
    result.sort_by(|a, b| b.modified.cmp(&a.modified));

    if let Some(limit) = limit {
        result.truncate(limit);
    }

    Ok(result)
}

/// Extract agent IDs from progress messages.
pub fn extract_agent_ids_from_messages(messages: &[Value]) -> Vec<String> {
    let mut agent_ids = Vec::new();
    for message in messages {
        if message.get("type").and_then(|v| v.as_str()) != Some("progress") {
            continue;
        }
        if let Some(data) = message.get("data") {
            let data_type = data.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if data_type == "agent_progress" || data_type == "skill_progress" {
                if let Some(aid) = data.get("agentId").and_then(|v| v.as_str()) {
                    agent_ids.push(aid.to_string());
                }
            }
        }
    }
    // Deduplicate
    let mut seen = HashSet::new();
    agent_ids.retain(|id| seen.insert(id.clone()));
    agent_ids
}

/// Extract teammate transcripts from tasks.
pub fn extract_teammate_transcripts_from_tasks(
    tasks: &HashMap<String, Value>,
) -> HashMap<String, Vec<Value>> {
    let mut transcripts = HashMap::new();
    for task in tasks.values() {
        let task_type = task.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if task_type != "in_process_teammate" {
            continue;
        }
        let agent_id = task
            .get("identity")
            .and_then(|i| i.get("agentId"))
            .and_then(|v| v.as_str());
        let messages = task.get("messages").and_then(|v| v.as_array());
        if let (Some(aid), Some(msgs)) = (agent_id, messages) {
            if !msgs.is_empty() {
                transcripts.insert(aid.to_string(), msgs.clone());
            }
        }
    }
    transcripts
}

/// Load subagent transcripts.
pub async fn load_subagent_transcripts(
    agent_ids: &[String],
    ctx: &SessionContext,
) -> Result<HashMap<String, Vec<Value>>> {
    let mut transcripts = HashMap::new();
    for agent_id in agent_ids {
        match get_agent_transcript(agent_id, ctx).await {
            Ok(Some((messages, _))) => {
                if !messages.is_empty() {
                    transcripts.insert(agent_id.clone(), messages);
                }
            }
            _ => continue,
        }
    }
    Ok(transcripts)
}

/// Get agent transcript.
pub async fn get_agent_transcript(
    agent_id: &str,
    ctx: &SessionContext,
) -> Result<Option<(Vec<Value>, Vec<Value>)>> {
    let agent_file = get_agent_transcript_path(agent_id, ctx);
    let result = match load_transcript_file(&agent_file, false).await {
        Ok(r) => r,
        Err(_) => return Ok(None),
    };

    let agent_messages: Vec<&Value> = result
        .messages
        .values()
        .filter(|msg| {
            msg.get("agentId").and_then(|v| v.as_str()) == Some(agent_id)
                && msg
                    .get("isSidechain")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
        })
        .collect::<Vec<_>>();

    if agent_messages.is_empty() {
        return Ok(None);
    }

    let parent_uuids: HashSet<String> = agent_messages
        .iter()
        .filter_map(|msg| msg.get("parentUuid").and_then(|v| v.as_str()))
        .map(|s| s.to_string())
        .collect();

    let leaf_message = find_latest_message(agent_messages.into_iter(), |msg| {
        let uuid = msg.get("uuid").and_then(|v| v.as_str()).unwrap_or("");
        !parent_uuids.contains(uuid)
    });

    match leaf_message {
        Some(leaf) => {
            let transcript = build_conversation_chain(&result.messages, leaf);
            let agent_transcript: Vec<Value> = transcript
                .into_iter()
                .filter(|msg| msg.get("agentId").and_then(|v| v.as_str()) == Some(agent_id))
                .map(|msg| {
                    let mut m = msg;
                    if let Some(obj) = m.as_object_mut() {
                        obj.remove("isSidechain");
                        obj.remove("parentUuid");
                    }
                    m
                })
                .collect();

            let content_replacements = result
                .agent_content_replacements
                .get(agent_id)
                .cloned()
                .unwrap_or_default();
            Ok(Some((agent_transcript, content_replacements)))
        }
        None => Ok(None),
    }
}

/// Load all subagent transcripts from disk.
pub async fn load_all_subagent_transcripts_from_disk(
    ctx: &SessionContext,
) -> Result<HashMap<String, Vec<Value>>> {
    let subagents_dir = ctx
        .session_project_dir
        .clone()
        .unwrap_or_else(|| get_project_dir(&ctx.original_cwd, ctx))
        .join(&ctx.session_id)
        .join("subagents");

    let mut entries = match tokio::fs::read_dir(&subagents_dir).await {
        Ok(e) => e,
        Err(_) => return Ok(HashMap::new()),
    };

    let mut agent_ids = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("agent-") && name.ends_with(".jsonl") {
            let agent_id = &name["agent-".len()..name.len() - ".jsonl".len()];
            agent_ids.push(agent_id.to_string());
        }
    }

    load_subagent_transcripts(&agent_ids, ctx).await
}

/// Deduplicate logs by session ID.
pub fn deduplicate_logs_by_session_id(logs: Vec<LogOption>) -> Vec<LogOption> {
    let mut deduped: HashMap<String, LogOption> = HashMap::new();
    for log in logs {
        if let Some(ref sid) = log.session_id {
            let existing = deduped.get(sid);
            if existing.is_none() || log.modified > existing.unwrap().modified {
                deduped.insert(sid.clone(), log);
            }
        }
    }
    let mut result: Vec<LogOption> = deduped.into_values().collect();
    mossen_types::logs::sort_logs(&mut result);
    for (i, log) in result.iter_mut().enumerate() {
        log.value = i as i64;
    }
    result
}

/// Enriches a lite log with metadata.
pub async fn enrich_log(log: &LogOption) -> Result<Option<LogOption>> {
    if log.is_lite != Some(true) || log.full_path.is_none() {
        return Ok(Some(log.clone()));
    }

    let file_path = PathBuf::from(log.full_path.as_ref().unwrap());
    let file_size = log.file_size.unwrap_or(0);
    let meta = read_lite_metadata(&file_path, file_size).await;

    let mut enriched = log.clone();
    enriched.is_lite = Some(false);
    enriched.first_prompt = meta.first_prompt;
    // git_branch is stored in the extra/flatten field of SerializedMessage,
    // not a direct field on LogOption. We skip this enrichment.
    enriched.is_sidechain = meta.is_sidechain;
    if let Some(tn) = meta.team_name {
        enriched.team_name = Some(tn);
    }
    enriched.custom_title = meta.custom_title;
    enriched.summary = meta.summary;
    enriched.tag = meta.tag;
    enriched.agent_setting = meta.agent_setting;
    enriched.pr_number = meta.pr_number;
    enriched.pr_url = meta.pr_url;
    enriched.pr_repository = meta.pr_repository;
    if let Some(pp) = meta.project_path {
        enriched.project_path = Some(pp);
    }

    // Provide fallback title
    if enriched.first_prompt.is_empty() && enriched.custom_title.is_none() {
        enriched.first_prompt = "(session)".to_string();
    }

    // Filter sidechains and agent sessions
    if enriched.is_sidechain {
        return Ok(None);
    }
    if enriched.team_name.is_some() {
        return Ok(None);
    }

    Ok(Some(enriched))
}

/// Enrich a batch of logs.
pub async fn enrich_logs(
    all_logs: &[LogOption],
    start_index: usize,
    count: usize,
) -> Result<(Vec<LogOption>, usize)> {
    let mut result = Vec::new();
    let mut i = start_index;

    while i < all_logs.len() && result.len() < count {
        let log = &all_logs[i];
        i += 1;
        if let Ok(Some(enriched)) = enrich_log(log).await {
            result.push(enriched);
        }
    }

    Ok((result, i))
}

/// Read lite metadata from head and tail of a JSONL file.
async fn read_lite_metadata(file_path: &Path, file_size: u64) -> LiteMetadata {
    let (head, tail) = match read_head_and_tail(file_path, file_size).await {
        Ok((h, t)) => (h, t),
        Err(_) => return LiteMetadata::default(),
    };

    let is_sidechain =
        head.contains("\"isSidechain\":true") || head.contains("\"isSidechain\": true");
    let project_path = extract_json_string_field(&head, "cwd");
    let team_name = extract_json_string_field(&head, "teamName");
    let agent_setting = extract_json_string_field(&head, "agentSetting");

    let first_prompt = extract_last_json_string_field(&tail, "lastPrompt")
        .or_else(|| {
            let prompt = extract_json_string_field_prefix(&head, "content", 200);
            if prompt.is_empty() {
                None
            } else {
                Some(prompt)
            }
        })
        .unwrap_or_default();

    let custom_title = extract_last_json_string_field(&tail, "customTitle")
        .or_else(|| extract_last_json_string_field(&head, "customTitle"))
        .or_else(|| extract_last_json_string_field(&tail, "aiTitle"))
        .or_else(|| extract_last_json_string_field(&head, "aiTitle"));

    let summary = extract_last_json_string_field(&tail, "summary");
    let tag = extract_last_json_string_field(&tail, "tag");
    let git_branch = extract_last_json_string_field(&tail, "gitBranch")
        .or_else(|| extract_json_string_field(&head, "gitBranch"));

    let pr_url = extract_last_json_string_field(&tail, "prUrl");
    let pr_repository = extract_last_json_string_field(&tail, "prRepository");
    let pr_number =
        extract_last_json_string_field(&tail, "prNumber").and_then(|s| s.parse::<u64>().ok());

    LiteMetadata {
        first_prompt,
        git_branch,
        is_sidechain,
        project_path,
        team_name,
        custom_title,
        summary,
        tag,
        agent_setting,
        pr_number,
        pr_url,
        pr_repository,
    }
}

/// Read head and tail of a file.
async fn read_head_and_tail(file_path: &Path, file_size: u64) -> Result<(String, String)> {
    if file_size == 0 {
        return Ok((String::new(), String::new()));
    }
    let buf_size = LITE_READ_BUF_SIZE as u64;

    // Read head
    let head_bytes = std::cmp::min(buf_size, file_size);
    let data = tokio::fs::read(file_path).await?;
    let head = String::from_utf8_lossy(&data[..head_bytes as usize]).to_string();

    // Read tail
    let tail_start = file_size.saturating_sub(buf_size);
    let tail = String::from_utf8_lossy(&data[tail_start as usize..]).to_string();

    Ok((head, tail))
}

/// Find unresolved tool use in transcript.
pub async fn find_unresolved_tool_use(
    tool_use_id: &str,
    ctx: &SessionContext,
) -> Result<Option<Value>> {
    let transcript_path = get_transcript_path(ctx);
    let result = match load_transcript_file(&transcript_path, false).await {
        Ok(r) => r,
        Err(_) => return Ok(None),
    };

    let mut tool_use_message: Option<Value> = None;

    for message in result.messages.values() {
        let msg_type = message.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if msg_type == "assistant" {
            if let Some(content) = message
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_array())
            {
                for block in content {
                    if block.get("type").and_then(|v| v.as_str()) == Some("tool_use")
                        && block.get("id").and_then(|v| v.as_str()) == Some(tool_use_id)
                    {
                        tool_use_message = Some(message.clone());
                        break;
                    }
                }
            }
        } else if msg_type == "user" {
            if let Some(content) = message
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_array())
            {
                for block in content {
                    if block.get("type").and_then(|v| v.as_str()) == Some("tool_result")
                        && block.get("tool_use_id").and_then(|v| v.as_str()) == Some(tool_use_id)
                    {
                        return Ok(None);
                    }
                }
            }
        }
    }

    Ok(tool_use_message)
}

/// Get a log by its index.
pub async fn get_log_by_index(index: usize, ctx: &SessionContext) -> Result<Option<LogOption>> {
    let logs = fetch_logs(None, ctx).await?;
    Ok(logs.into_iter().nth(index))
}

// ---------------------------------------------------------------------------
// Environment helpers / testing knobs
// ---------------------------------------------------------------------------

/// Return the effective `NODE_ENV` for this process, defaulting to
/// `development` when the env var is unset.
///
/// TS reference: `getNodeEnv()` in `utils/sessionStorage.ts`. The Rust
/// version prefers the value cached on `SessionContext` (injected at
/// startup) and falls back to reading the process environment so that
/// callers without a context can still resolve the value.
pub fn get_node_env(ctx: Option<&SessionContext>) -> String {
    if let Some(ctx) = ctx {
        if !ctx.node_env.is_empty() {
            return ctx.node_env.clone();
        }
    }
    std::env::var("NODE_ENV").unwrap_or_else(|_| "development".to_string())
}

/// Return the canonical user type for this process, reading from the
/// supplied `SessionContext` (which mirrors `utils/userType.ts`'s
/// `USER_TYPE`). Returns an empty string when no context is provided.
///
/// TS reference: the re-exported `getUserType` from
/// `utils/sessionStorage.ts`.
pub fn get_user_type(ctx: Option<&SessionContext>) -> String {
    ctx.map(|c| c.user_type.clone()).unwrap_or_default()
}

/// Feature flag — custom session titles are always enabled in the Rust
/// port (matches the TS source which simply returns `true`).
///
/// TS reference: `isCustomTitleEnabled()`.
pub fn is_custom_title_enabled() -> bool {
    true
}

/// Reset the flush/queue state of the supplied Project singleton, used
/// from the test harness to keep test cases isolated.
///
/// TS reference: `resetProjectFlushStateForTesting()`.
pub fn reset_project_flush_state_for_testing(project: &mut Project) {
    project.reset_flush_state();
}

/// Reset the entire Project singleton for testing. The TS version nulls
/// out the module-local `project` reference; in Rust we expose the same
/// behaviour by replacing the caller-owned project with a fresh one.
///
/// TS reference: `resetProjectForTesting()`.
pub fn reset_project_for_testing(project: &mut Project) {
    *project = Project::new();
}

/// Override the active session file path on the Project, used in unit
/// tests that need to point the writer at a temporary location.
///
/// TS reference: `setSessionFileForTesting(path)`.
pub fn set_session_file_for_testing(project: &mut Project, path: PathBuf) {
    project.session_file = Some(path);
    project.pending_entries.clear();
}

/// Register a CCR v2 internal event writer for transcript persistence.
/// When set, transcript messages are written as internal worker events
/// instead of going through v1 Session Ingress.
///
/// TS reference: `setInternalEventWriter(writer)`.
pub fn set_internal_event_writer(project: &mut Project, writer: InternalEventWriter) {
    project.set_internal_event_writer(writer);
}

/// Register CCR v2 internal event readers (foreground + subagent) for
/// session resume.
///
/// TS reference: `setInternalEventReader(reader, subagentReader)`.
pub fn set_internal_event_reader(
    project: &mut Project,
    reader: InternalEventReader,
    subagent_reader: InternalEventReader,
) {
    project.set_internal_event_reader(reader);
    project.set_internal_subagent_event_reader(subagent_reader);
}

/// Set the remote ingress URL on the supplied Project — used by tests
/// to simulate what `hydrate_remote_session` does in production.
///
/// TS reference: `setRemoteIngressUrlForTesting(url)`.
pub fn set_remote_ingress_url_for_testing(project: &mut Project, url: &str) {
    project.set_remote_ingress_url(url.to_string());
}

// ---------------------------------------------------------------------------
// Session message cache (UUID lookup for dedup on resume)
// ---------------------------------------------------------------------------

/// Cache of per-session message-UUID sets. Mirrors the memoized
/// `getSessionMessages` map in the TS source — used by
/// `does_message_exist_in_session` and primed by `get_last_session_log`.
static SESSION_MESSAGES_CACHE: Lazy<AsyncMutex<HashMap<String, Arc<HashSet<String>>>>> =
    Lazy::new(|| AsyncMutex::new(HashMap::new()));

/// Internal helper — load the JSONL file for a session and return the
/// `LoadTranscriptResult` produced by `load_transcript_file`.
async fn load_session_file_for_lookup(
    session_id: &str,
    ctx: &SessionContext,
) -> Result<LoadTranscriptResult> {
    let session_file = get_transcript_path_for_session(session_id, ctx);
    load_transcript_file(&session_file, false).await
}

/// Clear the memoized session-messages cache. Call after compaction
/// when old message UUIDs are no longer valid.
///
/// TS reference: `clearSessionMessagesCache()`.
pub async fn clear_session_messages_cache() {
    SESSION_MESSAGES_CACHE.lock().await.clear();
}

/// Synchronous variant — clears the cache without awaiting. Useful when
/// the caller already holds a runtime handle but doesn't want to await
/// (e.g. inside a cleanup hook). Acquires the lock via blocking and
/// requires being inside a Tokio runtime.
pub fn clear_session_messages_cache_blocking() {
    let mut guard = SESSION_MESSAGES_CACHE.blocking_lock();
    guard.clear();
}

/// Check whether a message UUID exists in the on-disk transcript for
/// the given session. Memoizes the loaded UUID set so repeated lookups
/// avoid re-reading the JSONL file.
///
/// TS reference: `doesMessageExistInSession(sessionId, messageUuid)`.
pub async fn does_message_exist_in_session(
    session_id: &str,
    message_uuid: &str,
    ctx: &SessionContext,
) -> Result<bool> {
    {
        let cache = SESSION_MESSAGES_CACHE.lock().await;
        if let Some(set) = cache.get(session_id) {
            return Ok(set.contains(message_uuid));
        }
    }
    let result = load_session_file_for_lookup(session_id, ctx).await?;
    let set: HashSet<String> = result.messages.keys().cloned().collect();
    let arc = Arc::new(set);
    let mut cache = SESSION_MESSAGES_CACHE.lock().await;
    cache.insert(session_id.to_string(), Arc::clone(&arc));
    Ok(arc.contains(message_uuid))
}

// ---------------------------------------------------------------------------
// Last session log lookup
// ---------------------------------------------------------------------------

/// Build a LogOption for the most recent non-sidechain leaf in a
/// session file. Returns `None` when the file is empty or has only
/// sidechain messages.
///
/// TS reference: `getLastSessionLog(sessionId)`.
pub async fn get_last_session_log(
    session_id: &str,
    ctx: &SessionContext,
) -> Result<Option<LogOption>> {
    let session_file = get_transcript_path_for_session(session_id, ctx);
    let result = match load_transcript_file(&session_file, false).await {
        Ok(r) => r,
        Err(_) => return Ok(None),
    };
    if result.messages.is_empty() {
        return Ok(None);
    }

    // Prime UUID cache (only if empty for this session — preserve mid-session
    // unflushed entries the way the TS guard does).
    {
        let mut cache = SESSION_MESSAGES_CACHE.lock().await;
        if !cache.contains_key(session_id) {
            let set: HashSet<String> = result.messages.keys().cloned().collect();
            cache.insert(session_id.to_string(), Arc::new(set));
        }
    }

    let last_message = find_latest_message(result.messages.values(), |m| {
        !m.get("isSidechain")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    });
    let last_message = match last_message {
        Some(m) => m,
        None => return Ok(None),
    };

    let transcript = build_conversation_chain(&result.messages, last_message);
    if transcript.is_empty() {
        return Ok(None);
    }

    let leaf_uuid = last_message
        .get("uuid")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let leaf_session_id = last_message
        .get("sessionId")
        .and_then(|v| v.as_str())
        .unwrap_or(session_id)
        .to_string();
    let leaf_timestamp = last_message
        .get("timestamp")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let first_timestamp = transcript
        .first()
        .and_then(|m| m.get("timestamp"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let is_sidechain_first = transcript
        .first()
        .and_then(|m| m.get("isSidechain"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let summary = result.summaries.get(&leaf_uuid).cloned();
    let custom_title = result.custom_titles.get(&leaf_session_id).cloned();
    let tag = result.tags.get(&leaf_session_id).cloned();
    let agent_setting = result.agent_settings.get(session_id).cloned();
    let content_replacements = result
        .content_replacements
        .get(session_id)
        .cloned()
        .unwrap_or_default();

    let mode = result.modes.get(session_id).and_then(|m| match m.as_str() {
        "coordinator" => Some(SessionMode::Coordinator),
        "normal" => Some(SessionMode::Normal),
        _ => None,
    });
    let worktree_session = result
        .worktree_states
        .get(session_id)
        .and_then(|opt| opt.clone());

    let collapse_commits: Vec<ContextCollapseCommitEntry> = result
        .context_collapse_commits
        .iter()
        .filter(|c| c.session_id == session_id)
        .cloned()
        .collect();
    let collapse_snapshot = result
        .context_collapse_snapshot
        .as_ref()
        .filter(|s| s.session_id == session_id)
        .cloned();

    let first_prompt = extract_first_prompt(&transcript);
    let message_count = count_visible_messages(&transcript);

    Ok(Some(LogOption {
        date: leaf_timestamp.clone(),
        messages: Vec::new(),
        full_path: Some(session_file.to_string_lossy().to_string()),
        value: 0,
        created: first_timestamp,
        modified: leaf_timestamp,
        first_prompt,
        message_count,
        file_size: None,
        is_sidechain: is_sidechain_first,
        is_lite: None,
        session_id: Some(leaf_session_id),
        team_name: None,
        agent_name: result.agent_names.get(session_id).cloned(),
        agent_color: result.agent_colors.get(session_id).cloned(),
        agent_setting,
        is_teammate: None,
        leaf_uuid: Some(leaf_uuid),
        summary,
        custom_title,
        tag,
        file_history_snapshots: None,
        attribution_snapshots: None,
        context_collapse_commits: if collapse_commits.is_empty() {
            None
        } else {
            Some(collapse_commits)
        },
        context_collapse_snapshot: collapse_snapshot,
        project_path: None,
        pr_number: result.pr_numbers.get(session_id).copied(),
        pr_url: result.pr_urls.get(session_id).cloned(),
        pr_repository: result.pr_repositories.get(session_id).cloned(),
        mode,
        worktree_session,
        content_replacements: if content_replacements.is_empty() {
            None
        } else {
            Some(content_replacements)
        },
    }))
}

// ---------------------------------------------------------------------------
// Message log loaders
// ---------------------------------------------------------------------------

/// Load the list of message logs for the current project, sorting by
/// modification time and re-indexing each entry's `value` field.
///
/// TS reference: `loadMessageLogs(limit?)`.
pub async fn load_message_logs(
    limit: Option<usize>,
    ctx: &SessionContext,
) -> Result<Vec<LogOption>> {
    let session_logs = fetch_logs(limit, ctx).await?;
    let total = session_logs.len();
    let (mut enriched, _next) = enrich_logs(&session_logs, 0, total).await?;
    mossen_types::logs::sort_logs(&mut enriched);
    for (i, log) in enriched.iter_mut().enumerate() {
        log.value = i as i64;
    }
    Ok(enriched)
}

/// Options for `load_all_projects_message_logs`.
#[derive(Debug, Clone, Default)]
pub struct LoadAllProjectsOptions {
    /// Skip the lite/index path and load every session in full.
    pub skip_index: bool,
    /// Initial number of sessions to enrich (defaults to `INITIAL_ENRICH_COUNT`).
    pub initial_enrich_count: Option<usize>,
}

/// Load message logs from all project directories under the projects
/// root. When `skip_index` is set, every session file is read in full;
/// otherwise the lite/progressive path is used.
///
/// TS reference: `loadAllProjectsMessageLogs(limit?, options?)`.
pub async fn load_all_projects_message_logs(
    limit: Option<usize>,
    options: Option<LoadAllProjectsOptions>,
    ctx: &SessionContext,
) -> Result<Vec<LogOption>> {
    let opts = options.unwrap_or_default();
    if opts.skip_index {
        return load_all_projects_message_logs_full(limit, ctx).await;
    }
    let initial = opts.initial_enrich_count.unwrap_or(INITIAL_ENRICH_COUNT);
    let result = load_all_projects_message_logs_progressive(limit, initial, ctx).await?;
    Ok(result.logs)
}

/// Load every session across every project directory in full. Used by
/// `/insights` and similar bulk analyses.
async fn load_all_projects_message_logs_full(
    limit: Option<usize>,
    ctx: &SessionContext,
) -> Result<Vec<LogOption>> {
    let projects_dir = get_projects_dir(ctx);
    let mut entries = match tokio::fs::read_dir(&projects_dir).await {
        Ok(e) => e,
        Err(_) => return Ok(Vec::new()),
    };

    let mut project_dirs: Vec<PathBuf> = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        match entry.file_type().await {
            Ok(ft) if ft.is_dir() => project_dirs.push(entry.path()),
            _ => continue,
        }
    }

    let mut all_logs: Vec<LogOption> = Vec::new();
    for project_dir in &project_dirs {
        let logs = get_logs_without_index(project_dir, limit).await?;
        all_logs.extend(logs);
    }

    // Deduplicate by sessionId + leafUuid, keeping the entry with the
    // newest `modified` timestamp.
    let mut deduped: HashMap<String, LogOption> = HashMap::new();
    for log in all_logs {
        let key = format!(
            "{}:{}",
            log.session_id.as_deref().unwrap_or(""),
            log.leaf_uuid.as_deref().unwrap_or(""),
        );
        match deduped.get(&key) {
            Some(existing) if log.modified <= existing.modified => {}
            _ => {
                deduped.insert(key, log);
            }
        }
    }

    let mut sorted: Vec<LogOption> = deduped.into_values().collect();
    mossen_types::logs::sort_logs(&mut sorted);
    for (i, log) in sorted.iter_mut().enumerate() {
        log.value = i as i64;
    }
    Ok(sorted)
}

/// Internal helper — load every leaf from a single session file using
/// the loadAllLogsFromSessionFile semantics.
async fn get_logs_without_index(
    project_dir: &Path,
    limit: Option<usize>,
) -> Result<Vec<LogOption>> {
    let session_files = get_session_files_with_mtime(project_dir).await?;
    if session_files.is_empty() {
        return Ok(Vec::new());
    }

    let mut entries: Vec<(String, SessionFileInfo)> = session_files.into_iter().collect();
    entries.sort_by(|a, b| b.1.mtime.cmp(&a.1.mtime));
    if let Some(limit) = limit {
        entries.truncate(limit);
    }

    let mut out: Vec<LogOption> = Vec::new();
    for (_, file_info) in entries {
        match load_all_logs_from_session_file(&file_info.path, None).await {
            Ok(logs) => out.extend(logs),
            Err(e) => tracing::debug!(
                "Failed to load session file {}: {}",
                file_info.path.display(),
                e
            ),
        }
    }
    Ok(out)
}

/// Progressive variant of [`load_all_projects_message_logs`] returning
/// a [`SessionLogResult`] so the caller can continue enrichment in
/// follow-up batches.
///
/// TS reference: `loadAllProjectsMessageLogsProgressive(limit?, initialEnrichCount?)`.
pub async fn load_all_projects_message_logs_progressive(
    limit: Option<usize>,
    initial_enrich_count: usize,
    ctx: &SessionContext,
) -> Result<SessionLogResult> {
    let projects_dir = get_projects_dir(ctx);
    let mut entries = match tokio::fs::read_dir(&projects_dir).await {
        Ok(e) => e,
        Err(_) => {
            return Ok(SessionLogResult {
                logs: Vec::new(),
                all_stat_logs: Vec::new(),
                next_index: 0,
            });
        }
    };

    let mut project_dirs: Vec<PathBuf> = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        match entry.file_type().await {
            Ok(ft) if ft.is_dir() => project_dirs.push(entry.path()),
            _ => continue,
        }
    }

    let mut raw_logs: Vec<LogOption> = Vec::new();
    for project_dir in &project_dirs {
        let logs = get_session_files_lite(project_dir, limit, None).await?;
        raw_logs.extend(logs);
    }

    let sorted = deduplicate_logs_by_session_id(raw_logs);
    let (mut enriched, next_index) = enrich_logs(&sorted, 0, initial_enrich_count).await?;
    for (i, log) in enriched.iter_mut().enumerate() {
        log.value = i as i64;
    }

    Ok(SessionLogResult {
        logs: enriched,
        all_stat_logs: sorted,
        next_index,
    })
}

/// Load message logs scoped to the supplied worktree paths. Falls back
/// to the single-project path when only one worktree is provided.
///
/// TS reference: `loadSameRepoMessageLogs(worktreePaths, limit?, initialEnrichCount?)`.
pub async fn load_same_repo_message_logs(
    worktree_paths: &[String],
    limit: Option<usize>,
    initial_enrich_count: usize,
    ctx: &SessionContext,
) -> Result<Vec<LogOption>> {
    let result =
        load_same_repo_message_logs_progressive(worktree_paths, limit, initial_enrich_count, ctx)
            .await?;
    Ok(result.logs)
}

/// Progressive variant of [`load_same_repo_message_logs`].
///
/// TS reference: `loadSameRepoMessageLogsProgressive(worktreePaths, limit?, initialEnrichCount?)`.
pub async fn load_same_repo_message_logs_progressive(
    worktree_paths: &[String],
    limit: Option<usize>,
    initial_enrich_count: usize,
    ctx: &SessionContext,
) -> Result<SessionLogResult> {
    tracing::debug!(
        "/resume: loading sessions for cwd={}, worktrees=[{}]",
        ctx.original_cwd.display(),
        worktree_paths.join(", "),
    );

    let all_stat_logs = get_stat_only_logs_for_worktrees(worktree_paths, limit, ctx).await?;
    tracing::debug!(
        "/resume: found {} session files on disk",
        all_stat_logs.len()
    );

    let (mut enriched, next_index) = enrich_logs(&all_stat_logs, 0, initial_enrich_count).await?;
    for (i, log) in enriched.iter_mut().enumerate() {
        log.value = i as i64;
    }

    Ok(SessionLogResult {
        logs: enriched,
        all_stat_logs,
        next_index,
    })
}

/// Expand a path into the variants we want to match against project
/// directory names (NFC + `/private` aliasing on macOS).
fn expand_project_path_aliases(path: &str) -> Vec<String> {
    let mut variants: HashSet<String> = HashSet::new();
    variants.insert(path.to_string());
    if cfg!(target_os = "macos") {
        if let Some(stripped) = path.strip_prefix("/private") {
            if !stripped.is_empty() {
                variants.insert(stripped.to_string());
            }
        } else if path == "/tmp"
            || path.starts_with("/tmp/")
            || path == "/var"
            || path.starts_with("/var/")
        {
            variants.insert(format!("/private{}", path));
        }
    }
    variants.into_iter().collect()
}

/// Compute the stat-only logs (no file reads) for the given worktree
/// paths. Matches project directories by sanitized-prefix length, with
/// the longest prefix winning.
async fn get_stat_only_logs_for_worktrees(
    worktree_paths: &[String],
    limit: Option<usize>,
    ctx: &SessionContext,
) -> Result<Vec<LogOption>> {
    let projects_dir = get_projects_dir(ctx);

    if worktree_paths.len() <= 1 {
        let cwd = &ctx.original_cwd;
        let project_dir = get_project_dir(cwd, ctx);
        return get_session_files_lite(&project_dir, None, Some(&cwd.to_string_lossy())).await;
    }

    let case_insensitive = cfg!(target_os = "windows");

    let mut prefix_entries: Vec<(String, String)> = Vec::new();
    for wt in worktree_paths {
        let mut prefixes: HashSet<String> = HashSet::new();
        for variant in expand_project_path_aliases(wt) {
            let s = sanitize_path(&variant);
            let key = if case_insensitive {
                s.to_lowercase()
            } else {
                s
            };
            prefixes.insert(key);
        }
        for p in prefixes {
            prefix_entries.push((wt.clone(), p));
        }
    }
    prefix_entries.sort_by(|a, b| b.1.len().cmp(&a.1.len()));

    let mut entries = match tokio::fs::read_dir(&projects_dir).await {
        Ok(e) => e,
        Err(e) => {
            tracing::debug!(
                "Failed to read projects dir {}, falling back: {}",
                projects_dir.display(),
                e
            );
            let project_dir = get_project_dir(&ctx.original_cwd, ctx);
            return get_session_files_lite(
                &project_dir,
                limit,
                Some(&ctx.original_cwd.to_string_lossy()),
            )
            .await;
        }
    };

    let mut all_logs: Vec<LogOption> = Vec::new();
    let mut seen_dirs: HashSet<String> = HashSet::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        let is_dir = matches!(entry.file_type().await, Ok(ft) if ft.is_dir());
        if !is_dir {
            continue;
        }
        let raw_name = entry.file_name().to_string_lossy().to_string();
        let dir_key = if case_insensitive {
            raw_name.to_lowercase()
        } else {
            raw_name.clone()
        };
        if seen_dirs.contains(&dir_key) {
            continue;
        }

        for (wt_path, prefix) in &prefix_entries {
            if &dir_key == prefix || dir_key.starts_with(&format!("{}-", prefix)) {
                seen_dirs.insert(dir_key.clone());
                let logs =
                    get_session_files_lite(&projects_dir.join(&raw_name), None, Some(wt_path))
                        .await?;
                all_logs.extend(logs);
                break;
            }
        }
    }

    Ok(deduplicate_logs_by_session_id(all_logs))
}

// ---------------------------------------------------------------------------
// Load all leaves from a single session file
// ---------------------------------------------------------------------------

/// Build a `LogOption` for every leaf message in a session file.
/// Replicates the TS `loadAllLogsFromSessionFile`, including trailing
/// children of the leaf in the output chain.
///
/// TS reference: `loadAllLogsFromSessionFile(sessionFile, projectPathOverride?)`.
pub async fn load_all_logs_from_session_file(
    session_file: &Path,
    project_path_override: Option<&str>,
) -> Result<Vec<LogOption>> {
    let result = load_transcript_file(session_file, true).await?;
    if result.messages.is_empty() {
        return Ok(Vec::new());
    }

    // Build parentUuid -> children index. Leaves are gathered into a
    // separate vector so we can iterate them in O(1) lookup time.
    let mut leaf_messages: Vec<Value> = Vec::new();
    let mut children_by_parent: HashMap<String, Vec<Value>> = HashMap::new();
    for msg in result.messages.values() {
        let uuid = msg
            .get("uuid")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if result.leaf_uuids.contains(&uuid) {
            leaf_messages.push(msg.clone());
        } else if let Some(parent_uuid) = msg
            .get("parentUuid")
            .and_then(|v| v.as_str())
            .map(String::from)
        {
            children_by_parent
                .entry(parent_uuid)
                .or_default()
                .push(msg.clone());
        }
    }

    let mut logs: Vec<LogOption> = Vec::new();

    for leaf in leaf_messages {
        let mut chain = build_conversation_chain(&result.messages, &leaf);
        if chain.is_empty() {
            continue;
        }

        // Append trailing children of the leaf, sorted by timestamp.
        let leaf_uuid = leaf
            .get("uuid")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if let Some(mut trailing) = children_by_parent.remove(&leaf_uuid) {
            trailing.sort_by(|a, b| {
                let ta = a.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
                let tb = b.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
                ta.cmp(tb)
            });
            chain.extend(trailing);
        }

        let session_id = leaf
            .get("sessionId")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let first_message = chain.first().cloned().unwrap_or_else(|| leaf.clone());
        let first_timestamp = first_message
            .get("timestamp")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let leaf_timestamp = leaf
            .get("timestamp")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let is_sidechain = first_message
            .get("isSidechain")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let first_prompt = extract_first_prompt(&chain);
        let message_count = count_visible_messages(&chain);
        let project_path = match project_path_override {
            Some(p) => Some(p.to_string()),
            None => first_message
                .get("cwd")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        };

        let summary = result.summaries.get(&leaf_uuid).cloned();
        let custom_title = result.custom_titles.get(&session_id).cloned();
        let tag = result.tags.get(&session_id).cloned();
        let agent_name = result.agent_names.get(&session_id).cloned();
        let agent_color = result.agent_colors.get(&session_id).cloned();
        let agent_setting = result.agent_settings.get(&session_id).cloned();
        let pr_number = result.pr_numbers.get(&session_id).copied();
        let pr_url = result.pr_urls.get(&session_id).cloned();
        let pr_repository = result.pr_repositories.get(&session_id).cloned();
        let mode = result
            .modes
            .get(&session_id)
            .and_then(|m| match m.as_str() {
                "coordinator" => Some(SessionMode::Coordinator),
                "normal" => Some(SessionMode::Normal),
                _ => None,
            });
        let content_replacements = result
            .content_replacements
            .get(&session_id)
            .cloned()
            .unwrap_or_default();

        logs.push(LogOption {
            date: leaf_timestamp.clone(),
            messages: Vec::new(),
            full_path: Some(session_file.to_string_lossy().to_string()),
            value: 0,
            created: first_timestamp,
            modified: leaf_timestamp,
            first_prompt,
            message_count,
            file_size: None,
            is_sidechain,
            is_lite: None,
            session_id: Some(session_id),
            team_name: None,
            agent_name,
            agent_color,
            agent_setting,
            is_teammate: None,
            leaf_uuid: Some(leaf_uuid),
            summary,
            custom_title,
            tag,
            file_history_snapshots: None,
            attribution_snapshots: None,
            context_collapse_commits: None,
            context_collapse_snapshot: None,
            project_path,
            pr_number,
            pr_url,
            pr_repository,
            mode,
            worktree_session: None,
            content_replacements: if content_replacements.is_empty() {
                None
            } else {
                Some(content_replacements)
            },
        });
    }

    Ok(logs)
}

// ---------------------------------------------------------------------------
// Hydrate from CCR v2 internal events
// ---------------------------------------------------------------------------

/// Hydrate session state from CCR v2 internal events. Fetches
/// foreground and subagent events via the registered readers on the
/// supplied Project, extracts transcript entries from payloads, and
/// writes them to the local transcript files (main + per-agent).
///
/// Returns `true` when at least one foreground event was hydrated.
///
/// TS reference: `hydrateFromCCRv2InternalEvents(sessionId)`.
pub async fn hydrate_from_ccrv2_internal_events(
    project: &mut Project,
    session_id: &str,
    ctx: &SessionContext,
) -> Result<bool> {
    let reader = match project.get_internal_event_reader() {
        Some(r) => Arc::clone(r),
        None => {
            tracing::debug!("No internal event reader registered for CCR v2 resume");
            return Ok(false);
        }
    };

    let events = match reader().await {
        Ok(Some(evts)) => evts,
        Ok(None) => {
            tracing::debug!("Internal event reader returned None for CCR v2 resume");
            return Ok(false);
        }
        Err(e) => {
            tracing::error!("Failed to read internal events for resume: {}", e);
            return Ok(false);
        }
    };

    let project_dir = get_project_dir(&ctx.original_cwd, ctx);
    tokio::fs::create_dir_all(&project_dir).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ =
            tokio::fs::set_permissions(&project_dir, std::fs::Permissions::from_mode(0o700)).await;
    }

    let session_file = get_transcript_path_for_session(session_id, ctx);
    let mut fg_content = String::new();
    for event in &events {
        let payload_value = Value::Object(event.payload.clone().into_iter().collect());
        if let Ok(line) = serde_json::to_string(&payload_value) {
            fg_content.push_str(&line);
            fg_content.push('\n');
        }
    }
    write_file_secure(&session_file, fg_content.as_bytes()).await?;

    tracing::debug!(
        "Hydrated {} foreground entries from CCR v2 internal events",
        events.len()
    );

    let mut subagent_event_count = 0usize;
    if let Some(sub_reader) = project.get_internal_subagent_event_reader() {
        let sub_reader = Arc::clone(sub_reader);
        if let Ok(Some(subagent_events)) = sub_reader().await {
            if !subagent_events.is_empty() {
                subagent_event_count = subagent_events.len();
                let mut by_agent: HashMap<String, Vec<Value>> = HashMap::new();
                for e in &subagent_events {
                    let agent_id = match &e.agent_id {
                        Some(id) if !id.is_empty() => id.clone(),
                        _ => continue,
                    };
                    let payload_value = Value::Object(e.payload.clone().into_iter().collect());
                    by_agent.entry(agent_id).or_default().push(payload_value);
                }
                let agent_count = by_agent.len();
                for (agent_id, entries) in by_agent {
                    let agent_file = get_agent_transcript_path(&agent_id, ctx);
                    if let Some(parent) = agent_file.parent() {
                        tokio::fs::create_dir_all(parent).await?;
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::PermissionsExt;
                            let _ = tokio::fs::set_permissions(
                                parent,
                                std::fs::Permissions::from_mode(0o700),
                            )
                            .await;
                        }
                    }
                    let mut agent_content = String::new();
                    for entry in entries {
                        if let Ok(line) = serde_json::to_string(&entry) {
                            agent_content.push_str(&line);
                            agent_content.push('\n');
                        }
                    }
                    write_file_secure(&agent_file, agent_content.as_bytes()).await?;
                }
                tracing::debug!(
                    "Hydrated {} subagent entries across {} agents",
                    subagent_events.len(),
                    agent_count,
                );
            }
        }
    }

    let _ = subagent_event_count; // recorded above; kept for telemetry parity
    Ok(!events.is_empty())
}

/// Write `data` to `path` with mode 0o600 on Unix, truncating any
/// existing file. Used by the CCR v2 hydration path so the transcript
/// file mirrors the security properties of the writer in `Project`.
async fn write_file_secure(path: &Path, data: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ =
                tokio::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700)).await;
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        use tokio::io::AsyncWriteExt;
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .await?;
        file.write_all(data).await?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        tokio::fs::write(path, data).await?;
        Ok(())
    }
}
