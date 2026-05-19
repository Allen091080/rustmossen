//! Root (root.ts) — render-root container.

use std::sync::{Arc, Mutex};

/// One mounted render instance.
#[derive(Debug, Clone, Default)]
pub struct Instance {
    pub id: u64,
    pub mounted: bool,
}

/// Root container — owns one instance plus the latest framebuffer.
#[derive(Debug, Clone, Default)]
pub struct Root {
    pub instance: Instance,
    pub last_frame_hash: u64,
}

/// Build a new root with the given id.
pub fn create_root(id: u64) -> Arc<Mutex<Root>> {
    Arc::new(Mutex::new(Root {
        instance: Instance { id, mounted: true },
        last_frame_hash: 0,
    }))
}

/// Synchronously render the latest committed tree. Returns whether the
/// frame changed (used to skip identical paints).
pub fn render_sync(root: &Arc<Mutex<Root>>, frame_hash: u64) -> bool {
    let mut r = root.lock().unwrap();
    if r.last_frame_hash == frame_hash {
        return false;
    }
    r.last_frame_hash = frame_hash;
    true
}

/// Hook-like helper named after the TS side.
pub const RENDER_SYNC: fn(&Arc<Mutex<Root>>, u64) -> bool = render_sync;
#[allow(non_upper_case_globals)]
pub const renderSync: fn(&Arc<Mutex<Root>>, u64) -> bool = render_sync;

#[derive(Debug, Clone, Default)]
pub struct RootState {
    pub initialized: bool,
}

impl RootState {
    pub fn new() -> Self {
        Self { initialized: false }
    }
    pub fn initialize(&mut self) {
        self.initialized = true;
    }
}
