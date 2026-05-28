//! Permission update schema validation.
//!
//! In TypeScript this uses Zod schemas. In Rust we rely on serde for
//! deserialization + validation via the types already defined in `permission_result`.

use super::permission_result::{
    ExternalPermissionMode, PermissionBehavior, PermissionRuleValue, PermissionUpdate,
    PermissionUpdateDestination,
};

/// Validate a permission update destination string.
pub fn validate_permission_update_destination(s: &str) -> Option<PermissionUpdateDestination> {
    match s {
        "userSettings" => Some(PermissionUpdateDestination::UserSettings),
        "projectSettings" => Some(PermissionUpdateDestination::ProjectSettings),
        "localSettings" => Some(PermissionUpdateDestination::LocalSettings),
        "session" => Some(PermissionUpdateDestination::Session),
        "cliArg" => Some(PermissionUpdateDestination::CliArg),
        _ => None,
    }
}

/// Validate a permission update from a JSON value.
/// Returns None if the value does not conform to the schema.
pub fn validate_permission_update(value: &serde_json::Value) -> Option<PermissionUpdate> {
    serde_json::from_value(value.clone()).ok()
}

/// Validate an array of permission updates from a JSON value.
pub fn validate_permission_updates(value: &serde_json::Value) -> Option<Vec<PermissionUpdate>> {
    match value {
        serde_json::Value::Array(arr) => {
            let mut results = Vec::with_capacity(arr.len());
            for item in arr {
                results.push(validate_permission_update(item)?);
            }
            Some(results)
        }
        _ => None,
    }
}

/// Validate a permission behavior string.
pub fn validate_permission_behavior(s: &str) -> Option<PermissionBehavior> {
    match s {
        "allow" => Some(PermissionBehavior::Allow),
        "deny" => Some(PermissionBehavior::Deny),
        "ask" => Some(PermissionBehavior::Ask),
        _ => None,
    }
}

/// Validate a permission rule value from a JSON value.
pub fn validate_permission_rule_value(value: &serde_json::Value) -> Option<PermissionRuleValue> {
    serde_json::from_value(value.clone()).ok()
}

/// Validate an external permission mode string.
pub fn validate_external_permission_mode(s: &str) -> Option<ExternalPermissionMode> {
    match s {
        "acceptEdits" => Some(ExternalPermissionMode::AcceptEdits),
        "bypassPermissions" => Some(ExternalPermissionMode::BypassPermissions),
        "default" => Some(ExternalPermissionMode::Default),
        "dontAsk" => Some(ExternalPermissionMode::DontAsk),
        "plan" => Some(ExternalPermissionMode::Plan),
        _ => None,
    }
}

/// Alias for the permission update destination validator (mirrors TS `permissionUpdateDestinationSchema`).
#[allow(non_camel_case_types)]
pub type permissionUpdateDestinationSchema = PermissionUpdateDestination;
