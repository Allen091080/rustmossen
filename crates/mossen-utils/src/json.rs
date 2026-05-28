//! JSON parsing and JSONL utilities.
//!
//! Provides safe JSON parsing (with BOM stripping), JSONL parsing, and
//! JSONL file reading with tail-read support for large files.

use std::path::Path;

use serde::de::DeserializeOwned;
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncSeekExt, SeekFrom};

// ---------------------------------------------------------------------------
// BOM handling
// ---------------------------------------------------------------------------

/// Strip UTF-8 BOM (EF BB BF) from the beginning of a string.
pub fn strip_bom(s: &str) -> &str {
    s.strip_prefix('\u{FEFF}').unwrap_or(s)
}

// ---------------------------------------------------------------------------
// Safe JSON parsing
// ---------------------------------------------------------------------------

/// Safely parse a JSON string. Returns `None` on invalid input or empty string.
/// Strips BOM before parsing.
pub fn safe_parse_json<T: DeserializeOwned>(json: &str) -> Option<T> {
    if json.is_empty() {
        return None;
    }
    let clean = strip_bom(json);
    serde_json::from_str(clean).ok()
}

/// Safely parse a JSON string into a generic `serde_json::Value`.
pub fn safe_parse_json_value(json: &str) -> Option<serde_json::Value> {
    if json.is_empty() {
        return None;
    }
    let clean = strip_bom(json);
    serde_json::from_str(clean).ok()
}

// ---------------------------------------------------------------------------
// JSONL parsing
// ---------------------------------------------------------------------------

/// Parses JSONL data from a string, skipping malformed lines.
pub fn parse_jsonl<T: DeserializeOwned>(data: &str) -> Vec<T> {
    let clean = strip_bom(data);
    let mut results = Vec::new();
    for line in clean.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<T>(trimmed) {
            results.push(value);
        }
        // Skip malformed lines silently
    }
    results
}

/// Parses JSONL data from bytes, skipping malformed lines.
pub fn parse_jsonl_bytes<T: DeserializeOwned>(data: &[u8]) -> Vec<T> {
    // Strip UTF-8 BOM bytes (EF BB BF)
    let start = if data.len() >= 3 && data[0] == 0xEF && data[1] == 0xBB && data[2] == 0xBF {
        3
    } else {
        0
    };
    let data = &data[start..];

    let mut results = Vec::new();
    for line in data.split(|&b| b == b'\n') {
        let trimmed = match std::str::from_utf8(line) {
            Ok(s) => s.trim(),
            Err(_) => continue,
        };
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<T>(trimmed) {
            results.push(value);
        }
    }
    results
}

// ---------------------------------------------------------------------------
// JSONL file reading
// ---------------------------------------------------------------------------

/// Maximum bytes to read from a JSONL file (100 MB).
const MAX_JSONL_READ_BYTES: u64 = 100 * 1024 * 1024;

/// Reads and parses a JSONL file, reading at most the last 100 MB.
/// For files larger than 100 MB, reads the tail and skips the first partial line.
pub async fn read_jsonl_file<T: DeserializeOwned>(file_path: &Path) -> anyhow::Result<Vec<T>> {
    let metadata = fs::metadata(file_path).await?;
    let size = metadata.len();

    if size <= MAX_JSONL_READ_BYTES {
        let data = fs::read(file_path).await?;
        return Ok(parse_jsonl_bytes(&data));
    }

    // Read the tail of the file
    let mut file = tokio::fs::File::open(file_path).await?;
    let offset = size - MAX_JSONL_READ_BYTES;
    file.seek(SeekFrom::Start(offset)).await?;

    let mut buf = vec![0u8; MAX_JSONL_READ_BYTES as usize];
    let mut total_read = 0;
    loop {
        let n = file.read(&mut buf[total_read..]).await?;
        if n == 0 {
            break;
        }
        total_read += n;
    }
    let buf = &buf[..total_read];

    // Skip the first partial line
    let newline_idx = buf.iter().position(|&b| b == b'\n');
    let data = match newline_idx {
        Some(idx) if idx < total_read - 1 => &buf[idx + 1..],
        _ => buf,
    };

    Ok(parse_jsonl_bytes(data))
}

// ---------------------------------------------------------------------------
// JSON serialization helpers
// ---------------------------------------------------------------------------

/// Pretty-print a value as JSON with the given indentation.
pub fn json_stringify_pretty<T: serde::Serialize>(
    value: &T,
    indent: usize,
) -> anyhow::Result<String> {
    let val = serde_json::to_value(value)?;
    Ok(pretty_print_json(&val, indent))
}

/// Pretty-print a `serde_json::Value` with custom indentation.
fn pretty_print_json(value: &serde_json::Value, indent: usize) -> String {
    let indent_str = " ".repeat(indent);
    let formatter = serde_json::ser::PrettyFormatter::with_indent(indent_str.as_bytes());
    let mut buf = Vec::new();
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, formatter);
    serde::Serialize::serialize(value, &mut ser).unwrap();
    String::from_utf8(buf).unwrap()
}

/// Compact JSON serialization.
pub fn json_stringify<T: serde::Serialize>(value: &T) -> anyhow::Result<String> {
    Ok(serde_json::to_string(value)?)
}

// ---------------------------------------------------------------------------
// JSON deep merge
// ---------------------------------------------------------------------------

/// Deep merge `overlay` into `base`. Arrays in overlay replace base arrays.
/// Objects are merged recursively.
pub fn deep_merge(base: &mut serde_json::Value, overlay: &serde_json::Value) {
    match (base, overlay) {
        (serde_json::Value::Object(base_map), serde_json::Value::Object(overlay_map)) => {
            for (key, overlay_val) in overlay_map {
                let entry = base_map
                    .entry(key.clone())
                    .or_insert(serde_json::Value::Null);
                deep_merge(entry, overlay_val);
            }
        }
        (base, overlay) => {
            *base = overlay.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_strip_bom() {
        assert_eq!(strip_bom("\u{FEFF}hello"), "hello");
        assert_eq!(strip_bom("hello"), "hello");
    }

    #[test]
    fn test_safe_parse_json() {
        let val: Option<serde_json::Value> = safe_parse_json(r#"{"a": 1}"#);
        assert_eq!(val, Some(json!({"a": 1})));

        let val: Option<serde_json::Value> = safe_parse_json("invalid");
        assert_eq!(val, None);
    }

    #[test]
    fn test_parse_jsonl() {
        let data = "{\"a\":1}\n{\"b\":2}\ninvalid\n{\"c\":3}\n";
        let results: Vec<serde_json::Value> = parse_jsonl(data);
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_deep_merge() {
        let mut base = json!({"a": 1, "b": {"c": 2}});
        let overlay = json!({"b": {"d": 3}, "e": 4});
        deep_merge(&mut base, &overlay);
        assert_eq!(base, json!({"a": 1, "b": {"c": 2, "d": 3}, "e": 4}));
    }
}
