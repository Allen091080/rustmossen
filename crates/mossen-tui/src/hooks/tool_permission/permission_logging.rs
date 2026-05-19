//! Permission logging (permissionLogging.ts).
//! Logs permission decisions for auditing.

use std::time::Instant;

#[derive(Debug, Clone)]
pub struct PermissionLogEntry {
    pub tool_name: String,
    pub decision: PermissionDecision,
    pub mode: String,
    pub timestamp: Instant,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionDecision {
    Approved,
    Denied,
    AutoApproved,
    SessionApproved,
}

#[derive(Debug, Clone)]
pub struct PermissionLoggingState {
    pub entries: Vec<PermissionLogEntry>,
    pub max_entries: usize,
}

impl PermissionLoggingState {
    pub fn new() -> Self {
        Self { entries: Vec::new(), max_entries: 1000 }
    }

    pub fn log(&mut self, tool_name: &str, decision: PermissionDecision, mode: &str, reason: Option<String>) {
        self.entries.push(PermissionLogEntry {
            tool_name: tool_name.to_string(),
            decision,
            mode: mode.to_string(),
            timestamp: Instant::now(),
            reason,
        });
        if self.entries.len() > self.max_entries {
            self.entries.remove(0);
        }
    }

    pub fn recent(&self, count: usize) -> &[PermissionLogEntry] {
        let start = self.entries.len().saturating_sub(count);
        &self.entries[start..]
    }

    pub fn denied_count(&self) -> usize {
        self.entries.iter().filter(|e| e.decision == PermissionDecision::Denied).count()
    }
}

impl Default for PermissionLoggingState {
    fn default() -> Self {
        Self::new()
    }
}
