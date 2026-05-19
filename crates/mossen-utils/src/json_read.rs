//! # json_read — JSON 读取工具
//!
//! 对应 TypeScript `utils/jsonRead.ts`。
//! 剥离 UTF-8 BOM 的工具。

/// UTF-8 BOM (U+FEFF)。
const UTF8_BOM: char = '\u{FEFF}';

/// 剥离字符串开头的 UTF-8 BOM。
///
/// PowerShell 5.x 默认以 UTF-8 with BOM 写入 (Out-File, Set-Content)。
/// 无法控制用户环境，所以在读取时剥离 BOM。
/// 没有这个，JSON.parse 会失败并显示 "Unexpected token"。
pub fn strip_bom(content: &str) -> &str {
    if content.starts_with(UTF8_BOM) {
        &content[UTF8_BOM.len_utf8()..]
    } else {
        content
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_bom() {
        let with_bom = format!("{} {{}}", UTF8_BOM);
        assert_eq!(strip_bom(&with_bom), "{}");
    }

    #[test]
    fn test_no_bom() {
        assert_eq!(strip_bom("{}"), "{}");
    }

    #[test]
    fn test_empty_string() {
        assert_eq!(strip_bom(""), "");
    }
}
