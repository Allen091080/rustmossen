//! Global keybindings hook.
//!
//! Registers application-wide keybindings that are always active.

use std::collections::HashMap;

/// A global keybinding action.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GlobalAction {
    ToggleHelp,
    ToggleSettings,
    QuickSearch,
    ClearScreen,
    ToggleVim,
    ToggleFullscreen,
    CycleTheme,
    ToggleDevBar,
    ShowExport,
    Custom(String),
}

/// State for global keybindings.
#[derive(Debug, Clone)]
pub struct GlobalKeybindingsState {
    pub bindings: HashMap<String, GlobalAction>,
    pub enabled: bool,
    pub last_triggered: Option<GlobalAction>,
}

impl GlobalKeybindingsState {
    pub fn new() -> Self {
        let mut bindings = HashMap::new();
        bindings.insert("ctrl+/".to_string(), GlobalAction::ToggleHelp);
        bindings.insert("ctrl+,".to_string(), GlobalAction::ToggleSettings);
        bindings.insert("ctrl+r".to_string(), GlobalAction::QuickSearch);
        bindings.insert("ctrl+l".to_string(), GlobalAction::ClearScreen);
        Self {
            bindings,
            enabled: true,
            last_triggered: None,
        }
    }

    /// Process an input key. Returns the action if matched.
    pub fn process_key(&mut self, key: &str) -> Option<&GlobalAction> {
        if !self.enabled {
            return None;
        }
        if let Some(action) = self.bindings.get(key) {
            self.last_triggered = Some(action.clone());
            Some(action)
        } else {
            None
        }
    }

    /// Register a custom keybinding.
    pub fn register(&mut self, key: String, action: GlobalAction) {
        self.bindings.insert(key, action);
    }

    /// Unregister a keybinding.
    pub fn unregister(&mut self, key: &str) {
        self.bindings.remove(key);
    }

    /// Enable/disable global keybindings.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }
}

impl Default for GlobalKeybindingsState {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// GlobalKeybindingHandlers.
// ============================================================================

/// Screen identifier mirroring `Screen` from `screens/REPL.ts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Prompt,
    Transcript,
    Help,
}

/// Expanded view union shared with `tasks_v2` (kept local to avoid a hard
/// dependency between modules).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpandedView {
    None,
    Tasks,
    Teammates,
}

/// Inputs needed by `global_keybinding_handlers` to make routing
/// decisions. Mirrors the props of `<GlobalKeybindingHandlers />`.
#[derive(Debug, Clone, Copy)]
pub struct GlobalKeybindingHandlersInput {
    pub screen: Screen,
    pub expanded_view: ExpandedView,
    pub has_teammates: bool,
    pub show_all_in_transcript: bool,
    pub message_count: usize,
    pub virtual_scroll_active: bool,
    pub search_bar_open: bool,
    pub is_brief_only: bool,
    pub brief_feature_enabled: bool,
}

/// Action one of the global keybindings should perform. Translated from
/// the per-binding callbacks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlobalKeybindingEffect {
    /// No-op (binding not active in this context).
    NoOp,
    /// Update `AppState.expandedView`.
    SetExpandedView(ExpandedView),
    /// Toggle between prompt and transcript screens.
    ToggleTranscript {
        next_screen: Screen,
        next_show_all: bool,
        entering_transcript: bool,
    },
    /// Toggle the `showAllInTranscript` flag.
    ToggleShowAll { next_show_all: bool },
    /// Exit transcript mode.
    ExitTranscript,
    /// Toggle the brief-only display filter.
    SetBriefOnly(bool),
    /// Toggle the built-in terminal panel.
    ToggleTerminal,
    /// Force a full redraw.
    Redraw,
    /// Toggle teammate-message preview.
    ToggleTeammatePreview,
}

/// `GlobalKeybindingHandlers` (logic only — no JSX). Computes the effect
/// each registered keybinding action should produce given the current
/// app/screen state.
///
/// Global keybinding handler state.
/// The TS component renders nothing and exists purely to register
/// callbacks; we model that as pure functions returning effects so the
/// caller can apply them to its state store.
pub fn global_keybinding_handlers(
    action: &str,
    input: &GlobalKeybindingHandlersInput,
) -> GlobalKeybindingEffect {
    match action {
        // ctrl+t — toggle todos / cycle teammates view
        "app:toggleTodos" => {
            let next = if input.has_teammates {
                match input.expanded_view {
                    ExpandedView::None => ExpandedView::Tasks,
                    ExpandedView::Tasks => ExpandedView::Teammates,
                    ExpandedView::Teammates => ExpandedView::None,
                }
            } else if input.expanded_view == ExpandedView::Tasks {
                ExpandedView::None
            } else {
                ExpandedView::Tasks
            };
            GlobalKeybindingEffect::SetExpandedView(next)
        }

        // ctrl+o — toggle transcript mode.
        "app:toggleTranscript" => {
            let entering = input.screen != Screen::Transcript;
            let next_screen = if entering {
                Screen::Transcript
            } else {
                Screen::Prompt
            };
            GlobalKeybindingEffect::ToggleTranscript {
                next_screen,
                next_show_all: false,
                entering_transcript: entering,
            }
        }

        // ctrl+e — toggle showing all messages in transcript (only when
        // transcript mode is active and virtual scroll isn't owning the
        // keystrokes).
        "transcript:toggleShowAll" => {
            if input.screen == Screen::Transcript && !input.virtual_scroll_active {
                GlobalKeybindingEffect::ToggleShowAll {
                    next_show_all: !input.show_all_in_transcript,
                }
            } else {
                GlobalKeybindingEffect::NoOp
            }
        }

        // Esc / ctrl+c in transcript mode → exit (search bar handles its
        // own Esc).
        "transcript:exit" => {
            if input.screen == Screen::Transcript && !input.search_bar_open {
                GlobalKeybindingEffect::ExitTranscript
            } else {
                GlobalKeybindingEffect::NoOp
            }
        }

        // ctrl+shift+b — toggle brief-only display mode (only when the
        // KAIROS brief feature flag is enabled).
        "app:toggleBrief" => {
            if !input.brief_feature_enabled && !input.is_brief_only {
                return GlobalKeybindingEffect::NoOp;
            }
            GlobalKeybindingEffect::SetBriefOnly(!input.is_brief_only)
        }

        // ctrl+l — force-redraw the terminal.
        "app:redraw" => GlobalKeybindingEffect::Redraw,

        // app:toggleTerminal (meta+j) — toggle built-in terminal panel.
        "app:toggleTerminal" => GlobalKeybindingEffect::ToggleTerminal,

        // app:toggleTeammatePreview
        "app:toggleTeammatePreview" => GlobalKeybindingEffect::ToggleTeammatePreview,

        _ => GlobalKeybindingEffect::NoOp,
    }
}

#[cfg(test)]
mod handlers_tests {
    use super::*;

    fn base() -> GlobalKeybindingHandlersInput {
        GlobalKeybindingHandlersInput {
            screen: Screen::Prompt,
            expanded_view: ExpandedView::None,
            has_teammates: false,
            show_all_in_transcript: false,
            message_count: 0,
            virtual_scroll_active: false,
            search_bar_open: false,
            is_brief_only: false,
            brief_feature_enabled: true,
        }
    }

    #[test]
    fn toggle_todos_no_teammates() {
        let r = global_keybinding_handlers("app:toggleTodos", &base());
        assert_eq!(
            r,
            GlobalKeybindingEffect::SetExpandedView(ExpandedView::Tasks)
        );
    }

    #[test]
    fn toggle_todos_cycle_teammates() {
        let mut i = base();
        i.has_teammates = true;
        i.expanded_view = ExpandedView::Tasks;
        let r = global_keybinding_handlers("app:toggleTodos", &i);
        assert_eq!(
            r,
            GlobalKeybindingEffect::SetExpandedView(ExpandedView::Teammates)
        );
    }

    #[test]
    fn transcript_exit_blocked_by_search() {
        let mut i = base();
        i.screen = Screen::Transcript;
        i.search_bar_open = true;
        assert_eq!(
            global_keybinding_handlers("transcript:exit", &i),
            GlobalKeybindingEffect::NoOp
        );
    }
}
