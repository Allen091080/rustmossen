//! `/rate-limit-options` — View and configure rate limit settings.
//!
//! Shows current rate limit status and allows configuration of
//! retry behavior, backoff strategies, and limit notifications.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};
use mossen_agent::services::root::hosted_limits::{
    get_current_limits, get_rate_limit_display_name, QuotaStatus,
};

/// Rate limit options command — rate limiting configuration.
///
/// Displays:
/// - Current rate limit status (requests remaining)
/// - Time until limit reset
/// - Retry policy configuration
/// - Backoff strategy settings (exponential, linear, fixed)
/// - Notification preferences for approaching limits
pub struct RateLimitDirective;

/// Backoff strategy options.
const BACKOFF_STRATEGIES: &[(&str, &str)] = &[
    ("exponential", "Double wait time between retries (default)"),
    ("linear", "Fixed increment between retries"),
    ("fixed", "Same wait time between all retries"),
];

#[async_trait]
impl Directive for RateLimitDirective {
    fn name(&self) -> &str {
        "rate-limit-options"
    }

    fn description(&self) -> &str {
        "View and configure rate limit settings"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn is_immediate(&self) -> bool {
        true
    }

    fn is_enabled(&self, ctx: &CommandContext) -> bool {
        ctx.can_use_hosted_platform_features()
            || ctx.is_env_truthy("MOSSEN_ENABLE_RATE_LIMIT_COMMAND")
    }

    async fn execute(&self, args: &[&str], _ctx: &CommandContext) -> Result<CommandResult> {
        if args
            .first()
            .map(|a| matches!(*a, "help" | "-h" | "--help"))
            .unwrap_or(false)
        {
            let mut help = String::from(
                "Usage: /rate-limit-options [setting] [value]\n\n                 View and configure how rate limits are handled.\n\n                 Settings:\n                   backoff <strategy>    Set backoff strategy\n                   threshold <percent>   Set notification threshold\n                   auto-retry <on|off>   Toggle auto-retry\n\n                 Backoff strategies:\n",
            );
            for (name, desc) in BACKOFF_STRATEGIES {
                help.push_str(&format!("  {:14} {}\n", name, desc));
            }
            return Ok(CommandResult::Text(help));
        }

        let limits = get_current_limits();
        let has_snapshot = limits.rate_limit_type.is_some()
            || limits.remaining_tokens.is_some()
            || limits.reset_at.is_some()
            || limits.warning_message.is_some()
            || limits.status != QuotaStatus::Allowed
            || limits.is_overage;

        let mut output = String::from("Rate Limit Status\n=================\n\n");
        if !has_snapshot {
            output.push_str("No API rate-limit snapshot has been recorded yet.\n");
            output.push_str(
                "This local build will show live limits only after the API layer records them.\n",
            );
            return Ok(CommandResult::Text(output));
        }

        output.push_str(&format!(
            "Status:            {}\n",
            quota_status_label(&limits.status)
        ));
        if let Some(rate_limit_type) = limits.rate_limit_type.as_ref() {
            output.push_str(&format!(
                "Limit type:        {}\n",
                get_rate_limit_display_name(rate_limit_type)
            ));
        }
        if let Some(remaining) = limits.remaining_tokens {
            output.push_str(&format!("Tokens remaining:  {remaining}\n"));
        } else {
            output.push_str("Tokens remaining:  not provided by API\n");
        }
        if let Some(reset_at) = limits.reset_at {
            output.push_str(&format!(
                "Reset:             {}\n",
                format_reset_duration(reset_at)
            ));
        }
        if limits.is_overage {
            output.push_str("Overage:           active\n");
        }
        if let Some(message) = limits.warning_message.as_ref() {
            output.push_str(&format!("Message:           {message}\n"));
        }

        Ok(CommandResult::Text(output))
    }
}

fn quota_status_label(status: &QuotaStatus) -> &'static str {
    match status {
        QuotaStatus::Allowed => "allowed",
        QuotaStatus::AllowedWarning => "warning",
        QuotaStatus::Rejected => "rejected",
    }
}

fn format_reset_duration(reset_at: std::time::Instant) -> String {
    let now = std::time::Instant::now();
    if reset_at <= now {
        "now".to_string()
    } else {
        let seconds = reset_at.duration_since(now).as_secs();
        if seconds < 60 {
            format!("{seconds}s")
        } else if seconds < 3_600 {
            format!("{}m", seconds / 60)
        } else {
            format!("{}h {}m", seconds / 3_600, (seconds % 3_600) / 60)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::CommandContext;
    use mossen_agent::services::root::hosted_limits::{
        reset_rate_limits, update_rate_limits, RateLimitType,
    };
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};

    fn hosted_limit_test_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .expect("hosted limit test lock poisoned")
    }

    fn test_context() -> CommandContext {
        CommandContext {
            cwd: PathBuf::from("."),
            is_non_interactive: true,
            is_remote_mode: false,
            is_custom_backend: false,
            user_type: None,
            env_vars: HashMap::new(),
            product_name: "Mossen".to_string(),
            cli_name: "mossen".to_string(),
            version: "test".to_string(),
            build_time: None,
            cost_snapshot: Default::default(),
        }
    }

    fn text(result: CommandResult) -> String {
        match result {
            CommandResult::Text(text) => text,
            other => panic!("unexpected result: {other:?}"),
        }
    }

    #[test]
    fn rate_limit_does_not_print_fake_unlimited_state_without_snapshot() {
        let _guard = hosted_limit_test_lock();
        reset_rate_limits();

        let output = text(
            tokio_test::block_on(RateLimitDirective.execute(&[], &test_context()))
                .expect("rate limit command"),
        );

        assert!(output.contains("No API rate-limit snapshot"), "{output}");
        assert!(!output.to_lowercase().contains("hosted"), "{output}");
        assert!(!output.contains("unlimited"), "{output}");
        assert!(!output.contains("Upgrade your plan"), "{output}");
    }

    #[test]
    fn rate_limit_reports_recorded_snapshot() {
        let _guard = hosted_limit_test_lock();
        reset_rate_limits();
        update_rate_limits(
            QuotaStatus::AllowedWarning,
            Some(RateLimitType::FiveHour),
            Some(123),
            Some(60),
        );

        let output = text(
            tokio_test::block_on(RateLimitDirective.execute(&[], &test_context()))
                .expect("rate limit command"),
        );

        assert!(output.contains("Status:            warning"), "{output}");
        assert!(output.contains("5-hour rate limit"), "{output}");
        assert!(output.contains("Tokens remaining:  123"), "{output}");
        reset_rate_limits();
    }
}
