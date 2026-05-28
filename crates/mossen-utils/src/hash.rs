//! # hash — 非加密哈希函数
//!
//! 对应 TypeScript `utils/hash.ts`。

use sha2::{Digest, Sha256};

/// djb2 字符串哈希 — 快速非加密哈希，返回有符号 32 位整数。
///
/// 跨运行时确定性（不同于 Bun.hash 使用 wyhash）。
pub fn djb2_hash(s: &str) -> i32 {
    let mut hash: i32 = 0;
    for byte in s.bytes() {
        hash = hash
            .wrapping_shl(5)
            .wrapping_sub(hash)
            .wrapping_add(byte as i32);
    }
    hash
}

/// 哈希内容用于变更检测。
///
/// 使用 SHA-256 用于内容指纹。
pub fn hash_content(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hex::encode(hasher.finalize())
}

/// 哈希两个字符串，不分配临时连接字符串。
///
/// 使用增量 SHA-256 update，用 NUL 字节分隔以消除歧义。
pub fn hash_pair(a: &str, b: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(a.as_bytes());
    hasher.update(b"\0");
    hasher.update(b.as_bytes());
    hex::encode(hasher.finalize())
}
