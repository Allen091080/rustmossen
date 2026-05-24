//! JetBrains IDE plugin detection utilities.
//!
//! Detects whether the Mossen JetBrains plugin is installed by scanning
//! known plugin directory locations across platforms.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use once_cell::sync::Lazy;
use regex::Regex;

const PLUGIN_PREFIX: &str = "mossen-code-jetbrains-plugin";

/// IDE type alias (matching TypeScript's IdeType).
pub type IdeType = String;

/// Map of IDE names to their directory patterns.
static IDE_NAME_TO_DIR_MAP: Lazy<HashMap<&'static str, Vec<&'static str>>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert("pycharm", vec!["PyCharm"]);
    m.insert("intellij", vec!["IntelliJIdea", "IdeaIC"]);
    m.insert("webstorm", vec!["WebStorm"]);
    m.insert("phpstorm", vec!["PhpStorm"]);
    m.insert("rubymine", vec!["RubyMine"]);
    m.insert("clion", vec!["CLion"]);
    m.insert("goland", vec!["GoLand"]);
    m.insert("rider", vec!["Rider"]);
    m.insert("datagrip", vec!["DataGrip"]);
    m.insert("appcode", vec!["AppCode"]);
    m.insert("dataspell", vec!["DataSpell"]);
    m.insert("aqua", vec!["Aqua"]);
    m.insert("gateway", vec!["Gateway"]);
    m.insert("fleet", vec!["Fleet"]);
    m.insert("androidstudio", vec!["AndroidStudio"]);
    m
});

/// Build common plugin directory paths for an IDE.
fn build_common_plugin_directory_paths(ide_name: &str) -> Vec<PathBuf> {
    let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let mut directories = Vec::new();
    let ide_lower = ide_name.to_lowercase();

    let ide_patterns = match IDE_NAME_TO_DIR_MAP.get(ide_lower.as_str()) {
        Some(p) => p,
        None => return directories,
    };

    let platform = std::env::consts::OS;

    match platform {
        "macos" => {
            directories.push(home_dir.join("Library/Application Support/JetBrains"));
            directories.push(home_dir.join("Library/Application Support"));
            if ide_lower == "androidstudio" {
                directories.push(home_dir.join("Library/Application Support/Google"));
            }
        }
        "windows" => {
            let app_data = std::env::var("APPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(|_| home_dir.join("AppData/Roaming"));
            let local_app_data = std::env::var("LOCALAPPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(|_| home_dir.join("AppData/Local"));
            directories.push(app_data.join("JetBrains"));
            directories.push(local_app_data.join("JetBrains"));
            directories.push(app_data.clone());
            if ide_lower == "androidstudio" {
                directories.push(local_app_data.join("Google"));
            }
        }
        "linux" => {
            directories.push(home_dir.join(".config/JetBrains"));
            directories.push(home_dir.join(".local/share/JetBrains"));
            for pattern in ide_patterns {
                directories.push(home_dir.join(format!(".{}", pattern)));
            }
            if ide_lower == "androidstudio" {
                directories.push(home_dir.join(".config/Google"));
            }
        }
        _ => {}
    }

    directories
}

/// Find all actual plugin directories that exist.
pub async fn detect_plugin_directories(ide_name: &str) -> Vec<PathBuf> {
    let mut found_directories = Vec::new();
    let ide_lower = ide_name.to_lowercase();

    let ide_patterns = match IDE_NAME_TO_DIR_MAP.get(ide_lower.as_str()) {
        Some(p) => p,
        None => return found_directories,
    };

    let plugin_dir_paths = build_common_plugin_directory_paths(ide_name);

    // Precompile regexes
    let regexes: Vec<Regex> = ide_patterns
        .iter()
        .filter_map(|p| Regex::new(&format!("^{}", p)).ok())
        .collect();

    let platform = std::env::consts::OS;

    for base_dir in &plugin_dir_paths {
        let entries = match tokio::fs::read_dir(base_dir).await {
            Ok(e) => e,
            Err(_) => continue,
        };

        let mut entries = entries;
        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            let matches_pattern = regexes.iter().any(|re| re.is_match(&name_str));
            if !matches_pattern {
                continue;
            }

            let metadata = match entry.metadata().await {
                Ok(m) => m,
                Err(_) => continue,
            };

            if !metadata.is_dir() && !metadata.file_type().is_symlink() {
                continue;
            }

            let dir = base_dir.join(&name_str.to_string());

            if platform == "linux" {
                found_directories.push(dir);
                continue;
            }

            let plugin_dir = dir.join("plugins");
            if tokio::fs::metadata(&plugin_dir).await.is_ok() {
                found_directories.push(plugin_dir);
            }
        }
    }

    // Deduplicate
    found_directories.sort();
    found_directories.dedup();
    found_directories
}

/// Check if the JetBrains plugin is installed for a given IDE type.
pub async fn is_jetbrains_plugin_installed(ide_type: &str) -> bool {
    let plugin_dirs = detect_plugin_directories(ide_type).await;
    for dir in plugin_dirs {
        let plugin_path = dir.join(PLUGIN_PREFIX);
        if tokio::fs::metadata(&plugin_path).await.is_ok() {
            return true;
        }
    }
    false
}

/// Cached plugin installation status.
static PLUGIN_INSTALLED_CACHE: Lazy<Mutex<HashMap<String, bool>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Check if JetBrains plugin is installed (with memoization).
pub async fn is_jetbrains_plugin_installed_cached(ide_type: &str, force_refresh: bool) -> bool {
    if !force_refresh {
        let cache = PLUGIN_INSTALLED_CACHE.lock().unwrap();
        if let Some(&result) = cache.get(ide_type) {
            return result;
        }
    } else {
        let mut cache = PLUGIN_INSTALLED_CACHE.lock().unwrap();
        cache.remove(ide_type);
    }

    let result = is_jetbrains_plugin_installed(ide_type).await;
    let mut cache = PLUGIN_INSTALLED_CACHE.lock().unwrap();
    cache.insert(ide_type.to_string(), result);
    result
}

/// Returns the cached result synchronously. Returns false if not yet resolved.
pub fn is_jetbrains_plugin_installed_cached_sync(ide_type: &str) -> bool {
    let cache = PLUGIN_INSTALLED_CACHE.lock().unwrap();
    cache.get(ide_type).copied().unwrap_or(false)
}
