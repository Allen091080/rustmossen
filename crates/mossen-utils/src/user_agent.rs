//! # user_agent — User-Agent 字符串工具
//!
//! 对应 TypeScript `utils/userAgent.ts`。
//! 提供 User-Agent 字符串生成函数。

/// 获取 Mossen 的 User-Agent 字符串。
pub fn get_mossen_user_agent() -> String {
    format!("mossen-code/{}", env!("CARGO_PKG_VERSION"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_mossen_user_agent() {
        let ua = get_mossen_user_agent();
        assert!(ua.starts_with("mossen-code/"));
    }
}
