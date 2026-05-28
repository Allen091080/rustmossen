//! # binary_check — 二进制检查工具
//!
//! 对应 TypeScript `utils/binaryCheck.ts`。
//! 检查二进制文件/命令是否已安装并可用。

use std::collections::HashMap;
use std::sync::Mutex;

/// 会话缓存以避免重复检查。
static BINARY_CACHE: std::sync::LazyLock<Mutex<HashMap<String, bool>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

/// 检查二进制文件/命令是否已安装并可用。
/// 在 Unix 系统 (macOS, Linux, WSL) 上使用 'which'，在 Windows 上使用 'where'。
pub async fn is_binary_installed(command: &str) -> bool {
    // 空命令检查
    if command.trim().is_empty() {
        return false;
    }

    let trimmed = command.trim();

    // 先检查缓存
    {
        let cache = BINARY_CACHE.lock().unwrap();
        if let Some(&cached) = cache.get(trimmed) {
            return cached;
        }
    }

    // 通过 `which` crate 解析 PATH（跨平台：Unix 使用 which 语义，Windows 使用 where 语义）。
    let exists = which::which(trimmed).is_ok();

    // 缓存结果
    {
        let mut cache = BINARY_CACHE.lock().unwrap();
        cache.insert(trimmed.to_string(), exists);
    }

    exists
}

/// 清除二进制检查缓存（用于测试）。
pub fn clear_binary_cache() {
    let mut cache = BINARY_CACHE.lock().unwrap();
    cache.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clear_cache() {
        clear_binary_cache();
        let cache = BINARY_CACHE.lock().unwrap();
        assert!(cache.is_empty());
    }
}
