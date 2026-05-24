//! Teleport utilities — translated from utils/teleport/
//! Remote session API, environment management, git bundles

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;
use tracing::debug;
use uuid::Uuid;

// --- Constants ---
const TELEPORT_RETRY_DELAYS: &[u64] = &[2000, 4000, 8000, 16000];
pub const CCR_BYOC_BETA: &str = "ccr-byoc-2025-07-29";

// --- Types ---

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    RequiresAction,
    Running,
    Idle,
    Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SessionContextSource {
    #[serde(rename = "git_repository")]
    Git {
        url: String,
        revision: Option<String>,
        allow_unrestricted_git_push: Option<bool>,
    },
    #[serde(rename = "knowledge_base")]
    KnowledgeBase { knowledge_base_id: String },
}

/// Standalone struct variant of [`SessionContextSource::Git`] — mirror of
/// TS `GitSource`. The TS port models `SessionContextSource` as a tagged
/// union and exposes the individual cases as named types; the Rust port
/// uses an enum, so this struct exists for callers that want the
/// shape without the tag wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitSource {
    #[serde(rename = "type")]
    pub source_type: String,
    pub url: String,
    pub revision: Option<String>,
    pub allow_unrestricted_git_push: Option<bool>,
}

/// Standalone struct variant of [`SessionContextSource::KnowledgeBase`] —
/// mirror of TS `KnowledgeBaseSource`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeBaseSource {
    #[serde(rename = "type")]
    pub source_type: String,
    pub knowledge_base_id: String,
}

impl GitSource {
    pub fn into_context_source(self) -> SessionContextSource {
        SessionContextSource::Git {
            url: self.url,
            revision: self.revision,
            allow_unrestricted_git_push: self.allow_unrestricted_git_push,
        }
    }
}

impl KnowledgeBaseSource {
    pub fn into_context_source(self) -> SessionContextSource {
        SessionContextSource::KnowledgeBase {
            knowledge_base_id: self.knowledge_base_id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutcomeGitInfo {
    #[serde(rename = "type")]
    pub outcome_type: String,
    pub repo: String,
    pub branches: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitRepositoryOutcome {
    #[serde(rename = "type")]
    pub outcome_type: String,
    pub git_info: OutcomeGitInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionContext {
    pub sources: Vec<SessionContextSource>,
    pub cwd: String,
    pub outcomes: Option<Vec<GitRepositoryOutcome>>,
    pub custom_system_prompt: Option<String>,
    pub append_system_prompt: Option<String>,
    pub model: Option<String>,
    pub seed_bundle_file_id: Option<String>,
    pub github_pr: Option<GithubPr>,
    pub reuse_outcome_branches: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubPr {
    pub owner: String,
    pub repo: String,
    pub number: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionResource {
    pub id: String,
    pub title: Option<String>,
    pub session_status: SessionStatus,
    pub environment_id: String,
    pub created_at: String,
    pub updated_at: String,
    pub session_context: SessionContext,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListSessionsResponse {
    pub data: Vec<SessionResource>,
    pub has_more: bool,
    pub first_id: Option<String>,
    pub last_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeSession {
    pub id: String,
    pub title: String,
    pub description: String,
    pub status: String,
    pub repo: Option<CodeSessionRepo>,
    pub turns: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeSessionRepo {
    pub name: String,
    pub owner: CodeSessionRepoOwner,
    pub default_branch: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeSessionRepoOwner {
    pub login: String,
}

/// Message content for remote sessions
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RemoteMessageContent {
    Text(String),
    Blocks(Vec<serde_json::Value>),
}

// --- API Functions ---

/// Check if an error is a transient network error worth retrying
pub fn is_transient_network_error(status: Option<u16>) -> bool {
    match status {
        None => true, // No response = network error
        Some(s) if s >= 500 => true,
        _ => false,
    }
}

/// Create OAuth headers for API requests
pub fn get_oauth_headers(access_token: &str) -> Vec<(String, String)> {
    vec![
        (
            "Authorization".to_string(),
            format!("Bearer {}", access_token),
        ),
        ("Content-Type".to_string(), "application/json".to_string()),
        ("mossen-version".to_string(), "2023-06-01".to_string()),
    ]
}

/// Make a GET request with retry for transient errors
pub async fn axios_get_with_retry(
    client: &reqwest::Client,
    url: &str,
    headers: &[(String, String)],
) -> Result<reqwest::Response> {
    let mut last_error = None;

    for attempt in 0..=TELEPORT_RETRY_DELAYS.len() {
        let mut request = client.get(url);
        for (key, value) in headers {
            request = request.header(key.as_str(), value.as_str());
        }

        match request.send().await {
            Ok(response) => {
                if response.status().is_success() || response.status().is_client_error() {
                    return Ok(response);
                }
                if !is_transient_network_error(Some(response.status().as_u16())) {
                    return Ok(response);
                }
                last_error = Some(anyhow::anyhow!("Server error: {}", response.status()));
            }
            Err(e) => {
                last_error = Some(e.into());
            }
        }

        if attempt < TELEPORT_RETRY_DELAYS.len() {
            let delay = TELEPORT_RETRY_DELAYS[attempt];
            debug!(
                "Teleport request failed (attempt {}/{}), retrying in {}ms",
                attempt + 1,
                TELEPORT_RETRY_DELAYS.len() + 1,
                delay
            );
            tokio::time::sleep(Duration::from_millis(delay)).await;
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Request failed after retries")))
}

/// Fetch a single session by ID
pub async fn fetch_session(
    client: &reqwest::Client,
    base_url: &str,
    session_id: &str,
    access_token: &str,
    org_uuid: &str,
) -> Result<SessionResource> {
    let url = format!("{}/v1/sessions/{}", base_url, session_id);
    let mut headers = get_oauth_headers(access_token);
    headers.push(("mossen-beta".to_string(), CCR_BYOC_BETA.to_string()));
    headers.push(("x-organization-uuid".to_string(), org_uuid.to_string()));

    let mut request = client.get(&url).timeout(Duration::from_secs(15));
    for (key, value) in &headers {
        request = request.header(key.as_str(), value.as_str());
    }

    let response = request.send().await?;
    let status = response.status().as_u16();

    if status == 404 {
        bail!("Session not found: {}", session_id);
    }
    if status == 401 {
        bail!("Hosted bridge session expired. Refresh the token before retrying.");
    }
    if status != 200 {
        bail!(
            "Failed to fetch session: {} {}",
            status,
            response.status().canonical_reason().unwrap_or("")
        );
    }

    let session: SessionResource = response.json().await?;
    Ok(session)
}

/// Get branch from a session's git repository outcomes
pub fn get_branch_from_session(session: &SessionResource) -> Option<&str> {
    session
        .session_context
        .outcomes
        .as_ref()?
        .iter()
        .find(|o| o.outcome_type == "git_repository")
        .and_then(|o| o.git_info.branches.first())
        .map(|s| s.as_str())
}

/// Send a user message event to an existing remote session
pub async fn send_event_to_remote_session(
    client: &reqwest::Client,
    base_url: &str,
    session_id: &str,
    message_content: &RemoteMessageContent,
    access_token: &str,
    org_uuid: &str,
    event_uuid: Option<&str>,
) -> bool {
    let url = format!("{}/v1/sessions/{}/events", base_url, session_id);
    let mut headers = get_oauth_headers(access_token);
    headers.push(("mossen-beta".to_string(), CCR_BYOC_BETA.to_string()));
    headers.push(("x-organization-uuid".to_string(), org_uuid.to_string()));

    let uuid = event_uuid
        .map(|s| s.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let body = serde_json::json!({
        "events": [{
            "uuid": uuid,
            "session_id": session_id,
            "type": "user",
            "parent_tool_use_id": null,
            "message": {
                "role": "user",
                "content": message_content,
            }
        }]
    });

    debug!(
        "[sendEventToRemoteSession] Sending event to session {}",
        session_id
    );

    let mut request = client
        .post(&url)
        .timeout(Duration::from_secs(30))
        .json(&body);
    for (key, value) in &headers {
        request = request.header(key.as_str(), value.as_str());
    }

    match request.send().await {
        Ok(response) => {
            let status = response.status().as_u16();
            if status == 200 || status == 201 {
                debug!("[sendEventToRemoteSession] Successfully sent event");
                true
            } else {
                debug!("[sendEventToRemoteSession] Failed with status {}", status);
                false
            }
        }
        Err(e) => {
            debug!("[sendEventToRemoteSession] Error: {}", e);
            false
        }
    }
}

/// Update the title of an existing remote session
pub async fn update_session_title(
    client: &reqwest::Client,
    base_url: &str,
    session_id: &str,
    title: &str,
    access_token: &str,
    org_uuid: &str,
) -> bool {
    let url = format!("{}/v1/sessions/{}", base_url, session_id);
    let mut headers = get_oauth_headers(access_token);
    headers.push(("mossen-beta".to_string(), CCR_BYOC_BETA.to_string()));
    headers.push(("x-organization-uuid".to_string(), org_uuid.to_string()));

    let body = serde_json::json!({ "title": title });

    let mut request = client.patch(&url).json(&body);
    for (key, value) in &headers {
        request = request.header(key.as_str(), value.as_str());
    }

    match request.send().await {
        Ok(response) => response.status().as_u16() == 200,
        Err(e) => {
            debug!("[updateSessionTitle] Error: {}", e);
            false
        }
    }
}

// --- Environments ---

/// Remote environment info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteEnvironment {
    pub id: String,
    pub name: String,
    pub status: String,
}

/// Fetch available remote environments
pub async fn fetch_environments(
    client: &reqwest::Client,
    base_url: &str,
    access_token: &str,
    org_uuid: &str,
) -> Result<Vec<RemoteEnvironment>> {
    let url = format!(
        "{}/api/oauth/organizations/{}/environments",
        base_url, org_uuid
    );
    let mut headers = get_oauth_headers(access_token);
    headers.push(("x-organization-uuid".to_string(), org_uuid.to_string()));

    let mut request = client.get(&url).timeout(Duration::from_secs(15));
    for (key, value) in &headers {
        request = request.header(key.as_str(), value.as_str());
    }

    let response = request.send().await?;
    if !response.status().is_success() {
        bail!("Failed to fetch environments: {}", response.status());
    }

    let environments: Vec<RemoteEnvironment> = response.json().await?;
    Ok(environments)
}

// --- Git Bundle ---

/// Create a git bundle from the current repository
pub async fn create_git_bundle(cwd: &Path, ref_name: &str) -> Result<Vec<u8>> {
    let bundle_path = cwd.join(".git").join("mossen-bundle.bundle");

    let output = tokio::process::Command::new("git")
        .args([
            "bundle",
            "create",
            bundle_path.to_str().unwrap_or("bundle"),
            ref_name,
        ])
        .current_dir(cwd)
        .output()
        .await?;

    if !output.status.success() {
        bail!(
            "git bundle create failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let data = tokio::fs::read(&bundle_path).await?;
    let _ = tokio::fs::remove_file(&bundle_path).await;
    Ok(data)
}

// --- Additional translations from utils/teleport/api.ts ---

/// Type alias matching the TS `Outcome` discriminated union. There is currently
/// only one variant (`GitRepositoryOutcome`), so we alias to it directly —
/// adding new variants here keeps the union extensible without touching call
/// sites that read `Outcome`.
pub type Outcome = GitRepositoryOutcome;

/// JSON schema for [`CodeSession`].
///
/// Mirrors `CodeSessionSchema = lazySchema(() => z.object({...}))` from TS.
/// Used by callers that need to validate untyped API responses before
/// `serde_json::from_value::<CodeSession>(...)`; the schema captures the same
/// enum constraint on `status` that the Zod schema enforces.
pub fn code_session_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "id": { "type": "string" },
            "title": { "type": "string" },
            "description": { "type": "string" },
            "status": {
                "type": "string",
                "enum": [
                    "idle",
                    "working",
                    "waiting",
                    "completed",
                    "archived",
                    "cancelled",
                    "rejected",
                ],
            },
            "repo": {
                "anyOf": [
                    { "type": "null" },
                    {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string" },
                            "owner": {
                                "type": "object",
                                "properties": { "login": { "type": "string" } },
                                "required": ["login"],
                            },
                            "default_branch": { "type": "string" },
                        },
                        "required": ["name", "owner"],
                    },
                ],
            },
            "turns": { "type": "array", "items": { "type": "string" } },
            "created_at": { "type": "string" },
            "updated_at": { "type": "string" },
        },
        "required": [
            "id", "title", "description", "status", "repo", "turns",
            "created_at", "updated_at",
        ],
    })
}

/// Map a session-status string (`session_status` on the API resource) to the
/// outward-facing `CodeSession.status` enum value.
///
/// Mirrors the TS expression
/// `status: session.session_status as CodeSession['status']` in
/// `fetchCodeSessionsFromSessionsAPI`. The cast is lenient: unknown server
/// values pass through unchanged so callers can spot drift.
pub fn map_session_status_to_code_session_status(status: &SessionStatus) -> String {
    match status {
        SessionStatus::RequiresAction => "waiting".to_string(),
        SessionStatus::Running => "working".to_string(),
        SessionStatus::Idle => "idle".to_string(),
        SessionStatus::Archived => "archived".to_string(),
    }
}

/// Pair returned by [`prepare_api_request`]: the access token and the
/// organization UUID resolved from the OAuth layer.
#[derive(Debug, Clone)]
pub struct ApiRequestCredentials {
    pub access_token: String,
    pub org_uuid: String,
}

/// Validate and prepare for API requests.
///
/// Mirrors `prepareApiRequest` from TS. The TS version reads tokens from
/// `getHostedOAuthTokens()` and the org UUID from `getOrganizationUUID()`;
/// in Rust we accept those as injected closures so this helper has no
/// implicit module-level state.
pub async fn prepare_api_request(
    get_access_token: impl FnOnce() -> Option<String>,
    get_org_uuid: impl std::future::Future<Output = Option<String>>,
) -> Result<ApiRequestCredentials> {
    let access_token = get_access_token().ok_or_else(|| {
        anyhow::anyhow!(
            "Hosted web sessions require an explicit Mossen bridge adapter token. \
             Backend API credentials alone are not sufficient. Enable \
             MOSSEN_CODE_ENABLE_HOSTED_AUTH_ADAPTER=1 only when wrapping that \
             external service, then inject the adapter token before retrying."
        )
    })?;

    let org_uuid = get_org_uuid
        .await
        .ok_or_else(|| anyhow::anyhow!("Unable to get organization UUID"))?;

    Ok(ApiRequestCredentials {
        access_token,
        org_uuid,
    })
}

/// Parse a Github repo URL into `owner/name`, returning `None` when the URL
/// doesn't look like a GitHub repository. Tolerates the common forms used by
/// the teleport API: `https://github.com/owner/repo(.git)?`, `git@github.com:owner/repo`,
/// and `github.com/owner/repo`.
fn parse_github_repository_url(url: &str) -> Option<(String, String)> {
    let trimmed = url.trim();
    // SSH style: git@github.com:owner/repo(.git)?
    if let Some(rest) = trimmed.strip_prefix("git@github.com:") {
        return split_owner_repo(rest);
    }
    // HTTPS / generic URL with a path containing github.com/owner/repo
    let lowered = trimmed.to_lowercase();
    if let Some(idx) = lowered.find("github.com/") {
        let path_start = idx + "github.com/".len();
        if path_start <= trimmed.len() {
            return split_owner_repo(&trimmed[path_start..]);
        }
    }
    None
}

fn split_owner_repo(rest: &str) -> Option<(String, String)> {
    let cleaned = rest.trim_start_matches('/').trim_end_matches('/');
    let mut parts = cleaned.splitn(3, '/');
    let owner = parts.next()?.to_string();
    let repo_raw = parts.next()?;
    let repo = repo_raw.trim_end_matches(".git").to_string();
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some((owner, repo))
}

/// Transform a [`SessionResource`] into a [`CodeSession`].
///
/// Mirrors the inline `.map(...)` body inside `fetchCodeSessionsFromSessionsAPI`
/// in TS. Extracted as its own function so callers can reuse it (e.g. when
/// hydrating sessions from a cache that already returns `SessionResource`).
pub fn code_session_from_session_resource(session: &SessionResource) -> CodeSession {
    let mut repo: Option<CodeSessionRepo> = None;
    for source in &session.session_context.sources {
        if let SessionContextSource::Git { url, revision, .. } = source {
            if !url.is_empty() {
                if let Some((owner, name)) = parse_github_repository_url(url) {
                    repo = Some(CodeSessionRepo {
                        name,
                        owner: CodeSessionRepoOwner { login: owner },
                        default_branch: revision.clone(),
                    });
                    break;
                }
            }
        }
    }

    CodeSession {
        id: session.id.clone(),
        title: session
            .title
            .clone()
            .unwrap_or_else(|| "Untitled".to_string()),
        description: String::new(),
        status: map_session_status_to_code_session_status(&session.session_status),
        repo,
        turns: Vec::new(),
        created_at: session.created_at.clone(),
        updated_at: session.updated_at.clone(),
    }
}

/// Fetch code sessions from the Sessions API (`/v1/sessions`).
///
/// Mirrors `fetchCodeSessionsFromSessionsAPI` from TS. Returns the parsed
/// `CodeSession` list on success; transient errors are retried under
/// [`axios_get_with_retry`].
pub async fn fetch_code_sessions_from_sessions_api(
    client: &reqwest::Client,
    base_url: &str,
    access_token: &str,
    org_uuid: &str,
) -> Result<Vec<CodeSession>> {
    let url = format!("{}/v1/sessions", base_url);
    let mut headers = get_oauth_headers(access_token);
    headers.push(("mossen-beta".to_string(), CCR_BYOC_BETA.to_string()));
    headers.push(("x-organization-uuid".to_string(), org_uuid.to_string()));

    let response = axios_get_with_retry(client, &url, &headers).await?;
    let status = response.status().as_u16();
    if status != 200 {
        bail!(
            "Failed to fetch code sessions: {} {}",
            status,
            response.status().canonical_reason().unwrap_or("")
        );
    }

    let body: ListSessionsResponse = response.json().await?;
    Ok(body
        .data
        .iter()
        .map(code_session_from_session_resource)
        .collect())
}
