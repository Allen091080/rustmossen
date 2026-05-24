//! Git diff utilities: fetch stats, hunks, and per-file diffs.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::process::Command;

const GIT_TIMEOUT_MS: u64 = 5000;
const MAX_FILES: usize = 50;
const MAX_DIFF_SIZE_BYTES: usize = 1_000_000;
const MAX_LINES_PER_FILE: usize = 400;
const MAX_FILES_FOR_DETAILS: usize = 500;
const SINGLE_FILE_DIFF_TIMEOUT_MS: u64 = 3000;

/// Git diff statistics.
#[derive(Debug, Clone, Default)]
pub struct GitDiffStats {
    pub files_count: usize,
    pub lines_added: usize,
    pub lines_removed: usize,
}

/// Per-file statistics.
#[derive(Debug, Clone)]
pub struct PerFileStats {
    pub added: usize,
    pub removed: usize,
    pub is_binary: bool,
    pub is_untracked: bool,
}

/// A structured patch hunk.
#[derive(Debug, Clone)]
pub struct StructuredPatchHunk {
    pub old_start: usize,
    pub old_lines: usize,
    pub new_start: usize,
    pub new_lines: usize,
    pub lines: Vec<String>,
}

/// Result of fetching git diff.
#[derive(Debug, Clone)]
pub struct GitDiffResult {
    pub stats: GitDiffStats,
    pub per_file_stats: HashMap<String, PerFileStats>,
    pub hunks: HashMap<String, Vec<StructuredPatchHunk>>,
}

/// Tool-use diff for a single file.
#[derive(Debug, Clone)]
pub struct ToolUseDiff {
    pub filename: String,
    pub status: DiffStatus,
    pub additions: usize,
    pub deletions: usize,
    pub changes: usize,
    pub patch: String,
    pub repository: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffStatus {
    Modified,
    Added,
}

/// 对应 TS `NumstatResult = { stats, perFileStats }`。
#[derive(Debug, Clone)]
pub struct NumstatResult {
    pub stats: GitDiffStats,
    pub per_file_stats: HashMap<String, PerFileStats>,
}

/// Parse `git diff --numstat` output.
pub fn parse_git_numstat(stdout: &str) -> (GitDiffStats, HashMap<String, PerFileStats>) {
    let mut added: usize = 0;
    let mut removed: usize = 0;
    let mut valid_file_count: usize = 0;
    let mut per_file_stats: HashMap<String, PerFileStats> = HashMap::new();

    for line in stdout.trim().lines().filter(|l| !l.is_empty()) {
        let parts: Vec<&str> = line.splitn(3, '\t').collect();
        if parts.len() < 3 {
            continue;
        }

        valid_file_count += 1;
        let add_str = parts[0];
        let rem_str = parts[1];
        let file_path = parts[2].to_string();
        let is_binary = add_str == "-" || rem_str == "-";
        let file_added = if is_binary {
            0
        } else {
            add_str.parse::<usize>().unwrap_or(0)
        };
        let file_removed = if is_binary {
            0
        } else {
            rem_str.parse::<usize>().unwrap_or(0)
        };

        added += file_added;
        removed += file_removed;

        if per_file_stats.len() < MAX_FILES {
            per_file_stats.insert(
                file_path,
                PerFileStats {
                    added: file_added,
                    removed: file_removed,
                    is_binary,
                    is_untracked: false,
                },
            );
        }
    }

    (
        GitDiffStats {
            files_count: valid_file_count,
            lines_added: added,
            lines_removed: removed,
        },
        per_file_stats,
    )
}

/// Parse unified diff output into per-file hunks.
pub fn parse_git_diff(stdout: &str) -> HashMap<String, Vec<StructuredPatchHunk>> {
    let mut result: HashMap<String, Vec<StructuredPatchHunk>> = HashMap::new();
    if stdout.trim().is_empty() {
        return result;
    }

    let header_re = regex::Regex::new(r"^a/(.+?) b/(.+)$").unwrap();
    let hunk_re = regex::Regex::new(r"^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@").unwrap();

    let file_diffs: Vec<&str> = stdout.split("diff --git ").skip(1).collect();

    for file_diff in file_diffs {
        if result.len() >= MAX_FILES {
            break;
        }

        if file_diff.len() > MAX_DIFF_SIZE_BYTES {
            continue;
        }

        let mut lines_iter = file_diff.lines();
        let first_line = match lines_iter.next() {
            Some(l) => l,
            None => continue,
        };

        let file_path = match header_re.captures(first_line) {
            Some(caps) => caps
                .get(2)
                .or(caps.get(1))
                .map(|m| m.as_str().to_string())
                .unwrap_or_default(),
            None => continue,
        };

        let mut file_hunks: Vec<StructuredPatchHunk> = Vec::new();
        let mut current_hunk: Option<StructuredPatchHunk> = None;
        let mut line_count: usize = 0;

        for line in lines_iter {
            if let Some(caps) = hunk_re.captures(line) {
                if let Some(hunk) = current_hunk.take() {
                    file_hunks.push(hunk);
                }
                current_hunk = Some(StructuredPatchHunk {
                    old_start: caps
                        .get(1)
                        .and_then(|m| m.as_str().parse().ok())
                        .unwrap_or(0),
                    old_lines: caps
                        .get(2)
                        .and_then(|m| m.as_str().parse().ok())
                        .unwrap_or(1),
                    new_start: caps
                        .get(3)
                        .and_then(|m| m.as_str().parse().ok())
                        .unwrap_or(0),
                    new_lines: caps
                        .get(4)
                        .and_then(|m| m.as_str().parse().ok())
                        .unwrap_or(1),
                    lines: Vec::new(),
                });
                continue;
            }

            if line.starts_with("index ")
                || line.starts_with("---")
                || line.starts_with("+++")
                || line.starts_with("new file")
                || line.starts_with("deleted file")
                || line.starts_with("old mode")
                || line.starts_with("new mode")
                || line.starts_with("Binary files")
            {
                continue;
            }

            if let Some(ref mut hunk) = current_hunk {
                if line.starts_with('+')
                    || line.starts_with('-')
                    || line.starts_with(' ')
                    || line.is_empty()
                {
                    if line_count >= MAX_LINES_PER_FILE {
                        continue;
                    }
                    hunk.lines.push(line.to_string());
                    line_count += 1;
                }
            }
        }

        if let Some(hunk) = current_hunk.take() {
            file_hunks.push(hunk);
        }

        if !file_hunks.is_empty() {
            result.insert(file_path, file_hunks);
        }
    }

    result
}

/// Parse git diff --shortstat output.
pub fn parse_shortstat(stdout: &str) -> Option<GitDiffStats> {
    let re = regex::Regex::new(
        r"(\d+)\s+files?\s+changed(?:,\s+(\d+)\s+insertions?\(\+\))?(?:,\s+(\d+)\s+deletions?\(-\))?",
    )
    .unwrap();

    let caps = re.captures(stdout)?;
    Some(GitDiffStats {
        files_count: caps
            .get(1)
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(0),
        lines_added: caps
            .get(2)
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(0),
        lines_removed: caps
            .get(3)
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(0),
    })
}

/// Check if we're in a transient git state.
pub async fn is_in_transient_git_state(git_dir: &Path) -> bool {
    let transient_files = [
        "MERGE_HEAD",
        "REBASE_HEAD",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
    ];

    for file in &transient_files {
        if fs::metadata(git_dir.join(file)).await.is_ok() {
            return true;
        }
    }
    false
}

/// Fetch git diff stats and hunks comparing working tree to HEAD.
pub async fn fetch_git_diff(cwd: &Path) -> Option<GitDiffResult> {
    // Check if git repo
    let status = Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .current_dir(cwd)
        .output()
        .await
        .ok()?;

    if !status.status.success() {
        return None;
    }

    let git_dir_str = String::from_utf8_lossy(&status.stdout).trim().to_string();
    let git_dir = if Path::new(&git_dir_str).is_absolute() {
        PathBuf::from(&git_dir_str)
    } else {
        cwd.join(&git_dir_str)
    };

    if is_in_transient_git_state(&git_dir).await {
        return None;
    }

    // Quick probe with --shortstat
    let shortstat_output = Command::new("git")
        .args(["--no-optional-locks", "diff", "HEAD", "--shortstat"])
        .current_dir(cwd)
        .output()
        .await
        .ok()?;

    if shortstat_output.status.success() {
        let shortstat_str = String::from_utf8_lossy(&shortstat_output.stdout);
        if let Some(quick_stats) = parse_shortstat(&shortstat_str) {
            if quick_stats.files_count > MAX_FILES_FOR_DETAILS {
                return Some(GitDiffResult {
                    stats: quick_stats,
                    per_file_stats: HashMap::new(),
                    hunks: HashMap::new(),
                });
            }
        }
    }

    // Get stats via --numstat
    let numstat_output = Command::new("git")
        .args(["--no-optional-locks", "diff", "HEAD", "--numstat"])
        .current_dir(cwd)
        .output()
        .await
        .ok()?;

    if !numstat_output.status.success() {
        return None;
    }

    let numstat_str = String::from_utf8_lossy(&numstat_output.stdout);
    let (mut stats, mut per_file_stats) = parse_git_numstat(&numstat_str);

    // Include untracked files
    let remaining_slots = MAX_FILES.saturating_sub(per_file_stats.len());
    if remaining_slots > 0 {
        if let Some(untracked) = fetch_untracked_files(cwd, remaining_slots).await {
            stats.files_count += untracked.len();
            for (path, file_stats) in untracked {
                per_file_stats.insert(path, file_stats);
            }
        }
    }

    Some(GitDiffResult {
        stats,
        per_file_stats,
        hunks: HashMap::new(),
    })
}

/// Fetch git diff hunks on-demand.
pub async fn fetch_git_diff_hunks(cwd: &Path) -> HashMap<String, Vec<StructuredPatchHunk>> {
    let output = Command::new("git")
        .args(["--no-optional-locks", "diff", "HEAD"])
        .current_dir(cwd)
        .output()
        .await;

    match output {
        Ok(o) if o.status.success() => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            parse_git_diff(&stdout)
        }
        _ => HashMap::new(),
    }
}

/// Fetch untracked file names.
async fn fetch_untracked_files(
    cwd: &Path,
    max_files: usize,
) -> Option<HashMap<String, PerFileStats>> {
    let output = Command::new("git")
        .args([
            "--no-optional-locks",
            "ls-files",
            "--others",
            "--exclude-standard",
        ])
        .current_dir(cwd)
        .output()
        .await
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let paths: Vec<&str> = stdout.trim().lines().filter(|l| !l.is_empty()).collect();
    if paths.is_empty() {
        return None;
    }

    let mut result: HashMap<String, PerFileStats> = HashMap::new();
    for path in paths.iter().take(max_files) {
        result.insert(
            path.to_string(),
            PerFileStats {
                added: 0,
                removed: 0,
                is_binary: false,
                is_untracked: true,
            },
        );
    }

    Some(result)
}

/// Fetch a structured diff for a single file against the merge base.
pub async fn fetch_single_file_git_diff(absolute_file_path: &Path) -> Option<ToolUseDiff> {
    let parent = absolute_file_path.parent()?;

    // Find git root
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(parent)
        .output()
        .await
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let git_root = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    let git_path = absolute_file_path
        .strip_prefix(&git_root)
        .ok()?
        .to_string_lossy()
        .replace('\\', "/");

    // Check if tracked
    let ls_output = Command::new("git")
        .args([
            "--no-optional-locks",
            "ls-files",
            "--error-unmatch",
            &git_path,
        ])
        .current_dir(&git_root)
        .output()
        .await
        .ok()?;

    if ls_output.status.success() {
        // File is tracked — diff against merge base
        let diff_ref = get_diff_ref(&git_root).await;
        let diff_output = Command::new("git")
            .args(["--no-optional-locks", "diff", &diff_ref, "--", &git_path])
            .current_dir(&git_root)
            .output()
            .await
            .ok()?;

        if !diff_output.status.success() {
            return None;
        }
        let stdout = String::from_utf8_lossy(&diff_output.stdout);
        if stdout.is_empty() {
            return None;
        }
        Some(parse_raw_diff_to_tool_use_diff(
            &git_path,
            &stdout,
            DiffStatus::Modified,
        ))
    } else {
        // File is untracked — generate synthetic diff
        generate_synthetic_diff(&git_path, absolute_file_path).await
    }
}

fn parse_raw_diff_to_tool_use_diff(
    filename: &str,
    raw_diff: &str,
    status: DiffStatus,
) -> ToolUseDiff {
    let mut patch_lines: Vec<&str> = Vec::new();
    let mut in_hunks = false;
    let mut additions: usize = 0;
    let mut deletions: usize = 0;

    for line in raw_diff.lines() {
        if line.starts_with("@@") {
            in_hunks = true;
        }
        if in_hunks {
            patch_lines.push(line);
            if line.starts_with('+') && !line.starts_with("+++") {
                additions += 1;
            } else if line.starts_with('-') && !line.starts_with("---") {
                deletions += 1;
            }
        }
    }

    ToolUseDiff {
        filename: filename.to_string(),
        status,
        additions,
        deletions,
        changes: additions + deletions,
        patch: patch_lines.join("\n"),
        repository: None,
    }
}

async fn get_diff_ref(git_root: &Path) -> String {
    let base_branch = std::env::var("MOSSEN_CODE_BASE_REF").unwrap_or_else(|_| "main".to_string());

    let output = Command::new("git")
        .args(["--no-optional-locks", "merge-base", "HEAD", &base_branch])
        .current_dir(git_root)
        .output()
        .await;

    match output {
        Ok(o) if o.status.success() => {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if s.is_empty() {
                "HEAD".to_string()
            } else {
                s
            }
        }
        _ => "HEAD".to_string(),
    }
}

async fn generate_synthetic_diff(git_path: &str, absolute_file_path: &Path) -> Option<ToolUseDiff> {
    let metadata = fs::metadata(absolute_file_path).await.ok()?;
    if metadata.len() > MAX_DIFF_SIZE_BYTES as u64 {
        return None;
    }

    let content = fs::read_to_string(absolute_file_path).await.ok()?;
    let mut lines: Vec<&str> = content.split('\n').collect();
    // Remove trailing empty line if file ends with newline
    if lines.last() == Some(&"") {
        lines.pop();
    }

    let line_count = lines.len();
    let added_lines: String = lines
        .iter()
        .map(|l| format!("+{}", l))
        .collect::<Vec<_>>()
        .join("\n");
    let patch = format!("@@ -0,0 +1,{} @@\n{}", line_count, added_lines);

    Some(ToolUseDiff {
        filename: git_path.to_string(),
        status: DiffStatus::Added,
        additions: line_count,
        deletions: 0,
        changes: line_count,
        patch,
        repository: None,
    })
}
