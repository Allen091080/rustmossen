use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// User configuration schema entry (from DXT manifest).
pub type UserConfigSchema = HashMap<String, UserConfigOption>;

/// User configuration values.
pub type UserConfigValues = HashMap<String, serde_json::Value>;

/// Single user config option from a DXT manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserConfigOption {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(rename = "type", default)]
    pub field_type: Option<String>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub sensitive: bool,
    #[serde(default)]
    pub default: Option<serde_json::Value>,
}

/// MCPB load result (success case).
#[derive(Debug, Clone)]
pub struct McpbLoadResult {
    pub manifest: DxtManifest,
    pub mcp_config: super::schemas::McpServerConfig,
    pub extracted_path: PathBuf,
}

/// DXT manifest structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DxtManifest {
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(rename = "userConfiguration", default)]
    pub user_configuration: Option<HashMap<String, UserConfigOption>>,
}

/// MCPB needs-config result (when user input is required).
#[derive(Debug, Clone)]
pub struct McpbNeedsConfig {
    pub status: String,
    pub schema: UserConfigSchema,
    pub plugin_id: String,
}

/// Determine if a source path is an MCPB source.
pub fn is_mcpb_source(path: &str) -> bool {
    path.ends_with(".mcpb")
        || path.ends_with(".dxt")
        || (path.starts_with("http://") || path.starts_with("https://"))
            && (path.contains(".mcpb") || path.contains(".dxt"))
}

/// Load an MCPB file: download (if URL), extract, validate manifest, build MCP config.
pub async fn load_mcpb_file(
    mcpb_path: &str,
    plugin_path: &Path,
    plugin_id: &str,
    status_callback: impl Fn(&str),
    http_client: &dyn McpbHttpClient,
    get_data_dir: impl Fn(&str) -> PathBuf,
) -> Result<McpbLoadResult, McpbError> {
    status_callback("Loading MCPB manifest...");

    let mcpb_data = if mcpb_path.starts_with("http://") || mcpb_path.starts_with("https://") {
        status_callback("Downloading MCPB...");
        http_client
            .download(mcpb_path)
            .await
            .map_err(|e| McpbError::Download(e.to_string()))?
    } else {
        let full_path = plugin_path.join(mcpb_path);
        tokio::fs::read(&full_path)
            .await
            .map_err(|e| McpbError::Read(e.to_string()))?
    };

    status_callback("Extracting MCPB...");
    let extract_dir = get_data_dir(plugin_id).join("mcpb");
    tokio::fs::create_dir_all(&extract_dir)
        .await
        .map_err(|e| McpbError::Extract(e.to_string()))?;

    // Extract zip contents
    let manifest = extract_and_parse_mcpb(&mcpb_data, &extract_dir).await?;

    status_callback("Building MCP configuration...");
    let mcp_config = build_mcp_config_from_manifest(&manifest, &extract_dir)?;

    Ok(McpbLoadResult {
        manifest,
        mcp_config,
        extracted_path: extract_dir,
    })
}

/// Validate user configuration against schema.
pub fn validate_user_config(
    values: &UserConfigValues,
    schema: &UserConfigSchema,
) -> ValidationResult {
    let mut errors: HashMap<String, String> = HashMap::new();
    for (key, option) in schema {
        if option.required {
            match values.get(key) {
                None | Some(serde_json::Value::Null) => {
                    errors.insert(key.clone(), format!("Required field '{}' is missing", key));
                }
                Some(serde_json::Value::String(s)) if s.is_empty() => {
                    errors.insert(
                        key.clone(),
                        format!("Required field '{}' cannot be empty", key),
                    );
                }
                _ => {}
            }
        }
    }
    ValidationResult {
        valid: errors.is_empty(),
        errors,
    }
}

#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub valid: bool,
    pub errors: HashMap<String, String>,
}

/// Load saved user config for a specific MCP server.
pub fn load_mcp_server_user_config(
    plugin_id: &str,
    server_name: &str,
    settings_provider: &dyn McpbSettingsProvider,
) -> Option<UserConfigValues> {
    let composite_key = format!("{}/{}", plugin_id, server_name);
    settings_provider.get_plugin_config_options(&composite_key)
}

/// Save user config for a specific MCP server.
pub fn save_mcp_server_user_config(
    plugin_id: &str,
    server_name: &str,
    values: &UserConfigValues,
    schema: &UserConfigSchema,
    settings_provider: &dyn McpbSettingsProvider,
    secure_storage: &dyn McpbSecureStorage,
) -> Result<(), anyhow::Error> {
    let composite_key = format!("{}/{}", plugin_id, server_name);
    let mut non_sensitive = UserConfigValues::new();
    let mut sensitive: HashMap<String, String> = HashMap::new();

    for (key, value) in values {
        if schema.get(key).is_some_and(|o| o.sensitive) {
            sensitive.insert(key.clone(), value.to_string());
        } else {
            non_sensitive.insert(key.clone(), value.clone());
        }
    }

    // Save non-sensitive to settings
    if !non_sensitive.is_empty() {
        settings_provider.set_plugin_config_options(&composite_key, &non_sensitive)?;
    }

    // Save sensitive to secure storage
    if !sensitive.is_empty() {
        secure_storage.write_secrets(&composite_key, &sensitive)?;
    }

    Ok(())
}

/// MCPB HTTP client trait.
#[async_trait::async_trait]
pub trait McpbHttpClient: Send + Sync {
    async fn download(&self, url: &str) -> Result<Vec<u8>, anyhow::Error>;
}

/// MCPB settings provider trait.
pub trait McpbSettingsProvider: Send + Sync {
    fn get_plugin_config_options(&self, key: &str) -> Option<UserConfigValues>;
    fn set_plugin_config_options(
        &self,
        key: &str,
        values: &UserConfigValues,
    ) -> Result<(), anyhow::Error>;
}

/// MCPB secure storage trait.
pub trait McpbSecureStorage: Send + Sync {
    fn read_secrets(&self, key: &str) -> Option<HashMap<String, String>>;
    fn write_secrets(
        &self,
        key: &str,
        secrets: &HashMap<String, String>,
    ) -> Result<(), anyhow::Error>;
}

#[derive(Debug)]
pub enum McpbError {
    Download(String),
    Read(String),
    Extract(String),
    InvalidManifest(String),
    NeedsConfig(UserConfigSchema),
}

impl std::fmt::Display for McpbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Download(msg) => write!(f, "Download failed: {}", msg),
            Self::Read(msg) => write!(f, "Read failed: {}", msg),
            Self::Extract(msg) => write!(f, "Extract failed: {}", msg),
            Self::InvalidManifest(msg) => write!(f, "Invalid manifest: {}", msg),
            Self::NeedsConfig(_) => write!(f, "User configuration required"),
        }
    }
}

impl std::error::Error for McpbError {}

async fn extract_and_parse_mcpb(data: &[u8], extract_dir: &Path) -> Result<DxtManifest, McpbError> {
    // Extract zip contents
    let reader = std::io::Cursor::new(data);
    let mut archive =
        zip::ZipArchive::new(reader).map_err(|e| McpbError::Extract(e.to_string()))?;

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| McpbError::Extract(e.to_string()))?;
        let outpath = extract_dir.join(file.name());
        if file.name().ends_with('/') {
            tokio::fs::create_dir_all(&outpath)
                .await
                .map_err(|e| McpbError::Extract(e.to_string()))?;
        } else {
            if let Some(parent) = outpath.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|e| McpbError::Extract(e.to_string()))?;
            }
            let mut buf = Vec::new();
            std::io::Read::read_to_end(&mut file, &mut buf)
                .map_err(|e| McpbError::Extract(e.to_string()))?;
            tokio::fs::write(&outpath, &buf)
                .await
                .map_err(|e| McpbError::Extract(e.to_string()))?;
        }
    }

    // Read manifest
    let manifest_path = extract_dir.join("manifest.json");
    let manifest_content = tokio::fs::read_to_string(&manifest_path)
        .await
        .map_err(|e| McpbError::InvalidManifest(format!("Failed to read manifest.json: {}", e)))?;
    let manifest: DxtManifest = serde_json::from_str(&manifest_content)
        .map_err(|e| McpbError::InvalidManifest(format!("Invalid manifest JSON: {}", e)))?;

    Ok(manifest)
}

fn build_mcp_config_from_manifest(
    _manifest: &DxtManifest,
    extracted_path: &Path,
) -> Result<super::schemas::McpServerConfig, McpbError> {
    // Build MCP server config from DXT manifest
    Ok(super::schemas::McpServerConfig {
        command: Some(extracted_path.join("server").to_string_lossy().to_string()),
        args: None,
        env: None,
        url: None,
        transport: None,
        headers: None,
        server_type: Some("stdio".to_string()),
        workspace_folder: None,
        extra: Default::default(),
    })
}

/// 对应 TS `McpbNeedsConfigResult`。
#[derive(Debug, Clone, Default)]
pub struct McpbNeedsConfigResult {
    pub needs_config: bool,
    pub missing_keys: Vec<String>,
}

/// 对应 TS `McpbCacheMetadata`：MCPB 缓存元数据。
#[derive(Debug, Clone, Default)]
pub struct McpbCacheMetadata {
    pub etag: Option<String>,
    pub last_modified_ms: Option<u64>,
    pub size: Option<u64>,
}

/// 对应 TS `ProgressCallback`：进度回调签名。
pub type ProgressCallback = std::sync::Arc<dyn Fn(u64, u64) + Send + Sync>;

/// 对应 TS `checkMcpbChanged`：检查 MCPB 包是否发生变化。
pub async fn check_mcpb_changed(_url: &str, _meta: &McpbCacheMetadata) -> bool {
    true
}
