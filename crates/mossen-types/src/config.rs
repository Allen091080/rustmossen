//! # config — 配置相关类型
//!
//! 定义产品配置、系统配置等类型。

use serde::{Deserialize, Serialize};

/// 产品信息常量。
pub mod product {
    /// 默认产品 URL。
    pub const PRODUCT_URL: &str = "https://mossen.invalid/code";

    /// 获取产品显示名称。
    pub fn get_product_display_name() -> &'static str {
        "Mossen"
    }

    /// 获取产品助手名称。
    pub fn get_product_assistant_name() -> &'static str {
        "Mossen"
    }

    /// 获取产品欢迎消息。
    pub fn get_product_welcome_message() -> String {
        format!("Welcome to {}", get_product_display_name())
    }

    /// 获取 CLI 名称。
    pub fn get_product_cli_name() -> &'static str {
        "mossen"
    }

    /// 获取项目指令文件名。
    pub fn get_project_instructions_display_name() -> &'static str {
        "MOSSEN.md"
    }

    /// 获取配置目录名。
    pub fn get_product_config_dir_name() -> &'static str {
        ".mossen"
    }

    /// 获取配置主目录显示路径。
    pub fn get_product_config_home_display_path() -> &'static str {
        "~/.mossen"
    }

    /// 获取桌面产品名。
    pub fn get_desktop_product_name() -> &'static str {
        "Mossen Desktop"
    }
}

/// Hosted 基础 URL 常量。
pub const HOSTED_BASE_URL: &str = "https://hosted.mossen.invalid";
pub const HOSTED_STAGING_BASE_URL: &str = "https://hosted-staging.mossen.invalid";
pub const HOSTED_LOCAL_BASE_URL: &str = "http://localhost:4000";

/// IDE 类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdeType {
    /// VSCode。
    Vscode,
    /// JetBrains。
    Jetbrains,
}

/// IDE 扩展安装状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdeExtensionInstallationStatus {
    Installed,
    NotInstalled,
    Unknown,
}

/// 主题名称。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeName(pub String);

/// 粘贴内容类型。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PastedContent {
    /// 文本。
    Text { id: usize, content: String },
    /// 图像。
    Image {
        id: usize,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        media_type: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        filename: Option<String>,
    },
}

/// 图像尺寸。
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ImageDimensions {
    pub width: u32,
    pub height: u32,
}

/// 文本高亮。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextHighlight {
    pub start: usize,
    pub end: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
}
