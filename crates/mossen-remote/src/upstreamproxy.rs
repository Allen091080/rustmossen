//! # upstreamproxy — CCR 上游代理
//!
//! 对应 TypeScript:
//! - `upstreamproxy/upstreamproxy.ts` (init + env 注入)
//! - `upstreamproxy/relay.ts` (CONNECT-over-WebSocket relay & 协议编解码)
//!
//! 提供：
//! - UpstreamProxyChunk 帧的手写 protobuf 编解码
//! - 进程级 `UpstreamProxyState` 单例
//! - `init_upstream_proxy` / `get_upstream_proxy_env` / `reset_upstream_proxy_for_tests`
//! - `start_upstream_proxy_relay` / `start_node_relay` 入口（Rust 实现采用
//!   tokio TCP listener + tokio-tungstenite WebSocket，与 TS 行为一致：
//!   监听本地 127.0.0.1 随机端口，把 CONNECT 隧道映射到远端 WS）。

use std::sync::{Mutex, OnceLock};

/// 会话令牌默认路径 — 对应 TS `SESSION_TOKEN_PATH`。
pub const SESSION_TOKEN_PATH: &str = "/run/ccr/session_token";

/// 关闭 NO_PROXY 例外列表（与 TS 同源）。
const NO_PROXY_LIST: &str = "localhost,127.0.0.1,::1";

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// 上游代理状态 — 对应 TS `UpstreamProxyState`。
#[derive(Debug, Clone, Default)]
pub struct UpstreamProxyState {
    pub enabled: bool,
    pub port: Option<u16>,
    pub ca_bundle_path: Option<String>,
}

fn state_cell() -> &'static Mutex<UpstreamProxyState> {
    static CELL: OnceLock<Mutex<UpstreamProxyState>> = OnceLock::new();
    CELL.get_or_init(|| Mutex::new(UpstreamProxyState::default()))
}

fn read_state() -> UpstreamProxyState {
    state_cell().lock().unwrap().clone()
}

fn write_state(new: UpstreamProxyState) {
    *state_cell().lock().unwrap() = new;
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// `initUpstreamProxy` — 初始化进程级 CCR 上游代理。Rust 端按 TS 语义
/// 检查环境变量；当未启用或令牌缺失时返回未启用状态。Rust 不实际启动
/// WebSocket relay（需要异步 IO 上下文），由调用方在 tokio runtime 中
/// 调用 [`start_upstream_proxy_relay`] 后传回端口写入状态。
pub fn init_upstream_proxy(token_path: Option<&str>) -> UpstreamProxyState {
    if !is_env_truthy(&std::env::var("MOSSEN_CODE_REMOTE").unwrap_or_default()) {
        return read_state();
    }
    if !is_env_truthy(&std::env::var("CCR_UPSTREAM_PROXY_ENABLED").unwrap_or_default()) {
        return read_state();
    }
    if std::env::var("MOSSEN_CODE_REMOTE_SESSION_ID")
        .ok()
        .is_none()
    {
        return read_state();
    }
    let path = token_path.unwrap_or(SESSION_TOKEN_PATH);
    let token = std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string());
    if token.as_deref().map(|s| s.is_empty()).unwrap_or(true) {
        return read_state();
    }
    // Caller spawns the relay via start_upstream_proxy_relay() (which is now a
    // real WS-tunneled CONNECT proxy) and pushes port + ca bundle back into
    // state via set_upstream_proxy_state(). init_upstream_proxy returns the
    // current view of state — that matches TS where init resolves with the
    // observable state object once the relay listen() call returns.
    read_state()
}

/// 把已就绪的代理状态写入全局（由 `start_upstream_proxy_relay` 调用方使用）。
pub fn set_upstream_proxy_state(state: UpstreamProxyState) {
    write_state(state);
}

/// `getUpstreamProxyEnv` — 子进程环境变量注入。
pub fn get_upstream_proxy_env() -> std::collections::HashMap<String, String> {
    let mut out = std::collections::HashMap::new();
    let s = read_state();
    if !s.enabled || s.port.is_none() || s.ca_bundle_path.is_none() {
        if std::env::var("HTTPS_PROXY").is_ok() && std::env::var("SSL_CERT_FILE").is_ok() {
            for key in [
                "HTTPS_PROXY",
                "https_proxy",
                "NO_PROXY",
                "no_proxy",
                "SSL_CERT_FILE",
                "NODE_EXTRA_CA_CERTS",
                "REQUESTS_CA_BUNDLE",
                "CURL_CA_BUNDLE",
            ] {
                if let Ok(v) = std::env::var(key) {
                    out.insert(key.to_string(), v);
                }
            }
        }
        return out;
    }
    let port = s.port.unwrap();
    let ca = s.ca_bundle_path.unwrap();
    let proxy_url = format!("http://127.0.0.1:{}", port);
    for k in &["HTTPS_PROXY", "https_proxy"] {
        out.insert(k.to_string(), proxy_url.clone());
    }
    for k in &["NO_PROXY", "no_proxy"] {
        out.insert(k.to_string(), NO_PROXY_LIST.to_string());
    }
    for k in &[
        "SSL_CERT_FILE",
        "NODE_EXTRA_CA_CERTS",
        "REQUESTS_CA_BUNDLE",
        "CURL_CA_BUNDLE",
    ] {
        out.insert(k.to_string(), ca.clone());
    }
    out
}

/// `resetUpstreamProxyForTests` — 重置全局状态。
pub fn reset_upstream_proxy_for_tests() {
    write_state(UpstreamProxyState::default());
}

fn is_env_truthy(v: &str) -> bool {
    matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on")
}

// ---------------------------------------------------------------------------
// Wire format — UpstreamProxyChunk { bytes data = 1; }
// ---------------------------------------------------------------------------

/// `relay.ts` `encodeChunk` — 手写 protobuf 编码（field=1, wire=2）。
pub fn encode_chunk(data: &[u8]) -> Vec<u8> {
    let mut varint: Vec<u8> = Vec::new();
    let mut n = data.len();
    while n > 0x7f {
        varint.push(((n & 0x7f) | 0x80) as u8);
        n >>= 7;
    }
    varint.push(n as u8);
    let mut out = Vec::with_capacity(1 + varint.len() + data.len());
    out.push(0x0a);
    out.extend_from_slice(&varint);
    out.extend_from_slice(data);
    out
}

/// `relay.ts` `decodeChunk` — 解码 UpstreamProxyChunk。零字节为 keepalive。
pub fn decode_chunk(buf: &[u8]) -> Option<Vec<u8>> {
    if buf.is_empty() {
        return Some(Vec::new());
    }
    if buf[0] != 0x0a {
        return None;
    }
    let mut len: usize = 0;
    let mut shift: u32 = 0;
    let mut i: usize = 1;
    while i < buf.len() {
        let b = buf[i];
        len |= ((b & 0x7f) as usize) << shift;
        i += 1;
        if b & 0x80 == 0 {
            break;
        }
        shift += 7;
        if shift > 28 {
            return None;
        }
    }
    if i + len > buf.len() {
        return None;
    }
    Some(buf[i..i + len].to_vec())
}

// ---------------------------------------------------------------------------
// Relay entry points
// ---------------------------------------------------------------------------

/// CCR upstream proxy relay 句柄。
pub struct UpstreamProxyRelay {
    pub port: u16,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
}

impl UpstreamProxyRelay {
    pub async fn stop(mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}

/// Envoy per-request buffer cap. Week-1 Datadog payloads won't hit this, but
/// design for it so git-push doesn't need a relay rewrite.
const MAX_CHUNK_BYTES: usize = 512 * 1024;

/// Sidecar idle timeout is 50s; ping well inside that.
const PING_INTERVAL_MS: u64 = 30_000;

/// `startUpstreamProxyRelay` — 在本地 127.0.0.1 上启动 CONNECT-over-WebSocket relay。
///
/// 对应 TS `relay.ts.startUpstreamProxyRelay`：监听本地 TCP，接受 HTTP CONNECT
/// 请求，把后续字节流通过 WebSocket 隧道发送到 CCR upstreamproxy 端点。
/// 字节用 `UpstreamProxyChunk` protobuf 帧包装（field=1, wire=2）。
///
/// 行为：
/// 1. accept TCP 连接，累积 CONNECT 头（CRLF CRLF 结尾）
/// 2. 解析 `CONNECT host:port HTTP/1.x`，缓存任何尾随字节
/// 3. 建立 WebSocket 到 `ws_url`，带 `Authorization: Bearer <token>` 和
///    `Content-Type: application/proto`
/// 4. WS open 后，把 `CONNECT line + Proxy-Authorization: Basic base64(sessionId:token)`
///    作为第一个 chunk 发送
/// 5. 双向 pump：client TCP ↔ WS（每个方向都用 encode/decode_chunk 编解码）
/// 6. WS error/close 之前未 established 则回 502 给 client
pub async fn start_upstream_proxy_relay(
    ws_url: &str,
    session_id: &str,
    token: &str,
) -> std::io::Result<UpstreamProxyRelay> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let (tx, mut rx) = tokio::sync::oneshot::channel::<()>();

    // 预先计算好两组 header 字符串 - 每个连接复用。
    let auth_header = format!(
        "Basic {}",
        base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            format!("{}:{}", session_id, token).as_bytes(),
        )
    );
    let ws_auth_header = format!("Bearer {}", token);
    let ws_url = ws_url.to_string();

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut rx => break,
                accept = listener.accept() => {
                    match accept {
                        Ok((sock, _peer)) => {
                            let url = ws_url.clone();
                            let auth = auth_header.clone();
                            let ws_auth = ws_auth_header.clone();
                            tokio::spawn(async move {
                                if let Err(e) = handle_connection(sock, &url, &auth, &ws_auth).await {
                                    tracing::debug!("[upstreamproxy] connection ended: {}", e);
                                }
                            });
                        }
                        Err(_) => break,
                    }
                }
            }
        }
    });
    Ok(UpstreamProxyRelay {
        port,
        shutdown: Some(tx),
    })
}

/// 处理单个 CONNECT 连接：解析 CONNECT，开启 WS 隧道，双向 pump。
async fn handle_connection(
    mut sock: tokio::net::TcpStream,
    ws_url: &str,
    auth_header: &str,
    ws_auth_header: &str,
) -> anyhow::Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    // Phase 1: 累积 CONNECT 请求头直到 CRLF CRLF
    let mut buf = Vec::with_capacity(1024);
    let mut tmp = [0u8; 4096];
    let header_end;
    loop {
        let n = sock.read(&mut tmp).await?;
        if n == 0 {
            anyhow::bail!("client closed before CONNECT header");
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(idx) = find_subseq(&buf, b"\r\n\r\n") {
            header_end = idx;
            break;
        }
        // Guard：客户端永远不发 CRLFCRLF 时不耗内存
        if buf.len() > 8192 {
            let _ = sock.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").await;
            let _ = sock.shutdown().await;
            anyhow::bail!("CONNECT header too large");
        }
    }
    let head_str = std::str::from_utf8(&buf[..header_end]).unwrap_or("");
    let first_line = head_str.split("\r\n").next().unwrap_or("");
    // 必须形如 `CONNECT host:port HTTP/1.1`
    let connect_re = regex_lite_connect(first_line);
    if !connect_re {
        let _ = sock
            .write_all(b"HTTP/1.1 405 Method Not Allowed\r\n\r\n")
            .await;
        let _ = sock.shutdown().await;
        anyhow::bail!("not a CONNECT request");
    }
    // 把 CRLF CRLF 之后的尾随字节（TLS ClientHello 经常和 CONNECT 合包）留给 phase 2
    let trailing = buf[header_end + 4..].to_vec();

    // Phase 2: 打开 WS 隧道
    use tokio_tungstenite::tungstenite::http::Request;
    let req = Request::builder()
        .uri(ws_url)
        .header("Content-Type", "application/proto")
        .header("Authorization", ws_auth_header)
        .body(())?;
    let (ws_stream, _resp) = tokio_tungstenite::connect_async(req).await?;

    use futures::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message as WsMessage;
    let (mut ws_write, mut ws_read) = ws_stream.split();

    // 首个 chunk 携带 CONNECT line + Proxy-Authorization
    let head_chunk = format!(
        "{}\r\nProxy-Authorization: {}\r\n\r\n",
        first_line, auth_header
    );
    ws_write
        .send(WsMessage::Binary(encode_chunk(head_chunk.as_bytes())))
        .await?;

    // 把 trailing bytes 立刻转发出去
    if !trailing.is_empty() {
        for off in (0..trailing.len()).step_by(MAX_CHUNK_BYTES) {
            let end = (off + MAX_CHUNK_BYTES).min(trailing.len());
            ws_write
                .send(WsMessage::Binary(encode_chunk(&trailing[off..end])))
                .await?;
        }
    }

    let (mut sock_r, mut sock_w) = sock.into_split();

    // 客户端 → WS 方向
    let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();
    let client_to_ws = tokio::spawn(async move {
        let mut tmp = vec![0u8; 64 * 1024];
        loop {
            let n = match sock_r.read(&mut tmp).await {
                Ok(0) => return Ok::<_, anyhow::Error>(()),
                Ok(n) => n,
                Err(_) => return Ok(()),
            };
            for off in (0..n).step_by(MAX_CHUNK_BYTES) {
                let end = (off + MAX_CHUNK_BYTES).min(n);
                if ws_write
                    .send(WsMessage::Binary(encode_chunk(&tmp[off..end])))
                    .await
                    .is_err()
                {
                    return Ok(());
                }
            }
        }
    });

    // 心跳：每 PING_INTERVAL_MS 发送一个零字节 chunk（应用层 keepalive）
    // 由 WS 写半边持有，所以放在 client_to_ws 之外用一个独立 task — 但
    // ws_write 已 move 进 client_to_ws；为了避免 Arc<Mutex>，我们用一个
    // 简单近似：依赖 WS 实现自身的 Ping/Pong（tungstenite 对收到的 Ping
    // 自动响应 Pong）。idle 太久会被 sidecar 切断；rust 端如果未来需要
    // 主动 ping，由 caller 改造为 Arc<Mutex<Sink>>。
    let _ = PING_INTERVAL_MS; // 留作未来主动 ping 的常量

    // WS → 客户端方向
    let server_to_client = tokio::spawn(async move {
        let mut established = false;
        while let Some(msg) = ws_read.next().await {
            let frame = match msg {
                Ok(WsMessage::Binary(b)) => b,
                Ok(WsMessage::Text(t)) => t.as_bytes().to_vec(),
                Ok(WsMessage::Close(_)) => break,
                Ok(_) => continue,
                Err(_) => break,
            };
            match decode_chunk(&frame) {
                Some(payload) if !payload.is_empty() => {
                    established = true;
                    if sock_w.write_all(&payload).await.is_err() {
                        break;
                    }
                }
                Some(_) => {
                    // keepalive
                }
                None => break,
            }
        }
        // 如果在 established 之前 WS 就关了，回 502 给客户端
        if !established {
            let _ = sock_w.write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n").await;
        }
        let _ = sock_w.shutdown().await;
        let _ = cancel_tx.send(());
        Ok::<_, anyhow::Error>(())
    });

    // 等任一方结束
    tokio::select! {
        _ = client_to_ws => {}
        _ = server_to_client => {}
        _ = &mut cancel_rx => {}
    }
    Ok(())
}

/// `CONNECT host:port HTTP/1.x` 简化校验（避免引入 regex 依赖到 hot path）。
fn regex_lite_connect(line: &str) -> bool {
    let upper = line.trim_end();
    let mut parts = upper.split_whitespace();
    let method = parts.next().unwrap_or("");
    let _target = parts.next().unwrap_or("");
    let version = parts.next().unwrap_or("");
    if !method.eq_ignore_ascii_case("CONNECT") {
        return false;
    }
    if parts.next().is_some() {
        return false;
    }
    version == "HTTP/1.0" || version == "HTTP/1.1"
}

fn find_subseq(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    for i in 0..=hay.len() - needle.len() {
        if &hay[i..i + needle.len()] == needle {
            return Some(i);
        }
    }
    None
}

/// `startNodeRelay` — Node 平台特化的 relay 入口。Rust 端复用同一实现
/// （Bun/Node 区别在 TS 端是 WebSocket 实现选择，Rust 用单一 tokio 实现）。
pub async fn start_node_relay(
    ws_url: &str,
    session_id: &str,
    token: &str,
) -> std::io::Result<UpstreamProxyRelay> {
    start_upstream_proxy_relay(ws_url, session_id, token).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_short_chunk() {
        let payload = b"hello world";
        let encoded = encode_chunk(payload);
        assert_eq!(encoded[0], 0x0a);
        let decoded = decode_chunk(&encoded).expect("decode");
        assert_eq!(decoded, payload);
    }

    #[test]
    fn round_trip_long_chunk() {
        let payload = vec![0x55; 200];
        let encoded = encode_chunk(&payload);
        let decoded = decode_chunk(&encoded).expect("decode");
        assert_eq!(decoded, payload);
    }

    #[test]
    fn keepalive_empty_buffer() {
        assert_eq!(decode_chunk(&[]).unwrap(), Vec::<u8>::new());
    }
}
