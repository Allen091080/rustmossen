//! Frame (frame.ts) — rendered frame metadata + patches.

#![allow(dead_code)]

use crate::ink::layout::Size;
use crate::ink::render_node_to_output::ScrollHint;
use crate::ink::screen::{create_screen, CharPool, HyperlinkPool, Screen, StylePool};

/// Terminal cursor state.
#[derive(Debug, Clone, Copy)]
pub struct Cursor {
    pub x: i32,
    pub y: i32,
    pub visible: bool,
}

impl Default for Cursor {
    fn default() -> Self { Self { x: 0, y: 0, visible: true } }
}

/// One rendered frame: a screen, viewport size, and a cursor.
#[derive(Debug, Clone)]
pub struct Frame {
    pub screen: Screen,
    pub viewport: Size,
    pub cursor: Cursor,
    pub scroll_hint: Option<ScrollHint>,
    pub scroll_drain_pending: bool,
}

/// Why the renderer chose to do a full reset this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlickerReason {
    Resize,
    Offscreen,
    Clear,
}

/// Stage timing breakdown for a frame event.
#[derive(Debug, Clone, Default)]
pub struct FrameEventPhases {
    pub renderer: f64,
    pub diff: f64,
    pub optimize: f64,
    pub write: f64,
    pub patches: u32,
    pub yoga: f64,
    pub commit: f64,
    pub yoga_visited: u32,
    pub yoga_measured: u32,
    pub yoga_cache_hits: u32,
    pub yoga_live: u32,
}

/// Per-frame metrics emitted by the renderer.
#[derive(Debug, Clone, Default)]
pub struct FrameEvent {
    pub duration_ms: f64,
    pub phases: Option<FrameEventPhases>,
    pub flickers: Vec<FrameFlicker>,
}

#[derive(Debug, Clone)]
pub struct FrameFlicker {
    pub desired_height: u32,
    pub available_height: u32,
    pub reason: FlickerReason,
}

/// Stdout-bound patch emitted by the diff pipeline.
#[derive(Debug, Clone)]
pub enum Patch {
    Stdout(String),
    Clear(u32),
    ClearTerminal {
        reason: FlickerReason,
        debug_trigger: Option<PatchTriggerDebug>,
    },
    CursorHide,
    CursorShow,
    CursorMove { x: i32, y: i32 },
    CursorTo { col: i32 },
    CarriageReturn,
    Hyperlink { uri: String },
    StyleStr(String),
}

#[derive(Debug, Clone)]
pub struct PatchTriggerDebug {
    pub trigger_y: i32,
    pub prev_line: String,
    pub next_line: String,
}

/// Diff is an ordered sequence of patches.
pub type Diff = Vec<Patch>;

/// Build an empty frame for the given terminal viewport.
pub fn empty_frame(
    rows: u32,
    columns: u32,
    style_pool: &mut StylePool,
    char_pool: CharPool,
    hyperlink_pool: HyperlinkPool,
) -> Frame {
    Frame {
        screen: create_screen(0, 0, style_pool, char_pool, hyperlink_pool),
        viewport: Size { width: columns as f32, height: rows as f32 },
        cursor: Cursor::default(),
        scroll_hint: None,
        scroll_drain_pending: false,
    }
}

/// Decide whether the screen should be cleared between frames.
pub fn should_clear_screen(prev: &Frame, next: &Frame) -> Option<FlickerReason> {
    let did_resize =
        next.viewport.height != prev.viewport.height || next.viewport.width != prev.viewport.width;
    if did_resize {
        return Some(FlickerReason::Resize);
    }
    let current_overflows = (next.screen.height as f32) >= next.viewport.height;
    let previous_overflowed = (prev.screen.height as f32) >= prev.viewport.height;
    if current_overflows || previous_overflowed {
        return Some(FlickerReason::Offscreen);
    }
    None
}
