//! # generators — 异步生成器工具
//!
//! 对应 TypeScript `utils/generators.ts`。

use futures::stream::{self, Stream, StreamExt};
use std::pin::Pin;
use tokio::sync::mpsc;

/// 获取异步流的最后一个元素
pub async fn last_x<A>(mut stream: Pin<Box<dyn Stream<Item = A> + Send>>) -> Result<A, GeneratorError> {
    let mut last_value: Option<A> = None;
    while let Some(a) = stream.next().await {
        last_value = Some(a);
    }
    last_value.ok_or(GeneratorError::EmptyGenerator)
}

/// 生成器错误
#[derive(Debug, Clone)]
pub enum GeneratorError {
    EmptyGenerator,
}

impl std::fmt::Display for GeneratorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GeneratorError::EmptyGenerator => write!(f, "No items in generator"),
        }
    }
}

impl std::error::Error for GeneratorError {}

/// 消耗异步流直到完成，返回最终值
pub async fn return_value<A>(
    mut stream: Pin<Box<dyn Stream<Item = A> + Send>>,
) -> Option<A> {
    let mut last = None;
    while let Some(v) = stream.next().await {
        last = Some(v);
    }
    last
}

/// 并发运行多个异步流，按完成顺序产出值
pub fn all<A: Send + 'static>(
    streams: Vec<Pin<Box<dyn Stream<Item = A> + Send>>>,
    concurrency_cap: usize,
) -> Pin<Box<dyn Stream<Item = A> + Send>> {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let cap = if concurrency_cap == 0 {
        usize::MAX
    } else {
        concurrency_cap
    };

    tokio::spawn(async move {
        let mut waiting: Vec<Pin<Box<dyn Stream<Item = A> + Send>>> = streams.into_iter().collect();
        let mut active: Vec<Pin<Box<dyn Stream<Item = A> + Send>>> = Vec::new();

        // 启动初始批次到并发上限
        while active.len() < cap && !waiting.is_empty() {
            active.push(waiting.remove(0));
        }

        loop {
            if active.is_empty() {
                break;
            }

            // 轮询所有活跃流
            let mut i = 0;
            let mut any_progress = false;
            while i < active.len() {
                match futures::poll!(active[i].next()) {
                    std::task::Poll::Ready(Some(value)) => {
                        if tx.send(value).is_err() {
                            return;
                        }
                        any_progress = true;
                        i += 1;
                    }
                    std::task::Poll::Ready(None) => {
                        // 流结束，移除并启动新的（如有）
                        active.remove(i);
                        if !waiting.is_empty() {
                            active.push(waiting.remove(0));
                        }
                        any_progress = true;
                    }
                    std::task::Poll::Pending => {
                        i += 1;
                    }
                }
            }

            if !any_progress {
                tokio::task::yield_now().await;
            }
        }
    });

    // 将 UnboundedReceiver 转为 Stream
    Box::pin(futures::stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|item| (item, rx))
    }))
}

/// 将异步流收集为数组
pub async fn to_array<A>(mut stream: Pin<Box<dyn Stream<Item = A> + Send>>) -> Vec<A> {
    let mut result = Vec::new();
    while let Some(a) = stream.next().await {
        result.push(a);
    }
    result
}

/// 从数组创建异步流
pub fn from_array<T: Send + 'static>(values: Vec<T>) -> Pin<Box<dyn Stream<Item = T> + Send>> {
    Box::pin(stream::iter(values))
}
