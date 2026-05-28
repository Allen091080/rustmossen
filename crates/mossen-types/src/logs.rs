//! # logs — 日志与会话条目类型
//!
//! 对应 TypeScript `types/logs.ts`。
//! 定义 `LogOption`、`Entry`（19 个变体联合）、`SerializedMessage` 等类型。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::ids::AgentId;

/// 序列化消息（Message + 附加元数据）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SerializedMessage {
    /// 角色。
    pub role: crate::message::Role,
    /// 内容块列表。
    pub content: Vec<crate::message::ContentBlock>,
    /// 消息 UUID。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,
    /// 当前工作目录。
    pub cwd: String,
    /// 用户类型。
    pub user_type: String,
    /// 入口点。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entrypoint: Option<String>,
    /// 会话 ID。
    pub session_id: String,
    /// 时间戳。
    pub timestamp: String,
    /// 版本。
    pub version: String,
    /// Git 分支。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_branch: Option<String>,
    /// 会话 slug。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
    /// 额外字段。
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// 日志选项。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogOption {
    /// 日期。
    pub date: String,
    /// 消息列表。
    pub messages: Vec<SerializedMessage>,
    /// 完整路径。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full_path: Option<String>,
    /// 值。
    pub value: i64,
    /// 创建时间（ISO 字符串）。
    pub created: String,
    /// 修改时间（ISO 字符串）。
    pub modified: String,
    /// 首个提示。
    pub first_prompt: String,
    /// 消息数量。
    pub message_count: usize,
    /// 文件大小（字节）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_size: Option<u64>,
    /// 是否为侧链。
    pub is_sidechain: bool,
    /// 是否为轻量日志。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_lite: Option<bool>,
    /// 会话 ID。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// 团队名称。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_name: Option<String>,
    /// Agent 自定义名称。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
    /// Agent 颜色。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_color: Option<String>,
    /// Agent 设置。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_setting: Option<String>,
    /// 是否为队友。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_teammate: Option<bool>,
    /// 叶子 UUID。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub leaf_uuid: Option<String>,
    /// 摘要。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// 自定义标题。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_title: Option<String>,
    /// 标签。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// 文件历史快照。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_history_snapshots: Option<Vec<serde_json::Value>>,
    /// 归因快照。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attribution_snapshots: Option<Vec<AttributionSnapshotMessage>>,
    /// 上下文折叠提交。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_collapse_commits: Option<Vec<ContextCollapseCommitEntry>>,
    /// 上下文折叠快照。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_collapse_snapshot: Option<ContextCollapseSnapshotEntry>,
    /// 项目路径。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,
    /// PR 编号。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr_number: Option<u64>,
    /// PR URL。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr_url: Option<String>,
    /// PR 仓库。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr_repository: Option<String>,
    /// 会话模式。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<SessionMode>,
    /// Worktree 会话状态。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree_session: Option<PersistedWorktreeSession>,
    /// 内容替换记录。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_replacements: Option<Vec<serde_json::Value>>,
}

/// 会话模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionMode {
    /// 协调者模式。
    Coordinator,
    /// 普通模式。
    Normal,
}

/// 摘要消息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryMessage {
    #[serde(rename = "type")]
    pub entry_type: String,
    pub leaf_uuid: String,
    pub summary: String,
}

/// 自定义标题消息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomTitleMessage {
    #[serde(rename = "type")]
    pub entry_type: String,
    pub session_id: String,
    pub custom_title: String,
}

/// AI 生成的标题消息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiTitleMessage {
    #[serde(rename = "type")]
    pub entry_type: String,
    pub session_id: String,
    pub ai_title: String,
}

/// 最后提示消息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LastPromptMessage {
    #[serde(rename = "type")]
    pub entry_type: String,
    pub session_id: String,
    pub last_prompt: String,
}

/// 任务摘要消息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSummaryMessage {
    #[serde(rename = "type")]
    pub entry_type: String,
    pub session_id: String,
    pub summary: String,
    pub timestamp: String,
}

/// 标签消息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagMessage {
    #[serde(rename = "type")]
    pub entry_type: String,
    pub session_id: String,
    pub tag: String,
}

/// Agent 名称消息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentNameMessage {
    #[serde(rename = "type")]
    pub entry_type: String,
    pub session_id: String,
    pub agent_name: String,
}

/// Agent 颜色消息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentColorMessage {
    #[serde(rename = "type")]
    pub entry_type: String,
    pub session_id: String,
    pub agent_color: String,
}

/// Agent 设置消息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSettingMessage {
    #[serde(rename = "type")]
    pub entry_type: String,
    pub session_id: String,
    pub agent_setting: String,
}

/// PR 链接消息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrLinkMessage {
    #[serde(rename = "type")]
    pub entry_type: String,
    pub session_id: String,
    pub pr_number: u64,
    pub pr_url: String,
    pub pr_repository: String,
    pub timestamp: String,
}

/// 模式条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModeEntry {
    #[serde(rename = "type")]
    pub entry_type: String,
    pub session_id: String,
    pub mode: SessionMode,
}

/// 持久化的 Worktree 会话。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedWorktreeSession {
    pub original_cwd: String,
    pub worktree_path: String,
    pub worktree_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree_branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_head_commit: Option<String>,
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tmux_session_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hook_based: Option<bool>,
}

/// Worktree 状态条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeStateEntry {
    #[serde(rename = "type")]
    pub entry_type: String,
    pub session_id: String,
    pub worktree_session: Option<PersistedWorktreeSession>,
}

/// 内容替换条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentReplacementEntry {
    #[serde(rename = "type")]
    pub entry_type: String,
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<AgentId>,
    pub replacements: Vec<serde_json::Value>,
}

/// 文件历史快照消息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileHistorySnapshotMessage {
    #[serde(rename = "type")]
    pub entry_type: String,
    pub message_id: String,
    pub snapshot: serde_json::Value,
    pub is_snapshot_update: bool,
}

/// 文件归因状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAttributionState {
    /// SHA-256 哈希。
    pub content_hash: String,
    /// Mossen 贡献的字符数。
    pub mossen_contribution: i64,
    /// 文件修改时间。
    pub mtime: f64,
}

/// 归因快照消息。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttributionSnapshotMessage {
    #[serde(rename = "type")]
    pub entry_type: String,
    pub message_id: String,
    pub surface: String,
    pub file_states: HashMap<String, FileAttributionState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_count_at_last_commit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_prompt_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_prompt_count_at_last_commit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub escape_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub escape_count_at_last_commit: Option<u64>,
}

/// 会话记录消息。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptMessage {
    /// 基础序列化消息字段。
    #[serde(flatten)]
    pub base: SerializedMessage,
    /// 父消息 UUID。
    pub parent_uuid: Option<String>,
    /// 逻辑父 UUID。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logical_parent_uuid: Option<String>,
    /// 是否为侧链。
    pub is_sidechain: bool,
    /// Git 分支。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_branch: Option<String>,
    /// Agent ID。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// 团队名称。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_name: Option<String>,
    /// Agent 名称。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
    /// Agent 颜色。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_color: Option<String>,
    /// 提示 ID。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_id: Option<String>,
}

/// 投机接受消息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeculationAcceptMessage {
    #[serde(rename = "type")]
    pub entry_type: String,
    pub timestamp: String,
    pub time_saved_ms: f64,
}

/// 上下文折叠提交条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextCollapseCommitEntry {
    #[serde(rename = "type")]
    pub entry_type: String,
    pub session_id: String,
    pub collapse_id: String,
    pub summary_uuid: String,
    pub summary_content: String,
    pub summary: String,
    pub first_archived_uuid: String,
    pub last_archived_uuid: String,
}

/// 上下文折叠快照中的暂存条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StagedCollapseEntry {
    pub start_uuid: String,
    pub end_uuid: String,
    pub summary: String,
    pub risk: f64,
    pub staged_at: f64,
}

/// 上下文折叠快照条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextCollapseSnapshotEntry {
    #[serde(rename = "type")]
    pub entry_type: String,
    pub session_id: String,
    pub staged: Vec<StagedCollapseEntry>,
    pub armed: bool,
    pub last_spawn_tokens: u64,
}

/// 日志条目联合类型（19 个变体）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
#[allow(clippy::large_enum_variant)]
pub enum Entry {
    /// 会话记录消息。
    #[serde(rename = "transcript")]
    Transcript(TranscriptMessage),
    /// 摘要消息。
    #[serde(rename = "summary")]
    Summary(SummaryMessage),
    /// 自定义标题。
    #[serde(rename = "custom-title")]
    CustomTitle(CustomTitleMessage),
    /// AI 标题。
    #[serde(rename = "ai-title")]
    AiTitle(AiTitleMessage),
    /// 最后提示。
    #[serde(rename = "last-prompt")]
    LastPrompt(LastPromptMessage),
    /// 任务摘要。
    #[serde(rename = "task-summary")]
    TaskSummary(TaskSummaryMessage),
    /// 标签。
    #[serde(rename = "tag")]
    Tag(TagMessage),
    /// Agent 名称。
    #[serde(rename = "agent-name")]
    AgentName(AgentNameMessage),
    /// Agent 颜色。
    #[serde(rename = "agent-color")]
    AgentColor(AgentColorMessage),
    /// Agent 设置。
    #[serde(rename = "agent-setting")]
    AgentSetting(AgentSettingMessage),
    /// PR 链接。
    #[serde(rename = "pr-link")]
    PrLink(PrLinkMessage),
    /// 文件历史快照。
    #[serde(rename = "file-history-snapshot")]
    FileHistorySnapshot(FileHistorySnapshotMessage),
    /// 归因快照。
    #[serde(rename = "attribution-snapshot")]
    AttributionSnapshot(AttributionSnapshotMessage),
    /// 投机接受。
    #[serde(rename = "speculation-accept")]
    SpeculationAccept(SpeculationAcceptMessage),
    /// 模式条目。
    #[serde(rename = "mode")]
    Mode(ModeEntry),
    /// Worktree 状态。
    #[serde(rename = "worktree-state")]
    WorktreeState(WorktreeStateEntry),
    /// 内容替换。
    #[serde(rename = "content-replacement")]
    ContentReplacement(ContentReplacementEntry),
    /// 上下文折叠提交。
    #[serde(rename = "marble-origami-commit")]
    ContextCollapseCommit(ContextCollapseCommitEntry),
    /// 队列操作消息。
    #[serde(rename = "queue-operation")]
    QueueOperation(QueueOperationMessage),
    /// 上下文折叠快照。
    #[serde(rename = "marble-origami-snapshot")]
    ContextCollapseSnapshot(ContextCollapseSnapshotEntry),
}

/// 队列操作消息。
/// 对应 TS `QueueOperationMessage` (from types/messageQueueTypes.ts).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueOperationMessage {
    #[serde(rename = "type")]
    pub entry_type: String,
    /// 操作类型。
    pub operation: String,
    /// 额外数据。
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// 排序日志（按修改时间降序）。
pub fn sort_logs(logs: &mut [LogOption]) {
    logs.sort_by(|a, b| b.modified.cmp(&a.modified).then(b.created.cmp(&a.created)));
}
