//! # stop_hooks — 停止钩子机制
//!
//! 对应 TS `query/stopHooks.ts`，负责会话结束时的清理逻辑。
//! 对 sdk/custom-backend 会话绕过 executeStopHooks。

use std::time::Duration;

use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use crate::types::OriginTag;

// ---------------------------------------------------------------------------
// Stop Hook
// ---------------------------------------------------------------------------

/// Stop hook 执行结果。
#[derive(Debug)]
pub enum StopHookResult {
    /// 允许停止。
    Allow,
    /// 阻塞停止（需要继续循环）。
    Block { reason: String },
    /// 阻止停止（终止会话）。
    Prevent { reason: String },
}

/// Stop hook trait。
///
/// 实现者提供在会话结束前运行的清理逻辑。
#[async_trait::async_trait]
pub trait StopHook: Send + Sync {
    /// 钩子名称。
    fn name(&self) -> &str;

    /// 执行钩子逻辑。
    async fn execute(&self, ctx: &StopHookContext) -> StopHookResult;
}

/// Stop hook 执行上下文。
#[derive(Debug, Clone)]
pub struct StopHookContext {
    /// 会话 ID。
    pub session_id: String,
    /// 来源标签。
    pub origin_tag: OriginTag,
    /// 是否为自动模式。
    pub auto_mode: bool,
    /// 轮次计数。
    pub turn_count: u32,
}

// ---------------------------------------------------------------------------
// Stop Hook 管理器
// ---------------------------------------------------------------------------

/// Stop hook 管理器。
pub struct StopHookManager {
    hooks: Vec<Box<dyn StopHook>>,
}

impl StopHookManager {
    pub fn new() -> Self {
        Self { hooks: Vec::new() }
    }

    /// 注册一个 stop hook。
    pub fn register(&mut self, hook: Box<dyn StopHook>) {
        self.hooks.push(hook);
    }

    /// 评估所有 stop hooks。
    ///
    /// 对应 TS `executeStopHooks()`。
    /// 对 SDK/custom-backend 来源，绕过 stop hooks。
    pub async fn evaluate_halt_signals(
        &self,
        ctx: &StopHookContext,
        cancel: &CancellationToken,
    ) -> StopHookResult {
        // 对 SDK 和自定义后端来源，绕过 stop hooks
        // 对应 TS 中修复的挂起问题
        if matches!(ctx.origin_tag, OriginTag::Sdk | OriginTag::CustomBackend) {
            debug!(
                origin = ?ctx.origin_tag,
                "Bypassing stop hooks for SDK/custom-backend session"
            );
            return StopHookResult::Allow;
        }

        if self.hooks.is_empty() {
            return StopHookResult::Allow;
        }

        debug!(hook_count = self.hooks.len(), "Evaluating stop hooks");

        for hook in &self.hooks {
            if cancel.is_cancelled() {
                return StopHookResult::Allow;
            }

            // 每个 hook 带超时执行
            let result = tokio::time::timeout(Duration::from_secs(30), hook.execute(ctx)).await;

            match result {
                Ok(StopHookResult::Allow) => continue,
                Ok(StopHookResult::Block { reason }) => {
                    debug!(hook = hook.name(), reason = %reason, "Stop hook blocked");
                    return StopHookResult::Block { reason };
                }
                Ok(StopHookResult::Prevent { reason }) => {
                    warn!(hook = hook.name(), reason = %reason, "Stop hook prevented");
                    return StopHookResult::Prevent { reason };
                }
                Err(_) => {
                    warn!(hook = hook.name(), "Stop hook timed out, treating as allow");
                    continue;
                }
            }
        }

        StopHookResult::Allow
    }
}

impl Default for StopHookManager {
    fn default() -> Self {
        Self::new()
    }
}
