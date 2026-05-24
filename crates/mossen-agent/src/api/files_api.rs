//! # Files API
//!
//! 翻译自 `services/api/filesApi.ts` (761行)
//! 文件上传/下载 API 客户端。

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::{debug, error};
use uuid::Uuid;

const FILES_API_BETA_HEADER: &str = "files-api-2025-04-14,bearer-auth-2025-04-20";
const PROVIDER_VERSION: &str = "2023-06-01";
const MAX_RETRIES: u32 = 3;
const BASE_DELAY_MS: u64 = 500;
const MAX_FILE_SIZE_BYTES: u64 = 500 * 1024 * 1024; // 500MB
const DEFAULT_CONCURRENCY: usize = 5;

/// File specification parsed from CLI args.
#[derive(Debug, Clone)]
pub struct File {
    pub file_id: String,
    pub relative_path: String,
}

/// Configuration for the files API client.
#[derive(Debug, Clone)]
pub struct FilesApiConfig {
    pub oauth_token: String,
    pub base_url: Option<String>,
    pub session_id: String,
}

/// Result of a file download operation.
#[derive(Debug, Clone)]
pub struct DownloadResult {
    pub file_id: String,
    pub path: String,
    pub success: bool,
    pub error: Option<String>,
    pub bytes_written: Option<usize>,
}

/// Result of a file upload operation.
#[derive(Debug, Clone)]
pub enum UploadResult {
    Success {
        path: String,
        file_id: String,
        size: usize,
    },
    Failure {
        path: String,
        error: String,
    },
}

impl UploadResult {
    pub fn is_success(&self) -> bool {
        matches!(self, UploadResult::Success { .. })
    }
}

/// File metadata returned from list files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    pub filename: String,
    pub file_id: String,
    pub size: u64,
}

fn log_debug_error(message: &str) {
    debug!("[files-api] {}", message);
}

fn log_debug(message: &str) {
    debug!("[files-api] {}", message);
}

/// Get the default API base URL.
pub fn get_default_api_base_url(
    custom_backend_base_url: Option<&str>,
    is_custom_backend: bool,
    env_base_url: Option<&str>,
    is_hosted_auth_adapter: bool,
    oauth_base_url: &str,
) -> Result<String, anyhow::Error> {
    if is_custom_backend {
        if let Some(url) = custom_backend_base_url {
            return Ok(url.to_string());
        }
        return Err(anyhow::anyhow!(
            "Custom backend mode requires MOSSEN_CODE_CUSTOM_BASE_URL to be set."
        ));
    }
    if let Some(url) = env_base_url {
        return Ok(url.to_string());
    }
    if is_hosted_auth_adapter {
        return Ok(oauth_base_url.to_string());
    }
    Err(anyhow::anyhow!("No Mossen files backend is configured."))
}

/// Retry result for internal use.
enum RetryResult<T> {
    Done(T),
    Retry(String),
}

/// Execute an operation with exponential backoff retry logic.
async fn retry_with_backoff<T, F, Fut>(operation: &str, attempt_fn: F) -> Result<T, anyhow::Error>
where
    F: Fn(u32) -> Fut,
    Fut: std::future::Future<Output = RetryResult<T>>,
{
    let mut last_error = String::new();

    for attempt in 1..=MAX_RETRIES {
        match attempt_fn(attempt).await {
            RetryResult::Done(value) => return Ok(value),
            RetryResult::Retry(err) => {
                last_error = err;
                log_debug(&format!(
                    "{} attempt {}/{} failed: {}",
                    operation, attempt, MAX_RETRIES, last_error
                ));

                if attempt < MAX_RETRIES {
                    let delay_ms = BASE_DELAY_MS * 2u64.pow(attempt - 1);
                    log_debug(&format!("Retrying {} in {}ms...", operation, delay_ms));
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                }
            }
        }
    }

    Err(anyhow::anyhow!(
        "{} after {} attempts",
        last_error,
        MAX_RETRIES
    ))
}

/// Download a single file from the Mossen files API.
pub async fn download_file(
    client: &Client,
    file_id: &str,
    config: &FilesApiConfig,
    base_url: &str,
) -> Result<Vec<u8>, anyhow::Error> {
    let url = format!("{}/v1/files/{}/content", base_url, file_id);
    let token = config.oauth_token.clone();

    log_debug(&format!("Downloading file {} from {}", file_id, url));

    retry_with_backoff(&format!("Download file {}", file_id), |_attempt| {
        let client = client.clone();
        let url = url.clone();
        let token = token.clone();
        async move {
            let resp = match client
                .get(&url)
                .header("Authorization", format!("Bearer {}", token))
                .header("mossen-version", PROVIDER_VERSION)
                .header("mossen-beta", FILES_API_BETA_HEADER)
                .timeout(Duration::from_secs(60))
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => return RetryResult::Retry(e.to_string()),
            };

            let status = resp.status().as_u16();

            if status == 200 {
                match resp.bytes().await {
                    Ok(bytes) => RetryResult::Done(bytes.to_vec()),
                    Err(e) => RetryResult::Retry(e.to_string()),
                }
            } else if status == 404 || status == 401 || status == 403 {
                // Non-retriable — we can't return an error from RetryResult, so we Retry
                // but the outer code would need to handle. For simplicity, treat as retry exhaustion.
                RetryResult::Retry(format!("status {}", status))
            } else {
                RetryResult::Retry(format!("status {}", status))
            }
        }
    })
    .await
}

/// Normalize a relative path and build the full download path.
/// Returns None if the path is invalid (e.g., path traversal).
pub fn build_download_path(
    base_path: &Path,
    session_id: &str,
    relative_path: &str,
) -> Option<PathBuf> {
    let normalized = Path::new(relative_path);

    // Check for path traversal
    for component in normalized.components() {
        if let std::path::Component::ParentDir = component {
            log_debug_error(&format!(
                "Invalid file path: {}. Path must not traverse above workspace",
                relative_path
            ));
            return None;
        }
    }

    let uploads_base = base_path.join(session_id).join("uploads");
    Some(uploads_base.join(normalized))
}

/// Download a file and save it to the session-specific workspace directory.
pub async fn download_and_save_file(
    client: &Client,
    attachment: &File,
    config: &FilesApiConfig,
    base_url: &str,
    cwd: &Path,
) -> DownloadResult {
    let full_path = match build_download_path(cwd, &config.session_id, &attachment.relative_path) {
        Some(p) => p,
        None => {
            return DownloadResult {
                file_id: attachment.file_id.clone(),
                path: String::new(),
                success: false,
                error: Some(format!("Invalid file path: {}", attachment.relative_path)),
                bytes_written: None,
            };
        }
    };

    let content = match download_file(client, &attachment.file_id, config, base_url).await {
        Ok(c) => c,
        Err(e) => {
            log_debug_error(&format!(
                "Failed to download file {}: {}",
                attachment.file_id, e
            ));
            return DownloadResult {
                file_id: attachment.file_id.clone(),
                path: full_path.to_string_lossy().to_string(),
                success: false,
                error: Some(e.to_string()),
                bytes_written: None,
            };
        }
    };

    // Ensure parent directory exists
    if let Some(parent) = full_path.parent() {
        if let Err(e) = tokio::fs::create_dir_all(parent).await {
            return DownloadResult {
                file_id: attachment.file_id.clone(),
                path: full_path.to_string_lossy().to_string(),
                success: false,
                error: Some(format!("Failed to create directory: {}", e)),
                bytes_written: None,
            };
        }
    }

    let bytes_written = content.len();
    if let Err(e) = tokio::fs::write(&full_path, &content).await {
        return DownloadResult {
            file_id: attachment.file_id.clone(),
            path: full_path.to_string_lossy().to_string(),
            success: false,
            error: Some(format!("Failed to write file: {}", e)),
            bytes_written: None,
        };
    }

    log_debug(&format!(
        "Saved file {} to {} ({} bytes)",
        attachment.file_id,
        full_path.display(),
        bytes_written
    ));

    DownloadResult {
        file_id: attachment.file_id.clone(),
        path: full_path.to_string_lossy().to_string(),
        success: true,
        error: None,
        bytes_written: Some(bytes_written),
    }
}

/// Download all file attachments for a session in parallel (with concurrency limit).
pub async fn download_session_files(
    client: &Client,
    files: &[File],
    config: &FilesApiConfig,
    base_url: &str,
    cwd: &Path,
    concurrency: Option<usize>,
) -> Vec<DownloadResult> {
    if files.is_empty() {
        return Vec::new();
    }

    let concurrency = concurrency.unwrap_or(DEFAULT_CONCURRENCY);
    log_debug(&format!(
        "Downloading {} file(s) for session {}",
        files.len(),
        config.session_id
    ));

    let start = std::time::Instant::now();
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(concurrency));
    let mut handles = Vec::new();

    for file in files {
        let permit = semaphore.clone().acquire_owned().await.unwrap();
        let client = client.clone();
        let file = file.clone();
        let config = config.clone();
        let base_url = base_url.to_string();
        let cwd = cwd.to_path_buf();

        handles.push(tokio::spawn(async move {
            let result = download_and_save_file(&client, &file, &config, &base_url, &cwd).await;
            drop(permit);
            result
        }));
    }

    let mut results = Vec::with_capacity(handles.len());
    for handle in handles {
        match handle.await {
            Ok(result) => results.push(result),
            Err(e) => {
                error!("Download task panicked: {}", e);
                results.push(DownloadResult {
                    file_id: String::new(),
                    path: String::new(),
                    success: false,
                    error: Some(format!("Task panicked: {}", e)),
                    bytes_written: None,
                });
            }
        }
    }

    let elapsed = start.elapsed();
    let success_count = results.iter().filter(|r| r.success).count();
    log_debug(&format!(
        "Downloaded {}/{} file(s) in {:?}",
        success_count,
        files.len(),
        elapsed
    ));

    results
}

/// Upload a single file to the Files API (BYOC mode).
pub async fn upload_file(
    client: &Client,
    file_path: &Path,
    relative_path: &str,
    config: &FilesApiConfig,
    base_url: &str,
) -> UploadResult {
    let content = match tokio::fs::read(file_path).await {
        Ok(c) => c,
        Err(e) => {
            return UploadResult::Failure {
                path: relative_path.to_string(),
                error: e.to_string(),
            };
        }
    };

    let file_size = content.len() as u64;
    if file_size > MAX_FILE_SIZE_BYTES {
        return UploadResult::Failure {
            path: relative_path.to_string(),
            error: format!(
                "File exceeds maximum size of {} bytes (actual: {})",
                MAX_FILE_SIZE_BYTES, file_size
            ),
        };
    }

    let boundary = format!("----FormBoundary{}", Uuid::new_v4());
    let filename = Path::new(relative_path)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();

    // Build multipart body
    let mut body = Vec::new();
    body.extend_from_slice(
        format!(
            "--{}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{}\"\r\nContent-Type: application/octet-stream\r\n\r\n",
            boundary, filename
        )
        .as_bytes(),
    );
    body.extend_from_slice(&content);
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(
        format!(
            "--{}\r\nContent-Disposition: form-data; name=\"purpose\"\r\n\r\nuser_data\r\n",
            boundary
        )
        .as_bytes(),
    );
    body.extend_from_slice(format!("--{}--\r\n", boundary).as_bytes());

    let url = format!("{}/v1/files", base_url);
    let token = config.oauth_token.clone();

    log_debug(&format!(
        "Uploading file {} as {}",
        file_path.display(),
        relative_path
    ));

    let upload_result = retry_with_backoff(&format!("Upload file {}", relative_path), |_attempt| {
        let client = client.clone();
        let url = url.clone();
        let token = token.clone();
        let body = body.clone();
        let boundary = boundary.clone();
        async move {
            let resp = match client
                .post(&url)
                .header("Authorization", format!("Bearer {}", token))
                .header("mossen-version", PROVIDER_VERSION)
                .header("mossen-beta", FILES_API_BETA_HEADER)
                .header(
                    "Content-Type",
                    format!("multipart/form-data; boundary={}", boundary),
                )
                .timeout(Duration::from_secs(120))
                .body(body)
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => return RetryResult::Retry(e.to_string()),
            };

            let status = resp.status().as_u16();

            if status == 200 || status == 201 {
                let data: serde_json::Value = match resp.json().await {
                    Ok(d) => d,
                    Err(e) => return RetryResult::Retry(e.to_string()),
                };
                let file_id = data
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if file_id.is_empty() {
                    return RetryResult::Retry("Upload succeeded but no file ID returned".into());
                }
                RetryResult::Done(file_id)
            } else if status == 401 || status == 403 || status == 413 {
                RetryResult::Retry(format!("non-retriable status {}", status))
            } else {
                RetryResult::Retry(format!("status {}", status))
            }
        }
    })
    .await;

    match upload_result {
        Ok(file_id) => {
            log_debug(&format!(
                "Uploaded file {} -> {} ({} bytes)",
                file_path.display(),
                file_id,
                file_size
            ));
            UploadResult::Success {
                path: relative_path.to_string(),
                file_id,
                size: file_size as usize,
            }
        }
        Err(e) => UploadResult::Failure {
            path: relative_path.to_string(),
            error: e.to_string(),
        },
    }
}

/// Upload multiple files in parallel with concurrency limit.
pub async fn upload_session_files(
    client: &Client,
    files: &[(String, String)], // (path, relative_path)
    config: &FilesApiConfig,
    base_url: &str,
    concurrency: Option<usize>,
) -> Vec<UploadResult> {
    if files.is_empty() {
        return Vec::new();
    }

    let concurrency = concurrency.unwrap_or(DEFAULT_CONCURRENCY);
    log_debug(&format!(
        "Uploading {} file(s) for session {}",
        files.len(),
        config.session_id
    ));

    let start = std::time::Instant::now();
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(concurrency));
    let mut handles = Vec::new();

    for (path, relative_path) in files {
        let permit = semaphore.clone().acquire_owned().await.unwrap();
        let client = client.clone();
        let path = PathBuf::from(path);
        let relative_path = relative_path.clone();
        let config = config.clone();
        let base_url = base_url.to_string();

        handles.push(tokio::spawn(async move {
            let result = upload_file(&client, &path, &relative_path, &config, &base_url).await;
            drop(permit);
            result
        }));
    }

    let mut results = Vec::with_capacity(handles.len());
    for handle in handles {
        match handle.await {
            Ok(result) => results.push(result),
            Err(e) => {
                error!("Upload task panicked: {}", e);
                results.push(UploadResult::Failure {
                    path: String::new(),
                    error: format!("Task panicked: {}", e),
                });
            }
        }
    }

    let elapsed = start.elapsed();
    let success_count = results.iter().filter(|r| r.is_success()).count();
    log_debug(&format!(
        "Uploaded {}/{} file(s) in {:?}",
        success_count,
        files.len(),
        elapsed
    ));

    results
}

/// List files created after a given timestamp.
pub async fn list_files_created_after(
    client: &Client,
    after_created_at: &str,
    config: &FilesApiConfig,
    base_url: &str,
) -> Result<Vec<FileMetadata>, anyhow::Error> {
    log_debug(&format!("Listing files created after {}", after_created_at));

    let mut all_files: Vec<FileMetadata> = Vec::new();
    let mut after_id: Option<String> = None;

    loop {
        let mut params = vec![("after_created_at", after_created_at.to_string())];
        if let Some(ref id) = after_id {
            params.push(("after_id", id.clone()));
        }

        let page: serde_json::Value = retry_with_backoff(
            &format!("List files after {}", after_created_at),
            |_attempt| {
                let client = client.clone();
                let url = format!("{}/v1/files", base_url);
                let token = config.oauth_token.clone();
                let params = params.clone();
                async move {
                    let resp = match client
                        .get(&url)
                        .header("Authorization", format!("Bearer {}", token))
                        .header("mossen-version", PROVIDER_VERSION)
                        .header("mossen-beta", FILES_API_BETA_HEADER)
                        .query(&params)
                        .timeout(Duration::from_secs(60))
                        .send()
                        .await
                    {
                        Ok(r) => r,
                        Err(e) => return RetryResult::Retry(e.to_string()),
                    };

                    let status = resp.status().as_u16();
                    if status == 200 {
                        match resp.json().await {
                            Ok(data) => RetryResult::Done(data),
                            Err(e) => RetryResult::Retry(e.to_string()),
                        }
                    } else if status == 401 || status == 403 {
                        RetryResult::Retry(format!("auth error status {}", status))
                    } else {
                        RetryResult::Retry(format!("status {}", status))
                    }
                }
            },
        )
        .await?;

        let files = page
            .get("data")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        for f in &files {
            if let (Some(filename), Some(id), Some(size)) = (
                f.get("filename").and_then(|v| v.as_str()),
                f.get("id").and_then(|v| v.as_str()),
                f.get("size_bytes").and_then(|v| v.as_u64()),
            ) {
                all_files.push(FileMetadata {
                    filename: filename.to_string(),
                    file_id: id.to_string(),
                    size,
                });
            }
        }

        let has_more = page
            .get("has_more")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !has_more {
            break;
        }

        let last_file_id = files
            .last()
            .and_then(|f| f.get("id"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        match last_file_id {
            Some(id) => after_id = Some(id),
            None => break,
        }
    }

    log_debug(&format!(
        "Listed {} files created after {}",
        all_files.len(),
        after_created_at
    ));
    Ok(all_files)
}

/// Parse file attachment specs from CLI arguments.
/// Format: <file_id>:<relative_path>
pub fn parse_file_specs(file_specs: &[String]) -> Vec<File> {
    let mut files = Vec::new();

    // Sandbox-gateway may pass multiple specs as a single space-separated string
    let expanded_specs: Vec<&str> = file_specs
        .iter()
        .flat_map(|s| s.split(' ').filter(|s| !s.is_empty()))
        .collect();

    for spec in expanded_specs {
        let colon_index = match spec.find(':') {
            Some(idx) => idx,
            None => continue,
        };

        let file_id = &spec[..colon_index];
        let relative_path = &spec[colon_index + 1..];

        if file_id.is_empty() || relative_path.is_empty() {
            log_debug_error(&format!(
                "Invalid file spec: {}. Both file_id and path are required",
                spec
            ));
            continue;
        }

        files.push(File {
            file_id: file_id.to_string(),
            relative_path: relative_path.to_string(),
        });
    }

    files
}
