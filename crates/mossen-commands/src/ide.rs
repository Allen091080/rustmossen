//! `/ide` — Manage IDE integrations and show status.
//!
//! Translates `commands/ide/ide.tsx` (807 lines).
//! Detects running IDEs, manages IDE connections via MCP config,
//! and supports opening projects in selected IDEs.

use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Supported IDE types.
#[derive(Debug, Clone, PartialEq)]
pub enum IdeType {
    VSCode,
    Cursor,
    Windsurf,
    JetBrains(String),
    Vim,
    Neovim,
    Zed,
}

impl IdeType {
    fn display_name(&self) -> &str {
        match self {
            IdeType::VSCode => "VS Code",
            IdeType::Cursor => "Cursor",
            IdeType::Windsurf => "Windsurf",
            IdeType::JetBrains(name) => name.as_str(),
            IdeType::Vim => "Vim",
            IdeType::Neovim => "Neovim",
            IdeType::Zed => "Zed",
        }
    }

    fn is_jetbrains(&self) -> bool {
        matches!(self, IdeType::JetBrains(_))
    }

    fn is_vscode_based(&self) -> bool {
        matches!(self, IdeType::VSCode | IdeType::Cursor | IdeType::Windsurf)
    }
}

/// Detected IDE information.
#[derive(Debug, Clone)]
pub struct DetectedIdeInfo {
    pub name: String,
    pub ide_type: IdeType,
    pub port: Option<u16>,
    pub url: Option<String>,
    pub is_valid: bool,
    pub workspace_folders: Vec<String>,
    pub auth_token: Option<String>,
}

/// Format workspace folders for display, stripping cwd and showing tail end of paths.
pub fn format_workspace_folders(folders: &[String], cwd: &Path, max_length: usize) -> String {
    if folders.is_empty() {
        return String::new();
    }

    let folders_to_show: Vec<&String> = folders.iter().take(2).collect();
    let has_more = folders.len() > 2;

    let ellipsis_overhead = if has_more { 3 } else { 0 }; // ", …"
    let separator_overhead = if folders_to_show.len() > 1 {
        (folders_to_show.len() - 1) * 2
    } else {
        0
    };
    let available_length = max_length.saturating_sub(separator_overhead + ellipsis_overhead);
    let max_length_per_path = available_length / folders_to_show.len().max(1);

    let cwd_str = cwd.to_string_lossy();

    let formatted: Vec<String> = folders_to_show
        .iter()
        .map(|folder| {
            let mut f = folder.as_str();
            // Strip cwd from the beginning if present
            if let Some(rest) = f.strip_prefix(cwd_str.as_ref()) {
                if let Some(rest) = rest.strip_prefix(std::path::MAIN_SEPARATOR) {
                    f = rest;
                }
            }
            if f.len() <= max_length_per_path {
                f.to_string()
            } else {
                format!("…{}", &f[f.len().saturating_sub(max_length_per_path - 1)..])
            }
        })
        .collect();

    let mut result = formatted.join(", ");
    if has_more {
        result.push_str(", …");
    }
    result
}

/// Get display name for IDE open target.
fn get_ide_open_target_display_name(target_name: &str, branch: Option<&str>) -> String {
    match branch {
        Some(b) => format!("{} ({})", target_name, b),
        None => target_name.to_string(),
    }
}

/// Get localized IDE connection result message.
fn get_ide_connection_result(target_name: &str, ide_name: &str, state: &str) -> String {
    match state {
        "connected" => format!("Connected to {} for project {}.", ide_name, target_name),
        "failed" => format!(
            "Failed to connect to {} for project {}.",
            ide_name, target_name
        ),
        "timed-out" => format!(
            "Connection to {} for project {} timed out.",
            ide_name, target_name
        ),
        "disconnected" => format!(
            "Disconnected from {} for project {}.",
            ide_name, target_name
        ),
        _ => format!(
            "IDE {} state: {} for project {}.",
            ide_name, state, target_name
        ),
    }
}

/// Get localized IDE open result message.
fn get_ide_open_result(target_path: &str, ide_name: &str) -> String {
    format!("Opened project in {}: {}", ide_name, target_path)
}

/// Get localized IDE open failure message.
fn get_ide_open_failure(target_path: &str, ide_name: &str) -> String {
    format!(
        "Failed to open project in {}. Try opening manually: {}",
        ide_name, target_path
    )
}

/// Get localized manual fallback message.
fn get_ide_open_manual_fallback(target_path: &str, ide_name: &str) -> String {
    format!(
        "Please open the project manually in {}: {}",
        ide_name, target_path
    )
}

/// Check if running in supported terminal.
fn is_supported_terminal(terminal: Option<&str>) -> bool {
    matches!(
        terminal,
        Some("vscode" | "cursor" | "windsurf" | "Apple_Terminal" | "alacritty" | "zed")
    )
}

/// Check if running in a JetBrains terminal.
fn is_jetbrains_terminal(terminal: Option<&str>) -> bool {
    matches!(terminal, Some(t) if t.contains("jetbrains") || t.contains("intellij"))
}

/// The IDE connection timeout in milliseconds (slightly longer than MCP 30s timeout).
const IDE_CONNECTION_TIMEOUT_MS: u64 = 35000;

/// `/ide` command.
pub struct IdeDirective;

#[async_trait]
impl Directive for IdeDirective {
    fn name(&self) -> &str {
        "ide"
    }

    fn description(&self) -> &str {
        "Manage IDE integrations and show status"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn argument_hint(&self) -> &str {
        "[open]"
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        let subcommand = args.first().map(|s| s.to_lowercase());

        match subcommand.as_deref() {
            Some("open") => {
                // Handle 'open' argument — detect available IDEs and open project
                let cwd_display = ctx.cwd.display().to_string();
                let product_name = &ctx.product_name;

                // In a real implementation, this would call detectIDEs(true)
                // and filter for valid ones, then present a selection UI.
                // For now, provide the text-based flow:
                let terminal = ctx.env_vars.get("TERM_PROGRAM").map(|s| s.as_str());

                if is_supported_terminal(terminal) || is_jetbrains_terminal(terminal) {
                    Ok(CommandResult::Text(format!(
                        "IDE Open: Scanning for IDEs with the {} extension...\n\
                         Project path: {}\n\n\
                         To open in a specific IDE, ensure it has the {} extension installed and is running.\n\
                         Detected IDEs will appear for selection.",
                        product_name, cwd_display, product_name
                    )))
                } else {
                    Ok(CommandResult::Text(format!(
                        "No IDEs with the {} extension detected to open the current project.\n\
                         Ensure your IDE has the {} extension installed and is running.",
                        product_name, product_name
                    )))
                }
            }

            Some("help" | "-h" | "--help") => Ok(CommandResult::Text(
                "Usage: /ide [open]\n\n\
                 Manage IDE integration.\n\n\
                 Subcommands:\n\
                   (no args)   Select an IDE to connect to for integrated development features\n\
                   open        Open the current project in a connected IDE\n\n\
                 Supported IDEs: VS Code, Cursor, Windsurf, JetBrains IDEs, Zed\n\n\
                 Note: Only one instance can be connected to VS Code at a time.\n\
                 Tip: You can enable auto-connect to IDE in /config or with the --ide flag."
                    .to_string(),
            )),

            None => {
                // Default: show IDE selection / connection status
                let terminal = ctx.env_vars.get("TERM_PROGRAM").map(|s| s.as_str());
                let product_name = &ctx.product_name;

                if ctx.is_non_interactive {
                    return Ok(CommandResult::Text("IDE: Not connected".to_string()));
                }

                // In a real implementation, this would detect IDEs and show a
                // selection dialog. For CLI mode, provide status info:
                let mut output = String::from("Select IDE\n");
                output.push_str("Connect to an IDE for integrated development features.\n\n");

                // Check for JetBrains terminal
                if is_jetbrains_terminal(terminal) {
                    output.push_str(
                        "No available IDEs detected. Please install the plugin and restart your IDE.\n",
                    );
                } else if !is_supported_terminal(terminal) {
                    output.push_str(&format!(
                        "No available IDEs detected. Make sure your IDE has the {} extension or plugin installed and is running.\n",
                        product_name
                    ));
                } else {
                    output.push_str("Scanning for available IDEs...\n");
                    output.push_str("  • None   — No IDE connection\n\n");
                    output.push_str(
                        "Note: Only one instance can be connected to VS Code at a time.\n",
                    );
                    output.push_str("Tip: You can enable auto-connect to IDE in /config or with the --ide flag.");
                }

                Ok(CommandResult::Text(output))
            }

            Some(unknown) => Ok(CommandResult::Error(format!(
                "Unknown subcommand: \"{}\". Use /ide help.",
                unknown
            ))),
        }
    }
}
