//! `/hooks` — View and manage hook configurations for tool events (local-widget).

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Watchers directive — view hook configurations for tool events.
pub struct WatchersDirective;

/// Represents a hook configuration entry.
#[derive(Debug, Clone)]
struct HookConfig {
    tool_name: String,
    hook_type: HookType,
    command: String,
    enabled: bool,
}

/// Types of hooks that can be attached to tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HookType {
    PreTool,
    PostTool,
}

impl HookType {
    fn as_str(&self) -> &'static str {
        match self {
            HookType::PreTool => "pre-tool",
            HookType::PostTool => "post-tool",
        }
    }
}

/// Load hook configurations from the current settings.
fn load_hooks(ctx: &CommandContext) -> Vec<HookConfig> {
    // In the full implementation, this reads from settings files.
    // Parse MOSSEN_HOOKS_CONFIG environment variable if available.
    let hooks_json = ctx
        .env_vars
        .get("MOSSEN_HOOKS_CONFIG")
        .cloned()
        .unwrap_or_else(|| "[]".to_string());

    // Parse JSON array of hook configs
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&hooks_json) {
        if let Some(arr) = parsed.as_array() {
            return arr
                .iter()
                .filter_map(|item| {
                    let tool_name = item.get("tool")?.as_str()?.to_string();
                    let hook_type_str = item.get("type")?.as_str()?;
                    let hook_type = match hook_type_str {
                        "pre-tool" => HookType::PreTool,
                        "post-tool" => HookType::PostTool,
                        _ => return None,
                    };
                    let command = item.get("command")?.as_str()?.to_string();
                    let enabled = item
                        .get("enabled")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(true);
                    Some(HookConfig {
                        tool_name,
                        hook_type,
                        command,
                        enabled,
                    })
                })
                .collect();
        }
    }

    Vec::new()
}

/// Format hook configurations for display.
fn format_hooks_display(hooks: &[HookConfig]) -> String {
    if hooks.is_empty() {
        return "No hooks configured.\n\n\
                Hooks allow you to run commands before or after tool execution.\n\
                Configure hooks in your settings file or via the settings UI."
            .to_string();
    }

    let mut output = String::from("Hook Configurations\n");
    output.push_str("====================\n\n");

    for hook in hooks {
        let status = if hook.enabled { "✓" } else { "○" };
        output.push_str(&format!(
            "  {} [{}] {} → {}\n",
            status,
            hook.hook_type.as_str(),
            hook.tool_name,
            hook.command,
        ));
    }

    let enabled_count = hooks.iter().filter(|h| h.enabled).count();
    output.push_str(&format!(
        "\n{} hook(s) configured, {} enabled.",
        hooks.len(),
        enabled_count
    ));

    output
}

#[async_trait]
impl Directive for WatchersDirective {
    fn name(&self) -> &str {
        "hooks"
    }

    fn description(&self) -> &str {
        "View hook configurations for tool events"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, _args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        let hooks = load_hooks(ctx);
        let display = format_hooks_display(&hooks);
        Ok(CommandResult::Text(display))
    }
}
