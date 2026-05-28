//! Memory usage hook (useMemoryUsage.ts).
//!
//! Monitors process memory usage and reports when thresholds are exceeded.

/// Memory usage status level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryUsageStatus {
    Normal,
    High,
    Critical,
}

/// Memory usage information.
#[derive(Debug, Clone)]
pub struct MemoryUsageInfo {
    pub heap_used: u64,
    pub status: MemoryUsageStatus,
}

/// Thresholds for memory monitoring.
const HIGH_MEMORY_THRESHOLD: u64 = 1_500_000_000; // 1.5GB
const CRITICAL_MEMORY_THRESHOLD: u64 = 2_500_000_000; // 2.5GB

/// State for memory usage monitoring.
#[derive(Debug, Clone)]
pub struct MemoryUsageState {
    pub current: Option<MemoryUsageInfo>,
    pub poll_interval_ms: u64,
    pub last_poll_ms: Option<u64>,
}

impl MemoryUsageState {
    pub fn new() -> Self {
        Self {
            current: None,
            poll_interval_ms: 10_000,
            last_poll_ms: None,
        }
    }

    /// Update with a new memory reading.
    pub fn update(&mut self, heap_used: u64) {
        let status = if heap_used >= CRITICAL_MEMORY_THRESHOLD {
            MemoryUsageStatus::Critical
        } else if heap_used >= HIGH_MEMORY_THRESHOLD {
            MemoryUsageStatus::High
        } else {
            MemoryUsageStatus::Normal
        };

        // Only store non-normal readings to avoid unnecessary re-renders
        self.current = if status == MemoryUsageStatus::Normal {
            None
        } else {
            Some(MemoryUsageInfo { heap_used, status })
        };
    }

    /// Get the current status.
    pub fn status(&self) -> MemoryUsageStatus {
        self.current
            .as_ref()
            .map_or(MemoryUsageStatus::Normal, |i| i.status)
    }

    /// Check if memory is in a warning state.
    pub fn is_warning(&self) -> bool {
        matches!(
            self.status(),
            MemoryUsageStatus::High | MemoryUsageStatus::Critical
        )
    }
}

impl Default for MemoryUsageState {
    fn default() -> Self {
        Self::new()
    }
}
