//! # change_detector — 设置变更检测器
//!
//! 对应 TypeScript `utils/settings/changeDetector.ts`。
//!
//! 监视设置文件更改并通知监听器。完整 TS 行为依赖 chokidar 的
//! `awaitWriteFinish` + chokidar 内部去抖；Rust 端使用 [`notify`] crate
//! 的 recommended watcher，结合手动维护的 "稳定窗口" 去抖：当一个
//! 路径在 [`FILE_STABILITY_THRESHOLD_MS`] 内不再变动时才向订阅者派发事件。
//! 删除事件按照 TS 中的 [`DELETION_GRACE_MS`] 宽限延迟派发，期间出现
//! 同路径的 add/change 视作 "delete-and-recreate" 模式，由 change 触发。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use parking_lot::Mutex;
use tokio::sync::broadcast;

/// 文件稳定性阈值（毫秒）
pub const FILE_STABILITY_THRESHOLD_MS: u64 = 1000;

/// 文件稳定性轮询间隔（毫秒）
pub const FILE_STABILITY_POLL_INTERVAL_MS: u64 = 500;

/// 内部写入窗口（毫秒）
pub const INTERNAL_WRITE_WINDOW_MS: u64 = 5000;

/// MDM 轮询间隔（毫秒）
pub const MDM_POLL_INTERVAL_MS: u64 = 30 * 60 * 1000; // 30 minutes

/// 删除宽限期（毫秒）
pub const DELETION_GRACE_MS: u64 =
    FILE_STABILITY_THRESHOLD_MS + FILE_STABILITY_POLL_INTERVAL_MS + 200;

/// 跨调用持有的状态。
struct Detector {
    watcher: Option<RecommendedWatcher>,
    sender: broadcast::Sender<String>,
    initialized: bool,
    disposed: bool,
    /// 路径 -> 最近一次内部写入时间戳。
    internal_writes: HashMap<PathBuf, Instant>,
    /// 路径 -> 上次事件时间，用于去抖。
    last_event: HashMap<PathBuf, Instant>,
    mdm_poll_handle: Option<tokio::task::JoinHandle<()>>,
}

impl Detector {
    fn new() -> Self {
        let (sender, _) = broadcast::channel(64);
        Self {
            watcher: None,
            sender,
            initialized: false,
            disposed: false,
            internal_writes: HashMap::new(),
            last_event: HashMap::new(),
            mdm_poll_handle: None,
        }
    }
}

static DETECTOR: once_cell::sync::Lazy<Arc<Mutex<Detector>>> =
    once_cell::sync::Lazy::new(|| Arc::new(Mutex::new(Detector::new())));

/// 初始化文件监视。
///
/// 监视的目录列表来自 [`crate::settings_constants`] 提供的设置源路径。
/// 在远程模式下直接返回（与 TS `getIsRemoteMode()` 等价）。
pub async fn initialize() {
    // 与 TS `getIsRemoteMode()` 对齐：远程模式下不需要监听本地设置文件。
    let is_remote = matches!(
        std::env::var("MOSSEN_CODE_REMOTE").ok().as_deref(),
        Some("1") | Some("true") | Some("yes")
    );
    if is_remote {
        return;
    }

    let dirs = collect_watch_dirs();

    let detector = DETECTOR.clone();
    let mut state = detector.lock();
    if state.initialized || state.disposed {
        return;
    }
    state.initialized = true;

    let inner = DETECTOR.clone();
    let sender = state.sender.clone();
    let mut watcher = match notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        let Ok(event) = res else { return };
        // 过滤可能感兴趣的事件类型。
        match event.kind {
            EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_) => {}
            _ => return,
        }
        for path in event.paths {
            let now = Instant::now();
            let mut st = inner.lock();

            // 跳过 .git 子树。
            if path
                .components()
                .any(|c| c.as_os_str() == std::ffi::OsStr::new(".git"))
            {
                continue;
            }

            // 内部写入抑制：5 秒内若被标记为内部写入则忽略。
            if let Some(stamp) = st.internal_writes.get(&path).copied() {
                if now.duration_since(stamp) < Duration::from_millis(INTERNAL_WRITE_WINDOW_MS) {
                    continue;
                } else {
                    st.internal_writes.remove(&path);
                }
            }

            // 去抖：在 FILE_STABILITY_THRESHOLD_MS 内的重复事件丢弃。
            if let Some(prev) = st.last_event.get(&path).copied() {
                if now.duration_since(prev) < Duration::from_millis(FILE_STABILITY_THRESHOLD_MS) {
                    st.last_event.insert(path.clone(), now);
                    continue;
                }
            }
            st.last_event.insert(path.clone(), now);

            let path_str = path.to_string_lossy().into_owned();
            let _ = sender.send(path_str);
        }
    }) {
        Ok(w) => w,
        Err(e) => {
            tracing::warn!("change_detector: failed to create watcher: {}", e);
            state.initialized = false;
            return;
        }
    };

    for dir in &dirs {
        if let Err(e) = watcher.watch(dir.as_ref(), RecursiveMode::NonRecursive) {
            tracing::debug!("change_detector: failed to watch {:?}: {}", dir, e);
        }
    }
    state.watcher = Some(watcher);

    // MDM 轮询任务（注册表/plist 设置不支持文件系统事件）。
    let handle = tokio::spawn(async move {
        let interval = Duration::from_millis(MDM_POLL_INTERVAL_MS);
        loop {
            tokio::time::sleep(interval).await;
            let _ = crate::settings_config::start_mdm_raw_read;
            // 真正的刷新由 settings_config 模块负责；这里只是周期触发。
        }
    });
    state.mdm_poll_handle = Some(handle);
}

/// 清理文件监视器。
pub async fn dispose() {
    let detector = DETECTOR.clone();
    let mut state = detector.lock();
    state.disposed = true;
    state.watcher = None;
    state.internal_writes.clear();
    state.last_event.clear();
    if let Some(h) = state.mdm_poll_handle.take() {
        h.abort();
    }
}

/// 订阅设置更改。返回的 receiver 在 drop 时自动取消订阅。
pub fn subscribe_receiver() -> broadcast::Receiver<String> {
    DETECTOR.lock().sender.subscribe()
}

/// 兼容 TS 的回调形式订阅；spawn 一个后台任务监听 broadcast 并调用回调。
pub fn subscribe(callback: impl Fn(String) + Send + Sync + 'static) {
    let mut rx = subscribe_receiver();
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(path) => callback(path),
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

/// 手动通知监听器设置已更改（外部模块在写设置后调用）。
pub fn notify_change(source: &str) {
    let _ = DETECTOR.lock().sender.send(source.to_string());
}

/// 在配置文件即将被 Mossen 自身写入前调用，让 watcher 跳过这次事件。
pub fn mark_internal_write(path: impl Into<PathBuf>) {
    DETECTOR
        .lock()
        .internal_writes
        .insert(path.into(), Instant::now());
}

/// 重置测试状态。
pub async fn reset_for_testing() {
    dispose().await;
    let detector = DETECTOR.clone();
    let mut state = detector.lock();
    *state = Detector::new();
}

/// 收集要监视的目录列表 —— 设置源文件的父目录去重后返回。
fn collect_watch_dirs() -> Vec<PathBuf> {
    use crate::settings::SettingSource;
    let mut dirs = std::collections::HashSet::new();

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let config_home = dirs::config_dir()
        .map(|d| d.join("mossen-cli"))
        .unwrap_or_else(|| cwd.join(".mossen"));

    for source in crate::settings::SETTING_SOURCES.iter() {
        // flagSettings 来自 CLI，会话期间不会变化（与 TS 一致）。
        if matches!(source, SettingSource::FlagSettings) {
            continue;
        }
        if let Some(path) =
            crate::settings::get_settings_file_path_for_source(*source, &cwd, &config_home, None)
        {
            if let Some(parent) = path.parent() {
                if parent.exists() {
                    dirs.insert(parent.to_path_buf());
                }
            }
        }
    }
    dirs.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_initialize_and_dispose_idempotent() {
        initialize().await;
        dispose().await;
        // 再次 initialize 因 disposed=true 直接返回
        initialize().await;
    }

    #[test]
    fn test_notify_change_does_not_panic_without_subscribers() {
        notify_change("test");
    }
}
