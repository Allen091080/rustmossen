//! # synthetic_output — StructuredEmitter 工具
//!
//! 对应 TS `SyntheticOutputTool`（164 行）。返回结构化 JSON 输出。

use std::collections::HashMap;

use async_trait::async_trait;
use serde_json::Value;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// 结构化发射器 — 返回结构化输出。
pub struct StructuredEmitter;

fn build_input_schema() -> ToolInputSchema {
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(HashMap::new()),
        required: Some(vec![]),
        extra: {
            let mut extra = HashMap::new();
            extra.insert("additionalProperties".to_string(), serde_json::json!(true));
            extra
        },
    }
}

#[async_trait]
impl Tool for StructuredEmitter {
    fn name(&self) -> &str {
        "StructuredOutput"
    }

    fn description(&self) -> &str {
        "Return structured output in the requested format"
    }

    fn tool_type(&self) -> ToolType {
        ToolType::SyntheticOutput
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
        // Passthrough: return the input as structured output.
        let output_msg = "Structured output provided successfully";

        let mut metadata = HashMap::new();
        metadata.insert("structured_output".to_string(), input);

        Ok(ToolResult {
            output: output_msg.to_string(),
            is_error: false,
            duration_ms: 0,
            metadata,
        })
    }
}

// ---------------------------------------------------------------------------
// TS-mirror — `tools/SyntheticOutputTool/SyntheticOutputTool.ts` exports.
// ---------------------------------------------------------------------------

/// `SyntheticOutputTool.ts` `SYNTHETIC_OUTPUT_TOOL_NAME`.
pub const SYNTHETIC_OUTPUT_TOOL_NAME: &str = "StructuredOutput";

/// `SyntheticOutputTool.ts` `SyntheticOutputTool` — value-shape constant.
#[derive(Debug, Clone, Default)]
pub struct SyntheticOutputTool;

impl SyntheticOutputTool {
    pub const TOOL_NAME: &'static str = SYNTHETIC_OUTPUT_TOOL_NAME;
}

/// `SyntheticOutputTool.ts` `isSyntheticOutputToolEnabled`.
pub fn is_synthetic_output_tool_enabled(has_output_schema: bool) -> bool {
    has_output_schema
}

/// `SyntheticOutputTool.ts` `createSyntheticOutputTool`.
pub fn create_synthetic_output_tool() -> StructuredEmitter {
    StructuredEmitter
}
