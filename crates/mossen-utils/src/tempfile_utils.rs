//! # tempfile_utils — 临时文件路径生成
//!
//! 对应 TypeScript `utils/tempfile.ts`。

use sha2::{Digest, Sha256};
use std::path::PathBuf;
use uuid::Uuid;

/// 生成临时文件路径。
///
/// - `prefix`: 可选前缀（默认 "mossen-prompt"）
/// - `extension`: 可选扩展名（默认 ".md"）
/// - `content_hash`: 如果提供，使用内容的 SHA-256 hash 前 16 字符作为标识符，
///   确保跨进程稳定性。否则使用随机 UUID。
pub fn generate_temp_file_path(
    prefix: Option<&str>,
    extension: Option<&str>,
    content_hash: Option<&str>,
) -> PathBuf {
    let prefix = prefix.unwrap_or("mossen-prompt");
    let extension = extension.unwrap_or(".md");

    let id = match content_hash {
        Some(content) => {
            let mut hasher = Sha256::new();
            hasher.update(content.as_bytes());
            let result = hasher.finalize();
            hex::encode(&result[..8]) // 16 hex chars
        }
        None => Uuid::new_v4().to_string(),
    };

    let tmp_dir = std::env::temp_dir();
    tmp_dir.join(format!("{}-{}{}", prefix, id, extension))
}
