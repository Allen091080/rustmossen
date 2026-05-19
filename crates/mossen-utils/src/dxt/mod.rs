//! DXT (Desktop Extension) utilities — translated from utils/dxt/

use std::path::{Path, PathBuf, Component};
use serde::{Deserialize, Serialize};
use anyhow::{Result, bail};
use tracing::debug;

// --- Types ---

/// DXT manifest user config value types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum McpbUserConfigValue {
    String(String),
    Number(f64),
    Boolean(bool),
    StringArray(Vec<String>),
}

pub type McpbUserConfigValues = std::collections::HashMap<String, McpbUserConfigValue>;

/// DXT manifest user configuration option
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpbUserConfigurationOption {
    #[serde(rename = "type")]
    pub option_type: String,
    pub title: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<McpbUserConfigValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multiple: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sensitive: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
}

/// DXT manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpbManifest {
    pub name: String,
    pub version: String,
    pub author: McpbAuthor,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_config: Option<std::collections::HashMap<String, McpbUserConfigurationOption>>,
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpbAuthor {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

// --- Validation Functions ---

/// Parses and validates a DXT manifest from a JSON value.
pub fn validate_manifest(manifest_json: &serde_json::Value) -> Result<McpbManifest> {
    let manifest: McpbManifest = serde_json::from_value(manifest_json.clone())
        .map_err(|e| anyhow::anyhow!("Invalid manifest: {}", e))?;

    // Validate required fields
    if manifest.name.is_empty() {
        bail!("Invalid manifest: name is required");
    }
    if manifest.version.is_empty() {
        bail!("Invalid manifest: version is required");
    }
    if manifest.author.name.is_empty() {
        bail!("Invalid manifest: author.name is required");
    }

    Ok(manifest)
}

/// Parses and validates a DXT manifest from raw text data.
pub fn parse_and_validate_manifest_from_text(manifest_text: &str) -> Result<McpbManifest> {
    let manifest_json: serde_json::Value = serde_json::from_str(manifest_text)
        .map_err(|e| anyhow::anyhow!("Invalid JSON in manifest.json: {}", e))?;
    validate_manifest(&manifest_json)
}

/// Parses and validates a DXT manifest from raw binary data.
pub fn parse_and_validate_manifest_from_bytes(manifest_data: &[u8]) -> Result<McpbManifest> {
    let manifest_text = std::str::from_utf8(manifest_data)
        .map_err(|e| anyhow::anyhow!("Invalid UTF-8 in manifest: {}", e))?;
    parse_and_validate_manifest_from_text(manifest_text)
}

/// Generates an extension ID from author name and extension name.
pub fn generate_extension_id(manifest: &McpbManifest, prefix: Option<&str>) -> String {
    let sanitize = |s: &str| -> String {
        s.to_lowercase()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join("-")
            .chars()
            .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-' || *c == '_' || *c == '.')
            .collect::<String>()
            .trim_matches('-')
            .to_string()
    };

    let sanitized_author = sanitize(&manifest.author.name);
    let sanitized_name = sanitize(&manifest.name);

    match prefix {
        Some(p) => format!("{}.{}.{}", p, sanitized_author, sanitized_name),
        None => format!("{}.{}", sanitized_author, sanitized_name),
    }
}

// --- Zip Utilities ---

/// Limits for zip file validation during extraction
const MAX_FILE_SIZE: u64 = 512 * 1024 * 1024;       // 512MB per file
const MAX_TOTAL_SIZE: u64 = 1024 * 1024 * 1024;     // 1024MB total
const MAX_FILE_COUNT: usize = 100_000;
const MAX_COMPRESSION_RATIO: f64 = 50.0;

/// State tracker for zip file validation during extraction
struct ZipValidationState {
    file_count: usize,
    total_uncompressed_size: u64,
    compressed_size: u64,
}

/// Validates a file path to prevent path traversal attacks
pub fn is_path_safe(file_path: &str) -> bool {
    let path = Path::new(file_path);

    // Check for path traversal
    for component in path.components() {
        if matches!(component, Component::ParentDir) {
            return false;
        }
    }

    // Check for absolute paths
    if path.is_absolute() {
        return false;
    }

    true
}

/// Validate a single file during zip extraction
fn validate_zip_file(
    name: &str,
    original_size: u64,
    state: &mut ZipValidationState,
) -> Result<()> {
    state.file_count += 1;

    // Check file count
    if state.file_count > MAX_FILE_COUNT {
        bail!(
            "Archive contains too many files: {} (max: {})",
            state.file_count,
            MAX_FILE_COUNT
        );
    }

    // Validate path safety
    if !is_path_safe(name) {
        bail!(
            "Unsafe file path detected: \"{}\". Path traversal or absolute paths are not allowed.",
            name
        );
    }

    // Check individual file size
    if original_size > MAX_FILE_SIZE {
        bail!(
            "File \"{}\" is too large: {}MB (max: {}MB)",
            name,
            original_size / 1024 / 1024,
            MAX_FILE_SIZE / 1024 / 1024
        );
    }

    // Track total uncompressed size
    state.total_uncompressed_size += original_size;

    // Check total size
    if state.total_uncompressed_size > MAX_TOTAL_SIZE {
        bail!(
            "Archive total size is too large: {}MB (max: {}MB)",
            state.total_uncompressed_size / 1024 / 1024,
            MAX_TOTAL_SIZE / 1024 / 1024
        );
    }

    // Check compression ratio for zip bomb detection
    if state.compressed_size > 0 {
        let current_ratio = state.total_uncompressed_size as f64 / state.compressed_size as f64;
        if current_ratio > MAX_COMPRESSION_RATIO {
            bail!(
                "Suspicious compression ratio detected: {:.1}:1 (max: {}:1). This may be a zip bomb.",
                current_ratio,
                MAX_COMPRESSION_RATIO
            );
        }
    }

    Ok(())
}

/// Unzip data and return contents as a map of file paths to bytes.
pub fn unzip_data(zip_data: &[u8]) -> Result<std::collections::HashMap<String, Vec<u8>>> {
    use std::io::{Read, Cursor};

    let reader = Cursor::new(zip_data);
    let mut archive = zip::ZipArchive::new(reader)
        .map_err(|e| anyhow::anyhow!("Failed to read zip archive: {}", e))?;

    let mut state = ZipValidationState {
        file_count: 0,
        total_uncompressed_size: 0,
        compressed_size: zip_data.len() as u64,
    };

    let mut result = std::collections::HashMap::new();

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)
            .map_err(|e| anyhow::anyhow!("Failed to read zip entry: {}", e))?;

        let name = file.name().to_string();

        // Skip directories
        if name.ends_with('/') {
            continue;
        }

        validate_zip_file(&name, file.size(), &mut state)?;

        let mut contents = Vec::new();
        file.read_to_end(&mut contents)
            .map_err(|e| anyhow::anyhow!("Failed to read file \"{}\": {}", name, e))?;

        result.insert(name, contents);
    }

    debug!(
        "Zip extraction completed: {} files, {}KB uncompressed",
        state.file_count,
        state.total_uncompressed_size / 1024
    );

    Ok(result)
}

/// Parse Unix file modes from a zip's central directory.
pub fn parse_zip_modes(data: &[u8]) -> std::collections::HashMap<String, u32> {
    let mut modes = std::collections::HashMap::new();

    if data.len() < 22 {
        return modes;
    }

    // Find the End of Central Directory record (sig 0x06054b50)
    let min_eocd = if data.len() > 22 + 0xffff {
        data.len() - 22 - 0xffff
    } else {
        0
    };

    let mut eocd: Option<usize> = None;
    for i in (min_eocd..=(data.len() - 22)).rev() {
        if data[i] == 0x50
            && data[i + 1] == 0x4b
            && data[i + 2] == 0x05
            && data[i + 3] == 0x06
        {
            eocd = Some(i);
            break;
        }
    }

    let eocd = match eocd {
        Some(e) => e,
        None => return modes,
    };

    let entry_count = u16::from_le_bytes([data[eocd + 10], data[eocd + 11]]) as usize;
    let mut off = u32::from_le_bytes([
        data[eocd + 16],
        data[eocd + 17],
        data[eocd + 18],
        data[eocd + 19],
    ]) as usize;

    // Walk central directory entries (sig 0x02014b50)
    for _ in 0..entry_count {
        if off + 46 > data.len() {
            break;
        }
        if data[off] != 0x50 || data[off + 1] != 0x4b || data[off + 2] != 0x01 || data[off + 3] != 0x02 {
            break;
        }

        let version_made_by = u16::from_le_bytes([data[off + 4], data[off + 5]]);
        let name_len = u16::from_le_bytes([data[off + 28], data[off + 29]]) as usize;
        let extra_len = u16::from_le_bytes([data[off + 30], data[off + 31]]) as usize;
        let comment_len = u16::from_le_bytes([data[off + 32], data[off + 33]]) as usize;
        let external_attr = u32::from_le_bytes([
            data[off + 38],
            data[off + 39],
            data[off + 40],
            data[off + 41],
        ]);

        if off + 46 + name_len > data.len() {
            break;
        }
        let name = String::from_utf8_lossy(&data[off + 46..off + 46 + name_len]).to_string();

        // versionMadeBy high byte = host OS. 3 = Unix.
        if version_made_by >> 8 == 3 {
            let mode = (external_attr >> 16) & 0xffff;
            if mode != 0 {
                modes.insert(name, mode);
            }
        }

        off += 46 + name_len + extra_len + comment_len;
    }

    modes
}

/// Reads a zip file from disk and unzips it.
pub async fn read_and_unzip_file(file_path: &Path) -> Result<std::collections::HashMap<String, Vec<u8>>> {
    let zip_data = tokio::fs::read(file_path).await
        .map_err(|e| anyhow::anyhow!("Failed to read file: {}", e))?;
    unzip_data(&zip_data)
}
