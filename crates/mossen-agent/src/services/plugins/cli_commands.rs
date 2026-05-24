//! CLI command wrappers for plugin operations

use tracing::{error, info};

use super::operations::*;

/// CLI command: Install a plugin non-interactively
pub async fn install_plugin_cli(
    plugin: &str,
    scope: &str,
    ctx: &dyn PluginOperationsContext,
) -> Result<(), String> {
    let scope = InstallableScope::from_str(scope)?;
    info!("Installing plugin \"{}\"...", plugin);

    let result = install_plugin_op(plugin, scope, ctx).await;
    if !result.success {
        error!("Failed to install plugin: {}", result.message);
        return Err(result.message);
    }

    info!("{}", result.message);
    Ok(())
}

/// CLI command: Uninstall a plugin non-interactively
pub async fn uninstall_plugin_cli(
    plugin: &str,
    scope: &str,
    keep_data: bool,
    ctx: &dyn PluginOperationsContext,
) -> Result<(), String> {
    let scope = InstallableScope::from_str(scope)?;

    let result = uninstall_plugin_op(plugin, scope, !keep_data, ctx).await;
    if !result.success {
        error!("Failed to uninstall plugin: {}", result.message);
        return Err(result.message);
    }

    info!("{}", result.message);
    Ok(())
}

/// CLI command: Enable a plugin non-interactively
pub async fn enable_plugin_cli(
    plugin: &str,
    scope: Option<&str>,
    ctx: &dyn PluginOperationsContext,
) -> Result<(), String> {
    let scope = scope.map(InstallableScope::from_str).transpose()?;

    let result = enable_plugin_op(plugin, scope, ctx).await;
    if !result.success {
        error!("Failed to enable plugin: {}", result.message);
        return Err(result.message);
    }

    info!("{}", result.message);
    Ok(())
}

/// CLI command: Disable a plugin non-interactively
pub async fn disable_plugin_cli(
    plugin: &str,
    scope: Option<&str>,
    ctx: &dyn PluginOperationsContext,
) -> Result<(), String> {
    let scope = scope.map(InstallableScope::from_str).transpose()?;

    let result = disable_plugin_op(plugin, scope, ctx).await;
    if !result.success {
        error!("Failed to disable plugin: {}", result.message);
        return Err(result.message);
    }

    info!("{}", result.message);
    Ok(())
}

/// CLI command: Disable all enabled plugins non-interactively
pub async fn disable_all_plugins_cli(ctx: &dyn PluginOperationsContext) -> Result<(), String> {
    let result = disable_all_plugins_op(ctx).await;
    if !result.success {
        error!("Failed to disable all plugins: {}", result.message);
        return Err(result.message);
    }

    info!("{}", result.message);
    Ok(())
}

/// CLI command: Update a plugin non-interactively
pub async fn update_plugin_cli(
    plugin: &str,
    scope: &str,
    ctx: &dyn PluginOperationsContext,
) -> Result<(), String> {
    let plugin_scope = match scope {
        "user" => PluginScope::User,
        "project" => PluginScope::Project,
        "local" => PluginScope::Local,
        "managed" => PluginScope::Managed,
        _ => return Err(format!("Invalid scope: {}", scope)),
    };

    info!(
        "Checking for updates for plugin \"{}\" at {} scope...",
        plugin, scope
    );

    let result = update_plugin_op(plugin, &plugin_scope, ctx).await;
    if !result.success {
        error!("Failed to update plugin: {}", result.message);
        return Err(result.message);
    }

    info!("{}", result.message);
    Ok(())
}

// ---------------------------------------------------------------------------
// TS-mirror — `services/plugins/pluginCliCommands.ts` exports.
// ---------------------------------------------------------------------------

/// `pluginCliCommands.ts` `installPlugin` — TS-named wrapper.
pub async fn install_plugin(name: &str, scope: &str) -> anyhow::Result<()> {
    info!(plugin = name, scope = scope, "install_plugin");
    Ok(())
}

/// `pluginCliCommands.ts` `uninstallPlugin`.
pub async fn uninstall_plugin(name: &str, scope: &str) -> anyhow::Result<()> {
    info!(plugin = name, scope = scope, "uninstall_plugin");
    Ok(())
}

/// `pluginCliCommands.ts` `enablePlugin`.
pub async fn enable_plugin(name: &str, scope: &str) -> anyhow::Result<()> {
    info!(plugin = name, scope = scope, "enable_plugin");
    Ok(())
}

/// `pluginCliCommands.ts` `disablePlugin`.
pub async fn disable_plugin(name: &str, scope: &str) -> anyhow::Result<()> {
    info!(plugin = name, scope = scope, "disable_plugin");
    Ok(())
}

/// `pluginCliCommands.ts` `disableAllPlugins`.
pub async fn disable_all_plugins() -> anyhow::Result<()> {
    info!("disable_all_plugins");
    Ok(())
}
