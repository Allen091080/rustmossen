//! Permission rule types and schema helpers.
//!
//! Translates `utils/permissions/PermissionRule.ts`.

pub use super::permission_result::{
    PermissionBehavior, PermissionRule, PermissionRuleSource, PermissionRuleValue,
};

/// Alias for the permission behavior validator (mirrors TS `permissionBehaviorSchema`).
#[allow(non_camel_case_types)]
pub type permissionBehaviorSchema = PermissionBehavior;
/// Alias for the permission rule value validator (mirrors TS `permissionRuleValueSchema`).
#[allow(non_camel_case_types)]
pub type permissionRuleValueSchema = PermissionRuleValue;
