//! # hooks — Hook 子系统（Watcher 框架）
//!
//! 对应 TypeScript `utils/hooks/` 目录。
//! 实现 Hook 注册、触发、管理的完整子系统。
//!
//! ## 模块结构
//!
//! - [`events`] — 27 种事件元数据（`describe_event()`）
//! - [`settings`] — Hook 来源与配置（`HookSource`, `IndividualHookConfig`）
//! - [`registry`] — HookRegistry：事件→处理器映射（`index_watchers()`）
//! - [`executor`] — Hook 执行引擎（并发执行、超时、错误处理）
//! - [`session_hooks`] — 会话级 Hook（临时内存 hooks）
//! - [`async_registry`] — 异步 Hook 跟踪注册表
//! - [`hook_events`] — Hook 事件广播系统
//! - [`config_snapshot`] — Hook 配置快照
//! - [`file_watcher`] — 文件变更 Watcher（`notify` crate）
//! - [`ssrf_guard`] — SSRF 防护（HTTP hook 地址验证）
//! - [`exec_command`] — Command hook 执行器
//! - [`exec_http`] — HTTP hook 执行器
//! - [`exec_prompt`] — Prompt hook 执行器
//! - [`exec_agent`] — Agent hook 执行器
//! - [`post_sampling`] — 推理后 Hook（`fire_post_inference_watchers()`）
//! - [`helpers`] — Hook 辅助函数

pub mod async_registry;
pub mod config_snapshot;
pub mod events;
pub mod exec_agent;
pub mod exec_command;
pub mod exec_http;
pub mod exec_prompt;
pub mod executor;
pub mod file_watcher;
pub mod helpers;
pub mod hook_events;
pub mod post_sampling;
pub mod registry;
pub mod session_hooks;
pub mod settings;
pub mod ssrf_guard;

// Re-export 核心类型
pub use async_registry::{AsyncHookRegistry, PendingAsyncHook};
pub use config_snapshot::HooksConfigSnapshot;
pub use events::{describe_event, HookEventMetadata, MatcherMetadata};
pub use executor::{HookExecutor, HookExecutorConfig};
pub use file_watcher::FileChangedWatcher;
pub use hook_events::{
    HookEventBroadcaster, HookExecutionEvent, HookResponseEvent, HookStartedEvent,
};
pub use post_sampling::PostSamplingHookRegistry;
pub use registry::HookRegistry;
pub use session_hooks::{SessionHookStore, SessionHooksManager};
pub use settings::{HookSource, IndividualHookConfig};
pub use ssrf_guard::is_blocked_address;
