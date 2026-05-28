//! 配置目录命名工具。
//!
//! 翻译自 `utils/naming.ts`。

use std::path::{Path, PathBuf};

/// 规范项目指令文件名。
const CANONICAL_PROJECT_INSTRUCTIONS_FILENAME: &str = "MOSSEN.md";

/// 规范配置目录名。
const CANONICAL_CONFIG_DIRNAME: &str = ".mossen";

/// 配置目录环境变量名。
const CANONICAL_CONFIG_DIR_ENV: &str = "MOSSEN_CONFIG_DIR";

/// 获取项目指令文件显示名称。
pub fn get_project_instructions_display_name() -> &'static str {
    CANONICAL_PROJECT_INSTRUCTIONS_FILENAME
}

/// 获取规范配置目录名。
pub fn get_canonical_config_dir_name() -> &'static str {
    CANONICAL_CONFIG_DIRNAME
}

/// 获取规范配置目录环境变量名。
pub fn get_canonical_config_dir_env_name() -> &'static str {
    CANONICAL_CONFIG_DIR_ENV
}

/// 获取规范配置主目录路径。
pub fn get_canonical_config_home_dir() -> PathBuf {
    if let Ok(canonical_override) = std::env::var(CANONICAL_CONFIG_DIR_ENV) {
        if !canonical_override.is_empty() {
            return PathBuf::from(canonical_override);
        }
    }
    dirs::home_dir()
        .unwrap_or_default()
        .join(CANONICAL_CONFIG_DIRNAME)
}

/// 获取配置主目录候选路径列表。
pub fn get_config_home_dir_candidates() -> Vec<PathBuf> {
    if let Ok(canonical_override) = std::env::var(CANONICAL_CONFIG_DIR_ENV) {
        if !canonical_override.is_empty() {
            return vec![PathBuf::from(canonical_override)];
        }
    }
    vec![dirs::home_dir()
        .unwrap_or_default()
        .join(CANONICAL_CONFIG_DIRNAME)]
}

/// 获取已解析的配置主目录。
pub fn get_resolved_config_home_dir() -> PathBuf {
    get_config_home_dir_candidates()
        .into_iter()
        .next()
        .unwrap_or_default()
}

/// 获取主项目指令文件路径。
pub fn get_primary_project_instructions_path(dir: &Path) -> PathBuf {
    dir.join(CANONICAL_PROJECT_INSTRUCTIONS_FILENAME)
}

/// 获取项目指令文件读取候选路径列表。
pub fn get_project_instructions_read_candidates(dir: &Path) -> Vec<PathBuf> {
    vec![get_primary_project_instructions_path(dir)]
}

/// 获取主范围配置目录路径。
pub fn get_primary_scoped_config_dir(dir: &Path) -> PathBuf {
    dir.join(CANONICAL_CONFIG_DIRNAME)
}

/// 获取范围配置目录读取候选路径列表。
pub fn get_scoped_config_dir_read_candidates(dir: &Path) -> Vec<PathBuf> {
    vec![get_primary_scoped_config_dir(dir)]
}

/// 获取范围配置指令文件读取候选路径列表。
pub fn get_scoped_config_instructions_read_candidates(dir: &Path) -> Vec<PathBuf> {
    vec![get_primary_scoped_config_dir(dir).join(CANONICAL_PROJECT_INSTRUCTIONS_FILENAME)]
}

/// 获取范围规则目录读取候选路径列表。
pub fn get_scoped_rules_dir_read_candidates(dir: &Path) -> Vec<PathBuf> {
    vec![get_primary_scoped_config_dir(dir).join("rules")]
}

/// 获取主目录指令文件读取候选路径列表。
pub fn get_home_instructions_read_candidates() -> Vec<PathBuf> {
    vec![get_canonical_config_home_dir().join(CANONICAL_PROJECT_INSTRUCTIONS_FILENAME)]
}

/// 获取主目录规则目录读取候选路径列表。
pub fn get_home_rules_dir_read_candidates() -> Vec<PathBuf> {
    vec![get_canonical_config_home_dir().join("rules")]
}
