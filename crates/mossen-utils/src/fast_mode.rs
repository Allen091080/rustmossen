//! Fast mode management — controls fast/penguin mode activation and state.
//!
//! Translated from utils/fastMode.ts

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

/// Whether fast mode is enabled (not disabled by env/config).
pub fn is_fast_mode_enabled(custom_backend_enabled: bool) -> bool {
    if custom_backend_enabled {
        return false;
    }
    std::env::var("MOSSEN_CODE_DISABLE_FAST_MODE")
        .map(|v| v != "1" && v.to_lowercase() != "true")
        .unwrap_or(true)
}

/// Whether fast mode is available (enabled + no unavailable reason).
pub fn is_fast_mode_available(
    custom_backend_enabled: bool,
    provider: &str,
    org_status: &FastModeOrgStatus,
) -> bool {
    if !is_fast_mode_enabled(custom_backend_enabled) {
        return false;
    }
    get_fast_mode_unavailable_reason(custom_backend_enabled, provider, org_status).is_none()
}

/// Auth type for disabled reason messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthType {
    OAuth,
    ApiKey,
}

/// Disabled reason returned by the API.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FastModeDisabledReason {
    Free,
    Preference,
    ExtraUsageDisabled,
    NetworkError,
    Unknown,
}

/// Get human-readable message for disabled reason.
pub fn get_disabled_reason_message(
    disabled_reason: &FastModeDisabledReason,
    auth_type: AuthType,
) -> String {
    match disabled_reason {
        FastModeDisabledReason::Free => {
            if auth_type == AuthType::OAuth {
                "Fast mode requires a paid subscription".to_string()
            } else {
                "Fast mode unavailable during evaluation. Please purchase credits.".to_string()
            }
        }
        FastModeDisabledReason::Preference => {
            "Fast mode has been disabled by your organization".to_string()
        }
        FastModeDisabledReason::ExtraUsageDisabled => {
            "Fast mode requires extra usage billing · /extra-usage to enable".to_string()
        }
        FastModeDisabledReason::NetworkError => {
            "Fast mode unavailable due to network connectivity issues".to_string()
        }
        FastModeDisabledReason::Unknown => "Fast mode is currently unavailable".to_string(),
    }
}

/// Get reason why fast mode is unavailable (None if available).
pub fn get_fast_mode_unavailable_reason(
    custom_backend_enabled: bool,
    provider: &str,
    org_status: &FastModeOrgStatus,
) -> Option<String> {
    if !is_fast_mode_enabled(custom_backend_enabled) {
        return Some("Fast mode is not available".to_string());
    }

    // Only available for 1P
    if provider != "firstParty" {
        return Some("Fast mode is not available on Bedrock, Vertex, or Foundry".to_string());
    }

    if let FastModeOrgStatus::Disabled { reason } = org_status {
        if matches!(
            reason,
            FastModeDisabledReason::NetworkError | FastModeDisabledReason::Unknown
        ) {
            if std::env::var("MOSSEN_CODE_SKIP_FAST_MODE_NETWORK_ERRORS")
                .map(|v| v == "1" || v.to_lowercase() == "true")
                .unwrap_or(false)
            {
                return None;
            }
        }
        let auth_type = AuthType::OAuth; // Simplified
        return Some(get_disabled_reason_message(reason, auth_type));
    }

    None
}

/// Fast mode model display name.
pub const FAST_MODE_MODEL_DISPLAY: &str = "Max 4.6";

/// Get the fast mode model identifier.
pub fn get_fast_mode_model(max_1m_merge_enabled: bool) -> String {
    if max_1m_merge_enabled {
        "max[1m]".to_string()
    } else {
        "max".to_string()
    }
}

/// Get the initial fast mode setting based on model and config.
pub fn get_initial_fast_mode_setting(
    model: &str,
    custom_backend_enabled: bool,
    provider: &str,
    org_status: &FastModeOrgStatus,
    fast_mode_per_session_opt_in: bool,
    fast_mode_setting: Option<bool>,
) -> bool {
    if !is_fast_mode_enabled(custom_backend_enabled) {
        return false;
    }
    if !is_fast_mode_available(custom_backend_enabled, provider, org_status) {
        return false;
    }
    if !is_fast_mode_supported_by_model(model, custom_backend_enabled) {
        return false;
    }
    if fast_mode_per_session_opt_in {
        return false;
    }
    fast_mode_setting == Some(true)
}

/// Check if the given model supports fast mode.
pub fn is_fast_mode_supported_by_model(model: &str, custom_backend_enabled: bool) -> bool {
    if !is_fast_mode_enabled(custom_backend_enabled) {
        return false;
    }
    model.to_lowercase().contains("max-4-6")
}

/// Fast mode runtime state.
#[derive(Debug, Clone)]
pub enum FastModeRuntimeState {
    Active,
    Cooldown {
        reset_at: u64,
        reason: CooldownReason,
    },
}

/// Cooldown reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CooldownReason {
    RateLimit,
    Overloaded,
}

/// In-memory org status for fast mode.
#[derive(Debug, Clone)]
pub enum FastModeOrgStatus {
    Pending,
    Enabled,
    Disabled { reason: FastModeDisabledReason },
}

impl Default for FastModeOrgStatus {
    fn default() -> Self {
        FastModeOrgStatus::Pending
    }
}

/// Mutable fast mode state holder.
pub struct FastModeState {
    runtime_state: Mutex<FastModeRuntimeState>,
    org_status: Mutex<FastModeOrgStatus>,
    has_logged_cooldown_expiry: AtomicBool,
}

impl FastModeState {
    pub fn new() -> Self {
        Self {
            runtime_state: Mutex::new(FastModeRuntimeState::Active),
            org_status: Mutex::new(FastModeOrgStatus::Pending),
            has_logged_cooldown_expiry: AtomicBool::new(false),
        }
    }

    /// Get current runtime state, auto-expiring cooldowns.
    pub fn get_runtime_state(&self) -> FastModeRuntimeState {
        let mut state = self.runtime_state.lock().unwrap();
        if let FastModeRuntimeState::Cooldown { reset_at, .. } = &*state {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            if now >= *reset_at {
                if !self.has_logged_cooldown_expiry.swap(true, Ordering::SeqCst) {
                    tracing::debug!("Fast mode cooldown expired, re-enabling fast mode");
                }
                *state = FastModeRuntimeState::Active;
            }
        }
        state.clone()
    }

    /// Trigger fast mode cooldown.
    pub fn trigger_cooldown(&self, reset_timestamp: u64, reason: CooldownReason) {
        let mut state = self.runtime_state.lock().unwrap();
        *state = FastModeRuntimeState::Cooldown {
            reset_at: reset_timestamp,
            reason,
        };
        self.has_logged_cooldown_expiry
            .store(false, Ordering::SeqCst);
    }

    /// Clear fast mode cooldown.
    pub fn clear_cooldown(&self) {
        let mut state = self.runtime_state.lock().unwrap();
        *state = FastModeRuntimeState::Active;
    }

    /// Check if currently in cooldown.
    pub fn is_cooldown(&self) -> bool {
        matches!(
            self.get_runtime_state(),
            FastModeRuntimeState::Cooldown { .. }
        )
    }

    /// Get org status.
    pub fn get_org_status(&self) -> FastModeOrgStatus {
        self.org_status.lock().unwrap().clone()
    }

    /// Set org status.
    pub fn set_org_status(&self, status: FastModeOrgStatus) {
        *self.org_status.lock().unwrap() = status;
    }

    /// Handle fast mode rejected by API.
    pub fn handle_rejected_by_api(&self) {
        let mut status = self.org_status.lock().unwrap();
        if matches!(*status, FastModeOrgStatus::Disabled { .. }) {
            return;
        }
        *status = FastModeOrgStatus::Disabled {
            reason: FastModeDisabledReason::Preference,
        };
    }

    /// Handle fast mode overage rejection.
    pub fn handle_overage_rejection(&self, reason: Option<&str>) -> String {
        let message = get_overage_disabled_message(reason);
        if !is_out_of_credits_reason(reason) {
            let mut status = self.org_status.lock().unwrap();
            *status = FastModeOrgStatus::Disabled {
                reason: FastModeDisabledReason::ExtraUsageDisabled,
            };
        }
        message
    }

    /// Resolve org status from persisted cache.
    pub fn resolve_from_cache(&self, is_internal: bool, cached_enabled: bool) {
        let mut status = self.org_status.lock().unwrap();
        if !matches!(*status, FastModeOrgStatus::Pending) {
            return;
        }
        *status = if is_internal || cached_enabled {
            FastModeOrgStatus::Enabled
        } else {
            FastModeOrgStatus::Disabled {
                reason: FastModeDisabledReason::Unknown,
            }
        };
    }
}

impl Default for FastModeState {
    fn default() -> Self {
        Self::new()
    }
}

/// Get the fast mode state string ('off', 'cooldown', 'on').
pub fn get_fast_mode_state(
    model: &str,
    fast_mode_user_enabled: Option<bool>,
    custom_backend_enabled: bool,
    provider: &str,
    org_status: &FastModeOrgStatus,
    is_cooldown: bool,
) -> &'static str {
    let enabled = is_fast_mode_enabled(custom_backend_enabled)
        && is_fast_mode_available(custom_backend_enabled, provider, org_status)
        && fast_mode_user_enabled == Some(true)
        && is_fast_mode_supported_by_model(model, custom_backend_enabled);

    if enabled && is_cooldown {
        "cooldown"
    } else if enabled {
        "on"
    } else {
        "off"
    }
}

fn get_overage_disabled_message(reason: Option<&str>) -> String {
    match reason {
        Some("out_of_credits") => "Fast mode disabled · extra usage credits exhausted".to_string(),
        Some("org_level_disabled") | Some("org_service_level_disabled") => {
            "Fast mode disabled · extra usage disabled by your organization".to_string()
        }
        Some("org_level_disabled_until") => {
            "Fast mode disabled · extra usage spending cap reached".to_string()
        }
        Some("member_level_disabled") => {
            "Fast mode disabled · extra usage disabled for your account".to_string()
        }
        Some("seat_tier_level_disabled")
        | Some("seat_tier_zero_credit_limit")
        | Some("member_zero_credit_limit") => {
            "Fast mode disabled · extra usage not available for your plan".to_string()
        }
        Some("overage_not_provisioned") | Some("no_limits_configured") => {
            "Fast mode requires extra usage billing · /extra-usage to enable".to_string()
        }
        _ => "Fast mode disabled · extra usage not available".to_string(),
    }
}

fn is_out_of_credits_reason(reason: Option<&str>) -> bool {
    matches!(
        reason,
        Some("org_level_disabled_until") | Some("out_of_credits")
    )
}

// =============================================================================
// fast-mode 进程内运行态 — 对应 TS `fastMode.ts` 中的 runtime helpers。
// =============================================================================

use crate::signal::Signal;

#[derive(Debug, Clone, Default)]
struct RuntimeStateInner {
    cooldown_until_ms: Option<u128>,
    cooldown_reason: Option<String>,
    overage_rejection_reason: Option<String>,
}

static RUNTIME_STATE: Lazy<Mutex<RuntimeStateInner>> =
    Lazy::new(|| Mutex::new(RuntimeStateInner::default()));

/// 冷却被触发的信号订阅入口（对应 TS `onCooldownTriggered`）。
pub static ON_COOLDOWN_TRIGGERED: Lazy<Signal> = Lazy::new(Signal::new);
/// 冷却结束的信号订阅入口（对应 TS `onCooldownExpired`）。
pub static ON_COOLDOWN_EXPIRED: Lazy<Signal> = Lazy::new(Signal::new);
/// 超额拒绝信号订阅入口（对应 TS `onFastModeOverageRejection`）。
pub static ON_FAST_MODE_OVERAGE_REJECTION: Lazy<Signal> = Lazy::new(Signal::new);
/// 组织级 fast-mode 配置变更信号（对应 TS `onOrgFastModeChanged`）。
pub static ON_ORG_FAST_MODE_CHANGED: Lazy<Signal> = Lazy::new(Signal::new);

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// 获取当前 fast-mode 运行态（对应 TS `getFastModeRuntimeState`）。
pub fn get_fast_mode_runtime_state() -> FastModeRuntimeState {
    let mut state = RUNTIME_STATE.lock().unwrap();
    if let Some(until) = state.cooldown_until_ms {
        if now_ms() >= until {
            state.cooldown_until_ms = None;
            state.cooldown_reason = None;
            drop(state);
            ON_COOLDOWN_EXPIRED.emit();
            return FastModeRuntimeState::Active;
        }
        let reason = match state.cooldown_reason.as_deref() {
            Some("overloaded") => CooldownReason::Overloaded,
            _ => CooldownReason::RateLimit,
        };
        return FastModeRuntimeState::Cooldown {
            reset_at: until as u64,
            reason,
        };
    }
    FastModeRuntimeState::Active
}

/// 触发 fast-mode 冷却（对应 TS `triggerFastModeCooldown`）。
pub fn trigger_fast_mode_cooldown(duration_ms: u64, reason: CooldownReason) {
    let until = now_ms() + duration_ms as u128;
    let reason_str = match reason {
        CooldownReason::RateLimit => "rate_limit",
        CooldownReason::Overloaded => "overloaded",
    };
    {
        let mut state = RUNTIME_STATE.lock().unwrap();
        state.cooldown_until_ms = Some(until);
        state.cooldown_reason = Some(reason_str.to_string());
    }
    ON_COOLDOWN_TRIGGERED.emit();
}

/// 清空 fast-mode 冷却（对应 TS `clearFastModeCooldown`）。
pub fn clear_fast_mode_cooldown() {
    let was_in_cooldown = {
        let mut state = RUNTIME_STATE.lock().unwrap();
        let was = state.cooldown_until_ms.is_some();
        state.cooldown_until_ms = None;
        state.cooldown_reason = None;
        was
    };
    if was_in_cooldown {
        ON_COOLDOWN_EXPIRED.emit();
    }
}

/// 处理 API 拒绝 fast-mode 请求（对应 TS `handleFastModeRejectedByAPI`）。
///
/// 默认应用一个 5 分钟的限流冷却。
pub fn handle_fast_mode_rejected_by_api() {
    trigger_fast_mode_cooldown(5 * 60 * 1000, CooldownReason::RateLimit);
}

/// 处理 fast-mode 因超额被拒（对应 TS `handleFastModeOverageRejection`）。
pub fn handle_fast_mode_overage_rejection(reason: Option<&str>) {
    {
        let mut state = RUNTIME_STATE.lock().unwrap();
        state.overage_rejection_reason = reason.map(|s| s.to_string());
    }
    ON_FAST_MODE_OVERAGE_REJECTION.emit();
}

/// 当前是否处于冷却（对应 TS `isFastModeCooldown`）。
pub fn is_fast_mode_cooldown() -> bool {
    matches!(
        get_fast_mode_runtime_state(),
        FastModeRuntimeState::Cooldown { .. }
    )
}

/// 同步从缓存解析组织级 fast-mode 状态（对应 TS `resolveFastModeStatusFromCache`）。
pub fn resolve_fast_mode_status_from_cache() {
    // 缓存层尚未在 Rust 中落地；这里仅触发一次 org fast-mode 变更信号，让
    // 订阅方在 cache 命中时能感知到 — 与 TS 行为一致。
    ON_ORG_FAST_MODE_CHANGED.emit();
}

/// 异步预取 fast-mode 状态（对应 TS `prefetchFastModeStatus`）。
pub async fn prefetch_fast_mode_status() {
    // 真实实现会向计费/组织服务发请求；Rust 端先发一次 cache 解析信号，调用
    // 方可以替换实现。
    resolve_fast_mode_status_from_cache();
}
