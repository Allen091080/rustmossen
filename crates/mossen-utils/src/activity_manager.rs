//! # activity_manager — 活动追踪管理器
//!
//! 对应 TypeScript `utils/activityManager.ts`。
//! 处理通用活动追踪（用户和 CLI 操作），自动去重重叠活动并暴露活动状态。

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

/// 活动状态快照
#[derive(Debug, Clone)]
pub struct ActivityStates {
    pub is_user_active: bool,
    pub is_cli_active: bool,
    pub active_operation_count: usize,
}

/// 活动管理器选项
pub struct ActivityManagerOptions {
    /// 自定义时间获取函数（用于测试）
    pub get_now: Option<Box<dyn Fn() -> f64 + Send + Sync>>,
}

impl Default for ActivityManagerOptions {
    fn default() -> Self {
        Self { get_now: None }
    }
}

/// ActivityManager 处理通用活动追踪。
///
/// Phase F 移除了 OTel `active_time` 指标发送；管理器仍追踪时间戳，
/// 以便 `get_activity_states()`（用于 UI/闲置检测）正常工作。
pub struct ActivityManager {
    active_operations: HashSet<String>,
    last_user_activity_time: f64,
    is_cli_active: bool,
    get_now: Box<dyn Fn() -> f64 + Send + Sync>,
}

/// 用户活动超时（5秒）
const USER_ACTIVITY_TIMEOUT_MS: f64 = 5000.0;

impl ActivityManager {
    /// 创建新的 ActivityManager 实例
    pub fn new(options: Option<ActivityManagerOptions>) -> Self {
        let get_now = match options {
            Some(opts) => opts.get_now.unwrap_or_else(|| {
                Box::new(|| {
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as f64
                })
            }),
            None => Box::new(|| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as f64
            }),
        };
        Self {
            active_operations: HashSet::new(),
            last_user_activity_time: 0.0,
            is_cli_active: false,
            get_now,
        }
    }

    /// 记录用户活动
    pub fn record_user_activity(&mut self) {
        self.last_user_activity_time = (self.get_now)();
    }

    /// 开始追踪 CLI 活动（工具执行、AI 响应等）
    pub fn start_cli_activity(&mut self, operation_id: &str) {
        if self.active_operations.contains(operation_id) {
            self.end_cli_activity(operation_id);
        }

        let was_empty = self.active_operations.is_empty();
        self.active_operations.insert(operation_id.to_string());

        if was_empty {
            self.is_cli_active = true;
        }
    }

    /// 停止追踪 CLI 活动
    pub fn end_cli_activity(&mut self, operation_id: &str) {
        self.active_operations.remove(operation_id);

        if self.active_operations.is_empty() {
            self.is_cli_active = false;
        }
    }

    /// 获取当前活动状态
    pub fn get_activity_states(&self) -> ActivityStates {
        let now = (self.get_now)();
        let time_since_user_activity = (now - self.last_user_activity_time) / 1000.0;
        let is_user_active = time_since_user_activity < USER_ACTIVITY_TIMEOUT_MS / 1000.0;

        ActivityStates {
            is_user_active,
            is_cli_active: self.is_cli_active,
            active_operation_count: self.active_operations.len(),
        }
    }
}

/// 全局单例活动管理器
static ACTIVITY_MANAGER: Lazy<Mutex<ActivityManager>> =
    Lazy::new(|| Mutex::new(ActivityManager::new(None)));

/// 获取全局活动管理器的可变引用
pub fn with_activity_manager<F, R>(f: F) -> R
where
    F: FnOnce(&mut ActivityManager) -> R,
{
    let mut mgr = ACTIVITY_MANAGER.lock();
    f(&mut mgr)
}

/// 重置全局单例（仅用于测试）
pub fn reset_activity_manager() {
    let mut mgr = ACTIVITY_MANAGER.lock();
    *mgr = ActivityManager::new(None);
}

/// 使用自定义选项创建新的全局实例（仅用于测试）
pub fn create_activity_manager_instance(options: ActivityManagerOptions) {
    let mut mgr = ACTIVITY_MANAGER.lock();
    *mgr = ActivityManager::new(Some(options));
}

/// 便捷函数：追踪一个异步操作
pub async fn track_operation<F, T>(operation_id: &str, f: F) -> T
where
    F: std::future::Future<Output = T>,
{
    with_activity_manager(|mgr| mgr.start_cli_activity(operation_id));
    let result = f.await;
    with_activity_manager(|mgr| mgr.end_cli_activity(operation_id));
    result
}
