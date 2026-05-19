//! Node cache (node-cache.ts).
//!
//! Tracks cached layout rectangles and pending clears for DOM nodes.

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::Mutex;

use once_cell::sync::Lazy;

use crate::ink::dom::NodeId;
use crate::ink::layout::Rect;

/// Cached layout for a rendered node.
#[derive(Debug, Clone, Default)]
pub struct CachedLayout {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub top: Option<f32>,
}

#[derive(Debug, Default)]
struct CacheState {
    nodes: HashMap<NodeId, CachedLayout>,
    pending_clears: HashMap<NodeId, Vec<Rect>>,
    absolute_node_removed: bool,
}

static STATE: Lazy<Mutex<CacheState>> = Lazy::new(|| Mutex::new(CacheState::default()));

/// Pseudo-`WeakMap` for node layout — handle the global cache.
pub struct NodeCache;
impl NodeCache {
    pub fn get(&self, id: NodeId) -> Option<CachedLayout> {
        STATE.lock().ok()?.nodes.get(&id).cloned()
    }
    pub fn set(&self, id: NodeId, layout: CachedLayout) {
        if let Ok(mut s) = STATE.lock() {
            s.nodes.insert(id, layout);
        }
    }
    pub fn delete(&self, id: NodeId) {
        if let Ok(mut s) = STATE.lock() {
            s.nodes.remove(&id);
        }
    }
}

/// Global handle (mirrors the TS `nodeCache` weak map).
pub const NODE_CACHE: NodeCache = NodeCache;
pub const PENDING_CLEARS: NodeCache = NodeCache;

/// Append a rectangle that must be cleared on the next render for `parent`.
pub fn add_pending_clear(parent: NodeId, child: NodeId) {
    let rect = STATE
        .lock()
        .ok()
        .and_then(|s| s.nodes.get(&child).cloned())
        .map(|c| Rect { x: c.x, y: c.y, width: c.width, height: c.height });
    if let Some(r) = rect {
        if let Ok(mut s) = STATE.lock() {
            s.pending_clears.entry(parent).or_default().push(r);
        }
    }
}

/// Append a rectangle with an explicit absolute-positioning flag.
pub fn add_pending_clear_rect(parent: NodeId, rect: Rect, is_absolute: bool) {
    if let Ok(mut s) = STATE.lock() {
        s.pending_clears.entry(parent).or_default().push(rect);
        if is_absolute {
            s.absolute_node_removed = true;
        }
    }
}

/// Check + reset the global "absolute node removed" flag.
pub fn consume_absolute_removed_flag() -> bool {
    if let Ok(mut s) = STATE.lock() {
        let had = s.absolute_node_removed;
        s.absolute_node_removed = false;
        return had;
    }
    false
}

/// Drain pending clears for a parent.
pub fn drain_pending_clears(parent: NodeId) -> Vec<Rect> {
    STATE
        .lock()
        .ok()
        .and_then(|mut s| s.pending_clears.remove(&parent))
        .unwrap_or_default()
}
