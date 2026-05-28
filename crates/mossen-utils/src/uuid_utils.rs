//! # uuid_utils — UUID 验证与 Agent ID 生成
//!
//! 对应 TypeScript `utils/uuid.ts`。

use once_cell::sync::Lazy;
use rand::Rng;
use regex::Regex;

static UUID_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$").unwrap()
});

/// Validate a UUID string.
/// Returns the string as-is if valid, or None if not a valid UUID.
pub fn validate_uuid(maybe_uuid: &str) -> Option<&str> {
    if UUID_REGEX.is_match(maybe_uuid) {
        Some(maybe_uuid)
    } else {
        None
    }
}

/// Generate a new agent ID with prefix for consistency with task IDs.
/// Format: `a{label-}{16 hex chars}`
/// Example: `aa3f2c1b4d5e6f7a8`, `acompact-a3f2c1b4d5e6f7a8`
pub fn create_agent_id(label: Option<&str>) -> String {
    let mut rng = rand::thread_rng();
    let mut bytes = [0u8; 8];
    rng.fill(&mut bytes);
    let suffix = hex::encode(bytes);
    match label {
        Some(l) => format!("a{}-{}", l, suffix),
        None => format!("a{}", suffix),
    }
}
