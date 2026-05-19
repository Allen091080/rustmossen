//! # keyboard_shortcuts — 键盘快捷键工具
//!
//! 对应 TypeScript `utils/keyboardShortcuts.ts`。
//! macOS Option+key 特殊字符映射。

use std::collections::HashMap;

/// macOS Option+key 特殊字符映射表。
/// 用于检测未启用 "Option as Meta" 的 macOS 终端上的 Option+key 快捷键。
pub const MACOS_OPTION_SPECIAL_CHARS: &[(&str, &str)] = &[
    ("†", "alt+t"),  // Option+T -> thinking toggle
    ("π", "alt+p"),  // Option+P -> model picker
    ("ø", "alt+o"),  // Option+O -> fast mode
];

/// 转换为 HashMap 以便快速查找。
pub fn get_macos_option_special_chars_map() -> HashMap<String, String> {
    MACOS_OPTION_SPECIAL_CHARS
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// 判断字符是否为 macOS Option 特殊字符。
pub fn is_macos_option_char(char: &str) -> bool {
    MACOS_OPTION_SPECIAL_CHARS.iter().any(|(k, _)| *k == char)
}

/// 获取字符对应的快捷键名称。
pub fn get_macos_option_shortcut(char: &str) -> Option<&'static str> {
    MACOS_OPTION_SPECIAL_CHARS
        .iter()
        .find(|(k, _)| *k == char)
        .map(|(_, v)| *v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_macos_option_char() {
        assert!(is_macos_option_char("†"));
        assert!(is_macos_option_char("π"));
        assert!(is_macos_option_char("ø"));
        assert!(!is_macos_option_char("a"));
    }

    #[test]
    fn test_get_macos_option_shortcut() {
        assert_eq!(get_macos_option_shortcut("†"), Some("alt+t"));
        assert_eq!(get_macos_option_shortcut("π"), Some("alt+p"));
        assert_eq!(get_macos_option_shortcut("ø"), Some("alt+o"));
        assert_eq!(get_macos_option_shortcut("x"), None);
    }
}
