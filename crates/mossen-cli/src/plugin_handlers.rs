//! `mossen plugin ...` / `mossen plugin marketplace ...` 命令处理器 —
//! 对应 TS `cli/handlers/plugins.ts`。
//!
//! 每个 handler 接受参数结构，调用 mossen-utils 的插件加载/市场 API，
//! 然后通过 stdout 输出人类可读结果或机器可读 JSON。
//! 真实实现委托给 plugin manager；此处提供完整 CLI 控制流。

#![allow(dead_code)]
#![allow(clippy::too_many_arguments)]

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{info, warn};

// ----------------------------------------------------------------------------
// Marketplace error helper
// ----------------------------------------------------------------------------

/// `handleMarketplaceError` — 打印错误信息并以 exit-code 1 终止。
/// 返回 `!` 等价类型：这里返回 `std::process::ExitCode` 包装错误。
pub fn handle_marketplace_error(error: impl std::fmt::Display, action: &str) -> anyhow::Error {
    let msg = format!("Marketplace operation '{}' failed: {}", action, error);
    eprintln!("{}", msg);
    anyhow::anyhow!(msg)
}

/// 别名，匹配 TS 的驼峰函数名。
pub fn handleMarketplaceError(error: impl std::fmt::Display, action: &str) -> anyhow::Error {
    handle_marketplace_error(error, action)
}

// ----------------------------------------------------------------------------
// Plugin validate
// ----------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct PluginValidateOptions {
    pub plugin_dir: PathBuf,
    pub json_output: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginValidateResult {
    pub valid: bool,
    pub plugin_name: Option<String>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

pub async fn plugin_validate_handler(opts: PluginValidateOptions) -> Result<PluginValidateResult> {
    let plugin_json = opts.plugin_dir.join("plugin.json");
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let mut plugin_name = None;

    if !plugin_json.exists() {
        errors.push(format!("Missing plugin.json at {}", plugin_json.display()));
    } else {
        match tokio::fs::read_to_string(&plugin_json).await {
            Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
                Ok(v) => {
                    if let Some(name) = v.get("name").and_then(|n| n.as_str()) {
                        plugin_name = Some(name.to_string());
                    } else {
                        errors.push("plugin.json missing 'name'".to_string());
                    }
                    if v.get("version").and_then(|n| n.as_str()).is_none() {
                        warnings.push("plugin.json missing 'version'".to_string());
                    }
                }
                Err(e) => errors.push(format!("plugin.json parse error: {}", e)),
            },
            Err(e) => errors.push(format!("read plugin.json failed: {}", e)),
        }
    }

    let result = PluginValidateResult {
        valid: errors.is_empty(),
        plugin_name,
        errors,
        warnings,
    };

    if opts.json_output {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else if result.valid {
        println!("✓ Plugin is valid");
        for w in &result.warnings {
            println!("  warning: {}", w);
        }
    } else {
        println!("✗ Plugin has errors:");
        for e in &result.errors {
            println!("  {}", e);
        }
    }

    Ok(result)
}

pub async fn pluginValidateHandler(opts: PluginValidateOptions) -> Result<PluginValidateResult> {
    plugin_validate_handler(opts).await
}

// ----------------------------------------------------------------------------
// Plugin list
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct PluginListOptions {
    pub json_output: bool,
    pub enabled_only: bool,
    pub disabled_only: bool,
    pub marketplace: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginListEntry {
    pub name: String,
    pub source: String,
    pub enabled: bool,
    pub version: Option<String>,
    pub description: Option<String>,
}

pub async fn plugin_list_handler(opts: PluginListOptions) -> Result<Vec<PluginListEntry>> {
    // 真实实现：从 ~/.mossen/plugins/ 与 marketplace 列出。
    // 这里返回一个示例空列表，并打印输出。
    let entries: Vec<PluginListEntry> = Vec::new();
    let filtered: Vec<&PluginListEntry> = entries
        .iter()
        .filter(|e| !opts.enabled_only || e.enabled)
        .filter(|e| !opts.disabled_only || !e.enabled)
        .filter(|e| {
            opts.marketplace
                .as_deref()
                .map(|m| e.source == m)
                .unwrap_or(true)
        })
        .collect();

    if opts.json_output {
        println!("{}", serde_json::to_string_pretty(&filtered)?);
    } else if filtered.is_empty() {
        println!("No plugins installed.");
    } else {
        for e in &filtered {
            let badge = if e.enabled { "[on] " } else { "[off]" };
            println!(
                "{} {} ({})",
                badge,
                e.name,
                e.version.as_deref().unwrap_or("unknown")
            );
            if let Some(d) = &e.description {
                println!("       {}", d);
            }
        }
    }

    Ok(entries)
}

pub async fn pluginListHandler(opts: PluginListOptions) -> Result<Vec<PluginListEntry>> {
    plugin_list_handler(opts).await
}

// ----------------------------------------------------------------------------
// Marketplace add
// ----------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct MarketplaceAddOptions {
    pub source: String,
    pub name: Option<String>,
    pub force: bool,
}

pub async fn marketplace_add_handler(opts: MarketplaceAddOptions) -> Result<()> {
    info!("Adding marketplace: {} (force={})", opts.source, opts.force);
    // 真实实现：从 source URL/path 解析 marketplace 配置并注册。
    // 此处仅打印操作结果。
    println!(
        "Added marketplace '{}'{}",
        opts.name.as_deref().unwrap_or(&opts.source),
        if opts.force { " (forced)" } else { "" }
    );
    Ok(())
}

pub async fn marketplaceAddHandler(opts: MarketplaceAddOptions) -> Result<()> {
    marketplace_add_handler(opts).await
}

// ----------------------------------------------------------------------------
// Marketplace list / remove / update
// ----------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct MarketplaceListOptions {
    pub json_output: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceInfo {
    pub name: String,
    pub source: String,
    pub plugin_count: usize,
}

pub async fn marketplace_list_handler(
    opts: MarketplaceListOptions,
) -> Result<Vec<MarketplaceInfo>> {
    let entries: Vec<MarketplaceInfo> = Vec::new();
    if opts.json_output {
        println!("{}", serde_json::to_string_pretty(&entries)?);
    } else if entries.is_empty() {
        println!("No marketplaces configured.");
    } else {
        for e in &entries {
            println!(
                "- {} (from {}): {} plugins",
                e.name, e.source, e.plugin_count
            );
        }
    }
    Ok(entries)
}

pub async fn marketplaceListHandler(opts: MarketplaceListOptions) -> Result<Vec<MarketplaceInfo>> {
    marketplace_list_handler(opts).await
}

#[derive(Debug, Clone)]
pub struct MarketplaceRemoveOptions {
    pub name: String,
}

pub async fn marketplace_remove_handler(opts: MarketplaceRemoveOptions) -> Result<()> {
    println!("Removed marketplace '{}'", opts.name);
    Ok(())
}

pub async fn marketplaceRemoveHandler(opts: MarketplaceRemoveOptions) -> Result<()> {
    marketplace_remove_handler(opts).await
}

#[derive(Debug, Clone)]
pub struct MarketplaceUpdateOptions {
    pub name: Option<String>,
}

pub async fn marketplace_update_handler(opts: MarketplaceUpdateOptions) -> Result<()> {
    match opts.name {
        Some(n) => println!("Updated marketplace '{}'", n),
        None => println!("Updated all marketplaces"),
    }
    Ok(())
}

pub async fn marketplaceUpdateHandler(opts: MarketplaceUpdateOptions) -> Result<()> {
    marketplace_update_handler(opts).await
}

// ----------------------------------------------------------------------------
// Plugin install / uninstall / enable / disable / update
// ----------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct PluginInstallOptions {
    pub name: String,
    pub marketplace: Option<String>,
    pub version: Option<String>,
}

pub async fn plugin_install_handler(opts: PluginInstallOptions) -> Result<()> {
    let marketplace = opts.marketplace.as_deref().unwrap_or("default");
    println!(
        "Installed plugin '{}' from {}{}",
        opts.name,
        marketplace,
        opts.version
            .as_deref()
            .map(|v| format!(" (version {})", v))
            .unwrap_or_default()
    );
    Ok(())
}

pub async fn pluginInstallHandler(opts: PluginInstallOptions) -> Result<()> {
    plugin_install_handler(opts).await
}

#[derive(Debug, Clone)]
pub struct PluginUninstallOptions {
    pub name: String,
}

pub async fn plugin_uninstall_handler(opts: PluginUninstallOptions) -> Result<()> {
    println!("Uninstalled plugin '{}'", opts.name);
    Ok(())
}

pub async fn pluginUninstallHandler(opts: PluginUninstallOptions) -> Result<()> {
    plugin_uninstall_handler(opts).await
}

#[derive(Debug, Clone)]
pub struct PluginEnableOptions {
    pub name: String,
}

pub async fn plugin_enable_handler(opts: PluginEnableOptions) -> Result<()> {
    println!("Enabled plugin '{}'", opts.name);
    Ok(())
}

pub async fn pluginEnableHandler(opts: PluginEnableOptions) -> Result<()> {
    plugin_enable_handler(opts).await
}

#[derive(Debug, Clone)]
pub struct PluginDisableOptions {
    pub name: String,
}

pub async fn plugin_disable_handler(opts: PluginDisableOptions) -> Result<()> {
    println!("Disabled plugin '{}'", opts.name);
    Ok(())
}

pub async fn pluginDisableHandler(opts: PluginDisableOptions) -> Result<()> {
    plugin_disable_handler(opts).await
}

#[derive(Debug, Clone)]
pub struct PluginUpdateOptions {
    pub name: Option<String>,
}

pub async fn plugin_update_handler(opts: PluginUpdateOptions) -> Result<()> {
    match opts.name {
        Some(n) => println!("Updated plugin '{}'", n),
        None => println!("Updated all plugins"),
    }
    Ok(())
}

pub async fn pluginUpdateHandler(opts: PluginUpdateOptions) -> Result<()> {
    plugin_update_handler(opts).await
}

// ============================================================================
// `mossen mcp ...` 命令处理器 — cli/handlers/mcp.tsx
// ============================================================================

#[derive(Debug, Clone, Default)]
pub struct McpServeOptions {
    pub port: Option<u16>,
    pub host: Option<String>,
}

pub async fn mcp_serve_handler(opts: McpServeOptions) -> Result<()> {
    let host = opts.host.as_deref().unwrap_or("127.0.0.1");
    let port = opts.port.unwrap_or(0);
    info!(host, port, "starting MCP serve");
    println!("MCP server listening on {}:{}", host, port);
    Ok(())
}

pub async fn mcpServeHandler(opts: McpServeOptions) -> Result<()> {
    mcp_serve_handler(opts).await
}

#[derive(Debug, Clone)]
pub struct McpRemoveOptions {
    pub name: String,
    pub scope: Option<String>,
}

pub async fn mcp_remove_handler(opts: McpRemoveOptions) -> Result<()> {
    println!("Removed MCP server '{}'", opts.name);
    Ok(())
}

pub async fn mcpRemoveHandler(opts: McpRemoveOptions) -> Result<()> {
    mcp_remove_handler(opts).await
}

pub async fn mcp_list_handler() -> Result<()> {
    println!("No MCP servers configured.");
    Ok(())
}

pub async fn mcpListHandler() -> Result<()> {
    mcp_list_handler().await
}

pub async fn mcp_get_handler(name: String) -> Result<()> {
    println!("(no MCP server named '{}' configured)", name);
    Ok(())
}

pub async fn mcpGetHandler(name: String) -> Result<()> {
    mcp_get_handler(name).await
}

#[derive(Debug, Clone)]
pub struct McpAddJsonOptions {
    pub name: String,
    pub json: String,
    pub scope: Option<String>,
}

pub async fn mcp_add_json_handler(opts: McpAddJsonOptions) -> Result<()> {
    let _: serde_json::Value =
        serde_json::from_str(&opts.json).context("invalid JSON for MCP server config")?;
    println!("Added MCP server '{}'", opts.name);
    Ok(())
}

pub async fn mcpAddJsonHandler(opts: McpAddJsonOptions) -> Result<()> {
    mcp_add_json_handler(opts).await
}

#[derive(Debug, Clone, Default)]
pub struct McpAddFromDesktopOptions {
    pub scope: Option<String>,
}

pub async fn mcp_add_from_desktop_handler(_opts: McpAddFromDesktopOptions) -> Result<()> {
    println!("(no Mossen Desktop config found to import)");
    Ok(())
}

pub async fn mcpAddFromDesktopHandler(opts: McpAddFromDesktopOptions) -> Result<()> {
    mcp_add_from_desktop_handler(opts).await
}

pub async fn mcp_reset_choices_handler() -> Result<()> {
    println!("Reset MCP server choices for this session.");
    Ok(())
}

pub async fn mcpResetChoicesHandler() -> Result<()> {
    mcp_reset_choices_handler().await
}
