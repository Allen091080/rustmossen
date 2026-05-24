//! Service for heap dump capture and memory diagnostics.
//!
//! Provides facilities for capturing memory diagnostics, writing heap snapshots,
//! and analyzing potential memory leaks.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;

/// Result of a heap dump operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeapDumpResult {
    pub success: bool,
    #[serde(default)]
    pub heap_path: Option<String>,
    #[serde(default)]
    pub diag_path: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

/// Memory diagnostics captured alongside heap dump.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryDiagnostics {
    pub timestamp: String,
    pub session_id: String,
    pub trigger: String,
    pub dump_number: u32,
    pub uptime_seconds: f64,
    pub memory_usage: MemoryUsageInfo,
    pub memory_growth_rate: MemoryGrowthRate,
    pub v8_heap_stats: V8HeapStats,
    #[serde(default)]
    pub v8_heap_spaces: Option<Vec<HeapSpaceInfo>>,
    pub resource_usage: ResourceUsageInfo,
    pub active_handles: usize,
    pub active_requests: usize,
    #[serde(default)]
    pub open_file_descriptors: Option<usize>,
    pub analysis: AnalysisResult,
    #[serde(default)]
    pub smaps_rollup: Option<String>,
    pub platform: String,
    pub node_version: String,
    pub cc_version: String,
}

/// Memory usage breakdown.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryUsageInfo {
    pub heap_used: u64,
    pub heap_total: u64,
    pub external: u64,
    pub array_buffers: u64,
    pub rss: u64,
}

/// Memory growth rate metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryGrowthRate {
    pub bytes_per_second: f64,
    pub mb_per_hour: f64,
}

/// V8 heap statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct V8HeapStats {
    pub heap_size_limit: u64,
    pub malloced_memory: u64,
    pub peak_malloced_memory: u64,
    pub detached_contexts: u64,
    pub native_contexts: u64,
}

/// Heap space information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeapSpaceInfo {
    pub name: String,
    pub size: u64,
    pub used: u64,
    pub available: u64,
}

/// Resource usage information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsageInfo {
    pub max_rss: u64,
    pub user_cpu_time: u64,
    pub system_cpu_time: u64,
}

/// Analysis results with potential leak indicators.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub potential_leaks: Vec<String>,
    pub recommendation: String,
}

/// Trigger types for heap dumps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeapDumpTrigger {
    Manual,
    Auto1_5GB,
}

impl HeapDumpTrigger {
    pub fn as_str(&self) -> &'static str {
        match self {
            HeapDumpTrigger::Manual => "manual",
            HeapDumpTrigger::Auto1_5GB => "auto-1.5GB",
        }
    }
}

/// Capture memory diagnostics for the current process.
///
/// Collects RSS, heap stats, active handles, file descriptors, and
/// provides analysis of potential memory leaks.
pub async fn capture_memory_diagnostics(
    trigger: HeapDumpTrigger,
    dump_number: u32,
    session_id: &str,
    uptime_seconds: f64,
    rss_bytes: u64,
    heap_used: u64,
    heap_total: u64,
    external_bytes: u64,
    array_buffers: u64,
    heap_size_limit: u64,
    malloced_memory: u64,
    peak_malloced_memory: u64,
    detached_contexts: u64,
    native_contexts: u64,
    active_handles: usize,
    active_requests: usize,
    max_rss: u64,
    user_cpu_time: u64,
    system_cpu_time: u64,
    cc_version: &str,
) -> MemoryDiagnostics {
    // Try to count open file descriptors (Linux)
    let open_file_descriptors = count_open_fds().await;

    // Try to read smaps_rollup (Linux)
    let smaps_rollup = read_smaps_rollup().await;

    // Calculate growth rate
    let bytes_per_second = if uptime_seconds > 0.0 {
        rss_bytes as f64 / uptime_seconds
    } else {
        0.0
    };
    let mb_per_hour = (bytes_per_second * 3600.0) / (1024.0 * 1024.0);

    // Identify potential leaks
    let native_memory = rss_bytes.saturating_sub(heap_used);
    let mut potential_leaks = Vec::new();

    if detached_contexts > 0 {
        potential_leaks.push(format!(
            "{} detached context(s) - possible iframe/context leak",
            detached_contexts
        ));
    }
    if active_handles > 100 {
        potential_leaks.push(format!(
            "{} active handles - possible timer/socket leak",
            active_handles
        ));
    }
    if native_memory > heap_used {
        potential_leaks.push(
            "Native memory > heap - leak may be in native addons (node-pty, sharp, etc.)"
                .to_string(),
        );
    }
    if mb_per_hour > 100.0 {
        potential_leaks.push(format!(
            "High memory growth rate: {:.1} MB/hour",
            mb_per_hour
        ));
    }
    if let Some(fd_count) = open_file_descriptors {
        if fd_count > 500 {
            potential_leaks.push(format!(
                "{} open file descriptors - possible file/socket leak",
                fd_count
            ));
        }
    }

    let recommendation = if !potential_leaks.is_empty() {
        format!(
            "WARNING: {} potential leak indicator(s) found. See potentialLeaks array.",
            potential_leaks.len()
        )
    } else {
        "No obvious leak indicators. Check heap snapshot for retained objects.".to_string()
    };

    MemoryDiagnostics {
        timestamp: chrono::Utc::now().to_rfc3339(),
        session_id: session_id.to_string(),
        trigger: trigger.as_str().to_string(),
        dump_number,
        uptime_seconds,
        memory_usage: MemoryUsageInfo {
            heap_used,
            heap_total,
            external: external_bytes,
            array_buffers,
            rss: rss_bytes,
        },
        memory_growth_rate: MemoryGrowthRate {
            bytes_per_second,
            mb_per_hour,
        },
        v8_heap_stats: V8HeapStats {
            heap_size_limit,
            malloced_memory,
            peak_malloced_memory,
            detached_contexts,
            native_contexts,
        },
        v8_heap_spaces: None,
        resource_usage: ResourceUsageInfo {
            max_rss,
            user_cpu_time,
            system_cpu_time,
        },
        active_handles,
        active_requests,
        open_file_descriptors,
        analysis: AnalysisResult {
            potential_leaks,
            recommendation,
        },
        smaps_rollup,
        platform: std::env::consts::OS.to_string(),
        node_version: String::new(),
        cc_version: cc_version.to_string(),
    }
}

/// Count open file descriptors (Linux only).
async fn count_open_fds() -> Option<usize> {
    match fs::read_dir("/proc/self/fd").await {
        Ok(mut entries) => {
            let mut count = 0;
            while entries.next_entry().await.ok().flatten().is_some() {
                count += 1;
            }
            Some(count)
        }
        Err(_) => None,
    }
}

/// Read Linux smaps_rollup.
async fn read_smaps_rollup() -> Option<String> {
    fs::read_to_string("/proc/self/smaps_rollup").await.ok()
}

/// Get the desktop path for dump output.
fn get_desktop_path() -> PathBuf {
    if let Some(home) = dirs::home_dir() {
        home.join("Desktop")
    } else {
        PathBuf::from(".")
    }
}

/// Perform a heap dump — captures diagnostics and writes them to ~/Desktop.
///
/// In Rust, we don't have V8 heap snapshots, so this function captures
/// memory diagnostics and writes them to a JSON file.
pub async fn perform_heap_dump(
    trigger: HeapDumpTrigger,
    dump_number: u32,
    session_id: &str,
    uptime_seconds: f64,
    rss_bytes: u64,
    heap_used: u64,
    heap_total: u64,
    external_bytes: u64,
    array_buffers: u64,
    heap_size_limit: u64,
    malloced_memory: u64,
    peak_malloced_memory: u64,
    detached_contexts: u64,
    native_contexts: u64,
    active_handles: usize,
    active_requests: usize,
    max_rss: u64,
    user_cpu_time: u64,
    system_cpu_time: u64,
    cc_version: &str,
) -> HeapDumpResult {
    let diagnostics = capture_memory_diagnostics(
        trigger,
        dump_number,
        session_id,
        uptime_seconds,
        rss_bytes,
        heap_used,
        heap_total,
        external_bytes,
        array_buffers,
        heap_size_limit,
        malloced_memory,
        peak_malloced_memory,
        detached_contexts,
        native_contexts,
        active_handles,
        active_requests,
        max_rss,
        user_cpu_time,
        system_cpu_time,
        cc_version,
    )
    .await;

    let dump_dir = get_desktop_path();
    if let Err(e) = fs::create_dir_all(&dump_dir).await {
        return HeapDumpResult {
            success: false,
            heap_path: None,
            diag_path: None,
            error: Some(format!("Failed to create dump directory: {}", e)),
        };
    }

    let suffix = if dump_number > 0 {
        format!("-dump{}", dump_number)
    } else {
        String::new()
    };
    let diag_filename = format!("{}{}-diagnostics.json", session_id, suffix);
    let diag_path = dump_dir.join(&diag_filename);

    let diag_json = match serde_json::to_string_pretty(&diagnostics) {
        Ok(j) => j,
        Err(e) => {
            return HeapDumpResult {
                success: false,
                heap_path: None,
                diag_path: None,
                error: Some(format!("Failed to serialize diagnostics: {}", e)),
            };
        }
    };

    if let Err(e) = fs::write(&diag_path, &diag_json).await {
        return HeapDumpResult {
            success: false,
            heap_path: None,
            diag_path: None,
            error: Some(format!("Failed to write diagnostics: {}", e)),
        };
    }

    HeapDumpResult {
        success: true,
        heap_path: None, // No V8 heap snapshot in Rust
        diag_path: Some(diag_path.to_string_lossy().to_string()),
        error: None,
    }
}

/// Format bytes as GB string for logging.
pub fn bytes_to_gb(bytes: u64) -> String {
    format!("{:.3}", bytes as f64 / 1024.0 / 1024.0 / 1024.0)
}
