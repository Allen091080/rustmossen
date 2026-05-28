//! Provider policy — determines semantic capabilities based on backend protocol.

use serde::{Deserialize, Serialize};

use super::canonical::{
    OfficialSemanticCapabilities, ThinkingParityStrategy, ToolCallArgsEncoding, ToolResultRoleStyle,
};

/// Provider model policy with capabilities and thinking strategy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderModelPolicy {
    pub capabilities: OfficialSemanticCapabilities,
    pub synthetic_tags: SyntheticTags,
    pub thinking_strategy: ThinkingParityStrategy,
}

/// Synthetic XML tags used for non-native thinking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyntheticTags {
    pub response_close: String,
    pub response_open: String,
    pub thinking_close: String,
    pub thinking_open: String,
}

impl Default for SyntheticTags {
    fn default() -> Self {
        Self {
            thinking_open: "<assistant_thinking>".to_string(),
            thinking_close: "</assistant_thinking>".to_string(),
            response_open: "<assistant_response>".to_string(),
            response_close: "</assistant_response>".to_string(),
        }
    }
}

/// Mossen-native (Mossen API) capabilities.
fn mossen_compatible_capabilities() -> OfficialSemanticCapabilities {
    OfficialSemanticCapabilities {
        mixed_content_tool_use: true,
        native_thinking_blocks: true,
        reasoning_budget: true,
        streaming_tool_arg_deltas: true,
        structured_stop_reasons: true,
        supports_assistant_prelude_before_tool_use: true,
        tool_call_args_encoding: ToolCallArgsEncoding::Object,
        tool_result_role_style: ToolResultRoleStyle::MossenUserToolResult,
    }
}

/// OpenAI-compatible capabilities.
fn openai_compatible_capabilities() -> OfficialSemanticCapabilities {
    OfficialSemanticCapabilities {
        mixed_content_tool_use: false,
        native_thinking_blocks: false,
        reasoning_budget: false,
        streaming_tool_arg_deltas: true,
        structured_stop_reasons: false,
        supports_assistant_prelude_before_tool_use: true,
        tool_call_args_encoding: ToolCallArgsEncoding::JsonString,
        tool_result_role_style: ToolResultRoleStyle::OpenaiToolRole,
    }
}

/// Resolve the thinking parity strategy given the protocol and request.
fn resolve_thinking_strategy(
    requested_thinking_disabled: bool,
    protocol: &str,
) -> ThinkingParityStrategy {
    if protocol == "mossen-compatible" {
        return ThinkingParityStrategy::Native;
    }

    if requested_thinking_disabled {
        return ThinkingParityStrategy::None;
    }

    if let Ok(env_strategy) = std::env::var("MOSSEN_CODE_CUSTOM_THINKING_PARITY_STRATEGY") {
        let trimmed = env_strategy.trim();
        if trimmed == "synthetic_two_pass" {
            return ThinkingParityStrategy::SyntheticTwoPass;
        }
        if trimmed == "none" {
            return ThinkingParityStrategy::None;
        }
    }

    ThinkingParityStrategy::SyntheticSinglePass
}

/// Get the official semantic capabilities for the current backend protocol.
pub fn get_official_semantic_capabilities(protocol: &str) -> OfficialSemanticCapabilities {
    if protocol == "openai-compatible" {
        openai_compatible_capabilities()
    } else {
        mossen_compatible_capabilities()
    }
}

/// Resolve the provider model policy for a given request.
pub fn resolve_provider_model_policy(
    protocol: &str,
    requested_thinking_disabled: bool,
) -> ProviderModelPolicy {
    ProviderModelPolicy {
        capabilities: if protocol == "openai-compatible" {
            openai_compatible_capabilities()
        } else {
            mossen_compatible_capabilities()
        },
        synthetic_tags: SyntheticTags::default(),
        thinking_strategy: resolve_thinking_strategy(requested_thinking_disabled, protocol),
    }
}
