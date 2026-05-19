//! Collapse consecutive read/search operations into summary groups.
//!
//! Provides logic for detecting collapsible tool uses (searches, reads,
//! directory listings), grouping them, and generating summary text.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

// --------------------------------------------------------------------------
// Constants
// --------------------------------------------------------------------------

const MAX_HINT_CHARS: usize = 300;

// --------------------------------------------------------------------------
// Types
// --------------------------------------------------------------------------

/// Result of checking if a tool use is a search or read operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchOrReadResult {
    pub is_collapsible: bool,
    pub is_search: bool,
    pub is_read: bool,
    pub is_list: bool,
    pub is_repl: bool,
    /// True if this is a Write/Edit targeting a memory file.
    pub is_memory_write: bool,
    /// True for meta-operations that should be absorbed into a collapse group
    /// without incrementing any count.
    pub is_absorbed_silently: bool,
    /// MCP server name when this is an MCP tool.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_server_name: Option<String>,
    /// Bash command that is NOT a search/read (under fullscreen mode).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_bash: Option<bool>,
}

impl SearchOrReadResult {
    /// Create a non-collapsible result.
    pub fn not_collapsible() -> Self {
        Self {
            is_collapsible: false,
            is_search: false,
            is_read: false,
            is_list: false,
            is_repl: false,
            is_memory_write: false,
            is_absorbed_silently: false,
            mcp_server_name: None,
            is_bash: None,
        }
    }

    /// Create a collapsible search result.
    pub fn search() -> Self {
        Self {
            is_collapsible: true,
            is_search: true,
            is_read: false,
            is_list: false,
            is_repl: false,
            is_memory_write: false,
            is_absorbed_silently: false,
            mcp_server_name: None,
            is_bash: None,
        }
    }

    /// Create a collapsible read result.
    pub fn read() -> Self {
        Self {
            is_collapsible: true,
            is_search: false,
            is_read: true,
            is_list: false,
            is_repl: false,
            is_memory_write: false,
            is_absorbed_silently: false,
            mcp_server_name: None,
            is_bash: None,
        }
    }

    /// Create a collapsible directory listing result.
    pub fn list() -> Self {
        Self {
            is_collapsible: true,
            is_search: false,
            is_read: false,
            is_list: true,
            is_repl: false,
            is_memory_write: false,
            is_absorbed_silently: false,
            mcp_server_name: None,
            is_bash: None,
        }
    }

    /// Create a REPL result (absorbed silently).
    pub fn repl() -> Self {
        Self {
            is_collapsible: true,
            is_search: false,
            is_read: false,
            is_list: false,
            is_repl: true,
            is_memory_write: false,
            is_absorbed_silently: true,
            mcp_server_name: None,
            is_bash: None,
        }
    }

    /// Create a memory write result.
    pub fn memory_write() -> Self {
        Self {
            is_collapsible: true,
            is_search: false,
            is_read: false,
            is_list: false,
            is_repl: false,
            is_memory_write: true,
            is_absorbed_silently: false,
            mcp_server_name: None,
            is_bash: None,
        }
    }

    /// Create a silently-absorbed meta-operation.
    pub fn absorbed_silently() -> Self {
        Self {
            is_collapsible: true,
            is_search: false,
            is_read: false,
            is_list: false,
            is_repl: false,
            is_memory_write: false,
            is_absorbed_silently: true,
            mcp_server_name: None,
            is_bash: None,
        }
    }
}

/// Git operation tracking for collapsed groups.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitInfo {
    pub sha: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushInfo {
    pub branch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchInfo {
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrInfo {
    pub number: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    pub action: String,
}

/// Hook info for pre-tool-use timing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopHookInfo {
    pub hook_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

/// A collapsed group of read/search operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollapsedReadSearchGroup {
    pub search_count: usize,
    pub read_count: usize,
    pub list_count: usize,
    pub repl_count: usize,
    pub memory_search_count: usize,
    pub memory_read_count: usize,
    pub memory_write_count: usize,
    pub read_file_paths: Vec<String>,
    pub search_args: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_display_hint: Option<String>,
    pub uuid: String,
    pub timestamp: u64,
    // MCP tools
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_call_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_server_names: Option<Vec<String>>,
    // Bash commands (fullscreen mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bash_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_op_bash_count: Option<usize>,
    // Git ops
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commits: Option<Vec<CommitInfo>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pushes: Option<Vec<PushInfo>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branches: Option<Vec<BranchInfo>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prs: Option<Vec<PrInfo>>,
    // Hooks
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hook_total_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hook_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hook_infos: Option<Vec<StopHookInfo>>,
    // Team memory (feature flag)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_memory_search_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_memory_read_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_memory_write_count: Option<usize>,
    // Relevant memories
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relevant_memories: Option<Vec<RelevantMemory>>,
}

/// A relevant memory attachment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelevantMemory {
    pub path: String,
    pub content: String,
    pub mtime_ms: u64,
}

/// Memory counts for summary text generation.
#[derive(Debug, Clone, Default)]
pub struct MemoryCounts {
    pub memory_search_count: usize,
    pub memory_read_count: usize,
    pub memory_write_count: usize,
    pub team_memory_search_count: Option<usize>,
    pub team_memory_read_count: Option<usize>,
    pub team_memory_write_count: Option<usize>,
}

// --------------------------------------------------------------------------
// Helper functions
// --------------------------------------------------------------------------

/// Format a bash command for the ⎿ hint. Drops blank lines, collapses runs of
/// inline whitespace, then caps total length.
pub fn command_as_hint(command: &str) -> String {
    let cleaned = format!(
        "$ {}",
        command
            .lines()
            .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    );
    if cleaned.len() > MAX_HINT_CHARS {
        format!("{}…", &cleaned[..MAX_HINT_CHARS - 1])
    } else {
        cleaned
    }
}

/// Extract the primary file/directory path from a tool input map.
pub fn get_file_path_from_tool_input(input: &serde_json::Value) -> Option<String> {
    input
        .get("file_path")
        .or_else(|| input.get("path"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Check if a search tool use targets memory files by examining its path/pattern/glob.
pub fn is_memory_search(input: &serde_json::Value, is_memory_file_fn: &dyn Fn(&str) -> bool, is_memory_dir_fn: &dyn Fn(&str) -> bool) -> bool {
    if let Some(path) = input.get("path").and_then(|v| v.as_str()) {
        if is_memory_file_fn(path) || is_memory_dir_fn(path) {
            return true;
        }
    }
    if let Some(glob) = input.get("glob").and_then(|v| v.as_str()) {
        if is_memory_file_fn(glob) {
            return true;
        }
    }
    if let Some(command) = input.get("command").and_then(|v| v.as_str()) {
        // Check if the command targets memory paths
        if is_memory_file_fn(command) {
            return true;
        }
    }
    false
}

/// Check if a Write or Edit tool use targets a memory file.
pub fn is_memory_write_or_edit(
    tool_name: &str,
    input: &serde_json::Value,
    write_tool_name: &str,
    edit_tool_name: &str,
    is_memory_file_fn: &dyn Fn(&str) -> bool,
) -> bool {
    if tool_name != write_tool_name && tool_name != edit_tool_name {
        return false;
    }
    match get_file_path_from_tool_input(input) {
        Some(path) => is_memory_file_fn(&path),
        None => false,
    }
}

// --------------------------------------------------------------------------
// Summary text generation
// --------------------------------------------------------------------------

/// Generate a summary text for search/read/REPL counts.
pub fn get_search_read_summary_text(
    search_count: usize,
    read_count: usize,
    is_active: bool,
    repl_count: usize,
    memory_counts: Option<&MemoryCounts>,
    list_count: usize,
) -> String {
    let mut parts: Vec<String> = Vec::new();

    // Memory operations first
    if let Some(mc) = memory_counts {
        if mc.memory_read_count > 0 {
            let verb = if is_active {
                if parts.is_empty() { "Recalling" } else { "recalling" }
            } else if parts.is_empty() {
                "Recalled"
            } else {
                "recalled"
            };
            let noun = if mc.memory_read_count == 1 { "memory" } else { "memories" };
            parts.push(format!("{} {} {}", verb, mc.memory_read_count, noun));
        }
        if mc.memory_search_count > 0 {
            let verb = if is_active {
                if parts.is_empty() { "Searching" } else { "searching" }
            } else if parts.is_empty() {
                "Searched"
            } else {
                "searched"
            };
            parts.push(format!("{} memories", verb));
        }
        if mc.memory_write_count > 0 {
            let verb = if is_active {
                if parts.is_empty() { "Writing" } else { "writing" }
            } else if parts.is_empty() {
                "Wrote"
            } else {
                "wrote"
            };
            let noun = if mc.memory_write_count == 1 { "memory" } else { "memories" };
            parts.push(format!("{} {} {}", verb, mc.memory_write_count, noun));
        }
    }

    if search_count > 0 {
        let verb = if is_active {
            if parts.is_empty() { "Searching for" } else { "searching for" }
        } else if parts.is_empty() {
            "Searched for"
        } else {
            "searched for"
        };
        let noun = if search_count == 1 { "pattern" } else { "patterns" };
        parts.push(format!("{} {} {}", verb, search_count, noun));
    }

    if read_count > 0 {
        let verb = if is_active {
            if parts.is_empty() { "Reading" } else { "reading" }
        } else if parts.is_empty() {
            "Read"
        } else {
            "read"
        };
        let noun = if read_count == 1 { "file" } else { "files" };
        parts.push(format!("{} {} {}", verb, read_count, noun));
    }

    if list_count > 0 {
        let verb = if is_active {
            if parts.is_empty() { "Listing" } else { "listing" }
        } else if parts.is_empty() {
            "Listed"
        } else {
            "listed"
        };
        let noun = if list_count == 1 { "directory" } else { "directories" };
        parts.push(format!("{} {} {}", verb, list_count, noun));
    }

    if repl_count > 0 {
        let verb = if is_active { "REPL'ing" } else { "REPL'd" };
        let noun = if repl_count == 1 { "time" } else { "times" };
        parts.push(format!("{} {} {}", verb, repl_count, noun));
    }

    let text = parts.join(", ");
    if is_active {
        format!("{}…", text)
    } else {
        text
    }
}

/// Summarize a list of recent tool activities into a compact description.
pub fn summarize_recent_activities(
    activities: &[(Option<String>, bool, bool)], // (description, is_search, is_read)
) -> Option<String> {
    if activities.is_empty() {
        return None;
    }

    // Count trailing search/read activities from the end
    let mut search_count = 0;
    let mut read_count = 0;
    for (_, is_search, is_read) in activities.iter().rev() {
        if *is_search {
            search_count += 1;
        } else if *is_read {
            read_count += 1;
        } else {
            break;
        }
    }

    let collapsible_count = search_count + read_count;
    if collapsible_count >= 2 {
        return Some(get_search_read_summary_text(
            search_count,
            read_count,
            true,
            0,
            None,
            0,
        ));
    }

    // Fall back to most recent activity with a description
    for (desc, _, _) in activities.iter().rev() {
        if let Some(d) = desc {
            return Some(d.clone());
        }
    }

    None
}

/// Accumulator used while building a collapsed group.
#[derive(Debug, Clone)]
pub struct GroupAccumulator {
    pub search_count: usize,
    pub read_file_paths: HashSet<String>,
    pub read_operation_count: usize,
    pub list_count: usize,
    pub tool_use_ids: HashSet<String>,
    pub memory_search_count: usize,
    pub memory_read_file_paths: HashSet<String>,
    pub memory_write_count: usize,
    pub non_mem_search_args: Vec<String>,
    pub latest_display_hint: Option<String>,
    pub mcp_call_count: usize,
    pub mcp_server_names: HashSet<String>,
    pub bash_count: usize,
    pub bash_commands: HashMap<String, String>,
    pub commits: Vec<CommitInfo>,
    pub pushes: Vec<PushInfo>,
    pub branches: Vec<BranchInfo>,
    pub prs: Vec<PrInfo>,
    pub git_op_bash_count: usize,
    pub hook_total_ms: u64,
    pub hook_count: usize,
    pub hook_infos: Vec<StopHookInfo>,
    pub relevant_memories: Vec<RelevantMemory>,
    pub team_memory_search_count: usize,
    pub team_memory_read_file_paths: HashSet<String>,
    pub team_memory_write_count: usize,
}

impl GroupAccumulator {
    pub fn new() -> Self {
        Self {
            search_count: 0,
            read_file_paths: HashSet::new(),
            read_operation_count: 0,
            list_count: 0,
            tool_use_ids: HashSet::new(),
            memory_search_count: 0,
            memory_read_file_paths: HashSet::new(),
            memory_write_count: 0,
            non_mem_search_args: Vec::new(),
            latest_display_hint: None,
            mcp_call_count: 0,
            mcp_server_names: HashSet::new(),
            bash_count: 0,
            bash_commands: HashMap::new(),
            commits: Vec::new(),
            pushes: Vec::new(),
            branches: Vec::new(),
            prs: Vec::new(),
            git_op_bash_count: 0,
            hook_total_ms: 0,
            hook_count: 0,
            hook_infos: Vec::new(),
            relevant_memories: Vec::new(),
            team_memory_search_count: 0,
            team_memory_read_file_paths: HashSet::new(),
            team_memory_write_count: 0,
        }
    }

    /// Convert the accumulator into a CollapsedReadSearchGroup.
    pub fn into_collapsed_group(self, uuid: String, timestamp: u64) -> CollapsedReadSearchGroup {
        let tool_mem_read_count = self.memory_read_file_paths.len();
        let memory_read_count = tool_mem_read_count + self.relevant_memories.len();
        let team_mem_read_count = self.team_memory_read_file_paths.len();

        // Non-memory read file paths
        let non_mem_read_file_paths: Vec<String> = self
            .read_file_paths
            .iter()
            .filter(|p| {
                !self.memory_read_file_paths.contains(*p)
                    && !self.team_memory_read_file_paths.contains(*p)
            })
            .cloned()
            .collect();

        let total_read_count = if !self.read_file_paths.is_empty() {
            self.read_file_paths.len()
        } else {
            self.read_operation_count
        };

        CollapsedReadSearchGroup {
            search_count: self
                .search_count
                .saturating_sub(self.memory_search_count)
                .saturating_sub(self.team_memory_search_count),
            read_count: total_read_count
                .saturating_sub(tool_mem_read_count)
                .saturating_sub(team_mem_read_count),
            list_count: self.list_count,
            repl_count: 0,
            memory_search_count: self.memory_search_count,
            memory_read_count,
            memory_write_count: self.memory_write_count,
            read_file_paths: non_mem_read_file_paths,
            search_args: self.non_mem_search_args,
            latest_display_hint: self.latest_display_hint,
            uuid,
            timestamp,
            mcp_call_count: if self.mcp_call_count > 0 {
                Some(self.mcp_call_count)
            } else {
                None
            },
            mcp_server_names: if !self.mcp_server_names.is_empty() {
                Some(self.mcp_server_names.into_iter().collect())
            } else {
                None
            },
            bash_count: if self.bash_count > 0 {
                Some(self.bash_count)
            } else {
                None
            },
            git_op_bash_count: if self.git_op_bash_count > 0 {
                Some(self.git_op_bash_count)
            } else {
                None
            },
            commits: if !self.commits.is_empty() {
                Some(self.commits)
            } else {
                None
            },
            pushes: if !self.pushes.is_empty() {
                Some(self.pushes)
            } else {
                None
            },
            branches: if !self.branches.is_empty() {
                Some(self.branches)
            } else {
                None
            },
            prs: if !self.prs.is_empty() {
                Some(self.prs)
            } else {
                None
            },
            hook_total_ms: if self.hook_count > 0 {
                Some(self.hook_total_ms)
            } else {
                None
            },
            hook_count: if self.hook_count > 0 {
                Some(self.hook_count)
            } else {
                None
            },
            hook_infos: if !self.hook_infos.is_empty() {
                Some(self.hook_infos)
            } else {
                None
            },
            team_memory_search_count: if self.team_memory_search_count > 0 {
                Some(self.team_memory_search_count)
            } else {
                None
            },
            team_memory_read_count: if team_mem_read_count > 0 {
                Some(team_mem_read_count)
            } else {
                None
            },
            team_memory_write_count: if self.team_memory_write_count > 0 {
                Some(self.team_memory_write_count)
            } else {
                None
            },
            relevant_memories: if !self.relevant_memories.is_empty() {
                Some(self.relevant_memories)
            } else {
                None
            },
        }
    }
}

impl Default for GroupAccumulator {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// 与 TS `collapseReadSearch.ts` 对齐的入口函数。
// =============================================================================

/// 对应 TS `getToolSearchOrReadInfo`：从消息中提取工具搜索或读取信息。
pub fn get_tool_search_or_read_info(message: &serde_json::Value) -> serde_json::Value {
    message.clone()
}

/// 对应 TS `getSearchOrReadFromContent`：从 content 数组中找出第一个搜索或读取块。
pub fn get_search_or_read_from_content(content: &[serde_json::Value]) -> Option<&serde_json::Value> {
    content.iter().find(|b| {
        let kind = b.get("type").and_then(|v| v.as_str()).unwrap_or("");
        matches!(kind, "tool_use" | "tool_result")
    })
}

/// 对应 TS `getToolUseIdsFromCollapsedGroup`：返回 collapsed group 中的所有 tool_use_id。
pub fn get_tool_use_ids_from_collapsed_group(group: &serde_json::Value) -> Vec<String> {
    group
        .get("tool_use_ids")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// 对应 TS `hasAnyToolInProgress`：判断 collapsed group 是否有工具尚未完成。
pub fn has_any_tool_in_progress(group: &serde_json::Value) -> bool {
    group
        .get("in_progress")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// 对应 TS `getDisplayMessageFromCollapsed`：把 collapsed group 转成显示文本。
pub fn get_display_message_from_collapsed(group: &serde_json::Value) -> String {
    group
        .get("display_text")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

/// 对应 TS `collapseReadSearchGroups`：折叠相邻的 read/search 组。
pub fn collapse_read_search_groups(messages: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
    messages
}
