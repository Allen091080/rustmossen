//! # hook_events — Hook 事件广播系统
//!
//! 对应 TS `utils/hooks/hookEvents.ts`。
//! 提供 Hook 执行事件的广播机制。
//! TS 的 EventEmitter → Rust broadcast channel 模式（文档 11）。

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};

use mossen_types::hooks::{HookEvent, ALWAYS_EMITTED_HOOK_EVENTS, HOOK_EVENTS};
use parking_lot::Mutex;
use tokio::sync::broadcast;
use tracing::debug;

/// 最大待处理事件数量。
const MAX_PENDING_EVENTS: usize = 100;

/// Hook 开始事件。
///
/// 对应 TS `HookStartedEvent`。
#[derive(Debug, Clone)]
pub struct HookStartedEvent {
    /// Hook ID。
    pub hook_id: String,
    /// Hook 名称。
    pub hook_name: String,
    /// Hook 事件。
    pub hook_event: String,
}

/// Hook 进度事件。
///
/// 对应 TS `HookProgressEvent`。
#[derive(Debug, Clone)]
pub struct HookProgressEvent {
    /// Hook ID。
    pub hook_id: String,
    /// Hook 名称。
    pub hook_name: String,
    /// Hook 事件。
    pub hook_event: String,
    /// 标准输出。
    pub stdout: String,
    /// 标准错误。
    pub stderr: String,
    /// 组合输出。
    pub output: String,
}

/// Hook 响应事件。
///
/// 对应 TS `HookResponseEvent`。
#[derive(Debug, Clone)]
pub struct HookResponseEvent {
    /// Hook ID。
    pub hook_id: String,
    /// Hook 名称。
    pub hook_name: String,
    /// Hook 事件。
    pub hook_event: String,
    /// 输出。
    pub output: String,
    /// 标准输出。
    pub stdout: String,
    /// 标准错误。
    pub stderr: String,
    /// 退出码。
    pub exit_code: Option<i32>,
    /// 结果状态。
    pub outcome: HookExecutionOutcome,
}

/// Hook 执行结果状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookExecutionOutcome {
    Success,
    Error,
    Cancelled,
}

impl std::fmt::Display for HookExecutionOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Success => write!(f, "success"),
            Self::Error => write!(f, "error"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

/// Hook 执行事件（联合类型）。
///
/// 对应 TS `HookExecutionEvent`。
#[derive(Debug, Clone)]
pub enum HookExecutionEvent {
    Started(HookStartedEvent),
    Progress(HookProgressEvent),
    Response(HookResponseEvent),
}

/// Hook 事件广播器 — 管理 Hook 执行事件的发送和接收。
///
/// 对应 TS 中的 `eventHandler` + `pendingEvents` 队列。
/// 使用 tokio broadcast channel 替代 TS 的 EventEmitter 模式。
pub struct HookEventBroadcaster {
    /// 广播发送器。
    sender: broadcast::Sender<HookExecutionEvent>,
    /// 待处理事件队列（在没有接收者时缓存）。
    pending_events: Mutex<VecDeque<HookExecutionEvent>>,
    /// 是否启用全事件发射。
    all_events_enabled: AtomicBool,
    /// 是否有活跃的接收者。
    has_receiver: AtomicBool,
}

impl HookEventBroadcaster {
    /// 创建新的广播器。
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(256);
        Self {
            sender,
            pending_events: Mutex::new(VecDeque::new()),
            all_events_enabled: AtomicBool::new(false),
            has_receiver: AtomicBool::new(false),
        }
    }

    /// 订阅 Hook 事件。
    ///
    /// 对应 TS `registerHookEventHandler()`。
    /// 返回接收器，同时刷新所有待处理事件。
    pub fn subscribe(&self) -> broadcast::Receiver<HookExecutionEvent> {
        self.has_receiver.store(true, Ordering::SeqCst);
        let receiver = self.sender.subscribe();

        // 刷新待处理事件
        let mut pending = self.pending_events.lock();
        while let Some(event) = pending.pop_front() {
            let _ = self.sender.send(event);
        }

        receiver
    }

    /// 取消注册接收者。
    pub fn unsubscribe(&self) {
        self.has_receiver.store(false, Ordering::SeqCst);
    }

    /// 判断事件是否应该发射。
    ///
    /// 对应 TS `shouldEmit()`。
    fn should_emit(&self, hook_event: &str) -> bool {
        // 始终发射的事件
        for e in ALWAYS_EMITTED_HOOK_EVENTS {
            if e.as_str() == hook_event {
                return true;
            }
        }

        // 其他事件需要启用全事件发射
        if !self.all_events_enabled.load(Ordering::SeqCst) {
            return false;
        }

        HOOK_EVENTS.iter().any(|e| e.as_str() == hook_event)
    }

    /// 发射 Hook 开始事件。
    ///
    /// 对应 TS `emitHookStarted()`。
    pub fn emit_hook_started(&self, hook_id: &str, hook_name: &str, hook_event: &str) {
        if !self.should_emit(hook_event) {
            return;
        }

        let event = HookExecutionEvent::Started(HookStartedEvent {
            hook_id: hook_id.to_string(),
            hook_name: hook_name.to_string(),
            hook_event: hook_event.to_string(),
        });

        self.emit(event);
    }

    /// 发射 Hook 进度事件。
    ///
    /// 对应 TS `emitHookProgress()`。
    pub fn emit_hook_progress(
        &self,
        hook_id: &str,
        hook_name: &str,
        hook_event: &str,
        stdout: &str,
        stderr: &str,
    ) {
        if !self.should_emit(hook_event) {
            return;
        }

        let output = format!("{stdout}{stderr}");
        let event = HookExecutionEvent::Progress(HookProgressEvent {
            hook_id: hook_id.to_string(),
            hook_name: hook_name.to_string(),
            hook_event: hook_event.to_string(),
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
            output,
        });

        self.emit(event);
    }

    /// 发射 Hook 响应事件。
    ///
    /// 对应 TS `emitHookResponse()`。
    pub fn emit_hook_response(
        &self,
        hook_id: &str,
        hook_name: &str,
        hook_event: &str,
        stdout: &str,
        stderr: &str,
        exit_code: Option<i32>,
        outcome: HookExecutionOutcome,
    ) {
        // 始终记录调试日志
        let output_to_log = if !stdout.is_empty() {
            stdout
        } else if !stderr.is_empty() {
            stderr
        } else {
            ""
        };
        if !output_to_log.is_empty() {
            debug!(
                hook_name = hook_name,
                hook_event = hook_event,
                outcome = %outcome,
                "Hook output: {}", output_to_log
            );
        }

        if !self.should_emit(hook_event) {
            return;
        }

        let output = format!("{stdout}{stderr}");
        let event = HookExecutionEvent::Response(HookResponseEvent {
            hook_id: hook_id.to_string(),
            hook_name: hook_name.to_string(),
            hook_event: hook_event.to_string(),
            output,
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
            exit_code,
            outcome,
        });

        self.emit(event);
    }

    /// 启用/禁用全事件发射。
    ///
    /// 对应 TS `setAllHookEventsEnabled()`。
    pub fn set_all_events_enabled(&self, enabled: bool) {
        self.all_events_enabled.store(enabled, Ordering::SeqCst);
    }

    /// 清除事件状态。
    ///
    /// 对应 TS `clearHookEventState()`。
    pub fn clear_state(&self) {
        self.has_receiver.store(false, Ordering::SeqCst);
        self.pending_events.lock().clear();
        self.all_events_enabled.store(false, Ordering::SeqCst);
    }

    /// 内部发射事件。
    fn emit(&self, event: HookExecutionEvent) {
        if self.has_receiver.load(Ordering::SeqCst) {
            let _ = self.sender.send(event);
        } else {
            let mut pending = self.pending_events.lock();
            pending.push_back(event);
            if pending.len() > MAX_PENDING_EVENTS {
                pending.pop_front();
            }
        }
    }
}

impl Default for HookEventBroadcaster {
    fn default() -> Self {
        Self::new()
    }
}
