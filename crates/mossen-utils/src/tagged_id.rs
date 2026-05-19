//! # tagged_id — Tagged ID 编码工具
//!
//! 对应 TypeScript `utils/taggedId.ts`。
//! 兼容 API 的 tagged_id.py 格式。
//! 产生如 "user_01PaGUP2rbg1XDh7Z9W1CEpd" 的 ID。

const BASE_58_CHARS: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
const VERSION: &str = "01";
/// ceil(128 / log2(58)) = 22
const ENCODED_LENGTH: usize = 22;

/// 将 128 位无符号整数编码为固定长度 base58 字符串。
fn base58_encode(mut n: u128) -> String {
    let base = BASE_58_CHARS.len() as u128;
    let mut result = vec![BASE_58_CHARS[0]; ENCODED_LENGTH];
    let mut i = ENCODED_LENGTH;

    while n > 0 && i > 0 {
        i -= 1;
        let rem = (n % base) as usize;
        result[i] = BASE_58_CHARS[rem];
        n /= base;
    }

    String::from_utf8(result).expect("BASE_58_CHARS are valid UTF-8")
}

/// 将 UUID 字符串（带或不带连字符）解析为 128 位整数。
fn uuid_to_u128(uuid: &str) -> Result<u128, String> {
    let hex: String = uuid.chars().filter(|c| *c != '-').collect();
    if hex.len() != 32 {
        return Err(format!("Invalid UUID hex length: {}", hex.len()));
    }
    u128::from_str_radix(&hex, 16).map_err(|e| format!("Invalid UUID hex: {}", e))
}

/// 将账户 UUID 转换为 API 格式的 tagged ID。
///
/// # 参数
/// - `tag`: 标签前缀（如 "user"、"org"）
/// - `uuid`: UUID 字符串（带或不带连字符）
///
/// # 返回
/// Tagged ID 字符串，如 "user_01PaGUP2rbg1XDh7Z9W1CEpd"
pub fn to_tagged_id(tag: &str, uuid: &str) -> Result<String, String> {
    let n = uuid_to_u128(uuid)?;
    Ok(format!("{}_{}{}", tag, VERSION, base58_encode(n)))
}
