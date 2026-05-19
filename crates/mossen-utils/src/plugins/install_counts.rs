//! Plugin install counts data layer.
//!
//! Translated from `utils/plugins/installCounts.ts` (292 lines).

use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::fs;
use tracing::debug;

use super::fetch_telemetry::{classify_fetch_error, log_plugin_fetch, PluginFetchOutcome, PluginFetchSource};
use super::plugin_directories::get_plugins_directory;

const INSTALL_COUNTS_CACHE_VERSION: u32 = 1;
const INSTALL_COUNTS_CACHE_FILENAME: &str = "install-counts-cache.json";
const INSTALL_COUNTS_URL: &str =
    "https://raw.githubusercontent.com/mossen/mossen-plugins-official/refs/heads/stats/stats/plugin-installs.json";
const CACHE_TTL_MS: u64 = 24 * 60 * 60 * 1000; // 24 hours

/// Structure of the install counts cache file.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct InstallCountsCache {
    version: u32,
    #[serde(rename = "fetchedAt")]
    fetched_at: String,
    counts: Vec<PluginInstallCount>,
}

/// Individual plugin install count entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PluginInstallCount {
    plugin: String,
    unique_installs: u64,
}

/// GitHub stats response structure.
#[derive(Debug, Deserialize)]
struct GitHubStatsResponse {
    plugins: Vec<PluginInstallCount>,
}

/// Get path to the install counts cache file.
fn get_install_counts_cache_path() -> PathBuf {
    get_plugins_directory().join(INSTALL_COUNTS_CACHE_FILENAME)
}

/// Load the install counts cache from disk.
async fn load_install_counts_cache() -> Option<InstallCountsCache> {
    let cache_path = get_install_counts_cache_path();

    let content = match fs::read_to_string(&cache_path).await {
        Ok(c) => c,
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                debug!("Failed to load install counts cache: {}", e);
            }
            return None;
        }
    };

    let parsed: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => {
            debug!("Install counts cache has invalid JSON");
            return None;
        }
    };

    // Validate structure
    let version = parsed.get("version")?.as_u64()?;
    if version != INSTALL_COUNTS_CACHE_VERSION as u64 {
        debug!("Install counts cache version mismatch (got {}, expected {})", version, INSTALL_COUNTS_CACHE_VERSION);
        return None;
    }

    let fetched_at = parsed.get("fetchedAt")?.as_str()?;
    let counts_val = parsed.get("counts")?;
    if !counts_val.is_array() {
        debug!("Install counts cache has invalid structure");
        return None;
    }

    // Validate fetchedAt is a valid date and check staleness
    let fetched_time = chrono::DateTime::parse_from_rfc3339(fetched_at)
        .or_else(|_| chrono::DateTime::parse_from_str(fetched_at, "%Y-%m-%dT%H:%M:%S%.fZ"))
        .ok()?;
    let now = chrono::Utc::now();
    let elapsed_ms = (now - fetched_time.with_timezone(&chrono::Utc)).num_milliseconds() as u64;
    if elapsed_ms > CACHE_TTL_MS {
        debug!("Install counts cache is stale (>24h old)");
        return None;
    }

    // Parse the full cache
    match serde_json::from_value::<InstallCountsCache>(parsed) {
        Ok(cache) => Some(cache),
        Err(_) => {
            debug!("Install counts cache has malformed entries");
            None
        }
    }
}

/// Save the install counts cache to disk atomically.
async fn save_install_counts_cache(cache: &InstallCountsCache) {
    let cache_path = get_install_counts_cache_path();
    let random_suffix: String = hex::encode(rand::thread_rng().gen::<[u8; 8]>());
    let temp_path = format!("{}.{}.tmp", cache_path.display(), random_suffix);

    let plugins_dir = get_plugins_directory();
    if let Err(e) = fs::create_dir_all(&plugins_dir).await {
        debug!("Failed to create plugins directory: {}", e);
        return;
    }

    let content = match serde_json::to_string_pretty(cache) {
        Ok(c) => c,
        Err(e) => {
            debug!("Failed to serialize install counts cache: {}", e);
            return;
        }
    };

    if let Err(e) = fs::write(&temp_path, &content).await {
        debug!("Failed to write install counts temp file: {}", e);
        let _ = fs::remove_file(&temp_path).await;
        return;
    }

    if let Err(e) = fs::rename(&temp_path, &cache_path).await {
        debug!("Failed to rename install counts cache: {}", e);
        let _ = fs::remove_file(&temp_path).await;
        return;
    }

    debug!("Install counts cache saved successfully");
}

/// Fetch install counts from GitHub stats repository.
async fn fetch_install_counts_from_github() -> Result<Vec<PluginInstallCount>, anyhow::Error> {
    debug!("Fetching install counts from {}", INSTALL_COUNTS_URL);

    let started = std::time::Instant::now();
    let client = reqwest::Client::new();

    match client
        .get(INSTALL_COUNTS_URL)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
    {
        Ok(response) => {
            let body = response.text().await?;
            let stats: GitHubStatsResponse = serde_json::from_str(&body)
                .map_err(|_| anyhow::anyhow!("Invalid response format from install counts API"))?;

            log_plugin_fetch(
                PluginFetchSource::InstallCounts,
                Some(INSTALL_COUNTS_URL),
                PluginFetchOutcome::Success,
                started.elapsed().as_millis() as f64,
                None,
            );

            Ok(stats.plugins)
        }
        Err(e) => {
            let err_str = e.to_string();
            log_plugin_fetch(
                PluginFetchSource::InstallCounts,
                Some(INSTALL_COUNTS_URL),
                PluginFetchOutcome::Failure,
                started.elapsed().as_millis() as f64,
                Some(classify_fetch_error(&err_str)),
            );
            Err(e.into())
        }
    }
}

/// Get plugin install counts as a HashMap.
/// Uses cached data if available and less than 24 hours old.
/// Returns None on errors so UI can hide counts rather than show misleading zeros.
pub async fn get_install_counts() -> Option<HashMap<String, u64>> {
    // Try to load from cache first
    if let Some(cache) = load_install_counts_cache().await {
        debug!("Using cached install counts");
        log_plugin_fetch(
            PluginFetchSource::InstallCounts,
            Some(INSTALL_COUNTS_URL),
            PluginFetchOutcome::CacheHit,
            0.0,
            None,
        );
        let map: HashMap<String, u64> = cache
            .counts
            .into_iter()
            .map(|e| (e.plugin, e.unique_installs))
            .collect();
        return Some(map);
    }

    // Cache miss or stale - fetch from GitHub
    match fetch_install_counts_from_github().await {
        Ok(counts) => {
            let new_cache = InstallCountsCache {
                version: INSTALL_COUNTS_CACHE_VERSION,
                fetched_at: chrono::Utc::now().to_rfc3339(),
                counts: counts.clone(),
            };
            save_install_counts_cache(&new_cache).await;

            let map: HashMap<String, u64> = counts
                .into_iter()
                .map(|e| (e.plugin, e.unique_installs))
                .collect();
            Some(map)
        }
        Err(e) => {
            debug!("Failed to fetch install counts: {}", e);
            None
        }
    }
}

/// Format an install count for display.
pub fn format_install_count(count: u64) -> String {
    if count < 1000 {
        return count.to_string();
    }

    if count < 1_000_000 {
        let k = count as f64 / 1000.0;
        let formatted = format!("{:.1}", k);
        if formatted.ends_with(".0") {
            return format!("{}K", &formatted[..formatted.len() - 2]);
        }
        return format!("{}K", formatted);
    }

    let m = count as f64 / 1_000_000.0;
    let formatted = format!("{:.1}", m);
    if formatted.ends_with(".0") {
        return format!("{}M", &formatted[..formatted.len() - 2]);
    }
    format!("{}M", formatted)
}
