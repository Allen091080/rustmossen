//! # sequential — 顺序执行包装器
//!
//! 对应 TypeScript `utils/sequential.ts`。
//! 为异步函数创建顺序执行包装器以防止竞争条件。

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::Mutex;

/// 创建一个顺序执行包装器。
/// 确保对包装函数的并发调用按接收顺序逐一执行，同时保留正确的返回值。
///
/// 适用于必须顺序执行的操作，如文件写入或可能在并发执行时引起冲突的数据库更新。
///
/// # 类型参数
/// - `F`: 异步函数类型
/// - `T`: 参数类型
/// - `R`: 返回值类型
///
/// # 返回
/// 顺序执行版本的函数句柄
pub fn sequential<F, T, R>(f: F) -> impl Fn(T) -> Pin<Box<dyn Future<Output = R> + Send>>
where
    F: Fn(T) -> Pin<Box<dyn Future<Output = R> + Send>> + Send + Sync + 'static,
    T: Send + 'static,
    R: Send + 'static,
{
    let mutex = Arc::new(Mutex::new(()));
    let f = Arc::new(f);

    move |args: T| {
        let mutex = Arc::clone(&mutex);
        let f = Arc::clone(&f);
        Box::pin(async move {
            let _guard = mutex.lock().await;
            f(args).await
        })
    }
}

/// 基于 Mutex 的顺序执行器（更简单的用法）。
/// 持有此结构体的引用并调用 `run` 来顺序执行异步任务。
pub struct SequentialExecutor {
    mutex: Arc<Mutex<()>>,
}

impl SequentialExecutor {
    pub fn new() -> Self {
        Self {
            mutex: Arc::new(Mutex::new(())),
        }
    }

    /// 在互斥锁保护下顺序执行异步任务。
    pub async fn run<F, R>(&self, f: F) -> R
    where
        F: Future<Output = R>,
    {
        let _guard = self.mutex.lock().await;
        f.await
    }
}

impl Default for SequentialExecutor {
    fn default() -> Self {
        Self::new()
    }
}
