//! # transport — 传输层 trait 定义
//!
//! 定义所有传输协议（WebSocket、SSE、Stdio）的统一抽象接口。
//! 对应 TS `cli/transports/Transport.ts` 中的 Transport 接口。

use async_trait::async_trait;
use serde_json::Value;
use std::fmt;

/// 传输层状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransportState {
    /// 空闲（未连接）。
    Idle,
    /// 正在连接。
    Connecting,
    /// 已连接。
    Connected,
    /// 正在重连。
    Reconnecting,
    /// 正在关闭。
    Closing,
    /// 已关闭。
    Closed,
}

impl fmt::Display for TransportState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Idle => write!(f, "idle"),
            Self::Connecting => write!(f, "connecting"),
            Self::Connected => write!(f, "connected"),
            Self::Reconnecting => write!(f, "reconnecting"),
            Self::Closing => write!(f, "closing"),
            Self::Closed => write!(f, "closed"),
        }
    }
}

/// 传输层事件回调。
pub type OnDataCallback = Box<dyn Fn(String) + Send + Sync>;

/// 传输层关闭回调。
pub type OnCloseCallback = Box<dyn Fn(Option<u16>) + Send + Sync>;

/// 传输层连接回调。
pub type OnConnectCallback = Box<dyn Fn() + Send + Sync>;

/// 传输层事件回调（用于 SSE 事件类型识别）。
pub type OnEventCallback = Box<dyn Fn(StreamClientEvent) + Send + Sync>;

/// SSE 客户端事件（来自服务器端事件流）。
#[derive(Debug, Clone)]
pub struct StreamClientEvent {
    /// SSE 事件类型。
    pub event_type: Option<String>,
    /// SSE 事件 ID。
    pub id: Option<String>,
    /// SSE 事件数据。
    pub data: Option<String>,
}

/// 标准输出消息（对应 TS `StdoutMessage`）。
///
/// 使用 `serde_json::Value` 作为通用载荷。
pub type StdoutMessage = Value;

/// 标准输入消息（对应 TS `StdinMessage`）。
pub type StdinMessage = Value;

/// 传输层统一抽象。
///
/// 所有传输协议（WebSocket、SSE、Stdio）均实现此 trait。
/// 提供连接、读取、写入、关闭等基本操作。
#[async_trait]
pub trait Transport: Send + Sync {
    /// 建立连接。
    async fn connect(&mut self) -> anyhow::Result<()>;

    /// 发送消息。
    async fn write(&self, message: StdoutMessage) -> anyhow::Result<()>;

    /// 关闭连接。
    fn close(&mut self);

    /// 获取当前状态。
    fn state(&self) -> TransportState;

    /// 是否已连接。
    fn is_connected(&self) -> bool {
        self.state() == TransportState::Connected
    }

    /// 注册数据接收回调。
    fn set_on_data(&mut self, callback: OnDataCallback);

    /// 注册连接关闭回调。
    fn set_on_close(&mut self, callback: OnCloseCallback);

    /// 注册连接建立回调。
    fn set_on_connect(&mut self, callback: OnConnectCallback);
}

/// 传输层配置选项。
#[derive(Debug, Clone)]
pub struct TransportOptions {
    /// 请求头。
    pub headers: std::collections::HashMap<String, String>,
    /// 会话 ID。
    pub session_id: Option<String>,
    /// 是否自动重连。
    pub auto_reconnect: bool,
    /// 是否为桥接模式。
    pub is_bridge: bool,
}

impl Default for TransportOptions {
    fn default() -> Self {
        Self {
            headers: std::collections::HashMap::new(),
            session_id: None,
            auto_reconnect: true,
            is_bridge: false,
        }
    }
}

/// 根据 URL 协议选择合适的传输层。
///
/// 传输选择优先级：
/// 1. `SSETransport` — 当 `MOSSEN_REMOTE_USE_CCR_V2` 为真时（SSE 读 + POST 写）
/// 2. `HybridTransport` — 当 `MOSSEN_REMOTE_POST_FOR_INGRESS_V2` 为真时（WS 读 + POST 写）
/// 3. `WebSocketTransport` — 默认（WS 读 + WS 写）
pub fn select_transport_for_url(url: &url::Url) -> TransportKind {
    let use_ccr_v2 = std::env::var("MOSSEN_REMOTE_USE_CCR_V2")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    if use_ccr_v2 {
        return TransportKind::Sse;
    }

    let use_hybrid = std::env::var("MOSSEN_REMOTE_POST_FOR_INGRESS_V2")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    match url.scheme() {
        "ws" | "wss" => {
            if use_hybrid {
                TransportKind::Hybrid
            } else {
                TransportKind::WebSocket
            }
        }
        _ => TransportKind::WebSocket,
    }
}

/// 传输层类型枚举。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportKind {
    /// WebSocket（全双工）。
    WebSocket,
    /// SSE（读）+ HTTP POST（写）。
    Sse,
    /// WS（读）+ HTTP POST（写）。
    Hybrid,
    /// 标准输入输出。
    Stdio,
}
