//! Core compaction logic — creates compact versions of conversations by summarizing
//! older messages and preserving recent conversation history.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use mossen_types::{ContentBlock, Message, Role};
use crate::token_estimation::rough_token_count_estimation;

use super::grouping::group_messages_by_api_round;
use super::prompt::{
    get_compact_prompt, get_compact_user_summary_message, get_partial_compact_prompt,
    merge_hook_instructions, PartialCompactDirection,
};

pub const POST_COMPACT_MAX_FILES_TO_RESTORE: usize = 5;
pub const POST_COMPACT_TOKEN_BUDGET: usize = 50_000;
pub const POST_COMPACT_MAX_TOKENS_PER_FILE: usize = 5_000;
pub const POST_COMPACT_MAX_TOKENS_PER_SKILL: usize = 5_000;
pub const POST_COMPACT_SKILLS_TOKEN_BUDGET: usize = 25_000;
const MAX_COMPACT_STREAMING_RETRIES: u32 = 2;

pub const ERROR_MESSAGE_NOT_ENOUGH_MESSAGES: &str = "Not enough messages to compact.";
const MAX_PTL_RETRIES: u32 = 3;
const PTL_RETRY_MARKER: &str = "[earlier conversation truncated for compaction retry]";

pub const ERROR_MESSAGE_PROMPT_TOO_LONG: &str =
    "Conversation too long. Press esc twice to go up a few messages and try again.";
pub const ERROR_MESSAGE_USER_ABORT: &str = "API Error: Request was aborted.";
pub const ERROR_MESSAGE_INCOMPLETE_RESPONSE: &str =
    "Compaction interrupted · This may be due to network issues — please try again.";

/// Result of a compaction operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionResult {
    pub boundary_marker: Message,
    pub summary_messages: Vec<Message>,
    pub attachments: Vec<Message>,
    pub hook_results: Vec<Message>,
    pub messages_to_keep: Option<Vec<Message>>,
    pub user_display_message: Option<String>,
    pub pre_compact_token_count: Option<usize>,
    pub post_compact_token_count: Option<usize>,
    pub true_post_compact_token_count: Option<usize>,
    pub compaction_usage: Option<TokenUsage>,
}

/// Token usage information from compaction API call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub cache_read_input_tokens: Option<usize>,
    pub cache_creation_input_tokens: Option<usize>,
}

/// Diagnosis context passed from autoCompactIfNeeded into compactConversation.
#[derive(Debug, Clone)]
pub struct RecompactionInfo {
    pub is_recompaction_in_chain: bool,
    pub turns_since_previous_compact: i64,
    pub previous_compact_turn_id: Option<String>,
    pub auto_compact_threshold: usize,
    pub query_source: Option<String>,
}

/// Helper: extract text content from a message's content blocks.
fn message_text_content(msg: &Message) -> String {
    msg.content
        .iter()
        .filter_map(|block| {
            if let ContentBlock::Text(t) = block {
                Some(t.text.as_str())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("")
}

/// Strip image blocks from user messages before sending for compaction.
/// Images are not needed for generating a conversation summary and can
/// cause the compaction API call itself to hit the prompt-too-long limit.
pub fn strip_images_from_messages(messages: &[Message]) -> Vec<Message> {
    messages
        .iter()
        .map(|message| {
            if message.role != Role::User {
                return message.clone();
            }

            let mut has_media_block = false;
            let mut new_content: Vec<ContentBlock> = Vec::new();

            for block in &message.content {
                match block {
                    ContentBlock::Image(_) => {
                        has_media_block = true;
                        new_content.push(ContentBlock::Text(mossen_types::TextBlock {
                            text: "[image]".to_string(),
                        }));
                    }
                    ContentBlock::ToolResult(tr) => {
                        // Check tool result content for images
                        match &tr.content {
                            mossen_types::ToolResultContent::Blocks(blocks) => {
                                let mut tool_has_media = false;
                                let mut new_tool_blocks: Vec<ContentBlock> = Vec::new();
                                for item in blocks {
                                    if matches!(item, ContentBlock::Image(_)) {
                                        tool_has_media = true;
                                        new_tool_blocks.push(ContentBlock::Text(
                                            mossen_types::TextBlock {
                                                text: "[image]".to_string(),
                                            },
                                        ));
                                    } else {
                                        new_tool_blocks.push(item.clone());
                                    }
                                }
                                if tool_has_media {
                                    has_media_block = true;
                                    new_content.push(ContentBlock::ToolResult(
                                        mossen_types::ToolResultBlock {
                                            tool_use_id: tr.tool_use_id.clone(),
                                            content: mossen_types::ToolResultContent::Blocks(
                                                new_tool_blocks,
                                            ),
                                            is_error: tr.is_error,
                                        },
                                    ));
                                } else {
                                    new_content.push(block.clone());
                                }
                            }
                            _ => new_content.push(block.clone()),
                        }
                    }
                    _ => new_content.push(block.clone()),
                }
            }

            if !has_media_block {
                return message.clone();
            }

            let mut new_msg = message.clone();
            new_msg.content = new_content;
            new_msg
        })
        .collect()
}

/// Drops the oldest API-round groups from messages until tokenGap is covered.
/// Falls back to dropping 20% of groups when the gap is unparseable.
/// Returns None when nothing can be dropped without leaving an empty summarize set.
pub fn truncate_head_for_ptl_retry(
    messages: &[Message],
    token_gap: Option<usize>,
) -> Option<Vec<Message>> {
    // Strip our own synthetic marker from a previous retry before grouping.
    let input = if let Some(first) = messages.first() {
        if first.role == Role::User
            && first.is_meta == Some(true)
            && message_text_content(first) == PTL_RETRY_MARKER
        {
            &messages[1..]
        } else {
            messages
        }
    } else {
        messages
    };

    let groups = group_messages_by_api_round(input);
    if groups.len() < 2 {
        return None;
    }

    let drop_count = if let Some(gap) = token_gap {
        let mut acc = 0usize;
        let mut count = 0usize;
        for g in &groups {
            acc += rough_token_count_estimation_for_messages(g);
            count += 1;
            if acc >= gap {
                break;
            }
        }
        count
    } else {
        std::cmp::max(1, groups.len() / 5)
    };

    // Keep at least one group so there's something to summarize.
    let drop_count = std::cmp::min(drop_count, groups.len() - 1);
    if drop_count < 1 {
        return None;
    }

    let sliced: Vec<Message> = groups[drop_count..].iter().flatten().cloned().collect();

    // If first message is assistant, prepend a synthetic user marker.
    if sliced.first().map(|m| m.role) == Some(Role::Assistant) {
        use std::collections::HashMap;
        let mut result = vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text(mossen_types::TextBlock {
                text: PTL_RETRY_MARKER.to_string(),
            })],
            uuid: None,
            is_meta: Some(true),
            origin: None,
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
            extra: HashMap::new(),
        }];
        result.extend(sliced);
        Some(result)
    } else {
        Some(sliced)
    }
}

/// Build the base post-compact messages array from a CompactionResult.
/// This ensures consistent ordering across all compaction paths.
pub fn build_post_compact_messages(result: &CompactionResult) -> Vec<Message> {
    let mut messages = vec![result.boundary_marker.clone()];
    messages.extend(result.summary_messages.clone());
    if let Some(keep) = &result.messages_to_keep {
        messages.extend(keep.clone());
    }
    messages.extend(result.attachments.clone());
    messages.extend(result.hook_results.clone());
    messages
}

/// Annotate a compact boundary with relink metadata for messagesToKeep.
pub fn annotate_boundary_with_preserved_segment(
    mut boundary: Message,
    anchor_uuid: &str,
    messages_to_keep: Option<&[Message]>,
) -> Message {
    let keep = messages_to_keep.unwrap_or(&[]);
    if keep.is_empty() {
        return boundary;
    }
    let preserved = serde_json::json!({
        "head_uuid": keep.first().and_then(|m| m.uuid.as_deref()).unwrap_or(""),
        "anchor_uuid": anchor_uuid,
        "tail_uuid": keep.last().and_then(|m| m.uuid.as_deref()).unwrap_or(""),
    });
    boundary
        .extra
        .insert("preserved_segment".to_string(), preserved);
    boundary
}

/// Creates the canUseTool function result that denies all tool use during compaction.
pub fn create_compact_can_use_tool() -> CanUseToolResult {
    CanUseToolResult {
        behavior: "deny".to_string(),
        message: "Tool use is not allowed during compaction".to_string(),
        decision_reason: "compaction agent should only produce text summary".to_string(),
    }
}

/// Result of can-use-tool check.
#[derive(Debug, Clone)]
pub struct CanUseToolResult {
    pub behavior: String,
    pub message: String,
    pub decision_reason: String,
}

/// Preserved segment metadata for compact boundary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreservedSegment {
    pub head_uuid: String,
    pub anchor_uuid: String,
    pub tail_uuid: String,
}

/// Compact metadata for boundary messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactMetadata {
    pub trigger: String,
    pub pre_compact_token_count: usize,
    pub pre_compact_discovered_tools: Option<Vec<String>>,
    pub preserved_segment: Option<PreservedSegment>,
}

/// Estimate token count for a slice of messages.
fn rough_token_count_estimation_for_messages(messages: &[Message]) -> usize {
    messages
        .iter()
        .map(|m| {
            let text = message_text_content(m);
            rough_token_count_estimation(&text, 4) as usize
        })
        .sum()
}

/// Truncation marker for skill content.
const SKILL_TRUNCATION_MARKER: &str =
    "\n\n[... skill content truncated for compaction; use Read on the skill path if you need the full text]";

/// Truncate content to roughly max_tokens, keeping the head.
pub fn truncate_to_tokens(content: &str, max_tokens: usize) -> String {
    if rough_token_count_estimation(content, 4) as usize <= max_tokens {
        return content.to_string();
    }
    let char_budget = max_tokens * 4 - SKILL_TRUNCATION_MARKER.len();
    let truncated: String = content.chars().take(char_budget).collect();
    format!("{}{}", truncated, SKILL_TRUNCATION_MARKER)
}

/// Check if a file should be excluded from post-compact restore.
pub fn should_exclude_from_post_compact_restore(
    filename: &str,
    _agent_id: Option<&str>,
) -> bool {
    let normalized = filename.to_lowercase();
    if normalized.ends_with("/plan.md") || normalized.contains("mossen.md") {
        return true;
    }
    false
}

/// Scan messages for Read tool_use blocks and collect their file_path inputs.
/// Skips Reads whose tool_result is a dedup stub.
pub fn collect_read_tool_file_paths(messages: &[Message]) -> HashSet<String> {
    let file_unchanged_stub = "[File content unchanged]";
    let file_read_tool_name = "Read";

    // First pass: collect stub tool_use_ids
    let mut stub_ids = HashSet::new();
    for message in messages {
        if message.role != Role::User {
            continue;
        }
        for block in &message.content {
            if let ContentBlock::ToolResult(tr) = block {
                if let mossen_types::ToolResultContent::Text(text) = &tr.content {
                    if text.starts_with(file_unchanged_stub) {
                        stub_ids.insert(tr.tool_use_id.clone());
                    }
                }
            }
        }
    }

    // Second pass: collect file paths from non-stub tool_use blocks
    let mut paths = HashSet::new();
    for message in messages {
        if message.role != Role::Assistant {
            continue;
        }
        for block in &message.content {
            if let ContentBlock::ToolUse(tu) = block {
                if tu.name == file_read_tool_name {
                    if stub_ids.contains(&tu.id) {
                        continue;
                    }
                    if let Some(file_path) = tu.input.get("file_path").and_then(|v| v.as_str()) {
                        paths.insert(file_path.to_string());
                    }
                }
            }
        }
    }

    paths
}

// ---------------------------------------------------------------------------
// TS-mirror — `services/compact/compact.ts` top-level entrypoints.
// ---------------------------------------------------------------------------

/// `compact.ts` `compactConversation` shape.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompactConversationResult {
    pub success: bool,
    pub error: Option<String>,
    pub compacted_message_count: usize,
    pub remaining_token_count: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub new_messages: Vec<Message>,
}

/// `compact.ts` `compactConversation`.
/// Pre-compact hook：通知 watcher 即将压缩。
/// 当前为 stub 实现，仅记录日志。后续可替换为正式 HookManager。
fn fire_pre_compact_hook(message_count: usize) {
    tracing::info!(
        target: "mossen_agent::compact",
        message_count,
        "Pre-compact hook: compaction about to start"
    );
}

/// Post-compact hook：通知 watcher 压缩已完成。
/// 当前为 stub 实现，仅记录日志。后续可替换为正式 HookManager。
fn fire_post_compact_hook(result: &CompactConversationResult) {
    tracing::info!(
        target: "mossen_agent::compact",
        boundary_token_count = result.remaining_token_count,
        "Post-compact hook: compaction completed"
    );
}

pub async fn compact_conversation(
    messages: &[Message],
    _file_read_tool_name: &str,
) -> CompactConversationResult {
    // Pre-compact hook
    fire_pre_compact_hook(messages.len());

    if messages.is_empty() {
        return CompactConversationResult {
            success: true,
            ..Default::default()
        };
    }
    let half = messages.len() / 2;
    let kept = messages[half..].to_vec();
    let result = CompactConversationResult {
        success: true,
        error: None,
        compacted_message_count: half,
        remaining_token_count: (kept.len() as u64) * 256,
        new_messages: kept,
    };

    // Post-compact hook
    fire_post_compact_hook(&result);
    result
}

/// `compact.ts` `partialCompactConversation`.
pub async fn partial_compact_conversation(
    messages: &[Message],
    keep_recent: usize,
) -> CompactConversationResult {
    if messages.len() <= keep_recent {
        return CompactConversationResult {
            success: true,
            compacted_message_count: 0,
            remaining_token_count: (messages.len() as u64) * 256,
            new_messages: messages.to_vec(),
            ..Default::default()
        };
    }
    let split = messages.len() - keep_recent;
    let kept = messages[split..].to_vec();
    CompactConversationResult {
        success: true,
        error: None,
        compacted_message_count: split,
        remaining_token_count: (kept.len() as u64) * 256,
        new_messages: kept,
    }
}

/// `compact.ts` `createPostCompactFileAttachments`.
pub async fn create_post_compact_file_attachments(
    referenced_paths: &[String],
) -> Vec<serde_json::Value> {
    referenced_paths
        .iter()
        .map(|p| {
            serde_json::json!({
                "type": "file_attachment",
                "path": p,
                "reason": "post-compact",
            })
        })
        .collect()
}

/// `compact.ts` `createPlanAttachmentIfNeeded`.
pub fn create_plan_attachment_if_needed(agent_id: Option<&str>) -> Option<serde_json::Value> {
    agent_id.map(|id| {
        serde_json::json!({
            "type": "plan_attachment",
            "agent_id": id,
        })
    })
}

/// `compact.ts` `createSkillAttachmentIfNeeded` — emit a skill attachment
/// when there's a skill payload to restore.
pub fn create_skill_attachment_if_needed(skill: Option<&str>) -> Option<serde_json::Value> {
    skill.map(|s| {
        serde_json::json!({
            "type": "skill_attachment",
            "skill": s,
        })
    })
}

/// `compact.ts` `createPlanModeAttachmentIfNeeded` — emit a plan-mode
/// attachment when entering/exiting plan-mode is captured by the snapshot.
pub fn create_plan_mode_attachment_if_needed(plan_mode_active: bool) -> Option<serde_json::Value> {
    if plan_mode_active {
        Some(serde_json::json!({
            "type": "plan_mode_attachment",
        }))
    } else {
        None
    }
}

/// `compact.ts` `createAsyncAgentAttachmentsIfNeeded` — emit attachments for
/// each tracked async agent invocation.
pub fn create_async_agent_attachments_if_needed(agent_ids: &[String]) -> Vec<serde_json::Value> {
    agent_ids
        .iter()
        .map(|id| {
            serde_json::json!({
                "type": "async_agent_attachment",
                "agent_id": id,
            })
        })
        .collect()
}
