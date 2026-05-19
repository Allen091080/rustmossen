//! API-based microcompact using native context management strategies.

use std::env;

use serde::{Deserialize, Serialize};

/// Default values for context management strategies.
const DEFAULT_MAX_INPUT_TOKENS: usize = 180_000;
const DEFAULT_TARGET_INPUT_TOKENS: usize = 40_000;

/// Tool names whose results can be cleared.
const TOOLS_CLEARABLE_RESULTS: &[&str] = &[
    "Bash", "Execute", "Glob", "Grep", "Read", "WebFetch", "WebSearch",
];

/// Tool names whose uses can be cleared.
const TOOLS_CLEARABLE_USES: &[&str] = &["Edit", "Write", "NotebookEdit"];

/// Context edit strategy types matching API documentation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContextEditStrategy {
    #[serde(rename = "clear_tool_uses_20250919")]
    ClearToolUses {
        trigger: Option<TokenTrigger>,
        keep: Option<ToolUsesKeep>,
        clear_tool_inputs: Option<ClearToolInputs>,
        exclude_tools: Option<Vec<String>>,
        clear_at_least: Option<TokenTrigger>,
    },
    #[serde(rename = "clear_thinking_20251015")]
    ClearThinking {
        keep: ThinkingKeep,
    },
}

/// Token-based trigger configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenTrigger {
    #[serde(rename = "type")]
    pub trigger_type: String,
    pub value: usize,
}

/// Keep configuration for tool uses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolUsesKeep {
    #[serde(rename = "type")]
    pub keep_type: String,
    pub value: usize,
}

/// Clear tool inputs configuration — either bool or list of tool names.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ClearToolInputs {
    All(bool),
    Specific(Vec<String>),
}

/// Keep configuration for thinking blocks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ThinkingKeep {
    All(String), // "all"
    Turns { #[serde(rename = "type")] keep_type: String, value: usize },
}

/// Context management configuration wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextManagementConfig {
    pub edits: Vec<ContextEditStrategy>,
}

/// Options for API context management generation.
#[derive(Debug, Clone, Default)]
pub struct ApiContextManagementOptions {
    pub has_thinking: bool,
    pub is_redact_thinking_active: bool,
    pub clear_all_thinking: bool,
}

/// Get API-based context management configuration.
pub fn get_api_context_management(
    options: Option<ApiContextManagementOptions>,
) -> Option<ContextManagementConfig> {
    let opts = options.unwrap_or_default();
    let mut strategies: Vec<ContextEditStrategy> = Vec::new();

    // Preserve thinking blocks in previous assistant turns
    if opts.has_thinking && !opts.is_redact_thinking_active {
        let keep = if opts.clear_all_thinking {
            ThinkingKeep::Turns {
                keep_type: "thinking_turns".to_string(),
                value: 1,
            }
        } else {
            ThinkingKeep::All("all".to_string())
        };
        strategies.push(ContextEditStrategy::ClearThinking { keep });
    }

    // Tool clearing strategies are internal-only
    let user_type = env::var("USER_TYPE").unwrap_or_default();
    if user_type != "ant" {
        return if strategies.is_empty() {
            None
        } else {
            Some(ContextManagementConfig { edits: strategies })
        };
    }

    let use_clear_tool_results = is_env_truthy("USE_API_CLEAR_TOOL_RESULTS");
    let use_clear_tool_uses = is_env_truthy("USE_API_CLEAR_TOOL_USES");

    if !use_clear_tool_results && !use_clear_tool_uses {
        return if strategies.is_empty() {
            None
        } else {
            Some(ContextManagementConfig { edits: strategies })
        };
    }

    if use_clear_tool_results {
        let trigger_threshold = env::var("API_MAX_INPUT_TOKENS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_MAX_INPUT_TOKENS);
        let keep_target = env::var("API_TARGET_INPUT_TOKENS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_TARGET_INPUT_TOKENS);

        strategies.push(ContextEditStrategy::ClearToolUses {
            trigger: Some(TokenTrigger {
                trigger_type: "input_tokens".to_string(),
                value: trigger_threshold,
            }),
            keep: None,
            clear_tool_inputs: Some(ClearToolInputs::Specific(
                TOOLS_CLEARABLE_RESULTS.iter().map(|s| s.to_string()).collect(),
            )),
            exclude_tools: None,
            clear_at_least: Some(TokenTrigger {
                trigger_type: "input_tokens".to_string(),
                value: trigger_threshold.saturating_sub(keep_target),
            }),
        });
    }

    if use_clear_tool_uses {
        let trigger_threshold = env::var("API_MAX_INPUT_TOKENS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_MAX_INPUT_TOKENS);
        let keep_target = env::var("API_TARGET_INPUT_TOKENS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_TARGET_INPUT_TOKENS);

        strategies.push(ContextEditStrategy::ClearToolUses {
            trigger: Some(TokenTrigger {
                trigger_type: "input_tokens".to_string(),
                value: trigger_threshold,
            }),
            keep: None,
            clear_tool_inputs: None,
            exclude_tools: Some(
                TOOLS_CLEARABLE_USES.iter().map(|s| s.to_string()).collect(),
            ),
            clear_at_least: Some(TokenTrigger {
                trigger_type: "input_tokens".to_string(),
                value: trigger_threshold.saturating_sub(keep_target),
            }),
        });
    }

    if strategies.is_empty() {
        None
    } else {
        Some(ContextManagementConfig { edits: strategies })
    }
}

fn is_env_truthy(key: &str) -> bool {
    env::var(key)
        .ok()
        .map(|v| {
            let v = v.to_lowercase();
            v == "1" || v == "true" || v == "yes"
        })
        .unwrap_or(false)
}
