//! Permissions loader - loads permission rules from settings files.
//!
//! Handles reading permission rules from disk (user/project/local/policy settings)
//! and managing rule persistence (add/delete).

use super::permission_result::{
    PermissionBehavior, PermissionRule, PermissionRuleSource, PermissionRuleValue,
};
use super::permission_rule_parser::{
    permission_rule_value_from_string, permission_rule_value_to_string,
};

/// Editable setting sources that can be modified.
pub const EDITABLE_SOURCES: &[&str] = &["userSettings", "projectSettings", "localSettings"];

/// All supported rule behaviors.
const SUPPORTED_RULE_BEHAVIORS: &[PermissionBehavior] = &[
    PermissionBehavior::Allow,
    PermissionBehavior::Deny,
    PermissionBehavior::Ask,
];

/// Returns true if allowManagedPermissionRulesOnly is enabled in managed settings.
/// When enabled, only permission rules from managed settings are respected.
pub fn should_allow_managed_permission_rules_only(
    get_settings_for_source: impl Fn(&str) -> Option<serde_json::Value>,
) -> bool {
    get_settings_for_source("policySettings")
        .and_then(|s| s.get("allowManagedPermissionRulesOnly")?.as_bool())
        .unwrap_or(false)
}

/// Returns true if "always allow" options should be shown in permission prompts.
pub fn should_show_always_allow_options(
    get_settings_for_source: impl Fn(&str) -> Option<serde_json::Value>,
) -> bool {
    !should_allow_managed_permission_rules_only(get_settings_for_source)
}

/// Converts permissions JSON to an array of PermissionRule objects.
fn settings_json_to_rules(
    data: Option<&serde_json::Value>,
    source: PermissionRuleSource,
) -> Vec<PermissionRule> {
    let data = match data {
        Some(d) => d,
        None => return Vec::new(),
    };

    let permissions = match data.get("permissions") {
        Some(p) => p,
        None => return Vec::new(),
    };

    let mut rules = Vec::new();
    for behavior in SUPPORTED_RULE_BEHAVIORS {
        let behavior_key = match behavior {
            PermissionBehavior::Allow => "allow",
            PermissionBehavior::Deny => "deny",
            PermissionBehavior::Ask => "ask",
        };

        if let Some(arr) = permissions.get(behavior_key).and_then(|v| v.as_array()) {
            for rule_value in arr {
                if let Some(rule_string) = rule_value.as_str() {
                    rules.push(PermissionRule {
                        source,
                        rule_behavior: *behavior,
                        rule_value: permission_rule_value_from_string(rule_string),
                    });
                }
            }
        }
    }
    rules
}

/// Loads all permission rules from all relevant sources.
pub fn load_all_permission_rules_from_disk(
    get_settings_for_source: impl Fn(&str) -> Option<serde_json::Value>,
    get_enabled_setting_sources: impl Fn() -> Vec<&'static str>,
) -> Vec<PermissionRule> {
    // If allowManagedPermissionRulesOnly is set, only use managed permission rules
    if should_allow_managed_permission_rules_only(&get_settings_for_source) {
        let data = get_settings_for_source("policySettings");
        return settings_json_to_rules(data.as_ref(), PermissionRuleSource::PolicySettings);
    }

    let mut rules = Vec::new();
    for source_str in get_enabled_setting_sources() {
        let source = match source_str {
            "userSettings" => PermissionRuleSource::UserSettings,
            "projectSettings" => PermissionRuleSource::ProjectSettings,
            "localSettings" => PermissionRuleSource::LocalSettings,
            "flagSettings" => PermissionRuleSource::FlagSettings,
            "policySettings" => PermissionRuleSource::PolicySettings,
            _ => continue,
        };
        let data = get_settings_for_source(source_str);
        rules.extend(settings_json_to_rules(data.as_ref(), source));
    }
    rules
}

/// Loads permission rules from a specific source.
pub fn get_permission_rules_for_source(
    source_str: &str,
    get_settings_for_source: impl Fn(&str) -> Option<serde_json::Value>,
) -> Vec<PermissionRule> {
    let source = match source_str {
        "userSettings" => PermissionRuleSource::UserSettings,
        "projectSettings" => PermissionRuleSource::ProjectSettings,
        "localSettings" => PermissionRuleSource::LocalSettings,
        "flagSettings" => PermissionRuleSource::FlagSettings,
        "policySettings" => PermissionRuleSource::PolicySettings,
        "cliArg" => PermissionRuleSource::CliArg,
        "command" => PermissionRuleSource::Command,
        "session" => PermissionRuleSource::Session,
        _ => return Vec::new(),
    };
    let data = get_settings_for_source(source_str);
    settings_json_to_rules(data.as_ref(), source)
}

/// Deletes a rule from the settings.
/// Returns true if the rule was successfully deleted.
pub fn delete_permission_rule_from_settings(
    rule: &PermissionRule,
    get_settings_for_source: impl Fn(&str) -> Option<serde_json::Value>,
    update_settings_for_source: impl Fn(&str, serde_json::Value) -> Result<(), String>,
) -> bool {
    let source_str = match rule.source {
        PermissionRuleSource::UserSettings => "userSettings",
        PermissionRuleSource::ProjectSettings => "projectSettings",
        PermissionRuleSource::LocalSettings => "localSettings",
        _ => return false,
    };

    // Runtime check to ensure source is actually editable
    if !EDITABLE_SOURCES.contains(&source_str) {
        return false;
    }

    let rule_string = permission_rule_value_to_string(&rule.rule_value);
    let settings_data = match get_settings_for_source(source_str) {
        Some(d) => d,
        None => return false,
    };

    let permissions = match settings_data.get("permissions") {
        Some(p) => p,
        None => return false,
    };

    let behavior_key = match rule.rule_behavior {
        PermissionBehavior::Allow => "allow",
        PermissionBehavior::Deny => "deny",
        PermissionBehavior::Ask => "ask",
    };

    let behavior_array = match permissions.get(behavior_key).and_then(|v| v.as_array()) {
        Some(a) => a,
        None => return false,
    };

    // Normalize entries via roundtrip parse→serialize
    let normalize_entry = |raw: &str| -> String {
        permission_rule_value_to_string(&permission_rule_value_from_string(raw))
    };

    if !behavior_array
        .iter()
        .filter_map(|v| v.as_str())
        .any(|raw| normalize_entry(raw) == rule_string)
    {
        return false;
    }

    // Build updated settings
    let filtered: Vec<serde_json::Value> = behavior_array
        .iter()
        .filter(|v| {
            v.as_str()
                .map(|raw| normalize_entry(raw) != rule_string)
                .unwrap_or(true)
        })
        .cloned()
        .collect();

    let mut updated = settings_data.clone();
    if let Some(perms) = updated.get_mut("permissions") {
        if let Some(obj) = perms.as_object_mut() {
            obj.insert(behavior_key.to_string(), serde_json::Value::Array(filtered));
        }
    }

    update_settings_for_source(source_str, updated).is_ok()
}

/// Adds rules to the settings file.
/// Returns true on success.
pub fn add_permission_rules_to_settings(
    rule_values: &[PermissionRuleValue],
    rule_behavior: PermissionBehavior,
    source_str: &str,
    get_settings_for_source: impl Fn(&str) -> Option<serde_json::Value>,
    update_settings_for_source: impl Fn(&str, serde_json::Value) -> Result<(), String>,
    should_allow_managed_only: bool,
) -> bool {
    // When allowManagedPermissionRulesOnly is enabled, don't persist new permission rules
    if should_allow_managed_only {
        return false;
    }

    if rule_values.is_empty() {
        return true;
    }

    let rule_strings: Vec<String> = rule_values
        .iter()
        .map(|rv| permission_rule_value_to_string(rv))
        .collect();

    let settings_data = get_settings_for_source(source_str)
        .unwrap_or_else(|| serde_json::json!({"permissions": {}}));

    let behavior_key = match rule_behavior {
        PermissionBehavior::Allow => "allow",
        PermissionBehavior::Deny => "deny",
        PermissionBehavior::Ask => "ask",
    };

    let existing_rules: Vec<String> = settings_data
        .get("permissions")
        .and_then(|p| p.get(behavior_key))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();

    // Filter out duplicates via roundtrip normalization
    let existing_set: std::collections::HashSet<String> = existing_rules
        .iter()
        .map(|raw| permission_rule_value_to_string(&permission_rule_value_from_string(raw)))
        .collect();

    let new_rules: Vec<&String> = rule_strings
        .iter()
        .filter(|rule| !existing_set.contains(rule.as_str()))
        .collect();

    if new_rules.is_empty() {
        return true;
    }

    // Build updated settings
    let mut all_rules: Vec<serde_json::Value> = existing_rules
        .iter()
        .map(|s| serde_json::Value::String(s.clone()))
        .collect();
    for rule in new_rules {
        all_rules.push(serde_json::Value::String(rule.clone()));
    }

    let mut updated = settings_data.clone();
    let perms = updated
        .as_object_mut()
        .unwrap()
        .entry("permissions")
        .or_insert_with(|| serde_json::json!({}));
    if let Some(obj) = perms.as_object_mut() {
        obj.insert(
            behavior_key.to_string(),
            serde_json::Value::Array(all_rules),
        );
    }

    update_settings_for_source(source_str, updated).is_ok()
}

/// 对应 TS `PermissionRuleFromEditableSettings`：从可编辑 settings 解析的权限规则 JSON 别名。
pub type PermissionRuleFromEditableSettings = serde_json::Value;
