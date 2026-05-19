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

#![allow(dead_code, unused_imports)]

pub mod api;
pub mod api_client;
pub mod commands;
pub mod condenser;
pub mod mcp;
pub mod context;
pub mod cost_tracker;
pub mod diagnostics;
pub mod dialogue;
pub mod engine;
pub mod history;
pub mod hooks;
pub mod retry;
pub mod stop_hooks;
pub mod streaming;
pub mod task;
pub mod token_estimation;
pub mod tool_registry;
pub mod transcript;
pub mod types;
pub mod services;
pub mod query;
pub mod coordinator;
pub mod tools_index;

// Re-export 核心类型
pub use engine::{submit_prompt, SessionOrchestrator};
pub use types::{
    ApiUsage, ContinueReason, DialogueSpec, EffortLevel, NonNullableUsage, OrchestratorConfig,
    OriginTag, PromptParams, SdkMessage, StreamEventData, SubmitOptions, TerminalReason,
    ThinkingConfig, TurnLedger,
};
