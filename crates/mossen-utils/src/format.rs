//! Pure display formatters — leaf-safe.
//!
//! Formats file sizes, durations, numbers, relative times, and log metadata.

use chrono::{DateTime, Datelike, Local, Utc};

/// Format a byte count to a human-readable string (KB, MB, GB).
pub fn format_file_size(size_in_bytes: u64) -> String {
    let kb = size_in_bytes as f64 / 1024.0;
    if kb < 1.0 {
        return format!("{} bytes", size_in_bytes);
    }
    if kb < 1024.0 {
        let s = format!("{:.1}", kb);
        let s = s.trim_end_matches(".0");
        return format!("{}KB", s);
    }
    let mb = kb / 1024.0;
    if mb < 1024.0 {
        let s = format!("{:.1}", mb);
        let s = s.trim_end_matches(".0");
        return format!("{}MB", s);
    }
    let gb = mb / 1024.0;
    let s = format!("{:.1}", gb);
    let s = s.trim_end_matches(".0");
    format!("{}GB", s)
}

/// Format milliseconds as seconds with 1 decimal place.
pub fn format_seconds_short(ms: f64) -> String {
    format!("{:.1}s", ms / 1000.0)
}

/// Options for format_duration.
#[derive(Debug, Clone, Default)]
pub struct FormatDurationOptions {
    pub hide_trailing_zeros: bool,
    pub most_significant_only: bool,
}

/// Format a duration in milliseconds to a human-readable string.
pub fn format_duration(ms: f64, options: Option<&FormatDurationOptions>) -> String {
    let opts = options.cloned().unwrap_or_default();

    if ms < 60000.0 {
        if ms == 0.0 {
            return "0s".to_string();
        }
        if ms < 1.0 {
            return format!("{:.1}s", ms / 1000.0);
        }
        let s = (ms / 1000.0).floor() as u64;
        return format!("{}s", s);
    }

    let mut days = (ms / 86400000.0).floor() as u64;
    let mut hours = ((ms % 86400000.0) / 3600000.0).floor() as u64;
    let mut minutes = ((ms % 3600000.0) / 60000.0).floor() as u64;
    let mut seconds = ((ms % 60000.0) / 1000.0).round() as u64;

    // Handle rounding carry-over
    if seconds == 60 {
        seconds = 0;
        minutes += 1;
    }
    if minutes == 60 {
        minutes = 0;
        hours += 1;
    }
    if hours == 24 {
        hours = 0;
        days += 1;
    }

    let hide = opts.hide_trailing_zeros;

    if opts.most_significant_only {
        if days > 0 {
            return format!("{}d", days);
        }
        if hours > 0 {
            return format!("{}h", hours);
        }
        if minutes > 0 {
            return format!("{}m", minutes);
        }
        return format!("{}s", seconds);
    }

    if days > 0 {
        if hide && hours == 0 && minutes == 0 {
            return format!("{}d", days);
        }
        if hide && minutes == 0 {
            return format!("{}d {}h", days, hours);
        }
        return format!("{}d {}h {}m", days, hours, minutes);
    }
    if hours > 0 {
        if hide && minutes == 0 && seconds == 0 {
            return format!("{}h", hours);
        }
        if hide && seconds == 0 {
            return format!("{}h {}m", hours, minutes);
        }
        return format!("{}h {}m {}s", hours, minutes, seconds);
    }
    if minutes > 0 {
        if hide && seconds == 0 {
            return format!("{}m", minutes);
        }
        return format!("{}m {}s", minutes, seconds);
    }
    format!("{}s", seconds)
}

/// Format a number in compact notation (like 1.3k, 2.1M).
pub fn format_number(number: f64) -> String {
    if number.abs() >= 1_000_000_000.0 {
        let v = number / 1_000_000_000.0;
        format!("{:.1}b", v).to_lowercase()
    } else if number.abs() >= 1_000_000.0 {
        let v = number / 1_000_000.0;
        format!("{:.1}m", v).to_lowercase()
    } else if number.abs() >= 1000.0 {
        let v = number / 1000.0;
        format!("{:.1}k", v).to_lowercase()
    } else {
        format!("{}", number as i64)
    }
}

/// Format token count (compact, strip trailing .0).
pub fn format_tokens(count: u64) -> String {
    format_number(count as f64).replace(".0", "")
}

/// Style for relative time formatting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelativeTimeStyle {
    Long,
    Short,
    Narrow,
}

/// Options for relative time formatting.
#[derive(Debug, Clone)]
pub struct RelativeTimeOptions {
    pub style: RelativeTimeStyle,
    pub numeric: RelativeTimeNumeric,
    pub now: Option<DateTime<Local>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelativeTimeNumeric {
    Always,
    Auto,
}

impl Default for RelativeTimeOptions {
    fn default() -> Self {
        Self {
            style: RelativeTimeStyle::Narrow,
            numeric: RelativeTimeNumeric::Always,
            now: None,
        }
    }
}

/// Time interval definitions.
struct TimeInterval {
    unit: &'static str,
    seconds: i64,
    short_unit: &'static str,
}

const INTERVALS: &[TimeInterval] = &[
    TimeInterval { unit: "year", seconds: 31536000, short_unit: "y" },
    TimeInterval { unit: "month", seconds: 2592000, short_unit: "mo" },
    TimeInterval { unit: "week", seconds: 604800, short_unit: "w" },
    TimeInterval { unit: "day", seconds: 86400, short_unit: "d" },
    TimeInterval { unit: "hour", seconds: 3600, short_unit: "h" },
    TimeInterval { unit: "minute", seconds: 60, short_unit: "m" },
    TimeInterval { unit: "second", seconds: 1, short_unit: "s" },
];

/// Format a relative time difference.
pub fn format_relative_time(
    date: &DateTime<Local>,
    options: &RelativeTimeOptions,
) -> String {
    let now = options.now.unwrap_or_else(Local::now);
    let diff_ms = date.signed_duration_since(now).num_milliseconds();
    let diff_in_seconds = diff_ms / 1000; // truncate towards zero

    for interval in INTERVALS {
        if diff_in_seconds.abs() >= interval.seconds {
            let value = diff_in_seconds / interval.seconds;
            if options.style == RelativeTimeStyle::Narrow {
                return if diff_in_seconds < 0 {
                    format!("{}{} ago", value.abs(), interval.short_unit)
                } else {
                    format!("in {}{}", value, interval.short_unit)
                };
            }
            // Long style
            let unit_str = if value.abs() == 1 {
                interval.unit.to_string()
            } else {
                format!("{}s", interval.unit)
            };
            return if value < 0 {
                format!("{} {} ago", value.abs(), unit_str)
            } else {
                format!("in {} {}", value, unit_str)
            };
        }
    }

    // Less than 1 second
    if options.style == RelativeTimeStyle::Narrow {
        return if diff_in_seconds <= 0 {
            "0s ago".to_string()
        } else {
            "in 0s".to_string()
        };
    }
    if diff_in_seconds <= 0 {
        "0 seconds ago".to_string()
    } else {
        "in 0 seconds".to_string()
    }
}

/// Format a relative time that happened in the past.
pub fn format_relative_time_ago(
    date: &DateTime<Local>,
    options: &RelativeTimeOptions,
) -> String {
    let now = options.now.unwrap_or_else(Local::now);
    if *date > now {
        return format_relative_time(date, options);
    }
    let mut opts = options.clone();
    opts.numeric = RelativeTimeNumeric::Always;
    format_relative_time(date, &opts)
}

/// Log metadata for display.
pub struct LogMetadata {
    pub modified: DateTime<Local>,
    pub message_count: usize,
    pub file_size: Option<u64>,
    pub git_branch: Option<String>,
    pub tag: Option<String>,
    pub agent_setting: Option<String>,
    pub pr_number: Option<u64>,
    pub pr_repository: Option<String>,
}

/// Format log metadata for display.
pub fn format_log_metadata(log: &LogMetadata) -> String {
    let size_or_count = if let Some(size) = log.file_size {
        format_file_size(size)
    } else {
        format!("{} messages", log.message_count)
    };

    let mut parts = vec![
        format_relative_time_ago(
            &log.modified,
            &RelativeTimeOptions {
                style: RelativeTimeStyle::Short,
                ..Default::default()
            },
        ),
    ];

    if let Some(ref branch) = log.git_branch {
        parts.push(branch.clone());
    }
    parts.push(size_or_count);

    if let Some(ref tag) = log.tag {
        parts.push(format!("#{}", tag));
    }
    if let Some(ref agent) = log.agent_setting {
        parts.push(format!("@{}", agent));
    }
    if let Some(pr) = log.pr_number {
        if let Some(ref repo) = log.pr_repository {
            parts.push(format!("{}#{}", repo, pr));
        } else {
            parts.push(format!("#{}", pr));
        }
    }

    parts.join(" · ")
}

/// Format a reset time (timestamp in seconds) to a locale-like string.
pub fn format_reset_time(
    timestamp_in_seconds: Option<u64>,
    show_timezone: bool,
    show_time: bool,
) -> Option<String> {
    let ts = timestamp_in_seconds?;
    if ts == 0 {
        return None;
    }

    let date = DateTime::from_timestamp(ts as i64, 0)?
        .with_timezone(&Local);
    let now = Local::now();
    let hours_until_reset =
        (date.signed_duration_since(now).num_seconds() as f64) / 3600.0;

    let tz_suffix = if show_timezone {
        format!(" ({})", now.format("%Z"))
    } else {
        String::new()
    };

    if hours_until_reset > 24.0 {
        // Show date + optional time for resets more than a day away
        if show_time {
            let minutes = date.format("%M").to_string();
            let fmt = if minutes == "00" {
                date.format("%b %-d %-I%P").to_string()
            } else {
                date.format("%b %-d %-I:%M%P").to_string()
            };
            // Add year if different
            let result = if date.year() != now.year() {
                format!("{} {}", date.format("%b %-d, %Y %-I:%M%P"), tz_suffix.trim())
            } else {
                format!("{}{}", fmt, tz_suffix)
            };
            Some(result)
        } else {
            let result = if date.year() != now.year() {
                format!("{}{}", date.format("%b %-d, %Y"), tz_suffix)
            } else {
                format!("{}{}", date.format("%b %-d"), tz_suffix)
            };
            Some(result)
        }
    } else {
        // For resets within 24h, show just the time
        let minutes = date.format("%M").to_string();
        let time_str = if minutes == "00" {
            date.format("%-I%P").to_string()
        } else {
            date.format("%-I:%M%P").to_string()
        };
        Some(format!("{}{}", time_str, tz_suffix))
    }
}

/// Format reset text from an ISO date string.
pub fn format_reset_text(resets_at: &str, show_timezone: bool, show_time: bool) -> String {
    if let Ok(dt) = resets_at.parse::<DateTime<Utc>>() {
        let ts = dt.timestamp() as u64;
        format_reset_time(Some(ts), show_timezone, show_time)
            .unwrap_or_default()
    } else {
        String::new()
    }
}
