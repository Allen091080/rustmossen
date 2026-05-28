//! # memory_snapshot — Agent memory snapshot management
//!
//! Translates `tools/AgentTool/agentMemorySnapshot.ts`.
//! Handles snapshot checking, initialization, and replacement of agent memory.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::debug;

use super::memory::{get_agent_memory_dir, AgentMemoryScope};

const SNAPSHOT_BASE: &str = "agent-memory-snapshots";
const SNAPSHOT_JSON: &str = "snapshot.json";
const SYNCED_JSON: &str = ".snapshot-synced.json";

/// Metadata stored in the snapshot.json file.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SnapshotMeta {
    #[serde(rename = "updatedAt")]
    updated_at: String,
}

/// Metadata tracking what snapshot has been synced.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SyncedMeta {
    #[serde(rename = "syncedFrom")]
    synced_from: String,
}

/// Result of checking agent memory snapshot state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotAction {
    /// No snapshot exists or it's up-to-date.
    None,
    /// Snapshot exists but local memory is empty — initialize from snapshot.
    Initialize { snapshot_timestamp: String },
    /// Snapshot is newer than synced version — prompt for update.
    PromptUpdate { snapshot_timestamp: String },
}

/// Returns the path to the snapshot directory for an agent in the current project.
/// e.g., <cwd>/.mossen/agent-memory-snapshots/<agentType>/
pub fn get_snapshot_dir_for_agent(agent_type: &str) -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    cwd.join(".mossen").join(SNAPSHOT_BASE).join(agent_type)
}

fn get_snapshot_json_path(agent_type: &str) -> PathBuf {
    get_snapshot_dir_for_agent(agent_type).join(SNAPSHOT_JSON)
}

fn get_synced_json_path(agent_type: &str, scope: AgentMemoryScope) -> PathBuf {
    let mem_dir = get_agent_memory_dir(agent_type, scope);
    PathBuf::from(&mem_dir).join(SYNCED_JSON)
}

/// Read and parse a JSON file with the given type.
async fn read_json_file<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    let content = tokio::fs::read_to_string(path).await.ok()?;
    serde_json::from_str(&content).ok()
}

/// Copy snapshot memory files to local agent memory directory.
async fn copy_snapshot_to_local(agent_type: &str, scope: AgentMemoryScope) {
    let snapshot_mem_dir = get_snapshot_dir_for_agent(agent_type);
    let local_mem_dir = PathBuf::from(get_agent_memory_dir(agent_type, scope));

    if let Err(e) = tokio::fs::create_dir_all(&local_mem_dir).await {
        debug!("Failed to create local agent memory dir: {}", e);
        return;
    }

    let mut entries = match tokio::fs::read_dir(&snapshot_mem_dir).await {
        Ok(e) => e,
        Err(e) => {
            debug!("Failed to read snapshot dir: {}", e);
            return;
        }
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let file_name = match path.file_name() {
            Some(n) => n.to_string_lossy().to_string(),
            None => continue,
        };
        if file_name == SNAPSHOT_JSON {
            continue;
        }
        match tokio::fs::read_to_string(&path).await {
            Ok(content) => {
                let dest = local_mem_dir.join(&file_name);
                if let Err(e) = tokio::fs::write(&dest, &content).await {
                    debug!("Failed to copy snapshot file {}: {}", file_name, e);
                }
            }
            Err(e) => {
                debug!("Failed to read snapshot file {}: {}", file_name, e);
            }
        }
    }
}

/// Save synced metadata to track which snapshot timestamp was last applied.
async fn save_synced_meta(agent_type: &str, scope: AgentMemoryScope, snapshot_timestamp: &str) {
    let synced_path = get_synced_json_path(agent_type, scope);
    let local_mem_dir = PathBuf::from(get_agent_memory_dir(agent_type, scope));

    let _ = tokio::fs::create_dir_all(&local_mem_dir).await;

    let meta = SyncedMeta {
        synced_from: snapshot_timestamp.to_string(),
    };

    match serde_json::to_string(&meta) {
        Ok(json) => {
            if let Err(e) = tokio::fs::write(&synced_path, &json).await {
                debug!("Failed to save snapshot sync metadata: {}", e);
            }
        }
        Err(e) => {
            debug!("Failed to serialize sync metadata: {}", e);
        }
    }
}

/// Check if a snapshot exists and whether it's newer than what we last synced.
pub async fn check_agent_memory_snapshot(
    agent_type: &str,
    scope: AgentMemoryScope,
) -> SnapshotAction {
    let snapshot_meta: Option<SnapshotMeta> =
        read_json_file(&get_snapshot_json_path(agent_type)).await;

    let snapshot_meta = match snapshot_meta {
        Some(m) if !m.updated_at.is_empty() => m,
        _ => return SnapshotAction::None,
    };

    let local_mem_dir = PathBuf::from(get_agent_memory_dir(agent_type, scope));

    let has_local_memory = match tokio::fs::read_dir(&local_mem_dir).await {
        Ok(mut entries) => {
            let mut found = false;
            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                if path.is_file() && path.extension().is_some_and(|e| e == "md") {
                    found = true;
                    break;
                }
            }
            found
        }
        Err(_) => false,
    };

    if !has_local_memory {
        return SnapshotAction::Initialize {
            snapshot_timestamp: snapshot_meta.updated_at,
        };
    }

    let synced_meta: Option<SyncedMeta> =
        read_json_file(&get_synced_json_path(agent_type, scope)).await;

    match synced_meta {
        Some(synced) => {
            // Compare timestamps — if snapshot is newer, prompt for update
            if snapshot_meta.updated_at > synced.synced_from {
                SnapshotAction::PromptUpdate {
                    snapshot_timestamp: snapshot_meta.updated_at,
                }
            } else {
                SnapshotAction::None
            }
        }
        None => SnapshotAction::PromptUpdate {
            snapshot_timestamp: snapshot_meta.updated_at,
        },
    }
}

/// Initialize local agent memory from a snapshot (first-time setup).
pub async fn initialize_from_snapshot(
    agent_type: &str,
    scope: AgentMemoryScope,
    snapshot_timestamp: &str,
) {
    debug!(
        "Initializing agent memory for {} from project snapshot",
        agent_type
    );
    copy_snapshot_to_local(agent_type, scope).await;
    save_synced_meta(agent_type, scope, snapshot_timestamp).await;
}

/// Replace local agent memory with the snapshot.
pub async fn replace_from_snapshot(
    agent_type: &str,
    scope: AgentMemoryScope,
    snapshot_timestamp: &str,
) {
    debug!(
        "Replacing agent memory for {} with project snapshot",
        agent_type
    );

    // Remove existing .md files before copying to avoid orphans
    let local_mem_dir = PathBuf::from(get_agent_memory_dir(agent_type, scope));
    if let Ok(mut entries) = tokio::fs::read_dir(&local_mem_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|e| e == "md") {
                let _ = tokio::fs::remove_file(&path).await;
            }
        }
    }

    copy_snapshot_to_local(agent_type, scope).await;
    save_synced_meta(agent_type, scope, snapshot_timestamp).await;
}

/// Mark the current snapshot as synced without changing local memory.
pub async fn mark_snapshot_synced(
    agent_type: &str,
    scope: AgentMemoryScope,
    snapshot_timestamp: &str,
) {
    save_synced_meta(agent_type, scope, snapshot_timestamp).await;
}
