//! # display_tags — 显示标签剥离
//!
//! 对应 TypeScript `utils/displayTags.ts`。

use regex::Regex;
use std::sync::LazyLock;

/// 匹配 XML 风格的 `<tag>…</tag>` 块。
static XML_TAG_BLOCK_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"<([a-z][\w-]*)(?:\s[^>]*)?>[\s\S]*?</\1>\n?").unwrap()
});

/// 仅匹配 IDE 注入的上下文标签。
static IDE_CONTEXT_TAGS_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"<(ide_opened_file|ide_selection)(?:\s[^>]*)?>[\s\S]*?</\1>\n?").unwrap()
});

/// 从文本中剥离 XML 风格标签块，用于 UI 标题。
///
/// 如果剥离后为空文本，返回原文不变。
pub fn strip_display_tags(text: &str) -> String {
    let result = XML_TAG_BLOCK_PATTERN.replace_all(text, "");
    let trimmed = result.trim();
    if trimmed.is_empty() {
        text.to_string()
    } else {
        trimmed.to_string()
    }
}

/// 类似 strip_display_tags 但允许返回空字符串。
pub fn strip_display_tags_allow_empty(text: &str) -> String {
    XML_TAG_BLOCK_PATTERN.replace_all(text, "").trim().to_string()
}

/// 仅剥离 IDE 注入的上下文标签（ide_opened_file, ide_selection）。
pub fn strip_ide_context_tags(text: &str) -> String {
    IDE_CONTEXT_TAGS_PATTERN.replace_all(text, "").trim().to_string()
}
