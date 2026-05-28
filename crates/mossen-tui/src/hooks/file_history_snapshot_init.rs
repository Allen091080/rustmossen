//! File history snapshot initialization (useFileHistorySnapshotInit.ts).
//!
//! Initializes file history snapshots at session start for rewind support.

use std::collections::HashMap;
use std::path::PathBuf;

/// A file snapshot taken at session start.
#[derive(Debug, Clone)]
pub struct FileSnapshot {
    pub path: PathBuf,
    pub content_hash: String,
    pub size: u64,
    pub modified_at: u64,
}

/// State for file history snapshot initialization.
#[derive(Debug, Clone)]
pub struct FileHistorySnapshotInitState {
    pub snapshots: HashMap<PathBuf, FileSnapshot>,
    pub initialized: bool,
    pub initializing: bool,
    pub error: Option<String>,
}

impl FileHistorySnapshotInitState {
    pub fn new() -> Self {
        Self {
            snapshots: HashMap::new(),
            initialized: false,
            initializing: false,
            error: None,
        }
    }

    /// Start initialization.
    pub fn start_init(&mut self) {
        self.initializing = true;
        self.error = None;
    }

    /// Add a snapshot.
    pub fn add_snapshot(&mut self, snapshot: FileSnapshot) {
        self.snapshots.insert(snapshot.path.clone(), snapshot);
    }

    /// Mark initialization as complete.
    pub fn finish_init(&mut self) {
        self.initializing = false;
        self.initialized = true;
    }

    /// Mark initialization as failed.
    pub fn fail_init(&mut self, error: String) {
        self.initializing = false;
        self.error = Some(error);
    }

    /// Get snapshot for a specific file.
    pub fn get_snapshot(&self, path: &PathBuf) -> Option<&FileSnapshot> {
        self.snapshots.get(path)
    }

    /// Check if a file has changed since snapshot.
    pub fn has_file_changed(&self, path: &PathBuf, current_hash: &str) -> bool {
        match self.snapshots.get(path) {
            Some(snapshot) => snapshot.content_hash != current_hash,
            None => true, // New file
        }
    }
}

impl Default for FileHistorySnapshotInitState {
    fn default() -> Self {
        Self::new()
    }
}
