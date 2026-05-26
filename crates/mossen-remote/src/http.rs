//! # http — HTTP/SSE 传输层
//!
//! 实现 SSE 读取 + HTTP POST 写入的传输模式。
//! 对应 TS `cli/transports/SSETransport.ts` 和 `cli/transports/HybridTransport.ts`。

use crate::connection::is_permanent_http_code;
use crate::ndjson::{ndjson_safe_stringify, parse_sse_frames};
use crate::transport::{
    OnCloseCallback, OnConnectCallback, OnDataCallback, OnEventCallback, StdoutMessage,
    StreamClientEvent, Transport, TransportState,
};
use async_trait::async_trait;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, RwLock};
use tracing;

/// 重连基础延迟。
#[allow(dead_code)]
const RECONNECT_BASE_DELAY_MS: u64 = 1000;
/// 重连最大延迟。
#[allow(dead_code)]
const RECONNECT_MAX_DELAY_MS: u64 = 30_000;
/// 重连时间预算（10 分钟）。
#[allow(dead_code)]
const RECONNECT_GIVE_UP_MS: u64 = 600_000;
/// 活跃性超时（45 秒无数据视为连接断开）。
#[allow(dead_code)]
const LIVENESS_TIMEOUT_MS: u64 = 45_000;
/// POST 最大重试次数。
const POST_MAX_RETRIES: u32 = 10;
/// POST 基础延迟。
const POST_BASE_DELAY_MS: u64 = 500;
/// POST 最大延迟。
const POST_MAX_DELAY_MS: u64 = 8000;

/// SSE 传输层。
///
/// 使用 Server-Sent Events 读取数据，HTTP POST 写入数据。
#[allow(dead_code)]
pub struct SseTransport {
    /// SSE 流 URL。
    sse_url: url::Url,
    /// POST 写入 URL。
    post_url: String,
    /// 请求头。
    headers: HashMap<String, String>,
    /// 会话 ID。
    session_id: Option<String>,
    /// 当前状态。
    state: Arc<RwLock<TransportState>>,
    /// 数据回调。
    on_data: Arc<RwLock<Option<OnDataCallback>>>,
    /// 事件回调（SSE 事件类型识别）。
    on_event: Arc<RwLock<Option<OnEventCallback>>>,
    /// 关闭回调。
    on_close: Arc<RwLock<Option<OnCloseCallback>>>,
    /// 连接回调。
    on_connect: Arc<RwLock<Option<OnConnectCallback>>>,
    /// HTTP 客户端。
    client: reqwest::Client,
    /// 关闭信号。
    shutdown_tx: Arc<Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
    /// 上次确认的序列号。
    last_sequence_num: Arc<RwLock<Option<u64>>>,
    /// 已接收序列号集合（用于去重）。
    seen_sequence_nums: Arc<RwLock<HashSet<u64>>>,
    /// 动态刷新头部的回调。
    refresh_headers: Option<Arc<dyn Fn() -> HashMap<String, String> + Send + Sync>>,
}

impl SseTransport {
    /// 创建新的 SSE 传输层。
    pub fn new(
        sse_url: url::Url,
        headers: HashMap<String, String>,
        session_id: Option<String>,
    ) -> Self {
        let mut post_url = sse_url.clone();
        // 将 SSE 流 URL 转换为 POST URL
        let path = post_url.path().trim_end_matches('/');
        let post_path = path.strip_suffix("/worker/events/stream").unwrap_or(path);
        post_url.set_path(&format!("{}/worker/events", post_path));

        Self {
            sse_url,
            post_url: post_url.to_string(),
            headers,
            session_id,
            state: Arc::new(RwLock::new(TransportState::Idle)),
            on_data: Arc::new(RwLock::new(None)),
            on_event: Arc::new(RwLock::new(None)),
            on_close: Arc::new(RwLock::new(None)),
            on_connect: Arc::new(RwLock::new(None)),
            client: reqwest::Client::new(),
            shutdown_tx: Arc::new(Mutex::new(None)),
            last_sequence_num: Arc::new(RwLock::new(None)),
            seen_sequence_nums: Arc::new(RwLock::new(HashSet::new())),
            refresh_headers: None,
        }
    }

    /// 设置事件回调。
    pub fn set_on_event(&mut self, callback: OnEventCallback) {
        let on_event = self.on_event.clone();
        tokio::spawn(async move {
            *on_event.write().await = Some(callback);
        });
    }

    /// 设置动态刷新头部回调。
    pub fn set_refresh_headers(
        &mut self,
        callback: impl Fn() -> HashMap<String, String> + Send + Sync + 'static,
    ) {
        self.refresh_headers = Some(Arc::new(callback));
    }

    /// 获取当前请求头。
    fn current_headers(&self) -> HashMap<String, String> {
        if let Some(ref refresh) = self.refresh_headers {
            refresh()
        } else {
            self.headers.clone()
        }
    }

    /// 通过 HTTP POST 发送消息（带重试）。
    async fn post_with_retry(&self, body: &str) -> anyhow::Result<()> {
        let mut attempt = 0;
        loop {
            let headers = self.current_headers();
            let mut req = self.client.post(&self.post_url).body(body.to_string());
            req = req.header("Content-Type", "application/json");
            for (key, value) in &headers {
                req = req.header(key.as_str(), value.as_str());
            }

            match req.send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        return Ok(());
                    }
                    let status = resp.status().as_u16();
                    if is_permanent_http_code(status) {
                        anyhow::bail!("SSETransport: permanent HTTP error {} on POST", status);
                    }
                    attempt += 1;
                    if attempt >= POST_MAX_RETRIES {
                        anyhow::bail!(
                            "SSETransport: POST failed after {} retries (last status: {})",
                            POST_MAX_RETRIES,
                            status
                        );
                    }
                }
                Err(e) => {
                    attempt += 1;
                    if attempt >= POST_MAX_RETRIES {
                        return Err(e.into());
                    }
                }
            }

            let delay = Duration::from_millis(
                POST_BASE_DELAY_MS.min(POST_MAX_DELAY_MS) * 2u64.pow(attempt.min(10)),
            );
            tokio::time::sleep(delay.min(Duration::from_millis(POST_MAX_DELAY_MS))).await;
        }
    }
}

#[async_trait]
impl Transport for SseTransport {
    async fn connect(&mut self) -> anyhow::Result<()> {
        *self.state.write().await = TransportState::Connecting;

        let headers = self.current_headers();
        let mut req = self
            .client
            .get(self.sse_url.as_str())
            .header("Accept", "text/event-stream")
            .header("Cache-Control", "no-cache");

        for (key, value) in &headers {
            req = req.header(key.as_str(), value.as_str());
        }

        // 添加 Last-Event-ID 以支持断点续传
        if let Some(seq) = *self.last_sequence_num.read().await {
            req = req.header("Last-Event-ID", seq.to_string());
        }

        let response = req.send().await?;
        let status = response.status().as_u16();

        if is_permanent_http_code(status) {
            *self.state.write().await = TransportState::Closed;
            anyhow::bail!("SSETransport: permanent HTTP {} on connect", status);
        }

        if !response.status().is_success() {
            anyhow::bail!("SSETransport: HTTP {} on connect", status);
        }

        *self.state.write().await = TransportState::Connected;
        if let Some(ref cb) = *self.on_connect.read().await {
            cb();
        }

        // 启动 SSE 读取循环
        let on_data = self.on_data.clone();
        let on_event = self.on_event.clone();
        let on_close = self.on_close.clone();
        let state = self.state.clone();
        let last_seq = self.last_sequence_num.clone();
        let seen_seqs = self.seen_sequence_nums.clone();
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        *self.shutdown_tx.lock().await = Some(shutdown_tx);

        tokio::spawn(async move {
            let mut buffer = String::new();
            use futures::StreamExt;
            let mut byte_stream = response.bytes_stream();
            let mut shutdown_rx = shutdown_rx;

            loop {
                tokio::select! {
                    chunk = byte_stream.next() => {
                        match chunk {
                            Some(Ok(bytes)) => {
                                let bytes: bytes::Bytes = bytes;
                                if let Ok(text) = std::str::from_utf8(&bytes) {
                                    buffer.push_str(text);
                                    let (frames, remaining) = parse_sse_frames(&buffer);
                                    buffer = remaining;

                                    for frame in frames {
                                        // 去重处理
                                        if let Some(ref id_str) = frame.id {
                                            if let Ok(seq) = id_str.parse::<u64>() {
                                                let mut seen = seen_seqs.write().await;
                                                if seen.contains(&seq) {
                                                    continue;
                                                }
                                                seen.insert(seq);
                                                *last_seq.write().await = Some(seq);
                                            }
                                        }

                                        // 分发数据
                                        if let Some(ref data) = frame.data {
                                            if let Some(ref cb) = *on_data.read().await {
                                                cb(data.clone());
                                            }
                                        }

                                        // 分发事件
                                        if let Some(ref cb) = *on_event.read().await {
                                            cb(StreamClientEvent {
                                                event_type: frame.event,
                                                id: frame.id,
                                                data: frame.data,
                                            });
                                        }
                                    }
                                }
                            }
                            Some(Err(e)) => {
                                tracing::error!("SSETransport: stream error: {}", e);
                                break;
                            }
                            None => {
                                tracing::debug!("SSETransport: stream ended");
                                break;
                            }
                        }
                    }
                    _ = &mut shutdown_rx => {
                        tracing::debug!("SSETransport: shutdown signal");
                        return;
                    }
                }
            }

            *state.write().await = TransportState::Closed;
            if let Some(ref cb) = *on_close.read().await {
                cb(None);
            }
        });

        Ok(())
    }

    async fn write(&self, message: StdoutMessage) -> anyhow::Result<()> {
        let json = ndjson_safe_stringify(&message)?;
        self.post_with_retry(&json).await
    }

    fn close(&mut self) {
        let state = self.state.clone();
        let shutdown_tx = self.shutdown_tx.clone();
        tokio::spawn(async move {
            *state.write().await = TransportState::Closed;
            if let Some(tx) = shutdown_tx.lock().await.take() {
                let _ = tx.send(());
            }
        });
    }

    fn state(&self) -> TransportState {
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

/// 串行批量事件上传器。
///
/// 确保事件按顺序发送，支持批处理、重试和背压。
/// 对应 TS `cli/transports/SerialBatchEventUploader.ts`。
#[allow(dead_code)]
pub struct SerialBatchEventUploader<T: Send + 'static> {
    /// 待发送队列。
    pending: Arc<Mutex<Vec<T>>>,
    /// 最大批次大小。
    max_batch_size: usize,
    /// 最大队列大小。
    max_queue_size: usize,
    /// 是否已关闭。
    closed: Arc<RwLock<bool>>,
    /// 发送回调。
    send_fn: Arc<
        dyn Fn(Vec<T>) -> futures::future::BoxFuture<'static, anyhow::Result<()>> + Send + Sync,
    >,
    /// 基础重试延迟。
    base_delay_ms: u64,
    /// 最大重试延迟。
    max_delay_ms: u64,
    /// 抖动范围（毫秒）。
    jitter_ms: u64,
    /// 最大连续失败次数（超过则丢弃批次）。
    max_consecutive_failures: Option<u32>,
}

/// 工作状态上传器。
///
/// 合并式上传，最多 1 个飞行中请求 + 1 个待处理补丁。
/// 对应 TS `cli/transports/WorkerStateUploader.ts`。
#[allow(dead_code)]
pub struct WorkerStateUploader {
    /// 是否已关闭。
    closed: Arc<RwLock<bool>>,
    /// 待处理补丁。
    pending: Arc<Mutex<Option<serde_json::Value>>>,
    /// 发送回调。
    send_fn: Arc<
        dyn Fn(serde_json::Value) -> futures::future::BoxFuture<'static, anyhow::Result<bool>>
            + Send
            + Sync,
    >,
    /// 基础重试延迟。
    base_delay_ms: u64,
    /// 最大重试延迟。
    max_delay_ms: u64,
    /// 抖动范围。
    jitter_ms: u64,
}

impl WorkerStateUploader {
    /// 创建新的工作状态上传器。
    pub fn new(
        send_fn: impl Fn(serde_json::Value) -> futures::future::BoxFuture<'static, anyhow::Result<bool>>
            + Send
            + Sync
            + 'static,
        base_delay_ms: u64,
        max_delay_ms: u64,
        jitter_ms: u64,
    ) -> Self {
        Self {
            closed: Arc::new(RwLock::new(false)),
            pending: Arc::new(Mutex::new(None)),
            send_fn: Arc::new(send_fn),
            base_delay_ms,
            max_delay_ms,
            jitter_ms,
        }
    }

    /// 入队补丁（合并到待处理）。
    pub async fn enqueue(&self, patch: serde_json::Value) {
        if *self.closed.read().await {
            return;
        }
        let mut pending = self.pending.lock().await;
        *pending = Some(match pending.take() {
            Some(existing) => coalesce_patches(existing, patch),
            None => patch,
        });
    }

    /// 关闭上传器。
    pub async fn close(&self) {
        *self.closed.write().await = true;
        *self.pending.lock().await = None;
    }
}

/// 合并两个 PUT /worker 补丁。
///
/// 顶层键：覆盖替换。
/// 元数据键（`external_metadata`, `internal_metadata`）：RFC 7396 单层合并。
fn coalesce_patches(base: serde_json::Value, overlay: serde_json::Value) -> serde_json::Value {
    use serde_json::Value;

    match (base, overlay) {
        (Value::Object(mut base_map), Value::Object(overlay_map)) => {
            for (key, value) in overlay_map {
                if (key == "external_metadata" || key == "internal_metadata")
                    && base_map.get(&key).is_some_and(|v| v.is_object())
                    && value.is_object()
                {
                    if let Some(Value::Object(ref mut bm)) = base_map.get_mut(&key) {
                        if let Value::Object(om) = value {
                            for (k, v) in om {
                                bm.insert(k, v);
                            }
                            continue;
                        }
                    }
                }
                base_map.insert(key, value);
            }
            Value::Object(base_map)
        }
        (_, overlay) => overlay,
    }
}
