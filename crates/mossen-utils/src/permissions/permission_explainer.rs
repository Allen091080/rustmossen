//! Permission explainer.
//!
//! Generates explanations of tool actions via a side-query to an LLM.
//! Used to provide context about why a command is being run and its risk level.

use std::collections::HashMap;

/// Risk level for permission explanations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RiskLevel {
    LOW,
    MEDIUM,
    HIGH,
}

impl RiskLevel {
    /// Map risk levels to numeric values for analytics.
    pub fn numeric_value(self) -> u8 {
        match self {
            RiskLevel::LOW => 1,
            RiskLevel::MEDIUM => 2,
            RiskLevel::HIGH => 3,
        }
    }
}

/// A permission explanation with risk assessment.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PermissionExplanation {
    pub risk_level: RiskLevel,
    pub explanation: String,
    pub reasoning: String,
    pub risk: String,
}

/// Parameters for generating a permission explanation.
pub struct GenerateExplanationParams {
    pub tool_name: String,
    pub tool_input: serde_json::Value,
    pub tool_description: Option<String>,
    pub messages: Option<Vec<serde_json::Value>>,
}

/// Error type codes for analytics.
pub const ERROR_TYPE_PARSE: u8 = 1;
pub const ERROR_TYPE_NETWORK: u8 = 2;
pub const ERROR_TYPE_UNKNOWN: u8 = 3;

const SYSTEM_PROMPT: &str =
    "Analyze shell commands and explain what they do, why you're running them, and potential risks.";

/// Tool definition for forced structured output.
pub fn explain_command_tool_schema() -> serde_json::Value {
    serde_json::json!({
        "name": "explain_command",
        "description": "Provide an explanation of a shell command",
        "input_schema": {
            "type": "object",
            "properties": {
                "explanation": {
                    "type": "string",
                    "description": "What this command does (1-2 sentences)"
                },
                "reasoning": {
                    "type": "string",
                    "description": "Why YOU are running this command. Start with \"I\" - e.g. \"I need to check the file contents\""
                },
                "risk": {
                    "type": "string",
                    "description": "What could go wrong, under 15 words"
                },
                "riskLevel": {
                    "type": "string",
                    "enum": ["LOW", "MEDIUM", "HIGH"],
                    "description": "LOW (safe dev workflows), MEDIUM (recoverable changes), HIGH (dangerous/irreversible)"
                }
            },
            "required": ["explanation", "reasoning", "risk", "riskLevel"]
        }
    })
}

fn format_tool_input(input: &serde_json::Value) -> String {
    match input {
        serde_json::Value::String(s) => s.clone(),
        _ => serde_json::to_string_pretty(input).unwrap_or_else(|_| input.to_string()),
    }
}

/// Extract recent conversation context from messages for the explainer.
/// Returns a summary of recent assistant messages to provide context.
fn extract_conversation_context(
    messages: &[serde_json::Value],
    max_chars: usize,
) -> String {
    let assistant_messages: Vec<&serde_json::Value> = messages
        .iter()
        .filter(|m| m.get("type").and_then(|t| t.as_str()) == Some("assistant"))
        .rev()
        .take(3)
        .collect();

    let mut context_parts: Vec<String> = Vec::new();
    let mut total_chars = 0;

    for msg in assistant_messages {
        let content = msg
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_array());

        if let Some(blocks) = content {
            let text_blocks: String = blocks
                .iter()
                .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("text"))
                .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join(" ");

            if !text_blocks.is_empty() && total_chars < max_chars {
                let remaining = max_chars - total_chars;
                let truncated = if text_blocks.len() > remaining {
                    format!("{}...", &text_blocks[..remaining])
                } else {
                    text_blocks
                };
                total_chars += truncated.len();
                context_parts.push(truncated);
            }
        }
    }

    context_parts.reverse();
    context_parts.join("\n\n")
}

/// Check if the permission explainer feature is enabled.
/// Enabled by default; users can opt out via config.
pub fn is_permission_explainer_enabled(config: &HashMap<String, serde_json::Value>) -> bool {
    config
        .get("permissionExplainerEnabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(true)
}

/// Build the user prompt for the explainer LLM call.
pub fn build_explainer_prompt(params: &GenerateExplanationParams) -> String {
    let formatted_input = format_tool_input(&params.tool_input);
    let conversation_context = params
        .messages
        .as_ref()
        .filter(|m| !m.is_empty())
        .map(|m| extract_conversation_context(m, 1000))
        .unwrap_or_default();

    let mut prompt = format!("Tool: {}\n", params.tool_name);
    if let Some(ref desc) = params.tool_description {
        prompt.push_str(&format!("Description: {}\n", desc));
    }
    prompt.push_str(&format!("\nInput:\n{}", formatted_input));
    if !conversation_context.is_empty() {
        prompt.push_str(&format!(
            "\n\nRecent conversation context:\n{}",
            conversation_context
        ));
    }
    prompt.push_str("\n\nExplain this command in context.");
    prompt
}

/// Get the system prompt for the explainer.
pub fn get_system_prompt() -> &'static str {
    SYSTEM_PROMPT
}

/// 对应 TS `generatePermissionExplanation`：通过侧 LLM 调用生成命令解释。
///
/// `request_fn` 由调用方提供，负责发起实际的 LLM 请求并返回结构化输出。
/// 这里聚合 prompt 构建与解析逻辑，便于测试时注入纯函数。
pub async fn generate_permission_explanation<F, Fut>(
    params: &GenerateExplanationParams,
    config: &HashMap<String, serde_json::Value>,
    request_fn: F,
) -> Option<PermissionExplanation>
where
    F: FnOnce(String, &'static str, serde_json::Value) -> Fut,
    Fut: std::future::Future<Output = Option<serde_json::Value>>,
{
    if !is_permission_explainer_enabled(config) {
        return None;
    }
    let prompt = build_explainer_prompt(params);
    let schema = explain_command_tool_schema();
    let raw = request_fn(prompt, SYSTEM_PROMPT, schema).await?;
    parse_risk_assessment(&raw)
}

/// Parse a risk assessment from a tool use block input.
pub fn parse_risk_assessment(input: &serde_json::Value) -> Option<PermissionExplanation> {
    let risk_level_str = input.get("riskLevel")?.as_str()?;
    let risk_level = match risk_level_str {
        "LOW" => RiskLevel::LOW,
        "MEDIUM" => RiskLevel::MEDIUM,
        "HIGH" => RiskLevel::HIGH,
        _ => return None,
    };

    let explanation = input.get("explanation")?.as_str()?.to_string();
    let reasoning = input.get("reasoning")?.as_str()?.to_string();
    let risk = input.get("risk")?.as_str()?.to_string();

    Some(PermissionExplanation {
        risk_level,
        explanation,
        reasoning,
        risk,
    })
}
