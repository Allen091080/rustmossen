// upstream_proxy.rs — Translation of upstreamproxy/ directory:
// upstreamproxy/relay.ts, upstreamproxy/upstreamproxy.ts

use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

// ============================================================================
// relay.ts — CONNECT-over-WebSocket relay
// ============================================================================

const MAX_CHUNK_BYTES: usize = 512 * 1024;
const PING_INTERVAL_MS: u64 = 30_000;

/// Encode an UpstreamProxyChunk protobuf message.
/// message UpstreamProxyChunk { bytes data = 1; }
/// tag = (1 << 3) | 2 = 0x0a, then varint length, then data.
pub fn encode_chunk(data: &[u8]) -> Vec<u8> {
    let len = data.len();
    let mut varint = Vec::new();
    let mut n = len;
    while n > 0x7f {
        varint.push(((n & 0x7f) | 0x80) as u8);
        n >>= 7;
    }
    varint.push(n as u8);

    let mut out = Vec::with_capacity(1 + varint.len() + len);
    out.push(0x0a);
    out.extend_from_slice(&varint);
    out.extend_from_slice(data);
    out
}

/// Decode an UpstreamProxyChunk. Returns the data field, or None if malformed.
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
        if (b & 0x80) == 0 {
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

pub struct UpstreamProxyRelay {
    pub port: u16,
    shutdown: tokio::sync::oneshot::Sender<()>,
}

impl UpstreamProxyRelay {
    pub fn stop(self) {
        let _ = self.shutdown.send(());
    }
}

struct ConnState {
    ws_open: bool,
    established: bool,
    closed: bool,
    pending: Vec<Vec<u8>>,
    connect_buf: Vec<u8>,
}

impl ConnState {
    fn new() -> Self {
        Self {
            ws_open: false,
            established: false,
            closed: false,
            pending: Vec::new(),
            connect_buf: Vec::new(),
        }
    }
}

/// Start the upstream proxy relay. Returns the ephemeral port and a handle to stop it.
pub async fn start_upstream_proxy_relay(
    ws_url: &str,
    session_id: &str,
    token: &str,
) -> Result<UpstreamProxyRelay, Box<dyn std::error::Error + Send + Sync>> {
    let auth_header = format!(
        "Basic {}",
        base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            format!("{}:{}", session_id, token),
        )
    );
    let ws_auth_header = format!("Bearer {}", token);

    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();

    let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    let ws_url = ws_url.to_string();
    let auth_header = auth_header;
    let ws_auth_header = ws_auth_header;

    tokio::spawn(async move {
        loop {
            tokio::select! {
                accept = listener.accept() => {
                    match accept {
                        Ok((stream, _)) => {
                            let ws_url = ws_url.clone();
                            let auth = auth_header.clone();
                            let ws_auth = ws_auth_header.clone();
                            tokio::spawn(async move {
                                handle_connection(stream, &ws_url, &auth, &ws_auth).await;
                            });
                        }
                        Err(_) => break,
                    }
                }
                _ = &mut shutdown_rx => {
                    break;
                }
            }
        }
    });

    tracing::debug!("[upstreamproxy] relay listening on 127.0.0.1:{}", port);

    Ok(UpstreamProxyRelay {
        port,
        shutdown: shutdown_tx,
    })
}

async fn handle_connection(
    mut stream: tokio::net::TcpStream,
    ws_url: &str,
    auth_header: &str,
    ws_auth_header: &str,
) {
    let mut buf = vec![0u8; 8192];
    let mut connect_buf = Vec::new();

    // Phase 1: Read CONNECT request
    loop {
        let n = match stream.read(&mut buf).await {
            Ok(0) => return,
            Ok(n) => n,
            Err(_) => return,
        };
        connect_buf.extend_from_slice(&buf[..n]);

        if let Some(header_end) = find_crlfcrlf(&connect_buf) {
            let req_head = String::from_utf8_lossy(&connect_buf[..header_end]).to_string();
            let first_line = req_head.lines().next().unwrap_or("");

            if !first_line.to_uppercase().starts_with("CONNECT ") {
                let _ = stream.write_all(b"HTTP/1.1 405 Method Not Allowed\r\n\r\n").await;
                return;
            }

            let trailing = connect_buf[header_end + 4..].to_vec();

            // Phase 2: Open WebSocket tunnel
            open_tunnel(
                stream,
                first_line,
                &trailing,
                ws_url,
                auth_header,
                ws_auth_header,
            ).await;
            return;
        }

        if connect_buf.len() > 8192 {
            let _ = stream.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").await;
            return;
        }
    }
}

fn find_crlfcrlf(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

async fn open_tunnel(
    mut stream: tokio::net::TcpStream,
    connect_line: &str,
    trailing: &[u8],
    ws_url: &str,
    auth_header: &str,
    ws_auth_header: &str,
) {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    use futures_util::{SinkExt, StreamExt};

    let mut request = match ws_url.into_client_request() {
        Ok(r) => r,
        Err(_) => {
            let _ = stream.write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n").await;
            return;
        }
    };
    request.headers_mut().insert(
        "Content-Type",
        "application/proto".parse().unwrap(),
    );
    request.headers_mut().insert(
        "Authorization",
        ws_auth_header.parse().unwrap(),
    );

    let ws_result = tokio_tungstenite::connect_async(request).await;
    let (ws_stream, _) = match ws_result {
        Ok(s) => s,
        Err(_) => {
            let _ = stream.write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n").await;
            return;
        }
    };

    let (mut ws_write, mut ws_read) = ws_stream.split();

    // Send CONNECT header + auth as first chunk
    let head = format!("{}\r\nProxy-Authorization: {}\r\n\r\n", connect_line, auth_header);
    let chunk = encode_chunk(head.as_bytes());
    if ws_write.send(tokio_tungstenite::tungstenite::Message::Binary(chunk)).await.is_err() {
        let _ = stream.write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n").await;
        return;
    }

    // Flush trailing bytes
    if !trailing.is_empty() {
        for off in (0..trailing.len()).step_by(MAX_CHUNK_BYTES) {
            let end = (off + MAX_CHUNK_BYTES).min(trailing.len());
            let chunk = encode_chunk(&trailing[off..end]);
            if ws_write.send(tokio_tungstenite::tungstenite::Message::Binary(chunk)).await.is_err() {
                return;
            }
        }
    }

    // Keepalive
    let ws_write = Arc::new(Mutex::new(ws_write));
    let ws_write2 = ws_write.clone();

    let keepalive_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(PING_INTERVAL_MS));
        loop {
            interval.tick().await;
            let empty_chunk = encode_chunk(&[]);
            let mut w = ws_write2.lock().await;
            if w.send(tokio_tungstenite::tungstenite::Message::Binary(empty_chunk)).await.is_err() {
                break;
            }
        }
    });

    // Use into_split for owned halves that can be moved into tasks
    let (mut tcp_read, mut tcp_write) = stream.into_split();

    // WS -> TCP
    let ws_write3 = ws_write.clone();
    let ws_to_tcp = tokio::spawn(async move {
        let _ = &ws_write3; // keep alive
        while let Some(msg) = ws_read.next().await {
            match msg {
                Ok(tokio_tungstenite::tungstenite::Message::Binary(data)) => {
                    if let Some(payload) = decode_chunk(&data) {
                        if !payload.is_empty() {
                            if tcp_write.write_all(&payload).await.is_err() {
                                break;
                            }
                        }
                    }
                }
                Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => break,
                Err(_) => break,
                _ => {}
            }
        }
    });

    // TCP -> WS
    let tcp_to_ws = tokio::spawn(async move {
        let mut buf = vec![0u8; MAX_CHUNK_BYTES];
        loop {
            let n = match tcp_read.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => n,
                Err(_) => break,
            };
            let chunk = encode_chunk(&buf[..n]);
            let mut w = ws_write.lock().await;
            if w.send(tokio_tungstenite::tungstenite::Message::Binary(chunk)).await.is_err() {
                break;
            }
        }
    });

    let _ = tokio::select! {
        r = ws_to_tcp => r,
        r = tcp_to_ws => r,
    };
    keepalive_handle.abort();
}

// ============================================================================
// upstreamproxy.ts — Container-side Wiring
// ============================================================================

pub const SESSION_TOKEN_PATH: &str = "/run/ccr/session_token";
const SYSTEM_CA_BUNDLE: &str = "/etc/ssl/certs/ca-certificates.crt";

const NO_PROXY_LIST: &str = "localhost,127.0.0.1,::1,169.254.0.0/16,10.0.0.0/8,172.16.0.0/12,192.168.0.0/16,mossen.invalid,.mossen.invalid,*.mossen.invalid,github.com,api.github.com,*.github.com,*.githubusercontent.com,registry.npmjs.org,pypi.org,files.pythonhosted.org,index.crates.io,proxy.golang.org";

#[derive(Debug, Clone)]
pub struct UpstreamProxyState {
    pub enabled: bool,
    pub port: Option<u16>,
    pub ca_bundle_path: Option<String>,
}

impl Default for UpstreamProxyState {
    fn default() -> Self {
        Self { enabled: false, port: None, ca_bundle_path: None }
    }
}

pub async fn init_upstream_proxy(
    token_path: Option<&str>,
    system_ca_path: Option<&str>,
    ca_bundle_path: Option<&str>,
    ccr_base_url: Option<&str>,
) -> UpstreamProxyState {
    let default_state = UpstreamProxyState::default();

    let is_remote = std::env::var("MOSSEN_CODE_REMOTE")
        .map(|v| matches!(v.trim().to_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false);
    if !is_remote {
        return default_state;
    }

    let proxy_enabled = std::env::var("CCR_UPSTREAM_PROXY_ENABLED")
        .map(|v| matches!(v.trim().to_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false);
    if !proxy_enabled {
        return default_state;
    }

    let session_id = match std::env::var("MOSSEN_CODE_REMOTE_SESSION_ID") {
        Ok(id) if !id.is_empty() => id,
        _ => {
            tracing::warn!("[upstreamproxy] MOSSEN_CODE_REMOTE_SESSION_ID unset; proxy disabled");
            return default_state;
        }
    };

    let token_path = token_path.unwrap_or(SESSION_TOKEN_PATH);
    let token = match read_token(token_path).await {
        Some(t) => t,
        None => {
            tracing::debug!("[upstreamproxy] no session token file; proxy disabled");
            return default_state;
        }
    };

    let base_url = ccr_base_url
        .map(|s| s.to_string())
        .or_else(|| std::env::var("MOSSEN_CODE_API_BASE_URL").ok())
        .unwrap_or_else(|| "https://api.mossen.invalid".to_string());

    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
    let ca_path = ca_bundle_path
        .map(|s| s.to_string())
        .unwrap_or_else(|| home.join(".ccr").join("ca-bundle.crt").to_string_lossy().to_string());

    let sys_ca = system_ca_path.unwrap_or(SYSTEM_CA_BUNDLE);
    if !download_ca_bundle(&base_url, sys_ca, &ca_path).await {
        return default_state;
    }

    let ws_url = base_url.replacen("http", "ws", 1) + "/v1/code/upstreamproxy/ws";
    match start_upstream_proxy_relay(&ws_url, &session_id, &token).await {
        Ok(relay) => {
            let port = relay.port;
            tracing::debug!("[upstreamproxy] enabled on 127.0.0.1:{}", port);
            // Unlink token file (best-effort)
            let _ = tokio::fs::remove_file(token_path).await;
            UpstreamProxyState {
                enabled: true,
                port: Some(port),
                ca_bundle_path: Some(ca_path),
            }
        }
        Err(e) => {
            tracing::warn!("[upstreamproxy] relay start failed: {}; proxy disabled", e);
            default_state
        }
    }
}

pub fn get_upstream_proxy_env(state: &UpstreamProxyState) -> std::collections::HashMap<String, String> {
    let mut env = std::collections::HashMap::new();
    if !state.enabled {
        // Pass through inherited proxy vars if present
        if let (Ok(proxy), Ok(cert)) = (std::env::var("HTTPS_PROXY"), std::env::var("SSL_CERT_FILE")) {
            env.insert("HTTPS_PROXY".into(), proxy.clone());
            env.insert("https_proxy".into(), proxy);
            env.insert("SSL_CERT_FILE".into(), cert.clone());
            env.insert("NODE_EXTRA_CA_CERTS".into(), cert);
            if let Ok(no_proxy) = std::env::var("NO_PROXY") {
                env.insert("NO_PROXY".into(), no_proxy.clone());
                env.insert("no_proxy".into(), no_proxy);
            }
        }
        return env;
    }
    let port = state.port.unwrap_or(0);
    let ca_path = state.ca_bundle_path.as_deref().unwrap_or("");
    let proxy_url = format!("http://127.0.0.1:{}", port);
    env.insert("HTTPS_PROXY".into(), proxy_url.clone());
    env.insert("https_proxy".into(), proxy_url);
    env.insert("NO_PROXY".into(), NO_PROXY_LIST.into());
    env.insert("no_proxy".into(), NO_PROXY_LIST.into());
    env.insert("SSL_CERT_FILE".into(), ca_path.into());
    env.insert("NODE_EXTRA_CA_CERTS".into(), ca_path.into());
    env.insert("REQUESTS_CA_BUNDLE".into(), ca_path.into());
    env.insert("CURL_CA_BUNDLE".into(), ca_path.into());
    env
}

async fn read_token(path: &str) -> Option<String> {
    match tokio::fs::read_to_string(path).await {
        Ok(content) => {
            let trimmed = content.trim().to_string();
            if trimmed.is_empty() { None } else { Some(trimmed) }
        }
        Err(_) => None,
    }
}

async fn download_ca_bundle(base_url: &str, system_ca_path: &str, out_path: &str) -> bool {
    let url = format!("{}/v1/code/upstreamproxy/ca-cert", base_url);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap_or_default();

    let resp = match client.get(&url).send().await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("[upstreamproxy] ca-cert download failed: {}; proxy disabled", e);
            return false;
        }
    };

    if !resp.status().is_success() {
        tracing::warn!("[upstreamproxy] ca-cert fetch {}; proxy disabled", resp.status());
        return false;
    }

    let ccr_ca = match resp.text().await {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("[upstreamproxy] ca-cert read failed: {}; proxy disabled", e);
            return false;
        }
    };

    let system_ca = tokio::fs::read_to_string(system_ca_path).await.unwrap_or_default();

    if let Some(parent) = std::path::Path::new(out_path).parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
    }

    match tokio::fs::write(out_path, format!("{}\n{}", system_ca, ccr_ca)).await {
        Ok(_) => true,
        Err(e) => {
            tracing::warn!("[upstreamproxy] ca-cert write failed: {}; proxy disabled", e);
            false
        }
    }
}
