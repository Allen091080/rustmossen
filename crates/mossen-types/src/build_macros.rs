//! # build_macros — 构建时宏常量
//!
//! 对应 TypeScript `types/macro.d.ts`。
//! 定义构建时注入的全局常量。

use serde::{Deserialize, Serialize};

/// 构建时宏常量。
/// 对应 TS 全局 `MACRO` 对象。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildMacros {
    /// 版本号。
    pub version: String,
    /// 构建时间。
    pub build_time: String,
    /// 包 URL。
    pub package_url: String,
    /// Native 包 URL。
    pub native_package_url: String,
    /// 反馈频道。
    pub feedback_channel: String,
    /// Issue 说明页面。
    pub issues_explainer: String,
    /// 版本变更日志。
    pub version_changelog: String,
}

impl Default for BuildMacros {
    fn default() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
            build_time: String::new(),
            package_url: String::new(),
            native_package_url: String::new(),
            feedback_channel: String::new(),
            issues_explainer: String::new(),
            version_changelog: String::new(),
        }
    }
}
