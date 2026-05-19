//! Context suggestions for optimizing token usage.
//!
//! Analyzes context data and generates suggestions to reduce token consumption.

/// Suggestion severity level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SuggestionSeverity {
    Warning,
    Info,
}

/// A context optimization suggestion.
#[derive(Debug, Clone)]
pub struct ContextSuggestion {
    pub severity: SuggestionSeverity,
    pub title: String,
    pub detail: String,
    pub savings_tokens: Option<usize>,
}

/// Thresholds for triggering suggestions.
const LARGE_TOOL_RESULT_PERCENT: f64 = 15.0;
const LARGE_TOOL_RESULT_TOKENS: usize = 10_000;
const READ_BLOAT_PERCENT: f64 = 5.0;
const NEAR_CAPACITY_PERCENT: f64 = 80.0;
const MEMORY_HIGH_PERCENT: f64 = 5.0;
const MEMORY_HIGH_TOKENS: usize = 5_000;

/// Tool call breakdown by type.
#[derive(Debug, Clone)]
pub struct ToolCallType {
    pub name: String,
    pub call_tokens: usize,
    pub result_tokens: usize,
}

/// Message breakdown data.
#[derive(Debug, Clone)]
pub struct MessageBreakdown {
    pub tool_calls_by_type: Vec<ToolCallType>,
}

/// Memory file info.
#[derive(Debug, Clone)]
pub struct MemoryFileInfo {
    pub path: String,
    pub tokens: usize,
}

/// Context data for analysis.
#[derive(Debug, Clone)]
pub struct ContextData {
    pub percentage: f64,
    pub raw_max_tokens: usize,
    pub is_auto_compact_enabled: bool,
    pub message_breakdown: Option<MessageBreakdown>,
    pub memory_files: Vec<MemoryFileInfo>,
}

/// Tool name constants.
const BASH_TOOL_NAME: &str = "Bash";
const FILE_READ_TOOL_NAME: &str = "Read";
const GREP_TOOL_NAME: &str = "Grep";
const WEB_FETCH_TOOL_NAME: &str = "WebFetch";

/// Generate context suggestions based on current data.
pub fn generate_context_suggestions(data: &ContextData) -> Vec<ContextSuggestion> {
    let mut suggestions = Vec::new();

    check_near_capacity(data, &mut suggestions);
    check_large_tool_results(data, &mut suggestions);
    check_read_result_bloat(data, &mut suggestions);
    check_memory_bloat(data, &mut suggestions);
    check_auto_compact_disabled(data, &mut suggestions);

    // Sort: warnings first, then by savings descending
    suggestions.sort_by(|a, b| {
        if a.severity != b.severity {
            return a.severity.cmp(&b.severity);
        }
        b.savings_tokens.unwrap_or(0).cmp(&a.savings_tokens.unwrap_or(0))
    });

    suggestions
}

fn check_near_capacity(data: &ContextData, suggestions: &mut Vec<ContextSuggestion>) {
    if data.percentage >= NEAR_CAPACITY_PERCENT {
        let detail = if data.is_auto_compact_enabled {
            "Autocompact will trigger soon, which discards older messages. Use /compact now to control what gets kept.".to_string()
        } else {
            "Autocompact is disabled. Use /compact to free space, or enable autocompact in /config.".to_string()
        };
        suggestions.push(ContextSuggestion {
            severity: SuggestionSeverity::Warning,
            title: format!("Context is {:.0}% full", data.percentage),
            detail,
            savings_tokens: None,
        });
    }
}

fn check_large_tool_results(data: &ContextData, suggestions: &mut Vec<ContextSuggestion>) {
    let breakdown = match &data.message_breakdown {
        Some(b) => b,
        None => return,
    };

    for tool in &breakdown.tool_calls_by_type {
        let total_tokens = tool.call_tokens + tool.result_tokens;
        let percent = (total_tokens as f64 / data.raw_max_tokens as f64) * 100.0;

        if percent < LARGE_TOOL_RESULT_PERCENT || total_tokens < LARGE_TOOL_RESULT_TOKENS {
            continue;
        }

        if let Some(suggestion) = get_large_tool_suggestion(&tool.name, total_tokens, percent) {
            suggestions.push(suggestion);
        }
    }
}

fn get_large_tool_suggestion(tool_name: &str, tokens: usize, percent: f64) -> Option<ContextSuggestion> {
    let token_str = format_tokens(tokens);

    match tool_name {
        BASH_TOOL_NAME => Some(ContextSuggestion {
            severity: SuggestionSeverity::Warning,
            title: format!("Bash results using {} tokens ({:.0}%)", token_str, percent),
            detail: "Pipe output through head, tail, or grep to reduce result size. Avoid cat on large files — use Read with offset/limit instead.".to_string(),
            savings_tokens: Some(tokens / 2),
        }),
        FILE_READ_TOOL_NAME => Some(ContextSuggestion {
            severity: SuggestionSeverity::Info,
            title: format!("Read results using {} tokens ({:.0}%)", token_str, percent),
            detail: "Use offset and limit parameters to read only the sections you need. Avoid re-reading entire files when you only need a few lines.".to_string(),
            savings_tokens: Some(tokens * 3 / 10),
        }),
        GREP_TOOL_NAME => Some(ContextSuggestion {
            severity: SuggestionSeverity::Info,
            title: format!("Grep results using {} tokens ({:.0}%)", token_str, percent),
            detail: "Add more specific patterns or use the glob or type parameter to narrow file types. Consider Glob for file discovery instead of Grep.".to_string(),
            savings_tokens: Some(tokens * 3 / 10),
        }),
        WEB_FETCH_TOOL_NAME => Some(ContextSuggestion {
            severity: SuggestionSeverity::Info,
            title: format!("WebFetch results using {} tokens ({:.0}%)", token_str, percent),
            detail: "Web page content can be very large. Consider extracting only the specific information needed.".to_string(),
            savings_tokens: Some(tokens * 4 / 10),
        }),
        _ => {
            if percent >= 20.0 {
                Some(ContextSuggestion {
                    severity: SuggestionSeverity::Info,
                    title: format!("{} using {} tokens ({:.0}%)", tool_name, token_str, percent),
                    detail: "This tool is consuming a significant portion of context.".to_string(),
                    savings_tokens: Some(tokens / 5),
                })
            } else {
                None
            }
        }
    }
}

fn check_read_result_bloat(data: &ContextData, suggestions: &mut Vec<ContextSuggestion>) {
    let breakdown = match &data.message_breakdown {
        Some(b) => b,
        None => return,
    };

    let read_tool = match breakdown.tool_calls_by_type.iter().find(|t| t.name == FILE_READ_TOOL_NAME) {
        Some(t) => t,
        None => return,
    };

    let total_read_tokens = read_tool.call_tokens + read_tool.result_tokens;
    let total_read_percent = (total_read_tokens as f64 / data.raw_max_tokens as f64) * 100.0;
    let read_percent = (read_tool.result_tokens as f64 / data.raw_max_tokens as f64) * 100.0;

    // Skip if already covered by large tool results
    if total_read_percent >= LARGE_TOOL_RESULT_PERCENT && total_read_tokens >= LARGE_TOOL_RESULT_TOKENS {
        return;
    }

    if read_percent >= READ_BLOAT_PERCENT && read_tool.result_tokens >= LARGE_TOOL_RESULT_TOKENS {
        suggestions.push(ContextSuggestion {
            severity: SuggestionSeverity::Info,
            title: format!(
                "File reads using {} tokens ({:.0}%)",
                format_tokens(read_tool.result_tokens),
                read_percent
            ),
            detail: "If you are re-reading files, consider referencing earlier reads. Use offset/limit for large files.".to_string(),
            savings_tokens: Some(read_tool.result_tokens * 3 / 10),
        });
    }
}

fn check_memory_bloat(data: &ContextData, suggestions: &mut Vec<ContextSuggestion>) {
    let total_memory_tokens: usize = data.memory_files.iter().map(|f| f.tokens).sum();
    let memory_percent = (total_memory_tokens as f64 / data.raw_max_tokens as f64) * 100.0;

    if memory_percent >= MEMORY_HIGH_PERCENT && total_memory_tokens >= MEMORY_HIGH_TOKENS {
        let mut sorted_files = data.memory_files.clone();
        sorted_files.sort_by(|a, b| b.tokens.cmp(&a.tokens));
        let largest_files: String = sorted_files
            .iter()
            .take(3)
            .map(|f| format!("{} ({})", get_display_path(&f.path), format_tokens(f.tokens)))
            .collect::<Vec<_>>()
            .join(", ");

        suggestions.push(ContextSuggestion {
            severity: SuggestionSeverity::Info,
            title: format!(
                "Memory files using {} tokens ({:.0}%)",
                format_tokens(total_memory_tokens),
                memory_percent
            ),
            detail: format!("Largest: {}. Use /memory to review and prune stale entries.", largest_files),
            savings_tokens: Some(total_memory_tokens * 3 / 10),
        });
    }
}

fn check_auto_compact_disabled(data: &ContextData, suggestions: &mut Vec<ContextSuggestion>) {
    if !data.is_auto_compact_enabled && data.percentage >= 50.0 && data.percentage < NEAR_CAPACITY_PERCENT {
        suggestions.push(ContextSuggestion {
            severity: SuggestionSeverity::Info,
            title: "Autocompact is disabled".to_string(),
            detail: "Without autocompact, you will hit context limits and lose the conversation. Enable it in /config or use /compact manually.".to_string(),
            savings_tokens: None,
        });
    }
}

/// Format token count for display.
fn format_tokens(tokens: usize) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}k", tokens as f64 / 1_000.0)
    } else {
        tokens.to_string()
    }
}

/// Get display path (basename or shortened).
fn get_display_path(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

use std::path::Path;
