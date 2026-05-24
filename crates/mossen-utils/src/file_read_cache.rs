//! # file_read_cache — 文件读取缓存
//!
//! 对应 TypeScript `utils/fileReadCache.ts`。
//! 提供基于修改时间自动失效的简单内存文件缓存，
//! 消除 FileEditTool 操作中的冗余文件读取。

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// 缓存的文件数据
#[derive(Clone)]
struct CachedFileData {
    content: String,
    encoding: String,
    mtime_ms: u128,
}

/// 文件读取缓存
///
/// 简单的内存缓存，基于文件修改时间自动失效。
struct FileReadCacheInner {
    cache: HashMap<PathBuf, CachedFileData>,
    max_cache_size: usize,
}

impl FileReadCacheInner {
    fn new() -> Self {
        Self {
            cache: HashMap::new(),
            max_cache_size: 1000,
        }
    }
}

/// 文件读取结果
pub struct FileReadResult {
    pub content: String,
    pub encoding: String,
}

/// 全局文件读取缓存单例
static FILE_READ_CACHE: Lazy<Mutex<FileReadCacheInner>> =
    Lazy::new(|| Mutex::new(FileReadCacheInner::new()));

/// 读取文件（带缓存）。返回内容和编码。
///
/// 缓存键包含文件路径和修改时间，自动失效。
pub fn cached_read_file(file_path: &Path) -> anyhow::Result<FileReadResult> {
    let mut cache = FILE_READ_CACHE.lock();

    // Get file stats for cache invalidation
    let stats = match std::fs::metadata(file_path) {
        Ok(s) => s,
        Err(e) => {
            // File was deleted, remove from cache and re-throw
            cache.cache.remove(file_path);
            return Err(e.into());
        }
    };

    let mtime_ms = stats
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis())
        .unwrap_or(0);

    // Check if we have valid cached data
    if let Some(cached_data) = cache.cache.get(file_path) {
        if cached_data.mtime_ms == mtime_ms {
            return Ok(FileReadResult {
                content: cached_data.content.clone(),
                encoding: cached_data.encoding.clone(),
            });
        }
    }

    // Cache miss or stale data - read the file
    let encoding = detect_encoding(file_path);
    let content = std::fs::read_to_string(file_path).map(|s| s.replace("\r\n", "\n"))?;

    // Update cache
    cache.cache.insert(
        file_path.to_path_buf(),
        CachedFileData {
            content: content.clone(),
            encoding: encoding.clone(),
            mtime_ms,
        },
    );

    // Evict oldest entries if cache is too large
    if cache.cache.len() > cache.max_cache_size {
        if let Some(first_key) = cache.cache.keys().next().cloned() {
            cache.cache.remove(&first_key);
        }
    }

    Ok(FileReadResult { content, encoding })
}

/// 清除整个缓存。用于测试或内存管理。
pub fn clear_file_read_cache() {
    let mut cache = FILE_READ_CACHE.lock();
    cache.cache.clear();
}

/// 从缓存中移除特定文件。
pub fn invalidate_file_cache(file_path: &Path) {
    let mut cache = FILE_READ_CACHE.lock();
    cache.cache.remove(file_path);
}

/// 获取缓存统计信息，用于调试/监控。
pub fn get_file_cache_stats() -> (usize, Vec<PathBuf>) {
    let cache = FILE_READ_CACHE.lock();
    (cache.cache.len(), cache.cache.keys().cloned().collect())
}

/// 简单编码检测
fn detect_encoding(file_path: &Path) -> String {
    let data = match std::fs::read(file_path) {
        Ok(d) => d,
        Err(_) => return "utf8".to_string(),
    };

    if data.len() >= 2 && data[0] == 0xFF && data[1] == 0xFE {
        return "utf16le".to_string();
    }

    "utf8".to_string()
}
