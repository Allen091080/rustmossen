//! IDE integration hook.
//!
//! Manages the full integration with an IDE: file syncing, selection
//! tracking, and command forwarding.

use std::collections::HashMap;

/// An IDE command that can be forwarded.
#[derive(Debug, Clone)]
pub struct IdeCommand {
    pub command: String,
    pub args: HashMap<String, serde_json::Value>,
}

/// State for IDE integration.
#[derive(Debug, Clone)]
pub struct IdeIntegrationState {
    pub connected: bool,
    pub active_file: Option<String>,
    pub selection: Option<IdeSelection>,
    pub pending_commands: Vec<IdeCommand>,
    pub synced_files: Vec<String>,
    pub ide_type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct IdeSelection {
    pub file_path: String,
    pub start_line: u32,
    pub end_line: u32,
    pub text: String,
}

impl IdeIntegrationState {
    pub fn new() -> Self {
        Self {
            connected: false,
            active_file: None,
            selection: None,
            pending_commands: Vec::new(),
            synced_files: Vec::new(),
            ide_type: None,
        }
    }

    /// Update the active file in IDE.
    pub fn set_active_file(&mut self, path: String) {
        self.active_file = Some(path);
    }

    /// Update the current selection.
    pub fn set_selection(&mut self, selection: IdeSelection) {
        self.selection = Some(selection);
    }

    /// Clear the selection.
    pub fn clear_selection(&mut self) {
        self.selection = None;
    }

    /// Queue a command to send to the IDE.
    pub fn send_command(&mut self, command: IdeCommand) {
        self.pending_commands.push(command);
    }

    /// Take all pending commands.
    pub fn take_commands(&mut self) -> Vec<IdeCommand> {
        std::mem::take(&mut self.pending_commands)
    }

    /// Open a file in the IDE.
    pub fn open_file(&mut self, path: &str, line: Option<u32>) {
        let mut args = HashMap::new();
        args.insert(
            "path".to_string(),
            serde_json::Value::String(path.to_string()),
        );
        if let Some(l) = line {
            args.insert("line".to_string(), serde_json::Value::Number(l.into()));
        }
        self.send_command(IdeCommand {
            command: "openFile".to_string(),
            args,
        });
    }

    /// Show a diff in the IDE.
    pub fn show_diff(&mut self, old_path: &str, new_path: &str, title: &str) {
        let mut args = HashMap::new();
        args.insert(
            "oldPath".to_string(),
            serde_json::Value::String(old_path.to_string()),
        );
        args.insert(
            "newPath".to_string(),
            serde_json::Value::String(new_path.to_string()),
        );
        args.insert(
            "title".to_string(),
            serde_json::Value::String(title.to_string()),
        );
        self.send_command(IdeCommand {
            command: "showDiff".to_string(),
            args,
        });
    }

    pub fn set_connected(&mut self, connected: bool, ide_type: Option<String>) {
        self.connected = connected;
        self.ide_type = ide_type;
    }
}

impl Default for IdeIntegrationState {
    fn default() -> Self {
        Self::new()
    }
}
