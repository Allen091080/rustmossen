//! # connection — 连接管理与重连策略
//!
//! 提供 WebSocket/SSE 连接的重连策略、指数退避、心跳等功能。
//! 对应 TS `remote/SessionsWebSocket.ts` 和 `cli/transports/WebSocketTransport.ts`
//! 中的重连逻辑。

use std::collections::HashSet;
use std::time::{Duration, Instant};

/// 重连策略配置。
#[derive(Debug, Clone)]
pub struct ReconnectPolicy {
    /// 基础重连延迟。
    pub base_delay: Duration,
    /// 最大重连延迟（指数退避上限）。
    pub max_delay: Duration,
    /// 最大重连尝试次数。
    pub max_attempts: u32,
    /// 重连时间预算（超时后放弃）。
    pub give_up_after: Duration,
    /// 抖动范围（毫秒）。
    pub jitter_ms: u64,
}

impl Default for ReconnectPolicy {
    fn default() -> Self {
        Self {
            base_delay: Duration::from_millis(1000),
            max_delay: Duration::from_secs(30),
            max_attempts: 5,
            give_up_after: Duration::from_secs(600),
            jitter_ms: 1000,
        }
    }
}

impl ReconnectPolicy {
    /// 计算第 `attempt` 次重连的退避延迟。
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let exponential =
            self.base_delay.as_millis() as u64 * 2u64.saturating_pow(attempt.saturating_sub(1));
        let clamped = exponential.min(self.max_delay.as_millis() as u64);
        let jitter = if self.jitter_ms > 0 {
            rand::random::<u64>() % self.jitter_ms
        } else {
            0
        };
        Duration::from_millis(clamped + jitter)
    }

    /// 带 `Retry-After` 头的延迟计算。
    pub fn delay_with_retry_after(&self, retry_after_ms: u64) -> Duration {
        let clamped = retry_after_ms
            .max(self.base_delay.as_millis() as u64)
            .min(self.max_delay.as_millis() as u64);
        let jitter = if self.jitter_ms > 0 {
            rand::random::<u64>() % self.jitter_ms
        } else {
            0
        };
        Duration::from_millis(clamped + jitter)
    }
}

/// 重连状态追踪器。
#[derive(Debug)]
pub struct ReconnectTracker {
    /// 重连策略。
    policy: ReconnectPolicy,
    /// 当前重连尝试次数。
    attempts: u32,
    /// 重连开始时间。
    start_time: Option<Instant>,
    /// 上次重连尝试时间。
    last_attempt_time: Option<Instant>,
}

impl ReconnectTracker {
    /// 创建新的追踪器。
    pub fn new(policy: ReconnectPolicy) -> Self {
        Self {
            policy,
            attempts: 0,
            start_time: None,
            last_attempt_time: None,
        }
    }

    /// 记录一次重连尝试。
    pub fn record_attempt(&mut self) {
        if self.start_time.is_none() {
            self.start_time = Some(Instant::now());
        }
        self.attempts += 1;
        self.last_attempt_time = Some(Instant::now());
    }

    /// 重置追踪器（连接成功后）。
    pub fn reset(&mut self) {
        self.attempts = 0;
        self.start_time = None;
        self.last_attempt_time = None;
    }

    /// 是否应继续重连。
    pub fn should_reconnect(&self) -> bool {
        if self.attempts >= self.policy.max_attempts {
            return false;
        }
        if let Some(start) = self.start_time {
            if start.elapsed() >= self.policy.give_up_after {
                return false;
            }
        }
        true
    }

    /// 获取下一次重连延迟。
    pub fn next_delay(&self) -> Duration {
        self.policy.delay_for_attempt(self.attempts)
    }

    /// 检测系统休眠唤醒（两次尝试间隔异常大）。
    pub fn detect_sleep_wake(&self, threshold: Duration) -> bool {
        if let Some(last) = self.last_attempt_time {
            last.elapsed() > threshold
        } else {
            false
        }
    }

    /// 当前尝试次数。
    pub fn attempts(&self) -> u32 {
        self.attempts
    }
}

/// WebSocket 永久关闭码集合。
///
/// 这些关闭码表示服务端永久拒绝，客户端应立即停止重连。
pub fn is_permanent_close_code(code: u16) -> bool {
    // 4001 = 会话未找到（压缩期间可能暂时出现）。
    // 4003 = 未授权。
    // 1002 = 协议错误。
    matches!(code, 1002 | 4003)
}

/// HTTP 永久拒绝状态码。
pub fn is_permanent_http_code(code: u16) -> bool {
    matches!(code, 401 | 403 | 404)
}

/// 会话未找到关闭码（4001）的重试追踪。
#[derive(Debug)]
pub struct SessionNotFoundTracker {
    /// 最大重试次数。
    max_retries: u32,
    /// 当前重试次数。
    retries: u32,
}

impl SessionNotFoundTracker {
    /// 创建新的追踪器。
    pub fn new(max_retries: u32) -> Self {
        Self {
            max_retries,
            retries: 0,
        }
    }

    /// 记录一次 4001 重试。返回 `false` 表示预算耗尽。
    pub fn record_retry(&mut self) -> bool {
        self.retries += 1;
        self.retries <= self.max_retries
    }

    /// 重置计数。
    pub fn reset(&mut self) {
        self.retries = 0;
    }

    /// 获取当前重试次数。
    pub fn retries(&self) -> u32 {
        self.retries
    }
}

/// 心跳管理器。
///
/// 维持连接活跃状态，检测连接断开。
#[derive(Debug)]
pub struct HeartbeatManager {
    /// 心跳间隔。
    pub interval: Duration,
    /// 上次 pong 接收时间。
    last_pong: Option<Instant>,
    /// 是否已收到 pong。
    pong_received: bool,
}

impl HeartbeatManager {
    /// 创建新的心跳管理器。
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            last_pong: None,
            pong_received: true,
        }
    }

    /// 记录收到 pong。
    pub fn record_pong(&mut self) {
        self.pong_received = true;
        self.last_pong = Some(Instant::now());
    }

    /// 检查是否需要发送 ping。
    pub fn should_ping(&self) -> bool {
        self.pong_received
    }

    /// 标记已发送 ping。
    pub fn mark_ping_sent(&mut self) {
        self.pong_received = false;
    }
}

/// 已解析的 tool_use ID 追踪集合。
///
/// 防止重复处理 WebSocket 重连导致的重复 control_response。
/// 对应 TS `StructuredIO` 中的 `resolvedToolUseIds`。
#[derive(Debug)]
pub struct ResolvedIdTracker {
    /// 已解析的 ID 集合。
    ids: HashSet<String>,
    /// 插入顺序（用于 FIFO 淘汰）。
    order: Vec<String>,
    /// 最大容量。
    max_size: usize,
}

impl ResolvedIdTracker {
    /// 创建新的追踪器。
    pub fn new(max_size: usize) -> Self {
        Self {
            ids: HashSet::with_capacity(max_size),
            order: Vec::with_capacity(max_size),
            max_size,
        }
    }

    /// 插入 ID。如果超出容量，淘汰最早的条目。
    pub fn insert(&mut self, id: String) {
        if self.ids.contains(&id) {
            return;
        }
        if self.ids.len() >= self.max_size {
            if let Some(oldest) = self.order.first().cloned() {
                self.ids.remove(&oldest);
                self.order.remove(0);
            }
        }
        self.ids.insert(id.clone());
        self.order.push(id);
    }

    /// 检查 ID 是否存在。
    pub fn contains(&self, id: &str) -> bool {
        self.ids.contains(id)
    }
}
