//! # remote_io — 远程 IO 管理
//!
//! 基于 `StructuredIo` 扩展，提供远程传输层集成。
//! 对应 TS `cli/remoteIO.ts` 中的 `RemoteIO` 类。
//!
//! ## CCR 集成
//!
//! TS 的 `RemoteIO.flushInternalEvents()` 委托给 `this.ccrClient`。Rust 端把
//! CCRClient 的真实实现放在 `mossen-cli`（避免下层依赖 reqwest 上的 worker
//! 协议），但仍允许任何调用者通过 [`RemoteIo::set_internal_event_hooks`] 注入
//! 自己的 flush/pending 回调，从而在保持架构分层的前提下让 `RemoteIo` 行为
//! 与 TS 一致——未注入时 flush 是 no-op、pending 为 0，正好对应 TS 中
//! `ccrClient` 为 `null` 时的语义 (`this.ccrClient?.… ?? 0`)。

use crate::http::SseTransport;
use crate::structured_io::StructuredIo;
use crate::transport::{select_transport_for_url, StdoutMessage, Transport, TransportKind};
use crate::websocket::WebSocketTransport;
use futures::future::BoxFuture;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, RwLock};
use tracing;

/// 内部事件 flush 回调签名。
pub type FlushInternalEventsHook = Arc<dyn Fn() -> BoxFuture<'static, ()> + Send + Sync + 'static>;

/// 内部事件待发送数量回调签名。
pub type InternalEventsPendingHook = Arc<dyn Fn() -> usize + Send + Sync + 'static>;

/// 远程 IO 管理器。
///
/// 在 `StructuredIo` 基础上集成远程传输层（WebSocket/SSE），
/// 提供双向流式通信。
pub struct RemoteIo {
    /// 结构化 IO 处理器。
    pub structured_io: StructuredIo,
    /// 远程传输 URL。
    url: url::Url,
    /// 传输层实例。
    transport: Arc<Mutex<Box<dyn Transport>>>,
    /// 输入流发送端（transport → structured_io 的桥接）。
    input_tx: mpsc::UnboundedSender<String>,
    /// 是否已关闭。
    closed: Arc<RwLock<bool>>,
    /// CCR flush 钩子（由 mossen-cli 注入）。
    flush_hook: Arc<RwLock<Option<FlushInternalEventsHook>>>,
    /// CCR pending 钩子（由 mossen-cli 注入）。
    pending_hook: Arc<RwLock<Option<InternalEventsPendingHook>>>,
}

impl RemoteIo {
    /// 创建新的远程 IO 管理器。
    ///
    /// # 参数
    /// - `stream_url`: 远程流 URL
    /// - `headers`: 请求头（包含 Authorization 等）
    /// - `session_id`: 会话 ID
    /// - `replay_user_messages`: 是否重播用户消息
    pub fn new(
        stream_url: &str,
        headers: HashMap<String, String>,
        session_id: Option<String>,
        replay_user_messages: bool,
    ) -> anyhow::Result<Self> {
        let url = url::Url::parse(stream_url)?;
        let structured_io = StructuredIo::new(replay_user_messages);
        let (input_tx, _input_rx) = mpsc::unbounded_channel();

        let transport_kind = select_transport_for_url(&url);
        let transport: Box<dyn Transport> = match transport_kind {
            TransportKind::Sse => Box::new(SseTransport::new(url.clone(), headers, session_id)),
            TransportKind::WebSocket | TransportKind::Hybrid => {
                Box::new(WebSocketTransport::new(
                    url.clone(),
                    headers,
                    session_id,
                    true, // auto_reconnect
                ))
            }
            TransportKind::Stdio => {
                // 远程 IO 不应使用 Stdio 传输
                anyhow::bail!("RemoteIo does not support Stdio transport");
            }
        };

        Ok(Self {
            structured_io,
            url,
            transport: Arc::new(Mutex::new(transport)),
            input_tx,
            closed: Arc::new(RwLock::new(false)),
            flush_hook: Arc::new(RwLock::new(None)),
            pending_hook: Arc::new(RwLock::new(None)),
        })
    }

    /// 注入 CCR v2 内部事件钩子。
    ///
    /// 对应 TS `RemoteIO` 在构造函数尾部 `this.ccrClient = new CCRClient(...)`
    /// 之后的钩子绑定——之后 `flushInternalEvents` / `internalEventsPending`
    /// 委托给 `ccrClient`。Rust 端把 CCRClient 真正实现放在 `mossen-cli`
    /// （能拿到 reqwest + analytics），通过这个 setter 在上层把钩子注入下来。
    pub async fn set_internal_event_hooks(
        &self,
        flush: FlushInternalEventsHook,
        pending: InternalEventsPendingHook,
    ) {
        *self.flush_hook.write().await = Some(flush);
        *self.pending_hook.write().await = Some(pending);
    }

    /// 清除已注入的内部事件钩子。
    pub async fn clear_internal_event_hooks(&self) {
        *self.flush_hook.write().await = None;
        *self.pending_hook.write().await = None;
    }

    /// 启动远程连接。
    ///
    /// 连接传输层，设置数据回调，开始接收消息。
    pub async fn connect(&self) -> anyhow::Result<()> {
        let input_tx = self.input_tx.clone();
        let closed = self.closed.clone();
        let input_closed_ref = closed.clone();

        // 设置数据回调：transport → input channel
        let tx = input_tx.clone();
        let mut transport = self.transport.lock().await;
        transport.set_on_data(Box::new(move |data: String| {
            let _ = tx.send(data);
        }));

        // 设置关闭回调
        let closed_ref = input_closed_ref;
        transport.set_on_close(Box::new(move |_code: Option<u16>| {
            let closed = closed_ref.clone();
            tokio::spawn(async move {
                *closed.write().await = true;
            });
        }));

        transport.connect().await?;

        tracing::info!("RemoteIo: connected to {}", self.url);
        Ok(())
    }

    /// 通过传输层写入消息。
    pub async fn write(&self, message: StdoutMessage) -> anyhow::Result<()> {
        let transport = self.transport.lock().await;
        transport.write(message).await
    }

    /// 关闭远程连接。
    pub async fn close(&self) {
        *self.closed.write().await = true;
        let mut transport = self.transport.lock().await;
        transport.close();
        self.structured_io.close_input().await;
    }

    /// 获取传输层 URL。
    pub fn url(&self) -> &url::Url {
        &self.url
    }

    /// 是否已关闭。
    pub async fn is_closed(&self) -> bool {
        *self.closed.read().await
    }

    /// 刷新内部事件（CCR v2 模式）。
    ///
    /// 如果调用方通过 [`Self::set_internal_event_hooks`] 注入了真正的 CCR
    /// 客户端，则委托给它。未注入时对应 TS `this.ccrClient?.…() ?? Promise.resolve()`
    /// 的右半（resolve 空值）。
    pub async fn flush_internal_events(&self) {
        let hook = self.flush_hook.read().await.clone();
        if let Some(hook) = hook {
            hook().await;
        }
    }

    /// 内部事件队列深度。
    ///
    /// 钩子未注入时返回 0（对应 TS `this.ccrClient?.internalEventsPending ?? 0`）。
    pub fn internal_events_pending(&self) -> usize {
        // 同步路径：使用 try_read 避免在异步调用栈外阻塞。注入端在
        // set_internal_event_hooks 之后才会查询，竞态下读 None 退化为 0，
        // 与 TS 早期调用未初始化 ccrClient 的语义一致。
        let guard = self.pending_hook.try_read();
        match guard {
            Ok(g) => g.as_ref().map(|h| h()).unwrap_or(0),
            Err(_) => 0,
        }
    }
}
