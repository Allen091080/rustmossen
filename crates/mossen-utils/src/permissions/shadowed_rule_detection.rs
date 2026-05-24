//! Shadowed rule detection.
//!
//! Detects unreachable permission rules that are shadowed by broader rules.

use super::permission_result::{PermissionRule, PermissionRuleSource};

/// Type of shadowing that makes a rule unreachable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShadowType {
    Ask,
    Deny,
}

/// Represents an unreachable permission rule with explanation.
#[derive(Debug, Clone)]
pub struct UnreachableRule {
    pub rule: PermissionRule,
    pub reason: String,
    pub shadowed_by: PermissionRule,
    pub shadow_type: ShadowType,
    pub fix: String,
}

/// Options for detecting unreachable rules.
pub struct DetectUnreachableRulesOptions {
    /// Whether sandbox auto-allow is enabled for Bash commands.
    pub sandbox_auto_allow_enabled: bool,
}

/// Result of checking if a rule is shadowed.
enum ShadowResult {
    NotShadowed,
    Shadowed {
        shadowed_by: PermissionRule,
        shadow_type: ShadowType,
    },
}

/// Check if a permission rule source is shared (visible to other users).
pub fn is_shared_setting_source(source: PermissionRuleSource) -> bool {
    matches!(
        source,
        PermissionRuleSource::ProjectSettings
            | PermissionRuleSource::PolicySettings
            | PermissionRuleSource::Command
    )
}

/// Format a rule source for display in warning messages.
fn format_source(source: PermissionRuleSource) -> &'static str {
    match source {
        PermissionRuleSource::UserSettings => "user settings",
        PermissionRuleSource::ProjectSettings => "project settings",
        PermissionRuleSource::LocalSettings => "local settings",
        PermissionRuleSource::FlagSettings => "flag settings",
        PermissionRuleSource::PolicySettings => "policy settings",
        PermissionRuleSource::CliArg => "CLI argument",
        PermissionRuleSource::Command => "command",
        PermissionRuleSource::Session => "session",
    }
}

/// Generate a fix suggestion based on the shadow type.
fn generate_fix_suggestion(
    shadow_type: ShadowType,
    shadowing_rule: &PermissionRule,
    shadowed_rule: &PermissionRule,
) -> String {
    let shadowing_source = format_source(shadowing_rule.source);
    let shadowed_source = format_source(shadowed_rule.source);
    let tool_name = &shadowing_rule.rule_value.tool_name;

    match shadow_type {
        ShadowType::Deny => {
            format!(
                "Remove the \"{}\" deny rule from {}, or remove the specific allow rule from {}",
                tool_name, shadowing_source, shadowed_source
            )
        }
        ShadowType::Ask => {
            format!(
                "Remove the \"{}\" ask rule from {}, or remove the specific allow rule from {}",
                tool_name, shadowing_source, shadowed_source
            )
        }
    }
}

const BASH_TOOL_NAME: &str = "Bash";

/// Check if a specific allow rule is shadowed by an ask rule.
fn is_allow_rule_shadowed_by_ask_rule(
    allow_rule: &PermissionRule,
    ask_rules: &[PermissionRule],
    options: &DetectUnreachableRulesOptions,
) -> ShadowResult {
    let tool_name = &allow_rule.rule_value.tool_name;
    let rule_content = &allow_rule.rule_value.rule_content;

    // Only check allow rules that have specific content
    if rule_content.is_none() {
        return ShadowResult::NotShadowed;
    }

    // Find any tool-wide ask rule for the same tool
    let shadowing_ask_rule = ask_rules.iter().find(|ask_rule| {
        ask_rule.rule_value.tool_name == *tool_name && ask_rule.rule_value.rule_content.is_none()
    });

    let shadowing_ask_rule = match shadowing_ask_rule {
        Some(r) => r,
        None => return ShadowResult::NotShadowed,
    };

    // Special case: Bash with sandbox auto-allow from personal settings
    if tool_name == BASH_TOOL_NAME && options.sandbox_auto_allow_enabled {
        if !is_shared_setting_source(shadowing_ask_rule.source) {
            return ShadowResult::NotShadowed;
        }
    }

    ShadowResult::Shadowed {
        shadowed_by: shadowing_ask_rule.clone(),
        shadow_type: ShadowType::Ask,
    }
}

/// Check if an allow rule is shadowed by a deny rule.
fn is_allow_rule_shadowed_by_deny_rule(
    allow_rule: &PermissionRule,
    deny_rules: &[PermissionRule],
) -> ShadowResult {
    let tool_name = &allow_rule.rule_value.tool_name;
    let rule_content = &allow_rule.rule_value.rule_content;

    // Only check allow rules that have specific content
    if rule_content.is_none() {
        return ShadowResult::NotShadowed;
    }

    // Find any tool-wide deny rule for the same tool
    let shadowing_deny_rule = deny_rules.iter().find(|deny_rule| {
        deny_rule.rule_value.tool_name == *tool_name && deny_rule.rule_value.rule_content.is_none()
    });

    match shadowing_deny_rule {
        Some(r) => ShadowResult::Shadowed {
            shadowed_by: r.clone(),
            shadow_type: ShadowType::Deny,
        },
        None => ShadowResult::NotShadowed,
    }
}

/// Detect all unreachable permission rules in the given context.
///
/// Currently detects:
/// - Allow rules shadowed by tool-wide deny rules (more severe - completely blocked)
/// - Allow rules shadowed by tool-wide ask rules (will always prompt)
pub fn detect_unreachable_rules(
    allow_rules: &[PermissionRule],
    ask_rules: &[PermissionRule],
    deny_rules: &[PermissionRule],
    options: &DetectUnreachableRulesOptions,
) -> Vec<UnreachableRule> {
    let mut unreachable = Vec::new();

    for allow_rule in allow_rules {
        // Check deny shadowing first (more severe)
        if let ShadowResult::Shadowed {
            shadowed_by,
            shadow_type,
        } = is_allow_rule_shadowed_by_deny_rule(allow_rule, deny_rules)
        {
            let shadow_source = format_source(shadowed_by.source);
            unreachable.push(UnreachableRule {
                rule: allow_rule.clone(),
                reason: format!(
                    "Blocked by \"{}\" deny rule (from {})",
                    shadowed_by.rule_value.tool_name, shadow_source
                ),
                fix: generate_fix_suggestion(shadow_type, &shadowed_by, allow_rule),
                shadowed_by,
                shadow_type,
            });
            continue;
        }

        // Check ask shadowing
        if let ShadowResult::Shadowed {
            shadowed_by,
            shadow_type,
        } = is_allow_rule_shadowed_by_ask_rule(allow_rule, ask_rules, options)
        {
            let shadow_source = format_source(shadowed_by.source);
            unreachable.push(UnreachableRule {
                rule: allow_rule.clone(),
                reason: format!(
                    "Shadowed by \"{}\" ask rule (from {})",
                    shadowed_by.rule_value.tool_name, shadow_source
                ),
                fix: generate_fix_suggestion(shadow_type, &shadowed_by, allow_rule),
                shadowed_by,
                shadow_type,
            });
        }
    }

    unreachable
}
