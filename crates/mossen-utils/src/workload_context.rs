//! # workload_context — 轮次级工作负载标签（通过 task_local）
//!
//! 对应 TypeScript `utils/workloadContext.ts`。
//! 使用 tokio::task_local 替代 Node.js 的 AsyncLocalStorage。

use std::future::Future;

tokio::task_local! {
    static WORKLOAD: Option<String>;
}

/// 工作负载类型。服务端净化器仅接受小写 [a-z0-9_-]{0,32}。
pub const WORKLOAD_CRON: &str = "cron";

/// 获取当前工作负载标签。
pub fn get_workload() -> Option<String> {
    WORKLOAD.try_with(|w| w.clone()).unwrap_or(None)
}

/// 在工作负载 task_local 上下文中包装 `fut`。
/// 始终建立新的上下文边界，即使 `workload` 为 None。
///
/// 前一实现在 `None` 时短路返回 fn()——但那是传递而非边界。
/// 如果调用者已经在泄漏的 cron 上下文中，传递会让 get_workload()
/// 在 fn 内返回泄漏的标签。始终调用 scope 保证 get_workload()
/// 在 fn 内返回调用者传递的确切值——包括 None。
pub async fn run_with_workload<F, T>(workload: Option<String>, fut: F) -> T
where
    F: Future<Output = T>,
{
    WORKLOAD.scope(workload, fut).await
}

/// 同步版本——用于不需要 async 的上下文。
pub fn run_with_workload_sync<F, T>(workload: Option<String>, f: F) -> T
where
    F: FnOnce() -> T,
{
    // task_local 的同步 scope 不可用，直接执行并设置变量
    // 在纯同步上下文中，使用 thread_local 作为后备
    WORKLOAD.sync_scope(workload, f)
}
