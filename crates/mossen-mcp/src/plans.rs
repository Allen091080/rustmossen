//! # plans — `/mcp add` / `mcp install` / `mcp template` 安装计划
//!
//! 对应 TypeScript:
//! - `services/mcp/slashAddPlan.ts`
//! - `services/mcp/remoteInstallPlan.ts`
//! - `services/mcp/builtinTemplatePlan.ts`
//! - `services/mcp/builtinTemplates.ts`
//!
//! 这些文件共享同一种模式：先用 `get_*_plan` 生成 token + 计划 JSON，
//! 用户确认后再 `execute_*_plan(token)` 真正写入配置。Plan 有 TTL
//! （默认 10 分钟），过期或未知 token 会失败。
//!
//! Rust 端用全局 `Mutex<HashMap<token, Plan>>` 实现存储。`execute_*_plan`
//! 接受一个写入 closure（避免直接依赖配置层），这样测试可以注入 in-memory
//! 后端。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};

// ---------------------------------------------------------------------------
// 常量
// ---------------------------------------------------------------------------

/// `slashAddPlan.ts` `MCP_SLASH_ADD_PLAN_TOKEN_TTL_MS`。
pub const MCP_SLASH_ADD_PLAN_TOKEN_TTL_MS: u64 = 10 * 60 * 1000;

/// `remoteInstallPlan.ts` `MCP_REMOTE_PLAN_TOKEN_TTL_MS`。
pub const MCP_REMOTE_PLAN_TOKEN_TTL_MS: u64 = 10 * 60 * 1000;

/// `builtinTemplatePlan.ts` `MCP_TEMPLATE_PLAN_TOKEN_TTL_MS`。
pub const MCP_TEMPLATE_PLAN_TOKEN_TTL_MS: u64 = 10 * 60 * 1000;

// ---------------------------------------------------------------------------
// 共用工具
// ---------------------------------------------------------------------------

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn random_token() -> String {
    // 8 hex chars from 4 bytes — matches TS `randomBytes(4).toString('hex')`.
    use rand_simple::rand;
    let r = rand();
    format!("{:08x}", r)
}

mod rand_simple {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static SEED: AtomicU64 = AtomicU64::new(0);

    pub fn rand() -> u32 {
        let mut s = SEED.load(Ordering::Relaxed);
        if s == 0 {
            s = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0xdeadbeef);
        }
        // xorshift64
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        SEED.store(s, Ordering::Relaxed);
        (s as u32) ^ ((s >> 32) as u32)
    }
}

// ---------------------------------------------------------------------------
// SLASH ADD PLAN — /mcp add ...
// ---------------------------------------------------------------------------

/// `slashAddPlan.ts` `McpSlashAddWritableScope`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum McpSlashAddWritableScope {
    Local,
    User,
    Project,
}

/// `slashAddPlan.ts` `McpSlashAddTransport`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum McpSlashAddTransport {
    Stdio,
    Sse,
    Http,
}

/// `slashAddPlan.ts` `McpSlashAddPlan`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpSlashAddPlan {
    pub token: String,
    pub created_at: u64,
    pub server_name: String,
    pub scope: McpSlashAddWritableScope,
    pub transport: McpSlashAddTransport,
    pub config: JsonValue,
}

/// `slashAddPlan.ts` `McpSlashAddPlanError`。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum McpSlashAddPlanError {
    MissingServerName,
    MissingCommand,
    InvalidScope { scope: Option<String> },
    InvalidTransport { transport: Option<String> },
    InvalidEnv { message: String },
    InvalidHeader { message: String },
    InvalidConfig { reason: String },
    UnknownToken { token: String },
    ExpiredToken { token: String },
    InstallFailed { message: String },
}

/// `slashAddPlan.ts` `McpSlashAddPlanResult`。
pub type McpSlashAddPlanResult = Result<McpSlashAddPlan, McpSlashAddPlanError>;

/// `getMcpSlashAddPlan` 输入参数。
#[derive(Debug, Clone, Default)]
pub struct GetSlashAddPlanInput {
    pub server_name: Option<String>,
    pub scope: Option<String>,
    pub transport: Option<String>,
    pub command_or_url: Option<String>,
    pub args: Option<Vec<String>>,
    pub env: Option<Vec<String>>,
    pub headers: Option<Vec<String>>,
}

fn normalize_scope(s: Option<&str>) -> Option<McpSlashAddWritableScope> {
    match s {
        None | Some("") | Some("local") => Some(McpSlashAddWritableScope::Local),
        Some("user") => Some(McpSlashAddWritableScope::User),
        Some("project") => Some(McpSlashAddWritableScope::Project),
        _ => None,
    }
}

fn normalize_transport(s: Option<&str>) -> Option<McpSlashAddTransport> {
    match s {
        None | Some("") | Some("stdio") => Some(McpSlashAddTransport::Stdio),
        Some("sse") => Some(McpSlashAddTransport::Sse),
        Some("http") => Some(McpSlashAddTransport::Http),
        _ => None,
    }
}

fn parse_env_vars(env: Option<&Vec<String>>) -> Result<HashMap<String, String>, String> {
    let mut out = HashMap::new();
    let Some(list) = env else { return Ok(out) };
    for kv in list {
        let Some(eq_idx) = kv.find('=') else {
            return Err(format!(
                "Invalid env var format: \"{}\". Expected KEY=value",
                kv
            ));
        };
        let key = kv[..eq_idx].trim().to_string();
        let value = kv[eq_idx + 1..].to_string();
        if key.is_empty() {
            return Err(format!("Env var name cannot be empty: \"{}\"", kv));
        }
        out.insert(key, value);
    }
    Ok(out)
}

/// `slashAddPlan.ts` `getMcpSlashAddPlan`。
pub fn get_mcp_slash_add_plan(opts: GetSlashAddPlanInput) -> McpSlashAddPlanResult {
    prune_slash_add_plans(now_ms());

    let server_name = match opts.server_name.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => s.to_string(),
        None => return Err(McpSlashAddPlanError::MissingServerName),
    };

    let scope = match normalize_scope(opts.scope.as_deref()) {
        Some(s) => s,
        None => {
            return Err(McpSlashAddPlanError::InvalidScope {
                scope: opts.scope.clone(),
            })
        }
    };

    let transport = match normalize_transport(opts.transport.as_deref()) {
        Some(t) => t,
        None => {
            return Err(McpSlashAddPlanError::InvalidTransport {
                transport: opts.transport.clone(),
            })
        }
    };

    let command_or_url = match opts.command_or_url.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => s.to_string(),
        None => return Err(McpSlashAddPlanError::MissingCommand),
    };

    let config = match transport {
        McpSlashAddTransport::Stdio => {
            let env = parse_env_vars(opts.env.as_ref())
                .map_err(|m| McpSlashAddPlanError::InvalidEnv { message: m })?;
            let mut obj = serde_json::Map::new();
            obj.insert("type".into(), JsonValue::String("stdio".into()));
            obj.insert("command".into(), JsonValue::String(command_or_url.clone()));
            obj.insert(
                "args".into(),
                JsonValue::Array(
                    opts.args
                        .clone()
                        .unwrap_or_default()
                        .into_iter()
                        .map(JsonValue::String)
                        .collect(),
                ),
            );
            if !env.is_empty() {
                obj.insert(
                    "env".into(),
                    JsonValue::Object(
                        env.into_iter()
                            .map(|(k, v)| (k, JsonValue::String(v)))
                            .collect(),
                    ),
                );
            }
            JsonValue::Object(obj)
        }
        McpSlashAddTransport::Sse | McpSlashAddTransport::Http => {
            let headers = match opts.headers.as_ref() {
                None => None,
                Some(h) if h.is_empty() => None,
                Some(h) => Some(
                    crate::utils::parse_headers(h)
                        .map_err(|m| McpSlashAddPlanError::InvalidHeader { message: m })?,
                ),
            };
            let mut obj = serde_json::Map::new();
            obj.insert(
                "type".into(),
                JsonValue::String(match transport {
                    McpSlashAddTransport::Sse => "sse".into(),
                    McpSlashAddTransport::Http => "http".into(),
                    _ => unreachable!(),
                }),
            );
            obj.insert("url".into(), JsonValue::String(command_or_url.clone()));
            if let Some(h) = headers {
                obj.insert(
                    "headers".into(),
                    JsonValue::Object(
                        h.into_iter()
                            .map(|(k, v)| (k, JsonValue::String(v)))
                            .collect(),
                    ),
                );
            }
            JsonValue::Object(obj)
        }
    };

    // Validate config — light schema check.
    validate_server_config(&config)
        .map_err(|m| McpSlashAddPlanError::InvalidConfig { reason: m })?;

    let token = new_slash_token();
    let plan = McpSlashAddPlan {
        token: token.clone(),
        created_at: now_ms(),
        server_name,
        scope,
        transport,
        config,
    };
    slash_store().lock().unwrap().insert(token, plan.clone());
    Ok(plan)
}

fn validate_server_config(c: &JsonValue) -> Result<(), String> {
    let ty = c
        .get("type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "type is required".to_string())?;
    match ty {
        "stdio" => {
            let cmd = c
                .get("command")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "command is required for stdio".to_string())?;
            if cmd.is_empty() {
                return Err("command must not be empty".into());
            }
        }
        "sse" | "http" | "ws" => {
            let url = c
                .get("url")
                .and_then(|v| v.as_str())
                .ok_or_else(|| format!("url is required for {}", ty))?;
            if url.is_empty() {
                return Err(format!("url must not be empty for {}", ty));
            }
        }
        other => return Err(format!("unsupported transport: {}", other)),
    }
    Ok(())
}

fn slash_store() -> &'static Mutex<HashMap<String, McpSlashAddPlan>> {
    static S: OnceLock<Mutex<HashMap<String, McpSlashAddPlan>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashMap::new()))
}

fn prune_slash_add_plans(now: u64) {
    let mut s = slash_store().lock().unwrap();
    s.retain(|_, p| now - p.created_at <= MCP_SLASH_ADD_PLAN_TOKEN_TTL_MS);
}

fn new_slash_token() -> String {
    loop {
        let t = random_token();
        let store = slash_store().lock().unwrap();
        if !store.contains_key(&t) {
            return t;
        }
    }
}

/// `slashAddPlan.ts` `executeMcpSlashAddPlan`。
pub async fn execute_mcp_slash_add_plan<F, Fut>(
    token: &str,
    install: F,
) -> McpSlashAddPlanResult
where
    F: FnOnce(McpSlashAddPlan) -> Fut,
    Fut: std::future::Future<Output = Result<(), String>>,
{
    prune_slash_add_plans(now_ms());
    let plan = {
        let mut s = slash_store().lock().unwrap();
        s.remove(token)
    };
    let plan = match plan {
        None => {
            return Err(McpSlashAddPlanError::UnknownToken {
                token: token.to_string(),
            })
        }
        Some(p) => p,
    };
    if now_ms() - plan.created_at > MCP_SLASH_ADD_PLAN_TOKEN_TTL_MS {
        return Err(McpSlashAddPlanError::ExpiredToken {
            token: token.to_string(),
        });
    }
    match install(plan.clone()).await {
        Ok(()) => Ok(plan),
        Err(m) => Err(McpSlashAddPlanError::InstallFailed { message: m }),
    }
}

/// `slashAddPlan.ts` `_resetMcpSlashAddPlanStoreForTesting`。
pub fn _reset_mcp_slash_add_plan_store_for_testing() {
    slash_store().lock().unwrap().clear();
}

// ---------------------------------------------------------------------------
// REMOTE INSTALL PLAN — mossen mcp install <remote>
// ---------------------------------------------------------------------------

/// `remoteInstallPlan.ts` `McpRemoteWritableScope`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum McpRemoteWritableScope {
    Local,
    User,
    Project,
}

/// `remoteInstallPlan.ts` `McpRemoteInstallPlan`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpRemoteInstallPlan {
    pub token: String,
    pub created_at: u64,
    pub identifier: String,
    pub server_name: String,
    pub scope: McpRemoteWritableScope,
    pub config: JsonValue,
}

/// `remoteInstallPlan.ts` `McpRemotePlanError`。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum McpRemotePlanError {
    MissingIdentifier,
    InvalidScope { scope: Option<String> },
    LookupFailed { message: String },
    ServerNotFound { identifier: String },
    NoCompatibleConfig { identifier: String },
    InvalidConfig { reason: String },
    UnknownToken { token: String },
    ExpiredToken { token: String },
    InstallFailed { message: String },
}

pub type McpRemoteInstallResult = Result<McpRemoteInstallPlan, McpRemotePlanError>;

fn remote_store() -> &'static Mutex<HashMap<String, McpRemoteInstallPlan>> {
    static S: OnceLock<Mutex<HashMap<String, McpRemoteInstallPlan>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashMap::new()))
}

fn prune_remote_plans(now: u64) {
    let mut s = remote_store().lock().unwrap();
    s.retain(|_, p| now - p.created_at <= MCP_REMOTE_PLAN_TOKEN_TTL_MS);
}

fn normalize_remote_scope(s: Option<&str>) -> Option<McpRemoteWritableScope> {
    match s {
        None | Some("") | Some("local") => Some(McpRemoteWritableScope::Local),
        Some("user") => Some(McpRemoteWritableScope::User),
        Some("project") => Some(McpRemoteWritableScope::Project),
        _ => None,
    }
}

/// `getMcpRemoteInstallPlan` 输入。
#[derive(Debug, Clone, Default)]
pub struct GetRemoteInstallPlanInput {
    pub identifier: Option<String>,
    pub scope: Option<String>,
}

/// `remoteInstallPlan.ts` `getMcpRemoteInstallPlan`。
///
/// `lookup` 接受一个 server identifier，异步返回 `(server_name, config_json)`
/// 或错误描述。Rust 端避免直接依赖 officialRegistry，由调用方注入。
pub async fn get_mcp_remote_install_plan<F, Fut>(
    opts: GetRemoteInstallPlanInput,
    lookup: F,
) -> McpRemoteInstallResult
where
    F: FnOnce(String) -> Fut,
    Fut: std::future::Future<Output = Result<Option<(String, JsonValue)>, String>>,
{
    prune_remote_plans(now_ms());
    let identifier = match opts.identifier.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => s.to_string(),
        None => return Err(McpRemotePlanError::MissingIdentifier),
    };
    let scope = match normalize_remote_scope(opts.scope.as_deref()) {
        Some(s) => s,
        None => {
            return Err(McpRemotePlanError::InvalidScope {
                scope: opts.scope.clone(),
            })
        }
    };
    let lookup_result = lookup(identifier.clone())
        .await
        .map_err(|m| McpRemotePlanError::LookupFailed { message: m })?;
    let (server_name, config) = match lookup_result {
        Some(pair) => pair,
        None => {
            return Err(McpRemotePlanError::ServerNotFound { identifier })
        }
    };
    validate_server_config(&config)
        .map_err(|m| McpRemotePlanError::InvalidConfig { reason: m })?;

    let token = new_remote_token();
    let plan = McpRemoteInstallPlan {
        token: token.clone(),
        created_at: now_ms(),
        identifier,
        server_name,
        scope,
        config,
    };
    remote_store().lock().unwrap().insert(token, plan.clone());
    Ok(plan)
}

fn new_remote_token() -> String {
    loop {
        let t = random_token();
        let store = remote_store().lock().unwrap();
        if !store.contains_key(&t) {
            return t;
        }
    }
}

/// `remoteInstallPlan.ts` `executeMcpRemoteInstallPlan`。
pub async fn execute_mcp_remote_install_plan<F, Fut>(
    token: &str,
    install: F,
) -> McpRemoteInstallResult
where
    F: FnOnce(McpRemoteInstallPlan) -> Fut,
    Fut: std::future::Future<Output = Result<(), String>>,
{
    prune_remote_plans(now_ms());
    let plan = {
        let mut s = remote_store().lock().unwrap();
        s.remove(token)
    };
    let plan = match plan {
        None => {
            return Err(McpRemotePlanError::UnknownToken {
                token: token.to_string(),
            })
        }
        Some(p) => p,
    };
    if now_ms() - plan.created_at > MCP_REMOTE_PLAN_TOKEN_TTL_MS {
        return Err(McpRemotePlanError::ExpiredToken {
            token: token.to_string(),
        });
    }
    match install(plan.clone()).await {
        Ok(()) => Ok(plan),
        Err(m) => Err(McpRemotePlanError::InstallFailed { message: m }),
    }
}

/// `remoteInstallPlan.ts` `_resetMcpRemotePlanStoreForTesting`。
pub fn _reset_mcp_remote_plan_store_for_testing() {
    remote_store().lock().unwrap().clear();
}

// ---------------------------------------------------------------------------
// BUILTIN TEMPLATE PLAN
// ---------------------------------------------------------------------------

/// `builtinTemplatePlan.ts` `McpTemplateWritableScope`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum McpTemplateWritableScope {
    Local,
    User,
    Project,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum McpTemplatePlanError {
    MissingTemplate,
    UnknownTemplate { name: String },
    InvalidScope { scope: Option<String> },
    MissingParameters { missing: Vec<String> },
    UnknownToken { token: String },
    ExpiredToken { token: String },
    InstallFailed { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTemplateInstallPlan {
    pub token: String,
    pub created_at: u64,
    pub template_name: String,
    pub server_name: String,
    pub scope: McpTemplateWritableScope,
    pub config: JsonValue,
}

pub type McpTemplateInstallResult = Result<McpTemplateInstallPlan, McpTemplatePlanError>;

fn template_store() -> &'static Mutex<HashMap<String, McpTemplateInstallPlan>> {
    static S: OnceLock<Mutex<HashMap<String, McpTemplateInstallPlan>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashMap::new()))
}

fn prune_template_plans(now: u64) {
    let mut s = template_store().lock().unwrap();
    s.retain(|_, p| now - p.created_at <= MCP_TEMPLATE_PLAN_TOKEN_TTL_MS);
}

fn new_template_token() -> String {
    loop {
        let t = random_token();
        let store = template_store().lock().unwrap();
        if !store.contains_key(&t) {
            return t;
        }
    }
}

fn normalize_template_scope(s: Option<&str>) -> Option<McpTemplateWritableScope> {
    match s {
        None | Some("") | Some("local") => Some(McpTemplateWritableScope::Local),
        Some("user") => Some(McpTemplateWritableScope::User),
        Some("project") => Some(McpTemplateWritableScope::Project),
        _ => None,
    }
}

#[derive(Debug, Clone, Default)]
pub struct GetTemplateInstallPlanInput {
    pub template_name: Option<String>,
    pub server_name: Option<String>,
    pub scope: Option<String>,
    pub root: Option<String>,
    pub db: Option<String>,
}

/// `builtinTemplatePlan.ts` `getMcpTemplateInstallPlan`。
pub fn get_mcp_template_install_plan(opts: GetTemplateInstallPlanInput) -> McpTemplateInstallResult {
    prune_template_plans(now_ms());

    let template_name = match opts.template_name.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => s.to_string(),
        None => return Err(McpTemplatePlanError::MissingTemplate),
    };
    let scope = match normalize_template_scope(opts.scope.as_deref()) {
        Some(s) => s,
        None => {
            return Err(McpTemplatePlanError::InvalidScope {
                scope: opts.scope.clone(),
            })
        }
    };

    let template = match get_builtin_mcp_template(&template_name) {
        Some(t) => t,
        None => {
            return Err(McpTemplatePlanError::UnknownTemplate {
                name: template_name,
            })
        }
    };

    let (config, missing) = instantiate_builtin_mcp_template(
        &template,
        InstantiateParams {
            root: opts.root.clone(),
            db: opts.db.clone(),
        },
    );
    if !missing.is_empty() {
        return Err(McpTemplatePlanError::MissingParameters {
            missing: missing.into_iter().map(|m| m.to_string()).collect(),
        });
    }
    let config = config.expect("instantiate returned no config but no missing params");

    let server_name = opts
        .server_name
        .unwrap_or_else(|| template_name.clone());
    let token = new_template_token();
    let plan = McpTemplateInstallPlan {
        token: token.clone(),
        created_at: now_ms(),
        template_name: template_name.clone(),
        server_name,
        scope,
        config,
    };
    template_store().lock().unwrap().insert(token, plan.clone());
    Ok(plan)
}

/// `builtinTemplatePlan.ts` `executeMcpTemplateInstallPlan`。
pub async fn execute_mcp_template_install_plan<F, Fut>(
    token: &str,
    install: F,
) -> McpTemplateInstallResult
where
    F: FnOnce(McpTemplateInstallPlan) -> Fut,
    Fut: std::future::Future<Output = Result<(), String>>,
{
    prune_template_plans(now_ms());
    let plan = {
        let mut s = template_store().lock().unwrap();
        s.remove(token)
    };
    let plan = match plan {
        None => {
            return Err(McpTemplatePlanError::UnknownToken {
                token: token.to_string(),
            })
        }
        Some(p) => p,
    };
    if now_ms() - plan.created_at > MCP_TEMPLATE_PLAN_TOKEN_TTL_MS {
        return Err(McpTemplatePlanError::ExpiredToken {
            token: token.to_string(),
        });
    }
    match install(plan.clone()).await {
        Ok(()) => Ok(plan),
        Err(m) => Err(McpTemplatePlanError::InstallFailed { message: m }),
    }
}

/// `builtinTemplatePlan.ts` `_resetMcpTemplatePlanStoreForTesting`。
pub fn _reset_mcp_template_plan_store_for_testing() {
    template_store().lock().unwrap().clear();
}

// ---------------------------------------------------------------------------
// BUILTIN TEMPLATES — 5 个静态模板
// ---------------------------------------------------------------------------

/// `builtinTemplates.ts` `BuiltinMcpTemplateRisk`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BuiltinMcpTemplateRisk {
    Low,
    Medium,
}

/// `builtinTemplates.ts` `BuiltinMcpTemplateParameter`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BuiltinMcpTemplateParameter {
    Root,
    Db,
}

impl BuiltinMcpTemplateParameter {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Root => "root",
            Self::Db => "db",
        }
    }
}

impl std::fmt::Display for BuiltinMcpTemplateParameter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// `builtinTemplates.ts` `BuiltinMcpTemplate`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuiltinMcpTemplate {
    pub name: String,
    pub title: String,
    pub description: String,
    pub config: JsonValue,
    pub parameters: Vec<BuiltinMcpTemplateParameter>,
    pub default_enabled: bool,
    pub read_only: bool,
    pub requires_credentials: bool,
    pub requires_network: bool,
    pub risk: BuiltinMcpTemplateRisk,
    pub notes: Vec<String>,
}

fn builtin_templates() -> Vec<BuiltinMcpTemplate> {
    vec![
        BuiltinMcpTemplate {
            name: "filesystem-readonly".into(),
            title: "Filesystem readonly".into(),
            description: "Template for a local filesystem MCP server scoped to explicit read-only roots.".into(),
            config: json!({
                "type": "stdio",
                "command": "mcp-server-filesystem",
                "args": ["--readonly", "<absolute-project-root>"],
            }),
            parameters: vec![BuiltinMcpTemplateParameter::Root],
            default_enabled: false,
            read_only: true,
            requires_credentials: false,
            requires_network: false,
            risk: BuiltinMcpTemplateRisk::Low,
            notes: vec![
                "User must replace <absolute-project-root> before enabling.".into(),
                "Keep writable filesystem tools in a separate explicit server.".into(),
            ],
        },
        BuiltinMcpTemplate {
            name: "git-readonly".into(),
            title: "Git readonly".into(),
            description: "Template for read-only repository inspection: status, branches, history, and metadata.".into(),
            config: json!({
                "type": "stdio",
                "command": "mcp-server-git",
                "args": ["--readonly", "<absolute-repo-root>"],
            }),
            parameters: vec![BuiltinMcpTemplateParameter::Root],
            default_enabled: false,
            read_only: true,
            requires_credentials: false,
            requires_network: false,
            risk: BuiltinMcpTemplateRisk::Low,
            notes: vec![
                "Do not expose commit, push, merge, or reset tools in this template.".into(),
                "Use Mossen permission gates for any future mutation-capable git server.".into(),
            ],
        },
        BuiltinMcpTemplate {
            name: "local-docs".into(),
            title: "Local docs".into(),
            description: "Template for searching local documentation folders without network or credential access.".into(),
            config: json!({
                "type": "stdio",
                "command": "mcp-server-local-docs",
                "args": ["--root", "<absolute-docs-root>"],
            }),
            parameters: vec![BuiltinMcpTemplateParameter::Root],
            default_enabled: false,
            read_only: true,
            requires_credentials: false,
            requires_network: false,
            risk: BuiltinMcpTemplateRisk::Low,
            notes: vec![
                "Good fit for project docs, API references, and internal runbooks.".into(),
                "Do not point this at secret directories.".into(),
            ],
        },
        BuiltinMcpTemplate {
            name: "playwright-local".into(),
            title: "Playwright local browser".into(),
            description: "Template for local browser automation against localhost or explicit test targets.".into(),
            config: json!({
                "type": "stdio",
                "command": "mcp-server-playwright",
                "args": ["--allow-localhost-only"],
            }),
            parameters: vec![],
            default_enabled: false,
            read_only: false,
            requires_credentials: false,
            requires_network: false,
            risk: BuiltinMcpTemplateRisk::Medium,
            notes: vec![
                "Not read-only: browser actions can click, type, and mutate local apps.".into(),
                "Keep remote browsing and authenticated sites out of the default template.".into(),
            ],
        },
        BuiltinMcpTemplate {
            name: "sqlite-readonly".into(),
            title: "SQLite readonly".into(),
            description: "Template for inspecting a local SQLite database in read-only mode.".into(),
            config: json!({
                "type": "stdio",
                "command": "mcp-server-sqlite",
                "args": ["--readonly", "<absolute-db-path>"],
            }),
            parameters: vec![BuiltinMcpTemplateParameter::Db],
            default_enabled: false,
            read_only: true,
            requires_credentials: false,
            requires_network: false,
            risk: BuiltinMcpTemplateRisk::Low,
            notes: vec![
                "Use read-only database flags at both MCP server and SQLite connection level.".into(),
                "Do not include production credential paths in templates.".into(),
            ],
        },
    ]
}

/// `builtinTemplates.ts` `getBuiltinMcpTemplates`。
pub fn get_builtin_mcp_templates() -> Vec<BuiltinMcpTemplate> {
    builtin_templates()
}

/// `builtinTemplates.ts` `getBuiltinMcpTemplate`。
pub fn get_builtin_mcp_template(name: &str) -> Option<BuiltinMcpTemplate> {
    builtin_templates().into_iter().find(|t| t.name == name)
}

/// `builtinTemplates.ts` `getLocalizedBuiltinMcpTemplateText` (zh-CN)。
pub fn get_localized_builtin_mcp_template_text(name: &str) -> LocalizedTemplateText {
    match name {
        "filesystem-readonly" => LocalizedTemplateText {
            title: Some("文件系统只读".into()),
            description: Some("用于本地 filesystem MCP server 的模板，仅暴露明确指定的只读根目录。".into()),
            notes: vec![
                "启用前必须把 <absolute-project-root> 替换成真实绝对路径。".into(),
                "可写文件系统工具应放在另一个明确声明的 server 中。".into(),
            ],
        },
        "git-readonly" => LocalizedTemplateText {
            title: Some("Git 只读".into()),
            description: Some("用于只读仓库检查：状态、分支、历史和元数据。".into()),
            notes: vec![
                "该模板不暴露 commit、push、merge 或 reset 工具。".into(),
                "未来如需可变更的 git server，必须走 Mossen 权限闸。".into(),
            ],
        },
        "local-docs" => LocalizedTemplateText {
            title: Some("本地文档".into()),
            description: Some("用于搜索本地文档目录，不需要网络或凭据访问。".into()),
            notes: vec![
                "适合项目文档、API reference 和内部 runbook。".into(),
                "不要把它指向 secret 目录。".into(),
            ],
        },
        "playwright-local" => LocalizedTemplateText {
            title: Some("本地 Playwright 浏览器".into()),
            description: Some(
                "用于针对 localhost 或明确测试目标的本地浏览器自动化。".into(),
            ),
            notes: vec![
                "这不是只读能力：浏览器动作可以点击、输入并改变本地应用。".into(),
                "默认模板不应包含远程浏览或已登录站点。".into(),
            ],
        },
        "sqlite-readonly" => LocalizedTemplateText {
            title: Some("SQLite 只读".into()),
            description: Some("用于以只读模式检查本地 SQLite 数据库。".into()),
            notes: vec![
                "MCP server 与 SQLite connection 两层都应使用只读参数。".into(),
                "不要在模板中包含生产凭据路径。".into(),
            ],
        },
        _ => LocalizedTemplateText::default(),
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LocalizedTemplateText {
    pub title: Option<String>,
    pub description: Option<String>,
    pub notes: Vec<String>,
}

/// `instantiateBuiltinMcpTemplate` 的输入参数。
#[derive(Debug, Clone, Default)]
pub struct InstantiateParams {
    pub root: Option<String>,
    pub db: Option<String>,
}

/// `builtinTemplates.ts` `instantiateBuiltinMcpTemplate`。
///
/// 返回 (config, missing_params)。若 missing 非空则 config 为 None。
pub fn instantiate_builtin_mcp_template(
    template: &BuiltinMcpTemplate,
    params: InstantiateParams,
) -> (Option<JsonValue>, Vec<BuiltinMcpTemplateParameter>) {
    let missing: Vec<BuiltinMcpTemplateParameter> = template
        .parameters
        .iter()
        .filter(|p| match p {
            BuiltinMcpTemplateParameter::Root => params.root.is_none(),
            BuiltinMcpTemplateParameter::Db => params.db.is_none(),
        })
        .copied()
        .collect();
    if !missing.is_empty() {
        return (None, missing);
    }
    let cfg = match template.name.as_str() {
        "filesystem-readonly" => json!({
            "type": "stdio",
            "command": "mcp-server-filesystem",
            "args": ["--readonly", params.root.unwrap()],
        }),
        "git-readonly" => json!({
            "type": "stdio",
            "command": "mcp-server-git",
            "args": ["--readonly", params.root.unwrap()],
        }),
        "local-docs" => json!({
            "type": "stdio",
            "command": "mcp-server-local-docs",
            "args": ["--root", params.root.unwrap()],
        }),
        "playwright-local" => json!({
            "type": "stdio",
            "command": "mcp-server-playwright",
            "args": ["--allow-localhost-only"],
        }),
        "sqlite-readonly" => json!({
            "type": "stdio",
            "command": "mcp-server-sqlite",
            "args": ["--readonly", params.db.unwrap()],
        }),
        _ => return (None, vec![]),
    };
    (Some(cfg), vec![])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slash_add_plan_missing_name() {
        _reset_mcp_slash_add_plan_store_for_testing();
        let r = get_mcp_slash_add_plan(GetSlashAddPlanInput::default());
        assert!(matches!(r, Err(McpSlashAddPlanError::MissingServerName)));
    }

    #[test]
    fn slash_add_plan_stdio_round_trip() {
        _reset_mcp_slash_add_plan_store_for_testing();
        let r = get_mcp_slash_add_plan(GetSlashAddPlanInput {
            server_name: Some("foo".into()),
            transport: Some("stdio".into()),
            command_or_url: Some("bar".into()),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(r.server_name, "foo");
        assert!(r.config.get("type").unwrap().as_str() == Some("stdio"));
    }

    #[test]
    fn template_lookup() {
        assert!(get_builtin_mcp_template("filesystem-readonly").is_some());
        assert!(get_builtin_mcp_template("does-not-exist").is_none());
    }

    #[test]
    fn instantiate_missing_root() {
        let t = get_builtin_mcp_template("git-readonly").unwrap();
        let (cfg, missing) = instantiate_builtin_mcp_template(&t, InstantiateParams::default());
        assert!(cfg.is_none());
        assert_eq!(missing, vec![BuiltinMcpTemplateParameter::Root]);
    }
}
