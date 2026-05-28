//! Magic docs generation service

use std::path::{Path, PathBuf};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

use super::prompts::build_magic_docs_prompt;

/// Result of documentation generation
#[derive(Debug, Clone)]
pub struct MagicDocsResult {
    pub file_path: String,
    pub documentation: String,
    pub tokens_used: u64,
}

/// Generate documentation for a single file
pub async fn generate_docs_for_file(
    file_path: &Path,
    context: Option<&str>,
    cancel_token: CancellationToken,
) -> Result<MagicDocsResult, String> {
    let content = tokio::fs::read_to_string(file_path)
        .await
        .map_err(|e| format!("Failed to read file: {}", e))?;

    let path_str = file_path.to_string_lossy().to_string();
    let prompt = build_magic_docs_prompt(&path_str, &content, context);

    debug!(
        "Generating docs for: {} (prompt len: {})",
        path_str,
        prompt.len()
    );

    // In full implementation: run forked agent with the prompt
    tokio::select! {
        _ = cancel_token.cancelled() => {
            Err("Cancelled".to_string())
        }
        result = async {
            // Simulate doc generation
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            Ok(MagicDocsResult {
                file_path: path_str,
                documentation: String::new(),
                tokens_used: 0,
            })
        } => {
            result
        }
    }
}

/// Generate documentation for multiple files in a directory
pub async fn generate_docs_for_directory(
    dir_path: &Path,
    extensions: &[&str],
    cancel_token: CancellationToken,
) -> Result<Vec<MagicDocsResult>, String> {
    let mut results = Vec::new();

    let mut entries = tokio::fs::read_dir(dir_path)
        .await
        .map_err(|e| format!("Failed to read directory: {}", e))?;

    while let Ok(Some(entry)) = entries.next_entry().await {
        if cancel_token.is_cancelled() {
            break;
        }

        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !extensions.contains(&ext) {
            continue;
        }

        match generate_docs_for_file(&path, None, cancel_token.clone()).await {
            Ok(result) => results.push(result),
            Err(e) => {
                debug!("Skipping {}: {}", path.display(), e);
            }
        }
    }

    info!("Generated docs for {} files", results.len());
    Ok(results)
}

// ---------------------------------------------------------------------------
// TS-mirror — `services/MagicDocs/magicDocs.ts` exports.
// ---------------------------------------------------------------------------

use once_cell::sync::Lazy;
use std::sync::Mutex;

static TRACKED_MAGIC_DOCS: Lazy<Mutex<std::collections::HashSet<String>>> =
    Lazy::new(|| Mutex::new(std::collections::HashSet::new()));

/// `magicDocs.ts` `clearTrackedMagicDocs`.
pub fn clear_tracked_magic_docs() {
    TRACKED_MAGIC_DOCS.lock().unwrap().clear();
}

/// `magicDocs.ts` `detectMagicDocHeader`.
pub fn detect_magic_doc_header(content: &str) -> Option<String> {
    let trimmed = content.trim_start();
    let prefix = "<!-- @mossen-magic-doc";
    if !trimmed.starts_with(prefix) {
        return None;
    }
    let rest = &trimmed[prefix.len()..];
    let end = rest.find("-->")?;
    Some(rest[..end].trim().to_string())
}

/// `magicDocs.ts` `registerMagicDoc`.
pub fn register_magic_doc(file_path: &str) {
    TRACKED_MAGIC_DOCS
        .lock()
        .unwrap()
        .insert(file_path.to_string());
}

/// `magicDocs.ts` `initMagicDocs`.
pub async fn init_magic_docs() {
    clear_tracked_magic_docs();
}
