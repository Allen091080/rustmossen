//! `/install-slack-app` — Install the Mossen Slack app.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive};

/// Slack app directive — install the Mossen Slack app.
pub struct SlackAppDirective;

/// Get the Slack app installation URL.
fn get_slack_app_url(ctx: &CommandContext) -> String {
    let base = ctx
        .env_vars
        .get("MOSSEN_CODE_PLATFORM_BASE_URL")
        .cloned()
        .unwrap_or_else(|| "https://mossen.ai".to_string());
    format!("{}/integrations/slack/install", base)
}

/// Attempt to open a URL in the default browser.
fn open_browser(url: &str) -> bool {
    let result = if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(url).status()
    } else if cfg!(target_os = "linux") {
        std::process::Command::new("xdg-open").arg(url).status()
    } else if cfg!(target_os = "windows") {
        std::process::Command::new("cmd")
            .args(["/C", "start", url])
            .status()
    } else {
        return false;
    };

    result.map(|s| s.success()).unwrap_or(false)
}

#[async_trait]
impl Directive for SlackAppDirective {
    fn name(&self) -> &str {
        "install-slack-app"
    }

    fn description(&self) -> &str {
        "Install the Mossen Slack app"
    }

    fn is_enabled(&self, ctx: &CommandContext) -> bool {
        ctx.can_use_hosted_platform_features()
    }

    async fn execute(&self, _args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        let slack_url = get_slack_app_url(ctx);

        // Track install attempt (in production: logEvent + saveGlobalConfig)
        let success = open_browser(&slack_url);

        if success {
            Ok(CommandResult::Text(
                "Opening Slack app installation page in browser…".to_string(),
            ))
        } else {
            Ok(CommandResult::Text(format!(
                "Couldn't open browser. Visit: {}",
                slack_url
            )))
        }
    }
}
