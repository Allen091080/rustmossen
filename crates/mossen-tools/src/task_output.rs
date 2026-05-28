//! # task_output — ResultEmitter 工具
//!
//! 对应 TS `TaskOutputTool`（515 行）。获取后台任务的输出。

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// 结果发射器 — 获取后台任务输出。
pub struct ResultEmitter;

const DEFAULT_TASK_OUTPUT_TIMEOUT_MS: u64 = 120_000;
const MAX_TASK_OUTPUT_TIMEOUT_MS: u64 = 600_000;

#[derive(Debug, Clone, Deserialize)]
pub struct ResultEmitterInput {
    /// 任务 ID。
    pub task_id: String,
    /// 是否阻塞等待完成。
    #[serde(default = "default_block")]
    pub block: bool,
    /// 最大等待时间（毫秒）。
    #[serde(default = "default_timeout")]
    pub timeout: u64,
}

fn default_block() -> bool {
    true
}
fn default_timeout() -> u64 {
    DEFAULT_TASK_OUTPUT_TIMEOUT_MS
}

#[derive(Debug, Clone, Serialize)]
pub struct ResultEmitterOutput {
    pub retrieval_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task: Option<TaskOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_action: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskOutput {
    pub task_id: String,
    pub task_type: String,
    pub status: String,
    pub description: String,
    pub output: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone)]
enum TaskLookup {
    Found(crate::task_store::TaskRecord),
    Ambiguous(Vec<String>),
    NotFound,
}

fn parse_input(input: Value) -> Result<ResultEmitterInput, String> {
    match input {
        Value::Null => Err(
            "TaskOutput requires a JSON object with a `task_id` string; received null."
                .to_string(),
        ),
        Value::Object(_) => serde_json::from_value(input).map_err(|error| {
            format!("TaskOutput received invalid input: {error}. Expected object: {{\"task_id\":\"...\"}}.")
        }),
        other => Err(format!(
            "TaskOutput requires a JSON object with a `task_id` string; received {}.",
            other
        )),
    }
}

fn error_output(message: impl Into<String>) -> ResultEmitterOutput {
    ResultEmitterOutput {
        retrieval_status: "error".to_string(),
        task: None,
        error: Some(message.into()),
        next_action: None,
    }
}

fn lookup_task(task_id: &str) -> TaskLookup {
    if let Some(record) = crate::task_store::get_task(task_id) {
        return TaskLookup::Found(record);
    }

    // Recover when the model abbreviates a visible agent task id such as
    // `agent-12-...` to `agent-12`. This happened in real multi-agent runs
    // after several launched task ids looked too similar in the transcript.
    if task_id.len() < "agent-0".len() {
        return TaskLookup::NotFound;
    }

    let matches: Vec<_> = crate::task_store::list_task_snapshots()
        .into_iter()
        .filter(|snapshot| snapshot.id.starts_with(task_id))
        .take(6)
        .collect();

    match matches.len() {
        0 => TaskLookup::NotFound,
        1 => {
            let id = matches.into_iter().next().expect("single match").id;
            crate::task_store::get_task(&id)
                .map(TaskLookup::Found)
                .unwrap_or(TaskLookup::NotFound)
        }
        _ => TaskLookup::Ambiguous(matches.into_iter().map(|snapshot| snapshot.id).collect()),
    }
}

fn build_input_schema() -> ToolInputSchema {
    let mut properties = HashMap::new();
    properties.insert(
        "task_id".to_string(),
        serde_json::json!({
            "type": "string", "description": "The task ID to get output from"
        }),
    );
    properties.insert(
        "block".to_string(),
        serde_json::json!({
            "type": "boolean", "description": "Whether to wait for completion", "default": true
        }),
    );
    properties.insert(
        "timeout".to_string(),
        serde_json::json!({
            "type": "number", "description": "Max wait time in ms", "default": DEFAULT_TASK_OUTPUT_TIMEOUT_MS,
            "minimum": 0, "maximum": MAX_TASK_OUTPUT_TIMEOUT_MS
        }),
    );
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(properties),
        required: Some(vec!["task_id".to_string()]),
        extra: HashMap::new(),
    }
}

#[async_trait]
impl Tool for ResultEmitter {
    fn name(&self) -> &str {
        "TaskOutput"
    }
    fn description(&self) -> &str {
        "Get the output of a background task. By default this waits up to 120 seconds; if retrieval_status is not_ready, the task is still running and you should call TaskOutput again with the same task_id."
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
        let start = std::time::Instant::now();
        let inp = match parse_input(input) {
            Ok(input) => input,
            Err(message) => {
                return Ok(ToolResult {
                    output: serde_json::to_string(&error_output(message))?,
                    is_error: true,
                    duration_ms: start.elapsed().as_millis() as u64,
                    metadata: HashMap::new(),
                })
            }
        };
        if inp.task_id.trim().is_empty() {
            return Ok(ToolResult {
                output: serde_json::to_string(&error_output(
                    "TaskOutput requires a non-empty `task_id` string.",
                ))?,
                is_error: true,
                duration_ms: start.elapsed().as_millis() as u64,
                metadata: HashMap::new(),
            });
        }
        let max_wait =
            std::time::Duration::from_millis(inp.timeout.min(MAX_TASK_OUTPUT_TIMEOUT_MS));
        let deadline = std::time::Instant::now() + max_wait;

        let record = loop {
            let current = lookup_task(&inp.task_id);
            let should_wait = inp.block
                && matches!(
                    &current,
                    TaskLookup::Found(r) if !crate::task_store::is_task_ready_status(&r.status)
                )
                && std::time::Instant::now() < deadline;
            if !should_wait {
                break current;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        };

        let output = match record {
            TaskLookup::NotFound => ResultEmitterOutput {
                retrieval_status: "not_found".to_string(),
                task: None,
                error: Some(format!(
                    "No background task found for task_id `{}`. Use the exact task_id returned by Agent.",
                    inp.task_id
                )),
                next_action: Some(
                    "Check the task_id from the Agent tool result and call TaskOutput again with the exact value.".to_string(),
                ),
            },
            TaskLookup::Ambiguous(ids) => ResultEmitterOutput {
                retrieval_status: "ambiguous".to_string(),
                task: None,
                error: Some(format!(
                    "TaskOutput task_id `{}` matched multiple tasks: {}. Use the exact task_id returned by Agent.",
                    inp.task_id,
                    ids.join(", ")
                )),
                next_action: Some(
                    "Use one full task_id from the matched task list and call TaskOutput again.".to_string(),
                ),
            },
            TaskLookup::Found(r) => {
                // For background-agent tasks the store carries `output`
                // and `exit_code` once the agent finishes; for plain
                // workitems they stay empty. Map status → retrieval to
                // match the TS contract: terminal statuses become "ready".
                let retrieval = if crate::task_store::is_task_ready_status(&r.status) {
                    "ready"
                } else {
                    "not_ready"
                }
                .to_string();
                let next_action = if retrieval == "not_ready" {
                    Some(format!(
                        "Task `{}` is still running. Do not treat this as failure; call TaskOutput again with the same task_id and block=true.",
                        r.id
                    ))
                } else {
                    None
                };
                ResultEmitterOutput {
                    retrieval_status: retrieval,
                    task: Some(TaskOutput {
                        task_id: r.id,
                        task_type: r
                            .metadata
                            .get("type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("workitem")
                            .to_string(),
                        status: r.status,
                        description: r.description,
                        output: r.output,
                        exit_code: r.exit_code,
                    }),
                    error: None,
                    next_action,
                }
            }
        };
        let is_error = matches!(
            output.retrieval_status.as_str(),
            "error" | "not_found" | "ambiguous"
        );
        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error,
            duration_ms: start.elapsed().as_millis() as u64,
            metadata: HashMap::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{ResultEmitter, DEFAULT_TASK_OUTPUT_TIMEOUT_MS};
    use mossen_agent::tool_registry::Tool;
    use mossen_types::ToolUseContext;
    use serde_json::Value;
    use std::collections::HashMap;

    #[tokio::test]
    async fn task_output_blocks_until_background_task_is_ready() {
        let _guard = crate::task_store::test_store_guard();
        let record = crate::task_store::create_background_shell_task(
            "printf task-output-ready".to_string(),
            ".".to_string(),
            None,
            1_000,
        );
        let task_id = record.id.clone();
        let task_id_for_finish = task_id.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            crate::task_store::finish_background_shell_task(
                &task_id_for_finish,
                "completed",
                "task-output-ready".to_string(),
                Some(0),
                false,
            );
        });

        let context = ToolUseContext {
            cwd: ".".to_string(),
            additional_working_directories: None,
            extra: HashMap::new(),
        };
        let result = ResultEmitter
            .execute(
                serde_json::json!({
                    "task_id": task_id,
                    "block": true,
                    "timeout": 1_000,
                }),
                &context,
            )
            .await
            .expect("TaskOutput result");
        let output: Value = serde_json::from_str(&result.output).expect("json output");
        assert_eq!(output["retrieval_status"], "ready");
        assert_eq!(output["task"]["status"], "completed");
        assert_eq!(output["task"]["output"], "task-output-ready");
        assert_eq!(output["task"]["exit_code"], 0);
    }

    #[tokio::test]
    async fn task_output_not_ready_tells_model_to_wait_again() {
        let _guard = crate::task_store::test_store_guard();
        crate::task_store::create_background_agent_task(
            "agent-99-slow".to_string(),
            "agent-99-slow".to_string(),
            "fork".to_string(),
            "slow agent".to_string(),
            "return slow marker".to_string(),
            ".".to_string(),
        );
        let context = ToolUseContext {
            cwd: ".".to_string(),
            additional_working_directories: None,
            extra: HashMap::new(),
        };

        let result = ResultEmitter
            .execute(
                serde_json::json!({
                    "task_id": "agent-99-slow",
                    "block": true,
                    "timeout": 1,
                }),
                &context,
            )
            .await
            .expect("TaskOutput result");
        let output: Value = serde_json::from_str(&result.output).expect("json output");

        assert!(!result.is_error, "{}", result.output);
        assert_eq!(output["retrieval_status"], "not_ready");
        assert_eq!(output["task"]["status"], "in_progress");
        assert!(output["next_action"]
            .as_str()
            .unwrap_or_default()
            .contains("call TaskOutput again"));
    }

    #[test]
    fn task_output_schema_default_waits_long_enough_for_real_agents() {
        let definition = ResultEmitter.definition();
        let timeout_default = definition.input_schema.properties.as_ref().unwrap()["timeout"]
            ["default"]
            .as_u64()
            .expect("timeout default");

        assert_eq!(timeout_default, DEFAULT_TASK_OUTPUT_TIMEOUT_MS);
        assert!(ResultEmitter.description().contains("120 seconds"));
    }

    #[tokio::test]
    async fn task_output_null_input_returns_structured_tool_error() {
        let context = ToolUseContext {
            cwd: ".".to_string(),
            additional_working_directories: None,
            extra: HashMap::new(),
        };

        let result = ResultEmitter
            .execute(serde_json::Value::Null, &context)
            .await
            .expect("TaskOutput result");
        let output: Value = serde_json::from_str(&result.output).expect("json output");

        assert!(result.is_error);
        assert_eq!(output["retrieval_status"], "error");
        assert!(output["error"]
            .as_str()
            .unwrap_or_default()
            .contains("task_id"));
    }

    #[tokio::test]
    async fn task_output_empty_task_id_returns_structured_tool_error() {
        let context = ToolUseContext {
            cwd: ".".to_string(),
            additional_working_directories: None,
            extra: HashMap::new(),
        };

        let result = ResultEmitter
            .execute(serde_json::json!({"task_id": ""}), &context)
            .await
            .expect("TaskOutput result");
        let output: Value = serde_json::from_str(&result.output).expect("json output");

        assert!(result.is_error);
        assert_eq!(output["retrieval_status"], "error");
        assert!(output["error"]
            .as_str()
            .unwrap_or_default()
            .contains("non-empty"));
    }

    #[tokio::test]
    async fn task_output_unknown_task_id_is_tool_error_with_recovery_hint() {
        let _guard = crate::task_store::test_store_guard();
        let context = ToolUseContext {
            cwd: ".".to_string(),
            additional_working_directories: None,
            extra: HashMap::new(),
        };

        let result = ResultEmitter
            .execute(
                serde_json::json!({
                    "task_id": "agent-missing",
                    "block": false,
                }),
                &context,
            )
            .await
            .expect("TaskOutput result");
        let output: Value = serde_json::from_str(&result.output).expect("json output");

        assert!(result.is_error, "{}", result.output);
        assert_eq!(output["retrieval_status"], "not_found");
        assert!(output["next_action"]
            .as_str()
            .unwrap_or_default()
            .contains("exact value"));
    }

    #[tokio::test]
    async fn task_output_resolves_unique_abbreviated_agent_task_id() {
        let _guard = crate::task_store::test_store_guard();
        crate::task_store::create_background_agent_task(
            "agent-42-abcdef".to_string(),
            "agent-42-abcdef".to_string(),
            "fork".to_string(),
            "abbrev".to_string(),
            "return marker".to_string(),
            ".".to_string(),
        );
        crate::task_store::finish_background_agent_task(
            "agent-42-abcdef",
            "completed",
            "abbreviated-agent-output".to_string(),
            Some(0),
        );
        let context = ToolUseContext {
            cwd: ".".to_string(),
            additional_working_directories: None,
            extra: HashMap::new(),
        };

        let result = ResultEmitter
            .execute(
                serde_json::json!({
                    "task_id": "agent-42",
                    "block": true,
                    "timeout": 100,
                }),
                &context,
            )
            .await
            .expect("TaskOutput result");
        let output: Value = serde_json::from_str(&result.output).expect("json output");

        assert!(!result.is_error, "{}", result.output);
        assert_eq!(output["retrieval_status"], "ready");
        assert_eq!(output["task"]["task_id"], "agent-42-abcdef");
        assert_eq!(output["task"]["output"], "abbreviated-agent-output");
    }

    #[tokio::test]
    async fn task_output_reports_ambiguous_abbreviated_agent_task_id() {
        let _guard = crate::task_store::test_store_guard();
        for id in ["agent-7-alpha", "agent-7-beta"] {
            crate::task_store::create_background_agent_task(
                id.to_string(),
                id.to_string(),
                "fork".to_string(),
                id.to_string(),
                "prompt".to_string(),
                ".".to_string(),
            );
        }
        let context = ToolUseContext {
            cwd: ".".to_string(),
            additional_working_directories: None,
            extra: HashMap::new(),
        };

        let result = ResultEmitter
            .execute(
                serde_json::json!({
                    "task_id": "agent-7",
                    "block": true,
                    "timeout": 100,
                }),
                &context,
            )
            .await
            .expect("TaskOutput result");
        let output: Value = serde_json::from_str(&result.output).expect("json output");

        assert!(result.is_error);
        assert_eq!(output["retrieval_status"], "ambiguous");
        assert!(output["error"]
            .as_str()
            .unwrap_or_default()
            .contains("agent-7-alpha"));
    }
}
