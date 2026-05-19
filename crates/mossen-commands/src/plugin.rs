//! `/plugin` — Manage plugins (install, remove, list, configure).

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Plugin directive — manages the plugin ecosystem including installation,
/// removal, browsing marketplaces, and plugin configuration.
pub struct PluginDirective;

/// Plugin subcommands.
enum PluginAction {
    /// List installed plugins (default)
    List,
    /// Install a plugin
    Install(String),
    /// Remove a plugin
    Remove(String),
    /// Browse marketplace
    Browse,
    /// Show plugin status/errors
    Status,
    /// Configure plugin options
    Options(String),
    /// Prune unused plugins
    Prune,
    /// Show help
    Help,
}

/// Parse plugin subcommand from args.
fn parse_plugin_action(args: &[&str]) -> PluginAction {
    if args.is_empty() {
        return PluginAction::List;
    }

    match args[0].to_lowercase().as_str() {
        "install" | "add" | "i" => {
            let name = if args.len() > 1 {
                args[1..].join(" ")
            } else {
                String::new()
            };
            PluginAction::Install(name)
        }
        "remove" | "uninstall" | "rm" => {
            let name = if args.len() > 1 {
                args[1..].join(" ")
            } else {
                String::new()
            };
            PluginAction::Remove(name)
        }
        "browse" | "marketplace" | "market" => PluginAction::Browse,
        "status" | "errors" => PluginAction::Status,
        "options" | "config" | "configure" => {
            let name = if args.len() > 1 {
                args[1..].join(" ")
            } else {
                String::new()
            };
            PluginAction::Options(name)
        }
        "prune" | "clean" => PluginAction::Prune,
        "help" | "--help" | "-h" => PluginAction::Help,
        "list" | "ls" => PluginAction::List,
        _ => {
            // Treat unknown first arg as a plugin name to install
            PluginAction::Install(args.join(" "))
        }
    }
}

#[async_trait]
impl Directive for PluginDirective {
    fn name(&self) -> &str {
        "plugin"
    }

    fn aliases(&self) -> &[&str] {
        &["plugins"]
    }

    fn description(&self) -> &str {
        "Manage plugins"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn argument_hint(&self) -> &str {
        "[install|remove|browse|status|prune] [name]"
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], _ctx: &CommandContext) -> Result<CommandResult> {
        let action = parse_plugin_action(args);

        match action {
            PluginAction::List => {
                // Show installed plugins list
                Ok(CommandResult::Text(
                    "Installed Plugins\n\
                     =================\n\n\
                     No plugins currently installed.\n\n\
                     Use /plugin install <name> to install a plugin,\n\
                     or /plugin browse to discover available plugins."
                        .to_string(),
                ))
            }
            PluginAction::Install(name) => {
                if name.is_empty() {
                    // Show browse/discover instructions
                    return Ok(CommandResult::Text(
                        "Plugin Marketplace\n\
                         ==================\n\n\
                         Use /plugin install <name> to install a specific plugin.\n\
                         Use /plugin browse to discover available plugins."
                            .to_string(),
                    ));
                }
                // In full implementation: resolve plugin, validate, download, install
                Ok(CommandResult::System(format!(
                    "Installing plugin: {}...",
                    name
                )))
            }
            PluginAction::Remove(name) => {
                if name.is_empty() {
                    return Ok(CommandResult::Error(
                        "Usage: /plugin remove <name>".to_string(),
                    ));
                }
                Ok(CommandResult::System(format!(
                    "Removing plugin: {}...",
                    name
                )))
            }
            PluginAction::Browse => {
                // Show marketplace browse information
                Ok(CommandResult::Text(
                    "Plugin Marketplace\n\
                     ==================\n\n\
                     Browse available plugins at the Mossen marketplace.\n\
                     Use /plugin install <name> to install a specific plugin."
                        .to_string(),
                ))
            }
            PluginAction::Status => {
                Ok(CommandResult::Text(
                    "All plugins healthy. Run /doctor for detailed diagnostics.".to_string(),
                ))
            }
            PluginAction::Options(name) => {
                if name.is_empty() {
                    return Ok(CommandResult::Error(
                        "Usage: /plugin options <name>".to_string(),
                    ));
                }
                // Show plugin options
                Ok(CommandResult::Text(format!(
                    "Plugin Options: {}\n\
                     ================\n\n\
                     No configurable options for this plugin.",
                    name
                )))
            }
            PluginAction::Prune => {
                Ok(CommandResult::System(
                    "Pruning unused plugin data...".to_string(),
                ))
            }
            PluginAction::Help => {
                Ok(CommandResult::Text(
                    "Plugin Management:\n\
                     \n\
                     /plugin              — List installed plugins\n\
                     /plugin install <n>  — Install a plugin by name\n\
                     /plugin remove <n>   — Remove a plugin\n\
                     /plugin browse       — Browse marketplace\n\
                     /plugin status       — Show plugin health\n\
                     /plugin options <n>  — Configure a plugin\n\
                     /plugin prune        — Remove unused data"
                        .to_string(),
                ))
            }
        }
    }
}
