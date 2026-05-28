//! # user_type — 用户类型访问器
//!
//! 对应 TypeScript `utils/userType.ts`。
//! 零依赖叶模块，暴露 USER_TYPE 访问器。

/// 获取用户类型。
pub fn get_user_type() -> String {
    std::env::var("USER_TYPE").unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_user_type() {
        std::env::set_var("USER_TYPE", "user");
        assert_eq!(get_user_type(), "user");
    }
}
