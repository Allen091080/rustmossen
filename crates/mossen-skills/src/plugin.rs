//! # plugin — 内置插件管理
//!
//! 对应 TypeScript `plugins/builtinPlugins.ts`。
//! 管理内置插件的注册、启用/禁用状态查询、技能提取。

use std::collections::HashMap;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

use mossen_types::command::ExecutionContext;
use mossen_types::plugin::LoadedPlugin;

use crate::skill::{BundledCraftDefinition, CraftCommand};

// ---------------------------------------------------------------------------
// 类型
// ---------------------------------------------------------------------------

/// 内置插件定义 — 对应 TS `BuiltinPluginDefinition`。
///
/// 注意：与 mossen_types::plugin::BuiltinPluginDefinition 不同，
/// 此处包含运行时回调（如 `is_available`、`skills`）。
#[derive(Debug, Clone)]
pub struct BuiltinPluginDefinition {
    /// 插件名称。
    pub name: String,
    /// 描述。
    pub description: String,
    /// 版本。
    pub version: Option<String>,
    /// 默认是否启用。
    pub default_enabled: bool,
    /// 包含的技能列表。
    pub skills: Vec<BundledCraftDefinition>,
    /// Hooks 配置。
    pub hooks: Option<serde_json::Value>,
    /// MCP 服务器配置。
    pub mcp_servers: Option<HashMap<String, serde_json::Value>>,
}

/// 内置插件的启用/禁用状态。
#[derive(Debug, Clone)]
pub struct BuiltinPluginsResult {
    /// 已启用的插件。
    pub enabled: Vec<LoadedPlugin>,
    /// 已禁用的插件。
    pub disabled: Vec<LoadedPlugin>,
}

// ---------------------------------------------------------------------------
// 常量
// ---------------------------------------------------------------------------

/// 内置插件市场名称。
pub const BUILTIN_MARKETPLACE_NAME: &str = "builtin";

// ---------------------------------------------------------------------------
// 全局注册表
// ---------------------------------------------------------------------------

static BUILTIN_PLUGINS: RwLock<Option<HashMap<String, BuiltinPluginDefinition>>> =
    RwLock::new(None);

fn with_registry<R>(f: impl FnOnce(&mut HashMap<String, BuiltinPluginDefinition>) -> R) -> R {
    let mut guard = BUILTIN_PLUGINS
        .write()
        .expect("builtin plugins lock poisoned");
    let map = guard.get_or_insert_with(HashMap::new);
    f(map)
}

fn with_registry_read<R>(f: impl FnOnce(&HashMap<String, BuiltinPluginDefinition>) -> R) -> R {
    let guard = BUILTIN_PLUGINS
        .read()
        .expect("builtin plugins lock poisoned");
    match guard.as_ref() {
        Some(map) => f(map),
        None => f(&HashMap::new()),
    }
}

// ---------------------------------------------------------------------------
// 公开 API
// ---------------------------------------------------------------------------

/// 注册一个内置插件。
///
/// 对应 TS `registerBuiltinPlugin(definition)`。
pub fn register_builtin_plugin(definition: BuiltinPluginDefinition) {
    with_registry(|map| {
        map.insert(definition.name.clone(), definition);
    });
}

/// 检查插件 ID 是否为内置插件。
///
/// 对应 TS `isBuiltinPluginId(pluginId)`。
pub fn is_builtin_plugin_id(plugin_id: &str) -> bool {
    plugin_id.ends_with(&format!("@{}", BUILTIN_MARKETPLACE_NAME))
}

/// 获取指定内置插件定义。
pub fn get_builtin_plugin_definition(name: &str) -> Option<BuiltinPluginDefinition> {
    with_registry_read(|map| map.get(name).cloned())
}

/// 获取所有内置插件，按启用/禁用分类。
///
/// 对应 TS `getBuiltinPlugins()`。
/// `enabled_plugins` 参数来自用户设置。
pub fn get_builtin_plugins(enabled_plugins: &HashMap<String, bool>) -> BuiltinPluginsResult {
    let mut enabled = Vec::new();
    let mut disabled = Vec::new();

    with_registry_read(|map| {
        for (name, definition) in map {
            let plugin_id = format!("{}@{}", name, BUILTIN_MARKETPLACE_NAME);
            let user_setting = enabled_plugins.get(&plugin_id);

            let is_enabled = match user_setting {
                Some(v) => *v,
                None => definition.default_enabled,
            };

            let manifest_data = {
                let mut data = HashMap::new();
                data.insert("name".to_string(), serde_json::Value::String(name.clone()));
                data.insert(
                    "description".to_string(),
                    serde_json::Value::String(definition.description.clone()),
                );
                if let Some(v) = &definition.version {
                    data.insert("version".to_string(), serde_json::Value::String(v.clone()));
                }
                data
            };

            let plugin = LoadedPlugin {
                name: name.clone(),
                manifest: mossen_types::plugin::PluginManifest {
                    data: manifest_data,
                },
                path: BUILTIN_MARKETPLACE_NAME.to_string(),
                source: plugin_id.clone(),
                repository: plugin_id,
                enabled: Some(is_enabled),
                is_builtin: Some(true),
                sha: None,
                commands_path: None,
                commands_paths: None,
                commands_metadata: None,
                agents_path: None,
                agents_paths: None,
                skills_path: None,
                skills_paths: None,
                output_styles_path: None,
                output_styles_paths: None,
                hooks_config: definition.hooks.clone(),
                mcp_servers: definition.mcp_servers.clone(),
                lsp_servers: None,
                settings: None,
            };

            if is_enabled {
                enabled.push(plugin);
            } else {
                disabled.push(plugin);
            }
        }
    });

    BuiltinPluginsResult { enabled, disabled }
}

/// 获取已启用内置插件的技能命令。
///
/// 对应 TS `getBuiltinPluginSkillCommands()`。
pub fn get_builtin_plugin_craft_commands(
    enabled_plugins: &HashMap<String, bool>,
) -> Vec<CraftCommand> {
    let result = get_builtin_plugins(enabled_plugins);
    let mut commands = Vec::new();

    for plugin in &result.enabled {
        if let Some(definition) = get_builtin_plugin_definition(&plugin.name) {
            for skill_def in &definition.skills {
                commands.push(craft_definition_to_command(skill_def));
            }
        }
    }

    commands
}

/// 清除内置插件注册表（测试用）。
pub fn clear_builtin_plugins() {
    with_registry(|map| map.clear());
}

// ---------------------------------------------------------------------------
// 辅助函数
// ---------------------------------------------------------------------------

/// 将 BundledCraftDefinition 转换为 CraftCommand。
fn craft_definition_to_command(definition: &BundledCraftDefinition) -> CraftCommand {
    use mossen_types::command::{
        CommandBase, CommandLoadedFrom, PromptCommandData, PromptCommandSource,
    };

    CraftCommand {
        base: CommandBase {
            name: definition.name.clone(),
            description: definition.description.clone(),
            aliases: definition.aliases.clone(),
            argument_hint: definition.argument_hint.clone(),
            when_to_use: definition.when_to_use.clone(),
            user_invocable: Some(definition.user_invocable),
            disable_model_invocation: Some(definition.disable_model_invocation),
            is_hidden: Some(!definition.user_invocable),
            has_user_specified_description: Some(true),
            loaded_from: Some(CommandLoadedFrom::Bundled),
            availability: None,
            version: None,
            is_mcp: None,
            kind: None,
            immediate: None,
            is_sensitive: None,
            extra: HashMap::new(),
        },
        prompt_data: PromptCommandData {
            progress_message: "running".to_string(),
            content_length: 0,
            arg_names: None,
            allowed_tools: definition.allowed_tools.clone().or_else(|| Some(vec![])),
            model: definition.model.clone(),
            source: PromptCommandSource::Bundled,
            plugin_info: None,
            disable_non_interactive: None,
            hooks: definition.hooks.clone(),
            skill_root: None,
            context: definition.context,
            agent: definition.agent.clone(),
            effort: None,
            paths: None,
        },
        loaded_from: CommandLoadedFrom::Bundled,
        markdown_content: None,
        skill_root: None,
        display_name: None,
    }
}
