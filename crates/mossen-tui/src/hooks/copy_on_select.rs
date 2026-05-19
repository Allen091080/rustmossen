//! Copy on select hook (useCopyOnSelect.ts).
//!
//! Monitors selection changes and copies selected text to clipboard
//! when the copyOnSelect config option is enabled.

/// State for copy-on-select behavior.
#[derive(Debug, Clone)]
pub struct CopyOnSelectState {
    pub enabled: bool,
    pub last_selection: Option<String>,
    pub copy_count: u64,
}

impl CopyOnSelectState {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            last_selection: None,
            copy_count: 0,
        }
    }

    /// Handle a selection change. Returns the text to copy if applicable.
    pub fn on_selection_change(&mut self, selected_text: Option<&str>) -> Option<&str> {
        if !self.enabled {
            return None;
        }

        match selected_text {
            Some(text) if !text.is_empty() => {
                // Only copy if selection changed
                let should_copy = self.last_selection.as_deref() != Some(text);
                self.last_selection = Some(text.to_string());
                if should_copy {
                    self.copy_count += 1;
                    self.last_selection.as_deref()
                } else {
                    None
                }
            }
            _ => {
                self.last_selection = None;
                None
            }
        }
    }

    /// Set whether copy-on-select is enabled.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }
}

impl Default for CopyOnSelectState {
    fn default() -> Self {
        Self::new(false)
    }
}

/// `useSelectionBgColor` — applies the active theme's selection background
/// color to the selection overlay. Returns the color value the caller
/// should plug into Ink's style pool (or its Rust equivalent).
///
/// TS source: `useSelectionBgColor(selection)`. The TS body resolves the
/// active theme's `selectionBg` and calls `selection.setSelectionBgColor`;
/// the Rust port returns the resolved color so the caller can apply it.
pub fn use_selection_bg_color(theme_selection_bg: &str) -> String {
    theme_selection_bg.to_string()
}
