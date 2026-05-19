//! Overlay tracking for Escape key coordination.
//!
//! Translates: context/overlayContext.tsx
//! React hooks → struct-based overlay registry.

use std::collections::HashSet;
use std::sync::{Arc, RwLock};

/// Non-modal overlays that shouldn't disable TextInput focus.
const NON_MODAL_OVERLAYS: &[&str] = &["autocomplete"];

/// Overlay state manager — tracks active overlays for escape key coordination.
///
/// Solves the problem of escape key handling when overlays (like Select with onCancel)
/// are open. The CancelRequestHandler needs to know when an overlay is active so it
/// doesn't cancel requests when the user just wants to dismiss the overlay.
#[derive(Debug, Clone)]
pub struct OverlayTracker {
    active_overlays: Arc<RwLock<HashSet<String>>>,
}

impl OverlayTracker {
    pub fn new() -> Self {
        Self {
            active_overlays: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// Register an overlay as active.
    pub fn register(&self, id: &str) {
        self.active_overlays
            .write()
            .unwrap()
            .insert(id.to_string());
    }

    /// Unregister an overlay (no longer active).
    pub fn unregister(&self, id: &str) {
        self.active_overlays.write().unwrap().remove(id);
    }

    /// Check if any overlay is currently active.
    pub fn is_overlay_active(&self) -> bool {
        !self.active_overlays.read().unwrap().is_empty()
    }

    /// Check if any modal overlay is currently active.
    /// Modal overlays are overlays that should capture all input (like Select dialogs).
    /// Non-modal overlays (like autocomplete) don't disable TextInput focus.
    pub fn is_modal_overlay_active(&self) -> bool {
        let overlays = self.active_overlays.read().unwrap();
        for id in overlays.iter() {
            if !NON_MODAL_OVERLAYS.contains(&id.as_str()) {
                return true;
            }
        }
        false
    }

    /// Get the set of currently active overlay IDs.
    pub fn active_ids(&self) -> HashSet<String> {
        self.active_overlays.read().unwrap().clone()
    }
}

impl Default for OverlayTracker {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// TS-mirror — `context/overlayContext.tsx` exports.
// ---------------------------------------------------------------------------

use std::sync::OnceLock;

fn global_overlay_tracker() -> &'static Arc<OverlayTracker> {
    static G: OnceLock<Arc<OverlayTracker>> = OnceLock::new();
    G.get_or_init(|| Arc::new(OverlayTracker::new()))
}

/// `overlayContext.tsx` `useRegisterOverlay`.
pub fn use_register_overlay(id: &str) {
    global_overlay_tracker().register(id);
}

/// `overlayContext.tsx` `useIsOverlayActive`.
pub fn use_is_overlay_active() -> bool {
    global_overlay_tracker().is_overlay_active()
}

/// `overlayContext.tsx` `useIsModalOverlayActive`.
pub fn use_is_modal_overlay_active() -> bool {
    global_overlay_tracker().is_modal_overlay_active()
}
