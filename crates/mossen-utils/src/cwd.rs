//! # cwd — 工作目录管理
//!
//! 对应 TypeScript `utils/cwd.ts`。

use std::path::PathBuf;
use std::sync::Mutex;
use tokio::task_local;

task_local! {
    static CWD_OVERRIDE: String;
}

static GLOBAL_CWD: Mutex<Option<String>> = Mutex::new(None);
static ORIGINAL_CWD: Mutex<Option<String>> = Mutex::new(None);

/// 初始化全局 CWD。
pub fn init_cwd(cwd: &str) {
    let mut global = GLOBAL_CWD.lock().unwrap();
    *global = Some(cwd.to_string());
    let mut original = ORIGINAL_CWD.lock().unwrap();
    if original.is_none() {
        *original = Some(cwd.to_string());
    }
}

/// 设置全局 CWD。
pub fn set_cwd(cwd: &str) {
    let mut global = GLOBAL_CWD.lock().unwrap();
    *global = Some(cwd.to_string());
}

/// 在覆盖的工作目录下运行函数（用于并发 agent）。
pub async fn run_with_cwd_override<F, T>(cwd: String, f: F) -> T
where
    F: std::future::Future<Output = T>,
{
    CWD_OVERRIDE.scope(cwd, f).await
}

/// 获取当前工作目录。
pub fn pwd() -> String {
    CWD_OVERRIDE
        .try_with(|s| s.clone())
        .unwrap_or_else(|_| get_cwd_state())
}

/// 获取当前工作目录，失败时回退到原始工作目录。
pub fn get_cwd() -> String {
    let result = std::panic::catch_unwind(|| pwd());
    match result {
        Ok(cwd) => cwd,
        Err(_) => get_original_cwd(),
    }
}

fn get_cwd_state() -> String {
    let global = GLOBAL_CWD.lock().unwrap();
    global
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap().to_string_lossy().to_string())
}

fn get_original_cwd() -> String {
    let original = ORIGINAL_CWD.lock().unwrap();
    original
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap().to_string_lossy().to_string())
}

/// Restore the working directory from the `MOSSENSRC_LAUNCH_CWD` env var if it
/// was set by a launcher wrapper, then clear the variable so child processes
/// don't see it. Matches the `restoreLaunchCwd` helper in `entrypoints/cli.tsx`.
pub fn restore_launch_cwd() {
    let Ok(launch_cwd) = std::env::var("MOSSENSRC_LAUNCH_CWD") else {
        return;
    };
    if launch_cwd.is_empty() {
        std::env::remove_var("MOSSENSRC_LAUNCH_CWD");
        return;
    }
    // We always remove the env var even if chdir fails — the variable is a
    // one-shot launcher signal and must not leak into nested subprocess calls.
    let _ = std::env::set_current_dir(&launch_cwd);
    std::env::remove_var("MOSSENSRC_LAUNCH_CWD");
}
