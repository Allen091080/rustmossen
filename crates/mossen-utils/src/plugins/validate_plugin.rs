use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde_json::Value;
use tokio::fs;
use tracing::debug;

/// Validation result for plugin/marketplace manifests.
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub success: bool,
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<ValidationWarning>,
    pub file_path: PathBuf,
    pub file_type: String,
}

#[derive(Debug, Clone)]
pub struct ValidationError {
    pub path: String,
    pub message: String,
    pub code: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ValidationWarning {
    pub path: String,
    pub message: String,
}

/// Marketplace-only fields that don't belong in plugin.json.
static MARKETPLACE_ONLY_MANIFEST_FIELDS: once_cell::sync::Lazy<HashSet<&'static str>> =
    once_cell::sync::Lazy::new(|| ["category", "source", "tags", "strict", "id"].into_iter().collect());

/// Detect whether a file is a plugin manifest or marketplace manifest.
fn detect_manifest_type(file_path: &Path) -> &'static str {
    let file_name = file_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let dir_name = file_path.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str()).unwrap_or("");
    if file_name == "plugin.json" { return "plugin"; }
    if file_name == "marketplace.json" { return "marketplace"; }
    if dir_name == ".mossen-plugin" { return "plugin"; }
    "unknown"
}

/// Check for parent-directory segments in a path string.
fn check_path_traversal(p: &str, field: &str, errors: &mut Vec<ValidationError>, hint: Option<&str>) {
    if p.contains("..") {
        let message = match hint {
            Some(h) => format!("Path contains \"..\": {}. {}", p, h),
            None => format!("Path contains \"..\" which could be a path traversal attempt: {}", p),
        };
        errors.push(ValidationError { path: field.to_string(), message, code: None });
    }
}

fn marketplace_source_hint(p: &str) -> String {
    let stripped = p.trim_start_matches("../");
    let corrected = if stripped != p { format!("./{}", stripped) } else { "./plugins/my-plugin".to_string() };
    format!(
        "Plugin source paths are resolved relative to the marketplace root, not relative to marketplace.json. Use \"{}\" instead of \"{}\".",
        corrected, p
    )
}

/// Validate a plugin manifest file (plugin.json).
pub async fn validate_plugin_manifest(file_path: &Path) -> ValidationResult {
    let absolute_path = file_path.canonicalize().unwrap_or_else(|_| file_path.to_path_buf());
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    // Read file
    let content = match fs::read_to_string(&absolute_path).await {
        Ok(c) => c,
        Err(e) => {
            let message = if e.kind() == std::io::ErrorKind::NotFound {
                format!("File not found: {:?}", absolute_path)
            } else {
                format!("Failed to read file: {}", e)
            };
            return ValidationResult {
                success: false,
                errors: vec![ValidationError { path: "file".to_string(), message, code: Some(format!("{:?}", e.kind())) }],
                warnings: vec![],
                file_path: absolute_path,
                file_type: "plugin".to_string(),
            };
        }
    };

    // Parse JSON
    let parsed: Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            return ValidationResult {
                success: false,
                errors: vec![ValidationError { path: "json".to_string(), message: format!("Invalid JSON syntax: {}", e), code: None }],
                warnings: vec![],
                file_path: absolute_path,
                file_type: "plugin".to_string(),
            };
        }
    };

    // Check path traversal
    if let Some(obj) = parsed.as_object() {
        if let Some(commands) = obj.get("commands") {
            if let Some(arr) = commands.as_array() {
                for (i, cmd) in arr.iter().enumerate() {
                    if let Some(s) = cmd.as_str() {
                        check_path_traversal(s, &format!("commands[{}]", i), &mut errors, None);
                    }
                }
            }
        }
        if let Some(agents) = obj.get("agents") {
            if let Some(arr) = agents.as_array() {
                for (i, agent) in arr.iter().enumerate() {
                    if let Some(s) = agent.as_str() {
                        check_path_traversal(s, &format!("agents[{}]", i), &mut errors, None);
                    }
                }
            }
        }
        if let Some(skills) = obj.get("skills") {
            if let Some(arr) = skills.as_array() {
                for (i, skill) in arr.iter().enumerate() {
                    if let Some(s) = skill.as_str() {
                        check_path_traversal(s, &format!("skills[{}]", i), &mut errors, None);
                    }
                }
            }
        }

        // Check marketplace-only fields
        let stray_keys: Vec<&String> = obj.keys().filter(|k| MARKETPLACE_ONLY_MANIFEST_FIELDS.contains(k.as_str())).collect();
        for key in &stray_keys {
            warnings.push(ValidationWarning {
                path: key.to_string(),
                message: format!("Field '{}' belongs in the marketplace entry (marketplace.json), not plugin.json.", key),
            });
        }

        // Validate required fields
        if obj.get("name").and_then(|v| v.as_str()).is_none() {
            errors.push(ValidationError { path: "name".to_string(), message: "Missing required field: name".to_string(), code: None });
        } else {
            let name = obj.get("name").unwrap().as_str().unwrap();
            if !regex::Regex::new(r"^[a-z0-9]+(-[a-z0-9]+)*$").unwrap().is_match(name) {
                warnings.push(ValidationWarning {
                    path: "name".to_string(),
                    message: format!("Plugin name \"{}\" is not kebab-case.", name),
                });
            }
        }

        if obj.get("version").is_none() {
            warnings.push(ValidationWarning { path: "version".to_string(), message: "No version specified.".to_string() });
        }
        if obj.get("description").is_none() {
            warnings.push(ValidationWarning { path: "description".to_string(), message: "No description provided.".to_string() });
        }
        if obj.get("author").is_none() {
            warnings.push(ValidationWarning { path: "author".to_string(), message: "No author information provided.".to_string() });
        }
    }

    ValidationResult {
        success: errors.is_empty(),
        errors,
        warnings,
        file_path: absolute_path,
        file_type: "plugin".to_string(),
    }
}

/// Validate a marketplace manifest file (marketplace.json).
pub async fn validate_marketplace_manifest(file_path: &Path) -> ValidationResult {
    let absolute_path = file_path.canonicalize().unwrap_or_else(|_| file_path.to_path_buf());
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    let content = match fs::read_to_string(&absolute_path).await {
        Ok(c) => c,
        Err(e) => {
            let message = if e.kind() == std::io::ErrorKind::NotFound {
                format!("File not found: {:?}", absolute_path)
            } else {
                format!("Failed to read file: {}", e)
            };
            return ValidationResult {
                success: false,
                errors: vec![ValidationError { path: "file".to_string(), message, code: None }],
                warnings: vec![],
                file_path: absolute_path,
                file_type: "marketplace".to_string(),
            };
        }
    };

    let parsed: Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            return ValidationResult {
                success: false,
                errors: vec![ValidationError { path: "json".to_string(), message: format!("Invalid JSON syntax: {}", e), code: None }],
                warnings: vec![],
                file_path: absolute_path,
                file_type: "marketplace".to_string(),
            };
        }
    };

    // Check path traversal in plugin sources
    if let Some(obj) = parsed.as_object() {
        if let Some(plugins) = obj.get("plugins").and_then(|v| v.as_array()) {
            for (i, plugin) in plugins.iter().enumerate() {
                if let Some(source) = plugin.get("source") {
                    if let Some(s) = source.as_str() {
                        let hint = marketplace_source_hint(s);
                        check_path_traversal(s, &format!("plugins[{}].source", i), &mut errors, Some(&hint));
                    }
                    if let Some(obj_source) = source.as_object() {
                        if let Some(path) = obj_source.get("path").and_then(|v| v.as_str()) {
                            check_path_traversal(path, &format!("plugins[{}].source.path", i), &mut errors, None);
                        }
                    }
                }
            }
        }

        // Validate structure
        if let Some(plugins) = obj.get("plugins").and_then(|v| v.as_array()) {
            if plugins.is_empty() {
                warnings.push(ValidationWarning { path: "plugins".to_string(), message: "Marketplace has no plugins defined".to_string() });
            }
            // Check duplicate names
            let mut names_seen: HashSet<String> = HashSet::new();
            for (i, plugin) in plugins.iter().enumerate() {
                if let Some(name) = plugin.get("name").and_then(|v| v.as_str()) {
                    if !names_seen.insert(name.to_string()) {
                        errors.push(ValidationError {
                            path: format!("plugins[{}].name", i),
                            message: format!("Duplicate plugin name \"{}\" found in marketplace", name),
                            code: None,
                        });
                    }
                }
            }
        }

        if obj.get("metadata").and_then(|v| v.get("description")).is_none() {
            warnings.push(ValidationWarning {
                path: "metadata.description".to_string(),
                message: "No marketplace description provided.".to_string(),
            });
        }
    }

    ValidationResult {
        success: errors.is_empty(),
        errors,
        warnings,
        file_path: absolute_path,
        file_type: "marketplace".to_string(),
    }
}

/// Validate a hooks configuration file.
pub async fn validate_hooks_config(file_path: &Path) -> ValidationResult {
    let absolute_path = file_path.canonicalize().unwrap_or_else(|_| file_path.to_path_buf());
    let content = match fs::read_to_string(&absolute_path).await {
        Ok(c) => c,
        Err(e) => {
            return ValidationResult {
                success: false,
                errors: vec![ValidationError { path: "file".to_string(), message: format!("Failed to read: {}", e), code: None }],
                warnings: vec![],
                file_path: absolute_path,
                file_type: "hooks".to_string(),
            };
        }
    };
    let parsed: Result<Value, _> = serde_json::from_str(&content);
    match parsed {
        Ok(_) => ValidationResult { success: true, errors: vec![], warnings: vec![], file_path: absolute_path, file_type: "hooks".to_string() },
        Err(e) => ValidationResult {
            success: false,
            errors: vec![ValidationError { path: "json".to_string(), message: format!("Invalid JSON: {}", e), code: None }],
            warnings: vec![],
            file_path: absolute_path,
            file_type: "hooks".to_string(),
        },
    }
}

/// Auto-detect and validate the appropriate file type.
pub async fn validate_plugin_file(file_path: &Path) -> ValidationResult {
    let manifest_type = detect_manifest_type(file_path);
    match manifest_type {
        "plugin" => validate_plugin_manifest(file_path).await,
        "marketplace" => validate_marketplace_manifest(file_path).await,
        _ => {
            // Try to detect from content
            validate_plugin_manifest(file_path).await
        }
    }
}

/// 对应 TS `validatePluginContents`：校验 plugin 目录内的清单与文件结构。
pub async fn validate_plugin_contents(plugin_dir: &str) -> Vec<String> {
    let mut errors = Vec::new();
    let manifest_path = Path::new(plugin_dir).join("plugin.json");
    let content = match fs::read_to_string(&manifest_path).await {
        Ok(c) => c,
        Err(e) => {
            errors.push(format!("missing plugin.json: {}", e));
            return errors;
        }
    };
    let value: Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            errors.push(format!("invalid plugin.json: {}", e));
            return errors;
        }
    };
    if value.get("name").and_then(|v| v.as_str()).is_none() {
        errors.push("plugin.json missing `name`".to_string());
    }
    if value.get("version").and_then(|v| v.as_str()).is_none() {
        errors.push("plugin.json missing `version`".to_string());
    }
    errors
}
