//! ToolSearchTool prompt and deferred-tool logic.
//!
//! Translated from tools/ToolSearchTool/prompt.ts

use super::constants::TOOL_SEARCH_TOOL_NAME;

const PROMPT_HEAD: &str =
    "Fetches full schema definitions for deferred tools so they can be called.\n\n";

const PROMPT_TAIL: &str = r#" Until fetched, only the name is known — there is no parameter schema, so the tool cannot be invoked. This tool takes a query, matches it against the deferred tool list, and returns the matched tools' complete JSONSchema definitions inside a <functions> block. Once a tool's schema appears in that result, it is callable exactly like any tool defined at the top of the prompt.

Result format: each matched tool appears as one <function>{"description": "...", "name": "...", "parameters": {...}}</function> line inside the <functions> block — the same encoding as the tool list at the top of this prompt.

Query forms:
- "select:Read,Edit,Grep" — fetch these exact tools by name
- "notebook jupyter" — keyword search, up to max_results best matches
- "+slack send" — require "slack" in the name, rank by remaining terms"#;

/// Hint about where deferred tools appear. Matches TS logic based on USER_TYPE and feature flags.
fn get_tool_location_hint() -> &'static str {
    let user_type = std::env::var("USER_TYPE").unwrap_or_default();
    if user_type == "mossen" {
        "Deferred tools appear by name in <system-reminder> messages."
    } else {
        // Default for external users (delta enabled by default going forward)
        "Deferred tools appear by name in <system-reminder> messages."
    }
}

/// Check if a tool should be deferred (requires ToolSearch to load).
/// A tool is deferred if:
/// - It's an MCP tool (always deferred - workflow-specific)
/// - It has should_defer: true
///
/// A tool is NEVER deferred if it has always_load: true.
pub fn is_deferred_tool(name: &str, is_mcp: bool, always_load: bool, should_defer: bool) -> bool {
    // Explicit opt-out
    if always_load {
        return false;
    }
    // MCP tools are always deferred
    if is_mcp {
        return true;
    }
    // Never defer ToolSearch itself
    if name == TOOL_SEARCH_TOOL_NAME {
        return false;
    }
    should_defer
}

/// Format one deferred-tool line for the available-deferred-tools list.
pub fn format_deferred_tool_line(name: &str) -> &str {
    name
}

/// Returns the full ToolSearch prompt.
pub fn get_prompt() -> String {
    format!("{}{}{}", PROMPT_HEAD, get_tool_location_hint(), PROMPT_TAIL)
}
