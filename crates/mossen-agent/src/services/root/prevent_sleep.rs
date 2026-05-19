//! Prevent sleep — keeps macOS awake during operations using caffeinate

use std::process::Stdio;
use std::sync::atomic::{AtomicU32, Ordering};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tracing::debug;

const CAFFEINATE_TIMEOUT_SECONDS: u32 = 300;
const RESTART_INTERVAL_SECS: u64 = 4 * 60;

static REF_COUNT: AtomicU32 = AtomicU32::new(0);
static CAFFEINATE: once_cell::sync::Lazy<Mutex<Option<Child>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(None));

/// Increment ref count and start preventing sleep if needed
pub async fn start_prevent_sleep() {
    let prev = REF_COUNT.fetch_add(1, Ordering::Relaxed);
    if prev == 0 {
        spawn_caffeinate().await;
    }
}

/// Decrement ref count and allow sleep if no more work
pub async fn stop_prevent_sleep() {
    let prev = REF_COUNT.fetch_sub(1, Ordering::Relaxed);
    if prev <= 1 {
        REF_COUNT.store(0, Ordering::Relaxed);
        kill_caffeinate().await;
    }
}

/// Force stop, regardless of ref count
pub async fn force_stop_prevent_sleep() {
    REF_COUNT.store(0, Ordering::Relaxed);
    kill_caffeinate().await;
}

async fn spawn_caffeinate() {
    if cfg!(not(target_os = "macos")) {
        return;
    }

    let mut guard = CAFFEINATE.lock().await;
    if guard.is_some() {
        return;
    }

    match Command::new("caffeinate")
        .args(&["-i", "-t", &CAFFEINATE_TIMEOUT_SECONDS.to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => {
            debug!("Started caffeinate to prevent sleep");
            *guard = Some(child);
        }
        Err(e) => {
            debug!("Failed to start caffeinate: {}", e);
        }
    }
}

async fn kill_caffeinate() {
    let mut guard = CAFFEINATE.lock().await;
    if let Some(mut child) = guard.take() {
        let _ = child.kill().await;
        debug!("Stopped caffeinate, allowing sleep");
    }
}
