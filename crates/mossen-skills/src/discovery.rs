//! # discovery — 插件与技能发现
//!
//! 对应 TypeScript `skills/loadSkillsDir.ts` 中的目录遍历与发现逻辑。
//! 负责从 /skills/、/commands/ 目录中发现并加载技能文件。

use std::path::{Path, PathBuf};

use tracing::debug;

// ---------------------------------------------------------------------------
// 技能目录发现
// ---------------------------------------------------------------------------

/// 沿文件路径向上发现技能目录。
///
/// 对应 TS `discoverSkillDirsForPaths()`。
/// 从文件的父目录开始向上遍历到 cwd（不含 cwd 自身），查找 `.mossen/skills` 目录。
/// 返回新发现的技能目录列表，按深度降序排列（最深的优先）。
pub async fn discover_skill_dirs_for_paths(
    file_paths: &[&str],
    cwd: &str,
    discovered_dirs: &mut std::collections::HashSet<String>,
) -> Vec<PathBuf> {
    let resolved_cwd = cwd.strip_suffix(std::path::MAIN_SEPARATOR).unwrap_or(cwd);
    let mut new_dirs: Vec<PathBuf> = Vec::new();

    for file_path in file_paths {
        let mut current_dir = match Path::new(file_path).parent() {
            Some(p) => p.to_path_buf(),
            None => continue,
        };

        // 向上遍历到 cwd（不含 cwd 自身）
        let cwd_prefix = format!("{}{}", resolved_cwd, std::path::MAIN_SEPARATOR);
        while current_dir
            .to_str()
            .is_some_and(|s| s.starts_with(&cwd_prefix))
        {
            let skill_dir = get_scoped_config_dir(&current_dir).join("skills");
            let skill_dir_str = skill_dir.to_string_lossy().to_string();

            if !discovered_dirs.contains(&skill_dir_str) {
                discovered_dirs.insert(skill_dir_str.clone());
                if skill_dir.is_dir() {
                    // 检查是否被 gitignore
                    // （简化：此处不做 git check-ignore，留给上层处理）
                    new_dirs.push(skill_dir);
                }
            }

            // 向上移动
            match current_dir.parent() {
                Some(parent) if parent != current_dir => {
                    current_dir = parent.to_path_buf();
                }
                _ => break,
            }
        }
    }

    // 按路径深度降序排列（最深优先）
    new_dirs.sort_by(|a, b| {
        let depth_a = a.components().count();
        let depth_b = b.components().count();
        depth_b.cmp(&depth_a)
    });

    new_dirs
}

/// 获取项目目录上溯到 Home 的 skills 目录列表。
///
/// 对应 TS `getProjectDirsUpToHome('skills', cwd)`。
pub fn get_project_skills_dirs(cwd: &str) -> Vec<PathBuf> {
    let home = dirs_home().unwrap_or_else(|| PathBuf::from("/"));
    let mut dirs = Vec::new();
    let mut current = PathBuf::from(cwd);

    loop {
        let skill_dir = get_scoped_config_dir(&current).join("skills");
        dirs.push(skill_dir);

        if current == home || current.parent().is_none() {
            break;
        }
        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => break,
        }
    }

    dirs
}

/// 获取技能路径。
///
/// 对应 TS `getSkillsPath(source, dir)`。
pub fn get_skills_path(source: &str, dir: &str) -> String {
    match source {
        "userSettings" => {
            let config_home = mossen_config_home_dir();
            format!("{}/{}", config_home.display(), dir)
        }
        "projectSettings" => {
            format!(".mossen/{}", dir)
        }
        "policySettings" => {
            let managed = managed_file_path();
            format!("{}/{}", get_scoped_config_dir(&managed).display(), dir)
        }
        "plugin" => "plugin".to_string(),
        _ => String::new(),
    }
}

// ---------------------------------------------------------------------------
// MCP 技能发现
// ---------------------------------------------------------------------------

/// MCP 技能资源过滤 — 检查资源 URI 是否为技能。
///
/// 对应 TS `isSkillResource(resource)`。
pub fn is_skill_resource_uri(uri: &str) -> bool {
    uri.starts_with("skill://")
}

/// 从资源 URI 提取技能名称。
///
/// 对应 TS `getSkillResourceName(resource)`。
pub fn get_skill_resource_name(uri: &str, display_name: Option<&str>) -> String {
    if let Some(name) = display_name {
        let trimmed = name.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    // 从 URI 解析
    let without_scheme = uri.strip_prefix("skill://").unwrap_or(uri);
    let first_part = without_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("skill");
    first_part.to_string()
}

/// 构建 MCP 技能名称。
///
/// 格式：`mcp__{server_name}__{resource_name}`。
pub fn build_mcp_skill_name(server_name: &str, resource_name: &str) -> String {
    format!(
        "mcp__{}__{}",
        normalize_name_for_mcp(server_name),
        normalize_name_for_mcp(resource_name)
    )
}

/// 规范化 MCP 名称（替换非法字符为下划线）。
fn normalize_name_for_mcp(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// 辅助函数
// ---------------------------------------------------------------------------

/// 获取 scoped 配置目录（.mossen 目录）。
fn get_scoped_config_dir(base: &Path) -> PathBuf {
    base.join(".mossen")
}

/// 获取 Mossen 配置 Home 目录。
fn mossen_config_home_dir() -> PathBuf {
    if let Ok(val) = std::env::var("MOSSEN_CONFIG_DIR") {
        return PathBuf::from(val);
    }
    dirs_home()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".mossen")
}

/// 获取托管文件路径。
fn managed_file_path() -> PathBuf {
    if let Ok(val) = std::env::var("MOSSEN_MANAGED_PATH") {
        return PathBuf::from(val);
    }
    mossen_config_home_dir().join("managed")
}

/// 获取用户 Home 目录。
fn dirs_home() -> Option<PathBuf> {
    #[cfg(unix)]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    }
    #[cfg(not(any(unix, windows)))]
    {
        None
    }
}
