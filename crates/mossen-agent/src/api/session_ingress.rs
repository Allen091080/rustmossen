//! # Session Ingress API
//!
//! 翻译自 `services/api/sessionIngress.ts` (515行)
//! 会话日志持久化：追加、获取、Teleport 事件。

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{debug, error};

const MAX_RETRIES: u32 = 10;
const BASE_DELAY_MS: u64 = 500;

/// A transcript message entry for session persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptMessage {
    pub uuid: String,
    #[serde(flatten)]
    pub data: Value,
}

/// A session log entry.
pub type Entry = Value;

/// Session ingress error response.
#[derive(Debug, Clone, Deserialize)]
struct SessionIngressError {
    error: Option<SessionIngressErrorDetail>,
}

#[derive(Debug, Clone, Deserialize)]
struct SessionIngressErrorDetail {
    message: Option<String>,
    #[allow(dead_code)]
    r#type: Option<String>,
}

/// Teleport events response.
#[derive(Debug, Clone, Deserialize)]
struct TeleportEventsResponse {
    data: Vec<TeleportEvent>,
    next_cursor: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct TeleportEvent {
    #[allow(dead_code)]
    event_id: String,
    #[allow(dead_code)]
    event_type: String,
    #[allow(dead_code)]
    is_compaction: bool,
    payload: Option<Entry>,
    #[allow(dead_code)]
    created_at: String,
}

/// Session state tracking.
pub struct SessionState {
    last_uuid_map: HashMap<String, String>,
}

impl SessionState {
    pub fn new() -> Self {
        Self {
            last_uuid_map: HashMap::new(),
        }
    }

    /// Clear cached state for a session.
    pub fn clear_session(&mut self, session_id: &str) {
        self.last_uuid_map.remove(session_id);
    }

    /// Clear all cached session state.
    pub fn clear_all_sessions(&mut self) {
        self.last_uuid_map.clear();
    }
}

impl Default for SessionState {
    fn default() -> Self {
        Self::new()
    }
}

/// Thread-safe session state.
pub type SharedSessionState = Arc<Mutex<SessionState>>;

/// Create a new shared session state.
pub fn new_shared_session_state() -> SharedSessionState {
    Arc::new(Mutex::new(SessionState::new()))
}

/// Internal implementation of append session log with retry logic.
async fn append_session_log_impl(
    client: &Client,
    state: &SharedSessionState,
    session_id: &str,
    entry: &TranscriptMessage,
    url: &str,
    headers: &[(String, String)],
) -> bool {
    for attempt in 1..=MAX_RETRIES {
        let last_uuid = {
            let state = state.lock().await;
            state.last_uuid_map.get(session_id).cloned()
        };

        let mut request_headers = headers.to_vec();
        if let Some(ref last_uuid) = last_uuid {
            request_headers.push(("Last-Uuid".into(), last_uuid.clone()));
        }

        let mut req = client.put(url).json(entry);
        for (key, value) in &request_headers {
            req = req.header(key.as_str(), value.as_str());
        }

        let result = req.send().await;

        match result {
            Ok(response) => {
                let status = response.status().as_u16();
                let resp_headers = response.headers().clone();

                if status == 200 || status == 201 {
                    let mut state = state.lock().await;
                    state
                        .last_uuid_map
                        .insert(session_id.to_string(), entry.uuid.clone());
                    debug!(
                        "Successfully persisted session log entry for session {}",
                        session_id
                    );
                    return true;
                }

                if status == 409 {
                    let server_last_uuid = resp_headers
                        .get("x-last-uuid")
                        .and_then(|v| v.to_str().ok())
                        .map(|s| s.to_string());

                    if server_last_uuid.as_deref() == Some(&entry.uuid) {
                        let mut state = state.lock().await;
                        state
                            .last_uuid_map
                            .insert(session_id.to_string(), entry.uuid.clone());
                        debug!(
                            "Session entry {} already present on server, recovering from stale state",
                            entry.uuid
                        );
                        return true;
                    }

                    if let Some(ref server_uuid) = server_last_uuid {
                        let mut state = state.lock().await;
                        state
                            .last_uuid_map
                            .insert(session_id.to_string(), server_uuid.clone());
                        debug!(
                            "Session 409: adopting server lastUuid={} from header, retrying entry {}",
                            server_uuid, entry.uuid
                        );
                    } else {
                        // Re-fetch session to discover current head
                        let logs =
                            fetch_session_logs_from_url(client, session_id, url, headers).await;
                        let adopted_uuid = find_last_uuid(logs.as_deref());
                        if let Some(adopted) = adopted_uuid {
                            let mut state = state.lock().await;
                            state
                                .last_uuid_map
                                .insert(session_id.to_string(), adopted.clone());
                            debug!(
                                "Session 409: re-fetched entries, adopting lastUuid={}, retrying entry {}",
                                adopted, entry.uuid
                            );
                        } else {
                            error!(
                                "Session persistence conflict: UUID mismatch for session {}, entry {}",
                                session_id, entry.uuid
                            );
                            return false;
                        }
                    }
                    continue;
                }

                if status == 401 {
                    debug!("Session token expired or invalid");
                    return false;
                }

                debug!(
                    "Failed to persist session log: {} {}",
                    status,
                    response.status().canonical_reason().unwrap_or("")
                );
            }
            Err(e) => {
                error!("Error persisting session log: {}", e);
            }
        }

        if attempt == MAX_RETRIES {
            debug!("Remote persistence failed after {} attempts", MAX_RETRIES);
            return false;
        }

        let delay_ms = (BASE_DELAY_MS * 2u64.pow(attempt - 1)).min(8000);
        debug!(
            "Remote persistence attempt {}/{} failed, retrying in {}ms…",
            attempt, MAX_RETRIES, delay_ms
        );
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
    }

    false
}

/// Append a log entry to the session using JWT token.
pub async fn append_session_log(
    client: &Client,
    state: &SharedSessionState,
    session_id: &str,
    entry: &TranscriptMessage,
    url: &str,
    session_token: Option<&str>,
) -> bool {
    let Some(token) = session_token else {
        debug!("No session token available for session persistence");
        return false;
    };

    let headers = vec![
        ("Authorization".to_string(), format!("Bearer {}", token)),
        ("Content-Type".to_string(), "application/json".to_string()),
    ];

    append_session_log_impl(client, state, session_id, entry, url, &headers).await
}

/// Get all session logs for hydration.
pub async fn get_session_logs(
    client: &Client,
    state: &SharedSessionState,
    session_id: &str,
    url: &str,
    session_token: Option<&str>,
) -> Option<Vec<Entry>> {
    let Some(token) = session_token else {
        debug!("No session token available for fetching session logs");
        return None;
    };

    let headers = vec![("Authorization".to_string(), format!("Bearer {}", token))];
    let logs = fetch_session_logs_from_url(client, session_id, url, &headers).await;

    if let Some(ref entries) = logs {
        if !entries.is_empty() {
            let last_uuid = find_last_uuid(Some(entries.as_slice()));
            if let Some(uuid) = last_uuid {
                let mut state = state.lock().await;
                state.last_uuid_map.insert(session_id.to_string(), uuid);
            }
        }
    }

    logs
}

/// Get all session logs for hydration via OAuth.
pub async fn get_session_logs_via_oauth(
    client: &Client,
    session_id: &str,
    base_api_url: &str,
    access_token: &str,
    org_uuid: &str,
) -> Option<Vec<Entry>> {
    let url = format!("{}/v1/session_ingress/session/{}", base_api_url, session_id);
    debug!("[session-ingress] Fetching session logs from: {}", url);
    let headers = vec![
        (
            "Authorization".to_string(),
            format!("Bearer {}", access_token),
        ),
        ("x-organization-uuid".to_string(), org_uuid.to_string()),
    ];
    fetch_session_logs_from_url(client, session_id, &url, &headers).await
}

/// Get worker events (transcript) via the CCR v2 Sessions API.
pub async fn get_teleport_events(
    client: &Client,
    session_id: &str,
    base_api_url: &str,
    access_token: &str,
    org_uuid: &str,
) -> Option<Vec<Entry>> {
    let base_url = format!(
        "{}/v1/code/sessions/{}/teleport-events",
        base_api_url, session_id
    );
    let headers = vec![
        (
            "Authorization".to_string(),
            format!("Bearer {}", access_token),
        ),
        ("x-organization-uuid".to_string(), org_uuid.to_string()),
    ];

    debug!("[teleport] Fetching events from: {}", base_url);

    let mut all: Vec<Entry> = Vec::new();
    let mut cursor: Option<String> = None;
    let mut pages: u32 = 0;
    let max_pages: u32 = 100;

    while pages < max_pages {
        let mut req = client
            .get(&base_url)
            .query(&[("limit", "1000")])
            .timeout(Duration::from_secs(20));

        if let Some(ref c) = cursor {
            req = req.query(&[("cursor", c.as_str())]);
        }

        for (key, value) in &headers {
            req = req.header(key.as_str(), value.as_str());
        }

        let response = match req.send().await {
            Ok(resp) => resp,
            Err(e) => {
                error!("Teleport events fetch failed: {}", e);
                return None;
            }
        };

        let status = response.status().as_u16();

        if status == 404 {
            debug!(
                "[teleport] Session {} not found (page {})",
                session_id, pages
            );
            return if pages == 0 { None } else { Some(all) };
        }

        if status == 401 {
            error!("Your session has expired. Refresh the Mossen bridge adapter credentials and try again.");
            return None;
        }

        if status != 200 {
            error!("Teleport events returned {}", status);
            return None;
        }

        let body: TeleportEventsResponse = match response.json().await {
            Ok(data) => data,
            Err(e) => {
                error!("Teleport events invalid response shape: {}", e);
                return None;
            }
        };

        for ev in &body.data {
            if let Some(ref payload) = ev.payload {
                all.push(payload.clone());
            }
        }

        pages += 1;

        match body.next_cursor {
            Some(next) if !next.is_empty() => cursor = Some(next),
            _ => break,
        }
    }

    if pages >= max_pages {
        error!(
            "Teleport events hit page cap ({}) for {}",
            max_pages, session_id
        );
    }

    debug!(
        "[teleport] Fetched {} events over {} page(s) for {}",
        all.len(),
        pages,
        session_id
    );
    Some(all)
}

/// Shared implementation for fetching session logs from a URL.
async fn fetch_session_logs_from_url(
    client: &Client,
    session_id: &str,
    url: &str,
    headers: &[(String, String)],
) -> Option<Vec<Entry>> {
    let mut req = client.get(url).timeout(Duration::from_secs(20));

    for (key, value) in headers {
        req = req.header(key.as_str(), value.as_str());
    }

    match req.send().await {
        Ok(response) => {
            let status = response.status().as_u16();

            if status == 200 {
                let data: Value = match response.json().await {
                    Ok(d) => d,
                    Err(e) => {
                        error!("Invalid session logs response format: {}", e);
                        return None;
                    }
                };

                let loglines = data.get("loglines").and_then(|v| v.as_array());
                match loglines {
                    Some(entries) => {
                        debug!(
                            "Fetched {} session logs for session {}",
                            entries.len(),
                            session_id
                        );
                        Some(entries.clone())
                    }
                    None => {
                        error!("Invalid session logs response format: missing loglines");
                        None
                    }
                }
            } else if status == 404 {
                debug!("No existing logs for session {}", session_id);
                Some(Vec::new())
            } else if status == 401 {
                debug!("Auth token expired or invalid");
                None
            } else {
                debug!("Failed to fetch session logs: {}", status);
                None
            }
        }
        Err(e) => {
            error!("Error fetching session logs: {}", e);
            None
        }
    }
}

/// Walk backward through entries to find the last one with a uuid.
fn find_last_uuid(logs: Option<&[Entry]>) -> Option<String> {
    let logs = logs?;
    logs.iter().rev().find_map(|entry| {
        entry
            .get("uuid")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    })
}
