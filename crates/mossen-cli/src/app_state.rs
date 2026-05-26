//! 应用状态管理 — 对应 TS 的 state/ 目录。
//!
//! 全局应用状态存储、变更通知和 teammate 视图辅助。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{watch, RwLock};
use tracing::info;

// ─── AppState (state/AppState.tsx + state/AppStateStore.ts) ────────────────

/// 工具权限上下文。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolPermissionContext {
    /// 权限模式。
    pub mode: String,
    /// 规则列表。
    pub rules: Vec<PermissionRule>,
}

/// 权限规则。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRule {
    pub tool_name: String,
    pub behavior: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
}

/// 完成边界类型。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum CompletionBoundary {
    #[serde(rename = "complete")]
    Complete {
        completed_at: i64,
        output_tokens: u64,
    },
    #[serde(rename = "bash")]
    Bash { command: String, completed_at: i64 },
    #[serde(rename = "edit")]
    Edit {
        tool_name: String,
        file_path: String,
        completed_at: i64,
    },
    #[serde(rename = "denied_tool")]
    DeniedTool {
        tool_name: String,
        detail: String,
        completed_at: i64,
    },
}

/// 投机执行状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
#[derive(Default)]
pub enum SpeculationState {
    #[serde(rename = "idle")]
    #[default]
    Idle,
    #[serde(rename = "active")]
    Active {
        id: String,
        start_time: i64,
        suggestion_length: usize,
        tool_use_count: usize,
        is_pipelined: bool,
    },
}

/// 底栏项。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FooterItem {
    pub label: String,
    pub key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
}

/// 任务状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskState {
    pub task_id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<f64>,
}

/// 努力程度值。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EffortValue {
    Low,
    Medium,
    High,
}

/// 全局应用状态 — 对应 TS 的 AppState。
///
/// 包含运行时所有可观察的全局状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppState {
    /// 消息列表。
    pub messages: Vec<serde_json::Value>,
    /// 工具权限上下文。
    pub tool_permission_context: ToolPermissionContext,
    /// 当前是否正在加载。
    pub is_loading: bool,
    /// 是否处于主循环。
    pub is_main_turn_active: bool,
    /// 当前 Agent ID。
    pub agent_id: Option<String>,
    /// 当前模型设置。
    pub model: Option<String>,
    /// 思考模式是否启用。
    pub thinking_enabled: bool,
    /// 努力程度。
    pub effort: EffortValue,
    /// 当前输入值。
    pub input_value: String,
    /// 任务状态列表。
    pub tasks: Vec<TaskState>,
    /// 通知列表。
    pub notifications: Vec<serde_json::Value>,
    /// 投机执行状态。
    pub speculation: SpeculationState,
    /// 是否启用提示建议。
    pub prompt_suggestions_enabled: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            messages: Vec::new(),
            tool_permission_context: ToolPermissionContext::default(),
            is_loading: false,
            is_main_turn_active: false,
            agent_id: None,
            model: None,
            thinking_enabled: true,
            effort: EffortValue::High,
            input_value: String::new(),
            tasks: Vec::new(),
            notifications: Vec::new(),
            speculation: SpeculationState::default(),
            prompt_suggestions_enabled: true,
        }
    }
}

// ─── AppStateStore (state/AppStateStore.ts) ─────────────────────────────────

/// 应用状态存储 — 使用 watch channel 实现响应式更新。
pub struct AppStateStore {
    /// 当前状态（可读写）。
    state: Arc<RwLock<AppState>>,
    /// 状态变更发送端。
    tx: watch::Sender<AppState>,
    /// 状态变更接收端（可克隆给多个订阅者）。
    rx: watch::Receiver<AppState>,
}

impl AppStateStore {
    /// 创建新的状态存储。
    pub fn new() -> Self {
        let initial = AppState::default();
        let (tx, rx) = watch::channel(initial.clone());
        Self {
            state: Arc::new(RwLock::new(initial)),
            tx,
            rx,
        }
    }

    /// 获取当前状态的快照。
    pub async fn get_state(&self) -> AppState {
        self.state.read().await.clone()
    }

    /// 更新状态（使用闭包）。
    pub async fn set_state<F>(&self, updater: F)
    where
        F: FnOnce(&mut AppState),
    {
        let mut state = self.state.write().await;
        updater(&mut state);
        let _ = self.tx.send(state.clone());
    }

    /// 替换整个状态。
    pub async fn replace_state(&self, new_state: AppState) {
        let mut state = self.state.write().await;
        *state = new_state.clone();
        let _ = self.tx.send(new_state);
    }

    /// 获取订阅接收端。
    pub fn subscribe(&self) -> watch::Receiver<AppState> {
        self.rx.clone()
    }

    /// 更新权限模式。
    pub async fn set_permission_mode(&self, mode: &str) {
        self.set_state(|s| {
            s.tool_permission_context.mode = mode.to_string();
        })
        .await;
    }

    /// 更新加载状态。
    pub async fn set_loading(&self, loading: bool) {
        self.set_state(|s| {
            s.is_loading = loading;
        })
        .await;
    }

    /// 添加消息。
    pub async fn push_message(&self, message: serde_json::Value) {
        self.set_state(|s| {
            s.messages.push(message);
        })
        .await;
    }

    /// 清空消息。
    pub async fn clear_messages(&self) {
        self.set_state(|s| {
            s.messages.clear();
        })
        .await;
    }

    /// 设置模型。
    pub async fn set_model(&self, model: Option<String>) {
        self.set_state(|s| {
            s.model = model;
        })
        .await;
    }

    /// 设置努力程度。
    pub async fn set_effort(&self, effort: EffortValue) {
        self.set_state(|s| {
            s.effort = effort;
        })
        .await;
    }
}

impl Default for AppStateStore {
    fn default() -> Self {
        Self::new()
    }
}

// ─── onChangeAppState (state/onChangeAppState.ts) ───────────────────────────

/// 从外部元数据恢复应用状态。
///
/// 对应 TS 的 externalMetadataToAppState()。
pub fn external_metadata_to_app_state(
    metadata: &serde_json::Value,
    current: &AppState,
) -> AppState {
    let mut state = current.clone();

    if let Some(mode) = metadata["permission_mode"].as_str() {
        state.tool_permission_context.mode = mode.to_string();
    }

    if let Some(model) = metadata["model"].as_str() {
        state.model = Some(model.to_string());
    }

    if let Some(thinking) = metadata["thinking_enabled"].as_bool() {
        state.thinking_enabled = thinking;
    }

    state
}

// ─── teammateViewHelpers (state/teammateViewHelpers.ts) ─────────────────────

/// Teammate 视图项。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeammateViewItem {
    pub id: String,
    pub name: String,
    pub status: String,
    pub current_task: Option<String>,
    pub model: Option<String>,
    pub color: Option<String>,
}

/// 获取 teammate 视图列表。
pub fn get_teammate_view_items(state: &AppState) -> Vec<TeammateViewItem> {
    // 从任务状态推断 teammate 信息
    state
        .tasks
        .iter()
        .filter(|t| t.status == "running" || t.status == "pending")
        .map(|t| TeammateViewItem {
            id: t.task_id.clone(),
            name: t.label.clone().unwrap_or_else(|| t.task_id.clone()),
            status: t.status.clone(),
            current_task: t.label.clone(),
            model: None,
            color: None,
        })
        .collect()
}

/// 获取活跃 teammate 数量。
pub fn get_active_teammate_count(state: &AppState) -> usize {
    state.tasks.iter().filter(|t| t.status == "running").count()
}

// ────────────────────────────────────────────────────────────────────────────
// state/AppState.tsx — React 风格 hooks 的 Rust 等价物
// ────────────────────────────────────────────────────────────────────────────

use once_cell::sync::OnceCell;

/// 全局 AppState store 单例（在 Rust 中替代 React Context）。
static GLOBAL_APP_STORE: OnceCell<Arc<AppStateStore>> = OnceCell::new();

/// AppStateProvider — 在 Rust 中等价于一次性初始化全局 store。
pub fn app_state_provider(initial: AppState) -> Arc<AppStateStore> {
    let store = Arc::new(AppStateStore::new());
    // 设置初始状态（spawn 一个后台任务来 replace）
    let s = store.clone();
    tokio::spawn(async move {
        s.replace_state(initial).await;
    });
    let _ = GLOBAL_APP_STORE.set(store.clone());
    store
}

pub fn AppStateProvider(initial: AppState) -> Arc<AppStateStore> {
    app_state_provider(initial)
}

/// `useAppState(selector)` — 异步从当前 state 上读取后应用 selector。
pub async fn use_app_state<R>(selector: impl FnOnce(&AppState) -> R) -> Option<R> {
    let store = GLOBAL_APP_STORE.get()?;
    let snapshot = store.get_state().await;
    Some(selector(&snapshot))
}

pub async fn useAppState<R>(selector: impl FnOnce(&AppState) -> R) -> Option<R> {
    use_app_state(selector).await
}

/// `useSetAppState()` — 返回一个 setter 闭包。
pub fn use_set_app_state() -> Option<impl Fn(Box<dyn FnOnce(&mut AppState) + Send>) + Clone> {
    let store = GLOBAL_APP_STORE.get()?.clone();
    Some(move |update: Box<dyn FnOnce(&mut AppState) + Send>| {
        let store = store.clone();
        tokio::spawn(async move {
            store.set_state(|s| update(s)).await;
        });
    })
}

pub fn useSetAppState() -> Option<impl Fn(Box<dyn FnOnce(&mut AppState) + Send>) + Clone> {
    use_set_app_state()
}

/// `useAppStateStore()` — 直接返回 store 句柄。
pub fn use_app_state_store() -> Option<Arc<AppStateStore>> {
    GLOBAL_APP_STORE.get().cloned()
}

pub fn useAppStateStore() -> Option<Arc<AppStateStore>> {
    use_app_state_store()
}

/// `useAppStateMaybeOutsideOfProvider(selector)` — 即使无 provider 也不 panic。
pub async fn use_app_state_maybe_outside_of_provider<R>(
    selector: impl FnOnce(&AppState) -> R,
) -> Option<R> {
    use_app_state(selector).await
}

pub async fn useAppStateMaybeOutsideOfProvider<R>(
    selector: impl FnOnce(&AppState) -> R,
) -> Option<R> {
    use_app_state_maybe_outside_of_provider(selector).await
}

/// `AppStoreContext` — 与 TS React Context 同名的字符串常量。
///
/// TS 端使用 `React.createContext<AppStore>(...)` 创建上下文；
/// Rust 没有 React Context，AppStore 通过 `app_state::APP_STATE_STORE`
/// 这个全局 `OnceCell<AppStateStore>` 提供同等可达性。
/// 该常量仅作为 SDK schema 中的稳定 namespace tag，保持文本对齐。
pub const AppStoreContext: &str = "AppStoreContext";
