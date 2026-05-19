//! Example commands — suggest context-aware example prompts to users.

use std::collections::HashMap;
use std::sync::Mutex;

use once_cell::sync::Lazy;
use rand::seq::SliceRandom;
use regex::Regex;

/// Patterns that mark a file as non-core (auto-generated, dependency, or config).
static NON_CORE_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        Regex::new(r"(?:^|/)(?:package-lock\.json|yarn\.lock|bun\.lock|bun\.lockb|pnpm-lock\.yaml|Pipfile\.lock|poetry\.lock|Cargo\.lock|Gemfile\.lock|go\.sum|composer\.lock|uv\.lock)$").unwrap(),
        Regex::new(r"\.generated\.").unwrap(),
        Regex::new(r"(?:^|/)(?:dist|build|out|target|node_modules|\.next|__pycache__)/").unwrap(),
        Regex::new(r"\.(?:min\.js|min\.css|map|pyc|pyo)$").unwrap(),
        Regex::new(r"(?i)\.(?:json|ya?ml|toml|xml|ini|cfg|conf|env|lock|txt|md|mdx|rst|csv|log|svg)$").unwrap(),
        Regex::new(r"(?:^|/)\.?(?:eslintrc|prettierrc|babelrc|editorconfig|gitignore|gitattributes|dockerignore|npmrc)").unwrap(),
        Regex::new(r"(?:^|/)(?:tsconfig|jsconfig|biome|vitest\.config|jest\.config|webpack\.config|vite\.config|rollup\.config)\.[a-z]+$").unwrap(),
        Regex::new(r"(?:^|/)\.(?:github|vscode|idea|mossen)/").unwrap(),
        Regex::new(r"(?i)(?:^|/)(?:CHANGELOG|LICENSE|CONTRIBUTING|CODEOWNERS|README)(?:\.[a-z]+)?$").unwrap(),
    ]
});

fn is_core_file(path: &str) -> bool {
    !NON_CORE_PATTERNS.iter().any(|p| p.is_match(path))
}

/// Counts occurrences of items and returns the top N items sorted by count descending.
pub fn count_and_sort_items(items: &[String], top_n: usize) -> String {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for item in items {
        *counts.entry(item.as_str()).or_insert(0) += 1;
    }
    let mut sorted: Vec<_> = counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));
    sorted
        .iter()
        .take(top_n)
        .map(|(item, count)| format!("{:>6} {}", count, item))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Picks up to `want` basenames from a frequency-sorted list of paths,
/// skipping non-core files and spreading across different directories.
pub fn pick_diverse_core_files(sorted_paths: &[String], want: usize) -> Vec<String> {
    let mut picked = Vec::new();
    let mut seen_basenames = std::collections::HashSet::new();
    let mut dir_tally: HashMap<String, usize> = HashMap::new();

    for cap in 1..=want {
        for p in sorted_paths {
            if picked.len() >= want {
                break;
            }
            if !is_core_file(p) {
                continue;
            }
            let last_sep = p.rfind('/').or_else(|| p.rfind('\\'));
            let base = match last_sep {
                Some(idx) => &p[idx + 1..],
                None => p.as_str(),
            };
            if base.is_empty() || seen_basenames.contains(base) {
                continue;
            }
            let dir = match last_sep {
                Some(idx) => &p[..idx],
                None => ".",
            };
            let dir_count = dir_tally.get(dir).copied().unwrap_or(0);
            if dir_count >= cap {
                continue;
            }
            picked.push(base.to_string());
            seen_basenames.insert(base.to_string());
            *dir_tally.entry(dir.to_string()).or_insert(0) += 1;
        }
    }

    if picked.len() >= want {
        picked
    } else {
        Vec::new()
    }
}

/// Cached example files state.
static EXAMPLE_CACHE: Lazy<Mutex<Option<Vec<String>>>> = Lazy::new(|| Mutex::new(None));

/// Get a random example command from cache.
pub fn get_example_command_from_cache() -> String {
    let cache = EXAMPLE_CACHE.lock().unwrap();
    let frequent_file = cache
        .as_ref()
        .and_then(|files| {
            let mut rng = rand::thread_rng();
            files.choose(&mut rng).cloned()
        })
        .unwrap_or_else(|| "<filepath>".to_string());

    let commands = vec![
        "fix lint errors".to_string(),
        "fix typecheck errors".to_string(),
        format!("how does {} work?", frequent_file),
        format!("refactor {}", frequent_file),
        "how do I log an error?".to_string(),
        format!("edit {} to...", frequent_file),
        format!("write a test for {}", frequent_file),
        "create a util logging.py that...".to_string(),
    ];

    let mut rng = rand::thread_rng();
    let selected = commands.choose(&mut rng).cloned().unwrap_or_default();
    format!("Try \"{}\"", selected)
}

/// Set example files in cache.
pub fn set_example_files(files: Vec<String>) {
    let mut cache = EXAMPLE_CACHE.lock().unwrap();
    *cache = Some(files);
}

/// 对应 TS `refreshExampleCommands`：异步刷新缓存的示例文件列表。
/// 当缓存为空或超过一周时通过 git 历史重建。
pub async fn refresh_example_commands(cwd: &str, git_exe: &str, user_email: Option<&str>) {
    {
        let cache = EXAMPLE_CACHE.lock().unwrap();
        if cache.as_ref().map(|v| !v.is_empty()).unwrap_or(false) {
            return;
        }
    }
    let files = get_frequently_modified_files(cwd, git_exe, user_email).await;
    if !files.is_empty() {
        set_example_files(files);
    }
}

/// Get frequently modified files from git history.
pub async fn get_frequently_modified_files(
    cwd: &str,
    git_exe: &str,
    user_email: Option<&str>,
) -> Vec<String> {
    use tokio::process::Command;

    let mut counts: HashMap<String, usize> = HashMap::new();

    let tally_into = |stdout: &str, counts: &mut HashMap<String, usize>| {
        for line in stdout.lines() {
            let f = line.trim();
            if !f.is_empty() {
                *counts.entry(f.to_string()).or_insert(0) += 1;
            }
        }
    };

    let base_args = [
        "log", "-n", "1000", "--pretty=format:", "--name-only", "--diff-filter=M",
    ];

    if let Some(email) = user_email {
        let output = Command::new(git_exe)
            .args(&base_args)
            .arg(format!("--author={}", email))
            .current_dir(cwd)
            .output()
            .await;

        if let Ok(out) = output {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                tally_into(&stdout, &mut counts);
            }
        }
    }

    // Fall back to all authors if the user's own history is thin
    if counts.len() < 10 {
        let output = Command::new(git_exe)
            .args(&base_args)
            .current_dir(cwd)
            .output()
            .await;

        if let Ok(out) = output {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                tally_into(&stdout, &mut counts);
            }
        }
    }

    let mut sorted: Vec<_> = counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));
    let sorted_paths: Vec<String> = sorted.into_iter().map(|(p, _)| p).collect();

    pick_diverse_core_files(&sorted_paths, 5)
}
