//! # hyperlink — 终端超链接
//!
//! 对应 TypeScript `utils/hyperlink.ts`。

/// OSC 8 超链接转义序列起始。
pub const OSC8_START: &str = "\x1b]8;;";
/// OSC 8 超链接转义序列结束。
pub const OSC8_END: &str = "\x07";

/// 创建可点击的 OSC 8 超链接。
///
/// 如果终端不支持超链接，则回退为纯文本 URL。
///
/// - `url`: 链接的 URL
/// - `content`: 可选的显示文本（仅在支持超链接时生效）
/// - `supports_hyperlinks`: 是否支持超链接
pub fn create_hyperlink(url: &str, content: Option<&str>, supports_hyperlinks: bool) -> String {
    if !supports_hyperlinks {
        return url.to_string();
    }

    let display_text = content.unwrap_or(url);
    // 应用基本 ANSI 蓝色
    let colored_text = format!("\x1b[34m{}\x1b[0m", display_text);
    format!("{}{}{}{}{}{}", OSC8_START, url, OSC8_END, colored_text, OSC8_START, OSC8_END)
}
