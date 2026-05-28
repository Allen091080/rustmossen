//! # idle_timeout — 空闲超时管理器
//!
//! 对应 TypeScript `utils/idleTimeout.ts`。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::time::{sleep, Duration};

/// 空闲超时管理器。
pub struct IdleTimeoutManager {
    running: Arc<AtomicBool>,
    delay_ms: Option<u64>,
    is_idle: Arc<dyn Fn() -> bool + Send + Sync>,
}

impl IdleTimeoutManager {
    /// 创建一个空闲超时管理器。
    pub fn new(is_idle: impl Fn() -> bool + Send + Sync + 'static) -> Self {
        let delay_ms = std::env::var("MOSSEN_CODE_EXIT_AFTER_STOP_DELAY")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|&d| d > 0);

        Self {
            running: Arc::new(AtomicBool::new(false)),
            delay_ms,
            is_idle: Arc::new(is_idle),
        }
    }

    /// 启动空闲计时器。
    pub fn start(&self) {
        self.running.store(true, Ordering::SeqCst);

        if let Some(delay) = self.delay_ms {
            let running = self.running.clone();
            let is_idle = self.is_idle.clone();
            tokio::spawn(async move {
                sleep(Duration::from_millis(delay)).await;
                if running.load(Ordering::SeqCst) && (is_idle)() {
                    tracing::debug!("Exiting after {}ms of idle time", delay);
                    std::process::exit(0);
                }
            });
        }
    }

    /// 停止空闲计时器。
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }
}
