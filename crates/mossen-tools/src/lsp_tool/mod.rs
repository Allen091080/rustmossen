pub mod formatters;
pub mod prompt;
pub mod schemas;
pub mod symbol_context;

// ---------------------------------------------------------------------------
// TS-mirror — `tools/LSPTool/LSPTool.ts` exports.
// ---------------------------------------------------------------------------

use serde::{Deserialize, Serialize};

/// `LSPTool.ts` `LSPTool` — value-shape constant.
#[derive(Debug, Clone, Default)]
pub struct LSPTool;

impl LSPTool {
    pub const TOOL_NAME: &'static str = "LSP";
}

/// `LSPTool.ts` `Input` shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Input {
    pub action: String,
    pub file_path: String,
    #[serde(default)]
    pub line: Option<u32>,
    #[serde(default)]
    pub character: Option<u32>,
    #[serde(default)]
    pub include_declaration: Option<bool>,
}

/// `LSPTool.ts` `Output` shape.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Output {
    pub results: Vec<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
