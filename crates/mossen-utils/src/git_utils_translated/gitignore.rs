//! Gitignore utilities — translated from utils/git/gitignore.ts

use std::path::Path;
use anyhow::Result;
use tokio::fs;

/// Checks if a path is ignored by git (via `git check-ignore`).
///
/// This consults all applicable gitignore sources: repo `.gitignore` files
/// (nested), `.git/info/exclude`, and the global gitignore — with correct
/// precedence, because git itself resolves it.
///
/// Exit codes: 0 = ignored, 1 = not ignored, 128 = not in a git repo.
/// Returns `false` for 128, so callers outside a git repo fail open.
pub async fn is_path_gitignored(file_path: &str, cwd: &str) -> Result<bool> {
    let output = tokio::process::Command::new("git")
        .args(["check-ignore", file_path])
        .current_dir(cwd)
        .output()
        .await?;

    Ok(output.status.code() == Some(0))
}

/// Gets the path to the global gitignore file (.config/git/ignore)
pub fn get_global_gitignore_path() -> String {
    dirs::home_dir()
        .map(|h| h.join(".config").join("git").join("ignore"))
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "~/.config/git/ignore".to_string())
}

/// Adds a file pattern to the global gitignore file (.config/git/ignore)
/// if it's not already ignored by existing patterns in any gitignore file
pub async fn add_file_glob_rule_to_gitignore(
    filename: &str,
    cwd: Option<&str>,
) -> Result<()> {
    let cwd = cwd.unwrap_or_else(|| ".");

    // First check if we're in a git repo
    let in_repo = crate::git::dir_is_in_git_repo(cwd).await?;
    if !in_repo {
        return Ok(());
    }

    // First check if the pattern is already ignored by any gitignore file (including global)
    let gitignore_entry = format!("**/{}", filename);
    // For directory patterns (ending with /), check with a sample file inside
    let test_path = if filename.ends_with('/') {
        format!("{}sample-file.txt", filename)
    } else {
        filename.to_string()
    };

    if is_path_gitignored(&test_path, cwd).await? {
        // File is already ignored by existing patterns (local or global)
        return Ok(());
    }

    // Use the global gitignore file in .config/git/ignore
    let global_gitignore_path = Path::new(&get_global_gitignore_path());

    // Create the directory if it doesn't exist
    if let Some(parent) = global_gitignore_path.parent() {
        fs::create_dir_all(parent).await?;
    }

    // Add the entry to the global gitignore
    match fs::read_to_string(global_gitignore_path).await {
        Ok(content) => {
            if content.contains(&gitignore_entry) {
                // Pattern already exists, don't add again
                return Ok(());
            }
            fs::write(global_gitignore_path, format!("\n{}\n", gitignore_entry)).await?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Create global gitignore with entry
            fs::write(global_gitignore_path, format!("{}\n", gitignore_entry)).await?;
        }
        Err(e) => return Err(e.into()),
    }

    Ok(())
}
