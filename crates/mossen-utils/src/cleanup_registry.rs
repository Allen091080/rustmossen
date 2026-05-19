//! # cleanup_registry — 清理注册表
//!
//! 对应 TypeScript `utils/cleanupRegistry.ts`。
//! 优雅关闭期间应运行的清理函数的全局注册表。

use std::sync::Mutex;

/// 清理函数全局注册表。
static CLEANUP_FUNCTIONS: Mutex<Vec<Box<dyn Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> + Send + Sync>>> = 
    Mutex::new(Vec::new());

/// 注册清理函数。
/// 返回取消注册函数。
pub fn register_cleanup<F, Fut>(cleanup_fn: F) -> impl Fn()
where
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let mut fns = CLEANUP_FUNCTIONS.lock().unwrap();
    let idx = fns.len();
    let boxed: Box<dyn Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> + Send + Sync> = 
        Box::new(move || Box::pin(cleanup_fn()));
    fns.push(boxed);
    drop(fns);
    
    move || {
        let mut fns = CLEANUP_FUNCTIONS.lock().unwrap();
        if idx < fns.len() {
            fns.swap_remove(idx);
        }
    }
}

/// 运行所有注册的清理函数。
pub async fn run_cleanup_functions() {
    let fns = {
        let guard = CLEANUP_FUNCTIONS.lock().unwrap();
        guard.iter().map(|f| f()).collect::<Vec<_>>()
    };
    for fut in fns {
        fut.await;
    }
}
