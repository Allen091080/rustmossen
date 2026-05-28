//! Queue processor hook (useQueueProcessor.ts).
//! Processes queued commands when conditions are met (no active query, no UI blocking).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueProcessorStatus {
    Idle,
    Processing,
    Blocked,
}

#[derive(Debug, Clone)]
pub struct QueueProcessorState {
    pub status: QueueProcessorStatus,
    pub is_query_active: bool,
    pub has_active_local_jsx_ui: bool,
    pub process_count: u64,
}

impl QueueProcessorState {
    pub fn new() -> Self {
        Self {
            status: QueueProcessorStatus::Idle,
            is_query_active: false,
            has_active_local_jsx_ui: false,
            process_count: 0,
        }
    }
    pub fn can_process(&self) -> bool {
        !self.is_query_active
            && !self.has_active_local_jsx_ui
            && self.status != QueueProcessorStatus::Processing
    }
    pub fn start_processing(&mut self) {
        self.status = QueueProcessorStatus::Processing;
        self.process_count += 1;
    }
    pub fn finish_processing(&mut self) {
        self.status = QueueProcessorStatus::Idle;
    }
    pub fn set_query_active(&mut self, active: bool) {
        self.is_query_active = active;
        if active {
            self.status = QueueProcessorStatus::Blocked;
        }
    }
    pub fn set_local_jsx_ui_active(&mut self, active: bool) {
        self.has_active_local_jsx_ui = active;
    }
}
impl Default for QueueProcessorState {
    fn default() -> Self {
        Self::new()
    }
}
