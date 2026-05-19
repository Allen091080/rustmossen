//! # timeouts — Bash 操作超时常量
//!
//! 对应 TypeScript `utils/timeouts.ts`。

const DEFAULT_TIMEOUT_MS: u64 = 120_000; // 2 minutes
const MAX_TIMEOUT_MS: u64 = 600_000; // 10 minutes

/// 获取 bash 操作的默认超时（毫秒）。
///
/// 检查 BASH_DEFAULT_TIMEOUT_MS 环境变量，否则返回 2 分钟默认值。
pub fn get_default_bash_timeout_ms() -> u64 {
    if let Ok(val) = std::env::var("BASH_DEFAULT_TIMEOUT_MS") {
        if let Ok(parsed) = val.parse::<u64>() {
            if parsed > 0 {
                return parsed;
            }
        }
    }
    DEFAULT_TIMEOUT_MS
}

/// 获取 bash 操作的最大超时（毫秒）。
///
/// 检查 BASH_MAX_TIMEOUT_MS 环境变量，否则返回 10 分钟默认值。
/// 确保最大值不小于默认值。
pub fn get_max_bash_timeout_ms() -> u64 {
    let default = get_default_bash_timeout_ms();
    if let Ok(val) = std::env::var("BASH_MAX_TIMEOUT_MS") {
        if let Ok(parsed) = val.parse::<u64>() {
            if parsed > 0 {
                return parsed.max(default);
            }
        }
    }
    MAX_TIMEOUT_MS.max(default)
}
