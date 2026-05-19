//! Token usage calculation utilities.
//!
//! Functions for computing token counts from API responses,
//! estimating context window sizes, and tracking usage metrics.

use serde::{Deserialize, Serialize};

/// Usage data from the Mossen API response.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MossenBetaUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_creation_input_tokens: Option<u64>,
    #[serde(default)]
    pub cache_read_input_tokens: Option<u64>,
    /// Server-side iteration usage (not yet in official types).
    #[serde(default)]
    pub iterations: Option<Vec<IterationUsage>>,
}

/// Usage for a single server-side iteration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IterationUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// Synthetic messages and model constants.
pub const SYNTHETIC_MODEL: &str = "__synthetic__";

/// A set of synthetic message texts that should be excluded from usage counting.
static SYNTHETIC_MESSAGES: once_cell::sync::Lazy<std::collections::HashSet<&'static str>> =
    once_cell::sync::Lazy::new(|| {
        let mut set = std::collections::HashSet::new();
        set.insert("[system]");
        set.insert("[context]");
        set
    });

/// Content block types in an assistant message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "thinking")]
    Thinking { thinking: String },
    #[serde(rename = "redacted_thinking")]
    RedactedThinking { data: String },
    #[serde(rename = "tool_use")]
    ToolUse { input: serde_json::Value },
    #[serde(other)]
    Other,
}

/// An assistant message with API response data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantMessageData {
    #[serde(default)]
    pub id: Option<String>,
    pub content: Vec<ContentBlock>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub usage: Option<MossenBetaUsage>,
}

/// A generic message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    #[serde(rename = "type")]
    pub message_type: String,
    #[serde(default)]
    pub message: Option<AssistantMessageData>,
}

/// Options for reading token usage.
#[derive(Debug, Clone, Default)]
pub struct TokenUsageReadOptions {
    pub ignore_empty_usage: bool,
}

/// Get the token usage from an assistant message.
pub fn get_token_usage(message: &Message) -> Option<&MossenBetaUsage> {
    if message.message_type != "assistant" {
        return None;
    }
    let msg_data = message.message.as_ref()?;

    // Skip synthetic messages
    if msg_data.model.as_deref() == Some(SYNTHETIC_MODEL) {
        return None;
    }
    if let Some(ContentBlock::Text { ref text }) = msg_data.content.first() {
        if SYNTHETIC_MESSAGES.contains(text.as_str()) {
            return None;
        }
    }

    msg_data.usage.as_ref()
}

/// Get the API response id for deduplication of split assistant records.
fn get_assistant_message_id(message: &Message) -> Option<&str> {
    if message.message_type != "assistant" {
        return None;
    }
    let msg_data = message.message.as_ref()?;
    if msg_data.model.as_deref() == Some(SYNTHETIC_MODEL) {
        return None;
    }
    msg_data.id.as_deref()
}

/// Calculate total context window tokens from usage data.
pub fn get_token_count_from_usage(usage: &MossenBetaUsage) -> u64 {
    usage.input_tokens
        + usage.cache_creation_input_tokens.unwrap_or(0)
        + usage.cache_read_input_tokens.unwrap_or(0)
        + usage.output_tokens
}

/// Check if usage has iteration data with actual token counts.
fn has_iteration_usage(usage: &MossenBetaUsage) -> bool {
    usage
        .iterations
        .as_ref()
        .map(|iters| {
            iters
                .iter()
                .any(|it| it.input_tokens > 0 || it.output_tokens > 0)
        })
        .unwrap_or(false)
}

/// Get usable token usage (respecting ignore_empty_usage option).
fn get_usable_token_usage<'a>(
    message: &'a Message,
    options: Option<&TokenUsageReadOptions>,
) -> Option<&'a MossenBetaUsage> {
    let usage = get_token_usage(message)?;
    if let Some(opts) = options {
        if opts.ignore_empty_usage
            && get_token_count_from_usage(usage) == 0
            && !has_iteration_usage(usage)
        {
            return None;
        }
    }
    Some(usage)
}

/// Get token count from the last API response in the message list.
pub fn token_count_from_last_api_response(messages: &[Message]) -> u64 {
    for message in messages.iter().rev() {
        if let Some(usage) = get_token_usage(message) {
            return get_token_count_from_usage(usage);
        }
    }
    0
}

/// Final context tokens from the last response's iterations.
pub fn final_context_tokens_from_last_response(messages: &[Message]) -> u64 {
    for message in messages.iter().rev() {
        if let Some(usage) = get_token_usage(message) {
            if let Some(ref iterations) = usage.iterations {
                if !iterations.is_empty() {
                    let last = iterations.last().unwrap();
                    return last.input_tokens + last.output_tokens;
                }
            }
            return usage.input_tokens + usage.output_tokens;
        }
    }
    0
}

/// Get only output_tokens from the last API response.
pub fn message_token_count_from_last_api_response(messages: &[Message]) -> u64 {
    for message in messages.iter().rev() {
        if let Some(usage) = get_token_usage(message) {
            return usage.output_tokens;
        }
    }
    0
}

/// Get the current usage breakdown from the last response.
pub fn get_current_usage(
    messages: &[Message],
    options: Option<&TokenUsageReadOptions>,
) -> Option<CurrentUsage> {
    for message in messages.iter().rev() {
        if let Some(usage) = get_usable_token_usage(message, options) {
            return Some(CurrentUsage {
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
                cache_creation_input_tokens: usage.cache_creation_input_tokens.unwrap_or(0),
                cache_read_input_tokens: usage.cache_read_input_tokens.unwrap_or(0),
            });
        }
    }
    None
}

/// Current usage breakdown.
#[derive(Debug, Clone)]
pub struct CurrentUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub cache_read_input_tokens: u64,
}

/// Check if the most recent assistant message exceeds 200k tokens.
pub fn does_most_recent_assistant_message_exceed_200k(messages: &[Message]) -> bool {
    const THRESHOLD: u64 = 200_000;
    let last_asst = messages.iter().rev().find(|m| m.message_type == "assistant");
    match last_asst {
        Some(msg) => match get_token_usage(msg) {
            Some(usage) => get_token_count_from_usage(usage) > THRESHOLD,
            None => false,
        },
        None => false,
    }
}

/// Calculate content length of an assistant message.
/// Used for spinner token estimation (characters / 4 ≈ tokens).
pub fn get_assistant_message_content_length(message: &AssistantMessageData) -> usize {
    let mut content_length = 0;
    for block in &message.content {
        match block {
            ContentBlock::Text { text } => content_length += text.len(),
            ContentBlock::Thinking { thinking } => content_length += thinking.len(),
            ContentBlock::RedactedThinking { data } => content_length += data.len(),
            ContentBlock::ToolUse { input } => {
                content_length += serde_json::to_string(input).unwrap_or_default().len();
            }
            ContentBlock::Other => {}
        }
    }
    content_length
}

/// Rough token count estimation from message content.
/// Approximation: 1 token ≈ 4 characters.
pub fn rough_token_count_estimation_for_messages(messages: &[Message]) -> u64 {
    let mut total_chars: usize = 0;
    for msg in messages {
        if let Some(ref data) = msg.message {
            total_chars += get_assistant_message_content_length(data);
        }
    }
    (total_chars / 4) as u64
}

/// Get the current context window size in tokens.
///
/// Uses the last API response's token count plus estimates for messages added since.
/// Handles parallel tool call deduplication by walking back to the first sibling
/// with the same message.id.
pub fn token_count_with_estimation(
    messages: &[Message],
    options: Option<&TokenUsageReadOptions>,
) -> u64 {
    let mut i = messages.len();
    while i > 0 {
        i -= 1;
        let message = &messages[i];
        if let Some(usage) = get_usable_token_usage(message, options) {
            // Walk back past earlier sibling records with the same API response ID
            let response_id = get_assistant_message_id(message).map(|s| s.to_string());
            if let Some(ref rid) = response_id {
                let mut j = i;
                while j > 0 {
                    j -= 1;
                    let prior = &messages[j];
                    let prior_id = get_assistant_message_id(prior);
                    if prior_id == Some(rid.as_str()) {
                        i = j;
                    } else if prior_id.is_some() {
                        break;
                    }
                    // prior_id is None: a user/tool_result message, keep walking
                }
            }
            return get_token_count_from_usage(usage)
                + rough_token_count_estimation_for_messages(&messages[i + 1..]);
        }
    }
    rough_token_count_estimation_for_messages(messages)
}
