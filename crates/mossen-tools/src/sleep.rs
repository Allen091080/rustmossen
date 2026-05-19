//! # sleep — DeferralTimer 工具
//!
//! 对应 TS `SleepTool`。使 agent 暂停指定秒数，支持中断。

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// 延迟定时器 — 使 agent 暂停一段时间。
pub struct DeferralTimer;

/// 工具输入。
#[derive(Debug, Clone, Deserialize)]
pub struct DeferralTimerInput {
    /// 暂停秒数（1-3600）。
    pub duration_seconds: u64,
    /// 可选暂停原因。
    #[serde(default)]
    pub reason: Option<String>,
}

/// 工具输出。
#[derive(Debug, Clone, Serialize)]
pub struct DeferralTimerOutput {
    pub slept_seconds: u64,
    pub interrupted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

fn build_input_schema() -> ToolInputSchema {
    let mut properties = HashMap::new();
    properties.insert(
        "duration_seconds".to_string(),
        serde_json::json!({
            "type": "number",
            "description": "How long to sleep before waking up again.",
            "minimum": 1,
            "maximum": 3600
        }),
    );
    properties.insert(
        "reason".to_string(),
        serde_json::json!({
            "type": "string",
            "description": "Optional short reason for sleeping."
        }),
    );

    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(properties),
        required: Some(vec!["duration_seconds".to_string()]),
        extra: HashMap::new(),
    }
}

#[async_trait]
impl Tool for DeferralTimer {
    fn name(&self) -> &str {
        "Sleep"
    }

    fn description(&self) -> &str {
        "Pause execution for a specified duration"
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
        let inp: DeferralTimerInput = serde_json::from_value(input)?;

        let duration = std::time::Duration::from_secs(inp.duration_seconds.min(3600));
        tokio::time::sleep(duration).await;

        let output = DeferralTimerOutput {
            slept_seconds: inp.duration_seconds,
            interrupted: false,
            reason: inp.reason,
        };

        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error: false,
            duration_ms: 0,
            metadata: HashMap::new(),
        })
    }
}
