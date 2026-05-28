//! Session memory service — manages per-session memory extraction and storage.

pub mod prompts;
pub mod utils;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{debug, info, warn};

/// Session memory configuration.
#[derive(Debug, Clone)]
pub struct SessionMemoryConfig {
    pub memory_dir: PathBuf,
    pub max_entries: usize,
    pub enabled: bool,
}

/// A session memory entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMemoryEntry {
    pub key: String,
    pub content: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub source: MemorySource,
}

/// Where the memory came from.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MemorySource {
    UserExplicit,
    AutoExtracted,
    SystemGenerated,
}

/// Session memory store.
pub struct SessionMemory {
    entries: HashMap<String, SessionMemoryEntry>,
    config: SessionMemoryConfig,
}

impl SessionMemory {
    pub fn new(config: SessionMemoryConfig) -> Self {
        Self {
            entries: HashMap::new(),
            config,
        }
    }

    /// Add or update a memory entry.
    pub fn upsert(&mut self, key: String, content: String, source: MemorySource) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        if let Some(entry) = self.entries.get_mut(&key) {
            entry.content = content;
            entry.updated_at = now;
        } else {
            self.entries.insert(
                key.clone(),
                SessionMemoryEntry {
                    key,
                    content,
                    created_at: now,
                    updated_at: now,
                    source,
                },
            );
        }
    }

    /// Remove a memory entry.
    pub fn remove(&mut self, key: &str) -> Option<SessionMemoryEntry> {
        self.entries.remove(key)
    }

    /// Get a memory entry by key.
    pub fn get(&self, key: &str) -> Option<&SessionMemoryEntry> {
        self.entries.get(key)
    }

    /// Get all memory entries.
    pub fn get_all(&self) -> Vec<&SessionMemoryEntry> {
        self.entries.values().collect()
    }

    /// Get entries matching a filter.
    pub fn search(&self, query: &str) -> Vec<&SessionMemoryEntry> {
        let query_lower = query.to_lowercase();
        self.entries
            .values()
            .filter(|e| {
                e.key.to_lowercase().contains(&query_lower)
                    || e.content.to_lowercase().contains(&query_lower)
            })
            .collect()
    }

    /// Persist session memory to disk.
    pub async fn save(&self) -> Result<(), String> {
        if !self.config.enabled {
            return Ok(());
        }
        let json = serde_json::to_string_pretty(&self.entries)
            .map_err(|e| format!("Failed to serialize session memory: {}", e))?;
        tokio::fs::create_dir_all(&self.config.memory_dir)
            .await
            .map_err(|e| format!("Failed to create memory dir: {}", e))?;
        let path = self.config.memory_dir.join("session_memory.json");
        tokio::fs::write(&path, json)
            .await
            .map_err(|e| format!("Failed to write session memory: {}", e))?;
        Ok(())
    }

    /// Load session memory from disk.
    pub async fn load(config: SessionMemoryConfig) -> Self {
        let path = config.memory_dir.join("session_memory.json");
        let entries = match tokio::fs::read_to_string(&path).await {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => HashMap::new(),
        };
        Self { entries, config }
    }

    /// Get the count of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Clear all entries.
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}
