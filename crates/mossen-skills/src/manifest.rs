//! # manifest — 插件清单解析
//!
//! 对应 TypeScript 中插件清单验证逻辑。
//! 负责解析 `manifest.json` / `package.json` 中的插件元数据。

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use tracing::warn;

use mossen_types::plugin::{PluginComponent, PluginManifest};

// ---------------------------------------------------------------------------
// 清单架构
// ---------------------------------------------------------------------------

/// 插件清单中的技能声明。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestSkillEntry {
    /// 技能名称。
    pub name: String,
    /// 描述。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 使用场景。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when_to_use: Option<String>,
    /// 参数提示。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub argument_hint: Option<String>,
}

/// 插件清单中的 MCP 服务器声明。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestMcpServer {
    /// 命令。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// 参数。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    /// 环境变量。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
    /// 额外字段。
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// 已解析的清单数据。
#[derive(Debug, Clone)]
pub struct ParsedManifest {
    /// 插件名称。
    pub name: String,
    /// 描述。
    pub description: String,
    /// 版本。
    pub version: Option<String>,
    /// 组件路径映射。
    pub components: HashMap<PluginComponent, Vec<String>>,
    /// Hooks 配置。
    pub hooks_config: Option<serde_json::Value>,
    /// MCP 服务器。
    pub mcp_servers: HashMap<String, serde_json::Value>,
    /// LSP 服务器。
    pub lsp_servers: HashMap<String, serde_json::Value>,
    /// 设置。
    pub settings: HashMap<String, serde_json::Value>,
    /// 依赖列表。
    pub dependencies: Vec<String>,
}

// ---------------------------------------------------------------------------
// 解析入口
// ---------------------------------------------------------------------------

/// 从文件路径加载并解析插件清单。
///
/// 支持 `manifest.json` 和 `package.json`。
pub async fn load_manifest(manifest_path: &Path) -> anyhow::Result<ParsedManifest> {
    let content = tokio::fs::read_to_string(manifest_path).await?;
    parse_manifest_content(&content, manifest_path)
}

/// 解析清单 JSON 内容。
pub fn parse_manifest_content(content: &str, source_path: &Path) -> anyhow::Result<ParsedManifest> {
    let raw: serde_json::Value = serde_json::from_str(content).map_err(|e| {
        anyhow::anyhow!(
            "Failed to parse manifest at {}: {}",
            source_path.display(),
            e
        )
    })?;

    let obj = raw
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("Manifest must be a JSON object"))?;

    let name = obj
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let description = obj
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let version = obj
        .get("version")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // 解析组件路径
    let mut components: HashMap<PluginComponent, Vec<String>> = HashMap::new();

    for (key, component) in &[
        ("commands", PluginComponent::Commands),
        ("agents", PluginComponent::Agents),
        ("skills", PluginComponent::Skills),
        ("hooks", PluginComponent::Hooks),
        ("outputStyles", PluginComponent::OutputStyles),
    ] {
        if let Some(paths) = extract_paths(obj, key) {
            components.insert(*component, paths);
        }
    }

    // 解析 hooks 配置
    let hooks_config = obj
        .get("hooksConfig")
        .cloned()
        .or_else(|| obj.get("hooks").cloned());

    // 解析 MCP 服务器
    let mcp_servers = obj
        .get("mcpServers")
        .and_then(|v| v.as_object())
        .map(|o| o.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default();

    // 解析 LSP 服务器
    let lsp_servers = obj
        .get("lspServers")
        .and_then(|v| v.as_object())
        .map(|o| o.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default();

    // 解析设置
    let settings = obj
        .get("settings")
        .and_then(|v| v.as_object())
        .map(|o| o.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default();

    // 解析依赖
    let dependencies = obj
        .get("dependencies")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    Ok(ParsedManifest {
        name,
        description,
        version,
        components,
        hooks_config,
        mcp_servers,
        lsp_servers,
        settings,
        dependencies,
    })
}

/// 从清单中提取路径列表。
fn extract_paths(
    obj: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Option<Vec<String>> {
    // 支持 "skills" (单路径) 和 "skillsPaths" (多路径) 两种格式
    let single_key = key;
    let plural_key = format!("{}Paths", key);

    let mut paths = Vec::new();

    if let Some(v) = obj.get(single_key) {
        if let Some(s) = v.as_str() {
            paths.push(s.to_string());
        }
    }

    if let Some(v) = obj.get(&plural_key) {
        if let Some(arr) = v.as_array() {
            for item in arr {
                if let Some(s) = item.as_str() {
                    paths.push(s.to_string());
                }
            }
        }
    }

    if paths.is_empty() {
        None
    } else {
        Some(paths)
    }
}

/// 验证清单必需字段。
pub fn validate_manifest(manifest: &ParsedManifest) -> Vec<String> {
    let mut errors = Vec::new();

    if manifest.name.is_empty() || manifest.name == "unknown" {
        errors.push("Missing required field: name".to_string());
    }

    if manifest.description.is_empty() {
        warn!("Plugin '{}' has no description", manifest.name);
    }

    errors
}

/// 将 ParsedManifest 转换为 PluginManifest（types 层类型）。
pub fn to_plugin_manifest(parsed: &ParsedManifest) -> PluginManifest {
    let mut data = HashMap::new();
    data.insert(
        "name".to_string(),
        serde_json::Value::String(parsed.name.clone()),
    );
    data.insert(
        "description".to_string(),
        serde_json::Value::String(parsed.description.clone()),
    );
    if let Some(v) = &parsed.version {
        data.insert("version".to_string(), serde_json::Value::String(v.clone()));
    }
    PluginManifest { data }
}
