//! Diagnostic tracking service — monitors IDE diagnostics for new errors

use std::collections::{HashMap, HashSet};

use mossen_utils::string_utils::truncate_chars_with_suffix;

/// Diagnostic severity
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
    Hint,
}

/// A source range in a file
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticRange {
    pub start_line: u32,
    pub start_character: u32,
    pub end_line: u32,
    pub end_character: u32,
}

/// Single diagnostic entry
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub message: String,
    pub severity: DiagnosticSeverity,
    pub range: DiagnosticRange,
    pub source: Option<String>,
    pub code: Option<String>,
}

/// Diagnostics for a file
#[derive(Debug, Clone)]
pub struct DiagnosticFile {
    pub uri: String,
    pub diagnostics: Vec<Diagnostic>,
}

const MAX_DIAGNOSTICS_SUMMARY_CHARS: usize = 4000;

/// Diagnostic tracking service (singleton pattern)
pub struct DiagnosticTrackingService {
    baseline: HashMap<String, Vec<Diagnostic>>,
    initialized: bool,
    last_processed_timestamps: HashMap<String, u64>,
    right_file_diagnostics_state: HashMap<String, Vec<Diagnostic>>,
}

impl DiagnosticTrackingService {
    pub fn new() -> Self {
        Self {
            baseline: HashMap::new(),
            initialized: false,
            last_processed_timestamps: HashMap::new(),
            right_file_diagnostics_state: HashMap::new(),
        }
    }

    pub fn initialize(&mut self) {
        if self.initialized {
            return;
        }
        self.initialized = true;
    }

    pub fn shutdown(&mut self) {
        self.initialized = false;
        self.baseline.clear();
        self.right_file_diagnostics_state.clear();
        self.last_processed_timestamps.clear();
    }

    pub fn reset(&mut self) {
        self.baseline.clear();
        self.right_file_diagnostics_state.clear();
        self.last_processed_timestamps.clear();
    }

    fn normalize_file_uri(file_uri: &str) -> String {
        let protocol_prefixes = ["file://", "_mossen_fs_right:", "_mossen_fs_left:"];
        let mut normalized = file_uri.to_string();
        for prefix in &protocol_prefixes {
            if file_uri.starts_with(prefix) {
                normalized = file_uri[prefix.len()..].to_string();
                break;
            }
        }
        // Normalize path separators for consistent lookups
        normalized.replace('\\', "/").to_lowercase()
    }

    /// Capture baseline diagnostics for a file before editing
    pub fn before_file_edited(&mut self, file_path: &str, diagnostics: Vec<Diagnostic>) {
        if !self.initialized {
            return;
        }
        let normalized = Self::normalize_file_uri(file_path);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        self.baseline.insert(normalized.clone(), diagnostics);
        self.last_processed_timestamps.insert(normalized, now);
    }

    /// Get new diagnostics not in the baseline
    pub fn get_new_diagnostics(&mut self, all_files: &[DiagnosticFile]) -> Vec<DiagnosticFile> {
        if !self.initialized {
            return Vec::new();
        }

        let mut new_diagnostic_files = Vec::new();

        // Filter for files with baselines and file:// protocol
        let files_with_baselines: Vec<&DiagnosticFile> = all_files
            .iter()
            .filter(|f| {
                let norm = Self::normalize_file_uri(&f.uri);
                self.baseline.contains_key(&norm) && f.uri.starts_with("file://")
            })
            .collect();

        // Build map of _mossen_fs_right files
        let right_files: HashMap<String, &DiagnosticFile> = all_files
            .iter()
            .filter(|f| {
                let norm = Self::normalize_file_uri(&f.uri);
                self.baseline.contains_key(&norm) && f.uri.starts_with("_mossen_fs_right:")
            })
            .map(|f| (Self::normalize_file_uri(&f.uri), f))
            .collect();

        for file in files_with_baselines {
            let normalized_path = Self::normalize_file_uri(&file.uri);
            let baseline_diagnostics = self
                .baseline
                .get(&normalized_path)
                .cloned()
                .unwrap_or_default();

            // Choose file or right-file based on state changes
            let file_to_use = if let Some(right_file) = right_files.get(&normalized_path) {
                let prev = self.right_file_diagnostics_state.get(&normalized_path);
                let use_right = prev.is_none()
                    || !Self::are_diagnostic_arrays_equal(prev.unwrap(), &right_file.diagnostics);
                self.right_file_diagnostics_state
                    .insert(normalized_path.clone(), right_file.diagnostics.clone());
                if use_right {
                    *right_file
                } else {
                    file
                }
            } else {
                file
            };

            // Find new diagnostics not in baseline
            let new_diagnostics: Vec<Diagnostic> = file_to_use
                .diagnostics
                .iter()
                .filter(|d| {
                    !baseline_diagnostics
                        .iter()
                        .any(|b| Self::are_diagnostics_equal(d, b))
                })
                .cloned()
                .collect();

            if !new_diagnostics.is_empty() {
                new_diagnostic_files.push(DiagnosticFile {
                    uri: file.uri.clone(),
                    diagnostics: new_diagnostics,
                });
            }

            // Update baseline
            self.baseline
                .insert(normalized_path, file_to_use.diagnostics.clone());
        }

        new_diagnostic_files
    }

    fn are_diagnostics_equal(a: &Diagnostic, b: &Diagnostic) -> bool {
        a.message == b.message
            && a.severity == b.severity
            && a.source == b.source
            && a.code == b.code
            && a.range == b.range
    }

    fn are_diagnostic_arrays_equal(a: &[Diagnostic], b: &[Diagnostic]) -> bool {
        if a.len() != b.len() {
            return false;
        }
        a.iter()
            .all(|da| b.iter().any(|db| Self::are_diagnostics_equal(da, db)))
            && b.iter()
                .all(|db| a.iter().any(|da| Self::are_diagnostics_equal(da, db)))
    }

    /// Format diagnostics into a summary string
    pub fn format_diagnostics_summary(files: &[DiagnosticFile]) -> String {
        let truncation_marker = "…[truncated]";
        let result: String = files
            .iter()
            .map(|file| {
                let filename = file.uri.rsplit('/').next().unwrap_or(&file.uri);
                let diagnostics: String = file
                    .diagnostics
                    .iter()
                    .map(|d| {
                        let symbol = Self::get_severity_symbol(&d.severity);
                        let code_str = d
                            .code
                            .as_deref()
                            .map(|c| format!(" [{}]", c))
                            .unwrap_or_default();
                        let source_str = d
                            .source
                            .as_deref()
                            .map(|s| format!(" ({})", s))
                            .unwrap_or_default();
                        format!(
                            "  {} [Line {}:{}] {}{}{}",
                            symbol,
                            d.range.start_line + 1,
                            d.range.start_character + 1,
                            d.message,
                            code_str,
                            source_str,
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                format!("{}:\n{}", filename, diagnostics)
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        if result.chars().count() > MAX_DIAGNOSTICS_SUMMARY_CHARS {
            truncate_chars_with_suffix(
                &result,
                MAX_DIAGNOSTICS_SUMMARY_CHARS.saturating_sub(truncation_marker.chars().count()),
                truncation_marker,
            )
        } else {
            result
        }
    }

    fn get_severity_symbol(severity: &DiagnosticSeverity) -> &'static str {
        match severity {
            DiagnosticSeverity::Error => "✖",
            DiagnosticSeverity::Warning => "⚠",
            DiagnosticSeverity::Info => "ℹ",
            DiagnosticSeverity::Hint => "★",
        }
    }
}

/// Module-level diagnostic tracker singleton. Mirrors TS
/// `export const diagnosticTracker = new DiagnosticTrackingService(...)`.
#[allow(non_upper_case_globals)]
pub static diagnosticTracker: once_cell::sync::Lazy<std::sync::Mutex<DiagnosticTrackingService>> =
    once_cell::sync::Lazy::new(|| std::sync::Mutex::new(DiagnosticTrackingService::new()));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_summary_truncates_multibyte_text_safely() {
        let files = vec![DiagnosticFile {
            uri: "file:///src/main.rs".to_string(),
            diagnostics: vec![Diagnostic {
                message: "读".repeat(MAX_DIAGNOSTICS_SUMMARY_CHARS + 32),
                severity: DiagnosticSeverity::Error,
                range: DiagnosticRange {
                    start_line: 0,
                    start_character: 0,
                    end_line: 0,
                    end_character: 1,
                },
                source: None,
                code: None,
            }],
        }];

        let summary = DiagnosticTrackingService::format_diagnostics_summary(&files);

        assert!(summary.ends_with("…[truncated]"));
        assert!(summary.is_char_boundary(summary.len()));
    }
}
