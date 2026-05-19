use std::path::{Path, PathBuf, MAIN_SEPARATOR};
use std::sync::Mutex;

use once_cell::sync::Lazy;
use tracing::debug;

use super::plugin_directories::get_plugins_directory;

const ORPHANED_AT_FILENAME: &str = ".orphaned_at";

/// Session-scoped cache. Frozen once computed — only cleared by explicit /reload-plugins.
static CACHED_EXCLUSIONS: Lazy<Mutex<Option<Vec<String>>>> = Lazy::new(|| Mutex::new(None));

/// Get ripgrep glob exclusion patterns for orphaned plugin versions.
///
/// When provided, exclusions are only returned if the search overlaps
/// the plugin cache directory.
pub async fn get_glob_exclusions_for_plugin_cache(
    search_path: Option<&str>,
    rip_grep: impl Fn(&[&str], &str) -> Result<Vec<String>, anyhow::Error>,
) -> Vec<String> {
    let cache_path = {
        let plugins_dir = get_plugins_directory();
        let p = PathBuf::from(&plugins_dir).join("cache");
        normalize_path(&p)
    };

    if let Some(sp) = search_path {
        if !paths_overlap(sp, &cache_path) {
            return vec![];
        }
    }

    {
        let guard = CACHED_EXCLUSIONS.lock().unwrap();
        if let Some(ref cached) = *guard {
            return cached.clone();
        }
    }

    let exclusions = match rip_grep(
        &[
            "--files",
            "--hidden",
            "--no-ignore",
            "--max-depth",
            "4",
            "--glob",
            ORPHANED_AT_FILENAME,
        ],
        &cache_path,
    ) {
        Ok(markers) => markers
            .iter()
            .map(|marker_path| {
                let version_dir = Path::new(marker_path)
                    .parent()
                    .unwrap_or(Path::new(""))
                    .to_string_lossy()
                    .to_string();
                let rel = if Path::new(&version_dir).is_absolute() {
                    pathdiff::diff_paths(&version_dir, &cache_path)
                        .unwrap_or_else(|| PathBuf::from(&version_dir))
                        .to_string_lossy()
                        .to_string()
                } else {
                    version_dir
                };
                let posix_relative = rel.replace('\\', "/");
                format!("!**/{posix_relative}/**")
            })
            .collect(),
        Err(_) => {
            // Best-effort — don't break core search tools if ripgrep fails
            vec![]
        }
    };

    let mut guard = CACHED_EXCLUSIONS.lock().unwrap();
    *guard = Some(exclusions.clone());
    exclusions
}

/// Clear the cached exclusions. Called by /reload-plugins.
pub fn clear_plugin_cache_exclusions() {
    let mut guard = CACHED_EXCLUSIONS.lock().unwrap();
    *guard = None;
}

/// One path is a prefix of the other.
fn paths_overlap(a: &str, b: &str) -> bool {
    let na = normalize_for_compare(a);
    let nb = normalize_for_compare(b);
    let sep = MAIN_SEPARATOR.to_string();

    na == nb
        || na == sep
        || nb == sep
        || na.starts_with(&format!("{}{}", nb, sep))
        || nb.starts_with(&format!("{}{}", na, sep))
}

fn normalize_for_compare(p: &str) -> String {
    let normalized = normalize_path(Path::new(p));
    if cfg!(windows) {
        normalized.to_lowercase()
    } else {
        normalized
    }
}

fn normalize_path(p: &Path) -> String {
    p.canonicalize()
        .unwrap_or_else(|_| p.to_path_buf())
        .to_string_lossy()
        .to_string()
}
