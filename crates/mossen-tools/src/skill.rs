//! # skill — CraftInvoker 工具
//!
//! 对应 TS `SkillTool`（858 行）。执行技能（slash commands）。

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// 技艺调用器 — 查找并执行技能/slash commands。
pub struct CraftInvoker;

#[derive(Debug, Clone, Deserialize)]
pub struct CraftInvokerInput {
    /// 技能名称（如 "commit", "review-pr", "pdf"）。
    pub skill: String,
    /// 可选参数。
    #[serde(default)]
    pub args: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CraftInvokerOutput {
    pub success: bool,
    #[serde(rename = "commandName")]
    pub command_name: String,
    #[serde(skip_serializing_if = "Option::is_none", rename = "allowedTools")]
    pub allowed_tools: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn build_input_schema() -> ToolInputSchema {
    let mut properties = HashMap::new();
    properties.insert(
        "skill".to_string(),
        serde_json::json!({
            "type": "string",
            "description": "The skill name. E.g., \"commit\", \"review-pr\", or \"pdf\""
        }),
    );
    properties.insert(
        "args".to_string(),
        serde_json::json!({
            "type": "string",
            "description": "Optional arguments for the skill"
        }),
    );
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(properties),
        required: Some(vec!["skill".to_string()]),
        extra: HashMap::new(),
    }
}

#[async_trait]
impl Tool for CraftInvoker {
    fn name(&self) -> &str {
        "Skill"
    }
    fn description(&self) -> &str {
        "Execute a skill (slash command) in a forked sub-agent context"
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
        false
    }

    async fn execute(&self, input: Value, _context: &ToolUseContext) -> anyhow::Result<ToolResult> {
        let inp: CraftInvokerInput = serde_json::from_value(input)?;
        let output = CraftInvokerOutput {
            success: false,
            command_name: inp.skill,
            allowed_tools: None,
            result: None,
            error: Some("Skill tool is a stub in this build.".to_string()),
        };
        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error: true,
            duration_ms: 0,
            metadata: HashMap::new(),
        })
    }
}
