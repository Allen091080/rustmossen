//! # user_type_runtime_lock — 用户类型运行时锁定
//!
//! 对应 TypeScript `utils/userTypeRuntimeLock.ts`。
//! 零依赖。

const PUBLIC_USER_TYPE: &str = "external";

/// 检查是否解锁了内部用户类型。
pub fn is_internal_user_type_unlocked() -> bool {
    std::env::var("MOSSEN_CODE_ALLOW_INTERNAL_USER_TYPE")
        .map(|v| v == "1")
        .unwrap_or(false)
}

/// 标准化用户类型。
///
/// 内部用户类型('ant', 'mossen')仅在显式解锁环境变量设置时才通过；
/// 其他情况均折叠为 'external'。
pub fn normalize_user_type(raw: Option<&str>) -> String {
    let raw = raw.unwrap_or("");
    if raw == PUBLIC_USER_TYPE {
        return PUBLIC_USER_TYPE.to_string();
    }
    if raw.is_empty() {
        return PUBLIC_USER_TYPE.to_string();
    }
    if (raw == "ant" || raw == "mossen") && is_internal_user_type_unlocked() {
        return raw.to_string();
    }
    PUBLIC_USER_TYPE.to_string()
}

/// 应用用户类型运行时锁定，设置 USER_TYPE 环境变量。
pub fn apply_user_type_runtime_lock() {
    let current = std::env::var("USER_TYPE").ok();
    let normalized = normalize_user_type(current.as_deref());
    std::env::set_var("USER_TYPE", &normalized);
}
