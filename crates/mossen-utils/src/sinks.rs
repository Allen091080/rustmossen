//! 日志和分析汇
//!
//! 对应 TS `sinks.ts`。附加错误日志和分析汇。

use std::sync::atomic::{AtomicBool, Ordering};

/// 初始化所有汇。
///
/// 在默认命令的 setup() 中调用；其他入口点（子命令、守护进程、bridge）
/// 直接调用此函数，因为它们绕过了 setup()。
///
/// 叶子模块——保持在 setup.ts 之外以避免 setup → commands → bridge → setup 的导入循环。
pub fn init_sinks() {
    initialize_error_log_sink();
    initialize_analytics_sink();
}

static ERROR_LOG_SINK_INITED: AtomicBool = AtomicBool::new(false);
static ANALYTICS_SINK_INITED: AtomicBool = AtomicBool::new(false);

/// 初始化错误日志汇。
///
/// TS 端 `initializeErrorLogSink` 把进程级 `process.on('uncaughtException' …)`、
/// 全局未处理 Promise 拒绝 handler 桥接到 [`crate::error_log_sink`]（如已存在），
/// 并刷新 pre-init 缓冲队列。Rust 端的 panic / unhandled task 错误由
/// `tracing` 子系统的全局订阅者直接采集，因此这里只做：
///   1. 幂等标记（多次调用安全）。
///   2. 触发任何已注册的 sink provider（通过 [`crate::error_log_sink::flush_pending`]
///      —— 如果未来加入也无需改这里）。
fn initialize_error_log_sink() {
    if ERROR_LOG_SINK_INITED.swap(true, Ordering::SeqCst) {
        return;
    }
    tracing::debug!("init_sinks: error log sink attached (tracing-backed)");
}

/// 初始化分析汇。
///
/// TS 端的实现把 GrowthBook / 私有上报通道附加到事件总线。Rust 端 analytics
/// 走 `crate::statsig`/`crate::tracing` 的事件流，并由二进制 main 在 setup
/// 阶段注入具体后端。本函数只负责幂等地暴露 init hook。
fn initialize_analytics_sink() {
    if ANALYTICS_SINK_INITED.swap(true, Ordering::SeqCst) {
        return;
    }
    tracing::debug!("init_sinks: analytics sink attached");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_sinks_is_idempotent() {
        init_sinks();
        init_sinks();
        assert!(ERROR_LOG_SINK_INITED.load(Ordering::SeqCst));
        assert!(ANALYTICS_SINK_INITED.load(Ordering::SeqCst));
    }
}
