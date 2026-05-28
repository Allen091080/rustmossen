//! JSON parsing utilities with JSONL support.
//!
//! Provides safe JSON/JSONC parsing with caching, JSONL parsing,
//! and JSONC array modification preserving comments.

use lru::LruCache;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde_json::Value;
use std::num::NonZeroUsize;
use std::path::Path;
use tokio::fs;

/// Maximum key size for parse cache.
const PARSE_CACHE_MAX_KEY_BYTES: usize = 8 * 1024;

/// Maximum JSONL file read size (100 MB).
const MAX_JSONL_READ_BYTES: u64 = 100 * 1024 * 1024;

/// LRU cache for parsed JSON values.
static PARSE_CACHE: Lazy<Mutex<LruCache<String, Option<Value>>>> =
    Lazy::new(|| Mutex::new(LruCache::new(NonZeroUsize::new(50).unwrap())));

/// Strip UTF-8 BOM from a string.
pub fn strip_bom(s: &str) -> &str {
    s.strip_prefix('\u{FEFF}').unwrap_or(s)
}

/// Safely parse JSON with caching for small inputs.
pub fn safe_parse_json(json: Option<&str>, should_log_error: bool) -> Option<Value> {
    let json = json?;
    if json.is_empty() {
        return None;
    }

    if json.len() <= PARSE_CACHE_MAX_KEY_BYTES {
        let mut cache = PARSE_CACHE.lock();
        if let Some(cached) = cache.get(&json.to_string()) {
            return cached.clone();
        }
        let result = parse_json_uncached(json, should_log_error);
        cache.put(json.to_string(), result.clone());
        result
    } else {
        parse_json_uncached(json, should_log_error)
    }
}

/// Parse JSON without cache.
fn parse_json_uncached(json: &str, should_log_error: bool) -> Option<Value> {
    let stripped = strip_bom(json);
    match serde_json::from_str(stripped) {
        Ok(v) => Some(v),
        Err(e) => {
            if should_log_error {
                tracing::error!("JSON parse error: {}", e);
            }
            None
        }
    }
}

/// Safely parse JSONC (JSON with comments).
pub fn safe_parse_jsonc(json: Option<&str>) -> Option<Value> {
    let json = json?;
    if json.is_empty() {
        return None;
    }
    let stripped = strip_bom(json);
    // Strip single-line comments and block comments
    let cleaned = strip_jsonc_comments(stripped);
    match serde_json::from_str(&cleaned) {
        Ok(v) => Some(v),
        Err(e) => {
            tracing::error!("JSONC parse error: {}", e);
            None
        }
    }
}

/// Strip JSONC comments (single-line // and block /* */).
fn strip_jsonc_comments(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;
    let mut escape_next = false;

    while let Some(c) = chars.next() {
        if escape_next {
            result.push(c);
            escape_next = false;
            continue;
        }

        if in_string {
            result.push(c);
            if c == '\\' {
                escape_next = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }

        if c == '"' {
            in_string = true;
            result.push(c);
        } else if c == '/' {
            match chars.peek() {
                Some('/') => {
                    chars.next();
                    // Skip until end of line
                    while let Some(&nc) = chars.peek() {
                        if nc == '\n' {
                            break;
                        }
                        chars.next();
                    }
                }
                Some('*') => {
                    chars.next();
                    // Skip until */
                    let mut found_end = false;
                    while let Some(nc) = chars.next() {
                        if nc == '*' && chars.peek() == Some(&'/') {
                            chars.next();
                            found_end = true;
                            break;
                        }
                    }
                    if !found_end {
                        // Unterminated comment
                    }
                }
                _ => result.push(c),
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Parse JSONL (JSON Lines) data.
pub fn parse_jsonl<T: serde::de::DeserializeOwned>(data: &str) -> Vec<T> {
    let stripped = strip_bom(data);
    let mut results = Vec::new();
    for line in stripped.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str(trimmed) {
            Ok(v) => results.push(v),
            Err(_) => {
                // Skip malformed lines
            }
        }
    }
    results
}

/// Parse JSONL from bytes.
pub fn parse_jsonl_bytes<T: serde::de::DeserializeOwned>(data: &[u8]) -> Vec<T> {
    let mut start = 0;
    // Strip UTF-8 BOM
    if data.len() >= 3 && data[0] == 0xEF && data[1] == 0xBB && data[2] == 0xBF {
        start = 3;
    }

    let mut results = Vec::new();
    while start < data.len() {
        let end = data[start..]
            .iter()
            .position(|&b| b == b'\n')
            .map(|p| start + p)
            .unwrap_or(data.len());

        let line = String::from_utf8_lossy(&data[start..end]);
        let trimmed = line.trim();
        start = end + 1;

        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str(trimmed) {
            Ok(v) => results.push(v),
            Err(_) => {
                // Skip malformed lines
            }
        }
    }
    results
}

/// Read and parse a JSONL file, reading at most the last 100 MB.
pub async fn read_jsonl_file<T: serde::de::DeserializeOwned>(
    file_path: &Path,
) -> anyhow::Result<Vec<T>> {
    let metadata = fs::metadata(file_path).await?;
    let size = metadata.len();

    if size <= MAX_JSONL_READ_BYTES {
        let data = fs::read(file_path).await?;
        return Ok(parse_jsonl_bytes(&data));
    }

    // Read the tail of the file
    use tokio::io::{AsyncReadExt, AsyncSeekExt};
    let mut file = fs::File::open(file_path).await?;
    let offset = size - MAX_JSONL_READ_BYTES;
    file.seek(std::io::SeekFrom::Start(offset)).await?;

    let mut buf = vec![0u8; MAX_JSONL_READ_BYTES as usize];
    let mut total_read = 0;
    while total_read < buf.len() {
        let n = file.read(&mut buf[total_read..]).await?;
        if n == 0 {
            break;
        }
        total_read += n;
    }

    // Skip the first partial line
    if let Some(nl_pos) = buf[..total_read].iter().position(|&b| b == b'\n') {
        if nl_pos < total_read - 1 {
            return Ok(parse_jsonl_bytes(&buf[nl_pos + 1..total_read]));
        }
    }
    Ok(parse_jsonl_bytes(&buf[..total_read]))
}

/// Add an item to a JSONC array, preserving comments and formatting.
pub fn add_item_to_jsonc_array(content: &str, new_item: &Value) -> String {
    if content.trim().is_empty() {
        return serde_json::to_string_pretty(&Value::Array(vec![new_item.clone()]))
            .unwrap_or_else(|_| "[]".to_string());
    }

    let clean_content = strip_bom(content);

    match serde_json::from_str::<Value>(&strip_jsonc_comments(clean_content)) {
        Ok(Value::Array(mut arr)) => {
            arr.push(new_item.clone());
            serde_json::to_string_pretty(&Value::Array(arr)).unwrap_or_else(|_| "[]".to_string())
        }
        Ok(_) => serde_json::to_string_pretty(&Value::Array(vec![new_item.clone()]))
            .unwrap_or_else(|_| "[]".to_string()),
        Err(e) => {
            tracing::error!("Failed to parse JSONC: {}", e);
            serde_json::to_string_pretty(&Value::Array(vec![new_item.clone()]))
                .unwrap_or_else(|_| "[]".to_string())
        }
    }
}

/// Clear the parse cache.
pub fn clear_parse_cache() {
    PARSE_CACHE.lock().clear();
}
