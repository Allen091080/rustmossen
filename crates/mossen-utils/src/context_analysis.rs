//! Context analysis for token usage tracking.
//!
//! Analyzes message history to determine token distribution across
//! tool requests, results, human messages, and assistant messages.

use serde_json::Value;
use std::collections::HashMap;

/// Token statistics for context analysis.
#[derive(Debug, Clone, Default)]
pub struct TokenStats {
    pub tool_requests: HashMap<String, usize>,
    pub tool_results: HashMap<String, usize>,
    pub human_messages: usize,
    pub assistant_messages: usize,
    pub local_command_outputs: usize,
    pub other: usize,
    pub attachments: HashMap<String, usize>,
    pub duplicate_file_reads: HashMap<String, DuplicateReadInfo>,
    pub total: usize,
}

/// Information about duplicate file reads.
#[derive(Debug, Clone)]
pub struct DuplicateReadInfo {
    pub count: usize,
    pub tokens: usize,
}

/// A normalized message for analysis.
#[derive(Debug, Clone)]
pub struct AnalysisMessage {
    pub msg_type: String,
    pub content: MessageContent,
}

/// Message content (string or blocks).
#[derive(Debug, Clone)]
pub enum MessageContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

/// A content block in a message.
#[derive(Debug, Clone)]
pub struct ContentBlock {
    pub block_type: String,
    pub name: Option<String>,
    pub id: Option<String>,
    pub tool_use_id: Option<String>,
    pub text: Option<String>,
    pub input: Option<Value>,
    pub raw: Value,
}

/// Rough token count estimation (4 chars per token heuristic).
pub fn rough_token_count_estimation(text: &str) -> usize {
    (text.len() + 3) / 4
}

/// Analyze context token distribution.
pub fn analyze_context(messages: &[AnalysisMessage]) -> TokenStats {
    let mut stats = TokenStats::default();
    let mut tool_ids_to_names: HashMap<String, String> = HashMap::new();
    let mut read_tool_id_to_path: HashMap<String, String> = HashMap::new();
    let mut file_read_stats: HashMap<String, (usize, usize)> = HashMap::new();

    for msg in messages {
        match &msg.content {
            MessageContent::Text(text) => {
                let tokens = rough_token_count_estimation(text);
                stats.total += tokens;
                if msg.msg_type == "user" && text.contains("local-command-stdout") {
                    stats.local_command_outputs += tokens;
                } else if msg.msg_type == "user" {
                    stats.human_messages += tokens;
                } else {
                    stats.assistant_messages += tokens;
                }
            }
            MessageContent::Blocks(blocks) => {
                for block in blocks {
                    process_block(
                        block,
                        &msg.msg_type,
                        &mut stats,
                        &mut tool_ids_to_names,
                        &mut read_tool_id_to_path,
                        &mut file_read_stats,
                    );
                }
            }
        }
    }

    // Calculate duplicate file reads
    for (path, (count, total_tokens)) in &file_read_stats {
        if *count > 1 {
            let avg_tokens = total_tokens / count;
            let duplicate_tokens = avg_tokens * (count - 1);
            stats.duplicate_file_reads.insert(
                path.clone(),
                DuplicateReadInfo {
                    count: *count,
                    tokens: duplicate_tokens,
                },
            );
        }
    }

    stats
}

fn process_block(
    block: &ContentBlock,
    msg_type: &str,
    stats: &mut TokenStats,
    tool_ids: &mut HashMap<String, String>,
    read_tool_paths: &mut HashMap<String, String>,
    file_reads: &mut HashMap<String, (usize, usize)>,
) {
    let raw_str = serde_json::to_string(&block.raw).unwrap_or_default();
    let tokens = rough_token_count_estimation(&raw_str);
    stats.total += tokens;

    match block.block_type.as_str() {
        "text" => {
            if msg_type == "user"
                && block.text.as_deref().map_or(false, |t| t.contains("local-command-stdout"))
            {
                stats.local_command_outputs += tokens;
            } else if msg_type == "user" {
                stats.human_messages += tokens;
            } else {
                stats.assistant_messages += tokens;
            }
        }
        "tool_use" => {
            if let (Some(name), Some(id)) = (&block.name, &block.id) {
                let tool_name = name.clone();
                *stats.tool_requests.entry(tool_name.clone()).or_insert(0) += tokens;
                tool_ids.insert(id.clone(), tool_name.clone());

                // Track Read tool file paths
                if tool_name == "Read" {
                    if let Some(input) = &block.input {
                        if let Some(fp) = input.get("file_path").and_then(|v| v.as_str()) {
                            read_tool_paths.insert(id.clone(), fp.to_string());
                        }
                    }
                }
            }
        }
        "tool_result" => {
            if let Some(tool_use_id) = &block.tool_use_id {
                let tool_name = tool_ids.get(tool_use_id).cloned().unwrap_or_else(|| "unknown".to_string());
                *stats.tool_results.entry(tool_name.clone()).or_insert(0) += tokens;

                if tool_name == "Read" {
                    if let Some(path) = read_tool_paths.get(tool_use_id) {
                        let entry = file_reads.entry(path.clone()).or_insert((0, 0));
                        entry.0 += 1;
                        entry.1 += tokens;
                    }
                }
            }
        }
        _ => {
            stats.other += tokens;
        }
    }
}

/// Convert token stats to metrics for analytics.
pub fn token_stats_to_metrics(stats: &TokenStats) -> HashMap<String, f64> {
    let mut metrics = HashMap::new();
    metrics.insert("total_tokens".to_string(), stats.total as f64);
    metrics.insert("human_message_tokens".to_string(), stats.human_messages as f64);
    metrics.insert("assistant_message_tokens".to_string(), stats.assistant_messages as f64);
    metrics.insert("local_command_output_tokens".to_string(), stats.local_command_outputs as f64);
    metrics.insert("other_tokens".to_string(), stats.other as f64);

    for (type_name, count) in &stats.attachments {
        metrics.insert(format!("attachment_{}_count", type_name), *count as f64);
    }

    for (tool, tokens) in &stats.tool_requests {
        metrics.insert(format!("tool_request_{}_tokens", tool), *tokens as f64);
    }

    for (tool, tokens) in &stats.tool_results {
        metrics.insert(format!("tool_result_{}_tokens", tool), *tokens as f64);
    }

    let duplicate_total: usize = stats.duplicate_file_reads.values().map(|d| d.tokens).sum();
    metrics.insert("duplicate_read_tokens".to_string(), duplicate_total as f64);
    metrics.insert("duplicate_read_file_count".to_string(), stats.duplicate_file_reads.len() as f64);

    if stats.total > 0 {
        let total = stats.total as f64;
        metrics.insert("human_message_percent".to_string(), (stats.human_messages as f64 / total * 100.0).round());
        metrics.insert("assistant_message_percent".to_string(), (stats.assistant_messages as f64 / total * 100.0).round());
        metrics.insert("local_command_output_percent".to_string(), (stats.local_command_outputs as f64 / total * 100.0).round());
        metrics.insert("duplicate_read_percent".to_string(), (duplicate_total as f64 / total * 100.0).round());

        let tool_request_total: usize = stats.tool_requests.values().sum();
        let tool_result_total: usize = stats.tool_results.values().sum();
        metrics.insert("tool_request_percent".to_string(), (tool_request_total as f64 / total * 100.0).round());
        metrics.insert("tool_result_percent".to_string(), (tool_result_total as f64 / total * 100.0).round());

        for (tool, tokens) in &stats.tool_requests {
            metrics.insert(format!("tool_request_{}_percent", tool), (*tokens as f64 / total * 100.0).round());
        }
        for (tool, tokens) in &stats.tool_results {
            metrics.insert(format!("tool_result_{}_percent", tool), (*tokens as f64 / total * 100.0).round());
        }
    }

    metrics
}

/// 对应 TS `tokenStatsToStatsigMetrics`：把 token stats 字典转换为 statsig 风格的指标。
pub fn token_stats_to_statsig_metrics(
    token_stats: &HashMap<String, usize>,
) -> HashMap<String, f64> {
    token_stats
        .iter()
        .map(|(k, v)| (format!("tokens.{}", k), *v as f64))
        .collect()
}
