//! # mailbox — 消息信箱
//!
//! 对应 TypeScript `utils/mailbox.ts`。
//! 实现带等待者的异步消息队列。

use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::{broadcast, oneshot};

/// 消息来源
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageSource {
    User,
    Teammate,
    System,
    Tick,
    Task,
}

/// 消息
#[derive(Debug, Clone)]
pub struct MailboxMessage {
    pub id: String,
    pub source: MessageSource,
    pub content: String,
    pub from: Option<String>,
    pub color: Option<String>,
    pub timestamp: String,
}

/// 等待者
struct Waiter {
    filter: Box<dyn Fn(&MailboxMessage) -> bool + Send>,
    sender: oneshot::Sender<MailboxMessage>,
}

/// 消息信箱
///
/// 支持发送、轮询和异步接收消息。
/// 等待者可以指定过滤条件来匹配特定消息。
pub struct Mailbox {
    queue: Vec<MailboxMessage>,
    waiters: Vec<Waiter>,
    changed: broadcast::Sender<()>,
    revision: AtomicU64,
}

impl Mailbox {
    /// 创建新的信箱
    pub fn new() -> Self {
        let (changed, _) = broadcast::channel(16);
        Self {
            queue: Vec::new(),
            waiters: Vec::new(),
            changed,
            revision: AtomicU64::new(0),
        }
    }

    /// 获取队列长度
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// 队列是否为空
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// 获取修订号
    pub fn revision(&self) -> u64 {
        self.revision.load(Ordering::Relaxed)
    }

    /// 发送消息
    pub fn send(&mut self, msg: MailboxMessage) {
        self.revision.fetch_add(1, Ordering::Relaxed);

        // 检查等待者是否匹配
        let idx = self.waiters.iter().position(|w| (w.filter)(&msg));
        if let Some(idx) = idx {
            let waiter = self.waiters.remove(idx);
            let _ = waiter.sender.send(msg);
            self.notify();
            return;
        }

        self.queue.push(msg);
        self.notify();
    }

    /// 轮询：同步获取第一个匹配的消息
    pub fn poll(&mut self, filter: impl Fn(&MailboxMessage) -> bool) -> Option<MailboxMessage> {
        let idx = self.queue.iter().position(|m| filter(m));
        idx.map(|i| self.queue.remove(i))
    }

    /// 异步接收：等待匹配的消息
    pub fn receive(
        &mut self,
        filter: impl Fn(&MailboxMessage) -> bool + Send + 'static,
    ) -> oneshot::Receiver<MailboxMessage> {
        // 先检查队列中是否有匹配的
        let idx = self.queue.iter().position(|m| filter(m));
        if let Some(idx) = idx {
            let msg = self.queue.remove(idx);
            self.notify();
            let (tx, rx) = oneshot::channel();
            let _ = tx.send(msg);
            return rx;
        }

        // 没有匹配的，注册等待者
        let (tx, rx) = oneshot::channel();
        self.waiters.push(Waiter {
            filter: Box::new(filter),
            sender: tx,
        });
        rx
    }

    /// 订阅变更通知
    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.changed.subscribe()
    }

    fn notify(&self) {
        let _ = self.changed.send(());
    }
}

impl Default for Mailbox {
    fn default() -> Self {
        Self::new()
    }
}
