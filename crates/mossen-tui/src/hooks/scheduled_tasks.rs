//! Scheduled tasks hook (useScheduledTasks.ts).
//! Manages periodic background tasks that run on a schedule.

use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct ScheduledTask {
    pub id: String,
    pub interval: Duration,
    pub last_run: Option<Instant>,
    pub enabled: bool,
    pub run_count: u64,
}

#[derive(Debug, Clone)]
pub struct ScheduledTasksState {
    pub tasks: HashMap<String, ScheduledTask>,
}

impl ScheduledTasksState {
    pub fn new() -> Self { Self { tasks: HashMap::new() } }
    pub fn register(&mut self, id: &str, interval_ms: u64) {
        self.tasks.insert(id.to_string(), ScheduledTask {
            id: id.to_string(), interval: Duration::from_millis(interval_ms),
            last_run: None, enabled: true, run_count: 0,
        });
    }
    pub fn unregister(&mut self, id: &str) { self.tasks.remove(id); }
    pub fn due_tasks(&self) -> Vec<&str> {
        self.tasks.values().filter(|t| t.enabled && t.last_run.map_or(true, |lr| lr.elapsed() >= t.interval))
            .map(|t| t.id.as_str()).collect()
    }
    pub fn mark_run(&mut self, id: &str) {
        if let Some(task) = self.tasks.get_mut(id) { task.last_run = Some(Instant::now()); task.run_count += 1; }
    }
    pub fn set_enabled(&mut self, id: &str, enabled: bool) {
        if let Some(task) = self.tasks.get_mut(id) { task.enabled = enabled; }
    }
}
impl Default for ScheduledTasksState { fn default() -> Self { Self::new() } }
