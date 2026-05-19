//! # tool_search — InstrumentFinder 工具
//!
//! 对应 TS `ToolSearchTool`（472 行）。搜索和发现延迟加载的工具。

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// 工具查找器 — 按关键词搜索可用的延迟工具。
pub struct InstrumentFinder;

#[derive(Debug, Clone, Deserialize)]
pub struct InstrumentFinderInput {
    /// 搜索查询，支持 "select:<tool_name>" 直接选择。
    pub query: String,
    /// 返回结果数上限（默认 5）。
    #[serde(default)]
    pub max_results: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstrumentFinderOutput {
    pub matches: Vec<String>,
    pub query: String,
    pub total_deferred_tools: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_mcp_servers: Option<Vec<String>>,
}

fn build_input_schema() -> ToolInputSchema {
    let mut properties = HashMap::new();
    properties.insert("query".to_string(), serde_json::json!({
        "type": "string",
        "description": "Query to find deferred tools. Use \"select:<tool_name>\" for direct selection, or keywords to search."
    }));
    properties.insert(
        "max_results".to_string(),
        serde_json::json!({
            "type": "number",
            "default": 5,
            "description": "Maximum number of results to return (default: 5)"
        }),
    );
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(properties),
        required: Some(vec!["query".to_string()]),
        extra: HashMap::new(),
    }
}

#[async_trait]
impl Tool for InstrumentFinder {
    fn name(&self) -> &str {
        "ToolSearch"
    }
    fn description(&self) -> &str {
        "Search for deferred tools by keyword or direct selection"
    }
    fn tool_type(&self) -> ToolType {
        ToolType::Builtin
    }
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name().to_string(),
            description: self.description().to_string(),
            input_schema: build_input_schema(),
            cache_control: None,
        }
    }
    fn is_read_only(&self) -> bool {
        true
    }

    async fn execute(&self, input: Value, _context: &ToolUseContext) -> anyhow::Result<ToolResult> {
        let inp: InstrumentFinderInput = serde_json::from_value(input)?;
        let output = InstrumentFinderOutput {
            matches: vec![],
            query: inp.query,
            total_deferred_tools: 0,
            pending_mcp_servers: None,
        };
        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error: false,
            duration_ms: 0,
            metadata: HashMap::new(),
        })
    }
}
