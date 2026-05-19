//! Event handler prop definitions (event-handlers.ts).
use std::collections::HashSet;

/// Map from event type to handler prop names.
pub struct HandlerMapping {
    pub bubble: Option<&'static str>,
    pub capture: Option<&'static str>,
}

pub fn handler_for_event(event_type: &str) -> Option<HandlerMapping> {
    match event_type {
        "keydown" => Some(HandlerMapping { bubble: Some("onKeyDown"), capture: Some("onKeyDownCapture") }),
        "focus" => Some(HandlerMapping { bubble: Some("onFocus"), capture: Some("onFocusCapture") }),
        "blur" => Some(HandlerMapping { bubble: Some("onBlur"), capture: Some("onBlurCapture") }),
        "paste" => Some(HandlerMapping { bubble: Some("onPaste"), capture: Some("onPasteCapture") }),
        "resize" => Some(HandlerMapping { bubble: Some("onResize"), capture: None }),
        "click" => Some(HandlerMapping { bubble: Some("onClick"), capture: None }),
        _ => None,
    }
}

/// Set of all event handler prop names.
pub fn event_handler_props() -> HashSet<&'static str> {
    let mut set = HashSet::new();
    for prop in &["onKeyDown", "onKeyDownCapture", "onFocus", "onFocusCapture", "onBlur", "onBlurCapture", "onPaste", "onPasteCapture", "onResize", "onClick", "onMouseEnter", "onMouseLeave"] {
        set.insert(*prop);
    }
    set
}
