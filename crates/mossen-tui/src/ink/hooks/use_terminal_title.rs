//! TerminalTitle hook (use-terminal-title.ts).
//! Sets the terminal window title.

#[derive(Debug, Clone)]
pub struct TerminalTitleHookState {
    pub active: bool,
    pub title: String,
}
impl TerminalTitleHookState {
    pub fn new() -> Self { Self { active: true, title: String::new() } }
    pub fn set_title(&mut self, title: &str) { self.title = title.to_string(); }
    pub fn get_title(&self) -> &str { &self.title }
    pub fn to_escape_sequence(&self) -> String { format!("\x1b]2;{}\x07", self.title) }
}
impl Default for TerminalTitleHookState { fn default() -> Self { Self::new() } }

/// Hook-equivalent useTerminalTitle.
pub fn use_terminal_title(state: &mut TerminalTitleHookState, title: &str) -> String {
    state.set_title(title);
    state.to_escape_sequence()
}
