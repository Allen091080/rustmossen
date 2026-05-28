//! Input buffer hook (useInputBuffer.ts).
//!
//! Manages an undo buffer for text input with debounced snapshots.

use std::time::{Duration, Instant};

/// A snapshot of input state for undo.
#[derive(Debug, Clone)]
pub struct BufferEntry {
    pub text: String,
    pub cursor_offset: usize,
    pub timestamp: Instant,
}

/// State for the input undo buffer.
#[derive(Debug, Clone)]
pub struct InputBufferState {
    pub buffer: Vec<BufferEntry>,
    pub current_index: i32,
    pub max_buffer_size: usize,
    pub debounce_ms: u64,
    pub last_push_time: Option<Instant>,
}

impl InputBufferState {
    pub fn new(max_size: usize, debounce_ms: u64) -> Self {
        Self {
            buffer: Vec::new(),
            current_index: -1,
            max_buffer_size: max_size,
            debounce_ms,
            last_push_time: None,
        }
    }

    /// Push a new entry to the buffer (with debounce).
    pub fn push(&mut self, text: String, cursor_offset: usize) {
        let now = Instant::now();

        // Check debounce
        if let Some(last) = self.last_push_time {
            if now.duration_since(last) < Duration::from_millis(self.debounce_ms) {
                // Update the last entry instead of pushing new
                if let Some(entry) = self.buffer.last_mut() {
                    entry.text = text;
                    entry.cursor_offset = cursor_offset;
                    entry.timestamp = now;
                    return;
                }
            }
        }

        // Truncate any redo history
        let idx = (self.current_index + 1) as usize;
        self.buffer.truncate(idx);

        // Push new entry
        self.buffer.push(BufferEntry {
            text,
            cursor_offset,
            timestamp: now,
        });

        // Trim to max size
        if self.buffer.len() > self.max_buffer_size {
            self.buffer.remove(0);
        }

        self.current_index = self.buffer.len() as i32 - 1;
        self.last_push_time = Some(now);
    }

    /// Undo: move back one entry. Returns the restored state.
    pub fn undo(&mut self) -> Option<&BufferEntry> {
        if self.current_index <= 0 {
            return None;
        }
        self.current_index -= 1;
        self.buffer.get(self.current_index as usize)
    }

    /// Check if undo is available.
    pub fn can_undo(&self) -> bool {
        self.current_index > 0
    }

    /// Clear the buffer.
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.current_index = -1;
        self.last_push_time = None;
    }
}

impl Default for InputBufferState {
    fn default() -> Self {
        Self::new(50, 300)
    }
}

/// Configuration passed to the `useInputBuffer` equivalent.
///
/// TS source: `export type UseInputBufferProps`.
#[derive(Debug, Clone, Copy)]
pub struct UseInputBufferProps {
    pub max_buffer_size: usize,
    pub debounce_ms: u64,
}

/// Result returned by the `useInputBuffer` equivalent. The Rust port
/// stores the state behind a mutable reference so callers don't need to
/// hold a tower of closures.
///
/// TS source: `export type UseInputBufferResult`.
#[derive(Debug, Clone)]
pub struct UseInputBufferResult {
    pub can_undo: bool,
}
