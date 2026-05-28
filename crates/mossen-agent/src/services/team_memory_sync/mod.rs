pub mod secret_guard;
pub mod secret_scanner;
pub mod service;
pub mod types;
pub mod watcher;

use std::path::Path;

pub use secret_guard::check_team_mem_secrets;
pub use secret_scanner::{get_secret_label, redact_secrets, scan_for_secrets, SecretMatch};
pub use service::{
    batch_delta_by_bytes, create_sync_state, hash_content, is_team_memory_file_path,
    is_team_memory_sync_available, pull_team_memory, push_team_memory, sync_team_memory, SyncState,
};
pub use types::*;
pub use watcher::{notify_team_memory_write, start_team_memory_watcher, stop_team_memory_watcher};

pub async fn notify_team_memory_file_write(file_path: impl AsRef<Path>) {
    let should_notify = service::is_team_memory_file_path(file_path.as_ref());
    if should_notify {
        watcher::notify_team_memory_write().await;
    }
}
