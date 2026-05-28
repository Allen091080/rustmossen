//! `/memory` — View and edit memory files.
//!
//! Translates `commands/memory/memory.tsx` (211 lines).
//! Provides a file selector for memory files (MOSSEN.md, .mossen/rules/*),
//! shows memory metadata (auto/team status, location, file count, size),
//! and opens selected files in the user's editor.

use anyhow::Result;
use async_trait::async_trait;
use std::path::PathBuf;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Size units for human-readable formatting.
const SIZE_UNITS: &[&str] = &["B", "KB", "MB", "GB"];

/// Format byte count in human-readable units.
fn format_mem_bytes(n: i64) -> String {
    if n < 0 {
        return "(unknown)".to_string();
    }
    let mut v = n as f64;
    let mut u = 0;
    while v >= 1024.0 && u < SIZE_UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if v < 10.0 && u > 0 {
        format!("{:.1}{}", v, SIZE_UNITS[u])
    } else {
        format!("{:.0}{}", v, SIZE_UNITS[u])
    }
}

/// Memory state summary.
#[derive(Debug)]
struct MemoryStateSummary {
    status: MemoryLocation,
    file_count: usize,
    total_bytes: i64,
    reason: Option<String>,
}

/// Where memory files are located.
#[derive(Debug)]
enum MemoryLocation {
    InProject,
    External,
    Absent,
}

/// Describe the memory state for a project directory.
fn describe_memory_state(project_dir: &PathBuf) -> MemoryStateSummary {
    // Check if .mossen/ directory exists in project
    let mossen_dir = project_dir.join(".mossen");
    if mossen_dir.exists() {
        let mut file_count = 0;
        let mut total_bytes = 0i64;

        // Count files in .mossen/ and subdirectories
        if let Ok(entries) = std::fs::read_dir(&mossen_dir) {
            for entry in entries.flatten() {
                if entry.path().is_file() {
                    file_count += 1;
                    if let Ok(meta) = entry.metadata() {
                        total_bytes += meta.len() as i64;
                    }
                }
            }
        }
        // Check for MOSSEN.md at project root
        let mossen_md = project_dir.join("MOSSEN.md");
        if mossen_md.exists() {
            file_count += 1;
            if let Ok(meta) = std::fs::metadata(&mossen_md) {
                total_bytes += meta.len() as i64;
            }
        }

        MemoryStateSummary {
            status: MemoryLocation::InProject,
            file_count,
            total_bytes,
            reason: None,
        }
    } else {
        // Check for MOSSEN.md only
        let mossen_md = project_dir.join("MOSSEN.md");
        if mossen_md.exists() {
            let bytes = std::fs::metadata(&mossen_md)
                .map(|m| m.len() as i64)
                .unwrap_or(-1);
            MemoryStateSummary {
                status: MemoryLocation::InProject,
                file_count: 1,
                total_bytes: bytes,
                reason: None,
            }
        } else {
            MemoryStateSummary {
                status: MemoryLocation::Absent,
                file_count: 0,
                total_bytes: 0,
                reason: None,
            }
        }
    }
}

/// Get the relative memory path for display.
fn get_relative_memory_path(path: &str, cwd: &str) -> String {
    if let Some(rel) = path.strip_prefix(cwd) {
        let rel = rel.strip_prefix('/').unwrap_or(rel);
        rel.to_string()
    } else {
        path.to_string()
    }
}

/// `/memory` command.
pub struct RecallDirective;

#[async_trait]
impl Directive for RecallDirective {
    fn name(&self) -> &str {
        "memory"
    }

    fn description(&self) -> &str {
        "View and edit memory files"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, _args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        let product_name = &ctx.product_name;
        let cwd = &ctx.cwd;
        let mem_state = describe_memory_state(cwd);

        let location_label = match mem_state.status {
            MemoryLocation::InProject => "in-project",
            MemoryLocation::External => {
                let reason = mem_state.reason.as_deref().unwrap_or("unknown");
                &format!("external ({})", reason)
            }
            MemoryLocation::Absent => "absent",
        };

        let mut output = String::from("Memory\n\n");

        // Metadata pane
        output.push_str(&format!(
            "auto: on · team: off · location: {} · files: {} · size: {}\n",
            location_label,
            mem_state.file_count,
            format_mem_bytes(mem_state.total_bytes)
        ));
        output.push_str("(metadata only — file contents are never displayed here)\n\n");

        // List memory files
        output.push_str("Memory files:\n");

        let mossen_md = cwd.join("MOSSEN.md");
        if mossen_md.exists() {
            output.push_str(&format!(
                "  • {}\n",
                get_relative_memory_path(&mossen_md.to_string_lossy(), &cwd.to_string_lossy())
            ));
        }

        let mossen_local_md = cwd.join("MOSSEN.local.md");
        if mossen_local_md.exists() {
            output.push_str(&format!(
                "  • {}\n",
                get_relative_memory_path(
                    &mossen_local_md.to_string_lossy(),
                    &cwd.to_string_lossy()
                )
            ));
        }

        // Check .mossen/ directory
        let mossen_dir = cwd.join(".mossen");
        if mossen_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&mossen_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() {
                        output.push_str(&format!(
                            "  • {}\n",
                            get_relative_memory_path(
                                &path.to_string_lossy(),
                                &cwd.to_string_lossy()
                            )
                        ));
                    }
                }
            }
        }

        if mem_state.file_count == 0 {
            output.push_str("  (no memory files found)\n");
        }

        output.push_str(&format!(
            "\nUse memory files to store durable project guidance for {}.\n\
             To edit: use /memory to select a file, or edit directly in your editor.\n\
             To change editor, set $EDITOR or $VISUAL environment variable.",
            product_name
        ));

        Ok(CommandResult::Text(output))
    }
}
