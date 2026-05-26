//! # mossen-tui
//!
//! Mossen TUI 层 — 基于 ratatui/crossterm 构建的终端用户界面，
//! 包含消息展示、输入框、Spinner、权限对话框等组件。

#![allow(
    ambiguous_glob_reexports,
    dead_code,
    unexpected_cfgs,
    unused_imports,
    unused_variables,
    clippy::doc_lazy_continuation,
    clippy::explicit_counter_loop,
    clippy::if_same_then_else,
    clippy::large_enum_variant,
    clippy::manual_checked_ops,
    clippy::manual_clamp,
    clippy::manual_strip,
    clippy::needless_range_loop,
    clippy::nonminimal_bool,
    clippy::question_mark,
    clippy::redundant_guards,
    clippy::should_implement_trait,
    clippy::too_many_arguments
)]

pub mod app;
pub mod app_services;
pub mod approval_state;
pub mod event;
pub mod hooks;
pub mod layout;
pub mod message_model;
pub mod render_cache;
pub mod render_events;
pub mod render_glyphs;
pub mod render_lifecycle;
pub mod render_model;
pub mod render_profile;
pub mod screens;
pub mod state;
pub mod theme;
pub mod widgets;

// Re-export core types
pub use app::{ActiveModal, App};
pub use app_services::{SearchPanelState, TerminalServices};
pub use event::{AppEvent, EventBus};
pub use state::AppState;
pub use theme::{Theme, ThemeName};
