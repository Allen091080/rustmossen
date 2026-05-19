//! # mossen-skills
//!
//! Mossen 技能与插件系统 — 提供可扩展的技能加载、注册和执行框架，
//! 支持内置技能和动态插件。
//!
//! ## 模块结构
//!
//! - [`skill`] — 技能定义、捆绑技能注册、文件提取
//! - [`config`] — Frontmatter 解析、技能配置
//! - [`manifest`] — 插件清单解析与验证
//! - [`registry`] — 统一技能注册表（捆绑/磁盘/动态/条件）
//! - [`discovery`] — 技能与插件目录发现
//! - [`loader`] — 技能文件加载、插件加载
//! - [`plugin`] — 内置插件管理
//! - [`executor`] — 技能执行引擎与命令参数解析

#![allow(dead_code, unused_imports)]

pub mod bundled;
pub mod config;
pub mod discovery;
pub mod dynamic;
pub mod executor;
pub mod loader;
pub mod manifest;
pub mod plugin;
pub mod registry;
pub mod skill;

pub use bundled::{
    fetch_mcp_skills_for_client, get_mcp_skill_builders, init_bundled_skills, register_batch_skill,
    register_debug_skill, register_dream_skill, register_keybindings_skill, register_loop_skill,
    register_lorem_ipsum_skill, register_mcp_skill_builders, register_mossen_api_skill,
    register_mossen_core_skills, register_mossen_in_chrome_skill, register_remember_skill,
    register_simplify_skill, register_skillify_skill, register_update_config_skill,
    register_verify_skill, MCPSkillBuilders, SKILL_FILES_API, SKILL_FILES_VERIFY, SKILL_MD_VERIFY,
    SKILL_MODEL_VARS_API, SKILL_PROMPT_API,
};

pub use dynamic::{
    activate_conditional_skills_for_paths, add_conditional_skill, add_skill_directories,
    clear_dynamic_skills, clear_skill_caches, create_skill_command, discover_skill_dirs_for_paths,
    get_conditional_skill_count, get_conditional_skills_count, get_dynamic_skills,
    get_skill_dir_commands, get_skills_path, on_dynamic_skills_loaded, CreateSkillCommandInput,
    LoadedFrom, SkillSettingSource,
};

// Re-export 核心类型
pub use config::{FrontmatterData, FrontmatterShell, ParsedSkillFields};
pub use executor::{
    execute_craft, find_craft_by_name, CraftExecutionContext, ParsedPluginCommand,
    ParsedSkillsCommand,
};
pub use loader::{load_plugins_from_dirs, load_skills_from_dir};
pub use manifest::{load_manifest, ParsedManifest};
pub use plugin::{
    get_builtin_plugin_craft_commands, get_builtin_plugins, is_builtin_plugin_id,
    register_builtin_plugin, BuiltinPluginDefinition, BuiltinPluginsResult,
    BUILTIN_MARKETPLACE_NAME,
};
pub use registry::{new_shared_registry, CraftRegistry, SharedCraftRegistry};
pub use skill::{
    bundled_craft_extract_dir, clear_bundled_crafts, extract_bundled_craft_files,
    get_bundled_crafts, register_bundled_craft, BundledCraftDefinition, ContentBlock, CraftCommand,
};
