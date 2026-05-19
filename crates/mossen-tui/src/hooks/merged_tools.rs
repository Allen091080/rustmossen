//! Merged tools hook (useMergedTools.ts).
//! Combines built-in tools with MCP-provided tools.

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub source: ToolSource,
    pub input_schema: serde_json::Value,
    pub requires_approval: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolSource { Builtin, Mcp(String), Plugin(String) }

#[derive(Debug, Clone)]
pub struct MergedToolsState {
    pub tools: HashMap<String, ToolDef>,
    pub override_order: Vec<String>,
}

impl MergedToolsState {
    pub fn new() -> Self { Self { tools: HashMap::new(), override_order: Vec::new() } }
    pub fn register(&mut self, tool: ToolDef) {
        self.override_order.push(tool.name.clone());
        self.tools.insert(tool.name.clone(), tool);
    }
    pub fn unregister(&mut self, name: &str) {
        self.tools.remove(name);
        self.override_order.retain(|n| n != name);
    }
    pub fn get_tool(&self, name: &str) -> Option<&ToolDef> { self.tools.get(name) }
    pub fn all_tools(&self) -> Vec<&ToolDef> {
        self.override_order.iter().filter_map(|n| self.tools.get(n)).collect()
    }
    pub fn tools_requiring_approval(&self) -> Vec<&ToolDef> {
        self.tools.values().filter(|t| t.requires_approval).collect()
    }
}
impl Default for MergedToolsState { fn default() -> Self { Self::new() } }
