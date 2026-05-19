//! Top-level screens — REPL, ResumeConversation, Doctor (screens/*.tsx).

/// REPL screen state.
#[derive(Debug, Clone, Default)]
pub struct REPL {
    pub input: String,
    pub history: Vec<String>,
    pub cursor: usize,
    pub messages: Vec<String>,
}

impl REPL {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn submit(&mut self) -> String {
        let v = std::mem::take(&mut self.input);
        self.history.push(v.clone());
        v
    }
}

/// Resume-conversation screen state — choose a previous session to resume.
#[derive(Debug, Clone, Default)]
pub struct ResumeConversation {
    pub sessions: Vec<String>,
    pub selected: usize,
    pub query: String,
}

impl ResumeConversation {
    pub fn new(sessions: Vec<String>) -> Self {
        Self {
            sessions,
            selected: 0,
            query: String::new(),
        }
    }
    pub fn current(&self) -> Option<&str> {
        self.sessions.get(self.selected).map(|s| s.as_str())
    }
}

/// Doctor screen state — runs diagnostics + shows the result panel.
#[derive(Debug, Clone, Default)]
pub struct Doctor {
    pub diagnostics: Vec<(String, bool)>,
    pub running: bool,
    pub completed: bool,
}

impl Doctor {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn set_diagnostics(&mut self, items: Vec<(String, bool)>) {
        self.diagnostics = items;
        self.completed = true;
    }
}

/// REPL.tsx exports `type Props = { commands, debug, initialTools, ... }`.
/// Concrete props struct for the REPL screen entry-point (string-typed fields
/// where the Rust port has not yet wired up the equivalent typed value).
#[derive(Debug, Clone, Default)]
pub struct ReplProps {
    pub commands: Vec<String>,
    pub debug: bool,
    pub initial_tools: Vec<String>,
    pub initial_messages: Vec<String>,
    pub initial_agent_name: Option<String>,
    pub initial_agent_color: Option<String>,
    pub mcp_clients: Vec<String>,
    pub auto_connect_ide_flag: bool,
    pub strict_mcp_config: bool,
    pub system_prompt: Option<String>,
    pub append_system_prompt: Option<String>,
    pub disabled: bool,
    pub disable_slash_commands: bool,
    pub task_list_id: Option<String>,
    pub thinking_config: Option<String>,
}

/// Mirror of TS `export type Screen = 'prompt' | 'transcript'`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Prompt,
    Transcript,
}

/// TS REPL.tsx exports `type Props`.
pub type Props = ReplProps;
