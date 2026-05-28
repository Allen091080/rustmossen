//! Remote-managed-settings security-check workflow.
//!
//! Translates `services/remoteManagedSettings/securityCheck.tsx`. The JSX
//! portion (dialog rendering) lives in the TUI crate; this Rust module
//! exposes the orchestration logic only.

use serde_json::Value;

/// TS `type SecurityCheckResult = 'approved' | 'rejected' | 'no_check_needed'`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityCheckResult {
    Approved,
    Rejected,
    NoCheckNeeded,
}

/// Run the security check over a new settings payload. Returns
/// `NoCheckNeeded` when:
///   * `new_settings` is `None`, OR
///   * `has_dangerous_settings(new)` is false, OR
///   * `dangerous_settings_changed(cached, new)` is false, OR
///   * `is_interactive` is false (matches the TS trust-dialog short-circuit).
///
/// Otherwise the caller must invoke the dialog (the TUI side) and feed the
/// user's choice back as `dialog_choice` (`Some(true)` = approved,
/// `Some(false)` = rejected). When `dialog_choice` is `None` and the gate
/// would otherwise require a check, the function defaults to `Rejected`
/// (fail-closed).
pub fn check_managed_settings_security(
    cached_settings: Option<&Value>,
    new_settings: Option<&Value>,
    is_interactive: bool,
    has_dangerous_settings: impl Fn(Option<&Value>) -> bool,
    dangerous_settings_changed: impl Fn(Option<&Value>, Option<&Value>) -> bool,
    dialog_choice: Option<bool>,
) -> SecurityCheckResult {
    if new_settings.is_none() || !has_dangerous_settings(new_settings) {
        return SecurityCheckResult::NoCheckNeeded;
    }
    if !dangerous_settings_changed(cached_settings, new_settings) {
        return SecurityCheckResult::NoCheckNeeded;
    }
    if !is_interactive {
        return SecurityCheckResult::NoCheckNeeded;
    }
    match dialog_choice {
        Some(true) => SecurityCheckResult::Approved,
        Some(false) => SecurityCheckResult::Rejected,
        None => SecurityCheckResult::Rejected,
    }
}

/// TS `handleSecurityCheckResult` — returns `true` when the caller should
/// continue, `false` when it should stop (rejected). The TS variant also
/// calls `gracefulShutdownSync(1)` on rejection; the Rust port leaves that
/// side-effect to the caller because the agent runtime owns the lifecycle.
pub fn handle_security_check_result(result: SecurityCheckResult) -> bool {
    !matches!(result, SecurityCheckResult::Rejected)
}
