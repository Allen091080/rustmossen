// output_styles.rs — Translation of outputStyles/loadOutputStylesDir.ts

use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputStyleConfig {
    pub name: String,
    pub description: String,
    pub prompt: String,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_coding_instructions: Option<bool>,
}

/// Load output styles from .mossen/output-styles directories throughout the project
/// and from ~/.mossen/output-styles directory.
pub async fn get_output_style_dir_styles(cwd: &Path) -> Vec<OutputStyleConfig> {
    let mut styles = Vec::new();

    // Load from user config dir
    let config_dir = mossen_utils::env::get_mossen_config_home_dir();
    let user_styles_dir = config_dir.join("output-styles");
    if let Ok(entries) = load_markdown_styles(&user_styles_dir).await {
        for style in entries {
            styles.push(style);
        }
    }

    // Load from project .mossen/output-styles (overrides user styles)
    let project_styles_dir = cwd.join(".mossen").join("output-styles");
    if let Ok(entries) = load_markdown_styles(&project_styles_dir).await {
        // Override user styles with project styles
        for style in entries {
            if let Some(existing) = styles.iter_mut().find(|s| s.name == style.name) {
                *existing = style;
            } else {
                styles.push(style);
            }
        }
    }

    styles
}

async fn load_markdown_styles(dir: &Path) -> Result<Vec<OutputStyleConfig>, std::io::Error> {
    let mut styles = Vec::new();
    let mut entries = tokio::fs::read_dir(dir).await?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.extension().map(|e| e == "md").unwrap_or(false) {
            if let Ok(content) = tokio::fs::read_to_string(&path).await {
                let file_name = path.file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();

                let (frontmatter, body) = parse_frontmatter_simple(&content);

                let name = frontmatter.get("name")
                    .cloned()
                    .unwrap_or_else(|| file_name.clone());

                let description = frontmatter.get("description")
                    .cloned()
                    .unwrap_or_else(|| format!("Custom {} output style", file_name));

                let keep_coding_instructions = frontmatter.get("keep-coding-instructions")
                    .and_then(|v| match v.as_str() {
                        "true" => Some(true),
                        "false" => Some(false),
                        _ => None,
                    });

                let source = dir.to_string_lossy().to_string();

                styles.push(OutputStyleConfig {
                    name,
                    description,
                    prompt: body.trim().to_string(),
                    source,
                    keep_coding_instructions,
                });
            }
        }
    }

    Ok(styles)
}

fn parse_frontmatter_simple(content: &str) -> (std::collections::HashMap<String, String>, String) {
    let mut frontmatter = std::collections::HashMap::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut body_start = 0;
    let mut in_frontmatter = false;

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed == "---" {
            if !in_frontmatter {
                in_frontmatter = true;
                continue;
            } else {
                body_start = i + 1;
                break;
            }
        }
        if in_frontmatter {
            if let Some(colon_pos) = trimmed.find(':') {
                let key = trimmed[..colon_pos].trim().to_string();
                let value = trimmed[colon_pos + 1..].trim().to_string();
                frontmatter.insert(key, value);
            }
        }
    }

    let body = if body_start < lines.len() {
        lines[body_start..].join("\n")
    } else {
        content.to_string()
    };

    (frontmatter, body)
}

/// Clear all output style caches.
pub fn clear_output_style_caches() {
    // In Rust, caching is handled differently.
    // This function exists for API compatibility.
}
