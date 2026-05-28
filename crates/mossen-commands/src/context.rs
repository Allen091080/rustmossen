//! Command execution context and result types.

use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// Per-model token and cost usage visible to slash commands.
#[derive(Debug, Clone, Default)]
pub struct CommandCostModelUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub web_search_requests: u64,
    pub cost_usd: f64,
    pub context_window: u64,
    pub max_output_tokens: u64,
}

/// Runtime cost snapshot injected by the CLI/TUI host.
#[derive(Debug, Clone, Default)]
pub struct CommandCostSnapshot {
    pub total_cost_usd: f64,
    pub total_api_duration_ms: u64,
    pub total_api_duration_without_retries_ms: u64,
    pub total_tool_duration_ms: u64,
    pub total_lines_added: u64,
    pub total_lines_removed: u64,
    pub has_unknown_model_cost: bool,
    pub model_usage: HashMap<String, CommandCostModelUsage>,
}

impl CommandCostSnapshot {
    pub fn has_recorded_usage(&self) -> bool {
        self.total_cost_usd > 0.0
            || self.total_api_duration_ms > 0
            || self.total_tool_duration_ms > 0
            || self.total_lines_added > 0
            || self.total_lines_removed > 0
            || !self.model_usage.is_empty()
    }
}

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
    /// Current session cost and token usage snapshot.
    pub cost_snapshot: CommandCostSnapshot,
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

    /// Whether hosted/platform-backed commands should be visible.
    ///
    /// Personal local builds default this off. Future OpenAI-compatible,
    /// Anthropic, or Responses providers can still opt into platform features
    /// by setting explicit hosted URLs or an explicit hosted feature flag.
    pub fn can_use_hosted_platform_features(&self) -> bool {
        self.is_env_truthy("MOSSEN_ENABLE_HOSTED_COMMANDS")
            || self.is_env_truthy("MOSSEN_HOSTED_SUBSCRIBER")
            || self.is_env_truthy("MOSSEN_CODE_HOSTED_SUBSCRIBER")
            || self.has_configured_hosted_platform_url()
    }

    /// Whether remote workspace/session commands should be visible.
    pub fn can_use_remote_workspace_features(&self) -> bool {
        self.is_remote_mode
            || self.is_env_truthy("MOSSEN_ENABLE_REMOTE_COMMANDS")
            || self.can_use_hosted_platform_features()
    }

    /// Whether Chrome browser integration should be visible.
    pub fn can_use_chrome_integration(&self) -> bool {
        self.is_env_truthy("MOSSEN_CODE_ENABLE_CHROME") || self.can_use_hosted_platform_features()
    }

    fn has_configured_hosted_platform_url(&self) -> bool {
        [
            "MOSSEN_CODE_PLATFORM_BASE_URL",
            "MOSSEN_CODE_GITHUB_APP_URL",
            "MOSSEN_GITHUB_APP_INSTALL_URL",
            "MOSSEN_CODE_REMOTE_BASE_URL",
            "MOSSEN_CODE_REMOTE_SETUP_URL",
            "MOSSEN_SHARE_BASE_URL",
        ]
        .iter()
        .any(|key| {
            self.env_vars
                .get(*key)
                .map(|value| {
                    let value = value.trim();
                    !value.is_empty() && !is_placeholder_platform_url(value)
                })
                .unwrap_or(false)
        })
    }
}

fn is_placeholder_platform_url(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return true;
    };
    let host = parsed.host_str().unwrap_or("").to_ascii_lowercase();
    host.is_empty()
        || host == "platform.example"
        || host.ends_with(".example")
        || host == "api.mossen.invalid"
        || host == "platform.mossen.invalid"
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
