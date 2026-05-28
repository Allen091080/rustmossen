//! 信号处理 — SIGINT / SIGTERM 的 graceful shutdown。
//!
//! 对应 TS 的 entrypoints/init.ts 中的 gracefulShutdown() 逻辑。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Notify;
use tracing::{info, warn};

/// 全局关闭信号。
///
/// 在首次收到 SIGINT 时执行 graceful shutdown，
/// 在连续收到多次时强制退出。
pub struct ShutdownSignal {
    /// 是否已请求关闭。
    shutdown_requested: Arc<AtomicBool>,
    /// 关闭通知器（唤醒等待者）。
    notify: Arc<Notify>,
    /// SIGINT 连续计数。
    interrupt_count: Arc<std::sync::atomic::AtomicU32>,
}

impl ShutdownSignal {
    /// 创建新的关闭信号处理器。
    pub fn new() -> Self {
        Self {
            shutdown_requested: Arc::new(AtomicBool::new(false)),
            notify: Arc::new(Notify::new()),
            interrupt_count: Arc::new(std::sync::atomic::AtomicU32::new(0)),
        }
    }

    /// 是否已请求关闭。
    pub fn is_shutdown_requested(&self) -> bool {
        self.shutdown_requested.load(Ordering::Relaxed)
    }

    /// 等待关闭信号。
    pub async fn wait_for_shutdown(&self) {
        self.notify.notified().await;
    }

    /// 获取可克隆的关闭标志（用于传递给 Agent 循环等）。
    pub fn shutdown_flag(&self) -> Arc<AtomicBool> {
        self.shutdown_requested.clone()
    }

    /// 安装系统信号处理器。
    ///
    /// - 第 1 次 SIGINT → 设置 shutdown_requested、通知等待者
    /// - 第 2 次 SIGINT → 打印警告
    /// - 第 3+ 次 SIGINT → 强制退出
    /// - SIGTERM → 立即 graceful shutdown
    pub fn install_handlers(&self) {
        let shutdown = self.shutdown_requested.clone();
        let notify = self.notify.clone();
        let count = self.interrupt_count.clone();

        // SIGINT (Ctrl+C) 处理
        tokio::spawn(async move {
            loop {
                match tokio::signal::ctrl_c().await {
                    Ok(()) => {
                        let prev = count.fetch_add(1, Ordering::SeqCst);
                        match prev {
                            0 => {
                                info!("received SIGINT, initiating graceful shutdown...");
                                shutdown.store(true, Ordering::SeqCst);
                                notify.notify_waiters();
                            }
                            1 => {
                                warn!(
                                    "received second SIGINT, press Ctrl+C once more to force exit"
                                );
                            }
                            _ => {
                                warn!("received third SIGINT, forcing immediate exit");
                                std::process::exit(130);
                            }
                        }
                    }
                    Err(e) => {
                        warn!("failed to listen for SIGINT: {}", e);
                        break;
                    }
                }
            }
        });

        // SIGTERM 处理（Unix only）
        #[cfg(unix)]
        {
            let shutdown = self.shutdown_requested.clone();
            let notify = self.notify.clone();
            tokio::spawn(async move {
                let mut sigterm =
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                        .expect("failed to install SIGTERM handler");
                sigterm.recv().await;
                info!("received SIGTERM, initiating graceful shutdown...");
                shutdown.store(true, Ordering::SeqCst);
                notify.notify_waiters();
            });
        }
    }
}

impl Default for ShutdownSignal {
    fn default() -> Self {
        Self::new()
    }
}
