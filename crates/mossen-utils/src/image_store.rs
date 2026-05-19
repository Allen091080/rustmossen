use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use once_cell::sync::Lazy;
use tokio::fs;

const IMAGE_STORE_DIR: &str = "image-cache";
const MAX_STORED_IMAGE_PATHS: usize = 200;

/// In-memory cache of stored image paths.
static STORED_IMAGE_PATHS: Lazy<Mutex<HashMap<u64, String>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Pasted content types.
#[derive(Debug, Clone)]
pub enum PastedContent {
    Image {
        id: u64,
        content: String, // base64 encoded
        media_type: Option<String>,
    },
    Text {
        id: u64,
        content: String,
    },
}

impl PastedContent {
    pub fn id(&self) -> u64 {
        match self {
            PastedContent::Image { id, .. } => *id,
            PastedContent::Text { id, .. } => *id,
        }
    }

    pub fn is_image(&self) -> bool {
        matches!(self, PastedContent::Image { .. })
    }
}

/// Get the image store directory for the current session.
fn get_image_store_dir(config_home: &Path, session_id: &str) -> PathBuf {
    config_home.join(IMAGE_STORE_DIR).join(session_id)
}

/// Ensure the image store directory exists.
async fn ensure_image_store_dir(config_home: &Path, session_id: &str) -> std::io::Result<()> {
    let dir = get_image_store_dir(config_home, session_id);
    fs::create_dir_all(&dir).await
}

/// Get the file path for an image by ID.
fn get_image_path(config_home: &Path, session_id: &str, image_id: u64, media_type: &str) -> PathBuf {
    let extension = media_type
        .split('/')
        .nth(1)
        .unwrap_or("png");
    get_image_store_dir(config_home, session_id).join(format!("{}.{}", image_id, extension))
}

/// Evict oldest entries if at capacity.
fn evict_oldest_if_at_cap(paths: &mut HashMap<u64, String>) {
    while paths.len() >= MAX_STORED_IMAGE_PATHS {
        // Remove the first key (oldest insertion)
        if let Some(&oldest_key) = paths.keys().next() {
            paths.remove(&oldest_key);
        } else {
            break;
        }
    }
}

/// Cache the image path immediately (fast, no file I/O).
pub fn cache_image_path(
    config_home: &Path,
    session_id: &str,
    content: &PastedContent,
) -> Option<String> {
    match content {
        PastedContent::Image { id, media_type, .. } => {
            let media = media_type.as_deref().unwrap_or("image/png");
            let image_path = get_image_path(config_home, session_id, *id, media);
            let path_str = image_path.to_string_lossy().to_string();

            let mut paths = STORED_IMAGE_PATHS.lock().unwrap();
            evict_oldest_if_at_cap(&mut paths);
            paths.insert(*id, path_str.clone());
            Some(path_str)
        }
        _ => None,
    }
}

/// Store an image from pastedContents to disk.
pub async fn store_image(
    config_home: &Path,
    session_id: &str,
    content: &PastedContent,
) -> Option<String> {
    match content {
        PastedContent::Image {
            id,
            content: base64_content,
            media_type,
        } => {
            if let Err(_) = ensure_image_store_dir(config_home, session_id).await {
                return None;
            }

            let media = media_type.as_deref().unwrap_or("image/png");
            let image_path = get_image_path(config_home, session_id, *id, media);

            // Decode base64 and write to file
            match base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                base64_content,
            ) {
                Ok(bytes) => {
                    if fs::write(&image_path, &bytes).await.is_err() {
                        return None;
                    }

                    let path_str = image_path.to_string_lossy().to_string();
                    let mut paths = STORED_IMAGE_PATHS.lock().unwrap();
                    evict_oldest_if_at_cap(&mut paths);
                    paths.insert(*id, path_str.clone());
                    Some(path_str)
                }
                Err(_) => None,
            }
        }
        _ => None,
    }
}

/// Store all images from pasted contents to disk.
pub async fn store_images(
    config_home: &Path,
    session_id: &str,
    pasted_contents: &HashMap<u64, PastedContent>,
) -> HashMap<u64, String> {
    let mut path_map = HashMap::new();

    for (id, content) in pasted_contents {
        if content.is_image() {
            if let Some(path) = store_image(config_home, session_id, content).await {
                path_map.insert(*id, path);
            }
        }
    }

    path_map
}

/// Get the file path for a stored image by ID.
pub fn get_stored_image_path(image_id: u64) -> Option<String> {
    STORED_IMAGE_PATHS
        .lock()
        .unwrap()
        .get(&image_id)
        .cloned()
}

/// Clear the in-memory cache of stored image paths.
pub fn clear_stored_image_paths() {
    STORED_IMAGE_PATHS.lock().unwrap().clear();
}

/// Clean up old image cache directories from previous sessions.
pub async fn cleanup_old_image_caches(config_home: &Path, current_session_id: &str) {
    let base_dir = config_home.join(IMAGE_STORE_DIR);

    let mut entries = match fs::read_dir(&base_dir).await {
        Ok(entries) => entries,
        Err(_) => return,
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_string();
        if name == current_session_id {
            continue;
        }

        let session_path = base_dir.join(&name);
        let _ = fs::remove_dir_all(&session_path).await;
    }

    // Remove base dir if empty
    if let Ok(mut remaining) = fs::read_dir(&base_dir).await {
        if remaining.next_entry().await.ok().flatten().is_none() {
            let _ = fs::remove_dir(&base_dir).await;
        }
    }
}
