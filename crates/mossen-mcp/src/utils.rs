//! # utils — MCP 工具函数
//!
//! 对应 TypeScript `services/mcp/utils.ts`。提供 MCP 工具/命令/资源的过滤、
//! 去重、配置哈希、范围标签等纯函数工具。Rust 端把 TS 中依赖 `Tool` /
//! `Command` 的接口抽象为 trait-free 的 JSON 操作 + 字符串名匹配，避免与
//! mossen-agent 形成循环依赖。

use std::collections::{BTreeMap, HashMap};

use serde_json::{json, Value as JsonValue};

use crate::config::ConfigScope;
use crate::normalization::normalize_name_for_mcp;

// ---------------------------------------------------------------------------
// 名称过滤工具
// ---------------------------------------------------------------------------

/// `utils.ts` `filterToolsByServer`。
///
/// 返回所有 `name.starts_with("mcp__<normalized>__")` 的工具 JSON。每个
/// 元素必须是 `Object`，否则被跳过。
pub fn filter_tools_by_server(tools: &[JsonValue], server_name: &str) -> Vec<JsonValue> {
    let prefix = format!("mcp__{}__", normalize_name_for_mcp(server_name));
    tools
        .iter()
        .filter(|t| {
            t.get("name")
                .and_then(|n| n.as_str())
                .map(|n| n.starts_with(&prefix))
                .unwrap_or(false)
        })
        .cloned()
        .collect()
}

/// `utils.ts` `commandBelongsToServer`。
///
/// 接受 JSON 形态的命令。MCP **prompts** 命名 `mcp__<server>__<prompt>`，
/// MCP **skills** 命名 `<server>:<skill>`。
pub fn command_belongs_to_server(command: &JsonValue, server_name: &str) -> bool {
    let normalized = normalize_name_for_mcp(server_name);
    let Some(name) = command.get("name").and_then(|n| n.as_str()) else {
        return false;
    };
    name.starts_with(&format!("mcp__{}__", normalized))
        || name.starts_with(&format!("{}:", normalized))
}

/// `utils.ts` `filterCommandsByServer`。
pub fn filter_commands_by_server(commands: &[JsonValue], server_name: &str) -> Vec<JsonValue> {
    commands
        .iter()
        .filter(|c| command_belongs_to_server(c, server_name))
        .cloned()
        .collect()
}

/// `utils.ts` `filterMcpPromptsByServer`。
///
/// 排除 `type === 'prompt' && loadedFrom === 'mcp'` 的项（那是 MCP skills，
/// 不应计入 prompts 能力）。
pub fn filter_mcp_prompts_by_server(commands: &[JsonValue], server_name: &str) -> Vec<JsonValue> {
    commands
        .iter()
        .filter(|c| {
            if !command_belongs_to_server(c, server_name) {
                return false;
            }
            let is_mcp_skill = c.get("type").and_then(|t| t.as_str()) == Some("prompt")
                && c.get("loadedFrom").and_then(|t| t.as_str()) == Some("mcp");
            !is_mcp_skill
        })
        .cloned()
        .collect()
}

/// `utils.ts` `filterResourcesByServer`。
pub fn filter_resources_by_server(resources: &[JsonValue], server_name: &str) -> Vec<JsonValue> {
    resources
        .iter()
        .filter(|r| r.get("server").and_then(|s| s.as_str()) == Some(server_name))
        .cloned()
        .collect()
}

/// `utils.ts` `excludeToolsByServer`。
pub fn exclude_tools_by_server(tools: &[JsonValue], server_name: &str) -> Vec<JsonValue> {
    let prefix = format!("mcp__{}__", normalize_name_for_mcp(server_name));
    tools
        .iter()
        .filter(|t| {
            !t.get("name")
                .and_then(|n| n.as_str())
                .map(|n| n.starts_with(&prefix))
                .unwrap_or(false)
        })
        .cloned()
        .collect()
}

/// `utils.ts` `excludeCommandsByServer`。
pub fn exclude_commands_by_server(commands: &[JsonValue], server_name: &str) -> Vec<JsonValue> {
    commands
        .iter()
        .filter(|c| !command_belongs_to_server(c, server_name))
        .cloned()
        .collect()
}

/// `utils.ts` `excludeResourcesByServer`。
pub fn exclude_resources_by_server(
    resources: &HashMap<String, Vec<JsonValue>>,
    server_name: &str,
) -> HashMap<String, Vec<JsonValue>> {
    resources
        .iter()
        .filter(|(k, _)| k.as_str() != server_name)
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

// ---------------------------------------------------------------------------
// 配置哈希
// ---------------------------------------------------------------------------

/// `utils.ts` `hashMcpConfig`。
///
/// 稳定哈希一个 MCP 服务器配置：排除 `scope`，对对象按键排序后序列化，
/// 取 SHA-256 前 16 个十六进制字符。
pub fn hash_mcp_config(config: &JsonValue) -> String {
    let mut clone = config.clone();
    if let Some(obj) = clone.as_object_mut() {
        obj.remove("scope");
    }
    let stable = stable_stringify(&clone);
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(stable.as_bytes());
    let hex = format!("{:x}", hasher.finalize());
    hex.chars().take(16).collect()
}

fn stable_stringify(v: &JsonValue) -> String {
    match v {
        JsonValue::Object(map) => {
            let sorted: BTreeMap<&String, &JsonValue> = map.iter().collect();
            let parts: Vec<String> = sorted
                .iter()
                .map(|(k, val)| format!("{}:{}", json!(k), stable_stringify(val)))
                .collect();
            format!("{{{}}}", parts.join(","))
        }
        JsonValue::Array(arr) => {
            let parts: Vec<String> = arr.iter().map(stable_stringify).collect();
            format!("[{}]", parts.join(","))
        }
        other => other.to_string(),
    }
}

// ---------------------------------------------------------------------------
// MCP 工具/命令辨别
// ---------------------------------------------------------------------------

/// `utils.ts` `isToolFromMcpServer`。
pub fn is_tool_from_mcp_server(tool_name: &str, server_name: &str) -> bool {
    mcp_info_from_string(tool_name)
        .map(|info| info.server_name == server_name)
        .unwrap_or(false)
}

/// `utils.ts` `isMcpTool`。
pub fn is_mcp_tool(tool: &JsonValue) -> bool {
    tool.get("name")
        .and_then(|n| n.as_str())
        .map(|n| n.starts_with("mcp__"))
        .unwrap_or(false)
        || tool.get("isMcp").and_then(|v| v.as_bool()).unwrap_or(false)
}

/// `utils.ts` `isMcpCommand`。
pub fn is_mcp_command(command: &JsonValue) -> bool {
    command
        .get("name")
        .and_then(|n| n.as_str())
        .map(|n| n.starts_with("mcp__"))
        .unwrap_or(false)
        || command
            .get("isMcp")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpToolNameInfo {
    pub server_name: String,
    pub tool_name: String,
}

/// 与 `mcpStringUtils.ts` `mcpInfoFromString` 等价。
/// 解析 `mcp__<server>__<tool>` 形式的字符串。
pub fn mcp_info_from_string(s: &str) -> Option<McpToolNameInfo> {
    let stripped = s.strip_prefix("mcp__")?;
    let mut split = stripped.splitn(2, "__");
    let server_name = split.next()?.to_string();
    let tool_name = split.next()?.to_string();
    if server_name.is_empty() || tool_name.is_empty() {
        return None;
    }
    Some(McpToolNameInfo {
        server_name,
        tool_name,
    })
}

// ---------------------------------------------------------------------------
// scope 描述
// ---------------------------------------------------------------------------

/// `utils.ts` `describeMcpConfigFilePath`。
///
/// Rust 端调用方提供 `user_settings_file` 与 `cwd`（避免直接依赖
/// utils 层），其余路径与 TS 行为一致。
pub fn describe_mcp_config_file_path(
    scope: ConfigScope,
    user_settings_file: &str,
    cwd: &str,
    enterprise_path: Option<&str>,
) -> String {
    match scope {
        ConfigScope::User => user_settings_file.to_string(),
        ConfigScope::Project => format!("{}/.mcp.json", cwd),
        ConfigScope::Local => format!("{} [project: {}]", user_settings_file, cwd),
        ConfigScope::Dynamic => "Dynamically configured".to_string(),
        ConfigScope::Enterprise => enterprise_path.unwrap_or("Enterprise config").to_string(),
        ConfigScope::Hosted => "Hosted platform".to_string(),
        ConfigScope::Managed => "Managed config".to_string(),
    }
}

/// `utils.ts` `getScopeLabel`。
pub fn get_scope_label(scope: ConfigScope) -> &'static str {
    match scope {
        ConfigScope::Local => "Local config (private to you in this project)",
        ConfigScope::Project => "Project config (shared via .mcp.json)",
        ConfigScope::User => "User config (available in all your projects)",
        ConfigScope::Dynamic => "Dynamic config (from command line)",
        ConfigScope::Enterprise => "Enterprise config (managed by your organization)",
        ConfigScope::Hosted => "Hosted platform config",
        ConfigScope::Managed => "Managed config (admin-controlled)",
    }
}

/// `utils.ts` `ensureConfigScope`。
pub fn ensure_config_scope(scope: Option<&str>) -> Result<ConfigScope, String> {
    let s = match scope {
        None | Some("") => return Ok(ConfigScope::Local),
        Some(v) => v,
    };
    match s {
        "user" => Ok(ConfigScope::User),
        "project" => Ok(ConfigScope::Project),
        "local" => Ok(ConfigScope::Local),
        "dynamic" => Ok(ConfigScope::Dynamic),
        "enterprise" => Ok(ConfigScope::Enterprise),
        "hosted" => Ok(ConfigScope::Hosted),
        "managed" => Ok(ConfigScope::Managed),
        _ => Err(format!(
            "Invalid scope: {}. Must be one of: user, project, local, dynamic, enterprise, hosted",
            s
        )),
    }
}

/// `utils.ts` `ensureTransport`。返回 ("stdio" | "sse" | "http")。
pub fn ensure_transport(type_: Option<&str>) -> Result<&'static str, String> {
    match type_ {
        None | Some("") => Ok("stdio"),
        Some("stdio") => Ok("stdio"),
        Some("sse") => Ok("sse"),
        Some("http") => Ok("http"),
        Some(other) => Err(format!(
            "Invalid transport type: {}. Must be one of: stdio, sse, http",
            other
        )),
    }
}

/// `utils.ts` `parseHeaders`。
///
/// 把 `["Header: value", ...]` 转换为映射。重复键的语义按 TS：后者覆盖前者。
pub fn parse_headers(header_array: &[String]) -> Result<HashMap<String, String>, String> {
    let mut out = HashMap::new();
    for header in header_array {
        let Some(colon_idx) = header.find(':') else {
            return Err(format!(
                "Invalid header format: \"{}\". Expected format: \"Header-Name: value\"",
                header
            ));
        };
        let key = header[..colon_idx].trim().to_string();
        let value = header[colon_idx + 1..].trim().to_string();
        if key.is_empty() {
            return Err(format!(
                "Invalid header: \"{}\". Header name cannot be empty.",
                header
            ));
        }
        out.insert(key, value);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// 来源识别
// ---------------------------------------------------------------------------

/// `utils.ts` `getMcpServerScopeFromToolName`。
///
/// 给定一个 MCP 工具名，调用 lookup 闭包查 server config，返回其 scope。
/// `lookup` 输出 `Option<JsonValue>`，需要包含 `scope` 字段；找不到时如果
/// 服务器名以 `hosted_` 开头则返回 `Hosted`。
pub fn get_mcp_server_scope_from_tool_name<F>(tool_name: &str, lookup: F) -> Option<ConfigScope>
where
    F: Fn(&str) -> Option<JsonValue>,
{
    if !is_mcp_tool(&json!({ "name": tool_name })) {
        return None;
    }
    let info = mcp_info_from_string(tool_name)?;
    if let Some(cfg) = lookup(&info.server_name) {
        cfg.get("scope")
            .and_then(|s| s.as_str())
            .and_then(|s| ensure_config_scope(Some(s)).ok())
    } else if info.server_name.starts_with("hosted_") {
        Some(ConfigScope::Hosted)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// 类型守卫（基于 JSON 形态）
// ---------------------------------------------------------------------------

/// `utils.ts` `isStdioConfig`（基于 `type` 字段判断）。
pub fn is_stdio_config(config: &JsonValue) -> bool {
    matches!(
        config.get("type").and_then(|t| t.as_str()),
        None | Some("stdio")
    )
}

pub fn is_sse_config(config: &JsonValue) -> bool {
    config.get("type").and_then(|t| t.as_str()) == Some("sse")
}

pub fn is_http_config(config: &JsonValue) -> bool {
    config.get("type").and_then(|t| t.as_str()) == Some("http")
}

pub fn is_websocket_config(config: &JsonValue) -> bool {
    config.get("type").and_then(|t| t.as_str()) == Some("ws")
}

// ---------------------------------------------------------------------------
// stale 客户端清理
// ---------------------------------------------------------------------------

/// `utils.ts` `excludeStalePluginClients` 的精简版（输入/输出为 JSON）。
///
/// `mcp` 应包含字段 `clients`, `tools`, `commands`, `resources`；`configs` 是
/// `name -> ScopedMcpServerConfig` 的映射。返回更新后的状态 + `stale` 列表。
pub fn exclude_stale_plugin_clients(
    mcp: &JsonValue,
    configs: &HashMap<String, JsonValue>,
) -> JsonValue {
    let clients = mcp
        .get("clients")
        .and_then(|c| c.as_array())
        .cloned()
        .unwrap_or_default();
    let tools = mcp
        .get("tools")
        .and_then(|c| c.as_array())
        .cloned()
        .unwrap_or_default();
    let commands = mcp
        .get("commands")
        .and_then(|c| c.as_array())
        .cloned()
        .unwrap_or_default();
    let resources: HashMap<String, Vec<JsonValue>> = mcp
        .get("resources")
        .and_then(|c| c.as_object())
        .map(|m| {
            m.iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        v.as_array().cloned().unwrap_or_default(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();

    let stale: Vec<JsonValue> = clients
        .iter()
        .filter(|c| {
            let name = c.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let cfg = c.get("config").cloned().unwrap_or(JsonValue::Null);
            match configs.get(name) {
                None => {
                    cfg.get("scope")
                        .and_then(|s| s.as_str())
                        .map(|s| s == "dynamic")
                        .unwrap_or(false)
                }
                Some(fresh) => hash_mcp_config(&cfg) != hash_mcp_config(fresh),
            }
        })
        .cloned()
        .collect();

    if stale.is_empty() {
        return json!({
            "clients": clients,
            "tools": tools,
            "commands": commands,
            "resources": resources,
            "stale": [],
        });
    }

    let mut tools_out = tools.clone();
    let mut commands_out = commands.clone();
    let mut resources_out = resources.clone();
    let mut stale_names: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    for s in &stale {
        if let Some(name) = s.get("name").and_then(|n| n.as_str()) {
            stale_names.insert(name.to_string());
            tools_out = exclude_tools_by_server(&tools_out, name);
            commands_out = exclude_commands_by_server(&commands_out, name);
            resources_out = exclude_resources_by_server(&resources_out, name);
        }
    }

    let clients_out: Vec<JsonValue> = clients
        .into_iter()
        .filter(|c| {
            let name = c.get("name").and_then(|n| n.as_str()).unwrap_or("");
            !stale_names.contains(name)
        })
        .collect();

    json!({
        "clients": clients_out,
        "tools": tools_out,
        "commands": commands_out,
        "resources": resources_out,
        "stale": stale,
    })
}

// ---------------------------------------------------------------------------
// agent server 提取
// ---------------------------------------------------------------------------

/// `utils.ts` `extractAgentMcpServers`。
///
/// 输入 `agents` JSON 数组（每个含 `agentType: string` 与 `mcpServers: Array`）。
/// 返回按 server 名分组的列表（支持 stdio/sse/http/ws 四种 transport）。
pub fn extract_agent_mcp_servers(agents: &[JsonValue]) -> Vec<JsonValue> {
    let mut map: BTreeMap<String, (JsonValue, Vec<String>)> = BTreeMap::new();

    for agent in agents {
        let agent_type = agent
            .get("agentType")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let Some(servers) = agent.get("mcpServers").and_then(|v| v.as_array()) else {
            continue;
        };
        for spec in servers {
            // String reference — skip
            if spec.is_string() {
                continue;
            }
            let Some(obj) = spec.as_object() else { continue };
            if obj.len() != 1 {
                continue;
            }
            let (name, cfg) = obj.iter().next().unwrap();
            let entry = map.entry(name.clone()).or_insert_with(|| {
                let mut c = cfg.clone();
                if let Some(o) = c.as_object_mut() {
                    o.insert("name".to_string(), JsonValue::String(name.clone()));
                }
                (c, Vec::new())
            });
            if !entry.1.contains(&agent_type) {
                entry.1.push(agent_type.clone());
            }
        }
    }

    let mut result = Vec::new();
    for (name, (config, source_agents)) in map {
        let entry = if is_stdio_config(&config) {
            json!({
                "name": name,
                "sourceAgents": source_agents,
                "transport": "stdio",
                "command": config.get("command"),
                "needsAuth": false,
            })
        } else if is_sse_config(&config) {
            json!({
                "name": name,
                "sourceAgents": source_agents,
                "transport": "sse",
                "url": config.get("url"),
                "needsAuth": true,
            })
        } else if is_http_config(&config) {
            json!({
                "name": name,
                "sourceAgents": source_agents,
                "transport": "http",
                "url": config.get("url"),
                "needsAuth": true,
            })
        } else if is_websocket_config(&config) {
            json!({
                "name": name,
                "sourceAgents": source_agents,
                "transport": "ws",
                "url": config.get("url"),
                "needsAuth": false,
            })
        } else {
            continue;
        };
        result.push(entry);
    }
    result.sort_by(|a, b| {
        let an = a.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let bn = b.get("name").and_then(|v| v.as_str()).unwrap_or("");
        an.cmp(bn)
    });
    result
}

// ---------------------------------------------------------------------------
// 日志安全 URL
// ---------------------------------------------------------------------------

/// `utils.ts` `getLoggingSafeMcpBaseUrl`。
///
/// 去除查询字符串与尾斜杠，便于安全日志记录。无 `url` 字段或解析失败返回 None。
pub fn get_logging_safe_mcp_base_url(config: &JsonValue) -> Option<String> {
    let url = config.get("url").and_then(|u| u.as_str())?;
    let parsed = url::Url::parse(url).ok()?;
    let mut cleaned = parsed.clone();
    cleaned.set_query(None);
    let s = cleaned.to_string();
    let trimmed = s.trim_end_matches('/').to_string();
    Some(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_headers_works() {
        let h = parse_headers(&["X-A: 1".into(), "X-B:2".into()]).unwrap();
        assert_eq!(h.get("X-A"), Some(&"1".to_string()));
        assert_eq!(h.get("X-B"), Some(&"2".to_string()));
    }

    #[test]
    fn parse_headers_bad_format() {
        let err = parse_headers(&["no-colon".into()]).unwrap_err();
        assert!(err.contains("Invalid header format"));
    }

    #[test]
    fn mcp_info_basic() {
        let info = mcp_info_from_string("mcp__myserver__do_thing").unwrap();
        assert_eq!(info.server_name, "myserver");
        assert_eq!(info.tool_name, "do_thing");
    }

    #[test]
    fn hash_excludes_scope() {
        let a = json!({"type":"stdio","command":"x","scope":"user"});
        let b = json!({"type":"stdio","command":"x","scope":"project"});
        assert_eq!(hash_mcp_config(&a), hash_mcp_config(&b));
    }
}
