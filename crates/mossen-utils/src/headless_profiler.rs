//! Headless mode profiling utility for measuring per-turn latency in -p (print) mode.

use std::collections::HashMap;
use std::env;
use std::sync::Mutex;
use std::time::Instant;

use once_cell::sync::Lazy;

/// Whether detailed profiling is enabled.
static DETAILED_PROFILING: Lazy<bool> =
    Lazy::new(|| is_env_truthy(env::var("MOSSEN_CODE_PROFILE_STARTUP").ok().as_deref()));

fn is_env_truthy(val: Option<&str>) -> bool {
    matches!(val, Some("1") | Some("true") | Some("yes"))
}

/// Whether sampled for Statsig logging.
static STATSIG_LOGGING_SAMPLED: Lazy<bool> = Lazy::new(|| {
    env::var("USER_TYPE").ok().as_deref() == Some("internal") || rand::random::<f64>() < 0.05
});

/// Whether profiling should be active.
static SHOULD_PROFILE: Lazy<bool> = Lazy::new(|| *DETAILED_PROFILING || *STATSIG_LOGGING_SAMPLED);

const MARK_PREFIX: &str = "headless_";

/// Profiler state for headless mode.
struct HeadlessState {
    start: Instant,
    checkpoints: Vec<(String, f64)>,
    current_turn_number: i32,
}

static STATE: Lazy<Mutex<HeadlessState>> = Lazy::new(|| {
    Mutex::new(HeadlessState {
        start: Instant::now(),
        checkpoints: Vec::new(),
        current_turn_number: -1,
    })
});

/// Start a new turn for profiling.
pub fn headless_profiler_start_turn(is_non_interactive: bool) {
    if !is_non_interactive || !*SHOULD_PROFILE {
        return;
    }

    let mut state = STATE.lock().unwrap();
    state.current_turn_number += 1;
    state.checkpoints.clear();

    let elapsed_ms = state.start.elapsed().as_secs_f64() * 1000.0;
    state
        .checkpoints
        .push(("turn_start".to_string(), elapsed_ms));

    if *DETAILED_PROFILING {
        tracing::debug!(
            "[headlessProfiler] Started turn {}",
            state.current_turn_number
        );
    }
}

/// Record a checkpoint with the given name.
pub fn headless_profiler_checkpoint(name: &str, is_non_interactive: bool) {
    if !is_non_interactive || !*SHOULD_PROFILE {
        return;
    }

    let mut state = STATE.lock().unwrap();
    let elapsed_ms = state.start.elapsed().as_secs_f64() * 1000.0;
    state.checkpoints.push((name.to_string(), elapsed_ms));

    if *DETAILED_PROFILING {
        tracing::debug!(
            "[headlessProfiler] Checkpoint: {} at {:.1}ms",
            name,
            elapsed_ms
        );
    }
}

/// Log headless latency metrics for the current turn.
pub fn log_headless_profiler_turn(is_non_interactive: bool) {
    if !is_non_interactive || !*SHOULD_PROFILE {
        return;
    }

    let state = STATE.lock().unwrap();
    if state.checkpoints.is_empty() {
        return;
    }

    // Build checkpoint lookup
    let checkpoint_times: HashMap<&str, f64> = state
        .checkpoints
        .iter()
        .map(|(name, time)| (name.as_str(), *time))
        .collect();

    let turn_start = match checkpoint_times.get("turn_start") {
        Some(&t) => t,
        None => return,
    };

    let mut metadata: HashMap<String, serde_json::Value> = HashMap::new();
    metadata.insert(
        "turn_number".to_string(),
        serde_json::Value::from(state.current_turn_number),
    );

    // Time to system message from process start (only for turn 0)
    if state.current_turn_number == 0 {
        if let Some(&time) = checkpoint_times.get("system_message_yielded") {
            metadata.insert(
                "time_to_system_message_ms".to_string(),
                serde_json::Value::from(time.round() as i64),
            );
        }
    }

    // Time to query start
    if let Some(&query_start) = checkpoint_times.get("query_started") {
        metadata.insert(
            "time_to_query_start_ms".to_string(),
            serde_json::Value::from((query_start - turn_start).round() as i64),
        );

        // Query overhead
        if let Some(&api_request) = checkpoint_times.get("api_request_sent") {
            metadata.insert(
                "query_overhead_ms".to_string(),
                serde_json::Value::from((api_request - query_start).round() as i64),
            );
        }
    }

    // Time to first response
    if let Some(&first_chunk) = checkpoint_times.get("first_chunk") {
        metadata.insert(
            "time_to_first_response_ms".to_string(),
            serde_json::Value::from((first_chunk - turn_start).round() as i64),
        );
    }

    metadata.insert(
        "checkpoint_count".to_string(),
        serde_json::Value::from(state.checkpoints.len()),
    );

    if let Ok(entrypoint) = env::var("MOSSEN_CODE_ENTRYPOINT") {
        metadata.insert(
            "entrypoint".to_string(),
            serde_json::Value::from(entrypoint),
        );
    }

    if *STATSIG_LOGGING_SAMPLED {
        tracing::info!(event = "mossen_headless_latency", ?metadata);
    }

    if *DETAILED_PROFILING {
        tracing::debug!(
            "[headlessProfiler] Turn {} metrics: {:?}",
            state.current_turn_number,
            metadata
        );
    }
}
