use once_cell::sync::Lazy;
use regex::Regex;
use std::path::{Path, PathBuf};
use tokio::fs;
use tracing::debug;

static SKILL_MD_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)^skill\.md$").unwrap());

#[derive(Default)]
pub struct WalkOptions {
    pub stop_at_skill_dir: bool,
    pub log_label: Option<String>,
}

/// Recursively walk a plugin directory, invoking on_file for each .md file.
///
/// The namespace vec tracks the subdirectory path relative to the root.
/// When stop_at_skill_dir is true and a directory contains SKILL.md, on_file is
/// called for all .md files in that directory but subdirectories are not scanned.
pub async fn walk_plugin_markdown<F, Fut>(root_dir: &Path, on_file: &F, opts: WalkOptions) -> ()
where
    F: Fn(PathBuf, Vec<String>) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let label = opts.log_label.unwrap_or_else(|| "plugin".to_string());
    scan(root_dir, vec![], &label, opts.stop_at_skill_dir, on_file).await;
}

async fn scan<F, Fut>(
    dir_path: &Path,
    namespace: Vec<String>,
    label: &str,
    stop_at_skill_dir: bool,
    on_file: &F,
) where
    F: Fn(PathBuf, Vec<String>) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let entries = match fs::read_dir(dir_path).await {
        Ok(mut rd) => {
            let mut entries = Vec::new();
            loop {
                match rd.next_entry().await {
                    Ok(Some(entry)) => entries.push(entry),
                    Ok(None) => break,
                    Err(_) => break,
                }
            }
            entries
        }
        Err(error) => {
            debug!(
                "Failed to scan {} directory {}: {}",
                label,
                dir_path.display(),
                error
            );
            return;
        }
    };

    if stop_at_skill_dir {
        let mut has_skill_md = false;
        for e in entries.iter() {
            if let Ok(ft) = e.file_type().await {
                if ft.is_file() {
                    if let Some(name) = e.file_name().to_str() {
                        if SKILL_MD_RE.is_match(name) {
                            has_skill_md = true;
                            break;
                        }
                    }
                }
            }
        }

        if has_skill_md {
            // Skill directory: collect .md files here, don't recurse.
            for entry in &entries {
                if let Ok(ft) = entry.file_type().await {
                    if ft.is_file() {
                        if let Some(name) = entry.file_name().to_str() {
                            if name.to_lowercase().ends_with(".md") {
                                let full_path = dir_path.join(name);
                                on_file(full_path, namespace.clone()).await;
                            }
                        }
                    }
                }
            }
            return;
        }
    }

    for entry in entries {
        let file_name = match entry.file_name().into_string() {
            Ok(n) => n,
            Err(_) => continue,
        };
        let full_path = dir_path.join(&file_name);

        if let Ok(ft) = entry.file_type().await {
            if ft.is_dir() {
                let mut child_ns = namespace.clone();
                child_ns.push(file_name);
                Box::pin(scan(
                    &full_path,
                    child_ns,
                    label,
                    stop_at_skill_dir,
                    on_file,
                ))
                .await;
            } else if ft.is_file() && file_name.to_lowercase().ends_with(".md") {
                on_file(full_path, namespace.clone()).await;
            }
        }
    }
}
