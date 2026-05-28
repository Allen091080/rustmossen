//! # constants — 所有常量定义
//!
//! 对应 TypeScript `constants/` 目录下所有 21 个文件。
//! 包含 API 限制、Beta 头、文件类型、工具限制、XML 标签、
//! OAuth、产品配置、GitHub App、系统提示等常量。

pub mod api_limits;
pub mod betas;
pub mod common;
pub mod cyber_risk;
pub mod error_ids;
pub mod figures;
pub mod files;
pub mod github_app;
pub mod messages;
pub mod oauth;
pub mod output_styles;
pub mod product;
pub mod prompts;
pub mod spinner_verbs;
pub mod system;
pub mod system_prompt_sections;
pub mod tool_limits;
pub mod tools;
pub mod turn_completion_verbs;
pub mod xml;

// Re-export all public items for backward compatibility
pub use api_limits::*;
pub use betas::*;
pub use common::*;
pub use cyber_risk::*;
pub use error_ids::*;
pub use figures::*;
pub use files::*;
pub use messages::*;
pub use tool_limits::*;
pub use turn_completion_verbs::*;
pub use xml::*;
