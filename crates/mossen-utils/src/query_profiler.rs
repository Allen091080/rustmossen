//! Query profiling utility for measuring pipeline timing.
//!
//! Tracks checkpoints from user input to first token arrival.
//! Enable by setting MOSSEN_CODE_PROFILE_QUERY=1.

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::time::Instant;

/// Whether profiling is enabled.
static ENABLED: Lazy<bool> = Lazy::new(|| {
    std::env::var("MOSSEN_CODE_PROFILE_QUERY")
        .map(|v| matches!(v.as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
});

/// A checkpoint mark.
#[derive(Debug, Clone)]
struct Mark {
    name: String,
    time: Instant,
    memory_rss: Option<usize>,
}

/// Profiling session state.
struct ProfileState {
    marks: Vec<Mark>,
    query_count: u64,
    first_token_time: Option<f64>,
    baseline: Option<Instant>,
}

static STATE: Lazy<Mutex<ProfileState>> = Lazy::new(|| {
    Mutex::new(ProfileState {
        marks: Vec::new(),
        query_count: 0,
        first_token_time: None,
        baseline: None,
    })
});

/// Start profiling a new query session.
pub fn start_query_profile() {
    if !*ENABLED {
        return;
    }
    let mut state = STATE.lock();
    state.marks.clear();
    state.first_token_time = None;
    state.query_count += 1;
    state.baseline = Some(Instant::now());
    drop(state);
    query_checkpoint("query_user_input_received");
}

/// Record a checkpoint with the given name.
pub fn query_checkpoint(name: &str) {
    if !*ENABLED {
        return;
    }
    let now = Instant::now();
    let memory_rss = get_memory_rss();

    let mut state = STATE.lock();
    state.marks.push(Mark {
        name: name.to_string(),
        time: now,
        memory_rss,
    });

    if name == "query_first_chunk_received" && state.first_token_time.is_none() {
        if let Some(baseline) = state.baseline {
            state.first_token_time = Some(now.duration_since(baseline).as_secs_f64() * 1000.0);
        }
    }
}

/// End the current query profiling session.
pub fn end_query_profile() {
    if !*ENABLED {
        return;
    }
    query_checkpoint("query_profile_end");
}

/// Get memory RSS (platform-specific).
fn get_memory_rss() -> Option<usize> {
    #[cfg(target_os = "macos")]
    {
        // Use mach API or proc_pid_rusage on macOS
        None // Simplified
    }
    #[cfg(target_os = "linux")]
    {
        std::fs::read_to_string("/proc/self/statm")
            .ok()
            .and_then(|s| s.split_whitespace().nth(1)?.parse::<usize>().ok())
            .map(|pages| pages * 4096)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

/// Identify slow operations.
fn get_slow_warning(delta_ms: f64, name: &str) -> &'static str {
    if name == "query_user_input_received" {
        return "";
    }
    if delta_ms > 1000.0 {
        return " ⚠️  VERY SLOW";
    }
    if delta_ms > 100.0 {
        return " ⚠️  SLOW";
    }
    if name.contains("git_status") && delta_ms > 50.0 {
        return " ⚠️  git status";
    }
    if name.contains("tool_schema") && delta_ms > 50.0 {
        return " ⚠️  tool schemas";
    }
    if name.contains("client_creation") && delta_ms > 50.0 {
        return " ⚠️  client creation";
    }
    ""
}

/// Format milliseconds.
fn format_ms(ms: f64) -> String {
    format!("{:.1}", ms)
}

/// Phase definition for summary.
struct Phase {
    name: &'static str,
    start: &'static str,
    end: &'static str,
}

const PHASES: &[Phase] = &[
    Phase {
        name: "Context loading",
        start: "query_context_loading_start",
        end: "query_context_loading_end",
    },
    Phase {
        name: "Microcompact",
        start: "query_microcompact_start",
        end: "query_microcompact_end",
    },
    Phase {
        name: "Autocompact",
        start: "query_autocompact_start",
        end: "query_autocompact_end",
    },
    Phase {
        name: "Query setup",
        start: "query_setup_start",
        end: "query_setup_end",
    },
    Phase {
        name: "Tool schemas",
        start: "query_tool_schema_build_start",
        end: "query_tool_schema_build_end",
    },
    Phase {
        name: "Message normalization",
        start: "query_message_normalization_start",
        end: "query_message_normalization_end",
    },
    Phase {
        name: "Client creation",
        start: "query_client_creation_start",
        end: "query_client_creation_end",
    },
    Phase {
        name: "Network TTFB",
        start: "query_api_request_sent",
        end: "query_first_chunk_received",
    },
    Phase {
        name: "Tool execution",
        start: "query_tool_execution_start",
        end: "query_tool_execution_end",
    },
];

/// Get phase-based summary.
fn get_phase_summary(marks: &[Mark], baseline: Instant) -> String {
    let mark_map: HashMap<&str, f64> = marks
        .iter()
        .map(|m| {
            (
                m.name.as_str(),
                m.time.duration_since(baseline).as_secs_f64() * 1000.0,
            )
        })
        .collect();

    let mut lines = Vec::new();
    lines.push(String::new());
    lines.push("PHASE BREAKDOWN:".to_string());

    for phase in PHASES {
        if let (Some(&start_time), Some(&end_time)) =
            (mark_map.get(phase.start), mark_map.get(phase.end))
        {
            let duration = end_time - start_time;
            let bar_len = ((duration / 10.0).ceil() as usize).min(50);
            let bar = "█".repeat(bar_len);
            lines.push(format!(
                "  {:22} {:>10}ms {}",
                phase.name,
                format_ms(duration),
                bar
            ));
        }
    }

    if let Some(&api_request_sent) = mark_map.get("query_api_request_sent") {
        lines.push(String::new());
        lines.push(format!(
            "  {:22} {:>10}ms",
            "Total pre-API overhead",
            format_ms(api_request_sent)
        ));
    }

    lines.join("\n")
}

/// Get a formatted report of all checkpoints.
pub fn get_query_profile_report() -> String {
    if !*ENABLED {
        return "Query profiling not enabled (set MOSSEN_CODE_PROFILE_QUERY=1)".to_string();
    }

    let state = STATE.lock();
    if state.marks.is_empty() {
        return "No query profiling checkpoints recorded".to_string();
    }

    let baseline = match state.baseline {
        Some(b) => b,
        None => return "No baseline recorded".to_string(),
    };

    let mut lines = Vec::new();
    lines.push("=".repeat(80));
    lines.push(format!(
        "QUERY PROFILING REPORT - Query #{}",
        state.query_count
    ));
    lines.push("=".repeat(80));
    lines.push(String::new());

    let mut prev_time = baseline;
    let mut api_request_sent_time: f64 = 0.0;
    let mut first_chunk_time: f64 = 0.0;

    for mark in &state.marks {
        let relative_time = mark.time.duration_since(baseline).as_secs_f64() * 1000.0;
        let delta_ms = mark.time.duration_since(prev_time).as_secs_f64() * 1000.0;
        let warning = get_slow_warning(delta_ms, &mark.name);

        let mem_str = match mark.memory_rss {
            Some(rss) => format!(" [{:.1}MB]", rss as f64 / 1_048_576.0),
            None => String::new(),
        };

        lines.push(format!(
            "{:>10}ms (+{:>9}ms) {}{}{}",
            format_ms(relative_time),
            format_ms(delta_ms),
            mark.name,
            mem_str,
            warning
        ));

        if mark.name == "query_api_request_sent" {
            api_request_sent_time = relative_time;
        }
        if mark.name == "query_first_chunk_received" {
            first_chunk_time = relative_time;
        }
        prev_time = mark.time;
    }

    lines.push(String::new());
    lines.push("-".repeat(80));

    if first_chunk_time > 0.0 {
        let pre_request_overhead = api_request_sent_time;
        let network_latency = first_chunk_time - api_request_sent_time;
        let pre_request_percent = (pre_request_overhead / first_chunk_time) * 100.0;
        let network_percent = (network_latency / first_chunk_time) * 100.0;

        lines.push(format!("Total TTFT: {}ms", format_ms(first_chunk_time)));
        lines.push(format!(
            "  - Pre-request overhead: {}ms ({:.1}%)",
            format_ms(pre_request_overhead),
            pre_request_percent
        ));
        lines.push(format!(
            "  - Network latency: {}ms ({:.1}%)",
            format_ms(network_latency),
            network_percent
        ));
    } else {
        let last_mark = state.marks.last().unwrap();
        let total_time = last_mark.time.duration_since(baseline).as_secs_f64() * 1000.0;
        lines.push(format!("Total time: {}ms", format_ms(total_time)));
    }

    lines.push(get_phase_summary(&state.marks, baseline));
    lines.push("=".repeat(80));

    lines.join("\n")
}

/// Log the query profile report to debug output.
pub fn log_query_profile_report() {
    if !*ENABLED {
        return;
    }
    let report = get_query_profile_report();
    tracing::debug!("{}", report);
}
