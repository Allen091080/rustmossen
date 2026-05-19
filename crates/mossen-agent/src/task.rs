//! # task — 任务定义与管理
//!
//! 对应 TS `Task.ts`，定义 Agent 任务的数据结构。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// 任务定义
// ---------------------------------------------------------------------------

/// 任务状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// 待处理。
    Pending,
    /// 进行中。
    InProgress,
    /// 已完成。
    Completed,
    /// 已取消。
    Cancelled,
    /// 失败。
    Failed,
}

/// Agent 任务定义。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTask {
    /// 任务 ID。
    pub id: String,
    /// 任务标题。
    pub subject: String,
    /// 任务描述。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 任务状态。
    pub status: TaskStatus,
    /// 所有者（Agent 名称）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// 阻塞的任务 ID。
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<String>,
    /// 被阻塞的任务 ID。
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub blocked_by: Vec<String>,
    /// 创建时间。
    pub created: String,
    /// 更新时间。
    pub updated: String,
    /// 额外元数据。
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, serde_json::Value>,
}

impl AgentTask {
    /// 创建新任务。
    pub fn new(id: String, subject: String) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id,
            subject,
            description: None,
            status: TaskStatus::Pending,
            owner: None,
            blocks: Vec::new(),
            blocked_by: Vec::new(),
            created: now.clone(),
            updated: now,
            metadata: HashMap::new(),
        }
    }

    /// 是否已完成。
    pub fn is_completed(&self) -> bool {
        matches!(
            self.status,
            TaskStatus::Completed | TaskStatus::Cancelled | TaskStatus::Failed
        )
    }

    /// 是否被阻塞。
    pub fn is_blocked(&self) -> bool {
        !self.blocked_by.is_empty()
    }

    /// 标记为进行中。
    pub fn start(&mut self) {
        self.status = TaskStatus::InProgress;
        self.updated = chrono::Utc::now().to_rfc3339();
    }

    /// 标记为已完成。
    pub fn complete(&mut self) {
        self.status = TaskStatus::Completed;
        self.updated = chrono::Utc::now().to_rfc3339();
    }

    /// 标记为失败。
    pub fn fail(&mut self) {
        self.status = TaskStatus::Failed;
        self.updated = chrono::Utc::now().to_rfc3339();
    }
}

// ---------------------------------------------------------------------------
// 任务列表
// ---------------------------------------------------------------------------

/// 任务列表管理器。
#[derive(Debug, Clone, Default)]
pub struct TaskList {
    tasks: Vec<AgentTask>,
}

impl TaskList {
    pub fn new() -> Self {
        Self { tasks: Vec::new() }
    }

    /// 添加任务。
    pub fn add(&mut self, task: AgentTask) {
        self.tasks.push(task);
    }

    /// 按 ID 获取任务。
    pub fn get(&self, id: &str) -> Option<&AgentTask> {
        self.tasks.iter().find(|t| t.id == id)
    }

    /// 按 ID 获取可变任务。
    pub fn get_mut(&mut self, id: &str) -> Option<&mut AgentTask> {
        self.tasks.iter_mut().find(|t| t.id == id)
    }

    /// 获取所有任务。
    pub fn all(&self) -> &[AgentTask] {
        &self.tasks
    }

    /// 获取待处理的任务。
    pub fn pending(&self) -> Vec<&AgentTask> {
        self.tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Pending)
            .collect()
    }

    /// 获取进行中的任务。
    pub fn in_progress(&self) -> Vec<&AgentTask> {
        self.tasks
            .iter()
            .filter(|t| t.status == TaskStatus::InProgress)
            .collect()
    }

    /// 按 ID 移除任务。
    pub fn remove(&mut self, id: &str) -> Option<AgentTask> {
        if let Some(pos) = self.tasks.iter().position(|t| t.id == id) {
            Some(self.tasks.remove(pos))
        } else {
            None
        }
    }

    /// 任务数量。
    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Top-level Task.ts mirror — 与上面的 AgentTask 模型并存，对应 TS 中的
// `TaskType` / `TaskStatus` / `TaskHandle` / 生成器等。
// ---------------------------------------------------------------------------

/// `Task.ts` `TaskType`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    LocalBash,
    LocalAgent,
    RemoteAgent,
    InProcessTeammate,
    LocalWorkflow,
    MonitorMcp,
    Dream,
}

/// `Task.ts` `TaskStatus` (top-level)。注意此处与 `TaskStatus`（业务实体）
/// 不同，因此另起一个枚举名字。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskRunStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Killed,
}

/// `Task.ts` `isTerminalTaskStatus`。
pub fn is_terminal_task_status(status: TaskRunStatus) -> bool {
    matches!(
        status,
        TaskRunStatus::Completed | TaskRunStatus::Failed | TaskRunStatus::Killed
    )
}

/// `Task.ts` `TaskHandle`。`cleanup` 用 `Option<Box<dyn FnOnce()>>` 持有。
pub struct TaskHandle {
    pub task_id: String,
    pub cleanup: Option<Box<dyn FnOnce() + Send>>,
}

impl std::fmt::Debug for TaskHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TaskHandle")
            .field("task_id", &self.task_id)
            .field("cleanup", &self.cleanup.as_ref().map(|_| "<fn>"))
            .finish()
    }
}

/// 共享给所有任务的基础字段 — 对应 TS `TaskStateBase`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStateBase {
    pub id: String,
    #[serde(rename = "type")]
    pub task_type: TaskType,
    pub status: TaskRunStatus,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_use_id: Option<String>,
    pub start_time: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_paused_ms: Option<i64>,
    pub output_file: String,
    pub output_offset: usize,
    pub notified: bool,
}

fn task_id_prefix(t: TaskType) -> char {
    match t {
        TaskType::LocalBash => 'b',
        TaskType::LocalAgent => 'a',
        TaskType::RemoteAgent => 'r',
        TaskType::InProcessTeammate => 't',
        TaskType::LocalWorkflow => 'w',
        TaskType::MonitorMcp => 'm',
        TaskType::Dream => 'd',
    }
}

const TASK_ID_ALPHABET: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";

/// `Task.ts` `generateTaskId` — prefix + 8 base36 chars from CSPRNG.
pub fn generate_task_id(task_type: TaskType) -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 8];
    rand::thread_rng().fill_bytes(&mut bytes);
    let prefix = task_id_prefix(task_type);
    let mut out = String::with_capacity(9);
    out.push(prefix);
    for b in bytes.iter() {
        let idx = (*b as usize) % TASK_ID_ALPHABET.len();
        out.push(TASK_ID_ALPHABET[idx] as char);
    }
    out
}

/// 由调用方注入的 task output 路径解析器。
pub type TaskOutputPathFn = fn(&str) -> String;

fn default_task_output_path(id: &str) -> String {
    // 与 TS `getTaskOutputPath` 一致的默认目录布局
    let mut p = std::env::temp_dir();
    p.push("mossen-tasks");
    p.push(format!("{}.out", id));
    p.to_string_lossy().into_owned()
}

/// `Task.ts` `createTaskStateBase`。
pub fn create_task_state_base(
    id: String,
    task_type: TaskType,
    description: String,
    tool_use_id: Option<String>,
) -> TaskStateBase {
    let start_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let output_file = default_task_output_path(&id);
    TaskStateBase {
        id,
        task_type,
        status: TaskRunStatus::Pending,
        description,
        tool_use_id,
        start_time,
        end_time: None,
        total_paused_ms: None,
        output_file,
        output_offset: 0,
        notified: false,
    }
}

/// `Task.ts` `LocalShellSpawnInput`。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalShellSpawnInput {
    pub command: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_use_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// `'bash'` 或 `'monitor'`。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

/// `Task.ts` `SetAppState = (f: (prev: AppState) => AppState) => void` — a
/// callback that takes a transform fn over `AppState`. We use `serde_json::Value`
/// for `AppState` until the strongly-typed view is wired through.
pub type SetAppState = Box<
    dyn Fn(Box<dyn Fn(&serde_json::Value) -> serde_json::Value + Send + Sync>) + Send + Sync,
>;

/// `Task.ts` `TaskContext` — runtime context passed to task handlers. Holds an
/// abort handle, an app-state getter, and a setter.
pub struct TaskContext {
    pub abort: tokio_util::sync::CancellationToken,
    pub get_app_state: Box<dyn Fn() -> serde_json::Value + Send + Sync>,
    pub set_app_state: SetAppState,
}

impl std::fmt::Debug for TaskContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TaskContext")
            .field("abort", &self.abort)
            .finish_non_exhaustive()
    }
}
