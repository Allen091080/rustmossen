//! # token_budget — Token 预算解析
//!
//! 对应 TypeScript `utils/tokenBudget.ts`。
//! 从用户输入中解析 token 预算指令。

use regex::Regex;
use std::sync::LazyLock;

/// 倍数映射
fn multiplier(suffix: &str) -> f64 {
    match suffix.to_lowercase().as_str() {
        "k" => 1_000.0,
        "m" => 1_000_000.0,
        "b" => 1_000_000_000.0,
        _ => 1.0,
    }
}

/// 解析匹配的预算数值
fn parse_budget_match(value: &str, suffix: &str) -> Option<f64> {
    value.parse::<f64>().ok().map(|v| v * multiplier(suffix))
}

static SHORTHAND_START_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^\s*\+(\d+(?:\.\d+)?)\s*(k|m|b)\b").unwrap());

static SHORTHAND_END_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\s\+(\d+(?:\.\d+)?)\s*(k|m|b)\s*[.!?]?\s*$").unwrap());

static VERBOSE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(?:use|spend)\s+(\d+(?:\.\d+)?)\s*(k|m|b)\s*tokens?\b").unwrap());

/// 从文本中解析 token 预算。
///
/// 支持格式：
/// - 简写开头: "+500k"
/// - 简写结尾: "... +2m"
/// - 详细格式: "use 500k tokens"
///
/// 返回 None 如果未找到预算指令。
pub fn parse_token_budget(text: &str) -> Option<f64> {
    if let Some(caps) = SHORTHAND_START_RE.captures(text) {
        return parse_budget_match(&caps[1], &caps[2]);
    }
    if let Some(caps) = SHORTHAND_END_RE.captures(text) {
        return parse_budget_match(&caps[1], &caps[2]);
    }
    if let Some(caps) = VERBOSE_RE.captures(text) {
        return parse_budget_match(&caps[1], &caps[2]);
    }
    None
}

/// 查找 token 预算在文本中的位置
pub fn find_token_budget_positions(text: &str) -> Vec<(usize, usize)> {
    let mut positions = Vec::new();

    if let Some(m) = SHORTHAND_START_RE.find(text) {
        let trimmed_start = m.start() + m.as_str().len() - m.as_str().trim_start().len();
        positions.push((trimmed_start, m.end()));
    }

    if let Some(m) = SHORTHAND_END_RE.find(text) {
        let end_start = m.start() + 1; // +1: regex 包含前导 \s
        let already_covered = positions.iter().any(|(s, e)| end_start >= *s && end_start < *e);
        if !already_covered {
            positions.push((end_start, m.end()));
        }
    }

    for m in VERBOSE_RE.find_iter(text) {
        positions.push((m.start(), m.end()));
    }

    positions
}

/// 获取预算继续消息
pub fn get_budget_continuation_message(pct: u32, turn_tokens: u64, budget: u64) -> String {
    fn fmt(n: u64) -> String {
        let s = n.to_string();
        let mut result = String::new();
        for (i, c) in s.chars().rev().enumerate() {
            if i > 0 && i % 3 == 0 {
                result.push(',');
            }
            result.push(c);
        }
        result.chars().rev().collect()
    }

    format!(
        "Stopped at {}% of token target ({} / {}). Keep working \u{2014} do not summarize.",
        pct,
        fmt(turn_tokens),
        fmt(budget)
    )
}
