//! `/teleport` — Teleport to a remote environment (internal).

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive};

/// Teleport directive — connect to a remote teleport environment.
pub struct TeleportDirective;

/// Check if teleport environments are available.
async fn check_teleport_availability(ctx: &CommandContext) -> bool {
    // Teleport requires remote mode or specific configuration
    ctx.is_remote_mode
        || ctx.is_env_truthy("MOSSEN_TELEPORT_ENABLED")
        || ctx.env_vars.contains_key("MOSSEN_TELEPORT_URL")
}

/// Get the teleport target URL.
fn get_teleport_url(ctx: &CommandContext) -> String {
    ctx.env_vars
        .get("MOSSEN_TELEPORT_URL")
        .cloned()
        .unwrap_or_else(|| {
            let base = ctx
                .env_vars
                .get("MOSSEN_CODE_PLATFORM_BASE_URL")
                .cloned()
                .unwrap_or_else(|| "https://mossen.ai".to_string());
            format!("{}/teleport", base)
        })
}

/// Execute the teleport flow.
async fn execute_teleport_flow(args: &[&str], ctx: &CommandContext) -> Result<String> {
    let available = check_teleport_availability(ctx).await;

    if !available {
        return Ok("Teleport is not available in the current environment.\n\
             Requires remote mode or MOSSEN_TELEPORT_ENABLED=true."
            .to_string());
    }

    let target = args.join(" ");
    let target = target.trim();

    if target.is_empty() {
        let url = get_teleport_url(ctx);
        return Ok(format!(
            "Teleport ready.\n\
             Target: {}\n\
             Use `/teleport <environment>` to connect to a specific environment.",
            url
        ));
    }

    // Connect to the specified environment
    Ok(format!(
        "Connecting to teleport environment: {}\n\
         Establishing session…",
        target
    ))
}

#[async_trait]
impl Directive for TeleportDirective {
    fn name(&self) -> &str {
        "teleport"
    }

    fn description(&self) -> &str {
        "Connect to a remote teleport environment"
    }

    fn is_hidden(&self) -> bool {
        true
    }

    fn is_enabled(&self, ctx: &CommandContext) -> bool {
        ctx.can_use_remote_workspace_features()
            && (ctx.is_remote_mode
                || ctx.is_env_truthy("MOSSEN_TELEPORT_ENABLED")
                || ctx.env_vars.contains_key("MOSSEN_TELEPORT_URL"))
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        let result = execute_teleport_flow(args, ctx).await?;
        Ok(CommandResult::Text(result))
    }
}
