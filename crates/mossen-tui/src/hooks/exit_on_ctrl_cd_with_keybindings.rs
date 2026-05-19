//! Exit on Ctrl+C/D with keybindings (useExitOnCtrlCDWithKeybindings.ts).
//!
//! Extended version that integrates with the keybinding system,
//! allowing custom exit key sequences.

use super::exit_on_ctrl_cd::{ExitAction, ExitOnCtrlCDState};

/// State for exit with keybinding integration.
#[derive(Debug, Clone)]
pub struct ExitOnCtrlCDWithKeybindingsState {
    pub inner: ExitOnCtrlCDState,
    pub custom_exit_binding: Option<String>,
    pub keybinding_context_active: bool,
}

impl ExitOnCtrlCDWithKeybindingsState {
    pub fn new() -> Self {
        Self {
            inner: ExitOnCtrlCDState::new(),
            custom_exit_binding: None,
            keybinding_context_active: false,
        }
    }

    /// Handle an input key, checking both standard and custom bindings.
    pub fn handle_key(&mut self, key: &str, has_input: bool, input_empty: bool) -> ExitAction {
        // Check custom exit binding first
        if let Some(ref binding) = self.custom_exit_binding {
            if key == binding && self.keybinding_context_active {
                return ExitAction::Exit;
            }
        }

        // Fall through to standard Ctrl+C/D handling
        match key {
            "ctrl+c" | "ctrl-c" => self.inner.on_ctrl_c(has_input),
            "ctrl+d" | "ctrl-d" => self.inner.on_ctrl_d(input_empty),
            _ => ExitAction::None,
        }
    }

    /// Set a custom exit keybinding.
    pub fn set_custom_binding(&mut self, binding: Option<String>) {
        self.custom_exit_binding = binding;
    }

    pub fn tick(&mut self) {
        self.inner.tick();
    }
}

impl Default for ExitOnCtrlCDWithKeybindingsState {
    fn default() -> Self {
        Self::new()
    }
}
