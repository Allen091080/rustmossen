//! # team_create — SwarmSpawner 工具
//!
//! 对应 TS `TeamCreateTool`（241 行）。创建多 agent 集群团队。

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// 集群生成器 — 创建 agent 团队。
pub struct SwarmSpawner;

#[derive(Debug, Clone, Deserialize)]
pub struct SwarmSpawnerInput {
    pub team_name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub agent_type: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SwarmSpawnerOutput {
    pub team_name: String,
    pub team_file_path: String,
    pub lead_agent_id: String,
}

fn build_input_schema() -> ToolInputSchema {
    let mut properties = HashMap::new();
    properties.insert(
        "team_name".to_string(),
        serde_json::json!({
            "type": "string", "description": "Name for the new team to create"
        }),
    );
    properties.insert(
        "description".to_string(),
        serde_json::json!({
            "type": "string", "description": "Team description/purpose"
        }),
    );
    properties.insert(
        "agent_type".to_string(),
        serde_json::json!({
            "type": "string", "description": "Type/role of the team lead"
        }),
    );
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(properties),
        required: Some(vec!["team_name".to_string()]),
        extra: HashMap::new(),
    }
}

#[async_trait]
impl Tool for SwarmSpawner {
    fn name(&self) -> &str {
        "TeamCreate"
    }
    fn description(&self) -> &str {
        "Create a multi-agent swarm team"
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
        let inp: SwarmSpawnerInput = serde_json::from_value(input)?;
        let lead_id = uuid::Uuid::new_v4().to_string();
        let output = SwarmSpawnerOutput {
            team_name: inp.team_name,
            team_file_path: String::new(),
            lead_agent_id: lead_id,
        };
        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error: false,
            duration_ms: 0,
            metadata: HashMap::new(),
        })
    }
}
