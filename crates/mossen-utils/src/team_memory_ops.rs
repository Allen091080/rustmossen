//! # team_memory_ops — 团队记忆操作
//!
//! 对应 TypeScript `utils/teamMemoryOps.ts`。

/// 文件编辑工具名称常量
const FILE_EDIT_TOOL_NAME: &str = "file_edit";
/// 文件写入工具名称常量
const FILE_WRITE_TOOL_NAME: &str = "file_write";

/// 判断路径是否为团队记忆文件
pub fn is_team_mem_file(path: &str) -> bool {
    // 团队记忆文件位于 .mossen/team-memory/ 目录下
    path.contains(".mossen/team-memory") || path.contains(".mossen\\team-memory")
}

/// 检查搜索工具使用是否针对团队记忆文件（通过检查路径）
pub fn is_team_memory_search(tool_input: &serde_json::Value) -> bool {
    if let Some(path) = tool_input.get("path").and_then(|v| v.as_str()) {
        if is_team_mem_file(path) {
            return true;
        }
    }
    false
}

/// 检查写入或编辑工具是否针对团队记忆文件
pub fn is_team_memory_write_or_edit(tool_name: &str, tool_input: &serde_json::Value) -> bool {
    if tool_name != FILE_WRITE_TOOL_NAME && tool_name != FILE_EDIT_TOOL_NAME {
        return false;
    }
    let file_path = tool_input
        .get("file_path")
        .or_else(|| tool_input.get("path"))
        .and_then(|v| v.as_str());
    match file_path {
        Some(p) => is_team_mem_file(p),
        None => false,
    }
}

/// 团队记忆计数
pub struct MemoryCounts {
    pub team_memory_read_count: u32,
    pub team_memory_search_count: u32,
    pub team_memory_write_count: u32,
}

/// 将团队记忆摘要部分追加到 parts 数组中。
/// 封装所有团队记忆动词/字符串逻辑用于 getSearchReadSummaryText。
pub fn append_team_memory_summary_parts(
    memory_counts: &MemoryCounts,
    is_active: bool,
    parts: &mut Vec<String>,
) {
    let team_read_count = memory_counts.team_memory_read_count;
    let team_search_count = memory_counts.team_memory_search_count;
    let team_write_count = memory_counts.team_memory_write_count;

    if team_read_count > 0 {
        let verb = if is_active {
            if parts.is_empty() { "Recalling" } else { "recalling" }
        } else if parts.is_empty() {
            "Recalled"
        } else {
            "recalled"
        };
        let noun = if team_read_count == 1 {
            "memory"
        } else {
            "memories"
        };
        parts.push(format!("{} {} team {}", verb, team_read_count, noun));
    }

    if team_search_count > 0 {
        let verb = if is_active {
            if parts.is_empty() { "Searching" } else { "searching" }
        } else if parts.is_empty() {
            "Searched"
        } else {
            "searched"
        };
        parts.push(format!("{} team memories", verb));
    }

    if team_write_count > 0 {
        let verb = if is_active {
            if parts.is_empty() { "Writing" } else { "writing" }
        } else if parts.is_empty() {
            "Wrote"
        } else {
            "wrote"
        };
        let noun = if team_write_count == 1 {
            "memory"
        } else {
            "memories"
        };
        parts.push(format!("{} {} team {}", verb, team_write_count, noun));
    }
}
