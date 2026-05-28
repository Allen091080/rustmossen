use std::path::{Path, PathBuf};
use std::sync::Mutex;

use once_cell::sync::Lazy;
use tokio::fs;
use tracing::debug;

use super::schemas::MarketplaceSource;

/// Check if the plugin zip cache mode is enabled.
pub fn is_plugin_zip_cache_enabled() -> bool {
    std::env::var("MOSSEN_CODE_PLUGIN_USE_ZIP_CACHE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Get the path to the zip cache directory.
pub fn get_plugin_zip_cache_path() -> Option<String> {
    if !is_plugin_zip_cache_enabled() {
        return None;
    }
    std::env::var("MOSSEN_CODE_PLUGIN_CACHE_DIR")
        .ok()
        .map(|d| expand_tilde(&d))
}

/// Get the path to known_marketplaces.json in the zip cache.
pub fn get_zip_cache_known_marketplaces_path() -> Result<String, anyhow::Error> {
    let cache_path = get_plugin_zip_cache_path()
        .ok_or_else(|| anyhow::anyhow!("Plugin zip cache is not enabled"))?;
    Ok(format!("{}/known_marketplaces.json", cache_path))
}

/// Get the path to installed_plugins.json in the zip cache.
pub fn get_zip_cache_installed_plugins_path() -> Result<String, anyhow::Error> {
    let cache_path = get_plugin_zip_cache_path()
        .ok_or_else(|| anyhow::anyhow!("Plugin zip cache is not enabled"))?;
    Ok(format!("{}/installed_plugins.json", cache_path))
}

/// Get the marketplaces directory within the zip cache.
pub fn get_zip_cache_marketplaces_dir() -> String {
    let cache_path = get_plugin_zip_cache_path().unwrap_or_default();
    format!("{}/marketplaces", cache_path)
}

/// Get the plugins directory within the zip cache.
pub fn get_zip_cache_plugins_dir() -> String {
    let cache_path = get_plugin_zip_cache_path().unwrap_or_default();
    format!("{}/plugins", cache_path)
}

// Session plugin cache state
static SESSION_PLUGIN_CACHE_PATH: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));

/// Get or create the session plugin cache directory.
pub async fn get_session_plugin_cache_path() -> Result<String, anyhow::Error> {
    {
        let guard = SESSION_PLUGIN_CACHE_PATH.lock().unwrap();
        if let Some(ref path) = *guard {
            return Ok(path.clone());
        }
    }

    let suffix = hex::encode(rand::random::<[u8; 8]>());
    let dir = format!(
        "{}/mossen-plugin-session-{}",
        std::env::temp_dir().display(),
        suffix
    );
    fs::create_dir_all(&dir).await?;

    let mut guard = SESSION_PLUGIN_CACHE_PATH.lock().unwrap();
    *guard = Some(dir.clone());
    debug!("Created session plugin cache at {}", dir);
    Ok(dir)
}

/// Clean up the session plugin cache directory.
pub async fn cleanup_session_plugin_cache() {
    let path = {
        let guard = SESSION_PLUGIN_CACHE_PATH.lock().unwrap();
        guard.clone()
    };

    if let Some(path) = path {
        match fs::remove_dir_all(&path).await {
            Ok(_) => debug!("Cleaned up session plugin cache at {}", path),
            Err(e) => debug!("Failed to clean up session plugin cache: {}", e),
        }
        let mut guard = SESSION_PLUGIN_CACHE_PATH.lock().unwrap();
        *guard = None;
    }
}

/// Reset the session plugin cache path (for testing).
pub fn reset_session_plugin_cache() {
    let mut guard = SESSION_PLUGIN_CACHE_PATH.lock().unwrap();
    *guard = None;
}

/// Write data to a file in the zip cache atomically.
pub async fn atomic_write_to_zip_cache(
    target_path: &str,
    data: &[u8],
) -> Result<(), anyhow::Error> {
    let dir = Path::new(target_path).parent().unwrap_or(Path::new("."));
    fs::create_dir_all(dir).await?;

    let tmp_name = format!(
        ".{}.tmp.{}",
        Path::new(target_path)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy(),
        hex::encode(rand::random::<[u8; 4]>())
    );
    let tmp_path = dir.join(&tmp_name);

    match fs::write(&tmp_path, data).await {
        Ok(_) => {
            if let Err(e) = fs::rename(&tmp_path, target_path).await {
                let _ = fs::remove_file(&tmp_path).await;
                return Err(e.into());
            }
            Ok(())
        }
        Err(e) => {
            let _ = fs::remove_file(&tmp_path).await;
            Err(e.into())
        }
    }
}

/// Create a ZIP archive from a directory.
pub async fn create_zip_from_directory(source_dir: &str) -> Result<Vec<u8>, anyhow::Error> {
    use zip::write::FileOptions;
    use zip::ZipWriter;

    let zip_buf = Vec::new();
    let cursor = std::io::Cursor::new(zip_buf);
    let mut zip_writer = ZipWriter::new(cursor);
    let options = FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let mut visited = std::collections::HashSet::new();
    collect_files_for_zip(source_dir, "", &mut zip_writer, &options, &mut visited).await?;
    let cursor = zip_writer.finish()?;
    let zip_buf = cursor.into_inner();

    debug!("Created ZIP from {}: {} bytes", source_dir, zip_buf.len());
    Ok(zip_buf)
}

async fn collect_files_for_zip<W: std::io::Write + std::io::Seek>(
    base_dir: &str,
    relative_path: &str,
    zip_writer: &mut zip::ZipWriter<W>,
    options: &zip::write::FileOptions,
    visited: &mut std::collections::HashSet<String>,
) -> Result<(), anyhow::Error> {
    use std::io::Write;
    let current_dir = if relative_path.is_empty() {
        PathBuf::from(base_dir)
    } else {
        PathBuf::from(base_dir).join(relative_path)
    };

    let mut entries = match fs::read_dir(&current_dir).await {
        Ok(rd) => rd,
        Err(_) => return Ok(()),
    };

    // Track visited directories by path for cycle detection
    let dir_key = current_dir.to_string_lossy().to_string();
    if visited.contains(&dir_key) {
        debug!("Skipping symlink cycle at {}", current_dir.display());
        return Ok(());
    }
    visited.insert(dir_key);

    let mut dir_entries = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        dir_entries.push(entry);
    }

    for entry in dir_entries {
        let file_name = entry.file_name().to_string_lossy().to_string();
        if file_name == ".git" {
            continue;
        }

        let full_path = current_dir.join(&file_name);
        let rel_path = if relative_path.is_empty() {
            file_name.clone()
        } else {
            format!("{}/{}", relative_path, file_name)
        };

        let meta = match fs::symlink_metadata(&full_path).await {
            Ok(m) => m,
            Err(_) => continue,
        };

        if meta.is_symlink() {
            match fs::metadata(&full_path).await {
                Ok(target_meta) => {
                    if target_meta.is_dir() {
                        continue; // Skip symlinked directories
                    }
                    // Symlinked file — read its contents
                    if target_meta.is_file() {
                        if let Ok(content) = fs::read(&full_path).await {
                            zip_writer.start_file(&rel_path, *options)?;
                            zip_writer.write_all(&content)?;
                        }
                    }
                }
                Err(_) => continue, // Broken symlink
            }
        } else if meta.is_dir() {
            Box::pin(collect_files_for_zip(
                base_dir, &rel_path, zip_writer, options, visited,
            ))
            .await?;
        } else if meta.is_file() {
            if let Ok(content) = fs::read(&full_path).await {
                zip_writer.start_file(&rel_path, *options)?;
                zip_writer.write_all(&content)?;
            }
        }
    }

    Ok(())
}

/// Extract a ZIP file to a target directory.
pub async fn extract_zip_to_directory(
    zip_path: &str,
    target_dir: &str,
) -> Result<(), anyhow::Error> {
    let zip_data = fs::read(zip_path).await?;
    let reader = std::io::Cursor::new(zip_data);
    let mut archive = zip::ZipArchive::new(reader)?;

    fs::create_dir_all(target_dir).await?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let rel_path = file.name().to_string();

        if rel_path.ends_with('/') {
            fs::create_dir_all(Path::new(target_dir).join(&rel_path)).await?;
            continue;
        }

        let full_path = Path::new(target_dir).join(&rel_path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let mut content = Vec::new();
        std::io::Read::read_to_end(&mut file, &mut content)?;
        fs::write(&full_path, &content).await?;

        // Restore exec bits on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Some(mode) = file.unix_mode() {
                if mode & 0o111 != 0 {
                    let _ = fs::set_permissions(
                        &full_path,
                        std::fs::Permissions::from_mode(mode & 0o777),
                    )
                    .await;
                }
            }
        }
    }

    debug!("Extracted ZIP to {}: {} entries", target_dir, archive.len());
    Ok(())
}

/// Convert a plugin directory to a ZIP in-place.
pub async fn convert_directory_to_zip_in_place(
    dir_path: &str,
    zip_path: &str,
) -> Result<(), anyhow::Error> {
    let zip_data = create_zip_from_directory(dir_path).await?;
    atomic_write_to_zip_cache(zip_path, &zip_data).await?;
    fs::remove_dir_all(dir_path).await?;
    Ok(())
}

/// Get the relative path for a marketplace JSON file within the zip cache.
pub fn get_marketplace_json_relative_path(marketplace_name: &str) -> String {
    let sanitized: String = marketplace_name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    format!("marketplaces/{}.json", sanitized)
}

/// Check if a marketplace source type is supported by zip cache mode.
pub fn is_marketplace_source_supported_by_zip_cache(source: &MarketplaceSource) -> bool {
    matches!(
        source,
        MarketplaceSource::GitHub { .. }
            | MarketplaceSource::Git { .. }
            | MarketplaceSource::Url { .. }
            | MarketplaceSource::Settings { .. }
    )
}

fn expand_tilde(path: &str) -> String {
    if path.starts_with('~') {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
        let rest = path.strip_prefix('~').unwrap_or("");
        home.join(rest.strip_prefix('/').unwrap_or(rest))
            .to_string_lossy()
            .to_string()
    } else {
        path.to_string()
    }
}
