//! API request and tool schema utilities.
//!
//! Translates `utils/api.ts` — provides tool schema generation, system prompt
//! splitting, context metrics logging, and tool input normalization.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::string_utils::prefix_chars;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInputSchema {
    #[serde(rename = "type")]
    pub schema_type: Option<String>,
    pub properties: Option<Value>,
    pub required: Option<Vec<String>>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MossenBetaToolWithExtras {
    pub name: String,
    pub description: String,
    pub input_schema: ToolInputSchema,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eager_input_streaming: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheControl {
    #[serde(rename = "type")]
    pub control_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<CacheScope>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CacheScope {
    Global,
    Org,
}

#[derive(Debug, Clone)]
pub struct SystemPromptBlock {
    pub text: String,
    pub cache_scope: Option<CacheScope>,
}

pub type SystemPrompt = Vec<String>;

/// Fields to filter from tool schemas when swarms are not enabled.
static SWARM_FIELDS_BY_TOOL: Lazy<HashMap<&str, Vec<&str>>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert("ExitPlanModeV2", vec!["launchSwarm", "teammateCount"]);
    m.insert("Agent", vec!["name", "team_name", "mode"]);
    m
});

// ---------------------------------------------------------------------------
// Tool schema cache
// ---------------------------------------------------------------------------

static TOOL_SCHEMA_CACHE: Lazy<Mutex<HashMap<String, MossenBetaToolWithExtras>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Get the tool schema cache.
pub fn get_tool_schema_cache() -> &'static Lazy<Mutex<HashMap<String, MossenBetaToolWithExtras>>> {
    &TOOL_SCHEMA_CACHE
}

// ---------------------------------------------------------------------------
// Filter swarm fields
// ---------------------------------------------------------------------------

/// Filter swarm-related fields from a tool's input schema.
pub fn filter_swarm_fields_from_schema(
    tool_name: &str,
    schema: ToolInputSchema,
) -> ToolInputSchema {
    let fields_to_remove = match SWARM_FIELDS_BY_TOOL.get(tool_name) {
        Some(fields) if !fields.is_empty() => fields,
        _ => return schema,
    };

    let mut filtered = schema;
    if let Some(ref mut props) = filtered.properties {
        if let Value::Object(ref mut map) = props {
            for field in fields_to_remove {
                map.remove(*field);
            }
        }
    }

    filtered
}

// ---------------------------------------------------------------------------
// Tool to API schema
// ---------------------------------------------------------------------------

/// Options for converting a tool to its API schema.
#[derive(Debug, Clone)]
pub struct ToolToApiSchemaOptions {
    pub model: Option<String>,
    pub defer_loading: bool,
    pub cache_control: Option<CacheControl>,
    pub strict_tools_enabled: bool,
    pub agent_swarms_enabled: bool,
    pub is_first_party: bool,
    pub is_first_party_base_url: bool,
    pub fgts_enabled: bool,
    pub disable_experimental_betas: bool,
}

/// Tool trait for schema generation.
pub trait ToolSchema {
    fn name(&self) -> &str;
    fn description(&self) -> String;
    fn input_schema(&self) -> ToolInputSchema;
    fn input_json_schema(&self) -> Option<ToolInputSchema>;
    fn strict(&self) -> bool;
    fn is_mcp(&self) -> bool;
}

/// Convert a tool to its API schema representation.
pub fn tool_to_api_schema(
    tool: &dyn ToolSchema,
    options: &ToolToApiSchemaOptions,
) -> MossenBetaToolWithExtras {
    // Build cache key
    let cache_key = if let Some(json_schema) = tool.input_json_schema() {
        format!(
            "{}:{}",
            tool.name(),
            serde_json::to_string(&json_schema).unwrap_or_default()
        )
    } else {
        tool.name().to_string()
    };

    // Check cache
    {
        let cache = TOOL_SCHEMA_CACHE.lock().unwrap();
        if let Some(cached) = cache.get(&cache_key) {
            let mut schema = cached.clone();
            if options.defer_loading {
                schema.defer_loading = Some(true);
            }
            if let Some(ref cc) = options.cache_control {
                schema.cache_control = Some(cc.clone());
            }
            return maybe_strip_betas(schema, options.disable_experimental_betas);
        }
    }

    // Build schema
    let mut input_schema = tool
        .input_json_schema()
        .unwrap_or_else(|| tool.input_schema());

    // Filter swarm fields if not enabled
    if !options.agent_swarms_enabled {
        input_schema = filter_swarm_fields_from_schema(tool.name(), input_schema);
    }

    let mut base = MossenBetaToolWithExtras {
        name: tool.name().to_string(),
        description: tool.description(),
        input_schema,
        strict: None,
        defer_loading: None,
        cache_control: None,
        eager_input_streaming: None,
    };

    // Strict mode
    if options.strict_tools_enabled && tool.strict() {
        if let Some(ref model) = options.model {
            if model_supports_structured_outputs(model) {
                base.strict = Some(true);
            }
        }
    }

    // Fine-grained tool streaming
    if options.is_first_party && options.is_first_party_base_url && options.fgts_enabled {
        base.eager_input_streaming = Some(true);
    }

    // Store in cache
    {
        let mut cache = TOOL_SCHEMA_CACHE.lock().unwrap();
        cache.insert(cache_key, base.clone());
    }

    // Apply per-request overlay
    if options.defer_loading {
        base.defer_loading = Some(true);
    }
    if let Some(ref cc) = options.cache_control {
        base.cache_control = Some(cc.clone());
    }

    maybe_strip_betas(base, options.disable_experimental_betas)
}

fn maybe_strip_betas(
    schema: MossenBetaToolWithExtras,
    disable_betas: bool,
) -> MossenBetaToolWithExtras {
    if !disable_betas {
        return schema;
    }

    // Strip everything except name, description, input_schema, cache_control
    let has_extra = schema.strict.is_some()
        || schema.defer_loading.is_some()
        || schema.eager_input_streaming.is_some();

    if has_extra {
        log_strip_once(&[]);
        MossenBetaToolWithExtras {
            name: schema.name,
            description: schema.description,
            input_schema: schema.input_schema,
            strict: None,
            defer_loading: None,
            cache_control: schema.cache_control,
            eager_input_streaming: None,
        }
    } else {
        schema
    }
}

static LOGGED_STRIP: Lazy<Mutex<bool>> = Lazy::new(|| Mutex::new(false));

fn log_strip_once(_stripped: &[&str]) {
    let mut logged = LOGGED_STRIP.lock().unwrap();
    if *logged {
        return;
    }
    *logged = true;
    tracing::debug!(
        "[betas] Stripped from tool schemas (MOSSEN_CODE_DISABLE_EXPERIMENTAL_BETAS=1)"
    );
}

fn model_supports_structured_outputs(model: &str) -> bool {
    // Models that support structured outputs
    model.contains("mossen-3") || model.contains("mossen-4") || model.contains("balanced")
}

// ---------------------------------------------------------------------------
// System prompt prefix splitting
// ---------------------------------------------------------------------------

/// CLI system prompt prefixes (known static prefixes).
static CLI_SYSPROMPT_PREFIXES: Lazy<HashSet<String>> = Lazy::new(|| {
    // Would be populated from constants/system.ts
    HashSet::new()
});

/// Split system prompt blocks by content type for API matching and cache control.
pub fn split_sys_prompt_prefix(
    system_prompt: &[String],
    skip_global_cache_for_system_prompt: bool,
    use_global_cache_feature: bool,
    dynamic_boundary: &str,
) -> Vec<SystemPromptBlock> {
    if use_global_cache_feature && skip_global_cache_for_system_prompt {
        // Filter out boundary marker, return blocks without global scope
        let mut attribution_header: Option<String> = None;
        let mut system_prompt_prefix: Option<String> = None;
        let mut rest: Vec<String> = Vec::new();

        for prompt in system_prompt {
            if prompt.is_empty() || prompt == dynamic_boundary {
                continue;
            }
            if prompt.starts_with("x-mossen-billing-header") {
                attribution_header = Some(prompt.clone());
            } else if CLI_SYSPROMPT_PREFIXES.contains(prompt) {
                system_prompt_prefix = Some(prompt.clone());
            } else {
                rest.push(prompt.clone());
            }
        }

        let mut result = Vec::new();
        if let Some(header) = attribution_header {
            result.push(SystemPromptBlock {
                text: header,
                cache_scope: None,
            });
        }
        if let Some(prefix) = system_prompt_prefix {
            result.push(SystemPromptBlock {
                text: prefix,
                cache_scope: Some(CacheScope::Org),
            });
        }
        let rest_joined = rest.join("\n\n");
        if !rest_joined.is_empty() {
            result.push(SystemPromptBlock {
                text: rest_joined,
                cache_scope: Some(CacheScope::Org),
            });
        }
        return result;
    }

    if use_global_cache_feature {
        let boundary_index = system_prompt.iter().position(|s| s == dynamic_boundary);

        if let Some(boundary_idx) = boundary_index {
            let mut attribution_header: Option<String> = None;
            let mut system_prompt_prefix: Option<String> = None;
            let mut static_blocks: Vec<String> = Vec::new();
            let mut dynamic_blocks: Vec<String> = Vec::new();

            for (i, block) in system_prompt.iter().enumerate() {
                if block.is_empty() || block == dynamic_boundary {
                    continue;
                }
                if block.starts_with("x-mossen-billing-header") {
                    attribution_header = Some(block.clone());
                } else if CLI_SYSPROMPT_PREFIXES.contains(block) {
                    system_prompt_prefix = Some(block.clone());
                } else if i < boundary_idx {
                    static_blocks.push(block.clone());
                } else {
                    dynamic_blocks.push(block.clone());
                }
            }

            let mut result = Vec::new();
            if let Some(header) = attribution_header {
                result.push(SystemPromptBlock {
                    text: header,
                    cache_scope: None,
                });
            }
            if let Some(prefix) = system_prompt_prefix {
                result.push(SystemPromptBlock {
                    text: prefix,
                    cache_scope: None,
                });
            }
            let static_joined = static_blocks.join("\n\n");
            if !static_joined.is_empty() {
                result.push(SystemPromptBlock {
                    text: static_joined,
                    cache_scope: Some(CacheScope::Global),
                });
            }
            let dynamic_joined = dynamic_blocks.join("\n\n");
            if !dynamic_joined.is_empty() {
                result.push(SystemPromptBlock {
                    text: dynamic_joined,
                    cache_scope: None,
                });
            }
            return result;
        }
    }

    // Default mode: org-level caching
    let mut attribution_header: Option<String> = None;
    let mut system_prompt_prefix: Option<String> = None;
    let mut rest: Vec<String> = Vec::new();

    for block in system_prompt {
        if block.is_empty() {
            continue;
        }
        if block.starts_with("x-mossen-billing-header") {
            attribution_header = Some(block.clone());
        } else if CLI_SYSPROMPT_PREFIXES.contains(block) {
            system_prompt_prefix = Some(block.clone());
        } else {
            rest.push(block.clone());
        }
    }

    let mut result = Vec::new();
    if let Some(header) = attribution_header {
        result.push(SystemPromptBlock {
            text: header,
            cache_scope: None,
        });
    }
    if let Some(prefix) = system_prompt_prefix {
        result.push(SystemPromptBlock {
            text: prefix,
            cache_scope: Some(CacheScope::Org),
        });
    }
    let rest_joined = rest.join("\n\n");
    if !rest_joined.is_empty() {
        result.push(SystemPromptBlock {
            text: rest_joined,
            cache_scope: Some(CacheScope::Org),
        });
    }
    result
}

/// Log stats about first block for analyzing prefix matching config.
pub fn log_api_prefix(system_prompt: &[String], dynamic_boundary: &str) {
    let blocks = split_sys_prompt_prefix(system_prompt, false, false, dynamic_boundary);
    if let Some(first) = blocks.first() {
        let snippet = prefix_chars(&first.text, 20);
        let hash = {
            let mut hasher = Sha256::new();
            hasher.update(first.text.as_bytes());
            hex::encode(hasher.finalize())
        };
        tracing::debug!(
            "sysprompt_block: snippet={}, length={}, hash={}",
            snippet,
            first.text.len(),
            hash
        );
    }
}

// ---------------------------------------------------------------------------
// Context helpers
// ---------------------------------------------------------------------------

/// Append system context entries to the system prompt.
pub fn append_system_context(
    system_prompt: &[String],
    context: &HashMap<String, String>,
) -> Vec<String> {
    let mut result: Vec<String> = system_prompt
        .iter()
        .filter(|s| !s.is_empty())
        .cloned()
        .collect();

    let context_str: String = context
        .iter()
        .map(|(k, v)| format!("{}: {}", k, v))
        .collect::<Vec<_>>()
        .join("\n");

    if !context_str.is_empty() {
        result.push(context_str);
    }

    result
}

/// Prepend user context as a system-reminder message.
pub fn prepend_user_context_text(context: &HashMap<String, String>) -> Option<String> {
    if context.is_empty() {
        return None;
    }

    let context_body = context
        .iter()
        .map(|(k, v)| format!("# {}\n{}", k, v))
        .collect::<Vec<_>>()
        .join("\n");

    Some(format!(
        "<system-reminder>\nAs you answer the user's questions, you can use the following context:\n{}\n\n      IMPORTANT: this context may or may not be relevant to your tasks. You should not respond to this context unless it is highly relevant to your task.\n</system-reminder>\n",
        context_body
    ))
}

// ---------------------------------------------------------------------------
// Tool input normalization
// ---------------------------------------------------------------------------

/// Normalized bash tool input.
#[derive(Debug, Clone)]
pub struct NormalizedBashInput {
    pub command: String,
    pub description: Option<String>,
    pub timeout: Option<u64>,
    pub run_in_background: Option<bool>,
}

/// Normalize tool input based on tool name.
pub fn normalize_tool_input(tool_name: &str, input: Value, cwd: &str, platform: &str) -> Value {
    match tool_name {
        "Bash" | "bash" => normalize_bash_input(input, cwd, platform),
        "FileEdit" | "file_edit" => normalize_file_edit_input(input),
        "FileWrite" | "file_write" => normalize_file_write_input(input),
        "TaskOutput" | "task_output" => normalize_task_output_input(input),
        "ExitPlanModeV2" => normalize_exit_plan_mode_input(input),
        _ => input,
    }
}

fn normalize_bash_input(mut input: Value, cwd: &str, platform: &str) -> Value {
    if let Some(command) = input.get("command").and_then(|v| v.as_str()) {
        let mut normalized = command.replace(&format!("cd {} && ", cwd), "");
        if platform == "windows" {
            let posix_cwd = cwd.replace('\\', "/");
            normalized = normalized.replace(&format!("cd {} && ", posix_cwd), "");
        }
        // Replace \\; with \;
        normalized = normalized.replace("\\\\;", "\\;");

        input["command"] = Value::String(normalized);
    }
    input
}

fn normalize_file_edit_input(mut input: Value) -> Value {
    // Strip trailing whitespace from old_string and new_string
    if let Some(old) = input.get("old_string").and_then(|v| v.as_str()) {
        input["old_string"] = Value::String(old.to_string());
    }
    if let Some(new) = input.get("new_string").and_then(|v| v.as_str()) {
        input["new_string"] = Value::String(new.to_string());
    }
    input
}

fn normalize_file_write_input(mut input: Value) -> Value {
    if let Some(file_path) = input.get("file_path").and_then(|v| v.as_str()) {
        let is_markdown = file_path.ends_with(".md") || file_path.ends_with(".mdx");
        if !is_markdown {
            if let Some(content) = input.get("content").and_then(|v| v.as_str()) {
                let stripped = strip_trailing_whitespace(content);
                input["content"] = Value::String(stripped);
            }
        }
    }
    input
}

fn normalize_task_output_input(input: Value) -> Value {
    // Normalize legacy parameter names
    let task_id = input
        .get("task_id")
        .or_else(|| input.get("agentId"))
        .or_else(|| input.get("bash_id"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let timeout = input
        .get("timeout")
        .and_then(|v| v.as_u64())
        .or_else(|| {
            input
                .get("wait_up_to")
                .and_then(|v| v.as_u64())
                .map(|w| w * 1000)
        })
        .unwrap_or(30000);

    let block = input.get("block").and_then(|v| v.as_bool()).unwrap_or(true);

    serde_json::json!({
        "task_id": task_id,
        "block": block,
        "timeout": timeout,
    })
}

fn normalize_exit_plan_mode_input(input: Value) -> Value {
    // Would inject plan content and file path
    input
}

/// Strip fields added by normalize_tool_input before sending to API.
pub fn normalize_tool_input_for_api(tool_name: &str, input: Value) -> Value {
    match tool_name {
        "ExitPlanModeV2" => {
            if let Value::Object(ref map) = input {
                if map.contains_key("plan") || map.contains_key("planFilePath") {
                    let mut result = map.clone();
                    result.remove("plan");
                    result.remove("planFilePath");
                    return Value::Object(result);
                }
            }
            input
        }
        "FileEdit" | "file_edit" => {
            if let Value::Object(ref map) = input {
                if map.contains_key("edits") {
                    let mut result = map.clone();
                    result.remove("old_string");
                    result.remove("new_string");
                    result.remove("replace_all");
                    return Value::Object(result);
                }
            }
            input
        }
        _ => input,
    }
}

// ---------------------------------------------------------------------------
// Context metrics
// ---------------------------------------------------------------------------

/// Context metrics for logging.
#[derive(Debug, Clone)]
pub struct ContextMetrics {
    pub git_status_size: usize,
    pub mossen_md_size: usize,
    pub total_context_size: usize,
    pub project_file_count_rounded: u64,
    pub mcp_tools_count: usize,
    pub mcp_servers_count: usize,
    pub mcp_tools_tokens: usize,
    pub non_mcp_tools_count: usize,
    pub non_mcp_tools_tokens: usize,
}

/// Compute context metrics from available data.
pub fn compute_context_metrics(
    git_status: Option<&str>,
    mossen_md: Option<&str>,
    mcp_tools: &[Value],
    non_mcp_tools: &[Value],
    file_count: u64,
) -> ContextMetrics {
    let git_status_size = git_status.map(|s| s.len()).unwrap_or(0);
    let mossen_md_size = mossen_md.map(|s| s.len()).unwrap_or(0);
    let total_context_size = git_status_size + mossen_md_size;

    // Extract unique server names from MCP tool names
    let mut server_names = HashSet::new();
    for tool in mcp_tools {
        if let Some(name) = tool.get("name").and_then(|v| v.as_str()) {
            let parts: Vec<&str> = name.split("__").collect();
            if parts.len() >= 3 {
                if let Some(server) = parts.get(1) {
                    server_names.insert(server.to_string());
                }
            }
        }
    }

    // Estimate tokens
    let mcp_tools_tokens: usize = mcp_tools
        .iter()
        .map(|t| rough_token_count_estimation(&serde_json::to_string(t).unwrap_or_default()))
        .sum();

    let non_mcp_tools_tokens: usize = non_mcp_tools
        .iter()
        .map(|t| rough_token_count_estimation(&serde_json::to_string(t).unwrap_or_default()))
        .sum();

    ContextMetrics {
        git_status_size,
        mossen_md_size,
        total_context_size,
        project_file_count_rounded: file_count,
        mcp_tools_count: mcp_tools.len(),
        mcp_servers_count: server_names.len(),
        mcp_tools_tokens,
        non_mcp_tools_count: non_mcp_tools.len(),
        non_mcp_tools_tokens,
    }
}

/// Rough token count estimation (chars / 4).
fn rough_token_count_estimation(text: &str) -> usize {
    text.len() / 4
}

/// Strip trailing whitespace from each line.
fn strip_trailing_whitespace(content: &str) -> String {
    content
        .lines()
        .map(|line| line.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
}

/// 对应 TS `prependUserContext`：在消息列表前追加一条 meta system-reminder。
pub fn prepend_user_context(
    messages: Vec<serde_json::Value>,
    context: std::collections::HashMap<String, String>,
) -> Vec<serde_json::Value> {
    if context.is_empty() {
        return messages;
    }
    if std::env::var("NODE_ENV").as_deref() == Ok("test") {
        return messages;
    }
    let entries = context
        .iter()
        .map(|(k, v)| format!("# {}\n{}", k, v))
        .collect::<Vec<_>>()
        .join("\n");
    let reminder = format!(
        "<system-reminder>\nAs you answer the user's questions, you can use the following context:\n{}\n\n      IMPORTANT: this context may or may not be relevant to your tasks. You should not respond to this context unless it is highly relevant to your task.\n</system-reminder>\n",
        entries
    );
    let user_msg = serde_json::json!({
        "role": "user",
        "content": reminder,
        "isMeta": true,
    });
    let mut out = Vec::with_capacity(messages.len() + 1);
    out.push(user_msg);
    out.extend(messages);
    out
}

/// Analytics sink 回调签名。
///
/// `event_name`/`payload` 与 TS `logEventTo1P(eventName, payload)` 完全对齐。
/// 真正发送由 `mossen-agent::services::analytics::first_party_event_logger::log_event_to_1p`
/// 实现；utils 无法直接 import agent（架构分层），所以在启动时由 cli/agent
/// 注入此 sink。
pub type AnalyticsSink = std::sync::Arc<
    dyn Fn(&str, std::collections::HashMap<String, serde_json::Value>) + Send + Sync + 'static,
>;

static ANALYTICS_SINK: once_cell::sync::OnceCell<std::sync::RwLock<Option<AnalyticsSink>>> =
    once_cell::sync::OnceCell::new();

fn sink_cell() -> &'static std::sync::RwLock<Option<AnalyticsSink>> {
    ANALYTICS_SINK.get_or_init(|| std::sync::RwLock::new(None))
}

/// 注册 analytics sink。由 agent 在启动时调用，传入 `log_event_to_1p` 的轻包装。
pub fn set_analytics_sink(sink: AnalyticsSink) {
    if let Ok(mut g) = sink_cell().write() {
        *g = Some(sink);
    }
}

/// 清除 analytics sink（仅供测试用）。
pub fn clear_analytics_sink() {
    if let Ok(mut g) = sink_cell().write() {
        *g = None;
    }
}

/// 内部：把事件发往已注册的 sink。
fn emit_analytics(event_name: &str, payload: std::collections::HashMap<String, serde_json::Value>) {
    if let Ok(g) = sink_cell().read() {
        if let Some(ref s) = *g {
            s(event_name, payload);
        }
    }
}

/// 对应 TS `logContextMetrics`：上报上下文/系统 prompt 体积。
///
/// 真正的事件投递通过 [`set_analytics_sink`] 注入的 sink 完成（agent 启动时
/// 注册到 `mossen-agent::services::analytics::first_party_event_logger::log_event_to_1p`）。
/// sink 未注册时仍打 tracing 日志，便于早期 bootstrap 阶段不丢观测点。
pub async fn log_context_metrics(
    git_status_size: usize,
    mossen_md_size: usize,
    file_count_rounded: usize,
) {
    let total = git_status_size + mossen_md_size;
    tracing::info!(
        target = "api.context_metrics",
        git_status_size,
        mossen_md_size,
        total_context_size = total,
        file_count_rounded,
        "mossen_context_metrics",
    );

    let mut payload = std::collections::HashMap::new();
    payload.insert(
        "gitStatusSize".to_string(),
        serde_json::Value::from(git_status_size),
    );
    payload.insert(
        "mossenMdSize".to_string(),
        serde_json::Value::from(mossen_md_size),
    );
    payload.insert(
        "totalContextSize".to_string(),
        serde_json::Value::from(total),
    );
    payload.insert(
        "fileCount".to_string(),
        serde_json::Value::from(file_count_rounded),
    );
    emit_analytics("mossen_context_metrics", payload);
}
