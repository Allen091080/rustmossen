//! Get next permission mode logic.
//!
//! Determines the next permission mode when cycling through modes with Shift+Tab.

use super::permission_result::{PermissionMode, ToolPermissionContext};

/// Checks both the cached isAutoModeAvailable and the live gate.
/// Returns false if TRANSCRIPT_CLASSIFIER feature is not enabled.
fn can_cycle_to_auto(ctx: &ToolPermissionContext) -> bool {
    // In Rust, feature flags are runtime booleans passed as config.
    // We check ctx.is_auto_mode_available which is set at startup.
    ctx.is_auto_mode_available.unwrap_or(false)
}

/// Determines the next permission mode when cycling through modes with Shift+Tab.
pub fn get_next_permission_mode(
    tool_permission_context: &ToolPermissionContext,
    user_type: &str,
) -> PermissionMode {
    match tool_permission_context.mode {
        PermissionMode::Default => {
            // Ants skip acceptEdits and plan — auto mode replaces them
            if user_type == "ant" {
                if tool_permission_context.is_bypass_permissions_mode_available {
                    return PermissionMode::BypassPermissions;
                }
                if can_cycle_to_auto(tool_permission_context) {
                    return PermissionMode::Auto;
                }
                return PermissionMode::Default;
            }
            PermissionMode::AcceptEdits
        }

        PermissionMode::AcceptEdits => PermissionMode::Plan,

        PermissionMode::Plan => {
            if tool_permission_context.is_bypass_permissions_mode_available {
                return PermissionMode::BypassPermissions;
            }
            if can_cycle_to_auto(tool_permission_context) {
                return PermissionMode::Auto;
            }
            PermissionMode::Default
        }

        PermissionMode::BypassPermissions => {
            if can_cycle_to_auto(tool_permission_context) {
                return PermissionMode::Auto;
            }
            PermissionMode::Default
        }

        PermissionMode::DontAsk => {
            // Not exposed in UI cycle yet, but return default if somehow reached
            PermissionMode::Default
        }

        // Covers auto (when TRANSCRIPT_CLASSIFIER is enabled) and any future modes
        _ => PermissionMode::Default,
    }
}

/// Computes the next permission mode and returns it.
/// The actual context transition (stripping dangerous permissions, etc.)
/// should be done by the caller using `transition_permission_mode`.
pub fn cycle_permission_mode(
    tool_permission_context: &ToolPermissionContext,
    user_type: &str,
) -> PermissionMode {
    get_next_permission_mode(tool_permission_context, user_type)
}
