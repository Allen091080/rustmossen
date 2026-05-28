//! Plugin command argument parsing
//!
//! This module provides argument parsing for the /plugin command.

/// Parsed plugin command types
#[derive(Debug, Clone)]
pub enum ParsedCommand {
    /// Show plugin menu
    Menu,
    /// Show help
    Help,
    /// Install a plugin
    Install {
        plugin: Option<String>,
        marketplace: Option<String>,
    },
    /// Install plan with dry-run or confirm
    InstallPlan {
        plugin: Option<String>,
        scope: Option<String>,
        confirm_token: Option<String>,
    },
    /// Manage plugins
    Manage,
    /// Uninstall a plugin
    Uninstall { plugin: Option<String> },
    /// Enable a plugin
    Enable { plugin: Option<String> },
    /// Disable a plugin
    Disable { plugin: Option<String> },
    /// Validate a plugin path
    Validate { path: Option<String> },
    /// Marketplace operations
    Marketplace {
        action: Option<String>,
        target: Option<String>,
    },
    /// Marketplace add plan
    MarketplaceAddPlan {
        target: Option<String>,
        confirm_token: Option<String>,
    },
    /// Prune plugins
    Prune { confirm_token: Option<String> },
    /// Show plugin status
    Status,
    /// Show plugin sources
    Sources,
    /// Show plugin paths
    Paths,
}

/// Parse plugin command arguments
pub fn parse_plugin_args(args: Option<&str>) -> ParsedCommand {
    let args = match args {
        Some(a) => a,
        None => return ParsedCommand::Menu,
    };

    let parts: Vec<&str> = args.split_whitespace().collect();
    let command = parts.first().map(|s| s.to_lowercase()).unwrap_or_default();

    match command.as_str() {
        "help" | "--help" | "-h" => ParsedCommand::Help,

        "install" | "i" => {
            if parts.len() > 1 && parts[1] == "--dry-run" {
                let scope = parts
                    .iter()
                    .position(|&p| p == "--scope")
                    .and_then(|idx| parts.get(idx + 1).map(|s| s.to_string()));
                let plugin = parts[2..]
                    .iter()
                    .filter(|&&p| p != "--scope")
                    .filter(|&&p| {
                        let idx = parts.iter().position(|&x| x == p).unwrap_or(0);
                        idx == 0 || parts[idx - 1] != "--scope"
                    })
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
                    .join(" ");
                ParsedCommand::InstallPlan {
                    plugin: if plugin.is_empty() {
                        None
                    } else {
                        Some(plugin)
                    },
                    scope,
                    confirm_token: None,
                }
            } else if parts.len() > 1 && parts[1] == "--confirm" {
                ParsedCommand::InstallPlan {
                    plugin: None,
                    scope: None,
                    confirm_token: parts.get(2).map(|s| s.to_string()),
                }
            } else {
                let target = parts.get(1).map(|s| s.to_string());
                if let Some(ref t) = target {
                    if t.contains('@') {
                        let parts: Vec<&str> = t.split('@').collect();
                        ParsedCommand::Install {
                            plugin: parts.first().map(|s| s.to_string()),
                            marketplace: parts.get(1).map(|s| s.to_string()),
                        }
                    } else if t.starts_with("http://")
                        || t.starts_with("https://")
                        || t.starts_with("file://")
                        || t.contains('/')
                        || t.contains('\\')
                    {
                        ParsedCommand::Install {
                            plugin: None,
                            marketplace: target,
                        }
                    } else {
                        ParsedCommand::Install {
                            plugin: target,
                            marketplace: None,
                        }
                    }
                } else {
                    ParsedCommand::Install {
                        plugin: None,
                        marketplace: None,
                    }
                }
            }
        }

        "manage" => ParsedCommand::Manage,

        "uninstall" | "remove" | "rm" => ParsedCommand::Uninstall {
            plugin: parts.get(1).map(|s| s.to_string()),
        },

        "enable" => ParsedCommand::Enable {
            plugin: parts.get(1).map(|s| s.to_string()),
        },

        "disable" => ParsedCommand::Disable {
            plugin: parts.get(1).map(|s| s.to_string()),
        },

        "validate" => ParsedCommand::Validate {
            path: parts.get(1..).map(|p| p.join(" ")),
        },

        "status" | "stat" => ParsedCommand::Status,

        "sources" | "source" => ParsedCommand::Sources,

        "paths" | "path" => ParsedCommand::Paths,

        "prune" => {
            let confirm_token = parts
                .iter()
                .position(|&p| p == "--confirm")
                .and_then(|idx| parts.get(idx + 1).map(|s| s.to_string()));
            ParsedCommand::Prune { confirm_token }
        }

        "marketplace" | "market" => {
            let action = parts.get(1).map(|s| s.to_lowercase());
            let rest: Vec<&str> = parts[2..].to_vec();
            let target = if rest.is_empty() {
                None
            } else {
                Some(rest.join(" "))
            };

            match action.as_deref() {
                Some("add") => {
                    if rest.len() > 1 && rest[0] == "--dry-run" {
                        ParsedCommand::MarketplaceAddPlan {
                            target: Some(rest[1..].join(" ")),
                            confirm_token: None,
                        }
                    } else if rest.len() > 1 && rest[0] == "--confirm" {
                        ParsedCommand::MarketplaceAddPlan {
                            target: None,
                            confirm_token: rest.get(1).map(|s| s.to_string()),
                        }
                    } else {
                        ParsedCommand::Marketplace {
                            action: Some("add".to_string()),
                            target,
                        }
                    }
                }
                Some("remove") | Some("rm") => ParsedCommand::Marketplace {
                    action: Some("remove".to_string()),
                    target,
                },
                Some("update") => ParsedCommand::Marketplace {
                    action: Some("update".to_string()),
                    target,
                },
                Some("list") => ParsedCommand::Marketplace {
                    action: Some("list".to_string()),
                    target: None,
                },
                _ => ParsedCommand::Marketplace {
                    action: None,
                    target: None,
                },
            }
        }

        _ => ParsedCommand::Menu,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_args() {
        assert!(matches!(parse_plugin_args(None), ParsedCommand::Menu));
    }

    #[test]
    fn test_help_command() {
        let result = parse_plugin_args(Some("help"));
        assert!(matches!(result, ParsedCommand::Help));
    }

    #[test]
    fn test_install_command() {
        let result = parse_plugin_args(Some("install my-plugin"));
        match result {
            ParsedCommand::Install {
                plugin,
                marketplace: _,
            } => {
                assert_eq!(plugin, Some("my-plugin".to_string()));
            }
            _ => panic!("Expected Install command"),
        }
    }

    #[test]
    fn test_install_with_marketplace() {
        let result = parse_plugin_args(Some("install my-plugin@npm"));
        match result {
            ParsedCommand::Install {
                plugin,
                marketplace,
            } => {
                assert_eq!(plugin, Some("my-plugin".to_string()));
                assert_eq!(marketplace, Some("npm".to_string()));
            }
            _ => panic!("Expected Install command"),
        }
    }

    #[test]
    fn test_status_command() {
        let result = parse_plugin_args(Some("status"));
        assert!(matches!(result, ParsedCommand::Status));
    }

    #[test]
    fn test_validate_command() {
        let result = parse_plugin_args(Some("validate /path/to/plugin"));
        match result {
            ParsedCommand::Validate { path } => {
                assert_eq!(path, Some("/path/to/plugin".to_string()));
            }
            _ => panic!("Expected Validate command"),
        }
    }

    #[test]
    fn test_prune_sources_paths_and_plan_commands() {
        assert!(matches!(
            parse_plugin_args(Some("prune --confirm abc123")),
            ParsedCommand::Prune {
                confirm_token: Some(token)
            } if token == "abc123"
        ));
        assert!(matches!(
            parse_plugin_args(Some("sources")),
            ParsedCommand::Sources
        ));
        assert!(matches!(
            parse_plugin_args(Some("paths")),
            ParsedCommand::Paths
        ));

        assert!(matches!(
            parse_plugin_args(Some("marketplace add --dry-run owner/repo")),
            ParsedCommand::MarketplaceAddPlan {
                target: Some(target),
                confirm_token: None,
            } if target == "owner/repo"
        ));
        assert!(matches!(
            parse_plugin_args(Some("marketplace add --confirm deadbeef")),
            ParsedCommand::MarketplaceAddPlan {
                target: None,
                confirm_token: Some(token),
            } if token == "deadbeef"
        ));

        assert!(matches!(
            parse_plugin_args(Some("install --dry-run demo@market --scope project")),
            ParsedCommand::InstallPlan {
                plugin: Some(plugin),
                scope: Some(scope),
                confirm_token: None,
            } if plugin == "demo@market" && scope == "project"
        ));
        assert!(matches!(
            parse_plugin_args(Some("install --confirm feedface")),
            ParsedCommand::InstallPlan {
                plugin: None,
                scope: None,
                confirm_token: Some(token),
            } if token == "feedface"
        ));
    }
}
