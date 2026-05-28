//! `/assistant` — Connect to a running assistant session (local-widget).

use anyhow::Result;
use async_trait::async_trait;
use std::path::PathBuf;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Assistant directive — connect to or set up an assistant session.
pub struct AssistantDirective;

/// Compute the default installation directory for the assistant.
fn compute_default_install_dir(ctx: &CommandContext) -> PathBuf {
    let config_home = ctx
        .env_vars
        .get("MOSSEN_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            ctx.env_vars
                .get("HOME")
                .map(|h| PathBuf::from(h).join(".mossen"))
                .unwrap_or_else(|| PathBuf::from("/tmp/.mossen"))
        });
    config_home.join("assistant")
}

/// Execute the assistant connection flow.
async fn execute_assistant_flow(args: &[&str], ctx: &CommandContext) -> Result<String> {
    let target = args.join(" ");
    let target = target.trim();

    if target.is_empty() {
        return Ok("Assistant session support is available.".to_string());
    }

    // If a session target is specified, attempt to connect to it
    Ok(format!(
        "Assistant session support is available. Session: {}",
        target
    ))
}

#[async_trait]
impl Directive for AssistantDirective {
    fn name(&self) -> &str {
        "assistant"
    }

    fn description(&self) -> &str {
        "Connect to a running assistant session"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn is_enabled(&self, ctx: &CommandContext) -> bool {
        ctx.is_env_truthy("MOSSEN_DEFERRED_SLASH_ASSISTANT")
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        let result = execute_assistant_flow(args, ctx).await?;
        Ok(CommandResult::Text(result))
    }
}
