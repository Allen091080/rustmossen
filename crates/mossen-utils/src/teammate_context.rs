//! # teammate_context — 进程内 Teammate 运行时上下文
//!
//! 对应 TypeScript `utils/teammateContext.ts`。
//! 使用 tokio task_local 提供进程内 teammate 的 AsyncLocalStorage 等效上下文，
//! 支持并发 teammate 执行而无全局状态冲突。
//!
//! ## 与其他 teammate 身份机制的关系
//!
//! - 环境变量 (MOSSEN_CODE_AGENT_ID): 基于进程的 teammates，通过 tmux 生成
//! - dynamicTeamContext (teammate.ts): 运行时加入的基于进程的 teammates
//! - TeammateContext (本文件): 通过 task_local 的进程内 teammates

use tokio::task_local;
use tokio_util::sync::CancellationToken;

/// 进程内 teammate 的运行时上下文。
/// 存储在 task_local 中用于并发访问。
#[derive(Debug, Clone)]
pub struct TeammateContext {
    /// 完整代理 ID，例如 "researcher@my-team"
    pub agent_id: String,
    /// 显示名称，例如 "researcher"
    pub agent_name: String,
    /// 此 teammate 所属的团队名
    pub team_name: String,
    /// 分配给此 teammate 的 UI 颜色
    pub color: Option<String>,
    /// teammate 是否必须在实施前进入计划模式
    pub plan_mode_required: bool,
    /// 领导者的会话 ID（用于 transcript 关联）
    pub parent_session_id: String,
    /// 取消令牌（用于生命周期管理，链接到父级）
    pub cancel_token: CancellationToken,
}

task_local! {
    static TEAMMATE_CONTEXT: TeammateContext;
}

/// 获取当前进程内 teammate 上下文（如果作为 teammate 运行）。
///
/// 如果不在进程内 teammate 上下文中运行，返回 None。
pub fn get_teammate_context() -> Option<TeammateContext> {
    TEAMMATE_CONTEXT.try_with(|ctx| ctx.clone()).ok()
}

/// 在设置了 teammate 上下文的情况下运行异步函数。
///
/// 用于在生成进程内 teammate 时建立其执行上下文。
///
/// # 参数
/// - `context`: 要设置的 teammate 上下文
/// - `f`: 在上下文中运行的 future
///
/// # 返回
/// f 的返回值
pub async fn run_with_teammate_context<F, T>(context: TeammateContext, f: F) -> T
where
    F: std::future::Future<Output = T>,
{
    TEAMMATE_CONTEXT.scope(context, f).await
}

/// 检查当前执行是否在进程内 teammate 中。
///
/// 比 `get_teammate_context().is_some()` 更快，用于简单检查。
pub fn is_in_process_teammate() -> bool {
    TEAMMATE_CONTEXT.try_with(|_| ()).is_ok()
}

/// 从生成配置创建 TeammateContext。
///
/// cancel_token 由调用者传入。对于进程内 teammates，
/// 这通常是一个独立的控制器（未链接到父级），
/// 这样当领导者的查询被中断时 teammates 继续运行。
///
/// # 参数
/// - `config`: teammate 上下文配置
///
/// # 返回
/// 完整的 TeammateContext
pub struct TeammateContextConfig {
    pub agent_id: String,
    pub agent_name: String,
    pub team_name: String,
    pub color: Option<String>,
    pub plan_mode_required: bool,
    pub parent_session_id: String,
    pub cancel_token: CancellationToken,
}

pub fn create_teammate_context(config: TeammateContextConfig) -> TeammateContext {
    TeammateContext {
        agent_id: config.agent_id,
        agent_name: config.agent_name,
        team_name: config.team_name,
        color: config.color,
        plan_mode_required: config.plan_mode_required,
        parent_session_id: config.parent_session_id,
        cancel_token: config.cancel_token,
    }
}
