//! Startup profiling utility for measuring and reporting time spent in various
//! initialization phases.
//!
//! Two modes:
//! 1. Sampled logging: 100% of ant users, 0.5% of external users - logs phases
//! 2. Detailed profiling: MOSSEN_CODE_PROFILE_STARTUP=1 - full report with memory snapshots

use std::env;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

use once_cell::sync::Lazy;

/// Whether detailed profiling is enabled via env var.
static DETAILED_PROFILING: Lazy<bool> = Lazy::new(|| {
    is_env_truthy(env::var("MOSSEN_CODE_PROFILE_STARTUP").ok().as_deref())
});

fn is_env_truthy(val: Option<&str>) -> bool {
    matches!(val, Some("1") | Some("true") | Some("yes"))
}

/// Whether this session was sampled for Statsig logging.
static STATSIG_LOGGING_SAMPLED: Lazy<bool> = Lazy::new(|| {
    env::var("USER_TYPE").ok().as_deref() == Some("ant") || rand::random::<f64>() < 0.005
});

/// Whether profiling should be active.
static SHOULD_PROFILE: Lazy<bool> =
    Lazy::new(|| *DETAILED_PROFILING || *STATSIG_LOGGING_SAMPLED);

/// A profiling checkpoint entry.
#[derive(Debug, Clone)]
struct Checkpoint {
    name: String,
    elapsed_ms: f64,
    memory_rss: Option<usize>,
}

/// Internal profiler state.
struct ProfilerState {
    start: Instant,
    checkpoints: Vec<Checkpoint>,
    reported: bool,
}

static PROFILER_STATE: Lazy<Mutex<ProfilerState>> = Lazy::new(|| {
    let state = ProfilerState {
        start: Instant::now(),
        checkpoints: Vec::new(),
        reported: false,
    };
    Mutex::new(state)
});

/// Phase definitions for analytics logging: (phase_name, start_checkpoint, end_checkpoint).
const PHASE_DEFINITIONS: &[(&str, &str, &str)] = &[
    ("import_time", "cli_entry", "main_tsx_imports_loaded"),
    ("init_time", "init_function_start", "init_function_end"),
    ("settings_time", "eagerLoadSettings_start", "eagerLoadSettings_end"),
    ("total_time", "cli_entry", "main_after_run"),
];

/// Record a checkpoint with the given name.
pub fn profile_checkpoint(name: &str) {
    if !*SHOULD_PROFILE {
        return;
    }

    let mut state = PROFILER_STATE.lock().unwrap();
    let elapsed_ms = state.start.elapsed().as_secs_f64() * 1000.0;

    let memory_rss = if *DETAILED_PROFILING {
        get_rss_bytes()
    } else {
        None
    };

    state.checkpoints.push(Checkpoint {
        name: name.to_string(),
        elapsed_ms,
        memory_rss,
    });
}

/// Get a formatted report of all checkpoints.
/// Only available when DETAILED_PROFILING is enabled.
fn get_report() -> String {
    if !*DETAILED_PROFILING {
        return "Startup profiling not enabled".to_string();
    }

    let state = PROFILER_STATE.lock().unwrap();
    if state.checkpoints.is_empty() {
        return "No profiling checkpoints recorded".to_string();
    }

    let mut lines = Vec::new();
    lines.push("=".repeat(80));
    lines.push("STARTUP PROFILING REPORT".to_string());
    lines.push("=".repeat(80));
    lines.push(String::new());

    let mut prev_time = 0.0f64;
    for cp in &state.checkpoints {
        let delta = cp.elapsed_ms - prev_time;
        let mem_str = match cp.memory_rss {
            Some(rss) => format!(" [RSS: {:.1}MB]", rss as f64 / 1_048_576.0),
            None => String::new(),
        };
        lines.push(format!(
            "{:>8.1}ms (+{:>7.1}ms) {}{}",
            cp.elapsed_ms, delta, cp.name, mem_str
        ));
        prev_time = cp.elapsed_ms;
    }

    if let Some(last) = state.checkpoints.last() {
        lines.push(String::new());
        lines.push(format!("Total startup time: {:.1}ms", last.elapsed_ms));
        lines.push("=".repeat(80));
    }

    lines.join("\n")
}

/// Generate and output the profiling report.
pub fn profile_report(session_id: &str, config_home_dir: &str) {
    let mut state = PROFILER_STATE.lock().unwrap();
    if state.reported {
        return;
    }
    state.reported = true;
    drop(state);

    // Log startup perf metrics
    log_startup_perf();

    // Output detailed report if MOSSEN_CODE_PROFILE_STARTUP=1
    if *DETAILED_PROFILING {
        let path = get_startup_perf_log_path(session_id, config_home_dir);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let report = get_report();
        let _ = std::fs::write(&path, &report);
        tracing::debug!("Startup profiling report:\n{}", report);
    }
}

/// Check if detailed profiling is enabled.
pub fn is_detailed_profiling_enabled() -> bool {
    *DETAILED_PROFILING
}

/// Get the path for the startup perf log file.
pub fn get_startup_perf_log_path(session_id: &str, config_home_dir: &str) -> PathBuf {
    PathBuf::from(config_home_dir)
        .join("startup-perf")
        .join(format!("{}.txt", session_id))
}

/// Log startup performance phases to analytics.
/// Only logs if this session was sampled at startup.
pub fn log_startup_perf() {
    if !*STATSIG_LOGGING_SAMPLED {
        return;
    }

    let state = PROFILER_STATE.lock().unwrap();
    if state.checkpoints.is_empty() {
        return;
    }

    // Build checkpoint lookup
    let mut checkpoint_times = std::collections::HashMap::new();
    for cp in &state.checkpoints {
        checkpoint_times.insert(cp.name.as_str(), cp.elapsed_ms);
    }

    // Compute phase durations
    let mut metadata = std::collections::HashMap::<String, f64>::new();

    for &(phase_name, start_cp, end_cp) in PHASE_DEFINITIONS {
        if let (Some(&start_time), Some(&end_time)) =
            (checkpoint_times.get(start_cp), checkpoint_times.get(end_cp))
        {
            metadata.insert(format!("{}_ms", phase_name), (end_time - start_time).round());
        }
    }

    metadata.insert("checkpoint_count".to_string(), state.checkpoints.len() as f64);

    tracing::info!(event = "tengu_startup_perf", ?metadata);
}

/// Get current RSS in bytes (platform-specific).
fn get_rss_bytes() -> Option<usize> {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_to_string("/proc/self/statm")
            .ok()
            .and_then(|s| {
                s.split_whitespace()
                    .nth(1)
                    .and_then(|v| v.parse::<usize>().ok())
                    .map(|pages| pages * 4096)
            })
    }
    #[cfg(target_os = "macos")]
    {
        use std::mem;
        unsafe {
            let mut info: libc::mach_task_basic_info = mem::zeroed();
            let mut count = (mem::size_of::<libc::mach_task_basic_info>()
                / mem::size_of::<libc::natural_t>()) as libc::mach_msg_type_number_t;
            let kr = libc::task_info(
                libc::mach_task_self(),
                libc::MACH_TASK_BASIC_INFO,
                &mut info as *mut _ as libc::task_info_t,
                &mut count,
            );
            if kr == libc::KERN_SUCCESS {
                Some(info.resident_size as usize)
            } else {
                None
            }
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        None
    }
}
