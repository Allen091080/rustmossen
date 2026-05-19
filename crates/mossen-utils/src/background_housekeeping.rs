//! # background_housekeeping — 后台维护任务
//!
//! 对应 TypeScript `utils/backgroundHousekeeping.ts`。

use std::env;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time;

/// 24 小时（毫秒）
const RECURRING_CLEANUP_INTERVAL_MS: u64 = 24 * 60 * 60 * 1000;

/// 10 分钟（启动后延迟执行非常慢的操作）
const DELAY_VERY_SLOW_OPERATIONS_MS: u64 = 10 * 60 * 1000;

/// 全局状态：是否交互式
static IS_INTERACTIVE: AtomicBool = AtomicBool::new(true);

/// 全局状态：最后交互时间（毫秒时间戳）
static LAST_INTERACTION_TIME: AtomicU64 = AtomicU64::new(0);

/// 设置是否为交互模式
pub fn set_is_interactive(interactive: bool) {
    IS_INTERACTIVE.store(interactive, Ordering::Relaxed);
}

/// 获取是否为交互模式
pub fn get_is_interactive() -> bool {
    IS_INTERACTIVE.load(Ordering::Relaxed)
}

/// 更新最后交互时间
pub fn update_last_interaction_time() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    LAST_INTERACTION_TIME.store(now, Ordering::Relaxed);
}

/// 获取最后交互时间
pub fn get_last_interaction_time() -> u64 {
    LAST_INTERACTION_TIME.load(Ordering::Relaxed)
}

/// 获取当前毫秒时间戳
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// 维护任务回调
pub struct HousekeepingCallbacks {
    pub init_magic_docs: Box<dyn Fn() + Send + Sync>,
    pub init_skill_improvement: Box<dyn Fn() + Send + Sync>,
    pub init_extract_memories: Box<dyn Fn() + Send + Sync>,
    pub init_auto_dream: Box<dyn Fn() + Send + Sync>,
    pub auto_update_plugins: Box<dyn Fn() + Send + Sync>,
    pub ensure_deep_link_registered: Option<Box<dyn Fn() + Send + Sync>>,
    pub cleanup_old_message_files: Box<dyn Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> + Send + Sync>,
    pub cleanup_old_versions: Box<dyn Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> + Send + Sync>,
    pub cleanup_npm_cache: Box<dyn Fn() + Send + Sync>,
    pub cleanup_old_versions_throttled: Box<dyn Fn() + Send + Sync>,
}

/// 启动后台维护任务
pub fn start_background_housekeeping(callbacks: Arc<HousekeepingCallbacks>, has_lodestone: bool) {
    (callbacks.init_magic_docs)();
    (callbacks.init_skill_improvement)();
    (callbacks.init_extract_memories)();
    (callbacks.init_auto_dream)();
    (callbacks.auto_update_plugins)();

    if has_lodestone && get_is_interactive() {
        if let Some(ref register) = callbacks.ensure_deep_link_registered {
            register();
        }
    }

    let callbacks_clone = callbacks.clone();
    let needs_cleanup = Arc::new(AtomicBool::new(true));
    let needs_cleanup_clone = needs_cleanup.clone();

    // 延迟执行非常慢的操作
    tokio::spawn(async move {
        run_very_slow_ops(callbacks_clone, needs_cleanup_clone).await;
    });

    // 对于长时间运行的会话，安排每 24 小时定期清理
    if env::var("USER_TYPE").as_deref() == Ok("ant") {
        let callbacks_recurring = callbacks.clone();
        tokio::spawn(async move {
            let mut interval =
                time::interval(Duration::from_millis(RECURRING_CLEANUP_INTERVAL_MS));
            interval.tick().await; // 第一次 tick 立即返回，跳过
            loop {
                interval.tick().await;
                (callbacks_recurring.cleanup_npm_cache)();
                (callbacks_recurring.cleanup_old_versions_throttled)();
            }
        });
    }
}

/// 运行非常慢的操作（延迟且检查用户活动）
async fn run_very_slow_ops(callbacks: Arc<HousekeepingCallbacks>, needs_cleanup: Arc<AtomicBool>) {
    time::sleep(Duration::from_millis(DELAY_VERY_SLOW_OPERATIONS_MS)).await;

    loop {
        // 如果用户在最近一分钟内有交互，延迟执行
        if get_is_interactive() && get_last_interaction_time() > now_ms() - 60_000 {
            time::sleep(Duration::from_millis(DELAY_VERY_SLOW_OPERATIONS_MS)).await;
            continue;
        }

        if needs_cleanup.load(Ordering::Relaxed) {
            needs_cleanup.store(false, Ordering::Relaxed);
            (callbacks.cleanup_old_message_files)().await;
        }

        // 再次检查用户活动
        if get_is_interactive() && get_last_interaction_time() > now_ms() - 60_000 {
            time::sleep(Duration::from_millis(DELAY_VERY_SLOW_OPERATIONS_MS)).await;
            continue;
        }

        (callbacks.cleanup_old_versions)().await;
        break;
    }
}
