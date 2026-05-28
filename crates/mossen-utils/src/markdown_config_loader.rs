use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::string_utils::truncate_chars_with_suffix;

/// Mossen configuration directory names.
pub const MOSSEN_CONFIG_DIRECTORIES: &[&str] =
    &["commands", "agents", "output-styles", "skills", "workflows"];

/// 对应 TS `MossenConfigDirectory`：MOSSEN_CONFIG_DIRECTORIES 中合法子目录的字符串别名。
///
/// TS 是字符串 union 类型；Rust 端用 `&'static str` 别名，调用方需保证值来自
/// [`MOSSEN_CONFIG_DIRECTORIES`]。
pub type MossenConfigDirectory = &'static str;

/// A parsed markdown file with metadata.
#[derive(Debug, Clone)]
pub struct MarkdownFile {
    pub file_path: String,
    pub base_dir: String,
    pub frontmatter: FrontmatterData,
    pub content: String,
    pub source: SettingSource,
}

/// Frontmatter data from a markdown file.
#[derive(Debug, Clone, Default)]
pub struct FrontmatterData {
    pub data: HashMap<String, serde_json::Value>,
}

impl FrontmatterData {
    pub fn get(&self, key: &str) -> Option<&serde_json::Value> {
        self.data.get(key)
    }

    pub fn get_str(&self, key: &str) -> Option<&str> {
        self.data.get(key).and_then(|v| v.as_str())
    }
}

/// Setting source for a markdown file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingSource {
    PolicySettings,
    UserSettings,
    ProjectSettings,
}

/// Extract a description from markdown content.
pub fn extract_description_from_markdown(content: &str, default_description: &str) -> String {
    for line in content.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            // If it's a header, strip the header prefix
            let text = if let Some(stripped) = trimmed.strip_prefix('#') {
                stripped.trim_start_matches('#').trim()
            } else {
                trimmed
            };
            return truncate_chars_with_suffix(text, 97, "...");
        }
    }
    default_description.to_string()
}

/// Parse tools from frontmatter, supporting both string and array formats.
fn parse_tool_list_string(tools_value: Option<&serde_json::Value>) -> Option<Vec<String>> {
    let value = tools_value?;

    if value.is_null() {
        return None;
    }

    if let Some(s) = value.as_str() {
        if s.is_empty() {
            return Some(Vec::new());
        }
        return Some(parse_tool_list_from_cli(&[s.to_string()]));
    }

    if let Some(arr) = value.as_array() {
        let tools: Vec<String> = arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
        if tools.is_empty() {
            return Some(Vec::new());
        }
        let parsed = parse_tool_list_from_cli(&tools);
        if parsed.contains(&"*".to_string()) {
            return Some(vec!["*".to_string()]);
        }
        return Some(parsed);
    }

    Some(Vec::new())
}

/// Parse tools from agent frontmatter.
/// Missing field = None (all tools), Empty field = Some(vec![]) (no tools).
pub fn parse_agent_tools_from_frontmatter(
    tools_value: Option<&serde_json::Value>,
) -> Option<Vec<String>> {
    match parse_tool_list_string(tools_value) {
        None => {
            // undefined means all tools
            if tools_value.is_none() {
                None
            } else {
                Some(Vec::new())
            }
        }
        Some(parsed) => {
            if parsed.contains(&"*".to_string()) {
                None
            } else {
                Some(parsed)
            }
        }
    }
}

/// Parse allowed-tools from slash command frontmatter.
pub fn parse_slash_command_tools_from_frontmatter(
    tools_value: Option<&serde_json::Value>,
) -> Vec<String> {
    parse_tool_list_string(tools_value).unwrap_or_default()
}

/// Get a unique identifier for a file based on device ID and inode.
async fn get_file_identity(file_path: &str) -> Option<String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        match tokio::fs::symlink_metadata(file_path).await {
            Ok(metadata) => {
                let dev = metadata.dev();
                let ino = metadata.ino();
                if dev == 0 && ino == 0 {
                    None
                } else {
                    Some(format!("{}:{}", dev, ino))
                }
            }
            Err(_) => None,
        }
    }
    #[cfg(not(unix))]
    {
        match tokio::fs::canonicalize(file_path).await {
            Ok(path) => Some(path.to_string_lossy().to_string()),
            Err(_) => None,
        }
    }
}

/// Traverse from the current directory up to the git root,
/// collecting all .mossen directories along the way.
pub fn get_project_dirs_up_to_home(
    subdir: &str,
    cwd: &str,
    home_dir: &str,
    git_root: Option<&str>,
) -> Vec<PathBuf> {
    let home = PathBuf::from(home_dir);
    let mut current = PathBuf::from(cwd);
    let mut dirs = Vec::new();

    loop {
        // Stop at home directory
        if current == home {
            break;
        }

        let config_subdir = current.join(".mossen").join(subdir);
        if config_subdir.exists() {
            dirs.push(config_subdir);
        }

        // Stop after processing git root
        if let Some(root) = git_root {
            if current == Path::new(root) {
                break;
            }
        }

        // Move to parent
        match current.parent() {
            Some(parent) if parent != current => {
                current = parent.to_path_buf();
            }
            _ => break,
        }
    }

    dirs
}

/// Load markdown files from managed, user, and project directories.
pub async fn load_markdown_files_for_subdir(
    subdir: &str,
    cwd: &str,
    config_home_dir: &str,
    managed_dir: &str,
    home_dir: &str,
    git_root: Option<&str>,
) -> Vec<MarkdownFile> {
    let user_dir = PathBuf::from(config_home_dir).join(subdir);
    let managed_path = PathBuf::from(managed_dir).join(subdir);
    let project_dirs = get_project_dirs_up_to_home(subdir, cwd, home_dir, git_root);

    // Load from all sources concurrently
    let managed_files = load_markdown_files(&managed_path).await;
    let user_files = load_markdown_files(&user_dir).await;
    let mut project_files = Vec::new();
    for dir in &project_dirs {
        let files = load_markdown_files(dir).await;
        project_files.extend(files);
    }

    // Assign sources
    let mut all_files: Vec<MarkdownFile> = Vec::new();

    for file in managed_files {
        all_files.push(MarkdownFile {
            file_path: file.file_path,
            base_dir: managed_path.to_string_lossy().to_string(),
            frontmatter: file.frontmatter,
            content: file.content,
            source: SettingSource::PolicySettings,
        });
    }

    for file in user_files {
        all_files.push(MarkdownFile {
            file_path: file.file_path,
            base_dir: user_dir.to_string_lossy().to_string(),
            frontmatter: file.frontmatter,
            content: file.content,
            source: SettingSource::UserSettings,
        });
    }

    for file in project_files {
        all_files.push(MarkdownFile {
            file_path: file.file_path.clone(),
            base_dir: file.base_dir,
            frontmatter: file.frontmatter,
            content: file.content,
            source: SettingSource::ProjectSettings,
        });
    }

    // Deduplicate by file identity
    let mut seen_ids: HashMap<String, SettingSource> = HashMap::new();
    let mut deduplicated = Vec::new();

    for file in &all_files {
        if let Some(file_id) = get_file_identity(&file.file_path).await {
            if seen_ids.contains_key(&file_id) {
                continue;
            }
            seen_ids.insert(file_id, file.source);
        }
        deduplicated.push(file.clone());
    }

    deduplicated
}

/// Parsed markdown file (before source assignment).
#[derive(Debug, Clone)]
struct ParsedMarkdownFile {
    file_path: String,
    base_dir: String,
    frontmatter: FrontmatterData,
    content: String,
}

/// Load markdown files from a directory.
async fn load_markdown_files(dir: &Path) -> Vec<ParsedMarkdownFile> {
    let files = find_markdown_files(dir).await;
    let mut results = Vec::new();

    for file_path in files {
        match tokio::fs::read_to_string(&file_path).await {
            Ok(raw_content) => {
                let (frontmatter, content) = parse_frontmatter(&raw_content);
                results.push(ParsedMarkdownFile {
                    file_path: file_path.clone(),
                    base_dir: dir.to_string_lossy().to_string(),
                    frontmatter,
                    content,
                });
            }
            Err(_) => continue,
        }
    }

    results
}

/// Find markdown files in a directory (recursive).
async fn find_markdown_files(dir: &Path) -> Vec<String> {
    let mut files = Vec::new();
    let mut visited = HashSet::new();

    async fn walk(dir: &Path, files: &mut Vec<String>, visited: &mut HashSet<String>) {
        let dir_key = match tokio::fs::canonicalize(dir).await {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(_) => return,
        };

        if visited.contains(&dir_key) {
            return;
        }
        visited.insert(dir_key);

        let mut entries = match tokio::fs::read_dir(dir).await {
            Ok(e) => e,
            Err(_) => return,
        };

        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            let file_type = match entry.file_type().await {
                Ok(ft) => ft,
                Err(_) => continue,
            };

            if file_type.is_dir() || file_type.is_symlink() {
                if let Ok(metadata) = tokio::fs::metadata(&path).await {
                    if metadata.is_dir() {
                        Box::pin(walk(&path, files, visited)).await;
                    } else if metadata.is_file()
                        && path.extension().and_then(|e| e.to_str()) == Some("md")
                    {
                        files.push(path.to_string_lossy().to_string());
                    }
                }
            } else if file_type.is_file() && path.extension().and_then(|e| e.to_str()) == Some("md")
            {
                files.push(path.to_string_lossy().to_string());
            }
        }
    }

    walk(dir, &mut files, &mut visited).await;
    files
}

/// Parse frontmatter from markdown content.
fn parse_frontmatter(raw_content: &str) -> (FrontmatterData, String) {
    if !raw_content.starts_with("---") {
        return (FrontmatterData::default(), raw_content.to_string());
    }

    let rest = &raw_content[3..];
    if let Some(end_idx) = rest.find("\n---") {
        let yaml_str = &rest[..end_idx];
        let content = &rest[end_idx + 4..];
        let content = content.strip_prefix('\n').unwrap_or(content);

        let data: HashMap<String, serde_json::Value> =
            serde_yaml::from_str(yaml_str).unwrap_or_default();

        (FrontmatterData { data }, content.to_string())
    } else {
        (FrontmatterData::default(), raw_content.to_string())
    }
}

/// Parse tool list from CLI format.
fn parse_tool_list_from_cli(tools: &[String]) -> Vec<String> {
    let mut result = Vec::new();
    for tool in tools {
        for part in tool.split(',') {
            let trimmed = part.trim();
            if !trimmed.is_empty() {
                result.push(trimmed.to_string());
            }
        }
    }
    result
}
