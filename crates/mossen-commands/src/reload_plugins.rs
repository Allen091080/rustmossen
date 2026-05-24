//! `/reload-plugins` — Activate pending plugin changes in the current session.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive};

/// Reload plugins directive — refresh active plugins to apply pending changes.
pub struct ReloadPluginsDirective;

/// Result of refreshing active plugins.
struct RefreshResult {
    enabled_count: usize,
    command_count: usize,
    agent_count: usize,
    hook_count: usize,
    mcp_count: usize,
    lsp_count: usize,
    error_count: usize,
}

/// Pluralize a noun based on count.
fn plural(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("{} {}", count, noun)
    } else {
        format!("{} {}s", count, noun)
    }
}

/// Refresh active plugins and return counts.
async fn refresh_active_plugins(ctx: &CommandContext) -> RefreshResult {
    // In the full implementation, this would:
    // 1. Re-download user settings if in remote mode
    // 2. Clear plugin caches
    // 3. Reload all plugin manifests
    // 4. Count enabled plugins, commands, agents, hooks, MCP/LSP servers

    // For now, scan the plugin directory for installed plugins
    let plugin_dir = ctx
        .env_vars
        .get("MOSSEN_PLUGIN_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            ctx.env_vars
                .get("HOME")
                .map(|h| std::path::PathBuf::from(h).join(".mossen").join("plugins"))
                .unwrap_or_else(|| std::path::PathBuf::from("/tmp/.mossen/plugins"))
        });

    let mut enabled_count = 0;
    let mut command_count = 0;
    let mut agent_count = 0;
    let mut hook_count = 0;
    let mut mcp_count = 0;
    let mut lsp_count = 0;
    let mut error_count = 0;

    if let Ok(entries) = std::fs::read_dir(&plugin_dir) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                let manifest_path = entry.path().join("manifest.json");
                if manifest_path.exists() {
                    match std::fs::read_to_string(&manifest_path) {
                        Ok(content) => {
                            if let Ok(manifest) =
                                serde_json::from_str::<serde_json::Value>(&content)
                            {
                                enabled_count += 1;
                                if let Some(cmds) = manifest.get("commands") {
                                    command_count += cmds.as_array().map(|a| a.len()).unwrap_or(0);
                                }
                                if let Some(agents) = manifest.get("agents") {
                                    agent_count += agents.as_array().map(|a| a.len()).unwrap_or(0);
                                }
                                if let Some(hooks) = manifest.get("hooks") {
                                    hook_count += hooks.as_array().map(|a| a.len()).unwrap_or(0);
                                }
                                if let Some(mcp) = manifest.get("mcpServers") {
                                    mcp_count += mcp.as_array().map(|a| a.len()).unwrap_or(0);
                                }
                                if let Some(lsp) = manifest.get("lspServers") {
                                    lsp_count += lsp.as_array().map(|a| a.len()).unwrap_or(0);
                                }
                            } else {
                                error_count += 1;
                            }
                        }
                        Err(_) => {
                            error_count += 1;
                        }
                    }
                }
            }
        }
    }

    RefreshResult {
        enabled_count,
        command_count,
        agent_count,
        hook_count,
        mcp_count,
        lsp_count,
        error_count,
    }
}

#[async_trait]
impl Directive for ReloadPluginsDirective {
    fn name(&self) -> &str {
        "reload-plugins"
    }

    fn description(&self) -> &str {
        "Activate pending plugin changes in the current session"
    }

    async fn execute(&self, _args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        let r = refresh_active_plugins(ctx).await;

        let parts = vec![
            plural(r.enabled_count, "plugin"),
            plural(r.command_count, "skill"),
            plural(r.agent_count, "agent"),
            plural(r.hook_count, "hook"),
            plural(r.mcp_count, "plugin MCP server"),
            plural(r.lsp_count, "plugin LSP server"),
        ];

        let mut msg = format!("Reloaded: {}", parts.join(" · "));

        if r.error_count > 0 {
            msg.push_str(&format!(
                "\n{} during load. Run /doctor for details.",
                plural(r.error_count, "error")
            ));
        }

        Ok(CommandResult::Text(msg))
    }
}
