use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Content portion of team memory data - flat key-value storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamMemoryContent {
    pub entries: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry_checksums: Option<HashMap<String, String>>,
}

/// Full response from GET /api/mossen/team_memory
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamMemoryData {
    pub organization_id: String,
    pub repo: String,
    pub version: u64,
    pub last_modified: String,
    pub checksum: String,
    pub content: TeamMemoryContent,
}

/// Structured 413 error body from the server.
#[derive(Debug, Clone, Deserialize)]
pub struct TeamMemoryTooManyEntriesError {
    pub error: TooManyEntriesErrorDetail,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TooManyEntriesErrorDetail {
    pub details: TooManyEntriesDetails,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TooManyEntriesDetails {
    pub error_code: String,
    pub max_entries: u64,
    pub received_entries: u64,
}

/// A file skipped during push because it contains a detected secret.
#[derive(Debug, Clone)]
pub struct SkippedSecretFile {
    pub path: String,
    pub rule_id: String,
    pub label: String,
}

/// Error type classification for sync results
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncErrorType {
    Auth,
    Timeout,
    Network,
    Parse,
    Conflict,
    NoOauth,
    NoRepo,
    Unknown,
}

/// Result from fetching team memory
#[derive(Debug, Clone)]
pub struct TeamMemorySyncFetchResult {
    pub success: bool,
    pub data: Option<TeamMemoryData>,
    pub is_empty: bool,
    pub not_modified: bool,
    pub checksum: Option<String>,
    pub error: Option<String>,
    pub skip_retry: bool,
    pub error_type: Option<SyncErrorType>,
    pub http_status: Option<u16>,
}

impl Default for TeamMemorySyncFetchResult {
    fn default() -> Self {
        Self {
            success: false,
            data: None,
            is_empty: false,
            not_modified: false,
            checksum: None,
            error: None,
            skip_retry: false,
            error_type: None,
            http_status: None,
        }
    }
}

/// Lightweight metadata-only probe result (GET ?view=hashes).
#[derive(Debug, Clone, Default)]
pub struct TeamMemoryHashesResult {
    pub success: bool,
    pub version: Option<u64>,
    pub checksum: Option<String>,
    pub entry_checksums: Option<HashMap<String, String>>,
    pub error: Option<String>,
    pub error_type: Option<SyncErrorType>,
    pub http_status: Option<u16>,
}

/// Result from uploading team memory with conflict info
#[derive(Debug, Clone)]
pub struct TeamMemorySyncPushResult {
    pub success: bool,
    pub files_uploaded: u64,
    pub checksum: Option<String>,
    pub conflict: bool,
    pub error: Option<String>,
    pub skipped_secrets: Vec<SkippedSecretFile>,
    pub error_type: Option<SyncErrorType>,
    pub http_status: Option<u16>,
    pub server_error_code: Option<String>,
    pub server_max_entries: Option<u64>,
    pub server_received_entries: Option<u64>,
}

impl Default for TeamMemorySyncPushResult {
    fn default() -> Self {
        Self {
            success: false,
            files_uploaded: 0,
            checksum: None,
            conflict: false,
            error: None,
            skipped_secrets: Vec::new(),
            error_type: None,
            http_status: None,
            server_error_code: None,
            server_max_entries: None,
            server_received_entries: None,
        }
    }
}

/// Result from uploading team memory
#[derive(Debug, Clone)]
pub struct TeamMemorySyncUploadResult {
    pub success: bool,
    pub checksum: Option<String>,
    pub last_modified: Option<String>,
    pub conflict: bool,
    pub error: Option<String>,
    pub error_type: Option<SyncErrorType>,
    pub http_status: Option<u16>,
    pub server_error_code: Option<String>,
    pub server_max_entries: Option<u64>,
    pub server_received_entries: Option<u64>,
}

impl Default for TeamMemorySyncUploadResult {
    fn default() -> Self {
        Self {
            success: false,
            checksum: None,
            last_modified: None,
            conflict: false,
            error: None,
            error_type: None,
            http_status: None,
            server_error_code: None,
            server_max_entries: None,
            server_received_entries: None,
        }
    }
}

/// Pull result returned by pull_team_memory
#[derive(Debug, Clone)]
pub struct PullResult {
    pub success: bool,
    pub files_written: u64,
    pub entry_count: u64,
    pub not_modified: bool,
    pub error: Option<String>,
}

/// Sync result returned by sync_team_memory
#[derive(Debug, Clone)]
pub struct SyncResult {
    pub success: bool,
    pub files_pulled: u64,
    pub files_pushed: u64,
    pub error: Option<String>,
}

/// TS `TeamMemoryTooManyEntriesSchema` — Zod schema validator. Mirrors the
/// shape `{ error: { details: { error_code, max_entries, received_entries } } }`.
pub struct TeamMemoryTooManyEntriesSchema;

impl TeamMemoryTooManyEntriesSchema {
    pub fn parse(
        value: &serde_json::Value,
    ) -> Result<TeamMemoryTooManyEntriesError, String> {
        let details = value
            .get("error")
            .and_then(|e| e.get("details"))
            .ok_or("missing error.details")?;
        let code = details
            .get("error_code")
            .and_then(|v| v.as_str())
            .ok_or("missing error_code")?;
        if code != "team_memory_too_many_entries" {
            return Err(format!("unexpected error_code: {code}"));
        }
        let max_entries = details
            .get("max_entries")
            .and_then(|v| v.as_u64())
            .ok_or("missing max_entries")?;
        let received_entries = details
            .get("received_entries")
            .and_then(|v| v.as_u64())
            .ok_or("missing received_entries")?;
        Ok(TeamMemoryTooManyEntriesError {
            error: TooManyEntriesErrorDetail {
                details: TooManyEntriesDetails {
                    error_code: code.to_string(),
                    max_entries,
                    received_entries,
                },
            },
        })
    }
}

/// Alias for the team memory content validator (mirrors TS `TeamMemoryContentSchema`).
pub type TeamMemoryContentSchema = TeamMemoryContent;
/// Alias for the team memory data validator (mirrors TS `TeamMemoryDataSchema`).
pub type TeamMemoryDataSchema = TeamMemoryData;
