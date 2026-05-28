//! Settings sync service — syncs user settings and memory files across environments.
//!
//! - Interactive CLI: uploads local settings to remote (incremental)
//! - CCR: downloads remote settings to local before plugin installation

pub mod service;
pub mod types;

pub use service::{
    download_user_settings, redownload_user_settings, upload_user_settings_in_background,
};
pub use types::{SettingsSyncFetchResult, SettingsSyncUploadResult, UserSyncData, SYNC_KEYS};
