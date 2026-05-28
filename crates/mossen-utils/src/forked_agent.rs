//! Helper for running forked agent query loops with usage tracking.
//!
//! This utility ensures forked agents:
//! 1. Share identical cache-critical params with the parent to guarantee prompt cache hits
//! 2. Track full usage metrics across the entire query loop
//! 3. Log metrics via the mossen_fork_agent_query event when complete
//! 4. Isolate mutable state to prevent interference with the main agent loop

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Parameters that must be identical between the fork and parent API requests
/// to share the parent's prompt cache.
#[derive(Debug, Clone)]
pub struct CacheSafeParams {
    /// System prompt - must match parent for cache hits
    pub system_prompt: SystemPrompt,
    /// User context - prepended to messages, affects cache
    pub user_context: std::collections::HashMap<String, String>,
    /// System context - appended to system prompt, affects cache
    pub system_context: std::collections::HashMap<String, String>,
    /// Tool use context containing tools, model, and other options
    pub tool_use_context: ToolUseContext,
    /// Parent context messages for prompt cache sharing
    pub fork_context_messages: Vec<Message>,
}

/// Slot written by handleStopHooks after each turn so post-turn forks
/// can share the main loop's prompt cache.
static LAST_CACHE_SAFE_PARAMS: Mutex<Option<CacheSafeParams>> = Mutex::new(None);

pub fn save_cache_safe_params(params: Option<CacheSafeParams>) {
    let mut guard = LAST_CACHE_SAFE_PARAMS
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    *guard = params;
}

pub fn get_last_cache_safe_params() -> Option<CacheSafeParams> {
    let guard = LAST_CACHE_SAFE_PARAMS
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    guard.clone()
}

/// Parameters for running a forked agent query loop.
#[derive(Clone)]
pub struct ForkedAgentParams {
    /// Messages to start the forked query loop with
    pub prompt_messages: Vec<Message>,
    /// Cache-safe parameters that must match the parent query
    pub cache_safe_params: CacheSafeParams,
    /// Permission check function for the forked agent
    pub can_use_tool: Arc<dyn Fn(&str) -> bool + Send + Sync>,
    /// Source identifier for tracking
    pub query_source: String,
    /// Label for analytics (e.g., 'session_memory', 'supervisor')
    pub fork_label: String,
    /// Optional overrides for the subagent context
    pub overrides: Option<SubagentContextOverrides>,
    /// Optional cap on output tokens
    pub max_output_tokens: Option<u32>,
    /// Optional cap on number of turns (API round-trips)
    pub max_turns: Option<u32>,
    /// Optional callback invoked for each message as it arrives
    pub on_message: Option<Arc<dyn Fn(&Message) + Send + Sync>>,
    /// Skip sidechain transcript recording
    pub skip_transcript: bool,
    /// Skip writing new prompt cache entries on the last message
    pub skip_cache_write: bool,
}

/// Result from a forked agent query loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForkedAgentResult {
    /// All messages yielded during the query loop
    pub messages: Vec<Message>,
    /// Accumulated usage across all API calls in the loop
    pub total_usage: NonNullableUsage,
}

/// Non-nullable usage tracking.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NonNullableUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub service_tier: Option<String>,
    pub cache_creation: CacheCreation,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheCreation {
    pub ephemeral_1h_input_tokens: u64,
    pub ephemeral_5m_input_tokens: u64,
}

/// Accumulates usage from one turn into the total.
pub fn accumulate_usage(total: &mut NonNullableUsage, turn: &NonNullableUsage) {
    total.input_tokens += turn.input_tokens;
    total.output_tokens += turn.output_tokens;
    total.cache_read_input_tokens += turn.cache_read_input_tokens;
    total.cache_creation_input_tokens += turn.cache_creation_input_tokens;
    total.cache_creation.ephemeral_1h_input_tokens += turn.cache_creation.ephemeral_1h_input_tokens;
    total.cache_creation.ephemeral_5m_input_tokens += turn.cache_creation.ephemeral_5m_input_tokens;
}

/// System prompt placeholder type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemPrompt {
    pub content: String,
}

/// Message type placeholder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub uuid: Uuid,
    #[serde(rename = "type")]
    pub msg_type: String,
    pub content: serde_json::Value,
    #[serde(default)]
    pub is_sidechain: bool,
}

/// Tool use context for subagents.
#[derive(Debug, Clone)]
pub struct ToolUseContext {
    pub options: ToolUseOptions,
    pub messages: Vec<Message>,
    pub agent_id: String,
    pub agent_type: Option<String>,
    pub query_tracking: Option<QueryTracking>,
    pub read_file_state: Arc<Mutex<FileStateCache>>,
    pub abort_controller: Arc<AbortController>,
}

#[derive(Debug, Clone)]
pub struct ToolUseOptions {
    pub agent_definitions: AgentDefinitions,
    pub thinking_config: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct AgentDefinitions {
    pub active_agents: Vec<AgentDefinition>,
}

#[derive(Debug, Clone)]
pub struct AgentDefinition {
    pub agent_type: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct QueryTracking {
    pub chain_id: Uuid,
    pub depth: u32,
}

#[derive(Debug, Clone, Default)]
pub struct FileStateCache {
    entries: std::collections::HashMap<String, Vec<u8>>,
}

impl FileStateCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn clone_cache(&self) -> Self {
        Self {
            entries: self.entries.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AbortController {
    aborted: Arc<std::sync::atomic::AtomicBool>,
}

impl AbortController {
    pub fn new() -> Self {
        Self {
            aborted: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    pub fn abort(&self) {
        self.aborted
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn is_aborted(&self) -> bool {
        self.aborted.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn child(&self) -> Self {
        // Child controller linked to parent
        Self {
            aborted: self.aborted.clone(),
        }
    }
}

impl Default for AbortController {
    fn default() -> Self {
        Self::new()
    }
}

/// Options for creating a subagent context.
#[derive(Debug, Clone, Default)]
pub struct SubagentContextOverrides {
    pub options: Option<ToolUseOptions>,
    pub agent_id: Option<String>,
    pub agent_type: Option<String>,
    pub messages: Option<Vec<Message>>,
    pub read_file_state: Option<Arc<Mutex<FileStateCache>>>,
    pub abort_controller: Option<Arc<AbortController>>,
    pub share_set_app_state: bool,
    pub share_set_response_length: bool,
    pub share_abort_controller: bool,
    pub critical_system_reminder_experimental: Option<String>,
    pub require_can_use_tool: Option<bool>,
    pub content_replacement_state: Option<ContentReplacementState>,
}

/// Content replacement state for forked agents.
#[derive(Debug, Clone)]
pub struct ContentReplacementState {
    pub replacements: std::collections::HashMap<String, String>,
}

impl ContentReplacementState {
    pub fn new() -> Self {
        Self {
            replacements: std::collections::HashMap::new(),
        }
    }

    pub fn clone_state(&self) -> Self {
        Self {
            replacements: self.replacements.clone(),
        }
    }
}

impl Default for ContentReplacementState {
    fn default() -> Self {
        Self::new()
    }
}

/// Clones the content replacement state.
pub fn clone_content_replacement_state(state: &ContentReplacementState) -> ContentReplacementState {
    state.clone_state()
}

/// Result from preparing a forked command context.
#[derive(Debug, Clone)]
pub struct PreparedForkedContext {
    /// Skill content with args replaced
    pub skill_content: String,
    /// The general-purpose agent to use
    pub base_agent: AgentDefinition,
    /// Initial prompt messages
    pub prompt_messages: Vec<Message>,
}

/// Creates CacheSafeParams from a REPL hook context.
pub fn create_cache_safe_params(
    system_prompt: SystemPrompt,
    user_context: std::collections::HashMap<String, String>,
    system_context: std::collections::HashMap<String, String>,
    tool_use_context: ToolUseContext,
    messages: Vec<Message>,
) -> CacheSafeParams {
    CacheSafeParams {
        system_prompt,
        user_context,
        system_context,
        tool_use_context,
        fork_context_messages: messages,
    }
}

/// Creates a modified tool permission context that adds allowed tools.
pub fn create_allowed_tools_set(
    existing_tools: &[String],
    additional_tools: &[String],
) -> Vec<String> {
    if additional_tools.is_empty() {
        return existing_tools.to_vec();
    }
    let mut set: HashSet<String> = existing_tools.iter().cloned().collect();
    for tool in additional_tools {
        set.insert(tool.clone());
    }
    set.into_iter().collect()
}

/// Prepares the context for executing a forked command/skill.
pub async fn prepare_forked_command_context(
    skill_content: String,
    _allowed_tools: &[String],
    agent_type_name: Option<&str>,
    agents: &[AgentDefinition],
) -> Result<PreparedForkedContext, anyhow::Error> {
    let agent_type = agent_type_name.unwrap_or("general-purpose");

    let base_agent = agents
        .iter()
        .find(|a| a.agent_type == agent_type)
        .or_else(|| agents.iter().find(|a| a.agent_type == "general-purpose"))
        .or_else(|| agents.first())
        .ok_or_else(|| anyhow::anyhow!("No agent available for forked execution"))?
        .clone();

    let prompt_messages = vec![Message {
        uuid: Uuid::new_v4(),
        msg_type: "user".to_string(),
        content: serde_json::json!({ "content": skill_content }),
        is_sidechain: false,
    }];

    Ok(PreparedForkedContext {
        skill_content,
        base_agent,
        prompt_messages,
    })
}

/// Extracts result text from agent messages.
pub fn extract_result_text(agent_messages: &[Message], default_text: &str) -> String {
    let last_assistant = agent_messages
        .iter()
        .rev()
        .find(|m| m.msg_type == "assistant");

    match last_assistant {
        Some(msg) => {
            if let Some(content) = msg.content.get("content") {
                if let Some(text) = content.as_str() {
                    if !text.is_empty() {
                        return text.to_string();
                    }
                }
                if let Some(arr) = content.as_array() {
                    let texts: Vec<&str> = arr
                        .iter()
                        .filter_map(|block| {
                            if block.get("type")?.as_str()? == "text" {
                                block.get("text")?.as_str()
                            } else {
                                None
                            }
                        })
                        .collect();
                    let joined = texts.join("\n");
                    if !joined.is_empty() {
                        return joined;
                    }
                }
            }
            default_text.to_string()
        }
        None => default_text.to_string(),
    }
}

/// Creates an isolated ToolUseContext for subagents.
pub fn create_subagent_context(
    parent_context: &ToolUseContext,
    overrides: Option<&SubagentContextOverrides>,
) -> ToolUseContext {
    let abort_controller = match overrides {
        Some(o) if o.abort_controller.is_some() => o.abort_controller.clone().unwrap(),
        Some(o) if o.share_abort_controller => parent_context.abort_controller.clone(),
        _ => Arc::new(parent_context.abort_controller.child()),
    };

    let read_file_state = match overrides {
        Some(o) if o.read_file_state.is_some() => o.read_file_state.clone().unwrap(),
        _ => {
            let parent_state = parent_context
                .read_file_state
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            Arc::new(Mutex::new(parent_state.clone_cache()))
        }
    };

    let options = match overrides {
        Some(o) if o.options.is_some() => o.options.clone().unwrap(),
        _ => parent_context.options.clone(),
    };

    let messages = match overrides {
        Some(o) if o.messages.is_some() => o.messages.clone().unwrap(),
        _ => parent_context.messages.clone(),
    };

    let agent_id = match overrides {
        Some(o) if o.agent_id.is_some() => o.agent_id.clone().unwrap(),
        _ => Uuid::new_v4().to_string(),
    };

    let agent_type = match overrides {
        Some(o) => o.agent_type.clone(),
        None => None,
    };

    let query_tracking = Some(QueryTracking {
        chain_id: Uuid::new_v4(),
        depth: parent_context
            .query_tracking
            .as_ref()
            .map(|qt| qt.depth + 1)
            .unwrap_or(0),
    });

    ToolUseContext {
        options,
        messages,
        agent_id,
        agent_type,
        query_tracking,
        read_file_state,
        abort_controller,
    }
}

/// Runs a forked agent query loop and tracks cache hit metrics.
pub async fn run_forked_agent(
    params: ForkedAgentParams,
) -> Result<ForkedAgentResult, anyhow::Error> {
    let start_time = Instant::now();
    let output_messages: Vec<Message> = Vec::new();
    let total_usage = NonNullableUsage::default();

    let isolated_context = create_subagent_context(
        &params.cache_safe_params.tool_use_context,
        params.overrides.as_ref(),
    );

    let mut initial_messages = params.cache_safe_params.fork_context_messages.clone();
    initial_messages.extend(params.prompt_messages.clone());

    let _agent_id = if params.skip_transcript {
        None
    } else {
        Some(format!("{}-{}", params.fork_label, Uuid::new_v4()))
    };

    // In a real implementation, this would run the query loop.
    // Here we represent the structure without the actual query engine.
    tracing::debug!(
        fork_label = %params.fork_label,
        "Forked agent started with {} initial messages",
        initial_messages.len()
    );

    // Cleanup
    {
        let mut state = isolated_context
            .read_file_state
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        state.clear();
    }

    let duration_ms = start_time.elapsed().as_millis() as u64;

    // Log the fork query metrics
    log_fork_agent_query_event(
        &params.fork_label,
        &params.query_source,
        duration_ms,
        output_messages.len(),
        &total_usage,
        params
            .cache_safe_params
            .tool_use_context
            .query_tracking
            .as_ref(),
    );

    Ok(ForkedAgentResult {
        messages: output_messages,
        total_usage,
    })
}

/// Logs the mossen_fork_agent_query event with full NonNullableUsage fields.
fn log_fork_agent_query_event(
    fork_label: &str,
    query_source: &str,
    duration_ms: u64,
    message_count: usize,
    total_usage: &NonNullableUsage,
    query_tracking: Option<&QueryTracking>,
) {
    let total_input_tokens = total_usage.input_tokens
        + total_usage.cache_creation_input_tokens
        + total_usage.cache_read_input_tokens;

    let cache_hit_rate = if total_input_tokens > 0 {
        total_usage.cache_read_input_tokens as f64 / total_input_tokens as f64
    } else {
        0.0
    };

    tracing::info!(
        fork_label = %fork_label,
        query_source = %query_source,
        duration_ms = duration_ms,
        message_count = message_count,
        input_tokens = total_usage.input_tokens,
        output_tokens = total_usage.output_tokens,
        cache_read_input_tokens = total_usage.cache_read_input_tokens,
        cache_creation_input_tokens = total_usage.cache_creation_input_tokens,
        cache_hit_rate = cache_hit_rate,
        query_chain_id = ?query_tracking.map(|qt| qt.chain_id),
        query_depth = ?query_tracking.map(|qt| qt.depth),
        "mossen_fork_agent_query"
    );
}

/// Parse tool list from CLI format.
pub fn parse_tool_list_from_cli(allowed_tools: &[String]) -> Vec<String> {
    allowed_tools
        .iter()
        .flat_map(|s| s.split(','))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// 对应 TS `createGetAppStateWithAllowedTools`：返回一个新的 `get_app_state`
/// 闭包，其内部 app state 的 `allowedTools` 字段被替换为给定列表。
///
/// Rust 端 `AppState` 由 `serde_json::Value` 携带，调用方传入原始 getter +
/// 想要覆盖的工具名列表，返回新闭包。
pub fn create_get_app_state_with_allowed_tools(
    original: std::sync::Arc<dyn Fn() -> serde_json::Value + Send + Sync>,
    allowed_tools: Vec<String>,
) -> std::sync::Arc<dyn Fn() -> serde_json::Value + Send + Sync> {
    std::sync::Arc::new(move || {
        let mut state = original();
        if let Some(obj) = state.as_object_mut() {
            obj.insert(
                "allowedTools".to_string(),
                serde_json::json!(allowed_tools.clone()),
            );
        }
        state
    })
}
