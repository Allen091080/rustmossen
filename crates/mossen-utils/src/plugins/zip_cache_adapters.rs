use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use tokio::fs;
use tracing::debug;

use super::schemas::{KnownMarketplacesFile, PluginMarketplace};
use super::zip_cache::{
    atomic_write_to_zip_cache, get_marketplace_json_relative_path, get_plugin_zip_cache_path,
    get_zip_cache_known_marketplaces_path,
};

/// Read known_marketplaces.json from the zip cache.
/// Returns empty object if file doesn't exist or fails validation.
pub async fn read_zip_cache_known_marketplaces() -> KnownMarketplacesFile {
    let path = match get_zip_cache_known_marketplaces_path() {
        Ok(p) => p,
        Err(_) => return HashMap::new(),
    };

    match fs::read_to_string(&path).await {
        Ok(content) => match serde_json::from_str::<KnownMarketplacesFile>(&content) {
            Ok(data) => data,
            Err(e) => {
                debug!(
                    "Invalid known_marketplaces.json in zip cache: {}",
                    e
                );
                HashMap::new()
            }
        },
        Err(_) => HashMap::new(),
    }
}

/// Write known_marketplaces.json to the zip cache atomically.
pub async fn write_zip_cache_known_marketplaces(
    data: &KnownMarketplacesFile,
) -> Result<(), anyhow::Error> {
    let path = get_zip_cache_known_marketplaces_path()?;
    let content = serde_json::to_string_pretty(data)?;
    atomic_write_to_zip_cache(&path, content.as_bytes()).await
}

/// Read a marketplace JSON file from the zip cache.
pub async fn read_marketplace_json(marketplace_name: &str) -> Option<PluginMarketplace> {
    let zip_cache_path = get_plugin_zip_cache_path()?;
    let rel_path = get_marketplace_json_relative_path(marketplace_name);
    let full_path = format!("{}/{}", zip_cache_path, rel_path);

    match fs::read_to_string(&full_path).await {
        Ok(content) => match serde_json::from_str::<PluginMarketplace>(&content) {
            Ok(marketplace) => Some(marketplace),
            Err(e) => {
                debug!(
                    "Invalid marketplace JSON for {}: {}",
                    marketplace_name, e
                );
                None
            }
        },
        Err(_) => None,
    }
}

/// Save a marketplace JSON to the zip cache from its install location.
pub async fn save_marketplace_json_to_zip_cache(
    marketplace_name: &str,
    install_location: &str,
) -> Result<(), anyhow::Error> {
    let zip_cache_path = match get_plugin_zip_cache_path() {
        Some(p) => p,
        None => return Ok(()),
    };

    if let Some(content) = read_marketplace_json_content(install_location).await {
        let rel_path = get_marketplace_json_relative_path(marketplace_name);
        let full_path = format!("{}/{}", zip_cache_path, rel_path);
        atomic_write_to_zip_cache(&full_path, content.as_bytes()).await?;
    }
    Ok(())
}

/// Read marketplace.json content from a cloned marketplace directory or file.
async fn read_marketplace_json_content(dir: &str) -> Option<String> {
    let candidates = [
        format!("{}/.mossen-plugin/marketplace.json", dir),
        format!("{}/marketplace.json", dir),
        dir.to_string(), // For URL sources, installLocation IS the marketplace JSON file
    ];

    for candidate in &candidates {
        match fs::read_to_string(candidate).await {
            Ok(content) => return Some(content),
            Err(_) => continue,
        }
    }
    None
}

/// Sync marketplace data to zip cache for offline access.
pub async fn sync_marketplaces_to_zip_cache(
    load_known_marketplaces_safe: impl std::future::Future<Output = KnownMarketplacesFile>,
) {
    let known_marketplaces = load_known_marketplaces_safe.await;

    // Save marketplace JSONs to zip cache
    for (name, entry) in &known_marketplaces {
        let install_location = &entry.install_location;
        if !install_location.is_empty() {
            if let Err(e) = save_marketplace_json_to_zip_cache(name, install_location).await {
                debug!("Failed to save marketplace JSON for {}: {}", name, e);
            }
        }
    }

    // Merge with previously cached data
    let zip_cache_known = read_zip_cache_known_marketplaces().await;
    let mut merged = zip_cache_known;
    for (name, entry) in known_marketplaces {
        merged.insert(name, entry);
    }

    if let Err(e) = write_zip_cache_known_marketplaces(&merged).await {
        debug!("Failed to write merged known_marketplaces to zip cache: {}", e);
    }
}
