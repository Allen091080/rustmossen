//! # config_ext — config.ts 中尚未翻译的辅助函数
//!
//! 对应 TypeScript `services/mcp/config.ts`。包含：
//! - `unwrap_ccr_proxy_url` — CCR 代理 URL 还原；
//! - `get_mcp_server_signature` — 计算 dedup 用的签名；
//! - `dedup_plugin_mcp_servers` / `dedup_hosted_mcp_servers` — 插件/hosted 去重；
//! - `get_project_mcp_configs_from_cwd` — 读取 .mcp.json；
//! - `get_mcp_configs_by_scope` — 按作用域分组；
//! - `get_mossen_mcp_configs` / `get_all_mcp_configs` — 合并所有作用域；
//! - `parse_mcp_config` / `parse_mcp_config_from_file_path` — 解析；
//! - `does_enterprise_mcp_config_exist`、`is_mcp_server_disabled`、
//!   `set_mcp_server_enabled` — 状态查询/设置；
//! - `should_allow_managed_mcp_servers_only`、
//!   `are_mcp_configs_allowed_with_enterprise_mcp_config`、
//!   `get_mcp_config_by_name`、`filter_mcp_servers_by_policy`。
//!
//! Rust 端把 TS 中依赖 axios/fs 的部分抽出 — 函数都接受 JSON 输入或调用方
//! 注入的 I/O 闭包。

use std::collections::HashMap;
use std::path::Path;

use serde_json::{json, Value as JsonValue};

use crate::config::ConfigScope;

const CCR_PROXY_PATH_MARKERS: &[&str] = &["/mcp-proxy/", "?mcp_url=", "mcp_url="];

/// `config.ts` `unwrapCcrProxyUrl`。
pub fn unwrap_ccr_proxy_url(url: &str) -> String {
    if !CCR_PROXY_PATH_MARKERS.iter().any(|m| url.contains(m)) {
        return url.to_string();
    }
    if let Ok(u) = url::Url::parse(url) {
        for (k, v) in u.query_pairs() {
            if k == "mcp_url" {
                return v.into_owned();
            }
        }
    }
    url.to_string()
}

fn get_server_command_array(config: &JsonValue) -> Option<Vec<String>> {
    if config.get("type").and_then(|t| t.as_str()).unwrap_or("stdio") != "stdio" {
        return None;
    }
    let cmd = config.get("command").and_then(|v| v.as_str())?;
    let mut out = vec![cmd.to_string()];
    if let Some(args) = config.get("args").and_then(|a| a.as_array()) {
        for a in args {
            if let Some(s) = a.as_str() {
                out.push(s.to_string());
            }
        }
    }
    Some(out)
}

fn get_server_url(config: &JsonValue) -> Option<String> {
    config
        .get("url")
        .and_then(|v| v.as_str())
        .map(String::from)
}

/// `config.ts` `getMcpServerSignature`。
pub fn get_mcp_server_signature(config: &JsonValue) -> Option<String> {
    if let Some(cmd) = get_server_command_array(config) {
        let json_cmd = serde_json::to_string(&cmd).unwrap_or_default();
        return Some(format!("stdio:{}", json_cmd));
    }
    if let Some(url) = get_server_url(config) {
        return Some(format!("url:{}", unwrap_ccr_proxy_url(&url)));
    }
    None
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SuppressedServer {
    pub name: String,
    pub duplicate_of: String,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct DedupResult {
    pub servers: HashMap<String, JsonValue>,
    pub suppressed: Vec<SuppressedServer>,
}

/// `config.ts` `dedupPluginMcpServers`。
pub fn dedup_plugin_mcp_servers(
    plugin_servers: &HashMap<String, JsonValue>,
    manual_servers: &HashMap<String, JsonValue>,
) -> DedupResult {
    let mut manual_sigs: HashMap<String, String> = HashMap::new();
    for (name, cfg) in manual_servers {
        if let Some(sig) = get_mcp_server_signature(cfg) {
            manual_sigs.entry(sig).or_insert_with(|| name.clone());
        }
    }
    let mut out = DedupResult::default();
    let mut seen_plugin_sigs: HashMap<String, String> = HashMap::new();

    // We sort plugin entries to keep first-seen deterministic
    let mut entries: Vec<(&String, &JsonValue)> = plugin_servers.iter().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));

    for (name, cfg) in entries {
        let sig = match get_mcp_server_signature(cfg) {
            None => {
                out.servers.insert(name.clone(), cfg.clone());
                continue;
            }
            Some(s) => s,
        };
        if let Some(dup) = manual_sigs.get(&sig) {
            out.suppressed.push(SuppressedServer {
                name: name.clone(),
                duplicate_of: dup.clone(),
            });
            continue;
        }
        if let Some(dup) = seen_plugin_sigs.get(&sig) {
            out.suppressed.push(SuppressedServer {
                name: name.clone(),
                duplicate_of: dup.clone(),
            });
            continue;
        }
        seen_plugin_sigs.insert(sig, name.clone());
        out.servers.insert(name.clone(), cfg.clone());
    }
    out
}

/// `config.ts` `dedupHostedMcpServers`。
///
/// 只把 `enabled` 的 manual server 作为去重源（与 TS 一致）。
pub fn dedup_hosted_mcp_servers(
    hosted_servers: &HashMap<String, JsonValue>,
    manual_servers: &HashMap<String, JsonValue>,
    is_enabled: impl Fn(&str) -> bool,
) -> DedupResult {
    let mut manual_sigs: HashMap<String, String> = HashMap::new();
    for (name, cfg) in manual_servers {
        if !is_enabled(name) {
            continue;
        }
        if let Some(sig) = get_mcp_server_signature(cfg) {
            manual_sigs.entry(sig).or_insert_with(|| name.clone());
        }
    }
    let mut out = DedupResult::default();
    for (name, cfg) in hosted_servers {
        let sig = match get_mcp_server_signature(cfg) {
            None => {
                out.servers.insert(name.clone(), cfg.clone());
                continue;
            }
            Some(s) => s,
        };
        if let Some(dup) = manual_sigs.get(&sig) {
            out.suppressed.push(SuppressedServer {
                name: name.clone(),
                duplicate_of: dup.clone(),
            });
            continue;
        }
        out.servers.insert(name.clone(), cfg.clone());
    }
    out
}

/// `config.ts` `filterMcpServersByPolicy`。
///
/// 应用策略时把每个键对应到 `(allowed, reason)`。`allowlist` 为 None 表示
/// 不强制白名单；为 Some 表示仅允许列表中名字。
pub fn filter_mcp_servers_by_policy(
    configs: &HashMap<String, JsonValue>,
    allowlist: Option<&[String]>,
    denylist: &[String],
) -> (HashMap<String, JsonValue>, Vec<(String, String)>) {
    let mut allowed = HashMap::new();
    let mut blocked = Vec::new();
    for (name, cfg) in configs {
        if denylist.iter().any(|d| d == name) {
            blocked.push((name.clone(), "denylisted".to_string()));
            continue;
        }
        if let Some(list) = allowlist {
            if !list.iter().any(|n| n == name) {
                blocked.push((name.clone(), "not in allowlist".to_string()));
                continue;
            }
        }
        allowed.insert(name.clone(), cfg.clone());
    }
    (allowed, blocked)
}

// ---------------------------------------------------------------------------
// 解析 / 读取
// ---------------------------------------------------------------------------

/// `config.ts` `parseMcpConfig`。
///
/// `params.json_text` 是 .mcp.json 的文本。返回每个 server 的配置。错误返回 Err。
pub fn parse_mcp_config(json_text: &str) -> Result<HashMap<String, JsonValue>, String> {
    let parsed: JsonValue = serde_json::from_str(json_text).map_err(|e| e.to_string())?;
    let Some(servers) = parsed.get("mcpServers").and_then(|m| m.as_object()) else {
        return Err("missing mcpServers".to_string());
    };
    Ok(servers
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect())
}

/// `config.ts` `parseMcpConfigFromFilePath`。
pub async fn parse_mcp_config_from_file_path(
    path: &Path,
) -> Result<HashMap<String, JsonValue>, String> {
    let text = tokio::fs::read_to_string(path)
        .await
        .map_err(|e| e.to_string())?;
    parse_mcp_config(&text)
}

/// `config.ts` `getProjectMcpConfigsFromCwd`。
pub async fn get_project_mcp_configs_from_cwd(cwd: &Path) -> HashMap<String, JsonValue> {
    let path = cwd.join(".mcp.json");
    parse_mcp_config_from_file_path(&path).await.unwrap_or_default()
}

/// `config.ts` `getMcpConfigsByScope` 的 Rust 形态。
///
/// 调用方提供四个作用域的配置 JSON 字典；本函数把它们按 `ScopedMcpServerConfig`
/// 形态（加 `scope` 字段）合并返回。
pub fn get_mcp_configs_by_scope(
    user: HashMap<String, JsonValue>,
    project: HashMap<String, JsonValue>,
    local: HashMap<String, JsonValue>,
    enterprise: HashMap<String, JsonValue>,
) -> HashMap<String, JsonValue> {
    let mut out = HashMap::new();
    let mut put = |scope: &str, src: HashMap<String, JsonValue>| {
        for (k, mut v) in src {
            if let Some(obj) = v.as_object_mut() {
                obj.insert("scope".into(), JsonValue::String(scope.to_string()));
            }
            out.entry(k).or_insert(v);
        }
    };
    put("local", local);
    put("project", project);
    put("enterprise", enterprise);
    put("user", user);
    out
}

/// `config.ts` `getMossenMcpConfigs`。
pub fn get_mossen_mcp_configs(scoped: &HashMap<String, JsonValue>) -> HashMap<String, JsonValue> {
    scoped
        .iter()
        .filter(|(_, v)| {
            let scope = v.get("scope").and_then(|s| s.as_str()).unwrap_or("");
            scope != "dynamic" && scope != "hosted"
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// `config.ts` `getAllMcpConfigs`。
pub fn get_all_mcp_configs(
    scoped: &HashMap<String, JsonValue>,
    dynamic: &HashMap<String, JsonValue>,
    hosted: &HashMap<String, JsonValue>,
) -> HashMap<String, JsonValue> {
    let mut out = scoped.clone();
    for (k, v) in dynamic {
        let mut cfg = v.clone();
        if let Some(obj) = cfg.as_object_mut() {
            obj.insert("scope".into(), JsonValue::String("dynamic".into()));
        }
        out.entry(k.clone()).or_insert(cfg);
    }
    for (k, v) in hosted {
        let mut cfg = v.clone();
        if let Some(obj) = cfg.as_object_mut() {
            obj.insert("scope".into(), JsonValue::String("hosted".into()));
        }
        out.entry(k.clone()).or_insert(cfg);
    }
    out
}

/// `config.ts` `getMcpConfigByName`。
pub fn get_mcp_config_by_name(configs: &HashMap<String, JsonValue>, name: &str) -> Option<JsonValue> {
    configs.get(name).cloned()
}

// ---------------------------------------------------------------------------
// add / remove
// ---------------------------------------------------------------------------

/// `config.ts` `addMcpConfig` 的 Rust 形态。
///
/// 调用方提供 `read` / `write` 闭包（读取/写入对应作用域的配置 JSON）。
pub async fn add_mcp_config<R, W, RFut, WFut>(
    server_name: &str,
    server_config: JsonValue,
    _scope: ConfigScope,
    read: R,
    write: W,
) -> Result<(), String>
where
    R: FnOnce() -> RFut,
    W: FnOnce(HashMap<String, JsonValue>) -> WFut,
    RFut: std::future::Future<Output = Result<HashMap<String, JsonValue>, String>>,
    WFut: std::future::Future<Output = Result<(), String>>,
{
    let mut configs = read().await?;
    configs.insert(server_name.to_string(), server_config);
    write(configs).await
}

/// `config.ts` `removeMcpConfig` 的 Rust 形态。
pub async fn remove_mcp_config<R, W, RFut, WFut>(
    server_name: &str,
    _scope: ConfigScope,
    read: R,
    write: W,
) -> Result<(), String>
where
    R: FnOnce() -> RFut,
    W: FnOnce(HashMap<String, JsonValue>) -> WFut,
    RFut: std::future::Future<Output = Result<HashMap<String, JsonValue>, String>>,
    WFut: std::future::Future<Output = Result<(), String>>,
{
    let mut configs = read().await?;
    configs.remove(server_name);
    write(configs).await
}

// ---------------------------------------------------------------------------
// 状态 / 策略
// ---------------------------------------------------------------------------

/// `config.ts` `doesEnterpriseMcpConfigExist`。
pub fn does_enterprise_mcp_config_exist(path: &Path) -> bool {
    path.exists()
}

/// `config.ts` `shouldAllowManagedMcpServersOnly`。
///
/// `policy` JSON 字段：`mcpServers: { managedOnly?: true }`。
pub fn should_allow_managed_mcp_servers_only(policy: &JsonValue) -> bool {
    policy
        .get("mcpServers")
        .and_then(|m| m.get("managedOnly"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// `config.ts` `areMcpConfigsAllowedWithEnterpriseMcpConfig`。
pub fn are_mcp_configs_allowed_with_enterprise_mcp_config(policy: &JsonValue) -> bool {
    !should_allow_managed_mcp_servers_only(policy)
}

/// `config.ts` `isMcpServerDisabled`。
pub fn is_mcp_server_disabled(name: &str, disabled_list: &[String]) -> bool {
    disabled_list.iter().any(|d| d == name)
}

/// `config.ts` `setMcpServerEnabled` — 返回更新后的 disabled 列表。
pub fn set_mcp_server_enabled(name: &str, enabled: bool, mut disabled_list: Vec<String>) -> Vec<String> {
    if enabled {
        disabled_list.retain(|n| n != name);
    } else if !disabled_list.iter().any(|n| n == name) {
        disabled_list.push(name.to_string());
    }
    disabled_list
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unwrap_proxy_extracts_inner_url() {
        let s = "https://ccr.example/mcp-proxy/foo?mcp_url=https%3A%2F%2Freal.example%2Fmcp";
        let r = unwrap_ccr_proxy_url(s);
        assert_eq!(r, "https://real.example/mcp");
    }

    #[test]
    fn signature_for_stdio() {
        let sig = get_mcp_server_signature(&json!({
            "type": "stdio",
            "command": "foo",
            "args": ["a"],
        }));
        assert!(sig.unwrap().starts_with("stdio:"));
    }

    #[test]
    fn signature_none_for_sdk() {
        let sig = get_mcp_server_signature(&json!({ "type": "sdk", "name": "x" }));
        assert!(sig.is_none());
    }

    #[test]
    fn set_mcp_server_enabled_round_trip() {
        let r = set_mcp_server_enabled("a", false, vec![]);
        assert_eq!(r, vec!["a".to_string()]);
        let r = set_mcp_server_enabled("a", true, r);
        assert!(r.is_empty());
    }
}
