// Release notes: changelog fetch/cache/parse.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::Result;
use tokio::fs;

const MAX_RELEASE_NOTES_SHOWN: usize = 5;

pub const CHANGELOG_URL: &str =
    "https://github.com/mossen/mossen-code/blob/main/CHANGELOG.md";
const RAW_CHANGELOG_URL: &str =
    "https://raw.githubusercontent.com/mossen/mossen-code/refs/heads/main/CHANGELOG.md";

static CHANGELOG_MEMORY_CACHE: once_cell::sync::Lazy<Mutex<Option<String>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(None));

fn get_changelog_cache_path(config_home: &Path) -> PathBuf {
    config_home.join("cache").join("changelog.md")
}

/// Reset the in-memory changelog cache (for testing).
pub fn reset_changelog_cache_for_testing() {
    let mut cache = CHANGELOG_MEMORY_CACHE.lock().unwrap();
    *cache = None;
}

/// Migrate changelog from old config-based storage to file-based storage.
pub async fn migrate_changelog_from_config(
    config_home: &Path,
    cached_changelog: Option<&str>,
) -> Result<()> {
    let content = match cached_changelog {
        Some(c) if !c.is_empty() => c,
        _ => return Ok(()),
    };

    let cache_path = get_changelog_cache_path(config_home);
    if let Some(parent) = cache_path.parent() {
        fs::create_dir_all(parent).await?;
    }
    // Write only if file doesn't exist
    if !cache_path.exists() {
        fs::write(&cache_path, content).await?;
    }
    Ok(())
}

/// Fetch the changelog from GitHub and store it in cache file.
pub async fn fetch_and_store_changelog(config_home: &Path) -> Result<()> {
    let response = reqwest::get(RAW_CHANGELOG_URL).await?;
    if response.status().is_success() {
        let changelog_content = response.text().await?;

        // Skip write if content unchanged
        {
            let cache = CHANGELOG_MEMORY_CACHE.lock().unwrap();
            if cache.as_deref() == Some(&changelog_content) {
                return Ok(());
            }
        }

        let cache_path = get_changelog_cache_path(config_home);
        if let Some(parent) = cache_path.parent() {
            fs::create_dir_all(parent).await?;
        }
        fs::write(&cache_path, &changelog_content).await?;

        let mut cache = CHANGELOG_MEMORY_CACHE.lock().unwrap();
        *cache = Some(changelog_content);
    }
    Ok(())
}

/// Get the stored changelog from cache file if available.
pub async fn get_stored_changelog(config_home: &Path) -> String {
    {
        let cache = CHANGELOG_MEMORY_CACHE.lock().unwrap();
        if let Some(ref c) = *cache {
            return c.clone();
        }
    }

    let cache_path = get_changelog_cache_path(config_home);
    match fs::read_to_string(&cache_path).await {
        Ok(content) => {
            let mut cache = CHANGELOG_MEMORY_CACHE.lock().unwrap();
            *cache = Some(content.clone());
            content
        }
        Err(_) => {
            let mut cache = CHANGELOG_MEMORY_CACHE.lock().unwrap();
            *cache = Some(String::new());
            String::new()
        }
    }
}

/// Synchronous accessor for the changelog from in-memory cache.
pub fn get_stored_changelog_from_memory() -> String {
    let cache = CHANGELOG_MEMORY_CACHE.lock().unwrap();
    cache.clone().unwrap_or_default()
}

/// Parses a changelog string in markdown format into a structured format.
pub fn parse_changelog(content: &str) -> std::collections::HashMap<String, Vec<String>> {
    use std::collections::HashMap;

    if content.is_empty() {
        return HashMap::new();
    }

    let mut release_notes: HashMap<String, Vec<String>> = HashMap::new();

    // Split by heading lines (## X.X.X)
    let sections: Vec<&str> = content.split("\n## ").skip(1).collect();

    for section in sections {
        let lines: Vec<&str> = section.trim().lines().collect();
        if lines.is_empty() {
            continue;
        }

        // Extract version from the first line
        let version_line = lines[0];
        let version = version_line
            .split(" - ")
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
        if version.is_empty() {
            continue;
        }

        // Extract bullet points
        let notes: Vec<String> = lines[1..]
            .iter()
            .filter(|line| line.trim().starts_with("- "))
            .map(|line| line.trim().strip_prefix("- ").unwrap_or("").trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        if !notes.is_empty() {
            release_notes.insert(version, notes);
        }
    }

    release_notes
}

/// Compare two semver strings. Returns true if a > b.
fn semver_gt(a: &str, b: &str) -> bool {
    let parse = |s: &str| -> (u64, u64, u64) {
        let parts: Vec<&str> = s.split('.').collect();
        let major = parts.first().and_then(|p| p.parse().ok()).unwrap_or(0);
        let minor = parts.get(1).and_then(|p| p.parse().ok()).unwrap_or(0);
        let patch = parts
            .get(2)
            .and_then(|p| p.split('-').next())
            .and_then(|p| p.parse().ok())
            .unwrap_or(0);
        (major, minor, patch)
    };
    parse(a) > parse(b)
}

/// Gets release notes to show based on the previously seen version.
pub fn get_recent_release_notes(
    current_version: &str,
    previous_version: Option<&str>,
    changelog_content: &str,
) -> Vec<String> {
    let release_notes = parse_changelog(changelog_content);

    let base_current = coerce_version(current_version);
    let base_previous = previous_version.map(coerce_version);

    let should_show = match &base_previous {
        None => true,
        Some(prev) => semver_gt(&base_current, prev),
    };

    if !should_show {
        return Vec::new();
    }

    let mut entries: Vec<(String, Vec<String>)> = release_notes
        .into_iter()
        .filter(|(version, _)| {
            base_previous
                .as_ref()
                .map_or(true, |prev| semver_gt(version, prev))
        })
        .collect();

    entries.sort_by(|(a, _), (b, _)| {
        if semver_gt(a, b) {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Greater
        }
    });

    entries
        .into_iter()
        .flat_map(|(_, notes)| notes)
        .take(MAX_RELEASE_NOTES_SHOWN)
        .collect()
}

/// Gets all release notes sorted oldest first.
pub fn get_all_release_notes(
    changelog_content: &str,
) -> Vec<(String, Vec<String>)> {
    let release_notes = parse_changelog(changelog_content);

    let mut sorted: Vec<(String, Vec<String>)> = release_notes
        .into_iter()
        .filter(|(_, notes)| !notes.is_empty())
        .collect();

    sorted.sort_by(|(a, _), (b, _)| {
        if semver_gt(a, b) {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Less
        }
    });

    sorted
}

/// Coerce a version string to just major.minor.patch
fn coerce_version(version: &str) -> String {
    let cleaned = version.trim();
    let parts: Vec<&str> = cleaned.split('.').collect();
    if parts.len() >= 3 {
        let patch = parts[2].split('-').next().unwrap_or("0");
        format!("{}.{}.{}", parts[0], parts[1], patch)
    } else if parts.len() == 2 {
        format!("{}.{}.0", parts[0], parts[1])
    } else {
        format!("{}.0.0", parts[0])
    }
}

/// Checks if there are release notes to show.
pub async fn check_for_release_notes(
    last_seen_version: Option<&str>,
    current_version: &str,
    config_home: &Path,
) -> (bool, Vec<String>) {
    let cached_changelog = get_stored_changelog(config_home).await;

    // If version changed or no cached changelog, fetch in background
    if last_seen_version != Some(current_version) || cached_changelog.is_empty() {
        let config_home_owned = config_home.to_path_buf();
        tokio::spawn(async move {
            let _ = fetch_and_store_changelog(&config_home_owned).await;
        });
    }

    let release_notes =
        get_recent_release_notes(current_version, last_seen_version, &cached_changelog);
    let has_release_notes = !release_notes.is_empty();

    (has_release_notes, release_notes)
}

/// Synchronous variant of check_for_release_notes.
pub fn check_for_release_notes_sync(
    last_seen_version: Option<&str>,
    current_version: &str,
) -> (bool, Vec<String>) {
    let changelog = get_stored_changelog_from_memory();
    let release_notes = get_recent_release_notes(current_version, last_seen_version, &changelog);
    (!release_notes.is_empty(), release_notes)
}
