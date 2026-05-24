//! Doctor context warnings — checks for oversized MOSSEN.md files, agent descriptions,
//! MCP tools context, and unreachable permission rules.

use std::collections::HashMap;

/// Warning severity level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarningSeverity {
    Warning,
    Error,
}

/// Type of context warning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarningType {
    MossenMdFiles,
    AgentDescriptions,
    McpTools,
    UnreachableRules,
}

/// A context warning detected by the doctor command.
#[derive(Debug, Clone)]
pub struct ContextWarning {
    pub warning_type: WarningType,
    pub severity: WarningSeverity,
    pub message: String,
    pub details: Vec<String>,
    pub current_value: usize,
    pub threshold: usize,
}

/// All context warnings from a doctor check.
#[derive(Debug, Clone, Default)]
pub struct ContextWarnings {
    pub mossen_md_warning: Option<ContextWarning>,
    pub agent_warning: Option<ContextWarning>,
    pub mcp_warning: Option<ContextWarning>,
    pub unreachable_rules_warning: Option<ContextWarning>,
}

/// Thresholds for context warnings.
pub const MCP_TOOLS_THRESHOLD: usize = 25_000;
pub const MAX_MEMORY_CHARACTER_COUNT: usize = 40_000;
pub const AGENT_DESCRIPTIONS_THRESHOLD: usize = 15_000;

/// A memory file with path and content.
#[derive(Debug, Clone)]
pub struct MemoryFile {
    pub path: String,
    pub content: String,
}

/// Information about an agent definition.
#[derive(Debug, Clone)]
pub struct AgentInfo {
    pub agent_type: String,
    pub when_to_use: String,
    pub source: String,
}

/// Result of loading agent definitions.
#[derive(Debug, Clone)]
pub struct AgentDefinitionsResult {
    pub active_agents: Vec<AgentInfo>,
}

/// Tool definition for MCP tools checking.
#[derive(Debug, Clone)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub is_mcp: bool,
}

/// Details about a counted MCP tool.
#[derive(Debug, Clone)]
pub struct McpToolDetail {
    pub name: String,
    pub tokens: usize,
}

/// Result from counting MCP tool tokens.
#[derive(Debug, Clone)]
pub struct McpToolTokensResult {
    pub mcp_tool_tokens: usize,
    pub mcp_tool_details: Vec<McpToolDetail>,
}

/// Unreachable permission rule detection result.
#[derive(Debug, Clone)]
pub struct UnreachableRule {
    pub rule_value: String,
    pub reason: String,
    pub fix: String,
}

/// Rough token count estimation (approximately 4 characters per token).
pub fn rough_token_count_estimation(text: &str) -> usize {
    (text.len() + 3) / 4
}

/// Pluralize a word based on count.
fn plural(count: usize, word: &str) -> String {
    if count == 1 {
        word.to_string()
    } else {
        format!("{}s", word)
    }
}

/// Format a number with locale-style thousand separators.
fn format_number(n: usize) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

/// Check for oversized MOSSEN.md files.
pub fn check_mossen_md_files(memory_files: &[MemoryFile]) -> Option<ContextWarning> {
    let large_files: Vec<&MemoryFile> = memory_files
        .iter()
        .filter(|f| f.content.len() > MAX_MEMORY_CHARACTER_COUNT)
        .collect();

    if large_files.is_empty() {
        return None;
    }

    let mut sorted_files = large_files;
    sorted_files.sort_by(|a, b| b.content.len().cmp(&a.content.len()));

    let details: Vec<String> = sorted_files
        .iter()
        .map(|f| format!("{}: {} chars", f.path, format_number(f.content.len())))
        .collect();

    let message = if sorted_files.len() == 1 {
        format!(
            "Large MOSSEN.md file detected ({} chars > {})",
            format_number(sorted_files[0].content.len()),
            format_number(MAX_MEMORY_CHARACTER_COUNT)
        )
    } else {
        format!(
            "{} large MOSSEN.md files detected (each > {} chars)",
            sorted_files.len(),
            format_number(MAX_MEMORY_CHARACTER_COUNT)
        )
    };

    Some(ContextWarning {
        warning_type: WarningType::MossenMdFiles,
        severity: WarningSeverity::Warning,
        message,
        details,
        current_value: sorted_files.len(),
        threshold: MAX_MEMORY_CHARACTER_COUNT,
    })
}

/// Check agent descriptions for excessive token usage.
pub fn check_agent_descriptions(
    agent_info: Option<&AgentDefinitionsResult>,
) -> Option<ContextWarning> {
    let info = agent_info?;

    let agent_tokens: Vec<(String, usize)> = info
        .active_agents
        .iter()
        .filter(|a| a.source != "built-in")
        .map(|agent| {
            let description = format!("{}: {}", agent.agent_type, agent.when_to_use);
            (
                agent.agent_type.clone(),
                rough_token_count_estimation(&description),
            )
        })
        .collect();

    let total_tokens: usize = agent_tokens.iter().map(|(_, t)| *t).sum();

    if total_tokens <= AGENT_DESCRIPTIONS_THRESHOLD {
        return None;
    }

    let mut sorted_agents = agent_tokens;
    sorted_agents.sort_by(|a, b| b.1.cmp(&a.1));

    let mut details: Vec<String> = sorted_agents
        .iter()
        .take(5)
        .map(|(name, tokens)| format!("{}: ~{} tokens", name, format_number(*tokens)))
        .collect();

    if sorted_agents.len() > 5 {
        details.push(format!("({} more custom agents)", sorted_agents.len() - 5));
    }

    Some(ContextWarning {
        warning_type: WarningType::AgentDescriptions,
        severity: WarningSeverity::Warning,
        message: format!(
            "Large agent descriptions (~{} tokens > {})",
            format_number(total_tokens),
            format_number(AGENT_DESCRIPTIONS_THRESHOLD)
        ),
        details,
        current_value: total_tokens,
        threshold: AGENT_DESCRIPTIONS_THRESHOLD,
    })
}

/// Check MCP tools for excessive context token usage.
pub fn check_mcp_tools(
    mcp_tool_result: Option<&McpToolTokensResult>,
    tools: &[ToolDef],
) -> Option<ContextWarning> {
    let mcp_tools: Vec<&ToolDef> = tools.iter().filter(|t| t.is_mcp).collect();

    if mcp_tools.is_empty() {
        return None;
    }

    if let Some(result) = mcp_tool_result {
        if result.mcp_tool_tokens <= MCP_TOOLS_THRESHOLD {
            return None;
        }

        // Group tools by server
        let mut tools_by_server: HashMap<String, (usize, usize)> = HashMap::new();
        for tool in &result.mcp_tool_details {
            let parts: Vec<&str> = tool.name.split("__").collect();
            let server_name = parts.get(1).unwrap_or(&"unknown").to_string();
            let entry = tools_by_server.entry(server_name).or_insert((0, 0));
            entry.0 += 1;
            entry.1 += tool.tokens;
        }

        let mut sorted_servers: Vec<(String, (usize, usize))> =
            tools_by_server.into_iter().collect();
        sorted_servers.sort_by(|a, b| b.1 .1.cmp(&a.1 .1));

        let mut details: Vec<String> = sorted_servers
            .iter()
            .take(5)
            .map(|(name, (count, tokens))| {
                format!(
                    "{}: {} tools (~{} tokens)",
                    name,
                    count,
                    format_number(*tokens)
                )
            })
            .collect();

        if sorted_servers.len() > 5 {
            details.push(format!("({} more servers)", sorted_servers.len() - 5));
        }

        return Some(ContextWarning {
            warning_type: WarningType::McpTools,
            severity: WarningSeverity::Warning,
            message: format!(
                "Large MCP tools context (~{} tokens > {})",
                format_number(result.mcp_tool_tokens),
                format_number(MCP_TOOLS_THRESHOLD)
            ),
            details,
            current_value: result.mcp_tool_tokens,
            threshold: MCP_TOOLS_THRESHOLD,
        });
    }

    // Fallback: character-based estimation
    let estimated_tokens: usize = mcp_tools
        .iter()
        .map(|tool| {
            let chars = tool.name.len() + tool.description.len();
            rough_token_count_estimation(&chars.to_string())
        })
        .sum();

    if estimated_tokens <= MCP_TOOLS_THRESHOLD {
        return None;
    }

    Some(ContextWarning {
        warning_type: WarningType::McpTools,
        severity: WarningSeverity::Warning,
        message: format!(
            "Large MCP tools context (~{} tokens estimated > {})",
            format_number(estimated_tokens),
            format_number(MCP_TOOLS_THRESHOLD)
        ),
        details: vec![format!(
            "{} MCP tools detected (token count estimated)",
            mcp_tools.len()
        )],
        current_value: estimated_tokens,
        threshold: MCP_TOOLS_THRESHOLD,
    })
}

/// Check for unreachable permission rules.
pub fn check_unreachable_rules(unreachable: &[UnreachableRule]) -> Option<ContextWarning> {
    if unreachable.is_empty() {
        return None;
    }

    let details: Vec<String> = unreachable
        .iter()
        .flat_map(|r| {
            vec![
                format!("{}: {}", r.rule_value, r.reason),
                format!("  Fix: {}", r.fix),
            ]
        })
        .collect();

    Some(ContextWarning {
        warning_type: WarningType::UnreachableRules,
        severity: WarningSeverity::Warning,
        message: format!(
            "{} {} detected",
            unreachable.len(),
            plural(unreachable.len(), "unreachable permission rule")
        ),
        details,
        current_value: unreachable.len(),
        threshold: 0,
    })
}

/// Check all context warnings for the doctor command.
///
/// Runs all checks and returns a combined result.
pub fn check_context_warnings(
    memory_files: &[MemoryFile],
    agent_info: Option<&AgentDefinitionsResult>,
    mcp_tool_result: Option<&McpToolTokensResult>,
    tools: &[ToolDef],
    unreachable_rules: &[UnreachableRule],
) -> ContextWarnings {
    let mossen_md_warning = check_mossen_md_files(memory_files);
    let agent_warning = check_agent_descriptions(agent_info);
    let mcp_warning = check_mcp_tools(mcp_tool_result, tools);
    let unreachable_rules_warning = check_unreachable_rules(unreachable_rules);

    ContextWarnings {
        mossen_md_warning,
        agent_warning,
        mcp_warning,
        unreachable_rules_warning,
    }
}
