//! # hosted — hosted MCP server 配置缓存与连接历史
//!
//! 对应 TypeScript `services/mcp/hosted.ts`。提供：
//! - `fetch_hosted_mcp_configs_if_eligible` — 拉取 hosted org 配置（按 session 缓存）；
//! - `clear_hosted_mcp_configs_cache` — 清除缓存；
//! - `mark_hosted_mcp_connected` / `has_hosted_mcp_ever_connected` — 持久化记录“曾经连接过”。
//!
//! 网络层（OAuth + axios）和全局配置层（saveGlobalConfig）在 Rust 端通过
//! 函数注入（`fetch` 与 `cfg_io`），避免对 utils/cli 的循环依赖。

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock, RwLock};

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::normalization::normalize_name_for_mcp;

/// hosted MCP server 描述 — 与 TS `HostedMcpServer` 对应（JSON 形态）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostedMcpServer {
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
    pub id: String,
    pub display_name: String,
    pub url: String,
    #[serde(default)]
    pub created_at: Option<String>,
}

/// hosted MCP servers 响应 — 与 TS `HostedMcpServersResponse` 对应。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HostedMcpServersResponse {
    pub data: Vec<HostedMcpServer>,
    #[serde(default)]
    pub has_more: bool,
    #[serde(default)]
    pub next_page: Option<String>,
}

static HOSTED_CONFIGS_CACHE: OnceLock<Mutex<Option<HashMap<String, JsonValue>>>> = OnceLock::new();
static HOSTED_EVER_CONNECTED: OnceLock<RwLock<HashSet<String>>> = OnceLock::new();

fn cache_cell() -> &'static Mutex<Option<HashMap<String, JsonValue>>> {
    HOSTED_CONFIGS_CACHE.get_or_init(|| Mutex::new(None))
}

fn ever_connected_cell() -> &'static RwLock<HashSet<String>> {
    HOSTED_EVER_CONNECTED.get_or_init(|| RwLock::new(HashSet::new()))
}

/// `hosted.ts` `fetchHostedMcpConfigsIfEligible` 的 Rust 形态。
///
/// 按 session 缓存：第一次调用真的去 `fetch_impl()` 拉数据，并把它转换
/// 成 `<name> -> scoped_config_json`。Rust 端：
/// - 已缓存 → 直接返回副本；
/// - 未缓存 → 调用注入的 `fetch_impl().await`，将其规范化后塞入缓存。
pub async fn fetch_hosted_mcp_configs_if_eligible<F, Fut>(
    fetch_impl: F,
) -> HashMap<String, JsonValue>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<HostedMcpServersResponse, String>>,
{
    if let Some(cached) = cache_cell().lock().unwrap().clone() {
        return cached;
    }
    let response = match fetch_impl().await {
        Ok(r) => r,
        Err(_) => {
            // Mirror TS: catch failures and return empty configs.
            return HashMap::new();
        }
    };

    let mut used_normalized = HashSet::new();
    let mut configs: HashMap<String, JsonValue> = HashMap::new();
    for server in response.data {
        let base_name = format!("hosted {}", server.display_name);
        let mut final_name = base_name.clone();
        let mut final_normalized = normalize_name_for_mcp(&final_name);
        let mut count = 1;
        while used_normalized.contains(&final_normalized) {
            count += 1;
            final_name = format!("{} ({})", base_name, count);
            final_normalized = normalize_name_for_mcp(&final_name);
        }
        used_normalized.insert(final_normalized);

        configs.insert(
            final_name,
            serde_json::json!({
                "type": "hosted-proxy",
                "url": server.url,
                "id": server.id,
                "scope": "hosted",
            }),
        );
    }

    *cache_cell().lock().unwrap() = Some(configs.clone());
    configs
}

/// `hosted.ts` `clearHostedMcpConfigsCache`。
pub fn clear_hosted_mcp_configs_cache() {
    *cache_cell().lock().unwrap() = None;
}

/// `hosted.ts` `markHostedMcpConnected`。幂等。
///
/// `persist` 接受当前 `hostedMcpEverConnected` 列表的可选追加项；在
/// 内存中记录的同时把可选的持久化函数转交给调用方。
pub fn mark_hosted_mcp_connected<F: FnOnce(&[String])>(name: &str, persist: F) {
    let mut set = ever_connected_cell().write().unwrap();
    if !set.insert(name.to_string()) {
        return;
    }
    let snapshot: Vec<String> = set.iter().cloned().collect();
    drop(set);
    persist(&snapshot);
}

/// `hosted.ts` `hasHostedMcpEverConnected`。
pub fn has_hosted_mcp_ever_connected(name: &str) -> bool {
    ever_connected_cell().read().unwrap().contains(name)
}

/// 测试入口：注入持久化记录。
pub fn _set_hosted_mcp_ever_connected_for_testing(names: &[String]) {
    let mut s = ever_connected_cell().write().unwrap();
    s.clear();
    for n in names {
        s.insert(n.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fetch_hosted_caches_results() {
        clear_hosted_mcp_configs_cache();
        let mut counter = 0u8;
        let r = fetch_hosted_mcp_configs_if_eligible(|| async {
            counter += 1;
            Ok(HostedMcpServersResponse {
                data: vec![HostedMcpServer {
                    kind: None,
                    id: "srv-1".into(),
                    display_name: "Example".into(),
                    url: "https://example".into(),
                    created_at: None,
                }],
                has_more: false,
                next_page: None,
            })
        })
        .await;
        assert!(r.contains_key("hosted Example"));
    }
}
