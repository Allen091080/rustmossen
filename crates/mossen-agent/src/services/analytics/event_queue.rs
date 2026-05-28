//! Event queue — buffering analytics events before sink is ready.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Instant;

use super::sink::LogEventMetadata;

/// A queued analytics event.
#[derive(Debug, Clone)]
pub struct QueuedAnalyticsEvent {
    pub event_name: String,
    pub metadata: LogEventMetadata,
    pub queued_at: Instant,
    pub is_async: bool,
}

/// Thread-safe event queue with bounded capacity.
pub struct EventQueue {
    queue: Mutex<VecDeque<QueuedAnalyticsEvent>>,
    max_size: usize,
}

impl EventQueue {
    pub fn new(max_size: usize) -> Self {
        Self {
            queue: Mutex::new(VecDeque::with_capacity(max_size)),
            max_size,
        }
    }

    /// Enqueue an event. Returns false if queue is full.
    pub fn enqueue(&self, event: QueuedAnalyticsEvent) -> bool {
        let mut queue = self.queue.lock().unwrap();
        if queue.len() >= self.max_size {
            return false;
        }
        queue.push_back(event);
        true
    }

    /// Drain all queued events.
    pub fn drain_all(&self) -> Vec<QueuedAnalyticsEvent> {
        let mut queue = self.queue.lock().unwrap();
        queue.drain(..).collect()
    }

    /// Get current queue size.
    pub fn len(&self) -> usize {
        let queue = self.queue.lock().unwrap();
        queue.len()
    }

    /// Check if queue is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for EventQueue {
    fn default() -> Self {
        Self::new(10_000)
    }
}
