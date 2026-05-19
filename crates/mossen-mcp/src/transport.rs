//! MCP 传输层
//!
//! 实现 MCP 协议的多种传输方式：stdio、SSE、Streamable HTTP、WebSocket。
//! 提供统一的传输 trait 接口。

use async_trait::async_trait;
use futures::stream::BoxStream;
use std::collections::HashMap;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

use crate::protocol::JsonRpcMessage;

// ─── 传输层 Trait ────────────────────────────────────────────────────────────

/// MCP 传输层抽象
#[async_trait]
pub trait McpTransport: Send + Sync {
    /// 发送消息
    async fn send(&self, message: &JsonRpcMessage) -> anyhow::Result<()>;

    /// 接收消息流
    fn receive(&self) -> BoxStream<'_, anyhow::Result<JsonRpcMessage>>;

    /// 关闭传输通道
    async fn close(&self) -> anyhow::Result<()>;
}

// ─── Stdio 传输 ──────────────────────────────────────────────────────────────

/// stdio 传输实现
///
/// 通过子进程的 stdin/stdout 进行 JSON-RPC 通信
pub struct StdioTransport {
    /// 发送端（写入子进程 stdin）
    tx: mpsc::UnboundedSender<JsonRpcMessage>,
    /// 接收端（从子进程 stdout 读取）
    rx: tokio::sync::Mutex<mpsc::UnboundedReceiver<JsonRpcMessage>>,
    /// 子进程句柄
    child: tokio::sync::Mutex<Option<Child>>,
}

impl StdioTransport {
    /// 启动子进程并建立 stdio 传输
    pub async fn spawn(
        command: &str,
        args: &[String],
        env: Option<&HashMap<String, String>>,
    ) -> anyhow::Result<Self> {
        let mut cmd = Command::new(command);
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        if let Some(env_vars) = env {
            for (key, value) in env_vars {
                cmd.env(key, value);
            }
        }

        let mut child = cmd.spawn()?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to capture child stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to capture child stdout"))?;

        // 发送通道
        let (write_tx, mut write_rx) = mpsc::unbounded_channel::<JsonRpcMessage>();
        // 接收通道
        let (read_tx, read_rx) = mpsc::unbounded_channel::<JsonRpcMessage>();

        // 写入任务
        tokio::spawn(async move {
            let mut stdin = stdin;
            while let Some(msg) = write_rx.recv().await {
                if let Ok(data) = serde_json::to_string(&msg) {
                    let line = format!("{}\n", data);
                    if stdin.write_all(line.as_bytes()).await.is_err() {
                        break;
                    }
                    if stdin.flush().await.is_err() {
                        break;
                    }
                }
            }
        });

        // 读取任务
        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if let Ok(msg) = serde_json::from_str::<JsonRpcMessage>(trimmed) {
                    if read_tx.send(msg).is_err() {
                        break;
                    }
                }
            }
        });

        Ok(Self {
            tx: write_tx,
            rx: tokio::sync::Mutex::new(read_rx),
            child: tokio::sync::Mutex::new(Some(child)),
        })
    }
}

#[async_trait]
impl McpTransport for StdioTransport {
    async fn send(&self, message: &JsonRpcMessage) -> anyhow::Result<()> {
        self.tx
            .send(message.clone())
            .map_err(|_| anyhow::anyhow!("Stdio transport send channel closed"))?;
        Ok(())
    }

    fn receive(&self) -> BoxStream<'_, anyhow::Result<JsonRpcMessage>> {
        Box::pin(futures::stream::unfold((), move |()| async move {
            let mut rx = self.rx.lock().await;
            rx.recv().await.map(|msg| (Ok(msg), ()))
        }))
    }

    async fn close(&self) -> anyhow::Result<()> {
        let mut child = self.child.lock().await;
        if let Some(mut c) = child.take() {
            let _ = c.kill().await;
        }
        Ok(())
    }
}

// ─── SSE 传输 ────────────────────────────────────────────────────────────────

/// SSE (Server-Sent Events) 传输实现
#[allow(dead_code)]
pub struct SseTransport {
    /// 服务器 URL
    base_url: String,
    /// 请求头
    headers: HashMap<String, String>,
    /// 发送通道
    tx: mpsc::UnboundedSender<JsonRpcMessage>,
    /// 接收通道
    rx: tokio::sync::Mutex<mpsc::UnboundedReceiver<JsonRpcMessage>>,
    /// HTTP 客户端
    client: reqwest::Client,
    /// 消息发送端点 URL
    post_url: tokio::sync::Mutex<Option<String>>,
}

impl SseTransport {
    /// 连接到 SSE 服务器
    pub async fn connect(url: &str, headers: HashMap<String, String>) -> anyhow::Result<Self> {
        let client = reqwest::Client::new();
        let (read_tx, read_rx) = mpsc::unbounded_channel::<JsonRpcMessage>();
        let (write_tx, mut write_rx) = mpsc::unbounded_channel::<JsonRpcMessage>();
        let post_url = tokio::sync::Mutex::new(None::<String>);

        let base_url = url.to_string();

        // SSE 读取任务
        let sse_url = base_url.clone();
        let sse_headers = headers.clone();
        let sse_client = client.clone();
        tokio::spawn(async move {
            let mut req = sse_client.get(&sse_url);
            for (k, v) in &sse_headers {
                req = req.header(k.as_str(), v.as_str());
            }
            req = req.header("Accept", "text/event-stream");

            if let Ok(response) = req.send().await {
                let mut stream = response.bytes_stream();
                use futures::StreamExt;
                let mut buffer = String::new();
                while let Some(Ok(chunk)) = stream.next().await {
                    let text = String::from_utf8_lossy(&chunk);
                    buffer.push_str(&text);

                    // 解析 SSE 事件
                    while let Some(pos) = buffer.find("\n\n") {
                        let event_text = buffer[..pos].to_string();
                        buffer = buffer[pos + 2..].to_string();

                        if let Some(data) = parse_sse_data(&event_text) {
                            if let Ok(msg) = serde_json::from_str::<JsonRpcMessage>(&data) {
                                if read_tx.send(msg).is_err() {
                                    return;
                                }
                            }
                        }
                    }
                }
            }
        });

        // 写入任务（通过 HTTP POST）
        let write_client = client.clone();
        let write_base = base_url.clone();
        let write_headers = headers.clone();
        tokio::spawn(async move {
            while let Some(msg) = write_rx.recv().await {
                if let Ok(body) = serde_json::to_string(&msg) {
                    let mut req = write_client.post(&write_base);
                    for (k, v) in &write_headers {
                        req = req.header(k.as_str(), v.as_str());
                    }
                    let _ = req
                        .header("Content-Type", "application/json")
                        .body(body)
                        .send()
                        .await;
                }
            }
        });

        Ok(Self {
            base_url,
            headers,
            tx: write_tx,
            rx: tokio::sync::Mutex::new(read_rx),
            client,
            post_url,
        })
    }
}

#[async_trait]
impl McpTransport for SseTransport {
    async fn send(&self, message: &JsonRpcMessage) -> anyhow::Result<()> {
        self.tx
            .send(message.clone())
            .map_err(|_| anyhow::anyhow!("SSE transport send channel closed"))?;
        Ok(())
    }

    fn receive(&self) -> BoxStream<'_, anyhow::Result<JsonRpcMessage>> {
        Box::pin(futures::stream::unfold((), move |()| async move {
            let mut rx = self.rx.lock().await;
            rx.recv().await.map(|msg| (Ok(msg), ()))
        }))
    }

    async fn close(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

// ─── Streamable HTTP 传输 ─────────────────────────────────────────────────────

/// Streamable HTTP 传输实现
pub struct HttpTransport {
    /// 服务器 URL
    url: String,
    /// 请求头
    headers: HashMap<String, String>,
    /// HTTP 客户端
    client: reqwest::Client,
    /// 发送通道
    tx: mpsc::UnboundedSender<JsonRpcMessage>,
    /// 接收通道
    rx: tokio::sync::Mutex<mpsc::UnboundedReceiver<JsonRpcMessage>>,
}

impl HttpTransport {
    /// 创建 HTTP 传输
    pub fn new(url: &str, headers: HashMap<String, String>) -> Self {
        let client = reqwest::Client::new();
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            url: url.to_string(),
            headers,
            client,
            tx,
            rx: tokio::sync::Mutex::new(rx),
        }
    }

    /// 发送请求并获取响应
    pub async fn send_request(&self, message: &JsonRpcMessage) -> anyhow::Result<JsonRpcMessage> {
        let body = serde_json::to_string(message)?;
        let mut req = self.client.post(&self.url);
        for (k, v) in &self.headers {
            req = req.header(k.as_str(), v.as_str());
        }
        let response = req
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .body(body)
            .send()
            .await?;

        let text = response.text().await?;
        let msg: JsonRpcMessage = serde_json::from_str(&text)?;
        Ok(msg)
    }
}

#[async_trait]
impl McpTransport for HttpTransport {
    async fn send(&self, message: &JsonRpcMessage) -> anyhow::Result<()> {
        let response = self.send_request(message).await?;
        self.tx
            .send(response)
            .map_err(|_| anyhow::anyhow!("HTTP transport internal channel closed"))?;
        Ok(())
    }

    fn receive(&self) -> BoxStream<'_, anyhow::Result<JsonRpcMessage>> {
        Box::pin(futures::stream::unfold((), move |()| async move {
            let mut rx = self.rx.lock().await;
            rx.recv().await.map(|msg| (Ok(msg), ()))
        }))
    }

    async fn close(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

// ─── WebSocket 传输 ──────────────────────────────────────────────────────────

/// WebSocket 传输实现
pub struct WsTransport {
    /// 发送通道
    tx: mpsc::UnboundedSender<JsonRpcMessage>,
    /// 接收通道
    rx: tokio::sync::Mutex<mpsc::UnboundedReceiver<JsonRpcMessage>>,
}

impl WsTransport {
    /// 连接到 WebSocket 服务器
    pub async fn connect(url: &str, headers: HashMap<String, String>) -> anyhow::Result<Self> {
        use futures::StreamExt;
        use tokio_tungstenite::tungstenite;

        let mut request = tungstenite::http::Request::builder().uri(url);
        for (k, v) in &headers {
            request = request.header(k.as_str(), v.as_str());
        }
        let request = request.body(())?;

        let (ws_stream, _) = tokio_tungstenite::connect_async(request).await?;
        let (write, read) = ws_stream.split();

        let (write_tx, write_rx) = mpsc::unbounded_channel::<JsonRpcMessage>();
        let (read_tx, read_rx) = mpsc::unbounded_channel::<JsonRpcMessage>();

        // 写入任务
        tokio::spawn(async move {
            use futures::SinkExt;
            let mut write = write;
            let mut write_rx = write_rx;
            while let Some(msg) = write_rx.recv().await {
                if let Ok(data) = serde_json::to_string(&msg) {
                    if write.send(tungstenite::Message::Text(data)).await.is_err() {
                        break;
                    }
                }
            }
        });

        // 读取任务
        tokio::spawn(async move {
            use futures::StreamExt;
            let mut read = read;
            while let Some(Ok(msg)) = read.next().await {
                if let tungstenite::Message::Text(text) = msg {
                    if let Ok(parsed) = serde_json::from_str::<JsonRpcMessage>(&text) {
                        if read_tx.send(parsed).is_err() {
                            break;
                        }
                    }
                }
            }
        });

        Ok(Self {
            tx: write_tx,
            rx: tokio::sync::Mutex::new(read_rx),
        })
    }
}

#[async_trait]
impl McpTransport for WsTransport {
    async fn send(&self, message: &JsonRpcMessage) -> anyhow::Result<()> {
        self.tx
            .send(message.clone())
            .map_err(|_| anyhow::anyhow!("WebSocket transport send channel closed"))?;
        Ok(())
    }

    fn receive(&self) -> BoxStream<'_, anyhow::Result<JsonRpcMessage>> {
        Box::pin(futures::stream::unfold((), move |()| async move {
            let mut rx = self.rx.lock().await;
            rx.recv().await.map(|msg| (Ok(msg), ()))
        }))
    }

    async fn close(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

// ─── 辅助函数 ────────────────────────────────────────────────────────────────

/// 解析 SSE 事件数据
fn parse_sse_data(event_text: &str) -> Option<String> {
    let mut data_parts = Vec::new();
    for line in event_text.lines() {
        if let Some(d) = line.strip_prefix("data: ") {
            data_parts.push(d.to_string());
        } else if let Some(d) = line.strip_prefix("data:") {
            data_parts.push(d.to_string());
        }
    }
    if data_parts.is_empty() {
        None
    } else {
        Some(data_parts.join("\n"))
    }
}

/// 展开环境变量（支持 `${VAR}` 和 `${VAR:-default}` 语法）
pub fn expand_env_vars(value: &str) -> (String, Vec<String>) {
    let mut missing_vars = Vec::new();
    let re = regex::Regex::new(r"\$\{([^}]+)\}").unwrap();

    let expanded = re
        .replace_all(value, |caps: &regex::Captures| {
            let var_content = &caps[1];
            let parts: Vec<&str> = var_content.splitn(2, ":-").collect();
            let var_name = parts[0];
            let default_value = parts.get(1).copied();

            match std::env::var(var_name) {
                Ok(env_value) => env_value,
                Err(_) => {
                    if let Some(default) = default_value {
                        default.to_string()
                    } else {
                        missing_vars.push(var_name.to_string());
                        caps[0].to_string()
                    }
                }
            }
        })
        .to_string();

    (expanded, missing_vars)
}
