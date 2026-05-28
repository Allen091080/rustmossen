//! # intl — 国际化工具（共享 Intl 对象实例，惰性初始化）
//!
//! 对应 TypeScript `utils/intl.ts`。

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::collections::HashMap;
use unicode_segmentation::UnicodeSegmentation;

/// 图素分割器（Rust 中使用 unicode-segmentation crate）
/// 无需显式缓存，因为 Rust 的 UnicodeSegmentation 是零成本 trait
/// 提取字符串的第一个字素簇
pub fn first_grapheme(text: &str) -> &str {
    if text.is_empty() {
        return "";
    }
    text.graphemes(true).next().unwrap_or("")
}

/// 提取字符串的最后一个字素簇
pub fn last_grapheme(text: &str) -> &str {
    if text.is_empty() {
        return "";
    }
    text.graphemes(true).next_back().unwrap_or("")
}

/// 将字符串按词分割
pub fn segment_words(text: &str) -> Vec<&str> {
    text.unicode_words().collect()
}

/// 相对时间格式化样式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RelativeTimeStyle {
    Long,
    Short,
    Narrow,
}

/// 相对时间数字格式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RelativeTimeNumeric {
    Always,
    Auto,
}

/// 相对时间单位
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelativeTimeUnit {
    Seconds,
    Minutes,
    Hours,
    Days,
    Weeks,
    Months,
    Years,
}

/// 缓存键
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct RtfKey {
    style: RelativeTimeStyle,
    numeric: RelativeTimeNumeric,
}

/// RelativeTimeFormat 缓存
static RTF_CACHE: Lazy<Mutex<HashMap<RtfKey, ()>>> = Lazy::new(|| Mutex::new(HashMap::new()));

/// 格式化相对时间（简化实现，返回英文描述）
pub fn format_relative_time(
    value: i64,
    unit: RelativeTimeUnit,
    style: RelativeTimeStyle,
    numeric: RelativeTimeNumeric,
) -> String {
    // 确保缓存条目存在（模拟 JS 缓存行为）
    let key = RtfKey { style, numeric };
    RTF_CACHE.lock().entry(key).or_insert(());

    let unit_str = match (unit, style) {
        (RelativeTimeUnit::Seconds, RelativeTimeStyle::Long) => {
            if value.abs() == 1 {
                "second"
            } else {
                "seconds"
            }
        }
        (RelativeTimeUnit::Seconds, RelativeTimeStyle::Short | RelativeTimeStyle::Narrow) => "sec",
        (RelativeTimeUnit::Minutes, RelativeTimeStyle::Long) => {
            if value.abs() == 1 {
                "minute"
            } else {
                "minutes"
            }
        }
        (RelativeTimeUnit::Minutes, RelativeTimeStyle::Short | RelativeTimeStyle::Narrow) => "min",
        (RelativeTimeUnit::Hours, RelativeTimeStyle::Long) => {
            if value.abs() == 1 {
                "hour"
            } else {
                "hours"
            }
        }
        (RelativeTimeUnit::Hours, RelativeTimeStyle::Short | RelativeTimeStyle::Narrow) => "hr",
        (RelativeTimeUnit::Days, RelativeTimeStyle::Long) => {
            if value.abs() == 1 {
                "day"
            } else {
                "days"
            }
        }
        (RelativeTimeUnit::Days, RelativeTimeStyle::Short | RelativeTimeStyle::Narrow) => "day",
        (RelativeTimeUnit::Weeks, RelativeTimeStyle::Long) => {
            if value.abs() == 1 {
                "week"
            } else {
                "weeks"
            }
        }
        (RelativeTimeUnit::Weeks, RelativeTimeStyle::Short | RelativeTimeStyle::Narrow) => "wk",
        (RelativeTimeUnit::Months, RelativeTimeStyle::Long) => {
            if value.abs() == 1 {
                "month"
            } else {
                "months"
            }
        }
        (RelativeTimeUnit::Months, RelativeTimeStyle::Short | RelativeTimeStyle::Narrow) => "mo",
        (RelativeTimeUnit::Years, RelativeTimeStyle::Long) => {
            if value.abs() == 1 {
                "year"
            } else {
                "years"
            }
        }
        (RelativeTimeUnit::Years, RelativeTimeStyle::Short | RelativeTimeStyle::Narrow) => "yr",
    };

    let abs_val = value.abs();
    match numeric {
        RelativeTimeNumeric::Always => {
            if value < 0 {
                format!("{} {} ago", abs_val, unit_str)
            } else {
                format!("in {} {}", abs_val, unit_str)
            }
        }
        RelativeTimeNumeric::Auto => {
            if value == -1 && unit == RelativeTimeUnit::Days {
                "yesterday".to_string()
            } else if value == 1 && unit == RelativeTimeUnit::Days {
                "tomorrow".to_string()
            } else if value < 0 {
                format!("{} {} ago", abs_val, unit_str)
            } else {
                format!("in {} {}", abs_val, unit_str)
            }
        }
    }
}

/// 获取系统时区
pub fn get_time_zone() -> String {
    // 尝试从环境变量获取时区，否则默认 UTC
    std::env::var("TZ").unwrap_or_else(|_| "UTC".to_string())
}

/// 缓存的时区值
static CACHED_TIME_ZONE: Lazy<String> = Lazy::new(get_time_zone);

/// 获取缓存的时区
pub fn get_cached_time_zone() -> &'static str {
    &CACHED_TIME_ZONE
}

/// 获取系统区域语言子标签（如 'en', 'ja'）
pub fn get_system_locale_language() -> Option<&'static str> {
    static CACHED: Lazy<Option<String>> = Lazy::new(|| {
        // 从 LANG 或 LC_ALL 环境变量获取
        let locale = std::env::var("LANG")
            .or_else(|_| std::env::var("LC_ALL"))
            .ok()?;
        // 提取语言子标签（第一个 '_' 或 '-' 或 '.' 之前的部分）
        let lang = locale.split(['_', '-', '.']).next().unwrap_or(&locale);
        Some(lang.to_string())
    });
    CACHED.as_deref()
}

/// 对应 TS `getGraphemeSegmenter`：返回 grapheme 分段函数。
pub fn get_grapheme_segmenter() -> impl Fn(&str) -> Vec<String> {
    |text: &str| {
        use unicode_segmentation::UnicodeSegmentation;
        text.graphemes(true).map(|g| g.to_string()).collect()
    }
}

/// 对应 TS `getWordSegmenter`：返回 word 分段函数。
pub fn get_word_segmenter() -> impl Fn(&str) -> Vec<String> {
    |text: &str| {
        use unicode_segmentation::UnicodeSegmentation;
        text.unicode_words().map(|w| w.to_string()).collect()
    }
}

/// 对应 TS `getRelativeTimeFormat`：返回相对时间格式化函数。
pub fn get_relative_time_format() -> impl Fn(i64, &str) -> String {
    |value: i64, unit: &str| format!("{} {}", value, unit)
}
