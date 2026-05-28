//! `/plugin` — Manage plugins (install, remove, list, configure).

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use mossen_agent::services::plugins::operations::{
    disable_plugin_op, enable_plugin_op, install_plugin_op, uninstall_plugin_op, InstallableScope,
    PluginOperationResult, PluginOperationsContext,
};
use mossen_utils::plugins::cache_utils::{
    execute_plugin_prune_plan, get_plugin_prune_plan, summarize_plugin_cache,
};
use mossen_utils::plugins::dependency_resolver::{self, DependencyLookupResult, ResolutionResult};
use mossen_utils::plugins::marketplace_add_plan::{
    execute_plugin_marketplace_add_plan, get_plugin_marketplace_add_plan, AddMarketplaceResult,
    PluginMarketplaceAddExecuteResult, PluginMarketplaceAddPlanError,
    PluginMarketplaceAddPlanResult,
};
use mossen_utils::plugins::marketplace_helpers::get_marketplace_source_display;
use mossen_utils::plugins::parse_marketplace_input::parse_marketplace_input;
use mossen_utils::plugins::plugin_install_plan::{
    execute_plugin_install_plan, get_plugin_install_plan, DependencyResolution, InstallResult,
    PluginInstallPlan, PluginInstallPlanError, PluginInstallResolver, PluginInstaller,
};
use mossen_utils::plugins::plugin_policy::is_plugin_blocked_by_policy;
use mossen_utils::plugins::schemas::{
    InstalledPluginsFileV2, KnownMarketplacesFile, MarketplaceSource, PluginMarketplace,
    PluginMarketplaceEntry,
};
use mossen_utils::plugins::source_status::{
    describe_plugin_sources, DeclaredMarketplaceInfo, KnownMarketplaceInfo, PluginSourceStatus,
};
use mossen_utils::plugins::status_ops::{
    describe_plugin_status, InstalledPluginEntry, InstalledPluginsData, PluginStatusSummary,
};
use mossen_utils::settings::{self, SettingSource};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};
use crate::plugin_parse_args::{parse_plugin_args, ParsedCommand};

/// Plugin directive — manages the plugin ecosystem including installation,
/// removal, marketplace visibility, and plugin cache maintenance.
pub struct PluginDirective;

#[derive(Clone)]
struct FilePluginContext {
    cwd: PathBuf,
    config_home: PathBuf,
}

impl FilePluginContext {
    fn from_command(ctx: &CommandContext) -> Self {
        Self {
            cwd: ctx.cwd.clone(),
            config_home: mossen_utils::naming::get_resolved_config_home_dir(),
        }
    }

    fn plugins_dir(&self) -> PathBuf {
        self.config_home.join("plugins")
    }

    fn cache_dir(&self) -> PathBuf {
        self.plugins_dir().join("cache")
    }

    fn marketplaces_dir(&self) -> PathBuf {
        self.plugins_dir().join("marketplaces")
    }

    fn settings_path_for_source(&self, source: &str) -> Option<PathBuf> {
        let source = setting_source_from_name(source)?;
        settings::get_settings_file_path_for_source(source, &self.cwd, &self.config_home, None)
    }

    fn read_settings_value_for_source(&self, source: &str) -> Option<Value> {
        let path = self.settings_path_for_source(source)?;
        let content = std::fs::read_to_string(path).ok()?;
        if content.trim().is_empty() {
            return Some(Value::Object(Map::new()));
        }
        serde_json::from_str(&content).ok()
    }

    fn update_raw_settings_for_source(
        &self,
        source: &str,
        updates: HashMap<String, Value>,
    ) -> Result<()> {
        let path = self
            .settings_path_for_source(source)
            .ok_or_else(|| anyhow!("Unsupported settings source: {source}"))?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let existing = match std::fs::read_to_string(&path) {
            Ok(content) if !content.trim().is_empty() => serde_json::from_str::<Value>(&content)
                .unwrap_or_else(|_| Value::Object(Map::new())),
            Ok(_) => Value::Object(Map::new()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Value::Object(Map::new()),
            Err(error) => return Err(error.into()),
        };
        let mut root = existing.as_object().cloned().unwrap_or_default();

        for (key, value) in updates {
            if key == "enabledPlugins" {
                merge_object_field_with_null_removal(&mut root, &key, value);
            } else if key == "extraKnownMarketplaces" {
                merge_object_field_with_null_removal(&mut root, &key, value);
            } else if value.is_null() {
                root.remove(&key);
            } else {
                root.insert(key, value);
            }
        }

        let json = serde_json::to_string_pretty(&Value::Object(root))?;
        std::fs::write(&path, format!("{json}\n"))?;
        settings::reset_settings_cache();
        Ok(())
    }

    fn save_marketplace_to_user_settings(
        &self,
        name: &str,
        source: &MarketplaceSource,
    ) -> Result<()> {
        let mut updates = HashMap::new();
        updates.insert(
            "extraKnownMarketplaces".to_string(),
            serde_json::json!({
                name: {
                    "source": source,
                }
            }),
        );
        self.update_raw_settings_for_source("userSettings", updates)
    }
}

impl PluginOperationsContext for FilePluginContext {
    fn get_original_cwd(&self) -> PathBuf {
        self.cwd.clone()
    }

    fn is_builtin_plugin_id(&self, id: &str) -> bool {
        id.ends_with("@builtin")
    }

    fn get_settings_for_source(&self, source: &str) -> Option<HashMap<String, Value>> {
        let value = self.read_settings_value_for_source(source)?;
        let object = value.as_object()?;
        Some(
            object
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
        )
    }

    fn update_settings_for_source(
        &self,
        source: &str,
        settings: HashMap<String, Value>,
    ) -> std::result::Result<(), String> {
        self.update_raw_settings_for_source(source, settings)
            .map_err(|error| error.to_string())
    }

    fn is_plugin_blocked_by_policy(&self, plugin_id: &str) -> bool {
        is_plugin_blocked_by_policy(plugin_id)
    }
}

#[derive(Clone)]
struct FilePluginInstallResolver {
    runtime: FilePluginContext,
}

impl FilePluginInstallResolver {
    fn new(runtime: FilePluginContext) -> Self {
        Self { runtime }
    }
}

#[async_trait]
impl PluginInstallResolver for FilePluginInstallResolver {
    async fn get_plugin_by_id(&self, id: &str) -> Option<(PluginMarketplaceEntry, String)> {
        lookup_marketplace_plugin(&self.runtime, id).await
    }

    async fn get_marketplace_cache_only(&self, marketplace: &str) -> Option<Vec<String>> {
        let marketplace_data = read_marketplace_by_name(&self.runtime, marketplace).await?;
        let mut allowed = marketplace_data
            .allow_cross_marketplace_dependencies_on
            .unwrap_or_default();
        if !allowed.iter().any(|name| name == marketplace) {
            allowed.push(marketplace.to_string());
        }
        Some(allowed)
    }

    fn get_enabled_plugin_ids_for_scope(&self, scope: &str) -> HashSet<String> {
        enabled_plugins_for_source(&self.runtime, scope)
    }

    fn is_plugin_blocked_by_policy(&self, plugin_id: &str) -> bool {
        is_plugin_blocked_by_policy(plugin_id)
    }

    fn parse_plugin_identifier(&self, id: &str) -> (String, Option<String>) {
        parse_plugin_identifier_parts(id)
    }

    fn scope_to_setting_source(&self, scope: &str) -> String {
        scope_to_setting_source_name(scope).to_string()
    }

    async fn resolve_dependency_closure(
        &self,
        plugin_id: &str,
        enabled_ids: &HashSet<String>,
        allowed_cross: &HashSet<String>,
    ) -> DependencyResolution {
        let runtime = self.runtime.clone();
        let result = dependency_resolver::resolve_dependency_closure(
            plugin_id,
            move |id| {
                let runtime = runtime.clone();
                async move {
                    let (entry, _) = lookup_marketplace_plugin(&runtime, &id).await?;
                    Some(DependencyLookupResult {
                        dependencies: entry.dependencies.unwrap_or_default(),
                    })
                }
            },
            enabled_ids,
            allowed_cross,
        )
        .await;

        match result {
            ResolutionResult::Ok { closure } => DependencyResolution {
                ok: true,
                closure,
                error_reason: None,
            },
            ResolutionResult::Cycle { chain } => DependencyResolution {
                ok: false,
                closure: Vec::new(),
                error_reason: Some(format!("dependency cycle: {}", chain.join(" -> "))),
            },
            ResolutionResult::NotFound {
                missing,
                required_by,
            } => DependencyResolution {
                ok: false,
                closure: Vec::new(),
                error_reason: Some(format!(
                    "missing dependency {missing} required by {required_by}"
                )),
            },
            ResolutionResult::CrossMarketplace {
                dependency,
                required_by,
            } => DependencyResolution {
                ok: false,
                closure: Vec::new(),
                error_reason: Some(format!(
                    "cross-marketplace dependency {dependency} required by {required_by}"
                )),
            },
        }
    }

    fn format_resolution_error(&self, resolution: &DependencyResolution) -> String {
        resolution
            .error_reason
            .clone()
            .unwrap_or_else(|| "dependency resolution failed".to_string())
    }

    fn format_dependency_count_suffix(&self, deps: &[String]) -> String {
        dependency_resolver::format_dependency_count_suffix(deps)
    }
}

struct FilePluginInstaller {
    runtime: FilePluginContext,
}

#[async_trait]
impl PluginInstaller for FilePluginInstaller {
    async fn install_resolved_plugin(
        &self,
        plan: &PluginInstallPlan,
    ) -> std::result::Result<InstallResult, String> {
        let scope = InstallableScope::from_str(&plan.scope)?;
        let result = install_plugin_op(&plan.plugin_id, scope, &self.runtime).await;
        if !result.success {
            return Err(result.message);
        }
        Ok(InstallResult {
            closure: plan.dependency_closure.clone(),
            dep_note: plan.dep_note.clone(),
        })
    }
}

#[async_trait]
impl Directive for PluginDirective {
    fn name(&self) -> &str {
        "plugin"
    }

    fn aliases(&self) -> &[&str] {
        &["plugins"]
    }

    fn description(&self) -> &str {
        "Manage plugins"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn argument_hint(&self) -> &str {
        "[list|install|enable|disable|uninstall|status|sources|prune|marketplace] [args]"
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        let runtime = FilePluginContext::from_command(ctx);
        let joined_args;
        let parsed = parse_plugin_args(if args.is_empty() {
            None
        } else {
            joined_args = args.join(" ");
            Some(&joined_args)
        });

        match parsed {
            ParsedCommand::Menu | ParsedCommand::Manage => {
                Ok(CommandResult::Text(format_plugin_list(&runtime)))
            }
            ParsedCommand::Help => Ok(CommandResult::Text(plugin_help_text())),
            ParsedCommand::Install {
                plugin,
                marketplace,
            } => {
                run_settings_plugin_operation(
                    plugin_target(plugin, marketplace),
                    "install",
                    &runtime,
                )
                .await
            }
            ParsedCommand::InstallPlan {
                plugin,
                scope,
                confirm_token,
            } => run_install_plan(plugin, scope, confirm_token, &runtime).await,
            ParsedCommand::Uninstall { plugin } => {
                run_settings_plugin_operation(plugin, "uninstall", &runtime).await
            }
            ParsedCommand::Enable { plugin } => {
                run_settings_plugin_operation(plugin, "enable", &runtime).await
            }
            ParsedCommand::Disable { plugin } => {
                run_settings_plugin_operation(plugin, "disable", &runtime).await
            }
            ParsedCommand::Validate { path } => run_validate(path),
            ParsedCommand::Marketplace { action, target } => {
                run_marketplace_command(action, target, &runtime).await
            }
            ParsedCommand::MarketplaceAddPlan {
                target,
                confirm_token,
            } => run_marketplace_add_plan(target, confirm_token, &runtime).await,
            ParsedCommand::Prune { confirm_token } => run_prune(confirm_token, &runtime).await,
            ParsedCommand::Status => run_status(&runtime).await,
            ParsedCommand::Sources => run_sources(&runtime).await,
            ParsedCommand::Paths => Ok(CommandResult::Text(format_plugin_paths(&runtime))),
        }
    }
}

fn merge_object_field_with_null_removal(root: &mut Map<String, Value>, key: &str, value: Value) {
    let target = root
        .entry(key.to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if !target.is_object() {
        *target = Value::Object(Map::new());
    }
    let Some(target_obj) = target.as_object_mut() else {
        return;
    };
    let Some(update_obj) = value.as_object() else {
        if value.is_null() {
            root.remove(key);
        } else {
            root.insert(key.to_string(), value);
        }
        return;
    };
    for (entry_key, update_value) in update_obj {
        if update_value.is_null() {
            target_obj.remove(entry_key);
        } else {
            target_obj.insert(entry_key.clone(), update_value.clone());
        }
    }
}

fn setting_source_from_name(source: &str) -> Option<SettingSource> {
    match source {
        "userSettings" | "user" => Some(SettingSource::UserSettings),
        "projectSettings" | "project" => Some(SettingSource::ProjectSettings),
        "localSettings" | "local" => Some(SettingSource::LocalSettings),
        _ => None,
    }
}

fn scope_to_setting_source_name(scope: &str) -> &'static str {
    match scope {
        "project" => "projectSettings",
        "local" => "localSettings",
        _ => "userSettings",
    }
}

fn plugin_target(plugin: Option<String>, marketplace: Option<String>) -> Option<String> {
    match (plugin, marketplace) {
        (Some(plugin), Some(marketplace)) => Some(format!("{plugin}@{marketplace}")),
        (Some(plugin), None) => Some(plugin),
        (None, Some(source)) => Some(source),
        (None, None) => None,
    }
}

async fn run_settings_plugin_operation(
    plugin: Option<String>,
    operation: &str,
    runtime: &FilePluginContext,
) -> Result<CommandResult> {
    let Some(plugin) = plugin.filter(|value| !value.trim().is_empty()) else {
        return Ok(CommandResult::Error(format!(
            "Usage: /plugin {operation} <plugin@marketplace>"
        )));
    };
    if plugin.starts_with("http://")
        || plugin.starts_with("https://")
        || plugin.starts_with("file://")
        || plugin.contains('/')
        || plugin.contains('\\')
    {
        return Ok(CommandResult::Error(
            "Direct path/URL plugin install is not enabled from /plugin install. Add a marketplace first with /plugin marketplace add --dry-run <path|url>.".to_string(),
        ));
    }

    let result = match operation {
        "install" => install_plugin_op(&plugin, InstallableScope::User, runtime).await,
        "uninstall" => uninstall_plugin_op(&plugin, InstallableScope::User, true, runtime).await,
        "enable" => enable_plugin_op(&plugin, Some(InstallableScope::User), runtime).await,
        "disable" => disable_plugin_op(&plugin, Some(InstallableScope::User), runtime).await,
        _ => unreachable!(),
    };
    operation_result_to_command(result)
}

fn operation_result_to_command(result: PluginOperationResult) -> Result<CommandResult> {
    if result.success {
        Ok(CommandResult::Text(result.message))
    } else {
        Ok(CommandResult::Error(result.message))
    }
}

async fn run_install_plan(
    plugin: Option<String>,
    scope: Option<String>,
    confirm_token: Option<String>,
    runtime: &FilePluginContext,
) -> Result<CommandResult> {
    if let Some(token) = confirm_token {
        let installer = FilePluginInstaller {
            runtime: runtime.clone(),
        };
        return match execute_plugin_install_plan(&token, &installer).await {
            Ok(plan) => Ok(CommandResult::Text(format!(
                "Installed plugin {} in {} scope{}.",
                plan.plugin_id, plan.scope, plan.dep_note
            ))),
            Err(error) => Ok(CommandResult::Error(format_plugin_install_plan_error(
                error,
            ))),
        };
    }

    let resolver = FilePluginInstallResolver::new(runtime.clone());
    let scope = scope.as_deref().unwrap_or("user");
    match get_plugin_install_plan(plugin.as_deref(), Some(scope), &resolver).await {
        Ok(plan) => Ok(CommandResult::Text(format!(
            "Plugin install dry-run\nPlugin: {}\nScope: {}\nDependencies: {}\nConfirm: /plugin install --confirm {}",
            plan.plugin_id,
            plan.scope,
            if plan.dependency_closure.is_empty() {
                "(none)".to_string()
            } else {
                plan.dependency_closure.join(", ")
            },
            plan.token
        ))),
        Err(error) => Ok(CommandResult::Error(format_plugin_install_plan_error(error))),
    }
}

fn format_plugin_install_plan_error(error: PluginInstallPlanError) -> String {
    match error {
        PluginInstallPlanError::MissingPlugin => {
            "Usage: /plugin install --dry-run <plugin@marketplace> [--scope user|project|local]"
                .to_string()
        }
        PluginInstallPlanError::PluginNotFound { plugin } => {
            format!("Plugin not found in cached marketplaces: {plugin}")
        }
        PluginInstallPlanError::MarketplaceRequired { plugin } => {
            format!("Marketplace is required for install dry-run: {plugin}@<marketplace>")
        }
        PluginInstallPlanError::InvalidGithubTarget { reason } => reason,
        PluginInstallPlanError::InvalidScope { scope } => format!(
            "Invalid scope {}. Must be user, project, or local.",
            scope.unwrap_or_else(|| "(missing)".to_string())
        ),
        PluginInstallPlanError::BlockedByPolicy { plugin_id } => {
            format!("Plugin is blocked by policy: {plugin_id}")
        }
        PluginInstallPlanError::ResolutionFailed { message } => message,
        PluginInstallPlanError::UnknownToken { token } => {
            format!("Unknown or already-used plugin install token: {token}")
        }
        PluginInstallPlanError::ExpiredToken { token } => {
            format!("Expired plugin install token: {token}")
        }
        PluginInstallPlanError::InstallFailed { message } => message,
    }
}

fn run_validate(path: Option<String>) -> Result<CommandResult> {
    let Some(path) = path.filter(|value| !value.trim().is_empty()) else {
        return Ok(CommandResult::Error(
            "Usage: /plugin validate <path-to-plugin>".to_string(),
        ));
    };
    let manifest = PathBuf::from(&path)
        .join(".mossen-plugin")
        .join("plugin.json");
    if manifest.exists() {
        Ok(CommandResult::Text(format!(
            "Plugin manifest found: {}",
            manifest.display()
        )))
    } else {
        Ok(CommandResult::Error(format!(
            "Plugin manifest not found: {}",
            manifest.display()
        )))
    }
}

async fn run_marketplace_command(
    action: Option<String>,
    target: Option<String>,
    runtime: &FilePluginContext,
) -> Result<CommandResult> {
    match action.as_deref() {
        Some("add") => run_marketplace_add_plan(target, None, runtime).await,
        Some("list") | None => run_sources(runtime).await,
        Some("remove") => run_marketplace_remove(target, runtime).await,
        Some("update") => Ok(CommandResult::Error(
            "Unsupported marketplace action: update. Use /plugin marketplace add --dry-run <path|url|owner/repo> to refresh a source.".to_string(),
        )),
        Some(other) => Ok(CommandResult::Error(format!(
            "Unsupported marketplace action: {other}"
        ))),
    }
}

async fn run_marketplace_remove(
    target: Option<String>,
    runtime: &FilePluginContext,
) -> Result<CommandResult> {
    let Some(name) = target
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(CommandResult::Error(
            "Usage: /plugin marketplace remove <marketplace-name>".to_string(),
        ));
    };

    let mut known = read_known_marketplaces(runtime);
    let removed_known = known.remove(name).is_some();
    if removed_known {
        write_known_marketplaces(runtime, &known)?;
        remove_marketplace_cache_files(runtime, name);
    }

    let removed_declared = remove_marketplace_from_settings(runtime, name);
    if !removed_known && !removed_declared {
        return Ok(CommandResult::Error(format!(
            "Marketplace not found: {name}"
        )));
    }

    mossen_utils::plugins::marketplace_manager::clear_marketplaces_cache();
    let mut details = Vec::new();
    if removed_known {
        details.push("known cache");
    }
    if removed_declared {
        details.push("settings");
    }
    Ok(CommandResult::Text(format!(
        "Removed marketplace {name} from {}.",
        details.join(" and ")
    )))
}

async fn run_marketplace_add_plan(
    target: Option<String>,
    confirm_token: Option<String>,
    runtime: &FilePluginContext,
) -> Result<CommandResult> {
    if let Some(token) = confirm_token {
        let runtime_for_save = runtime.clone();
        let result = execute_plugin_marketplace_add_plan(
            &token,
            |plan| {
                let source = plan.source.clone();
                Box::pin(async move {
                    Ok(AddMarketplaceResult {
                        name: marketplace_name_for_source(&source),
                        already_materialized: false,
                        resolved_source: source,
                    })
                })
            },
            move |name, source| {
                let _ = runtime_for_save.save_marketplace_to_user_settings(name, source);
            },
            || {
                mossen_utils::plugins::marketplace_manager::clear_marketplaces_cache();
                mossen_utils::plugins::load_plugin_commands::clear_plugin_command_cache();
            },
        )
        .await;
        return match result {
            PluginMarketplaceAddExecuteResult::Ok {
                name,
                already_materialized,
                ..
            } => Ok(CommandResult::Text(format!(
                "Added marketplace {name}{}.",
                if already_materialized {
                    " (already materialized)"
                } else {
                    ""
                }
            ))),
            PluginMarketplaceAddExecuteResult::Err { error } => {
                Ok(CommandResult::Error(format_marketplace_add_error(error)))
            }
        };
    }

    let input = target.as_deref();
    let parse_future = async {
        match input {
            Some(value) => parse_marketplace_input(value).await,
            None => Ok(None),
        }
    };
    match get_plugin_marketplace_add_plan(input, parse_future, get_marketplace_source_display).await
    {
        PluginMarketplaceAddPlanResult::Ok { plan } => Ok(CommandResult::Text(format!(
            "Plugin marketplace add dry-run\nSource: {}\nResolved: {}\nConfirm: /plugin marketplace add --confirm {}",
            plan.input, plan.source_display, plan.token
        ))),
        PluginMarketplaceAddPlanResult::Err { error } => {
            Ok(CommandResult::Error(format_marketplace_add_error(error)))
        }
    }
}

fn format_marketplace_add_error(error: PluginMarketplaceAddPlanError) -> String {
    match error {
        PluginMarketplaceAddPlanError::MissingSource => {
            "Usage: /plugin marketplace add --dry-run <path|url|owner/repo>".to_string()
        }
        PluginMarketplaceAddPlanError::InvalidSource { message } => message,
        PluginMarketplaceAddPlanError::UnknownToken { token } => {
            format!("Unknown or already-used marketplace add token: {token}")
        }
        PluginMarketplaceAddPlanError::ExpiredToken { token } => {
            format!("Expired marketplace add token: {token}")
        }
        PluginMarketplaceAddPlanError::AddFailed { message } => message,
    }
}

async fn run_prune(
    confirm_token: Option<String>,
    runtime: &FilePluginContext,
) -> Result<CommandResult> {
    let installed_paths = installed_plugin_paths(runtime);
    if let Some(token) = confirm_token {
        return match execute_plugin_prune_plan(&token, &installed_paths).await {
            Ok(result) => Ok(CommandResult::Text(format!(
                "Plugin prune complete: marked {}, deleted {}, cleaned {}, errors {}.",
                result.marked.len(),
                result.deleted.len(),
                result.cleaned_dirs.len(),
                result.errors.len()
            ))),
            Err(error) => Ok(CommandResult::Error(format!(
                "Plugin prune failed: {error:?}"
            ))),
        };
    }

    let plan = get_plugin_prune_plan(&runtime.cache_dir(), &installed_paths, false).await;
    Ok(CommandResult::Text(format!(
        "Plugin prune dry-run\nExpired orphans: {}\nUnmarked orphans: {}\nFresh orphans: {}\nInstalled skipped: {}\nConfirm: /plugin prune --confirm {}",
        plan.expired_orphans.len(),
        plan.unmarked_orphans.len(),
        plan.fresh_orphans.len(),
        plan.installed_skipped.len(),
        plan.token
    )))
}

async fn run_status(runtime: &FilePluginContext) -> Result<CommandResult> {
    let summary = plugin_status_summary(runtime).await;
    Ok(CommandResult::Text(format_status_summary(&summary)))
}

async fn plugin_status_summary(runtime: &FilePluginContext) -> PluginStatusSummary {
    let installed_paths = installed_plugin_paths(runtime);
    let cache_dir = runtime.cache_dir();
    let config_home = runtime.config_home.to_string_lossy().to_string();
    let marketplaces_dir = runtime.marketplaces_dir().to_string_lossy().to_string();
    let runtime_for_installed = runtime.clone();
    describe_plugin_status(
        &config_home,
        summarize_plugin_cache(&cache_dir, &installed_paths, false),
        move || marketplaces_dir.clone(),
        move || load_installed_status_data(&runtime_for_installed),
    )
    .await
}

async fn run_sources(runtime: &FilePluginContext) -> Result<CommandResult> {
    let status = plugin_source_status(runtime).await;
    Ok(CommandResult::Text(format_source_status(&status)))
}

async fn plugin_source_status(runtime: &FilePluginContext) -> PluginSourceStatus {
    let declared = load_declared_marketplaces(runtime);
    let known = load_known_marketplaces_info(runtime);
    let plugins_dir = runtime.plugins_dir().to_string_lossy().to_string();
    let marketplace_dir = runtime.marketplaces_dir().to_string_lossy().to_string();
    describe_plugin_sources(
        || declared.clone(),
        async { known.clone() },
        get_marketplace_source_display,
        || plugins_dir.clone(),
        || marketplace_dir.clone(),
        Vec::new,
        &mossen_utils::plugins::official_marketplace::OFFICIAL_MARKETPLACE_SOURCE,
    )
    .await
}

fn format_plugin_list(runtime: &FilePluginContext) -> String {
    let rows = configured_plugin_rows(runtime);
    if rows.is_empty() {
        return "Configured plugins\n==================\n\nNo plugins configured in user, project, or local settings.".to_string();
    }
    let mut output = format!(
        "Configured plugins\n==================\n\n{} plugin setting(s):\n",
        rows.len()
    );
    for row in rows {
        output.push_str(&format!(
            "- {} [{}] ({})\n",
            row.plugin_id,
            if row.enabled { "enabled" } else { "disabled" },
            row.scope
        ));
    }
    output
}

fn plugin_help_text() -> String {
    "Plugin Management\n=================\n\n\
     /plugin list\n\
     /plugin install <plugin@marketplace>\n\
     /plugin install --dry-run <plugin@marketplace> [--scope user|project|local]\n\
     /plugin install --confirm <token>\n\
     /plugin enable <plugin@marketplace>\n\
     /plugin disable <plugin@marketplace>\n\
     /plugin uninstall <plugin@marketplace>\n\
     /plugin status\n\
     /plugin sources\n\
     /plugin paths\n\
     /plugin prune [--confirm <token>]\n\
     /plugin marketplace add --dry-run <path|url|owner/repo>\n\
     /plugin marketplace add --confirm <token>"
        .to_string()
}

fn format_plugin_paths(runtime: &FilePluginContext) -> String {
    format!(
        "Plugin paths\n============\n\nRoot: {}\nCache: {}\nMarketplaces: {}\nRegistry: {}",
        runtime.plugins_dir().display(),
        runtime.cache_dir().display(),
        runtime.marketplaces_dir().display(),
        runtime
            .plugins_dir()
            .join("installed_plugins.json")
            .display()
    )
}

fn format_status_summary(summary: &PluginStatusSummary) -> String {
    format!(
        "Plugin status\n=============\n\nRoot: {} ({})\nCache versions: {}\nInstalled records: {}\nInstalled versions: {}\nPrune eligible: {}\nSuggested: {}",
        summary.plugin_root_path,
        if summary.plugin_root_exists { "exists" } else { "missing" },
        summary.cache.cache_version_count,
        summary.installed_record_count,
        summary.installed_version_count,
        summary.prune_eligible,
        summary.suggested_command
    )
}

fn format_source_status(status: &PluginSourceStatus) -> String {
    let mut output = format!(
        "Plugin sources\n==============\n\nRoot: {}\nMarketplace cache: {}\nOfficial: {} (known: {}, declared: {})\n",
        status.plugin_root,
        status.marketplace_cache_dir,
        status.official_marketplace.name,
        status.official_marketplace.known,
        status.official_marketplace.declared
    );
    if status.entries.is_empty() {
        output.push_str("\nNo marketplace sources configured.\n");
    } else {
        output.push_str("\nSources:\n");
        for entry in &status.entries {
            output.push_str(&format!(
                "- {} [declared: {}, known: {}] {}\n",
                entry.name, entry.declared, entry.known, entry.source_display
            ));
        }
    }
    output
}

#[derive(Debug)]
struct ConfiguredPluginRow {
    plugin_id: String,
    enabled: bool,
    scope: &'static str,
}

fn configured_plugin_rows(runtime: &FilePluginContext) -> Vec<ConfiguredPluginRow> {
    let mut rows = Vec::new();
    for (source, scope) in [
        ("userSettings", "user"),
        ("projectSettings", "project"),
        ("localSettings", "local"),
    ] {
        let Some(settings) = runtime.read_settings_value_for_source(source) else {
            continue;
        };
        let Some(enabled_plugins) = settings.get("enabledPlugins").and_then(Value::as_object)
        else {
            continue;
        };
        for (plugin_id, enabled) in enabled_plugins {
            let Some(enabled) = enabled.as_bool() else {
                continue;
            };
            rows.push(ConfiguredPluginRow {
                plugin_id: plugin_id.clone(),
                enabled,
                scope,
            });
        }
    }
    rows.sort_by(|left, right| {
        left.plugin_id
            .cmp(&right.plugin_id)
            .then(left.scope.cmp(right.scope))
    });
    rows
}

fn enabled_plugins_for_source(runtime: &FilePluginContext, source: &str) -> HashSet<String> {
    runtime
        .read_settings_value_for_source(source)
        .and_then(|settings| settings.get("enabledPlugins").cloned())
        .and_then(|enabled| enabled.as_object().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(id, enabled)| (enabled.as_bool() == Some(true)).then_some(id))
        .collect()
}

fn installed_plugin_paths(runtime: &FilePluginContext) -> HashSet<PathBuf> {
    let path = runtime.plugins_dir().join("installed_plugins.json");
    let data = std::fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str::<InstalledPluginsFileV2>(&content).ok())
        .unwrap_or_default();
    data.plugins
        .values()
        .flat_map(|entries| entries.iter())
        .map(|entry| PathBuf::from(&entry.install_path))
        .collect()
}

fn load_installed_status_data(runtime: &FilePluginContext) -> Result<InstalledPluginsData> {
    let path = runtime.plugins_dir().join("installed_plugins.json");
    let data = std::fs::read_to_string(path)
        .map_err(anyhow::Error::from)
        .and_then(|content| Ok(serde_json::from_str::<InstalledPluginsFileV2>(&content)?))?;
    Ok(InstalledPluginsData {
        plugins: data
            .plugins
            .into_iter()
            .map(|(id, entries)| {
                (
                    id,
                    entries
                        .into_iter()
                        .map(|entry| InstalledPluginEntry {
                            scope: entry.scope.to_string(),
                            project_path: entry.project_path,
                        })
                        .collect(),
                )
            })
            .collect(),
    })
}

fn load_declared_marketplaces(
    runtime: &FilePluginContext,
) -> HashMap<String, DeclaredMarketplaceInfo> {
    let mut declared = HashMap::new();
    for source in ["userSettings", "projectSettings", "localSettings"] {
        let Some(settings) = runtime.read_settings_value_for_source(source) else {
            continue;
        };
        let Some(extra) = settings
            .get("extraKnownMarketplaces")
            .and_then(Value::as_object)
        else {
            continue;
        };
        for (name, value) in extra {
            let source_value = value
                .get("source")
                .cloned()
                .unwrap_or_else(|| value.clone());
            let parsed_source = serde_json::from_value::<MarketplaceSource>(source_value).ok();
            declared.insert(
                name.clone(),
                DeclaredMarketplaceInfo {
                    source: parsed_source,
                    auto_update: value.get("autoUpdate").and_then(Value::as_bool),
                    source_is_fallback: value.get("sourceIsFallback").and_then(Value::as_bool),
                },
            );
        }
    }
    declared
}

fn load_known_marketplaces_info(
    runtime: &FilePluginContext,
) -> HashMap<String, KnownMarketplaceInfo> {
    read_known_marketplaces(runtime)
        .into_iter()
        .map(|(name, entry)| {
            (
                name,
                KnownMarketplaceInfo {
                    source: Some(entry.source),
                    install_location: Some(entry.install_location),
                    auto_update: entry.auto_update,
                },
            )
        })
        .collect()
}

fn read_known_marketplaces(runtime: &FilePluginContext) -> KnownMarketplacesFile {
    let path = runtime.plugins_dir().join("known_marketplaces.json");
    std::fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str::<KnownMarketplacesFile>(&content).ok())
        .unwrap_or_default()
}

fn write_known_marketplaces(
    runtime: &FilePluginContext,
    known: &KnownMarketplacesFile,
) -> Result<()> {
    let path = runtime.plugins_dir().join("known_marketplaces.json");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(known)?;
    std::fs::write(path, format!("{content}\n"))?;
    Ok(())
}

fn remove_marketplace_cache_files(runtime: &FilePluginContext, name: &str) {
    let cache_dir = runtime.marketplaces_dir();
    let _ = std::fs::remove_dir_all(cache_dir.join(name));
    let _ = std::fs::remove_file(cache_dir.join(format!("{name}.json")));
}

fn remove_marketplace_from_settings(runtime: &FilePluginContext, name: &str) -> bool {
    let mut removed = false;
    for source in ["userSettings", "projectSettings", "localSettings"] {
        let Some(settings) = runtime.read_settings_value_for_source(source) else {
            continue;
        };
        let contains_name = settings
            .get("extraKnownMarketplaces")
            .and_then(Value::as_object)
            .map(|extra| extra.contains_key(name))
            .unwrap_or(false);
        if contains_name {
            let mut updates = HashMap::new();
            updates.insert(
                "extraKnownMarketplaces".to_string(),
                serde_json::json!({ name: null }),
            );
            if runtime
                .update_raw_settings_for_source(source, updates)
                .is_ok()
            {
                removed = true;
            }
        }
    }
    removed
}

async fn read_marketplace_by_name(
    runtime: &FilePluginContext,
    marketplace: &str,
) -> Option<PluginMarketplace> {
    let known = read_known_marketplaces(runtime);
    let entry = known.get(marketplace)?;
    read_marketplace_from_location(Path::new(&entry.install_location)).await
}

async fn read_marketplace_from_location(path: &Path) -> Option<PluginMarketplace> {
    let nested = path.join(".mossen-plugin").join("marketplace.json");
    if let Ok(content) = tokio::fs::read_to_string(&nested).await {
        return serde_json::from_str(&content).ok();
    }
    let direct_child = path.join("marketplace.json");
    if let Ok(content) = tokio::fs::read_to_string(&direct_child).await {
        return serde_json::from_str(&content).ok();
    }
    let content = tokio::fs::read_to_string(path).await.ok()?;
    serde_json::from_str(&content).ok()
}

async fn lookup_marketplace_plugin(
    runtime: &FilePluginContext,
    plugin_id: &str,
) -> Option<(PluginMarketplaceEntry, String)> {
    let (plugin_name, marketplace_name) = parse_plugin_identifier_parts(plugin_id);
    let marketplace_name = marketplace_name?;
    let known = read_known_marketplaces(runtime);
    let known_entry = known.get(&marketplace_name)?;
    let marketplace =
        read_marketplace_from_location(Path::new(&known_entry.install_location)).await?;
    let entry = marketplace
        .plugins
        .into_iter()
        .find(|plugin| plugin.name == plugin_name)?;
    Some((entry, known_entry.install_location.clone()))
}

fn parse_plugin_identifier_parts(plugin_id: &str) -> (String, Option<String>) {
    if let Some((name, marketplace)) = plugin_id.rsplit_once('@') {
        (name.to_string(), Some(marketplace.to_string()))
    } else {
        (plugin_id.to_string(), None)
    }
}

fn marketplace_name_for_source(source: &MarketplaceSource) -> String {
    let raw = match source {
        MarketplaceSource::GitHub { repo, .. } => repo.rsplit('/').next().unwrap_or(repo),
        MarketplaceSource::Git { url, .. } | MarketplaceSource::Url { url, .. } => url
            .trim_end_matches(".git")
            .rsplit('/')
            .next()
            .unwrap_or(url),
        MarketplaceSource::Directory { path } | MarketplaceSource::File { path } => Path::new(path)
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("marketplace"),
        MarketplaceSource::Settings { name, .. } => name,
        MarketplaceSource::Npm { package } => package,
        MarketplaceSource::HostPattern { host_pattern } => host_pattern,
        MarketplaceSource::PathPattern { path_pattern } => path_pattern,
    };
    sanitize_marketplace_name(raw)
}

fn sanitize_marketplace_name(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if sanitized.is_empty() {
        "marketplace".to_string()
    } else {
        sanitized
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::CommandContext;
    use mossen_utils::plugins::schemas::{KnownMarketplace, PluginAuthor, PluginSource};

    fn test_context(cwd: PathBuf) -> CommandContext {
        CommandContext {
            cwd,
            is_non_interactive: false,
            is_remote_mode: false,
            is_custom_backend: false,
            user_type: None,
            env_vars: HashMap::new(),
            product_name: "Mossen".to_string(),
            cli_name: "mossen".to_string(),
            version: "test".to_string(),
            build_time: None,
            cost_snapshot: Default::default(),
        }
    }

    fn text(result: CommandResult) -> String {
        match result {
            CommandResult::Text(value)
            | CommandResult::System(value)
            | CommandResult::Error(value) => value,
            other => panic!("expected text-like result, got {other:?}"),
        }
    }

    async fn assert_plugin_directive_core_actions() {
        let _guard = crate::test_support::env_lock();
        let temp = tempfile::tempdir().expect("tempdir");
        std::env::set_var("MOSSEN_CONFIG_DIR", temp.path().join("config"));
        let ctx = test_context(temp.path().join("project"));
        let directive = PluginDirective;

        let installed = text(
            directive
                .execute(&["install", "demo@local-market"], &ctx)
                .await
                .unwrap(),
        );
        assert!(installed.contains("Successfully installed plugin: demo@local-market"));
        let settings_path = temp.path().join("config").join("settings.json");
        let settings: Value =
            serde_json::from_str(&std::fs::read_to_string(&settings_path).unwrap()).unwrap();
        assert_eq!(settings["enabledPlugins"]["demo@local-market"], true);

        let disabled = text(
            directive
                .execute(&["disable", "demo@local-market"], &ctx)
                .await
                .unwrap(),
        );
        assert!(disabled.contains("Successfully disabled plugin"));
        let settings: Value =
            serde_json::from_str(&std::fs::read_to_string(&settings_path).unwrap()).unwrap();
        assert_eq!(settings["enabledPlugins"]["demo@local-market"], false);

        let uninstalled = text(
            directive
                .execute(&["uninstall", "demo@local-market"], &ctx)
                .await
                .unwrap(),
        );
        assert!(uninstalled.contains("Successfully uninstalled plugin"));
        let settings: Value =
            serde_json::from_str(&std::fs::read_to_string(&settings_path).unwrap()).unwrap();
        assert!(settings["enabledPlugins"]
            .get("demo@local-market")
            .is_none());

        std::env::remove_var("MOSSEN_CONFIG_DIR");
    }

    #[tokio::test]
    async fn plugin_directive_routes_core_actions() {
        assert_plugin_directive_core_actions().await;
    }

    #[tokio::test]
    async fn plugin_directive_writes_settings_for_core_operations() {
        assert_plugin_directive_core_actions().await;
    }

    #[tokio::test]
    async fn plugin_directive_install_plan_uses_cached_marketplace_and_confirm_token() {
        let _guard = crate::test_support::env_lock();
        let temp = tempfile::tempdir().expect("tempdir");
        let config_home = temp.path().join("config");
        std::env::set_var("MOSSEN_CONFIG_DIR", &config_home);
        let project = temp.path().join("project");
        let ctx = test_context(project.clone());
        let marketplace_dir = temp.path().join("marketplace-cache");
        std::fs::create_dir_all(&marketplace_dir).unwrap();
        std::fs::write(
            marketplace_dir.join("marketplace.json"),
            serde_json::to_string(&PluginMarketplace {
                name: "local-market".to_string(),
                owner: PluginAuthor {
                    name: "test".to_string(),
                    email: None,
                    url: None,
                },
                plugins: vec![
                    PluginMarketplaceEntry {
                        name: "demo".to_string(),
                        source: PluginSource::RelativePath("./demo".to_string()),
                        dependencies: Some(vec!["helper".to_string()]),
                        ..Default::default()
                    },
                    PluginMarketplaceEntry {
                        name: "helper".to_string(),
                        source: PluginSource::RelativePath("./helper".to_string()),
                        ..Default::default()
                    },
                ],
                force_remove_deleted_plugins: None,
                metadata: None,
                allow_cross_marketplace_dependencies_on: None,
            })
            .unwrap(),
        )
        .unwrap();
        let plugins_dir = config_home.join("plugins");
        std::fs::create_dir_all(&plugins_dir).unwrap();
        std::fs::write(
            plugins_dir.join("known_marketplaces.json"),
            serde_json::to_string(&KnownMarketplacesFile::from([(
                "local-market".to_string(),
                KnownMarketplace {
                    source: MarketplaceSource::Directory {
                        path: marketplace_dir.to_string_lossy().to_string(),
                    },
                    install_location: marketplace_dir.to_string_lossy().to_string(),
                    last_updated: None,
                    auto_update: None,
                },
            )]))
            .unwrap(),
        )
        .unwrap();

        let directive = PluginDirective;
        let dry_run = text(
            directive
                .execute(
                    &[
                        "install",
                        "--dry-run",
                        "demo@local-market",
                        "--scope",
                        "project",
                    ],
                    &ctx,
                )
                .await
                .unwrap(),
        );
        assert!(dry_run.contains("Plugin install dry-run"));
        assert!(dry_run.contains("helper@local-market"));
        let token = dry_run
            .lines()
            .find_map(|line| line.strip_prefix("Confirm: /plugin install --confirm "))
            .expect("confirm token")
            .to_string();

        let confirmed = text(
            directive
                .execute(&["install", "--confirm", &token], &ctx)
                .await
                .unwrap(),
        );
        assert!(confirmed.contains("Installed plugin demo@local-market in project scope"));
        let project_settings = project.join(".mossen").join("settings.json");
        let settings: Value =
            serde_json::from_str(&std::fs::read_to_string(project_settings).unwrap()).unwrap();
        assert_eq!(settings["enabledPlugins"]["demo@local-market"], true);

        std::env::remove_var("MOSSEN_CONFIG_DIR");
    }

    #[tokio::test]
    async fn plugin_directive_surfaces_status_sources_paths_and_prune() {
        let _guard = crate::test_support::env_lock();
        let temp = tempfile::tempdir().expect("tempdir");
        let config_home = temp.path().join("config");
        std::env::set_var("MOSSEN_CONFIG_DIR", &config_home);
        let ctx = test_context(temp.path().join("project"));
        let directive = PluginDirective;

        let status = text(directive.execute(&["status"], &ctx).await.unwrap());
        assert!(status.contains("Plugin status"));

        let sources = text(directive.execute(&["sources"], &ctx).await.unwrap());
        assert!(sources.contains("Plugin sources"));

        let paths = text(directive.execute(&["paths"], &ctx).await.unwrap());
        assert!(paths.contains("Plugin paths"));

        let prune = text(directive.execute(&["prune"], &ctx).await.unwrap());
        assert!(prune.contains("Plugin prune dry-run"));
        assert!(prune.contains("/plugin prune --confirm"));

        std::env::remove_var("MOSSEN_CONFIG_DIR");
    }

    #[tokio::test]
    async fn plugin_directive_marketplace_add_plan_writes_settings() {
        let _guard = crate::test_support::env_lock();
        mossen_utils::plugins::marketplace_add_plan::reset_plugin_marketplace_add_plan_store_for_testing();
        let temp = tempfile::tempdir().expect("tempdir");
        let config_home = temp.path().join("config");
        let marketplace_dir = temp.path().join("local-marketplace");
        std::fs::create_dir_all(&marketplace_dir).unwrap();
        std::env::set_var("MOSSEN_CONFIG_DIR", &config_home);
        let ctx = test_context(temp.path().join("project"));
        let directive = PluginDirective;

        let dry_run = text(
            directive
                .execute(
                    &[
                        "marketplace",
                        "add",
                        "--dry-run",
                        marketplace_dir.to_str().unwrap(),
                    ],
                    &ctx,
                )
                .await
                .unwrap(),
        );
        assert!(dry_run.contains("Plugin marketplace add dry-run"));
        let token = dry_run
            .lines()
            .find_map(|line| line.strip_prefix("Confirm: /plugin marketplace add --confirm "))
            .expect("confirm token")
            .to_string();

        let confirmed = text(
            directive
                .execute(&["marketplace", "add", "--confirm", &token], &ctx)
                .await
                .unwrap(),
        );
        assert!(confirmed.contains("Added marketplace local-marketplace."));
        let settings_path = config_home.join("settings.json");
        let settings: Value =
            serde_json::from_str(&std::fs::read_to_string(settings_path).unwrap()).unwrap();
        assert_eq!(
            settings["extraKnownMarketplaces"]["local-marketplace"]["source"]["source"],
            "directory"
        );

        std::env::remove_var("MOSSEN_CONFIG_DIR");
    }

    #[tokio::test]
    async fn plugin_directive_marketplace_remove_cleans_known_cache_and_settings() {
        let _guard = crate::test_support::env_lock();
        let temp = tempfile::tempdir().expect("tempdir");
        let config_home = temp.path().join("config");
        std::env::set_var("MOSSEN_CONFIG_DIR", &config_home);
        let ctx = test_context(temp.path().join("project"));
        let directive = PluginDirective;

        let plugins_dir = config_home.join("plugins");
        let cache_dir = plugins_dir.join("marketplaces");
        std::fs::create_dir_all(cache_dir.join("local-market")).unwrap();
        std::fs::write(cache_dir.join("local-market.json"), "{}").unwrap();
        std::fs::create_dir_all(&plugins_dir).unwrap();
        std::fs::write(
            plugins_dir.join("known_marketplaces.json"),
            serde_json::to_string(&KnownMarketplacesFile::from([(
                "local-market".to_string(),
                KnownMarketplace {
                    source: MarketplaceSource::Directory {
                        path: temp
                            .path()
                            .join("local-market")
                            .to_string_lossy()
                            .to_string(),
                    },
                    install_location: cache_dir.join("local-market").to_string_lossy().to_string(),
                    last_updated: None,
                    auto_update: None,
                },
            )]))
            .unwrap(),
        )
        .unwrap();
        std::fs::create_dir_all(&config_home).unwrap();
        std::fs::write(
            config_home.join("settings.json"),
            serde_json::json!({
                "extraKnownMarketplaces": {
                    "local-market": {
                        "source": {
                            "source": "directory",
                            "path": temp.path().join("local-market").to_string_lossy()
                        }
                    }
                }
            })
            .to_string(),
        )
        .unwrap();

        let removed = text(
            directive
                .execute(&["marketplace", "remove", "local-market"], &ctx)
                .await
                .unwrap(),
        );
        assert!(removed.contains("Removed marketplace local-market"));

        let known: KnownMarketplacesFile = serde_json::from_str(
            &std::fs::read_to_string(plugins_dir.join("known_marketplaces.json")).unwrap(),
        )
        .unwrap();
        assert!(!known.contains_key("local-market"));
        let settings: Value = serde_json::from_str(
            &std::fs::read_to_string(config_home.join("settings.json")).unwrap(),
        )
        .unwrap();
        assert!(settings["extraKnownMarketplaces"]
            .get("local-market")
            .is_none());
        assert!(!cache_dir.join("local-market").exists());
        assert!(!cache_dir.join("local-market.json").exists());

        std::env::remove_var("MOSSEN_CONFIG_DIR");
    }
}
