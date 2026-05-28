//! LSP diagnostic registry — stores diagnostics received asynchronously from LSP servers.

use lru::LruCache;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use tracing::debug;
use uuid::Uuid;

/// Volume limiting constants.
const MAX_DIAGNOSTICS_PER_FILE: usize = 10;
const MAX_TOTAL_DIAGNOSTICS: usize = 30;
const MAX_DELIVERED_FILES: usize = 500;

/// A diagnostic for a specific location in a file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub message: String,
    pub severity: Option<String>,
    pub range: Option<DiagnosticRange>,
    pub source: Option<String>,
    pub code: Option<String>,
}

/// Range within a file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticRange {
    pub start: Position,
    pub end: Position,
}

/// Position in a file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

/// A file with its diagnostics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticFile {
    pub uri: String,
    pub diagnostics: Vec<Diagnostic>,
}

/// A pending LSP diagnostic notification.
#[derive(Debug, Clone)]
pub struct PendingLspDiagnostic {
    pub server_name: String,
    pub files: Vec<DiagnosticFile>,
    pub timestamp: u64,
    pub attachment_sent: bool,
}

/// Global LSP diagnostic registry.
pub struct LspDiagnosticRegistry {
    pending: Mutex<HashMap<String, PendingLspDiagnostic>>,
    delivered: Mutex<LruCache<String, Vec<String>>>,
}

impl LspDiagnosticRegistry {
    pub fn new() -> Self {
        Self {
            pending: Mutex::new(HashMap::new()),
            delivered: Mutex::new(LruCache::new(
                std::num::NonZeroUsize::new(MAX_DELIVERED_FILES).unwrap(),
            )),
        }
    }

    /// Register LSP diagnostics received from a server.
    pub fn register_pending(&self, server_name: &str, files: Vec<DiagnosticFile>) {
        let diagnostic_id = Uuid::new_v4().to_string();
        debug!(
            "LSP Diagnostics: Registering {} diagnostic file(s) from {} (ID: {})",
            files.len(),
            server_name,
            diagnostic_id
        );

        let mut pending = self.pending.lock().unwrap();
        pending.insert(
            diagnostic_id,
            PendingLspDiagnostic {
                server_name: server_name.to_string(),
                files,
                timestamp: chrono::Utc::now().timestamp_millis() as u64,
                attachment_sent: false,
            },
        );
    }

    /// Get all pending LSP diagnostics that haven't been delivered yet.
    pub fn check_for_diagnostics(&self) -> Vec<(String, Vec<DiagnosticFile>)> {
        debug!(
            "LSP Diagnostics: Checking registry - {} pending",
            self.pending.lock().unwrap().len()
        );

        let mut all_files: Vec<DiagnosticFile> = Vec::new();
        let mut server_names: Vec<String> = Vec::new();
        let mut ids_to_mark: Vec<String> = Vec::new();

        {
            let pending = self.pending.lock().unwrap();
            for (id, diagnostic) in pending.iter() {
                if !diagnostic.attachment_sent {
                    all_files.extend(diagnostic.files.clone());
                    if !server_names.contains(&diagnostic.server_name) {
                        server_names.push(diagnostic.server_name.clone());
                    }
                    ids_to_mark.push(id.clone());
                }
            }
        }

        if all_files.is_empty() {
            return Vec::new();
        }

        // Deduplicate
        let mut deduped_files = self.deduplicate_diagnostic_files(&all_files);

        // Mark as sent and remove
        {
            let mut pending = self.pending.lock().unwrap();
            for id in &ids_to_mark {
                if let Some(d) = pending.get_mut(id) {
                    d.attachment_sent = true;
                }
            }
            pending.retain(|_, v| !v.attachment_sent);
        }

        // Apply volume limiting
        let mut total_diagnostics = 0usize;
        for file in &mut deduped_files {
            // Sort by severity
            file.diagnostics
                .sort_by_key(|d| severity_to_number(d.severity.as_deref()));

            // Cap per file
            if file.diagnostics.len() > MAX_DIAGNOSTICS_PER_FILE {
                file.diagnostics.truncate(MAX_DIAGNOSTICS_PER_FILE);
            }

            // Cap total
            let remaining = MAX_TOTAL_DIAGNOSTICS.saturating_sub(total_diagnostics);
            if file.diagnostics.len() > remaining {
                file.diagnostics.truncate(remaining);
            }
            total_diagnostics += file.diagnostics.len();
        }

        // Filter empty files
        deduped_files.retain(|f| !f.diagnostics.is_empty());

        if deduped_files.is_empty() {
            return Vec::new();
        }

        // Track delivered diagnostics
        {
            let mut delivered = self.delivered.lock().unwrap();
            for file in &deduped_files {
                let keys: Vec<String> = file
                    .diagnostics
                    .iter()
                    .map(|d| create_diagnostic_key(d))
                    .collect();
                delivered.put(file.uri.clone(), keys);
            }
        }

        let combined_server = server_names.join(", ");
        vec![(combined_server, deduped_files)]
    }

    /// Clear all pending diagnostics.
    pub fn clear_all(&self) {
        let mut pending = self.pending.lock().unwrap();
        debug!(
            "LSP Diagnostics: Clearing {} pending diagnostic(s)",
            pending.len()
        );
        pending.clear();
    }

    /// Reset all diagnostic state including cross-turn tracking.
    pub fn reset_all(&self) {
        self.pending.lock().unwrap().clear();
        self.delivered.lock().unwrap().clear();
    }

    /// Clear delivered diagnostics for a specific file.
    pub fn clear_delivered_for_file(&self, file_uri: &str) {
        let mut delivered = self.delivered.lock().unwrap();
        if delivered.pop(file_uri).is_some() {
            debug!(
                "LSP Diagnostics: Clearing delivered diagnostics for {}",
                file_uri
            );
        }
    }

    /// Get count of pending diagnostics.
    pub fn pending_count(&self) -> usize {
        self.pending.lock().unwrap().len()
    }

    fn deduplicate_diagnostic_files(&self, all_files: &[DiagnosticFile]) -> Vec<DiagnosticFile> {
        let mut file_map: HashMap<String, Vec<String>> = HashMap::new();
        let mut deduped: Vec<DiagnosticFile> = Vec::new();
        let delivered = self.delivered.lock().unwrap();

        for file in all_files {
            let seen = file_map.entry(file.uri.clone()).or_default();
            let mut target = deduped.iter_mut().find(|f| f.uri == file.uri);

            if target.is_none() {
                deduped.push(DiagnosticFile {
                    uri: file.uri.clone(),
                    diagnostics: Vec::new(),
                });
                target = deduped.last_mut();
            }
            let target = target.unwrap();

            let previously_delivered = delivered.peek(&file.uri);

            for diag in &file.diagnostics {
                let key = create_diagnostic_key(diag);
                if seen.contains(&key) {
                    continue;
                }
                if let Some(prev) = previously_delivered {
                    if prev.contains(&key) {
                        continue;
                    }
                }
                seen.push(key);
                target.diagnostics.push(diag.clone());
            }
        }

        deduped.retain(|f| !f.diagnostics.is_empty());
        deduped
    }
}

impl Default for LspDiagnosticRegistry {
    fn default() -> Self {
        Self::new()
    }
}

fn severity_to_number(severity: Option<&str>) -> u8 {
    match severity {
        Some("Error") => 1,
        Some("Warning") => 2,
        Some("Info") => 3,
        Some("Hint") => 4,
        _ => 4,
    }
}

fn create_diagnostic_key(diag: &Diagnostic) -> String {
    serde_json::to_string(&serde_json::json!({
        "message": diag.message,
        "severity": diag.severity,
        "range": diag.range,
        "source": diag.source,
        "code": diag.code,
    }))
    .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// TS-mirror — `services/lsp/LSPDiagnosticRegistry.ts` module-level functions.
// ---------------------------------------------------------------------------

use std::sync::OnceLock;

fn global_registry() -> &'static LspDiagnosticRegistry {
    static R: OnceLock<LspDiagnosticRegistry> = OnceLock::new();
    R.get_or_init(LspDiagnosticRegistry::new)
}

/// `LSPDiagnosticRegistry.ts` `registerPendingLSPDiagnostic`.
pub fn register_pending_lsp_diagnostic(server_name: &str, files: Vec<DiagnosticFile>) {
    global_registry().register_pending(server_name, files);
}

/// `LSPDiagnosticRegistry.ts` `checkForLSPDiagnostics`.
pub fn check_for_lsp_diagnostics() -> Vec<(String, Vec<DiagnosticFile>)> {
    global_registry().check_for_diagnostics()
}

/// `LSPDiagnosticRegistry.ts` `clearAllLSPDiagnostics`.
pub fn clear_all_lsp_diagnostics() {
    global_registry().clear_all();
}

/// `LSPDiagnosticRegistry.ts` `resetAllLSPDiagnosticState`.
pub fn reset_all_lsp_diagnostic_state() {
    global_registry().reset_all();
}

/// `LSPDiagnosticRegistry.ts` `clearDeliveredDiagnosticsForFile`.
pub fn clear_delivered_diagnostics_for_file(file_uri: &str) {
    global_registry().clear_delivered_for_file(file_uri);
}

/// `LSPDiagnosticRegistry.ts` `getPendingLSPDiagnosticCount`.
pub fn get_pending_lsp_diagnostic_count() -> usize {
    global_registry().pending_count()
}
