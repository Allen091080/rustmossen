use serde::{Deserialize, Serialize};

use crate::string_utils::suffix_chars;

const MAX_CONVERSATION_TEXT: usize = 1000;

/// Message content block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextBlock {
    #[serde(rename = "type")]
    pub block_type: String,
    pub text: String,
}

/// Simplified message for session title generation
#[derive(Debug, Clone)]
pub struct TitleMessage {
    pub msg_type: String, // "user" | "assistant" | other
    pub is_meta: bool,
    pub origin_kind: Option<String>,
    pub content: TitleMessageContent,
}

/// Message content can be a string or blocks
#[derive(Debug, Clone)]
pub enum TitleMessageContent {
    Text(String),
    Blocks(Vec<TextBlock>),
}

/// Flatten a message array into a single text string for Fast title input.
/// Skips meta/non-human messages. Tail-slices to the last 1000 chars.
pub fn extract_conversation_text(messages: &[TitleMessage]) -> String {
    let mut parts: Vec<String> = Vec::new();

    for msg in messages {
        if msg.msg_type != "user" && msg.msg_type != "assistant" {
            continue;
        }
        if msg.is_meta {
            continue;
        }
        if let Some(ref origin) = msg.origin_kind {
            if origin != "human" {
                continue;
            }
        }
        match &msg.content {
            TitleMessageContent::Text(text) => {
                parts.push(text.clone());
            }
            TitleMessageContent::Blocks(blocks) => {
                for block in blocks {
                    if block.block_type == "text" {
                        parts.push(block.text.clone());
                    }
                }
            }
        }
    }

    let text = parts.join("\n");
    if text.chars().count() > MAX_CONVERSATION_TEXT {
        suffix_chars(&text, MAX_CONVERSATION_TEXT)
    } else {
        text
    }
}

/// The session title generation prompt
pub const SESSION_TITLE_PROMPT: &str = r#"Generate a concise, sentence-case title (3-7 words) that captures the main topic or goal of this coding session. The title should be clear enough that the user recognizes the session in a list. Use sentence case: capitalize only the first word and proper nouns.

Return JSON with a single "title" field.

Good examples:
{"title": "Fix login button on mobile"}
{"title": "Add OAuth authentication"}
{"title": "Debug failing CI tests"}
{"title": "Refactor API client error handling"}

Bad (too vague): {"title": "Code changes"}
Bad (too long): {"title": "Investigate and fix the issue where the login button does not respond on mobile devices"}
Bad (wrong case): {"title": "Fix Login Button On Mobile"}"#;

/// Parse a session title from a JSON response string.
/// Returns None if parsing fails or title is empty.
pub fn parse_session_title_response(response_text: &str) -> Option<String> {
    let trimmed = response_text.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Try to parse as JSON
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if let Some(title) = value.get("title").and_then(|t| t.as_str()) {
            let title = title.trim();
            if title.is_empty() {
                return None;
            }
            return Some(title.to_string());
        }
    }

    None
}

/// Generate session title (async, requires a query function to be provided)
pub async fn generate_session_title<F, Fut>(description: &str, query_fn: F) -> Option<String>
where
    F: FnOnce(&str, &str) -> Fut,
    Fut: std::future::Future<Output = Result<String, Box<dyn std::error::Error + Send + Sync>>>,
{
    let trimmed = description.trim();
    if trimmed.is_empty() {
        return None;
    }

    match query_fn(SESSION_TITLE_PROMPT, trimmed).await {
        Ok(response_text) => parse_session_title_response(&response_text),
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{extract_conversation_text, TitleMessage, TitleMessageContent};

    #[test]
    fn extract_conversation_text_tail_truncates_multibyte_without_panic() {
        let text = format!("开头{}", "读代码".repeat(500));
        let messages = vec![TitleMessage {
            msg_type: "user".to_string(),
            is_meta: false,
            origin_kind: Some("human".to_string()),
            content: TitleMessageContent::Text(text),
        }];

        let extracted = extract_conversation_text(&messages);

        assert!(extracted.chars().count() <= 1000);
        assert!(extracted.ends_with("读代码"));
    }
}
