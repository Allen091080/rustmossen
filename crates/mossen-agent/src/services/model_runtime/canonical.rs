//! Canonical types — provider-agnostic representation of model interactions.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// How tool results are represented in the conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolResultRoleStyle {
    #[serde(rename = "mossen_user_tool_result")]
    MossenUserToolResult,
    #[serde(rename = "openai_tool_role")]
    OpenaiToolRole,
}

/// How tool call arguments are encoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolCallArgsEncoding {
    #[serde(rename = "json_string")]
    JsonString,
    #[serde(rename = "object")]
    Object,
}

/// Strategy for thinking/reasoning parity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThinkingParityStrategy {
    #[serde(rename = "native")]
    Native,
    #[serde(rename = "none")]
    None,
    #[serde(rename = "synthetic_single_pass")]
    SyntheticSinglePass,
    #[serde(rename = "synthetic_two_pass")]
    SyntheticTwoPass,
}

/// Semantic capabilities of the official model API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfficialSemanticCapabilities {
    pub mixed_content_tool_use: bool,
    pub native_thinking_blocks: bool,
    pub reasoning_budget: bool,
    pub streaming_tool_arg_deltas: bool,
    pub structured_stop_reasons: bool,
    pub supports_assistant_prelude_before_tool_use: bool,
    pub tool_call_args_encoding: ToolCallArgsEncoding,
    pub tool_result_role_style: ToolResultRoleStyle,
}

/// Token usage from a model response.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CanonicalUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// A tool call request from the assistant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantToolRequest {
    pub arguments_object: HashMap<String, serde_json::Value>,
    pub id: String,
    pub name: String,
}

/// Assistant prelude text before tool use.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantPrelude {
    pub text: String,
}

/// Result of a tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecutionResult {
    pub content: String,
    pub is_error: bool,
    pub tool_use_id: String,
}

/// A round of conversation (prelude + tool requests + results).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanonicalConversationRound {
    pub prelude: Option<AssistantPrelude>,
    pub tool_requests: Vec<AssistantToolRequest>,
    pub tool_results: Vec<ToolExecutionResult>,
}

/// Role of a message in conversation history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

/// A message in conversation history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanonicalHistoryMessage {
    pub content: String,
    pub role: MessageRole,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<AssistantToolRequest>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

/// Request for a model turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanonicalTurnRequest {
    pub max_tokens: u64,
    pub messages: Vec<CanonicalHistoryMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<ReasoningConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
    pub system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<serde_json::Value>>,
}

/// Reasoning/thinking configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningConfig {
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
}

/// Canonical stop reason for a model response.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CanonicalStopReason {
    Compaction,
    EndTurn,
    MaxTokens,
    PauseTurn,
    Refusal,
    StopSequence,
    ToolUse,
}

/// Streaming event from the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CanonicalStreamEvent {
    MessageStart {
        message_id: String,
        model: String,
    },
    ThinkingStart,
    ThinkingDelta {
        text: String,
    },
    ThinkingEnd,
    TextStart,
    TextDelta {
        text: String,
    },
    TextEnd,
    ToolUseStart {
        id: String,
        name: String,
    },
    ToolUseArgsDelta {
        id: String,
        partial_json: String,
    },
    ToolUseEnd {
        id: String,
    },
    MessageStop {
        stop_reason: CanonicalStopReason,
        usage: CanonicalUsage,
    },
    ProviderError {
        error: String,
    },
}

/// Result of a complete model turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanonicalTurnResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_diagnostics: Option<HashMap<String, serde_json::Value>>,
    pub stop_reason: CanonicalStopReason,
    pub thinking_text: String,
    pub tool_requests: Vec<AssistantToolRequest>,
    pub usage: CanonicalUsage,
    pub visible_text: String,
}

/// Side effects classified from a stop reason.
#[derive(Debug, Clone, Default)]
pub struct MossenStopSideEffects {
    pub is_context_window_exceeded: bool,
    pub is_max_tokens: bool,
    pub is_refusal: bool,
}

/// Observed stop state from a Mossen stream.
#[derive(Debug, Clone)]
pub struct ObservedMossenStopState {
    pub canonical_stop_reason: Option<CanonicalStopReason>,
    pub stop_reason: Option<String>,
}

/// Check if a stop reason indicates context window exceeded.
pub fn is_mossen_context_window_exceeded_stop_reason(stop_reason: &str) -> bool {
    stop_reason == "model_context_window_exceeded"
}

/// Check if a stop reason indicates refusal.
pub fn is_mossen_refusal_stop_reason(stop_reason: &str) -> bool {
    stop_reason == "refusal"
}

/// Check if canonical stop reason is max_tokens.
pub fn is_canonical_max_tokens_stop_reason(stop_reason: Option<CanonicalStopReason>) -> bool {
    stop_reason == Some(CanonicalStopReason::MaxTokens)
}

/// Check if stream terminated without a canonical stop reason.
pub fn did_mossen_stream_terminate_without_canonical_stop_reason(
    has_partial_message: bool,
    yielded_assistant_message_count: u32,
    canonical_stop_reason: Option<CanonicalStopReason>,
) -> bool {
    !has_partial_message
        || (yielded_assistant_message_count == 0 && canonical_stop_reason.is_none())
}

/// Convert a Mossen stop reason string to canonical.
pub fn canonical_stop_reason_from_mossen(stop_reason: &str) -> CanonicalStopReason {
    match stop_reason {
        "tool_use" => CanonicalStopReason::ToolUse,
        "refusal" => CanonicalStopReason::Refusal,
        "stop_sequence" => CanonicalStopReason::StopSequence,
        "pause_turn" => CanonicalStopReason::PauseTurn,
        "compaction" => CanonicalStopReason::Compaction,
        "max_tokens" | "model_context_window_exceeded" => CanonicalStopReason::MaxTokens,
        _ => CanonicalStopReason::EndTurn,
    }
}

/// Classify side effects from a stop reason.
pub fn classify_mossen_stop_side_effects(
    stop_reason: Option<&str>,
    canonical_stop_reason: Option<CanonicalStopReason>,
) -> MossenStopSideEffects {
    MossenStopSideEffects {
        is_context_window_exceeded: stop_reason
            .map(|r| is_mossen_context_window_exceeded_stop_reason(r))
            .unwrap_or(false),
        is_max_tokens: is_canonical_max_tokens_stop_reason(canonical_stop_reason),
        is_refusal: stop_reason
            .map(|r| is_mossen_refusal_stop_reason(r))
            .unwrap_or(false),
    }
}

/// Observe stop state from a Mossen stream.
pub fn observe_mossen_stop_state(stop_reason: Option<&str>) -> ObservedMossenStopState {
    ObservedMossenStopState {
        canonical_stop_reason: stop_reason.map(|r| canonical_stop_reason_from_mossen(r)),
        stop_reason: stop_reason.map(|s| s.to_string()),
    }
}

/// Classify observed stop state into side effects.
pub fn classify_observed_mossen_stop_state(
    observed: Option<&ObservedMossenStopState>,
) -> MossenStopSideEffects {
    classify_mossen_stop_side_effects(
        observed.and_then(|o| o.stop_reason.as_deref()),
        observed.and_then(|o| o.canonical_stop_reason),
    )
}

/// TS `didMossenStreamTerminateWithoutObservedStopState` — returns `true` when
/// the most-recent stream observation lacks both a structured stop reason and
/// a final `message_stop` event (i.e. the connection dropped mid-turn).
pub fn did_mossen_stream_terminate_without_observed_stop_state(
    last_stop_reason: Option<&CanonicalStopReason>,
    saw_message_stop: bool,
) -> bool {
    last_stop_reason.is_none() && !saw_message_stop
}
