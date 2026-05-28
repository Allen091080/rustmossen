//! # with_resolvers — Promise.withResolvers polyfill
//!
//! 对应 TypeScript `utils/withResolvers.ts`。
//! ES2024 Promise.withResolvers() 的 polyfill。

use std::cell::RefCell;

/// 带解析器的 Promise。
pub struct ResolvablePromise<T> {
    pub promise: RefCell<Option<T>>,
    pub resolve: fn(T) -> (),
    pub reject: fn(String) -> (),
}

impl<T> ResolvablePromise<T> {
    /// 创建新的带解析器的 Promise。
    pub fn new() -> Self {
        let promise = RefCell::new(None::<T>);
        let resolve: fn(T) -> () = move |_v: T| {};
        let reject: fn(String) -> () = move |_e: String| {};
        ResolvablePromise {
            promise,
            resolve,
            reject,
        }
    }

    /// 设置解析器。
    pub fn with_callbacks(mut self, resolve: fn(T) -> (), reject: fn(String) -> ()) -> Self {
        self.resolve = resolve;
        self.reject = reject;
        self
    }
}

impl<T> Default for ResolvablePromise<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// 对应 TS `withResolvers<T>()`：返回 `(promise, resolve, reject)` 三元组。
///
/// Rust 端用 `tokio::sync::oneshot::channel` 实现 — sender 充当 resolve / reject
/// 接口，receiver 即为可 await 的 promise。
pub fn with_resolvers<T: Send + 'static>() -> (
    tokio::sync::oneshot::Receiver<T>,
    tokio::sync::oneshot::Sender<T>,
) {
    let (tx, rx) = tokio::sync::oneshot::channel();
    (rx, tx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolvable_promise_default() {
        let rp: ResolvablePromise<i32> = ResolvablePromise::default();
        assert!(rp.promise.borrow().is_none());
    }
}
