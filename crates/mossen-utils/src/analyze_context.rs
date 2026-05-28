//! Context window analysis utilities.
//!
//! Provides token counting, context category breakdown, grid visualization,
//! and budget enforcement for LLM context windows.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::string_utils::truncate_chars;

// --------------------------------------------------------------------------
// Constants
// --------------------------------------------------------------------------

/// Fixed token overhead added by the API when tools are present.
pub const TOOL_TOKEN_COUNT_OVERHEAD: usize = 500;

const RESERVED_CATEGORY_NAME: &str = "Autocompact buffer";
const MANUAL_COMPACT_BUFFER_NAME: &str = "Compact buffer";

// --------------------------------------------------------------------------
// Types
// --------------------------------------------------------------------------

/// A category of tokens in the context window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextCategory {
    pub name: String,
    pub tokens: usize,
    pub color: String,
    /// When true, these tokens are deferred and don't count toward context usage.
    #[serde(default)]
    pub is_deferred: bool,
}

/// A single square in the context grid visualization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GridSquare {
    pub color: String,
    pub is_filled: bool,
    pub category_name: String,
    pub tokens: usize,
    pub percentage: usize,
    /// 0.0–1.0 representing how full this individual square is.
    pub square_fullness: f64,
}

/// Information about a memory file in the context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryFile {
    pub path: String,
    #[serde(rename = "type")]
    pub file_type: String,
    pub tokens: usize,
}

/// Information about an MCP tool in the context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub server_name: String,
    pub tokens: usize,
    #[serde(default)]
    pub is_loaded: bool,
}

/// Per-tool breakdown of deferred built-in tools.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeferredBuiltinTool {
    pub name: String,
    pub tokens: usize,
    pub is_loaded: bool,
}

/// Per-tool breakdown of always-loaded built-in tools.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemToolDetail {
    pub name: String,
    pub tokens: usize,
}

/// Per-section breakdown of system prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemPromptSectionDetail {
    pub name: String,
    pub tokens: usize,
}

/// Agent definition info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub agent_type: String,
    pub source: String,
    pub tokens: usize,
}

/// Slash command information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlashCommandInfo {
    pub total_commands: usize,
    pub included_commands: usize,
    pub tokens: usize,
}

/// Individual skill detail for context display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillFrontmatter {
    pub name: String,
    pub source: String,
    pub tokens: usize,
}

/// Information about skills included in the context window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInfo {
    pub total_skills: usize,
    pub included_skills: usize,
    pub tokens: usize,
    pub skill_frontmatter: Vec<SkillFrontmatter>,
}

/// Recent compaction info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentCompact {
    pub has_boundary: bool,
    pub messages_since_compact: usize,
}

/// Tool calls breakdown entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallBreakdown {
    pub name: String,
    pub call_tokens: usize,
    pub result_tokens: usize,
}

/// Attachment breakdown entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentBreakdown {
    pub name: String,
    pub tokens: usize,
}

/// Message breakdown details.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageBreakdown {
    pub tool_call_tokens: usize,
    pub tool_result_tokens: usize,
    pub attachment_tokens: usize,
    pub assistant_message_tokens: usize,
    pub user_message_tokens: usize,
    pub tool_calls_by_type: Vec<ToolCallBreakdown>,
    pub attachments_by_type: Vec<AttachmentBreakdown>,
}

/// API usage information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiUsage {
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub cache_creation_input_tokens: usize,
    pub cache_read_input_tokens: usize,
}

/// Full context data returned by analyzeContextUsage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextData {
    pub categories: Vec<ContextCategory>,
    pub total_tokens: usize,
    pub max_tokens: usize,
    pub raw_max_tokens: usize,
    pub percentage: usize,
    pub grid_rows: Vec<Vec<GridSquare>>,
    pub model: String,
    pub memory_files: Vec<MemoryFile>,
    pub mcp_tools: Vec<McpTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deferred_builtin_tools: Option<Vec<DeferredBuiltinTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_tools: Option<Vec<SystemToolDetail>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt_sections: Option<Vec<SystemPromptSectionDetail>>,
    pub agents: Vec<Agent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slash_commands: Option<SlashCommandInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skills: Option<SkillInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_compact_threshold: Option<usize>,
    pub is_auto_compact_enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recent_compact: Option<RecentCompact>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_breakdown: Option<MessageBreakdown>,
    pub api_usage: Option<ApiUsage>,
}

// --------------------------------------------------------------------------
// Helper functions
// --------------------------------------------------------------------------

/// Extract a human-readable name from a system prompt section's content.
pub fn extract_section_name(content: &str) -> String {
    // Try to find first markdown heading
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            let name = trimmed.trim_start_matches('#').trim();
            if !name.is_empty() {
                return name.to_string();
            }
        }
    }
    // Fall back to a truncated preview of the first non-empty line
    let first_line = content.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    truncate_chars(first_line, 40)
}

/// Rough token count estimation based on character count (4 chars ≈ 1 token).
pub fn rough_token_count_estimation(text: &str) -> usize {
    // Approximation: ~4 bytes per token for English text
    text.len().div_ceil(4)
}

/// Compute grid layout parameters based on context window size and terminal width.
pub fn compute_grid_params(context_window: usize, terminal_width: Option<usize>) -> (usize, usize) {
    let is_narrow_screen = terminal_width.is_some_and(|w| w < 80);
    let grid_width = if context_window >= 1_000_000 {
        if is_narrow_screen {
            5
        } else {
            20
        }
    } else if is_narrow_screen {
        5
    } else {
        10
    };
    let grid_height = if context_window >= 1_000_000 {
        10
    } else if is_narrow_screen {
        5
    } else {
        10
    };
    (grid_width, grid_height)
}

/// Internal struct for computing grid squares.
struct CategorySquareInfo {
    name: String,
    tokens: usize,
    color: String,
    squares: usize,
    percentage_of_total: usize,
    is_deferred: bool,
}

/// Build the grid visualization from categories.
pub fn build_grid(
    categories: &[ContextCategory],
    context_window: usize,
    terminal_width: Option<usize>,
) -> Vec<Vec<GridSquare>> {
    let (grid_width, grid_height) = compute_grid_params(context_window, terminal_width);
    let total_squares = grid_width * grid_height;

    // Filter out deferred categories for grid layout
    let non_deferred_cats: Vec<&ContextCategory> =
        categories.iter().filter(|c| !c.is_deferred).collect();

    // Calculate squares per category
    let category_squares: Vec<CategorySquareInfo> = non_deferred_cats
        .iter()
        .map(|cat| {
            let squares = if cat.name == "Free space" {
                ((cat.tokens as f64 / context_window as f64) * total_squares as f64).round()
                    as usize
            } else {
                ((cat.tokens as f64 / context_window as f64) * total_squares as f64)
                    .round()
                    .max(1.0) as usize
            };
            CategorySquareInfo {
                name: cat.name.clone(),
                tokens: cat.tokens,
                color: cat.color.clone(),
                squares,
                percentage_of_total: ((cat.tokens as f64 / context_window as f64) * 100.0).round()
                    as usize,
                is_deferred: cat.is_deferred,
            }
        })
        .collect();

    // Separate reserved category for end placement
    let reserved_category = category_squares
        .iter()
        .find(|c| c.name == RESERVED_CATEGORY_NAME || c.name == MANUAL_COMPACT_BUFFER_NAME);
    let non_reserved_categories: Vec<&CategorySquareInfo> = category_squares
        .iter()
        .filter(|c| {
            c.name != RESERVED_CATEGORY_NAME
                && c.name != MANUAL_COMPACT_BUFFER_NAME
                && c.name != "Free space"
        })
        .collect();

    let mut grid_squares: Vec<GridSquare> = Vec::with_capacity(total_squares);

    // Add all non-reserved, non-free-space squares first
    for cat in &non_reserved_categories {
        let exact_squares = (cat.tokens as f64 / context_window as f64) * total_squares as f64;
        let whole_squares = exact_squares.floor() as usize;
        let fractional_part = exact_squares - exact_squares.floor();

        for i in 0..cat.squares {
            if grid_squares.len() >= total_squares {
                break;
            }
            let square_fullness = if i == whole_squares && fractional_part > 0.0 {
                fractional_part
            } else {
                1.0
            };
            grid_squares.push(GridSquare {
                color: cat.color.clone(),
                is_filled: true,
                category_name: cat.name.clone(),
                tokens: cat.tokens,
                percentage: cat.percentage_of_total,
                square_fullness,
            });
        }
    }

    // Calculate how many squares are needed for reserved
    let reserved_square_count = reserved_category.map_or(0, |c| c.squares);

    // Fill with free space, leaving room for reserved at the end
    let free_space_cat = category_squares.iter().find(|c| c.name == "Free space");
    let free_space_target = total_squares.saturating_sub(reserved_square_count);

    while grid_squares.len() < free_space_target {
        grid_squares.push(GridSquare {
            color: "promptBorder".to_string(),
            is_filled: true,
            category_name: "Free space".to_string(),
            tokens: free_space_cat.map_or(0, |c| c.tokens),
            percentage: free_space_cat.map_or(0, |c| c.percentage_of_total),
            square_fullness: 1.0,
        });
    }

    // Add reserved squares at the end
    if let Some(reserved) = reserved_category {
        let exact_squares = (reserved.tokens as f64 / context_window as f64) * total_squares as f64;
        let whole_squares = exact_squares.floor() as usize;
        let fractional_part = exact_squares - exact_squares.floor();

        for i in 0..reserved.squares {
            if grid_squares.len() >= total_squares {
                break;
            }
            let square_fullness = if i == whole_squares && fractional_part > 0.0 {
                fractional_part
            } else {
                1.0
            };
            grid_squares.push(GridSquare {
                color: reserved.color.clone(),
                is_filled: true,
                category_name: reserved.name.clone(),
                tokens: reserved.tokens,
                percentage: reserved.percentage_of_total,
                square_fullness,
            });
        }
    }

    // Convert to rows for rendering
    let mut grid_rows: Vec<Vec<GridSquare>> = Vec::with_capacity(grid_height);
    for i in 0..grid_height {
        let start = i * grid_width;
        let end = ((i + 1) * grid_width).min(grid_squares.len());
        if start < grid_squares.len() {
            grid_rows.push(grid_squares[start..end].to_vec());
        }
    }
    grid_rows
}

// --------------------------------------------------------------------------
// Message breakdown helpers
// --------------------------------------------------------------------------

/// Internal accumulator for message breakdown.
#[derive(Debug, Default)]
pub struct MessageBreakdownAccumulator {
    pub total_tokens: usize,
    pub tool_call_tokens: usize,
    pub tool_result_tokens: usize,
    pub attachment_tokens: usize,
    pub assistant_message_tokens: usize,
    pub user_message_tokens: usize,
    pub tool_calls_by_type: HashMap<String, usize>,
    pub tool_results_by_type: HashMap<String, usize>,
    pub attachments_by_type: HashMap<String, usize>,
}

impl MessageBreakdownAccumulator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add tool call tokens for a given tool name.
    pub fn add_tool_call(&mut self, tool_name: &str, tokens: usize) {
        self.tool_call_tokens += tokens;
        *self
            .tool_calls_by_type
            .entry(tool_name.to_string())
            .or_insert(0) += tokens;
    }

    /// Add tool result tokens for a given tool name.
    pub fn add_tool_result(&mut self, tool_name: &str, tokens: usize) {
        self.tool_result_tokens += tokens;
        *self
            .tool_results_by_type
            .entry(tool_name.to_string())
            .or_insert(0) += tokens;
    }

    /// Add attachment tokens for a given attachment type.
    pub fn add_attachment(&mut self, attachment_type: &str, tokens: usize) {
        self.attachment_tokens += tokens;
        *self
            .attachments_by_type
            .entry(attachment_type.to_string())
            .or_insert(0) += tokens;
    }

    /// Convert to the final MessageBreakdown structure.
    pub fn into_breakdown(self) -> MessageBreakdown {
        // Merge tool calls and results into combined entries
        let mut tools_map: HashMap<String, (usize, usize)> = HashMap::new();
        for (name, tokens) in &self.tool_calls_by_type {
            tools_map.entry(name.clone()).or_insert((0, 0)).0 += tokens;
        }
        for (name, tokens) in &self.tool_results_by_type {
            tools_map.entry(name.clone()).or_insert((0, 0)).1 += tokens;
        }

        let mut tool_calls_by_type: Vec<ToolCallBreakdown> = tools_map
            .into_iter()
            .map(|(name, (call_tokens, result_tokens))| ToolCallBreakdown {
                name,
                call_tokens,
                result_tokens,
            })
            .collect();
        tool_calls_by_type.sort_by(|a, b| {
            (b.call_tokens + b.result_tokens).cmp(&(a.call_tokens + a.result_tokens))
        });

        let mut attachments_by_type: Vec<AttachmentBreakdown> = self
            .attachments_by_type
            .into_iter()
            .map(|(name, tokens)| AttachmentBreakdown { name, tokens })
            .collect();
        attachments_by_type.sort_by(|a, b| b.tokens.cmp(&a.tokens));

        MessageBreakdown {
            tool_call_tokens: self.tool_call_tokens,
            tool_result_tokens: self.tool_result_tokens,
            attachment_tokens: self.attachment_tokens,
            assistant_message_tokens: self.assistant_message_tokens,
            user_message_tokens: self.user_message_tokens,
            tool_calls_by_type,
            attachments_by_type,
        }
    }
}

// --------------------------------------------------------------------------
// Context analysis builder
// --------------------------------------------------------------------------

/// Builder for constructing ContextData.
pub struct ContextAnalysisBuilder {
    categories: Vec<ContextCategory>,
    context_window: usize,
    model: String,
    memory_files: Vec<MemoryFile>,
    mcp_tools: Vec<McpTool>,
    deferred_builtin_tools: Option<Vec<DeferredBuiltinTool>>,
    system_tools: Option<Vec<SystemToolDetail>>,
    system_prompt_sections: Option<Vec<SystemPromptSectionDetail>>,
    agents: Vec<Agent>,
    slash_commands: Option<SlashCommandInfo>,
    skills: Option<SkillInfo>,
    auto_compact_threshold: Option<usize>,
    is_auto_compact_enabled: bool,
    recent_compact: Option<RecentCompact>,
    message_breakdown: Option<MessageBreakdown>,
    api_usage: Option<ApiUsage>,
    terminal_width: Option<usize>,
}

impl ContextAnalysisBuilder {
    pub fn new(context_window: usize, model: String) -> Self {
        Self {
            categories: Vec::new(),
            context_window,
            model,
            memory_files: Vec::new(),
            mcp_tools: Vec::new(),
            deferred_builtin_tools: None,
            system_tools: None,
            system_prompt_sections: None,
            agents: Vec::new(),
            slash_commands: None,
            skills: None,
            auto_compact_threshold: None,
            is_auto_compact_enabled: false,
            recent_compact: None,
            message_breakdown: None,
            api_usage: None,
            terminal_width: None,
        }
    }

    pub fn add_category(&mut self, name: &str, tokens: usize, color: &str) -> &mut Self {
        self.categories.push(ContextCategory {
            name: name.to_string(),
            tokens,
            color: color.to_string(),
            is_deferred: false,
        });
        self
    }

    pub fn add_deferred_category(&mut self, name: &str, tokens: usize, color: &str) -> &mut Self {
        self.categories.push(ContextCategory {
            name: name.to_string(),
            tokens,
            color: color.to_string(),
            is_deferred: true,
        });
        self
    }

    pub fn set_memory_files(&mut self, files: Vec<MemoryFile>) -> &mut Self {
        self.memory_files = files;
        self
    }

    pub fn set_mcp_tools(&mut self, tools: Vec<McpTool>) -> &mut Self {
        self.mcp_tools = tools;
        self
    }

    pub fn set_deferred_builtin_tools(&mut self, tools: Vec<DeferredBuiltinTool>) -> &mut Self {
        self.deferred_builtin_tools = Some(tools);
        self
    }

    pub fn set_system_tools(&mut self, tools: Vec<SystemToolDetail>) -> &mut Self {
        self.system_tools = Some(tools);
        self
    }

    pub fn set_system_prompt_sections(
        &mut self,
        sections: Vec<SystemPromptSectionDetail>,
    ) -> &mut Self {
        self.system_prompt_sections = Some(sections);
        self
    }

    pub fn set_agents(&mut self, agents: Vec<Agent>) -> &mut Self {
        self.agents = agents;
        self
    }

    pub fn set_slash_commands(&mut self, info: SlashCommandInfo) -> &mut Self {
        self.slash_commands = Some(info);
        self
    }

    pub fn set_skills(&mut self, info: SkillInfo) -> &mut Self {
        self.skills = Some(info);
        self
    }

    pub fn set_auto_compact(&mut self, threshold: usize) -> &mut Self {
        self.auto_compact_threshold = Some(threshold);
        self.is_auto_compact_enabled = true;
        self
    }

    pub fn set_recent_compact(&mut self, info: RecentCompact) -> &mut Self {
        self.recent_compact = Some(info);
        self
    }

    pub fn set_message_breakdown(&mut self, breakdown: MessageBreakdown) -> &mut Self {
        self.message_breakdown = Some(breakdown);
        self
    }

    pub fn set_api_usage(&mut self, usage: ApiUsage) -> &mut Self {
        self.api_usage = Some(usage);
        self
    }

    pub fn set_terminal_width(&mut self, width: usize) -> &mut Self {
        self.terminal_width = Some(width);
        self
    }

    /// Build the final ContextData.
    pub fn build(mut self) -> ContextData {
        // Calculate actual usage (excluding deferred categories)
        let actual_usage: usize = self
            .categories
            .iter()
            .filter(|c| !c.is_deferred)
            .map(|c| c.tokens)
            .sum();

        // Reserved space
        let reserved_tokens = if self.is_auto_compact_enabled {
            self.auto_compact_threshold
                .map(|threshold| self.context_window.saturating_sub(threshold))
                .unwrap_or(0)
        } else {
            3000 // MANUAL_COMPACT_BUFFER_TOKENS
        };

        if reserved_tokens > 0 {
            let name = if self.is_auto_compact_enabled {
                RESERVED_CATEGORY_NAME
            } else {
                MANUAL_COMPACT_BUFFER_NAME
            };
            self.categories.push(ContextCategory {
                name: name.to_string(),
                tokens: reserved_tokens,
                color: "inactive".to_string(),
                is_deferred: false,
            });
        }

        // Free space
        let free_tokens = self
            .context_window
            .saturating_sub(actual_usage)
            .saturating_sub(reserved_tokens);
        self.categories.push(ContextCategory {
            name: "Free space".to_string(),
            tokens: free_tokens,
            color: "promptBorder".to_string(),
            is_deferred: false,
        });

        // Determine total tokens (use API if available)
        let total_from_api = self
            .api_usage
            .as_ref()
            .map(|u| u.input_tokens + u.cache_creation_input_tokens + u.cache_read_input_tokens);
        let final_total_tokens = total_from_api.unwrap_or(actual_usage);

        let percentage = if self.context_window > 0 {
            ((final_total_tokens as f64 / self.context_window as f64) * 100.0).round() as usize
        } else {
            0
        };

        // Build grid
        let grid_rows = build_grid(&self.categories, self.context_window, self.terminal_width);

        ContextData {
            categories: self.categories,
            total_tokens: final_total_tokens,
            max_tokens: self.context_window,
            raw_max_tokens: self.context_window,
            percentage,
            grid_rows,
            model: self.model,
            memory_files: self.memory_files,
            mcp_tools: self.mcp_tools,
            deferred_builtin_tools: self.deferred_builtin_tools,
            system_tools: self.system_tools,
            system_prompt_sections: self.system_prompt_sections,
            agents: self.agents,
            slash_commands: self.slash_commands,
            skills: self.skills,
            auto_compact_threshold: self.auto_compact_threshold,
            is_auto_compact_enabled: self.is_auto_compact_enabled,
            recent_compact: self.recent_compact,
            message_breakdown: self.message_breakdown,
            api_usage: self.api_usage,
        }
    }
}

/// Count tool definition tokens by summing rough estimates of each tool's schema.
pub fn count_tool_definition_tokens(tool_schemas: &[String]) -> usize {
    tool_schemas
        .iter()
        .map(|schema| rough_token_count_estimation(schema))
        .sum()
}

/// 估算单个 MCP 工具集合的 token 用量（对应 TS `countMcpToolTokens`）。
///
/// `tool_descriptions` 是按工具收集的 JSON schema/描述文本片段。返回总 token 估算值。
pub async fn count_mcp_tool_tokens(tool_descriptions: &[String]) -> usize {
    tool_descriptions
        .iter()
        .map(|d| rough_token_count_estimation(d))
        .sum()
}

/// 对话上下文使用分析（对应 TS `analyzeContextUsage`）。
#[derive(Debug, Clone, Default)]
pub struct ContextUsageAnalysis {
    pub total_tokens: usize,
    pub system_prompt_tokens: usize,
    pub messages_tokens: usize,
    pub tool_definition_tokens: usize,
}

/// 分析上下文使用情况。输入为系统 prompt、消息列表（按 JSON 值形式提供）与
/// 工具 schema 列表；返回各部分的 token 估算占比汇总。
pub async fn analyze_context_usage(
    system_prompt: &str,
    messages: &[serde_json::Value],
    tool_schemas: &[String],
) -> ContextUsageAnalysis {
    let system_prompt_tokens = rough_token_count_estimation(system_prompt);
    let messages_tokens: usize = messages
        .iter()
        .map(|m| rough_token_count_estimation(&m.to_string()))
        .sum();
    let tool_definition_tokens = count_tool_definition_tokens(tool_schemas);
    ContextUsageAnalysis {
        total_tokens: system_prompt_tokens + messages_tokens + tool_definition_tokens,
        system_prompt_tokens,
        messages_tokens,
        tool_definition_tokens,
    }
}
