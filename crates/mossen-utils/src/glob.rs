//! # glob — Glob 文件匹配工具
//!
//! 对应 TypeScript `utils/glob.ts`。
//! 提供从 glob 模式提取基目录以及使用 ripgrep 执行 glob 搜索的功能。

use std::path::{Path, PathBuf, MAIN_SEPARATOR};
use tokio_util::sync::CancellationToken;

/// 从 glob 模式中提取静态基目录的结果
pub struct GlobBaseDirectory {
    pub base_dir: String,
    pub relative_pattern: String,
}

/// 从 glob 模式中提取静态基目录。
///
/// 基目录是第一个 glob 特殊字符 `*`, `?`, `[`, `{` 之前的所有内容。
/// 返回目录部分和剩余的相对模式。
pub fn extract_glob_base_directory(pattern: &str) -> GlobBaseDirectory {
    // Find the first glob special character: *, ?, [, {
    let glob_pos = pattern.find(&['*', '?', '[', '{'][..]);

    match glob_pos {
        None => {
            // No glob characters - this is a literal path
            let path = Path::new(pattern);
            let dir = path
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| ".".to_string());
            let file = path
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| pattern.to_string());
            GlobBaseDirectory {
                base_dir: dir,
                relative_pattern: file,
            }
        }
        Some(idx) => {
            let static_prefix = &pattern[..idx];

            // Find the last path separator in the static prefix
            let last_sep_fwd = static_prefix.rfind('/');
            let last_sep_native = if MAIN_SEPARATOR != '/' {
                static_prefix.rfind(MAIN_SEPARATOR)
            } else {
                None
            };
            let last_sep_index = match (last_sep_fwd, last_sep_native) {
                (Some(a), Some(b)) => Some(a.max(b)),
                (Some(a), None) => Some(a),
                (None, Some(b)) => Some(b),
                (None, None) => None,
            };

            match last_sep_index {
                None => {
                    // No path separator before the glob - pattern is relative to cwd
                    GlobBaseDirectory {
                        base_dir: String::new(),
                        relative_pattern: pattern.to_string(),
                    }
                }
                Some(sep_idx) => {
                    let mut base_dir = static_prefix[..sep_idx].to_string();
                    let relative_pattern = pattern[sep_idx + 1..].to_string();

                    // Handle root directory patterns (e.g., /*.txt on Unix)
                    if base_dir.is_empty() && sep_idx == 0 {
                        base_dir = "/".to_string();
                    }

                    // Handle Windows drive root paths (e.g., C:/*.txt)
                    if cfg!(windows) {
                        let bytes = base_dir.as_bytes();
                        if bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
                            base_dir.push(MAIN_SEPARATOR);
                        }
                    }

                    GlobBaseDirectory {
                        base_dir,
                        relative_pattern,
                    }
                }
            }
        }
    }
}

/// Glob 搜索结果
pub struct GlobResult {
    pub files: Vec<PathBuf>,
    pub truncated: bool,
}

/// 使用 ripgrep 执行 glob 文件搜索。
///
/// # 参数
/// - `file_pattern`: glob 文件模式
/// - `cwd`: 当前工作目录
/// - `limit`: 最多返回的文件数
/// - `offset`: 跳过前 offset 个文件
/// - `cancel_token`: 取消令牌
/// - `ignore_patterns`: 要忽略的文件模式列表
///
/// # 返回
/// 匹配文件列表和是否被截断的标志
pub async fn glob_search(
    file_pattern: &str,
    cwd: &Path,
    limit: usize,
    offset: usize,
    cancel_token: &CancellationToken,
    ignore_patterns: &[String],
) -> anyhow::Result<GlobResult> {
    let mut search_dir = cwd.to_path_buf();
    let mut search_pattern = file_pattern.to_string();

    // Handle absolute paths by extracting the base directory and converting to relative pattern
    if Path::new(file_pattern).is_absolute() {
        let extracted = extract_glob_base_directory(file_pattern);
        if !extracted.base_dir.is_empty() {
            search_dir = PathBuf::from(&extracted.base_dir);
            search_pattern = extracted.relative_pattern;
        }
    }

    // Use ripgrep --files --glob for better memory performance
    let no_ignore = std::env::var("MOSSEN_CODE_GLOB_NO_IGNORE")
        .ok()
        .map(|v| is_env_truthy(&v))
        .unwrap_or(true);
    let hidden = std::env::var("MOSSEN_CODE_GLOB_HIDDEN")
        .ok()
        .map(|v| is_env_truthy(&v))
        .unwrap_or(true);

    let mut args: Vec<String> = vec![
        "--files".to_string(),
        "--glob".to_string(),
        search_pattern,
        "--sort=modified".to_string(),
    ];
    if no_ignore {
        args.push("--no-ignore".to_string());
    }
    if hidden {
        args.push("--hidden".to_string());
    }

    // Add ignore patterns
    for pattern in ignore_patterns {
        args.push("--glob".to_string());
        args.push(format!("!{}", pattern));
    }

    // Execute ripgrep
    let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let output =
        crate::ripgrep::rip_grep(&args_refs, &search_dir.to_string_lossy(), cancel_token).await?;

    // ripgrep returns relative paths, convert to absolute
    let absolute_paths: Vec<PathBuf> = output
        .iter()
        .map(|p| {
            let path = Path::new(p);
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                search_dir.join(p)
            }
        })
        .collect();

    let truncated = absolute_paths.len() > offset + limit;
    let files: Vec<PathBuf> = absolute_paths
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect();

    Ok(GlobResult { files, truncated })
}

/// 对应 TS `glob` 默认导出（参数顺序与 [`glob_search`] 一致）。
pub async fn glob(
    file_pattern: &str,
    cwd: &Path,
    limit: usize,
    offset: usize,
    cancel_token: &CancellationToken,
    ignore_patterns: &[String],
) -> anyhow::Result<GlobResult> {
    glob_search(
        file_pattern,
        cwd,
        limit,
        offset,
        cancel_token,
        ignore_patterns,
    )
    .await
}

/// 简单的环境变量真值检查
fn is_env_truthy(value: &str) -> bool {
    matches!(value.to_lowercase().as_str(), "1" | "true" | "yes")
}
