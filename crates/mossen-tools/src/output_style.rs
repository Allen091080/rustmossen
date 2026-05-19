//! # output_style — StyleDirective 工具
//!
//! 对应 TS `commands/output-style`（已弃用）。
//!
//! 保留以兼容旧 ToolUse 调用——当模型在历史中触发 `OutputStyle` 工具时，
//! 我们仍然解析与响应，但返回一条提示用户改用 `/config` 的消息，而不再
//! 真正切换输出样式。新代码不应依赖此工具。

use std::collections::HashMap;

use async_trait::async_trait;
use serde_json::Value;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// 样式指令 — 已弃用的输出样式切换工具。
pub struct StyleDirective;

fn build_input_schema() -> ToolInputSchema {
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(HashMap::new()),
        required: None,
        extra: HashMap::new(),
    }
}

#[async_trait]
impl Tool for StyleDirective {
    fn name(&self) -> &str {
        "OutputStyle"
    }

    fn description(&self) -> &str {
        "Deprecated: use /config to change output style"
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

    async fn execute(
        &self,
        _input: Value,
        _context: &ToolUseContext,
    ) -> anyhow::Result<ToolResult> {
        Ok(ToolResult {
            output: serde_json::json!({
                "message": "/output-style has been deprecated. Use /config to change your output style."
            })
            .to_string(),
            is_error: false,
            duration_ms: 0,
            metadata: HashMap::new(),
        })
    }
}
