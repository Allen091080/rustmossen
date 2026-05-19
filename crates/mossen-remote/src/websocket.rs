//! # websocket — WebSocket 传输层
//!
//! 实现 WebSocket 双向通信，包含自动重连、心跳检测、消息缓冲等功能。
//! 对应 TS `cli/transports/WebSocketTransport.ts` 和 `remote/SessionsWebSocket.ts`。

use crate::connection::{ReconnectPolicy, ReconnectTracker};
use crate::ndjson::ndjson_safe_stringify;
use crate::transport::{
    OnCloseCallback, OnConnectCallback, OnDataCallback, StdoutMessage, Transport, TransportState,
};
use async_trait::async_trait;
use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, RwLock};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tracing;

/// 默认 ping 间隔（10 秒）。
#[allow(dead_code)]
const DEFAULT_PING_INTERVAL: Duration = Duration::from_secs(10);
/// 默认 keep-alive 间隔（5 分钟）。
#[allow(dead_code)]
const DEFAULT_KEEPALIVE_INTERVAL: Duration = Duration::from_secs(300);
/// 重连延迟（SessionsWebSocket 用）。
#[allow(dead_code)]
const SESSION_RECONNECT_DELAY: Duration = Duration::from_millis(2000);
/// 最大重连尝试次数（SessionsWebSocket 用）。
#[allow(dead_code)]
const SESSION_MAX_RECONNECT_ATTEMPTS: u32 = 5;
/// 4001 最大重试次数。
#[allow(dead_code)]
const MAX_SESSION_NOT_FOUND_RETRIES: u32 = 3;

/// WebSocket 传输层。
///
/// 支持全双工通信，包含自动重连和心跳检测。
#[allow(dead_code)]
pub struct WebSocketTransport {
    /// 连接 URL。
    url: url::Url,
    /// 请求头。
    headers: HashMap<String, String>,
    /// 会话 ID。
    session_id: Option<String>,
    /// 当前状态。
    state: Arc<RwLock<TransportState>>,
    /// 数据回调。
    on_data: Arc<RwLock<Option<OnDataCallback>>>,
    /// 关闭回调。
    on_close: Arc<RwLock<Option<OnCloseCallback>>>,
    /// 连接回调。
    on_connect: Arc<RwLock<Option<OnConnectCallback>>>,
    /// 重连追踪器。
    reconnect_tracker: Arc<Mutex<ReconnectTracker>>,
    /// 是否自动重连。
    auto_reconnect: bool,
    /// 写入端。
    write_sink: Arc<
        Mutex<
            Option<
                SplitSink<
                    tokio_tungstenite::WebSocketStream<
                        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
                    >,
                    WsMessage,
                >,
            >,
        >,
    >,
    /// 关闭信号。
    shutdown_tx: Arc<Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
    /// 上次发送的请求 ID。
    last_sent_id: Arc<RwLock<Option<String>>>,
    /// 动态刷新头部的回调。
    refresh_headers: Option<Arc<dyn Fn() -> HashMap<String, String> + Send + Sync>>,
}

impl WebSocketTransport {
    /// 创建新的 WebSocket 传输。
    pub fn new(
        url: url::Url,
        headers: HashMap<String, String>,
        session_id: Option<String>,
        auto_reconnect: bool,
    ) -> Self {
        let policy = ReconnectPolicy {
            base_delay: Duration::from_millis(1000),
            max_delay: Duration::from_secs(30),
            max_attempts: u32::MAX,
            give_up_after: Duration::from_secs(600),
            jitter_ms: 1000,
        };
        Self {
            url,
            headers,
            session_id,
            state: Arc::new(RwLock::new(TransportState::Idle)),
            on_data: Arc::new(RwLock::new(None)),
            on_close: Arc::new(RwLock::new(None)),
            on_connect: Arc::new(RwLock::new(None)),
            reconnect_tracker: Arc::new(Mutex::new(ReconnectTracker::new(policy))),
            auto_reconnect,
            write_sink: Arc::new(Mutex::new(None)),
            shutdown_tx: Arc::new(Mutex::new(None)),
            last_sent_id: Arc::new(RwLock::new(None)),
            refresh_headers: None,
        }
    }

    /// 设置动态刷新头部的回调。
    pub fn set_refresh_headers(
        &mut self,
        callback: impl Fn() -> HashMap<String, String> + Send + Sync + 'static,
    ) {
        self.refresh_headers = Some(Arc::new(callback));
    }

    /// 内部连接逻辑。
    async fn do_connect(&self) -> anyhow::Result<()> {
        let url_str = self.url.as_str();
        tracing::debug!("WebSocketTransport: opening {}", url_str);

        // 构建请求
        let mut request = tokio_tungstenite::tungstenite::http::Request::builder().uri(url_str);

        // 应用刷新后的头部（如果有）或使用初始头部
        let headers = if let Some(ref refresh) = self.refresh_headers {
            refresh()
        } else {
            self.headers.clone()
        };

        for (key, value) in &headers {
            request = request.header(key.as_str(), value.as_str());
        }

        // 添加 last-request-id 头（用于断点续传）
        if let Some(ref last_id) = *self.last_sent_id.read().await {
            request = request.header("X-Last-Request-Id", last_id.as_str());
        }

        let request = request.body(())?;

        let (ws_stream, _response) = tokio_tungstenite::connect_async(request).await?;

        let (write, read) = ws_stream.split();
        *self.write_sink.lock().await = Some(write);
        *self.state.write().await = TransportState::Connected;

        // 通知连接成功
        if let Some(ref cb) = *self.on_connect.read().await {
            cb();
        }

        // 启动读取循环
        let on_data = self.on_data.clone();
        let on_close = self.on_close.clone();
        let state = self.state.clone();
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        *self.shutdown_tx.lock().await = Some(shutdown_tx);

        tokio::spawn(Self::read_loop(read, on_data, on_close, state, shutdown_rx));

        Ok(())
    }

    /// 读取循环。
    async fn read_loop(
        mut read: SplitStream<
            tokio_tungstenite::WebSocketStream<
                tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
            >,
        >,
        on_data: Arc<RwLock<Option<OnDataCallback>>>,
        on_close: Arc<RwLock<Option<OnCloseCallback>>>,
        state: Arc<RwLock<TransportState>>,
        mut shutdown_rx: tokio::sync::oneshot::Receiver<()>,
    ) {
        loop {
            tokio::select! {
                msg = read.next() => {
                    match msg {
                        Some(Ok(WsMessage::Text(text))) => {
                            if let Some(ref cb) = *on_data.read().await {
                                cb(text.to_string());
                            }
                        }
                        Some(Ok(WsMessage::Binary(data))) => {
                            if let Ok(text) = String::from_utf8(data.to_vec()) {
                                if let Some(ref cb) = *on_data.read().await {
                                    cb(text);
                                }
                            }
                        }
                        Some(Ok(WsMessage::Ping(_))) | Some(Ok(WsMessage::Pong(_))) => {
                            // 心跳帧，忽略
                        }
                        Some(Ok(WsMessage::Close(frame))) => {
                            let code = frame.as_ref().map(|f| f.code.into());
                            tracing::debug!("WebSocketTransport: closed with code {:?}", code);
                            *state.write().await = TransportState::Closed;
                            if let Some(ref cb) = *on_close.read().await {
                                cb(code);
                            }
                            return;
                        }
                        Some(Ok(WsMessage::Frame(_))) => {
                            // 原始帧，通常不会到达
                        }
                        Some(Err(e)) => {
                            tracing::error!("WebSocketTransport: read error: {}", e);
                            *state.write().await = TransportState::Closed;
                            if let Some(ref cb) = *on_close.read().await {
                                cb(None);
                            }
                            return;
                        }
                        None => {
                            *state.write().await = TransportState::Closed;
                            if let Some(ref cb) = *on_close.read().await {
                                cb(None);
                            }
                            return;
                        }
                    }
                }
                _ = &mut shutdown_rx => {
                    tracing::debug!("WebSocketTransport: shutdown signal received");
                    return;
                }
            }
        }
    }
}

#[async_trait]
impl Transport for WebSocketTransport {
    async fn connect(&mut self) -> anyhow::Result<()> {
        *self.state.write().await = TransportState::Connecting;
        self.do_connect().await
    }

    async fn write(&self, message: StdoutMessage) -> anyhow::Result<()> {
        let json = ndjson_safe_stringify(&message)?;
        if let Some(ref mut sink) = *self.write_sink.lock().await {
            sink.send(WsMessage::Text(json.into())).await?;
            // 追踪最后发送的 ID（如果消息中包含 uuid 字段）
            if let Some(uuid) = message.get("uuid").and_then(|v| v.as_str()) {
                *self.last_sent_id.write().await = Some(uuid.to_string());
            }
        } else {
            anyhow::bail!("WebSocket not connected");
        }
        Ok(())
    }

    fn close(&mut self) {
        let state = self.state.clone();
        let shutdown_tx = self.shutdown_tx.clone();
        let write_sink = self.write_sink.clone();

        tokio::spawn(async move {
            *state.write().await = TransportState::Closed;
            if let Some(tx) = shutdown_tx.lock().await.take() {
                let _ = tx.send(());
            }
            if let Some(ref mut sink) = *write_sink.lock().await {
                let _ = sink.close().await;
            }
        });
    }

    fn state(&self) -> TransportState {
        // 返回缓存的状态（同步访问需要用 try_read）
        self.state
            .try_read()
            .map(|s| *s)
            .unwrap_or(TransportState::Idle)
    }

    fn set_on_data(&mut self, callback: OnDataCallback) {
        let on_data = self.on_data.clone();
        tokio::spawn(async move {
            *on_data.write().await = Some(callback);
        });
    }

    fn set_on_close(&mut self, callback: OnCloseCallback) {
        let on_close = self.on_close.clone();
        tokio::spawn(async move {
            *on_close.write().await = Some(callback);
        });
    }

    fn set_on_connect(&mut self, callback: OnConnectCallback) {
        let on_connect = self.on_connect.clone();
        tokio::spawn(async move {
            *on_connect.write().await = Some(callback);
        });
    }
}

/// 会话 WebSocket 客户端。
///
/// 用于连接到远程会话的 WebSocket 订阅端点。
/// 对应 TS `remote/SessionsWebSocket.ts`。
///
/// 协议：
/// 1. 连接到 `wss://api.mossen.invalid/v1/sessions/ws/{sessionId}/subscribe`
/// 2. 通过 Authorization 头进行认证
/// 3. 接收 SDK 消息流
#[allow(dead_code)]
pub struct SessionsWebSocket {
    /// 会话 ID。
    session_id: String,
    /// 组织 UUID。
    org_uuid: String,
    /// 访问令牌获取器。
    get_access_token: Box<dyn Fn() -> String + Send + Sync>,
    /// 消息回调。
    on_message: Box<dyn Fn(serde_json::Value) + Send + Sync>,
    /// 连接回调。
    on_connected: Option<Box<dyn Fn() + Send + Sync>>,
    /// 断开回调。
    on_close: Option<Box<dyn Fn() + Send + Sync>>,
    /// 重连回调。
    on_reconnecting: Option<Box<dyn Fn() + Send + Sync>>,
    /// 错误回调。
    on_error: Option<Box<dyn Fn(String) + Send + Sync>>,
    /// 连接状态。
    state: Arc<RwLock<TransportState>>,
    /// 关闭信号。
    shutdown_tx: Arc<Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
}

impl SessionsWebSocket {
    /// 创建新的会话 WebSocket。
    pub fn new(
        session_id: String,
        org_uuid: String,
        get_access_token: impl Fn() -> String + Send + Sync + 'static,
        on_message: impl Fn(serde_json::Value) + Send + Sync + 'static,
    ) -> Self {
        Self {
            session_id,
            org_uuid,
            get_access_token: Box::new(get_access_token),
            on_message: Box::new(on_message),
            on_connected: None,
            on_close: None,
            on_reconnecting: None,
            on_error: None,
            state: Arc::new(RwLock::new(TransportState::Idle)),
            shutdown_tx: Arc::new(Mutex::new(None)),
        }
    }

    /// 设置连接建立回调。
    pub fn set_on_connected(&mut self, cb: impl Fn() + Send + Sync + 'static) {
        self.on_connected = Some(Box::new(cb));
    }

    /// 设置断开回调。
    pub fn set_on_close(&mut self, cb: impl Fn() + Send + Sync + 'static) {
        self.on_close = Some(Box::new(cb));
    }

    /// 设置重连回调。
    pub fn set_on_reconnecting(&mut self, cb: impl Fn() + Send + Sync + 'static) {
        self.on_reconnecting = Some(Box::new(cb));
    }

    /// 是否已连接。
    pub async fn is_connected(&self) -> bool {
        *self.state.read().await == TransportState::Connected
    }

    /// 关闭连接。
    pub async fn close(&self) {
        *self.state.write().await = TransportState::Closed;
        if let Some(tx) = self.shutdown_tx.lock().await.take() {
            let _ = tx.send(());
        }
    }
}

/// 发送控制响应到 WebSocket。
pub async fn send_control_response(
    sink: &Arc<
        Mutex<
            Option<
                SplitSink<
                    tokio_tungstenite::WebSocketStream<
                        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
                    >,
                    WsMessage,
                >,
            >,
        >,
    >,
    response: &serde_json::Value,
) -> anyhow::Result<()> {
    if let Some(ref mut sink) = *sink.lock().await {
        let json = serde_json::to_string(response)?;
        sink.send(WsMessage::Text(json.into())).await?;
    }
    Ok(())
}

/// 发送控制请求到 WebSocket。
pub async fn send_control_request(
    sink: &Arc<
        Mutex<
            Option<
                SplitSink<
                    tokio_tungstenite::WebSocketStream<
                        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
                    >,
                    WsMessage,
                >,
            >,
        >,
    >,
    request_subtype: &str,
) -> anyhow::Result<()> {
    let request = serde_json::json!({
        "type": "control_request",
        "request_id": uuid::Uuid::new_v4().to_string(),
        "request": {
            "subtype": request_subtype,
        }
    });
    send_control_response(sink, &request).await
}
