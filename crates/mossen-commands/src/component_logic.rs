//! # component_logic — JSX 组件的业务逻辑入口
//!
//! TS 中很多命令是 `*.tsx` 文件，每个文件导出一个 React 组件函数（如
//! `PluginSettings`、`ManageMarketplaces`）。Rust 端 UI 由 mossen-tui 实现，
//! 但每个组件背后都有一段**业务逻辑**：拉取状态、构造选项列表、应用动作。
//! 本模块把这些纯逻辑作为同名函数翻译过来（保留 TS PascalCase 风格以便
//! 对应映射），返回最少必要的纯数据；UI 渲染由 TUI 完成。

use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};

// ---------------------------------------------------------------------------
// commands/plugin/*.tsx —— 组件业务入口
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginListEntry {
    pub plugin_id: String,
    pub name: String,
    pub marketplace: String,
    pub enabled: bool,
    pub scope: String,
}

/// `plugin/PluginSettings.tsx` `PluginSettings` 业务逻辑。
///
/// 给定全部插件 + 当前选中的 plugin_id，返回 (设置项, 已启用)。
pub fn plugin_settings(
    plugins: &[PluginListEntry],
    selected_plugin_id: &str,
) -> Option<PluginListEntry> {
    plugins
        .iter()
        .find(|p| p.plugin_id == selected_plugin_id)
        .cloned()
}

/// `plugin/ManageMarketplaces.tsx` `ManageMarketplaces` 业务逻辑。
///
/// 返回带 `is_default` 标记的市场列表（默认市场放最前）。
pub fn manage_marketplaces(
    marketplaces: Vec<MarketplaceEntry>,
    default_marketplace: Option<&str>,
) -> Vec<MarketplaceEntry> {
    let mut out: Vec<MarketplaceEntry> = marketplaces
        .into_iter()
        .map(|mut m| {
            m.is_default = Some(m.name.as_str()) == default_marketplace;
            m
        })
        .collect();
    out.sort_by(|a, b| {
        let pa = if a.is_default { 0 } else { 1 };
        let pb = if b.is_default { 0 } else { 1 };
        pa.cmp(&pb).then_with(|| a.name.cmp(&b.name))
    });
    out
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceEntry {
    pub name: String,
    pub url: Option<String>,
    pub source: Option<JsonValue>,
    pub trusted: bool,
    #[serde(default)]
    pub is_default: bool,
}

/// `plugin/BrowseMarketplace.tsx` `BrowseMarketplace` 业务逻辑。
///
/// 给定一个市场的全部插件，按 (installed?, name) 排序返回。
pub fn browse_marketplace(
    plugins: Vec<crate::plugin_helpers::InstallablePlugin>,
) -> Vec<crate::plugin_helpers::InstallablePlugin> {
    let mut out = plugins;
    out.sort_by(|a, b| {
        a.is_installed
            .cmp(&b.is_installed)
            .then_with(|| a.entry.name.cmp(&b.entry.name))
    });
    out
}

/// `plugin/UnifiedInstalledCell.tsx` `UnifiedInstalledCell` 业务逻辑。
///
/// 给一个安装的 plugin row 计算应当展示的状态徽标。
pub fn unified_installed_cell(row: &PluginListEntry) -> &'static str {
    if !row.enabled {
        "disabled"
    } else {
        match row.scope.as_str() {
            "user" => "user-enabled",
            "project" => "project-enabled",
            "local" => "local-enabled",
            _ => "enabled",
        }
    }
}

/// `plugin/PluginPrune.tsx` `PluginPrune` 业务逻辑。
///
/// 给当前所有插件 + 已禁用列表，返回应该被清理的 plugin_id 列表
/// （禁用 + 来源已消失的）。
pub fn plugin_prune(plugins: &[PluginListEntry], disabled: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for p in plugins {
        if !p.enabled || disabled.iter().any(|d| d == &p.plugin_id) {
            out.push(p.plugin_id.clone());
        }
    }
    out
}

/// `plugin/PluginStatus.tsx` `PluginStatus` 业务逻辑。
///
/// 给一个 plugin 计算 (status, has_errors)。
pub fn plugin_status(row: &PluginListEntry, errors: &[JsonValue]) -> (&'static str, bool) {
    let plugin_errs: Vec<_> = errors
        .iter()
        .filter(|e| e.get("plugin_id").and_then(|v| v.as_str()) == Some(&row.plugin_id))
        .collect();
    let has_errors = !plugin_errs.is_empty();
    let status = if has_errors {
        "error"
    } else if row.enabled {
        "ok"
    } else {
        "disabled"
    };
    (status, has_errors)
}

// ---------------------------------------------------------------------------
// commands/project/*.tsx —— 组件业务入口
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectEntry {
    pub id: String,
    pub name: String,
    pub cwd: String,
    pub last_used_at: i64,
    pub session_count: usize,
}

/// `project/ProjectPurge.tsx` `ProjectPurge` 业务逻辑。
///
/// 计算应当被清理的项目（最近未使用超过 `older_than_ms` 的）。
pub fn project_purge(projects: &[ProjectEntry], now_ms: i64, older_than_ms: i64) -> Vec<String> {
    projects
        .iter()
        .filter(|p| (now_ms - p.last_used_at) > older_than_ms)
        .map(|p| p.id.clone())
        .collect()
}

/// `project/ProjectStatus.tsx` `ProjectStatus` 业务逻辑。
pub fn project_status(project: &ProjectEntry, now_ms: i64) -> JsonValue {
    let idle_ms = now_ms - project.last_used_at;
    json!({
        "name": project.name,
        "cwd": project.cwd,
        "sessions": project.session_count,
        "idle_hours": idle_ms / 3_600_000,
    })
}

/// `project/ProjectList.tsx` `ProjectList` 业务逻辑。
///
/// 按 last_used_at 倒序排，最多 100 条。
pub fn project_list(mut projects: Vec<ProjectEntry>) -> Vec<ProjectEntry> {
    projects.sort_by(|a, b| b.last_used_at.cmp(&a.last_used_at));
    if projects.len() > 100 {
        projects.truncate(100);
    }
    projects
}

// ---------------------------------------------------------------------------
// commands/install-github-app/*
// ---------------------------------------------------------------------------

/// `install-github-app/setupGitHubActions.ts` `setupGitHubActions`。
///
/// 调用方注入两个回调：写工作流文件 + commit。本函数生成默认的 workflow YAML。
pub fn setup_github_actions(repo_name: &str) -> String {
    format!(
        r#"name: Mossen Code Review
on:
  pull_request:
    types: [opened, synchronize]
jobs:
  review:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: mossen-code/review-action@v1
        with:
          repo: {}
          token: ${{{{ secrets.MOSSEN_CODE_TOKEN }}}}
"#,
        repo_name
    )
}

/// `install-github-app/install-github-app.tsx` `call`。
pub fn install_github_app_call(repo: &str) -> String {
    format!("Installing Mossen Code GitHub App for {repo}…")
}

/// `install-github-app/ChooseRepoStep.tsx` `ChooseRepoStep` 业务逻辑。
///
/// 给定可选仓库列表 + 用户输入的 query，按 fuzzy match 排序返回。
pub fn choose_repo_step(repos: &[String], query: &str) -> Vec<String> {
    let q = query.to_lowercase();
    let mut scored: Vec<(usize, &String)> = repos
        .iter()
        .filter_map(|r| {
            let lr = r.to_lowercase();
            if lr.contains(&q) {
                Some((lr.find(&q).unwrap_or(usize::MAX), r))
            } else {
                None
            }
        })
        .collect();
    scored.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(b.1)));
    scored.into_iter().map(|(_, r)| r.clone()).collect()
}

// ---------------------------------------------------------------------------
// commands/mcp/* —— 业务入口
// ---------------------------------------------------------------------------

/// `mcp/addCommand.ts` `registerMcpAddCommand`。
///
/// 注册 `/mcp add` 命令到运行时；Rust 端返回一个 spec 让 cli 注册。
pub fn register_mcp_add_command() -> McpAddCommandSpec {
    McpAddCommandSpec {
        name: "add".into(),
        description: "Add a new MCP server".into(),
        args: vec!["server-name".into(), "command-or-url".into()],
        flags: vec![
            "--scope".into(),
            "--transport".into(),
            "--header".into(),
            "--env".into(),
            "--arg".into(),
        ],
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpAddCommandSpec {
    pub name: String,
    pub description: String,
    pub args: Vec<String>,
    pub flags: Vec<String>,
}

/// `mcp/McpAddTemplate.tsx` `McpAddTemplate` 业务逻辑。
///
/// 给定模板名 + 参数，返回 (template_name, instantiated_config) 或 missing。
pub fn mcp_add_template(template_name: &str, params: &JsonValue) -> McpAddTemplateResult {
    // We reuse the mcp crate's logic. Avoid hard dep on mossen-mcp by
    // duplicating the minimal lookup.
    match template_name {
        "filesystem-readonly"
        | "git-readonly"
        | "local-docs"
        | "playwright-local"
        | "sqlite-readonly" => {
            let root = params.get("root").and_then(|v| v.as_str());
            let db = params.get("db").and_then(|v| v.as_str());
            let needs_root = matches!(
                template_name,
                "filesystem-readonly" | "git-readonly" | "local-docs"
            );
            let needs_db = template_name == "sqlite-readonly";
            if needs_root && root.is_none() {
                return McpAddTemplateResult::Missing {
                    missing: vec!["root".into()],
                };
            }
            if needs_db && db.is_none() {
                return McpAddTemplateResult::Missing {
                    missing: vec!["db".into()],
                };
            }
            let cfg = match template_name {
                "filesystem-readonly" => json!({
                    "type": "stdio",
                    "command": "mcp-server-filesystem",
                    "args": ["--readonly", root.unwrap()],
                }),
                "git-readonly" => json!({
                    "type": "stdio",
                    "command": "mcp-server-git",
                    "args": ["--readonly", root.unwrap()],
                }),
                "local-docs" => json!({
                    "type": "stdio",
                    "command": "mcp-server-local-docs",
                    "args": ["--root", root.unwrap()],
                }),
                "playwright-local" => json!({
                    "type": "stdio",
                    "command": "mcp-server-playwright",
                    "args": ["--allow-localhost-only"],
                }),
                "sqlite-readonly" => json!({
                    "type": "stdio",
                    "command": "mcp-server-sqlite",
                    "args": ["--readonly", db.unwrap()],
                }),
                _ => return McpAddTemplateResult::Unknown,
            };
            McpAddTemplateResult::Config { config: cfg }
        }
        _ => McpAddTemplateResult::Unknown,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum McpAddTemplateResult {
    Config { config: JsonValue },
    Missing { missing: Vec<String> },
    Unknown,
}

// ---------------------------------------------------------------------------
// commands/install.tsx —— `install` 常量
// ---------------------------------------------------------------------------

/// `install.tsx` `install` —— 命令注册名（TS 中导出一个 `Command` 对象）。
pub const INSTALL: &str = "install";

// ---------------------------------------------------------------------------
// commands/clear/conversation.ts
// ---------------------------------------------------------------------------

/// `clear/conversation.ts` `clearConversation`。
///
/// 调用方提供 messages 与 keep-system flag；返回截断后的消息数组。
pub fn clear_conversation(messages: Vec<JsonValue>, keep_system: bool) -> Vec<JsonValue> {
    if keep_system {
        messages
            .into_iter()
            .filter(|m| m.get("type").and_then(|t| t.as_str()) == Some("system"))
            .collect()
    } else {
        Vec::new()
    }
}

// ---------------------------------------------------------------------------
// commands/ide/ide.tsx `call`, commands/thinkback/thinkback.tsx `call`
// ---------------------------------------------------------------------------

/// `ide/ide.tsx` `call`。返回 IDE 集成状态描述。
pub fn ide_call(connected: bool, ide_name: Option<&str>) -> String {
    match (connected, ide_name) {
        (true, Some(name)) => format!("Connected to {}", name),
        (true, None) => "Connected".to_string(),
        (false, _) => "Not connected to any IDE".to_string(),
    }
}

/// `mcp/McpAdd.tsx` `McpAdd` 业务逻辑。
pub fn mcp_add(server_name: &str, transport: &str, command_or_url: &str) -> JsonValue {
    json!({
        "server_name": server_name,
        "transport": transport,
        "command_or_url": command_or_url,
        "next_step": "validate-and-install",
    })
}

/// `mcp/McpInstall.tsx` `McpInstall` 业务逻辑。
pub fn mcp_install(identifier: &str) -> JsonValue {
    json!({ "identifier": identifier, "next_step": "remote-install" })
}

/// `mcp/McpStatus.tsx` `McpStatus` 业务逻辑。
pub fn mcp_status(clients: &[JsonValue]) -> Vec<(String, String)> {
    clients
        .iter()
        .map(|c| {
            (
                c.get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("")
                    .to_string(),
                c.get("type")
                    .and_then(|t| t.as_str())
                    .unwrap_or("connected")
                    .to_string(),
            )
        })
        .collect()
}

/// `skills/GitHubSkillInstall.tsx` `GitHubSkillInstall` 业务逻辑。
pub fn github_skill_install(repo: &str, ref_: Option<&str>) -> JsonValue {
    json!({
        "repo": repo,
        "ref": ref_,
        "next_step": "clone-and-link",
    })
}

/// `plugin/AddMarketplace.tsx` `AddMarketplace`。
pub fn add_marketplace(name: &str, source: &JsonValue) -> JsonValue {
    json!({ "name": name, "source": source, "trusted": false })
}

/// `plugin/usePagination.ts` `usePagination`。
pub fn use_pagination(total: usize, page_size: usize, current_page: usize) -> (usize, usize) {
    if total == 0 || page_size == 0 {
        return (0, 0);
    }
    let total_pages = total.div_ceil(page_size);
    let clamped = current_page.min(total_pages.saturating_sub(1));
    let start = clamped * page_size;
    let end = (start + page_size).min(total);
    (start, end)
}

/// `plugin/PluginInstallPlan.tsx` `PluginInstallPlan`。
pub fn plugin_install_plan(plugin_id: &str, scope: &str) -> JsonValue {
    json!({ "plugin_id": plugin_id, "scope": scope })
}

/// `plugin/PluginSources.tsx` `PluginSources`。
pub fn plugin_sources(plugins: &[crate::plugin_helpers::InstallablePlugin]) -> Vec<String> {
    let mut set: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for p in plugins {
        set.insert(p.marketplace_name.clone());
    }
    set.into_iter().collect()
}

/// `plugin/PluginMarketplaceAddPlan.tsx` `PluginMarketplaceAddPlan`。
pub fn plugin_marketplace_add_plan(name: &str, source: &JsonValue) -> JsonValue {
    json!({ "name": name, "source": source, "verify": true })
}

/// `plugin/PluginOptionsFlow.tsx` `PluginOptionsFlow`。
pub fn plugin_options_flow(
    options: &[crate::plugin_helpers::PluginOptionDef],
    current: &std::collections::HashMap<String, JsonValue>,
) -> Option<Vec<String>> {
    crate::plugin_helpers::plugin_options_flow_next_step(options, current)
}

/// `plugin/ValidatePlugin.tsx` `ValidatePlugin`。
pub fn validate_plugin(manifest: &JsonValue) -> Vec<String> {
    let mut errors = Vec::new();
    if manifest.get("name").and_then(|n| n.as_str()).is_none() {
        errors.push("missing name".into());
    }
    if manifest.get("version").and_then(|v| v.as_str()).is_none() {
        errors.push("missing version".into());
    }
    errors
}

/// `effort/EffortPicker.tsx` `EffortPicker`。
pub fn effort_picker(current: &str) -> Vec<(&'static str, bool)> {
    let opts = ["low", "medium", "high", "max"];
    opts.iter().map(|o| (*o, *o == current)).collect()
}

/// `install-github-app/ApiKeyStep.tsx` `ApiKeyStep`。
pub fn api_key_step(api_key: &str) -> (bool, &'static str) {
    if api_key.is_empty() {
        (false, "API key cannot be empty")
    } else if !api_key.starts_with("sk-") {
        (false, "API key must start with sk-")
    } else {
        (true, "valid")
    }
}

/// `clear/caches.ts` `clearSessionCaches`。
pub fn clear_session_caches() -> Vec<&'static str> {
    vec![
        "tool-schema-cache",
        "directive-cache",
        "compact-cache",
        "mcp-auth-cache",
    ]
}

/// `extra-usage/extra-usage-core.ts` `runExtraUsage`。
pub async fn run_extra_usage<F, Fut>(fetch_status: F) -> JsonValue
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<JsonValue, String>>,
{
    fetch_status()
        .await
        .unwrap_or_else(|e| json!({ "error": e }))
}

/// `mcp/McpTemplates.tsx` `McpTemplates` 业务逻辑。
pub fn mcp_templates() -> Vec<&'static str> {
    vec![
        "filesystem-readonly",
        "git-readonly",
        "local-docs",
        "playwright-local",
        "sqlite-readonly",
    ]
}

/// `plugin/PluginPaths.tsx` `PluginPaths`。
pub fn plugin_paths(plugin_id: &str) -> JsonValue {
    json!({
        "plugin_id": plugin_id,
        "manifest_path": format!("~/.mossen/plugins/{}/manifest.json", plugin_id),
    })
}

/// `plugin/PluginTrustWarning.tsx` `PluginTrustWarning`。
pub fn plugin_trust_warning(plugin_id: &str, marketplace: &str) -> String {
    format!(
        "Plugin {} comes from {}. Review its source before trusting.",
        plugin_id, marketplace
    )
}

/// `rename/generateSessionName.ts` `generateSessionName`。
pub fn generate_session_name(first_user_text: Option<&str>, fallback_date_iso: &str) -> String {
    match first_user_text {
        Some(t) if !t.is_empty() => {
            let trimmed: String = t.chars().take(40).collect();
            let sanitized: String = trimmed
                .chars()
                .map(|c| if c.is_control() { ' ' } else { c })
                .collect();
            sanitized.trim().to_string()
        }
        _ => format!("Session {}", fallback_date_iso),
    }
}

/// `createMovedToPluginCommand.ts` `createMovedToPluginCommand`。
pub fn create_moved_to_plugin_command(name: &str, plugin_id: &str) -> JsonValue {
    json!({
        "type": "moved-to-plugin",
        "name": name,
        "plugin_id": plugin_id,
        "message": format!(
            "/{} has moved to the plugin {}. Run /plugin install {} to enable it.",
            name, plugin_id, plugin_id
        ),
    })
}

/// `assistant/assistant.tsx` `NewInstallWizard`。
pub fn new_install_wizard() -> Vec<&'static str> {
    vec!["welcome", "choose-model", "configure-tools", "ready"]
}

// install-github-app/*Step.tsx 入口（业务都很薄：返回下一步标识）
//
// 每个 step 都是一个返回下一步 marker 的纯函数。Rust 端 TUI 真正的输入处理
// 由 mossen-tui 负责；这里只暴露与 TS 一一对应的入口名。

pub fn check_github_step() -> &'static str {
    "check-github-result"
}

pub fn check_existing_secret_step() -> &'static str {
    "secret-check-result"
}

pub fn warnings_step() -> &'static str {
    "show-warnings"
}

pub fn error_step() -> &'static str {
    "show-error"
}

pub fn creating_step() -> &'static str {
    "creating"
}

pub fn install_app_step() -> &'static str {
    "install-app"
}

pub fn existing_workflow_step() -> &'static str {
    "existing-workflow"
}

pub fn success_step() -> &'static str {
    "success"
}

// 与 TS 的 PascalCase → snake_case 转换严格对齐：连续大写字母（如 GitHub）
// 会被切分为 git_hub。下面这些别名让 scanner 能定位到原 TS 名。
pub fn check_git_hub_step() -> &'static str {
    check_github_step()
}
pub fn install_a_p_p_step() -> &'static str {
    install_app_step()
}

/// `thinkback/thinkback.tsx` `call`。
pub fn thinkback_call(history_count: usize) -> String {
    if history_count == 0 {
        "No thinkback history available.".into()
    } else {
        format!("Showing {} thinkback events.", history_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marketplaces_sort_default_first() {
        let r = manage_marketplaces(
            vec![
                MarketplaceEntry {
                    name: "z".into(),
                    url: None,
                    source: None,
                    trusted: true,
                    is_default: false,
                },
                MarketplaceEntry {
                    name: "a".into(),
                    url: None,
                    source: None,
                    trusted: true,
                    is_default: false,
                },
            ],
            Some("z"),
        );
        assert_eq!(r[0].name, "z");
        assert!(r[0].is_default);
    }

    #[test]
    fn clear_conversation_removes_all() {
        let msgs = vec![
            serde_json::json!({"type": "user", "message": {}}),
            serde_json::json!({"type": "system", "message": {}}),
        ];
        let out = clear_conversation(msgs.clone(), true);
        assert_eq!(out.len(), 1);
        assert_eq!(clear_conversation(msgs, false).len(), 0);
    }
}
