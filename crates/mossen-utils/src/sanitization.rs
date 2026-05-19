//! # sanitization — Unicode 清理（隐藏字符攻击防护）
//!
//! 对应 TypeScript `utils/sanitization.ts`。

use regex::Regex;
use once_cell::sync::Lazy;
use unicode_normalization::UnicodeNormalization;

/// 最大迭代次数，防止无限循环
const MAX_ITERATIONS: usize = 10;

// 预编译正则表达式
static RE_ZERO_WIDTH: Lazy<Regex> = Lazy::new(|| Regex::new(r"[\u{200B}-\u{200F}]").unwrap());
static RE_DIRECTIONAL: Lazy<Regex> = Lazy::new(|| Regex::new(r"[\u{202A}-\u{202E}]").unwrap());
static RE_ISOLATES: Lazy<Regex> = Lazy::new(|| Regex::new(r"[\u{2066}-\u{2069}]").unwrap());
static RE_BOM: Lazy<Regex> = Lazy::new(|| Regex::new(r"\u{FEFF}").unwrap());
static RE_PRIVATE_USE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[\u{E000}-\u{F8FF}]").unwrap());

/// 对字符串进行部分 Unicode 清理。
///
/// 实现针对 Unicode 隐藏字符攻击的安全措施，特别是 ASCII 走私和隐藏提示注入漏洞。
/// 这些攻击使用不可见 Unicode 字符（如标签字符、格式控制、私有使用区和非字符）
/// 来隐藏对用户不可见但被 AI 模型处理的恶意指令。
pub fn partially_sanitize_unicode(prompt: &str) -> Result<String, SanitizationError> {
    let mut current = prompt.to_string();
    let mut previous = String::new();
    let mut iterations = 0;

    // 迭代清理直到没有更多更改或达到最大迭代次数
    while current != previous && iterations < MAX_ITERATIONS {
        previous = current.clone();

        // 应用 NFKC 规范化以处理组合字符序列
        current = current.nfkc().collect::<String>();

        // 方法 1：移除危险的 Unicode 类别（Cf=格式控制, Co=私有使用, Cn=未分配）
        current = current
            .chars()
            .filter(|c| {
                let cp = *c as u32;
                // Format characters (Cf) - common ranges
                let is_format = matches!(cp,
                    0x00AD | 0x0600..=0x0605 | 0x061C | 0x06DD | 0x070F |
                    0x08E2 | 0x180E | 0x200B..=0x200F | 0x202A..=0x202E |
                    0x2060..=0x2064 | 0x2066..=0x206F | 0xFEFF | 0xFFF9..=0xFFFB |
                    0x110BD | 0x110CD | 0x13430..=0x13438 | 0x1BCA0..=0x1BCA3 |
                    0x1D173..=0x1D17A | 0xE0001 | 0xE0020..=0xE007F
                );
                // Private Use (Co)
                let is_private_use = matches!(cp,
                    0xE000..=0xF8FF | 0xF0000..=0xFFFFD | 0x100000..=0x10FFFD
                );
                // Tags block (used in ASCII smuggling attacks)
                let is_tag = matches!(cp, 0xE0000..=0xE007F);
                !is_format && !is_private_use && !is_tag
            })
            .collect();

        // 方法 2：显式字符范围（某些环境中正则不支持 Unicode 属性类，作为后备）
        current = RE_ZERO_WIDTH.replace_all(&current, "").to_string();
        current = RE_DIRECTIONAL.replace_all(&current, "").to_string();
        current = RE_ISOLATES.replace_all(&current, "").to_string();
        current = RE_BOM.replace_all(&current, "").to_string();
        current = RE_PRIVATE_USE.replace_all(&current, "").to_string();

        iterations += 1;
    }

    // 如果达到最大迭代次数，返回错误
    if iterations >= MAX_ITERATIONS {
        return Err(SanitizationError::MaxIterationsReached {
            max: MAX_ITERATIONS,
            input_preview: prompt.chars().take(100).collect(),
        });
    }

    Ok(current)
}

/// 清理错误类型
#[derive(Debug, Clone)]
pub enum SanitizationError {
    MaxIterationsReached { max: usize, input_preview: String },
}

impl std::fmt::Display for SanitizationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SanitizationError::MaxIterationsReached { max, input_preview } => {
                write!(
                    f,
                    "Unicode sanitization reached maximum iterations ({}) for input: {}",
                    max, input_preview
                )
            }
        }
    }
}

impl std::error::Error for SanitizationError {}

/// 递归清理 JSON 值中的 Unicode
pub fn recursively_sanitize_unicode(
    value: serde_json::Value,
) -> Result<serde_json::Value, SanitizationError> {
    match value {
        serde_json::Value::String(s) => {
            Ok(serde_json::Value::String(partially_sanitize_unicode(&s)?))
        }
        serde_json::Value::Array(arr) => {
            let sanitized: Result<Vec<_>, _> =
                arr.into_iter().map(recursively_sanitize_unicode).collect();
            Ok(serde_json::Value::Array(sanitized?))
        }
        serde_json::Value::Object(map) => {
            let mut sanitized = serde_json::Map::new();
            for (key, val) in map {
                let sanitized_key = partially_sanitize_unicode(&key)?;
                let sanitized_val = recursively_sanitize_unicode(val)?;
                sanitized.insert(sanitized_key, sanitized_val);
            }
            Ok(serde_json::Value::Object(sanitized))
        }
        // 其他原始值（数字、布尔、null）保持不变
        other => Ok(other),
    }
}
