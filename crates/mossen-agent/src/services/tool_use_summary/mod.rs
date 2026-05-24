//! Tool use summary generator — generates brief summaries of completed tool batches.

use serde::Serialize;
use serde_json::Value;

use mossen_utils::string_utils::truncate_chars_with_suffix;

/// Information about a tool execution.
#[derive(Debug, Clone)]
pub struct ToolInfo {
    pub name: String,
    pub input: Value,
    pub output: Value,
}

/// Parameters for generating a tool use summary.
pub struct GenerateToolUseSummaryParams {
    pub tools: Vec<ToolInfo>,
    pub is_non_interactive_session: bool,
    pub last_assistant_text: Option<String>,
}

const TOOL_USE_SUMMARY_SYSTEM_PROMPT: &str = r#"Write a short summary label describing what these tool calls accomplished. It appears as a single-line row in a mobile app and truncates around 30 characters, so think git-commit-subject, not sentence.

Keep the verb in past tense and the most distinctive noun. Drop articles, connectors, and long location context first.

Examples:
- Searched in auth/
- Fixed NPE in UserService
- Created signup endpoint
- Read config.json
- Ran failing tests"#;

/// Truncate a JSON value to a maximum string length.
fn truncate_json(value: &Value, max_length: usize) -> String {
    let s = serde_json::to_string(value).unwrap_or_else(|_| "[unable to serialize]".to_string());
    if max_length > 3 {
        truncate_chars_with_suffix(&s, max_length - 3, "...")
    } else if s.chars().count() <= max_length {
        s
    } else {
        s.chars().take(max_length).collect()
    }
}

/// Result of summary generation.
#[derive(Debug, Clone, Serialize)]
pub struct ToolUseSummaryResult {
    pub summary: Option<String>,
}

/// Generate a human-readable summary of completed tools.
///
/// In the full implementation, this calls the fast-tier model (queryFast).
/// Here we build the prompt and simulate the API call structure.
pub async fn generate_tool_use_summary(
    params: GenerateToolUseSummaryParams,
    api_caller: impl AsyncToolSummaryCaller,
) -> Option<String> {
    if params.tools.is_empty() {
        return None;
    }

    let tool_summaries: Vec<String> = params
        .tools
        .iter()
        .map(|tool| {
            let input_str = truncate_json(&tool.input, 300);
            let output_str = truncate_json(&tool.output, 300);
            format!(
                "Tool: {}\nInput: {}\nOutput: {}",
                tool.name, input_str, output_str
            )
        })
        .collect();

    let context_prefix = match &params.last_assistant_text {
        Some(text) => {
            let truncated = truncate_chars_with_suffix(text, 200, "...");
            format!(
                "User's intent (from assistant's last message): {}\n\n",
                truncated
            )
        }
        None => String::new(),
    };

    let user_prompt = format!(
        "{}Tools completed:\n\n{}\n\nLabel:",
        context_prefix,
        tool_summaries.join("\n\n")
    );

    match api_caller
        .call_fast(TOOL_USE_SUMMARY_SYSTEM_PROMPT, &user_prompt)
        .await
    {
        Ok(summary) => {
            let trimmed = summary.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        }
        Err(e) => {
            tracing::error!("Tool use summary generation failed: {}", e);
            None
        }
    }
}

/// Trait for calling the fast model (allows mocking in tests).
#[async_trait::async_trait]
pub trait AsyncToolSummaryCaller: Send + Sync {
    async fn call_fast(
        &self,
        system_prompt: &str,
        user_prompt: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>>;
}
