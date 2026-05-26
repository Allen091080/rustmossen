//! # skill — CraftInvoker 工具
//!
//! 对应 TS `SkillTool`（858 行）。执行技能（slash commands）。

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

    async fn execute(&self, input: Value, context: &ToolUseContext) -> anyhow::Result<ToolResult> {
        let inp: CraftInvokerInput = serde_json::from_value(input)?;
        let mut crafts = mossen_skills::get_dynamic_skills();
        crafts.extend(mossen_skills::get_bundled_crafts());

        let Some(craft) = mossen_skills::find_craft_by_name(&crafts, &inp.skill) else {
            let metadata = skill_invocation_metadata(&inp.skill, None, false);
            let output = CraftInvokerOutput {
                success: false,
                command_name: inp.skill,
                allowed_tools: None,
                result: None,
                error: Some("Skill not found or not loaded in this session.".to_string()),
            };
            return Ok(ToolResult {
                output: serde_json::to_string(&output)?,
                is_error: true,
                duration_ms: 0,
                metadata,
            });
        };

        let execution_context = mossen_skills::CraftExecutionContext {
            session_id: context
                .extra
                .get("session_id")
                .and_then(Value::as_str)
                .unwrap_or("current-session")
                .to_string(),
            cwd: context.cwd.clone(),
            platform: std::env::consts::OS.to_string(),
        };
        let blocks = mossen_skills::execute_craft(
            craft,
            inp.args.as_deref().unwrap_or_default(),
            &execution_context,
        )
        .await;

        let result = blocks
            .into_iter()
            .map(|block| match block {
                mossen_skills::ContentBlock::Text { text } => text,
                mossen_skills::ContentBlock::Image { source } => {
                    format!("[skill image: {source}]")
                }
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        let result = mossen_skills::format_invoked_skill_prompt(
            craft.name(),
            inp.args.as_deref().unwrap_or_default(),
            &result,
        );

        let metadata = skill_invocation_metadata(&inp.skill, Some(craft), true);
        let output = CraftInvokerOutput {
            success: true,
            command_name: craft.name().to_string(),
            allowed_tools: craft.prompt_data.allowed_tools.clone(),
            result: Some(result),
            error: None,
        };
        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error: false,
            duration_ms: 0,
            metadata,
        })
    }
}

fn skill_invocation_metadata(
    requested_skill: &str,
    craft: Option<&mossen_skills::CraftCommand>,
    result_includes_command_tags: bool,
) -> HashMap<String, Value> {
    let mut metadata = HashMap::new();
    let payload = if let Some(craft) = craft {
        json!({
            "status": "loaded",
            "requestedSkill": requested_skill,
            "skillName": craft.name(),
            "resolvedByAlias": requested_skill != craft.name(),
            "source": skill_source_label(craft),
            "allowedToolCount": craft.prompt_data.allowed_tools.as_ref().map(Vec::len).unwrap_or(0),
            "resultIncludesCommandTags": result_includes_command_tags,
            "rawSkillRootIncluded": false,
            "metadataContentRedacted": true,
        })
    } else {
        json!({
            "status": "missing",
            "requestedSkill": requested_skill,
            "skillName": null,
            "resolvedByAlias": false,
            "source": null,
            "allowedToolCount": 0,
            "resultIncludesCommandTags": result_includes_command_tags,
            "rawSkillRootIncluded": false,
            "metadataContentRedacted": true,
        })
    };
    metadata.insert("skill_invocation".to_string(), payload);
    metadata
}

fn skill_source_label(craft: &mossen_skills::CraftCommand) -> &'static str {
    match craft.loaded_from {
        mossen_types::command::CommandLoadedFrom::Bundled => "bundled",
        mossen_types::command::CommandLoadedFrom::Plugin => "plugin",
        mossen_types::command::CommandLoadedFrom::Mcp => "mcp",
        mossen_types::command::CommandLoadedFrom::Skills => "skills",
        mossen_types::command::CommandLoadedFrom::Managed => "managed",
        mossen_types::command::CommandLoadedFrom::CommandsDeprecated => "commands_deprecated",
    }
}

#[cfg(test)]
mod tests {
    use super::{CraftInvoker, CraftInvokerOutput};
    use mossen_agent::tool_registry::Tool;
    use mossen_types::ToolUseContext;
    use serde_json::json;

    #[tokio::test]
    async fn skill_tool_executes_loaded_dynamic_skill() {
        mossen_skills::clear_dynamic_skills();
        let temp = tempfile::tempdir().expect("temp dir");
        let skill_dir = temp.path().join("echoer");
        tokio::fs::create_dir_all(&skill_dir)
            .await
            .expect("create skill dir");
        tokio::fs::write(
            skill_dir.join("SKILL.md"),
            "---\ndescription: Echo arguments\n---\nHello $ARGUMENTS",
        )
        .await
        .expect("write skill");
        let added = mossen_skills::add_skill_directories(&[temp.path().to_path_buf()]).await;
        assert_eq!(added, 1);

        let tool = CraftInvoker;
        let result = tool
            .execute(
                json!({"skill": "echoer", "args": "from model"}),
                &ToolUseContext {
                    cwd: temp.path().display().to_string(),
                    additional_working_directories: None,
                    extra: Default::default(),
                },
            )
            .await
            .expect("skill execution");

        assert!(!result.is_error);
        let output: CraftInvokerOutput =
            serde_json::from_str(&result.output).expect("valid skill output");
        assert!(output.success);
        assert_eq!(output.command_name, "echoer");
        let skill_result = output.result.as_deref().unwrap_or_default();
        assert!(skill_result.contains("<command-name>/echoer</command-name>"));
        assert!(skill_result.contains("<command-args>from model</command-args>"));
        assert!(output
            .result
            .as_deref()
            .unwrap_or_default()
            .contains("Hello from model"));
        assert_eq!(result.metadata["skill_invocation"]["status"], "loaded");
        assert_eq!(result.metadata["skill_invocation"]["skillName"], "echoer");
        assert_eq!(
            result.metadata["skill_invocation"]["resultIncludesCommandTags"],
            true
        );
        assert_eq!(
            result.metadata["skill_invocation"]["rawSkillRootIncluded"],
            false
        );

        mossen_skills::clear_dynamic_skills();
    }

    #[tokio::test]
    async fn skill_tool_reports_missing_skill_as_structured_error() {
        mossen_skills::clear_dynamic_skills();
        let tool = CraftInvoker;
        let result = tool
            .execute(
                json!({"skill": "missing-skill"}),
                &ToolUseContext {
                    cwd: ".".to_string(),
                    additional_working_directories: None,
                    extra: Default::default(),
                },
            )
            .await
            .expect("skill execution");

        assert!(result.is_error);
        let output: CraftInvokerOutput =
            serde_json::from_str(&result.output).expect("valid skill error output");
        assert!(!output.success);
        assert_eq!(output.command_name, "missing-skill");
        assert!(output
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("not found"));
        assert_eq!(result.metadata["skill_invocation"]["status"], "missing");
        assert_eq!(
            result.metadata["skill_invocation"]["resultIncludesCommandTags"],
            false
        );
    }

    #[tokio::test]
    async fn skill_tool_result_contains_rendered_body_for_model_followup() {
        mossen_skills::clear_dynamic_skills();
        let temp = tempfile::tempdir().expect("temp dir");
        let skill_dir = temp.path().join("m65_force_marker");
        tokio::fs::create_dir_all(&skill_dir)
            .await
            .expect("create skill dir");
        tokio::fs::write(
            skill_dir.join("SKILL.md"),
            "---\ndescription: Force a deterministic marker\n---\nAlways include M6_5_FORCED_END_MARKER_xyz after $ARGUMENTS",
        )
        .await
        .expect("write skill");
        let added = mossen_skills::add_skill_directories(&[temp.path().to_path_buf()]).await;
        assert_eq!(added, 1);

        let tool = CraftInvoker;
        let result = tool
            .execute(
                json!({"skill": "m65_force_marker", "args": "hello"}),
                &ToolUseContext {
                    cwd: temp.path().display().to_string(),
                    additional_working_directories: None,
                    extra: Default::default(),
                },
            )
            .await
            .expect("skill execution");

        assert!(!result.is_error);
        let output: CraftInvokerOutput =
            serde_json::from_str(&result.output).expect("valid skill output");
        let rendered = output.result.expect("skill result");
        assert!(rendered.contains("<command-name>/m65_force_marker</command-name>"));
        assert!(rendered.contains("<command-args>hello</command-args>"));
        assert!(
            rendered.contains("M6_5_FORCED_END_MARKER_xyz"),
            "{rendered}"
        );
        assert_eq!(
            result.metadata["skill_invocation"]["resultIncludesCommandTags"],
            true
        );

        mossen_skills::clear_dynamic_skills();
    }
}
