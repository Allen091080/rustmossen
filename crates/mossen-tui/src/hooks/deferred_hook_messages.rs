//! Deferred hook messages (useDeferredHookMessages.ts).
//!
//! Collects messages produced by hooks during render and flushes them
//! to the message list in a deferred effect (to avoid setState during render).

/// A hook-produced message to be added to the message list.
#[derive(Debug, Clone)]
pub struct HookMessage {
    pub id: String,
    pub content: String,
    pub source: String,
    pub timestamp: u64,
}

/// State for deferred hook messages.
#[derive(Debug, Clone)]
pub struct DeferredHookMessagesState {
    pending: Vec<HookMessage>,
    flushed: Vec<HookMessage>,
}

impl DeferredHookMessagesState {
    pub fn new() -> Self {
        Self {
            pending: Vec::new(),
            flushed: Vec::new(),
        }
    }

    /// Queue a message to be flushed after render.
    pub fn push(&mut self, message: HookMessage) {
        self.pending.push(message);
    }

    /// Flush all pending messages. Returns the messages that were flushed.
    pub fn flush(&mut self) -> Vec<HookMessage> {
        let messages = std::mem::take(&mut self.pending);
        self.flushed.extend(messages.clone());
        messages
    }

    /// Check if there are pending messages.
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Get the count of flushed messages.
    pub fn flushed_count(&self) -> usize {
        self.flushed.len()
    }

    /// Clear all state.
    pub fn clear(&mut self) {
        self.pending.clear();
        self.flushed.clear();
    }
}

impl Default for DeferredHookMessagesState {
    fn default() -> Self {
        Self::new()
    }
}
