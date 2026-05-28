//! # Common utilities (common.ts)
//!
//! 日期工具函数。

use chrono::Local;
use once_cell::sync::Lazy;

/// Get the local date in ISO format (YYYY-MM-DD).
/// Checks for `MOSSEN_CODE_OVERRIDE_DATE` env var first.
pub fn get_local_iso_date() -> String {
    if let Ok(override_date) = std::env::var("MOSSEN_CODE_OVERRIDE_DATE") {
        if !override_date.is_empty() {
            return override_date;
        }
    }
    Local::now().format("%Y-%m-%d").to_string()
}

/// Memoized for prompt-cache stability — captures the date once at session start.
/// The main interactive path gets this behavior via memoize(getUserContext) in
/// context.ts; simple mode (--bare) calls getSystemPrompt per-request and needs
/// an explicit memoized date to avoid busting the cached prefix at midnight.
pub static SESSION_START_DATE: Lazy<String> = Lazy::new(get_local_iso_date);

/// Returns `get_session_start_date()` — the date captured at session start.
pub fn get_session_start_date() -> &'static str {
    &SESSION_START_DATE
}

/// Returns "Month YYYY" (e.g. "February 2026") in the user's local timezone.
/// Changes monthly, not daily — used in tool prompts to minimize cache busting.
pub fn get_local_month_year() -> String {
    if let Ok(override_date) = std::env::var("MOSSEN_CODE_OVERRIDE_DATE") {
        if !override_date.is_empty() {
            // Parse the override date and format as "Month YYYY"
            if let Ok(parsed) = chrono::NaiveDate::parse_from_str(&override_date, "%Y-%m-%d") {
                return parsed.format("%B %Y").to_string();
            }
        }
    }
    Local::now().format("%B %Y").to_string()
}
