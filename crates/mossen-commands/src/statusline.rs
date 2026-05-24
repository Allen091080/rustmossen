//! `/statusline` — Set up and configure the Mossen status line UI.

use anyhow::Result;
use async_trait::async_trait;
use std::path::{Path, PathBuf};

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Statusline directive — configure the status line display.
pub struct StatuslineDirective;

/// Status line inspection result.
struct StatusLineInspection {
    command: Option<String>,
    exists: bool,
    padding: Option<i32>,
    script_readable: bool,
    script_path: Option<String>,
    script_summary: Option<String>,
}

/// Get the existing status line command from settings.
fn get_existing_statusline_command(ctx: &CommandContext) -> Option<String> {
    let config_home = get_mossen_config_home(ctx);
    let settings_path = config_home.join("settings.json");

    let raw = std::fs::read_to_string(&settings_path).ok()?;
    let settings: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let command = settings.get("statusLine")?.get("command")?.as_str()?;

    if command.is_empty() {
        None
    } else {
        Some(command.to_string())
    }
}

/// Get the Mossen config home directory.
fn get_mossen_config_home(ctx: &CommandContext) -> PathBuf {
    ctx.env_vars
        .get("MOSSEN_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            ctx.env_vars
                .get("HOME")
                .map(|h| PathBuf::from(h).join(".mossen"))
                .unwrap_or_else(|| PathBuf::from("/tmp/.mossen"))
        })
}

/// Unquote a shell token (remove surrounding quotes).
fn unquote_shell_token(token: &str) -> &str {
    if (token.starts_with('"') && token.ends_with('"'))
        || (token.starts_with('\'') && token.ends_with('\''))
    {
        &token[1..token.len() - 1]
    } else {
        token
    }
}

/// Expand ~ to home directory in paths.
fn expand_home_path(path: &str, home: &str) -> String {
    if path == "~" {
        home.to_string()
    } else if let Some(rest) = path.strip_prefix("~/") {
        format!("{}/{}", home, rest)
    } else {
        path.to_string()
    }
}

/// Get the script path from a status line command.
fn get_statusline_script_path(command: &str, home: &str) -> Option<String> {
    let tokens: Vec<&str> = command.split_whitespace().collect();
    if tokens.is_empty() {
        return None;
    }

    let executable = unquote_shell_token(tokens[0]);
    let script_token = if executable == "bash" || executable == "sh" || executable == "zsh" {
        tokens.get(1).copied()
    } else {
        tokens.first().copied()
    };

    script_token.map(|t| expand_home_path(unquote_shell_token(t), home))
}

/// Analyze script content to determine what features it shows.
fn get_statusline_script_summary(script: &str) -> Option<String> {
    let mut features = Vec::new();

    if script.contains("display_name") || script.contains("model") {
        features.push("model");
    }
    if script.contains("context_window") || script.contains("used_percentage") {
        features.push("context usage");
    }
    if script.contains("session_id") || script.contains("uptime") {
        features.push("session uptime");
    }
    if script.contains("rate_limits") {
        features.push("rate limits");
    }
    if script.contains("current_dir") || script.contains("cwd") {
        features.push("workspace path");
    }
    if script.contains("vim") {
        features.push("editor mode");
    }

    if features.is_empty() {
        None
    } else {
        Some(format!(
            "Current script appears to show: {}",
            features.join(", ")
        ))
    }
}

/// Inspect the current status line configuration.
fn inspect_statusline(ctx: &CommandContext) -> StatusLineInspection {
    let config_home = get_mossen_config_home(ctx);
    let settings_path = config_home.join("settings.json");
    let home = ctx
        .env_vars
        .get("HOME")
        .cloned()
        .unwrap_or_else(|| "/tmp".to_string());

    let raw = match std::fs::read_to_string(&settings_path) {
        Ok(content) => content,
        Err(_) => {
            return StatusLineInspection {
                command: None,
                exists: false,
                padding: None,
                script_readable: false,
                script_path: None,
                script_summary: None,
            };
        }
    };

    let settings: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(_) => {
            return StatusLineInspection {
                command: None,
                exists: false,
                padding: None,
                script_readable: false,
                script_path: None,
                script_summary: None,
            };
        }
    };

    let status_line = settings.get("statusLine");
    let command = status_line
        .and_then(|sl| sl.get("command"))
        .and_then(|c| c.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let padding = status_line
        .and_then(|sl| sl.get("padding"))
        .and_then(|p| p.as_i64())
        .map(|p| p as i32);

    if command.is_none() {
        return StatusLineInspection {
            command: None,
            exists: false,
            padding,
            script_readable: false,
            script_path: None,
            script_summary: None,
        };
    }

    let cmd = command.as_ref().unwrap();
    let script_path = get_statusline_script_path(cmd, &home);

    let read_path = script_path.as_deref().unwrap_or(cmd.as_str());
    match std::fs::read_to_string(read_path) {
        Ok(script_content) => StatusLineInspection {
            command,
            exists: true,
            padding,
            script_readable: true,
            script_path,
            script_summary: get_statusline_script_summary(&script_content),
        },
        Err(_) => StatusLineInspection {
            command,
            exists: true,
            padding,
            script_readable: false,
            script_path,
            script_summary: None,
        },
    }
}

/// Build a summary of the current status line configuration.
fn build_statusline_summary(inspection: &StatusLineInspection, ctx: &CommandContext) -> String {
    let config_home = get_mossen_config_home(ctx);
    let settings_path = config_home.join("settings.json");

    if !inspection.exists || inspection.command.is_none() {
        return format!(
            "No statusLine is configured right now.\n\
             Checked: {}\n\
             If you want to change it, run `/statusline <what to change>`.",
            settings_path.display()
        );
    }

    let cmd = inspection.command.as_ref().unwrap();
    let script_line = if inspection.script_readable {
        let path = inspection.script_path.as_deref().unwrap_or(cmd.as_str());
        format!("Script file found: {}", path)
    } else {
        format!("Configured command points to an unreadable file: {}", cmd)
    };

    let summary_line = inspection.script_summary.as_deref().unwrap_or_else(|| {
        // basename of the command
        cmd.split('/').last().unwrap_or(cmd)
    });

    format!(
        "Current statusLine setup\n\
         Type: command\n\
         Command: {}\n\
         Padding: {}\n\
         {}\n\
         {}\n\
         If you want to update it, run `/statusline <what to change>`.",
        cmd,
        inspection.padding.unwrap_or(0),
        script_line,
        summary_line
    )
}

/// Build the agent prompt for statusline configuration changes.
fn build_agent_prompt(args: &str, existing_command: Option<&str>) -> String {
    let trimmed = args.trim();
    if !trimmed.is_empty() {
        return trimmed.to_string();
    }

    match existing_command {
        Some(cmd) => format!(
            "Inspect my current statusLine setup first. ~/.mossen/settings.json already \
             contains statusLine.command = \"{}\". Explain or update that existing setup. \
             Do not ask whether a statusLine exists. Only import from my shell PS1 if I \
             explicitly ask to replace it from PS1.",
            cmd
        ),
        None => {
            "Inspect my current statusLine setup first. If a statusLine is already configured, \
                 explain or update that existing setup. Only import from my shell PS1 if no \
                 statusLine is configured or if I explicitly ask to replace it from PS1."
                .to_string()
        }
    }
}

#[async_trait]
impl Directive for StatuslineDirective {
    fn name(&self) -> &str {
        "statusline"
    }

    fn description(&self) -> &str {
        "Set up the Mossen status line UI"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        let trimmed = args.join(" ");
        let trimmed = trimmed.trim();

        if trimmed.is_empty() {
            // No args: inspect and show current status
            let inspection = inspect_statusline(ctx);
            let summary = build_statusline_summary(&inspection, ctx);
            return Ok(CommandResult::System(summary));
        }

        // Args provided: trigger agent-based statusline setup
        let existing_command = get_existing_statusline_command(ctx);
        let prompt = build_agent_prompt(trimmed, existing_command.as_deref());

        Ok(CommandResult::System(format!(
            "Inspecting and updating the current statusLine setup…\n\
             Agent prompt: {}",
            prompt
        )))
    }
}
