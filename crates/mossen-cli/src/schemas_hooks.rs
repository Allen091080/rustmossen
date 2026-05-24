// schemas_hooks.rs — Translation of schemas/hooks.ts
// Hook Zod schemas extracted to break import cycles.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// Hook Types
// ============================================================================

pub const HOOK_EVENTS: &[&str] = &[
    "PreToolUse",
    "PostToolUse",
    "Notification",
    "Stop",
    "SubagentStop",
];

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HookEvent {
    PreToolUse,
    PostToolUse,
    Notification,
    Stop,
    SubagentStop,
}

impl HookEvent {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "PreToolUse" => Some(Self::PreToolUse),
            "PostToolUse" => Some(Self::PostToolUse),
            "Notification" => Some(Self::Notification),
            "Stop" => Some(Self::Stop),
            "SubagentStop" => Some(Self::SubagentStop),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PreToolUse => "PreToolUse",
            Self::PostToolUse => "PostToolUse",
            Self::Notification => "Notification",
            Self::Stop => "Stop",
            Self::SubagentStop => "SubagentStop",
        }
    }
}

pub const SHELL_TYPES: &[&str] = &["bash", "powershell"];

// ============================================================================
// Hook Command Variants
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum HookCommand {
    #[serde(rename = "command")]
    BashCommand {
        command: String,
        #[serde(rename = "if", skip_serializing_if = "Option::is_none")]
        if_condition: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        shell: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        timeout: Option<f64>,
        #[serde(rename = "statusMessage", skip_serializing_if = "Option::is_none")]
        status_message: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        once: Option<bool>,
        #[serde(rename = "async", skip_serializing_if = "Option::is_none")]
        async_exec: Option<bool>,
        #[serde(rename = "asyncRewake", skip_serializing_if = "Option::is_none")]
        async_rewake: Option<bool>,
    },
    #[serde(rename = "prompt")]
    Prompt {
        prompt: String,
        #[serde(rename = "if", skip_serializing_if = "Option::is_none")]
        if_condition: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        timeout: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        #[serde(rename = "statusMessage", skip_serializing_if = "Option::is_none")]
        status_message: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        once: Option<bool>,
    },
    #[serde(rename = "http")]
    Http {
        url: String,
        #[serde(rename = "if", skip_serializing_if = "Option::is_none")]
        if_condition: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        timeout: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        headers: Option<HashMap<String, String>>,
        #[serde(rename = "allowedEnvVars", skip_serializing_if = "Option::is_none")]
        allowed_env_vars: Option<Vec<String>>,
        #[serde(rename = "statusMessage", skip_serializing_if = "Option::is_none")]
        status_message: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        once: Option<bool>,
    },
    #[serde(rename = "agent")]
    Agent {
        prompt: String,
        #[serde(rename = "if", skip_serializing_if = "Option::is_none")]
        if_condition: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        timeout: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        #[serde(rename = "statusMessage", skip_serializing_if = "Option::is_none")]
        status_message: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        once: Option<bool>,
    },
}

impl HookCommand {
    pub fn get_if_condition(&self) -> Option<&str> {
        match self {
            HookCommand::BashCommand { if_condition, .. } => if_condition.as_deref(),
            HookCommand::Prompt { if_condition, .. } => if_condition.as_deref(),
            HookCommand::Http { if_condition, .. } => if_condition.as_deref(),
            HookCommand::Agent { if_condition, .. } => if_condition.as_deref(),
        }
    }

    pub fn get_timeout(&self) -> Option<f64> {
        match self {
            HookCommand::BashCommand { timeout, .. } => *timeout,
            HookCommand::Prompt { timeout, .. } => *timeout,
            HookCommand::Http { timeout, .. } => *timeout,
            HookCommand::Agent { timeout, .. } => *timeout,
        }
    }

    pub fn get_status_message(&self) -> Option<&str> {
        match self {
            HookCommand::BashCommand { status_message, .. } => status_message.as_deref(),
            HookCommand::Prompt { status_message, .. } => status_message.as_deref(),
            HookCommand::Http { status_message, .. } => status_message.as_deref(),
            HookCommand::Agent { status_message, .. } => status_message.as_deref(),
        }
    }

    pub fn is_once(&self) -> bool {
        match self {
            HookCommand::BashCommand { once, .. } => once.unwrap_or(false),
            HookCommand::Prompt { once, .. } => once.unwrap_or(false),
            HookCommand::Http { once, .. } => once.unwrap_or(false),
            HookCommand::Agent { once, .. } => once.unwrap_or(false),
        }
    }

    pub fn hook_type_name(&self) -> &'static str {
        match self {
            HookCommand::BashCommand { .. } => "command",
            HookCommand::Prompt { .. } => "prompt",
            HookCommand::Http { .. } => "http",
            HookCommand::Agent { .. } => "agent",
        }
    }
}

// ============================================================================
// Hook Matcher
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookMatcher {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matcher: Option<String>,
    pub hooks: Vec<HookCommand>,
}

/// Hooks configuration: event -> array of matchers
pub type HooksSettings = HashMap<String, Vec<HookMatcher>>;

/// Validate a hooks settings configuration.
pub fn validate_hooks_settings(settings: &HooksSettings) -> Vec<String> {
    let mut errors = Vec::new();
    for (event_name, matchers) in settings {
        if HookEvent::from_str(event_name).is_none() {
            errors.push(format!("Unknown hook event: {}", event_name));
        }
        for matcher in matchers {
            if matcher.hooks.is_empty() {
                errors.push(format!(
                    "Hook matcher for event '{}' has empty hooks array",
                    event_name
                ));
            }
            for hook in &matcher.hooks {
                match hook {
                    HookCommand::BashCommand { command, .. } => {
                        if command.is_empty() {
                            errors.push(format!(
                                "BashCommand hook in '{}' has empty command",
                                event_name
                            ));
                        }
                    }
                    HookCommand::Prompt { prompt, .. } => {
                        if prompt.is_empty() {
                            errors
                                .push(format!("Prompt hook in '{}' has empty prompt", event_name));
                        }
                    }
                    HookCommand::Http { url, .. } => {
                        if url.is_empty() {
                            errors.push(format!("Http hook in '{}' has empty url", event_name));
                        }
                    }
                    HookCommand::Agent { prompt, .. } => {
                        if prompt.is_empty() {
                            errors.push(format!("Agent hook in '{}' has empty prompt", event_name));
                        }
                    }
                }
            }
        }
    }
    errors
}
