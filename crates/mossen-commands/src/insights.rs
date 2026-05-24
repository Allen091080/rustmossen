//! `/insights` — Usage analytics and insights report (local).
//!
//! Generates a comprehensive usage report by analyzing session data,
//! extracting facets via API, and producing an HTML report.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Insights directive — generate detailed usage analytics and session insights.
pub struct InsightsDirective;

// ============================================================================
// Types
// ============================================================================

/// Remote host information for homespace data collection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteHostInfo {
    pub name: String,
    pub session_count: usize,
}

/// Metadata extracted from a single session log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub session_id: String,
    pub project_path: String,
    pub start_time: String,
    pub duration_minutes: f64,
    pub user_message_count: usize,
    pub assistant_message_count: usize,
    pub tool_counts: HashMap<String, usize>,
    pub languages: HashMap<String, usize>,
    pub git_commits: usize,
    pub git_pushes: usize,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub first_prompt: String,
    pub summary: Option<String>,
    pub user_interruptions: usize,
    pub user_response_times: Vec<f64>,
    pub tool_errors: usize,
    pub tool_error_categories: HashMap<String, usize>,
    pub uses_task_agent: bool,
    pub uses_mcp: bool,
    pub uses_web_search: bool,
    pub uses_web_fetch: bool,
    pub lines_added: usize,
    pub lines_removed: usize,
    pub files_modified: usize,
    pub message_hours: Vec<u32>,
    pub user_message_timestamps: Vec<String>,
}

/// Facets extracted from a session via AI analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionFacets {
    pub session_id: String,
    pub underlying_goal: String,
    pub goal_categories: HashMap<String, usize>,
    pub outcome: String,
    pub user_satisfaction_counts: HashMap<String, usize>,
    pub mossen_helpfulness: String,
    pub session_type: String,
    pub friction_counts: HashMap<String, usize>,
    pub friction_detail: String,
    pub primary_success: String,
    pub brief_summary: String,
    pub user_instructions_to_mossen: Option<Vec<String>>,
}

/// Aggregated data from all sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedData {
    pub total_sessions: usize,
    pub total_sessions_scanned: Option<usize>,
    pub sessions_with_facets: usize,
    pub date_range: DateRange,
    pub total_messages: usize,
    pub total_duration_hours: f64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub tool_counts: HashMap<String, usize>,
    pub languages: HashMap<String, usize>,
    pub git_commits: usize,
    pub git_pushes: usize,
    pub projects: HashMap<String, usize>,
    pub goal_categories: HashMap<String, usize>,
    pub outcomes: HashMap<String, usize>,
    pub satisfaction: HashMap<String, usize>,
    pub helpfulness: HashMap<String, usize>,
    pub session_types: HashMap<String, usize>,
    pub friction: HashMap<String, usize>,
    pub success: HashMap<String, usize>,
    pub session_summaries: Vec<SessionSummaryEntry>,
    pub avg_session_minutes: f64,
    pub avg_messages_per_session: f64,
    pub avg_tools_per_session: f64,
    pub total_lines_added: usize,
    pub total_lines_removed: usize,
    pub total_files_modified: usize,
    pub total_user_interruptions: usize,
    pub total_tool_errors: usize,
    pub tool_error_categories: HashMap<String, usize>,
    pub avg_user_response_time_s: f64,
    pub sessions_using_task_agent: usize,
    pub sessions_using_mcp: usize,
    pub sessions_using_web_search: usize,
    pub sessions_using_web_fetch: usize,
    pub message_hours: Vec<u32>,
    pub remote_hosts: Vec<RemoteHostInfo>,
    pub concurrent_sessions: Vec<ConcurrentSessionGroup>,
}

/// Date range for the report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DateRange {
    pub start: String,
    pub end: String,
}

/// Entry in the session summaries list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummaryEntry {
    pub session_id: String,
    pub date: String,
    pub summary: String,
    pub goal: String,
    pub outcome: String,
    pub duration_minutes: f64,
}

/// Group of sessions that overlapped in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConcurrentSessionGroup {
    pub session_ids: Vec<String>,
    pub overlap_minutes: f64,
}

/// Insight section definition.
#[derive(Debug, Clone)]
pub struct InsightSection {
    pub key: &'static str,
    pub title: &'static str,
    pub prompt_suffix: &'static str,
}

/// Generated insight results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsightResults {
    pub at_a_glance: Option<AtAGlance>,
    pub sections: HashMap<String, String>,
}

/// At-a-glance summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtAGlance {
    pub whats_working: Option<String>,
    pub whats_hindering: Option<String>,
    pub quick_wins: Option<String>,
    pub ambitious_workflows: Option<String>,
}

/// Exportable insights data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsightsExport {
    pub aggregated: AggregatedData,
    pub insights: InsightResults,
    pub generated_at: String,
}

/// Lightweight session info for scanning.
#[derive(Debug, Clone)]
struct LiteSessionInfo {
    path: PathBuf,
    session_id: String,
    mtime: i64,
}

// ============================================================================
// Constants
// ============================================================================

/// Extension to programming language mapping.
fn extension_to_language(ext: &str) -> Option<&'static str> {
    match ext {
        "ts" | "tsx" => Some("TypeScript"),
        "js" | "jsx" | "mjs" | "cjs" => Some("JavaScript"),
        "py" => Some("Python"),
        "rs" => Some("Rust"),
        "go" => Some("Go"),
        "java" => Some("Java"),
        "kt" | "kts" => Some("Kotlin"),
        "swift" => Some("Swift"),
        "rb" => Some("Ruby"),
        "php" => Some("PHP"),
        "c" | "h" => Some("C"),
        "cpp" | "cc" | "cxx" | "hpp" => Some("C++"),
        "cs" => Some("C#"),
        "scala" => Some("Scala"),
        "clj" | "cljs" => Some("Clojure"),
        "ex" | "exs" => Some("Elixir"),
        "erl" => Some("Erlang"),
        "hs" => Some("Haskell"),
        "ml" | "mli" => Some("OCaml"),
        "r" => Some("R"),
        "jl" => Some("Julia"),
        "dart" => Some("Dart"),
        "lua" => Some("Lua"),
        "vim" | "vimrc" => Some("Vim Script"),
        "sh" | "bash" | "zsh" => Some("Shell"),
        "ps1" => Some("PowerShell"),
        "sql" => Some("SQL"),
        "html" | "htm" => Some("HTML"),
        "css" | "scss" | "sass" | "less" => Some("CSS"),
        "json" => Some("JSON"),
        "yaml" | "yml" => Some("YAML"),
        "toml" => Some("TOML"),
        "xml" => Some("XML"),
        "md" | "mdx" => Some("Markdown"),
        "proto" => Some("Protocol Buffers"),
        "graphql" | "gql" => Some("GraphQL"),
        "tf" => Some("Terraform"),
        "sol" => Some("Solidity"),
        "zig" => Some("Zig"),
        "nim" => Some("Nim"),
        "v" => Some("V"),
        _ => None,
    }
}

/// Human-readable label map for tool names.
fn label_for_tool(tool: &str) -> &str {
    match tool {
        "Read" => "File Read",
        "Write" => "File Write",
        "Edit" => "File Edit",
        "MultiEdit" => "Multi Edit",
        "Bash" => "Shell Command",
        "Glob" => "File Search",
        "Grep" => "Code Search",
        "LS" => "List Directory",
        "Task" | "Agent" => "Sub-agent",
        "WebSearch" => "Web Search",
        "WebFetch" => "Web Fetch",
        "TodoWrite" => "Todo List",
        "AskUserQuestion" => "Ask User",
        _ => tool,
    }
}

/// Insight sections for report generation.
const INSIGHT_SECTIONS: &[InsightSection] = &[
    InsightSection {
        key: "impressive_things",
        title: "Impressive Things You Did",
        prompt_suffix: "Identify the most impressive, creative, or efficient things the user accomplished with the assistant.",
    },
    InsightSection {
        key: "where_things_go_wrong",
        title: "Where Things Go Wrong",
        prompt_suffix: "Identify patterns where sessions failed, got stuck, or had friction.",
    },
    InsightSection {
        key: "features_to_try",
        title: "Features to Try",
        prompt_suffix: "Based on their usage patterns, suggest features or workflows they haven't tried.",
    },
    InsightSection {
        key: "on_the_horizon",
        title: "On the Horizon",
        prompt_suffix: "Suggest more ambitious workflows they could attempt based on their skill level.",
    },
];

/// Satisfaction ordering for charts.
const SATISFACTION_ORDER: &[&str] = &[
    "very_satisfied",
    "satisfied",
    "neutral",
    "frustrated",
    "very_frustrated",
];

/// Outcome ordering for charts.
const OUTCOME_ORDER: &[&str] = &[
    "complete_success",
    "partial_success",
    "abandoned",
    "ongoing",
    "unclear",
];

// ============================================================================
// Helper Functions
// ============================================================================

/// Get the data directory for insights cache.
fn get_data_dir(ctx: &CommandContext) -> PathBuf {
    let config_home = ctx
        .env_vars
        .get("MOSSEN_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home = ctx
                .env_vars
                .get("HOME")
                .cloned()
                .unwrap_or_else(|| "/tmp".to_string());
            PathBuf::from(home).join(".mossen")
        });
    config_home.join("insights")
}

/// Get the facets cache directory.
fn get_facets_dir(ctx: &CommandContext) -> PathBuf {
    get_data_dir(ctx).join("facets")
}

/// Get the session meta cache directory.
fn get_session_meta_dir(ctx: &CommandContext) -> PathBuf {
    get_data_dir(ctx).join("session_meta")
}

/// Get the projects directory where session logs are stored.
fn get_projects_dir(ctx: &CommandContext) -> PathBuf {
    let config_home = ctx
        .env_vars
        .get("MOSSEN_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home = ctx
                .env_vars
                .get("HOME")
                .cloned()
                .unwrap_or_else(|| "/tmp".to_string());
            PathBuf::from(home).join(".mossen")
        });
    config_home.join("projects")
}

/// Extract language from a file path based on extension.
fn get_language_from_path(file_path: &str) -> Option<&'static str> {
    let ext = Path::new(file_path).extension()?.to_str()?;
    extension_to_language(ext)
}

/// Deduplicate session branches by choosing the most recent session per branch.
pub fn deduplicate_session_branches(sessions: &[SessionMeta]) -> Vec<&SessionMeta> {
    let mut branch_map: HashMap<String, &SessionMeta> = HashMap::new();
    for session in sessions {
        let key = format!("{}:{}", session.project_path, session.session_id);
        branch_map
            .entry(key)
            .and_modify(|existing| {
                if session.start_time > existing.start_time {
                    *existing = session;
                }
            })
            .or_insert(session);
    }
    branch_map.into_values().collect()
}

/// Detect concurrent sessions from timestamps.
pub fn detect_concurrent_sessions(sessions: &[SessionMeta]) -> Vec<ConcurrentSessionGroup> {
    let mut groups: Vec<ConcurrentSessionGroup> = Vec::new();

    // Sort sessions by start time
    let mut sorted: Vec<&SessionMeta> = sessions.iter().collect();
    sorted.sort_by(|a, b| a.start_time.cmp(&b.start_time));

    for i in 0..sorted.len() {
        for j in (i + 1)..sorted.len() {
            let a = sorted[i];
            let b = sorted[j];

            // Parse start times
            let a_start = parse_iso_datetime(&a.start_time);
            let b_start = parse_iso_datetime(&b.start_time);

            if let (Some(a_start_dt), Some(b_start_dt)) = (a_start, b_start) {
                let a_end = a_start_dt + chrono::Duration::minutes(a.duration_minutes as i64);
                if b_start_dt < a_end {
                    let overlap = (a_end - b_start_dt).num_minutes() as f64;
                    if overlap > 5.0 {
                        groups.push(ConcurrentSessionGroup {
                            session_ids: vec![a.session_id.clone(), b.session_id.clone()],
                            overlap_minutes: overlap,
                        });
                    }
                }
            }
        }
    }

    groups
}

/// Parse ISO datetime string.
fn parse_iso_datetime(s: &str) -> Option<DateTime<Utc>> {
    // Try parsing with various formats
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc));
    }
    if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f") {
        return Some(DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc));
    }
    if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return Some(DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc));
    }
    None
}

/// Aggregate session data from all sessions and facets.
fn aggregate_data(sessions: &[SessionMeta], facets: &[SessionFacets]) -> AggregatedData {
    let total_sessions = sessions.len();
    let sessions_with_facets = facets.len();

    let mut total_messages: usize = 0;
    let mut total_duration_minutes: f64 = 0.0;
    let mut total_input_tokens: u64 = 0;
    let mut total_output_tokens: u64 = 0;
    let mut tool_counts: HashMap<String, usize> = HashMap::new();
    let mut languages: HashMap<String, usize> = HashMap::new();
    let mut git_commits: usize = 0;
    let mut git_pushes: usize = 0;
    let mut projects: HashMap<String, usize> = HashMap::new();
    let mut total_lines_added: usize = 0;
    let mut total_lines_removed: usize = 0;
    let mut total_files_modified: usize = 0;
    let mut total_user_interruptions: usize = 0;
    let mut total_tool_errors: usize = 0;
    let mut tool_error_categories: HashMap<String, usize> = HashMap::new();
    let mut all_response_times: Vec<f64> = Vec::new();
    let mut sessions_using_task_agent: usize = 0;
    let mut sessions_using_mcp: usize = 0;
    let mut sessions_using_web_search: usize = 0;
    let mut sessions_using_web_fetch: usize = 0;
    let mut message_hours: Vec<u32> = Vec::new();
    let mut start_dates: Vec<String> = Vec::new();
    let mut end_dates: Vec<String> = Vec::new();

    for session in sessions {
        total_messages += session.user_message_count + session.assistant_message_count;
        total_duration_minutes += session.duration_minutes;
        total_input_tokens += session.input_tokens;
        total_output_tokens += session.output_tokens;
        git_commits += session.git_commits;
        git_pushes += session.git_pushes;
        total_lines_added += session.lines_added;
        total_lines_removed += session.lines_removed;
        total_files_modified += session.files_modified;
        total_user_interruptions += session.user_interruptions;
        total_tool_errors += session.tool_errors;

        for (tool, count) in &session.tool_counts {
            *tool_counts.entry(tool.clone()).or_insert(0) += count;
        }
        for (lang, count) in &session.languages {
            *languages.entry(lang.clone()).or_insert(0) += count;
        }
        for (cat, count) in &session.tool_error_categories {
            *tool_error_categories.entry(cat.clone()).or_insert(0) += count;
        }

        *projects.entry(session.project_path.clone()).or_insert(0) += 1;
        all_response_times.extend(&session.user_response_times);
        message_hours.extend(&session.message_hours);

        if session.uses_task_agent {
            sessions_using_task_agent += 1;
        }
        if session.uses_mcp {
            sessions_using_mcp += 1;
        }
        if session.uses_web_search {
            sessions_using_web_search += 1;
        }
        if session.uses_web_fetch {
            sessions_using_web_fetch += 1;
        }

        if !session.start_time.is_empty() {
            start_dates.push(session.start_time.clone());
            end_dates.push(session.start_time.clone());
        }
    }

    // Aggregate facets
    let mut goal_categories: HashMap<String, usize> = HashMap::new();
    let mut outcomes: HashMap<String, usize> = HashMap::new();
    let mut satisfaction: HashMap<String, usize> = HashMap::new();
    let mut helpfulness: HashMap<String, usize> = HashMap::new();
    let mut session_types: HashMap<String, usize> = HashMap::new();
    let mut friction: HashMap<String, usize> = HashMap::new();
    let mut success: HashMap<String, usize> = HashMap::new();
    let mut session_summaries: Vec<SessionSummaryEntry> = Vec::new();

    for facet in facets {
        for (cat, count) in &facet.goal_categories {
            *goal_categories.entry(cat.clone()).or_insert(0) += count;
        }
        *outcomes.entry(facet.outcome.clone()).or_insert(0) += 1;
        for (sat, count) in &facet.user_satisfaction_counts {
            *satisfaction.entry(sat.clone()).or_insert(0) += count;
        }
        *helpfulness
            .entry(facet.mossen_helpfulness.clone())
            .or_insert(0) += 1;
        *session_types.entry(facet.session_type.clone()).or_insert(0) += 1;
        for (f, count) in &facet.friction_counts {
            *friction.entry(f.clone()).or_insert(0) += count;
        }
        *success.entry(facet.primary_success.clone()).or_insert(0) += 1;

        // Find matching session meta for summary
        if let Some(meta) = sessions.iter().find(|s| s.session_id == facet.session_id) {
            session_summaries.push(SessionSummaryEntry {
                session_id: facet.session_id.clone(),
                date: meta.start_time.clone(),
                summary: facet.brief_summary.clone(),
                goal: facet.underlying_goal.clone(),
                outcome: facet.outcome.clone(),
                duration_minutes: meta.duration_minutes,
            });
        }
    }

    start_dates.sort();
    end_dates.sort();

    let date_range = DateRange {
        start: start_dates
            .first()
            .cloned()
            .unwrap_or_else(|| "unknown".to_string()),
        end: end_dates
            .last()
            .cloned()
            .unwrap_or_else(|| "unknown".to_string()),
    };

    let avg_session_minutes = if total_sessions > 0 {
        total_duration_minutes / total_sessions as f64
    } else {
        0.0
    };
    let avg_messages_per_session = if total_sessions > 0 {
        total_messages as f64 / total_sessions as f64
    } else {
        0.0
    };
    let total_tool_uses: usize = tool_counts.values().sum();
    let avg_tools_per_session = if total_sessions > 0 {
        total_tool_uses as f64 / total_sessions as f64
    } else {
        0.0
    };
    let avg_user_response_time_s = if !all_response_times.is_empty() {
        all_response_times.iter().sum::<f64>() / all_response_times.len() as f64
    } else {
        0.0
    };

    AggregatedData {
        total_sessions,
        total_sessions_scanned: None,
        sessions_with_facets,
        date_range,
        total_messages,
        total_duration_hours: total_duration_minutes / 60.0,
        total_input_tokens,
        total_output_tokens,
        tool_counts,
        languages,
        git_commits,
        git_pushes,
        projects,
        goal_categories,
        outcomes,
        satisfaction,
        helpfulness,
        session_types,
        friction,
        success,
        session_summaries,
        avg_session_minutes,
        avg_messages_per_session,
        avg_tools_per_session,
        total_lines_added,
        total_lines_removed,
        total_files_modified,
        total_user_interruptions,
        total_tool_errors,
        tool_error_categories,
        avg_user_response_time_s,
        sessions_using_task_agent,
        sessions_using_mcp,
        sessions_using_web_search,
        sessions_using_web_fetch,
        message_hours,
        remote_hosts: Vec::new(),
        concurrent_sessions: Vec::new(),
    }
}

/// Generate a bar chart in text form for the report.
fn generate_bar_chart(data: &HashMap<String, usize>, order: Option<&[&str]>) -> String {
    let mut entries: Vec<(&String, &usize)> = data.iter().collect();

    if let Some(ord) = order {
        entries.sort_by_key(|(k, _)| {
            let k_str: &str = k.as_str();
            ord.iter().position(|o| *o == k_str).unwrap_or(usize::MAX)
        });
    } else {
        entries.sort_by(|a, b| b.1.cmp(a.1));
    }

    let max_val = entries.iter().map(|(_, v)| **v).max().unwrap_or(1);
    let max_bar_width = 30;

    let mut result = String::new();
    for (key, val) in &entries {
        let bar_len = if max_val > 0 {
            (**val * max_bar_width) / max_val
        } else {
            0
        };
        let bar: String = "█".repeat(bar_len);
        let label = label_for_tool(key);
        result.push_str(&format!("  {:<20} {} {}\n", label, bar, val));
    }
    result
}

/// Generate response time histogram.
fn generate_response_time_histogram(times: &[f64]) -> String {
    if times.is_empty() {
        return "  No response time data available.\n".to_string();
    }

    let buckets = [
        (0.0, 5.0, "0-5s"),
        (5.0, 15.0, "5-15s"),
        (15.0, 30.0, "15-30s"),
        (30.0, 60.0, "30-60s"),
        (60.0, 120.0, "1-2min"),
        (120.0, 300.0, "2-5min"),
        (300.0, f64::INFINITY, "5min+"),
    ];

    let mut counts: Vec<(&&str, usize)> = Vec::new();
    for (min, max, label) in &buckets {
        let count = times.iter().filter(|t| **t >= *min && **t < *max).count();
        counts.push((label, count));
    }

    let max_count = counts.iter().map(|(_, c)| *c).max().unwrap_or(1);
    let max_bar = 20;

    let mut result = String::new();
    for (label, count) in &counts {
        let bar_len = if max_count > 0 {
            (*count * max_bar) / max_count
        } else {
            0
        };
        let bar: String = "▓".repeat(bar_len);
        result.push_str(&format!("  {:<8} {} {}\n", label, bar, count));
    }
    result
}

/// Generate time-of-day activity chart.
fn generate_time_of_day_chart(message_hours: &[u32]) -> String {
    if message_hours.is_empty() {
        return "  No time-of-day data available.\n".to_string();
    }

    let mut hour_counts = [0usize; 24];
    for &h in message_hours {
        if (h as usize) < 24 {
            hour_counts[h as usize] += 1;
        }
    }

    let max_count = hour_counts.iter().max().copied().unwrap_or(1);
    let max_bar = 15;

    let mut result = String::new();
    for hour in 0..24 {
        let count = hour_counts[hour];
        if count > 0 {
            let bar_len = (count * max_bar) / max_count;
            let bar: String = "▒".repeat(bar_len);
            result.push_str(&format!("  {:02}:00 {} {}\n", hour, bar, count));
        }
    }
    result
}

/// Escape HTML special characters.
fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Escape HTML with bold markers converted to <b> tags.
fn escape_html_with_bold(text: &str) -> String {
    let escaped = escape_html(text);
    // Convert **text** to <b>text</b>
    let mut result = String::new();
    let mut chars = escaped.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '*' {
            if chars.peek() == Some(&'*') {
                chars.next(); // consume second *
                let mut inner = String::new();
                let mut found_end = false;
                while let Some(inner_ch) = chars.next() {
                    if inner_ch == '*' && chars.peek() == Some(&'*') {
                        chars.next();
                        found_end = true;
                        break;
                    }
                    inner.push(inner_ch);
                }
                if found_end {
                    result.push_str(&format!("<b>{}</b>", inner));
                } else {
                    result.push_str("**");
                    result.push_str(&inner);
                }
            } else {
                result.push(ch);
            }
        } else {
            result.push(ch);
        }
    }
    result
}

/// Generate the full HTML report.
fn generate_html_report(
    data: &AggregatedData,
    insights: &InsightResults,
    product_name: &str,
) -> String {
    let title = format!("{} Insights Report", product_name);
    let total_tokens = data.total_input_tokens + data.total_output_tokens;

    let mut sections_html = String::new();
    for section_def in INSIGHT_SECTIONS {
        if let Some(content) = insights.sections.get(section_def.key) {
            sections_html.push_str(&format!(
                "<div class=\"section\"><h2>{}</h2><p>{}</p></div>\n",
                escape_html(section_def.title),
                escape_html_with_bold(content)
            ));
        }
    }

    let tool_chart = generate_bar_chart(&data.tool_counts, None);
    let lang_chart = generate_bar_chart(&data.languages, None);

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>{title}</title>
<style>
body {{ font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; max-width: 900px; margin: 0 auto; padding: 2rem; background: #1a1a2e; color: #e0e0e0; }}
h1 {{ color: #8b5cf6; border-bottom: 2px solid #8b5cf6; padding-bottom: 0.5rem; }}
h2 {{ color: #a78bfa; margin-top: 2rem; }}
.stats {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr)); gap: 1rem; margin: 1rem 0; }}
.stat-card {{ background: #16213e; border-radius: 8px; padding: 1rem; text-align: center; }}
.stat-value {{ font-size: 2rem; font-weight: bold; color: #8b5cf6; }}
.stat-label {{ font-size: 0.85rem; color: #999; }}
.section {{ background: #16213e; border-radius: 8px; padding: 1.5rem; margin: 1rem 0; }}
pre {{ background: #0f3460; padding: 1rem; border-radius: 4px; overflow-x: auto; font-size: 0.85rem; }}
</style>
</head>
<body>
<h1>{title}</h1>
<p>{start} to {end}</p>
<div class="stats">
<div class="stat-card"><div class="stat-value">{sessions}</div><div class="stat-label">Sessions</div></div>
<div class="stat-card"><div class="stat-value">{hours:.1}</div><div class="stat-label">Hours</div></div>
<div class="stat-card"><div class="stat-value">{messages}</div><div class="stat-label">Messages</div></div>
<div class="stat-card"><div class="stat-value">{tokens}</div><div class="stat-label">Tokens</div></div>
<div class="stat-card"><div class="stat-value">{commits}</div><div class="stat-label">Git Commits</div></div>
<div class="stat-card"><div class="stat-value">{lines_added}</div><div class="stat-label">Lines Added</div></div>
</div>
{sections_html}
<h2>Tool Usage</h2>
<pre>{tool_chart}</pre>
<h2>Languages</h2>
<pre>{lang_chart}</pre>
</body>
</html>"#,
        start = escape_html(&data.date_range.start),
        end = escape_html(&data.date_range.end),
        sessions = data.total_sessions,
        hours = data.total_duration_hours,
        messages = data.total_messages,
        tokens = total_tokens,
        commits = data.git_commits,
        lines_added = data.total_lines_added,
    )
}

/// Validate that a parsed object is valid session facets.
fn is_valid_session_facets(facets: &SessionFacets) -> bool {
    !facets.underlying_goal.is_empty()
        && !facets.outcome.is_empty()
        && !facets.brief_summary.is_empty()
}

/// Build the exportable insights data.
pub fn build_export_data(aggregated: AggregatedData, insights: InsightResults) -> InsightsExport {
    InsightsExport {
        aggregated,
        insights,
        generated_at: Utc::now().to_rfc3339(),
    }
}

/// Main report generation logic.
pub async fn generate_usage_report(
    ctx: &CommandContext,
    collect_remote: bool,
    days: Option<u32>,
) -> Result<String> {
    let projects_dir = get_projects_dir(ctx);
    let facets_dir = get_facets_dir(ctx);
    let data_dir = get_data_dir(ctx);

    // Create cache directories
    tokio::fs::create_dir_all(&facets_dir).await.ok();
    tokio::fs::create_dir_all(&data_dir).await.ok();

    // Scan all sessions
    let sessions = scan_all_sessions(&projects_dir, days).await?;

    if sessions.is_empty() {
        return Ok("No sessions found. Use the assistant for a while and try again.".to_string());
    }

    // Load/compute session meta for each session
    let session_metas = load_all_session_metas(&sessions, ctx).await;

    // Load/extract facets
    let session_facets = load_all_facets(&session_metas, ctx).await;

    // Aggregate
    let mut aggregated = aggregate_data(&session_metas, &session_facets);

    // Detect concurrent sessions
    aggregated.concurrent_sessions = detect_concurrent_sessions(&session_metas);

    // Generate insights (would normally call API, here we build from data)
    let insights = generate_insights_from_data(&aggregated, ctx).await;

    // Generate HTML report
    let html = generate_html_report(&aggregated, &insights, &ctx.product_name);

    // Write HTML file
    let html_path = data_dir.join("insights.html");
    tokio::fs::write(&html_path, &html).await.ok();

    // Build summary stats
    let total_tokens = aggregated.total_input_tokens + aggregated.total_output_tokens;
    let stats = format!(
        "{} sessions | {:.1} hours | {} messages | {} tokens",
        aggregated.total_sessions,
        aggregated.total_duration_hours,
        aggregated.total_messages,
        format_number(total_tokens),
    );

    let report_url = format!("file://{}", html_path.display());

    // Build user-facing summary
    let at_a_glance = if let Some(ref glance) = insights.at_a_glance {
        let mut parts = Vec::new();
        if let Some(ref w) = glance.whats_working {
            parts.push(format!("**What's working:** {}", w));
        }
        if let Some(ref h) = glance.whats_hindering {
            parts.push(format!("**What's hindering you:** {}", h));
        }
        if let Some(ref q) = glance.quick_wins {
            parts.push(format!("**Quick wins to try:** {}", q));
        }
        parts.join("\n\n")
    } else {
        "_No insights generated_".to_string()
    };

    Ok(format!(
        "# {} Insights\n\n{}\n{} to {}\n\n## At a Glance\n\n{}\n\nYour full insights report is ready: {}",
        ctx.product_name,
        stats,
        aggregated.date_range.start,
        aggregated.date_range.end,
        at_a_glance,
        report_url,
    ))
}

/// Scan all session files from the projects directory.
async fn scan_all_sessions(projects_dir: &Path, days: Option<u32>) -> Result<Vec<LiteSessionInfo>> {
    let mut sessions = Vec::new();

    let mut entries = match tokio::fs::read_dir(projects_dir).await {
        Ok(e) => e,
        Err(_) => return Ok(sessions),
    };

    let cutoff = days.map(|d| Utc::now() - chrono::Duration::days(d as i64));

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let mut project_entries = match tokio::fs::read_dir(&path).await {
            Ok(e) => e,
            Err(_) => continue,
        };

        while let Some(file_entry) = project_entries.next_entry().await? {
            let file_path = file_entry.path();
            if file_path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }

            let metadata = match tokio::fs::metadata(&file_path).await {
                Ok(m) => m,
                Err(_) => continue,
            };

            let mtime = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);

            if let Some(cutoff_dt) = cutoff {
                let file_dt = DateTime::<Utc>::from_timestamp(mtime, 0);
                if let Some(fdt) = file_dt {
                    if fdt < cutoff_dt {
                        continue;
                    }
                }
            }

            let session_id = file_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();

            sessions.push(LiteSessionInfo {
                path: file_path,
                session_id,
                mtime,
            });
        }
    }

    // Sort by mtime descending (most recent first)
    sessions.sort_by(|a, b| b.mtime.cmp(&a.mtime));
    Ok(sessions)
}

/// Load session metadata for all sessions (using cache where available).
async fn load_all_session_metas(
    sessions: &[LiteSessionInfo],
    _ctx: &CommandContext,
) -> Vec<SessionMeta> {
    let mut metas = Vec::new();
    for session in sessions {
        // In full implementation, this would:
        // 1. Check cache in session_meta_dir
        // 2. If not cached, parse the JSONL log file
        // 3. Extract metadata from log entries
        // For now, create a basic meta from what we know
        metas.push(SessionMeta {
            session_id: session.session_id.clone(),
            project_path: session
                .path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string(),
            start_time: DateTime::<Utc>::from_timestamp(session.mtime, 0)
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_default(),
            duration_minutes: 0.0,
            user_message_count: 0,
            assistant_message_count: 0,
            tool_counts: HashMap::new(),
            languages: HashMap::new(),
            git_commits: 0,
            git_pushes: 0,
            input_tokens: 0,
            output_tokens: 0,
            first_prompt: String::new(),
            summary: None,
            user_interruptions: 0,
            user_response_times: Vec::new(),
            tool_errors: 0,
            tool_error_categories: HashMap::new(),
            uses_task_agent: false,
            uses_mcp: false,
            uses_web_search: false,
            uses_web_fetch: false,
            lines_added: 0,
            lines_removed: 0,
            files_modified: 0,
            message_hours: Vec::new(),
            user_message_timestamps: Vec::new(),
        });
    }
    metas
}

/// Load facets for all sessions (using cache or API extraction).
async fn load_all_facets(_sessions: &[SessionMeta], _ctx: &CommandContext) -> Vec<SessionFacets> {
    // In full implementation, this would:
    // 1. Check facets cache directory
    // 2. If not cached, format transcript and call API for extraction
    // 3. Cache results
    Vec::new()
}

/// Generate insights from aggregated data.
async fn generate_insights_from_data(
    data: &AggregatedData,
    _ctx: &CommandContext,
) -> InsightResults {
    // In full implementation, this would call the API with aggregated data
    // to generate narrative insights for each section.
    // For now, generate basic insights from the data.

    let at_a_glance = if data.total_sessions > 0 {
        Some(AtAGlance {
            whats_working: Some(format!(
                "You've had {} sessions totaling {:.1} hours of productive work.",
                data.total_sessions, data.total_duration_hours
            )),
            whats_hindering: if data.total_tool_errors > 0 {
                Some(format!(
                    "Tool errors occurred {} times across sessions.",
                    data.total_tool_errors
                ))
            } else {
                None
            },
            quick_wins: if data.sessions_using_mcp == 0 {
                Some("Try MCP integrations to expand the assistant's capabilities.".to_string())
            } else {
                None
            },
            ambitious_workflows: if data.sessions_using_task_agent == 0 {
                Some("Consider using sub-agents for complex multi-step tasks.".to_string())
            } else {
                None
            },
        })
    } else {
        None
    };

    InsightResults {
        at_a_glance,
        sections: HashMap::new(),
    }
}

/// Format a number with thousand separators.
fn format_number(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
    }
    result.chars().rev().collect()
}

// ============================================================================
// Directive Implementation
// ============================================================================

#[async_trait]
impl Directive for InsightsDirective {
    fn name(&self) -> &str {
        "insights"
    }

    fn description(&self) -> &str {
        "Generate usage insights and analytics report"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::Prompt
    }

    fn is_enabled(&self, ctx: &CommandContext) -> bool {
        ctx.is_internal_user()
    }

    fn is_hidden(&self) -> bool {
        true
    }

    fn argument_hint(&self) -> &str {
        "[--homespaces] [--days N]"
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        let args_str = args.join(" ");
        let collect_remote = args_str.contains("--homespaces");

        let days = if let Some(pos) = args.iter().position(|a| *a == "--days") {
            args.get(pos + 1).and_then(|d| d.parse::<u32>().ok())
        } else {
            None
        };

        let report = generate_usage_report(ctx, collect_remote, days).await?;
        Ok(CommandResult::Text(report))
    }
}
