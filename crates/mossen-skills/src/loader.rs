//! # loader — 技能与插件加载器
//!
//! 对应 TypeScript `skills/loadSkillsDir.ts` 中的文件加载逻辑
//! 以及 `services/plugins/pluginCliCommands.ts` 中的插件加载逻辑。
//! 负责从文件系统加载 SKILL.md、解析插件目录结构。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tracing::{debug, warn};

use mossen_types::command::{
    CommandBase, CommandLoadedFrom, PromptCommandData, PromptCommandSource,
};
use mossen_types::plugin::{LoadedPlugin, PluginError, PluginLoadResult};

use crate::config::{parse_frontmatter, parse_skill_frontmatter_fields, parse_skill_paths};
use crate::manifest::{load_manifest, to_plugin_manifest, validate_manifest};
use crate::skill::CraftCommand;

// ---------------------------------------------------------------------------
// 技能加载
// ---------------------------------------------------------------------------

/// 从 /skills/ 目录加载技能。
///
/// 对应 TS `loadSkillsFromSkillsDir(basePath, source)`。
/// 仅支持目录格式：skill-name/SKILL.md。
pub async fn load_skills_from_dir(
    base_path: &Path,
    source: PromptCommandSource,
) -> Vec<(CraftCommand, PathBuf)> {
    let entries = match tokio::fs::read_dir(base_path).await {
        Ok(entries) => entries,
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                warn!("Failed to read skills dir {}: {}", base_path.display(), e);
            }
            return vec![];
        }
    };

    let mut results = Vec::new();
    let mut read_dir = entries;

    while let Ok(Some(entry)) = read_dir.next_entry().await {
        let file_type = match entry.file_type().await {
            Ok(ft) => ft,
            Err(_) => continue,
        };

        // 仅支持目录或符号链接
        if !file_type.is_dir() && !file_type.is_symlink() {
            continue;
        }

        let skill_dir_path = entry.path();
        let skill_file_path = skill_dir_path.join("SKILL.md");

        let content = match tokio::fs::read_to_string(&skill_file_path).await {
            Ok(c) => c,
            Err(e) => {
                if e.kind() != std::io::ErrorKind::NotFound {
                    debug!(
                        "[skills] failed to read {}: {}",
                        skill_file_path.display(),
                        e
                    );
                }
                continue;
            }
        };

        let skill_name = entry.file_name().to_string_lossy().to_string();

        match build_craft_from_content(
            &content,
            &skill_name,
            source.clone(),
            Some(&skill_dir_path),
            CommandLoadedFrom::Skills,
        ) {
            Some(craft) => results.push((craft, skill_file_path)),
            None => continue,
        }
    }

    results
}

/// 从 Markdown 内容构建技能命令。
fn build_craft_from_content(
    content: &str,
    skill_name: &str,
    source: PromptCommandSource,
    base_dir: Option<&Path>,
    loaded_from: CommandLoadedFrom,
) -> Option<CraftCommand> {
    let (frontmatter, markdown_content) = parse_frontmatter(content);
    let parsed = parse_skill_frontmatter_fields(&frontmatter, &markdown_content, skill_name);
    let paths = parse_skill_paths(&frontmatter);

    let base_dir_str = base_dir.map(|p| p.to_string_lossy().to_string());

    Some(CraftCommand {
        base: CommandBase {
            name: skill_name.to_string(),
            description: parsed.description.clone(),
            aliases: None,
            argument_hint: parsed.argument_hint.clone(),
            when_to_use: parsed.when_to_use.clone(),
            user_invocable: Some(parsed.user_invocable),
            disable_model_invocation: Some(parsed.disable_model_invocation),
            is_hidden: Some(!parsed.user_invocable),
            has_user_specified_description: Some(parsed.has_user_specified_description),
            loaded_from: Some(loaded_from),
            availability: None,
            version: parsed.version.clone(),
            is_mcp: None,
            kind: None,
            immediate: None,
            is_sensitive: None,
            extra: HashMap::new(),
        },
        prompt_data: PromptCommandData {
            progress_message: "running".to_string(),
            content_length: markdown_content.len(),
            arg_names: if parsed.argument_names.is_empty() {
                None
            } else {
                Some(parsed.argument_names.clone())
            },
            allowed_tools: Some(parsed.allowed_tools.clone()),
            model: parsed.model.clone(),
            source,
            plugin_info: None,
            disable_non_interactive: None,
            hooks: parsed.hooks.clone(),
            skill_root: base_dir_str.clone(),
            context: parsed.execution_context,
            agent: parsed.agent.clone(),
            effort: parsed.effort,
            paths,
        },
        loaded_from,
        markdown_content: Some(markdown_content),
        skill_root: base_dir_str,
        display_name: parsed.display_name.clone(),
    })
}

// ---------------------------------------------------------------------------
// 插件加载
// ---------------------------------------------------------------------------

/// 加载指定路径的插件。
///
/// 对应 TS 插件加载管线中的单个插件解析。
pub async fn load_plugin_from_path(
    plugin_path: &Path,
    source: &str,
    repository: &str,
) -> Result<LoadedPlugin, PluginError> {
    // 查找清单文件
    let manifest_path = find_manifest_file(plugin_path);
    let manifest_path = manifest_path.ok_or_else(|| PluginError::ManifestParseError {
        source: source.to_string(),
        plugin: None,
        manifest_path: plugin_path
            .join("manifest.json")
            .to_string_lossy()
            .to_string(),
        parse_error: "No manifest.json or package.json found".to_string(),
    })?;

    let parsed =
        load_manifest(&manifest_path)
            .await
            .map_err(|e| PluginError::ManifestParseError {
                source: source.to_string(),
                plugin: None,
                manifest_path: manifest_path.to_string_lossy().to_string(),
                parse_error: e.to_string(),
            })?;

    // 验证
    let errors = validate_manifest(&parsed);
    if !errors.is_empty() {
        return Err(PluginError::ManifestValidationError {
            source: source.to_string(),
            plugin: Some(parsed.name.clone()),
            manifest_path: manifest_path.to_string_lossy().to_string(),
            validation_errors: errors,
        });
    }

    let manifest = to_plugin_manifest(&parsed);

    // 构造 LoadedPlugin
    let plugin = LoadedPlugin {
        name: parsed.name.clone(),
        manifest,
        path: plugin_path.to_string_lossy().to_string(),
        source: source.to_string(),
        repository: repository.to_string(),
        enabled: Some(true),
        is_builtin: Some(false),
        sha: None,
        commands_path: extract_first_path(
            &parsed.components,
            &mossen_types::plugin::PluginComponent::Commands,
        ),
        commands_paths: extract_all_paths(
            &parsed.components,
            &mossen_types::plugin::PluginComponent::Commands,
        ),
        commands_metadata: None,
        agents_path: extract_first_path(
            &parsed.components,
            &mossen_types::plugin::PluginComponent::Agents,
        ),
        agents_paths: extract_all_paths(
            &parsed.components,
            &mossen_types::plugin::PluginComponent::Agents,
        ),
        skills_path: extract_first_path(
            &parsed.components,
            &mossen_types::plugin::PluginComponent::Skills,
        ),
        skills_paths: extract_all_paths(
            &parsed.components,
            &mossen_types::plugin::PluginComponent::Skills,
        ),
        output_styles_path: extract_first_path(
            &parsed.components,
            &mossen_types::plugin::PluginComponent::OutputStyles,
        ),
        output_styles_paths: extract_all_paths(
            &parsed.components,
            &mossen_types::plugin::PluginComponent::OutputStyles,
        ),
        hooks_config: parsed.hooks_config,
        mcp_servers: if parsed.mcp_servers.is_empty() {
            None
        } else {
            Some(parsed.mcp_servers)
        },
        lsp_servers: if parsed.lsp_servers.is_empty() {
            None
        } else {
            Some(parsed.lsp_servers)
        },
        settings: if parsed.settings.is_empty() {
            None
        } else {
            Some(parsed.settings)
        },
    };

    Ok(plugin)
}

/// 批量加载多个插件目录。
///
/// 返回 PluginLoadResult（enabled / disabled / errors）。
pub async fn load_plugins_from_dirs(
    plugin_dirs: &[(PathBuf, String, String)], // (path, source, repository)
) -> PluginLoadResult {
    let mut enabled = Vec::new();
    let mut disabled = Vec::new();
    let mut errors = Vec::new();

    for (path, source, repository) in plugin_dirs {
        match load_plugin_from_path(path, source, repository).await {
            Ok(plugin) => {
                if plugin.enabled.unwrap_or(true) {
                    enabled.push(plugin);
                } else {
                    disabled.push(plugin);
                }
            }
            Err(e) => {
                errors.push(e);
            }
        }
    }

    PluginLoadResult {
        enabled,
        disabled,
        errors,
    }
}

// ---------------------------------------------------------------------------
// 辅助函数
// ---------------------------------------------------------------------------

/// 查找清单文件路径。
fn find_manifest_file(plugin_path: &Path) -> Option<PathBuf> {
    let candidates = ["manifest.json", "package.json"];
    for candidate in &candidates {
        let path = plugin_path.join(candidate);
        if path.exists() {
            return Some(path);
        }
    }
    None
}

/// 从组件映射中提取第一个路径。
fn extract_first_path(
    components: &HashMap<mossen_types::plugin::PluginComponent, Vec<String>>,
    component: &mossen_types::plugin::PluginComponent,
) -> Option<String> {
    components.get(component).and_then(|v| v.first().cloned())
}

/// 从组件映射中提取所有路径。
fn extract_all_paths(
    components: &HashMap<mossen_types::plugin::PluginComponent, Vec<String>>,
    component: &mossen_types::plugin::PluginComponent,
) -> Option<Vec<String>> {
    components.get(component).cloned()
}

/// 获取文件的规范路径（解析符号链接），用于去重。
///
/// 对应 TS `getFileIdentity(filePath)`。
pub async fn get_file_identity(file_path: &Path) -> Option<PathBuf> {
    tokio::fs::canonicalize(file_path).await.ok()
}
