//! Settings sync types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Content portion of user sync data — flat key-value storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSyncContent {
    pub entries: HashMap<String, String>,
}

/// Full response from GET /api/mossen/user_settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSyncData {
    #[serde(rename = "userId")]
    pub user_id: String,
    pub version: u64,
    #[serde(rename = "lastModified")]
    pub last_modified: String,
    pub checksum: String,
    pub content: UserSyncContent,
}

/// Result from fetching user settings.
#[derive(Debug, Clone)]
pub struct SettingsSyncFetchResult {
    pub success: bool,
    pub data: Option<UserSyncData>,
    pub is_empty: bool,
    pub error: Option<String>,
    pub skip_retry: bool,
}

/// Result from uploading user settings.
#[derive(Debug, Clone)]
pub struct SettingsSyncUploadResult {
    pub success: bool,
    pub checksum: Option<String>,
    pub last_modified: Option<String>,
    pub error: Option<String>,
}

/// Sync entry keys.
pub struct SyncKeys;

impl SyncKeys {
    pub fn user_settings() -> String {
        "~/.mossen/settings.json".to_string()
    }

    pub fn user_memory() -> String {
        "~/.mossen/MOSSEN.md".to_string()
    }

    pub fn project_settings(project_id: &str) -> String {
        format!("projects/{}/.mossen/settings.local.json", project_id)
    }

    pub fn project_memory(project_id: &str) -> String {
        format!("projects/{}/MOSSEN.local.md", project_id)
    }
}

pub const SYNC_KEYS: SyncKeys = SyncKeys;

/// Alias for the user sync content validator (mirrors TS `UserSyncContentSchema`).
pub type UserSyncContentSchema = UserSyncContent;
/// Alias for the user sync data validator (mirrors TS `UserSyncDataSchema`).
pub type UserSyncDataSchema = UserSyncData;
