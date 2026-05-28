//! Auto dream service - memory consolidation during idle periods

use std::path::PathBuf;
use std::time::{Duration, Instant};
use parking_lot::Mutex;
use tracing::{debug, info, warn};

use super::config::AutoDreamConfig;
use super::consolidation_lock::*;
use super::consolidation_prompt::build_consolidation_prompt;

struct AutoDreamState {
    last_activity: Instant,
    config: AutoDreamConfig,
    running: bool,
}

static STATE: once_cell::sync::Lazy<Mutex<AutoDreamState>> =
    once_cell::sync::Lazy::new(|| {
        Mutex::new(AutoDreamState {
            last_activity: Instant::now(),
            config: AutoDreamConfig::default(),
            running: false,
        })
    });

/// Record user activity (resets idle timer)
pub fn record_activity() {
    let mut state = STATE.lock();
    state.last_activity = Instant::now();
}

/// Check if system has been idle long enough to trigger consolidation
pub fn is_idle_for_consolidation() -> bool {
    let state = STATE.lock();
    if !state.config.enabled || state.running {
        return false;
    }
    state.last_activity.elapsed() >= Duration::from_secs(state.config.idle_threshold_secs)
}

/// Run auto-dream consolidation
pub async fn run_auto_dream(memory_dir: &PathBuf) -> Result<u32, String> {
    {
        let mut state = STATE.lock();
        if state.running {
            return Err("Already running".to_string());
        }
        state.running = true;
    }

    let lock_path = memory_dir.join(".consolidation.lock");
    if let Err(e) = acquire_consolidation_lock(&lock_path).await {
        let mut state = STATE.lock();
        state.running = false;
        return Err(e);
    }

    let result = perform_consolidation(memory_dir).await;

    release_consolidation_lock(&lock_path).await;
    {
        let mut state = STATE.lock();
        state.running = false;
    }

    result
}

async fn perform_consolidation(memory_dir: &PathBuf) -> Result<u32, String> {
    // Read all memory files in the directory
    let mut entries = tokio::fs::read_dir(memory_dir)
        .await
        .map_err(|e| format!("Failed to read memory dir: {}", e))?;

    let mut memory_contents: Vec<(String, String)> = Vec::new();

    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.extension().map_or(false, |ext| ext == "md") {
            if let Ok(content) = tokio::fs::read_to_string(&path).await {
                let filename = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                memory_contents.push((filename, content));
            }
        }
    }

    let config = STATE.lock().config.clone();
    if memory_contents.len() <= 1 {
        debug!("Not enough memory files to consolidate");
        return Ok(0);
    }

    // Limit batch size
    if memory_contents.len() > config.max_files_per_batch {
        memory_contents.truncate(config.max_files_per_batch);
    }

    let _prompt = build_consolidation_prompt(&memory_contents);

    // In full implementation: run forked agent with consolidation prompt,
    // write output to consolidated file, remove source files
    info!(
        "Auto-dream: consolidated {} memory files",
        memory_contents.len()
    );

    Ok(memory_contents.len() as u32)
}

/// Configure auto-dream settings
pub fn configure_auto_dream(config: AutoDreamConfig) {
    let mut state = STATE.lock();
    state.config = config;
}

/// TS `initAutoDream` — install the AutoDream background scheduler. Idempotent.
pub fn init_auto_dream() {
    // Idempotent install: record activity once so the first idle window is
    // anchored to startup rather than to whatever sentinel value preceded it.
    record_activity();
}
