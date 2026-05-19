//! # web_fetch — NetRetriever 工具
//!
//! 对应 TS `WebFetchTool`（319 行）。抓取 URL 内容并处理。

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// 网络检索器 — 获取 URL 页面内容并应用 prompt 提取。
pub struct NetRetriever;

#[derive(Debug, Clone, Deserialize)]
pub struct NetRetrieverInput {
    /// 要获取内容的 URL。
    pub url: String,
    /// 用于处理获取内容的 prompt。
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct NetRetrieverOutput {
    pub bytes: u64,
    pub code: u16,
    #[serde(rename = "codeText")]
    pub code_text: String,
    pub result: String,
    #[serde(rename = "durationMs")]
    pub duration_ms: u64,
    pub url: String,
}

fn build_input_schema() -> ToolInputSchema {
    let mut properties = HashMap::new();
    properties.insert(
        "url".to_string(),
        serde_json::json!({
            "type": "string",
            "format": "uri",
            "description": "The URL to fetch content from"
        }),
    );
    properties.insert(
        "prompt".to_string(),
        serde_json::json!({
            "type": "string",
            "description": "The prompt to run on the fetched content"
        }),
    );
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(properties),
        required: Some(vec!["url".to_string(), "prompt".to_string()]),
        extra: HashMap::new(),
    }
}

#[async_trait]
impl Tool for NetRetriever {
    fn name(&self) -> &str {
        "WebFetch"
    }
    fn description(&self) -> &str {
        "Fetch and extract content from a URL"
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
        let inp: NetRetrieverInput = serde_json::from_value(input)?;
        let output = NetRetrieverOutput {
            bytes: 0,
            code: 0,
            code_text: "stub".to_string(),
            result: "WebFetch is a stub in this build.".to_string(),
            duration_ms: 0,
            url: inp.url,
        };
        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error: true,
            duration_ms: 0,
            metadata: HashMap::new(),
        })
    }
}
