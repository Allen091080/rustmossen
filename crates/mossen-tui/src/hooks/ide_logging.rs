//! IDE logging hook (useIdeLogging.ts).
//!
//! Forwards log messages to the connected IDE for display.

/// Log level for IDE messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdeLogLevel {
    Debug,
    Info,
    Warning,
    Error,
}

/// State for IDE logging.
#[derive(Debug, Clone)]
pub struct IdeLoggingState {
    pub enabled: bool,
    pub buffer: Vec<IdeLogEntry>,
    pub max_buffer_size: usize,
    pub min_level: IdeLogLevel,
}

#[derive(Debug, Clone)]
pub struct IdeLogEntry {
    pub level: IdeLogLevel,
    pub message: String,
    pub source: String,
    pub timestamp: u64,
}

impl IdeLoggingState {
    pub fn new() -> Self {
        Self {
            enabled: false,
            buffer: Vec::new(),
            max_buffer_size: 1000,
            min_level: IdeLogLevel::Info,
        }
    }

    /// Log a message.
    pub fn log(&mut self, level: IdeLogLevel, message: String, source: String) {
        if !self.enabled || (level as u8) < (self.min_level as u8) {
            return;
        }
        self.buffer.push(IdeLogEntry {
            level,
            message,
            source,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        });
        if self.buffer.len() > self.max_buffer_size {
            self.buffer.remove(0);
        }
    }

    /// Take all buffered log entries.
    pub fn take_entries(&mut self) -> Vec<IdeLogEntry> {
        std::mem::take(&mut self.buffer)
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn set_min_level(&mut self, level: IdeLogLevel) {
        self.min_level = level;
    }
}

impl Default for IdeLoggingState {
    fn default() -> Self {
        Self::new()
    }
}
