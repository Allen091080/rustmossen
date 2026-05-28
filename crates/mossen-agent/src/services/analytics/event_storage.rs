//! Event storage — persistent storage for analytics events (disk-backed queue).

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;

/// Persistent event storage for retry-able analytics events.
pub struct EventStorage {
    storage_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredEvent {
    pub event_name: String,
    pub metadata: serde_json::Value,
    pub timestamp: u64,
    pub attempt_count: u32,
}

impl EventStorage {
    pub fn new(storage_path: PathBuf) -> Self {
        Self { storage_path }
    }

    /// Store an event to disk for later retry.
    pub async fn store(&self, event: &StoredEvent) -> Result<()> {
        fs::create_dir_all(&self.storage_path).await?;
        let filename = format!("{}_{}.json", event.timestamp, event.event_name);
        let path = self.storage_path.join(filename);
        let data = serde_json::to_string(event)?;
        fs::write(path, data).await?;
        Ok(())
    }

    /// Load all stored events from disk.
    pub async fn load_all(&self) -> Result<Vec<StoredEvent>> {
        let mut events = Vec::new();
        if !self.storage_path.exists() {
            return Ok(events);
        }
        let mut entries = fs::read_dir(&self.storage_path).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                if let Ok(data) = fs::read_to_string(&path).await {
                    if let Ok(event) = serde_json::from_str::<StoredEvent>(&data) {
                        events.push(event);
                    }
                }
            }
        }
        Ok(events)
    }

    /// Remove a stored event after successful delivery.
    pub async fn remove(&self, event: &StoredEvent) -> Result<()> {
        let filename = format!("{}_{}.json", event.timestamp, event.event_name);
        let path = self.storage_path.join(filename);
        if path.exists() {
            fs::remove_file(path).await?;
        }
        Ok(())
    }

    /// Clear all stored events.
    pub async fn clear(&self) -> Result<()> {
        if self.storage_path.exists() {
            fs::remove_dir_all(&self.storage_path).await?;
        }
        Ok(())
    }
}
