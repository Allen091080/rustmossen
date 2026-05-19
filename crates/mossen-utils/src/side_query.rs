//! Side query — lightweight API wrapper for queries outside the main conversation loop.
//!
//! Handles fingerprint computation, attribution headers, CLI system prompt prefix,
//! model betas, API metadata, and model string normalization.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Options for a side query API call.
#[derive(Debug, Clone)]
pub struct SideQueryOptions {
    /// Model to use for the query.
    pub model: String,
    /// System prompt — string or array of text blocks.
    pub system: Option<SystemPrompt>,
    /// Messages to send.
    pub messages: Vec<MessageParam>,
    /// Optional tools.
    pub tools: Option<Vec<serde_json::Value>>,
    /// Optional tool choice.
    pub tool_choice: Option<serde_json::Value>,
    /// Optional JSON output format for structured responses.
    pub output_format: Option<serde_json::Value>,
    /// Max tokens (default: 1024).
    pub max_tokens: Option<u32>,
    /// Max retries (default: 2).
    pub max_retries: Option<u32>,
    /// Skip CLI system prompt prefix.
    pub skip_system_prompt_prefix: bool,
    /// Temperature override.
    pub temperature: Option<f64>,
    /// Thinking budget (enables thinking), or None to disable.
    pub thinking: Option<ThinkingConfig>,
    /// Stop sequences.
    pub stop_sequences: Option<Vec<String>>,
    /// Query source identifier for analytics.
    pub query_source: String,
}

/// System prompt type.
#[derive(Debug, Clone)]
pub enum SystemPrompt {
    Text(String),
    Blocks(Vec<TextBlock>),
}

/// Thinking configuration.
#[derive(Debug, Clone)]
pub enum ThinkingConfig {
    Enabled { budget_tokens: u32 },
    Disabled,
}

/// Text block parameter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextBlock {
    #[serde(rename = "type")]
    pub block_type: String,
    pub text: String,
}

/// Message parameter for the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageParam {
    pub role: String,
    pub content: serde_json::Value,
}

/// API response message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaMessage {
    pub id: String,
    pub content: Vec<serde_json::Value>,
    pub model: String,
    pub stop_reason: Option<String>,
    pub usage: MessageUsage,
    #[serde(skip)]
    pub request_id: Option<String>,
}

/// Usage info from API response.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MessageUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_input_tokens: Option<u64>,
    pub cache_creation_input_tokens: Option<u64>,
}

/// Extract text from the first user message for fingerprint computation.
pub fn extract_first_user_message_text(messages: &[MessageParam]) -> String {
    let first_user = messages.iter().find(|m| m.role == "user");
    let Some(msg) = first_user else {
        return String::new();
    };

    match &msg.content {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(arr) => {
            for block in arr {
                if block.get("type").and_then(|v| v.as_str()) == Some("text") {
                    if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                        return text.to_string();
                    }
                }
            }
            String::new()
        }
        _ => String::new(),
    }
}

/// Build the system blocks array for a side query.
///
/// - `attribution_header`: the attribution header string
/// - `cli_sysprompt_prefix`: the CLI system prompt prefix
/// - `skip_system_prompt_prefix`: whether to skip the prefix
/// - `system`: the user-provided system prompt
pub fn build_system_blocks(
    attribution_header: Option<&str>,
    cli_sysprompt_prefix: Option<&str>,
    skip_system_prompt_prefix: bool,
    system: Option<&SystemPrompt>,
) -> Vec<TextBlock> {
    let mut blocks = Vec::new();

    if let Some(header) = attribution_header {
        blocks.push(TextBlock {
            block_type: "text".to_string(),
            text: header.to_string(),
        });
    }

    if !skip_system_prompt_prefix {
        if let Some(prefix) = cli_sysprompt_prefix {
            blocks.push(TextBlock {
                block_type: "text".to_string(),
                text: prefix.to_string(),
            });
        }
    }

    match system {
        Some(SystemPrompt::Text(text)) => {
            blocks.push(TextBlock {
                block_type: "text".to_string(),
                text: text.clone(),
            });
        }
        Some(SystemPrompt::Blocks(user_blocks)) => {
            blocks.extend(user_blocks.iter().cloned());
        }
        None => {}
    }

    blocks
}

/// Build the thinking config parameter for the API request.
pub fn build_thinking_param(
    thinking: Option<&ThinkingConfig>,
    max_tokens: u32,
) -> Option<serde_json::Value> {
    match thinking {
        Some(ThinkingConfig::Disabled) => {
            Some(serde_json::json!({ "type": "disabled" }))
        }
        Some(ThinkingConfig::Enabled { budget_tokens }) => {
            let budget = (*budget_tokens).min(max_tokens.saturating_sub(1));
            Some(serde_json::json!({
                "type": "enabled",
                "budget_tokens": budget,
            }))
        }
        None => None,
    }
}

/// Build the full API request body for a side query.
pub fn build_side_query_request(
    opts: &SideQueryOptions,
    system_blocks: &[TextBlock],
    normalized_model: &str,
    betas: &[String],
    api_metadata: Option<serde_json::Value>,
    thinking_param: Option<serde_json::Value>,
) -> serde_json::Value {
    let max_tokens = opts.max_tokens.unwrap_or(1024);

    let mut request = serde_json::json!({
        "model": normalized_model,
        "max_tokens": max_tokens,
        "system": system_blocks,
        "messages": opts.messages,
    });

    let obj = request.as_object_mut().unwrap();

    if let Some(ref tools) = opts.tools {
        obj.insert("tools".to_string(), serde_json::json!(tools));
    }
    if let Some(ref tool_choice) = opts.tool_choice {
        obj.insert("tool_choice".to_string(), tool_choice.clone());
    }
    if let Some(ref output_format) = opts.output_format {
        obj.insert(
            "output_config".to_string(),
            serde_json::json!({ "format": output_format }),
        );
    }
    if let Some(temp) = opts.temperature {
        obj.insert("temperature".to_string(), serde_json::json!(temp));
    }
    if let Some(ref stop_seqs) = opts.stop_sequences {
        obj.insert("stop_sequences".to_string(), serde_json::json!(stop_seqs));
    }
    if let Some(ref thinking) = thinking_param {
        obj.insert("thinking".to_string(), thinking.clone());
    }
    if !betas.is_empty() {
        obj.insert("betas".to_string(), serde_json::json!(betas));
    }
    if let Some(metadata) = api_metadata {
        obj.insert("metadata".to_string(), metadata);
    }

    request
}

/// 对应 TS `sideQuery`：调用方组装好参数后该函数构建并执行请求。
///
/// 由于 Rust 端尚未引入完整的 `MossenClient`，这里仅完成请求体构造并返回
/// JSON 描述供调用方实际发送（保持 API parity）。`attribution_header` 与
/// `cli_sysprompt_prefix` 由调用方提前算好后传入，避免把 OAuth/指纹依赖耦合
/// 到本模块。
pub fn side_query(
    opts: &SideQueryOptions,
    attribution_header: Option<&str>,
    cli_sysprompt_prefix: Option<&str>,
    normalized_model: &str,
    betas: &[String],
    api_metadata: Option<serde_json::Value>,
) -> serde_json::Value {
    let system_blocks = build_system_blocks(
        attribution_header,
        cli_sysprompt_prefix,
        opts.skip_system_prompt_prefix,
        opts.system.as_ref(),
    );
    let thinking_param = build_thinking_param(opts.thinking.as_ref(), opts.max_tokens.unwrap_or(1024));
    build_side_query_request(
        opts,
        &system_blocks,
        normalized_model,
        betas,
        api_metadata,
        thinking_param,
    )
}

/// Analytics event data for a successful side query.
#[derive(Debug, Clone)]
pub struct SideQueryAnalytics {
    pub request_id: Option<String>,
    pub query_source: String,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
    pub uncached_input_tokens: u64,
    pub duration_ms: u64,
    pub time_since_last_api_call_ms: Option<u64>,
}
