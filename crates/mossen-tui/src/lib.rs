//! # mossen-tui
//!
//! Mossen TUI 层 — 基于 ratatui/crossterm 构建的终端用户界面，
//! 包含消息展示、输入框、Spinner、权限对话框等组件。

#![allow(dead_code, unused_imports)]

pub mod app;
pub mod app_services;
pub mod components;
pub mod event;
pub mod hooks;
pub mod ink;
pub mod layout;
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
