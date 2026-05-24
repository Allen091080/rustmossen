//! Command execution context and result types.

use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// Command execution context — provides access to session state,
/// configuration, and services needed by command implementations.
#[derive(Debug, Clone)]
pub struct CommandContext {
    /// Current working directory.
    pub cwd: PathBuf,
    /// Whether the session is in non-interactive (headless) mode.
    pub is_non_interactive: bool,
    /// Whether the session is in remote mode.
    pub is_remote_mode: bool,
    /// Whether custom backend is enabled.
    pub is_custom_backend: bool,
    /// Current user type (e.g., "internal" for internal users).
    pub user_type: Option<String>,
    /// Environment variables snapshot.
    pub env_vars: HashMap<String, String>,
    /// Product display name.
    pub product_name: String,
    /// Product CLI name.
    pub cli_name: String,
    /// Current version string.
    pub version: String,
    /// Build time string.
    pub build_time: Option<String>,
}

impl CommandContext {
    /// Check if an environment variable is truthy ("1", "true", "yes").
    pub fn is_env_truthy(&self, key: &str) -> bool {
        self.env_vars
            .get(key)
            .map(|v| matches!(v.as_str(), "1" | "true" | "yes" | "TRUE" | "YES"))
            .unwrap_or(false)
    }

    /// Check if the current user is an internal user.
    pub fn is_internal_user(&self) -> bool {
        self.user_type.as_deref() == Some("internal")
    }
}

/// Result of a command execution.
#[derive(Debug, Clone)]
pub enum CommandResult {
    /// Text output to display.
    Text(String),
    /// System message display.
    System(String),
    /// Command completed with no output.
    Empty,
    /// Command produced a widget/UI (placeholder for Phase 5 TUI).
    Widget,
    /// Command requests exit.
    Exit(Option<String>),
    /// Command produced an error message.
    Error(String),
}

/// Command type classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectiveType {
    /// Pure logic command with text result.
    Local,
    /// Command that renders UI (deferred to Phase 5).
    LocalWidget,
    /// Prompt-type command (sends to model).
    Prompt,
}

/// The core Command (Directive) trait that all slash commands implement.
#[async_trait]
pub trait Directive: Send + Sync {
    /// Primary command name (e.g., "help", "exit").
    fn name(&self) -> &str;

    /// Alternative names for the command.
    fn aliases(&self) -> &[&str] {
        &[]
    }

    /// Human-readable description.
    fn description(&self) -> &str;

    /// Command type.
    fn directive_type(&self) -> DirectiveType {
        DirectiveType::Local
    }

    /// Whether this command is hidden from help listings.
    fn is_hidden(&self) -> bool {
        false
    }

    /// Whether this command is enabled in the current context.
    fn is_enabled(&self, _ctx: &CommandContext) -> bool {
        true
    }

    /// Argument hint for help display (e.g., "[on|off]").
    fn argument_hint(&self) -> &str {
        ""
    }

    /// Whether this command executes immediately without model interaction.
    fn is_immediate(&self) -> bool {
        false
    }

    /// Whether this command supports non-interactive (headless) mode.
    fn supports_non_interactive(&self) -> bool {
        false
    }

    /// Execute the command with given arguments and context.
    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult>;
}

/// Type alias for a boxed directive.
pub type BoxedDirective = Box<dyn Directive>;
