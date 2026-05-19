//! # stale_session — 过期会话检测
//!
//! 对应 TypeScript `utils/staleSession.ts`。
//! 纯函数，无 IO，无 React。

use chrono::{DateTime, Utc};

/// 会话被视为过期的天数阈值。
pub const STALE_SESSION_THRESHOLD_DAYS: i64 = 7;

const MS_PER_DAY: i64 = 24 * 60 * 60 * 1000;

/// 计算会话自上次修改以来的天数。
///
/// 向下取整，6.9 天报告为 6。
pub fn get_stale_session_age_days(modified: DateTime<Utc>, now: DateTime<Utc>) -> i64 {
    let age_ms = (now - modified).num_milliseconds();
    if age_ms <= 0 {
        return 0;
    }
    age_ms / MS_PER_DAY
}

/// 会话是否已过期（超过阈值天数未修改）。
pub fn is_session_stale(modified: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    get_stale_session_age_days(modified, now) >= STALE_SESSION_THRESHOLD_DAYS
}
