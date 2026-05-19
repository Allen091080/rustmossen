//! 轻量级事件信号原语
//!
//! 用于纯事件信号（无存储状态）的监听器集合。

use std::sync::{Arc, Mutex};

/// 事件信号 — 无状态事件发布/订阅。
pub struct Signal {
    listeners: Arc<Mutex<Vec<Box<dyn Fn() + Send + Sync>>>>,
}

impl Signal {
    /// 创建新的信号。
    pub fn new() -> Self {
        Signal {
            listeners: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// 订阅信号。返回取消订阅的函数。
    pub fn subscribe<F>(&self, listener: F)
    where
        F: Fn() + Send + Sync + 'static,
    {
        let listener_box: Box<dyn Fn() + Send + Sync> = Box::new(listener);
        self.listeners.lock().unwrap().push(listener_box);
    }

    /// 发射信号，通知所有订阅者。
    pub fn emit(&self) {
        let guard = self.listeners.lock().unwrap();
        for callback in guard.iter() {
            callback();
        }
    }

    /// 清空所有监听器。
    pub fn clear(&self) {
        self.listeners.lock().unwrap().clear();
    }
}

impl Default for Signal {
    fn default() -> Self {
        Self::new()
    }
}

/// 带参数的信号。
pub struct SignalWithArgs<T: Clone + 'static> {
    listeners: Arc<Mutex<Vec<Box<dyn Fn(&T) + Send + Sync>>>>,
}

impl<T: Clone + 'static> SignalWithArgs<T> {
    /// 创建新的带参信号。
    pub fn new() -> Self {
        SignalWithArgs {
            listeners: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// 订阅信号。
    pub fn subscribe<F>(&self, listener: F)
    where
        F: Fn(&T) + Send + Sync + 'static,
    {
        let listener_box: Box<dyn Fn(&T) + Send + Sync> = Box::new(listener);
        self.listeners.lock().unwrap().push(listener_box);
    }

    /// 发射信号，传递参数给所有订阅者。
    pub fn emit(&self, value: &T) {
        let guard = self.listeners.lock().unwrap();
        for callback in guard.iter() {
            callback(value);
        }
    }

    /// 清空所有监听器。
    pub fn clear(&self) {
        self.listeners.lock().unwrap().clear();
    }
}

impl<T: Clone + 'static> Default for SignalWithArgs<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// 工厂函数 — 对应 TS 中的 `createSignal()`。
///
/// Rust 没有可变长度类型参数，因此暴露一个返回零参信号的工厂作为默认入口；
/// 调用方需要带参信号时可直接使用 `SignalWithArgs::new()`。
pub fn create_signal() -> Signal {
    Signal::new()
}
