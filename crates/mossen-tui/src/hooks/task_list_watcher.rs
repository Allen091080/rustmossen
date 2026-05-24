//! Task List Watcher hook (useTaskListWatcher.ts).
//! Watches for changes to the task list and triggers updates.

#[derive(Debug, Clone)]
pub struct TaskListWatcherState {
    pub active: bool,
    pub initialized: bool,
}

impl TaskListWatcherState {
    pub fn new() -> Self {
        Self {
            active: false,
            initialized: false,
        }
    }
    pub fn initialize(&mut self) {
        self.initialized = true;
    }
    pub fn activate(&mut self) {
        self.active = true;
    }
    pub fn deactivate(&mut self) {
        self.active = false;
    }
    pub fn is_active(&self) -> bool {
        self.active
    }
}
impl Default for TaskListWatcherState {
    fn default() -> Self {
        Self::new()
    }
}
