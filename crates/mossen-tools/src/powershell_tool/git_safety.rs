//! # git_safety — Git safety checks for PowerShell
//!
//! Translates `tools/PowerShellTool/gitSafety.ts`.
//! Detects git-internal path writes that could be weaponized for sandbox escape.

use std::env;
use std::path::{Path, PathBuf, MAIN_SEPARATOR};

/// PowerShell tokenizer dash characters (en-dash, em-dash, horizontal bar).
const PS_TOKENIZER_DASH_CHARS: &[char] = &['\u{2013}', '\u{2014}', '\u{2015}', '-'];

/// Git-internal directory prefixes.
const GIT_INTERNAL_PREFIXES: &[&str] = &["head", "objects", "refs", "hooks"];

/// Get the current working directory.
fn get_cwd() -> PathBuf {
    env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// If a normalized path starts with `../<cwd-basename>/`, it re-enters cwd
/// via the parent — resolve it to the cwd-relative form.
fn resolve_cwd_reentry(normalized: &str) -> String {
    if !normalized.starts_with("../") {
        return normalized.to_string();
    }

    let cwd = get_cwd();
    let cwd_base = cwd
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    if cwd_base.is_empty() {
        return normalized.to_string();
    }

    let prefix = format!("../{}/", cwd_base);
    let mut s = normalized.to_string();
    while s.starts_with(&prefix) {
        s = s[prefix.len()..].to_string();
    }

    let exact = format!("../{}", cwd_base);
    if s == exact {
        return ".".to_string();
    }

    s
}

/// Normalize PS arg text → canonical path for git-internal matching.
fn normalize_git_path_arg(arg: &str) -> String {
    let mut s = arg.to_string();

    // Strip parameter prefix (dash chars and forward-slash)
    if !s.is_empty() {
        let first_char = s.chars().next().unwrap();
        if PS_TOKENIZER_DASH_CHARS.contains(&first_char) || first_char == '/' {
            if let Some(colon_pos) = s[1..].find(':') {
                s = s[colon_pos + 2..].to_string();
            }
        }
    }

    // Strip surrounding quotes
    if (s.starts_with('\'') && s.ends_with('\'')) || (s.starts_with('"') && s.ends_with('"')) {
        s = s[1..s.len() - 1].to_string();
    }

    // Strip backtick escapes
    s = s.replace('`', "");

    // Strip PS provider-qualified path: FileSystem::path → path
    if let Some(pos) = s.find("FileSystem::") {
        s = s[pos + "FileSystem::".len()..].to_string();
    }

    // Drive-relative: C:foo (no sep after colon) → foo
    if s.len() >= 2
        && s.as_bytes()[0].is_ascii_alphabetic()
        && s.as_bytes()[1] == b':'
        && (s.len() == 2 || (s.as_bytes()[2] != b'/' && s.as_bytes()[2] != b'\\'))
    {
        s = s[2..].to_string();
    }

    // Convert backslashes to forward slashes
    s = s.replace('\\', "/");

    // NTFS per-component: strip trailing spaces and dots
    s = s
        .split('/')
        .map(|c| {
            if c.is_empty() {
                return c.to_string();
            }
            let mut component = c.to_string();
            loop {
                let prev = component.clone();
                component = component.trim_end().to_string();
                if component == "." || component == ".." {
                    return component;
                }
                component = component.trim_end_matches('.').to_string();
                if component == prev {
                    break;
                }
            }
            if component.is_empty() {
                ".".to_string()
            } else {
                component
            }
        })
        .collect::<Vec<_>>()
        .join("/");

    // Normalize path (resolve .., ., //)
    s = normalize_posix_path(&s);

    // Strip leading ./
    if s.starts_with("./") {
        s = s[2..].to_string();
    }

    s.to_lowercase()
}

/// Simple POSIX path normalization.
fn normalize_posix_path(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    let is_absolute = path.starts_with('/');

    for part in path.split('/') {
        match part {
            "" | "." => continue,
            ".." => {
                if !parts.is_empty() && *parts.last().unwrap() != ".." {
                    parts.pop();
                } else if !is_absolute {
                    parts.push("..");
                }
            }
            _ => parts.push(part),
        }
    }

    let result = parts.join("/");
    if is_absolute {
        format!("/{}", result)
    } else if result.is_empty() {
        ".".to_string()
    } else {
        result
    }
}

/// Check if a normalized path matches git-internal prefixes.
fn matches_git_internal_prefix(n: &str) -> bool {
    if n == "head" || n == ".git" {
        return true;
    }
    if n.starts_with(".git/") {
        return true;
    }
    // git~N pattern (NTFS short names)
    if n.starts_with("git~") && n[4..].starts_with(|c: char| c.is_ascii_digit()) {
        return true;
    }

    for prefix in GIT_INTERNAL_PREFIXES {
        if *prefix == "head" {
            continue;
        }
        if n == *prefix || n.starts_with(&format!("{}/", prefix)) {
            return true;
        }
    }
    false
}

/// Check if a normalized path starts with .git/ (standard-repo metadata dir).
fn matches_dot_git_prefix(n: &str) -> bool {
    if n == ".git" || n.starts_with(".git/") {
        return true;
    }
    // NTFS 8.3 short names
    if n.starts_with("git~") && n[4..].starts_with(|c: char| c.is_ascii_digit()) {
        return true;
    }
    false
}

/// Resolve an escaping path against actual cwd, return cwd-relative remainder if inside.
fn resolve_escaping_path_to_cwd_relative(n: &str) -> Option<String> {
    let cwd = get_cwd();
    let cwd_str = cwd.to_string_lossy().to_lowercase();

    // Reconstruct platform path from posix-normalized form
    let platform_path = n.replace('/', std::path::MAIN_SEPARATOR_STR);
    let abs = cwd.join(&platform_path);
    let abs_str = abs.to_string_lossy().to_lowercase();

    if abs_str == cwd_str {
        return Some(".".to_string());
    }

    let cwd_with_sep = format!("{}{}", cwd_str, MAIN_SEPARATOR);
    if abs_str.starts_with(&cwd_with_sep) {
        let relative = &abs.to_string_lossy()[cwd.to_string_lossy().len() + 1..];
        return Some(relative.replace('\\', "/").to_lowercase());
    }

    None
}

/// True if arg (raw PS arg text) resolves to a git-internal path in cwd.
/// Covers both bare-repo paths (hooks/, refs/) and standard-repo paths (.git/hooks/).
pub fn is_git_internal_path_ps(arg: &str) -> bool {
    let n = resolve_cwd_reentry(&normalize_git_path_arg(arg));

    if matches_git_internal_prefix(&n) {
        return true;
    }

    // Leading ../ or absolute paths that couldn't be fully resolved
    if n.starts_with("../") || n.starts_with('/') || (n.len() >= 2 && n.as_bytes()[1] == b':') {
        if let Some(rel) = resolve_escaping_path_to_cwd_relative(&n) {
            if matches_git_internal_prefix(&rel) {
                return true;
            }
        }
    }

    false
}

/// True if arg resolves to a path inside .git/ (standard-repo metadata dir).
pub fn is_dot_git_path_ps(arg: &str) -> bool {
    let n = resolve_cwd_reentry(&normalize_git_path_arg(arg));

    if matches_dot_git_prefix(&n) {
        return true;
    }

    if n.starts_with("../") || n.starts_with('/') || (n.len() >= 2 && n.as_bytes()[1] == b':') {
        if let Some(rel) = resolve_escaping_path_to_cwd_relative(&n) {
            if matches_dot_git_prefix(&rel) {
                return true;
            }
        }
    }

    false
}
