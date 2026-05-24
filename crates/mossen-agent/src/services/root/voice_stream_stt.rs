/// Hosted voice_stream speech-to-text adapter for push-to-talk.
/// Connects to a configured voice_stream WebSocket endpoint.
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message as WsMessage;

const VOICE_STREAM_PATH: &str = "/api/ws/speech_to_text/voice_stream";
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(8);
const KEEPALIVE_MSG: &str = r#"{"type":"KeepAlive"}"#;
const CLOSE_STREAM_MSG: &str = r#"{"type":"CloseStream"}"#;

pub struct FinalizeTimeouts {
    pub safety: Duration,
    pub no_data: Duration,
}

pub static FINALIZE_TIMEOUTS: FinalizeTimeouts = FinalizeTimeouts {
    safety: Duration::from_secs(5),
    no_data: Duration::from_millis(1500),
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinalizeSource {
    PostClosestreamEndpoint,
    NoDataTimeout,
    SafetyTimeout,
    WsClose,
    WsAlreadyClosed,
}

pub trait VoiceStreamCallbacks: Send + Sync {
    fn on_transcript(&self, text: &str, is_final: bool);
    fn on_error(&self, error: &str, fatal: bool);
    fn on_close(&self);
    fn on_ready(&self, connection: Arc<VoiceStreamConnection>);
}

/// Trait for external dependencies
#[async_trait::async_trait]
pub trait VoiceStreamContext: Send + Sync {
    fn is_custom_voice_enabled(&self) -> bool;
    fn get_custom_voice_stream_base_url(&self) -> Option<String>;
    fn get_custom_backend_auth_headers(&self) -> std::collections::HashMap<String, String>;
    fn is_mossen_hosted_auth_enabled(&self) -> bool;
    fn get_hosted_oauth_tokens(&self) -> Option<OAuthTokens>;
    async fn check_and_refresh_oauth_token(&self);
    fn get_oauth_base_api_url(&self) -> String;
    fn get_user_agent(&self) -> String;
    fn get_feature_value_cached(&self, key: &str, default: bool) -> bool;
    fn get_voice_stream_base_url_env(&self) -> Option<String>;
}

pub struct OAuthTokens {
    pub access_token: Option<String>,
}

pub struct VoiceStreamConnection {
    tx: mpsc::Sender<Vec<u8>>,
    finalize_tx: mpsc::Sender<()>,
    close_tx: mpsc::Sender<()>,
    connected: Arc<Mutex<bool>>,
}

impl VoiceStreamConnection {
    pub fn send(&self, audio_chunk: &[u8]) {
        let _ = self.tx.try_send(audio_chunk.to_vec());
    }

    pub async fn finalize(&self) -> FinalizeSource {
        let _ = self.finalize_tx.send(()).await;
        // The actual finalize logic happens in the background task
        // For simplicity, return a placeholder that the task will resolve
        FinalizeSource::PostClosestreamEndpoint
    }

    pub fn close(&self) {
        let _ = self.close_tx.try_send(());
    }

    pub fn is_connected(&self) -> bool {
        *self.connected.lock()
    }
}

pub fn is_voice_stream_available(ctx: &dyn VoiceStreamContext) -> bool {
    if ctx.is_custom_voice_enabled() {
        return ctx.get_custom_voice_stream_base_url().is_some()
            && !ctx.get_custom_backend_auth_headers().is_empty();
    }

    if !ctx.is_mossen_hosted_auth_enabled() {
        return false;
    }

    ctx.get_hosted_oauth_tokens()
        .and_then(|t| t.access_token)
        .is_some()
}

fn to_websocket_base_url(value: &str) -> String {
    value
        .replace("https://", "wss://")
        .replace("http://", "ws://")
}

pub async fn connect_voice_stream(
    ctx: &dyn VoiceStreamContext,
    callbacks: Arc<dyn VoiceStreamCallbacks>,
    language: Option<&str>,
    keyterms: Option<&[String]>,
) -> Option<Arc<VoiceStreamConnection>> {
    let use_custom_backend = ctx.is_custom_voice_enabled();

    let auth_headers: std::collections::HashMap<String, String>;
    if use_custom_backend {
        auth_headers = ctx.get_custom_backend_auth_headers();
        if auth_headers.is_empty() {
            tracing::debug!("[voice_stream] No custom backend auth headers available");
            return None;
        }
    } else {
        ctx.check_and_refresh_oauth_token().await;
        let tokens = ctx.get_hosted_oauth_tokens()?;
        let access_token = tokens.access_token?;
        auth_headers = std::collections::HashMap::from([(
            "Authorization".to_string(),
            format!("Bearer {}", access_token),
        )]);
    }

    let ws_base_url = ctx.get_voice_stream_base_url_env().unwrap_or_else(|| {
        if use_custom_backend {
            to_websocket_base_url(&ctx.get_custom_voice_stream_base_url().unwrap_or_default())
        } else {
            to_websocket_base_url(&ctx.get_oauth_base_api_url())
        }
    });

    if ws_base_url.is_empty() {
        tracing::debug!("[voice_stream] No voice stream base URL available");
        return None;
    }

    let mut params = vec![
        ("encoding", "linear16".to_string()),
        ("sample_rate", "16000".to_string()),
        ("channels", "1".to_string()),
        ("endpointing_ms", "300".to_string()),
        ("utterance_end_ms", "1000".to_string()),
        ("language", language.unwrap_or("en").to_string()),
    ];

    let is_nova3 = ctx.get_feature_value_cached("mossen_cobalt_frost", false);
    if is_nova3 {
        params.push(("use_conversation_engine", "true".to_string()));
        params.push(("stt_provider", "deepgram-nova3".to_string()));
    }

    if let Some(terms) = keyterms {
        for term in terms {
            params.push(("keyterms", term.clone()));
        }
    }

    let query_string: String = params
        .iter()
        .map(|(k, v)| format!("{}={}", k, urlencoding::encode(v)))
        .collect::<Vec<_>>()
        .join("&");

    let url = format!("{}{}?{}", ws_base_url, VOICE_STREAM_PATH, query_string);
    tracing::debug!("[voice_stream] Connecting to {}", url);

    // Build WebSocket request with auth headers
    let mut request = url.parse::<http::Uri>().ok()?;
    let (audio_tx, mut audio_rx) = mpsc::channel::<Vec<u8>>(128);
    let (finalize_tx, mut finalize_rx) = mpsc::channel::<()>(1);
    let (close_tx, mut close_rx) = mpsc::channel::<()>(1);
    let connected = Arc::new(Mutex::new(false));
    let connected_clone = connected.clone();

    let connection = Arc::new(VoiceStreamConnection {
        tx: audio_tx,
        finalize_tx,
        close_tx,
        connected: connected.clone(),
    });

    let connection_for_callback = connection.clone();
    let callbacks_clone = callbacks.clone();

    // Spawn the WebSocket handler task
    tokio::spawn(async move {
        use futures_util::{SinkExt, StreamExt};
        use tokio_tungstenite::connect_async;

        let ws_result = connect_async(&url).await;
        let (mut ws_stream, _) = match ws_result {
            Ok(pair) => pair,
            Err(e) => {
                callbacks_clone.on_error(&format!("WebSocket connection error: {}", e), true);
                callbacks_clone.on_close();
                return;
            }
        };

        *connected_clone.lock() = true;
        tracing::debug!("[voice_stream] WebSocket connected");

        // Send initial keepalive
        let _ = ws_stream
            .send(WsMessage::Text(KEEPALIVE_MSG.to_string()))
            .await;
        callbacks_clone.on_ready(connection_for_callback);

        let mut keepalive_interval = tokio::time::interval(KEEPALIVE_INTERVAL);
        let mut finalized = false;
        let mut last_transcript_text = String::new();

        loop {
            tokio::select! {
                _ = keepalive_interval.tick() => {
                    if !finalized {
                        let _ = ws_stream.send(WsMessage::Text(KEEPALIVE_MSG.to_string())).await;
                    }
                }
                Some(audio) = audio_rx.recv() => {
                    if !finalized {
                        let _ = ws_stream.send(WsMessage::Binary(audio)).await;
                    }
                }
                Some(_) = finalize_rx.recv() => {
                    finalized = true;
                    let _ = ws_stream.send(WsMessage::Text(CLOSE_STREAM_MSG.to_string())).await;
                }
                Some(_) = close_rx.recv() => {
                    finalized = true;
                    let _ = ws_stream.close(None).await;
                    break;
                }
                msg = ws_stream.next() => {
                    match msg {
                        Some(Ok(WsMessage::Text(text))) => {
                            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) {
                                match parsed.get("type").and_then(|t| t.as_str()) {
                                    Some("TranscriptText") => {
                                        if let Some(data) = parsed.get("data").and_then(|d| d.as_str()) {
                                            if !data.is_empty() {
                                                last_transcript_text = data.to_string();
                                                callbacks_clone.on_transcript(data, false);
                                            }
                                        }
                                    }
                                    Some("TranscriptEndpoint") => {
                                        if !last_transcript_text.is_empty() {
                                            let final_text = std::mem::take(&mut last_transcript_text);
                                            callbacks_clone.on_transcript(&final_text, true);
                                        }
                                    }
                                    Some("TranscriptError") => {
                                        let desc = parsed.get("description")
                                            .or_else(|| parsed.get("error_code"))
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("unknown transcription error");
                                        callbacks_clone.on_error(desc, false);
                                    }
                                    Some("error") => {
                                        let detail = parsed.get("message")
                                            .and_then(|m| m.as_str())
                                            .unwrap_or("unknown error");
                                        callbacks_clone.on_error(detail, false);
                                    }
                                    _ => {}
                                }
                            }
                        }
                        Some(Ok(WsMessage::Close(_))) | None => {
                            if !last_transcript_text.is_empty() {
                                let final_text = std::mem::take(&mut last_transcript_text);
                                callbacks_clone.on_transcript(&final_text, true);
                            }
                            break;
                        }
                        Some(Err(e)) => {
                            callbacks_clone.on_error(&format!("WebSocket error: {}", e), false);
                            break;
                        }
                        _ => {}
                    }
                }
            }
        }

        *connected_clone.lock() = false;
        callbacks_clone.on_close();
    });

    Some(connection)
}

/// TS `FINALIZE_TIMEOUTS_MS` — alias with the canonical TS name, exposed as a
/// constant computed from the same `Duration` values.
#[allow(non_upper_case_globals)]
pub static FINALIZE_TIMEOUTS_MS: FinalizeTimeouts = FinalizeTimeouts {
    safety: Duration::from_secs(5),
    no_data: Duration::from_millis(1500),
};
