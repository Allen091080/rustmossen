// Agentic session search: LLM-driven semantic session search.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

// Limits for transcript extraction
const MAX_TRANSCRIPT_CHARS: usize = 2000;
const MAX_MESSAGES_TO_SCAN: usize = 100;
const MAX_SESSIONS_TO_SEARCH: usize = 100;

const SESSION_SEARCH_SYSTEM_PROMPT: &str = r#"Your goal is to find relevant sessions based on a user's search query.

You will be given a list of sessions with their metadata and a search query. Identify which sessions are most relevant to the query.

Each session may include:
- Title (display name or custom title)
- Tag (user-assigned category, shown as [tag: name] - users tag sessions with /tag command to categorize them)
- Branch (git branch name, shown as [branch: name])
- Summary (AI-generated summary)
- First message (beginning of the conversation)
- Transcript (excerpt of conversation content)

IMPORTANT: Tags are user-assigned labels that indicate the session's topic or category. If the query matches a tag exactly or partially, those sessions should be highly prioritized.

For each session, consider (in order of priority):
1. Exact tag matches (highest priority - user explicitly categorized this session)
2. Partial tag matches or tag-related terms
3. Title matches (custom titles or first message content)
4. Branch name matches
5. Summary and transcript content matches
6. Semantic similarity and related concepts

CRITICAL: Be VERY inclusive in your matching. Include sessions that:
- Contain the query term anywhere in any field
- Are semantically related to the query
- Discuss topics that could be related to the query
- Have transcripts that mention the concept even in passing

When in doubt, INCLUDE the session. It's better to return too many results than too few.

Respond with ONLY the JSON object, no markdown formatting:
{"relevant_indices": [2, 5, 0]}"#;

#[derive(Debug, Clone)]
pub struct SerializedMessage {
    pub message_type: String,
    pub content: Option<MessageContent>,
}

#[derive(Debug, Clone)]
pub enum MessageContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

#[derive(Debug, Clone)]
pub struct ContentBlock {
    pub block_type: String,
    pub text: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LogOption {
    pub session_id: String,
    pub summary: Option<String>,
    pub custom_title: Option<String>,
    pub tag: Option<String>,
    pub git_branch: Option<String>,
    pub first_prompt: Option<String>,
    pub messages: Vec<SerializedMessage>,
    pub is_sidechain: bool,
}

#[derive(Debug, Deserialize)]
struct AgenticSearchResult {
    relevant_indices: Vec<usize>,
}

/// Extracts searchable text content from a message.
fn extract_message_text(message: &SerializedMessage) -> String {
    if message.message_type != "user" && message.message_type != "assistant" {
        return String::new();
    }

    match &message.content {
        None => String::new(),
        Some(MessageContent::Text(s)) => s.clone(),
        Some(MessageContent::Blocks(blocks)) => blocks
            .iter()
            .filter_map(|block| block.text.as_deref())
            .collect::<Vec<_>>()
            .join(" "),
    }
}

/// Extracts a truncated transcript from session messages.
fn extract_transcript(messages: &[SerializedMessage]) -> String {
    if messages.is_empty() {
        return String::new();
    }

    let messages_to_scan: Vec<&SerializedMessage> = if messages.len() <= MAX_MESSAGES_TO_SCAN {
        messages.iter().collect()
    } else {
        let half = MAX_MESSAGES_TO_SCAN / 2;
        let mut result: Vec<&SerializedMessage> = messages[..half].iter().collect();
        result.extend(messages[messages.len() - half..].iter());
        result
    };

    let text: String = messages_to_scan
        .iter()
        .map(|m| extract_message_text(m))
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    // Collapse whitespace
    let collapsed: String = text.split_whitespace().collect::<Vec<_>>().join(" ");

    if collapsed.len() > MAX_TRANSCRIPT_CHARS {
        format!("{}…", &collapsed[..MAX_TRANSCRIPT_CHARS])
    } else {
        collapsed
    }
}

/// Gets the display title for a log.
fn get_log_display_title(log: &LogOption) -> String {
    log.custom_title
        .as_deref()
        .or(log.summary.as_deref())
        .or(log.first_prompt.as_deref())
        .unwrap_or("No prompt")
        .to_string()
}

/// Checks if a log contains the query term in any searchable field.
fn log_contains_query(log: &LogOption, query_lower: &str) -> bool {
    let title = get_log_display_title(log).to_lowercase();
    if title.contains(query_lower) {
        return true;
    }

    if let Some(ref ct) = log.custom_title {
        if ct.to_lowercase().contains(query_lower) {
            return true;
        }
    }

    if let Some(ref tag) = log.tag {
        if tag.to_lowercase().contains(query_lower) {
            return true;
        }
    }

    if let Some(ref branch) = log.git_branch {
        if branch.to_lowercase().contains(query_lower) {
            return true;
        }
    }

    if let Some(ref summary) = log.summary {
        if summary.to_lowercase().contains(query_lower) {
            return true;
        }
    }

    if let Some(ref first_prompt) = log.first_prompt {
        if first_prompt.to_lowercase().contains(query_lower) {
            return true;
        }
    }

    if !log.messages.is_empty() {
        let transcript = extract_transcript(&log.messages).to_lowercase();
        if transcript.contains(query_lower) {
            return true;
        }
    }

    false
}

/// Configuration for calling the LLM side query.
pub struct SideQueryConfig {
    pub model: String,
    pub system: String,
    pub user_message: String,
}

/// Trait for performing the LLM side query.
#[async_trait::async_trait]
pub trait SideQueryExecutor: Send + Sync {
    async fn execute(&self, config: SideQueryConfig) -> Result<String, String>;
}

/// Performs an agentic search using an LLM to find relevant sessions.
pub async fn agentic_session_search(
    query: &str,
    logs: &[LogOption],
    executor: &dyn SideQueryExecutor,
) -> Vec<usize> {
    if query.trim().is_empty() || logs.is_empty() {
        return Vec::new();
    }

    let query_lower = query.to_lowercase();

    // Pre-filter: find sessions that contain the query term
    let matching_indices: Vec<usize> = logs
        .iter()
        .enumerate()
        .filter(|(_, log)| log_contains_query(log, &query_lower))
        .map(|(i, _)| i)
        .collect();

    // Take up to MAX_SESSIONS_TO_SEARCH
    let logs_to_search_indices: Vec<usize> = if matching_indices.len() >= MAX_SESSIONS_TO_SEARCH {
        matching_indices[..MAX_SESSIONS_TO_SEARCH].to_vec()
    } else {
        let matching_set: HashSet<usize> = matching_indices.iter().cloned().collect();
        let non_matching: Vec<usize> = (0..logs.len())
            .filter(|i| !matching_set.contains(i))
            .collect();
        let remaining_slots = MAX_SESSIONS_TO_SEARCH.saturating_sub(matching_indices.len());
        let mut result = matching_indices.clone();
        result.extend(non_matching.iter().take(remaining_slots));
        result
    };

    // Build session list for the prompt
    let session_list: String = logs_to_search_indices
        .iter()
        .enumerate()
        .map(|(idx, &log_idx)| {
            let log = &logs[log_idx];
            let mut parts: Vec<String> = vec![format!("{}:", idx)];

            let display_title = get_log_display_title(log);
            parts.push(display_title.clone());

            if let Some(ref ct) = log.custom_title {
                if *ct != display_title {
                    parts.push(format!("[custom title: {}]", ct));
                }
            }

            if let Some(ref tag) = log.tag {
                parts.push(format!("[tag: {}]", tag));
            }

            if let Some(ref branch) = log.git_branch {
                parts.push(format!("[branch: {}]", branch));
            }

            if let Some(ref summary) = log.summary {
                parts.push(format!("- Summary: {}", summary));
            }

            if let Some(ref fp) = log.first_prompt {
                if fp != "No prompt" {
                    let truncated: String = fp.chars().take(300).collect();
                    parts.push(format!("- First message: {}", truncated));
                }
            }

            if !log.messages.is_empty() {
                let transcript = extract_transcript(&log.messages);
                if !transcript.is_empty() {
                    parts.push(format!("- Transcript: {}", transcript));
                }
            }

            parts.join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n");

    let user_message = format!(
        "Sessions:\n{}\n\nSearch query: \"{}\"\n\nFind the sessions that are most relevant to this query.",
        session_list, query
    );

    let config = SideQueryConfig {
        model: "haiku".to_string(),
        system: SESSION_SEARCH_SYSTEM_PROMPT.to_string(),
        user_message,
    };

    let response = match executor.execute(config).await {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };

    // Parse the JSON response
    let json_match = extract_json_object(&response);
    let json_str = match json_match {
        Some(s) => s,
        None => return Vec::new(),
    };

    let result: AgenticSearchResult = match serde_json::from_str(json_str) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };

    // Map indices back to original log indices
    result
        .relevant_indices
        .iter()
        .filter(|&&idx| idx < logs_to_search_indices.len())
        .map(|&idx| logs_to_search_indices[idx])
        .collect()
}

/// Extract the first JSON object from a string.
fn extract_json_object(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end >= start {
        Some(&text[start..=end])
    } else {
        None
    }
}
