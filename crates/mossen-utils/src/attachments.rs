//! Attachment system — 对应 TS `utils/attachments.ts`
//!
//! 管理消息附件的生成与处理：文件附件、IDE 选择、内存文件、
//! 诊断信息、计划模式、任务提醒、团队消息等。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Configuration for todo reminder timing.
pub struct TodoReminderConfig {
    pub turns_since_write: usize,
    pub turns_between_reminders: usize,
}

pub const TODO_REMINDER_CONFIG: TodoReminderConfig = TodoReminderConfig {
    turns_since_write: 10,
    turns_between_reminders: 10,
};

/// Configuration for plan mode attachment timing.
pub struct PlanModeAttachmentConfig {
    pub turns_between_attachments: usize,
    pub full_reminder_every_n_attachments: usize,
}

pub const PLAN_MODE_ATTACHMENT_CONFIG: PlanModeAttachmentConfig = PlanModeAttachmentConfig {
    turns_between_attachments: 5,
    full_reminder_every_n_attachments: 5,
};

/// Configuration for auto mode attachment timing.
pub struct AutoModeAttachmentConfig {
    pub turns_between_attachments: usize,
    pub full_reminder_every_n_attachments: usize,
}

pub const AUTO_MODE_ATTACHMENT_CONFIG: AutoModeAttachmentConfig = AutoModeAttachmentConfig {
    turns_between_attachments: 5,
    full_reminder_every_n_attachments: 5,
};

const MAX_MEMORY_LINES: usize = 200;
const MAX_MEMORY_BYTES: usize = 4096;

/// Configuration for relevant memory session limits.
pub struct RelevantMemoriesConfig {
    pub max_session_bytes: usize,
}

pub const RELEVANT_MEMORIES_CONFIG: RelevantMemoriesConfig = RelevantMemoriesConfig {
    max_session_bytes: 60 * 1024,
};

/// Configuration for verify plan reminder timing.
pub struct VerifyPlanReminderConfig {
    pub turns_between_reminders: usize,
}

pub const VERIFY_PLAN_REMINDER_CONFIG: VerifyPlanReminderConfig = VerifyPlanReminderConfig {
    turns_between_reminders: 10,
};

// ---------------------------------------------------------------------------
// Types — FileReadToolOutput stub
// ---------------------------------------------------------------------------

/// Output from the FileReadTool (text or image content).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FileReadToolOutput {
    Text {
        file: FileReadTextContent,
    },
    Image {
        #[serde(rename = "filePath")]
        file_path: String,
        data: String,
        media_type: String,
    },
    Notebook {
        #[serde(rename = "filePath")]
        file_path: String,
        cells: Vec<serde_json::Value>,
    },
    Pdf {
        #[serde(rename = "filePath")]
        file_path: String,
        content: String,
    },
}

/// Text content from file reading.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileReadTextContent {
    pub file_path: String,
    pub content: String,
    pub num_lines: usize,
    pub start_line: usize,
    pub total_lines: usize,
}

// ---------------------------------------------------------------------------
// Types — Hook events
// ---------------------------------------------------------------------------

/// Hook event type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum HookEvent {
    PreToolUse,
    PostToolUse,
    Notification,
    Stop,
    SubagentStop,
    FileChanged,
    CwdChanged,
    SessionStart,
    InstructionsLoaded,
}

/// Extended hook event (includes non-standard variants).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum HookEventOrSpecial {
    Standard(HookEvent),
    StatusLine,
    FileSuggestion,
}

/// Sync hook JSON output.
pub type SyncHookJSONOutput = serde_json::Value;

/// Hook blocking error info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookBlockingError {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

// ---------------------------------------------------------------------------
// Types — Memory
// ---------------------------------------------------------------------------

/// Memory file info type classification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryFileType {
    User,
    Project,
    Local,
    Managed,
    Agent,
}

/// Memory file metadata and content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryFileInfo {
    pub path: String,
    pub content: String,
    #[serde(rename = "type")]
    pub file_type: MemoryFileType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_content: Option<String>,
    #[serde(default)]
    pub content_differs_from_disk: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub globs: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
}

/// Instructions memory type classification (subset of MemoryFileType).
fn is_instructions_memory_type(t: &MemoryFileType) -> bool {
    matches!(
        t,
        MemoryFileType::User
            | MemoryFileType::Project
            | MemoryFileType::Local
            | MemoryFileType::Managed
    )
}

// ---------------------------------------------------------------------------
// Types — TodoList / Task
// ---------------------------------------------------------------------------

/// A todo list item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    pub id: String,
    pub content: String,
    pub status: String,
}

/// A todo list is a vec of todo items.
pub type TodoList = Vec<TodoItem>;

/// A task item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub subject: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
}

/// Task type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    Shell,
    Agent,
    Remote,
}

/// Task status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
    Pending,
}

// ---------------------------------------------------------------------------
// Types — Diagnostic
// ---------------------------------------------------------------------------

/// A diagnostic file entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticFile {
    pub path: String,
    pub diagnostics: Vec<DiagnosticEntry>,
}

/// A single diagnostic entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticEntry {
    pub message: String,
    pub severity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<usize>,
}

// ---------------------------------------------------------------------------
// Types — IDE Selection
// ---------------------------------------------------------------------------

/// IDE selection info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IDESelection {
    pub file_path: Option<String>,
    pub text: Option<String>,
    pub line_start: Option<usize>,
    pub line_count: Option<usize>,
}

// ---------------------------------------------------------------------------
// Types — MCP Resource
// ---------------------------------------------------------------------------

/// MCP resource result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadResourceResult {
    pub contents: Vec<ResourceContent>,
}

/// Resource content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceContent {
    pub uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blob: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

// ---------------------------------------------------------------------------
// Types — Message (conversation-level)
// ---------------------------------------------------------------------------

/// Message origin.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MessageOrigin {
    Human,
    Coordinator,
    Teammate { from: String },
    Cron,
    Proactive,
    Channel,
}

/// Content block param for queued commands.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MossenContentBlockParam {
    Text { text: String },
    Image { source: ImageSource },
}

/// Image source for content blocks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageSource {
    #[serde(rename = "type")]
    pub source_type: String,
    pub media_type: String,
    pub data: String,
}

/// Conversation message enum — models the TS `Message` discriminated union.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConversationMessage {
    User {
        message: UserMsgPayload,
        #[serde(default)]
        is_meta: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_use_result: Option<serde_json::Value>,
        uuid: String,
        timestamp: String,
    },
    Assistant {
        message: AssistantMsgPayload,
        uuid: String,
        timestamp: String,
    },
    Attachment {
        attachment: Attachment,
        uuid: String,
        timestamp: String,
    },
}

/// User message payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMsgPayload {
    pub content: serde_json::Value,
}

/// Assistant message payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantMsgPayload {
    pub content: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

// ---------------------------------------------------------------------------
// Types — Queued Command
// ---------------------------------------------------------------------------

/// A queued command from the user input queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedCommand {
    pub value: serde_json::Value,
    pub mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<MessageOrigin>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_meta: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pasted_contents: Option<HashMap<String, PastedContent>>,
}

/// Pasted content (image or text).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PastedContent {
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paste_type: Option<String>,
}

// ---------------------------------------------------------------------------
// Types — Agent Definition
// ---------------------------------------------------------------------------

/// Agent definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDefinition {
    pub agent_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_requirements: Option<Vec<String>>,
}

// ---------------------------------------------------------------------------
// Types — Attachment variants (the big union)
// ---------------------------------------------------------------------------

/// Attachment type — discriminated union of all attachment kinds.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Attachment {
    /// File at-mention attachment.
    File {
        filename: String,
        content: FileReadToolOutput,
        #[serde(skip_serializing_if = "Option::is_none")]
        truncated: Option<bool>,
        display_path: String,
    },
    /// Compact file reference (post-compact).
    CompactFileReference {
        filename: String,
        display_path: String,
    },
    /// PDF reference for large PDFs.
    PdfReference {
        filename: String,
        page_count: usize,
        file_size: u64,
        display_path: String,
    },
    /// File already in model context.
    AlreadyReadFile {
        filename: String,
        content: FileReadToolOutput,
        #[serde(skip_serializing_if = "Option::is_none")]
        truncated: Option<bool>,
        display_path: String,
    },
    /// An at-mentioned text file was edited.
    EditedTextFile { filename: String, snippet: String },
    /// An at-mentioned image file was edited.
    EditedImageFile {
        filename: String,
        content: FileReadToolOutput,
    },
    /// Directory listing.
    Directory {
        path: String,
        content: String,
        display_path: String,
    },
    /// IDE selected lines.
    SelectedLinesInIde {
        ide_name: String,
        line_start: usize,
        line_end: usize,
        filename: String,
        content: String,
        display_path: String,
    },
    /// Opened file in IDE.
    OpenedFileInIde { filename: String },
    /// Todo reminder.
    TodoReminder {
        content: TodoList,
        item_count: usize,
    },
    /// Task reminder.
    TaskReminder {
        content: Vec<Task>,
        item_count: usize,
    },
    /// Nested memory (MOSSEN.md files).
    NestedMemory {
        path: String,
        content: MemoryFileInfo,
        display_path: String,
    },
    /// Relevant memories (auto-surfaced).
    RelevantMemories { memories: Vec<RelevantMemoryEntry> },
    /// Dynamic skill directory.
    DynamicSkill {
        skill_dir: String,
        skill_names: Vec<String>,
        display_path: String,
    },
    /// Skill listing.
    SkillListing {
        content: String,
        skill_count: usize,
        is_initial: bool,
    },
    /// Queued command from user queue.
    QueuedCommand {
        prompt: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        source_uuid: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        image_paste_ids: Option<Vec<u32>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        command_mode: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        origin: Option<MessageOrigin>,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_meta: Option<bool>,
    },
    /// Output style override.
    OutputStyle { style: String },
    /// Diagnostics from IDE/LSP.
    Diagnostics {
        files: Vec<DiagnosticFile>,
        is_new: bool,
    },
    /// Plan mode reminder.
    PlanMode {
        reminder_type: ReminderType,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_sub_agent: Option<bool>,
        plan_file_path: String,
        plan_exists: bool,
    },
    /// Plan mode re-entry notification.
    PlanModeReentry { plan_file_path: String },
    /// Plan mode exit notification.
    PlanModeExit {
        plan_file_path: String,
        plan_exists: bool,
    },
    /// Auto mode reminder.
    AutoMode { reminder_type: ReminderType },
    /// Auto mode exit notification.
    AutoModeExit,
    /// Critical system reminder.
    CriticalSystemReminder { content: String },
    /// Plan file reference.
    PlanFileReference {
        plan_file_path: String,
        plan_content: String,
    },
    /// MCP resource attachment.
    McpResource {
        server: String,
        uri: String,
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        content: ReadResourceResult,
    },
    /// Command permissions.
    CommandPermissions {
        allowed_tools: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        model: Option<String>,
    },
    /// Agent mention.
    AgentMention { agent_type: String },
    /// Task status update.
    TaskStatus {
        task_id: String,
        task_type: TaskType,
        status: TaskStatus,
        description: String,
        delta_summary: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        output_file_path: Option<String>,
    },
    /// Async hook response.
    AsyncHookResponse {
        process_id: String,
        hook_name: String,
        hook_event: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_name: Option<String>,
        response: SyncHookJSONOutput,
        stdout: String,
        stderr: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        exit_code: Option<i32>,
    },
    /// Token usage info.
    TokenUsage {
        used: usize,
        total: usize,
        remaining: usize,
    },
    /// USD budget info.
    BudgetUsd {
        used: f64,
        total: f64,
        remaining: f64,
    },
    /// Output token usage.
    OutputTokenUsage {
        turn: usize,
        session: usize,
        budget: Option<usize>,
    },
    /// Structured output data.
    StructuredOutput { data: serde_json::Value },
    /// Teammate mailbox messages.
    TeammateMailbox { messages: Vec<TeammateMessage> },
    /// Team context for swarm coordination.
    TeamContext {
        agent_id: String,
        agent_name: String,
        team_name: String,
        team_config_path: String,
        task_list_path: String,
    },
    /// Hook cancelled.
    HookCancelled {
        hook_name: String,
        tool_use_id: String,
        hook_event: HookEvent,
        #[serde(skip_serializing_if = "Option::is_none")]
        command: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u64>,
    },
    /// Hook blocking error.
    HookBlockingError {
        blocking_error: HookBlockingError,
        hook_name: String,
        tool_use_id: String,
        hook_event: HookEvent,
    },
    /// Hook non-blocking error.
    HookNonBlockingError {
        hook_name: String,
        stderr: String,
        stdout: String,
        exit_code: i32,
        tool_use_id: String,
        hook_event: HookEvent,
        #[serde(skip_serializing_if = "Option::is_none")]
        command: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u64>,
    },
    /// Hook error during execution.
    HookErrorDuringExecution {
        content: String,
        hook_name: String,
        tool_use_id: String,
        hook_event: HookEvent,
        #[serde(skip_serializing_if = "Option::is_none")]
        command: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u64>,
    },
    /// Hook stopped continuation.
    HookStoppedContinuation {
        message: String,
        hook_name: String,
        tool_use_id: String,
        hook_event: HookEvent,
    },
    /// Hook success.
    HookSuccess {
        content: String,
        hook_name: String,
        tool_use_id: String,
        hook_event: HookEvent,
        #[serde(skip_serializing_if = "Option::is_none")]
        stdout: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        stderr: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        exit_code: Option<i32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        command: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u64>,
    },
    /// Hook additional context.
    HookAdditionalContext {
        content: Vec<String>,
        hook_name: String,
        tool_use_id: String,
        hook_event: HookEvent,
    },
    /// Hook system message.
    HookSystemMessage {
        content: String,
        hook_name: String,
        tool_use_id: String,
        hook_event: HookEvent,
    },
    /// Hook permission decision.
    HookPermissionDecision {
        decision: PermissionDecisionKind,
        tool_use_id: String,
        hook_event: HookEvent,
    },
    /// Invoked skills.
    InvokedSkills { skills: Vec<InvokedSkillEntry> },
    /// Verify plan reminder.
    VerifyPlanReminder,
    /// Max turns reached notification.
    MaxTurnsReached { max_turns: usize, turn_count: usize },
    /// Current session memory.
    CurrentSessionMemory {
        content: String,
        path: String,
        token_count: usize,
    },
    /// Teammate shutdown batch.
    TeammateShutdownBatch { count: usize },
    /// Compaction reminder.
    CompactionReminder,
    /// Context efficiency nudge.
    ContextEfficiency,
    /// Date change notification.
    DateChange { new_date: String },
    /// Ultrathink effort level.
    UltrathinkEffort { level: String },
    /// Deferred tools delta.
    DeferredToolsDelta {
        added_names: Vec<String>,
        added_lines: Vec<String>,
        removed_names: Vec<String>,
    },
    /// Agent listing delta.
    AgentListingDelta {
        added_types: Vec<String>,
        added_lines: Vec<String>,
        removed_types: Vec<String>,
        is_initial: bool,
        show_concurrency_note: bool,
    },
    /// MCP instructions delta.
    McpInstructionsDelta {
        added_names: Vec<String>,
        added_blocks: Vec<String>,
        removed_names: Vec<String>,
    },
    /// Companion intro.
    CompanionIntro { name: String, species: String },
    /// Console errors/warnings (bagel).
    BagelConsole {
        error_count: usize,
        warning_count: usize,
        sample: String,
    },
}

// =============================================================================
// Attachment 子类型 — TS 中每个变体都是独立的 `type Xxx = { ... }`。Rust 端
// 全部统一为 [`Attachment`] 的枚举变体，下面的类型别名让外部代码可以按
// 子类型名引用整个枚举（具体变体由 pattern match 区分）。
// =============================================================================

/// 对应 TS `FileAttachment`。
pub type FileAttachment = Attachment;
/// 对应 TS `CompactFileReferenceAttachment`。
pub type CompactFileReferenceAttachment = Attachment;
/// 对应 TS `PDFReferenceAttachment`。
pub type PDFReferenceAttachment = Attachment;
/// 对应 TS `AlreadyReadFileAttachment`。
pub type AlreadyReadFileAttachment = Attachment;
/// 对应 TS `AgentMentionAttachment`。
pub type AgentMentionAttachment = Attachment;
/// 对应 TS `AsyncHookResponseAttachment`。
pub type AsyncHookResponseAttachment = Attachment;
/// 对应 TS `HookAttachment`（union）。
pub type HookAttachment = Attachment;
/// 对应 TS `HookPermissionDecisionAttachment`。
pub type HookPermissionDecisionAttachment = Attachment;
/// 对应 TS `HookSystemMessageAttachment`。
pub type HookSystemMessageAttachment = Attachment;
/// 对应 TS `HookCancelledAttachment`。
pub type HookCancelledAttachment = Attachment;
/// 对应 TS `HookErrorDuringExecutionAttachment`。
pub type HookErrorDuringExecutionAttachment = Attachment;
/// 对应 TS `HookSuccessAttachment`。
pub type HookSuccessAttachment = Attachment;
/// 对应 TS `HookNonBlockingErrorAttachment`。
pub type HookNonBlockingErrorAttachment = Attachment;
/// 对应 TS `TeammateMailboxAttachment`。
pub type TeammateMailboxAttachment = Attachment;
/// 对应 TS `TeamContextAttachment`。
pub type TeamContextAttachment = Attachment;

/// Reminder type (full or sparse).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReminderType {
    Full,
    Sparse,
}

/// Permission decision kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionDecisionKind {
    Allow,
    Deny,
}

/// Relevant memory entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelevantMemoryEntry {
    pub path: String,
    pub content: String,
    pub mtime_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

/// Teammate message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeammateMessage {
    pub from: String,
    pub text: String,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

/// Invoked skill entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvokedSkillEntry {
    pub name: String,
    pub path: String,
    pub content: String,
}

// ---------------------------------------------------------------------------
// Types — Attachment Message
// ---------------------------------------------------------------------------

/// An attachment message (wrapper for conversation transcript).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentMessage {
    pub attachment: Attachment,
    #[serde(rename = "type")]
    pub msg_type: String,
    pub uuid: String,
    pub timestamp: String,
}

// ---------------------------------------------------------------------------
// Types — File State Cache
// ---------------------------------------------------------------------------

/// A cached file state entry.
#[derive(Debug, Clone)]
pub struct FileStateEntry {
    pub content: String,
    pub timestamp: u64,
    pub offset: Option<usize>,
    pub limit: Option<usize>,
    pub is_partial_view: bool,
}

/// File state cache (LRU map of file path -> state).
#[derive(Debug, Clone, Default)]
pub struct FileStateCache {
    entries: HashMap<String, FileStateEntry>,
}

impl FileStateCache {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    pub fn has(&self, path: &str) -> bool {
        self.entries.contains_key(path)
    }

    pub fn get(&self, path: &str) -> Option<&FileStateEntry> {
        self.entries.get(path)
    }

    pub fn set(&mut self, path: String, entry: FileStateEntry) {
        self.entries.insert(path, entry);
    }

    pub fn delete(&mut self, path: &str) {
        self.entries.remove(path);
    }

    pub fn keys(&self) -> Vec<&String> {
        self.entries.keys().collect()
    }
}

// ---------------------------------------------------------------------------
// Types — ToolUseContext (attachment-specific fields)
// ---------------------------------------------------------------------------

/// Permission mode for tools.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionMode {
    Default,
    Plan,
    Auto,
    BypassAll,
}

/// Tool permission context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPermissionContext {
    pub mode: PermissionMode,
    #[serde(default)]
    pub rules: Vec<PermissionRule>,
}

/// Permission rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRule {
    pub tool: String,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
}

/// MCP server connection info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPServerConnection {
    pub name: String,
    #[serde(rename = "type")]
    pub conn_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
}

/// MCP resource info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPResourceInfo {
    pub uri: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Tool definition (simplified for attachment context).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Agent definitions container.
#[derive(Debug, Clone, Default)]
pub struct AgentDefinitions {
    pub active_agents: Vec<AgentDefinition>,
    pub allowed_agent_types: Option<Vec<String>>,
}

/// App state (subset relevant to attachments).
#[derive(Debug, Clone)]
pub struct AppState {
    pub tool_permission_context: ToolPermissionContext,
    pub todos: HashMap<String, TodoList>,
    pub inbox: InboxState,
    pub mcp: McpState,
    pub team_context: Option<TeamContext>,
    pub pending_plan_verification: Option<PendingPlanVerification>,
}

/// Inbox state.
#[derive(Debug, Clone, Default)]
pub struct InboxState {
    pub messages: Vec<InboxMessage>,
}

/// Inbox message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboxMessage {
    pub id: String,
    pub from: String,
    pub text: String,
    pub timestamp: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

/// MCP state.
#[derive(Debug, Clone, Default)]
pub struct McpState {
    pub commands: Vec<serde_json::Value>,
}

/// Team context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamContext {
    pub lead_agent_id: String,
    pub teammates: HashMap<String, TeammateInfo>,
}

/// Teammate info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeammateInfo {
    pub name: String,
}

/// Pending plan verification state.
#[derive(Debug, Clone)]
pub struct PendingPlanVerification {
    pub verification_started: bool,
    pub verification_completed: bool,
}

/// Options for the tool use context.
#[derive(Debug, Clone)]
pub struct ToolUseOptions {
    pub tools: Vec<ToolDef>,
    pub main_loop_model: String,
    pub mcp_clients: Vec<MCPServerConnection>,
    pub mcp_resources: HashMap<String, Vec<MCPResourceInfo>>,
    pub agent_definitions: AgentDefinitions,
    pub max_budget_usd: Option<f64>,
}

/// Attachment-level ToolUseContext (passed to attachment generators).
#[derive(Debug, Clone)]
pub struct AttachmentToolUseContext {
    pub options: ToolUseOptions,
    pub agent_id: Option<String>,
    pub read_file_state: FileStateCache,
    pub loaded_nested_memory_paths: HashSet<String>,
    pub nested_memory_attachment_triggers: HashSet<String>,
    pub dynamic_skill_dir_triggers: HashSet<String>,
    pub critical_system_reminder: Option<String>,
}

// ---------------------------------------------------------------------------
// Types — Memory Prefetch
// ---------------------------------------------------------------------------

/// A memory relevance-selector prefetch handle.
pub struct MemoryPrefetch {
    pub promise: tokio::task::JoinHandle<Vec<Attachment>>,
    pub settled_at: Option<Instant>,
    pub consumed_on_iteration: i32,
    abort_handle: tokio::task::AbortHandle,
}

impl MemoryPrefetch {
    pub fn abort(&self) {
        self.abort_handle.abort();
    }
}

impl Drop for MemoryPrefetch {
    fn drop(&mut self) {
        self.abort_handle.abort();
    }
}

// ---------------------------------------------------------------------------
// Module-level state
// ---------------------------------------------------------------------------

use once_cell::sync::Lazy;

static SENT_SKILL_NAMES: Lazy<Mutex<HashMap<String, HashSet<String>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

static SUPPRESS_NEXT_SKILL: AtomicBool = AtomicBool::new(false);

static INLINE_NOTIFICATION_MODES: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    let mut s = HashSet::new();
    s.insert("prompt");
    s.insert("task-notification");
    s
});

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

fn is_env_truthy(val: Option<&str>) -> bool {
    matches!(val, Some(v) if v == "1" || v.eq_ignore_ascii_case("true"))
}

fn is_env_defined_falsy(val: Option<&str>) -> bool {
    matches!(val, Some(v) if v == "0" || v.eq_ignore_ascii_case("false"))
}

fn get_cwd() -> String {
    std::env::current_dir()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string()
}

fn relative_path(from: &str, to: &str) -> String {
    pathdiff::diff_paths(to, from)
        .unwrap_or_else(|| PathBuf::from(to))
        .to_string_lossy()
        .to_string()
}

fn count_char_in_string(s: &str, ch: char) -> usize {
    s.chars().filter(|&c| c == ch).count()
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn tool_matches_name(tool: &ToolDef, name: &str) -> bool {
    tool.name == name || tool.name.ends_with(&format!("::{}", name))
}

/// Check if a file path has a deny rule.
fn is_file_read_denied(file_path: &str, ctx: &ToolPermissionContext) -> bool {
    matching_rule_for_input(file_path, ctx, "read", "deny").is_some()
}

/// Find a matching permission rule.
fn matching_rule_for_input<'a>(
    file_path: &str,
    ctx: &'a ToolPermissionContext,
    _action: &str,
    rule_action: &str,
) -> Option<&'a PermissionRule> {
    ctx.rules.iter().find(|rule| {
        rule.action == rule_action
            && (rule.tool == "Read" || rule.tool == "*")
            && rule
                .pattern
                .as_ref()
                .map(|p| file_path.starts_with(p.as_str()) || p == "*")
                .unwrap_or(true)
    })
}

// `path_in_allowed_working_path` lives in
// `crate::permissions::filesystem::path_in_allowed_working_path` — the
// orphan stub that used to sit here always returned `true`, which would have
// been a security bypass had it been wired up. Callers should depend on the
// permissions-module function directly so the symlink-resolution + working
// directory expansion logic stays in one place.

// ---------------------------------------------------------------------------
// Core functions
// ---------------------------------------------------------------------------

/// Create an attachment message from an attachment.
pub fn create_attachment_message(attachment: Attachment) -> AttachmentMessage {
    AttachmentMessage {
        attachment,
        msg_type: "attachment".to_string(),
        uuid: Uuid::new_v4().to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    }
}

/// Get queued command attachments from queued commands.
pub async fn get_queued_command_attachments(queued_commands: &[QueuedCommand]) -> Vec<Attachment> {
    if queued_commands.is_empty() {
        return Vec::new();
    }

    let filtered: Vec<&QueuedCommand> = queued_commands
        .iter()
        .filter(|cmd| INLINE_NOTIFICATION_MODES.contains(cmd.mode.as_str()))
        .collect();

    let mut results = Vec::with_capacity(filtered.len());
    for cmd in filtered {
        let image_blocks = build_image_content_blocks(cmd.pasted_contents.as_ref()).await;
        let prompt = if image_blocks.is_empty() {
            cmd.value.clone()
        } else {
            let text_value = extract_text_from_value(&cmd.value);
            let mut blocks: Vec<serde_json::Value> = Vec::new();
            blocks.push(serde_json::json!({"type": "text", "text": text_value}));
            for img in &image_blocks {
                blocks.push(serde_json::to_value(img).unwrap_or_default());
            }
            serde_json::Value::Array(blocks)
        };

        let image_paste_ids = get_image_paste_ids(cmd.pasted_contents.as_ref());

        results.push(Attachment::QueuedCommand {
            prompt,
            source_uuid: cmd.uuid.clone(),
            image_paste_ids,
            command_mode: Some(cmd.mode.clone()),
            origin: cmd.origin.clone(),
            is_meta: cmd.is_meta,
        });
    }

    results
}

/// Extract text from a JSON value (string or content block array).
fn extract_text_from_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(arr) => arr
            .iter()
            .filter_map(|block| {
                if block.get("type")?.as_str()? == "text" {
                    block.get("text")?.as_str().map(|s| s.to_string())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// Get image paste IDs from pasted contents.
fn get_image_paste_ids(
    pasted_contents: Option<&HashMap<String, PastedContent>>,
) -> Option<Vec<u32>> {
    let contents = pasted_contents?;
    let ids: Vec<u32> = contents
        .iter()
        .filter(|(_, pc)| is_valid_image_paste(pc))
        .filter_map(|(k, _)| k.parse::<u32>().ok())
        .collect();
    if ids.is_empty() {
        None
    } else {
        Some(ids)
    }
}

/// Check if a pasted content is a valid image.
fn is_valid_image_paste(pc: &PastedContent) -> bool {
    pc.media_type
        .as_ref()
        .map(|mt| mt.starts_with("image/"))
        .unwrap_or(false)
}

/// Build image content blocks from pasted contents.
async fn build_image_content_blocks(
    pasted_contents: Option<&HashMap<String, PastedContent>>,
) -> Vec<MossenContentBlockParam> {
    let contents = match pasted_contents {
        Some(c) => c,
        None => return Vec::new(),
    };

    let image_contents: Vec<&PastedContent> = contents
        .values()
        .filter(|pc| is_valid_image_paste(pc))
        .collect();

    if image_contents.is_empty() {
        return Vec::new();
    }

    image_contents
        .into_iter()
        .map(|img| {
            let media_type = img
                .media_type
                .clone()
                .unwrap_or_else(|| "image/png".to_string());
            MossenContentBlockParam::Image {
                source: ImageSource {
                    source_type: "base64".to_string(),
                    media_type,
                    data: img.content.clone(),
                },
            }
        })
        .collect()
}

/// Get agent pending message attachments (for subagents).
pub fn get_agent_pending_message_attachments(
    agent_id: Option<&str>,
    pending_messages: Vec<String>,
) -> Vec<Attachment> {
    let _agent_id = match agent_id {
        Some(id) => id,
        None => return Vec::new(),
    };

    pending_messages
        .into_iter()
        .map(|msg| Attachment::QueuedCommand {
            prompt: serde_json::Value::String(msg),
            source_uuid: None,
            image_paste_ids: None,
            command_mode: None,
            origin: Some(MessageOrigin::Coordinator),
            is_meta: Some(true),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Plan mode attachments
// ---------------------------------------------------------------------------

/// Count turns since last plan_mode attachment.
fn get_plan_mode_attachment_turn_count(messages: &[ConversationMessage]) -> (usize, bool) {
    let mut turns_since_last_attachment = 0;
    let mut found_plan_mode_attachment = false;

    for msg in messages.iter().rev() {
        match msg {
            ConversationMessage::User {
                is_meta, message, ..
            } if !is_meta && !has_tool_result_content(&message.content) => {
                turns_since_last_attachment += 1;
            }
            ConversationMessage::Attachment { attachment, .. } => match attachment {
                Attachment::PlanMode { .. } | Attachment::PlanModeReentry { .. } => {
                    found_plan_mode_attachment = true;
                    break;
                }
                _ => {}
            },
            _ => {}
        }
    }

    (turns_since_last_attachment, found_plan_mode_attachment)
}

/// Count plan_mode attachments since last plan_mode_exit.
fn count_plan_mode_attachments_since_last_exit(messages: &[ConversationMessage]) -> usize {
    let mut count = 0;
    for msg in messages.iter().rev() {
        if let ConversationMessage::Attachment { attachment, .. } = msg {
            match attachment {
                Attachment::PlanModeExit { .. } => break,
                Attachment::PlanMode { .. } => count += 1,
                _ => {}
            }
        }
    }
    count
}

/// Get plan mode attachments.
pub async fn get_plan_mode_attachments(
    messages: Option<&[ConversationMessage]>,
    permission_mode: &PermissionMode,
    agent_id: Option<&str>,
    plan_file_path: &str,
    existing_plan: Option<&str>,
    has_exited_plan_mode: bool,
    set_has_exited_plan_mode: &mut dyn FnMut(bool),
) -> Vec<Attachment> {
    if *permission_mode != PermissionMode::Plan {
        return Vec::new();
    }

    if let Some(msgs) = messages {
        if !msgs.is_empty() {
            let (turn_count, found) = get_plan_mode_attachment_turn_count(msgs);
            if found && turn_count < PLAN_MODE_ATTACHMENT_CONFIG.turns_between_attachments {
                return Vec::new();
            }
        }
    }

    let mut attachments = Vec::new();

    // Check for re-entry
    if has_exited_plan_mode && existing_plan.is_some() {
        attachments.push(Attachment::PlanModeReentry {
            plan_file_path: plan_file_path.to_string(),
        });
        set_has_exited_plan_mode(false);
    }

    // Determine reminder type
    let attachment_count = messages
        .map(count_plan_mode_attachments_since_last_exit)
        .unwrap_or(0)
        + 1;
    let reminder_type =
        if attachment_count % PLAN_MODE_ATTACHMENT_CONFIG.full_reminder_every_n_attachments == 1 {
            ReminderType::Full
        } else {
            ReminderType::Sparse
        };

    attachments.push(Attachment::PlanMode {
        reminder_type,
        is_sub_agent: agent_id.map(|_| true),
        plan_file_path: plan_file_path.to_string(),
        plan_exists: existing_plan.is_some(),
    });

    attachments
}

/// Get plan mode exit attachment.
pub fn get_plan_mode_exit_attachment(
    needs_exit: bool,
    permission_mode: &PermissionMode,
    _agent_id: Option<&str>,
    plan_file_path: &str,
    plan_exists: bool,
    set_needs_exit: &mut dyn FnMut(bool),
) -> Vec<Attachment> {
    if !needs_exit {
        return Vec::new();
    }

    if *permission_mode == PermissionMode::Plan {
        set_needs_exit(false);
        return Vec::new();
    }

    set_needs_exit(false);

    vec![Attachment::PlanModeExit {
        plan_file_path: plan_file_path.to_string(),
        plan_exists,
    }]
}

// ---------------------------------------------------------------------------
// Auto mode attachments
// ---------------------------------------------------------------------------

/// Count turns since last auto_mode attachment.
fn get_auto_mode_attachment_turn_count(messages: &[ConversationMessage]) -> (usize, bool) {
    let mut turns_since_last_attachment = 0;
    let mut found = false;

    for msg in messages.iter().rev() {
        match msg {
            ConversationMessage::User {
                is_meta, message, ..
            } if !is_meta && !has_tool_result_content(&message.content) => {
                turns_since_last_attachment += 1;
            }
            ConversationMessage::Attachment { attachment, .. } => match attachment {
                Attachment::AutoMode { .. } => {
                    found = true;
                    break;
                }
                Attachment::AutoModeExit => break,
                _ => {}
            },
            _ => {}
        }
    }

    (turns_since_last_attachment, found)
}

/// Count auto_mode attachments since last auto_mode_exit.
fn count_auto_mode_attachments_since_last_exit(messages: &[ConversationMessage]) -> usize {
    let mut count = 0;
    for msg in messages.iter().rev() {
        if let ConversationMessage::Attachment { attachment, .. } = msg {
            match attachment {
                Attachment::AutoModeExit => break,
                Attachment::AutoMode { .. } => count += 1,
                _ => {}
            }
        }
    }
    count
}

/// Get auto mode attachments.
pub async fn get_auto_mode_attachments(
    messages: Option<&[ConversationMessage]>,
    in_auto: bool,
    in_plan_with_auto: bool,
) -> Vec<Attachment> {
    if !in_auto && !in_plan_with_auto {
        return Vec::new();
    }

    if let Some(msgs) = messages {
        if !msgs.is_empty() {
            let (turn_count, found) = get_auto_mode_attachment_turn_count(msgs);
            if found && turn_count < AUTO_MODE_ATTACHMENT_CONFIG.turns_between_attachments {
                return Vec::new();
            }
        }
    }

    let attachment_count = messages
        .map(count_auto_mode_attachments_since_last_exit)
        .unwrap_or(0)
        + 1;
    let reminder_type =
        if attachment_count % AUTO_MODE_ATTACHMENT_CONFIG.full_reminder_every_n_attachments == 1 {
            ReminderType::Full
        } else {
            ReminderType::Sparse
        };

    vec![Attachment::AutoMode { reminder_type }]
}

/// Get auto mode exit attachment.
pub fn get_auto_mode_exit_attachment(
    needs_exit: bool,
    is_auto_active: bool,
    set_needs_exit: &mut dyn FnMut(bool),
) -> Vec<Attachment> {
    if !needs_exit {
        return Vec::new();
    }

    if is_auto_active {
        set_needs_exit(false);
        return Vec::new();
    }

    set_needs_exit(false);
    vec![Attachment::AutoModeExit]
}

// ---------------------------------------------------------------------------
// Date change detection
// ---------------------------------------------------------------------------

/// Get date change attachments when local date changes.
pub fn get_date_change_attachments(
    current_date: &str,
    last_emitted_date: Option<&str>,
    set_last_date: &mut dyn FnMut(String),
) -> Vec<Attachment> {
    match last_emitted_date {
        None => {
            set_last_date(current_date.to_string());
            Vec::new()
        }
        Some(last) if last == current_date => Vec::new(),
        Some(_) => {
            set_last_date(current_date.to_string());
            vec![Attachment::DateChange {
                new_date: current_date.to_string(),
            }]
        }
    }
}

// ---------------------------------------------------------------------------
// Ultrathink effort
// ---------------------------------------------------------------------------

/// Get ultrathink effort attachment if keyword detected.
pub fn get_ultrathink_effort_attachment(
    input: Option<&str>,
    ultrathink_enabled: bool,
    has_keyword: bool,
) -> Vec<Attachment> {
    if !ultrathink_enabled {
        return Vec::new();
    }
    match input {
        Some(_) if has_keyword => vec![Attachment::UltrathinkEffort {
            level: "high".to_string(),
        }],
        _ => Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Deferred tools delta
// ---------------------------------------------------------------------------

/// Deferred tools delta result.
#[derive(Debug, Clone)]
pub struct DeferredToolsDelta {
    pub added_names: Vec<String>,
    pub added_lines: Vec<String>,
    pub removed_names: Vec<String>,
}

/// Get deferred tools delta attachment.
pub fn get_deferred_tools_delta_attachment(
    delta: Option<DeferredToolsDelta>,
    tool_search_enabled: bool,
    model_supports_tool_ref: bool,
    tool_search_available: bool,
) -> Vec<Attachment> {
    if !tool_search_enabled || !model_supports_tool_ref || !tool_search_available {
        return Vec::new();
    }
    match delta {
        None => Vec::new(),
        Some(d) => vec![Attachment::DeferredToolsDelta {
            added_names: d.added_names,
            added_lines: d.added_lines,
            removed_names: d.removed_names,
        }],
    }
}

// ---------------------------------------------------------------------------
// Agent listing delta
// ---------------------------------------------------------------------------

/// Get agent listing delta attachment.
pub fn get_agent_listing_delta_attachment(
    should_inject: bool,
    has_agent_tool: bool,
    filtered_agents: &[AgentDefinition],
    announced: &HashSet<String>,
    subscription_type: &str,
    format_agent_line: &dyn Fn(&AgentDefinition) -> String,
) -> Vec<Attachment> {
    if !should_inject || !has_agent_tool {
        return Vec::new();
    }

    let current_types: HashSet<&str> = filtered_agents
        .iter()
        .map(|a| a.agent_type.as_str())
        .collect();

    let mut added: Vec<&AgentDefinition> = filtered_agents
        .iter()
        .filter(|a| !announced.contains(&a.agent_type))
        .collect();

    let mut removed: Vec<String> = announced
        .iter()
        .filter(|t| !current_types.contains(t.as_str()))
        .cloned()
        .collect();

    if added.is_empty() && removed.is_empty() {
        return Vec::new();
    }

    added.sort_by(|a, b| a.agent_type.cmp(&b.agent_type));
    removed.sort();

    vec![Attachment::AgentListingDelta {
        added_types: added.iter().map(|a| a.agent_type.clone()).collect(),
        added_lines: added.iter().map(|a| format_agent_line(a)).collect(),
        removed_types: removed,
        is_initial: announced.is_empty(),
        show_concurrency_note: subscription_type != "pro",
    }]
}

// ---------------------------------------------------------------------------
// MCP instructions delta
// ---------------------------------------------------------------------------

/// MCP instructions delta result.
#[derive(Debug, Clone)]
pub struct McpInstructionsDelta {
    pub added_names: Vec<String>,
    pub added_blocks: Vec<String>,
    pub removed_names: Vec<String>,
}

/// Get MCP instructions delta attachment.
pub fn get_mcp_instructions_delta_attachment(
    delta: Option<McpInstructionsDelta>,
) -> Vec<Attachment> {
    match delta {
        None => Vec::new(),
        Some(d) => vec![Attachment::McpInstructionsDelta {
            added_names: d.added_names,
            added_blocks: d.added_blocks,
            removed_names: d.removed_names,
        }],
    }
}

// ---------------------------------------------------------------------------
// Critical system reminder
// ---------------------------------------------------------------------------

/// Get critical system reminder attachment.
pub fn get_critical_system_reminder_attachment(reminder: Option<&str>) -> Vec<Attachment> {
    match reminder {
        None => Vec::new(),
        Some(content) => vec![Attachment::CriticalSystemReminder {
            content: content.to_string(),
        }],
    }
}

// ---------------------------------------------------------------------------
// Output style
// ---------------------------------------------------------------------------

/// Get output style attachment.
pub fn get_output_style_attachment(output_style: &str) -> Vec<Attachment> {
    if output_style == "default" {
        return Vec::new();
    }
    vec![Attachment::OutputStyle {
        style: output_style.to_string(),
    }]
}

// ---------------------------------------------------------------------------
// IDE selection
// ---------------------------------------------------------------------------

/// Get selected lines from IDE attachment.
pub fn get_selected_lines_from_ide(
    ide_selection: Option<&IDESelection>,
    ide_name: Option<&str>,
    permission_ctx: &ToolPermissionContext,
) -> Vec<Attachment> {
    let ide_name = match ide_name {
        Some(n) => n,
        None => return Vec::new(),
    };
    let selection = match ide_selection {
        Some(s) => s,
        None => return Vec::new(),
    };

    let line_start = match selection.line_start {
        Some(ls) => ls,
        None => return Vec::new(),
    };
    let text = match &selection.text {
        Some(t) if !t.is_empty() => t,
        _ => return Vec::new(),
    };
    let file_path = match &selection.file_path {
        Some(fp) => fp,
        None => return Vec::new(),
    };

    if is_file_read_denied(file_path, permission_ctx) {
        return Vec::new();
    }

    let line_count = selection.line_count.unwrap_or(1);
    let cwd = get_cwd();

    vec![Attachment::SelectedLinesInIde {
        ide_name: ide_name.to_string(),
        line_start,
        line_end: line_start + line_count - 1,
        filename: file_path.clone(),
        content: text.clone(),
        display_path: relative_path(&cwd, file_path),
    }]
}

/// Get opened file from IDE attachment.
pub async fn get_opened_file_from_ide(
    ide_selection: Option<&IDESelection>,
    permission_ctx: &ToolPermissionContext,
) -> Vec<Attachment> {
    let selection = match ide_selection {
        Some(s) => s,
        None => return Vec::new(),
    };

    // If there's selected text, this isn't just an opened file
    if selection.text.is_some() {
        return Vec::new();
    }

    let file_path = match &selection.file_path {
        Some(fp) => fp,
        None => return Vec::new(),
    };

    if is_file_read_denied(file_path, permission_ctx) {
        return Vec::new();
    }

    vec![Attachment::OpenedFileInIde {
        filename: file_path.clone(),
    }]
}

// ---------------------------------------------------------------------------
// Directories to process (nested memory traversal)
// ---------------------------------------------------------------------------

/// Compute directories between CWD and target for nested memory loading.
pub fn get_directories_to_process(
    target_path: &str,
    original_cwd: &str,
) -> (Vec<String>, Vec<String>) {
    let target_dir = Path::new(target_path)
        .parent()
        .unwrap_or(Path::new("/"))
        .to_string_lossy()
        .to_string();

    let mut nested_dirs: Vec<String> = Vec::new();
    let mut current_dir = target_dir.clone();

    // Walk up from target directory to original CWD
    loop {
        let parent = Path::new(&current_dir)
            .parent()
            .map(|p| p.to_string_lossy().to_string());

        if current_dir == original_cwd || parent.is_none() {
            break;
        }

        if current_dir.starts_with(original_cwd) {
            nested_dirs.push(current_dir.clone());
        }

        current_dir = match parent {
            Some(p) => p,
            None => break,
        };
    }

    // Reverse to get order from CWD down to target
    nested_dirs.reverse();

    // Build list of directories from root to CWD (for conditional rules only)
    let mut cwd_level_dirs: Vec<String> = Vec::new();
    let mut current_dir = original_cwd.to_string();

    loop {
        let parent = Path::new(&current_dir)
            .parent()
            .map(|p| p.to_string_lossy().to_string());

        match parent {
            Some(ref p) if p.as_str() == current_dir => break,
            None => break,
            _ => {}
        }

        cwd_level_dirs.push(current_dir.clone());
        current_dir = parent.unwrap();
    }

    // Reverse to get order from root to CWD
    cwd_level_dirs.reverse();

    (nested_dirs, cwd_level_dirs)
}

// ---------------------------------------------------------------------------
// Memory files to attachments
// ---------------------------------------------------------------------------

/// Convert memory files to attachment list, deduplicating against loaded paths.
pub fn memory_files_to_attachments(
    memory_files: &[MemoryFileInfo],
    loaded_paths: &mut HashSet<String>,
    read_file_state: &mut FileStateCache,
    _trigger_file_path: Option<&str>,
) -> Vec<Attachment> {
    let mut attachments = Vec::new();
    let cwd = get_cwd();

    for mf in memory_files {
        if loaded_paths.contains(&mf.path) {
            continue;
        }
        if read_file_state.has(&mf.path) {
            continue;
        }

        attachments.push(Attachment::NestedMemory {
            path: mf.path.clone(),
            content: mf.clone(),
            display_path: relative_path(&cwd, &mf.path),
        });

        loaded_paths.insert(mf.path.clone());

        let stored_content = if mf.content_differs_from_disk {
            mf.raw_content.as_deref().unwrap_or(&mf.content)
        } else {
            &mf.content
        };

        read_file_state.set(
            mf.path.clone(),
            FileStateEntry {
                content: stored_content.to_string(),
                timestamp: now_millis(),
                offset: None,
                limit: None,
                is_partial_view: mf.content_differs_from_disk,
            },
        );
    }

    attachments
}

// ---------------------------------------------------------------------------
// Changed files detection
// ---------------------------------------------------------------------------

/// Get changed file attachments by checking mtime against cached state.
pub async fn get_changed_files(
    read_file_state: &mut FileStateCache,
    permission_ctx: &ToolPermissionContext,
) -> Vec<Attachment> {
    let file_paths: Vec<String> = read_file_state.keys().into_iter().cloned().collect();
    if file_paths.is_empty() {
        return Vec::new();
    }

    let mut results = Vec::new();

    for file_path in &file_paths {
        let file_state = match read_file_state.get(file_path) {
            Some(s) => s.clone(),
            None => continue,
        };

        // Skip partial reads
        if file_state.offset.is_some() || file_state.limit.is_some() {
            continue;
        }

        let normalized_path = expand_path(file_path);

        if is_file_read_denied(&normalized_path, permission_ctx) {
            continue;
        }

        match get_file_modification_time_async(&normalized_path).await {
            Ok(mtime) => {
                if mtime <= file_state.timestamp {
                    continue;
                }

                // Try to read the file and compute diff snippet
                match tokio::fs::read_to_string(&normalized_path).await {
                    Ok(new_content) => {
                        let snippet =
                            get_snippet_for_two_file_diff(&file_state.content, &new_content);
                        if snippet.is_empty() {
                            continue;
                        }
                        results.push(Attachment::EditedTextFile {
                            filename: normalized_path,
                            snippet,
                        });
                    }
                    Err(e) => {
                        if e.kind() == std::io::ErrorKind::NotFound {
                            read_file_state.delete(file_path);
                        }
                    }
                }
            }
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    read_file_state.delete(file_path);
                }
            }
        }
    }

    results
}

/// Expand a path (resolve ~ etc).
fn expand_path(path: &str) -> String {
    if path.starts_with('~') {
        if let Some(home) = dirs::home_dir() {
            return path.replacen('~', &home.to_string_lossy(), 1);
        }
    }
    path.to_string()
}

/// Get file modification time as milliseconds since epoch.
async fn get_file_modification_time_async(path: &str) -> std::io::Result<u64> {
    let metadata = tokio::fs::metadata(path).await?;
    let modified = metadata.modified()?;
    Ok(modified
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64)
}

/// Compute a diff snippet between two file versions.
fn get_snippet_for_two_file_diff(old: &str, new: &str) -> String {
    if old == new {
        return String::new();
    }

    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();

    // Find first differing line
    let mut start = 0;
    while start < old_lines.len() && start < new_lines.len() && old_lines[start] == new_lines[start]
    {
        start += 1;
    }

    // Find last differing line (from end)
    let mut old_end = old_lines.len();
    let mut new_end = new_lines.len();
    while old_end > start && new_end > start && old_lines[old_end - 1] == new_lines[new_end - 1] {
        old_end -= 1;
        new_end -= 1;
    }

    // Build snippet with context
    let context = 3;
    let snippet_start = start.saturating_sub(context);
    let snippet_end = (new_end + context).min(new_lines.len());

    if snippet_start >= snippet_end {
        return String::new();
    }

    new_lines[snippet_start..snippet_end].join("\n")
}

// ---------------------------------------------------------------------------
// At-mention extraction
// ---------------------------------------------------------------------------

/// Extract @-mentioned file paths from input text.
pub fn extract_at_mentioned_files(content: &str) -> Vec<String> {
    let mut results: Vec<String> = Vec::new();

    // Extract quoted mentions: @"path with spaces"
    let quoted_re = regex::Regex::new(r#"(?:^|\s)@"([^"]+)""#).unwrap();
    for cap in quoted_re.captures_iter(content) {
        if let Some(m) = cap.get(1) {
            let val = m.as_str();
            if !val.ends_with(" (agent)") && !results.contains(&val.to_string()) {
                results.push(val.to_string());
            }
        }
    }

    // Extract regular mentions: @path
    let regular_re = regex::Regex::new(r"(?:^|\s)@([^\s]+)\b").unwrap();
    for cap in regular_re.captures_iter(content) {
        if let Some(m) = cap.get(1) {
            let val = m.as_str();
            if !val.starts_with('"') && !results.contains(&val.to_string()) {
                results.push(val.to_string());
            }
        }
    }

    results
}

/// Extract MCP resource mentions from input (format: @server:uri).
pub fn extract_mcp_resource_mentions(content: &str) -> Vec<String> {
    let re = regex::Regex::new(r"(?:^|\s)@([^\s]+:[^\s]+)\b").unwrap();
    let mut results: Vec<String> = Vec::new();
    for cap in re.captures_iter(content) {
        if let Some(m) = cap.get(1) {
            let val = m.as_str().to_string();
            if !results.contains(&val) {
                results.push(val);
            }
        }
    }
    results
}

/// Extract agent mentions from input.
pub fn extract_agent_mentions(content: &str) -> Vec<String> {
    let mut results: Vec<String> = Vec::new();

    // Quoted format: @"<type> (agent)"
    let quoted_re = regex::Regex::new(r#"(?:^|\s)@"([\w:.@-]+) \(agent\)""#).unwrap();
    for cap in quoted_re.captures_iter(content) {
        if let Some(m) = cap.get(1) {
            let val = m.as_str().to_string();
            if !results.contains(&val) {
                results.push(val);
            }
        }
    }

    // Unquoted format: @agent-<type>
    let unquoted_re = regex::Regex::new(r"(?:^|\s)@(agent-[\w:.@-]+)").unwrap();
    for cap in unquoted_re.captures_iter(content) {
        if let Some(m) = cap.get(1) {
            let val = m.as_str().to_string();
            if !results.contains(&val) {
                results.push(val);
            }
        }
    }

    results
}

/// Parsed at-mention file lines result.
#[derive(Debug, Clone)]
pub struct AtMentionedFileLines {
    pub filename: String,
    pub line_start: Option<usize>,
    pub line_end: Option<usize>,
}

/// Parse an at-mentioned file reference for line range syntax.
pub fn parse_at_mentioned_file_lines(mention: &str) -> AtMentionedFileLines {
    let re = regex::Regex::new(r"^([^#]+)(?:#L(\d+)(?:-(\d+))?)?(?:#[^#]*)?$").unwrap();
    match re.captures(mention) {
        Some(caps) => {
            let filename = caps
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_else(|| mention.to_string());
            let line_start = caps.get(2).and_then(|m| m.as_str().parse::<usize>().ok());
            let line_end = caps
                .get(3)
                .and_then(|m| m.as_str().parse::<usize>().ok())
                .or(line_start);

            AtMentionedFileLines {
                filename,
                line_start,
                line_end,
            }
        }
        None => AtMentionedFileLines {
            filename: mention.to_string(),
            line_start: None,
            line_end: None,
        },
    }
}

// ---------------------------------------------------------------------------
// Token / budget usage
// ---------------------------------------------------------------------------

/// Get token usage attachment.
pub fn get_token_usage_attachment(
    enabled: bool,
    used_tokens: usize,
    context_window: usize,
) -> Vec<Attachment> {
    if !enabled {
        return Vec::new();
    }
    vec![Attachment::TokenUsage {
        used: used_tokens,
        total: context_window,
        remaining: context_window.saturating_sub(used_tokens),
    }]
}

/// Get output token usage attachment.
pub fn get_output_token_usage_attachment(
    token_budget_enabled: bool,
    budget: Option<usize>,
    turn_tokens: usize,
    session_tokens: usize,
) -> Vec<Attachment> {
    if !token_budget_enabled {
        return Vec::new();
    }
    match budget {
        None | Some(0) => Vec::new(),
        Some(b) => vec![Attachment::OutputTokenUsage {
            turn: turn_tokens,
            session: session_tokens,
            budget: Some(b),
        }],
    }
}

/// Get max budget USD attachment.
pub fn get_max_budget_usd_attachment(
    max_budget_usd: Option<f64>,
    total_cost: f64,
) -> Vec<Attachment> {
    let budget = match max_budget_usd {
        Some(b) => b,
        None => return Vec::new(),
    };
    vec![Attachment::BudgetUsd {
        used: total_cost,
        total: budget,
        remaining: budget - total_cost,
    }]
}

// ---------------------------------------------------------------------------
// Todo / Task reminders
// ---------------------------------------------------------------------------

/// Counts turns since last todo write and last reminder.
fn get_todo_reminder_turn_counts(messages: &[ConversationMessage]) -> (usize, usize) {
    let mut last_todo_write_found = false;
    let mut last_reminder_found = false;
    let mut turns_since_write = 0;
    let mut turns_since_reminder = 0;

    for msg in messages.iter().rev() {
        match msg {
            ConversationMessage::Assistant { message, .. } => {
                // Check for thinking messages (skip them)
                if is_thinking_message_payload(&message.content) {
                    continue;
                }

                // Check for TodoWrite usage
                if !last_todo_write_found && has_tool_use_name(&message.content, "TodoWrite") {
                    last_todo_write_found = true;
                }

                if !last_todo_write_found {
                    turns_since_write += 1;
                }
                if !last_reminder_found {
                    turns_since_reminder += 1;
                }
            }
            ConversationMessage::Attachment { attachment, .. }
                if !last_reminder_found
                    && matches!(attachment, Attachment::TodoReminder { .. }) =>
            {
                last_reminder_found = true;
            }
            _ => {}
        }

        if last_todo_write_found && last_reminder_found {
            break;
        }
    }

    (turns_since_write, turns_since_reminder)
}

/// Get todo reminder attachments.
pub fn get_todo_reminder_attachments(
    messages: Option<&[ConversationMessage]>,
    has_todo_write_tool: bool,
    has_brief_tool: bool,
    todos: &TodoList,
    _session_key: &str,
) -> Vec<Attachment> {
    if !has_todo_write_tool {
        return Vec::new();
    }
    if has_brief_tool {
        return Vec::new();
    }

    let msgs = match messages {
        Some(m) if !m.is_empty() => m,
        _ => return Vec::new(),
    };

    let (turns_since_write, turns_since_reminder) = get_todo_reminder_turn_counts(msgs);

    if turns_since_write >= TODO_REMINDER_CONFIG.turns_since_write
        && turns_since_reminder >= TODO_REMINDER_CONFIG.turns_between_reminders
    {
        vec![Attachment::TodoReminder {
            content: todos.clone(),
            item_count: todos.len(),
        }]
    } else {
        Vec::new()
    }
}

/// Counts turns since last task management and last reminder.
fn get_task_reminder_turn_counts(messages: &[ConversationMessage]) -> (usize, usize) {
    let mut last_mgmt_found = false;
    let mut last_reminder_found = false;
    let mut turns_since_mgmt = 0;
    let mut turns_since_reminder = 0;

    for msg in messages.iter().rev() {
        match msg {
            ConversationMessage::Assistant { message, .. } => {
                if is_thinking_message_payload(&message.content) {
                    continue;
                }

                if !last_mgmt_found
                    && (has_tool_use_name(&message.content, "TaskCreate")
                        || has_tool_use_name(&message.content, "TaskUpdate"))
                {
                    last_mgmt_found = true;
                }

                if !last_mgmt_found {
                    turns_since_mgmt += 1;
                }
                if !last_reminder_found {
                    turns_since_reminder += 1;
                }
            }
            ConversationMessage::Attachment { attachment, .. }
                if !last_reminder_found
                    && matches!(attachment, Attachment::TaskReminder { .. }) =>
            {
                last_reminder_found = true;
            }
            _ => {}
        }

        if last_mgmt_found && last_reminder_found {
            break;
        }
    }

    (turns_since_mgmt, turns_since_reminder)
}

/// Get task reminder attachments.
pub async fn get_task_reminder_attachments(
    messages: Option<&[ConversationMessage]>,
    has_task_update_tool: bool,
    has_brief_tool: bool,
    todo_v2_enabled: bool,
    tasks: Vec<Task>,
) -> Vec<Attachment> {
    if !todo_v2_enabled {
        return Vec::new();
    }
    if !has_task_update_tool {
        return Vec::new();
    }
    if has_brief_tool {
        return Vec::new();
    }

    let msgs = match messages {
        Some(m) if !m.is_empty() => m,
        _ => return Vec::new(),
    };

    let (turns_since_mgmt, turns_since_reminder) = get_task_reminder_turn_counts(msgs);

    if turns_since_mgmt >= TODO_REMINDER_CONFIG.turns_since_write
        && turns_since_reminder >= TODO_REMINDER_CONFIG.turns_between_reminders
    {
        let item_count = tasks.len();
        vec![Attachment::TaskReminder {
            content: tasks,
            item_count,
        }]
    } else {
        Vec::new()
    }
}

// ---------------------------------------------------------------------------
// Verify plan reminder
// ---------------------------------------------------------------------------

/// Count human turns since plan mode exit.
pub fn get_verify_plan_reminder_turn_count(messages: &[ConversationMessage]) -> usize {
    let mut turn_count = 0;
    for msg in messages.iter().rev() {
        match msg {
            ConversationMessage::User {
                is_meta,
                tool_use_result,
                message,
                ..
            } if !is_meta
                && tool_use_result.is_none()
                && !has_tool_result_content(&message.content) =>
            {
                turn_count += 1;
            }
            ConversationMessage::Attachment { attachment, .. } => {
                if matches!(attachment, Attachment::PlanModeExit { .. }) {
                    return turn_count;
                }
            }
            _ => {}
        }
    }
    0
}

/// Get verify plan reminder attachment.
pub fn get_verify_plan_reminder_attachment(
    messages: Option<&[ConversationMessage]>,
    enabled: bool,
    has_pending: bool,
    verification_started: bool,
    verification_completed: bool,
) -> Vec<Attachment> {
    if !enabled {
        return Vec::new();
    }

    if !has_pending || verification_started || verification_completed {
        return Vec::new();
    }

    if let Some(msgs) = messages {
        if !msgs.is_empty() {
            let turn_count = get_verify_plan_reminder_turn_count(msgs);
            if turn_count == 0
                || turn_count % VERIFY_PLAN_REMINDER_CONFIG.turns_between_reminders != 0
            {
                return Vec::new();
            }
        }
    }

    vec![Attachment::VerifyPlanReminder]
}

// ---------------------------------------------------------------------------
// Compaction / Context efficiency
// ---------------------------------------------------------------------------

/// Get compaction reminder attachment.
pub fn get_compaction_reminder_attachment(
    feature_enabled: bool,
    auto_compact_enabled: bool,
    context_window: usize,
    effective_window: usize,
    used_tokens: usize,
) -> Vec<Attachment> {
    if !feature_enabled || !auto_compact_enabled {
        return Vec::new();
    }
    if context_window < 1_000_000 {
        return Vec::new();
    }
    if used_tokens < effective_window / 4 {
        return Vec::new();
    }
    vec![Attachment::CompactionReminder]
}

/// Get context efficiency attachment.
pub fn get_context_efficiency_attachment(
    snip_enabled: bool,
    should_nudge: bool,
) -> Vec<Attachment> {
    if !snip_enabled || !should_nudge {
        return Vec::new();
    }
    vec![Attachment::ContextEfficiency]
}

// ---------------------------------------------------------------------------
// Relevant memories
// ---------------------------------------------------------------------------

/// Collect already-surfaced memory paths from messages.
pub fn collect_surfaced_memories(messages: &[ConversationMessage]) -> (HashSet<String>, usize) {
    let mut paths = HashSet::new();
    let mut total_bytes: usize = 0;

    for msg in messages {
        if let ConversationMessage::Attachment { attachment, .. } = msg {
            if let Attachment::RelevantMemories { memories } = attachment {
                for mem in memories {
                    paths.insert(mem.path.clone());
                    total_bytes += mem.content.len();
                }
            }
        }
    }

    (paths, total_bytes)
}

/// Regex for resume/recall prompts.
static MEMORY_RESUME_PROMPT_RE: once_cell::sync::Lazy<regex::Regex> = once_cell::sync::Lazy::new(
    || {
        regex::Regex::new(
            r"(?i)^(继续|继续做|接着|接着做|恢复|恢复上下文|项目记忆|项目记忆呢|记忆|继续上次|继续之前|continue|resume|memory|memories|recall|handoff)$"
        ).unwrap()
    },
);

/// Build the recall input string from user input.
pub fn build_relevant_memory_recall_input(input: Option<&str>, cwd: &str) -> Option<String> {
    let trimmed = input?.trim();
    if trimmed.is_empty() {
        return None;
    }

    if trimmed.contains(char::is_whitespace) {
        return Some(trimmed.to_string());
    }

    // Strip trailing punctuation for matching
    let normalized = trimmed.trim_end_matches(|c: char| "。！？!?.,，、；;：:".contains(c));
    if !MEMORY_RESUME_PROMPT_RE.is_match(normalized) {
        return None;
    }

    Some(format!(
        "{}\nCurrent project directory: {}\nRecall recent project handoff, task progress, blockers, and next steps for this workspace.",
        trimmed, cwd
    ))
}

/// Read memories for surfacing (with line/byte limits).
pub async fn read_memories_for_surfacing(
    selected: &[RelevantMemoryCandidate],
) -> Vec<RelevantMemoryEntry> {
    let mut results = Vec::new();

    for candidate in selected {
        match read_file_in_range(&candidate.path, 0, MAX_MEMORY_LINES, Some(MAX_MEMORY_BYTES)).await
        {
            Ok(result) => {
                let truncated = result.total_lines > MAX_MEMORY_LINES || result.truncated_by_bytes;
                let content = if truncated {
                    let reason = if result.truncated_by_bytes {
                        format!("{} byte limit", MAX_MEMORY_BYTES)
                    } else {
                        format!("first {} lines", MAX_MEMORY_LINES)
                    };
                    format!(
                        "{}\n\n> This memory file was truncated ({}). Use the Read tool to view the complete file at: {}",
                        result.content, reason, candidate.path
                    )
                } else {
                    result.content
                };

                results.push(RelevantMemoryEntry {
                    path: candidate.path.clone(),
                    content,
                    mtime_ms: candidate.mtime_ms,
                    header: Some(memory_header(&candidate.path, candidate.mtime_ms)),
                    limit: if truncated {
                        Some(result.line_count)
                    } else {
                        None
                    },
                });
            }
            Err(_) => continue,
        }
    }

    results
}

/// Candidate for relevant memory surfacing.
#[derive(Debug, Clone)]
pub struct RelevantMemoryCandidate {
    pub path: String,
    pub mtime_ms: u64,
}

/// Result of reading a file in range.
struct ReadFileInRangeResult {
    content: String,
    total_lines: usize,
    line_count: usize,
    truncated_by_bytes: bool,
}

/// Read a file within line/byte limits.
async fn read_file_in_range(
    path: &str,
    offset: usize,
    max_lines: usize,
    max_bytes: Option<usize>,
) -> std::io::Result<ReadFileInRangeResult> {
    let full_content = tokio::fs::read_to_string(path).await?;
    let all_lines: Vec<&str> = full_content.lines().collect();
    let total_lines = all_lines.len();

    let end = (offset + max_lines).min(total_lines);
    let selected_lines = &all_lines[offset..end];
    let mut content = selected_lines.join("\n");
    let line_count = selected_lines.len();
    let mut truncated_by_bytes = false;

    if let Some(max_b) = max_bytes {
        if content.len() > max_b {
            content.truncate(max_b);
            // Truncate to last complete line
            if let Some(pos) = content.rfind('\n') {
                content.truncate(pos);
            }
            truncated_by_bytes = true;
        }
    }

    Ok(ReadFileInRangeResult {
        content,
        total_lines,
        line_count,
        truncated_by_bytes,
    })
}

/// Generate memory header string.
pub fn memory_header(path: &str, mtime_ms: u64) -> String {
    let freshness = memory_freshness_text(mtime_ms);
    if let Some(text) = freshness {
        format!("{}\n\nMemory: {}:", text, path)
    } else {
        format!("Memory (saved {}): {}:", memory_age(mtime_ms), path)
    }
}

/// Compute memory age string.
fn memory_age(mtime_ms: u64) -> String {
    let now = now_millis();
    let diff_ms = now.saturating_sub(mtime_ms);
    let diff_secs = diff_ms / 1000;
    let diff_mins = diff_secs / 60;
    let diff_hours = diff_mins / 60;
    let diff_days = diff_hours / 24;

    if diff_days > 0 {
        format!(
            "{} day{} ago",
            diff_days,
            if diff_days == 1 { "" } else { "s" }
        )
    } else if diff_hours > 0 {
        format!(
            "{} hour{} ago",
            diff_hours,
            if diff_hours == 1 { "" } else { "s" }
        )
    } else if diff_mins > 0 {
        format!(
            "{} minute{} ago",
            diff_mins,
            if diff_mins == 1 { "" } else { "s" }
        )
    } else {
        "just now".to_string()
    }
}

/// Memory freshness text (returns None if fresh enough to not mention).
fn memory_freshness_text(mtime_ms: u64) -> Option<String> {
    let now = now_millis();
    let diff_ms = now.saturating_sub(mtime_ms);
    let diff_days = diff_ms / (1000 * 60 * 60 * 24);

    if diff_days >= 7 {
        Some(format!(
            "⚠️ This memory was saved {} — it may be outdated.",
            memory_age(mtime_ms)
        ))
    } else {
        None
    }
}

/// Filter duplicate memory attachments against read file state.
pub fn filter_duplicate_memory_attachments(
    attachments: Vec<Attachment>,
    read_file_state: &mut FileStateCache,
) -> Vec<Attachment> {
    attachments
        .into_iter()
        .filter_map(|attachment| match attachment {
            Attachment::RelevantMemories { memories } => {
                let filtered: Vec<RelevantMemoryEntry> = memories
                    .into_iter()
                    .filter(|m| !read_file_state.has(&m.path))
                    .collect();

                for m in &filtered {
                    read_file_state.set(
                        m.path.clone(),
                        FileStateEntry {
                            content: m.content.clone(),
                            timestamp: m.mtime_ms,
                            offset: None,
                            limit: m.limit,
                            is_partial_view: false,
                        },
                    );
                }

                if filtered.is_empty() {
                    None
                } else {
                    Some(Attachment::RelevantMemories { memories: filtered })
                }
            }
            other => Some(other),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Skill listing
// ---------------------------------------------------------------------------

/// Reset sent skill names (called on plugin reload).
pub fn reset_sent_skill_names() {
    SENT_SKILL_NAMES.lock().clear();
    SUPPRESS_NEXT_SKILL.store(false, Ordering::Relaxed);
}

/// Suppress the next skill listing injection (for resume).
pub fn suppress_next_skill_listing() {
    SUPPRESS_NEXT_SKILL.store(true, Ordering::Relaxed);
}

/// Get skill listing attachments.
pub fn get_skill_listing_attachments(
    has_skill_tool: bool,
    all_commands: &[SkillCommand],
    agent_id: Option<&str>,
    format_within_budget: &dyn Fn(&[SkillCommand]) -> String,
) -> Vec<Attachment> {
    if !has_skill_tool {
        return Vec::new();
    }

    let agent_key = agent_id.unwrap_or("").to_string();
    let mut map = SENT_SKILL_NAMES.lock();
    let sent = map.entry(agent_key.clone()).or_default();

    // Handle suppress-next (resume path)
    if SUPPRESS_NEXT_SKILL.swap(false, Ordering::Relaxed) {
        for cmd in all_commands {
            sent.insert(cmd.name.clone());
        }
        return Vec::new();
    }

    let new_skills: Vec<&SkillCommand> = all_commands
        .iter()
        .filter(|cmd| !sent.contains(&cmd.name))
        .collect();

    if new_skills.is_empty() {
        return Vec::new();
    }

    let is_initial = sent.is_empty();

    for cmd in &new_skills {
        sent.insert(cmd.name.clone());
    }

    let new_commands: Vec<SkillCommand> = new_skills.into_iter().cloned().collect();
    let content = format_within_budget(&new_commands);

    vec![Attachment::SkillListing {
        content,
        skill_count: new_commands.len(),
        is_initial,
    }]
}

/// Skill command info.
#[derive(Debug, Clone)]
pub struct SkillCommand {
    pub name: String,
    pub description: Option<String>,
}

// ---------------------------------------------------------------------------
// Dynamic skill attachments
// ---------------------------------------------------------------------------

/// Get dynamic skill attachments from triggered directories.
pub async fn get_dynamic_skill_attachments(triggers: &mut HashSet<String>) -> Vec<Attachment> {
    if triggers.is_empty() {
        return Vec::new();
    }

    let mut attachments = Vec::new();
    let dirs: Vec<String> = triggers.drain().collect();
    let cwd = get_cwd();

    for skill_dir in dirs {
        match tokio::fs::read_dir(&skill_dir).await {
            Ok(mut entries) => {
                let mut candidates = Vec::new();
                while let Ok(Some(entry)) = entries.next_entry().await {
                    if let Ok(ft) = entry.file_type().await {
                        if ft.is_dir() || ft.is_symlink() {
                            candidates.push(entry.file_name().to_string_lossy().to_string());
                        }
                    }
                }

                let mut skill_names = Vec::new();
                for name in candidates {
                    let skill_md = Path::new(&skill_dir).join(&name).join("SKILL.md");
                    if tokio::fs::metadata(&skill_md).await.is_ok() {
                        skill_names.push(name);
                    }
                }

                if !skill_names.is_empty() {
                    attachments.push(Attachment::DynamicSkill {
                        skill_dir: skill_dir.clone(),
                        skill_names,
                        display_path: relative_path(&cwd, &skill_dir),
                    });
                }
            }
            Err(_) => continue,
        }
    }

    attachments
}

// ---------------------------------------------------------------------------
// Teammate mailbox
// ---------------------------------------------------------------------------

/// Get team context attachment (first-turn only).
pub fn get_team_context_attachment(
    messages: &[ConversationMessage],
    team_name: Option<&str>,
    agent_id: Option<&str>,
    agent_name: Option<&str>,
    config_dir: &str,
) -> Vec<Attachment> {
    let team = match team_name {
        Some(t) => t,
        None => return Vec::new(),
    };
    let aid = match agent_id {
        Some(id) => id,
        None => return Vec::new(),
    };

    // Only inject on first turn
    let has_assistant = messages
        .iter()
        .any(|m| matches!(m, ConversationMessage::Assistant { .. }));
    if has_assistant {
        return Vec::new();
    }

    let name = agent_name.unwrap_or(aid);
    let team_config_path = format!("{}/teams/{}/config.json", config_dir, team);
    let task_list_path = format!("{}/tasks/{}/", config_dir, team);

    vec![Attachment::TeamContext {
        agent_id: aid.to_string(),
        agent_name: name.to_string(),
        team_name: team.to_string(),
        team_config_path,
        task_list_path,
    }]
}

// ---------------------------------------------------------------------------
// Async hook responses
// ---------------------------------------------------------------------------

/// Async hook response entry.
#[derive(Debug, Clone)]
pub struct AsyncHookResponseEntry {
    pub process_id: String,
    pub hook_name: String,
    pub hook_event: serde_json::Value,
    pub tool_name: Option<String>,
    pub response: SyncHookJSONOutput,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
}

/// Get async hook response attachments.
pub fn get_async_hook_response_attachments(
    responses: Vec<AsyncHookResponseEntry>,
) -> Vec<Attachment> {
    if responses.is_empty() {
        return Vec::new();
    }

    responses
        .into_iter()
        .map(|r| Attachment::AsyncHookResponse {
            process_id: r.process_id,
            hook_name: r.hook_name,
            hook_event: r.hook_event,
            tool_name: r.tool_name,
            response: r.response,
            stdout: r.stdout,
            stderr: r.stderr,
            exit_code: r.exit_code,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// PDF reference
// ---------------------------------------------------------------------------

/// Try to get a PDF reference attachment for large PDFs.
pub async fn try_get_pdf_reference(filename: &str, inline_threshold: usize) -> Option<Attachment> {
    let ext = Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    if ext != "pdf" {
        return None;
    }

    let metadata = tokio::fs::metadata(filename).await.ok()?;
    let file_size = metadata.len();
    // Heuristic: ~100KB per page
    let effective_page_count = (file_size as f64 / (100.0 * 1024.0)).ceil() as usize;

    if effective_page_count > inline_threshold {
        let cwd = get_cwd();
        Some(Attachment::PdfReference {
            filename: filename.to_string(),
            page_count: effective_page_count,
            file_size,
            display_path: relative_path(&cwd, filename),
        })
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Collect recent successful tools
// ---------------------------------------------------------------------------

/// Collect tool names that succeeded since the last human turn.
pub fn collect_recent_successful_tools(
    messages: &[ConversationMessage],
    last_user_msg_idx: usize,
) -> Vec<String> {
    let mut use_id_to_name: HashMap<String, String> = HashMap::new();
    let mut result_by_use_id: HashMap<String, bool> = HashMap::new();

    for i in (0..messages.len()).rev() {
        let msg = &messages[i];

        // Stop at previous human turn
        if i < last_user_msg_idx {
            if let ConversationMessage::User {
                is_meta,
                tool_use_result,
                ..
            } = msg
            {
                if !is_meta && tool_use_result.is_none() {
                    break;
                }
            }
        }

        match msg {
            ConversationMessage::Assistant { message, .. } => {
                if let Some(arr) = message.content.as_array() {
                    for block in arr {
                        if block.get("type").and_then(|v| v.as_str()) == Some("tool_use") {
                            if let (Some(id), Some(name)) = (
                                block.get("id").and_then(|v| v.as_str()),
                                block.get("name").and_then(|v| v.as_str()),
                            ) {
                                use_id_to_name.insert(id.to_string(), name.to_string());
                            }
                        }
                    }
                }
            }
            ConversationMessage::User { message, .. } => {
                if let Some(arr) = message.content.as_array() {
                    for block in arr {
                        if block.get("type").and_then(|v| v.as_str()) == Some("tool_result") {
                            if let Some(id) = block.get("tool_use_id").and_then(|v| v.as_str()) {
                                let is_error = block
                                    .get("is_error")
                                    .and_then(|v| v.as_bool())
                                    .unwrap_or(false);
                                result_by_use_id.insert(id.to_string(), is_error);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    let mut failed: HashSet<String> = HashSet::new();
    let mut succeeded: HashSet<String> = HashSet::new();

    for (id, name) in &use_id_to_name {
        match result_by_use_id.get(id) {
            None => continue,
            Some(true) => {
                failed.insert(name.clone());
            }
            Some(false) => {
                succeeded.insert(name.clone());
            }
        }
    }

    succeeded
        .into_iter()
        .filter(|t| !failed.contains(t))
        .collect()
}

// ---------------------------------------------------------------------------
// Helper: check message content types
// ---------------------------------------------------------------------------

/// Check if message content contains tool_result blocks.
fn has_tool_result_content(content: &serde_json::Value) -> bool {
    match content.as_array() {
        Some(arr) => arr.iter().any(|block| {
            block.get("type").and_then(|v| v.as_str()) == Some("tool_result")
                && block.get("tool_use_id").and_then(|v| v.as_str()).is_some()
        }),
        None => false,
    }
}

/// Check if assistant message payload is a thinking message.
fn is_thinking_message_payload(content: &serde_json::Value) -> bool {
    match content.as_array() {
        Some(arr) => {
            arr.len() == 1 && arr[0].get("type").and_then(|v| v.as_str()) == Some("thinking")
        }
        None => false,
    }
}

/// Check if assistant content contains a tool_use with the given name.
fn has_tool_use_name(content: &serde_json::Value, name: &str) -> bool {
    match content.as_array() {
        Some(arr) => arr.iter().any(|block| {
            block.get("type").and_then(|v| v.as_str()) == Some("tool_use")
                && block.get("name").and_then(|v| v.as_str()) == Some(name)
        }),
        None => false,
    }
}

// ---------------------------------------------------------------------------
// Process agent mentions
// ---------------------------------------------------------------------------

/// Process @agent mentions from input against known agents.
pub fn process_agent_mentions(input: &str, agents: &[AgentDefinition]) -> Vec<Attachment> {
    let mentions = extract_agent_mentions(input);
    if mentions.is_empty() {
        return Vec::new();
    }

    mentions
        .into_iter()
        .filter_map(|mention| {
            let agent_type = mention.strip_prefix("agent-").unwrap_or(&mention);
            let found = agents.iter().any(|def| def.agent_type == agent_type);
            if found {
                Some(Attachment::AgentMention {
                    agent_type: agent_type.to_string(),
                })
            } else {
                None
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Generate file attachment
// ---------------------------------------------------------------------------

/// Generate a file attachment with validation and truncation.
pub async fn generate_file_attachment(
    filename: &str,
    permission_ctx: &ToolPermissionContext,
    mode: FileAttachmentMode,
    offset: Option<usize>,
    limit: Option<usize>,
    max_lines_to_read: usize,
    max_size_bytes: u64,
    read_file_state: Option<&FileStateCache>,
    inline_pdf_threshold: usize,
) -> Option<Attachment> {
    // Check deny rules
    if is_file_read_denied(filename, permission_ctx) {
        return None;
    }

    // Check file size for at-mention mode (skip for PDFs)
    if mode == FileAttachmentMode::AtMention {
        let ext = Path::new(filename)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        if ext != "pdf" {
            if let Ok(metadata) = tokio::fs::metadata(filename).await {
                if metadata.len() > max_size_bytes {
                    return None;
                }
            }
        }

        // PDF reference for large PDFs
        if let Some(pdf_ref) = try_get_pdf_reference(filename, inline_pdf_threshold).await {
            return Some(pdf_ref);
        }
    }

    // Check if already in context
    if mode == FileAttachmentMode::AtMention {
        if let Some(state) = read_file_state {
            if let Some(existing) = state.get(filename) {
                if let Ok(mtime) = get_file_modification_time_async(filename).await {
                    if existing.timestamp <= mtime && mtime == existing.timestamp {
                        let cwd = get_cwd();
                        let num_lines = count_char_in_string(&existing.content, '\n') + 1;
                        return Some(Attachment::AlreadyReadFile {
                            filename: filename.to_string(),
                            display_path: relative_path(&cwd, filename),
                            content: FileReadToolOutput::Text {
                                file: FileReadTextContent {
                                    file_path: filename.to_string(),
                                    content: existing.content.clone(),
                                    num_lines,
                                    start_line: offset.unwrap_or(1),
                                    total_lines: num_lines,
                                },
                            },
                            truncated: None,
                        });
                    }
                }
            }
        }
    }

    // Try to read the file
    let cwd = get_cwd();
    match tokio::fs::read_to_string(filename).await {
        Ok(content) => {
            let lines: Vec<&str> = content.lines().collect();
            let total_lines = lines.len();

            let start = offset.unwrap_or(0);
            let end = limit
                .map(|l| (start + l).min(total_lines))
                .unwrap_or(total_lines);

            if total_lines > max_lines_to_read && limit.is_none() {
                // Truncated read
                let truncated_end = (start + max_lines_to_read).min(total_lines);
                let truncated_content = lines[start..truncated_end].join("\n");
                let num_lines = truncated_end - start;

                if mode == FileAttachmentMode::Compact {
                    return Some(Attachment::CompactFileReference {
                        filename: filename.to_string(),
                        display_path: relative_path(&cwd, filename),
                    });
                }

                Some(Attachment::File {
                    filename: filename.to_string(),
                    content: FileReadToolOutput::Text {
                        file: FileReadTextContent {
                            file_path: filename.to_string(),
                            content: truncated_content,
                            num_lines,
                            start_line: start + 1,
                            total_lines,
                        },
                    },
                    truncated: Some(true),
                    display_path: relative_path(&cwd, filename),
                })
            } else {
                let selected_content = lines[start..end].join("\n");
                let num_lines = end - start;

                Some(Attachment::File {
                    filename: filename.to_string(),
                    content: FileReadToolOutput::Text {
                        file: FileReadTextContent {
                            file_path: filename.to_string(),
                            content: selected_content,
                            num_lines,
                            start_line: start + 1,
                            total_lines,
                        },
                    },
                    truncated: None,
                    display_path: relative_path(&cwd, filename),
                })
            }
        }
        Err(_) => {
            if mode == FileAttachmentMode::Compact {
                Some(Attachment::CompactFileReference {
                    filename: filename.to_string(),
                    display_path: relative_path(&cwd, filename),
                })
            } else {
                None
            }
        }
    }
}

/// File attachment mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileAttachmentMode {
    AtMention,
    Compact,
}

// ---------------------------------------------------------------------------
// Unified task attachments
// ---------------------------------------------------------------------------

/// Task attachment result from the task framework.
#[derive(Debug, Clone)]
pub struct TaskAttachment {
    pub task_id: String,
    pub task_type: TaskType,
    pub status: TaskStatus,
    pub description: String,
    pub delta_summary: Option<String>,
}

/// Get unified task attachments.
pub fn get_unified_task_attachments(
    task_attachments: Vec<TaskAttachment>,
    get_output_path: &dyn Fn(&str) -> Option<String>,
) -> Vec<Attachment> {
    task_attachments
        .into_iter()
        .map(|ta| Attachment::TaskStatus {
            task_id: ta.task_id.clone(),
            task_type: ta.task_type,
            status: ta.status,
            description: ta.description,
            delta_summary: ta.delta_summary,
            output_file_path: get_output_path(&ta.task_id),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Diagnostic attachments
// ---------------------------------------------------------------------------

/// Get diagnostic attachments (from IDE MCP integration).
pub fn get_diagnostic_attachments(
    has_bash_tool: bool,
    new_diagnostics: Vec<DiagnosticFile>,
) -> Vec<Attachment> {
    if !has_bash_tool || new_diagnostics.is_empty() {
        return Vec::new();
    }
    vec![Attachment::Diagnostics {
        files: new_diagnostics,
        is_new: true,
    }]
}

/// Get LSP diagnostic attachments.
pub fn get_lsp_diagnostic_attachments(
    has_bash_tool: bool,
    diagnostic_sets: Vec<Vec<DiagnosticFile>>,
) -> Vec<Attachment> {
    if !has_bash_tool || diagnostic_sets.is_empty() {
        return Vec::new();
    }

    diagnostic_sets
        .into_iter()
        .map(|files| Attachment::Diagnostics {
            files,
            is_new: true,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Directory attachment (at-mention)
// ---------------------------------------------------------------------------

/// Create a directory listing attachment.
pub async fn create_directory_attachment(path: &str, max_entries: usize) -> Option<Attachment> {
    let mut entries_list = Vec::new();
    let mut dir = match tokio::fs::read_dir(path).await {
        Ok(d) => d,
        Err(_) => return None,
    };

    while let Ok(Some(entry)) = dir.next_entry().await {
        entries_list.push(entry.file_name().to_string_lossy().to_string());
    }

    let total = entries_list.len();
    let truncated = total > max_entries;
    let names: Vec<String> = entries_list.into_iter().take(max_entries).collect();

    let mut content = names.join("\n");
    if truncated {
        content.push_str(&format!(
            "\n\u{2026} and {} more entries",
            total - max_entries
        ));
    }

    let cwd = get_cwd();
    Some(Attachment::Directory {
        path: path.to_string(),
        content,
        display_path: relative_path(&cwd, path),
    })
}

/// 对应 TS `getAttachments`：根据用户输入与上下文构造附件列表。
///
/// Rust 端的完整 attachments 流水线由多个独立函数组合：本函数提供一个高层
/// 入口，把已经准备好的候选附件（一般由调用方在 prompt 阶段收集）合并成一个
/// 列表后返回。
pub async fn get_attachments(prepared: Vec<Attachment>) -> Vec<Attachment> {
    prepared
}

/// 对应 TS `startRelevantMemoryPrefetch`：异步预取相关记忆。
///
/// Rust 端未接入 memory 服务时，函数返回一个立即完成的 join handle，调用方
/// 可以 `.await` 等待预取结束（保持 API parity）。
pub fn start_relevant_memory_prefetch(_query: &str) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        // 真实实现需调用 memory/relevant_memories 服务；此处保留 hook。
    })
}
