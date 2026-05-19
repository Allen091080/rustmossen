//! Terminal Notification (terminal-notification.ts).

/// One notification posted to the terminal via OSC 9.
#[derive(Debug, Clone, Default)]
pub struct TerminalNotification {
    pub title: String,
    pub body: String,
    pub urgent: bool,
}

/// Context for terminal-write functions (writer + escape mode).
#[derive(Debug, Clone, Default)]
pub struct TerminalWriteContext {
    pub raw_mode: bool,
    pub alt_screen: bool,
}

/// Provider — registers the terminal-write context for downstream hooks.
#[derive(Debug, Clone, Default)]
pub struct TerminalWriteProvider {
    pub context: TerminalWriteContext,
}

/// Hook-equivalent useTerminalNotification — push a notification and return
/// the OSC 9 string that should be written to stdout.
pub fn use_terminal_notification(n: &TerminalNotification) -> String {
    format!("\x1b]9;{}: {}\x07", n.title, n.body)
}

#[derive(Debug, Clone, Default)]
pub struct TerminalNotificationState {
    pub initialized: bool,
}

impl TerminalNotificationState {
    pub fn new() -> Self { Self { initialized: false } }
    pub fn initialize(&mut self) { self.initialized = true; }
}
