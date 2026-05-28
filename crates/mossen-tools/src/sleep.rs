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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
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

fn parse_input(input: Value) -> Result<DeferralTimerInput, String> {
    match input {
        Value::Null => Err(
            "Sleep requires a JSON object with `duration_seconds`; received null.".to_string(),
        ),
        Value::Object(_) => serde_json::from_value(input).map_err(|error| {
            format!("Sleep received invalid input: {error}. Expected object: {{\"duration_seconds\":1}}.")
        }),
        other => Err(format!(
            "Sleep requires a JSON object with `duration_seconds`; received {}.",
            other
        )),
    }
}

fn error_result(message: impl Into<String>) -> anyhow::Result<ToolResult> {
    let output = DeferralTimerOutput {
        slept_seconds: 0,
        interrupted: true,
        reason: None,
        error: Some(message.into()),
    };
    Ok(ToolResult {
        output: serde_json::to_string(&output)?,
        is_error: true,
        duration_ms: 0,
        metadata: HashMap::new(),
    })
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
        let inp = match parse_input(input) {
            Ok(input) => input,
            Err(message) => return error_result(message),
        };
        if inp.duration_seconds == 0 || inp.duration_seconds > 3600 {
            return error_result("Sleep `duration_seconds` must be between 1 and 3600.");
        }

        let duration = std::time::Duration::from_secs(inp.duration_seconds);
        tokio::time::sleep(duration).await;

        let output = DeferralTimerOutput {
            slept_seconds: inp.duration_seconds,
            interrupted: false,
            reason: inp.reason,
            error: None,
        };

        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error: false,
            duration_ms: 0,
            metadata: HashMap::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::DeferralTimer;
    use mossen_agent::tool_registry::Tool;
    use mossen_types::ToolUseContext;
    use std::collections::HashMap;

    fn context() -> ToolUseContext {
        ToolUseContext {
            cwd: ".".to_string(),
            additional_working_directories: None,
            extra: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn sleep_null_input_returns_structured_tool_error() {
        let result = DeferralTimer
            .execute(serde_json::Value::Null, &context())
            .await
            .expect("sleep result");
        let output: serde_json::Value = serde_json::from_str(&result.output).expect("json");

        assert!(result.is_error);
        assert_eq!(output["interrupted"], true);
        assert!(output["error"]
            .as_str()
            .unwrap_or_default()
            .contains("duration_seconds"));
    }

    #[tokio::test]
    async fn sleep_zero_duration_returns_structured_tool_error() {
        let result = DeferralTimer
            .execute(serde_json::json!({"duration_seconds": 0}), &context())
            .await
            .expect("sleep result");
        let output: serde_json::Value = serde_json::from_str(&result.output).expect("json");

        assert!(result.is_error);
        assert_eq!(output["slept_seconds"], 0);
        assert!(output["error"]
            .as_str()
            .unwrap_or_default()
            .contains("between 1 and 3600"));
    }
}
