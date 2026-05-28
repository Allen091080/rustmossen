//! # prevent_sleep — macOS 防睡眠服务
//!
//! 对应 TS `services/preventSleep.ts`。使用 `caffeinate` 命令
//! 防止 macOS 在长时间操作期间进入休眠。
//!
//! 采用引用计数模式：每个需要保持唤醒的操作调用 `start_prevent_sleep()`，
//! 完成后调用 `stop_prevent_sleep()`。当计数归零时终止 caffeinate 进程。

use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

use tracing::debug;

// ---------------------------------------------------------------------------
// 常量
// ---------------------------------------------------------------------------

/// caffeinate 超时（秒）。进程在此时间后自动退出。
const CAFFEINATE_TIMEOUT_SECONDS: u32 = 300;

/// 重启间隔（毫秒）。在超时前重启 caffeinate。
#[allow(dead_code)]
const RESTART_INTERVAL_MS: u64 = 4 * 60 * 1000;

// ---------------------------------------------------------------------------
// 全局状态
// ---------------------------------------------------------------------------

/// 引用计数。
static REF_COUNT: AtomicU32 = AtomicU32::new(0);

/// caffeinate 进程 PID（0 表示未运行）。
static CAFFEINATE_PID: AtomicU32 = AtomicU32::new(0);

// ---------------------------------------------------------------------------
// 公开 API
// ---------------------------------------------------------------------------

/// 增加引用计数，必要时启动防睡眠。
///
/// 对应 TS `startPreventSleep()`。
pub fn start_prevent_sleep() {
    let prev = REF_COUNT.fetch_add(1, Ordering::SeqCst);
    if prev == 0 {
        spawn_caffeinate();
    }
}

/// 减少引用计数，计数归零时允许休眠。
///
/// 对应 TS `stopPreventSleep()`。
pub fn stop_prevent_sleep() {
    let prev = REF_COUNT.fetch_sub(1, Ordering::SeqCst);
    if prev <= 1 {
        REF_COUNT.store(0, Ordering::SeqCst);
        kill_caffeinate();
    }
}

/// 强制停止防睡眠（忽略引用计数）。
///
/// 对应 TS `forceStopPreventSleep()`。用于退出清理。
pub fn force_stop_prevent_sleep() {
    REF_COUNT.store(0, Ordering::SeqCst);
    kill_caffeinate();
}

/// 获取当前引用计数。
pub fn prevent_sleep_ref_count() -> u32 {
    REF_COUNT.load(Ordering::SeqCst)
}

// ---------------------------------------------------------------------------
// 内部实现
// ---------------------------------------------------------------------------

/// 启动 caffeinate 进程。
///
/// 仅在 macOS 上有效，其他平台为 no-op。
fn spawn_caffeinate() {
    if cfg!(not(target_os = "macos")) {
        return;
    }

    // 已有进程运行
    if CAFFEINATE_PID.load(Ordering::SeqCst) != 0 {
        return;
    }

    match Command::new("caffeinate")
        .args(["-i", "-t", &CAFFEINATE_TIMEOUT_SECONDS.to_string()])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(child) => {
            let pid = child.id();
            CAFFEINATE_PID.store(pid, Ordering::SeqCst);
            debug!("Started caffeinate (pid={}) to prevent sleep", pid);

            // 进程完成后自动清理 PID
            std::thread::spawn(move || {
                let mut child = child;
                let _ = child.wait();
                // 只在 PID 匹配时清零（避免清理新启动的进程）
                let _ = CAFFEINATE_PID.compare_exchange(pid, 0, Ordering::SeqCst, Ordering::SeqCst);
            });
        }
        Err(e) => {
            debug!("Failed to spawn caffeinate: {}", e);
        }
    }
}

/// 终止 caffeinate 进程。
fn kill_caffeinate() {
    let pid = CAFFEINATE_PID.swap(0, Ordering::SeqCst);
    if pid == 0 {
        return;
    }

    // 发送 SIGKILL
    #[cfg(unix)]
    {
        use nix::sys::signal::{kill, Signal};
        use nix::unistd::Pid;
        let _ = kill(Pid::from_raw(pid as i32), Signal::SIGKILL);
        debug!("Stopped caffeinate (pid={}), allowing sleep", pid);
    }

    #[cfg(not(unix))]
    {
        debug!("caffeinate kill not supported on this platform");
    }
}
