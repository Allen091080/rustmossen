//! # async_registry — 异步 Hook 跟踪注册表
//!
//! 对应 TS `utils/hooks/AsyncHookRegistry.ts`。
//! 跟踪后台异步执行的 Hook（如 shell 命令），
//! 轮询完成状态并收集结果。

use std::collections::HashMap;
use std::time::Instant;

use mossen_types::hooks::HookEvent;
use parking_lot::Mutex;
use tracing::{debug, warn};

/// 待处理的异步 Hook。
///
/// 对应 TS `PendingAsyncHook`。
#[derive(Debug, Clone)]
pub struct PendingAsyncHook {
    /// 进程 ID。
    pub process_id: String,
    /// Hook ID。
    pub hook_id: String,
    /// Hook 名称。
    pub hook_name: String,
    /// Hook 事件。
    pub hook_event: HookEvent,
    /// 工具名称（可选）。
    pub tool_name: Option<String>,
    /// 插件 ID（可选）。
    pub plugin_id: Option<String>,
    /// 开始时间。
    pub start_time: Instant,
    /// 超时时间（毫秒）。
    pub timeout_ms: u64,
    /// 命令文本。
    pub command: String,
    /// 响应附件是否已发送。
    pub response_attachment_sent: bool,
}

/// 异步 Hook 响应。
#[derive(Debug, Clone)]
pub struct AsyncHookResponse {
    /// 进程 ID。
    pub process_id: String,
    /// 同步响应内容（JSON）。
    pub response: serde_json::Value,
    /// Hook 名称。
    pub hook_name: String,
    /// Hook 事件。
    pub hook_event: HookEvent,
    /// 工具名称。
    pub tool_name: Option<String>,
    /// 插件 ID。
    pub plugin_id: Option<String>,
    /// 标准输出。
    pub stdout: String,
    /// 标准错误。
    pub stderr: String,
    /// 退出码。
    pub exit_code: Option<i32>,
}

/// 异步 Hook 注册表 — 全局跟踪正在执行的异步 Hook。
///
/// 对应 TS `pendingHooks` Map 全局状态。
pub struct AsyncHookRegistry {
    /// 进程 ID → 待处理 Hook 的映射。
    pending: Mutex<HashMap<String, PendingAsyncHook>>,
}

impl AsyncHookRegistry {
    /// 创建新的注册表。
    pub fn new() -> Self {
        Self {
            pending: Mutex::new(HashMap::new()),
        }
    }

    /// 注册一个待处理的异步 Hook。
    ///
    /// 对应 TS `registerPendingAsyncHook()`。
    pub fn register(&self, hook: PendingAsyncHook) {
        debug!(
            process_id = %hook.process_id,
            hook_name = %hook.hook_name,
            timeout_ms = hook.timeout_ms,
            "Registering async hook"
        );
        self.pending.lock().insert(hook.process_id.clone(), hook);
    }

    /// 获取所有未发送响应的待处理 Hook。
    ///
    /// 对应 TS `getPendingAsyncHooks()`。
    pub fn get_pending(&self) -> Vec<PendingAsyncHook> {
        self.pending
            .lock()
            .values()
            .filter(|h| !h.response_attachment_sent)
            .cloned()
            .collect()
    }

    /// 标记 Hook 的响应已发送。
    pub fn mark_response_sent(&self, process_id: &str) {
        if let Some(hook) = self.pending.lock().get_mut(process_id) {
            hook.response_attachment_sent = true;
        }
    }

    /// 移除已交付的异步 Hook。
    ///
    /// 对应 TS `removeDeliveredAsyncHooks()`。
    pub fn remove_delivered(&self, process_ids: &[String]) {
        let mut pending = self.pending.lock();
        for id in process_ids {
            if let Some(hook) = pending.get(id) {
                if hook.response_attachment_sent {
                    debug!(process_id = %id, "Removing delivered async hook");
                    pending.remove(id);
                }
            }
        }
    }

    /// 终结所有待处理的异步 Hook。
    ///
    /// 对应 TS `finalizePendingAsyncHooks()`。
    pub fn finalize_all(&self) {
        let mut pending = self.pending.lock();
        let count = pending.len();
        if count > 0 {
            debug!(count = count, "Finalizing all pending async hooks");
        }
        pending.clear();
    }

    /// 清空所有异步 Hook（用于测试）。
    ///
    /// 对应 TS `clearAllAsyncHooks()`。
    pub fn clear_all(&self) {
        self.pending.lock().clear();
    }

    /// 获取待处理 Hook 数量。
    pub fn pending_count(&self) -> usize {
        self.pending.lock().len()
    }

    /// 检查是否有超时的 Hook。
    pub fn check_timeouts(&self) -> Vec<String> {
        let pending = self.pending.lock();
        let now = Instant::now();
        pending
            .iter()
            .filter(|(_, hook)| {
                now.duration_since(hook.start_time).as_millis() as u64 > hook.timeout_ms
            })
            .map(|(id, hook)| {
                warn!(
                    process_id = %id,
                    hook_name = %hook.hook_name,
                    "Async hook timed out"
                );
                id.clone()
            })
            .collect()
    }
}

impl Default for AsyncHookRegistry {
    fn default() -> Self {
        Self::new()
    }
}
