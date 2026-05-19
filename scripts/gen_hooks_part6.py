#!/usr/bin/env python3
"""Generate hooks/notifs and hooks/tool_permission modules."""
import os

BASE = "/Users/allen/Documents/rustmossen/crates/mossen-tui/src/hooks"
files = []

# notifs/mod.rs
files.append(("notifs/mod.rs", '''//! Notification hooks — one-shot or reactive notifications shown in the UI.

mod auto_mode_unavailable;
mod can_switch_subscription;
mod deprecation_warning;
mod fast_mode;
mod ide_status_indicator;
mod install_messages;
mod lsp_initialization;
mod mcp_connectivity_status;
mod model_migration;
mod npm_deprecation;
mod plugin_autoupdate;
mod plugin_installation_status;
mod rate_limit_warning;
mod settings_errors;
mod startup;
mod teammate_shutdown;

pub use auto_mode_unavailable::*;
pub use can_switch_subscription::*;
pub use deprecation_warning::*;
pub use fast_mode::*;
pub use ide_status_indicator::*;
pub use install_messages::*;
pub use lsp_initialization::*;
pub use mcp_connectivity_status::*;
pub use model_migration::*;
pub use npm_deprecation::*;
pub use plugin_autoupdate::*;
pub use plugin_installation_status::*;
pub use rate_limit_warning::*;
pub use settings_errors::*;
pub use startup::*;
pub use teammate_shutdown::*;
'''))

# Individual notif files
notif_hooks = [
    ("auto_mode_unavailable", "AutoModeUnavailable", "Shows notification when auto mode is unavailable (settings, circuit-breaker, org-allowlist)."),
    ("can_switch_subscription", "CanSwitchSubscription", "Notifies when user can switch to an existing subscription."),
    ("deprecation_warning", "DeprecationWarning", "Shows deprecation warnings for deprecated features."),
    ("fast_mode", "FastMode", "Notification for fast mode activation/status."),
    ("ide_status_indicator", "IdeStatusIndicator", "Shows IDE connection status in the notification area."),
    ("install_messages", "InstallMessages", "Shows messages during plugin/extension installation."),
    ("lsp_initialization", "LspInitialization", "Notification for LSP server initialization progress."),
    ("mcp_connectivity_status", "McpConnectivityStatus", "Shows MCP server connectivity status notifications."),
    ("model_migration", "ModelMigration", "Notifications for model migration/deprecation events."),
    ("npm_deprecation", "NpmDeprecation", "Warns about deprecated npm installation method."),
    ("plugin_autoupdate", "PluginAutoupdate", "Notification when plugins are auto-updated."),
    ("plugin_installation_status", "PluginInstallationStatus", "Shows plugin installation progress and status."),
    ("rate_limit_warning", "RateLimitWarning", "Warns when approaching or hitting rate limits."),
    ("settings_errors", "SettingsErrors", "Shows notification for invalid settings configuration."),
    ("startup", "Startup", "One-shot startup notifications (welcome, changelog, etc.)."),
    ("teammate_shutdown", "TeammateShutdown", "Notification when a teammate session shuts down."),
]

for fname, struct_prefix, doc in notif_hooks:
    struct_name = f"{struct_prefix}NotificationState"
    content = f'''//! {struct_prefix} notification ({fname}.ts).
//! {doc}

#[derive(Debug, Clone)]
pub struct {struct_name} {{
    pub shown: bool,
    pub dismissed: bool,
    pub notification_key: String,
    pub message: Option<String>,
}}

impl {struct_name} {{
    pub fn new() -> Self {{
        Self {{
            shown: false,
            dismissed: false,
            notification_key: "{fname.replace("_", "-")}".to_string(),
            message: None,
        }}
    }}

    /// Check conditions and set notification message if needed.
    pub fn check(&mut self, should_show: bool, message: &str) {{
        if should_show && !self.shown && !self.dismissed {{
            self.shown = true;
            self.message = Some(message.to_string());
        }}
    }}

    /// Dismiss the notification.
    pub fn dismiss(&mut self) {{
        self.dismissed = true;
        self.message = None;
    }}

    /// Get the notification message if active.
    pub fn active_message(&self) -> Option<&str> {{
        if self.shown && !self.dismissed {{
            self.message.as_deref()
        }} else {{
            None
        }}
    }}

    /// Reset state for re-evaluation.
    pub fn reset(&mut self) {{
        self.shown = false;
        self.message = None;
    }}
}}

impl Default for {struct_name} {{
    fn default() -> Self {{
        Self::new()
    }}
}}
'''
    files.append((f"notifs/{fname}.rs", content))

# tool_permission/mod.rs
files.append(("tool_permission/mod.rs", '''//! Tool permission handling — manages permission requests for tool execution.

mod permission_context;
mod permission_logging;
mod handlers;

pub use permission_context::*;
pub use permission_logging::*;
pub use handlers::*;
'''))

files.append(("tool_permission/permission_context.rs", '''//! Permission context (PermissionContext.ts).
//! Provides the permission mode and tool approval state.

/// Permission mode for tool execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionMode {
    Default,
    Plan,
    Auto,
}

impl PermissionMode {
    pub fn from_str(s: &str) -> Self {
        match s {
            "plan" => Self::Plan,
            "auto" => Self::Auto,
            _ => Self::Default,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Default => "default",
            Self::Plan => "plan",
            Self::Auto => "auto",
        }
    }
}

/// State for the permission context.
#[derive(Debug, Clone)]
pub struct PermissionContextState {
    pub mode: PermissionMode,
    pub is_auto_mode_available: bool,
    pub always_approved_tools: Vec<String>,
    pub session_approved: Vec<String>,
}

impl PermissionContextState {
    pub fn new() -> Self {
        Self {
            mode: PermissionMode::Default,
            is_auto_mode_available: false,
            always_approved_tools: Vec::new(),
            session_approved: Vec::new(),
        }
    }

    /// Set the permission mode.
    pub fn set_mode(&mut self, mode: PermissionMode) {
        self.mode = mode;
    }

    /// Cycle to the next permission mode.
    pub fn cycle_mode(&mut self) {
        self.mode = match self.mode {
            PermissionMode::Default => PermissionMode::Plan,
            PermissionMode::Plan => {
                if self.is_auto_mode_available {
                    PermissionMode::Auto
                } else {
                    PermissionMode::Default
                }
            }
            PermissionMode::Auto => PermissionMode::Default,
        };
    }

    /// Check if a tool is approved (always or for this session).
    pub fn is_tool_approved(&self, tool_name: &str) -> bool {
        self.mode == PermissionMode::Auto
            || self.always_approved_tools.contains(&tool_name.to_string())
            || self.session_approved.contains(&tool_name.to_string())
    }

    /// Approve a tool for this session.
    pub fn approve_for_session(&mut self, tool_name: &str) {
        if !self.session_approved.contains(&tool_name.to_string()) {
            self.session_approved.push(tool_name.to_string());
        }
    }

    /// Approve a tool permanently.
    pub fn approve_always(&mut self, tool_name: &str) {
        if !self.always_approved_tools.contains(&tool_name.to_string()) {
            self.always_approved_tools.push(tool_name.to_string());
        }
    }
}

impl Default for PermissionContextState {
    fn default() -> Self {
        Self::new()
    }
}
'''))

files.append(("tool_permission/permission_logging.rs", '''//! Permission logging (permissionLogging.ts).
//! Logs permission decisions for auditing.

use std::time::Instant;

#[derive(Debug, Clone)]
pub struct PermissionLogEntry {
    pub tool_name: String,
    pub decision: PermissionDecision,
    pub mode: String,
    pub timestamp: Instant,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionDecision {
    Approved,
    Denied,
    AutoApproved,
    SessionApproved,
}

#[derive(Debug, Clone)]
pub struct PermissionLoggingState {
    pub entries: Vec<PermissionLogEntry>,
    pub max_entries: usize,
}

impl PermissionLoggingState {
    pub fn new() -> Self {
        Self { entries: Vec::new(), max_entries: 1000 }
    }

    pub fn log(&mut self, tool_name: &str, decision: PermissionDecision, mode: &str, reason: Option<String>) {
        self.entries.push(PermissionLogEntry {
            tool_name: tool_name.to_string(),
            decision,
            mode: mode.to_string(),
            timestamp: Instant::now(),
            reason,
        });
        if self.entries.len() > self.max_entries {
            self.entries.remove(0);
        }
    }

    pub fn recent(&self, count: usize) -> &[PermissionLogEntry] {
        let start = self.entries.len().saturating_sub(count);
        &self.entries[start..]
    }

    pub fn denied_count(&self) -> usize {
        self.entries.iter().filter(|e| e.decision == PermissionDecision::Denied).count()
    }
}

impl Default for PermissionLoggingState {
    fn default() -> Self {
        Self::new()
    }
}
'''))

files.append(("tool_permission/handlers.rs", '''//! Permission handlers (handlers/*.ts).
//! Different handlers for interactive, coordinator, and swarm worker modes.

/// Handler type for permission processing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionHandlerType {
    Interactive,
    Coordinator,
    SwarmWorker,
}

/// A permission request to be handled.
#[derive(Debug, Clone)]
pub struct PermissionRequest {
    pub id: String,
    pub tool_name: String,
    pub tool_input: serde_json::Value,
    pub description: String,
    pub handler_type: PermissionHandlerType,
}

/// Response to a permission request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionResponse {
    Allow,
    Deny,
    AllowAlways,
    AllowSession,
}

/// State for permission handler management.
#[derive(Debug, Clone)]
pub struct PermissionHandlerState {
    pub handler_type: PermissionHandlerType,
    pub pending_request: Option<PermissionRequest>,
    pub auto_approve_patterns: Vec<String>,
}

impl PermissionHandlerState {
    pub fn new(handler_type: PermissionHandlerType) -> Self {
        Self {
            handler_type,
            pending_request: None,
            auto_approve_patterns: Vec::new(),
        }
    }

    /// Submit a permission request for handling.
    pub fn submit_request(&mut self, request: PermissionRequest) {
        // Check auto-approve patterns
        if self.should_auto_approve(&request.tool_name) {
            // Auto-approved, no need to queue
            return;
        }
        self.pending_request = Some(request);
    }

    /// Respond to the pending request.
    pub fn respond(&mut self, response: PermissionResponse) -> Option<PermissionRequest> {
        let request = self.pending_request.take();
        if let Some(ref req) = request {
            if response == PermissionResponse::AllowAlways {
                self.auto_approve_patterns.push(req.tool_name.clone());
            }
        }
        request
    }

    /// Check if a tool should be auto-approved.
    fn should_auto_approve(&self, tool_name: &str) -> bool {
        self.auto_approve_patterns.iter().any(|p| p == tool_name || p == "*")
    }

    /// Check if there is a pending request.
    pub fn has_pending(&self) -> bool {
        self.pending_request.is_some()
    }

    /// Get the pending request for display.
    pub fn pending(&self) -> Option<&PermissionRequest> {
        self.pending_request.as_ref()
    }
}

impl Default for PermissionHandlerState {
    fn default() -> Self {
        Self::new(PermissionHandlerType::Interactive)
    }
}
'''))

for fname, content in files:
    path = os.path.join(BASE, fname)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, 'w') as f:
        f.write(content)
    print(f"Created {fname}")

print(f"\nTotal: {len(files)} files created")
