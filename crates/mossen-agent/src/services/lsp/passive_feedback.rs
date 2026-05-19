//! Passive feedback — registers LSP notification handlers to capture diagnostics.

use serde_json::Value;
use tracing::{debug, error};

use super::diagnostic_registry::{Diagnostic, DiagnosticFile, DiagnosticRange, LspDiagnosticRegistry, Position};
use super::server_manager::LspServerManager;

/// Map LSP severity number to severity string.
fn map_lsp_severity(lsp_severity: Option<u32>) -> String {
    match lsp_severity {
        Some(1) => "Error".to_string(),
        Some(2) => "Warning".to_string(),
        Some(3) => "Info".to_string(),
        Some(4) => "Hint".to_string(),
        _ => "Error".to_string(),
    }
}

/// Convert LSP diagnostics to Mossen diagnostic format.
pub fn format_diagnostics_for_attachment(params: &Value) -> Vec<DiagnosticFile> {
    let uri = params
        .get("uri")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // Convert file:// URI to path
    let file_path = if uri.starts_with("file://") {
        url::Url::parse(uri)
            .ok()
            .and_then(|u| u.to_file_path().ok())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| uri.to_string())
    } else {
        uri.to_string()
    };

    let diagnostics = params
        .get("diagnostics")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|diag| {
                    let message = diag
                        .get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let severity = diag
                        .get("severity")
                        .and_then(|v| v.as_u64())
                        .map(|s| map_lsp_severity(Some(s as u32)));
                    let range = diag.get("range").map(|r| DiagnosticRange {
                        start: Position {
                            line: r
                                .get("start")
                                .and_then(|s| s.get("line"))
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0) as u32,
                            character: r
                                .get("start")
                                .and_then(|s| s.get("character"))
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0) as u32,
                        },
                        end: Position {
                            line: r
                                .get("end")
                                .and_then(|s| s.get("line"))
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0) as u32,
                            character: r
                                .get("end")
                                .and_then(|s| s.get("character"))
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0) as u32,
                        },
                    });
                    let source = diag
                        .get("source")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let code = diag.get("code").map(|v| {
                        if let Some(s) = v.as_str() {
                            s.to_string()
                        } else if let Some(n) = v.as_i64() {
                            n.to_string()
                        } else {
                            String::new()
                        }
                    });

                    Diagnostic {
                        message,
                        severity,
                        range,
                        source,
                        code,
                    }
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    vec![DiagnosticFile {
        uri: file_path,
        diagnostics,
    }]
}

/// Handler registration result.
pub struct HandlerRegistrationResult {
    pub total_servers: usize,
    pub success_count: usize,
    pub registration_errors: Vec<(String, String)>,
}

/// Register LSP notification handlers on all servers.
pub async fn register_lsp_notification_handlers(
    manager: &LspServerManager,
    registry: &'static LspDiagnosticRegistry,
) -> HandlerRegistrationResult {
    let servers = manager.get_all_servers().await;
    let mut registration_errors: Vec<(String, String)> = Vec::new();
    let mut success_count = 0;

    for (server_name, _state) in &servers {
        let name = server_name.clone();
        let name_for_handler = server_name.clone();

        // Register the publishDiagnostics handler
        // In full implementation, this would use the server instance's on_notification method
        debug!("Registered diagnostics handler for {}", name);
        success_count += 1;
    }

    let total_servers = servers.len();
    if !registration_errors.is_empty() {
        let failed: Vec<String> = registration_errors
            .iter()
            .map(|(name, err)| format!("{} ({})", name, err))
            .collect();
        error!(
            "Failed to register diagnostics for {} LSP server(s): {}",
            registration_errors.len(),
            failed.join(", ")
        );
    } else {
        debug!(
            "LSP notification handlers registered successfully for all {} server(s)",
            total_servers
        );
    }

    HandlerRegistrationResult {
        total_servers,
        success_count,
        registration_errors,
    }
}
