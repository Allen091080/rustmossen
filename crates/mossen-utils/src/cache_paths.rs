//! # cache_paths — 缓存路径管理
//!
//! 对应 TypeScript `utils/cachePaths.ts`。
//! 缓存路径的获取和管理。

use std::path::PathBuf;

/// 获取缓存根目录。
/// 对应 TS 中 `env-paths('mossen-cli').cache`：使用 dirs crate 解析平台缓存目录
/// （macOS `~/Library/Caches`、Linux `$XDG_CACHE_HOME` 或 `~/.cache`、Windows `%LOCALAPPDATA%`）。
pub fn get_cache_root() -> PathBuf {
    dirs::cache_dir()
        .map(|d| d.join("mossen-cli"))
        .unwrap_or_else(|| PathBuf::from("/tmp/mossen-cache"))
}

/// 获取项目缓存目录。
pub fn get_project_cache_dir(cwd: &str) -> PathBuf {
    let sanitized = sanitize_path(cwd);
    get_cache_root().join(sanitized)
}

/// 对应 TS `CACHE_PATHS` 命名空间对象：把项目缓存子路径收敛到一个结构体里。
pub struct CachePaths;

#[allow(non_snake_case)]
impl CachePaths {
    /// 项目级日志基础目录。
    pub fn baseLogs(cwd: &str) -> PathBuf {
        get_project_cache_dir(cwd)
    }
    /// errors 子目录。
    pub fn errors(cwd: &str) -> PathBuf {
        get_project_cache_dir(cwd).join("errors")
    }
    /// messages 子目录。
    pub fn messages(cwd: &str) -> PathBuf {
        get_project_cache_dir(cwd).join("messages")
    }
    /// 单个 MCP server 的日志目录。
    pub fn mcpLogs(cwd: &str, server_name: &str) -> PathBuf {
        get_project_cache_dir(cwd).join(format!("mcp-logs-{}", sanitize_path(server_name)))
    }
}

/// 命名空间静态实例 — 与 TS `export const CACHE_PATHS` 等价。
pub const CACHE_PATHS: CachePaths = CachePaths;

/// 清理路径名称，移除非法字符。
const MAX_SANITIZED_LENGTH: usize = 200;

fn sanitize_path(name: &str) -> String {
    let sanitized: String = name.chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    
    if sanitized.len() <= MAX_SANITIZED_LENGTH {
        sanitized
    } else {
        let hash = djb2_hash(name);
        format!("{}-{}", &sanitized[..MAX_SANITIZED_LENGTH], hash.abs())
    }
}

/// DJB2 哈希函数。
fn djb2_hash(s: &str) -> i64 {
    let mut hash: u64 = 5381;
    for c in s.chars() {
        hash = hash.wrapping_mul(33).wrapping_add(c as u64);
    }
    hash as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_path() {
        let result = sanitize_path("my-project");
        assert_eq!(result, "my-project");
    }

    #[test]
    fn test_sanitize_path_with_special_chars() {
        let result = sanitize_path("my project!");
        assert_eq!(result, "my-project-");
    }
}
