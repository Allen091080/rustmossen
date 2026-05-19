//! # lockfile — 文件锁操作
//!
//! 对应 TypeScript `utils/lockfile.ts`。
//! 懒加载文件锁实现。

use std::path::Path;
use tokio::fs;

/// 锁定选项。
#[derive(Debug, Clone, Default)]
pub struct LockOptions {
    pub stale: Option<u64>,
    pub retries: Option<u32>,
}

/// 获取文件锁。
///
/// 返回一个解锁函数（异步）。
pub async fn lock(file: &Path, _options: Option<&LockOptions>) -> Result<(), String> {
    let lock_path = file.with_extension("lock");
    // 尝试创建锁文件（原子操作）
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
        .await
    {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            Err(format!("Lock file already exists: {}", lock_path.display()))
        }
        Err(e) => Err(format!("Failed to acquire lock: {}", e)),
    }
}

/// 释放文件锁。
pub async fn unlock(file: &Path) -> Result<(), String> {
    let lock_path = file.with_extension("lock");
    fs::remove_file(&lock_path)
        .await
        .map_err(|e| format!("Failed to release lock: {}", e))
}

/// 检查文件是否已被锁定。
pub async fn check(file: &Path) -> bool {
    let lock_path = file.with_extension("lock");
    lock_path.exists()
}

/// 同步版本的锁定。
pub fn lock_sync(file: &Path, _options: Option<&LockOptions>) -> Result<(), String> {
    let lock_path = file.with_extension("lock");
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
    {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            Err(format!("Lock file already exists: {}", lock_path.display()))
        }
        Err(e) => Err(format!("Failed to acquire lock: {}", e)),
    }
}
