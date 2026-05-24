//! # spawn_in_process — 进程内队友生成
//!
//! 对应 TypeScript `utils/swarm/spawnInProcess.ts`。
//! 创建并注册进程内队友任务。

use serde::{Deserialize, Serialize};

use crate::abort_controller::AbortController;
use crate::teammate::get_parent_session_id;

/// 进程内队友生成配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InProcessSpawnConfig {
    /// 队友的显示名称，例如 "researcher"
    pub name: String,
    /// 队友所属的团队
    pub team_name: String,
    /// 队友的初始提示/任务
    pub prompt: String,
    /// 队友的可选 UI 颜色
    pub color: Option<String>,
    /// 队友是否必须在实现前进入计划模式
    pub plan_mode_required: bool,
    /// 此队友的可选模型覆盖
    pub model: Option<String>,
}

/// 进程内队友生成输出
#[derive(Clone)]
pub struct InProcessSpawnOutput {
    /// 生成是否成功
    pub success: bool,
    /// 完整的代理 ID（格式："name@team"）
    pub agent_id: String,
    /// 用于在 AppState 中跟踪的任务 ID
    pub task_id: Option<String>,
    /// 此队友的 AbortController（链接到父级）。Rust 端使用
    /// [`crate::abort_controller::AbortController`]，相比 TS 的
    /// DOM `AbortController` 更轻量，但语义一致：单次 abort 触发广播。
    pub abort_controller: Option<AbortController>,
    /// 队友上下文用于 AsyncLocalStorage
    pub teammate_context: Option<TeammateContext>,
    /// 如果生成失败则包含错误消息
    pub error: Option<String>,
}

/// 队友上下文（简化版本）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeammateContext {
    pub agent_id: String,
    pub agent_name: String,
    pub team_name: String,
    pub color: Option<String>,
    pub plan_mode_required: bool,
    pub parent_session_id: String,
}

/// 队友身份
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeammateIdentity {
    pub agent_id: String,
    pub agent_name: String,
    pub team_name: String,
    pub color: Option<String>,
    pub plan_mode_required: bool,
    pub parent_session_id: String,
}

/// 全局表：task_id -> AbortController，供 [`kill_in_process_teammate`] 查找。
static SPAWNED_TASKS: once_cell::sync::Lazy<
    parking_lot::Mutex<std::collections::HashMap<String, AbortController>>,
> = once_cell::sync::Lazy::new(|| parking_lot::Mutex::new(std::collections::HashMap::new()));

/// 生成确定性代理 ID
pub fn format_agent_id(name: &str, team_name: &str) -> String {
    format!("{}@{}", name, team_name)
}

/// 生成进程内队友。
///
/// 创建队友的上下文，在进程内 task table 中注册取消信号，并返回生成结果。
///
/// 与 TS 端 `spawnInProcessTeammate` 的差异：
/// - Rust 端没有 AppState 通道；调用方拿到 [`InProcessSpawnOutput::task_id`]
///   后可用 [`kill_in_process_teammate`] 取消。
/// - 真正的推理 loop（TS 端 `runInProcessTeammate`）尚未在 Rust 端实现；
///   见 [`crate::in_process_runner`]。本函数只负责注册 / 返回 handle。
pub async fn spawn_in_process_teammate(config: InProcessSpawnConfig) -> InProcessSpawnOutput {
    let agent_id = format_agent_id(&config.name, &config.team_name);
    let task_id = format!("in_process_{}", uuid::Uuid::new_v4());

    let abort = AbortController::new();
    SPAWNED_TASKS.lock().insert(task_id.clone(), abort.clone());

    // 从当前线程的 teammate context 或全局 dynamic context 推导父会话 ID。
    let parent_session_id = get_parent_session_id().unwrap_or_default();

    InProcessSpawnOutput {
        success: true,
        agent_id: agent_id.clone(),
        task_id: Some(task_id),
        abort_controller: Some(abort),
        teammate_context: Some(TeammateContext {
            agent_id,
            agent_name: config.name,
            team_name: config.team_name,
            color: config.color,
            plan_mode_required: config.plan_mode_required,
            parent_session_id,
        }),
        error: None,
    }
}

/// 通过中止其控制器来终止进程内队友。
///
/// * `task_id` - 要终止的队友的任务 ID。
/// 返回是否成功终止 —— `false` 表示找不到 task_id。
pub fn kill_in_process_teammate(task_id: &str) -> bool {
    let mut map = SPAWNED_TASKS.lock();
    if let Some(abort) = map.remove(task_id) {
        abort.abort();
        true
    } else {
        false
    }
}

/// 对应 TS `SpawnContext`：spawn 调用时的运行上下文。
#[derive(Debug, Clone, Default)]
pub struct SpawnContext {
    pub agent_id: String,
    pub team_name: String,
    pub abort: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_agent_id() {
        assert_eq!(
            format_agent_id("researcher", "my-team"),
            "researcher@my-team"
        );
    }

    #[tokio::test]
    async fn test_spawn_in_process_teammate() {
        let config = InProcessSpawnConfig {
            name: "tester".to_string(),
            team_name: "test-team".to_string(),
            prompt: "Run tests".to_string(),
            color: Some("blue".to_string()),
            plan_mode_required: false,
            model: None,
        };
        let result = spawn_in_process_teammate(config).await;
        assert!(result.success);
        assert_eq!(result.agent_id, "tester@test-team");
        let task_id = result.task_id.unwrap();
        assert!(kill_in_process_teammate(&task_id));
        // 第二次取消同一个 ID 应返回 false。
        assert!(!kill_in_process_teammate(&task_id));
    }
}
