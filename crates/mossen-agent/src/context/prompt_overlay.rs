//! Prompt overlay — portal for content that floats above the prompt.
//!
//! Translates: context/promptOverlayContext.tsx
//! React context/provider → struct-based state.

use std::sync::{Arc, RwLock};

/// Suggestion item for prompt overlay display.
#[derive(Debug, Clone)]
pub struct SuggestionItem {
    pub label: String,
    pub description: Option<String>,
    pub value: String,
}

/// Data for the prompt overlay (slash-command suggestions).
#[derive(Debug, Clone)]
pub struct PromptOverlayData {
    pub suggestions: Vec<SuggestionItem>,
    pub selected_suggestion: usize,
    pub max_column_width: Option<usize>,
}

/// Prompt overlay state manager.
///
/// Two channels:
/// - Suggestion data (structured, written by prompt input footer)
/// - Dialog content (arbitrary, written by prompt input)
#[derive(Debug, Clone)]
pub struct PromptOverlayState {
    data: Arc<RwLock<Option<PromptOverlayData>>>,
    dialog_active: Arc<RwLock<bool>>,
}

impl PromptOverlayState {
    pub fn new() -> Self {
        Self {
            data: Arc::new(RwLock::new(None)),
            dialog_active: Arc::new(RwLock::new(false)),
        }
    }

    /// Get the current prompt overlay data.
    pub fn get_data(&self) -> Option<PromptOverlayData> {
        self.data.read().unwrap().clone()
    }

    /// Set the prompt overlay suggestion data. Pass None to clear.
    pub fn set_data(&self, data: Option<PromptOverlayData>) {
        *self.data.write().unwrap() = data;
    }

    /// Check if a dialog is active in the overlay.
    pub fn is_dialog_active(&self) -> bool {
        *self.dialog_active.read().unwrap()
    }

    /// Set whether a dialog is active in the overlay.
    pub fn set_dialog_active(&self, active: bool) {
        *self.dialog_active.write().unwrap() = active;
    }
}

impl Default for PromptOverlayState {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// TS-mirror — `context/promptOverlayContext.tsx` exports.
// ---------------------------------------------------------------------------

use std::sync::OnceLock;

fn global_overlay() -> &'static Arc<PromptOverlayState> {
    static G: OnceLock<Arc<PromptOverlayState>> = OnceLock::new();
    G.get_or_init(|| Arc::new(PromptOverlayState::new()))
}

/// `promptOverlayContext.tsx` `PromptOverlayProvider`.
pub fn prompt_overlay_provider() -> Arc<PromptOverlayState> {
    Arc::clone(global_overlay())
}

/// `promptOverlayContext.tsx` `usePromptOverlay`.
pub fn use_prompt_overlay() -> Arc<PromptOverlayState> {
    Arc::clone(global_overlay())
}

/// `promptOverlayContext.tsx` `usePromptOverlayDialog`.
pub fn use_prompt_overlay_dialog() -> bool {
    global_overlay().is_dialog_active()
}

/// `promptOverlayContext.tsx` `useSetPromptOverlay`.
pub fn use_set_prompt_overlay() -> Box<dyn Fn(bool) + Send + Sync + 'static> {
    let overlay = Arc::clone(global_overlay());
    Box::new(move |active: bool| {
        overlay.set_dialog_active(active);
    })
}

/// TS `useSetPromptOverlayDialog()` — returns a setter for the prompt overlay
/// dialog visibility flag.
pub fn use_set_prompt_overlay_dialog() -> Box<dyn Fn(bool) + Send + Sync + 'static> {
    use_set_prompt_overlay()
}
