//! # binary_check_utils — 二进制/命令检测
//!
//! 对应 TypeScript `utils/binaryCheck.ts`。

use std::collections::HashMap;
use std::sync::Mutex;

static BINARY_CACHE: Mutex<Option<HashMap<String, bool>>> = Mutex::new(None);

/// 检查二进制/命令是否已安装且可用。
pub async fn is_binary_installed(command: &str) -> bool {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        tracing::debug!("[binaryCheck] Empty command provided, returning false");
        return false;
    }

    // 检查缓存
    {
        let guard = BINARY_CACHE.lock().unwrap();
        if let Some(cache) = guard.as_ref() {
            if let Some(&cached) = cache.get(trimmed) {
                tracing::debug!("[binaryCheck] Cache hit for '{}': {}", trimmed, cached);
                return cached;
            }
        }
    }

    let exists = crate::which::which_sync(trimmed).is_some();

    // 缓存结果
    {
        let mut guard = BINARY_CACHE.lock().unwrap();
        let cache = guard.get_or_insert_with(HashMap::new);
        cache.insert(trimmed.to_string(), exists);
    }

    tracing::debug!(
        "[binaryCheck] Binary '{}' {}",
        trimmed,
        if exists { "found" } else { "not found" }
    );

    exists
}

/// 清除二进制检查缓存（用于测试）。
pub fn clear_binary_cache() {
    let mut guard = BINARY_CACHE.lock().unwrap();
    if let Some(cache) = guard.as_mut() {
        cache.clear();
    }
}
