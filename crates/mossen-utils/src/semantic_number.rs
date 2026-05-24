//! # semantic_number — 语义数字解析
//!
//! 对应 TypeScript `utils/semanticNumber.ts`。

use regex::Regex;
use std::sync::LazyLock;

static NUMBER_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^-?\d+(\.\d+)?$").unwrap());

/// 将值解析为语义数字。
///
/// 接受数字或匹配十进制数字格式的字符串（如 "30", "-5", "3.14"）。
/// 模型偶尔引用数字参数，此函数处理这种情况。
pub fn parse_semantic_number(value: &serde_json::Value) -> Option<f64> {
    match value {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => {
            if NUMBER_PATTERN.is_match(s) {
                s.parse::<f64>().ok().filter(|n| n.is_finite())
            } else {
                None
            }
        }
        _ => None,
    }
}

/// 从字符串解析语义数字。
pub fn semantic_number_from_str(s: &str) -> Option<f64> {
    if NUMBER_PATTERN.is_match(s) {
        s.parse::<f64>().ok().filter(|n| n.is_finite())
    } else {
        None
    }
}

/// 对应 TS 默认导出 `semanticNumber(inner)`：把一个值规范化为数字（接受数字
/// 或符合 `^-?\d+(\.\d+)?$` 的字符串）。返回 `None` 表示原值无法被 coerce。
pub fn semantic_number(value: &serde_json::Value) -> Option<f64> {
    parse_semantic_number(value)
}
