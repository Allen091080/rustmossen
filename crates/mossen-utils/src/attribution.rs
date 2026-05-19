//! Attribution
//!
//! Generates attribution text for commits and PRs, counts user prompts,
//! and computes enhanced PR attribution with contribution stats.

use std::collections::HashSet;

/// Attribution texts for commits and PRs.
#[derive(Debug, Clone)]
pub struct AttributionTexts {
    pub commit: String,
    pub pr: String,
}

/// Attribution data summary.
#[derive(Debug, Clone)]
pub struct AttributionDataSummary {
    pub mossen_percent: u32,
}

/// Attribution data.
#[derive(Debug, Clone)]
pub struct AttributionData {
    pub summary: AttributionDataSummary,
}

/// Terminal output tags that indicate a message is not a user prompt.
const TERMINAL_OUTPUT_TAGS: &[&str] = &[
    "bash_input",
    "bash_output",
    "bash_stderr",
    "local_command_output",
    "caveat",
];

/// Content block type for message analysis.
#[derive(Debug, Clone)]
pub enum ContentBlock {
    Text { text: String },
    Image,
    Document,
    ToolResult,
    ToolUse { name: String, input: serde_json::Value },
    Other,
}

/// A message entry for prompt counting.
#[derive(Debug, Clone)]
pub struct MessageEntry {
    pub entry_type: String,
    pub content: Option<MessageContent>,
    pub is_sidechain: bool,
}

/// Message content — either a string or an array of blocks.
#[derive(Debug, Clone)]
pub enum MessageContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

/// Check if a message content string is terminal output.
fn is_terminal_output(content: &str) -> bool {
    TERMINAL_OUTPUT_TAGS
        .iter()
        .any(|tag| content.contains(&format!("<{}>", tag)))
}

/// Count user messages with visible text content in a list of non-sidechain messages.
pub fn count_user_prompts_in_messages(messages: &[MessageEntry]) -> usize {
    let mut count = 0;

    for message in messages {
        if message.entry_type != "user" {
            continue;
        }

        let content = match &message.content {
            Some(c) => c,
            None => continue,
        };

        let has_user_text = match content {
            MessageContent::Text(text) => {
                if is_terminal_output(text) {
                    continue;
                }
                !text.trim().is_empty()
            }
            MessageContent::Blocks(blocks) => blocks.iter().any(|block| match block {
                ContentBlock::Text { text } => !is_terminal_output(text),
                ContentBlock::Image | ContentBlock::Document => true,
                _ => false,
            }),
        };

        if has_user_text {
            count += 1;
        }
    }

    count
}

/// Count user prompts from entries, excluding sidechain messages.
pub fn count_user_prompts_from_entries(entries: &[MessageEntry]) -> usize {
    let non_sidechain: Vec<&MessageEntry> = entries
        .iter()
        .filter(|e| e.entry_type == "user" && !e.is_sidechain)
        .collect();

    // Re-wrap as MessageEntry slice for counting
    let refs: Vec<MessageEntry> = non_sidechain.into_iter().cloned().collect();
    count_user_prompts_in_messages(&refs)
}

/// Memory access tool names for counting memory file accesses.
static MEMORY_ACCESS_TOOL_NAMES: once_cell::sync::Lazy<HashSet<&'static str>> =
    once_cell::sync::Lazy::new(|| {
        let mut s = HashSet::new();
        s.insert("Read");
        s.insert("Grep");
        s.insert("Glob");
        s.insert("Edit");
        s.insert("Write");
        s
    });

/// Count memory file accesses in transcript entries.
pub fn count_memory_file_access_from_entries(entries: &[MessageEntry]) -> usize {
    let mut count = 0;
    for entry in entries {
        if entry.entry_type != "assistant" {
            continue;
        }
        if let Some(MessageContent::Blocks(blocks)) = &entry.content {
            for block in blocks {
                if let ContentBlock::ToolUse { name, input } = block {
                    if MEMORY_ACCESS_TOOL_NAMES.contains(name.as_str()) {
                        // Check if this is a memory file access based on file path
                        if is_memory_file_access(name, input) {
                            count += 1;
                        }
                    }
                }
            }
        }
    }
    count
}

/// Check if a tool use is a memory file access.
fn is_memory_file_access(tool_name: &str, input: &serde_json::Value) -> bool {
    let file_path = input
        .get("file_path")
        .or_else(|| input.get("path"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // Memory files are in MOSSEN.md or .mossen/ directory
    file_path.contains("MOSSEN.md")
        || file_path.contains(".mossen/")
        || file_path.contains(".mossen\\")
}

/// Returns attribution text for commits and PRs.
pub fn get_attribution_texts(
    model_name: &str,
    product_display_name: &str,
    product_url: &str,
    is_undercover: bool,
    user_type: &str,
    client_type: &str,
    custom_commit: Option<&str>,
    custom_pr: Option<&str>,
    include_co_authored_by: Option<bool>,
) -> AttributionTexts {
    if user_type == "ant" && is_undercover {
        return AttributionTexts {
            commit: String::new(),
            pr: String::new(),
        };
    }

    if client_type == "remote" {
        return AttributionTexts {
            commit: String::new(),
            pr: String::new(),
        };
    }

    let default_attribution = format!(
        "🤖 Generated with [{}]({})",
        product_display_name, product_url
    );
    let default_commit = format!("Co-Authored-By: {} <noreply@mossen.invalid>", model_name);

    // Custom attribution settings take precedence
    if custom_commit.is_some() || custom_pr.is_some() {
        return AttributionTexts {
            commit: custom_commit.unwrap_or(&default_commit).to_string(),
            pr: custom_pr.unwrap_or(&default_attribution).to_string(),
        };
    }

    // Backward compatibility: deprecated includeCoAuthoredBy setting
    if include_co_authored_by == Some(false) {
        return AttributionTexts {
            commit: String::new(),
            pr: String::new(),
        };
    }

    AttributionTexts {
        commit: default_commit,
        pr: default_attribution,
    }
}

/// Build enhanced PR attribution text with contribution stats.
pub fn build_enhanced_pr_attribution(
    product_display_name: &str,
    product_url: &str,
    mossen_percent: u32,
    prompt_count: usize,
    memory_access_count: usize,
    short_model_name: &str,
) -> String {
    let mem_suffix = if memory_access_count > 0 {
        let word = if memory_access_count == 1 {
            "memory"
        } else {
            "memories"
        };
        format!(", {} {} recalled", memory_access_count, word)
    } else {
        String::new()
    };

    format!(
        "🤖 Generated with [{}]({}) ({}% {}-shotted by {}{})",
        product_display_name,
        product_url,
        mossen_percent,
        prompt_count,
        short_model_name,
        mem_suffix
    )
}

/// 对应 TS `getEnhancedPRAttribution`：根据用户类型、配置与 AppState 生成
/// 增强版 PR 归属文本。
///
/// Rust 端没有完整 `AppState`，调用方需预先聚合：是否 ant undercover、
/// 是否 remote、用户配置的 attribution（如果有）。函数按照 TS 优先级回退
/// 到默认 `🤖 Generated with [Mossen](url)`。
pub async fn get_enhanced_pr_attribution(
    is_ant_undercover: bool,
    is_remote: bool,
    remote_session_url: Option<String>,
    user_attribution: Option<String>,
    include_co_authored: bool,
    product_display_name: &str,
    product_url: &str,
) -> String {
    if is_ant_undercover {
        return String::new();
    }
    if is_remote {
        return remote_session_url.unwrap_or_default();
    }
    if let Some(custom) = user_attribution {
        return custom;
    }
    if !include_co_authored {
        return String::new();
    }
    format!(
        "🤖 Generated with [{}]({})",
        product_display_name, product_url
    )
}
