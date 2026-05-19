use std::path::{Path, PathBuf, MAIN_SEPARATOR};
use regex::Regex;

/// Extract the static base directory from a glob pattern.
/// The base directory is everything before the first glob special character (* ? [ {).
/// Returns the directory portion and the remaining relative pattern.
pub fn extract_glob_base_directory(pattern: &str) -> (String, String) {
    // Find the first glob special character: *, ?, [, {
    let glob_re = Regex::new(r"[*?\[{]").unwrap();

    let match_pos = glob_re.find(pattern);

    if match_pos.is_none() {
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
        return (dir, file);
    }

    let match_idx = match_pos.unwrap().start();
    let static_prefix = &pattern[..match_idx];

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

    if last_sep_index.is_none() {
        // No path separator before the glob - pattern is relative to cwd
        return (String::new(), pattern.to_string());
    }

    let last_sep = last_sep_index.unwrap();
    let mut base_dir = pattern[..last_sep].to_string();
    let relative_pattern = pattern[last_sep + 1..].to_string();

    // Handle root directory patterns
    if base_dir.is_empty() && last_sep == 0 {
        base_dir = "/".to_string();
    }

    // Handle Windows drive root paths (e.g., C:/*.txt)
    if cfg!(windows) {
        let drive_re = Regex::new(r"^[A-Za-z]:$").unwrap();
        if drive_re.is_match(&base_dir) {
            base_dir = format!("{}{}", base_dir, MAIN_SEPARATOR);
        }
    }

    (base_dir, relative_pattern)
}

/// Glob file matching using ripgrep backend.
/// Returns matched file paths and whether results were truncated.
pub async fn glob_files(
    file_pattern: &str,
    cwd: &str,
    limit: usize,
    offset: usize,
    ignore_patterns: &[String],
    no_ignore: bool,
    hidden: bool,
    plugin_exclusions: &[String],
    rg_runner: impl AsyncRipgrepRunner,
) -> Result<GlobResult, std::io::Error> {
    let mut search_dir = cwd.to_string();
    let mut search_pattern = file_pattern.to_string();

    // Handle absolute paths by extracting the base directory
    if Path::new(file_pattern).is_absolute() {
        let (base_dir, relative_pattern) = extract_glob_base_directory(file_pattern);
        if !base_dir.is_empty() {
            search_dir = base_dir;
            search_pattern = relative_pattern;
        }
    }

    let mut args = vec![
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

    // Exclude orphaned plugin version directories
    for exclusion in plugin_exclusions {
        args.push("--glob".to_string());
        args.push(exclusion.clone());
    }

    let all_paths = rg_runner.run(&args, &search_dir).await?;

    // Convert relative paths to absolute
    let absolute_paths: Vec<String> = all_paths
        .into_iter()
        .map(|p| {
            if Path::new(&p).is_absolute() {
                p
            } else {
                PathBuf::from(&search_dir)
                    .join(&p)
                    .to_string_lossy()
                    .to_string()
            }
        })
        .collect();

    let truncated = absolute_paths.len() > offset + limit;
    let files = absolute_paths
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect();

    Ok(GlobResult { files, truncated })
}

/// Result of a glob operation
#[derive(Debug, Clone)]
pub struct GlobResult {
    pub files: Vec<String>,
    pub truncated: bool,
}

/// Trait for running ripgrep commands (allows testing/mocking)
#[async_trait::async_trait]
pub trait AsyncRipgrepRunner {
    async fn run(&self, args: &[String], cwd: &str) -> Result<Vec<String>, std::io::Error>;
}
