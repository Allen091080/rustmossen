//! Shared infrastructure for classifier-based permission systems.
//!
//! Translates `utils/permissions/classifierShared.ts`.
//! Provides common types, schemas, and utilities used by both
//! bashClassifier and yoloClassifier.

use serde_json::Value;

/// Represents a tool_use content block.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolUseBlock {
    pub name: String,
    pub input: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

/// Represents a content block from the API.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text { text: String },
    ToolUse { name: String, input: Value, id: Option<String> },
    ToolResult { tool_use_id: String, content: Value },
    Image { source: Value },
}

/// Extract tool use block from message content by tool name.
pub fn extract_tool_use_block<'a>(content: &'a [ContentBlock], tool_name: &str) -> Option<&'a ContentBlock> {
    content.iter().find(|b| matches!(b, ContentBlock::ToolUse { name, .. } if name == tool_name))
}

/// Parse and validate classifier response from tool use block.
/// Returns None if parsing fails.
pub fn parse_classifier_response<T: serde::de::DeserializeOwned>(
    tool_use_block: &ContentBlock,
) -> Option<T> {
    match tool_use_block {
        ContentBlock::ToolUse { input, .. } => serde_json::from_value(input.clone()).ok(),
        _ => None,
    }
}
