//! # combined_abort_signal — 组合中止信号
//!
//! 对应 TypeScript `utils/combinedAbortSignal.ts`。

use tokio::time::{sleep, Duration};
use tokio_util::sync::CancellationToken;

/// 创建组合中止信号。
///
/// 当输入信号中止、可选的第二信号中止、或可选超时到达时中止。
/// 返回信号和清理函数。
pub struct CombinedAbortSignal {
    pub token: CancellationToken,
    _guards: Vec<tokio::task::JoinHandle<()>>,
}

impl CombinedAbortSignal {
    /// 丢弃时自动清理后台任务。
    pub fn cancel(&self) {
        self.token.cancel();
    }
}

/// 创建组合中止信号。
pub fn create_combined_abort_signal(
    signal: Option<CancellationToken>,
    signal_b: Option<CancellationToken>,
    timeout_ms: Option<u64>,
) -> CombinedAbortSignal {
    let combined = CancellationToken::new();
    let mut guards = Vec::new();

    // 检查是否已经中止
    if signal.as_ref().map(|s| s.is_cancelled()).unwrap_or(false)
        || signal_b.as_ref().map(|s| s.is_cancelled()).unwrap_or(false)
    {
        combined.cancel();
        return CombinedAbortSignal {
            token: combined,
            _guards: guards,
        };
    }

    // 监听第一个信号
    if let Some(sig) = signal {
        let combined_clone = combined.clone();
        let handle = tokio::spawn(async move {
            sig.cancelled().await;
            combined_clone.cancel();
        });
        guards.push(handle);
    }

    // 监听第二个信号
    if let Some(sig_b) = signal_b {
        let combined_clone = combined.clone();
        let handle = tokio::spawn(async move {
            sig_b.cancelled().await;
            combined_clone.cancel();
        });
        guards.push(handle);
    }

    // 超时
    if let Some(ms) = timeout_ms {
        let combined_clone = combined.clone();
        let handle = tokio::spawn(async move {
            sleep(Duration::from_millis(ms)).await;
            combined_clone.cancel();
        });
        guards.push(handle);
    }

    CombinedAbortSignal {
        token: combined,
        _guards: guards,
    }
}
