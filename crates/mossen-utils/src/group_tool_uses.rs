//! Group tool uses — groups consecutive same-type tool uses from the same API response.

use std::collections::{HashMap, HashSet};

/// 对应 TS `MessageWithoutProgress = Exclude<NormalizedMessage, ProgressMessage>`。
///
/// Rust 端尚未把 `NormalizedMessage` 拆为代数和类型；该别名透传 JSON 形式，
/// 使下游类型签名与 TS 保持一致。
pub type MessageWithoutProgress = serde_json::Value;

/// A tool use info extracted from a message.
#[derive(Debug, Clone)]
pub struct ToolUseInfo {
    pub message_id: String,
    pub tool_use_id: String,
    pub tool_name: String,
}

/// A grouped tool use message for rendering.
#[derive(Debug, Clone)]
pub struct GroupedToolUseMessage {
    pub tool_name: String,
    pub message_indices: Vec<usize>,
    pub result_indices: Vec<usize>,
    pub uuid: String,
    pub timestamp: u64,
    pub message_id: String,
}

/// Result of the grouping operation.
pub struct GroupingResult {
    /// Indices of messages to render, with grouped messages replaced by GroupedToolUseMessage.
    pub renderable: Vec<RenderableItem>,
}

/// An item in the renderable output.
#[derive(Debug, Clone)]
pub enum RenderableItem {
    /// A single original message (by index).
    Single(usize),
    /// A grouped tool use.
    Grouped(GroupedToolUseMessage),
}

/// Apply grouping to tool use messages.
///
/// Groups 2+ tools of the same type from the same API message if the tool
/// supports grouped rendering. Also collects corresponding tool_results.
/// When verbose is true, skips grouping so messages render at original positions.
pub fn apply_grouping(
    tool_use_infos: &[Option<ToolUseInfo>],
    message_types: &[&str],
    tool_result_ids: &[Vec<String>],
    tools_with_grouping: &HashSet<String>,
    verbose: bool,
) -> GroupingResult {
    let count = tool_use_infos.len();

    // In verbose mode, don't group
    if verbose {
        return GroupingResult {
            renderable: (0..count).map(RenderableItem::Single).collect(),
        };
    }

    // First pass: group tool uses by message_id + tool_name
    let mut groups: HashMap<String, Vec<usize>> = HashMap::new();

    for (idx, info) in tool_use_infos.iter().enumerate() {
        if let Some(ref info) = info {
            if tools_with_grouping.contains(&info.tool_name) {
                let key = format!("{}:{}", info.message_id, info.tool_name);
                groups.entry(key).or_default().push(idx);
            }
        }
    }

    // Identify valid groups (2+ items) and collect their tool use IDs
    let mut valid_groups: HashMap<String, Vec<usize>> = HashMap::new();
    let mut grouped_tool_use_ids: HashSet<String> = HashSet::new();

    for (key, group) in &groups {
        if group.len() >= 2 {
            valid_groups.insert(key.clone(), group.clone());
            for &idx in group {
                if let Some(ref info) = tool_use_infos[idx] {
                    grouped_tool_use_ids.insert(info.tool_use_id.clone());
                }
            }
        }
    }

    // Collect result messages for grouped tool_uses
    let mut results_by_tool_use_id: HashMap<String, usize> = HashMap::new();
    for (idx, result_ids) in tool_result_ids.iter().enumerate() {
        if message_types[idx] == "user" {
            for id in result_ids {
                if grouped_tool_use_ids.contains(id) {
                    results_by_tool_use_id.insert(id.clone(), idx);
                }
            }
        }
    }

    // Second pass: build output
    let mut result: Vec<RenderableItem> = Vec::new();
    let mut emitted_groups: HashSet<String> = HashSet::new();

    for idx in 0..count {
        if let Some(ref info) = tool_use_infos[idx] {
            let key = format!("{}:{}", info.message_id, info.tool_name);

            if let Some(group) = valid_groups.get(&key) {
                if !emitted_groups.contains(&key) {
                    emitted_groups.insert(key.clone());

                    // Collect result indices for this group
                    let mut result_indices = Vec::new();
                    for &g_idx in group {
                        if let Some(ref g_info) = tool_use_infos[g_idx] {
                            if let Some(&r_idx) =
                                results_by_tool_use_id.get(&g_info.tool_use_id)
                            {
                                result_indices.push(r_idx);
                            }
                        }
                    }

                    result.push(RenderableItem::Grouped(GroupedToolUseMessage {
                        tool_name: info.tool_name.clone(),
                        message_indices: group.clone(),
                        result_indices,
                        uuid: format!("grouped-{}", idx),
                        timestamp: 0,
                        message_id: info.message_id.clone(),
                    }));
                }
                continue;
            }
        }

        // Skip user messages whose tool_results are all grouped
        if message_types[idx] == "user" && !tool_result_ids[idx].is_empty() {
            let all_grouped = tool_result_ids[idx]
                .iter()
                .all(|id| grouped_tool_use_ids.contains(id));
            if all_grouped {
                continue;
            }
        }

        result.push(RenderableItem::Single(idx));
    }

    GroupingResult { renderable: result }
}
