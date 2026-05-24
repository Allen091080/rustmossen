//! # tool_registry — 工具注册与分发
//!
//! 对应 TS `tools.ts` + `Tool.ts`，定义工具 trait 和注册表。

use std::collections::HashMap;

use serde_json::Value;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error};

use mossen_types::{ToolDefinition, ToolUseContext};

/// 工具类型（本地定义，mossen-types 中未导出）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolType {
    Builtin,
    Mcp,
    SyntheticOutput,
}

// ---------------------------------------------------------------------------
// 工具 Trait
// ---------------------------------------------------------------------------

/// 工具执行结果。
#[derive(Debug, Clone)]
pub struct ToolResult {
    /// 输出内容。
    pub output: String,
    /// 是否为错误。
    pub is_error: bool,
    /// 执行持续时间（毫秒）。
    pub duration_ms: u64,
    /// 额外元数据。
    pub metadata: HashMap<String, Value>,
}

/// 工具 trait——所有工具实现此接口。
///
/// 对应 TS `Tool` 基类。
#[async_trait::async_trait]
pub trait Tool: Send + Sync {
    /// 工具名称。
    fn name(&self) -> &str;

    /// 工具描述。
    fn description(&self) -> &str;

    /// 工具类型。
    fn tool_type(&self) -> ToolType {
        ToolType::Builtin
    }

    /// 获取工具的 API 定义。
    fn definition(&self) -> ToolDefinition;

    /// 是否为只读工具。
    fn is_read_only(&self) -> bool {
        false
    }

    /// 是否需要权限审批。
    fn needs_permission(&self) -> bool {
        !self.is_read_only()
    }

    /// 执行工具。
    async fn execute(&self, input: Value, context: &ToolUseContext) -> anyhow::Result<ToolResult>;
}

// ---------------------------------------------------------------------------
// 工具注册表
// ---------------------------------------------------------------------------

/// 工具注册表——管理所有可用工具。
pub struct ToolRegistry {
    /// 工具映射（名称 → 工具实例）。
    tools: HashMap<String, Box<dyn Tool>>,
}

impl std::fmt::Debug for ToolRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolRegistry")
            .field("tool_count", &self.tools.len())
            .finish()
    }
}

impl ToolRegistry {
    /// 创建空注册表。
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// 注册一个工具。
    pub fn register(&mut self, tool: Box<dyn Tool>) {
        let name = tool.name().to_string();
        debug!(tool = %name, "Registered tool");
        self.tools.insert(name, tool);
    }

    /// 批量注册工具。
    pub fn register_all(&mut self, tools: Vec<Box<dyn Tool>>) {
        for tool in tools {
            self.register(tool);
        }
    }

    /// 按名称查找工具。
    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools
            .get(name)
            .or_else(|| alias_tool_name(name).and_then(|alias| self.tools.get(alias)))
            .map(|t| t.as_ref())
    }

    /// 获取所有工具名称。
    pub fn tool_names(&self) -> Vec<&str> {
        self.tools.keys().map(|s| s.as_str()).collect()
    }

    /// 获取所有工具定义（用于 API 请求）。
    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools.values().map(|t| t.definition()).collect()
    }

    /// 工具数量。
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// 执行指定工具。
    pub async fn execute(
        &self,
        tool_name: &str,
        input: Value,
        context: &ToolUseContext,
    ) -> anyhow::Result<ToolResult> {
        let resolved_name = alias_tool_name(tool_name).unwrap_or(tool_name);
        let tool = self
            .tools
            .get(resolved_name)
            .ok_or_else(|| anyhow::anyhow!("Tool not found: {}", tool_name))?;

        let start = std::time::Instant::now();
        let result = tool.execute(input, context).await;
        let duration_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(mut r) => {
                r.duration_ms = duration_ms;
                Ok(r)
            }
            Err(e) => {
                error!(tool = tool_name, error = %e, "Tool execution failed");
                Ok(ToolResult {
                    output: format!("Error: {}", e),
                    is_error: true,
                    duration_ms,
                    metadata: HashMap::new(),
                })
            }
        }
    }

    /// Execute a tool while watching the current turn cancellation token.
    ///
    /// Dropping the in-flight tool future is significant for shell tools:
    /// their foreground child process is owned by the future and configured
    /// with `kill_on_drop(true)`, so Ctrl-C can stop a running command instead
    /// of waiting for the tool timeout.
    pub async fn execute_with_cancel(
        &self,
        tool_name: &str,
        input: Value,
        context: &ToolUseContext,
        cancel: &CancellationToken,
    ) -> anyhow::Result<ToolResult> {
        if cancel.is_cancelled() {
            anyhow::bail!("Tool execution cancelled");
        }

        let resolved_name = alias_tool_name(tool_name).unwrap_or(tool_name);
        let tool = self
            .tools
            .get(resolved_name)
            .ok_or_else(|| anyhow::anyhow!("Tool not found: {}", tool_name))?;

        let start = std::time::Instant::now();
        let result = tokio::select! {
            result = tool.execute(input, context) => result,
            _ = cancel.cancelled() => {
                anyhow::bail!("Tool execution cancelled");
            }
        };
        let duration_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(mut r) => {
                r.duration_ms = duration_ms;
                Ok(r)
            }
            Err(e) => {
                error!(tool = tool_name, error = %e, "Tool execution failed");
                Ok(ToolResult {
                    output: format!("Error: {}", e),
                    is_error: true,
                    duration_ms,
                    metadata: HashMap::new(),
                })
            }
        }
    }

    /// 获取只读工具列表。
    pub fn read_only_tools(&self) -> Vec<&str> {
        self.tools
            .iter()
            .filter(|(_, t)| t.is_read_only())
            .map(|(name, _)| name.as_str())
            .collect()
    }
}

fn alias_tool_name(name: &str) -> Option<&'static str> {
    match name {
        // Mossen Code-compatible model prompts often call the sub-agent
        // launcher `Task`; the Rust registry's implementation is named
        // `Agent`. Accept both spellings so model output does not dead-end
        // after the user approves a perfectly valid sub-agent call.
        "Task" => Some("Agent"),
        _ => None,
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// TS-mirror — `Tool.ts` top-level exports.
// ---------------------------------------------------------------------------

/// `Tool.ts` `ToolInputJSONSchema`. The TS type is structural; here we
/// keep the same JSON shape (`type: 'object'`, optional `properties`).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolInputJSONSchema {
    #[serde(rename = "type")]
    pub schema_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<serde_json::Map<String, Value>>,
    /// Catch-all for additional schema fields (e.g. `required`).
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

impl ToolInputJSONSchema {
    pub fn new_object() -> Self {
        Self {
            schema_type: "object".to_string(),
            properties: None,
            extra: serde_json::Map::new(),
        }
    }
}

/// `Tool.ts` `QueryChainTracking`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QueryChainTracking {
    pub chain_id: String,
    pub depth: u32,
}

/// `Tool.ts` `ValidationResult`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "result")]
pub enum ValidationResult {
    #[serde(rename = "true")]
    Ok,
    #[serde(rename = "false")]
    Err { message: String, error_code: i32 },
}

impl ValidationResult {
    pub fn is_ok(&self) -> bool {
        matches!(self, ValidationResult::Ok)
    }
}

/// `Tool.ts` `ToolPermissionContext`. The TS type uses `DeepImmutable`
/// from `type-fest`; in Rust we hold owned data and surface immutability
/// through `&` access patterns.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ToolPermissionContext {
    pub mode: String, // PermissionMode (default | bypass | …)
    pub additional_working_directories: HashMap<String, Value>,
    pub always_allow_rules: HashMap<String, Value>,
    pub always_deny_rules: HashMap<String, Value>,
    pub always_ask_rules: HashMap<String, Value>,
    pub is_bypass_permissions_mode_available: bool,
    pub is_auto_mode_available: Option<bool>,
    pub stripped_dangerous_rules: Option<HashMap<String, Value>>,
    pub should_avoid_permission_prompts: Option<bool>,
    pub await_automated_checks_before_dialog: Option<bool>,
    pub pre_plan_mode: Option<String>,
}

/// `Tool.ts` `getEmptyToolPermissionContext`.
pub fn get_empty_tool_permission_context() -> ToolPermissionContext {
    ToolPermissionContext {
        mode: "default".to_string(),
        ..Default::default()
    }
}

/// `Tool.ts` `CompactProgressEvent`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CompactProgressEvent {
    HooksStart { hook_type: String },
    CompactStart,
    CompactEnd,
}

/// `Tool.ts` `Progress` (union of tool progress and hook progress payloads).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum Progress {
    Tool(Value),
    Hook(Value),
}

/// `Tool.ts` `ToolProgress`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolProgress {
    #[serde(rename = "toolUseID")]
    pub tool_use_id: String,
    pub data: Value,
}

/// `Tool.ts` `filterToolProgressMessages` — drops `hook_progress` entries.
pub fn filter_tool_progress_messages(progress_messages: Vec<Value>) -> Vec<Value> {
    progress_messages
        .into_iter()
        .filter(|m| {
            m.get("data")
                .and_then(|d| d.get("type"))
                .and_then(|t| t.as_str())
                != Some("hook_progress")
        })
        .collect()
}

/// `Tool.ts` `ToolResult` — value-side mirror to the TS structural type.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolResultValue {
    pub data: Value,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub new_messages: Vec<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_meta: Option<Value>,
}

/// `Tool.ts` `toolMatchesName` — checks primary name + aliases.
pub fn tool_matches_name(name: &str, aliases: &[String], candidate: &str) -> bool {
    name == candidate || aliases.iter().any(|a| a == candidate)
}

/// `Tool.ts` `findToolByName` — runs `tool_matches_name` over a slice of
/// (name, aliases) pairs.
pub fn find_tool_by_name<'a>(
    tools: &'a [(String, Vec<String>)],
    candidate: &str,
) -> Option<&'a (String, Vec<String>)> {
    tools
        .iter()
        .find(|(name, aliases)| tool_matches_name(name, aliases, candidate))
}

/// `Tool.ts` `ToolDef` — partial tool definition used by `buildTool`.
#[derive(Debug, Clone, Default)]
pub struct ToolDef {
    pub name: String,
    pub aliases: Vec<String>,
    pub description: String,
    pub max_result_size_chars: usize,
    pub is_enabled: Option<bool>,
    pub is_read_only: Option<bool>,
    pub is_concurrency_safe: Option<bool>,
    pub is_destructive: Option<bool>,
    pub user_facing_name: Option<String>,
    pub to_auto_classifier_input: Option<String>,
    pub input_schema: Option<ToolInputJSONSchema>,
    pub extra: HashMap<String, Value>,
}

/// `Tool.ts` `BuiltTool` — fully resolved tool definition with defaults
/// populated. Same shape as `ToolDef` but all defaultable fields are filled.
#[derive(Debug, Clone)]
pub struct BuiltTool {
    pub name: String,
    pub aliases: Vec<String>,
    pub description: String,
    pub max_result_size_chars: usize,
    pub is_enabled: bool,
    pub is_read_only: bool,
    pub is_concurrency_safe: bool,
    pub is_destructive: bool,
    pub user_facing_name: String,
    pub to_auto_classifier_input: String,
    pub input_schema: ToolInputJSONSchema,
    pub extra: HashMap<String, Value>,
}

/// `Tool.ts` `SetToolJSXFn` — callback that owns tool-induced UI state.
pub type SetToolJSXFn = Box<dyn Fn(Option<Value>) + Send + Sync + 'static>;

/// `Tool.ts` `ToolCallProgress` — callback receiving incremental progress.
pub type ToolCallProgress = Box<dyn Fn(ToolProgress) + Send + Sync + 'static>;

/// `Tool.ts` `AnyObject` — generic JSON-object shape (the TS variant is
/// `z.ZodType<{ [key: string]: unknown }>`).
pub type AnyObject = serde_json::Map<String, Value>;

/// `Tool.ts` `Tools = readonly Tool[]` — a slice of tool definitions. The Rust
/// equivalent is a vector of built tools; consumers typically pass `&[BuiltTool]`
/// but `Vec<BuiltTool>` is the owned form. We expose both names.
pub type Tools = Vec<BuiltTool>;

// Sub-module to re-publish `ToolUseContext` as a public alias (the file already
// imports `mossen_types::ToolUseContext` by the same name, so we cannot shadow
// it at module scope — wrap the alias in a nested module).
pub mod tool_use_context_alias {
    /// Local `ToolUseContext` alias pointing at the value-type holding the
    /// fields the TS `Tool.ts` `ToolUseContext` carries.
    pub type ToolUseContext = super::ToolUseContextValue;
}

/// `Tool.ts` `ToolUseContext` — structural mirror of the runtime context
/// passed to tool calls. Holds JSON-serialisable bits; concrete handles
/// (file caches, hook routers) are wired by the agent runtime.
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolUseContextValue {
    pub options: Value,
    pub agent_id: Option<String>,
    pub agent_type: Option<String>,
    pub messages: Vec<Value>,
    pub permission_context: ToolPermissionContext,
    pub query_tracking: Option<QueryChainTracking>,
    pub user_modified: bool,
    pub require_can_use_tool: bool,
    pub tool_use_id: Option<String>,
}

/// `Tool.ts` `buildTool` — applies TOOL_DEFAULTS spread over `def`.
pub fn build_tool(def: ToolDef) -> BuiltTool {
    BuiltTool {
        user_facing_name: def
            .user_facing_name
            .clone()
            .unwrap_or_else(|| def.name.clone()),
        name: def.name,
        aliases: def.aliases,
        description: def.description,
        max_result_size_chars: if def.max_result_size_chars == 0 {
            128 * 1024
        } else {
            def.max_result_size_chars
        },
        is_enabled: def.is_enabled.unwrap_or(true),
        is_read_only: def.is_read_only.unwrap_or(false),
        is_concurrency_safe: def.is_concurrency_safe.unwrap_or(false),
        is_destructive: def.is_destructive.unwrap_or(false),
        to_auto_classifier_input: def.to_auto_classifier_input.unwrap_or_default(),
        input_schema: def
            .input_schema
            .unwrap_or_else(ToolInputJSONSchema::new_object),
        extra: def.extra,
    }
}

#[cfg(test)]
mod tests {
    use super::{Tool, ToolRegistry, ToolResult, ToolType};
    use async_trait::async_trait;
    use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};
    use serde_json::Value;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use tokio::sync::oneshot;
    use tokio_util::sync::CancellationToken;

    struct DropFlag(Arc<AtomicBool>);

    impl Drop for DropFlag {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    struct BlockingTool {
        started: Mutex<Option<oneshot::Sender<()>>>,
        dropped: Arc<AtomicBool>,
    }

    #[async_trait]
    impl Tool for BlockingTool {
        fn name(&self) -> &str {
            "Blocking"
        }

        fn description(&self) -> &str {
            "Blocks until cancelled"
        }

        fn tool_type(&self) -> ToolType {
            ToolType::Builtin
        }

        fn definition(&self) -> ToolDefinition {
            ToolDefinition {
                name: self.name().to_string(),
                description: self.description().to_string(),
                input_schema: ToolInputSchema {
                    schema_type: "object".to_string(),
                    properties: None,
                    required: None,
                    extra: HashMap::new(),
                },
                cache_control: None,
            }
        }

        async fn execute(
            &self,
            _input: Value,
            _context: &ToolUseContext,
        ) -> anyhow::Result<ToolResult> {
            let _drop_flag = DropFlag(self.dropped.clone());
            if let Some(started) = self.started.lock().expect("started lock").take() {
                let _ = started.send(());
            }
            std::future::pending::<()>().await;
            unreachable!("pending future should be dropped by cancellation")
        }
    }

    #[tokio::test]
    async fn execute_with_cancel_drops_in_flight_tool_future() {
        let dropped = Arc::new(AtomicBool::new(false));
        let (started_tx, started_rx) = oneshot::channel();
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(BlockingTool {
            started: Mutex::new(Some(started_tx)),
            dropped: dropped.clone(),
        }));
        let registry = Arc::new(registry);
        let cancel = CancellationToken::new();
        let context = ToolUseContext {
            cwd: ".".to_string(),
            additional_working_directories: None,
            extra: HashMap::new(),
        };

        let task = {
            let registry = registry.clone();
            let cancel = cancel.clone();
            tokio::spawn(async move {
                registry
                    .execute_with_cancel("Blocking", serde_json::json!({}), &context, &cancel)
                    .await
            })
        };

        started_rx.await.expect("blocking tool started");
        cancel.cancel();
        let result = tokio::time::timeout(std::time::Duration::from_millis(250), task)
            .await
            .expect("cancelled tool returned")
            .expect("task joined");

        assert!(result.is_err());
        assert!(dropped.load(Ordering::SeqCst));
    }
}
