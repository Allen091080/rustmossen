//! # web_search — WebExplorer 工具
//!
//! 对应 TS `WebSearchTool`（447 行）。执行网页搜索并返回结果。

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// 网络探索器 — 执行网页搜索。
pub struct WebExplorer;

#[derive(Debug, Clone, Deserialize)]
pub struct WebExplorerInput {
    /// 搜索查询字符串。
    pub query: String,
    /// 仅包含来自这些域名的结果。
    #[serde(default)]
    pub allowed_domains: Option<Vec<String>>,
    /// 排除来自这些域名的结果。
    #[serde(default)]
    pub blocked_domains: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WebExplorerOutput {
    pub query: String,
    pub results: Vec<Value>,
    #[serde(rename = "durationSeconds")]
    pub duration_seconds: f64,
}

fn build_input_schema() -> ToolInputSchema {
    let mut properties = HashMap::new();
    properties.insert(
        "query".to_string(),
        serde_json::json!({
            "type": "string",
            "minLength": 2,
            "description": "The search query to use"
        }),
    );
    properties.insert(
        "allowed_domains".to_string(),
        serde_json::json!({
            "type": "array",
            "items": { "type": "string" },
            "description": "Only include search results from these domains"
        }),
    );
    properties.insert(
        "blocked_domains".to_string(),
        serde_json::json!({
            "type": "array",
            "items": { "type": "string" },
            "description": "Never include search results from these domains"
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
impl Tool for WebExplorer {
    fn name(&self) -> &str {
        "WebSearch"
    }
    fn description(&self) -> &str {
        "Search the web and return results"
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
        let inp: WebExplorerInput = serde_json::from_value(input)?;
        let output = WebExplorerOutput {
            query: inp.query,
            results: vec![],
            duration_seconds: 0.0,
        };
        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error: true,
            duration_ms: 0,
            metadata: HashMap::new(),
        })
    }
}
