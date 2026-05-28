//! Temporary file path generation utilities.
//!
//! This module provides utilities for generating temporary file paths
//! with optional content-based hashing for stable paths.

use std::env::temp_dir;
use std::path::PathBuf;

/// Generate a temporary file path.
///
/// # Arguments
///
/// * `prefix` - Optional prefix for the temp file name (default: 'mossen-prompt')
/// * `extension` - Optional file extension (default: '.md')
/// * `options` - Optional configuration
///
/// # Options
///
/// * `content_hash` - When provided, the identifier is derived from a
///   SHA-256 hash of this string (first 16 hex chars). This produces a path
///   that is stable across process boundaries - any process with the same
///   content will get the same path. Use this when the path ends up in content
///   sent to the Mossen API (e.g., sandbox deny lists in tool descriptions),
///   because a random UUID would change on every subprocess spawn and
///   invalidate the prompt cache prefix.
///
/// # Returns
///
/// Temp file path as a String
pub fn generate_temp_file_path(
    prefix: Option<&str>,
    extension: Option<&str>,
    options: Option<TempFileOptions>,
) -> String {
    let prefix = prefix.unwrap_or("mossen-prompt");
    let extension = extension.unwrap_or(".md");

    let id = match options {
        Some(opts) => {
            if let Some(content_hash) = opts.content_hash {
                // Use hash for stable path
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};

                let mut hasher = DefaultHasher::new();
                content_hash.hash(&mut hasher);
                let hash = hasher.finish();
                format!("{:016x}", hash)
            } else {
                // Generate a simple random-like ID based on time and a static counter
                use std::time::{SystemTime, UNIX_EPOCH};
                static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
                let time = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos();
                let counter = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                format!("{:016x}-{:08x}", time as u64, counter)
            }
        }
        None => {
            // Generate a simple random-like ID based on time
            use std::time::{SystemTime, UNIX_EPOCH};
            let time = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            format!("{:016x}", time as u64)
        }
    };

    let temp_dir = temp_dir();
    let _file_name = format!("{}-{}-{}{}", prefix, id, id, extension);

    // Actually we want just the id without duplication
    let file_name = format!("{}-{}{}", prefix, id, extension);

    let path = temp_dir.join(file_name);
    path.to_string_lossy().to_string()
}

/// Options for temp file generation
#[derive(Debug, Clone, Default)]
pub struct TempFileOptions {
    /// Content hash for stable path generation
    pub content_hash: Option<String>,
}

/// Generate a stable temp file path based on content hash.
/// This is useful for caching scenarios where the same content
/// should always produce the same temp file path.
pub fn generate_stable_temp_file_path(
    prefix: &str,
    content: &str,
    extension: Option<&str>,
) -> String {
    let extension = extension.unwrap_or(".md");

    // Create a hash of the content
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    let hash = hasher.finish();
    let id = format!("{:016x}", hash);

    let temp_dir = temp_dir();
    let file_name = format!("{}-{}-{}{}", prefix, &id[..8], &id[8..16], extension);

    let path = temp_dir.join(file_name);
    path.to_string_lossy().to_string()
}

/// Get the system temp directory
pub fn get_temp_dir() -> PathBuf {
    temp_dir()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_temp_file_path_default() {
        let path = generate_temp_file_path(None, None, None);
        assert!(path.contains("mossen-prompt"));
        assert!(path.ends_with(".md"));
    }

    #[test]
    fn test_generate_temp_file_path_custom() {
        let path = generate_temp_file_path(Some("custom"), Some(".txt"), None);
        assert!(path.contains("custom"));
        assert!(path.ends_with(".txt"));
    }

    #[test]
    fn test_generate_temp_file_path_with_hash() {
        let options = TempFileOptions {
            content_hash: Some("test content".to_string()),
        };
        let path1 = generate_temp_file_path(None, None, Some(options.clone()));
        let path2 = generate_temp_file_path(None, None, Some(options));

        // Same content should produce same path (stable)
        assert_eq!(path1, path2);
    }

    #[test]
    fn test_generate_stable_temp_file_path() {
        let path = generate_stable_temp_file_path("test", "same content", Some(".md"));
        assert!(path.contains("test"));
        assert!(path.ends_with(".md"));
    }

    #[test]
    fn test_get_temp_dir() {
        let temp = get_temp_dir();
        assert!(temp.exists());
        assert!(temp.is_dir());
    }
}
