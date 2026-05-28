//! 睡眠和超时工具
//!
//! 对应 TS `sleep.ts`。

/// 异步睡眠。
pub async fn async_sleep(ms: u64) {
    tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
}
