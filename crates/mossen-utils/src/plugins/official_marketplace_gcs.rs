use std::collections::HashSet;
use std::path::Path;
use std::time::Instant;

use tokio::fs;
use tracing::debug;

/// CDN-fronted domain for the public GCS bucket.
const GCS_BASE: &str =
    "https://downloads.mossen.invalid/cli-releases/plugins/mossen-plugins-official";

/// Zip arc paths are seed-dir-relative; strip this prefix when extracting.
const ARC_PREFIX: &str = "marketplaces/mossen-plugins-official/";

/// Known FS error codes for telemetry classification.
static KNOWN_FS_CODES: once_cell::sync::Lazy<HashSet<&'static str>> =
    once_cell::sync::Lazy::new(|| {
        [
            "ENOSPC",
            "EACCES",
            "EPERM",
            "EXDEV",
            "EBUSY",
            "ENOENT",
            "ENOTDIR",
            "EROFS",
            "EMFILE",
            "ENAMETOOLONG",
        ]
        .into_iter()
        .collect()
    });

/// Telemetry event data for GCS fetch.
#[derive(Debug, Clone)]
pub struct GcsFetchTelemetry {
    pub source: String,
    pub host: String,
    pub is_official: bool,
    pub outcome: String,
    pub duration_ms: u64,
    pub bytes: Option<usize>,
    pub sha: Option<String>,
    pub error_kind: Option<String>,
}

/// Trait for analytics event logging.
pub trait AnalyticsLogger: Send + Sync {
    fn log_event(&self, event_name: &str, telemetry: GcsFetchTelemetry);
}

/// Trait for HTTP client operations.
#[async_trait::async_trait]
pub trait HttpClient: Send + Sync {
    async fn get_text(&self, url: &str, timeout_ms: u64) -> Result<String, HttpError>;
    async fn get_bytes(&self, url: &str, timeout_ms: u64) -> Result<Vec<u8>, HttpError>;
}

#[derive(Debug)]
pub enum HttpError {
    Timeout,
    Network(String),
    Status(u16),
    Other(String),
}

/// Trait for zip extraction.
pub trait ZipExtractor: Send + Sync {
    /// Unzip a buffer into (path, data) pairs.
    fn unzip_file(&self, data: &[u8]) -> Result<Vec<(String, Vec<u8>)>, anyhow::Error>;
    /// Parse zip modes from the central directory.
    fn parse_zip_modes(&self, data: &[u8]) -> std::collections::HashMap<String, u32>;
}

/// Fetch the official marketplace from GCS and extract to install_location.
/// Idempotent — checks a `.gcs-sha` sentinel before downloading.
///
/// Returns the fetched SHA on success (including no-op), None on any failure.
pub async fn fetch_official_marketplace_from_gcs(
    install_location: &Path,
    marketplaces_cache_dir: &Path,
    http_client: &dyn HttpClient,
    zip_extractor: &dyn ZipExtractor,
    analytics: Option<&dyn AnalyticsLogger>,
) -> Option<String> {
    // Defense in depth: refuse any path outside the marketplaces cache dir.
    let cache_dir = marketplaces_cache_dir
        .canonicalize()
        .unwrap_or_else(|_| marketplaces_cache_dir.to_path_buf());
    let resolved_loc = install_location
        .canonicalize()
        .unwrap_or_else(|_| install_location.to_path_buf());
    if resolved_loc != cache_dir && !resolved_loc.starts_with(&cache_dir) {
        debug!(
            "fetchOfficialMarketplaceFromGcs: refusing path outside cache dir: {:?}",
            install_location
        );
        return None;
    }

    let start = Instant::now();
    let mut outcome = "failed";
    let mut sha: Option<String> = None;
    let mut bytes: Option<usize> = None;
    let mut err_kind: Option<String> = None;

    let result = fetch_inner(
        install_location,
        http_client,
        zip_extractor,
        &mut sha,
        &mut bytes,
    )
    .await;

    match result {
        Ok(FetchOutcome::Noop) => {
            outcome = "noop";
        }
        Ok(FetchOutcome::Updated) => {
            outcome = "updated";
        }
        Err(e) => {
            err_kind = Some(classify_gcs_error(&e));
            debug!("Official marketplace GCS fetch failed: {}", e);
        }
    }

    if let Some(logger) = analytics {
        logger.log_event(
            "mossen_plugin_remote_fetch",
            GcsFetchTelemetry {
                source: "marketplace_gcs".to_string(),
                host: "downloads.mossen.invalid".to_string(),
                is_official: true,
                outcome: outcome.to_string(),
                duration_ms: start.elapsed().as_millis() as u64,
                bytes,
                sha: sha.clone(),
                error_kind: err_kind,
            },
        );
    }

    if outcome == "failed" {
        None
    } else {
        sha
    }
}

enum FetchOutcome {
    Noop,
    Updated,
}

async fn fetch_inner(
    install_location: &Path,
    http_client: &dyn HttpClient,
    zip_extractor: &dyn ZipExtractor,
    sha_out: &mut Option<String>,
    bytes_out: &mut Option<usize>,
) -> Result<FetchOutcome, anyhow::Error> {
    // 1. Fetch latest pointer
    let latest_url = format!("{}/latest", GCS_BASE);
    let latest_text = http_client
        .get_text(&latest_url, 10_000)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to fetch latest: {:?}", e))?;
    let fetched_sha = latest_text.trim().to_string();
    if fetched_sha.is_empty() {
        return Err(anyhow::anyhow!("latest pointer returned empty body"));
    }
    *sha_out = Some(fetched_sha.clone());

    // 2. Sentinel check
    let sentinel_path = install_location.join(".gcs-sha");
    let current_sha = fs::read_to_string(&sentinel_path)
        .await
        .ok()
        .map(|s| s.trim().to_string());
    if current_sha.as_deref() == Some(&fetched_sha) {
        return Ok(FetchOutcome::Noop);
    }

    // 3. Download zip and extract
    let zip_url = format!("{}/{}.zip", GCS_BASE, fetched_sha);
    let zip_buf = http_client
        .get_bytes(&zip_url, 60_000)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to download zip: {:?}", e))?;
    *bytes_out = Some(zip_buf.len());

    let files = zip_extractor.unzip_file(&zip_buf)?;
    let modes = zip_extractor.parse_zip_modes(&zip_buf);

    let staging = install_location.with_extension("staging");
    let _ = fs::remove_dir_all(&staging).await;
    fs::create_dir_all(&staging).await?;

    for (arc_path, data) in &files {
        if !arc_path.starts_with(ARC_PREFIX) {
            continue;
        }
        let rel = &arc_path[ARC_PREFIX.len()..];
        if rel.is_empty() || rel.ends_with('/') {
            continue;
        }
        let dest = staging.join(rel);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).await?;
        }
        fs::write(&dest, data).await?;

        // Set exec bits if present
        #[cfg(unix)]
        if let Some(&mode) = modes.get(arc_path.as_str()) {
            if mode & 0o111 != 0 {
                use std::os::unix::fs::PermissionsExt;
                let _ =
                    fs::set_permissions(&dest, std::fs::Permissions::from_mode(mode & 0o777)).await;
            }
        }
    }

    fs::write(staging.join(".gcs-sha"), &fetched_sha).await?;

    // Atomic swap
    let _ = fs::remove_dir_all(install_location).await;
    fs::rename(&staging, install_location).await?;

    Ok(FetchOutcome::Updated)
}

/// Classify a GCS fetch error into a stable telemetry bucket.
pub fn classify_gcs_error(e: &anyhow::Error) -> String {
    let msg = e.to_string();

    if msg.contains("timeout") || msg.contains("ECONNABORTED") {
        return "timeout".to_string();
    }
    if msg.contains("http_") {
        // Extract status code pattern
        if let Some(idx) = msg.find("http_") {
            let rest = &msg[idx + 5..];
            let code: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if !code.is_empty() {
                return format!("http_{}", code);
            }
        }
        return "network".to_string();
    }
    if msg.contains("network") || msg.contains("Network") {
        return "network".to_string();
    }

    // Check for fs errno codes
    for code in KNOWN_FS_CODES.iter() {
        if msg.contains(code) {
            return format!("fs_{}", code);
        }
    }

    if msg.contains("unzip") || msg.contains("invalid zip") || msg.contains("central directory") {
        return "zip_parse".to_string();
    }
    if msg.contains("empty body") {
        return "empty_latest".to_string();
    }
    "other".to_string()
}
