//! # mossen-agent
//!
//! Mossen Agent 核心引擎 — 负责查询编排（Query Orchestration）、
//! 上下文管理、流式处理、LLM API 调用和 Agent Loop 控制。
//!
//! ## 模块结构
//!
//! - [`types`] — 核心类型定义（TurnLedger, DialogueSpec, SdkMessage 等）
//! - [`engine`] — SessionOrchestrator 会话编排器
//! - [`dialogue`] — Agent 对话循环（initiate_dialogue / execute_turn_cycle）
//! - [`api_client`] — API 调用层（HTTP + SSE）
//! - [`streaming`] — SSE 流式响应解析
//! - [`retry`] — 重试逻辑与指数退避
//! - [`context`] — 上下文管理与窗口裁剪
//! - [`history`] — 消息历史管理
//! - [`cost_tracker`] — 成本追踪
//! - [`task`] — 任务定义与管理
//! - [`tool_registry`] — 工具注册与分发
//! - [`stop_hooks`] — 停止钩子机制
//! - [`hooks`] — Hook 子系统（27 种事件、注册、执行、Watcher）
//! - [`transcript`] — Transcript 持久化
//! - [`condenser`] — 自动压缩编排与压缩请求管理
//! - [`token_estimation`] — Token 计数估算
//! - [`diagnostics`] — IDE 诊断跟踪服务

#![allow(
    dead_code,
    non_upper_case_globals,
    unused_assignments,
    unused_imports,
    unused_must_use,
    unused_mut,
    unused_variables,
    clippy::collapsible_match,
    clippy::collapsible_str_replace,
    clippy::derivable_impls,
    clippy::empty_line_after_doc_comments,
    clippy::if_same_then_else,
    clippy::io_other_error,
    clippy::large_enum_variant,
    clippy::len_zero,
    clippy::let_unit_value,
    clippy::map_flatten,
    clippy::map_identity,
    clippy::manual_contains,
    clippy::manual_div_ceil,
    clippy::manual_inspect,
    clippy::manual_pattern_char_comparison,
    clippy::manual_range_contains,
    clippy::manual_split_once,
    clippy::manual_strip,
    clippy::manual_unwrap_or_default,
    clippy::module_inception,
    clippy::needless_lifetimes,
    clippy::needless_borrow,
    clippy::needless_borrows_for_generic_args,
    clippy::needless_range_loop,
    clippy::needless_return,
    clippy::never_loop,
    clippy::new_without_default,
    clippy::clone_on_copy,
    clippy::items_after_test_module,
    clippy::question_mark,
    clippy::redundant_closure,
    clippy::result_large_err,
    clippy::should_implement_trait,
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::unnecessary_cast,
    clippy::unnecessary_lazy_evaluations,
    clippy::unnecessary_map_or,
    clippy::unnecessary_unwrap,
    clippy::unwrap_or_default,
    clippy::useless_conversion,
    clippy::useless_vec
)]

pub mod api;
pub mod api_client;
pub mod commands;
pub mod condenser;
pub mod context;
pub mod coordinator;
pub mod cost_tracker;
pub mod diagnostics;
pub mod dialogue;
pub mod engine;
pub mod history;
pub mod hooks;
pub mod mcp;
pub mod query;
pub mod retry;
pub mod services;
pub mod stop_hooks;
pub mod streaming;
pub mod task;
pub mod token_estimation;
pub mod tool_registry;
pub mod tools_index;
pub mod transcript;
pub mod types;

#[cfg(test)]
pub(crate) mod test_support {
    use std::sync::{Mutex, MutexGuard, OnceLock};

    pub(crate) async fn env_lock_async() -> tokio::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
            .lock()
            .await
    }

    pub(crate) fn config_lock() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .expect("config test lock poisoned")
    }
}

// Re-export 核心类型
pub use engine::{submit_prompt, SessionOrchestrator};
pub use types::{
    ApiUsage, ContinueReason, DialogueSpec, EffortLevel, NonNullableUsage, OrchestratorConfig,
    OriginTag, PromptParams, SdkMessage, StreamEventData, SubmitOptions, TerminalReason,
    ThinkingConfig, TurnLedger,
};
