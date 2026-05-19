//! 内置插件注册 — 对应 TS 的 plugins/builtinPlugins.ts + plugins/bundled/。
//!
//! 管理随 CLI 发布的内置插件，用户可通过 /plugin UI 启用/禁用。

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// 内置插件的 Marketplace 名称后缀。
pub const BUILTIN_MARKETPLACE_NAME: &str = "builtin";

/// 技能定义。
#[derive(Debug, Clone)]
pub struct BundledSkillDefinition {
    pub name: String,
    pub description: String,
    pub argument_hint: Option<String>,
    pub when_to_use: Option<String>,
    pub allowed_tools: Vec<String>,
    pub model: Option<String>,
    pub disable_model_invocation: bool,
    pub user_invocable: bool,
    pub hooks: Option<serde_json::Value>,
    pub context: Option<serde_json::Value>,
    pub agent: Option<serde_json::Value>,
    pub is_enabled: Option<bool>,
    /// 返回 prompt 内容的函数。
    pub get_prompt_for_command: Option<fn(&str) -> Vec<PromptContent>>,
}

/// Prompt 内容块。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptContent {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: String,
}

/// Hook 配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HooksConfig {
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// MCP 服务器配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// 内置插件定义。
#[derive(Debug, Clone)]
pub struct BuiltinPluginDefinition {
    pub name: String,
    pub description: String,
    pub version: String,
    pub default_enabled: bool,
    pub skills: Vec<BundledSkillDefinition>,
    pub hooks: Option<HooksConfig>,
    pub mcp_servers: Option<Vec<McpServerConfig>>,
    pub is_available: Option<fn() -> bool>,
}

/// 插件 manifest。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub description: String,
    pub version: String,
}

/// 加载后的插件。
#[derive(Debug, Clone)]
pub struct LoadedPlugin {
    pub name: String,
    pub manifest: PluginManifest,
    pub path: String,
    pub source: String,
    pub repository: String,
    pub enabled: bool,
    pub is_builtin: bool,
    pub hooks_config: Option<HooksConfig>,
    pub mcp_servers: Option<Vec<McpServerConfig>>,
}

/// 启用/禁用分组。
pub struct BuiltinPluginsSplit {
    pub enabled: Vec<LoadedPlugin>,
    pub disabled: Vec<LoadedPlugin>,
}

/// 命令。
#[derive(Debug, Clone)]
pub struct PluginCommand {
    pub command_type: String,
    pub name: String,
    pub description: String,
    pub has_user_specified_description: bool,
    pub allowed_tools: Vec<String>,
    pub argument_hint: Option<String>,
    pub when_to_use: Option<String>,
    pub model: Option<String>,
    pub disable_model_invocation: bool,
    pub user_invocable: bool,
    pub content_length: usize,
    pub source: String,
    pub loaded_from: String,
    pub is_hidden: bool,
    pub progress_message: String,
}

// ---------------------------------------------------------------------------
// Global registry
// ---------------------------------------------------------------------------

static BUILTIN_PLUGINS: Lazy<RwLock<HashMap<String, BuiltinPluginDefinition>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

/// 注册一个内置插件（启动时调用）。
pub fn register_builtin_plugin(definition: BuiltinPluginDefinition) {
    let name = definition.name.clone();
    BUILTIN_PLUGINS
        .write()
        .expect("plugin registry poisoned")
        .insert(name, definition);
}

/// 判断 plugin ID 是否为内置插件（以 @builtin 结尾）。
pub fn is_builtin_plugin_id(plugin_id: &str) -> bool {
    plugin_id.ends_with(&format!("@{}", BUILTIN_MARKETPLACE_NAME))
}

/// 获取指定名称的内置插件定义。
pub fn get_builtin_plugin_definition(name: &str) -> Option<BuiltinPluginDefinition> {
    BUILTIN_PLUGINS
        .read()
        .expect("plugin registry poisoned")
        .get(name)
        .cloned()
}

/// 获取所有已注册的内置插件，按启用/禁用分组。
/// 不可用的插件（is_available 返回 false）完全忽略。
pub fn get_builtin_plugins() -> BuiltinPluginsSplit {
    let registry = BUILTIN_PLUGINS.read().expect("plugin registry poisoned");
    let mut enabled = Vec::new();
    let mut disabled = Vec::new();

    for (name, definition) in registry.iter() {
        // 检查插件是否可用
        if let Some(is_available) = definition.is_available {
            if !is_available() {
                continue;
            }
        }

        let plugin_id = format!("{}@{}", name, BUILTIN_MARKETPLACE_NAME);

        // 启用状态：用户偏好 > 插件默认值。
        // 从 session-cached settings 读取 enabled_plugins[pluginId]；
        // session 缓存为空时回退到 default_enabled。
        let is_enabled = mossen_utils::settings::get_session_settings_cache()
            .and_then(|s| s.settings.enabled_plugins.clone())
            .and_then(|map| map.get(&plugin_id).cloned())
            .and_then(|v| v.as_bool())
            .unwrap_or(definition.default_enabled);

        let plugin = LoadedPlugin {
            name: name.clone(),
            manifest: PluginManifest {
                name: name.clone(),
                description: definition.description.clone(),
                version: definition.version.clone(),
            },
            path: BUILTIN_MARKETPLACE_NAME.to_string(),
            source: plugin_id.clone(),
            repository: plugin_id,
            enabled: is_enabled,
            is_builtin: true,
            hooks_config: definition.hooks.clone(),
            mcp_servers: definition.mcp_servers.clone(),
        };

        if is_enabled {
            enabled.push(plugin);
        } else {
            disabled.push(plugin);
        }
    }

    BuiltinPluginsSplit { enabled, disabled }
}

/// 从启用的内置插件获取技能命令。
pub fn get_builtin_plugin_skill_commands() -> Vec<PluginCommand> {
    let split = get_builtin_plugins();
    let registry = BUILTIN_PLUGINS.read().expect("plugin registry poisoned");
    let mut commands = Vec::new();

    for plugin in &split.enabled {
        if let Some(definition) = registry.get(&plugin.name) {
            for skill in &definition.skills {
                commands.push(skill_definition_to_command(skill));
            }
        }
    }

    commands
}

/// 清除内置插件注册表（测试用）。
pub fn clear_builtin_plugins() {
    BUILTIN_PLUGINS
        .write()
        .expect("plugin registry poisoned")
        .clear();
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn skill_definition_to_command(definition: &BundledSkillDefinition) -> PluginCommand {
    PluginCommand {
        command_type: "prompt".to_string(),
        name: definition.name.clone(),
        description: definition.description.clone(),
        has_user_specified_description: true,
        allowed_tools: definition.allowed_tools.clone(),
        argument_hint: definition.argument_hint.clone(),
        when_to_use: definition.when_to_use.clone(),
        model: definition.model.clone(),
        disable_model_invocation: definition.disable_model_invocation,
        user_invocable: definition.user_invocable,
        content_length: 0,
        source: "bundled".to_string(),
        loaded_from: "bundled".to_string(),
        is_hidden: !definition.user_invocable,
        progress_message: "running".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Prompt helper (对应 TS 的 prompt(body) 高阶函数)
// ---------------------------------------------------------------------------

fn make_prompt(body: &str, args: &str) -> Vec<PromptContent> {
    let text = if args.trim().is_empty() {
        body.to_string()
    } else {
        format!("{}\n\n## User Request\n{}", body, args.trim())
    };
    vec![PromptContent {
        content_type: "text".to_string(),
        text,
    }]
}

// ---------------------------------------------------------------------------
// Bundled plugin: mossen-plugin-dev
// ---------------------------------------------------------------------------

fn plugin_structure_prompt(args: &str) -> Vec<PromptContent> {
    make_prompt(
        "# Mossen Plugin Structure\n\n\
         Design the plugin using Mossen's existing plugin systems. Reuse manifest loading, \
         installed plugin registry, cache behavior, and built-in plugin patterns.\n\n\
         ## Required output\n\
         - Recommended plugin directory tree.\n\
         - Manifest fields.\n\
         - Skills, commands, hooks, MCP servers, agents, or settings included.\n\
         - Default enabled/disabled behavior.\n\
         - Safety boundaries and validation plan.\n\n\
         Do not invent a second plugin system.",
        args,
    )
}

fn skill_development_prompt(args: &str) -> Vec<PromptContent> {
    make_prompt(
        "# Mossen Skill Development\n\n\
         Build skills as small workflow capsules.\n\n\
         ## Checklist\n\
         - Narrow name and description.\n\
         - Clear when-to-use trigger.\n\
         - Minimal allowed tools.\n\
         - No broad auto-execution for mutation.\n\
         - References live under the skill directory when needed.\n\
         - Tests or smoke coverage when the skill is built into Mossen.\n\n\
         Prefer existing bundled skill and plugin skill loaders over custom dispatch.",
        args,
    )
}

fn command_development_prompt(args: &str) -> Vec<PromptContent> {
    make_prompt(
        "# Mossen Command Development\n\n\
         Use the existing command registry and local JSX command patterns.\n\n\
         ## Checklist\n\
         - Parse arguments once at the command boundary.\n\
         - Keep router thin.\n\
         - Use dry-run + confirm for mutation or deletion.\n\
         - Add focused smoke.\n\
         - Do not touch query loop unless explicitly approved.\n\
         - Do not create fake protocol surfaces.\n\n\
         If the command exposes existing runtime state, prefer read-only helpers.",
        args,
    )
}

fn hook_development_prompt(args: &str) -> Vec<PromptContent> {
    make_prompt(
        "# Mossen Hook Development\n\n\
         Hooks must be explicit and predictable.\n\n\
         ## Safety rules\n\
         - Keep mutation hooks disabled by default unless the user explicitly enables them.\n\
         - Document event names and side effects.\n\
         - Bound output and timeout behavior.\n\
         - Do not hide failures.\n\
         - Do not bypass permissions.\n\n\
         Reuse existing hook settings and plugin loading code.",
        args,
    )
}

fn mcp_integration_prompt(args: &str) -> Vec<PromptContent> {
    make_prompt(
        "# Mossen MCP Integration\n\n\
         Design MCP integration as local-first and least-privilege.\n\n\
         ## Checklist\n\
         - Read-only and mutation tools separated.\n\
         - Credentials are never embedded in templates.\n\
         - Network servers require explicit setup.\n\
         - Servers are not auto-connected by surprise.\n\
         - Tool schemas describe side effects.\n\
         - Config uses Mossen's existing MCP runtime and plugin MCP integration.\n\n\
         Prefer templates and user confirmation before enabling servers.",
        args,
    )
}

fn plugin_settings_prompt(args: &str) -> Vec<PromptContent> {
    make_prompt(
        "# Mossen Plugin Settings\n\n\
         Settings must be stable, reversible, and user-visible.\n\n\
         ## Checklist\n\
         - Defaults are safe.\n\
         - Sensitive values are not printed.\n\
         - Migration path is explicit.\n\
         - Settings are scoped: user, project, local, or plugin.\n\
         - Validation errors are readable.\n\
         - Feature gates do not accidentally enable unrelated systems.",
        args,
    )
}

fn agent_development_prompt(args: &str) -> Vec<PromptContent> {
    make_prompt(
        "# Mossen Agent Development\n\n\
         Agents should be small, named responsibilities, not broad replacements for \
         the main assistant.\n\n\
         ## Checklist\n\
         - Specific job description.\n\
         - Clear handoff boundary.\n\
         - Minimal tools.\n\
         - No hidden mutation.\n\
         - Explicit output contract.\n\
         - Smoke or fixture coverage for critical behavior.\n\n\
         Prefer plugin-provided agents only when they materially simplify repeated workflows.",
        args,
    )
}

/// 注册 mossen-plugin-dev 内置插件。
pub fn register_mossen_plugin_dev_plugin() {
    register_builtin_plugin(BuiltinPluginDefinition {
        name: "mossen-plugin-dev".to_string(),
        description: "Mossen extension development pack: plugins, skills, commands, hooks, \
                      MCP servers, settings, and agents."
            .to_string(),
        version: "0.1.0".to_string(),
        default_enabled: true,
        skills: vec![
            BundledSkillDefinition {
                name: "plugin-structure".to_string(),
                description: "Design a Mossen plugin layout, manifest, component folders, \
                             and review checklist."
                    .to_string(),
                argument_hint: Some("[plugin goal]".to_string()),
                when_to_use: Some(
                    "Use when the user wants to create or review a Mossen plugin package structure."
                        .to_string(),
                ),
                allowed_tools: Vec::new(),
                model: None,
                disable_model_invocation: false,
                user_invocable: true,
                hooks: None,
                context: None,
                agent: None,
                is_enabled: None,
                get_prompt_for_command: Some(plugin_structure_prompt),
            },
            BundledSkillDefinition {
                name: "skill-development".to_string(),
                description: "Create or refine a Mossen skill with narrow trigger text and \
                             safe allowed-tools policy."
                    .to_string(),
                argument_hint: Some("[skill goal]".to_string()),
                when_to_use: Some(
                    "Use when implementing a SKILL.md, bundled skill, or plugin-provided skill."
                        .to_string(),
                ),
                allowed_tools: Vec::new(),
                model: None,
                disable_model_invocation: false,
                user_invocable: true,
                hooks: None,
                context: None,
                agent: None,
                is_enabled: None,
                get_prompt_for_command: Some(skill_development_prompt),
            },
            BundledSkillDefinition {
                name: "command-development".to_string(),
                description: "Add or review a Mossen slash command while preserving parser, \
                             router, smoke, and docs conventions."
                    .to_string(),
                argument_hint: Some("[/command goal]".to_string()),
                when_to_use: Some(
                    "Use when adding or modifying a Mossen slash command.".to_string(),
                ),
                allowed_tools: Vec::new(),
                model: None,
                disable_model_invocation: false,
                user_invocable: true,
                hooks: None,
                context: None,
                agent: None,
                is_enabled: None,
                get_prompt_for_command: Some(command_development_prompt),
            },
            BundledSkillDefinition {
                name: "hook-development".to_string(),
                description: "Design Mossen hooks with safe event scope, disabled-by-default \
                             risk posture, and observable failure behavior."
                    .to_string(),
                argument_hint: Some("[hook goal]".to_string()),
                when_to_use: Some(
                    "Use when building or reviewing plugin hooks or hook settings.".to_string(),
                ),
                allowed_tools: Vec::new(),
                model: None,
                disable_model_invocation: false,
                user_invocable: true,
                hooks: None,
                context: None,
                agent: None,
                is_enabled: None,
                get_prompt_for_command: Some(hook_development_prompt),
            },
            BundledSkillDefinition {
                name: "mcp-integration".to_string(),
                description: "Design MCP server integration for Mossen plugins without \
                             default auto-connect or hidden credentials."
                    .to_string(),
                argument_hint: Some("[MCP integration goal]".to_string()),
                when_to_use: Some(
                    "Use when adding MCP servers to a plugin or designing MCP templates."
                        .to_string(),
                ),
                allowed_tools: Vec::new(),
                model: None,
                disable_model_invocation: false,
                user_invocable: true,
                hooks: None,
                context: None,
                agent: None,
                is_enabled: None,
                get_prompt_for_command: Some(mcp_integration_prompt),
            },
            BundledSkillDefinition {
                name: "plugin-settings".to_string(),
                description: "Plan plugin settings, defaults, migrations, and user-visible \
                             toggles safely."
                    .to_string(),
                argument_hint: Some("[settings goal]".to_string()),
                when_to_use: Some(
                    "Use when adding settings to a Mossen plugin or built-in extension."
                        .to_string(),
                ),
                allowed_tools: Vec::new(),
                model: None,
                disable_model_invocation: false,
                user_invocable: true,
                hooks: None,
                context: None,
                agent: None,
                is_enabled: None,
                get_prompt_for_command: Some(plugin_settings_prompt),
            },
            BundledSkillDefinition {
                name: "agent-development".to_string(),
                description: "Design plugin-provided agents with bounded scope, clear prompts, \
                             and permission-safe tool access."
                    .to_string(),
                argument_hint: Some("[agent goal]".to_string()),
                when_to_use: Some(
                    "Use when creating or reviewing a Mossen plugin-provided agent.".to_string(),
                ),
                allowed_tools: Vec::new(),
                model: None,
                disable_model_invocation: false,
                user_invocable: true,
                hooks: None,
                context: None,
                agent: None,
                is_enabled: None,
                get_prompt_for_command: Some(agent_development_prompt),
            },
        ],
        hooks: None,
        mcp_servers: None,
        is_available: None,
    });
}

// ---------------------------------------------------------------------------
// Init (对应 plugins/bundled/index.ts)
// ---------------------------------------------------------------------------

/// 初始化内置插件（CLI 启动时调用）。
pub fn init_builtin_plugins() {
    register_mossen_plugin_dev_plugin();
}
