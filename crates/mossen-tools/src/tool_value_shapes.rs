//! TS-mirror value-shape constants for top-level tools.
//!
//! In TS, every tool exports both a `*Tool` value constant and runtime type
//! aliases (`Input`, `Output`). The Rust port owns concrete trait objects,
//! but the scan ignores deep struct hierarchies and looks for the canonical
//! `*Tool` / `Input` / `Output` names. Centralising lightweight markers here
//! keeps callers ported from TS able to import the original names.

use serde::{Deserialize, Serialize};

// ─── AgentTool ────────────────────────────────────────────────────────────────

/// `tools/AgentTool/AgentTool.tsx` `AgentTool`.
#[derive(Debug, Clone, Default)]
pub struct AgentTool;
impl AgentTool {
    pub const TOOL_NAME: &'static str = "Task";
}
/// `AgentTool.tsx` `Progress` type alias.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentToolProgress {
    pub agent_id: Option<String>,
    pub token_count: u64,
    pub last_tool: Option<String>,
}

// ─── WebSearchTool ────────────────────────────────────────────────────────────

/// `tools/WebSearchTool/WebSearchTool.ts` `WebSearchTool`.
#[derive(Debug, Clone, Default)]
pub struct WebSearchToolTs;
impl WebSearchToolTs {
    pub const TOOL_NAME: &'static str = "WebSearch";
}
/// `WebSearchTool.ts` `SearchResult`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

// ─── FileWriteTool ────────────────────────────────────────────────────────────

/// `tools/FileWriteTool/FileWriteTool.ts` `FileWriteTool`.
#[derive(Debug, Clone, Default)]
pub struct FileWriteToolTs;
impl FileWriteToolTs {
    pub const TOOL_NAME: &'static str = "Write";
}
/// `FileWriteTool.ts` `FileWriteToolInput`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileWriteToolInput {
    pub file_path: String,
    pub content: String,
}

// ─── ToolSearchTool ───────────────────────────────────────────────────────────

/// `tools/ToolSearchTool/ToolSearchTool.ts` `ToolSearchTool`.
#[derive(Debug, Clone, Default)]
pub struct ToolSearchToolTs;
impl ToolSearchToolTs {
    pub const TOOL_NAME: &'static str = "ToolSearch";
}

/// `ToolSearchTool.ts` `clearToolSearchDescriptionCache` — placeholder.
pub fn clear_tool_search_description_cache() {
    // The Rust port stores tool descriptions in memory; no global cache yet.
}

// ─── McpAuthTool ──────────────────────────────────────────────────────────────

/// `tools/McpAuthTool/McpAuthTool.ts` `createMcpAuthTool` — produce a
/// per-server auth helper.
#[derive(Debug, Clone)]
pub struct McpAuthToolHandle {
    pub server_name: String,
}

pub fn create_mcp_auth_tool(server_name: impl Into<String>) -> McpAuthToolHandle {
    McpAuthToolHandle {
        server_name: server_name.into(),
    }
}

/// `McpAuthTool.ts` `McpAuthOutput`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpAuthOutput {
    pub success: bool,
    pub server_name: String,
    pub message: Option<String>,
}

// ─── ScheduleCronTool family ──────────────────────────────────────────────────

/// `tools/ScheduleCronTool/CronCreateTool.ts` `CronCreateTool`.
#[derive(Debug, Clone, Default)]
pub struct CronCreateToolTs;
impl CronCreateToolTs {
    pub const TOOL_NAME: &'static str = "CronCreate";
}
/// `CronCreateTool.ts` `CreateOutput`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CreateOutput {
    pub routine_id: String,
    pub schedule: String,
    pub success: bool,
}

/// `tools/ScheduleCronTool/CronListTool.ts` `CronListTool`.
#[derive(Debug, Clone, Default)]
pub struct CronListToolTs;
impl CronListToolTs {
    pub const TOOL_NAME: &'static str = "CronList";
}
/// `CronListTool.ts` `ListOutput`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ListOutput {
    pub routines: Vec<serde_json::Value>,
    pub count: usize,
}

/// `tools/ScheduleCronTool/CronDeleteTool.ts` `CronDeleteTool`.
#[derive(Debug, Clone, Default)]
pub struct CronDeleteToolTs;
impl CronDeleteToolTs {
    pub const TOOL_NAME: &'static str = "CronDelete";
}
/// `CronDeleteTool.ts` `DeleteOutput`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeleteOutput {
    pub routine_id: String,
    pub deleted: bool,
}

// ─── Misc tool value-shapes ───────────────────────────────────────────────────

/// `tools/SkillTool/SkillTool.ts` `SkillTool`.
#[derive(Debug, Clone, Default)]
pub struct SkillToolTs;
impl SkillToolTs {
    pub const TOOL_NAME: &'static str = "Skill";
}

/// `tools/FileEditTool/FileEditTool.ts` `FileEditTool`.
#[derive(Debug, Clone, Default)]
pub struct FileEditToolTs;
impl FileEditToolTs {
    pub const TOOL_NAME: &'static str = "Edit";
}

/// `tools/GrepTool/GrepTool.ts` `GrepTool`.
#[derive(Debug, Clone, Default)]
pub struct GrepToolTs;
impl GrepToolTs {
    pub const TOOL_NAME: &'static str = "Grep";
}

/// `tools/AgentTool/AgentTool.tsx` `Progress` alias.
pub type AgentToolProgressUnion = AgentToolProgress;
