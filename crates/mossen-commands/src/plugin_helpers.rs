//! # plugin_helpers — plugin/*.tsx 中可独立翻译的逻辑
//!
//! 对应 TypeScript:
//! - `commands/plugin/pluginDetailsHelpers.tsx`
//! - `commands/plugin/DiscoverPlugins.tsx`（仅业务逻辑，跳过 JSX）
//! - `commands/plugin/ManagePlugins.tsx`（仅业务逻辑）
//! - `commands/plugin/PluginOptionsDialog.tsx`（仅业务逻辑）
//! - `commands/plugin/PluginOptionsFlow.tsx`（仅业务逻辑）
//!
//! 不翻译 UI 组件 — Rust 端 TUI 由 mossen-tui 处理。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

// ---------------------------------------------------------------------------
// pluginDetailsHelpers.tsx
// ---------------------------------------------------------------------------

/// 一个市场条目的最简表示 — 对应 TS `PluginMarketplaceEntry` 部分字段。
///
/// 真正的市场 schema 在 utils/plugins/schemas.ts 中复杂得多，但所有 helper
/// 只需要这几项；其余字段在 Rust 端以原始 JSON 形态保留。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMarketplaceEntry {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// `{ source: 'github', repo: 'org/name' }` 或 `{ source: 'http', url: ... }`。
    /// 用泛型 JSON 承载。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<JsonValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
}

/// `pluginDetailsHelpers.tsx` `InstallablePlugin`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallablePlugin {
    pub entry: PluginMarketplaceEntry,
    pub marketplace_name: String,
    pub plugin_id: String,
    pub is_installed: bool,
}

/// `pluginDetailsHelpers.tsx` `PluginDetailsMenuOption`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDetailsMenuOption {
    pub label: String,
    pub action: String,
}

/// `pluginDetailsHelpers.tsx` `extractGitHubRepo`。
pub fn extract_github_repo(plugin: &InstallablePlugin) -> Option<String> {
    let src = plugin.entry.source.as_ref()?;
    if src.get("source").and_then(|v| v.as_str()) != Some("github") {
        return None;
    }
    src.get("repo").and_then(|v| v.as_str()).map(String::from)
}

/// `pluginDetailsHelpers.tsx` `buildPluginDetailsMenuOptions`。
///
/// `is_chinese` 在 Rust 中替换 TS `getLocalizedText`：true→中文，false→英文。
pub fn build_plugin_details_menu_options(
    has_homepage: bool,
    github_repo: Option<&str>,
    is_chinese: bool,
) -> Vec<PluginDetailsMenuOption> {
    let l = |en: &str, zh: &str| -> String {
        if is_chinese {
            zh.into()
        } else {
            en.into()
        }
    };
    let mut options = vec![
        PluginDetailsMenuOption {
            label: l("Install for you (user scope)", "为你安装（用户范围）"),
            action: "install-user".into(),
        },
        PluginDetailsMenuOption {
            label: l(
                "Install for all collaborators on this repository (project scope)",
                "为此仓库的所有协作者安装（项目范围）",
            ),
            action: "install-project".into(),
        },
        PluginDetailsMenuOption {
            label: l(
                "Install for you, in this repo only (local scope)",
                "仅在此仓库为你安装（本地范围）",
            ),
            action: "install-local".into(),
        },
    ];
    if has_homepage {
        options.push(PluginDetailsMenuOption {
            label: l("Open homepage", "打开主页"),
            action: "homepage".into(),
        });
    }
    if github_repo.is_some() {
        options.push(PluginDetailsMenuOption {
            label: l("View on GitHub", "在 GitHub 查看"),
            action: "github".into(),
        });
    }
    options.push(PluginDetailsMenuOption {
        label: l("Back to plugin list", "返回插件列表"),
        action: "back".into(),
    });
    options
}

/// `pluginDetailsHelpers.tsx` `PluginSelectionKeyHint` 的纯文本版本。
///
/// Rust 不渲染 React 组件 — 我们返回一个文本数组，由 TUI 层决定如何显示。
pub fn plugin_selection_key_hint(has_selection: bool, is_chinese: bool) -> Vec<String> {
    let l = |en: &str, zh: &str| -> String {
        if is_chinese {
            zh.into()
        } else {
            en.into()
        }
    };
    let mut hints = Vec::new();
    if has_selection {
        hints.push(format!("i: {}", l("install", "安装")));
    }
    hints.push(format!("Space: {}", l("toggle", "切换")));
    hints.push(format!("Enter: {}", l("details", "详情")));
    hints.push(format!("Esc: {}", l("back", "返回")));
    hints
}

// ---------------------------------------------------------------------------
// DiscoverPlugins.tsx — 非 JSX 业务逻辑
// ---------------------------------------------------------------------------

/// `DiscoverPlugins.tsx` `getDiscoverPluginsMarketplaceName`。
///
/// `getInitialMarketplace` 返回当前默认市场（如 `mossen`），缺省时返回
/// `"All marketplaces"` 翻译。
pub fn get_discover_plugins_marketplace_name(initial: Option<&str>, is_chinese: bool) -> String {
    match initial {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => {
            if is_chinese {
                "全部市场".into()
            } else {
                "All marketplaces".into()
            }
        }
    }
}

/// `DiscoverPlugins.tsx` `getDiscoverPluginsGitRestartCopy`。
pub fn get_discover_plugins_git_restart_copy(is_chinese: bool) -> String {
    if is_chinese {
        "重启 Mossen 以应用插件变更。".into()
    } else {
        "Restart Mossen for plugin changes to take effect.".into()
    }
}

/// `DiscoverPlugins.tsx` `DiscoverPlugins` 业务逻辑入口。
///
/// 给定可用插件列表 + 已安装 set，返回应当展示的列表（已安装的排在后面，
/// 同时保留可搜索的 metadata），其余 UI 行为由 mossen-tui 完成。
pub fn discover_plugins(
    all: Vec<InstallablePlugin>,
    installed_plugin_ids: &[String],
) -> Vec<InstallablePlugin> {
    let installed_set: std::collections::HashSet<&String> = installed_plugin_ids.iter().collect();
    let mut not_installed = Vec::new();
    let mut installed = Vec::new();
    for mut p in all {
        let id = p.plugin_id.clone();
        p.is_installed = installed_set.contains(&id);
        if p.is_installed {
            installed.push(p);
        } else {
            not_installed.push(p);
        }
    }
    not_installed.append(&mut installed);
    not_installed
}

// ---------------------------------------------------------------------------
// ManagePlugins.tsx — 业务逻辑
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedPluginRow {
    pub plugin_id: String,
    pub name: String,
    pub marketplace: String,
    pub enabled: bool,
    pub managed_disabled: bool,
}

/// `ManagePlugins.tsx` `filterManagedDisabledPlugins`。
///
/// 把 managed disabled 的插件单独筛出来（这些插件由 policy 强制禁用，
/// 用户不能手动切换）。
pub fn filter_managed_disabled_plugins(
    rows: Vec<ManagedPluginRow>,
) -> (Vec<ManagedPluginRow>, Vec<ManagedPluginRow>) {
    rows.into_iter().partition(|r| !r.managed_disabled)
}

/// `ManagePlugins.tsx` `ManagePlugins` 业务逻辑（排除 UI）。
///
/// 输入是所有可见插件，输出是按字母序排序的列表 + managed-disabled 列表。
pub fn manage_plugins(
    rows: Vec<ManagedPluginRow>,
) -> (Vec<ManagedPluginRow>, Vec<ManagedPluginRow>) {
    let (mut available, mut managed_off) = filter_managed_disabled_plugins(rows);
    available.sort_by(|a, b| a.name.cmp(&b.name));
    managed_off.sort_by(|a, b| a.name.cmp(&b.name));
    (available, managed_off)
}

// ---------------------------------------------------------------------------
// PluginOptionsDialog.tsx — 业务逻辑
// ---------------------------------------------------------------------------

/// 插件选项的字段定义（与 TS `PluginOption` 对应的最小子集）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginOptionDef {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<JsonValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// `PluginOptionsDialog.tsx` `buildFinalValues`。
///
/// 把表单值与默认值合并：用户没填的字段回退到 default。返回最终对象。
pub fn build_final_values(
    options: &[PluginOptionDef],
    form_values: &HashMap<String, JsonValue>,
) -> HashMap<String, JsonValue> {
    let mut out: HashMap<String, JsonValue> = HashMap::new();
    for opt in options {
        let value = match form_values.get(&opt.name) {
            Some(v) if !v.is_null() => v.clone(),
            _ => opt.default.clone().unwrap_or(JsonValue::Null),
        };
        if !value.is_null() {
            out.insert(opt.name.clone(), value);
        }
    }
    out
}

/// `PluginOptionsDialog.tsx` `PluginOptionsDialog` 业务逻辑：把字段定义 +
/// 现有值 → (`displayed_values`, `missing_required`)。
pub fn plugin_options_dialog(
    options: &[PluginOptionDef],
    current: &HashMap<String, JsonValue>,
) -> (HashMap<String, JsonValue>, Vec<String>) {
    let mut displayed = HashMap::new();
    let mut missing = Vec::new();
    for opt in options {
        let value = current.get(&opt.name).cloned();
        let resolved = value.clone().or_else(|| opt.default.clone());
        match resolved {
            Some(v) if !v.is_null() => {
                displayed.insert(opt.name.clone(), v);
            }
            _ if opt.kind == "string" || opt.kind == "number" || opt.kind == "boolean" => {
                missing.push(opt.name.clone());
            }
            _ => {}
        }
    }
    (displayed, missing)
}

// ---------------------------------------------------------------------------
// PluginOptionsFlow.tsx — 业务逻辑
// ---------------------------------------------------------------------------

/// `PluginOptionsFlow.tsx` `findPluginOptionsTarget`。
///
/// 查找当前 cwd 下哪个安装作用域的插件需要配置（local > project > user）。
/// 返回该插件的 `plugin_id`。
pub fn find_plugin_options_target<'a>(
    plugin_id: &str,
    candidates_in_scope_order: &'a [String],
) -> Option<&'a String> {
    candidates_in_scope_order
        .iter()
        .find(|id| id.as_str() == plugin_id)
}

/// `PluginOptionsFlow.tsx` `PluginOptionsFlow` 入口业务逻辑。
///
/// 决定下一个 step 应当显示哪个 dialog；缺失字段时返回 `Some(missing)`，
/// 否则返回 None（表示可以执行写入）。
pub fn plugin_options_flow_next_step(
    options: &[PluginOptionDef],
    current: &HashMap<String, JsonValue>,
) -> Option<Vec<String>> {
    let (_, missing) = plugin_options_dialog(options, current);
    if missing.is_empty() {
        None
    } else {
        Some(missing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extract_repo_for_github_source() {
        let p = InstallablePlugin {
            entry: PluginMarketplaceEntry {
                name: "foo".into(),
                description: None,
                source: Some(json!({ "source": "github", "repo": "a/b" })),
                homepage: None,
            },
            marketplace_name: "m".into(),
            plugin_id: "m:foo".into(),
            is_installed: false,
        };
        assert_eq!(extract_github_repo(&p).as_deref(), Some("a/b"));
    }

    #[test]
    fn extract_repo_none_for_http_source() {
        let p = InstallablePlugin {
            entry: PluginMarketplaceEntry {
                name: "foo".into(),
                description: None,
                source: Some(json!({ "source": "http", "url": "x" })),
                homepage: None,
            },
            marketplace_name: "m".into(),
            plugin_id: "m:foo".into(),
            is_installed: false,
        };
        assert!(extract_github_repo(&p).is_none());
    }

    #[test]
    fn final_values_fills_defaults() {
        let opts = vec![PluginOptionDef {
            name: "foo".into(),
            kind: "string".into(),
            default: Some(json!("bar")),
            description: None,
        }];
        let r = build_final_values(&opts, &HashMap::new());
        assert_eq!(r.get("foo"), Some(&json!("bar")));
    }
}
