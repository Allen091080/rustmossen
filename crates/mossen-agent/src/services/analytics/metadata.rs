//! Analytics metadata — event enrichment with environment, session, and user context.

use std::collections::HashMap;
use std::env;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Enriched event metadata type alias.
pub type EventMetadata = HashMap<String, Value>;

/// Base metadata fields collected for every event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseMetadata {
    pub session_id: String,
    pub user_id: String,
    pub device_id: String,
    pub platform: String,
    pub app_version: String,
    pub node_version: String,
    pub os_version: String,
    pub arch: String,
    pub timestamp_ms: u64,
    pub user_type: Option<String>,
    pub organization_uuid: Option<String>,
    pub account_uuid: Option<String>,
    pub subscription_type: Option<String>,
    pub rate_limit_tier: Option<String>,
    pub model: Option<String>,
    pub query_source: Option<String>,
    pub is_non_interactive: bool,
}

impl BaseMetadata {
    /// Collect base metadata from the current environment.
    pub fn collect() -> Self {
        Self {
            session_id: String::new(),
            user_id: String::new(),
            device_id: String::new(),
            platform: env::consts::OS.to_string(),
            app_version: env::var("MOSSEN_VERSION").unwrap_or_else(|_| "unknown".to_string()),
            node_version: String::new(),
            os_version: String::new(),
            arch: env::consts::ARCH.to_string(),
            timestamp_ms: chrono::Utc::now().timestamp_millis() as u64,
            user_type: env::var("USER_TYPE").ok(),
            organization_uuid: None,
            account_uuid: None,
            subscription_type: None,
            rate_limit_tier: None,
            model: None,
            query_source: None,
            is_non_interactive: false,
        }
    }

    /// Convert to event metadata map.
    pub fn to_metadata(&self) -> EventMetadata {
        let mut map = EventMetadata::new();
        map.insert("session_id".to_string(), Value::String(self.session_id.clone()));
        map.insert("user_id".to_string(), Value::String(self.user_id.clone()));
        map.insert("device_id".to_string(), Value::String(self.device_id.clone()));
        map.insert("platform".to_string(), Value::String(self.platform.clone()));
        map.insert("app_version".to_string(), Value::String(self.app_version.clone()));
        map.insert("arch".to_string(), Value::String(self.arch.clone()));
        map.insert("timestamp_ms".to_string(), Value::Number(serde_json::Number::from(self.timestamp_ms)));
        map.insert("is_non_interactive".to_string(), Value::Bool(self.is_non_interactive));
        if let Some(ref v) = self.user_type { map.insert("user_type".to_string(), Value::String(v.clone())); }
        if let Some(ref v) = self.model { map.insert("model".to_string(), Value::String(v.clone())); }
        if let Some(ref v) = self.query_source { map.insert("query_source".to_string(), Value::String(v.clone())); }
        map
    }
}

/// Merge base metadata with event-specific metadata.
pub fn merge_metadata(base: &EventMetadata, event_specific: &EventMetadata) -> EventMetadata {
    let mut merged = base.clone();
    for (k, v) in event_specific {
        merged.insert(k.clone(), v.clone());
    }
    merged
}

// ---------------------------------------------------------------------------
// TS-mirror functions — `services/analytics/metadata.ts` exports.
// ---------------------------------------------------------------------------

/// Marker type alias for verifying analytics metadata is not raw code or
/// filepaths (mirrors the TS `never`-based marker).
pub type AnalyticsMetadataVerified = String;

/// `services/analytics/metadata.ts` `sanitizeToolNameForAnalytics`.
/// MCP tools (`mcp__server__tool`) are redacted to `mcp_tool` to avoid
/// exposing user-specific configurations.
pub fn sanitize_tool_name_for_analytics(tool_name: &str) -> String {
    if tool_name.starts_with("mcp__") {
        "mcp_tool".to_string()
    } else {
        tool_name.to_string()
    }
}

/// `services/analytics/metadata.ts` `isToolDetailsLoggingEnabled` — gated by
/// `OTEL_LOG_TOOL_DETAILS` env truthiness.
pub fn is_tool_details_logging_enabled() -> bool {
    matches!(
        env::var("OTEL_LOG_TOOL_DETAILS").as_deref(),
        Ok("1" | "true" | "TRUE" | "yes" | "YES")
    )
}

/// `services/analytics/metadata.ts` `isAnalyticsToolDetailsLoggingEnabled`.
/// Enables MCP server/tool names for: cowork (local-agent entrypoint),
/// hosted-proxy transports, and official-registry-matching base URLs.
pub fn is_analytics_tool_details_logging_enabled(
    mcp_server_type: Option<&str>,
    mcp_server_base_url: Option<&str>,
) -> bool {
    if env::var("MOSSEN_CODE_ENTRYPOINT").as_deref() == Ok("local-agent") {
        return true;
    }
    if mcp_server_type == Some("hosted-proxy") {
        return true;
    }
    if let Some(url) = mcp_server_base_url {
        if is_official_mcp_url(url) {
            return true;
        }
    }
    false
}

/// Local helper — mirrors `services/mcp/officialRegistry.ts` `isOfficialMcpUrl`
/// at the minimal granularity needed here (Mossen-controlled subdomains).
fn is_official_mcp_url(url: &str) -> bool {
    let lower = url.to_lowercase();
    lower.contains("mcp.mossen.ai")
        || lower.contains("mcp.mossen.com")
        || lower.contains("api.mossen.ai/mcp/")
}

/// MCP tool details extracted from `mcp__server__tool` names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpToolDetails {
    pub server_name: String,
    pub mcp_tool_name: String,
}

/// `services/analytics/metadata.ts` `extractMcpToolDetails`.
pub fn extract_mcp_tool_details(tool_name: &str) -> Option<McpToolDetails> {
    if !tool_name.starts_with("mcp__") {
        return None;
    }
    let parts: Vec<&str> = tool_name.split("__").collect();
    if parts.len() < 3 {
        return None;
    }
    let server = parts[1];
    let rest = parts[2..].join("__");
    if server.is_empty() || rest.is_empty() {
        return None;
    }
    Some(McpToolDetails {
        server_name: server.to_string(),
        mcp_tool_name: rest,
    })
}

/// Pair returned by `mcpToolDetailsForAnalytics`.
#[derive(Debug, Default, Clone)]
pub struct McpToolDetailsForAnalytics {
    pub mcp_server_name: Option<String>,
    pub mcp_tool_name: Option<String>,
}

/// `services/analytics/metadata.ts` `mcpToolDetailsForAnalytics`.
/// Returns names when the gate (built-in server / analytics-enabled) passes;
/// otherwise both fields are `None`.
pub fn mcp_tool_details_for_analytics(
    tool_name: &str,
    mcp_server_type: Option<&str>,
    mcp_server_base_url: Option<&str>,
) -> McpToolDetailsForAnalytics {
    let Some(details) = extract_mcp_tool_details(tool_name) else {
        return McpToolDetailsForAnalytics::default();
    };
    if !is_builtin_mcp_server(&details.server_name)
        && !is_analytics_tool_details_logging_enabled(mcp_server_type, mcp_server_base_url)
    {
        return McpToolDetailsForAnalytics::default();
    }
    McpToolDetailsForAnalytics {
        mcp_server_name: Some(details.server_name),
        mcp_tool_name: Some(details.mcp_tool_name),
    }
}

/// Built-in first-party MCP servers — names are reserved strings, so logging
/// them is not PII. The TS variant is feature-gated; here we just match the
/// known reserved names.
fn is_builtin_mcp_server(name: &str) -> bool {
    matches!(name, "computer-use" | "mossen-builtin")
}

/// `services/analytics/metadata.ts` `extractSkillName`.
pub fn extract_skill_name(tool_name: &str, input: &Value) -> Option<String> {
    if tool_name != "Skill" {
        return None;
    }
    input
        .get("skill")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

const TOOL_INPUT_STRING_TRUNCATE_AT: usize = 512;
const TOOL_INPUT_STRING_TRUNCATE_TO: usize = 128;
const TOOL_INPUT_MAX_JSON_CHARS: usize = 4 * 1024;
const TOOL_INPUT_MAX_COLLECTION_ITEMS: usize = 20;
const TOOL_INPUT_MAX_DEPTH: u8 = 2;

fn truncate_tool_input_value(value: &Value, depth: u8) -> Value {
    match value {
        Value::String(s) => {
            if s.len() > TOOL_INPUT_STRING_TRUNCATE_AT {
                Value::String(format!(
                    "{}…[{} chars]",
                    &s[..TOOL_INPUT_STRING_TRUNCATE_TO.min(s.len())],
                    s.len()
                ))
            } else {
                Value::String(s.clone())
            }
        }
        Value::Array(arr) => {
            if depth >= TOOL_INPUT_MAX_DEPTH {
                return Value::String("<nested>".to_string());
            }
            let mut mapped: Vec<Value> = arr
                .iter()
                .take(TOOL_INPUT_MAX_COLLECTION_ITEMS)
                .map(|v| truncate_tool_input_value(v, depth + 1))
                .collect();
            if arr.len() > TOOL_INPUT_MAX_COLLECTION_ITEMS {
                mapped.push(Value::String(format!("…[{} items]", arr.len())));
            }
            Value::Array(mapped)
        }
        Value::Object(map) => {
            if depth >= TOOL_INPUT_MAX_DEPTH {
                return Value::String("<nested>".to_string());
            }
            let entries: Vec<(&String, &Value)> =
                map.iter().filter(|(k, _)| !k.starts_with('_')).collect();
            let mut out = serde_json::Map::new();
            for (k, v) in entries.iter().take(TOOL_INPUT_MAX_COLLECTION_ITEMS) {
                out.insert((*k).clone(), truncate_tool_input_value(v, depth + 1));
            }
            if entries.len() > TOOL_INPUT_MAX_COLLECTION_ITEMS {
                out.insert(
                    "…".to_string(),
                    Value::String(format!("{} keys", entries.len())),
                );
            }
            Value::Object(out)
        }
        other => other.clone(),
    }
}

/// `services/analytics/metadata.ts` `extractToolInputForTelemetry`.
pub fn extract_tool_input_for_telemetry(input: &Value) -> Option<String> {
    if !is_tool_details_logging_enabled() {
        return None;
    }
    let truncated = truncate_tool_input_value(input, 0);
    let mut json = serde_json::to_string(&truncated).unwrap_or_default();
    if json.len() > TOOL_INPUT_MAX_JSON_CHARS {
        json.truncate(TOOL_INPUT_MAX_JSON_CHARS);
        json.push_str("…[truncated]");
    }
    Some(json)
}

const MAX_FILE_EXTENSION_LENGTH: usize = 10;

/// `services/analytics/metadata.ts` `getFileExtensionForAnalytics`.
pub fn get_file_extension_for_analytics(file_path: &str) -> Option<String> {
    let p = std::path::Path::new(file_path);
    let ext = p.extension()?.to_str()?.to_lowercase();
    if ext.is_empty() {
        return None;
    }
    if ext.len() > MAX_FILE_EXTENSION_LENGTH {
        return Some("other".to_string());
    }
    Some(ext)
}

const FILE_COMMANDS: &[&str] = &[
    "rm", "mv", "cp", "touch", "mkdir", "chmod", "chown", "cat", "head", "tail", "sort", "stat",
    "diff", "wc", "grep", "rg", "sed",
];

/// `services/analytics/metadata.ts` `getFileExtensionsFromBashCommand`.
pub fn get_file_extensions_from_bash_command(
    command: &str,
    simulated_sed_edit_file_path: Option<&str>,
) -> Option<String> {
    if !command.contains('.') && simulated_sed_edit_file_path.is_none() {
        return None;
    }
    let mut result: Option<String> = None;
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    if let Some(p) = simulated_sed_edit_file_path {
        if let Some(ext) = get_file_extension_for_analytics(p) {
            seen.insert(ext.clone());
            result = Some(ext);
        }
    }

    // Split on compound operators &&, ||, ;, |.
    let split_re = regex::Regex::new(r"\s*(?:&&|\|\||[;|])\s*").unwrap();
    let ws_re = regex::Regex::new(r"\s+").unwrap();
    for subcmd in split_re.split(command) {
        if subcmd.is_empty() {
            continue;
        }
        let tokens: Vec<&str> = ws_re.split(subcmd).collect();
        if tokens.len() < 2 {
            continue;
        }
        let first = tokens[0];
        let base = if let Some(idx) = first.rfind('/') {
            &first[idx + 1..]
        } else {
            first
        };
        if !FILE_COMMANDS.contains(&base) {
            continue;
        }
        for arg in &tokens[1..] {
            if arg.starts_with('-') {
                continue;
            }
            if let Some(ext) = get_file_extension_for_analytics(arg) {
                if !seen.contains(&ext) {
                    seen.insert(ext.clone());
                    result = Some(match result {
                        Some(prev) => format!("{},{}", prev, ext),
                        None => ext,
                    });
                }
            }
        }
    }

    result
}

/// `services/analytics/metadata.ts` `EnvContext` — environment context
/// metadata included with every analytics event.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct EnvContext {
    pub platform: String,
    pub platform_raw: String,
    pub arch: String,
    pub node_version: String,
    pub terminal: Option<String>,
    pub package_managers: String,
    pub runtimes: String,
    pub is_running_with_bun: bool,
    pub is_ci: bool,
    pub is_mossenbit: bool,
    pub is_mossen_remote: bool,
    pub is_local_agent_mode: bool,
    pub is_conductor: bool,
    pub remote_environment_type: Option<String>,
    pub coworker_type: Option<String>,
    pub mossen_container_id: Option<String>,
    pub mossen_remote_session_id: Option<String>,
    pub tags: Option<String>,
    pub is_github_action: bool,
    pub is_mossen_action: bool,
    pub is_hosted_auth: bool,
    pub version: String,
    pub version_base: Option<String>,
    pub build_time: String,
    pub deployment_environment: String,
    pub github_event_name: Option<String>,
    pub github_actions_runner_environment: Option<String>,
    pub github_actions_runner_os: Option<String>,
    pub github_action_ref: Option<String>,
    pub wsl_version: Option<String>,
    pub linux_distro_id: Option<String>,
    pub linux_distro_version: Option<String>,
    pub linux_kernel: Option<String>,
    pub vcs: Option<String>,
}

/// `services/analytics/metadata.ts` `ProcessMetrics`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProcessMetrics {
    pub uptime: f64,
    pub rss: u64,
    pub heap_total: u64,
    pub heap_used: u64,
    pub external: u64,
    pub array_buffers: u64,
    pub constrained_memory: Option<u64>,
    pub cpu_user_us: u64,
    pub cpu_system_us: u64,
    pub cpu_percent: Option<f64>,
}

/// `services/analytics/metadata.ts` `EnrichMetadataOptions`.
#[derive(Debug, Clone, Default)]
pub struct EnrichMetadataOptions {
    pub model: Option<String>,
    pub betas: Option<String>,
    pub additional_metadata: HashMap<String, Value>,
}

/// `services/analytics/metadata.ts` `getEventMetadata` — collect the rich
/// EventMetadata snapshot. In the Rust port the network-bound bits (repo
/// remote hash, async distro probes) are omitted; callers can layer them.
pub async fn get_event_metadata(options: EnrichMetadataOptions) -> HashMap<String, Value> {
    let mut map = HashMap::new();
    let model = options
        .model
        .clone()
        .unwrap_or_else(|| "claude-3-5-sonnet".to_string());
    map.insert("model".to_string(), Value::String(model.clone()));
    map.insert(
        "sessionId".to_string(),
        Value::String(env::var("MOSSEN_SESSION_ID").unwrap_or_default()),
    );
    map.insert(
        "userType".to_string(),
        Value::String(env::var("USER_TYPE").unwrap_or_default()),
    );
    if let Some(betas) = options.betas.clone() {
        if !betas.is_empty() {
            map.insert("betas".to_string(), Value::String(betas));
        }
    }
    map.insert(
        "isInteractive".to_string(),
        Value::String(
            env::var("MOSSEN_IS_INTERACTIVE")
                .unwrap_or_else(|_| "true".to_string()),
        ),
    );
    map.insert(
        "clientType".to_string(),
        Value::String(env::var("MOSSEN_CLIENT_TYPE").unwrap_or_else(|_| "cli".to_string())),
    );
    map.insert(
        "sweBenchRunId".to_string(),
        Value::String(env::var("SWE_BENCH_RUN_ID").unwrap_or_default()),
    );
    map.insert(
        "sweBenchInstanceId".to_string(),
        Value::String(env::var("SWE_BENCH_INSTANCE_ID").unwrap_or_default()),
    );
    map.insert(
        "sweBenchTaskId".to_string(),
        Value::String(env::var("SWE_BENCH_TASK_ID").unwrap_or_default()),
    );
    for (k, v) in options.additional_metadata {
        map.insert(k, v);
    }
    map
}

/// `services/analytics/metadata.ts` `FirstPartyEventLoggingCoreMetadata`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct FirstPartyEventLoggingCoreMetadata {
    pub session_id: String,
    pub model: String,
    pub user_type: String,
    pub betas: Option<String>,
    pub entrypoint: Option<String>,
    pub agent_sdk_version: Option<String>,
    pub is_interactive: bool,
    pub client_type: String,
    pub swe_bench_run_id: Option<String>,
    pub swe_bench_instance_id: Option<String>,
    pub swe_bench_task_id: Option<String>,
    pub agent_id: Option<String>,
    pub parent_session_id: Option<String>,
    pub agent_type: Option<String>,
    pub team_name: Option<String>,
}

/// `services/analytics/metadata.ts` `FirstPartyEventLoggingMetadata`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FirstPartyEventLoggingMetadata {
    pub env: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<Value>,
    pub core: FirstPartyEventLoggingCoreMetadata,
    pub additional: HashMap<String, Value>,
}

/// `services/analytics/metadata.ts` `to1PEventFormat` — flatten the runtime
/// EventMetadata into snake-cased 1P proto-compatible shape.
pub fn to_1p_event_format(
    metadata: &HashMap<String, Value>,
    user_metadata: &HashMap<String, Value>,
    additional_metadata: HashMap<String, Value>,
) -> FirstPartyEventLoggingMetadata {
    let env_value = metadata
        .get("envContext")
        .cloned()
        .unwrap_or(Value::Object(Default::default()));
    let core = FirstPartyEventLoggingCoreMetadata {
        session_id: metadata
            .get("sessionId")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        model: metadata
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        user_type: metadata
            .get("userType")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        betas: metadata
            .get("betas")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        entrypoint: metadata
            .get("entrypoint")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        agent_sdk_version: metadata
            .get("agentSdkVersion")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        is_interactive: metadata
            .get("isInteractive")
            .and_then(|v| v.as_str())
            .map(|s| s == "true")
            .unwrap_or(false),
        client_type: metadata
            .get("clientType")
            .and_then(|v| v.as_str())
            .unwrap_or("cli")
            .to_string(),
        swe_bench_run_id: metadata
            .get("sweBenchRunId")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string()),
        swe_bench_instance_id: metadata
            .get("sweBenchInstanceId")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string()),
        swe_bench_task_id: metadata
            .get("sweBenchTaskId")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string()),
        agent_id: metadata
            .get("agentId")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        parent_session_id: metadata
            .get("parentSessionId")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        agent_type: metadata
            .get("agentType")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        team_name: metadata
            .get("teamName")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    };
    let auth = {
        let account = user_metadata.get("accountUuid").and_then(|v| v.as_str());
        let org = user_metadata.get("organizationUuid").and_then(|v| v.as_str());
        if account.is_some() || org.is_some() {
            let mut a = serde_json::Map::new();
            if let Some(v) = account {
                a.insert("account_uuid".to_string(), Value::String(v.to_string()));
            }
            if let Some(v) = org {
                a.insert("organization_uuid".to_string(), Value::String(v.to_string()));
            }
            Some(Value::Object(a))
        } else {
            None
        }
    };
    FirstPartyEventLoggingMetadata {
        env: env_value,
        process: None,
        auth,
        core,
        additional: additional_metadata,
    }
}

/// Marker for analytics metadata that has been verified to NOT contain code
/// or filepaths. Mirrors TS `AnalyticsMetadata_I_VERIFIED_THIS_IS_NOT_CODE_OR_FILEPATHS`
/// (a TS `never` marker type). The Rust port re-exposes the same type name
/// scoped to `metadata` so the export name resolves locally.
#[allow(non_camel_case_types)]
pub enum AnalyticsMetadata_I_VERIFIED_THIS_IS_NOT_CODE_OR_FILEPATHS {}
