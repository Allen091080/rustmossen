//! `/thinkback-play` — Play the thinkback animation (hidden, internal).

use anyhow::Result;
use async_trait::async_trait;
use std::path::PathBuf;

use crate::context::{CommandContext, CommandResult, Directive};
use crate::thinkback;

/// Thinkback play directive — play the thinkback animation after generation.
pub struct ThinkbackPlayDirective;

/// Official marketplace name constant.
const OFFICIAL_MARKETPLACE_NAME: &str = "mossen-plugins-official";
/// Internal marketplace name constant.
const INTERNAL_MARKETPLACE_NAME: &str = "mossen-code-marketplace";
/// Skill name.
const SKILL_NAME: &str = "thinkback";

/// Get the plugin ID based on user type.
fn get_plugin_id(ctx: &CommandContext) -> String {
    let marketplace_name = if ctx.is_internal_user() {
        INTERNAL_MARKETPLACE_NAME
    } else {
        OFFICIAL_MARKETPLACE_NAME
    };
    format!("thinkback@{}", marketplace_name)
}

/// Load installed plugins V2 data and find the thinkback plugin path.
fn find_thinkback_install_path(ctx: &CommandContext) -> Option<PathBuf> {
    let plugin_dir = ctx
        .env_vars
        .get("MOSSEN_PLUGIN_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            ctx.env_vars
                .get("HOME")
                .map(|h| PathBuf::from(h).join(".mossen").join("plugins"))
                .unwrap_or_else(|| PathBuf::from("/tmp/.mossen/plugins"))
        });

    // Check for installed plugins v2 config
    let config_path = plugin_dir.join("installed-plugins-v2.json");
    if let Ok(content) = std::fs::read_to_string(&config_path) {
        if let Ok(data) = serde_json::from_str::<serde_json::Value>(&content) {
            let plugin_id = get_plugin_id(ctx);
            if let Some(installations) = data.get("plugins").and_then(|p| p.get(&plugin_id)) {
                if let Some(arr) = installations.as_array() {
                    if let Some(first) = arr.first() {
                        if let Some(path) = first.get("installPath").and_then(|p| p.as_str()) {
                            return Some(PathBuf::from(path));
                        }
                    }
                }
            }
        }
    }

    // Fallback: scan plugin directory for thinkback
    if let Ok(entries) = std::fs::read_dir(&plugin_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let skill_dir = path.join("skills").join(SKILL_NAME);
                if skill_dir.exists() {
                    return Some(path);
                }
            }
        }
    }

    None
}

#[async_trait]
impl Directive for ThinkbackPlayDirective {
    fn name(&self) -> &str {
        "thinkback-play"
    }

    fn description(&self) -> &str {
        "Play the thinkback animation"
    }

    fn is_hidden(&self) -> bool {
        true
    }

    fn is_enabled(&self, ctx: &CommandContext) -> bool {
        ctx.is_env_truthy("MOSSEN_THINKBACK_ENABLED")
    }

    async fn execute(&self, _args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        // Get skill directory from installed plugins config
        let install_path = find_thinkback_install_path(ctx);

        match install_path {
            None => Ok(CommandResult::Text(
                "Thinkback plugin not installed. Run /think-back first to install it.".to_string(),
            )),
            Some(path) => {
                let skill_dir = path.join("skills").join(SKILL_NAME);
                if !skill_dir.exists() {
                    return Ok(CommandResult::Text(
                        "Thinkback plugin installation path not found.".to_string(),
                    ));
                }
                let result = thinkback::play_animation(&skill_dir)?;
                Ok(CommandResult::Text(result.message))
            }
        }
    }
}
