//! `/think-back` — Your year-in-review with Mossen (local-widget).

use anyhow::Result;
use async_trait::async_trait;
use std::path::{Path, PathBuf};

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Thinkback directive — generate and view year-in-review stats.
pub struct ThinkbackDirective;

/// Marketplace and plugin identifiers.
const OFFICIAL_MARKETPLACE_NAME: &str = "mossen-plugins-official";
const INTERNAL_MARKETPLACE_NAME: &str = "mossen-code-marketplace";
const OFFICIAL_MARKETPLACE_REPO: &str = "mossen/mossen-plugins-official";
const INTERNAL_MARKETPLACE_REPO: &str = "mossen/mossen-code-marketplace";
const SKILL_NAME: &str = "thinkback";

/// Get the marketplace name based on user type.
fn get_marketplace_name(ctx: &CommandContext) -> &'static str {
    if ctx.is_internal_user() {
        INTERNAL_MARKETPLACE_NAME
    } else {
        OFFICIAL_MARKETPLACE_NAME
    }
}

/// Get the marketplace repository based on user type.
fn get_marketplace_repo(ctx: &CommandContext) -> &'static str {
    if ctx.is_internal_user() {
        INTERNAL_MARKETPLACE_REPO
    } else {
        OFFICIAL_MARKETPLACE_REPO
    }
}

/// Get the plugin ID for thinkback.
fn get_plugin_id(ctx: &CommandContext) -> String {
    format!("thinkback@{}", get_marketplace_name(ctx))
}

/// Get the thinkback skill directory from installed plugins.
fn get_thinkback_skill_dir(ctx: &CommandContext) -> Option<PathBuf> {
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

    // Look for the thinkback plugin in installed plugins
    let plugin_id = get_plugin_id(ctx);
    let thinkback_path = plugin_dir.join("thinkback");

    if thinkback_path.exists() {
        let skill_dir = thinkback_path.join("skills").join(SKILL_NAME);
        if skill_dir.exists() {
            return Some(skill_dir);
        }
    }

    // Also check in the generic plugin path
    if let Ok(entries) = std::fs::read_dir(&plugin_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let skill_dir = path.join("skills").join(SKILL_NAME);
                if skill_dir.exists() {
                    return Some(skill_dir);
                }
            }
        }
    }

    None
}

/// Play the thinkback animation from the skill directory.
pub fn play_animation(skill_dir: &Path) -> Result<PlayResult> {
    let data_path = skill_dir.join("year_in_review.js");
    let player_path = skill_dir.join("player.js");

    // Check if data file exists
    if !data_path.exists() {
        return Ok(PlayResult {
            success: false,
            message: "No animation found. Run /think-back first to generate one.".to_string(),
        });
    }

    // Check if player script exists
    if !player_path.exists() {
        return Ok(PlayResult {
            success: false,
            message: "Player script not found. The player.js file is missing from the thinkback skill.".to_string(),
        });
    }

    // Attempt to run the player via node subprocess
    let output = std::process::Command::new("node")
        .arg(&player_path)
        .arg(&data_path)
        .output();

    match output {
        Ok(result) if result.status.success() => {
            let stdout = String::from_utf8_lossy(&result.stdout).to_string();
            Ok(PlayResult {
                success: true,
                message: if stdout.is_empty() {
                    "Animation complete.".to_string()
                } else {
                    stdout
                },
            })
        }
        Ok(result) => {
            let stderr = String::from_utf8_lossy(&result.stderr).to_string();
            Ok(PlayResult {
                success: false,
                message: format!("Animation playback failed: {}", stderr),
            })
        }
        Err(e) => Ok(PlayResult {
            success: false,
            message: format!("Could not run animation player: {}", e),
        }),
    }
}

/// Result of playing the thinkback animation.
pub struct PlayResult {
    pub success: bool,
    pub message: String,
}

/// Check if the thinkback plugin is installed.
fn is_plugin_installed(ctx: &CommandContext) -> bool {
    get_thinkback_skill_dir(ctx).is_some()
}

/// Execute the thinkback flow — install plugin if needed, then show/generate review.
async fn execute_thinkback_flow(ctx: &CommandContext) -> Result<String> {
    // Check if plugin is already installed
    if let Some(skill_dir) = get_thinkback_skill_dir(ctx) {
        // Plugin installed — check if animation data exists
        let data_path = skill_dir.join("year_in_review.js");
        if data_path.exists() {
            // Play existing animation
            let result = play_animation(&skill_dir)?;
            return Ok(result.message);
        } else {
            return Ok(
                "Thinkback plugin is installed but no year-in-review data found.\n\
                 The review will be generated based on your usage history."
                    .to_string(),
            );
        }
    }

    // Plugin not installed — guide user through installation
    let marketplace_name = get_marketplace_name(ctx);
    let marketplace_repo = get_marketplace_repo(ctx);

    Ok(format!(
        "The thinkback plugin is not yet installed.\n\n\
         To install it:\n\
         1. The plugin will be fetched from {}\n\
         2. It will analyze your Mossen usage history\n\
         3. Generate a personalized year-in-review animation\n\n\
         Installing thinkback from {}…",
        marketplace_name, marketplace_repo
    ))
}

#[async_trait]
impl Directive for ThinkbackDirective {
    fn name(&self) -> &str {
        "think-back"
    }

    fn description(&self) -> &str {
        "Your Mossen year in review"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn is_enabled(&self, ctx: &CommandContext) -> bool {
        ctx.is_env_truthy("MOSSEN_THINKBACK_ENABLED")
    }

    async fn execute(&self, _args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        let result = execute_thinkback_flow(ctx).await?;
        Ok(CommandResult::Text(result))
    }
}
