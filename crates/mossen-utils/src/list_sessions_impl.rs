//! List sessions implementation — portable session enumeration.
//!
//! Translated from utils/listSessionsImpl.ts

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio::fs;

use crate::string_utils::{safe_prefix_by_bytes, safe_suffix_by_bytes};

/// Session metadata returned by listSessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub session_id: String,
    pub summary: String,
    pub last_modified: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// Epoch ms — from first entry's ISO timestamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<u64>,
}

/// Options for listing sessions.
#[derive(Debug, Clone, Default)]
pub struct ListSessionsOptions {
    /// Directory to list sessions for.
    pub dir: Option<String>,
    /// Maximum number of sessions to return.
    pub limit: Option<usize>,
    /// Number of sessions to skip from the start.
    pub offset: Option<usize>,
    /// When dir is provided, include sessions from all git worktree paths.
    pub include_worktrees: Option<bool>,
}

/// Internal candidate for session enumeration.
#[derive(Debug, Clone)]
struct Candidate {
    session_id: String,
    file_path: PathBuf,
    mtime: u64,
    project_path: Option<String>,
}

const READ_BATCH_SIZE: usize = 32;

/// Validate a string as a UUID (simplified check).
fn validate_uuid(s: &str) -> Option<String> {
    // UUID format: xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx (36 chars)
    if s.len() == 36
        && s.chars().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_hexdigit(),
        })
    {
        Some(s.to_string())
    } else {
        None
    }
}

/// Extract a JSON string field from text using simple pattern matching.
fn extract_json_string_field(text: &str, field: &str) -> Option<String> {
    let pattern = format!("\"{}\":\"", field);
    if let Some(start) = text.find(&pattern) {
        let value_start = start + pattern.len();
        let remaining = &text[value_start..];
        if let Some(end) = remaining.find('"') {
            return Some(remaining[..end].to_string());
        }
    }
    // Try with space after colon
    let pattern2 = format!("\"{}\": \"", field);
    if let Some(start) = text.find(&pattern2) {
        let value_start = start + pattern2.len();
        let remaining = &text[value_start..];
        if let Some(end) = remaining.find('"') {
            return Some(remaining[..end].to_string());
        }
    }
    None
}

/// Extract the last occurrence of a JSON string field.
fn extract_last_json_string_field(text: &str, field: &str) -> Option<String> {
    let pattern = format!("\"{}\":\"", field);
    let pattern2 = format!("\"{}\": \"", field);

    let mut last_val = None;
    let mut search_from = 0;

    loop {
        let found = text[search_from..]
            .find(&pattern)
            .map(|i| (i + search_from, pattern.len()));
        let found2 = text[search_from..]
            .find(&pattern2)
            .map(|i| (i + search_from, pattern2.len()));

        let (pos, pat_len) = match (found, found2) {
            (Some((a, al)), Some((b, bl))) => {
                if a <= b {
                    (a, al)
                } else {
                    (b, bl)
                }
            }
            (Some(a), None) => a,
            (None, Some(b)) => b,
            (None, None) => break,
        };

        let value_start = pos + pat_len;
        if let Some(end) = text[value_start..].find('"') {
            last_val = Some(text[value_start..value_start + end].to_string());
        }
        search_from = pos + 1;
    }

    last_val
}

/// Parse SessionInfo fields from a lite session read (head/tail/stat).
pub fn parse_session_info_from_lite(
    session_id: &str,
    head: &str,
    tail: &str,
    mtime: u64,
    size: u64,
    project_path: Option<&str>,
) -> Option<SessionInfo> {
    // Check first line for sidechain sessions
    let first_line = head.lines().next().unwrap_or("");
    if first_line.contains("\"isSidechain\":true") || first_line.contains("\"isSidechain\": true") {
        return None;
    }

    // User title wins over AI title
    let custom_title = extract_last_json_string_field(tail, "customTitle")
        .or_else(|| extract_last_json_string_field(head, "customTitle"))
        .or_else(|| extract_last_json_string_field(tail, "aiTitle"))
        .or_else(|| extract_last_json_string_field(head, "aiTitle"));

    let first_prompt = extract_json_string_field(head, "firstPrompt");

    // First entry's ISO timestamp → epoch ms
    let first_timestamp = extract_json_string_field(head, "timestamp");
    let created_at = first_timestamp.and_then(|ts| {
        chrono::DateTime::parse_from_rfc3339(&ts)
            .ok()
            .map(|dt| dt.timestamp_millis() as u64)
    });

    let summary = custom_title
        .clone()
        .or_else(|| extract_last_json_string_field(tail, "lastPrompt"))
        .or_else(|| extract_last_json_string_field(tail, "summary"))
        .or_else(|| first_prompt.clone());

    // Skip metadata-only sessions
    let summary = summary?;

    let git_branch = extract_last_json_string_field(tail, "gitBranch")
        .or_else(|| extract_json_string_field(head, "gitBranch"));

    let session_cwd =
        extract_json_string_field(head, "cwd").or_else(|| project_path.map(|p| p.to_string()));

    // Tag extraction scoped to {"type":"tag"} lines
    let tag = tail
        .lines()
        .rev()
        .find(|l| l.starts_with("{\"type\":\"tag\""))
        .and_then(|line| extract_last_json_string_field(line, "tag"));

    Some(SessionInfo {
        session_id: session_id.to_string(),
        summary,
        last_modified: mtime,
        file_size: Some(size),
        custom_title,
        first_prompt,
        git_branch,
        cwd: session_cwd,
        tag,
        created_at,
    })
}

/// Lists candidate session files in a directory.
pub async fn list_candidates(
    project_dir: &Path,
    do_stat: bool,
    project_path: Option<&str>,
) -> Vec<Candidate> {
    let entries = match fs::read_dir(project_dir).await {
        Ok(mut e) => {
            let mut names = Vec::new();
            while let Ok(Some(entry)) = e.next_entry().await {
                if let Ok(name) = entry.file_name().into_string() {
                    names.push((name, entry.path()));
                }
            }
            names
        }
        Err(_) => return Vec::new(),
    };

    let mut results = Vec::new();
    for (name, path) in entries {
        if !name.ends_with(".jsonl") {
            continue;
        }
        let session_id = match validate_uuid(&name[..name.len() - 6]) {
            Some(id) => id,
            None => continue,
        };

        if !do_stat {
            results.push(Candidate {
                session_id,
                file_path: path,
                mtime: 0,
                project_path: project_path.map(|s| s.to_string()),
            });
            continue;
        }

        match fs::metadata(&path).await {
            Ok(meta) => {
                let mtime = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0);
                results.push(Candidate {
                    session_id,
                    file_path: path,
                    mtime,
                    project_path: project_path.map(|s| s.to_string()),
                });
            }
            Err(_) => continue,
        }
    }

    results
}

/// Read a candidate's file contents and extract full SessionInfo.
async fn read_candidate(c: &Candidate) -> Option<SessionInfo> {
    let content = fs::read_to_string(&c.file_path).await.ok()?;
    let size = content.len() as u64;

    // Read head (first 4KB) and tail (last 4KB)
    let head = if content.len() > 4096 {
        safe_prefix_by_bytes(&content, 4096)
    } else {
        &content
    };
    let tail = if content.len() > 4096 {
        safe_suffix_by_bytes(&content, 4096)
    } else {
        &content
    };

    let mtime = if c.mtime > 0 {
        c.mtime
    } else {
        fs::metadata(&c.file_path)
            .await
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    };

    let mut info = parse_session_info_from_lite(
        &c.session_id,
        head,
        tail,
        mtime,
        size,
        c.project_path.as_deref(),
    )?;

    if c.mtime > 0 {
        info.last_modified = c.mtime;
    }

    Some(info)
}

/// Sort comparator: lastModified desc, then sessionId desc.
fn compare_desc(a: &Candidate, b: &Candidate) -> std::cmp::Ordering {
    b.mtime
        .cmp(&a.mtime)
        .then_with(|| b.session_id.cmp(&a.session_id))
}

/// Apply sort and limit to candidates.
async fn apply_sort_and_limit(
    mut candidates: Vec<Candidate>,
    limit: Option<usize>,
    offset: usize,
) -> Vec<SessionInfo> {
    candidates.sort_by(compare_desc);

    let mut sessions = Vec::new();
    let want = limit.unwrap_or(usize::MAX);
    let mut skipped = 0;
    let mut seen = HashSet::new();

    let mut i = 0;
    while i < candidates.len() && sessions.len() < want {
        let batch_end = (i + READ_BATCH_SIZE).min(candidates.len());
        let batch = &candidates[i..batch_end];

        let mut results = Vec::new();
        for c in batch {
            results.push(read_candidate(c).await);
        }

        for r in results {
            i += 1;
            if sessions.len() >= want {
                break;
            }
            let info = match r {
                Some(info) => info,
                None => continue,
            };
            if seen.contains(&info.session_id) {
                continue;
            }
            seen.insert(info.session_id.clone());
            if skipped < offset {
                skipped += 1;
                continue;
            }
            sessions.push(info);
        }
    }

    sessions
}

/// Read-all path: reads every candidate, then sorts/dedups.
async fn read_all_and_sort(candidates: Vec<Candidate>) -> Vec<SessionInfo> {
    let mut by_id: HashMap<String, SessionInfo> = HashMap::new();

    for c in &candidates {
        if let Some(info) = read_candidate(c).await {
            let existing = by_id.get(&info.session_id);
            if existing.map_or(true, |e| info.last_modified > e.last_modified) {
                by_id.insert(info.session_id.clone(), info);
            }
        }
    }

    let mut sessions: Vec<SessionInfo> = by_id.into_values().collect();
    sessions.sort_by(|a, b| {
        b.last_modified
            .cmp(&a.last_modified)
            .then_with(|| b.session_id.cmp(&a.session_id))
    });
    sessions
}

/// Gathers candidate session files for a specific project directory.
async fn gather_project_candidates(
    dir: &str,
    _include_worktrees: bool,
    do_stat: bool,
    projects_dir: &str,
) -> Vec<Candidate> {
    // Simplified: just scan the single project dir
    let sanitized = sanitize_path(dir);
    let project_dir = PathBuf::from(projects_dir).join(&sanitized);
    if project_dir.exists() {
        list_candidates(&project_dir, do_stat, Some(dir)).await
    } else {
        Vec::new()
    }
}

/// Gathers candidate session files across all project directories.
async fn gather_all_candidates(do_stat: bool, projects_dir: &str) -> Vec<Candidate> {
    let dir_path = Path::new(projects_dir);
    let entries = match fs::read_dir(dir_path).await {
        Ok(mut e) => {
            let mut dirs = Vec::new();
            while let Ok(Some(entry)) = e.next_entry().await {
                if let Ok(ft) = entry.file_type().await {
                    if ft.is_dir() {
                        dirs.push(entry.path());
                    }
                }
            }
            dirs
        }
        Err(_) => return Vec::new(),
    };

    let mut all = Vec::new();
    for dir in entries {
        let mut candidates = list_candidates(&dir, do_stat, None).await;
        all.append(&mut candidates);
    }
    all
}

/// Sanitize a path for use as a directory name.
fn sanitize_path(path: &str) -> String {
    path.chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect()
}

/// Lists sessions with metadata extracted from stat + head/tail reads.
pub async fn list_sessions_impl(
    options: Option<ListSessionsOptions>,
    projects_dir: &str,
) -> Vec<SessionInfo> {
    let opts = options.unwrap_or_default();
    let offset = opts.offset.unwrap_or(0);
    let do_stat = (opts.limit.is_some() && opts.limit.unwrap_or(0) > 0) || offset > 0;

    let candidates = if let Some(ref dir) = opts.dir {
        gather_project_candidates(
            dir,
            opts.include_worktrees.unwrap_or(true),
            do_stat,
            projects_dir,
        )
        .await
    } else {
        gather_all_candidates(do_stat, projects_dir).await
    };

    if !do_stat {
        read_all_and_sort(candidates).await
    } else {
        apply_sort_and_limit(candidates, opts.limit, offset).await
    }
}
