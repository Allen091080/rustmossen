use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use once_cell::sync::Lazy;
use tracing::debug;

use super::schemas::PluginManifest;

/// Plugin command type.
#[derive(Debug, Clone)]
pub struct Command {
    pub command_type: String,
    pub name: String,
    pub description: String,
    pub has_user_specified_description: bool,
    pub allowed_tools: Vec<String>,
    pub argument_hint: Option<String>,
    pub arg_names: Option<Vec<String>>,
    pub when_to_use: Option<String>,
    pub version: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub disable_model_invocation: bool,
    pub user_invocable: bool,
    pub content_length: usize,
    pub source: String,
    pub loaded_from: Option<String>,
    pub is_hidden: bool,
    pub progress_message: String,
    pub plugin_info: Option<PluginCommandInfo>,
    pub content: String,
    pub file_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct PluginCommandInfo {
    pub plugin_manifest: PluginManifest,
    pub repository: String,
}

/// Plugin markdown file.
struct PluginMarkdownFile {
    file_path: PathBuf,
    base_dir: PathBuf,
    frontmatter: HashMap<String, serde_json::Value>,
    content: String,
}

/// Load config for commands or skills.
#[derive(Debug, Clone)]
pub struct LoadConfig {
    pub is_skill_mode: bool,
}

impl Default for LoadConfig {
    fn default() -> Self {
        Self {
            is_skill_mode: false,
        }
    }
}

static PLUGIN_COMMAND_CACHE: Lazy<Mutex<Option<Vec<Command>>>> = Lazy::new(|| Mutex::new(None));

/// Clear plugin command cache.
pub fn clear_plugin_command_cache() {
    *PLUGIN_COMMAND_CACHE.lock().unwrap() = None;
}

/// Check if a file is a skill file (SKILL.md).
fn is_skill_file(file_path: &Path) -> bool {
    file_path
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.eq_ignore_ascii_case("skill.md"))
        .unwrap_or(false)
}

/// Get command name from file path.
fn get_command_name_from_file(file_path: &Path, base_dir: &Path, plugin_name: &str) -> String {
    let is_skill = is_skill_file(file_path);

    if is_skill {
        let skill_dir = file_path.parent().unwrap_or(file_path);
        let command_base = skill_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        let parent_of_skill = skill_dir.parent().unwrap_or(skill_dir);
        let relative = parent_of_skill
            .strip_prefix(base_dir)
            .unwrap_or(Path::new(""));
        let namespace: String = relative
            .components()
            .filter_map(|c| c.as_os_str().to_str())
            .collect::<Vec<_>>()
            .join(":");

        if namespace.is_empty() {
            format!("{}:{}", plugin_name, command_base)
        } else {
            format!("{}:{}:{}", plugin_name, namespace, command_base)
        }
    } else {
        let command_base = file_path
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        let file_dir = file_path.parent().unwrap_or(file_path);
        let relative = file_dir.strip_prefix(base_dir).unwrap_or(Path::new(""));
        let namespace: String = relative
            .components()
            .filter_map(|c| c.as_os_str().to_str())
            .collect::<Vec<_>>()
            .join(":");

        if namespace.is_empty() {
            format!("{}:{}", plugin_name, command_base)
        } else {
            format!("{}:{}:{}", plugin_name, namespace, command_base)
        }
    }
}

/// Recursively collect all markdown files from a directory.
pub async fn collect_markdown_files(dir_path: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_markdown_recursive(dir_path, &mut files).await;
    files
}

fn collect_markdown_recursive<'a>(
    dir_path: &'a Path,
    files: &'a mut Vec<PathBuf>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
    Box::pin(async move {
        let mut entries = match tokio::fs::read_dir(dir_path).await {
            Ok(e) => e,
            Err(_) => return,
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if let Ok(ft) = entry.file_type().await {
                if ft.is_dir() {
                    collect_markdown_recursive(&path, files).await;
                } else if ft.is_file() {
                    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                        if ext.eq_ignore_ascii_case("md") {
                            files.push(path);
                        }
                    }
                }
            }
        }
    })
}

/// Transform plugin skill files — for directories containing SKILL.md, only include the skill file.
fn transform_plugin_skill_files(files: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut files_by_dir: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
    for file in files {
        let dir = file.parent().unwrap_or(Path::new("")).to_path_buf();
        files_by_dir.entry(dir).or_default().push(file);
    }

    let mut result = Vec::new();
    for (_dir, dir_files) in files_by_dir {
        let skill_files: Vec<&PathBuf> = dir_files.iter().filter(|f| is_skill_file(f)).collect();
        if !skill_files.is_empty() {
            result.push(skill_files[0].clone());
        } else {
            result.extend(dir_files);
        }
    }
    result
}

/// Load commands from a plugin's commands directory.
pub async fn load_commands_from_directory(
    commands_path: &Path,
    plugin_name: &str,
    source_name: &str,
    plugin_manifest: &PluginManifest,
    _plugin_path: &Path,
    config: &LoadConfig,
) -> Vec<Command> {
    let markdown_files = collect_markdown_files(commands_path).await;
    let processed_files = transform_plugin_skill_files(markdown_files);

    let mut commands = Vec::new();
    for file_path in processed_files {
        let command_name = get_command_name_from_file(&file_path, commands_path, plugin_name);
        let content = match tokio::fs::read_to_string(&file_path).await {
            Ok(c) => c,
            Err(_) => continue,
        };

        let (frontmatter, body) = parse_simple_frontmatter(&content);
        let is_skill = is_skill_file(&file_path);

        let description = frontmatter
            .get("description")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                if is_skill {
                    "Plugin skill".to_string()
                } else {
                    "Plugin command".to_string()
                }
            });

        let user_invocable = frontmatter
            .get("user-invocable")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let command = Command {
            command_type: "prompt".to_string(),
            name: command_name.clone(),
            description,
            has_user_specified_description: frontmatter.contains_key("description"),
            allowed_tools: Vec::new(),
            argument_hint: frontmatter
                .get("argument-hint")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            arg_names: None,
            when_to_use: frontmatter
                .get("when_to_use")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            version: frontmatter
                .get("version")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            model: frontmatter
                .get("model")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            effort: frontmatter
                .get("effort")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            disable_model_invocation: frontmatter
                .get("disable-model-invocation")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            user_invocable,
            content_length: body.len(),
            source: "plugin".to_string(),
            loaded_from: if is_skill || config.is_skill_mode {
                Some("plugin".to_string())
            } else {
                None
            },
            is_hidden: !user_invocable,
            progress_message: if is_skill || config.is_skill_mode {
                "loading".to_string()
            } else {
                "running".to_string()
            },
            plugin_info: Some(PluginCommandInfo {
                plugin_manifest: plugin_manifest.clone(),
                repository: source_name.to_string(),
            }),
            content: body,
            file_path,
        };
        commands.push(command);
    }
    commands
}

/// Simple frontmatter parser (--- delimited YAML).
fn parse_simple_frontmatter(content: &str) -> (HashMap<String, serde_json::Value>, String) {
    if !content.starts_with("---") {
        return (HashMap::new(), content.to_string());
    }
    let rest = &content[3..];
    if let Some(end_idx) = rest.find("\n---") {
        let yaml_str = &rest[..end_idx];
        let body = &rest[end_idx + 4..];
        let body = body.strip_prefix('\n').unwrap_or(body);
        let frontmatter: HashMap<String, serde_json::Value> =
            serde_yaml::from_str(yaml_str).unwrap_or_default();
        (frontmatter, body.to_string())
    } else {
        (HashMap::new(), content.to_string())
    }
}

/// Get plugin commands (memoized).
pub async fn get_plugin_commands(plugins: &[PluginInfo], is_bare_mode: bool) -> Vec<Command> {
    {
        let cache = PLUGIN_COMMAND_CACHE.lock().unwrap();
        if let Some(ref cached) = *cache {
            return cached.clone();
        }
    }

    if is_bare_mode && plugins.is_empty() {
        return Vec::new();
    }

    let mut all_commands = Vec::new();
    for plugin in plugins {
        if !plugin.enabled {
            continue;
        }

        // Load from commands path
        if let Some(ref commands_path) = plugin.commands_path {
            let commands = load_commands_from_directory(
                commands_path,
                &plugin.name,
                &plugin.source,
                &plugin.manifest,
                &plugin.path,
                &LoadConfig::default(),
            )
            .await;
            if !commands.is_empty() {
                debug!(
                    "Loaded {} commands from plugin {} default directory",
                    commands.len(),
                    plugin.name
                );
            }
            all_commands.extend(commands);
        }

        // Load from additional paths
        for extra_path in &plugin.commands_paths {
            let commands = load_commands_from_directory(
                extra_path,
                &plugin.name,
                &plugin.source,
                &plugin.manifest,
                &plugin.path,
                &LoadConfig::default(),
            )
            .await;
            all_commands.extend(commands);
        }

        // Load from skills path
        if let Some(ref skills_path) = plugin.skills_path {
            let commands = load_commands_from_directory(
                skills_path,
                &plugin.name,
                &plugin.source,
                &plugin.manifest,
                &plugin.path,
                &LoadConfig {
                    is_skill_mode: true,
                },
            )
            .await;
            all_commands.extend(commands);
        }
    }

    let mut cache = PLUGIN_COMMAND_CACHE.lock().unwrap();
    *cache = Some(all_commands.clone());
    all_commands
}

/// Plugin info for command loading.
#[derive(Debug, Clone)]
pub struct PluginInfo {
    pub name: String,
    pub source: String,
    pub path: PathBuf,
    pub enabled: bool,
    pub manifest: PluginManifest,
    pub commands_path: Option<PathBuf>,
    pub commands_paths: Vec<PathBuf>,
    pub skills_path: Option<PathBuf>,
}

static PLUGIN_SKILLS_CACHE: Lazy<Mutex<Option<Vec<String>>>> = Lazy::new(|| Mutex::new(None));

/// 对应 TS `clearPluginSkillsCache`：清空 plugin skill 缓存。
pub fn clear_plugin_skills_cache() {
    *PLUGIN_SKILLS_CACHE.lock().unwrap() = None;
}

/// 对应 TS `getPluginSkills`：返回当前 plugin 提供的 skill 名列表。
///
/// 当前 Rust 端 plugin 加载流程尚未对 skill 做集中收集，此函数仅返回缓存
/// 的扁平列表（默认空），调用方可以通过 [`clear_plugin_skills_cache`] 触发
/// 重新拉取（由其它模块负责填充）。
pub fn get_plugin_skills() -> Vec<String> {
    PLUGIN_SKILLS_CACHE
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_default()
}
