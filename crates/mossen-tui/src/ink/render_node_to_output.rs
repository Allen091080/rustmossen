//! Render-node-to-output (render-node-to-output.ts) — global render hints.
//!
//! The full TS file is ~1500 lines of yoga-bound rendering logic that doesn't
//! translate verbatim to ratatui. We capture the exported types and the global
//! getter/setter state so callers in higher layers can read and reset hints.

#![allow(dead_code)]

use std::sync::Mutex;

use once_cell::sync::Lazy;

use crate::ink::dom::NodeId;
use crate::ink::layout::Rect;

/// DECSTBM scroll optimisation hint.
#[derive(Debug, Clone, Copy)]
pub struct ScrollHint {
    pub top: i32,
    pub bottom: i32,
    pub delta: i32,
}

/// Follow-scroll event recorded when streaming content scrolls a ScrollBox.
#[derive(Debug, Clone, Copy)]
pub struct FollowScroll {
    pub delta: i32,
    pub viewport_top: i32,
    pub viewport_bottom: i32,
}

#[derive(Debug, Default)]
struct State {
    layout_shifted: bool,
    scroll_hint: Option<ScrollHint>,
    absolute_rects_prev: Vec<Rect>,
    absolute_rects_cur: Vec<Rect>,
    scroll_drain_node: Option<NodeId>,
    follow_scroll: Option<FollowScroll>,
}

static STATE: Lazy<Mutex<State>> = Lazy::new(|| Mutex::new(State::default()));

/// Clear the per-frame layout-shift bit.
pub fn reset_layout_shifted() {
    if let Ok(mut s) = STATE.lock() {
        s.layout_shifted = false;
    }
}

/// True if any node shifted position this frame.
pub fn did_layout_shift() -> bool {
    STATE.lock().ok().map(|s| s.layout_shifted).unwrap_or(false)
}

/// Record that layout shifted this frame.
pub fn mark_layout_shifted() {
    if let Ok(mut s) = STATE.lock() {
        s.layout_shifted = true;
    }
}

/// Clear the scroll hint and rotate the absolute-rect lists.
pub fn reset_scroll_hint() {
    if let Ok(mut s) = STATE.lock() {
        s.scroll_hint = None;
        let cur = std::mem::take(&mut s.absolute_rects_cur);
        s.absolute_rects_prev = cur;
    }
}

/// Read the active scroll hint (if any).
pub fn get_scroll_hint() -> Option<ScrollHint> {
    STATE.lock().ok().and_then(|s| s.scroll_hint)
}

/// Set the active scroll hint.
pub fn set_scroll_hint(hint: ScrollHint) {
    if let Ok(mut s) = STATE.lock() {
        s.scroll_hint = Some(hint);
    }
}

/// Clear the pending scroll drain node.
pub fn reset_scroll_drain_node() {
    if let Ok(mut s) = STATE.lock() {
        s.scroll_drain_node = None;
    }
}

/// Read the pending scroll drain node.
pub fn get_scroll_drain_node() -> Option<NodeId> {
    STATE.lock().ok().and_then(|s| s.scroll_drain_node)
}

/// Update the pending scroll drain node.
pub fn set_scroll_drain_node(node: Option<NodeId>) {
    if let Ok(mut s) = STATE.lock() {
        s.scroll_drain_node = node;
    }
}

/// Consume the latest follow-scroll event (clearing it after read).
pub fn consume_follow_scroll() -> Option<FollowScroll> {
    STATE.lock().ok().and_then(|mut s| s.follow_scroll.take())
}

/// Publish a follow-scroll event from the renderer.
pub fn set_follow_scroll(event: FollowScroll) {
    if let Ok(mut s) = STATE.lock() {
        s.follow_scroll = Some(event);
    }
}

/// Record an absolute-positioned rectangle painted this frame.
pub fn record_absolute_rect(rect: Rect) {
    if let Ok(mut s) = STATE.lock() {
        s.absolute_rects_cur.push(rect);
    }
}

/// Inspect the previous frame's absolute rectangles.
pub fn absolute_rects_prev() -> Vec<Rect> {
    STATE.lock().ok().map(|s| s.absolute_rects_prev.clone()).unwrap_or_default()
}
