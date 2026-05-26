//! Tool Search utilities for dynamically discovering deferred tools.
//!
//! When enabled, deferred tools (MCP and shouldDefer tools) are sent with
//! defer_loading: true and discovered via ToolSearchTool rather than being
//! loaded upfront.

use std::collections::HashSet;
use std::sync::Mutex;

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

/// Default percentage of context window at which to auto-enable tool search.
const DEFAULT_AUTO_TOOL_SEARCH_PERCENTAGE: u32 = 10;

/// Approximate chars per token for MCP tool definitions.
const CHARS_PER_TOKEN: f64 = 2.5;

/// Tool search tool name constant.
pub const TOOL_SEARCH_TOOL_NAME: &str = "tool_search";

/// Tool search mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolSearchMode {
    /// Tool Search Tool — deferred tools discovered via ToolSearchTool
    Tst,
    /// Auto — tools deferred only when they exceed threshold
    TstAuto,
    /// Tool search disabled — all tools exposed inline
    Standard,
}

impl ToolSearchMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Tst => "tst",
            Self::TstAuto => "tst-auto",
            Self::Standard => "standard",
        }
    }
}

/// Parse auto:N syntax from ENABLE_TOOL_SEARCH env var.
fn parse_auto_percentage(value: &str) -> Option<u32> {
    if !value.starts_with("auto:") {
        return None;
    }
    let percent_str = &value[5..];
    let percent: i32 = percent_str.parse().ok()?;
    Some(percent.clamp(0, 100) as u32)
}

/// Check if ENABLE_TOOL_SEARCH is set to auto mode.
fn is_auto_tool_search_mode(value: Option<&str>) -> bool {
    match value {
        None => false,
        Some(v) => v == "auto" || v.starts_with("auto:"),
    }
}

/// Get the auto-enable percentage from env var or default.
pub fn get_auto_tool_search_percentage() -> u32 {
    let value = std::env::var("ENABLE_TOOL_SEARCH").ok();
    match value.as_deref() {
        None => DEFAULT_AUTO_TOOL_SEARCH_PERCENTAGE,
        Some("auto") => DEFAULT_AUTO_TOOL_SEARCH_PERCENTAGE,
        Some(v) => parse_auto_percentage(v).unwrap_or(DEFAULT_AUTO_TOOL_SEARCH_PERCENTAGE),
    }
}

/// Get the character threshold for auto-enabling tool search for a given context window.
pub fn get_auto_tool_search_char_threshold(context_window: u64) -> u64 {
    let percentage = get_auto_tool_search_percentage() as f64 / 100.0;
    let token_threshold = (context_window as f64 * percentage) as u64;
    (token_threshold as f64 * CHARS_PER_TOKEN) as u64
}

/// Determines the tool search mode from ENABLE_TOOL_SEARCH env var.
pub fn get_tool_search_mode() -> ToolSearchMode {
    // Kill switch check
    if std::env::var("MOSSEN_CODE_DISABLE_EXPERIMENTAL_BETAS")
        .map(|v| is_env_truthy(&v))
        .unwrap_or(false)
    {
        return ToolSearchMode::Standard;
    }

    let value = std::env::var("ENABLE_TOOL_SEARCH").ok();

    // Handle auto:N syntax
    if let Some(ref v) = value {
        if let Some(percent) = parse_auto_percentage(v) {
            if percent == 0 {
                return ToolSearchMode::Tst;
            }
            if percent == 100 {
                return ToolSearchMode::Standard;
            }
        }
    }

    if is_auto_tool_search_mode(value.as_deref()) {
        return ToolSearchMode::TstAuto;
    }

    match value.as_deref() {
        Some(v) if is_env_truthy(v) => ToolSearchMode::Tst,
        Some(v) if is_env_defined_falsy(v) => ToolSearchMode::Standard,
        _ => ToolSearchMode::Tst, // default
    }
}

/// Default patterns for models that do NOT support tool_reference.
const DEFAULT_UNSUPPORTED_MODEL_PATTERNS: &[&str] = &["fast"];

/// Check if a model supports tool_reference blocks.
pub fn model_supports_tool_reference(model: &str) -> bool {
    let normalized = model.to_lowercase();
    for pattern in DEFAULT_UNSUPPORTED_MODEL_PATTERNS {
        if normalized.contains(pattern) {
            return false;
        }
    }
    true
}

/// Optimistic check if tool search might be enabled.
static LOGGED_OPTIMISTIC: Lazy<Mutex<bool>> = Lazy::new(|| Mutex::new(false));

pub fn is_tool_search_enabled_optimistic() -> bool {
    let mode = get_tool_search_mode();
    if mode == ToolSearchMode::Standard {
        return false;
    }

    // Check for third-party API proxy
    let enable_var = std::env::var("ENABLE_TOOL_SEARCH").ok();
    if enable_var.is_none() || enable_var.as_deref() == Some("") {
        if let Ok(base_url) = std::env::var("MOSSEN_CODE_API_BASE_URL") {
            if !base_url.is_empty() && !is_first_party_mossen_base_url(&base_url) {
                return false;
            }
        }
    }

    true
}

/// Check if ToolSearchTool is available in the tools list.
pub fn is_tool_search_tool_available(tool_names: &[&str]) -> bool {
    tool_names.contains(&TOOL_SEARCH_TOOL_NAME)
}

/// Check if an object is a tool_reference block.
pub fn is_tool_reference_block(value: &serde_json::Value) -> bool {
    value.get("type").and_then(|t| t.as_str()) == Some("tool_reference")
}

/// Extract tool names from tool_reference blocks in message history.
pub fn extract_discovered_tool_names(messages: &[serde_json::Value]) -> HashSet<String> {
    let mut discovered_tools = HashSet::new();

    for msg in messages {
        // Compact boundary carries the pre-compact discovered set
        if msg.get("type").and_then(|t| t.as_str()) == Some("system")
            && msg.get("subtype").and_then(|t| t.as_str()) == Some("compact_boundary")
        {
            if let Some(carried) = msg
                .get("compactMetadata")
                .and_then(|cm| cm.get("preCompactDiscoveredTools"))
                .and_then(|tools| tools.as_array())
            {
                for name in carried.iter().filter_map(|v| v.as_str()) {
                    discovered_tools.insert(name.to_string());
                }
            }
            continue;
        }

        // Only user messages contain tool_result blocks
        if msg.get("type").and_then(|t| t.as_str()) != Some("user") {
            continue;
        }

        let content = match msg
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_array())
        {
            Some(arr) => arr,
            None => continue,
        };

        for block in content {
            if block.get("type").and_then(|t| t.as_str()) != Some("tool_result") {
                continue;
            }
            if let Some(inner_content) = block.get("content").and_then(|c| c.as_array()) {
                for item in inner_content {
                    if item.get("type").and_then(|t| t.as_str()) == Some("tool_reference") {
                        if let Some(name) = item.get("tool_name").and_then(|n| n.as_str()) {
                            discovered_tools.insert(name.to_string());
                        }
                    }
                }
            }
        }
    }

    discovered_tools
}

/// Deferred tools delta.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeferredToolsDelta {
    pub added_names: Vec<String>,
    pub added_lines: Vec<String>,
    pub removed_names: Vec<String>,
}

/// Scan context for deferred tools delta.
#[derive(Debug, Clone)]
pub enum DeferredToolsDeltaScanContext {
    AttachmentsMain,
    AttachmentsSubagent,
    CompactFull,
    CompactPartial,
    ReactiveCompact,
}

impl DeferredToolsDeltaScanContext {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AttachmentsMain => "attachments_main",
            Self::AttachmentsSubagent => "attachments_subagent",
            Self::CompactFull => "compact_full",
            Self::CompactPartial => "compact_partial",
            Self::ReactiveCompact => "reactive_compact",
        }
    }
}

/// Check if deferred tools delta feature is enabled.
pub fn is_deferred_tools_delta_enabled() -> bool {
    std::env::var("USER_TYPE")
        .map(|v| v == "internal")
        .unwrap_or(false)
}

/// Tool info for deferred tool calculations.
#[derive(Debug, Clone)]
pub struct DeferredToolInfo {
    pub name: String,
    pub is_deferred: bool,
    pub description_chars: usize,
}

/// Calculate total deferred tool description size in characters.
pub fn calculate_deferred_tool_description_chars(tools: &[DeferredToolInfo]) -> usize {
    tools
        .iter()
        .filter(|t| t.is_deferred)
        .map(|t| t.name.len() + t.description_chars)
        .sum()
}

/// Get deferred tools delta between current pool and what's announced in messages.
pub fn get_deferred_tools_delta(
    tools: &[DeferredToolInfo],
    messages: &[serde_json::Value],
) -> Option<DeferredToolsDelta> {
    let mut announced = HashSet::new();

    for msg in messages {
        if msg.get("type").and_then(|t| t.as_str()) != Some("attachment") {
            continue;
        }
        let attachment = match msg.get("attachment") {
            Some(a) => a,
            None => continue,
        };
        if attachment.get("type").and_then(|t| t.as_str()) != Some("deferred_tools_delta") {
            continue;
        }
        if let Some(added) = attachment.get("addedNames").and_then(|a| a.as_array()) {
            for name in added.iter().filter_map(|v| v.as_str()) {
                announced.insert(name.to_string());
            }
        }
        if let Some(removed) = attachment.get("removedNames").and_then(|r| r.as_array()) {
            for name in removed.iter().filter_map(|v| v.as_str()) {
                announced.remove(name);
            }
        }
    }

    let deferred_names: HashSet<String> = tools
        .iter()
        .filter(|t| t.is_deferred)
        .map(|t| t.name.clone())
        .collect();

    let pool_names: HashSet<String> = tools.iter().map(|t| t.name.clone()).collect();

    let added: Vec<String> = tools
        .iter()
        .filter(|t| t.is_deferred && !announced.contains(&t.name))
        .map(|t| t.name.clone())
        .collect();

    let removed: Vec<String> = announced
        .iter()
        .filter(|n| !deferred_names.contains(*n) && !pool_names.contains(*n))
        .cloned()
        .collect();

    if added.is_empty() && removed.is_empty() {
        return None;
    }

    let mut sorted_added = added;
    sorted_added.sort();
    let added_lines: Vec<String> = sorted_added
        .iter()
        .map(|name| format!("- {}", name))
        .collect();
    let mut sorted_removed = removed;
    sorted_removed.sort();

    Some(DeferredToolsDelta {
        added_names: sorted_added,
        added_lines,
        removed_names: sorted_removed,
    })
}

// --------------------------------------------------------------------------
// Helper functions
// --------------------------------------------------------------------------

fn is_env_truthy(value: &str) -> bool {
    matches!(value.to_lowercase().as_str(), "1" | "true" | "yes")
}

fn is_env_defined_falsy(value: &str) -> bool {
    matches!(value.to_lowercase().as_str(), "0" | "false" | "no")
}

fn is_first_party_mossen_base_url(url: &str) -> bool {
    let lower = url.to_lowercase();
    lower.contains("provider.com") || lower.contains("mossen.ai")
}

/// 对应 TS `isToolSearchEnabled`：异步版本，综合设置/灰度后判断 tool-search 是否开启。
pub async fn is_tool_search_enabled(settings_enabled: bool, feature_gate_enabled: bool) -> bool {
    if std::env::var("MOSSEN_DISABLE_TOOL_SEARCH")
        .map(|v| v == "1" || v == "true")
        .unwrap_or(false)
    {
        return false;
    }
    settings_enabled && feature_gate_enabled
}
