//! # warning_handler — 警告处理器
//!
//! 对应 TypeScript `utils/warningHandler.ts`。
//! 管理进程警告的去重、过滤和日志记录。

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use regex::Regex;
use std::collections::HashMap;
use tracing::{debug, warn};

/// 警告键的最大数量，防止无限内存增长
pub const MAX_WARNING_KEYS: usize = 1000;

/// 已知应抑制的内部警告模式
static INTERNAL_WARNINGS: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        Regex::new(r"MaxListenersExceededWarning.*AbortSignal").unwrap(),
        Regex::new(r"MaxListenersExceededWarning.*EventTarget").unwrap(),
    ]
});

/// 警告计数器（按key去重）
static WARNING_COUNTS: Lazy<Mutex<HashMap<String, usize>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// 是否已安装处理器
static HANDLER_INSTALLED: Lazy<Mutex<bool>> = Lazy::new(|| Mutex::new(false));

/// 检查是否从构建目录运行（开发模式）
fn is_running_from_build_directory() -> bool {
    let invoked_path = std::env::args().nth(1).unwrap_or_default();
    let exec_path = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    let paths_to_check = [&invoked_path, &exec_path];
    let build_dirs = [
        "/build-ant/",
        "/build-external/",
        "/build-external-native/",
        "/build-ant-native/",
    ];

    paths_to_check
        .iter()
        .any(|path| build_dirs.iter().any(|dir| path.contains(dir)))
}

/// 检查警告是否为已知的内部警告
fn is_internal_warning(warning_name: &str, warning_message: &str) -> bool {
    let warning_str = format!("{}: {}", warning_name, warning_message);
    INTERNAL_WARNINGS
        .iter()
        .any(|pattern| pattern.is_match(&warning_str))
}

/// 重置警告处理器状态（仅用于测试）
pub fn reset_warning_handler() {
    let mut installed = HANDLER_INSTALLED.lock();
    *installed = false;
    let mut counts = WARNING_COUNTS.lock();
    counts.clear();
}

/// 处理单个警告事件
///
/// 记录警告、去重并根据调试模式选择性显示。
///
/// # 参数
/// - `warning_name`: 警告名称（如 "MaxListenersExceededWarning"）
/// - `warning_message`: 警告消息内容
pub fn handle_warning(warning_name: &str, warning_message: &str) {
    // Build warning key (truncate message to 50 chars)
    let truncated_msg: String = warning_message.chars().take(50).collect();
    let warning_key = format!("{}: {}", warning_name, truncated_msg);

    let mut counts = WARNING_COUNTS.lock();
    let count = counts.get(&warning_key).copied().unwrap_or(0);

    // Bound the map to prevent unbounded memory growth from unique warning keys
    if counts.contains_key(&warning_key) || counts.len() < MAX_WARNING_KEYS {
        counts.insert(warning_key.clone(), count + 1);
    }
    drop(counts);

    let is_internal = is_internal_warning(warning_name, warning_message);

    // In debug mode, show all warnings with context
    let debug_mode = std::env::var("MOSSEN_DEBUG")
        .ok()
        .map(|v| matches!(v.to_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false);

    if debug_mode {
        let prefix = if is_internal {
            "[Internal Warning]"
        } else {
            "[Warning]"
        };
        warn!("{} {}: {}", prefix, warning_name, warning_message);
    }
    // Hide all warnings from users - they are only logged for monitoring
}

/// 初始化警告处理器。
///
/// 仅设置一次处理器。对于外部用户，移除默认警告输出。
/// 对于内部用户（开发模式），保留默认警告。
pub fn initialize_warning_handler() {
    let mut installed = HANDLER_INSTALLED.lock();
    if *installed {
        return;
    }

    let is_development = std::env::var("NODE_ENV")
        .map(|v| v == "development")
        .unwrap_or(false)
        || is_running_from_build_directory();

    if !is_development {
        // For external users, suppress stderr warning output
        // In Rust, we use tracing filters instead of process.removeAllListeners
        debug!("Warning handler initialized (production mode, warnings suppressed)");
    } else {
        debug!("Warning handler initialized (development mode)");
    }

    *installed = true;
}

/// 检查是否为开发模式
pub fn is_development_mode() -> bool {
    std::env::var("NODE_ENV")
        .map(|v| v == "development")
        .unwrap_or(false)
        || is_running_from_build_directory()
}
