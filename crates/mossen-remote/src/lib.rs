//! # mossen-remote
//!
//! Mossen 远程通信层 — 提供 WebSocket、HTTP/SSE、Stdio 等传输协议支持，
//! 用于远程会话管理、结构化 IO 协议和数据同步。

pub mod connection;
pub mod http;
pub mod ndjson;
pub mod print;
pub mod remote_io;
pub mod remote_session;
pub mod server;
pub mod stdio;
pub mod structured_io;
pub mod transport;
pub mod upstreamproxy;
pub mod websocket;

pub use remote_session::{
    convert_sdk_message, convert_sdk_message_typed, create_remote_session_config,
    create_synthetic_assistant_message, create_tool_stub, get_result_text, is_session_end_message,
    is_success_result, ConvertedMessage, RemotePermissionResponse, RemoteSessionCallbacks,
    RemoteSessionConfig, RemoteSessionManager, SessionsWebSocketCallbacks,
};
pub use upstreamproxy::{
    decode_chunk, encode_chunk, get_upstream_proxy_env, init_upstream_proxy,
    reset_upstream_proxy_for_tests, set_upstream_proxy_state, start_node_relay,
    start_upstream_proxy_relay, UpstreamProxyRelay, UpstreamProxyState, SESSION_TOKEN_PATH,
};

// Re-export 核心类型
pub use connection::{HeartbeatManager, ReconnectPolicy, ReconnectTracker, ResolvedIdTracker};
pub use http::{SerialBatchEventUploader, SseTransport, WorkerStateUploader};
pub use ndjson::{ndjson_safe_line, ndjson_safe_stringify, parse_ndjson_buffer, parse_ndjson_line};
pub use print::{write_to_stdout, write_to_stdout_sync};
pub use remote_io::RemoteIo;
pub use server::{
    create_direct_connect_session, ConnectResponse, DirectConnectConfig,
    DirectConnectSessionManager, ServerConfig,
};
pub use stdio::StdioTransport;
pub use structured_io::StructuredIo;
pub use transport::{
    select_transport_for_url, OnCloseCallback, OnConnectCallback, OnDataCallback, StdinMessage,
    StdoutMessage, Transport, TransportKind, TransportOptions, TransportState,
};
pub use websocket::{SessionsWebSocket, WebSocketTransport};
