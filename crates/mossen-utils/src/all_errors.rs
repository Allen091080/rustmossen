//! # all_errors — 合并所有设置错误
//!
//! 对应 TypeScript `utils/settings/allErrors.ts`。
//! 将设置验证错误与 MCP 配置错误合并。

use crate::validation::{SettingsWithErrors, ValidationError};

/// MCP 配置错误的可插拔提供方：当其它 crate 拥有 MCP 配置服务（即
/// `services/mcp/config.ts` 的 Rust 对应）时，通过 [`set_mcp_errors_provider`]
/// 注入；缺省下不返回任何 MCP 错误。
///
/// 用此 hook 避免对 `mcp` crate 的反向依赖（与 TS 端把
/// `getSettingsWithAllErrors` 放在叶子模块的目的一致）。
type McpErrorsProvider = fn() -> Vec<ValidationError>;

static MCP_ERRORS_PROVIDER: parking_lot::RwLock<Option<McpErrorsProvider>> =
    parking_lot::RwLock::new(None);

/// 安装/替换 MCP 错误提供方（通常在进程启动时由组装层调用）。
pub fn set_mcp_errors_provider(provider: McpErrorsProvider) {
    *MCP_ERRORS_PROVIDER.write() = Some(provider);
}

/// 卸载 MCP 错误提供方（主要用于测试）。
pub fn clear_mcp_errors_provider() {
    *MCP_ERRORS_PROVIDER.write() = None;
}

/// 获取合并的设置和所有验证错误，包括 MCP 配置错误。
///
/// 当需要完整错误集（设置 + MCP）时使用此函数。底层 settings 加载在
/// [`crate::settings::get_settings_with_errors`]（接受 sources/getter/initial
/// 参数）；本函数接受一个 `base` 作为 settings 部分的入参，并把注入的 MCP
/// 错误追加到 `errors` 末尾，与 TS 行为一致。
pub fn get_settings_with_all_errors(base: SettingsWithErrors) -> SettingsWithErrors {
    let mut result = base;
    if let Some(provider) = *MCP_ERRORS_PROVIDER.read() {
        result.errors.extend(provider());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// 全局 provider 状态在并发测试中会互相干扰，因此通过一个共享的串行锁
    /// 强制两类测试顺序执行。
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn test_get_settings_with_all_errors_passes_through() {
        let _guard = TEST_LOCK.lock().unwrap();
        clear_mcp_errors_provider();
        let base = SettingsWithErrors {
            settings: serde_json::json!({}),
            errors: vec![],
        };
        let result = get_settings_with_all_errors(base);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_get_settings_with_all_errors_appends_mcp_errors() {
        let _guard = TEST_LOCK.lock().unwrap();
        fn provider() -> Vec<ValidationError> {
            vec![ValidationError {
                file: Some("mcp.json".to_string()),
                path: "servers".to_string(),
                message: "bad config".to_string(),
                expected: None,
                invalid_value: None,
                suggestion: None,
                doc_link: None,
            }]
        }
        set_mcp_errors_provider(provider);
        let base = SettingsWithErrors {
            settings: serde_json::json!({}),
            errors: vec![],
        };
        let result = get_settings_with_all_errors(base);
        assert_eq!(result.errors.len(), 1);
        clear_mcp_errors_provider();
    }
}
