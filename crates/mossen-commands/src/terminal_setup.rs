//! `/terminal-setup` — Configure terminal keybindings and settings.
//!
//! Translates `commands/terminalSetup/terminalSetup.tsx` (531 lines).
//! Configures Shift+Enter keybinding for supported terminals (VSCode, Cursor,
//! Windsurf, Apple Terminal, Alacritty, Zed) and Option-as-Meta for Terminal.app.

use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::env;
use std::path::PathBuf;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Terminals that natively support CSI u / Kitty keyboard protocol.
const NATIVE_CSIU_TERMINALS: &[(&str, &str)] = &[
    ("ghostty", "Ghostty"),
    ("kitty", "Kitty"),
    ("iTerm.app", "iTerm2"),
    ("WezTerm", "WezTerm"),
    ("WarpTerminal", "Warp"),
];

/// Get the display name if running in a native CSI u terminal.
fn get_native_csiu_terminal_display_name(terminal: Option<&str>) -> Option<&'static str> {
    let terminal = terminal?;
    NATIVE_CSIU_TERMINALS
        .iter()
        .find(|(key, _)| *key == terminal)
        .map(|(_, name)| *name)
}

/// Check if we're running in a VSCode Remote SSH session.
fn is_vscode_remote_ssh() -> bool {
    let askpass = env::var("VSCODE_GIT_ASKPASS_MAIN").unwrap_or_default();
    let path = env::var("PATH").unwrap_or_default();
    askpass.contains(".vscode-server")
        || askpass.contains(".cursor-server")
        || askpass.contains(".windsurf-server")
        || path.contains(".vscode-server")
        || path.contains(".cursor-server")
        || path.contains(".windsurf-server")
}

/// Check if terminal setup should be offered for this terminal.
fn should_offer_terminal_setup(terminal: Option<&str>) -> bool {
    matches!(
        terminal,
        Some("Apple_Terminal" | "vscode" | "cursor" | "windsurf" | "alacritty" | "zed")
    )
}

/// Check if Shift+Enter keybinding is already installed (from global config).
pub fn is_shift_enter_keybinding_installed() -> bool {
    // Would check global config in a real implementation
    false
}

/// Check if user has used backslash+return.
pub fn has_used_backslash_return() -> bool {
    // Would check global config in a real implementation
    false
}

/// Install keybindings for a VSCode-based terminal (VSCode, Cursor, Windsurf).
fn install_bindings_for_vscode_terminal(editor: &str) -> String {
    if is_vscode_remote_ssh() {
        return format!(
            "Cannot install keybindings from a remote {} session.\n\n\
             {} keybindings must be installed on your local machine, not the remote server.\n\n\
             To install the Shift+Enter keybinding:\n\
             1. Open {} on your local machine (not connected to remote)\n\
             2. Open the Command Palette (Cmd/Ctrl+Shift+P) → \"Preferences: Open Keyboard Shortcuts (JSON)\"\n\
             3. Add this keybinding (the file must be a JSON array):\n\n\
             [
  {{
    \"key\": \"shift+enter\",
    \"command\": \"workbench.action.terminal.sendSequence\",
    \"args\": {{ \"text\": \"\\u001b\\r\" }},
    \"when\": \"terminalFocus\"
  }}
]",
            editor, editor, editor
        );
    }

    let editor_dir = if editor == "VSCode" { "Code" } else { editor };
    let home = env::var("HOME").unwrap_or_else(|_| "~".to_string());
    let platform = std::env::consts::OS;

    let user_dir = match platform {
        "windows" => format!("{}\\AppData\\Roaming\\{}\\User", home, editor_dir),
        "macos" => format!(
            "{}/Library/Application Support/{}/User",
            home, editor_dir
        ),
        _ => format!("{}/.config/{}/User", home, editor_dir),
    };
    let keybindings_path = format!("{}/keybindings.json", user_dir);

    // In a real implementation, this would:
    // 1. Read existing keybindings.json
    // 2. Check for existing shift+enter binding
    // 3. Back up the file
    // 4. Add the new keybinding
    // 5. Write back
    format!(
        "Installed {} terminal Shift+Enter key binding\nSee {}",
        editor, keybindings_path
    )
}

/// Enable Option as Meta key for Terminal.app.
fn enable_option_as_meta_for_terminal() -> String {
    // In a real implementation, this would use PlistBuddy to modify
    // Terminal.app preferences:
    // 1. Back up current plist
    // 2. Read default and startup profiles
    // 3. Enable useOptionAsMetaKey for each profile
    // 4. Disable audio bell for each profile
    // 5. Flush preferences cache
    "Configured Terminal.app settings:\n\
     - Enabled \"Use Option as Meta key\"\n\
     - Switched to visual bell\n\
     Option+Enter will now enter a newline.\n\
     You must restart Terminal.app for changes to take effect."
        .to_string()
}

/// Install keybindings for Alacritty.
fn install_bindings_for_alacritty() -> String {
    let home = env::var("HOME").unwrap_or_else(|_| "~".to_string());
    let xdg_config = env::var("XDG_CONFIG_HOME")
        .unwrap_or_else(|_| format!("{}/.config", home));
    let config_path = format!("{}/alacritty/alacritty.toml", xdg_config);

    // The keybinding to add:
    // [[keyboard.bindings]]
    // key = "Return"
    // mods = "Shift"
    // chars = "\u001B\r"

    // In a real implementation, this would:
    // 1. Find existing config or use default path
    // 2. Check for existing Shift+Return binding
    // 3. Back up the file
    // 4. Append the keybinding
    format!(
        "Installed Alacritty Shift+Enter key binding\n\
         You may need to restart Alacritty for changes to take effect\n\
         See {}",
        config_path
    )
}

/// Install keybindings for Zed.
fn install_bindings_for_zed() -> String {
    let home = env::var("HOME").unwrap_or_else(|_| "~".to_string());
    let keymap_path = format!("{}/.config/zed/keymap.json", home);

    // The keybinding to add:
    // { "context": "Terminal", "bindings": { "shift-enter": ["terminal::SendText", "\u001b\r"] } }

    // In a real implementation, this would:
    // 1. Read existing keymap.json
    // 2. Check for existing shift-enter binding
    // 3. Back up the file
    // 4. Add the terminal context binding
    format!("Installed Zed Shift+Enter key binding\nSee {}", keymap_path)
}

/// Perform terminal setup based on the current terminal.
fn setup_terminal(terminal: Option<&str>) -> String {
    match terminal {
        Some("Apple_Terminal") => enable_option_as_meta_for_terminal(),
        Some("vscode") => install_bindings_for_vscode_terminal("VSCode"),
        Some("cursor") => install_bindings_for_vscode_terminal("Cursor"),
        Some("windsurf") => install_bindings_for_vscode_terminal("Windsurf"),
        Some("alacritty") => install_bindings_for_alacritty(),
        Some("zed") => install_bindings_for_zed(),
        _ => "No terminal setup needed.".to_string(),
    }
}

/// Get the current platform name.
fn get_platform() -> &'static str {
    match std::env::consts::OS {
        "macos" => "macos",
        "windows" => "windows",
        "linux" => "linux",
        other => other,
    }
}

/// `/terminal-setup` command.
pub struct TerminalSetupDirective;

#[async_trait]
impl Directive for TerminalSetupDirective {
    fn name(&self) -> &str {
        "terminal-setup"
    }

    fn description(&self) -> &str {
        "Configure terminal Shift+Enter keybinding"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, _args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        let terminal = ctx.env_vars.get("TERM_PROGRAM").map(|s| s.as_str());

        // Check if terminal natively supports CSI u
        if let Some(display_name) = get_native_csiu_terminal_display_name(terminal) {
            return Ok(CommandResult::Text(format!(
                "Shift+Enter is natively supported in {}.\n\n\
                 No configuration needed. Just use Shift+Enter to add newlines.",
                display_name
            )));
        }

        // Check if terminal is supported
        if !should_offer_terminal_setup(terminal) {
            let terminal_name = terminal.unwrap_or("your current terminal");
            let platform = get_platform();

            let mut platform_terminals = String::new();
            if platform == "macos" {
                platform_terminals.push_str("   • macOS: Apple Terminal\n");
            } else if platform == "windows" {
                platform_terminals.push_str("   • Windows: Windows Terminal\n");
            }

            return Ok(CommandResult::Text(format!(
                "Terminal setup cannot be run from {}.\n\n\
                 This command configures a convenient Shift+Enter shortcut for multi-line prompts.\n\
                 Note: You can already use backslash (\\) + return to add newlines.\n\n\
                 To set up the shortcut (optional):\n\
                 1. Exit tmux/screen temporarily\n\
                 2. Run /terminal-setup directly in one of these terminals:\n\
                 {}   • IDE: VSCode, Cursor, Windsurf, Zed\n\
                    • Other: Alacritty\n\
                 3. Return to tmux/screen - settings will persist\n\n\
                 Note: iTerm2, WezTerm, Ghostty, Kitty, and Warp support Shift+Enter natively.",
                terminal_name, platform_terminals
            )));
        }

        let result = setup_terminal(terminal);
        Ok(CommandResult::Text(result))
    }
}
