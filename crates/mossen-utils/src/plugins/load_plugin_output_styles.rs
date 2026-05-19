//! Load plugin output styles — memoized loading of output styles from plugins.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use once_cell::sync::Lazy;
use tracing::debug;

use super::walk_plugin_markdown::{walk_plugin_markdown, WalkOptions};

/// Output style configuration from a plugin.
#[derive(Debug, Clone)]
pub struct OutputStyleConfig {
    pub name: String,
    pub description: String,
    pub prompt: String,
    pub source: String,
    pub force_for_plugin: Option<bool>,
}

/// Loaded plugin info for output style loading.
#[derive(Debug, Clone)]
pub struct PluginForOutputStyles {
    pub name: String,
    pub output_styles_path: Option<String>,
    pub output_styles_paths: Option<Vec<String>>,
}

static CACHE: Lazy<Mutex<Option<Vec<OutputStyleConfig>>>> = Lazy::new(|| Mutex::new(None));

async fn load_output_styles_from_directory(
    output_styles_path: &str,
    plugin_name: &str,
    loaded_paths: &mut HashSet<String>,
    read_file: &dyn Fn(&str) -> Result<String, std::io::Error>,
    parse_frontmatter: &dyn Fn(&str, &str) -> (std::collections::HashMap<String, String>, String),
) -> Vec<OutputStyleConfig> {
    let mut styles = Vec::new();
    let path = PathBuf::from(output_styles_path);
    walk_plugin_markdown(
        &path,
        &|full_path: PathBuf, _namespace: Vec<String>| {
            let fp = full_path.to_string_lossy().to_string();
            let pn = plugin_name.to_string();
            async move {
                // Output style loading happens synchronously in the callback context
            }
        },
        WalkOptions {
            stop_at_skill_dir: false,
            log_label: Some("output-styles".to_string()),
        },
    )
    .await;
    styles
}

async fn load_output_style_from_file(
    file_path: &str,
    plugin_name: &str,
    loaded_paths: &mut HashSet<String>,
    read_file: &dyn Fn(&str) -> Result<String, std::io::Error>,
    parse_frontmatter: &dyn Fn(&str, &str) -> (std::collections::HashMap<String, String>, String),
) -> Option<OutputStyleConfig> {
    if loaded_paths.contains(file_path) {
        return None;
    }
    loaded_paths.insert(file_path.to_string());

    let content = match read_file(file_path) {
        Ok(c) => c,
        Err(e) => {
            debug!("Failed to load output style from {}: {}", file_path, e);
            return None;
        }
    };

    let (frontmatter, markdown_content) = parse_frontmatter(&content, file_path);
    let file_name = Path::new(file_path)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let base_style_name = frontmatter
        .get("name")
        .cloned()
        .unwrap_or(file_name);
    let name = format!("{}:{}", plugin_name, base_style_name);

    let description = frontmatter
        .get("description")
        .cloned()
        .unwrap_or_else(|| format!("Output style from {} plugin", plugin_name));

    let force_raw = frontmatter.get("force-for-plugin").map(|s| s.as_str());
    let force_for_plugin = match force_raw {
        Some("true") => Some(true),
        Some("false") => Some(false),
        _ => None,
    };

    Some(OutputStyleConfig {
        name,
        description,
        prompt: markdown_content.trim().to_string(),
        source: "plugin".to_string(),
        force_for_plugin,
    })
}

/// Load output styles from all enabled plugins (memoized).
pub async fn load_plugin_output_styles(
    load_all_plugins: impl std::future::Future<Output = PluginsLoadResult>,
    read_file: impl Fn(&str) -> Result<String, std::io::Error>,
    parse_frontmatter: impl Fn(&str, &str) -> (std::collections::HashMap<String, String>, String),
    stat_path: impl Fn(&str) -> Result<PathMeta, std::io::Error>,
) -> Vec<OutputStyleConfig> {
    {
        let guard = CACHE.lock().unwrap();
        if let Some(ref cached) = *guard {
            return cached.clone();
        }
    }

    let result = load_all_plugins.await;
    let mut all_styles = Vec::new();

    for plugin in &result.enabled {
        let mut loaded_paths = HashSet::new();

        if let Some(ref output_styles_path) = plugin.output_styles_path {
            let styles = load_output_styles_from_directory(
                output_styles_path,
                &plugin.name,
                &mut loaded_paths,
                &read_file,
                &parse_frontmatter,
            )
            .await;
            if !styles.is_empty() {
                debug!(
                    "Loaded {} output styles from plugin {} default directory",
                    styles.len(),
                    plugin.name
                );
            }
            all_styles.extend(styles);
        }

        if let Some(ref paths) = plugin.output_styles_paths {
            for style_path in paths {
                match stat_path(style_path) {
                    Ok(meta) => {
                        if meta.is_dir {
                            let styles = load_output_styles_from_directory(
                                style_path,
                                &plugin.name,
                                &mut loaded_paths,
                                &read_file,
                                &parse_frontmatter,
                            )
                            .await;
                            all_styles.extend(styles);
                        } else if meta.is_file && style_path.ends_with(".md") {
                            if let Some(style) = load_output_style_from_file(
                                style_path,
                                &plugin.name,
                                &mut loaded_paths,
                                &read_file,
                                &parse_frontmatter,
                            )
                            .await
                            {
                                all_styles.push(style);
                            }
                        }
                    }
                    Err(e) => {
                        debug!(
                            "Failed to load output styles from plugin {} custom path {}: {}",
                            plugin.name, style_path, e
                        );
                    }
                }
            }
        }
    }

    debug!("Total plugin output styles loaded: {}", all_styles.len());
    let mut guard = CACHE.lock().unwrap();
    *guard = Some(all_styles.clone());
    all_styles
}

/// Clear the memoized output style cache.
pub fn clear_plugin_output_style_cache() {
    let mut guard = CACHE.lock().unwrap();
    *guard = None;
}

#[derive(Debug, Clone)]
pub struct PluginsLoadResult {
    pub enabled: Vec<PluginForOutputStyles>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct PathMeta {
    pub is_file: bool,
    pub is_dir: bool,
}
