//! External-provider model ID helpers.
//!
//! Direct translation of `utils/model/externalProviderIds.ts`. The TS file uses
//! `String.fromCharCode(...)` to construct the literal strings "anthropic" and
//! "claude" at runtime to avoid embedding the literal vendor strings into the
//! source code. We mirror that obfuscation pattern in Rust.

use once_cell::sync::Lazy;
use regex::Regex;

fn external_text(codes: &[u32]) -> String {
    codes
        .iter()
        .filter_map(|c| char::from_u32(*c))
        .collect()
}

static EXTERNAL_VENDOR_ID: Lazy<String> = Lazy::new(|| {
    external_text(&[97, 110, 116, 104, 114, 111, 112, 105, 99])
});

static EXTERNAL_MODEL_PREFIX: Lazy<String> =
    Lazy::new(|| external_text(&[99, 108, 97, 117, 100, 101]));

pub fn external_provider_vendor_id() -> String {
    EXTERNAL_VENDOR_ID.clone()
}

pub fn external_provider_model_prefix() -> String {
    EXTERNAL_MODEL_PREFIX.clone()
}

pub fn external_provider_model_stem(tail: &str) -> String {
    format!("{}-{}", &*EXTERNAL_MODEL_PREFIX, tail)
}

static MOSSEN_1M_2M_SUFFIX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)\[(1|2)m\]$").unwrap());
static MOSSEN_TRAILING_DATE: Lazy<Regex> = Lazy::new(|| Regex::new(r"-\d{8}$").unwrap());

pub fn external_provider_model_stem_from_mossen_id(model_id: &str) -> String {
    let lower = model_id.to_lowercase();
    let stripped_suffix = MOSSEN_1M_2M_SUFFIX.replace(&lower, "").to_string();
    if !stripped_suffix.starts_with("mossen-") {
        return model_id.to_string();
    }
    let tail = &stripped_suffix["mossen-".len()..];
    let tail = MOSSEN_TRAILING_DATE.replace(tail, "").to_string();
    external_provider_model_stem(&tail)
}

pub fn external_provider_messages_route() -> String {
    format!("/{}", &*EXTERNAL_VENDOR_ID)
}

#[derive(Debug, Default, Clone)]
pub struct ExternalBedrockOptions<'a> {
    pub date: Option<&'a str>,
    pub region: Option<&'a str>,
    pub variant: Option<&'a str>,
}

pub fn external_bedrock_model_id(tail: &str, options: ExternalBedrockOptions<'_>) -> String {
    let mut parts: Vec<String> = Vec::new();
    parts.push(external_provider_model_stem(tail));
    if let Some(d) = options.date {
        if !d.is_empty() {
            parts.push(d.to_string());
        }
    }
    if let Some(v) = options.variant {
        if !v.is_empty() {
            parts.push(v.to_string());
        }
    }
    let model_parts = parts.join("-");
    let provider_model_id = format!("{}.{}", &*EXTERNAL_VENDOR_ID, model_parts);
    if let Some(r) = options.region {
        if !r.is_empty() {
            return format!("{}.{}", r, provider_model_id);
        }
    }
    provider_model_id
}

#[derive(Debug, Default, Clone)]
pub struct ExternalVertexOptions<'a> {
    pub date: Option<&'a str>,
    pub variant: Option<&'a str>,
}

pub fn external_vertex_model_id(tail: &str, options: ExternalVertexOptions<'_>) -> String {
    let variant_suffix = options
        .variant
        .filter(|v| !v.is_empty())
        .map(|v| format!("-{}", v))
        .unwrap_or_default();
    let date_suffix = options
        .date
        .filter(|d| !d.is_empty())
        .map(|d| format!("@{}", d))
        .unwrap_or_default();
    format!(
        "{}{}{}",
        external_provider_model_stem(tail),
        variant_suffix,
        date_suffix
    )
}

pub fn external_foundry_model_id(tail: &str) -> String {
    external_provider_model_stem(tail)
}

pub fn external_provider_model_stem_pattern() -> Regex {
    let pattern = format!(
        r"(?:^|[.:/])({}-[a-z0-9]+(?:-[a-z0-9]+)*)",
        regex::escape(&EXTERNAL_MODEL_PREFIX)
    );
    Regex::new(&pattern).expect("valid external provider model stem regex")
}

pub fn extract_external_provider_model_stem(value: &str) -> Option<String> {
    let re = external_provider_model_stem_pattern();
    re.captures(value)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
}

pub fn is_external_bedrock_foundation_model(model_id: &str) -> bool {
    model_id.starts_with(&format!("{}.", &*EXTERNAL_VENDOR_ID))
}

pub fn has_external_provider_vendor_id(value: &str) -> bool {
    value.contains(&*EXTERNAL_VENDOR_ID)
}
