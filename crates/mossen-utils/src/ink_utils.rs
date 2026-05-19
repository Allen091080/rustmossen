//! # ink_utils — Ink 颜色转换
//!
//! 对应 TypeScript `utils/ink.ts`。

use std::collections::HashMap;
use std::sync::LazyLock;

static AGENT_COLOR_TO_THEME_COLOR: LazyLock<HashMap<&'static str, &'static str>> =
    LazyLock::new(|| {
        let mut m = HashMap::new();
        m.insert("blue", "blue_FOR_SUBAGENTS_ONLY");
        m.insert("green", "green_FOR_SUBAGENTS_ONLY");
        m.insert("yellow", "yellow_FOR_SUBAGENTS_ONLY");
        m.insert("magenta", "magenta_FOR_SUBAGENTS_ONLY");
        m.insert("cyan", "cyan_FOR_SUBAGENTS_ONLY");
        m.insert("red", "red_FOR_SUBAGENTS_ONLY");
        m
    });

const DEFAULT_AGENT_THEME_COLOR: &str = "cyan_FOR_SUBAGENTS_ONLY";

/// 将颜色字符串转换为 Ink TextProps 的 color 格式。
///
/// 已知的 agent 颜色名映射为 theme key，未知颜色回退到原始 ANSI 颜色。
pub fn to_ink_color(color: Option<&str>) -> String {
    match color {
        None => DEFAULT_AGENT_THEME_COLOR.to_string(),
        Some(c) => {
            if let Some(theme_color) = AGENT_COLOR_TO_THEME_COLOR.get(c) {
                theme_color.to_string()
            } else {
                format!("ansi:{}", c)
            }
        }
    }
}
