//! # format_brief_timestamp — 简报时间戳格式化
//!
//! 对应 TypeScript `utils/formatBriefTimestamp.ts`。
//! 将 ISO 时间戳格式化为适合消息标签的简要展示。

use chrono::{DateTime, Datelike, Local, NaiveDate, Utc};

/// 格式化 ISO 时间戳为简要显示。
///
/// 显示规则基于时间差：
///   - 当天:      "13:30"
///   - 6天内:     "Sunday, 16:15"
///   - 更早:      "Sunday, Feb 20, 16:30"
///
/// `now` 参数可注入用于测试。
pub fn format_brief_timestamp(iso_string: &str, now: Option<DateTime<Local>>) -> String {
    let now = now.unwrap_or_else(Local::now);

    let parsed = match iso_string.parse::<DateTime<Utc>>() {
        Ok(dt) => dt.with_timezone(&Local),
        Err(_) => return String::new(),
    };

    let days_ago = {
        let now_date = now.date_naive();
        let parsed_date = parsed.date_naive();
        (now_date - parsed_date).num_days()
    };

    if days_ago == 0 {
        // 当天: "13:30"
        parsed.format("%H:%M").to_string()
    } else if days_ago > 0 && days_ago < 7 {
        // 7天内: "Sunday, 16:15"
        parsed.format("%A, %H:%M").to_string()
    } else {
        // 更早: "Sunday, Feb 20, 16:30"
        parsed.format("%A, %b %d, %H:%M").to_string()
    }
}

/// 从 POSIX 环境变量推导 locale 信息。
/// LC_ALL > LC_TIME > LANG，回退到 None（系统默认）。
/// 将 POSIX 格式 (en_GB.UTF-8) 转换为 BCP 47 (en-GB)。
pub fn get_locale() -> Option<String> {
    let raw = std::env::var("LC_ALL")
        .or_else(|_| std::env::var("LC_TIME"))
        .or_else(|_| std::env::var("LANG"))
        .unwrap_or_default();

    if raw.is_empty() || raw == "C" || raw == "POSIX" {
        return None;
    }

    // 去除 codeset (.UTF-8) 和 modifier (@euro)，将 _ 替换为 -
    let base = raw.split('.').next().unwrap_or("");
    let base = base.split('@').next().unwrap_or("");

    if base.is_empty() {
        return None;
    }

    let tag = base.replace('_', "-");
    Some(tag)
}

/// 获取日期的开始时间（零点）
fn start_of_day(dt: &DateTime<Local>) -> NaiveDate {
    dt.date_naive()
}
