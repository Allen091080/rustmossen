//! Rate limit messages — centralized message generation

use super::hosted_limits::{QuotaStatus, RateLimitType};

/// All possible rate limit error message prefixes
pub const RATE_LIMIT_ERROR_PREFIXES: &[&str] = &[
    "You've hit your",
    "You've used",
    "You're now using extra usage",
    "You're now using hosted usage",
    "You're close to",
    "You're out of extra usage",
    "You're out of hosted usage",
];

/// Check if a message is a rate limit error
pub fn is_rate_limit_error_message(text: &str) -> bool {
    RATE_LIMIT_ERROR_PREFIXES
        .iter()
        .any(|prefix| text.starts_with(prefix))
}

/// Rate limit message with severity
#[derive(Debug, Clone)]
pub struct RateLimitMessage {
    pub message: String,
    pub severity: RateLimitSeverity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RateLimitSeverity {
    Error,
    Warning,
}

/// Hosted limits state
#[derive(Debug, Clone)]
pub struct HostedLimitsState {
    pub status: QuotaStatus,
    pub resets_at: Option<u64>,
    pub rate_limit_type: Option<RateLimitType>,
    pub utilization: Option<f64>,
    pub overage_status: Option<QuotaStatus>,
    pub overage_resets_at: Option<u64>,
    pub overage_disabled_reason: Option<String>,
    pub is_using_overage: bool,
}

fn get_hosted_usage_label() -> &'static str {
    if std::env::var("CUSTOM_BACKEND_ENABLED").is_ok() {
        "hosted usage"
    } else {
        "extra usage"
    }
}

fn get_rate_limit_display_name(t: &RateLimitType) -> &'static str {
    match t {
        RateLimitType::FiveHour => "session limit",
        RateLimitType::SevenDay => "weekly limit",
        RateLimitType::SevenDayMax => "Max limit",
        RateLimitType::SevenDayBalanced => "Balanced limit",
        RateLimitType::Overage => "extra usage limit",
    }
}

fn format_reset_time(resets_at: u64) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let diff = resets_at.saturating_sub(now);
    let hours = diff / 3600;
    let minutes = (diff % 3600) / 60;
    if hours > 24 {
        format!("in {} days", hours / 24)
    } else if hours > 0 {
        format!("in {}h {}m", hours, minutes)
    } else if minutes > 0 {
        format!("in {}m", minutes)
    } else {
        "soon".to_string()
    }
}

/// Get the appropriate rate limit message
pub fn get_rate_limit_message(
    limits: &HostedLimitsState,
    _model: &str,
) -> Option<RateLimitMessage> {
    // Check overage scenarios first
    if limits.is_using_overage {
        if limits.overage_status.as_ref() == Some(&QuotaStatus::AllowedWarning) {
            return Some(RateLimitMessage {
                message: format!(
                    "You're close to your {} spending limit",
                    get_hosted_usage_label()
                ),
                severity: RateLimitSeverity::Warning,
            });
        }
        return None;
    }

    // ERROR STATES
    if limits.status == QuotaStatus::Rejected {
        let msg = get_limit_reached_text(limits);
        return Some(RateLimitMessage {
            message: msg,
            severity: RateLimitSeverity::Error,
        });
    }

    // WARNING STATES
    if limits.status == QuotaStatus::AllowedWarning {
        if let Some(util) = limits.utilization {
            if util < 0.7 {
                return None;
            }
        }
        if let Some(text) = get_early_warning_text(limits) {
            return Some(RateLimitMessage {
                message: text,
                severity: RateLimitSeverity::Warning,
            });
        }
    }

    None
}

/// Get error message for API errors
pub fn get_rate_limit_error_message(limits: &HostedLimitsState, model: &str) -> Option<String> {
    let msg = get_rate_limit_message(limits, model)?;
    if msg.severity == RateLimitSeverity::Error {
        Some(msg.message)
    } else {
        None
    }
}

/// Get warning message for UI
pub fn get_rate_limit_warning(limits: &HostedLimitsState, model: &str) -> Option<String> {
    let msg = get_rate_limit_message(limits, model)?;
    if msg.severity == RateLimitSeverity::Warning {
        Some(msg.message)
    } else {
        None
    }
}

fn get_limit_reached_text(limits: &HostedLimitsState) -> String {
    let reset_message = limits
        .resets_at
        .map(|r| format!(" · resets {}", format_reset_time(r)))
        .unwrap_or_default();

    if limits.overage_status.as_ref() == Some(&QuotaStatus::Rejected) {
        let overage_reset = limits.overage_resets_at.map(|r| format_reset_time(r));
        let best_reset = match (limits.resets_at, limits.overage_resets_at) {
            (Some(a), Some(b)) => {
                if a < b {
                    format!(" · resets {}", format_reset_time(a))
                } else {
                    format!(" · resets {}", format_reset_time(b))
                }
            }
            (Some(a), None) => format!(" · resets {}", format_reset_time(a)),
            (None, Some(b)) => format!(" · resets {}", format_reset_time(b)),
            (None, None) => String::new(),
        };

        if limits.overage_disabled_reason.as_deref() == Some("out_of_credits") {
            return format!("You're out of {}{}", get_hosted_usage_label(), best_reset);
        }
        return format_limit_reached("limit", &best_reset);
    }

    if let Some(ref rlt) = limits.rate_limit_type {
        let limit_name = match rlt {
            RateLimitType::SevenDayBalanced => "Balanced limit",
            RateLimitType::SevenDayMax => "Max limit",
            RateLimitType::SevenDay => "weekly limit",
            RateLimitType::FiveHour => "session limit",
            _ => "usage limit",
        };
        return format_limit_reached(limit_name, &reset_message);
    }

    format_limit_reached("usage limit", &reset_message)
}

fn get_early_warning_text(limits: &HostedLimitsState) -> Option<String> {
    let limit_name = match limits.rate_limit_type.as_ref()? {
        RateLimitType::SevenDay => "weekly limit",
        RateLimitType::FiveHour => "session limit",
        RateLimitType::SevenDayMax => "Max limit",
        RateLimitType::SevenDayBalanced => "Balanced limit",
        RateLimitType::Overage => get_hosted_usage_label(),
    };

    let used = limits.utilization.map(|u| (u * 100.0) as u32);
    let reset_time = limits.resets_at.map(|r| format_reset_time(r));

    match (used, reset_time) {
        (Some(pct), Some(rt)) => Some(format!(
            "You've used {}% of your {} · resets {}",
            pct, limit_name, rt
        )),
        (Some(pct), None) => Some(format!("You've used {}% of your {}", pct, limit_name)),
        (None, Some(rt)) => Some(format!("Approaching {} · resets {}", limit_name, rt)),
        (None, None) => Some(format!("Approaching {}", limit_name)),
    }
}

fn format_limit_reached(limit: &str, reset_message: &str) -> String {
    if std::env::var("USER_TYPE").as_deref() == Ok("internal") {
        format!(
            "You've hit your {}{}. If you have feedback about this limit, post in #briarpatch-cc. You can reset your limits with /reset-limits",
            limit, reset_message
        )
    } else {
        format!("You've hit your {}{}", limit, reset_message)
    }
}

/// Get notification text for overage mode transitions
pub fn get_using_overage_text(limits: &HostedLimitsState) -> String {
    let reset_time = limits.resets_at.map(|r| format_reset_time(r));

    let limit_name = match limits.rate_limit_type.as_ref() {
        Some(RateLimitType::FiveHour) => Some("session limit"),
        Some(RateLimitType::SevenDay) => Some("weekly limit"),
        Some(RateLimitType::SevenDayMax) => Some("Max limit"),
        Some(RateLimitType::SevenDayBalanced) => Some("Balanced limit"),
        _ => None,
    };

    match (limit_name, reset_time) {
        (Some(name), Some(rt)) => format!(
            "You're now using {} · Your {} resets {}",
            get_hosted_usage_label(),
            name,
            rt
        ),
        (Some(_), None) | (None, _) => {
            format!("Now using {}", get_hosted_usage_label())
        }
    }
}
