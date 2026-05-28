//! # paste_store — 粘贴内容存储
//!
//! 对应 TypeScript `utils/pasteStore.ts`。
//! 提供粘贴内容的持久化存储，基于内容哈希进行寻址。

use sha2::{Digest, Sha256};
use std::path::PathBuf;
use tokio::fs;
use tracing::debug;

use crate::env::get_mossen_config_home_dir;

const PASTE_STORE_DIR: &str = "paste-cache";

/// 获取粘贴存储目录（跨会话持久）
fn get_paste_store_dir() -> PathBuf {
    get_mossen_config_home_dir().join(PASTE_STORE_DIR)
}

/// 为粘贴内容生成哈希值用作文件名。
///
/// 导出以便调用者可以在异步存储之前同步获取哈希。
pub fn hash_pasted_text(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let result = hasher.finalize();
    hex::encode(&result[..8]) // 取前16个十六进制字符（8字节）
}

/// 获取粘贴内容的文件路径
fn get_paste_path(hash: &str) -> PathBuf {
    get_paste_store_dir().join(format!("{}.txt", hash))
}

/// 将粘贴文本内容存储到磁盘。
///
/// 哈希应该使用 `hash_pasted_text()` 预先计算，
/// 这样调用者可以立即使用它而不必等待异步磁盘写入。
pub async fn store_pasted_text(hash: &str, content: &str) {
    let result = store_pasted_text_inner(hash, content).await;
    if let Err(e) = result {
        debug!("Failed to store paste: {}", e);
    }
}

async fn store_pasted_text_inner(hash: &str, content: &str) -> anyhow::Result<()> {
    let dir = get_paste_store_dir();
    fs::create_dir_all(&dir).await?;

    let paste_path = get_paste_path(hash);

    // Content-addressable: same hash = same content, so overwriting is safe
    fs::write(&paste_path, content).await?;

    // Set file permissions to 0o600 (owner read/write only)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        fs::set_permissions(&paste_path, perms).await?;
    }

    debug!("Stored paste {} to {:?}", hash, paste_path);
    Ok(())
}

/// 通过哈希检索粘贴文本内容。
///
/// 如果未找到或出错则返回 None。
pub async fn retrieve_pasted_text(hash: &str) -> Option<String> {
    let paste_path = get_paste_path(hash);
    match fs::read_to_string(&paste_path).await {
        Ok(content) => Some(content),
        Err(e) => {
            // ENOENT is expected when paste doesn't exist
            if e.kind() != std::io::ErrorKind::NotFound {
                debug!("Failed to retrieve paste {}: {}", hash, e);
            }
            None
        }
    }
}

/// 清理不再引用的旧粘贴文件。
///
/// 简单的基于时间的清理——移除早于 cutoff_time 的文件。
pub async fn cleanup_old_pastes(cutoff_time: std::time::SystemTime) {
    let paste_dir = get_paste_store_dir();

    let mut entries = match fs::read_dir(&paste_dir).await {
        Ok(entries) => entries,
        Err(_) => return, // Directory doesn't exist or can't be read
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        if !name.ends_with(".txt") {
            continue;
        }

        let file_path = entry.path();
        match fs::metadata(&file_path).await {
            Ok(metadata) => {
                if let Ok(modified) = metadata.modified() {
                    if modified < cutoff_time {
                        if let Err(_) = fs::remove_file(&file_path).await {
                            // Ignore errors for individual files
                        } else {
                            debug!("Cleaned up old paste: {:?}", file_path);
                        }
                    }
                }
            }
            Err(_) => {
                // Ignore errors for individual files
            }
        }
    }
}
