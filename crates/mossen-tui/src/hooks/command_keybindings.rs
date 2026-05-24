//! Command keybindings hook.
//!
//! Registers keybinding handlers for command bindings within the
//! keybinding system context.

use std::collections::HashMap;

/// A command keybinding registration.
#[derive(Debug, Clone)]
pub struct CommandKeybinding {
    pub command: String,
    pub key_sequence: String,
    pub context: String,
    pub description: String,
    pub enabled: bool,
}

/// State for managing command keybindings.
#[derive(Debug, Clone)]
pub struct CommandKeybindingsState {
    pub bindings: Vec<CommandKeybinding>,
    pub active_bindings: HashMap<String, String>,
    pub context_active: bool,
}

impl CommandKeybindingsState {
    pub fn new() -> Self {
        Self {
            bindings: Vec::new(),
            active_bindings: HashMap::new(),
            context_active: false,
        }
    }

    /// Register a new command keybinding.
    pub fn register(&mut self, binding: CommandKeybinding) {
        if binding.enabled {
            self.active_bindings
                .insert(binding.key_sequence.clone(), binding.command.clone());
        }
        self.bindings.push(binding);
    }

    /// Unregister a command keybinding.
    pub fn unregister(&mut self, command: &str) {
        self.bindings.retain(|b| b.command != command);
        self.active_bindings.retain(|_, v| v != command);
    }

    /// Look up a command for a key sequence.
    pub fn lookup(&self, key_sequence: &str) -> Option<&str> {
        if !self.context_active {
            return None;
        }
        self.active_bindings.get(key_sequence).map(|s| s.as_str())
    }

    /// Set whether the keybinding context is active.
    pub fn set_context_active(&mut self, active: bool) {
        self.context_active = active;
    }

    /// Get all registered bindings for display.
    pub fn all_bindings(&self) -> &[CommandKeybinding] {
        &self.bindings
    }
}

impl Default for CommandKeybindingsState {
    fn default() -> Self {
        Self::new()
    }
}

/// One slash-command name to invoke when the corresponding keybinding
/// fires. Translated from the per-action callback map in
/// `CommandKeybindingHandlers`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandKeybindingDispatch {
    pub action: String,
    pub command_name: String,
}

/// `CommandKeybindingHandlers` — pure-logic translation. Reads `command:*`
/// actions from the active keybinding context and returns the dispatch
/// map the caller should wire into `useKeybindings`.
///
/// TS source: `CommandKeybindingHandlers({ onSubmit, isActive })`. The
/// JSX wrapper is dropped — the function returns the dispatch list.
pub fn command_keybinding_handlers(
    actions: &[String],
    is_active: bool,
    is_modal_overlay_active: bool,
) -> Vec<CommandKeybindingDispatch> {
    if !is_active || is_modal_overlay_active {
        return Vec::new();
    }
    actions
        .iter()
        .filter_map(|a| {
            a.strip_prefix("command:")
                .map(|cmd| CommandKeybindingDispatch {
                    action: a.clone(),
                    command_name: cmd.to_string(),
                })
        })
        .collect()
}
