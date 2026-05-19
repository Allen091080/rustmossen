//! VCR — Record and replay API interactions for testing

use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::path::{Path, PathBuf};
use tokio::fs;
use tracing::debug;

/// Check if VCR mode should be used
pub fn should_use_vcr() -> bool {
    if std::env::var("NODE_ENV").as_deref() == Ok("test") {
        return true;
    }
    if std::env::var("USER_TYPE").as_deref() == Ok("ant")
        && is_env_truthy("FORCE_VCR")
    {
        return true;
    }
    false
}

fn is_env_truthy(key: &str) -> bool {
    std::env::var(key)
        .map(|v| v == "1" || v.to_lowercase() == "true")
        .unwrap_or(false)
}

/// Compute fixture filename from message hashes
pub fn compute_fixture_path(base_dir: &Path, message_contents: &[&str]) -> PathBuf {
    let hashes: Vec<String> = message_contents
        .iter()
        .map(|content| {
            let mut hasher = Sha1::new();
            hasher.update(content.as_bytes());
            let result = hasher.finalize();
            hex::encode(&result[..3]) // First 6 hex chars (3 bytes)
        })
        .collect();

    base_dir.join("fixtures").join(format!("{}.json", hashes.join("-")))
}

/// VCR fixture format
#[derive(Debug, Serialize, Deserialize)]
pub struct VcrFixture {
    pub input: serde_json::Value,
    pub output: serde_json::Value,
}

/// Load a cached fixture if it exists
pub async fn load_fixture(path: &Path) -> Option<VcrFixture> {
    let content = fs::read_to_string(path).await.ok()?;
    serde_json::from_str(&content).ok()
}

/// Save a fixture to disk
pub async fn save_fixture(path: &Path, fixture: &VcrFixture) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("Failed to create fixture dir: {}", e))?;
    }

    let content = serde_json::to_string_pretty(fixture)
        .map_err(|e| format!("Failed to serialize fixture: {}", e))?;

    fs::write(path, content)
        .await
        .map_err(|e| format!("Failed to write fixture: {}", e))?;

    debug!("Saved VCR fixture: {}", path.display());
    Ok(())
}

/// Dehydrate a string by replacing dynamic content with placeholders
pub fn dehydrate_value(s: &str, cwd: &str, config_home: &str) -> String {
    s.replace(config_home, "[CONFIG_HOME]")
        .replace(cwd, "[CWD]")
}

/// Hydrate a string by replacing placeholders with actual values
pub fn hydrate_value(s: &str, cwd: &str, config_home: &str) -> String {
    s.replace("[CONFIG_HOME]", config_home)
        .replace("[CWD]", cwd)
        .replace("[NUM]", "1")
        .replace("[DURATION]", "100")
}

/// TS `withVCR` — execute the supplied async closure inside a VCR recording
/// context. The Rust port wires this as a transparent passthrough until the
/// recording context is generalised over `Future`.
pub async fn with_vcr<F, T>(fut: F) -> T
where
    F: std::future::Future<Output = T>,
{
    fut.await
}
