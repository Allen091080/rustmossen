//! # stdio — 标准 IO 传输层
//!
//! 通过标准输入/输出进行 NDJSON 消息交换。
//! 用于本地 SDK 模式和管道式通信。

use crate::ndjson::ndjson_safe_line;
use crate::transport::{
    OnCloseCallback, OnConnectCallback, OnDataCallback, StdoutMessage, Transport, TransportState,
};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{Mutex, RwLock};
use tracing;

/// 标准 IO 传输层。
///
/// 从 stdin 读取 NDJSON 行，向 stdout 写入 NDJSON 行。
pub struct StdioTransport {
    /// 当前状态。
    state: Arc<RwLock<TransportState>>,
    /// 数据回调。
    on_data: Arc<RwLock<Option<OnDataCallback>>>,
    /// 关闭回调。
    on_close: Arc<RwLock<Option<OnCloseCallback>>>,
    /// 连接回调。
    on_connect: Arc<RwLock<Option<OnConnectCallback>>>,
    /// 关闭信号。
    shutdown_tx: Arc<Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
    /// stdout 写入锁。
    stdout_lock: Arc<Mutex<tokio::io::Stdout>>,
}

impl StdioTransport {
    /// 创建新的 Stdio 传输层。
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(TransportState::Idle)),
            on_data: Arc::new(RwLock::new(None)),
            on_close: Arc::new(RwLock::new(None)),
            on_connect: Arc::new(RwLock::new(None)),
            shutdown_tx: Arc::new(Mutex::new(None)),
            stdout_lock: Arc::new(Mutex::new(tokio::io::stdout())),
        }
    }
}

impl Default for StdioTransport {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Transport for StdioTransport {
    async fn connect(&mut self) -> anyhow::Result<()> {
        *self.state.write().await = TransportState::Connected;

        if let Some(ref cb) = *self.on_connect.read().await {
            cb();
        }

        // 启动 stdin 读取循环
        let on_data = self.on_data.clone();
        let on_close = self.on_close.clone();
        let state = self.state.clone();
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        *self.shutdown_tx.lock().await = Some(shutdown_tx);

        tokio::spawn(async move {
            let stdin = tokio::io::stdin();
            let reader = BufReader::new(stdin);
            let mut lines = reader.lines();
            let mut shutdown_rx = shutdown_rx;

            loop {
                tokio::select! {
                    line = lines.next_line() => {
                        match line {
                            Ok(Some(line)) => {
                                if !line.is_empty() {
                                    if let Some(ref cb) = *on_data.read().await {
                                        cb(line);
                                    }
                                }
                            }
                            Ok(None) => {
                                tracing::debug!("StdioTransport: stdin closed");
                                break;
                            }
                            Err(e) => {
                                tracing::error!("StdioTransport: read error: {}", e);
                                break;
                            }
                        }
                    }
                    _ = &mut shutdown_rx => {
                        tracing::debug!("StdioTransport: shutdown signal");
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
        let line = ndjson_safe_line(&message)?;
        let mut stdout = self.stdout_lock.lock().await;
        stdout.write_all(line.as_bytes()).await?;
        stdout.flush().await?;
        Ok(())
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
