//! # file_watcher — 文件变更 Watcher
//!
//! 对应 TS `utils/hooks/fileChangedWatcher.ts`。
//! TS `chokidar` → Rust `notify` crate 文件系统监听。
//!
//! 负责监听文件变更并触发 FileChanged / CwdChanged Hook。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use mossen_types::hooks::HookEvent;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use parking_lot::Mutex;
use tokio::sync::mpsc;
use tracing::{debug, warn};

use super::config_snapshot::HooksConfigSnapshot;

/// 文件事件类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileEventKind {
    /// 文件变更。
    Change,
    /// 文件新增。
    Add,
    /// 文件删除。
    Unlink,
}

impl std::fmt::Display for FileEventKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Change => write!(f, "change"),
            Self::Add => write!(f, "add"),
            Self::Unlink => write!(f, "unlink"),
        }
    }
}

/// 文件变更事件。
#[derive(Debug, Clone)]
pub struct FileChangeEvent {
    /// 文件路径。
    pub path: PathBuf,
    /// 事件类型。
    pub kind: FileEventKind,
}

/// 文件变更 Watcher — 监听文件系统变更。
///
/// 对应 TS `fileChangedWatcher.ts`。
/// TS `chokidar` → Rust `notify` crate。
pub struct FileChangedWatcher {
    /// 当前工作目录。
    current_cwd: Mutex<PathBuf>,
    /// 动态监听路径。
    dynamic_watch_paths: Mutex<Vec<PathBuf>>,
    /// 是否已初始化。
    initialized: Mutex<bool>,
    /// 底层 watcher（持有所有权以保持监听活跃）。
    _watcher: Mutex<Option<RecommendedWatcher>>,
    /// 事件接收器发送端。
    event_tx: mpsc::UnboundedSender<FileChangeEvent>,
    /// Hooks 配置快照，用于解析 FileChanged matcher 中的静态路径。
    config_snapshot: Mutex<Option<Arc<HooksConfigSnapshot>>>,
}

impl FileChangedWatcher {
    /// 创建新的 Watcher。
    ///
    /// 返回 (Watcher, 事件接收器)。
    pub fn new() -> (Arc<Self>, mpsc::UnboundedReceiver<FileChangeEvent>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let watcher = Arc::new(Self {
            current_cwd: Mutex::new(PathBuf::new()),
            dynamic_watch_paths: Mutex::new(Vec::new()),
            initialized: Mutex::new(false),
            _watcher: Mutex::new(None),
            event_tx: tx,
            config_snapshot: Mutex::new(None),
        });
        (watcher, rx)
    }

    /// 绑定 Hooks 配置快照——用于解析 `FileChanged` matcher 中声明的静态路径。
    ///
    /// 对应 TS `fileChangedWatcher.ts` 中 `getHooksConfigFromSnapshot()` 全局访问。
    pub fn set_config_snapshot(&self, snapshot: Arc<HooksConfigSnapshot>) {
        *self.config_snapshot.lock() = Some(snapshot);
    }

    /// 从配置快照解析 `FileChanged` matcher 字段中声明的静态路径。
    ///
    /// 对应 TS `resolveWatchPaths()` 中 `staticPaths` 部分：
    /// matcher 字段以 `|` 分隔（例如 `".envrc|.env"`），每个文件名相对 `cwd`
    /// 解析（除非已是绝对路径）。
    fn static_paths_from_snapshot(&self) -> Vec<PathBuf> {
        let snapshot_guard = self.config_snapshot.lock();
        let Some(snapshot) = snapshot_guard.as_ref() else {
            return Vec::new();
        };
        let Some(settings) = snapshot.get() else {
            return Vec::new();
        };
        let cwd = self.current_cwd.lock().clone();
        let mut paths = Vec::new();
        if let Some(matchers) = settings.get(&HookEvent::FileChanged) {
            for matcher in matchers {
                let Some(expr) = matcher.matcher.as_deref() else {
                    continue;
                };
                for name in expr.split('|').map(str::trim).filter(|s| !s.is_empty()) {
                    let path = Path::new(name);
                    paths.push(if path.is_absolute() {
                        path.to_path_buf()
                    } else {
                        cwd.join(path)
                    });
                }
            }
        }
        paths
    }

    /// 初始化文件变更 Watcher。
    ///
    /// 对应 TS `initializeFileChangedWatcher()`。
    pub fn initialize(&self, cwd: &Path, static_paths: Vec<PathBuf>) -> Result<(), String> {
        let mut init = self.initialized.lock();
        if *init {
            return Ok(());
        }
        *init = true;
        *self.current_cwd.lock() = cwd.to_path_buf();

        let all_paths = self.resolve_watch_paths(&static_paths);
        if all_paths.is_empty() {
            return Ok(());
        }

        self.start_watching(&all_paths)
    }

    /// 更新监听路径。
    ///
    /// 对应 TS `updateWatchPaths()`。
    pub fn update_watch_paths(&self, paths: Vec<PathBuf>) {
        if !*self.initialized.lock() {
            return;
        }

        let mut sorted = paths.clone();
        sorted.sort();

        let current = self.dynamic_watch_paths.lock();
        let mut current_sorted: Vec<PathBuf> = current.clone();
        current_sorted.sort();

        if sorted == current_sorted {
            return;
        }
        drop(current);

        *self.dynamic_watch_paths.lock() = paths;
        let _ = self.restart_watching();
    }

    /// 处理工作目录变更。
    ///
    /// 对应 TS `onCwdChangedForHooks()`。
    pub fn on_cwd_changed(&self, _old_cwd: &Path, new_cwd: &Path) -> Result<(), String> {
        *self.current_cwd.lock() = new_cwd.to_path_buf();

        if *self.initialized.lock() {
            self.restart_watching()?;
        }
        Ok(())
    }

    /// 销毁 Watcher。
    pub fn dispose(&self) {
        *self._watcher.lock() = None;
        self.dynamic_watch_paths.lock().clear();
        *self.initialized.lock() = false;
    }

    /// 解析监听路径（合并静态和动态路径）。
    fn resolve_watch_paths(&self, static_paths: &[PathBuf]) -> Vec<PathBuf> {
        let dynamic = self.dynamic_watch_paths.lock();
        let mut all: Vec<PathBuf> = static_paths.to_vec();
        all.extend(dynamic.iter().cloned());

        // 去重
        all.sort();
        all.dedup();
        all
    }

    /// 开始监听文件。
    fn start_watching(&self, paths: &[PathBuf]) -> Result<(), String> {
        debug!(path_count = paths.len(), "Starting file watcher");

        let tx = self.event_tx.clone();
        let watcher =
            notify::recommended_watcher(move |res: Result<Event, notify::Error>| match res {
                Ok(event) => {
                    let kind = match event.kind {
                        EventKind::Create(_) => FileEventKind::Add,
                        EventKind::Modify(_) => FileEventKind::Change,
                        EventKind::Remove(_) => FileEventKind::Unlink,
                        _ => return,
                    };
                    for path in event.paths {
                        let _ = tx.send(FileChangeEvent { path, kind });
                    }
                }
                Err(e) => {
                    warn!("File watcher error: {e}");
                }
            })
            .map_err(|e| format!("Failed to create file watcher: {e}"))?;

        *self._watcher.lock() = Some(watcher);

        // 注册监听路径
        let mut w = self._watcher.lock();
        if let Some(ref mut watcher) = *w {
            for path in paths {
                if let Err(e) = watcher.watch(path, RecursiveMode::NonRecursive) {
                    warn!(path = %path.display(), "Failed to watch path: {e}");
                }
            }
        }

        Ok(())
    }

    /// 重启监听。
    fn restart_watching(&self) -> Result<(), String> {
        *self._watcher.lock() = None;

        let static_paths = self.static_paths_from_snapshot();
        let all_paths = self.resolve_watch_paths(&static_paths);
        if !all_paths.is_empty() {
            self.start_watching(&all_paths)?;
        }
        Ok(())
    }
}
