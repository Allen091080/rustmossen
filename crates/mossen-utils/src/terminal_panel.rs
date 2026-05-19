//! Built-in terminal panel toggled with Meta+J.
//!
//! Uses tmux for shell persistence: a separate tmux server with a per-instance
//! socket holds the shell session. Falls back to a non-persistent shell when
//! tmux is not available.

use std::env;
use std::process::Command;
use std::sync::Mutex;

use once_cell::sync::Lazy;

const TMUX_SESSION: &str = "panel";

/// Get the tmux socket name for the terminal panel.
/// Uses a unique socket per Mossen instance (based on session ID).
pub fn get_terminal_panel_socket(session_id: &str) -> String {
    let prefix = &session_id[..8.min(session_id.len())];
    format!("mossen-panel-{}", prefix)
}

/// Singleton terminal panel state.
static INSTANCE: Lazy<Mutex<TerminalPanel>> = Lazy::new(|| Mutex::new(TerminalPanel::new()));

/// Return the singleton TerminalPanel.
pub fn get_terminal_panel() -> &'static Mutex<TerminalPanel> {
    &INSTANCE
}

/// Terminal panel that manages tmux-based shell sessions.
pub struct TerminalPanel {
    has_tmux: Option<bool>,
    cleanup_registered: bool,
}

impl TerminalPanel {
    fn new() -> Self {
        Self {
            has_tmux: None,
            cleanup_registered: false,
        }
    }

    /// Toggle the terminal panel (show/hide).
    pub fn toggle(&mut self, session_id: &str, cwd: &str) {
        self.show_shell(session_id, cwd);
    }

    /// Check if tmux is available.
    fn check_tmux(&mut self) -> bool {
        if let Some(has) = self.has_tmux {
            return has;
        }
        let result = Command::new("tmux").arg("-V").output();
        let has = result.map(|o| o.status.success()).unwrap_or(false);
        if !has {
            tracing::debug!("Terminal panel: tmux not found, falling back to non-persistent shell");
        }
        self.has_tmux = Some(has);
        has
    }

    /// Check if a tmux session exists.
    fn has_session(&self, session_id: &str) -> bool {
        let socket = get_terminal_panel_socket(session_id);
        Command::new("tmux")
            .args(["-L", &socket, "has-session", "-t", TMUX_SESSION])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Create a new tmux session.
    fn create_session(&mut self, session_id: &str, cwd: &str) -> bool {
        let shell = env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
        let socket = get_terminal_panel_socket(session_id);

        let result = Command::new("tmux")
            .args([
                "-L", &socket, "new-session", "-d", "-s", TMUX_SESSION, "-c", cwd, &shell, "-l",
            ])
            .output();

        match result {
            Ok(output) if output.status.success() => {}
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                tracing::debug!(
                    "Terminal panel: failed to create tmux session: {}",
                    stderr
                );
                return false;
            }
            Err(e) => {
                tracing::debug!("Terminal panel: failed to spawn tmux: {}", e);
                return false;
            }
        }

        // Bind Meta+J and configure status bar
        let _ = Command::new("tmux")
            .args([
                "-L",
                &socket,
                "bind-key",
                "-n",
                "M-j",
                "detach-client",
                ";",
                "set-option",
                "-g",
                "status-style",
                "bg=default",
                ";",
                "set-option",
                "-g",
                "status-left",
                "",
                ";",
                "set-option",
                "-g",
                "status-right",
                " Alt+J to return to Mossen ",
                ";",
                "set-option",
                "-g",
                "status-right-style",
                "fg=brightblack",
            ])
            .output();

        if !self.cleanup_registered {
            self.cleanup_registered = true;
            // In Rust, cleanup is handled by the caller or a drop guard
            // The socket name is stored for cleanup on exit
        }

        true
    }

    /// Attach to an existing tmux session.
    fn attach_session(&self, session_id: &str) {
        let socket = get_terminal_panel_socket(session_id);
        let _ = Command::new("tmux")
            .args(["-L", &socket, "attach-session", "-t", TMUX_SESSION])
            .stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status();
    }

    /// Ensure a tmux session exists, creating one if needed.
    fn ensure_session(&mut self, session_id: &str, cwd: &str) -> bool {
        if self.has_session(session_id) {
            return true;
        }
        self.create_session(session_id, cwd)
    }

    /// Fallback when tmux is not available — runs a non-persistent shell.
    fn run_shell_direct(&self, cwd: &str) {
        let shell = env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
        let _ = Command::new(&shell)
            .args(["-i", "-l"])
            .current_dir(cwd)
            .stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status();
    }

    /// Show shell (main logic).
    fn show_shell(&mut self, session_id: &str, cwd: &str) {
        if self.check_tmux() && self.ensure_session(session_id, cwd) {
            self.attach_session(session_id);
        } else {
            self.run_shell_direct(cwd);
        }
    }
}

/// Kill the tmux server for cleanup on exit.
pub fn cleanup_terminal_panel(session_id: &str) {
    let socket = get_terminal_panel_socket(session_id);
    let _ = Command::new("tmux")
        .args(["-L", &socket, "kill-server"])
        .spawn();
}
