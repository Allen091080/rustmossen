//! 查询守卫 — 查询生命周期状态机
//!
//! 对应 TS `QueryGuard`。兼容 React 的 `useSyncExternalStore`。
//!
//! 三种状态：
//! - idle: 无查询，可安全出队处理
//! - dispatching: 已出队，异步链尚未到达 onQuery
//! - running: onQuery 调用了 tryStart()，查询正在执行
//!
//! 转换规则：
//! - idle → dispatching (reserve)
//! - dispatching → running (tryStart)
//! - idle → running (tryStart，直接用户提交)
//! - running → idle (end / forceEnd)
//! - dispatching → idle (cancelReservation)

use std::sync::{Arc, Mutex};

/// 查询守卫状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QueryStatus {
    /// 空闲状态。
    #[default]
    Idle,
    /// 分派中。
    Dispatching,
    /// 运行中。
    Running,
}

type Subscriber = Arc<dyn Fn() + Send + Sync + 'static>;

/// 内部共享状态。
#[derive(Default)]
struct Inner {
    status: QueryStatus,
    generation: usize,
    subscribers: Vec<Subscriber>,
}

/// 查询生命周期状态机。
///
/// 对应 TS `QueryGuard`，使用 `Arc<Mutex<…>>` 包裹内部状态以便在异步任务
/// 之间安全共享。订阅回调被克隆出来后再调用，避免持有锁时回调重入。
#[derive(Clone, Default)]
pub struct QueryGuard {
    inner: Arc<Mutex<Inner>>,
}

impl QueryGuard {
    /// 创建新的查询守卫。
    pub fn new() -> Self {
        Self::default()
    }

    /// 为队列处理预留守卫。转换 idle → dispatching。
    /// 如果不空闲（另一个查询或分派正在进行）返回 false。
    pub fn reserve(&self) -> bool {
        let subs = {
            let mut g = self.inner.lock().unwrap();
            if g.status != QueryStatus::Idle {
                return false;
            }
            g.status = QueryStatus::Dispatching;
            g.subscribers.clone()
        };
        Self::fire(&subs);
        true
    }

    /// 当 processQueueIfReady 没有可处理项时取消预留。
    /// 转换 dispatching → idle。
    pub fn cancel_reservation(&self) {
        let subs = {
            let mut g = self.inner.lock().unwrap();
            if g.status != QueryStatus::Dispatching {
                return;
            }
            g.status = QueryStatus::Idle;
            g.subscribers.clone()
        };
        Self::fire(&subs);
    }

    /// 启动查询。成功返回代数号，已运行则返回 None。
    /// 接受来自 idle（直接用户提交）和 dispatching（队列处理器路径）的转换。
    pub fn try_start(&self) -> Option<usize> {
        let (gen, subs) = {
            let mut g = self.inner.lock().unwrap();
            if g.status == QueryStatus::Running {
                return None;
            }
            g.status = QueryStatus::Running;
            g.generation += 1;
            (g.generation, g.subscribers.clone())
        };
        Self::fire(&subs);
        Some(gen)
    }

    /// 结束查询。如果代数仍然是当前的返回 true（调用者应执行清理）。
    /// 如果新查询已启动返回 false（来自已取消查询的过期 finally 块）。
    pub fn end(&self, generation: usize) -> bool {
        let subs = {
            let mut g = self.inner.lock().unwrap();
            if g.generation != generation {
                return false;
            }
            if g.status != QueryStatus::Running {
                return false;
            }
            g.status = QueryStatus::Idle;
            g.subscribers.clone()
        };
        Self::fire(&subs);
        true
    }

    /// 强制结束当前查询，不考虑代数。
    /// 用于 onCancel，任何正在运行的查询都应终止。
    /// 增加代数以便被取消查询的 promise 拒绝中的过期 finally 块看到不匹配并跳过清理。
    pub fn force_end(&self) {
        let subs = {
            let mut g = self.inner.lock().unwrap();
            if g.status == QueryStatus::Idle {
                return;
            }
            g.status = QueryStatus::Idle;
            g.generation += 1;
            g.subscribers.clone()
        };
        Self::fire(&subs);
    }

    /// 守卫是否活跃（dispatching 或 running）？
    pub fn is_active(&self) -> bool {
        self.inner.lock().unwrap().status != QueryStatus::Idle
    }

    /// 当前代数。
    pub fn generation(&self) -> usize {
        self.inner.lock().unwrap().generation
    }

    /// 当前状态。
    pub fn status(&self) -> QueryStatus {
        self.inner.lock().unwrap().status
    }

    /// 订阅状态变化。
    pub fn subscribe<F>(&self, callback: F)
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.inner
            .lock()
            .unwrap()
            .subscribers
            .push(Arc::new(callback));
    }

    /// 获取快照（用于 useSyncExternalStore）。
    pub fn get_snapshot(&self) -> bool {
        self.is_active()
    }

    fn fire(subs: &[Subscriber]) {
        for s in subs {
            s();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn idle_to_running_via_direct_submit() {
        let g = QueryGuard::new();
        assert!(!g.is_active());
        let gen = g.try_start().unwrap();
        assert_eq!(gen, 1);
        assert!(g.is_active());
        assert!(g.end(gen));
        assert!(!g.is_active());
    }

    #[test]
    fn queue_path_reserve_then_start() {
        let g = QueryGuard::new();
        assert!(g.reserve());
        assert_eq!(g.status(), QueryStatus::Dispatching);
        let gen = g.try_start().unwrap();
        assert_eq!(g.status(), QueryStatus::Running);
        assert!(g.end(gen));
    }

    #[test]
    fn reserve_fails_when_busy() {
        let g = QueryGuard::new();
        assert!(g.try_start().is_some());
        assert!(!g.reserve());
    }

    #[test]
    fn force_end_bumps_generation() {
        let g = QueryGuard::new();
        let gen = g.try_start().unwrap();
        g.force_end();
        // stale end() must be a no-op
        assert!(!g.end(gen));
        assert_eq!(g.generation(), gen + 1);
    }

    #[test]
    fn cancel_reservation_returns_to_idle() {
        let g = QueryGuard::new();
        assert!(g.reserve());
        g.cancel_reservation();
        assert_eq!(g.status(), QueryStatus::Idle);
    }

    #[test]
    fn subscribers_fire_on_change() {
        let g = QueryGuard::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let c = counter.clone();
        g.subscribe(move || {
            c.fetch_add(1, Ordering::SeqCst);
        });
        g.try_start();
        g.force_end();
        // expect at least 2 (one per transition)
        assert!(counter.load(Ordering::SeqCst) >= 2);
    }
}
