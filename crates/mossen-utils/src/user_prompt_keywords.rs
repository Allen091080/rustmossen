//! # user_prompt_keywords — 用户提示关键词匹配
//!
//! 对应 TypeScript `utils/userPromptKeywords.ts`。

use regex::Regex;
use std::sync::LazyLock;

static NEGATIVE_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(wtf|wth|ffs|omfg|shit(ty|tiest)?|dumbass|horrible|awful|piss(ed|ing)? off|piece of (shit|crap|junk)|what the (fuck|hell)|fucking? (broken|useless|terrible|awful|horrible)|fuck you|screw (this|you)|so frustrating|this sucks|damn it)\b").unwrap()
});

static KEEP_GOING_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(keep going|go on)\b").unwrap()
});

/// 检查输入是否匹配负面关键词模式。
pub fn matches_negative_keyword(input: &str) -> bool {
    NEGATIVE_PATTERN.is_match(input)
}

/// 检查输入是否匹配继续/保持运行的关键词模式。
pub fn matches_keep_going_keyword(input: &str) -> bool {
    let lower = input.to_lowercase();
    let trimmed = lower.trim();

    // 如果整个提示就是 "continue"
    if trimmed == "continue" {
        return true;
    }

    // 在输入中匹配 "keep going" 或 "go on"
    KEEP_GOING_PATTERN.is_match(&lower)
}
