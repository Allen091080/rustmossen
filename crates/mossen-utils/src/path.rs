//! 路径工具
//!
//! 对应 TS `path.ts`。

use std::path::{Path, PathBuf};

/// 展开可能包含波浪号表示法（~）的路径为绝对路径。
///
/// 在 Windows 上，POSIX 风格路径（例如 `/c/Users/...`）会自动转换为
/// Windows 格式（例如 `C:\Users\...`）。
/// 函数始终返回适合当前平台的本机格式路径。
///
/// # 参数
/// - `path`: 要展开的路径，可能包含:
///   - `~` - 展开为用户主目录
///   - `~/path` - 展开为用户主目录下的路径
///   - 绝对路径 - 返回规范化后的路径
///   - 相对路径 - 相对于 base_dir 解析
///   - Windows 上的 POSIX 路径 - 转换为 Windows 格式
/// - `base_dir`: 解析相对路径的基目录（默认为当前工作目录）
///
/// # 返回
/// 展开后的绝对路径，使用本机格式
pub fn expand_path(path: &str, base_dir: Option<&str>) -> PathBuf {
    let actual_base_dir = base_dir
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    // 空路径
    let trimmed_path = path.trim();
    if trimmed_path.is_empty() {
        return normalize_path(&actual_base_dir);
    }

    // 处理主目录表示法
    if trimmed_path == "~" {
        if let Some(home) = dirs::home_dir() {
            return normalize_path(&home);
        }
        return normalize_path(&actual_base_dir);
    }

    if trimmed_path.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            let relative = &trimmed_path[2..];
            return normalize_path(&home.join(relative));
        }
    }

    // 处理绝对路径
    if Path::new(trimmed_path).is_absolute() {
        return normalize_path(Path::new(trimmed_path));
    }

    // 处理相对路径
    normalize_path(&actual_base_dir.join(trimmed_path))
}

/// 将绝对路径转换为相对于 cwd 的路径。
///
/// 如果路径在 cwd 之外（相对路径以 .. 开头），
/// 则返回原始绝对路径以保持明确性。
pub fn to_relative_path(absolute_path: &str) -> String {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let relative = cwd.join(absolute_path);

    if let Ok(rel) = relative.strip_prefix(&cwd) {
        let rel_str = rel.to_string_lossy();
        if rel_str.starts_with("..") {
            absolute_path.to_string()
        } else {
            rel_str.to_string()
        }
    } else {
        absolute_path.to_string()
    }
}

/// 获取给定文件或目录路径的目录路径。
///
/// 如果路径是目录，返回路径本身。
/// 如果路径是文件或不存在，返回父目录。
pub fn get_directory_for_path(path: &str) -> PathBuf {
    let absolute_path = expand_path(path, None);

    // 安全：跳过 UNC 路径的文件系统操作
    let path_str = absolute_path.to_string_lossy();
    if path_str.starts_with("\\\\") || path_str.starts_with("//") {
        return absolute_path
            .parent()
            .map(PathBuf::from)
            .unwrap_or(absolute_path);
    }

    // 检查是否是目录
    if absolute_path.is_dir() {
        return absolute_path;
    }

    // 如果不是目录或不存在，返回父目录
    absolute_path
        .parent()
        .map(PathBuf::from)
        .unwrap_or(absolute_path)
}

/// 检查路径是否包含目录遍历模式。
///
/// 目录遍历模式包括 '../'、'..\' 或以 '..' 结尾。
pub fn contains_path_traversal(path: &str) -> bool {
    path.contains("..")
}

/// 规范化路径用于 JSON 配置键。
///
/// 在 Windows 上，路径可能有不一致的分隔符（取决于来源）。
/// 这会规范化为正斜杠以保持一致的 JSON 序列化。
pub fn normalize_path_for_config_key(path: &str) -> String {
    let normalized = normalize_path(Path::new(path));
    normalized.to_string_lossy().replace('\\', "/")
}

/// 规范化路径。
fn normalize_path(path: &Path) -> PathBuf {
    path.components()
        .collect::<PathBuf>()
        .components()
        .filter(|c| !matches!(c, std::path::Component::ParentDir))
        .collect()
}
