//! Minimal cron expression parsing and next-run calculation.
//!
//! Supports the standard 5-field cron subset:
//!   minute hour day-of-month month day-of-week
//!
//! Field syntax: wildcard, N, step (*/N), range (N-M), list (N,M,...).
//! No L, W, ?, or name aliases. All times are interpreted in the process's
//! local timezone.

use chrono::{Datelike, Local, NaiveDate, NaiveTime, TimeZone, Timelike};
use std::collections::HashSet;

/// Expanded cron fields.
#[derive(Debug, Clone)]
pub struct CronFields {
    pub minute: Vec<u32>,
    pub hour: Vec<u32>,
    pub day_of_month: Vec<u32>,
    pub month: Vec<u32>,
    pub day_of_week: Vec<u32>,
}

struct FieldRange {
    min: u32,
    max: u32,
}

const FIELD_RANGES: [FieldRange; 5] = [
    FieldRange { min: 0, max: 59 }, // minute
    FieldRange { min: 0, max: 23 }, // hour
    FieldRange { min: 1, max: 31 }, // dayOfMonth
    FieldRange { min: 1, max: 12 }, // month
    FieldRange { min: 0, max: 6 },  // dayOfWeek (0=Sunday; 7 accepted as Sunday alias)
];

/// Day names for human-readable output.
const DAY_NAMES: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

/// Parse a single cron field into a sorted array of matching values.
fn expand_field(field: &str, range: &FieldRange) -> Option<Vec<u32>> {
    let min = range.min;
    let max = range.max;
    let mut out = HashSet::new();
    let is_dow = min == 0 && max == 6;

    for part in field.split(',') {
        // wildcard or */N
        if part.starts_with('*') {
            let step = if part.contains('/') {
                let parts: Vec<&str> = part.splitn(2, '/').collect();
                parts[1].parse::<u32>().ok()?
            } else {
                1
            };
            if step < 1 {
                return None;
            }
            let mut i = min;
            while i <= max {
                out.insert(i);
                i += step;
            }
            continue;
        }

        // N-M or N-M/S
        if part.contains('-') {
            let (range_part, step) = if part.contains('/') {
                let parts: Vec<&str> = part.splitn(2, '/').collect();
                (parts[0], parts[1].parse::<u32>().ok()?)
            } else {
                (part, 1u32)
            };
            let bounds: Vec<&str> = range_part.splitn(2, '-').collect();
            if bounds.len() != 2 {
                return None;
            }
            let lo = bounds[0].parse::<u32>().ok()?;
            let hi = bounds[1].parse::<u32>().ok()?;
            let eff_max = if is_dow { 7 } else { max };
            if lo > hi || step < 1 || lo < min || hi > eff_max {
                return None;
            }
            let mut i = lo;
            while i <= hi {
                let val = if is_dow && i == 7 { 0 } else { i };
                out.insert(val);
                i += step;
            }
            continue;
        }

        // plain N
        if let Ok(mut n) = part.parse::<u32>() {
            if is_dow && n == 7 {
                n = 0;
            }
            if n < min || n > max {
                return None;
            }
            out.insert(n);
        } else {
            return None;
        }
    }

    if out.is_empty() {
        return None;
    }
    let mut result: Vec<u32> = out.into_iter().collect();
    result.sort();
    Some(result)
}

/// Parse a 5-field cron expression into expanded number arrays.
/// Returns None if invalid or unsupported syntax.
pub fn parse_cron_expression(expr: &str) -> Option<CronFields> {
    let parts: Vec<&str> = expr.trim().split_whitespace().collect();
    if parts.len() != 5 {
        return None;
    }

    let mut expanded = Vec::with_capacity(5);
    for i in 0..5 {
        let result = expand_field(parts[i], &FIELD_RANGES[i])?;
        expanded.push(result);
    }

    Some(CronFields {
        minute: expanded[0].clone(),
        hour: expanded[1].clone(),
        day_of_month: expanded[2].clone(),
        month: expanded[3].clone(),
        day_of_week: expanded[4].clone(),
    })
}

/// Compute the next Date strictly after `from` that matches the cron fields,
/// using the process's local timezone.
pub fn compute_next_cron_run(
    fields: &CronFields,
    from: &chrono::DateTime<Local>,
) -> Option<chrono::DateTime<Local>> {
    let minute_set: HashSet<u32> = fields.minute.iter().copied().collect();
    let hour_set: HashSet<u32> = fields.hour.iter().copied().collect();
    let dom_set: HashSet<u32> = fields.day_of_month.iter().copied().collect();
    let month_set: HashSet<u32> = fields.month.iter().copied().collect();
    let dow_set: HashSet<u32> = fields.day_of_week.iter().copied().collect();

    let dom_wild = fields.day_of_month.len() == 31;
    let dow_wild = fields.day_of_week.len() == 7;

    // Round up to next whole minute (strictly after `from`)
    let mut t = *from;
    t = t
        .with_second(0)
        .unwrap_or(t)
        .with_nanosecond(0)
        .unwrap_or(t);
    t = t + chrono::Duration::minutes(1);

    let max_iter = 366 * 24 * 60;
    for _ in 0..max_iter {
        let month = t.month();
        if !month_set.contains(&month) {
            // Jump to start of next month
            let (next_year, next_month) = if month == 12 {
                (t.year() + 1, 1)
            } else {
                (t.year(), month + 1)
            };
            if let Some(nd) = NaiveDate::from_ymd_opt(next_year, next_month, 1) {
                let nt = nd.and_hms_opt(0, 0, 0)?;
                t = Local.from_local_datetime(&nt).single()?;
            } else {
                return None;
            }
            continue;
        }

        let dom = t.day();
        let dow = t.weekday().num_days_from_sunday();
        let day_matches = if dom_wild && dow_wild {
            true
        } else if dom_wild {
            dow_set.contains(&dow)
        } else if dow_wild {
            dom_set.contains(&dom)
        } else {
            dom_set.contains(&dom) || dow_set.contains(&dow)
        };

        if !day_matches {
            // Jump to start of next day
            let next_day = t.date_naive().succ_opt()?;
            let nt = next_day.and_hms_opt(0, 0, 0)?;
            t = Local.from_local_datetime(&nt).single()?;
            continue;
        }

        if !hour_set.contains(&t.hour()) {
            t = t + chrono::Duration::hours(1);
            t = t.with_minute(0).unwrap_or(t).with_second(0).unwrap_or(t);
            continue;
        }

        if !minute_set.contains(&t.minute()) {
            t = t + chrono::Duration::minutes(1);
            continue;
        }

        return Some(t);
    }

    None
}

/// Format a local time from hour and minute.
fn format_local_time(minute: u32, hour: u32) -> String {
    let _time = NaiveTime::from_hms_opt(hour, minute, 0).unwrap_or_default();
    let ampm = if hour < 12 { "AM" } else { "PM" };
    let h12 = if hour == 0 {
        12
    } else if hour > 12 {
        hour - 12
    } else {
        hour
    };
    if minute == 0 {
        format!("{}:00 {}", h12, ampm)
    } else {
        format!("{}:{:02} {}", h12, minute, ampm)
    }
}

/// Format a UTC time as local time string.
fn format_utc_time_as_local(minute: u32, hour: u32) -> String {
    use chrono::Utc;
    let now = Utc::now();
    let dt = now
        .date_naive()
        .and_hms_opt(hour, minute, 0)
        .map(|naive| chrono::Utc.from_utc_datetime(&naive).with_timezone(&Local));

    match dt {
        Some(local_dt) => {
            let h = local_dt.hour();
            let m = local_dt.minute();
            let ampm = if h < 12 { "AM" } else { "PM" };
            let h12 = if h == 0 {
                12
            } else if h > 12 {
                h - 12
            } else {
                h
            };
            let tz = local_dt.format("%Z");
            if m == 0 {
                format!("{}:00 {} {}", h12, ampm, tz)
            } else {
                format!("{}:{:02} {} {}", h12, m, ampm, tz)
            }
        }
        None => format_local_time(minute, hour),
    }
}

/// Convert a cron expression to a human-readable description.
pub fn cron_to_human(cron: &str, utc: bool) -> String {
    let parts: Vec<&str> = cron.trim().split_whitespace().collect();
    if parts.len() != 5 {
        return cron.to_string();
    }

    let minute = parts[0];
    let hour = parts[1];
    let day_of_month = parts[2];
    let month = parts[3];
    let day_of_week = parts[4];

    // Every N minutes: */N * * * *
    if minute.starts_with("*/")
        && hour == "*"
        && day_of_month == "*"
        && month == "*"
        && day_of_week == "*"
    {
        if let Ok(n) = minute[2..].parse::<u32>() {
            return if n == 1 {
                "Every minute".to_string()
            } else {
                format!("Every {} minutes", n)
            };
        }
    }

    // Every hour: N * * * *
    if minute.parse::<u32>().is_ok()
        && hour == "*"
        && day_of_month == "*"
        && month == "*"
        && day_of_week == "*"
    {
        let m: u32 = minute.parse().unwrap_or(0);
        if m == 0 {
            return "Every hour".to_string();
        }
        return format!("Every hour at :{:02}", m);
    }

    // Every N hours: M */N * * *
    if minute.parse::<u32>().is_ok()
        && hour.starts_with("*/")
        && day_of_month == "*"
        && month == "*"
        && day_of_week == "*"
    {
        if let Ok(n) = hour[2..].parse::<u32>() {
            let m: u32 = minute.parse().unwrap_or(0);
            let suffix = if m == 0 {
                String::new()
            } else {
                format!(" at :{:02}", m)
            };
            return if n == 1 {
                format!("Every hour{}", suffix)
            } else {
                format!("Every {} hours{}", n, suffix)
            };
        }
    }

    // Remaining cases need numeric hour and minute
    let m_val = match minute.parse::<u32>() {
        Ok(v) => v,
        Err(_) => return cron.to_string(),
    };
    let h_val = match hour.parse::<u32>() {
        Ok(v) => v,
        Err(_) => return cron.to_string(),
    };

    let fmt_time = if utc {
        format_utc_time_as_local(m_val, h_val)
    } else {
        format_local_time(m_val, h_val)
    };

    // Daily: M H * * *
    if day_of_month == "*" && month == "*" && day_of_week == "*" {
        return format!("Every day at {}", fmt_time);
    }

    // Specific day of week: M H * * D
    if day_of_month == "*" && month == "*" && day_of_week.len() == 1 {
        if let Ok(day_index) = day_of_week.parse::<u32>() {
            let idx = (day_index % 7) as usize;
            if utc {
                // Compute actual local weekday
                let now = chrono::Utc::now();
                let current_dow = now.weekday().num_days_from_sunday();
                let days_to_add = ((idx as i32 - current_dow as i32) + 7) % 7;
                let target = now + chrono::Duration::days(days_to_add as i64);
                let target = target
                    .date_naive()
                    .and_hms_opt(h_val, m_val, 0)
                    .map(|naive| chrono::Utc.from_utc_datetime(&naive).with_timezone(&Local));
                if let Some(local_dt) = target {
                    let local_dow = local_dt.weekday().num_days_from_sunday() as usize;
                    if local_dow < DAY_NAMES.len() {
                        return format!("Every {} at {}", DAY_NAMES[local_dow], fmt_time);
                    }
                }
            }
            if idx < DAY_NAMES.len() {
                return format!("Every {} at {}", DAY_NAMES[idx], fmt_time);
            }
        }
    }

    // Weekdays: M H * * 1-5
    if day_of_month == "*" && month == "*" && day_of_week == "1-5" {
        return format!("Weekdays at {}", fmt_time);
    }

    cron.to_string()
}
