//! # ask_user — UserProbe 工具
//!
//! 对应 TS `AskUserQuestionTool`。向用户发送多选题问答，等待用户回答。

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// 用户探询器 — 向用户提出多选题并等待回答。
pub struct UserProbe;

/// 问题选项。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionOption {
    /// 选项显示文本。
    pub label: String,
    /// 选项说明。
    pub description: String,
    /// 可选预览内容。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
}

/// 单个问题。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Question {
    /// 问题文本。
    pub question: String,
    /// 短标签。
    pub header: String,
    /// 选项列表（2-4 个）。
    pub options: Vec<QuestionOption>,
    /// 是否允许多选。
    #[serde(default)]
    pub multi_select: bool,
}

/// 工具输入。
#[derive(Debug, Clone, Deserialize)]
pub struct UserProbeInput {
    /// 要询问用户的问题（1-4 个）。
    pub questions: Vec<Question>,
    /// 用户答案（由权限组件收集）。
    #[serde(default)]
    pub answers: Option<HashMap<String, String>>,
    /// 可选注解。
    #[serde(default)]
    pub annotations: Option<Value>,
}

/// 工具输出。
#[derive(Debug, Clone, Serialize)]
pub struct UserProbeOutput {
    /// 被问的问题。
    pub questions: Vec<Question>,
    /// 用户回答。
    pub answers: HashMap<String, String>,
    /// 可选注解。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Value>,
}

fn build_input_schema() -> ToolInputSchema {
    let mut properties = HashMap::new();
    properties.insert(
        "questions".to_string(),
        serde_json::json!({
            "type": "array",
            "description": "Questions to ask the user (1-4 questions)",
            "minItems": 1,
            "maxItems": 4,
            "items": {
                "type": "object",
                "properties": {
                    "question": { "type": "string", "description": "The question to ask" },
                    "header": { "type": "string", "description": "Short label (max 20 chars)" },
                    "options": {
                        "type": "array",
                        "minItems": 2,
                        "maxItems": 4,
                        "items": {
                            "type": "object",
                            "properties": {
                                "label": { "type": "string" },
                                "description": { "type": "string" },
                                "preview": { "type": "string" }
                            },
                            "required": ["label", "description"]
                        }
                    },
                    "multiSelect": { "type": "boolean", "default": false }
                },
                "required": ["question", "header", "options"]
            }
        }),
    );
    properties.insert(
        "answers".to_string(),
        serde_json::json!({
            "type": "object",
            "description": "User answers collected by the permission component",
            "additionalProperties": { "type": "string" }
        }),
    );
    properties.insert(
        "annotations".to_string(),
        serde_json::json!({
            "type": "object",
            "description": "Optional per-question annotations from the user"
        }),
    );

    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(properties),
        required: Some(vec!["questions".to_string()]),
        extra: HashMap::new(),
    }
}

#[async_trait]
impl Tool for UserProbe {
    fn name(&self) -> &str {
        "AskUserQuestion"
    }

    fn description(&self) -> &str {
        "Ask the user a question with multiple-choice options"
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
        let inp: UserProbeInput = serde_json::from_value(input)?;

        let answers = inp.answers.unwrap_or_default();

        let output = UserProbeOutput {
            questions: inp.questions,
            answers,
            annotations: inp.annotations,
        };

        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error: false,
            duration_ms: 0,
            metadata: HashMap::new(),
        })
    }
}
