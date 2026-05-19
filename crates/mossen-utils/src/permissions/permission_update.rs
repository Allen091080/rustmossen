//! Permission update application and persistence.
//!
//! Applies permission updates to the in-memory context and persists them to disk.

use std::collections::{HashMap, HashSet};

use super::permission_result::{
    AdditionalWorkingDirectory, ExternalPermissionMode, PermissionBehavior, PermissionMode,
    PermissionRule, PermissionRuleSource, PermissionRuleValue, PermissionUpdate,
    PermissionUpdateDestination, ToolPermissionContext, ToolPermissionRulesBySource,
};
use super::permission_rule_parser::{permission_rule_value_from_string, permission_rule_value_to_string};

/// Extract rules from permission updates (only addRules type).
pub fn extract_rules(updates: Option<&[PermissionUpdate]>) -> Vec<PermissionRuleValue> {
    let updates = match updates {
        Some(u) => u,
        None => return Vec::new(),
    };

    updates
        .iter()
        .flat_map(|update| match update {
            PermissionUpdate::AddRules { rules, .. } => rules.clone(),
            _ => Vec::new(),
        })
        .collect()
}

/// Check if updates contain any rules.
pub fn has_rules(updates: Option<&[PermissionUpdate]>) -> bool {
    !extract_rules(updates).is_empty()
}

/// Convert PermissionUpdateDestination to the rules key.
fn rules_key_for_behavior(behavior: PermissionBehavior) -> &'static str {
    match behavior {
        PermissionBehavior::Allow => "alwaysAllowRules",
        PermissionBehavior::Deny => "alwaysDenyRules",
        PermissionBehavior::Ask => "alwaysAskRules",
    }
}

fn destination_to_str(dest: PermissionUpdateDestination) -> &'static str {
    match dest {
        PermissionUpdateDestination::UserSettings => "userSettings",
        PermissionUpdateDestination::ProjectSettings => "projectSettings",
        PermissionUpdateDestination::LocalSettings => "localSettings",
        PermissionUpdateDestination::Session => "session",
        PermissionUpdateDestination::CliArg => "cliArg",
    }
}

fn get_rules_for_source<'a>(
    rules: &'a ToolPermissionRulesBySource,
    source: PermissionRuleSource,
) -> Vec<String> {
    rules.get(&source).cloned().unwrap_or_default()
}

fn dest_to_source(dest: PermissionUpdateDestination) -> PermissionRuleSource {
    match dest {
        PermissionUpdateDestination::UserSettings => PermissionRuleSource::UserSettings,
        PermissionUpdateDestination::ProjectSettings => PermissionRuleSource::ProjectSettings,
        PermissionUpdateDestination::LocalSettings => PermissionRuleSource::LocalSettings,
        PermissionUpdateDestination::Session => PermissionRuleSource::Session,
        PermissionUpdateDestination::CliArg => PermissionRuleSource::CliArg,
    }
}

/// Applies a single permission update to the context and returns the updated context.
pub fn apply_permission_update(
    context: &ToolPermissionContext,
    update: &PermissionUpdate,
) -> ToolPermissionContext {
    let mut ctx = context.clone();

    match update {
        PermissionUpdate::SetMode { mode, .. } => {
            ctx.mode = match mode {
                ExternalPermissionMode::AcceptEdits => PermissionMode::AcceptEdits,
                ExternalPermissionMode::BypassPermissions => PermissionMode::BypassPermissions,
                ExternalPermissionMode::Default => PermissionMode::Default,
                ExternalPermissionMode::DontAsk => PermissionMode::DontAsk,
                ExternalPermissionMode::Plan => PermissionMode::Plan,
            };
        }

        PermissionUpdate::AddRules {
            destination,
            rules,
            behavior,
        } => {
            let rule_strings: Vec<String> =
                rules.iter().map(|r| permission_rule_value_to_string(r)).collect();
            let source = dest_to_source(*destination);
            let rules_map = match behavior {
                PermissionBehavior::Allow => &mut ctx.always_allow_rules,
                PermissionBehavior::Deny => &mut ctx.always_deny_rules,
                PermissionBehavior::Ask => &mut ctx.always_ask_rules,
            };
            let entry = rules_map.entry(source).or_insert_with(Vec::new);
            entry.extend(rule_strings);
        }

        PermissionUpdate::ReplaceRules {
            destination,
            rules,
            behavior,
        } => {
            let rule_strings: Vec<String> =
                rules.iter().map(|r| permission_rule_value_to_string(r)).collect();
            let source = dest_to_source(*destination);
            let rules_map = match behavior {
                PermissionBehavior::Allow => &mut ctx.always_allow_rules,
                PermissionBehavior::Deny => &mut ctx.always_deny_rules,
                PermissionBehavior::Ask => &mut ctx.always_ask_rules,
            };
            rules_map.insert(source, rule_strings);
        }

        PermissionUpdate::RemoveRules {
            destination,
            rules,
            behavior,
        } => {
            let rule_strings: HashSet<String> =
                rules.iter().map(|r| permission_rule_value_to_string(r)).collect();
            let source = dest_to_source(*destination);
            let rules_map = match behavior {
                PermissionBehavior::Allow => &mut ctx.always_allow_rules,
                PermissionBehavior::Deny => &mut ctx.always_deny_rules,
                PermissionBehavior::Ask => &mut ctx.always_ask_rules,
            };
            if let Some(existing) = rules_map.get_mut(&source) {
                existing.retain(|r| !rule_strings.contains(r));
            }
        }

        PermissionUpdate::AddDirectories {
            destination,
            directories,
        } => {
            let source = dest_to_source(*destination);
            for dir in directories {
                ctx.additional_working_directories.insert(
                    dir.clone(),
                    AdditionalWorkingDirectory {
                        path: dir.clone(),
                        source,
                    },
                );
            }
        }

        PermissionUpdate::RemoveDirectories { directories, .. } => {
            for dir in directories {
                ctx.additional_working_directories.remove(dir);
            }
        }
    }

    ctx
}

/// Applies multiple permission updates to the context.
pub fn apply_permission_updates(
    context: &ToolPermissionContext,
    updates: &[PermissionUpdate],
) -> ToolPermissionContext {
    let mut ctx = context.clone();
    for update in updates {
        ctx = apply_permission_update(&ctx, update);
    }
    ctx
}

/// Check if a destination supports persistence to disk.
pub fn supports_persistence(destination: PermissionUpdateDestination) -> bool {
    matches!(
        destination,
        PermissionUpdateDestination::LocalSettings
            | PermissionUpdateDestination::UserSettings
            | PermissionUpdateDestination::ProjectSettings
    )
}

/// Creates a Read rule suggestion for a directory.
/// Returns None for the root directory.
pub fn create_read_rule_suggestion(
    dir_path: &str,
    destination: PermissionUpdateDestination,
) -> Option<PermissionUpdate> {
    let path_for_pattern = to_posix_path(dir_path);

    // Root directory is too broad
    if path_for_pattern == "/" {
        return None;
    }

    // For absolute paths, prepend an extra / to create //path/** pattern
    let rule_content = if path_for_pattern.starts_with('/') {
        format!("/{}/**", path_for_pattern)
    } else {
        format!("{}/**", path_for_pattern)
    };

    Some(PermissionUpdate::AddRules {
        destination,
        rules: vec![PermissionRuleValue {
            tool_name: "Read".to_string(),
            rule_content: Some(rule_content),
        }],
        behavior: PermissionBehavior::Allow,
    })
}

/// Convert a path to POSIX format (forward slashes).
fn to_posix_path(path: &str) -> String {
    path.replace('\\', "/")
}

/// 对应 TS `persistPermissionUpdate`：把单条更新写入对应来源的设置文件。
///
/// Rust 端尚未集成 settings 写入流水线，这里把序列化结果暂存到 ~/.mossen/permission-updates/。
pub async fn persist_permission_update(update: &PermissionUpdate) -> std::io::Result<()> {
    if let Some(home) = dirs::home_dir() {
        let dir = home.join(".mossen").join("permission-updates");
        tokio::fs::create_dir_all(&dir).await?;
        let id = uuid::Uuid::new_v4().to_string();
        let path = dir.join(format!("{}.json", id));
        let body = serde_json::to_vec_pretty(update)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        tokio::fs::write(path, body).await?;
    }
    Ok(())
}

/// 对应 TS `persistPermissionUpdates`：批量持久化。
pub async fn persist_permission_updates(updates: &[PermissionUpdate]) -> std::io::Result<()> {
    for u in updates {
        persist_permission_update(u).await?;
    }
    Ok(())
}
