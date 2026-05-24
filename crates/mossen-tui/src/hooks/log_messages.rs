//! Log messages hook (useLogMessages.ts).
//!
//! Manages a buffer of log messages for display in the dev bar.

use std::collections::VecDeque;

/// A log message entry.
#[derive(Debug, Clone)]
pub struct LogMessage {
    pub id: u64,
    pub level: LogLevel,
    pub message: String,
    pub source: String,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

/// State for log message buffer.
#[derive(Debug, Clone)]
pub struct LogMessagesState {
    pub messages: VecDeque<LogMessage>,
    pub max_messages: usize,
    pub next_id: u64,
    pub filter_level: LogLevel,
    pub is_visible: bool,
}

impl LogMessagesState {
    pub fn new(max_messages: usize) -> Self {
        Self {
            messages: VecDeque::new(),
            max_messages,
            next_id: 0,
            filter_level: LogLevel::Info,
            is_visible: false,
        }
    }

    /// Add a log message.
    pub fn push(&mut self, level: LogLevel, message: String, source: String) {
        let id = self.next_id;
        self.next_id += 1;
        self.messages.push_back(LogMessage {
            id,
            level,
            message,
            source,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        });
        if self.messages.len() > self.max_messages {
            self.messages.pop_front();
        }
    }

    /// Get messages filtered by current level.
    pub fn filtered(&self) -> Vec<&LogMessage> {
        self.messages
            .iter()
            .filter(|m| (m.level as u8) >= (self.filter_level as u8))
            .collect()
    }

    /// Set the minimum display level.
    pub fn set_filter(&mut self, level: LogLevel) {
        self.filter_level = level;
    }

    /// Clear all messages.
    pub fn clear(&mut self) {
        self.messages.clear();
    }

    /// Toggle visibility.
    pub fn toggle_visible(&mut self) {
        self.is_visible = !self.is_visible;
    }
}

impl Default for LogMessagesState {
    fn default() -> Self {
        Self::new(500)
    }
}
