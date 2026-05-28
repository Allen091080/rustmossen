//! Teleport Resume hook (useTeleportResume.ts).
//! Resumes a teleported session after reconnection.

#[derive(Debug, Clone)]
pub struct TeleportResumeState {
    pub active: bool,
    pub initialized: bool,
}

impl TeleportResumeState {
    pub fn new() -> Self {
        Self {
            active: false,
            initialized: false,
        }
    }
    pub fn initialize(&mut self) {
        self.initialized = true;
    }
    pub fn activate(&mut self) {
        self.active = true;
    }
    pub fn deactivate(&mut self) {
        self.active = false;
    }
    pub fn is_active(&self) -> bool {
        self.active
    }
}
impl Default for TeleportResumeState {
    fn default() -> Self {
        Self::new()
    }
}

/// Error surface from a teleport resume attempt.
///
/// TS source: `export type TeleportResumeError`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeleportResumeError {
    pub message: String,
    pub formatted_message: Option<String>,
    pub is_operation_error: bool,
}

/// Where the resume request originated.
///
/// TS source: `export type TeleportSource = 'cliArg' | 'localCommand'`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeleportSource {
    CliArg,
    LocalCommand,
}

impl TeleportSource {
    pub fn as_str(self) -> &'static str {
        match self {
            TeleportSource::CliArg => "cliArg",
            TeleportSource::LocalCommand => "localCommand",
        }
    }
}
