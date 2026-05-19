//! Time and duration formatting utilities.
//!
//! Mirrors the TS `format.ts` — file size formatting, duration formatting,
//! relative time display, and number formatting.

use std::time::Duration;

// ---------------------------------------------------------------------------
// File size formatting
// ---------------------------------------------------------------------------

/// Formats a byte count to a human-readable string (KB, MB, GB).
pub fn format_file_size(size_in_bytes: u64) -> String {
    let kb = size_in_bytes as f64 / 1024.0;
    if kb < 1.0 {
        return format!("{size_in_bytes} bytes");
    }
    if kb < 1024.0 {
        return format_decimal(kb, "KB");
    }
    let mb = kb / 1024.0;
    if mb < 1024.0 {
        return format_decimal(mb, "MB");
    }
    let gb = mb / 1024.0;
    format_decimal(gb, "GB")
}

fn format_decimal(value: f64, suffix: &str) -> String {
    let formatted = format!("{:.1}", value);
    let formatted = formatted.trim_end_matches(".0");
    format!("{formatted}{suffix}")
}

// ---------------------------------------------------------------------------
// Duration formatting
// ---------------------------------------------------------------------------

/// Format milliseconds as seconds with 1 decimal place (e.g. `1234` → `"1.2s"`).
pub fn format_seconds_short(ms: u64) -> String {
    format!("{:.1}s", ms as f64 / 1000.0)
}

/// Options for `format_duration`.
#[derive(Debug, Clone, Copy, Default)]
pub struct FormatDurationOptions {
    pub hide_trailing_zeros: bool,
    pub most_significant_only: bool,
}

/// Format a duration in milliseconds to a human-readable string.
pub fn format_duration(ms: u64, opts: FormatDurationOptions) -> String {
    if ms < 60_000 {
        if ms == 0 {
            return "0s".to_string();
        }
        let s = ms / 1000;
        return format!("{s}s");
    }

    let mut days = ms / 86_400_000;
    let mut hours = (ms % 86_400_000) / 3_600_000;
    let mut minutes = (ms % 3_600_000) / 60_000;
    let mut seconds = ((ms % 60_000) as f64 / 1000.0).round() as u64;

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
            return format!("{days}d");
        }
        if hours > 0 {
            return format!("{hours}h");
        }
        if minutes > 0 {
            return format!("{minutes}m");
        }
        return format!("{seconds}s");
    }

    if days > 0 {
        if hide && hours == 0 && minutes == 0 {
            return format!("{days}d");
        }
        if hide && minutes == 0 {
            return format!("{days}d {hours}h");
        }
        return format!("{days}d {hours}h {minutes}m");
    }
    if hours > 0 {
        if hide && minutes == 0 && seconds == 0 {
            return format!("{hours}h");
        }
        if hide && seconds == 0 {
            return format!("{hours}h {minutes}m");
        }
        return format!("{hours}h {minutes}m {seconds}s");
    }
    if minutes > 0 {
        if hide && seconds == 0 {
            return format!("{minutes}m");
        }
        return format!("{minutes}m {seconds}s");
    }
    format!("{seconds}s")
}

/// Convenience: format a `std::time::Duration`.
pub fn format_std_duration(d: Duration, opts: FormatDurationOptions) -> String {
    format_duration(d.as_millis() as u64, opts)
}

// ---------------------------------------------------------------------------
// Number formatting
// ---------------------------------------------------------------------------

/// Format a number in compact notation (1321 → "1.3k", 900 → "900").
pub fn format_number(n: u64) -> String {
    if n >= 1_000_000_000 {
        format!("{:.1}b", n as f64 / 1_000_000_000.0)
    } else if n >= 1_000_000 {
        format!("{:.1}m", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

/// Format token count (compact, strip trailing .0).
pub fn format_tokens(count: u64) -> String {
    format_number(count).replace(".0", "")
}

// ---------------------------------------------------------------------------
// Timestamp helpers
// ---------------------------------------------------------------------------

/// Convert a `chrono::DateTime` to a filename-safe string (ISO 8601 with
/// colons and dots replaced by dashes).
pub fn date_to_filename(dt: &chrono::DateTime<chrono::Utc>) -> String {
    dt.format("%Y-%m-%dT%H-%M-%S-%3fZ").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_file_size() {
        assert_eq!(format_file_size(512), "512 bytes");
        assert_eq!(format_file_size(1536), "1.5KB");
        assert_eq!(format_file_size(1_048_576), "1MB");
        assert_eq!(format_file_size(1_073_741_824), "1GB");
    }

    #[test]
    fn test_format_duration_simple() {
        let opts = FormatDurationOptions::default();
        assert_eq!(format_duration(0, opts), "0s");
        assert_eq!(format_duration(5000, opts), "5s");
        assert_eq!(format_duration(65_000, opts), "1m 5s");
        assert_eq!(format_duration(3_661_000, opts), "1h 1m 1s");
    }

    #[test]
    fn test_format_number() {
        assert_eq!(format_number(900), "900");
        assert_eq!(format_number(1321), "1.3k");
        assert_eq!(format_number(1_500_000), "1.5m");
    }
}
