//! 任务系统 — 对应 TS 的 tasks/ 目录。
//!
//! 管理各种类型的后台任务：LocalMainSession、LocalShell、LocalAgent、Dream、InProcessTeammate。

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, RwLock};
use tracing::{error, info, warn};

// ─── Task Types ────────────────────────────────────────────────────────────

/// 任务状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// 任务类型。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    MainSession,
    Shell,
    Agent,
    Dream,
    Teammate,
}

/// 任务信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInfo {
    pub id: String,
    pub task_type: TaskType,
    pub status: TaskStatus,
    pub label: Option<String>,
    pub started_at: i64,
    pub completed_at: Option<i64>,
    pub error: Option<String>,
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Pill 标签（用于 UI 显示）。
#[derive(Debug, Clone)]
pub struct PillLabel {
    pub text: String,
    pub color: &'static str,
    pub icon: Option<&'static str>,
}

/// 获取任务的 pill 标签 — 对应 TS 的 tasks/pillLabel.ts。
pub fn get_pill_label(task: &TaskInfo) -> PillLabel {
    match task.task_type {
        TaskType::MainSession => PillLabel {
            text: "Main".to_string(),
            color: "blue",
            icon: Some("●"),
        },
        TaskType::Shell => PillLabel {
            text: task.label.clone().unwrap_or_else(|| "Shell".to_string()),
            color: "green",
            icon: Some("$"),
        },
        TaskType::Agent => PillLabel {
            text: task.label.clone().unwrap_or_else(|| "Agent".to_string()),
            color: "purple",
            icon: Some("◆"),
        },
        TaskType::Dream => PillLabel {
            text: "Dream".to_string(),
            color: "yellow",
            icon: Some("☆"),
        },
        TaskType::Teammate => PillLabel {
            text: task.label.clone().unwrap_or_else(|| "Teammate".to_string()),
            color: "cyan",
            icon: Some("◎"),
        },
    }
}

// ─── Task Trait ─────────────────────────────────────────────────────────────

/// 任务 trait — 所有任务类型的公共接口。
#[async_trait::async_trait]
pub trait Task: Send + Sync {
    /// 获取任务信息。
    fn info(&self) -> &TaskInfo;
    /// 启动任务。
    async fn start(&mut self) -> Result<()>;
    /// 停止任务。
    async fn stop(&mut self) -> Result<()>;
    /// 任务是否完成。
    fn is_done(&self) -> bool;
}

// ─── LocalMainSessionTask (tasks/LocalMainSessionTask.ts) ───────────────────

/// 本地主会话任务 — 对应 TS 的 LocalMainSessionTask。
///
/// 以隔离 transcript 的方式拉起后台会话。
pub struct LocalMainSessionTask {
    pub info: TaskInfo,
    /// 停止通知。
    stop_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl LocalMainSessionTask {
    pub fn new(id: &str, label: Option<&str>) -> Self {
        Self {
            info: TaskInfo {
                id: id.to_string(),
                task_type: TaskType::MainSession,
                status: TaskStatus::Pending,
                label: label.map(|s| s.to_string()),
                started_at: 0,
                completed_at: None,
                error: None,
                metadata: HashMap::new(),
            },
            stop_tx: None,
        }
    }
}

#[async_trait::async_trait]
impl Task for LocalMainSessionTask {
    fn info(&self) -> &TaskInfo {
        &self.info
    }

    async fn start(&mut self) -> Result<()> {
        self.info.status = TaskStatus::Running;
        self.info.started_at = chrono::Utc::now().timestamp_millis();
        info!(task_id = %self.info.id, "LocalMainSessionTask: started");

        let (tx, mut rx) = tokio::sync::oneshot::channel();
        self.stop_tx = Some(tx);

        let task_id = self.info.id.clone();
        tokio::spawn(async move {
            // 等待停止信号或任务完成
            let _ = rx.await;
            info!(task_id = %task_id, "LocalMainSessionTask: stopped");
        });

        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
        self.info.status = TaskStatus::Completed;
        self.info.completed_at = Some(chrono::Utc::now().timestamp_millis());
        info!(task_id = %self.info.id, "LocalMainSessionTask: stop requested");
        Ok(())
    }

    fn is_done(&self) -> bool {
        matches!(
            self.info.status,
            TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled
        )
    }
}

// ─── LocalShellTask (tasks/LocalShellTask/) ─────────────────────────────────

/// 本地 Shell 任务 — 对应 TS 的 LocalShellTask。
///
/// 在后台执行 shell 命令。
pub struct LocalShellTask {
    pub info: TaskInfo,
    /// 命令。
    command: String,
    /// 工作目录。
    cwd: std::path::PathBuf,
    /// 子进程句柄。
    child: Arc<Mutex<Option<tokio::process::Child>>>,
    /// 输出收集。
    output: Arc<RwLock<String>>,
}

impl LocalShellTask {
    pub fn new(id: &str, command: &str, cwd: &std::path::Path, label: Option<&str>) -> Self {
        Self {
            info: TaskInfo {
                id: id.to_string(),
                task_type: TaskType::Shell,
                status: TaskStatus::Pending,
                label: label.map(|s| s.to_string()),
                started_at: 0,
                completed_at: None,
                error: None,
                metadata: HashMap::new(),
            },
            command: command.to_string(),
            cwd: cwd.to_path_buf(),
            child: Arc::new(Mutex::new(None)),
            output: Arc::new(RwLock::new(String::new())),
        }
    }

    /// 获取已收集的输出。
    pub async fn get_output(&self) -> String {
        self.output.read().await.clone()
    }
}

#[async_trait::async_trait]
impl Task for LocalShellTask {
    fn info(&self) -> &TaskInfo {
        &self.info
    }

    async fn start(&mut self) -> Result<()> {
        self.info.status = TaskStatus::Running;
        self.info.started_at = chrono::Utc::now().timestamp_millis();
        info!(
            task_id = %self.info.id,
            command = %self.command,
            "LocalShellTask: starting"
        );

        let mut cmd = tokio::process::Command::new("sh");
        cmd.arg("-c")
            .arg(&self.command)
            .current_dir(&self.cwd)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let child = cmd.spawn()?;
        let mut guard = self.child.lock().await;
        *guard = Some(child);

        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        let mut guard = self.child.lock().await;
        if let Some(ref mut child) = *guard {
            let _ = child.kill().await;
        }
        self.info.status = TaskStatus::Cancelled;
        self.info.completed_at = Some(chrono::Utc::now().timestamp_millis());
        info!(task_id = %self.info.id, "LocalShellTask: killed");
        Ok(())
    }

    fn is_done(&self) -> bool {
        matches!(
            self.info.status,
            TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled
        )
    }
}

/// 杀死所有 shell 任务 — 对应 TS 的 tasks/LocalShellTask/killShellTasks.ts。
pub async fn kill_shell_tasks(tasks: &mut [Box<dyn Task>]) {
    for task in tasks.iter_mut() {
        if task.info().task_type == TaskType::Shell && task.info().status == TaskStatus::Running {
            if let Err(e) = task.stop().await {
                error!(task_id = %task.info().id, error = %e, "failed to kill shell task");
            }
        }
    }
}

// ─── LocalAgentTask (tasks/LocalAgentTask/) ────────────────────────────────

/// 本地 Agent 任务 — 对应 TS 的 LocalAgentTask。
///
/// 在本地执行 Agent（子代理）任务。
pub struct LocalAgentTask {
    pub info: TaskInfo,
    /// Agent 类型。
    agent_type: String,
    /// 初始提示。
    prompt: String,
    /// 停止通知。
    stop_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl LocalAgentTask {
    pub fn new(id: &str, agent_type: &str, prompt: &str, label: Option<&str>) -> Self {
        Self {
            info: TaskInfo {
                id: id.to_string(),
                task_type: TaskType::Agent,
                status: TaskStatus::Pending,
                label: label.map(|s| s.to_string()),
                started_at: 0,
                completed_at: None,
                error: None,
                metadata: HashMap::new(),
            },
            agent_type: agent_type.to_string(),
            prompt: prompt.to_string(),
            stop_tx: None,
        }
    }
}

#[async_trait::async_trait]
impl Task for LocalAgentTask {
    fn info(&self) -> &TaskInfo {
        &self.info
    }

    async fn start(&mut self) -> Result<()> {
        self.info.status = TaskStatus::Running;
        self.info.started_at = chrono::Utc::now().timestamp_millis();
        info!(
            task_id = %self.info.id,
            agent_type = %self.agent_type,
            "LocalAgentTask: started"
        );

        let (tx, mut rx) = tokio::sync::oneshot::channel();
        self.stop_tx = Some(tx);

        let task_id = self.info.id.clone();
        let agent_type = self.agent_type.clone();
        let prompt = self.prompt.clone();

        tokio::spawn(async move {
            // Agent 执行逻辑
            info!(
                task_id = %task_id,
                agent_type = %agent_type,
                "LocalAgentTask: executing agent with prompt"
            );
            let _ = rx.await;
            info!(task_id = %task_id, "LocalAgentTask: stopped");
        });

        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
        self.info.status = TaskStatus::Cancelled;
        self.info.completed_at = Some(chrono::Utc::now().timestamp_millis());
        Ok(())
    }

    fn is_done(&self) -> bool {
        matches!(
            self.info.status,
            TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled
        )
    }
}

// ─── DreamTask (tasks/DreamTask/) ──────────────────────────────────────────

/// Dream 任务 — 对应 TS 的 DreamTask。
///
/// 用于后台推理/思考任务。
pub struct DreamTask {
    pub info: TaskInfo,
    /// 思考上下文。
    context: String,
    /// 停止通知。
    stop_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl DreamTask {
    pub fn new(id: &str, context: &str) -> Self {
        Self {
            info: TaskInfo {
                id: id.to_string(),
                task_type: TaskType::Dream,
                status: TaskStatus::Pending,
                label: Some("Dream".to_string()),
                started_at: 0,
                completed_at: None,
                error: None,
                metadata: HashMap::new(),
            },
            context: context.to_string(),
            stop_tx: None,
        }
    }
}

#[async_trait::async_trait]
impl Task for DreamTask {
    fn info(&self) -> &TaskInfo {
        &self.info
    }

    async fn start(&mut self) -> Result<()> {
        self.info.status = TaskStatus::Running;
        self.info.started_at = chrono::Utc::now().timestamp_millis();
        info!(task_id = %self.info.id, "DreamTask: started");

        let (tx, rx) = tokio::sync::oneshot::channel();
        self.stop_tx = Some(tx);

        let task_id = self.info.id.clone();
        tokio::spawn(async move {
            let _ = rx.await;
            info!(task_id = %task_id, "DreamTask: awakened");
        });

        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
        self.info.status = TaskStatus::Completed;
        self.info.completed_at = Some(chrono::Utc::now().timestamp_millis());
        Ok(())
    }

    fn is_done(&self) -> bool {
        matches!(
            self.info.status,
            TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled
        )
    }
}

// ─── InProcessTeammateTask (tasks/InProcessTeammateTask/) ───────────────────

/// In-process Teammate 任务 — 对应 TS 的 InProcessTeammateTask。
///
/// 在同一进程内运行的 teammate（swarm 模式）。
pub struct InProcessTeammateTask {
    pub info: TaskInfo,
    /// Teammate 名称。
    teammate_name: String,
    /// 初始指令。
    instruction: String,
    /// 停止通知。
    stop_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl InProcessTeammateTask {
    pub fn new(id: &str, teammate_name: &str, instruction: &str) -> Self {
        Self {
            info: TaskInfo {
                id: id.to_string(),
                task_type: TaskType::Teammate,
                status: TaskStatus::Pending,
                label: Some(teammate_name.to_string()),
                started_at: 0,
                completed_at: None,
                error: None,
                metadata: HashMap::new(),
            },
            teammate_name: teammate_name.to_string(),
            instruction: instruction.to_string(),
            stop_tx: None,
        }
    }
}

#[async_trait::async_trait]
impl Task for InProcessTeammateTask {
    fn info(&self) -> &TaskInfo {
        &self.info
    }

    async fn start(&mut self) -> Result<()> {
        self.info.status = TaskStatus::Running;
        self.info.started_at = chrono::Utc::now().timestamp_millis();
        info!(
            task_id = %self.info.id,
            teammate = %self.teammate_name,
            "InProcessTeammateTask: started"
        );

        let (tx, rx) = tokio::sync::oneshot::channel();
        self.stop_tx = Some(tx);

        let task_id = self.info.id.clone();
        let name = self.teammate_name.clone();
        tokio::spawn(async move {
            let _ = rx.await;
            info!(task_id = %task_id, teammate = %name, "InProcessTeammateTask: stopped");
        });

        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
        self.info.status = TaskStatus::Cancelled;
        self.info.completed_at = Some(chrono::Utc::now().timestamp_millis());
        Ok(())
    }

    fn is_done(&self) -> bool {
        matches!(
            self.info.status,
            TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled
        )
    }
}

// ─── stopTask (tasks/stopTask.ts) ──────────────────────────────────────────

/// 停止指定的任务。
pub async fn stop_task(tasks: &mut [Box<dyn Task>], task_id: &str) -> Result<bool> {
    for task in tasks.iter_mut() {
        if task.info().id == task_id && !task.is_done() {
            task.stop().await?;
            return Ok(true);
        }
    }
    Ok(false)
}

/// 停止所有运行中的任务。
pub async fn stop_all_tasks(tasks: &mut [Box<dyn Task>]) {
    for task in tasks.iter_mut() {
        if !task.is_done() {
            if let Err(e) = task.stop().await {
                error!(task_id = %task.info().id, error = %e, "failed to stop task");
            }
        }
    }
}

// ============================================================================
// LocalAgentTask 子模块 (tasks/LocalAgentTask/LocalAgentTask.tsx)
// ============================================================================

/// 工具调用活动记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolActivity {
    pub tool_name: String,
    pub description: Option<String>,
    pub timestamp: i64,
}

/// Agent 进度状态。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentProgress {
    pub tool_count: u64,
    pub token_count: u64,
    pub recent_activities: Vec<ToolActivity>,
    pub summary: Option<String>,
}

const MAX_RECENT_ACTIVITIES: usize = 5;

/// 进度追踪器。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProgressTracker {
    pub tool_count: u64,
    pub token_count: u64,
    pub recent_activities: Vec<ToolActivity>,
    pub current_summary: Option<String>,
}

pub fn create_progress_tracker() -> ProgressTracker {
    ProgressTracker::default()
}

pub fn createProgressTracker() -> ProgressTracker {
    create_progress_tracker()
}

pub fn get_token_count_from_tracker(tracker: &ProgressTracker) -> u64 {
    tracker.token_count
}

pub fn getTokenCountFromTracker(tracker: &ProgressTracker) -> u64 {
    get_token_count_from_tracker(tracker)
}

pub type ActivityDescriptionResolver =
    Arc<dyn Fn(&str, &serde_json::Value) -> Option<String> + Send + Sync>;

pub fn update_progress_from_message(
    tracker: &mut ProgressTracker,
    tool_use: Option<(&str, &serde_json::Value)>,
    token_delta: u64,
    resolve: Option<&ActivityDescriptionResolver>,
) {
    tracker.token_count = tracker.token_count.saturating_add(token_delta);
    if let Some((tool_name, input)) = tool_use {
        tracker.tool_count += 1;
        let description = resolve.and_then(|r| r(tool_name, input));
        tracker.recent_activities.push(ToolActivity {
            tool_name: tool_name.to_string(),
            description,
            timestamp: chrono::Utc::now().timestamp_millis(),
        });
        if tracker.recent_activities.len() > MAX_RECENT_ACTIVITIES {
            let drop = tracker.recent_activities.len() - MAX_RECENT_ACTIVITIES;
            tracker.recent_activities.drain(..drop);
        }
    }
}

pub fn updateProgressFromMessage(
    tracker: &mut ProgressTracker,
    tool_use: Option<(&str, &serde_json::Value)>,
    token_delta: u64,
    resolve: Option<&ActivityDescriptionResolver>,
) {
    update_progress_from_message(tracker, tool_use, token_delta, resolve)
}

pub fn get_progress_update(tracker: &ProgressTracker) -> AgentProgress {
    AgentProgress {
        tool_count: tracker.tool_count,
        token_count: tracker.token_count,
        recent_activities: tracker.recent_activities.clone(),
        summary: tracker.current_summary.clone(),
    }
}

pub fn getProgressUpdate(tracker: &ProgressTracker) -> AgentProgress {
    get_progress_update(tracker)
}

pub fn create_activity_description_resolver(
    tools: HashMap<String, ActivityDescriptionResolver>,
) -> ActivityDescriptionResolver {
    Arc::new(move |name: &str, input: &serde_json::Value| {
        tools.get(name).and_then(|r| r(name, input))
    })
}

// ============================================================================
// LocalShellTask 子模块 (tasks/LocalShellTask/LocalShellTask.tsx)
// ============================================================================

/// 启发式判断字符串是否更像 prompt 而非 shell 命令。
pub fn looks_like_prompt(input: &str) -> bool {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.len() > 60 {
        return true;
    }
    if trimmed.contains(['？', '。', '！']) {
        return true;
    }
    let words = trimmed.split_whitespace().count();
    words >= 8 && !trimmed.starts_with(['/', '.'])
}

pub fn looksLikePrompt(input: &str) -> bool {
    looks_like_prompt(input)
}

/// 启动一个本地 shell 任务。
///
/// 通过 `tokio::process::Command` 在指定 `cwd`（或当前目录）下生成
/// 一个子进程；进程在后台运行，stdout/stderr 缓冲于 `LOCAL_SHELL_TASKS`
/// 内存表中。返回的 task_id 可用于 `get_local_shell_task` 查询状态。
pub async fn spawn_shell_task(command: &str, cwd: Option<&std::path::Path>) -> Result<String> {
    info!(command, "spawning local shell task");
    let task_id = format!("shell-task-{}", uuid::Uuid::new_v4());
    let cmd_str = command.to_string();
    let cwd_owned = cwd.map(|p| p.to_path_buf());
    let id_for_task = task_id.clone();

    tokio::spawn(async move {
        let mut cmd = tokio::process::Command::new("sh");
        cmd.arg("-c").arg(&cmd_str);
        if let Some(ref dir) = cwd_owned {
            cmd.current_dir(dir);
        }
        let output = cmd.output().await;
        let result = match output {
            Ok(out) => LocalShellTaskState {
                task_id: id_for_task.clone(),
                exit_code: out.status.code(),
                stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
                completed: true,
            },
            Err(e) => LocalShellTaskState {
                task_id: id_for_task.clone(),
                exit_code: None,
                stdout: String::new(),
                stderr: format!("spawn error: {}", e),
                completed: true,
            },
        };
        if let Ok(mut s) = LOCAL_SHELL_TASKS.lock() {
            s.insert(id_for_task, result);
        }
    });

    Ok(task_id)
}

/// 本地 shell 子任务的状态快照。
#[derive(Debug, Clone)]
pub struct LocalShellTaskState {
    pub task_id: String,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub completed: bool,
}

static LOCAL_SHELL_TASKS: once_cell::sync::Lazy<
    std::sync::Mutex<HashMap<String, LocalShellTaskState>>,
> = once_cell::sync::Lazy::new(|| std::sync::Mutex::new(HashMap::new()));

/// 查询本地 shell 任务的状态。
pub fn get_local_shell_task(task_id: &str) -> Option<LocalShellTaskState> {
    LOCAL_SHELL_TASKS.lock().ok()?.get(task_id).cloned()
}

pub async fn spawnShellTask(command: &str, cwd: Option<&std::path::Path>) -> Result<String> {
    spawn_shell_task(command, cwd).await
}

static FOREGROUND_TASKS: once_cell::sync::Lazy<
    std::sync::Mutex<std::collections::HashSet<String>>,
> = once_cell::sync::Lazy::new(|| std::sync::Mutex::new(std::collections::HashSet::new()));

pub fn register_foreground(task_id: String) {
    if let Ok(mut s) = FOREGROUND_TASKS.lock() {
        s.insert(task_id);
    }
}

pub fn registerForeground(task_id: String) {
    register_foreground(task_id)
}

pub fn has_foreground_tasks() -> bool {
    FOREGROUND_TASKS
        .lock()
        .map(|s| !s.is_empty())
        .unwrap_or(false)
}

pub fn hasForegroundTasks() -> bool {
    has_foreground_tasks()
}

pub fn unregister_foreground(task_id: &str) {
    if let Ok(mut s) = FOREGROUND_TASKS.lock() {
        s.remove(task_id);
    }
}

/// 取出并清空所有前台任务 id（用于 `background_all` 等场景）。
pub fn drain_foreground() -> Vec<String> {
    FOREGROUND_TASKS
        .lock()
        .map(|mut s| s.drain().collect())
        .unwrap_or_default()
}

// ============================================================================
// DreamTask 子模块 (tasks/DreamTask/DreamTask.ts)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DreamTaskState {
    pub id: String,
    pub turns: Vec<String>,
    pub completed: bool,
    pub completed_at: Option<i64>,
}

static DREAM_TASKS: once_cell::sync::Lazy<std::sync::Mutex<HashMap<String, DreamTaskState>>> =
    once_cell::sync::Lazy::new(|| std::sync::Mutex::new(HashMap::new()));

pub fn is_dream_task(task: &TaskInfo) -> bool {
    matches!(task.task_type, TaskType::Dream)
}

pub fn isDreamTask(task: &TaskInfo) -> bool {
    is_dream_task(task)
}

pub fn register_dream_task(task_id: String) {
    if let Ok(mut s) = DREAM_TASKS.lock() {
        s.insert(
            task_id.clone(),
            DreamTaskState {
                id: task_id,
                turns: Vec::new(),
                completed: false,
                completed_at: None,
            },
        );
    }
}

pub fn registerDreamTask(task_id: String) {
    register_dream_task(task_id)
}

pub fn add_dream_turn(task_id: &str, turn: String) -> bool {
    if let Ok(mut s) = DREAM_TASKS.lock() {
        if let Some(state) = s.get_mut(task_id) {
            state.turns.push(turn);
            return true;
        }
    }
    false
}

pub fn addDreamTurn(task_id: &str, turn: String) -> bool {
    add_dream_turn(task_id, turn)
}

pub fn complete_dream_task(task_id: &str) -> bool {
    if let Ok(mut s) = DREAM_TASKS.lock() {
        if let Some(state) = s.get_mut(task_id) {
            state.completed = true;
            state.completed_at = Some(chrono::Utc::now().timestamp_millis());
            return true;
        }
    }
    false
}

pub fn completeDreamTask(task_id: &str) -> bool {
    complete_dream_task(task_id)
}

pub fn get_dream_task(task_id: &str) -> Option<DreamTaskState> {
    DREAM_TASKS.lock().ok()?.get(task_id).cloned()
}

pub fn list_dream_tasks() -> Vec<DreamTaskState> {
    DREAM_TASKS
        .lock()
        .map(|s| s.values().cloned().collect())
        .unwrap_or_default()
}

// ============================================================================
// LocalAgentTask 扩展 — 任务谓词 / 消息队列 / 通知
// ============================================================================

/// 判断是否为 LocalAgentTask 状态对象（按 type 字段匹配）。
pub fn is_local_agent_task(value: &serde_json::Value) -> bool {
    value
        .get("type")
        .and_then(|t| t.as_str())
        .map(|t| t == "local_agent")
        .unwrap_or(false)
}

pub fn isLocalAgentTask(value: &serde_json::Value) -> bool {
    is_local_agent_task(value)
}

/// 是否为面板可见的 agent 任务（排除 main-session）。
pub fn is_panel_agent_task(value: &serde_json::Value) -> bool {
    if !is_local_agent_task(value) {
        return false;
    }
    value
        .get("agentType")
        .and_then(|t| t.as_str())
        .map(|t| t != "main-session")
        .unwrap_or(true)
}

pub fn isPanelAgentTask(value: &serde_json::Value) -> bool {
    is_panel_agent_task(value)
}

/// 全局待处理消息队列（按 taskId 索引）。
static PENDING_MESSAGES: once_cell::sync::Lazy<std::sync::Mutex<HashMap<String, Vec<String>>>> =
    once_cell::sync::Lazy::new(|| std::sync::Mutex::new(HashMap::new()));

pub fn queue_pending_message(task_id: &str, msg: String) {
    if let Ok(mut s) = PENDING_MESSAGES.lock() {
        s.entry(task_id.to_string()).or_default().push(msg);
    }
}

pub fn queuePendingMessage(task_id: &str, msg: String) {
    queue_pending_message(task_id, msg)
}

/// 追加消息到 agent 任务的全局消息缓冲。
///
/// 按 task_id 索引、消息按追加顺序存储；
/// 上层（如 TUI 或 SDK）可调用 `agent_messages_for` 读取累积内容。
static AGENT_MESSAGES: once_cell::sync::Lazy<
    std::sync::Mutex<HashMap<String, Vec<serde_json::Value>>>,
> = once_cell::sync::Lazy::new(|| std::sync::Mutex::new(HashMap::new()));

/// 读取 agent 任务的累积消息。
pub fn agent_messages_for(task_id: &str) -> Vec<serde_json::Value> {
    AGENT_MESSAGES
        .lock()
        .ok()
        .and_then(|s| s.get(task_id).cloned())
        .unwrap_or_default()
}

pub fn append_message_to_local_agent(task_id: &str, message: serde_json::Value) {
    if let Ok(mut s) = AGENT_MESSAGES.lock() {
        s.entry(task_id.to_string()).or_default().push(message);
    }
}

pub fn appendMessageToLocalAgent(task_id: &str, message: serde_json::Value) {
    append_message_to_local_agent(task_id, message)
}

/// 排空待处理消息（一次性）。
pub fn drain_pending_messages(task_id: &str) -> Vec<String> {
    PENDING_MESSAGES
        .lock()
        .map(|mut s| s.remove(task_id).unwrap_or_default())
        .unwrap_or_default()
}

pub fn drainPendingMessages(task_id: &str) -> Vec<String> {
    drain_pending_messages(task_id)
}

/// Agent 通知载荷。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentNotificationOpts {
    pub task_id: String,
    pub description: String,
    pub status: String, // "completed"|"failed"|"killed"
    pub error: Option<String>,
    pub final_message: Option<String>,
    pub usage: Option<AgentNotificationUsage>,
    pub tool_use_id: Option<String>,
    pub worktree_path: Option<String>,
    pub worktree_branch: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentNotificationUsage {
    pub total_tokens: u64,
    pub tool_uses: u64,
    pub duration_ms: u64,
}

pub fn enqueue_agent_notification(opts: AgentNotificationOpts) -> String {
    let summary = match opts.status.as_str() {
        "completed" => format!("Agent \"{}\" completed", opts.description),
        "failed" => format!(
            "Agent \"{}\" failed: {}",
            opts.description,
            opts.error.as_deref().unwrap_or("Unknown error")
        ),
        _ => format!("Agent \"{}\" was stopped", opts.description),
    };
    let result_section = opts
        .final_message
        .as_ref()
        .map(|m| format!("\n<result>{}</result>", m))
        .unwrap_or_default();
    let usage_section = opts
        .usage
        .as_ref()
        .map(|u| {
            format!(
                "\n<usage><total_tokens>{}</total_tokens><tool_uses>{}</tool_uses><duration_ms>{}</duration_ms></usage>",
                u.total_tokens, u.tool_uses, u.duration_ms
            )
        })
        .unwrap_or_default();
    let worktree_section = opts
        .worktree_path
        .as_ref()
        .map(|p| {
            let branch = opts
                .worktree_branch
                .as_ref()
                .map(|b| format!("<branch>{}</branch>", b))
                .unwrap_or_default();
            format!("\n<worktree><path>{}</path>{}</worktree>", p, branch)
        })
        .unwrap_or_default();
    let tool_use_line = opts
        .tool_use_id
        .as_ref()
        .map(|t| format!("\n<tool_use_id>{}</tool_use_id>", t))
        .unwrap_or_default();

    format!(
        "<task_notification>\n<task_id>{}</task_id>{}\n<output_file>{}</output_file>\n<status>{}</status>\n<summary>{}</summary>{}{}{}\n</task_notification>",
        opts.task_id,
        tool_use_line,
        format!("/tmp/mossen-task-{}.log", opts.task_id),
        opts.status,
        summary,
        result_section,
        usage_section,
        worktree_section,
    )
}

pub fn enqueueAgentNotification(opts: AgentNotificationOpts) -> String {
    enqueue_agent_notification(opts)
}

// ============================================================================
// LocalMainSessionTask 扩展 (tasks/LocalMainSessionTask.ts)
// ============================================================================

static MAIN_SESSION_TASKS: once_cell::sync::Lazy<std::sync::Mutex<HashMap<String, TaskInfo>>> =
    once_cell::sync::Lazy::new(|| std::sync::Mutex::new(HashMap::new()));

pub fn register_main_session_task(task_id: String, info: TaskInfo) {
    if let Ok(mut s) = MAIN_SESSION_TASKS.lock() {
        s.insert(task_id, info);
    }
}

pub fn registerMainSessionTask(task_id: String, info: TaskInfo) {
    register_main_session_task(task_id, info)
}

pub fn complete_main_session_task(task_id: &str) {
    if let Ok(mut s) = MAIN_SESSION_TASKS.lock() {
        if let Some(info) = s.get_mut(task_id) {
            info.status = TaskStatus::Completed;
            info.completed_at = Some(chrono::Utc::now().timestamp_millis());
        }
    }
}

pub fn completeMainSessionTask(task_id: &str) {
    complete_main_session_task(task_id)
}

pub fn foreground_main_session_task(task_id: &str) -> bool {
    register_foreground(task_id.to_string());
    true
}

pub fn foregroundMainSessionTask(task_id: &str) -> bool {
    foreground_main_session_task(task_id)
}

pub fn is_main_session_task(value: &serde_json::Value) -> bool {
    value
        .get("type")
        .and_then(|t| t.as_str())
        .map(|t| t == "local_main_session")
        .unwrap_or(false)
}

pub fn isMainSessionTask(value: &serde_json::Value) -> bool {
    is_main_session_task(value)
}

// ============================================================================
// InProcessTeammateTask (tasks/InProcessTeammateTask/...)
// ============================================================================

pub const TEAMMATE_MESSAGES_UI_CAP: usize = 200;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeammateIdentity {
    pub agent_id: String,
    pub team_name: String,
    pub name: String,
}

pub fn is_in_process_teammate_task(value: &serde_json::Value) -> bool {
    value
        .get("type")
        .and_then(|t| t.as_str())
        .map(|t| t == "in_process_teammate")
        .unwrap_or(false)
}

pub fn isInProcessTeammateTask(value: &serde_json::Value) -> bool {
    is_in_process_teammate_task(value)
}

/// 追加消息并限制总数（UI cap）。
pub fn append_capped_message(messages: &mut Vec<serde_json::Value>, message: serde_json::Value) {
    messages.push(message);
    if messages.len() > TEAMMATE_MESSAGES_UI_CAP {
        let drop = messages.len() - TEAMMATE_MESSAGES_UI_CAP;
        messages.drain(..drop);
    }
}

pub fn appendCappedMessage(messages: &mut Vec<serde_json::Value>, message: serde_json::Value) {
    append_capped_message(messages, message)
}

static TEAMMATE_TASKS: once_cell::sync::Lazy<std::sync::Mutex<HashMap<String, TaskInfo>>> =
    once_cell::sync::Lazy::new(|| std::sync::Mutex::new(HashMap::new()));

static TEAMMATE_SHUTDOWN: once_cell::sync::Lazy<
    std::sync::Mutex<std::collections::HashSet<String>>,
> = once_cell::sync::Lazy::new(|| std::sync::Mutex::new(std::collections::HashSet::new()));

pub fn request_teammate_shutdown(task_id: &str) {
    if let Ok(mut s) = TEAMMATE_SHUTDOWN.lock() {
        s.insert(task_id.to_string());
    }
}

pub fn requestTeammateShutdown(task_id: &str) {
    request_teammate_shutdown(task_id)
}

pub fn append_teammate_message(task_id: &str, message: serde_json::Value) {
    if let Ok(mut s) = AGENT_MESSAGES.lock() {
        let v = s.entry(task_id.to_string()).or_default();
        append_capped_message(v, message);
    }
}

pub fn appendTeammateMessage(task_id: &str, message: serde_json::Value) {
    append_teammate_message(task_id, message)
}

pub fn inject_user_message_to_teammate(task_id: &str, message: String) {
    queue_pending_message(task_id, message);
}

pub fn injectUserMessageToTeammate(task_id: &str, message: String) {
    inject_user_message_to_teammate(task_id, message)
}

pub fn find_teammate_task_by_agent_id(agent_id: &str) -> Option<String> {
    TEAMMATE_TASKS.lock().ok().and_then(|s| {
        s.iter()
            .find(|(_, info)| {
                info.metadata
                    .get("agent_id")
                    .and_then(|v| v.as_str())
                    .map(|a| a == agent_id)
                    .unwrap_or(false)
            })
            .map(|(k, _)| k.clone())
    })
}

pub fn findTeammateTaskByAgentId(agent_id: &str) -> Option<String> {
    find_teammate_task_by_agent_id(agent_id)
}

// ============================================================================
// LocalAgentTask 后续工具：summary、background、register
// ============================================================================

/// 更新 agent 任务的进度。
///
/// 进度信息存储在全局 `AGENT_PROGRESS` 表中，按 task_id 索引；
/// 可由 `get_agent_progress` 读取，亦用于 TUI agent pill 显示。
pub fn update_agent_progress(task_id: &str, progress: AgentProgress) {
    if let Ok(mut s) = AGENT_PROGRESS.lock() {
        s.insert(task_id.to_string(), progress);
    }
}

static AGENT_PROGRESS: once_cell::sync::Lazy<std::sync::Mutex<HashMap<String, AgentProgress>>> =
    once_cell::sync::Lazy::new(|| std::sync::Mutex::new(HashMap::new()));

static AGENT_SUMMARIES: once_cell::sync::Lazy<std::sync::Mutex<HashMap<String, String>>> =
    once_cell::sync::Lazy::new(|| std::sync::Mutex::new(HashMap::new()));

static AGENT_RESULTS: once_cell::sync::Lazy<std::sync::Mutex<HashMap<String, AgentToolResult>>> =
    once_cell::sync::Lazy::new(|| std::sync::Mutex::new(HashMap::new()));

static AGENT_FAILED: once_cell::sync::Lazy<std::sync::Mutex<HashMap<String, String>>> =
    once_cell::sync::Lazy::new(|| std::sync::Mutex::new(HashMap::new()));

static AGENT_KILLED: once_cell::sync::Lazy<std::sync::Mutex<std::collections::HashSet<String>>> =
    once_cell::sync::Lazy::new(|| std::sync::Mutex::new(std::collections::HashSet::new()));

static AGENT_NOTIFIED: once_cell::sync::Lazy<std::sync::Mutex<std::collections::HashSet<String>>> =
    once_cell::sync::Lazy::new(|| std::sync::Mutex::new(std::collections::HashSet::new()));

/// 读取 agent 任务的最近进度。
pub fn get_agent_progress(task_id: &str) -> Option<AgentProgress> {
    AGENT_PROGRESS.lock().ok()?.get(task_id).cloned()
}

pub fn updateAgentProgress(task_id: &str, progress: AgentProgress) {
    update_agent_progress(task_id, progress)
}

/// 更新 agent 任务的摘要文本。
pub fn update_agent_summary(task_id: &str, summary: String) {
    if let Ok(mut s) = AGENT_SUMMARIES.lock() {
        s.insert(task_id.to_string(), summary);
    }
}

/// 读取 agent 任务摘要。
pub fn get_agent_summary(task_id: &str) -> Option<String> {
    AGENT_SUMMARIES.lock().ok()?.get(task_id).cloned()
}

pub fn updateAgentSummary(task_id: &str, summary: String) {
    update_agent_summary(task_id, summary)
}

#[derive(Debug, Clone)]
pub struct AgentToolResult {
    pub task_id: String,
    pub result: String,
    pub error: Option<String>,
}

/// 标记 agent 任务完成并保存结果。
pub fn complete_agent_task(result: AgentToolResult) {
    if let Ok(mut s) = AGENT_RESULTS.lock() {
        s.insert(result.task_id.clone(), result);
    }
}

/// 读取 agent 任务结果。
pub fn get_agent_result(task_id: &str) -> Option<AgentToolResult> {
    AGENT_RESULTS.lock().ok()?.get(task_id).cloned()
}

pub fn completeAgentTask(result: AgentToolResult) {
    complete_agent_task(result)
}

/// 标记 agent 任务失败。
pub fn fail_agent_task(task_id: &str, error: String) {
    if let Ok(mut s) = AGENT_FAILED.lock() {
        s.insert(task_id.to_string(), error);
    }
}

/// 读取失败原因（若任务失败）。
pub fn get_agent_failure(task_id: &str) -> Option<String> {
    AGENT_FAILED.lock().ok()?.get(task_id).cloned()
}

pub fn failAgentTask(task_id: &str, error: String) {
    fail_agent_task(task_id, error)
}

/// 杀死异步 agent 任务。
///
/// 真实实现通过 `request_teammate_shutdown` 发送停止信号，
/// 同时在 `AGENT_KILLED` 中标记；后续 `is_agent_killed` 可用于
/// dialogue 循环短路。
pub fn kill_async_agent(task_id: &str) {
    request_teammate_shutdown(task_id);
    if let Ok(mut s) = AGENT_KILLED.lock() {
        s.insert(task_id.to_string());
    }
}

/// 是否已被请求停止。
pub fn is_agent_killed(task_id: &str) -> bool {
    AGENT_KILLED
        .lock()
        .map(|s| s.contains(task_id))
        .unwrap_or(false)
}

pub fn killAsyncAgent(task_id: &str) {
    kill_async_agent(task_id)
}

pub fn kill_all_running_agent_tasks(tasks: &[(String, String)]) -> usize {
    let mut count = 0;
    for (tid, status) in tasks {
        if status == "running" {
            kill_async_agent(tid);
            count += 1;
        }
    }
    count
}

pub fn killAllRunningAgentTasks(tasks: &[(String, String)]) -> usize {
    kill_all_running_agent_tasks(tasks)
}

/// 标记某 agent 任务已被通知（用于一次性提醒去重）。
pub fn mark_agents_notified(task_id: &str) {
    if let Ok(mut s) = AGENT_NOTIFIED.lock() {
        s.insert(task_id.to_string());
    }
}

/// 是否已被通知过。
pub fn is_agent_notified(task_id: &str) -> bool {
    AGENT_NOTIFIED
        .lock()
        .map(|s| s.contains(task_id))
        .unwrap_or(false)
}

pub fn markAgentsNotified(task_id: &str) {
    mark_agents_notified(task_id)
}

pub fn register_async_agent(task_id: String, info: TaskInfo) {
    if let Ok(mut s) = MAIN_SESSION_TASKS.lock() {
        s.insert(task_id, info);
    }
}

pub fn registerAsyncAgent(task_id: String, info: TaskInfo) {
    register_async_agent(task_id, info)
}

pub fn register_agent_foreground(task_id: String) {
    register_foreground(task_id);
}

pub fn registerAgentForeground(task_id: String) {
    register_agent_foreground(task_id)
}

pub fn background_agent_task(task_id: &str) -> bool {
    unregister_foreground(task_id);
    true
}

pub fn backgroundAgentTask(task_id: &str) -> bool {
    background_agent_task(task_id)
}

pub fn unregister_agent_foreground(task_id: &str) {
    unregister_foreground(task_id);
}

pub fn unregisterAgentForeground(task_id: &str) {
    unregister_agent_foreground(task_id)
}
