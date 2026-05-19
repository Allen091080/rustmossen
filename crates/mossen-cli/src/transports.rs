//! 传输层 — 对应 TS 的 cli/transports/ 目录。
//!
//! 实现多种 transport 策略：WebSocket、SSE、Hybrid 和 CCR Client。

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, RwLock};
use tracing::{error, info, warn};
use url::Url;

/// Transport 通用接口 — 对应 TS 的 Transport interface。
#[async_trait::async_trait]
pub trait Transport: Send + Sync {
    /// 建立连接。
    async fn connect(&self) -> Result<()>;
    /// 发送消息。
    async fn write(&self, message: &serde_json::Value) -> Result<()>;
    /// 关闭连接。
    fn close(&self);
    /// 设置数据回调。
    fn set_on_data(&self, callback: Box<dyn Fn(String) + Send + Sync>);
    /// 设置关闭回调。
    fn set_on_close(&self, callback: Box<dyn Fn() + Send + Sync>);
    /// 设置事件回调。
    fn set_on_event(&self, callback: Box<dyn Fn(serde_json::Value) + Send + Sync>);
}

/// 传输层工具函数 — 对应 TS 的 cli/transports/transportUtils.ts。
///
/// 根据 URL 协议和环境变量选择合适的传输策略：
/// 1. SSETransport (SSE reads + POST writes) — 当 MOSSEN_CODE_USE_CCR_V2 设置时
/// 2. HybridTransport (WS reads + POST writes) — 当 MOSSEN_CODE_POST_FOR_SESSION_INGRESS_V2 设置时
/// 3. WebSocketTransport (WS reads + WS writes) — 默认
pub fn get_transport_for_url(
    url: &Url,
    headers: HashMap<String, String>,
    session_id: Option<&str>,
    refresh_headers: Option<Box<dyn Fn() -> HashMap<String, String> + Send + Sync>>,
) -> Box<dyn Transport> {
    let use_ccr_v2 = std::env::var("MOSSEN_CODE_USE_CCR_V2")
        .map(|v| is_truthy(&v))
        .unwrap_or(false);

    if use_ccr_v2 {
        // v2: SSE for reads, HTTP POST for writes
        let mut sse_url = url.clone();
        match sse_url.scheme() {
            "wss" => {
                let _ = sse_url.set_scheme("https");
            }
            "ws" => {
                let _ = sse_url.set_scheme("http");
            }
            _ => {}
        }
        let path = sse_url.path().trim_end_matches('/').to_string();
        sse_url.set_path(&format!("{}/worker/events/stream", path));
        return Box::new(SSETransport::new(
            sse_url,
            headers,
            session_id.map(|s| s.to_string()),
            refresh_headers,
        ));
    }

    let scheme = url.scheme();
    if scheme == "ws" || scheme == "wss" {
        let use_hybrid = std::env::var("MOSSEN_CODE_POST_FOR_SESSION_INGRESS_V2")
            .map(|v| is_truthy(&v))
            .unwrap_or(false);

        if use_hybrid {
            return Box::new(HybridTransport::new(
                url.clone(),
                headers,
                session_id.map(|s| s.to_string()),
                refresh_headers,
            ));
        }
        return Box::new(WebSocketTransport::new(
            url.clone(),
            headers,
            session_id.map(|s| s.to_string()),
            refresh_headers,
        ));
    }

    panic!("Unsupported protocol: {}", scheme);
}

fn is_truthy(value: &str) -> bool {
    matches!(value, "1" | "true" | "yes" | "on")
}

// ─── WebSocketTransport ────────────────────────────────────────────────────

/// Construction options for [`WebSocketTransport`] — mirrors TS `WebSocketTransportOptions`.
#[derive(Debug, Clone, Default)]
pub struct WebSocketTransportOptions {
    /// When `false`, the transport does not attempt automatic reconnection on
    /// disconnect. Use this when the caller has its own recovery mechanism
    /// (e.g. the REPL bridge poll loop). Defaults to `true`.
    pub auto_reconnect: Option<bool>,
    /// Gates the `tengu_ws_transport_*` telemetry events. Set `true` at the
    /// REPL-bridge construction site so only Remote Control sessions (the
    /// Cloudflare-idle-timeout population) emit; print-mode workers stay
    /// silent. Defaults to `false`.
    pub is_bridge: Option<bool>,
}

/// WebSocket 传输 — 对应 TS 的 cli/transports/WebSocketTransport.ts。
///
/// 使用 WebSocket 同时进行读写。
pub struct WebSocketTransport {
    url: Url,
    headers: HashMap<String, String>,
    session_id: Option<String>,
    refresh_headers: Option<Box<dyn Fn() -> HashMap<String, String> + Send + Sync>>,
    on_data: Arc<RwLock<Option<Box<dyn Fn(String) + Send + Sync>>>>,
    on_close: Arc<RwLock<Option<Box<dyn Fn() + Send + Sync>>>>,
    on_event: Arc<RwLock<Option<Box<dyn Fn(serde_json::Value) + Send + Sync>>>>,
    write_tx: Arc<Mutex<Option<mpsc::Sender<String>>>>,
    closed: Arc<std::sync::atomic::AtomicBool>,
}

impl WebSocketTransport {
    pub fn new(
        url: Url,
        headers: HashMap<String, String>,
        session_id: Option<String>,
        refresh_headers: Option<Box<dyn Fn() -> HashMap<String, String> + Send + Sync>>,
    ) -> Self {
        Self {
            url,
            headers,
            session_id,
            refresh_headers,
            on_data: Arc::new(RwLock::new(None)),
            on_close: Arc::new(RwLock::new(None)),
            on_event: Arc::new(RwLock::new(None)),
            write_tx: Arc::new(Mutex::new(None)),
            closed: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// 构建 WebSocket 连接 URL（附加 session_id 参数）。
    fn build_connection_url(&self) -> Url {
        let mut url = self.url.clone();
        if let Some(ref sid) = self.session_id {
            url.query_pairs_mut()
                .append_pair("session_id", sid);
        }
        url
    }
}

#[async_trait::async_trait]
impl Transport for WebSocketTransport {
    async fn connect(&self) -> Result<()> {
        let url = self.build_connection_url();
        info!(url = %url, "WebSocketTransport: connecting");

        // Build request with headers
        let mut request = tokio_tungstenite::tungstenite::http::Request::builder()
            .uri(url.as_str());

        for (key, value) in &self.headers {
            request = request.header(key.as_str(), value.as_str());
        }

        let request = request
            .body(())
            .context("failed to build WebSocket request")?;

        let (ws_stream, _response) = tokio_tungstenite::connect_async(request)
            .await
            .context("WebSocket connection failed")?;

        let (write_half, mut read_half) = ws_stream.split();
        let write_half = Arc::new(Mutex::new(write_half));

        // Setup write channel
        let (tx, mut rx) = mpsc::channel::<String>(256);
        {
            let mut write_tx = self.write_tx.lock().await;
            *write_tx = Some(tx);
        }

        // Spawn write task
        let write_half_clone = write_half.clone();
        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                let mut writer = write_half_clone.lock().await;
                if let Err(e) = writer
                    .send(tokio_tungstenite::tungstenite::Message::Text(msg))
                    .await
                {
                    error!("WebSocketTransport: write error: {}", e);
                    break;
                }
            }
        });

        // Spawn read task
        let on_data = self.on_data.clone();
        let on_close = self.on_close.clone();
        let closed = self.closed.clone();

        tokio::spawn(async move {
            while let Some(msg_result) = read_half.next().await {
                match msg_result {
                    Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                        let cb = on_data.read().await;
                        if let Some(ref callback) = *cb {
                            callback(text.to_string());
                        }
                    }
                    Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => {
                        info!("WebSocketTransport: received close frame");
                        break;
                    }
                    Err(e) => {
                        error!("WebSocketTransport: read error: {}", e);
                        break;
                    }
                    _ => {}
                }
            }
            closed.store(true, std::sync::atomic::Ordering::SeqCst);
            let cb = on_close.read().await;
            if let Some(ref callback) = *cb {
                callback();
            }
        });

        info!("WebSocketTransport: connected");
        Ok(())
    }

    async fn write(&self, message: &serde_json::Value) -> Result<()> {
        let json = serde_json::to_string(message)?;
        let tx = self.write_tx.lock().await;
        if let Some(ref sender) = *tx {
            sender
                .send(json)
                .await
                .context("WebSocketTransport: write channel closed")?;
        }
        Ok(())
    }

    fn close(&self) {
        self.closed
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    fn set_on_data(&self, callback: Box<dyn Fn(String) + Send + Sync>) {
        let on_data = self.on_data.clone();
        tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                let mut guard = on_data.write().await;
                *guard = Some(callback);
            });
        });
    }

    fn set_on_close(&self, callback: Box<dyn Fn() + Send + Sync>) {
        let on_close = self.on_close.clone();
        tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                let mut guard = on_close.write().await;
                *guard = Some(callback);
            });
        });
    }

    fn set_on_event(&self, callback: Box<dyn Fn(serde_json::Value) + Send + Sync>) {
        let on_event = self.on_event.clone();
        tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                let mut guard = on_event.write().await;
                *guard = Some(callback);
            });
        });
    }
}

// ─── SSETransport ──────────────────────────────────────────────────────────

/// SSE 传输 — 对应 TS 的 cli/transports/SSETransport.ts。
///
/// 使用 Server-Sent Events 读取数据，HTTP POST 写入。
pub struct SSETransport {
    url: Url,
    headers: HashMap<String, String>,
    session_id: Option<String>,
    refresh_headers: Option<Box<dyn Fn() -> HashMap<String, String> + Send + Sync>>,
    on_data: Arc<RwLock<Option<Box<dyn Fn(String) + Send + Sync>>>>,
    on_close: Arc<RwLock<Option<Box<dyn Fn() + Send + Sync>>>>,
    on_event: Arc<RwLock<Option<Box<dyn Fn(serde_json::Value) + Send + Sync>>>>,
    client: reqwest::Client,
    closed: Arc<std::sync::atomic::AtomicBool>,
}

impl SSETransport {
    pub fn new(
        url: Url,
        headers: HashMap<String, String>,
        session_id: Option<String>,
        refresh_headers: Option<Box<dyn Fn() -> HashMap<String, String> + Send + Sync>>,
    ) -> Self {
        Self {
            url,
            headers,
            session_id,
            refresh_headers,
            on_data: Arc::new(RwLock::new(None)),
            on_close: Arc::new(RwLock::new(None)),
            on_event: Arc::new(RwLock::new(None)),
            client: reqwest::Client::new(),
            closed: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// 获取 POST 写入的 URL（从 SSE stream URL 推导）。
    fn write_url(&self) -> Url {
        let mut url = self.url.clone();
        // Replace /worker/events/stream with /worker/events
        let path = url.path().replace("/worker/events/stream", "/worker/events");
        url.set_path(&path);
        url
    }

    /// 获取最新的请求头（支持动态 token 刷新）。
    fn current_headers(&self) -> HashMap<String, String> {
        if let Some(ref refresh) = self.refresh_headers {
            refresh()
        } else {
            self.headers.clone()
        }
    }
}

#[async_trait::async_trait]
impl Transport for SSETransport {
    async fn connect(&self) -> Result<()> {
        info!(url = %self.url, "SSETransport: connecting");

        let mut request = self.client.get(self.url.as_str());
        for (key, value) in &self.headers {
            request = request.header(key.as_str(), value.as_str());
        }
        request = request.header("Accept", "text/event-stream");

        let response = request.send().await.context("SSE connection failed")?;

        if !response.status().is_success() {
            anyhow::bail!("SSE connection failed with status: {}", response.status());
        }

        let on_data = self.on_data.clone();
        let on_close = self.on_close.clone();
        let on_event = self.on_event.clone();
        let closed = self.closed.clone();

        // Spawn SSE reader task
        tokio::spawn(async move {
            let mut stream = response.bytes_stream();
            let mut buffer = String::new();

            while let Some(chunk_result) = stream.next().await {
                if closed.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }
                match chunk_result {
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes);
                        buffer.push_str(&text);

                        // Process SSE lines
                        while let Some(newline_pos) = buffer.find('\n') {
                            let line = buffer[..newline_pos].trim_end().to_string();
                            buffer = buffer[newline_pos + 1..].to_string();

                            if line.starts_with("data: ") {
                                let data = &line[6..];
                                // Try to parse as JSON event
                                if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                                    let cb = on_event.read().await;
                                    if let Some(ref callback) = *cb {
                                        callback(json);
                                    }
                                }
                                let cb = on_data.read().await;
                                if let Some(ref callback) = *cb {
                                    callback(data.to_string() + "\n");
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("SSETransport: read error: {}", e);
                        break;
                    }
                }
            }

            closed.store(true, std::sync::atomic::Ordering::SeqCst);
            let cb = on_close.read().await;
            if let Some(ref callback) = *cb {
                callback();
            }
        });

        info!("SSETransport: connected");
        Ok(())
    }

    async fn write(&self, message: &serde_json::Value) -> Result<()> {
        let write_url = self.write_url();
        let headers = self.current_headers();

        let mut request = self.client.post(write_url.as_str());
        for (key, value) in &headers {
            request = request.header(key.as_str(), value.as_str());
        }
        request = request.header("Content-Type", "application/json");
        request = request.json(message);

        let response = request.send().await.context("SSE POST write failed")?;
        if !response.status().is_success() {
            warn!(
                "SSETransport: write returned status {}",
                response.status()
            );
        }
        Ok(())
    }

    fn close(&self) {
        self.closed
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    fn set_on_data(&self, callback: Box<dyn Fn(String) + Send + Sync>) {
        let on_data = self.on_data.clone();
        tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                let mut guard = on_data.write().await;
                *guard = Some(callback);
            });
        });
    }

    fn set_on_close(&self, callback: Box<dyn Fn() + Send + Sync>) {
        let on_close = self.on_close.clone();
        tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                let mut guard = on_close.write().await;
                *guard = Some(callback);
            });
        });
    }

    fn set_on_event(&self, callback: Box<dyn Fn(serde_json::Value) + Send + Sync>) {
        let on_event = self.on_event.clone();
        tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                let mut guard = on_event.write().await;
                *guard = Some(callback);
            });
        });
    }
}

// ─── HybridTransport ───────────────────────────────────────────────────────

/// Hybrid 传输 — 对应 TS 的 cli/transports/HybridTransport.ts。
///
/// 使用 WebSocket 读取、HTTP POST 写入的混合模式。
pub struct HybridTransport {
    url: Url,
    headers: HashMap<String, String>,
    session_id: Option<String>,
    refresh_headers: Option<Box<dyn Fn() -> HashMap<String, String> + Send + Sync>>,
    on_data: Arc<RwLock<Option<Box<dyn Fn(String) + Send + Sync>>>>,
    on_close: Arc<RwLock<Option<Box<dyn Fn() + Send + Sync>>>>,
    on_event: Arc<RwLock<Option<Box<dyn Fn(serde_json::Value) + Send + Sync>>>>,
    client: reqwest::Client,
    closed: Arc<std::sync::atomic::AtomicBool>,
}

impl HybridTransport {
    pub fn new(
        url: Url,
        headers: HashMap<String, String>,
        session_id: Option<String>,
        refresh_headers: Option<Box<dyn Fn() -> HashMap<String, String> + Send + Sync>>,
    ) -> Self {
        Self {
            url,
            headers,
            session_id,
            refresh_headers,
            on_data: Arc::new(RwLock::new(None)),
            on_close: Arc::new(RwLock::new(None)),
            on_event: Arc::new(RwLock::new(None)),
            client: reqwest::Client::new(),
            closed: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// 获取 POST 写入的 HTTP URL。
    fn write_url(&self) -> Url {
        let mut url = self.url.clone();
        match url.scheme() {
            "wss" => {
                let _ = url.set_scheme("https");
            }
            "ws" => {
                let _ = url.set_scheme("http");
            }
            _ => {}
        }
        url
    }

    /// 获取最新的请求头。
    fn current_headers(&self) -> HashMap<String, String> {
        if let Some(ref refresh) = self.refresh_headers {
            refresh()
        } else {
            self.headers.clone()
        }
    }
}

#[async_trait::async_trait]
impl Transport for HybridTransport {
    async fn connect(&self) -> Result<()> {
        info!(url = %self.url, "HybridTransport: connecting WebSocket for reads");

        let mut ws_url = self.url.clone();
        if let Some(ref sid) = self.session_id {
            ws_url.query_pairs_mut().append_pair("session_id", sid);
        }

        let mut request = tokio_tungstenite::tungstenite::http::Request::builder()
            .uri(ws_url.as_str());
        for (key, value) in &self.headers {
            request = request.header(key.as_str(), value.as_str());
        }
        let request = request.body(()).context("failed to build WS request")?;

        let (ws_stream, _) = tokio_tungstenite::connect_async(request)
            .await
            .context("HybridTransport WS connection failed")?;

        let (_write_half, mut read_half) = ws_stream.split();
        let on_data = self.on_data.clone();
        let on_close = self.on_close.clone();
        let closed = self.closed.clone();

        tokio::spawn(async move {
            while let Some(msg_result) = read_half.next().await {
                if closed.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }
                match msg_result {
                    Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                        let cb = on_data.read().await;
                        if let Some(ref callback) = *cb {
                            callback(text.to_string());
                        }
                    }
                    Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => break,
                    Err(e) => {
                        error!("HybridTransport: WS read error: {}", e);
                        break;
                    }
                    _ => {}
                }
            }
            closed.store(true, std::sync::atomic::Ordering::SeqCst);
            let cb = on_close.read().await;
            if let Some(ref callback) = *cb {
                callback();
            }
        });

        info!("HybridTransport: connected");
        Ok(())
    }

    async fn write(&self, message: &serde_json::Value) -> Result<()> {
        let write_url = self.write_url();
        let headers = self.current_headers();

        let mut request = self.client.post(write_url.as_str());
        for (key, value) in &headers {
            request = request.header(key.as_str(), value.as_str());
        }
        request = request.header("Content-Type", "application/json");
        request = request.json(message);

        let response = request.send().await.context("HybridTransport POST failed")?;
        if !response.status().is_success() {
            warn!(
                "HybridTransport: write returned status {}",
                response.status()
            );
        }
        Ok(())
    }

    fn close(&self) {
        self.closed
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    fn set_on_data(&self, callback: Box<dyn Fn(String) + Send + Sync>) {
        let on_data = self.on_data.clone();
        tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                let mut guard = on_data.write().await;
                *guard = Some(callback);
            });
        });
    }

    fn set_on_close(&self, callback: Box<dyn Fn() + Send + Sync>) {
        let on_close = self.on_close.clone();
        tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                let mut guard = on_close.write().await;
                *guard = Some(callback);
            });
        });
    }

    fn set_on_event(&self, callback: Box<dyn Fn(serde_json::Value) + Send + Sync>) {
        let on_event = self.on_event.clone();
        tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                let mut guard = on_event.write().await;
                *guard = Some(callback);
            });
        });
    }
}

// ─── SerialBatchEventUploader ──────────────────────────────────────────────

/// 串行批量事件上传器 — 对应 TS 的 cli/transports/SerialBatchEventUploader.ts。
///
/// 按顺序批量上传事件，避免并发写入冲突。
pub struct SerialBatchEventUploader {
    /// POST 目标 URL。
    url: Url,
    /// 请求头。
    headers: HashMap<String, String>,
    /// HTTP 客户端。
    client: reqwest::Client,
    /// 待发送队列。
    queue: Arc<Mutex<Vec<serde_json::Value>>>,
    /// 是否正在发送。
    flushing: Arc<std::sync::atomic::AtomicBool>,
    /// 已关闭标记。
    closed: Arc<std::sync::atomic::AtomicBool>,
}

impl SerialBatchEventUploader {
    pub fn new(url: Url, headers: HashMap<String, String>) -> Self {
        Self {
            url,
            headers,
            client: reqwest::Client::new(),
            queue: Arc::new(Mutex::new(Vec::new())),
            flushing: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            closed: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// 入队事件。
    pub async fn enqueue(&self, event: serde_json::Value) {
        if self.closed.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        let mut queue = self.queue.lock().await;
        queue.push(event);
        drop(queue);
        self.try_flush().await;
    }

    /// 尝试刷新队列（如果当前没有在刷新中）。
    async fn try_flush(&self) {
        if self
            .flushing
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::Relaxed,
            )
            .is_err()
        {
            return; // 已有刷新在进行中
        }

        loop {
            let batch = {
                let mut queue = self.queue.lock().await;
                if queue.is_empty() {
                    self.flushing
                        .store(false, std::sync::atomic::Ordering::SeqCst);
                    return;
                }
                std::mem::take(&mut *queue)
            };

            if let Err(e) = self.send_batch(&batch).await {
                error!("SerialBatchEventUploader: send failed: {}", e);
                // Re-queue failed items at the front
                let mut queue = self.queue.lock().await;
                let mut combined = batch;
                combined.append(&mut *queue);
                *queue = combined;
                self.flushing
                    .store(false, std::sync::atomic::Ordering::SeqCst);
                return;
            }
        }
    }

    /// 发送一批事件。
    async fn send_batch(&self, batch: &[serde_json::Value]) -> Result<()> {
        let payload = serde_json::json!({ "events": batch });
        let mut request = self.client.post(self.url.as_str());
        for (key, value) in &self.headers {
            request = request.header(key.as_str(), value.as_str());
        }
        request = request
            .header("Content-Type", "application/json")
            .json(&payload);

        let response = request.send().await?;
        if !response.status().is_success() {
            anyhow::bail!("batch upload failed with status {}", response.status());
        }
        Ok(())
    }

    /// 刷新所有待发送事件（graceful shutdown 时调用）。
    pub async fn flush_all(&self) -> Result<()> {
        let batch = {
            let mut queue = self.queue.lock().await;
            std::mem::take(&mut *queue)
        };
        if !batch.is_empty() {
            self.send_batch(&batch).await?;
        }
        Ok(())
    }

    /// 关闭上传器。
    pub async fn close(&self) {
        self.closed
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let _ = self.flush_all().await;
    }
}

// ─── WorkerStateUploader ───────────────────────────────────────────────────

/// Worker 状态上传器 — 对应 TS 的 cli/transports/WorkerStateUploader.ts。
///
/// 定期上传 worker 状态到服务端（用于远程模式的心跳和状态同步）。
pub struct WorkerStateUploader {
    /// POST 目标 URL。
    url: Url,
    /// 请求头。
    headers: HashMap<String, String>,
    /// HTTP 客户端。
    client: reqwest::Client,
    /// 停止通知。
    stop_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl WorkerStateUploader {
    pub fn new(url: Url, headers: HashMap<String, String>) -> Self {
        Self {
            url,
            headers,
            client: reqwest::Client::new(),
            stop_tx: None,
        }
    }

    /// 启动定期状态上传。
    pub fn start(&mut self, interval_ms: u64) {
        let (tx, mut rx) = tokio::sync::oneshot::channel();
        self.stop_tx = Some(tx);

        let url = self.url.clone();
        let headers = self.headers.clone();
        let client = self.client.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(interval_ms));
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let state = serde_json::json!({
                            "type": "heartbeat",
                            "timestamp": chrono::Utc::now().timestamp_millis(),
                        });
                        let mut request = client.post(url.as_str());
                        for (key, value) in &headers {
                            request = request.header(key.as_str(), value.as_str());
                        }
                        request = request
                            .header("Content-Type", "application/json")
                            .json(&state);
                        if let Err(e) = request.send().await {
                            warn!("WorkerStateUploader: heartbeat failed: {}", e);
                        }
                    }
                    _ = &mut rx => {
                        info!("WorkerStateUploader: stopped");
                        break;
                    }
                }
            }
        });
    }

    /// 停止上传。
    pub fn stop(&mut self) {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
    }
}

// ─── CCRClient ─────────────────────────────────────────────────────────────

/// CCR 初始化错误类型 — 对应 TS 的 CCRInitError。
#[derive(Debug, thiserror::Error)]
#[error("CCR initialization failed: {reason}")]
pub struct CCRInitError {
    pub reason: String,
}

/// CCR v2 客户端 — 对应 TS 的 cli/transports/ccrClient.ts。
///
/// 管理 CCR (Command-Control-Relay) v2 协议：
/// - 心跳 (heartbeat)
/// - 纪元管理 (epoch)
/// - 状态上报 (state reporting)
/// - 事件读写 (event read/write)
/// - 交付确认 (delivery ack)
pub struct CCRClient {
    /// SSE Transport 引用。
    transport: Arc<dyn Transport>,
    /// 基础 URL（session URL）。
    base_url: Url,
    /// HTTP 客户端。
    client: reqwest::Client,
    /// 当前纪元 ID。
    epoch_id: Arc<RwLock<Option<String>>>,
    /// 内部事件队列。
    internal_event_queue: Arc<Mutex<Vec<InternalEvent>>>,
    /// 批量上传器。
    uploader: Arc<SerialBatchEventUploader>,
    /// 停止通知。
    stop_tx: Arc<Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
    /// 是否已关闭。
    closed: Arc<std::sync::atomic::AtomicBool>,
}

/// 内部事件结构。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InternalEvent {
    pub event_type: String,
    pub payload: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<serde_json::Value>,
}

/// 会话外部元数据 — 对应 TS 的 SessionExternalMetadata。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionExternalMetadata {
    #[serde(default)]
    pub permission_mode: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub custom_state: Option<serde_json::Value>,
}

impl CCRClient {
    pub fn new(transport: Arc<dyn Transport>, base_url: Url) -> Self {
        // Derive the event upload URL
        let mut upload_url = base_url.clone();
        let path = upload_url.path().trim_end_matches('/').to_string();
        upload_url.set_path(&format!("{}/worker/events", path));

        let uploader = Arc::new(SerialBatchEventUploader::new(
            upload_url,
            HashMap::new(),
        ));

        Self {
            transport,
            base_url,
            client: reqwest::Client::new(),
            epoch_id: Arc::new(RwLock::new(None)),
            internal_event_queue: Arc::new(Mutex::new(Vec::new())),
            uploader,
            stop_tx: Arc::new(Mutex::new(None)),
            closed: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// 初始化 CCR 连接 — 获取 epoch、启动心跳。
    pub async fn initialize(&self) -> Result<SessionExternalMetadata> {
        info!("CCRClient: initializing");

        // 1. Fetch epoch
        let epoch_url = {
            let mut url = self.base_url.clone();
            let path = url.path().trim_end_matches('/').to_string();
            url.set_path(&format!("{}/worker/epoch", path));
            url
        };

        let response = self
            .client
            .post(epoch_url.as_str())
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({}))
            .send()
            .await
            .map_err(|e| CCRInitError {
                reason: format!("epoch fetch failed: {}", e),
            })
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "CCR epoch fetch failed with status: {}",
                response.status()
            ));
        }

        let epoch_body: serde_json::Value = response.json().await?;
        let epoch_id = epoch_body["epoch_id"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();

        {
            let mut guard = self.epoch_id.write().await;
            *guard = Some(epoch_id.clone());
        }

        info!(epoch_id = %epoch_id, "CCRClient: epoch acquired");

        // 2. Fetch external metadata (worker state)
        let metadata = self.fetch_external_metadata().await.unwrap_or_default();

        // 3. Start heartbeat
        self.start_heartbeat().await;

        info!("CCRClient: initialized");
        Ok(metadata)
    }

    /// 获取外部元数据。
    async fn fetch_external_metadata(&self) -> Result<SessionExternalMetadata> {
        let mut url = self.base_url.clone();
        let path = url.path().trim_end_matches('/').to_string();
        url.set_path(&format!("{}/worker/metadata", path));

        let response = self
            .client
            .get(url.as_str())
            .send()
            .await
            .context("failed to fetch external metadata")?;

        if !response.status().is_success() {
            return Ok(SessionExternalMetadata::default());
        }

        let metadata: SessionExternalMetadata = response.json().await.unwrap_or_default();
        Ok(metadata)
    }

    /// 启动心跳任务。
    async fn start_heartbeat(&self) {
        let (tx, mut rx) = tokio::sync::oneshot::channel();
        {
            let mut guard = self.stop_tx.lock().await;
            *guard = Some(tx);
        }

        let base_url = self.base_url.clone();
        let client = self.client.clone();
        let epoch_id = self.epoch_id.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let mut url = base_url.clone();
                        let path = url.path().trim_end_matches('/').to_string();
                        url.set_path(&format!("{}/worker/heartbeat", path));

                        let current_epoch = {
                            let guard = epoch_id.read().await;
                            guard.clone()
                        };

                        let body = serde_json::json!({
                            "epoch_id": current_epoch,
                            "timestamp": chrono::Utc::now().timestamp_millis(),
                        });

                        if let Err(e) = client.post(url.as_str())
                            .header("Content-Type", "application/json")
                            .json(&body)
                            .send()
                            .await
                        {
                            warn!("CCRClient: heartbeat failed: {}", e);
                        }
                    }
                    _ = &mut rx => {
                        info!("CCRClient: heartbeat stopped");
                        break;
                    }
                }
            }
        });
    }

    /// 写入事件。
    pub async fn write_event(&self, message: &serde_json::Value) -> Result<()> {
        self.uploader.enqueue(message.clone()).await;
        Ok(())
    }

    /// 写入内部事件。
    pub async fn write_internal_event(
        &self,
        event_type: &str,
        payload: serde_json::Value,
        options: Option<serde_json::Value>,
    ) -> Result<()> {
        let event = InternalEvent {
            event_type: event_type.to_string(),
            payload,
            options,
        };
        let mut queue = self.internal_event_queue.lock().await;
        queue.push(event.clone());
        drop(queue);

        // Also enqueue for upload
        self.uploader
            .enqueue(serde_json::to_value(&event)?)
            .await;
        Ok(())
    }

    /// 刷新内部事件队列。
    pub async fn flush_internal_events(&self) -> Result<()> {
        self.uploader.flush_all().await
    }

    /// 获取待发送内部事件数量。
    pub async fn internal_events_pending(&self) -> usize {
        let queue = self.internal_event_queue.lock().await;
        queue.len()
    }

    /// 读取内部事件（用于会话恢复）。
    pub async fn read_internal_events(&self) -> Result<Vec<InternalEvent>> {
        let mut url = self.base_url.clone();
        let path = url.path().trim_end_matches('/').to_string();
        url.set_path(&format!("{}/worker/internal-events", path));

        let response = self
            .client
            .get(url.as_str())
            .send()
            .await
            .context("failed to read internal events")?;

        if !response.status().is_success() {
            return Ok(Vec::new());
        }

        let events: Vec<InternalEvent> = response.json().await.unwrap_or_default();
        Ok(events)
    }

    /// 读取子 Agent 内部事件。
    pub async fn read_subagent_internal_events(&self) -> Result<Vec<InternalEvent>> {
        let mut url = self.base_url.clone();
        let path = url.path().trim_end_matches('/').to_string();
        url.set_path(&format!("{}/worker/subagent-internal-events", path));

        let response = self
            .client
            .get(url.as_str())
            .send()
            .await
            .context("failed to read subagent internal events")?;

        if !response.status().is_success() {
            return Ok(Vec::new());
        }

        let events: Vec<InternalEvent> = response.json().await.unwrap_or_default();
        Ok(events)
    }

    /// 上报交付状态。
    pub async fn report_delivery(&self, uuid: &str, status: &str) {
        let mut url = self.base_url.clone();
        let path = url.path().trim_end_matches('/').to_string();
        url.set_path(&format!("{}/worker/delivery", path));

        let body = serde_json::json!({
            "uuid": uuid,
            "status": status,
        });

        if let Err(e) = self
            .client
            .post(url.as_str())
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
        {
            warn!("CCRClient: report_delivery failed: {}", e);
        }
    }

    /// 上报会话状态。
    pub async fn report_state(&self, state: &str, details: Option<&serde_json::Value>) {
        let mut url = self.base_url.clone();
        let path = url.path().trim_end_matches('/').to_string();
        url.set_path(&format!("{}/worker/state", path));

        let body = serde_json::json!({
            "state": state,
            "details": details,
        });

        if let Err(e) = self
            .client
            .post(url.as_str())
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
        {
            warn!("CCRClient: report_state failed: {}", e);
        }
    }

    /// 上报会话元数据。
    pub async fn report_metadata(&self, metadata: &serde_json::Value) {
        let mut url = self.base_url.clone();
        let path = url.path().trim_end_matches('/').to_string();
        url.set_path(&format!("{}/worker/metadata", path));

        if let Err(e) = self
            .client
            .put(url.as_str())
            .header("Content-Type", "application/json")
            .json(metadata)
            .send()
            .await
        {
            warn!("CCRClient: report_metadata failed: {}", e);
        }
    }

    /// 关闭 CCR 客户端。
    pub async fn close(&self) {
        if self
            .closed
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::Relaxed,
            )
            .is_ok()
        {
            info!("CCRClient: closing");
            let _ = self.flush_internal_events().await;
            self.uploader.close().await;
            let mut guard = self.stop_tx.lock().await;
            if let Some(tx) = guard.take() {
                let _ = tx.send(());
            }
        }
    }
}
