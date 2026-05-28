//! Bypass permissions killswitch logic.
//!
//! Provides run-once checks to disable bypass/auto permissions modes
//! when external gates or settings dictate. The React hook logic from
//! TypeScript is converted to pure async functions.

use std::sync::atomic::{AtomicBool, Ordering};

use super::permission_result::ToolPermissionContext;

// Run-once flags
static BYPASS_PERMISSIONS_CHECK_RAN: AtomicBool = AtomicBool::new(false);
static AUTO_MODE_CHECK_RAN: AtomicBool = AtomicBool::new(false);

/// Check if bypass permissions should be disabled and apply the change.
/// This should be called once before the first query.
///
/// Returns the updated context if changes were made, or None if no change needed.
pub async fn check_and_disable_bypass_permissions_if_needed(
    tool_permission_context: &ToolPermissionContext,
    should_disable_bypass_fn: impl std::future::Future<Output = bool>,
) -> Option<ToolPermissionContext> {
    if BYPASS_PERMISSIONS_CHECK_RAN.swap(true, Ordering::SeqCst) {
        return None;
    }

    if !tool_permission_context.is_bypass_permissions_mode_available {
        return None;
    }

    let should_disable = should_disable_bypass_fn.await;
    if !should_disable {
        return None;
    }

    Some(create_disabled_bypass_permissions_context(
        tool_permission_context,
    ))
}

/// Reset the run-once flag for check_and_disable_bypass_permissions_if_needed.
/// Call this after /login so the gate check re-runs with the new org.
pub fn reset_bypass_permissions_check() {
    BYPASS_PERMISSIONS_CHECK_RAN.store(false, Ordering::SeqCst);
}

/// Result from verifying auto mode gate access.
pub struct AutoModeGateResult {
    /// Transform function to apply to context
    pub updated_context: Option<ToolPermissionContext>,
    /// Notification message if auto mode was disabled
    pub notification: Option<String>,
}

/// Check if auto mode should be disabled and apply the change.
/// This should be called once on startup and when model/fast mode changes.
///
/// `verify_fn` should be an async function that returns (updated_context_transform, notification).
pub async fn check_and_disable_auto_mode_if_needed(
    _tool_permission_context: &ToolPermissionContext,
    verify_fn: impl std::future::Future<Output = AutoModeGateResult>,
) -> AutoModeGateResult {
    if AUTO_MODE_CHECK_RAN.swap(true, Ordering::SeqCst) {
        return AutoModeGateResult {
            updated_context: None,
            notification: None,
        };
    }

    let result = verify_fn.await;

    // Apply the transform to CURRENT context, not the stale snapshot
    if let Some(ref _ctx) = result.updated_context {
        // Caller is responsible for applying this to current app state
    }

    result
}

/// Reset the run-once flag for check_and_disable_auto_mode_if_needed.
/// Call this after /login so the gate check re-runs with the new org.
pub fn reset_auto_mode_gate_check() {
    AUTO_MODE_CHECK_RAN.store(false, Ordering::SeqCst);
}

/// Creates an updated context with bypassPermissions disabled.
pub fn create_disabled_bypass_permissions_context(
    current_context: &ToolPermissionContext,
) -> ToolPermissionContext {
    use super::permission_result::PermissionMode;

    let mut updated = current_context.clone();
    if updated.mode == PermissionMode::BypassPermissions {
        updated.mode = PermissionMode::Default;
    }
    updated.is_bypass_permissions_mode_available = false;
    updated
}

// =============================================================================
// React-hook 别名 — TS 中 `useKickOffCheckAndDisableXxxIfNeeded` 是 React 钩子。
// Rust 端无 React，因此把核心逻辑保留在上面的 `check_and_disable_*` async 函
// 数中，hook 名作为公开 re-export，供调用方按 TS 同名引用。
// =============================================================================

/// 对应 TS `useKickOffCheckAndDisableBypassPermissionsIfNeeded`：
/// TS 中是 React 钩子，仅在挂载后调用一次；这里以 async fn 暴露同名入口。
pub async fn use_kick_off_check_and_disable_bypass_permissions_if_needed(
    tool_permission_context: &ToolPermissionContext,
    should_disable_bypass_fn: impl std::future::Future<Output = bool>,
) -> Option<ToolPermissionContext> {
    check_and_disable_bypass_permissions_if_needed(
        tool_permission_context,
        should_disable_bypass_fn,
    )
    .await
}

/// 对应 TS `useKickOffCheckAndDisableAutoModeIfNeeded`。
pub async fn use_kick_off_check_and_disable_auto_mode_if_needed(
    tool_permission_context: &ToolPermissionContext,
    verify_fn: impl std::future::Future<Output = AutoModeGateResult>,
) -> AutoModeGateResult {
    check_and_disable_auto_mode_if_needed(tool_permission_context, verify_fn).await
}
