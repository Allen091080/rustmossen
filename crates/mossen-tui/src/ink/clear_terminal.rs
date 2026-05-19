//! Clear Terminal (clear-terminal.ts).

/// Build the ANSI sequence that clears the screen and homes the cursor.
pub fn get_clear_terminal_sequence() -> &'static str {
    "\x1b[2J\x1b[H"
}

/// Static command sequence used by callers that want the "clear" verb.
pub static CLEAR_TERMINAL: &str = "\x1b[2J\x1b[H";

/// Lower-case alias matching the TS name.
#[allow(non_upper_case_globals)]
pub static clearTerminal: &str = "\x1b[2J\x1b[H";

#[derive(Debug, Clone, Default)]
pub struct ClearTerminalState {
    pub initialized: bool,
}

impl ClearTerminalState {
    pub fn new() -> Self { Self { initialized: false } }
    pub fn initialize(&mut self) { self.initialized = true; }
}
