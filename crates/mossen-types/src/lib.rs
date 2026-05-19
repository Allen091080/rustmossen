//! # mossen-types — Mossen 共享类型
//!
//! 消息、工具、配置、权限、插件等核心类型定义。

pub mod api;
pub mod build_macros;
pub mod command;
pub mod config;
pub mod connector_text;
pub mod constants;
pub mod error;
pub mod generated;
pub mod hooks;
pub mod ids;
pub mod logs;
pub mod message;
pub mod model;
pub mod permissions;
pub mod plugin;
pub mod session;
pub mod text_input;
pub mod tool;

// Re-export 核心类型
pub use api::*;
pub use command::{CommandBase, CommandType, LocalCommandResult, ResumeEntrypoint, is_command_enabled};
pub use config::{IdeType, ImageDimensions, PastedContent, ThemeName};
pub use connector_text::*;
pub use error::MossenError;
pub use ids::*;
pub use message::*;
pub use model::*;
pub use permissions::{
    ExternalPermissionMode, PermissionBehavior, PermissionDecision, PermissionMode,
    PermissionResult, PermissionRule, PermissionUpdate,
};
pub use plugin::{LoadedPlugin, PluginComponent, PluginError, PluginLoadResult, PluginManifest};
pub use session::*;
pub use text_input::{is_valid_image_paste, get_image_paste_ids};
pub use tool::{ToolDefinition, ToolInputSchema, ToolUseContext};
pub use build_macros::BuildMacros;
pub use generated::{PublicApiAuth, MossenCodeInternalEvent, GrowthbookExperimentEvent, ProtoTimestamp};
