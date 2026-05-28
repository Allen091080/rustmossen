//! UUID 工具
//!
//! 对应 TS `uuid.ts`。

use regex::Regex;

/// UUID 格式正则表达式。
const UUID_REGEX: &str = r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$";

lazy_static::lazy_static! {
    static ref UUID_RE: Regex = Regex::new(UUID_REGEX).unwrap();
}

/// 验证 UUID。
///
/// # 参数
/// - `maybe_uuid`: 要检查的值
///
/// # 返回
/// 如果是有效的 UUID 则返回 Some(uuid)，否则返回 None。
pub fn validate_uuid<S: AsRef<str>>(maybe_uuid: S) -> Option<String> {
    let uuid = maybe_uuid.as_ref();
    if UUID_RE.is_match(uuid) {
        Some(uuid.to_string())
    } else {
        None
    }
}

/// 创建新的代理 ID。
///
/// 格式: a{label-}{16 hex chars}
///
/// # 示例
/// - `create_agent_id(None)` → "a3f2c1b4d5e6f7a8"
/// - `create_agent_id(Some("compact"))` → "acompact-a3f2c1b4d5e6f7a8"
pub fn create_agent_id(label: Option<&str>) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();

    let random: u64 = (timestamp ^ (timestamp >> 32)) as u64;
    let hex = format!("{:016x}", random);

    match label {
        Some(l) => format!("a{}-{}", l, hex),
        None => format!("a{}", hex),
    }
}
