//! Voice keyterms for improving STT accuracy

use std::collections::HashSet;
use std::path::Path;

/// Global keyterms for STT
const GLOBAL_KEYTERMS: &[&str] = &[
    "MCP",
    "symlink",
    "grep",
    "regex",
    "localhost",
    "codebase",
    "TypeScript",
    "JSON",
    "webhook",
    "gRPC",
    "dotfiles",
    "subagent",
    "worktree",
];

const MAX_KEYTERMS: usize = 50;

/// Split an identifier into words
pub fn split_identifier(name: &str) -> Vec<String> {
    // Split on camelCase boundaries
    let mut result = String::new();
    let chars: Vec<char> = name.chars().collect();
    for i in 0..chars.len() {
        if i > 0 && chars[i].is_uppercase() && chars[i - 1].is_lowercase() {
            result.push(' ');
        }
        result.push(chars[i]);
    }

    // Split on separators
    result
        .split(|c: char| c == '-' || c == '_' || c == '.' || c == '/' || c.is_whitespace())
        .map(|w| w.trim().to_string())
        .filter(|w| w.len() > 2 && w.len() <= 20)
        .collect()
}

fn file_name_words(file_path: &str) -> Vec<String> {
    let path = Path::new(file_path);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    split_identifier(stem)
}

/// Build a list of keyterms for the voice_stream STT endpoint
pub async fn get_voice_keyterms(
    project_root: Option<&str>,
    git_branch: Option<&str>,
    recent_files: Option<&HashSet<String>>,
) -> Vec<String> {
    let mut terms: HashSet<String> = GLOBAL_KEYTERMS.iter().map(|s| s.to_string()).collect();

    // Project root basename
    if let Some(root) = project_root {
        let name = Path::new(root)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        if name.len() > 2 && name.len() <= 50 {
            terms.insert(name.to_string());
        }
    }

    // Git branch words
    if let Some(branch) = git_branch {
        for word in split_identifier(branch) {
            terms.insert(word);
        }
    }

    // Recent file names
    if let Some(files) = recent_files {
        for file_path in files {
            if terms.len() >= MAX_KEYTERMS {
                break;
            }
            for word in file_name_words(file_path) {
                terms.insert(word);
            }
        }
    }

    terms.into_iter().take(MAX_KEYTERMS).collect()
}
