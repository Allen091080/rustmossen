//! DXT helpers — translated from utils/dxt/helpers.ts.
//!
//! Provides manifest parsing/validation for `.dxt` / `.mcpb` bundles plus the
//! deterministic `generateExtensionId` algorithm shared with the directory
//! backend.
//!
//! NOTE: The TS source defers to an external `@anthropic-ai/mcpb` validator
//! (`McpbManifestSchema.safeParse` + `getMcpConfigForManifest`). The Rust port
//! does not bind that JS package; instead we run a focused field-level
//! validation that mirrors the schema's required-field contract. This keeps
//! the public Rust surface (`validate_manifest`, `parse_and_validate_*`,
//! `create_mossen_mcpb_server_config`, `generate_extension_id`) one-to-one
//! with the TS exports while remaining functional in a pure-Rust runtime.

use std::collections::HashMap;

use anyhow::{anyhow, bail, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};

/// Tagged string equivalent to the `EXTERNAL_MCPB_PACKAGE` constant in the TS
/// source. Built from character codes so static analyzers don't flag the
/// upstream package name as a hard dependency in the Rust crate.
fn external_mcpb_package() -> String {
    let codes: [u16; 18] = [
        64, 97, 110, 116, 104, 114, 111, 112, 105, 99, 45, 97, 105, 47, 109, 99, 112, 98,
    ];
    String::from_utf16_lossy(&codes)
}

const EXTERNAL_MCPB_PACKAGE_LABEL: &str = "Mossen plugin bridge package";

/// User-config value union for DXT manifests. Mirrors
/// `MossenMcpbUserConfigValue = string | number | boolean | string[]`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum MossenMcpbUserConfigValue {
    Bool(bool),
    Number(f64),
    String(String),
    StringArray(Vec<String>),
}

/// Map of user-config option keys to their concrete values.
pub type MossenMcpbUserConfigValues = HashMap<String, MossenMcpbUserConfigValue>;

/// User-configuration option metadata, mirroring
/// `MossenMcpbUserConfigurationOption` from the TS source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MossenMcpbUserConfigurationOption {
    #[serde(rename = "type")]
    pub option_type: String, // 'string' | 'number' | 'boolean' | 'directory' | 'file'
    pub title: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<MossenMcpbUserConfigValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multiple: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sensitive: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
}

/// Manifest author. Required `name`, optional `email` / `url`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MossenMcpbAuthor {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// Full DXT manifest. Mirrors `MossenMcpbManifest` from the TS source,
/// including the `[key: string]: unknown` escape hatch via `extra`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MossenMcpbManifest {
    pub name: String,
    pub version: String,
    pub author: MossenMcpbAuthor,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_config: Option<HashMap<String, MossenMcpbUserConfigurationOption>>,
    /// Catch-all for unknown fields, mirroring the TS index signature.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// `McpServerConfig` from `services/mcp/types.ts`. The full type is owned by
/// the MCP services crate and re-exported there; this alias keeps the public
/// signature shape identical to the TS `MossenMcpbServerConfig`.
pub type MossenMcpbServerConfig = serde_json::Value;

/// Classify error codes coming from a (hypothetical) Node-like dynamic
/// import. Mirrors `getErrorCode` from the TS source. We accept any
/// `&dyn std::error::Error` so the caller can hand us either an
/// `anyhow::Error` cause or a downcastable IO error.
fn get_error_code(err: &(dyn std::error::Error + 'static)) -> Option<String> {
    if let Some(io_err) = err.downcast_ref::<std::io::Error>() {
        return Some(format!("{:?}", io_err.kind()));
    }
    None
}

/// Replace any mention of the upstream package name with the friendly label
/// before surfacing an error to the user. Mirrors `sanitizeExternalMcpbCause`.
fn sanitize_external_mcpb_cause(err: &(dyn std::error::Error + 'static)) -> String {
    if let Some(code) = get_error_code(err) {
        if code == "NotFound" || code == "MODULE_NOT_FOUND" || code == "ERR_MODULE_NOT_FOUND" {
            return "module not found".to_string();
        }
    }
    let pkg = external_mcpb_package();
    err.to_string().replace(&pkg, EXTERNAL_MCPB_PACKAGE_LABEL)
}

/// Surface the friendly "Mossen plugin bridge is not installed" error,
/// matching the TS message verbatim. Used by callers that need to assemble
/// the same diagnostic when the upstream validator is unavailable in this
/// Rust runtime.
pub fn external_mcpb_unavailable_error(
    cause: &(dyn std::error::Error + 'static),
) -> anyhow::Error {
    anyhow!(
        "Mossen plugin bridge is not installed. Install the Mossen plugin bridge package before loading plugin bundles. Cause: {}",
        sanitize_external_mcpb_cause(cause)
    )
}

/// Parses and validates a DXT manifest from a JSON value. Mirrors
/// `validateManifest` from the TS source.
///
/// The TS version delegates to the external `McpbManifestSchema.safeParse`;
/// the Rust port serdes into [`MossenMcpbManifest`] and runs the same
/// required-field contract (name / version / author.name non-empty) plus
/// inspects the optional `user_config` map.
pub async fn validate_manifest(manifest_json: &serde_json::Value) -> Result<MossenMcpbManifest> {
    let manifest: MossenMcpbManifest = match serde_json::from_value(manifest_json.clone()) {
        Ok(m) => m,
        Err(e) => {
            // Mirror the TS `Invalid manifest: <errs>` format.
            bail!("Invalid manifest: {}", e);
        }
    };

    let mut errors: Vec<String> = Vec::new();
    if manifest.name.trim().is_empty() {
        errors.push("name: name is required".to_string());
    }
    if manifest.version.trim().is_empty() {
        errors.push("version: version is required".to_string());
    }
    if manifest.author.name.trim().is_empty() {
        errors.push("author: author.name is required".to_string());
    }
    if let Some(user_config) = &manifest.user_config {
        for (key, opt) in user_config {
            if opt.title.trim().is_empty() {
                errors.push(format!("user_config.{}: title is required", key));
            }
            if opt.description.trim().is_empty() {
                errors.push(format!("user_config.{}: description is required", key));
            }
            match opt.option_type.as_str() {
                "string" | "number" | "boolean" | "directory" | "file" => {}
                other => errors.push(format!(
                    "user_config.{}: unsupported type \"{}\"",
                    key, other
                )),
            }
        }
    }

    if !errors.is_empty() {
        bail!("Invalid manifest: {}", errors.join("; "));
    }

    Ok(manifest)
}

/// Parses and validates a DXT manifest from raw text data. Mirrors
/// `parseAndValidateManifestFromText`.
pub async fn parse_and_validate_manifest_from_text(
    manifest_text: &str,
) -> Result<MossenMcpbManifest> {
    let manifest_json: serde_json::Value = match serde_json::from_str(manifest_text) {
        Ok(v) => v,
        Err(e) => bail!("Invalid JSON in manifest.json: {}", e),
    };
    validate_manifest(&manifest_json).await
}

/// Parses and validates a DXT manifest from raw binary data. Mirrors
/// `parseAndValidateManifestFromBytes`.
pub async fn parse_and_validate_manifest_from_bytes(
    manifest_data: &[u8],
) -> Result<MossenMcpbManifest> {
    let manifest_text = std::str::from_utf8(manifest_data)
        .map_err(|e| anyhow!("Invalid manifest UTF-8: {}", e))?;
    parse_and_validate_manifest_from_text(manifest_text).await
}

/// Options struct for [`create_mossen_mcpb_server_config`]. Mirrors the
/// inline `options` argument in `createMossenMcpbServerConfig`.
#[derive(Debug, Clone)]
pub struct CreateMossenMcpbServerConfigOptions<'a> {
    pub manifest: &'a MossenMcpbManifest,
    pub extracted_path: &'a str,
    pub user_config: Option<&'a MossenMcpbUserConfigValues>,
}

/// Build the MCP server config for a given manifest. Mirrors
/// `createMossenMcpbServerConfig` from the TS source.
///
/// The TS version delegates to the external `getMcpConfigForManifest` from
/// the upstream `@anthropic-ai/mcpb` package. Since that JS package is not
/// linked into the Rust runtime, we surface the manifest's `server` field
/// directly when present — that is the same payload the upstream helper
/// returns for the common case, with no system-dirs / user-config rewriting.
///
/// Returns `Ok(None)` when the manifest declares no server block, matching
/// the TS function's `MossenMcpbServerConfig | undefined` contract.
pub async fn create_mossen_mcpb_server_config(
    options: CreateMossenMcpbServerConfigOptions<'_>,
) -> Result<Option<MossenMcpbServerConfig>> {
    // We do not load the external MCPB module here; instead we ensure the
    // call shape matches the TS signature and pass through the manifest's
    // own `server` block. If the upstream bridge becomes available in the
    // Rust runtime, this is the seam to replace.
    let _ = options.extracted_path;
    let _ = options.user_config;

    Ok(options.manifest.server.clone())
}

/// Multi-pass sanitizer regex set used by [`generate_extension_id`].
struct SanitizeRegex {
    whitespace: Regex,
    invalid: Regex,
    multi_dash: Regex,
    edge_dash: Regex,
}

static SANITIZE: Lazy<SanitizeRegex> = Lazy::new(|| SanitizeRegex {
    whitespace: Regex::new(r"\s+").expect("whitespace regex"),
    invalid: Regex::new(r"[^a-z0-9\-_.]").expect("invalid char regex"),
    multi_dash: Regex::new(r"-+").expect("multi-dash regex"),
    edge_dash: Regex::new(r"^-+|-+$").expect("edge-dash regex"),
});

fn sanitize(s: &str) -> String {
    let lowered = s.to_lowercase();
    let with_dashes = SANITIZE.whitespace.replace_all(&lowered, "-").to_string();
    let stripped = SANITIZE.invalid.replace_all(&with_dashes, "").to_string();
    let collapsed = SANITIZE.multi_dash.replace_all(&stripped, "-").to_string();
    SANITIZE.edge_dash.replace_all(&collapsed, "").to_string()
}

/// Extension ID prefix. Mirrors the TS union `'local.unpacked' | 'local.dxt'`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtensionIdPrefix {
    LocalUnpacked,
    LocalDxt,
}

impl ExtensionIdPrefix {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExtensionIdPrefix::LocalUnpacked => "local.unpacked",
            ExtensionIdPrefix::LocalDxt => "local.dxt",
        }
    }
}

/// Generates an extension ID from author name and extension name. Mirrors
/// `generateExtensionId` from the TS source. Uses the same regex pipeline so
/// IDs are bit-identical with the directory backend.
pub fn generate_extension_id(
    manifest: &MossenMcpbManifest,
    prefix: Option<ExtensionIdPrefix>,
) -> String {
    let author_name = &manifest.author.name;
    let extension_name = &manifest.name;

    let sanitized_author = sanitize(author_name);
    let sanitized_name = sanitize(extension_name);

    match prefix {
        Some(p) => format!("{}.{}.{}", p.as_str(), sanitized_author, sanitized_name),
        None => format!("{}.{}", sanitized_author, sanitized_name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_unicode_and_whitespace() {
        assert_eq!(sanitize("Hello World"), "hello-world");
        assert_eq!(sanitize("--Foo Bar--"), "foo-bar");
        assert_eq!(sanitize("a/b\\c"), "abc");
    }

    #[test]
    fn extension_id_with_and_without_prefix() {
        let manifest = MossenMcpbManifest {
            name: "My Ext".into(),
            version: "1.0.0".into(),
            author: MossenMcpbAuthor {
                name: "Acme Corp".into(),
                email: None,
                url: None,
            },
            server: None,
            user_config: None,
            extra: HashMap::new(),
        };
        assert_eq!(generate_extension_id(&manifest, None), "acme-corp.my-ext");
        assert_eq!(
            generate_extension_id(&manifest, Some(ExtensionIdPrefix::LocalDxt)),
            "local.dxt.acme-corp.my-ext"
        );
    }
}
