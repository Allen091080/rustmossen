//! # stream — 异步迭代器流
//!
//! 对应 TypeScript `utils/stream.ts`。
//! 提供一个可写入的异步流实现。

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;

/// 异步流错误
#[derive(Debug, Clone)]
pub struct StreamError(pub String);

impl std::fmt::Display for StreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for StreamError {}

/// 内部状态
struct StreamInner<T> {
    queue: VecDeque<T>,
    is_done: bool,
    error: Option<StreamError>,
    started: bool,
}

/// 异步迭代器流。
///
/// 只能迭代一次。生产者通过 `enqueue` 推入值，通过 `done` 标记结束，
/// 消费者通过 `next` 异步拉取值。
pub struct Stream<T> {
    inner: Arc<Mutex<StreamInner<T>>>,
    notify: Arc<Notify>,
    returned: Option<Box<dyn Fn() + Send + Sync>>,
}

impl<T: Send + 'static> Stream<T> {
    /// 创建新的异步流
    pub fn new(returned: Option<Box<dyn Fn() + Send + Sync>>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(StreamInner {
                queue: VecDeque::new(),
                is_done: false,
                error: None,
                started: false,
            })),
            notify: Arc::new(Notify::new()),
            returned,
        }
    }

    /// 获取下一个元素。流结束返回 None。
    pub async fn next(&self) -> Result<Option<T>, StreamError> {
        {
            let mut inner = self.inner.lock().unwrap();
            if !inner.started {
                inner.started = true;
            }
        }

        loop {
            {
                let mut inner = self.inner.lock().unwrap();
                if let Some(value) = inner.queue.pop_front() {
                    return Ok(Some(value));
                }
                if inner.is_done {
                    return Ok(None);
                }
                if let Some(ref err) = inner.error {
                    return Err(err.clone());
                }
            }
            self.notify.notified().await;
        }
    }

    /// 入队一个值
    pub fn enqueue(&self, value: T) {
        let mut inner = self.inner.lock().unwrap();
        inner.queue.push_back(value);
        drop(inner);
        self.notify.notify_waiters();
    }

    /// 标记流已完成
    pub fn done(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.is_done = true;
        drop(inner);
        self.notify.notify_waiters();
    }

    /// 标记流出错
    pub fn error(&self, error: StreamError) {
        let mut inner = self.inner.lock().unwrap();
        inner.error = Some(error);
        drop(inner);
        self.notify.notify_waiters();
    }

    /// 返回/取消流（调用 returned 回调）
    pub fn return_stream(&self) {
        {
            let mut inner = self.inner.lock().unwrap();
            inner.is_done = true;
        }
        if let Some(ref returned) = self.returned {
            returned();
        }
        self.notify.notify_waiters();
    }
}
