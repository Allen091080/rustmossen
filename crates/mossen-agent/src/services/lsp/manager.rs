//! LSP manager singleton — global initialization and lifecycle management.

use std::sync::Arc;
use anyhow::Result;
use tokio::sync::{Mutex, OnceCell};
use tracing::{debug, error};

use super::server_manager::LspServerManager;

/// Initialization state of the LSP server manager.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitializationState {
    NotStarted,
    Pending,
    Success,
    Failed,
}

/// Global singleton state for the LSP manager.
struct LspManagerState {
    manager: Option<Arc<LspServerManager>>,
    state: InitializationState,
    error: Option<String>,
    generation: u64,
}

static LSP_STATE: OnceCell<Mutex<LspManagerState>> = OnceCell::const_new();

async fn get_state() -> &'static Mutex<LspManagerState> {
    LSP_STATE
        .get_or_init(|| async {
            Mutex::new(LspManagerState {
                manager: None,
                state: InitializationState::NotStarted,
                error: None,
                generation: 0,
            })
        })
        .await
}

/// Get the singleton LSP server manager instance.
/// Returns None if not yet initialized, initialization failed, or still pending.
pub async fn get_lsp_server_manager() -> Option<Arc<LspServerManager>> {
    let state = get_state().await.lock().await;
    if state.state == InitializationState::Failed {
        return None;
    }
    state.manager.clone()
}

/// Get the current initialization status.
pub async fn get_initialization_status() -> (InitializationState, Option<String>) {
    let state = get_state().await.lock().await;
    (state.state, state.error.clone())
}

/// Check whether at least one language server is connected and healthy.
pub async fn is_lsp_connected() -> bool {
    let state = get_state().await.lock().await;
    if state.state == InitializationState::Failed {
        return false;
    }
    if let Some(ref manager) = state.manager {
        let servers = manager.get_all_servers().await;
        servers.iter().any(|(_, s)| {
            *s != super::config::LspServerState::Error
        })
    } else {
        false
    }
}

/// Wait for LSP server manager initialization to complete.
pub async fn wait_for_initialization() {
    let state = get_state().await.lock().await;
    if state.state == InitializationState::Success
        || state.state == InitializationState::Failed
        || state.state == InitializationState::NotStarted
    {
        return;
    }
    // If pending, we'd need to wait on a signal. For now just return.
}

/// Initialize the LSP server manager singleton.
/// Called during Mossen startup. Safe to call multiple times (idempotent).
pub async fn initialize_lsp_server_manager(is_bare_mode: bool) {
    if is_bare_mode {
        return;
    }
    debug!("[LSP MANAGER] initialize_lsp_server_manager() called");

    let state_mutex = get_state().await;
    let mut state = state_mutex.lock().await;

    // Skip if already initialized or currently initializing
    if state.manager.is_some() && state.state != InitializationState::Failed {
        debug!("[LSP MANAGER] Already initialized or initializing, skipping");
        return;
    }

    // Reset state for retry
    if state.state == InitializationState::Failed {
        state.manager = None;
        state.error = None;
    }

    let manager = Arc::new(LspServerManager::new());
    state.manager = Some(manager.clone());
    state.state = InitializationState::Pending;
    state.generation += 1;
    let current_generation = state.generation;

    debug!("[LSP MANAGER] Created manager instance, state=pending");
    drop(state);

    // Start initialization asynchronously
    let state_mutex_clone = state_mutex;
    tokio::spawn(async move {
        match manager.initialize().await {
            Ok(()) => {
                let mut state = state_mutex_clone.lock().await;
                if state.generation == current_generation {
                    state.state = InitializationState::Success;
                    debug!("LSP server manager initialized successfully");
                }
            }
            Err(e) => {
                let mut state = state_mutex_clone.lock().await;
                if state.generation == current_generation {
                    state.state = InitializationState::Failed;
                    state.error = Some(e.to_string());
                    state.manager = None;
                    error!("Failed to initialize LSP server manager: {}", e);
                }
            }
        }
    });
}

/// Force re-initialization of the LSP server manager.
pub async fn reinitialize_lsp_server_manager() {
    let state_mutex = get_state().await;
    let mut state = state_mutex.lock().await;

    if state.state == InitializationState::NotStarted {
        return;
    }

    debug!("[LSP MANAGER] reinitialize_lsp_server_manager() called");

    // Best-effort shutdown of old instance
    if let Some(ref manager) = state.manager {
        let m = manager.clone();
        tokio::spawn(async move {
            if let Err(e) = m.shutdown().await {
                debug!("[LSP MANAGER] old instance shutdown during reinit failed: {}", e);
            }
        });
    }

    state.manager = None;
    state.state = InitializationState::NotStarted;
    state.error = None;
    drop(state);

    initialize_lsp_server_manager(false).await;
}

/// Shutdown the LSP server manager and clean up resources.
pub async fn shutdown_lsp_server_manager() {
    let state_mutex = get_state().await;
    let mut state = state_mutex.lock().await;

    if state.manager.is_none() {
        return;
    }

    if let Some(ref manager) = state.manager {
        if let Err(e) = manager.shutdown().await {
            error!("Failed to shutdown LSP server manager: {}", e);
        } else {
            debug!("LSP server manager shut down successfully");
        }
    }

    state.manager = None;
    state.state = InitializationState::NotStarted;
    state.error = None;
    state.generation += 1;
}

/// TS `_resetLspManagerForTesting` — drop the cached LSP manager so subsequent
/// `get_lsp_server_manager()` calls re-initialise from scratch.
pub async fn reset_lsp_manager_for_testing() {
    shutdown_lsp_server_manager().await;
}
