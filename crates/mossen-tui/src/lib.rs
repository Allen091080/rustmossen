//! # mossen-tui
//!
//! Mossen TUI 层 — 基于 ratatui/crossterm 构建的终端用户界面，
//! 包含消息展示、输入框、Spinner、权限对话框等组件。

#![allow(
    ambiguous_glob_reexports,
    dead_code,
    unexpected_cfgs,
    unused_imports,
    unused_variables
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
