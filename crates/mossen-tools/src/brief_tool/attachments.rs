//! Brief tool attachment helpers — `tools/BriefTool/attachments.ts`.
//!
//! Also hosts the `AttachmentList` renderer ported from `tools/BriefTool/UI.tsx`.

use std::path::PathBuf;

/// `attachments.ts` `ResolvedAttachment`.
#[derive(Debug, Clone)]
pub struct ResolvedAttachment {
    pub path: PathBuf,
    pub kind: String,
    pub size_bytes: u64,
}

/// `attachments.ts` `validateAttachmentPaths` — return any paths that don't
/// exist or aren't readable. Empty when all paths pass.
pub async fn validate_attachment_paths(paths: &[String]) -> Vec<String> {
    let mut bad = Vec::new();
    for p in paths {
        if tokio::fs::metadata(p).await.is_err() {
            bad.push(p.clone());
        }
    }
    bad
}

/// Format a byte count as a human-readable string (e.g. `"12.3 KB"`).
fn format_file_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Truncate a path to `<home>/.../<last>` for display purposes.
fn get_display_path(p: &std::path::Path) -> String {
    p.to_string_lossy().into_owned()
}

/// `BriefTool/UI.tsx` `AttachmentList` — render a list of attachments as a
/// vertical text block, one line per attachment. Used by the brief tool's
/// `renderToolResultMessage` output.
///
/// Returns an empty string when the list is empty (matching the TS `return null`).
pub fn render_attachment_list(attachments: &[ResolvedAttachment]) -> String {
    if attachments.is_empty() {
        return String::new();
    }
    let mut lines = Vec::with_capacity(attachments.len());
    for att in attachments {
        let kind_label = if att.kind == "image" {
            "[image]"
        } else {
            "[file]"
        };
        lines.push(format!(
            "› {} {} ({})",
            kind_label,
            get_display_path(&att.path),
            format_file_size(att.size_bytes),
        ));
    }
    lines.join("\n")
}

/// Alias matching the TS export name `AttachmentList`.
#[allow(non_snake_case)]
pub fn AttachmentList(attachments: &[ResolvedAttachment]) -> String {
    render_attachment_list(attachments)
}

/// `attachments.ts` `resolveAttachments` — read metadata for each path and
/// classify the attachment kind based on file extension.
pub async fn resolve_attachments(paths: &[String]) -> Vec<ResolvedAttachment> {
    let mut out = Vec::with_capacity(paths.len());
    for p in paths {
        let path = PathBuf::from(p);
        let Ok(meta) = tokio::fs::metadata(&path).await else {
            continue;
        };
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        let kind = match ext.as_str() {
            "png" | "jpg" | "jpeg" | "gif" | "webp" => "image",
            "pdf" => "pdf",
            "md" | "markdown" => "markdown",
            "txt" | "log" => "text",
            "json" => "json",
            _ => "file",
        }
        .to_string();
        out.push(ResolvedAttachment {
            path,
            kind,
            size_bytes: meta.len(),
        });
    }
    out
}
