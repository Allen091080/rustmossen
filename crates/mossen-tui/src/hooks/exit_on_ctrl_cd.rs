//! Exit on Ctrl+C/D hook (useExitOnCtrlCD.ts).
//!
//! Double-press Ctrl+C or Ctrl+D to exit the application.

use super::double_press::{DoublePressAction, DoublePressState};

/// State for Ctrl+C/D exit behavior.
#[derive(Debug, Clone)]
pub struct ExitOnCtrlCDState {
    pub ctrl_c_press: DoublePressState,
    pub ctrl_d_press: DoublePressState,
    pub show_exit_message: bool,
    pub exit_key: Option<String>,
}

impl ExitOnCtrlCDState {
    pub fn new() -> Self {
        Self {
            ctrl_c_press: DoublePressState::new(),
            ctrl_d_press: DoublePressState::new(),
            show_exit_message: false,
            exit_key: None,
        }
    }

    /// Handle Ctrl+C press. Returns true if should exit.
    pub fn on_ctrl_c(&mut self, has_input: bool) -> ExitAction {
        if has_input {
            // Clear input on first press when there's text
            return ExitAction::ClearInput;
        }
        match self.ctrl_c_press.press() {
            DoublePressAction::FirstPress => {
                self.show_exit_message = true;
                self.exit_key = Some("Ctrl-C".to_string());
                ExitAction::ShowMessage
            }
            DoublePressAction::DoublePress => {
                self.show_exit_message = false;
                ExitAction::Exit
            }
        }
    }

    /// Handle Ctrl+D press. Returns true if should exit.
    pub fn on_ctrl_d(&mut self, input_empty: bool) -> ExitAction {
        if !input_empty {
            return ExitAction::DeleteForward;
        }
        match self.ctrl_d_press.press() {
            DoublePressAction::FirstPress => {
                self.show_exit_message = true;
                self.exit_key = Some("Ctrl-D".to_string());
                ExitAction::ShowMessage
            }
            DoublePressAction::DoublePress => {
                self.show_exit_message = false;
                ExitAction::Exit
            }
        }
    }

    /// Tick timeout state.
    pub fn tick(&mut self) {
        if self.ctrl_c_press.tick() || self.ctrl_d_press.tick() {
            self.show_exit_message = false;
            self.exit_key = None;
        }
    }
}

impl Default for ExitOnCtrlCDState {
    fn default() -> Self {
        Self::new()
    }
}

/// Action resulting from Ctrl+C/D handling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExitAction {
    ShowMessage,
    Exit,
    ClearInput,
    DeleteForward,
    None,
}
