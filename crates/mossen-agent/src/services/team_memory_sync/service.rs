//! Team memory sync service — syncs team memory files between local filesystem and server API.

use reqwest::Client;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use tokio::fs;
use tracing::{debug, info, warn};

use mossen_utils::auth::get_hosted_oauth_tokens;
use mossen_utils::detect_repository::parse_git_remote;

use super::secret_scanner::scan_for_secrets;
use super::types::*;

const TEAM_MEMORY_SYNC_TIMEOUT_MS: u64 = 30_000;
const MAX_FILE_SIZE_BYTES: u64 = 250_000;
const MAX_PUT_BODY_BYTES: usize = 200_000;
const MAX_RETRIES: u32 = 3;
const MAX_CONFLICT_RETRIES: u32 = 2;

/// Mutable state for the team memory sync service.
pub struct SyncState {
    /// Last known server checksum (ETag) for conditional requests.
    pub last_known_checksum: Option<String>,
    /// Per-key content hash of what the server holds.
    pub server_checksums: HashMap<String, String>,
    /// Server-enforced max_entries cap, learned from a structured 413.
    pub server_max_entries: Option<u64>,
}

/// Create a fresh sync state.
pub fn create_sync_state() -> SyncState {
    SyncState {
        last_known_checksum: None,
        server_checksums: HashMap::new(),
        server_max_entries: None,
    }
}

/// Compute `sha256:<hex>` over the UTF-8 bytes of the given content.
pub fn hash_content(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("sha256:{:x}", hasher.finalize())
}

/// Check if team memory sync is available (requires first-party OAuth).
pub fn is_team_memory_sync_available() -> bool {
    get_team_memory_access_token().is_some()
}

/// Configuration for team memory sync endpoints.
struct SyncConfig {
    base_api_url: String,
    access_token: Option<String>,
    user_agent: String,
}

fn get_sync_config() -> SyncConfig {
    SyncConfig {
        base_api_url: resolve_base_api_url(get_env_config_value(&[
            "TEAM_MEMORY_SYNC_URL",
            "MOSSEN_TEAM_MEMORY_SYNC_URL",
            "MOSSEN_API_BASE_URL",
        ])),
        access_token: get_team_memory_access_token(),
        user_agent: "mossen-agent/1.0".to_string(),
    }
}

fn clean_config_value(value: impl AsRef<str>) -> Option<String> {
    let trimmed = value.as_ref().trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn get_env_config_value(names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|name| std::env::var(name).ok().and_then(clean_config_value))
}

fn resolve_base_api_url(explicit_url: Option<String>) -> String {
    explicit_url
        .and_then(clean_config_value)
        .and_then(|url| clean_config_value(url.trim_end_matches('/')))
        .unwrap_or_else(|| "https://api.mossen.ai".to_string())
}

fn resolve_access_token(
    explicit_token: Option<String>,
    oauth_token: Option<String>,
) -> Option<String> {
    explicit_token
        .and_then(clean_config_value)
        .or_else(|| oauth_token.and_then(clean_config_value))
}

fn get_team_memory_access_token() -> Option<String> {
    resolve_access_token(
        get_env_config_value(&["TEAM_MEMORY_SYNC_TOKEN", "MOSSEN_TEAM_MEMORY_SYNC_TOKEN"]),
        get_hosted_oauth_tokens().map(|tokens| tokens.access_token),
    )
}

fn normalize_repo_slug(slug: impl AsRef<str>) -> Option<String> {
    let trimmed = slug.as_ref().trim().trim_matches('/');
    if trimmed.is_empty() {
        return None;
    }

    if let Some(parsed) = parse_git_remote(trimmed) {
        return Some(format!("{}/{}", parsed.owner, parsed.name));
    }

    let without_host = trimmed
        .strip_prefix("github.com/")
        .or_else(|| trimmed.strip_prefix("www.github.com/"))
        .unwrap_or(trimmed);
    let mut parts = without_host.split('/');
    let owner = parts.next()?.trim();
    let repo = parts.next()?.trim().trim_end_matches(".git");
    if owner.is_empty() || repo.is_empty() || parts.next().is_some() {
        return None;
    }
    Some(format!("{}/{}", owner, repo))
}

fn repo_slug_from_git_remote(remote_url: impl AsRef<str>) -> Option<String> {
    let parsed = parse_git_remote(remote_url.as_ref())?;
    if parsed.owner.is_empty() || parsed.name.is_empty() {
        return None;
    }
    Some(format!("{}/{}", parsed.owner, parsed.name))
}

fn resolve_repo_slug(explicit_slug: Option<String>, remote_url: Option<String>) -> Option<String> {
    explicit_slug
        .and_then(normalize_repo_slug)
        .or_else(|| remote_url.and_then(repo_slug_from_git_remote))
}

fn read_git_remote_url_by_name(remote_name: &str) -> Option<String> {
    let output = Command::new("git")
        .args(["remote", "get-url", remote_name])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()
        .and_then(clean_config_value)
}

fn read_git_remote_url() -> Option<String> {
    if let Some(origin) = read_git_remote_url_by_name("origin") {
        return Some(origin);
    }

    let output = Command::new("git").arg("remote").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let remotes = String::from_utf8(output.stdout).ok()?;
    remotes.lines().find_map(|name| {
        clean_config_value(name).and_then(|name| read_git_remote_url_by_name(&name))
    })
}

fn get_team_memory_repo_slug() -> Option<String> {
    resolve_repo_slug(
        get_env_config_value(&["TEAM_MEMORY_REPO_SLUG", "MOSSEN_TEAM_MEMORY_REPO_SLUG"]),
        read_git_remote_url(),
    )
}

fn validate_absolute_memory_path(raw: &str) -> Option<PathBuf> {
    let path = PathBuf::from(raw);
    if !path.is_absolute() {
        return None;
    }
    let normalized = path.to_string_lossy();
    if normalized.len() < 3 || normalized.contains('\0') {
        return None;
    }
    Some(path)
}

fn sanitize_project_path_for_memory(path: &Path) -> String {
    path.to_string_lossy()
        .replace('/', "_")
        .replace('\\', "_")
        .replace(':', "_")
}

fn resolve_project_team_memory_dir(
    project_root: &Path,
    memory_path_override: Option<String>,
    remote_memory_dir: Option<String>,
    config_home_dir: PathBuf,
) -> PathBuf {
    if let Some(override_path) = memory_path_override
        .and_then(clean_config_value)
        .and_then(|path| validate_absolute_memory_path(&path))
    {
        return override_path.join("team");
    }

    let memory_base_dir = remote_memory_dir
        .and_then(clean_config_value)
        .map(PathBuf::from)
        .unwrap_or(config_home_dir);
    let sanitized = sanitize_project_path_for_memory(project_root);
    memory_base_dir
        .join("projects")
        .join(sanitized)
        .join("memory")
        .join("team")
}

fn current_project_root() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn resolve_team_memory_dir(explicit_dir: Option<String>, project_root: &Path) -> PathBuf {
    if let Some(explicit_dir) = explicit_dir.and_then(clean_config_value) {
        return PathBuf::from(explicit_dir);
    }

    resolve_project_team_memory_dir(
        project_root,
        get_env_config_value(&["MOSSEN_COWORK_MEMORY_PATH_OVERRIDE"]),
        get_env_config_value(&["MOSSEN_CODE_REMOTE_MEMORY_DIR"]),
        mossen_utils::env::get_mossen_config_home_dir(),
    )
}

pub(crate) fn get_team_memory_dir() -> PathBuf {
    resolve_team_memory_dir(
        get_env_config_value(&["TEAM_MEMORY_DIR", "MOSSEN_TEAM_MEMORY_DIR"]),
        &current_project_root(),
    )
}

fn existing_or_absolute_path(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(path)
        }
    })
}

fn is_path_inside_dir(file_path: &Path, dir: &Path) -> bool {
    existing_or_absolute_path(file_path).starts_with(existing_or_absolute_path(dir))
}

pub fn is_team_memory_file_path(file_path: impl AsRef<Path>) -> bool {
    is_path_inside_dir(file_path.as_ref(), &get_team_memory_dir())
}

fn get_team_memory_sync_endpoint(base_url: &str, repo_slug: &str) -> String {
    format!(
        "{}/api/mossen/team_memory?repo={}",
        base_url,
        urlencoding::encode(repo_slug)
    )
}

fn get_auth_headers(config: &SyncConfig) -> Result<HashMap<String, String>, String> {
    let token = config
        .access_token
        .as_ref()
        .ok_or_else(|| "No OAuth token available for team memory sync".to_string())?;

    let mut headers = HashMap::new();
    headers.insert("Authorization".to_string(), format!("Bearer {}", token));
    headers.insert("User-Agent".to_string(), config.user_agent.clone());
    Ok(headers)
}

// ─── Fetch (pull) ────────────────────────────────────────────

async fn fetch_team_memory_once(
    state: &mut SyncState,
    repo_slug: &str,
    etag: Option<&str>,
) -> TeamMemorySyncFetchResult {
    let config = get_sync_config();
    let headers = match get_auth_headers(&config) {
        Ok(h) => h,
        Err(e) => {
            return TeamMemorySyncFetchResult {
                success: false,
                error: Some(e),
                skip_retry: true,
                error_type: Some(SyncErrorType::Auth),
                ..Default::default()
            };
        }
    };

    let endpoint = get_team_memory_sync_endpoint(&config.base_api_url, repo_slug);
    let client = Client::builder()
        .timeout(Duration::from_millis(TEAM_MEMORY_SYNC_TIMEOUT_MS))
        .build()
        .unwrap_or_default();

    let mut req = client.get(&endpoint);
    for (k, v) in &headers {
        req = req.header(k.as_str(), v.as_str());
    }
    if let Some(etag_val) = etag {
        req = req.header(
            "If-None-Match",
            format!("\"{}\"", etag_val.replace('"', "")),
        );
    }

    let response = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            let error_type = if e.is_timeout() {
                SyncErrorType::Timeout
            } else if e.is_connect() {
                SyncErrorType::Network
            } else {
                SyncErrorType::Unknown
            };
            return TeamMemorySyncFetchResult {
                success: false,
                error: Some(e.to_string()),
                error_type: Some(error_type),
                ..Default::default()
            };
        }
    };

    let status = response.status().as_u16();

    if status == 304 {
        debug!("team-memory-sync: not modified (304)");
        return TeamMemorySyncFetchResult {
            success: true,
            not_modified: true,
            checksum: etag.map(String::from),
            ..Default::default()
        };
    }

    if status == 404 {
        debug!("team-memory-sync: no remote data (404)");
        state.last_known_checksum = None;
        return TeamMemorySyncFetchResult {
            success: true,
            is_empty: true,
            ..Default::default()
        };
    }

    if status != 200 {
        return TeamMemorySyncFetchResult {
            success: false,
            error: Some(format!("HTTP {}", status)),
            http_status: Some(status),
            error_type: Some(if status == 401 || status == 403 {
                SyncErrorType::Auth
            } else {
                SyncErrorType::Unknown
            }),
            ..Default::default()
        };
    }

    let body: serde_json::Value = match response.json().await {
        Ok(v) => v,
        Err(e) => {
            return TeamMemorySyncFetchResult {
                success: false,
                error: Some(format!("Invalid response: {}", e)),
                skip_retry: true,
                error_type: Some(SyncErrorType::Parse),
                ..Default::default()
            };
        }
    };

    let checksum = body
        .get("checksum")
        .and_then(|v| v.as_str())
        .map(String::from);
    if let Some(ref cs) = checksum {
        state.last_known_checksum = Some(cs.clone());
    }

    let data = match serde_json::from_value::<TeamMemoryData>(body) {
        Ok(d) => d,
        Err(e) => {
            return TeamMemorySyncFetchResult {
                success: false,
                error: Some(format!("Parse error: {}", e)),
                skip_retry: true,
                error_type: Some(SyncErrorType::Parse),
                ..Default::default()
            };
        }
    };

    TeamMemorySyncFetchResult {
        success: true,
        data: Some(data),
        is_empty: false,
        checksum,
        ..Default::default()
    }
}

/// Fetch only per-key checksums (no entry bodies).
async fn fetch_team_memory_hashes(
    state: &mut SyncState,
    repo_slug: &str,
) -> TeamMemoryHashesResult {
    let config = get_sync_config();
    let headers = match get_auth_headers(&config) {
        Ok(h) => h,
        Err(e) => {
            return TeamMemoryHashesResult {
                success: false,
                error: Some(e),
                error_type: Some(SyncErrorType::Auth),
                ..Default::default()
            };
        }
    };

    let endpoint = format!(
        "{}&view=hashes",
        get_team_memory_sync_endpoint(&config.base_api_url, repo_slug)
    );
    let client = Client::builder()
        .timeout(Duration::from_millis(TEAM_MEMORY_SYNC_TIMEOUT_MS))
        .build()
        .unwrap_or_default();

    let mut req = client.get(&endpoint);
    for (k, v) in &headers {
        req = req.header(k.as_str(), v.as_str());
    }

    let response = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            let error_type = if e.is_timeout() {
                SyncErrorType::Timeout
            } else {
                SyncErrorType::Network
            };
            return TeamMemoryHashesResult {
                success: false,
                error: Some(e.to_string()),
                error_type: Some(error_type),
                ..Default::default()
            };
        }
    };

    let status = response.status().as_u16();
    if status == 404 {
        state.last_known_checksum = None;
        return TeamMemoryHashesResult {
            success: true,
            entry_checksums: Some(HashMap::new()),
            ..Default::default()
        };
    }

    if status != 200 {
        return TeamMemoryHashesResult {
            success: false,
            error: Some(format!("HTTP {}", status)),
            http_status: Some(status),
            ..Default::default()
        };
    }

    let body: serde_json::Value = match response.json().await {
        Ok(v) => v,
        Err(e) => {
            return TeamMemoryHashesResult {
                success: false,
                error: Some(format!("Parse error: {}", e)),
                error_type: Some(SyncErrorType::Parse),
                ..Default::default()
            };
        }
    };

    let checksum = body
        .get("checksum")
        .and_then(|v| v.as_str())
        .map(String::from);
    if let Some(ref cs) = checksum {
        state.last_known_checksum = Some(cs.clone());
    }

    let entry_checksums = body
        .get("entryChecksums")
        .and_then(|v| v.as_object())
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect::<HashMap<String, String>>()
        });

    if entry_checksums.is_none() {
        return TeamMemoryHashesResult {
            success: false,
            error: Some("Server did not return entryChecksums".to_string()),
            error_type: Some(SyncErrorType::Parse),
            ..Default::default()
        };
    }

    TeamMemoryHashesResult {
        success: true,
        version: body.get("version").and_then(|v| v.as_u64()),
        checksum,
        entry_checksums,
        ..Default::default()
    }
}

async fn fetch_team_memory(
    state: &mut SyncState,
    repo_slug: &str,
    etag: Option<&str>,
) -> TeamMemorySyncFetchResult {
    let mut last_result = TeamMemorySyncFetchResult::default();

    for attempt in 1..=(MAX_RETRIES + 1) {
        last_result = fetch_team_memory_once(state, repo_slug, etag).await;
        if last_result.success || last_result.skip_retry {
            return last_result;
        }
        if attempt > MAX_RETRIES {
            return last_result;
        }
        let delay = Duration::from_millis(1000 * (1 << (attempt - 1)));
        debug!("team-memory-sync: retry {}/{}", attempt, MAX_RETRIES);
        tokio::time::sleep(delay).await;
    }

    last_result
}

// ─── Upload (push) ───────────────────────────────────────────

/// Split a delta into PUT-sized batches under MAX_PUT_BODY_BYTES each.
pub fn batch_delta_by_bytes(delta: &HashMap<String, String>) -> Vec<HashMap<String, String>> {
    let mut keys: Vec<&String> = delta.keys().collect();
    keys.sort();
    if keys.is_empty() {
        return Vec::new();
    }

    let empty_body_bytes = r#"{"entries":{}}"#.len();
    let entry_bytes = |k: &str, v: &str| -> usize {
        serde_json::to_string(k).unwrap_or_default().len()
            + serde_json::to_string(v).unwrap_or_default().len()
            + 2 // colon + comma
    };

    let mut batches = Vec::new();
    let mut current: HashMap<String, String> = HashMap::new();
    let mut current_bytes = empty_body_bytes;

    for key in keys {
        let value = &delta[key];
        let added = entry_bytes(key, value);
        if current_bytes + added > MAX_PUT_BODY_BYTES && !current.is_empty() {
            batches.push(current);
            current = HashMap::new();
            current_bytes = empty_body_bytes;
        }
        current.insert(key.clone(), value.clone());
        current_bytes += added;
    }
    batches.push(current);
    batches
}

async fn upload_team_memory(
    state: &mut SyncState,
    repo_slug: &str,
    entries: &HashMap<String, String>,
    if_match_checksum: Option<&str>,
) -> TeamMemorySyncUploadResult {
    let config = get_sync_config();
    let headers = match get_auth_headers(&config) {
        Ok(h) => h,
        Err(e) => {
            return TeamMemorySyncUploadResult {
                success: false,
                error: Some(e),
                error_type: Some(SyncErrorType::Auth),
                ..Default::default()
            };
        }
    };

    let endpoint = get_team_memory_sync_endpoint(&config.base_api_url, repo_slug);
    let client = Client::builder()
        .timeout(Duration::from_millis(TEAM_MEMORY_SYNC_TIMEOUT_MS))
        .build()
        .unwrap_or_default();

    let body = serde_json::json!({ "entries": entries });

    let mut req = client.put(&endpoint).json(&body);
    for (k, v) in &headers {
        req = req.header(k.as_str(), v.as_str());
    }
    req = req.header("Content-Type", "application/json");
    if let Some(cs) = if_match_checksum {
        req = req.header("If-Match", format!("\"{}\"", cs.replace('"', "")));
    }

    let response = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            let error_type = if e.is_timeout() {
                SyncErrorType::Timeout
            } else if e.is_connect() {
                SyncErrorType::Network
            } else {
                SyncErrorType::Unknown
            };
            return TeamMemorySyncUploadResult {
                success: false,
                error: Some(e.to_string()),
                error_type: Some(error_type),
                ..Default::default()
            };
        }
    };

    let status = response.status().as_u16();

    if status == 412 {
        info!("team-memory-sync: conflict (412 Precondition Failed)");
        return TeamMemorySyncUploadResult {
            success: false,
            conflict: true,
            error: Some("ETag mismatch".to_string()),
            ..Default::default()
        };
    }

    if status != 200 {
        // Check for structured 413
        let mut server_error_code = None;
        let mut server_max_entries = None;
        let mut server_received_entries = None;

        if status == 413 {
            if let Ok(body) = response.json::<serde_json::Value>().await {
                if let Some(details) = body.pointer("/error/details") {
                    server_error_code = details
                        .get("error_code")
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    server_max_entries = details.get("max_entries").and_then(|v| v.as_u64());
                    server_received_entries =
                        details.get("received_entries").and_then(|v| v.as_u64());
                }
            }
        }

        return TeamMemorySyncUploadResult {
            success: false,
            error: Some(format!("HTTP {}", status)),
            error_type: Some(SyncErrorType::Unknown),
            http_status: Some(status),
            server_error_code,
            server_max_entries,
            server_received_entries,
            ..Default::default()
        };
    }

    let resp_body: serde_json::Value = response.json().await.unwrap_or_default();
    let response_checksum = resp_body
        .get("checksum")
        .and_then(|v| v.as_str())
        .map(String::from);
    if let Some(ref cs) = response_checksum {
        state.last_known_checksum = Some(cs.clone());
    }

    debug!("team-memory-sync: uploaded {} entries", entries.len());

    TeamMemorySyncUploadResult {
        success: true,
        checksum: response_checksum,
        last_modified: resp_body
            .get("lastModified")
            .and_then(|v| v.as_str())
            .map(String::from),
        ..Default::default()
    }
}

// ─── Local file operations ───────────────────────────────────

async fn read_local_team_memory(
    team_dir: &Path,
    max_entries: Option<u64>,
) -> Result<(HashMap<String, String>, Vec<SkippedSecretFile>), String> {
    let mut entries = HashMap::new();
    let mut skipped_secrets = Vec::new();

    async fn walk_dir(
        dir: &Path,
        team_dir: &Path,
        entries: &mut HashMap<String, String>,
        skipped_secrets: &mut Vec<SkippedSecretFile>,
    ) -> Result<(), String> {
        let mut read_dir = match fs::read_dir(dir).await {
            Ok(rd) => rd,
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound
                    || e.kind() == std::io::ErrorKind::PermissionDenied
                {
                    return Ok(());
                }
                return Err(e.to_string());
            }
        };

        while let Ok(Some(entry)) = read_dir.next_entry().await {
            let path = entry.path();
            let file_type = match entry.file_type().await {
                Ok(ft) => ft,
                Err(_) => continue,
            };

            if file_type.is_dir() {
                Box::pin(walk_dir(&path, team_dir, entries, skipped_secrets)).await?;
            } else if file_type.is_file() {
                let metadata = match fs::metadata(&path).await {
                    Ok(m) => m,
                    Err(_) => continue,
                };

                if metadata.len() > MAX_FILE_SIZE_BYTES {
                    debug!(
                        "team-memory-sync: skipping oversized file {} ({} > {} bytes)",
                        path.display(),
                        metadata.len(),
                        MAX_FILE_SIZE_BYTES
                    );
                    continue;
                }

                let content = match fs::read_to_string(&path).await {
                    Ok(c) => c,
                    Err(_) => continue,
                };

                let rel_path = path
                    .strip_prefix(team_dir)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");

                // Scan for secrets before adding to upload payload
                let secret_matches = scan_for_secrets(&content);
                if !secret_matches.is_empty() {
                    let first = &secret_matches[0];
                    skipped_secrets.push(SkippedSecretFile {
                        path: rel_path,
                        rule_id: first.rule_id.clone(),
                        label: first.label.clone(),
                    });
                    continue;
                }

                entries.insert(rel_path, content);
            }
        }
        Ok(())
    }

    walk_dir(team_dir, team_dir, &mut entries, &mut skipped_secrets).await?;

    // Truncate if we've learned a server cap
    if let Some(max) = max_entries {
        let max = max as usize;
        let mut keys: Vec<String> = entries.keys().cloned().collect();
        keys.sort();
        if keys.len() > max {
            warn!(
                "team-memory-sync: {} local entries exceeds server cap of {}",
                keys.len(),
                max
            );
            let truncated: HashMap<String, String> = keys[..max]
                .iter()
                .map(|k| (k.clone(), entries[k].clone()))
                .collect();
            return Ok((truncated, skipped_secrets));
        }
    }

    Ok((entries, skipped_secrets))
}

async fn write_remote_entries_to_local(team_dir: &Path, entries: &HashMap<String, String>) -> u64 {
    let mut files_written = 0u64;

    for (rel_path, content) in entries {
        let full_path = team_dir.join(rel_path);

        if content.len() as u64 > MAX_FILE_SIZE_BYTES {
            debug!(
                "team-memory-sync: skipping oversized remote entry \"{}\"",
                rel_path
            );
            continue;
        }

        // Skip if on-disk content already matches
        if let Ok(existing) = fs::read_to_string(&full_path).await {
            if existing == *content {
                continue;
            }
        }

        // Write the file
        if let Some(parent) = full_path.parent() {
            if let Err(e) = fs::create_dir_all(parent).await {
                warn!(
                    "team-memory-sync: failed to create dir for \"{}\": {}",
                    rel_path, e
                );
                continue;
            }
        }

        match fs::write(&full_path, content).await {
            Ok(_) => files_written += 1,
            Err(e) => {
                warn!("team-memory-sync: failed to write \"{}\": {}", rel_path, e);
            }
        }
    }

    files_written
}

// ─── Public API ──────────────────────────────────────────────

/// Pull team memory from the server and write to local directory.
pub async fn pull_team_memory(state: &mut SyncState) -> PullResult {
    pull_team_memory_with_options(state, false).await
}

async fn pull_team_memory_with_options(state: &mut SyncState, skip_etag_cache: bool) -> PullResult {
    if !is_team_memory_sync_available() {
        return PullResult {
            success: false,
            files_written: 0,
            entry_count: 0,
            not_modified: false,
            error: Some("OAuth not available".to_string()),
        };
    }

    let repo_slug = match get_team_memory_repo_slug() {
        Some(repo_slug) => repo_slug,
        None => {
            return PullResult {
                success: false,
                files_written: 0,
                entry_count: 0,
                not_modified: false,
                error: Some("No git remote found".to_string()),
            };
        }
    };
    let etag = if skip_etag_cache {
        None
    } else {
        state.last_known_checksum.clone()
    };

    let result = fetch_team_memory(state, &repo_slug, etag.as_deref()).await;
    if !result.success {
        return PullResult {
            success: false,
            files_written: 0,
            entry_count: 0,
            not_modified: false,
            error: result.error,
        };
    }

    if result.not_modified {
        return PullResult {
            success: true,
            files_written: 0,
            entry_count: 0,
            not_modified: true,
            error: None,
        };
    }

    if result.is_empty || result.data.is_none() {
        state.server_checksums.clear();
        return PullResult {
            success: true,
            files_written: 0,
            entry_count: 0,
            not_modified: false,
            error: None,
        };
    }

    let data = result.data.unwrap();
    let entries = &data.content.entries;

    // Refresh server checksums
    state.server_checksums.clear();
    if let Some(ref checksums) = data.content.entry_checksums {
        for (key, hash) in checksums {
            state.server_checksums.insert(key.clone(), hash.clone());
        }
    }

    let team_dir = get_team_memory_dir();
    let files_written = write_remote_entries_to_local(&team_dir, entries).await;
    let entry_count = entries.len() as u64;

    info!("team-memory-sync: pulled {} files", files_written);

    PullResult {
        success: true,
        files_written,
        entry_count,
        not_modified: false,
        error: None,
    }
}

/// Push local team memory files to the server with optimistic locking.
pub async fn push_team_memory(state: &mut SyncState) -> TeamMemorySyncPushResult {
    if !is_team_memory_sync_available() {
        return TeamMemorySyncPushResult {
            success: false,
            files_uploaded: 0,
            error: Some("OAuth not available".to_string()),
            error_type: Some(SyncErrorType::NoOauth),
            ..Default::default()
        };
    }

    let repo_slug = match get_team_memory_repo_slug() {
        Some(repo_slug) => repo_slug,
        None => {
            return TeamMemorySyncPushResult {
                success: false,
                files_uploaded: 0,
                error: Some("No git remote found".to_string()),
                error_type: Some(SyncErrorType::NoRepo),
                ..Default::default()
            };
        }
    };
    let team_dir = get_team_memory_dir();
    let (entries, skipped_secrets) =
        match read_local_team_memory(&team_dir, state.server_max_entries).await {
            Ok(r) => r,
            Err(e) => {
                return TeamMemorySyncPushResult {
                    success: false,
                    files_uploaded: 0,
                    error: Some(e),
                    ..Default::default()
                };
            }
        };

    if !skipped_secrets.is_empty() {
        let summary: Vec<String> = skipped_secrets
            .iter()
            .map(|s| format!("\"{}\" ({})", s.path, s.label))
            .collect();
        warn!(
            "team-memory-sync: {} file(s) skipped due to detected secrets: {}",
            skipped_secrets.len(),
            summary.join(", ")
        );
    }

    // Hash each local entry
    let mut local_hashes = HashMap::new();
    for (key, content) in &entries {
        local_hashes.insert(key.clone(), hash_content(content));
    }

    for conflict_attempt in 0..=MAX_CONFLICT_RETRIES {
        // Compute delta
        let mut delta = HashMap::new();
        for (key, local_hash) in &local_hashes {
            if state.server_checksums.get(key) != Some(local_hash) {
                delta.insert(key.clone(), entries[key].clone());
            }
        }

        if delta.is_empty() {
            return TeamMemorySyncPushResult {
                success: true,
                files_uploaded: 0,
                skipped_secrets: if skipped_secrets.is_empty() {
                    Vec::new()
                } else {
                    skipped_secrets
                },
                ..Default::default()
            };
        }

        let batches = batch_delta_by_bytes(&delta);
        let mut files_uploaded = 0u64;
        let mut last_result: Option<TeamMemorySyncUploadResult> = None;

        for batch in &batches {
            let checksum_clone = state.last_known_checksum.clone();
            let result =
                upload_team_memory(state, &repo_slug, batch, checksum_clone.as_deref()).await;

            if !result.success {
                last_result = Some(result);
                break;
            }

            for key in batch.keys() {
                if let Some(hash) = local_hashes.get(key) {
                    state.server_checksums.insert(key.clone(), hash.clone());
                }
            }
            files_uploaded += batch.len() as u64;
            last_result = Some(result);
        }

        let result = last_result.unwrap();

        if result.success {
            info!("team-memory-sync: pushed {} files (delta)", files_uploaded);
            return TeamMemorySyncPushResult {
                success: true,
                files_uploaded,
                checksum: result.checksum,
                skipped_secrets: if skipped_secrets.is_empty() {
                    Vec::new()
                } else {
                    skipped_secrets
                },
                ..Default::default()
            };
        }

        if !result.conflict {
            if let Some(max) = result.server_max_entries {
                state.server_max_entries = Some(max);
                warn!(
                    "team-memory-sync: learned server max_entries={} from 413",
                    max
                );
            }
            return TeamMemorySyncPushResult {
                success: false,
                files_uploaded,
                error: result.error,
                error_type: result.error_type,
                http_status: result.http_status,
                ..Default::default()
            };
        }

        // 412 conflict — refresh server checksums and retry
        if conflict_attempt >= MAX_CONFLICT_RETRIES {
            warn!(
                "team-memory-sync: giving up after {} conflict retries",
                MAX_CONFLICT_RETRIES
            );
            return TeamMemorySyncPushResult {
                success: false,
                files_uploaded: 0,
                conflict: true,
                error: Some("Conflict resolution failed after retries".to_string()),
                ..Default::default()
            };
        }

        info!("team-memory-sync: conflict (412), probing server hashes");
        let probe = fetch_team_memory_hashes(state, &repo_slug).await;
        if !probe.success || probe.entry_checksums.is_none() {
            return TeamMemorySyncPushResult {
                success: false,
                files_uploaded: 0,
                conflict: true,
                error: Some(format!(
                    "Conflict resolution hashes probe failed: {:?}",
                    probe.error
                )),
                ..Default::default()
            };
        }

        state.server_checksums.clear();
        if let Some(checksums) = probe.entry_checksums {
            for (key, hash) in checksums {
                state.server_checksums.insert(key, hash);
            }
        }
    }

    TeamMemorySyncPushResult {
        success: false,
        files_uploaded: 0,
        error: Some("Unexpected end of conflict resolution loop".to_string()),
        ..Default::default()
    }
}

/// Bidirectional sync: pull from server, merge with local, push back.
pub async fn sync_team_memory(state: &mut SyncState) -> SyncResult {
    let pull_result = pull_team_memory_with_options(state, true).await;
    if !pull_result.success {
        return SyncResult {
            success: false,
            files_pulled: 0,
            files_pushed: 0,
            error: pull_result.error,
        };
    }

    let push_result = push_team_memory(state).await;
    if !push_result.success {
        return SyncResult {
            success: false,
            files_pulled: pull_result.files_written,
            files_pushed: 0,
            error: push_result.error,
        };
    }

    info!(
        "team-memory-sync: synced (pulled {}, pushed {})",
        pull_result.files_written, push_result.files_uploaded
    );

    SyncResult {
        success: true,
        files_pulled: pull_result.files_written,
        files_pushed: push_result.files_uploaded,
        error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_access_token_prefers_explicit_token() {
        assert_eq!(
            resolve_access_token(Some(" explicit ".to_string()), Some("oauth".to_string()))
                .as_deref(),
            Some("explicit")
        );
        assert_eq!(
            resolve_access_token(Some("   ".to_string()), Some(" oauth ".to_string())).as_deref(),
            Some("oauth")
        );
        assert_eq!(resolve_access_token(None, Some("   ".to_string())), None);
    }

    #[test]
    fn resolve_base_api_url_uses_clean_override_or_default() {
        assert_eq!(
            resolve_base_api_url(Some(" https://api.example.test/ ".to_string())),
            "https://api.example.test"
        );
        assert_eq!(
            resolve_base_api_url(Some(" / ".to_string())),
            "https://api.mossen.ai"
        );
        assert_eq!(resolve_base_api_url(None), "https://api.mossen.ai");
    }

    #[test]
    fn normalize_repo_slug_accepts_explicit_slug_or_url() {
        assert_eq!(
            normalize_repo_slug(" owner/repo ").as_deref(),
            Some("owner/repo")
        );
        assert_eq!(
            normalize_repo_slug("github.com/owner/repo.git").as_deref(),
            Some("owner/repo")
        );
        assert_eq!(
            normalize_repo_slug("https://github.com/owner/repo.git").as_deref(),
            Some("owner/repo")
        );
        assert_eq!(normalize_repo_slug("owner/repo/extra"), None);
    }

    #[test]
    fn repo_slug_from_git_remote_supports_common_remote_forms() {
        let cases = [
            ("https://github.com/owner/repo.git", "owner/repo"),
            ("git@github.com:owner/repo.git", "owner/repo"),
            ("ssh://git@github.com/owner/repo.git", "owner/repo"),
            (
                "https://gitlab.example.com/team/project.git",
                "team/project",
            ),
        ];

        for (remote, expected) in cases {
            assert_eq!(repo_slug_from_git_remote(remote).as_deref(), Some(expected));
        }
    }

    #[test]
    fn resolve_repo_slug_prefers_explicit_slug_then_remote() {
        assert_eq!(
            resolve_repo_slug(
                Some("manual/repo".to_string()),
                Some("https://github.com/remote/repo.git".to_string()),
            )
            .as_deref(),
            Some("manual/repo")
        );
        assert_eq!(
            resolve_repo_slug(None, Some("git@github.com:remote/repo.git".to_string())).as_deref(),
            Some("remote/repo")
        );
        assert_eq!(resolve_repo_slug(Some("invalid".to_string()), None), None);
    }

    #[test]
    fn resolve_team_memory_dir_uses_override_or_default() {
        assert_eq!(
            resolve_team_memory_dir(
                Some(" /tmp/team-memory ".to_string()),
                Path::new("/workspace/project")
            ),
            PathBuf::from("/tmp/team-memory")
        );
    }

    #[test]
    fn resolve_project_team_memory_dir_matches_cli_prompt_path_shape() {
        assert_eq!(
            resolve_project_team_memory_dir(
                Path::new("/Users/allen/project:one"),
                None,
                None,
                PathBuf::from("/tmp/mossen-home"),
            ),
            PathBuf::from("/tmp/mossen-home")
                .join("projects")
                .join("_Users_allen_project_one")
                .join("memory")
                .join("team")
        );
    }

    #[test]
    fn resolve_project_team_memory_dir_honors_auto_memory_override_root() {
        assert_eq!(
            resolve_project_team_memory_dir(
                Path::new("/workspace/project"),
                Some(" /tmp/auto-memory ".to_string()),
                None,
                PathBuf::from("/tmp/mossen-home"),
            ),
            PathBuf::from("/tmp/auto-memory").join("team")
        );
        assert_eq!(
            resolve_project_team_memory_dir(
                Path::new("/workspace/project"),
                Some(" relative/path ".to_string()),
                None,
                PathBuf::from("/tmp/mossen-home"),
            ),
            PathBuf::from("/tmp/mossen-home")
                .join("projects")
                .join("_workspace_project")
                .join("memory")
                .join("team")
        );
    }

    #[test]
    fn resolve_project_team_memory_dir_honors_remote_memory_base_dir() {
        assert_eq!(
            resolve_project_team_memory_dir(
                Path::new("/workspace/project"),
                None,
                Some(" /remote/memory ".to_string()),
                PathBuf::from("/tmp/mossen-home"),
            ),
            PathBuf::from("/remote/memory")
                .join("projects")
                .join("_workspace_project")
                .join("memory")
                .join("team")
        );
    }

    #[test]
    fn is_path_inside_dir_uses_path_component_boundaries() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let team_dir = tmp.path().join("team-memory");
        let team_file = team_dir.join("notes").join("memory.md");
        let sibling = tmp.path().join("team-memory-other").join("memory.md");
        std::fs::create_dir_all(team_file.parent().expect("team parent")).expect("team mkdir");
        std::fs::create_dir_all(sibling.parent().expect("sibling parent")).expect("sibling mkdir");
        std::fs::write(&team_file, "team").expect("team write");
        std::fs::write(&sibling, "other").expect("sibling write");

        assert!(is_path_inside_dir(&team_file, &team_dir));
        assert!(!is_path_inside_dir(&sibling, &team_dir));
    }
}
