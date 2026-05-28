//! # post_sampling — 推理后 Hook
//!
//! 对应 TS `utils/hooks/postSamplingHooks.ts`。
//! 管理模型采样完成后的 Hook 回调。
//!
//! 按文档 12 命名：
//! - `executePostSamplingHooks()` → `fire_post_inference_watchers()`

use tracing::{debug, warn};

/// 推理后 Hook 上下文。
///
/// 对应 TS `REPLHookContext`。
#[derive(Debug, Clone)]
pub struct PostInferenceContext {
    /// 消息历史（序列化为 JSON）。
    pub messages_json: String,
    /// 系统提示。
    pub system_prompt: String,
    /// 用户上下文。
    pub user_context: std::collections::HashMap<String, String>,
    /// 系统上下文。
    pub system_context: std::collections::HashMap<String, String>,
    /// 查询来源。
    pub query_source: Option<String>,
}

/// 推理后 Hook 回调类型。
pub type PostSamplingHookFn = Box<
    dyn Fn(&PostInferenceContext) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
        + Send
        + Sync,
>;

/// 推理后 Hook 注册表。
///
/// 对应 TS 中的 `postSamplingHooks` 全局数组。
pub struct PostSamplingHookRegistry {
    /// 注册的 Hook 列表。
    hooks: parking_lot::Mutex<Vec<PostSamplingHookFn>>,
}

impl PostSamplingHookRegistry {
    /// 创建新的注册表。
    pub fn new() -> Self {
        Self {
            hooks: parking_lot::Mutex::new(Vec::new()),
        }
    }

    /// 注册推理后 Hook。
    ///
    /// 对应 TS `registerPostSamplingHook()`。
    pub fn register(&self, hook: PostSamplingHookFn) {
        self.hooks.lock().push(hook);
    }

    /// 清除所有注册的 Hook（用于测试）。
    ///
    /// 对应 TS `clearPostSamplingHooks()`。
    pub fn clear(&self) {
        self.hooks.lock().clear();
    }

    /// 触发推理后观察者 — 执行所有注册的 Hook。
    ///
    /// 对应 TS `executePostSamplingHooks()` → Rust `fire_post_inference_watchers()`。
    /// 顺序执行，错误隔离（单个 Hook 失败不影响其他）。
    pub fn fire_post_inference_watchers(&self, context: &PostInferenceContext) {
        let hooks = self.hooks.lock();
        debug!(hook_count = hooks.len(), "Firing post-inference watchers");

        for (i, hook) in hooks.iter().enumerate() {
            match hook(context) {
                Ok(()) => {}
                Err(e) => {
                    warn!(
                        hook_index = i,
                        error = %e,
                        "Post-inference watcher failed"
                    );
                }
            }
        }
    }

    /// 获取注册的 Hook 数量。
    pub fn count(&self) -> usize {
        self.hooks.lock().len()
    }
}

impl Default for PostSamplingHookRegistry {
    fn default() -> Self {
        Self::new()
    }
}
