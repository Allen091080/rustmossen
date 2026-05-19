//! Permission mode types and utilities.
//!
//! Translates `utils/permissions/PermissionMode.ts`.

use super::permission_result::{ExternalPermissionMode, PermissionMode};

/// Color key for mode display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeColorKey {
    Text,
    PlanMode,
    Permission,
    AutoAccept,
    Error,
    Warning,
}

/// Configuration for a permission mode's display properties.
#[derive(Debug, Clone)]
pub struct PermissionModeConfig {
    pub title: &'static str,
    pub short_title: &'static str,
    pub symbol: &'static str,
    pub color: ModeColorKey,
    pub external: ExternalPermissionMode,
}

/// Get the configuration for a permission mode.
pub fn get_mode_config(mode: PermissionMode) -> PermissionModeConfig {
    match mode {
        PermissionMode::Default => PermissionModeConfig {
            title: "Default",
            short_title: "Default",
            symbol: "",
            color: ModeColorKey::Text,
            external: ExternalPermissionMode::Default,
        },
        PermissionMode::Plan => PermissionModeConfig {
            title: "Plan Mode",
            short_title: "Plan",
            symbol: "\u{23F8}",
            color: ModeColorKey::PlanMode,
            external: ExternalPermissionMode::Plan,
        },
        PermissionMode::AcceptEdits => PermissionModeConfig {
            title: "Accept edits",
            short_title: "Accept",
            symbol: "\u{23F5}\u{23F5}",
            color: ModeColorKey::AutoAccept,
            external: ExternalPermissionMode::AcceptEdits,
        },
        PermissionMode::BypassPermissions => PermissionModeConfig {
            title: "Bypass Permissions",
            short_title: "Bypass",
            symbol: "\u{23F5}\u{23F5}",
            color: ModeColorKey::Error,
            external: ExternalPermissionMode::BypassPermissions,
        },
        PermissionMode::DontAsk => PermissionModeConfig {
            title: "Don't Ask",
            short_title: "DontAsk",
            symbol: "\u{23F5}\u{23F5}",
            color: ModeColorKey::Error,
            external: ExternalPermissionMode::DontAsk,
        },
        PermissionMode::Auto => PermissionModeConfig {
            title: "Auto mode",
            short_title: "Auto",
            symbol: "\u{23F5}\u{23F5}",
            color: ModeColorKey::Warning,
            external: ExternalPermissionMode::Default,
        },
        PermissionMode::Bubble => PermissionModeConfig {
            title: "Default",
            short_title: "Default",
            symbol: "",
            color: ModeColorKey::Text,
            external: ExternalPermissionMode::Default,
        },
    }
}

/// Type guard to check if a PermissionMode is an ExternalPermissionMode.
pub fn is_external_permission_mode(mode: PermissionMode, is_ant_user: bool) -> bool {
    if !is_ant_user {
        return true;
    }
    !matches!(mode, PermissionMode::Auto | PermissionMode::Bubble)
}

/// Convert internal mode to external mode.
pub fn to_external_permission_mode(mode: PermissionMode) -> ExternalPermissionMode {
    get_mode_config(mode).external
}

/// Parse a string to a PermissionMode, defaulting to Default if unknown.
pub fn permission_mode_from_string(s: &str) -> PermissionMode {
    match s {
        "default" => PermissionMode::Default,
        "plan" => PermissionMode::Plan,
        "acceptEdits" => PermissionMode::AcceptEdits,
        "bypassPermissions" => PermissionMode::BypassPermissions,
        "dontAsk" => PermissionMode::DontAsk,
        "auto" => PermissionMode::Auto,
        "bubble" => PermissionMode::Bubble,
        _ => PermissionMode::Default,
    }
}

/// Get the localized title for a permission mode.
pub fn permission_mode_title(mode: PermissionMode) -> &'static str {
    get_mode_config(mode).title
}

/// Check if the mode is the default mode.
pub fn is_default_mode(mode: Option<PermissionMode>) -> bool {
    matches!(mode, Some(PermissionMode::Default) | None)
}

/// Get the short title for a permission mode.
pub fn permission_mode_short_title(mode: PermissionMode) -> &'static str {
    get_mode_config(mode).short_title
}

/// Get the symbol for a permission mode.
pub fn permission_mode_symbol(mode: PermissionMode) -> &'static str {
    get_mode_config(mode).symbol
}

/// Get the color key for a permission mode.
pub fn get_mode_color(mode: PermissionMode) -> ModeColorKey {
    get_mode_config(mode).color
}

/// Permission mode category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionModeCategory {
    Permission,
    Execution,
    Auto,
}

/// Get which category a permission mode belongs to.
pub fn permission_mode_category(mode: PermissionMode) -> PermissionModeCategory {
    match mode {
        PermissionMode::Plan => PermissionModeCategory::Execution,
        PermissionMode::Auto => PermissionModeCategory::Auto,
        _ => PermissionModeCategory::Permission,
    }
}

/// Get the label for a permission mode's category.
pub fn permission_mode_category_label(mode: PermissionMode) -> &'static str {
    match permission_mode_category(mode) {
        PermissionModeCategory::Permission => "permission",
        PermissionModeCategory::Execution => "execution",
        PermissionModeCategory::Auto => "auto",
    }
}

/// Alias for the permission mode validator (mirrors TS `permissionModeSchema`).
#[allow(non_camel_case_types)]
pub type permissionModeSchema = PermissionMode;
/// Alias for the external permission mode validator (mirrors TS `externalPermissionModeSchema`).
#[allow(non_camel_case_types)]
pub type externalPermissionModeSchema = ExternalPermissionMode;
