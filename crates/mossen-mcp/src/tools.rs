//! MCP 工具调用转发
//!
//! 将 MCP 服务器提供的工具映射为 Mossen 内部工具格式，
//! 并转发工具调用请求到对应的 MCP 服务器。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::client::McpClient;
use crate::normalization::normalize_name_for_mcp;
use crate::protocol::{CallToolResult, ContentBlock, ToolDefinition};
use mossen_types::{ToolDefinition as MossenToolDefinition, ToolInputSchema};

// ─── MCP 工具封装 ────────────────────────────────────────────────────────────

/// 来自 MCP 服务器的工具封装
#[derive(Debug, Clone)]
pub struct McpTool {
    /// 完全限定名（mcp__server__tool 格式）
    pub qualified_name: String,
    /// 原始工具名（服务器提供的名称）
    pub original_name: String,
    /// 服务器名称
    pub server_name: String,
    /// 工具描述
    pub description: Option<String>,
    /// 输入 JSON Schema
    pub input_schema: Option<Value>,
}

impl McpTool {
    /// 用户可见名称
    pub fn display_name(&self) -> String {
        format!("{} - {} (MCP)", self.server_name, self.original_name)
    }

    /// 是否为只读工具（MCP 工具默认非只读）
    pub fn is_read_only(&self) -> bool {
        false
    }
}

// ─── 工具注册表 ──────────────────────────────────────────────────────────────

/// MCP 工具注册表——管理所有已注册的 MCP 工具
pub struct McpToolRegistry {
    /// 工具映射：qualified_name → McpTool
    tools: HashMap<String, McpTool>,
    /// 服务器名称 → 工具列表
    tools_by_server: HashMap<String, Vec<String>>,
    /// 规范化名称映射：normalized → original
    normalized_names: HashMap<String, String>,
}

impl McpToolRegistry {
    /// 创建空注册表
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
            tools_by_server: HashMap::new(),
            normalized_names: HashMap::new(),
        }
    }

    /// 注册来自服务器的工具列表
    pub fn register_server_tools(&mut self, server_name: &str, tools: Vec<ToolDefinition>) {
        // 清除该服务器的旧工具
        if let Some(old_names) = self.tools_by_server.remove(server_name) {
            for name in &old_names {
                self.tools.remove(name);
            }
        }

        let mut tool_names = Vec::new();
        for tool_def in tools {
            let qualified = build_mcp_tool_name(server_name, &tool_def.name);
            let mcp_tool = McpTool {
                qualified_name: qualified.clone(),
                original_name: tool_def.name.clone(),
                server_name: server_name.to_string(),
                description: tool_def.description,
                input_schema: tool_def.input_schema,
            };
            self.tools.insert(qualified.clone(), mcp_tool);
            self.normalized_names
                .insert(qualified.clone(), tool_def.name.clone());
            tool_names.push(qualified);
        }
        self.tools_by_server
            .insert(server_name.to_string(), tool_names);
    }

    /// 移除服务器的所有工具
    pub fn remove_server_tools(&mut self, server_name: &str) {
        if let Some(names) = self.tools_by_server.remove(server_name) {
            for name in names {
                self.tools.remove(&name);
                self.normalized_names.remove(&name);
            }
        }
    }

    /// 按限定名查找工具
    pub fn get_tool(&self, qualified_name: &str) -> Option<&McpTool> {
        self.tools.get(qualified_name)
    }

    /// 获取所有已注册工具
    pub fn all_tools(&self) -> Vec<&McpTool> {
        self.tools.values().collect()
    }

    /// 获取指定服务器的工具
    pub fn tools_for_server(&self, server_name: &str) -> Vec<&McpTool> {
        self.tools_by_server
            .get(server_name)
            .map(|names| names.iter().filter_map(|n| self.tools.get(n)).collect())
            .unwrap_or_default()
    }

    /// 获取规范化名称映射
    pub fn normalized_names(&self) -> &HashMap<String, String> {
        &self.normalized_names
    }
}

impl Default for McpToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ─── 工具调用执行 ────────────────────────────────────────────────────────────

/// MCP 工具调用结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolCallResult {
    /// 文本输出
    pub text: String,
    /// 是否为错误
    pub is_error: bool,
    /// 原始内容块
    pub content_blocks: Vec<ContentBlock>,
}

/// 执行 MCP 工具调用
pub async fn execute_mcp_tool_call(
    client: &McpClient,
    tool_name: &str,
    arguments: Option<Value>,
) -> anyhow::Result<McpToolCallResult> {
    let result = client.call_tool(tool_name, arguments).await?;
    Ok(convert_call_result(result))
}

/// 将 MCP CallToolResult 转换为内部结果格式
fn convert_call_result(result: CallToolResult) -> McpToolCallResult {
    let mut text_parts = Vec::new();

    for block in &result.content {
        match block {
            ContentBlock::Text { text } => {
                text_parts.push(text.clone());
            }
            ContentBlock::Image { data, mime_type } => {
                text_parts.push(format!("[Image: {} ({} bytes)]", mime_type, data.len()));
            }
            ContentBlock::Resource { resource } => {
                text_parts.push(format!("[Resource: {}]", resource.uri));
            }
        }
    }

    McpToolCallResult {
        text: text_parts.join("\n"),
        is_error: result.is_error.unwrap_or(false),
        content_blocks: result.content,
    }
}

// ─── 名称工具函数 ────────────────────────────────────────────────────────────

/// 构建完全限定的 MCP 工具名称
///
/// 格式: `mcp__<normalized_server>__<normalized_tool>`
pub fn build_mcp_tool_name(server_name: &str, tool_name: &str) -> String {
    format!(
        "mcp__{}__{}",
        normalize_name_for_mcp(server_name),
        normalize_name_for_mcp(tool_name)
    )
}

/// 获取 MCP 工具名称前缀
pub fn get_mcp_prefix(server_name: &str) -> String {
    format!("mcp__{}__", normalize_name_for_mcp(server_name))
}

/// 从完全限定名中解析 MCP 信息
///
/// 输入: `mcp__server_name__tool_name`
/// 输出: `Some((server_name, Some(tool_name)))` 或 `None`
pub fn parse_mcp_tool_name(full_name: &str) -> Option<(String, Option<String>)> {
    let parts: Vec<&str> = full_name.splitn(3, "__").collect();
    if parts.len() < 2 || parts[0] != "mcp" {
        return None;
    }
    let server_name = parts[1].to_string();
    let tool_name = if parts.len() > 2 {
        Some(parts[2].to_string())
    } else {
        None
    };
    Some((server_name, tool_name))
}

/// 获取 MCP 工具的显示名称（去除前缀）
pub fn get_mcp_display_name(full_name: &str, server_name: &str) -> String {
    let prefix = get_mcp_prefix(server_name);
    full_name
        .strip_prefix(&prefix)
        .unwrap_or(full_name)
        .to_string()
}

/// 按服务器名称过滤工具
pub fn filter_tools_by_server<'a>(tools: &'a [McpTool], server_name: &str) -> Vec<&'a McpTool> {
    let prefix = get_mcp_prefix(server_name);
    tools
        .iter()
        .filter(|t| t.qualified_name.starts_with(&prefix))
        .collect()
}

/// Convert an MCP protocol tool into the Mossen API tool-definition shape.
///
/// The model only sees the normalized, fully-qualified name. Execution code
/// later resolves that back to the MCP server's original tool name before
/// calling `tools/call`.
pub fn to_mossen_tool_definition(server_name: &str, tool: &ToolDefinition) -> MossenToolDefinition {
    MossenToolDefinition {
        name: build_mcp_tool_name(server_name, &tool.name),
        description: tool
            .description
            .clone()
            .unwrap_or_else(|| format!("MCP tool '{}' from server '{}'.", tool.name, server_name)),
        input_schema: mcp_input_schema_to_mossen(tool.input_schema.as_ref()),
        cache_control: None,
    }
}

fn mcp_input_schema_to_mossen(schema: Option<&Value>) -> ToolInputSchema {
    let Some(Value::Object(raw)) = schema else {
        return ToolInputSchema {
            schema_type: "object".to_string(),
            properties: None,
            required: None,
            extra: HashMap::new(),
        };
    };

    let schema_type = raw
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("object")
        .to_string();

    let mut extra = HashMap::new();
    for (key, value) in raw {
        if key != "type" && key != "properties" && key != "required" {
            extra.insert(key.clone(), value.clone());
        }
    }

    let properties = raw.get("properties").and_then(|value| {
        value.as_object().map(|map| {
            map.iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect()
        })
    });

    if properties.is_none() {
        if let Some(value) = raw.get("properties") {
            extra.insert("properties".to_string(), value.clone());
        }
    }

    let required = raw.get("required").and_then(|value| {
        value.as_array().map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(ToString::to_string))
                .collect::<Vec<_>>()
        })
    });

    if required.is_none() {
        if let Some(value) = raw.get("required") {
            extra.insert("required".to_string(), value.clone());
        }
    }

    ToolInputSchema {
        schema_type,
        properties,
        required,
        extra,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn converts_mcp_tool_to_model_visible_definition() {
        let tool = ToolDefinition {
            name: "list repos".to_string(),
            description: Some("List repositories".to_string()),
            input_schema: Some(json!({
                "type": "object",
                "properties": {
                    "owner": { "type": "string" }
                },
                "required": ["owner"],
                "additionalProperties": false
            })),
        };

        let converted = to_mossen_tool_definition("hosted GitHub", &tool);

        assert_eq!(converted.name, "mcp__hosted_GitHub__list_repos");
        assert_eq!(converted.description, "List repositories");
        assert_eq!(converted.input_schema.schema_type, "object");
        assert_eq!(
            converted
                .input_schema
                .properties
                .as_ref()
                .unwrap()
                .get("owner")
                .unwrap(),
            &json!({ "type": "string" })
        );
        assert_eq!(
            converted.input_schema.required.as_deref(),
            Some(&["owner".to_string()][..])
        );
        assert_eq!(
            converted.input_schema.extra.get("additionalProperties"),
            Some(&json!(false))
        );
    }

    #[test]
    fn converts_missing_mcp_schema_to_empty_object_schema() {
        let tool = ToolDefinition {
            name: "ping".to_string(),
            description: None,
            input_schema: None,
        };

        let converted = to_mossen_tool_definition("dev", &tool);

        assert_eq!(converted.name, "mcp__dev__ping");
        assert_eq!(converted.description, "MCP tool 'ping' from server 'dev'.");
        assert_eq!(converted.input_schema.schema_type, "object");
        assert!(converted.input_schema.properties.is_none());
        assert!(converted.input_schema.required.is_none());
        assert!(converted.input_schema.extra.is_empty());
    }
}
