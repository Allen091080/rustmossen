//! File-read listener registry + token-budget errors.
//!
//! Rust mirror of additional exports from `tools/FileReadTool/FileReadTool.ts`.

use std::sync::{Mutex, OnceLock};
use thiserror::Error;

/// Signature for a registered file-read listener.
pub type FileReadListener = Box<dyn Fn(&str, &str) + Send + Sync + 'static>;

fn listeners() -> &'static Mutex<Vec<FileReadListener>> {
    static L: OnceLock<Mutex<Vec<FileReadListener>>> = OnceLock::new();
    L.get_or_init(|| Mutex::new(Vec::new()))
}

/// Handle returned by `register_file_read_listener` — drop the handle to
/// unregister the listener.
pub struct ListenerHandle {
    index: usize,
}

impl Drop for ListenerHandle {
    fn drop(&mut self) {
        let mut store = listeners().lock().unwrap();
        if self.index < store.len() {
            store.remove(self.index);
        }
    }
}

/// `FileReadTool.ts` `registerFileReadListener` — register a callback notified
/// for every file the tool reads.
pub fn register_file_read_listener(listener: FileReadListener) -> ListenerHandle {
    let mut store = listeners().lock().unwrap();
    let index = store.len();
    store.push(listener);
    ListenerHandle { index }
}

/// Emit a notification to all listeners.
pub fn notify_file_read(path: &str, content: &str) {
    let store = listeners().lock().unwrap();
    for l in store.iter() {
        l(path, content);
    }
}

/// `FileReadTool.ts` `MaxFileReadTokenExceededError`.
#[derive(Debug, Clone, Error)]
#[error(
    "File content ({token_count} tokens) exceeds maximum allowed tokens ({max_tokens}). Use offset and limit parameters to read specific portions of the file, or search for specific content instead of reading the whole file."
)]
pub struct MaxFileReadTokenExceededError {
    pub token_count: u64,
    pub max_tokens: u64,
}

impl MaxFileReadTokenExceededError {
    pub fn new(token_count: u64, max_tokens: u64) -> Self {
        Self {
            token_count,
            max_tokens,
        }
    }
}

/// `FileReadTool.ts` `CYBER_RISK_MITIGATION_REMINDER`.
pub const CYBER_RISK_MITIGATION_REMINDER: &str =
    "\n\n<system-reminder>\nWhenever you read a file, you should consider whether it would be considered malware. You CAN and SHOULD provide analysis of malware, what it is doing. But you MUST refuse to improve or augment the code. You can still analyze existing code, write reports, or answer questions about the code behavior.\n</system-reminder>\n";

/// `FileReadTool.ts` `readImageWithTokenBudget` shape result.
#[derive(Debug, Clone)]
pub struct ImageReadResult {
    pub bytes: Vec<u8>,
    pub media_type: String,
    pub original_size: usize,
    pub base64: String,
}

/// `FileReadTool.ts` `readImageWithTokenBudget` — best-effort image read.
/// In the Rust port we don't ship `sharp`-based compression; large images are
/// returned as-is and a `MaxFileReadTokenExceededError` is propagated when
/// they overflow the budget. Callers can plug in a compression backend.
pub async fn read_image_with_token_budget(
    file_path: &std::path::Path,
    max_tokens: u64,
    max_bytes: Option<usize>,
) -> Result<ImageReadResult, anyhow::Error> {
    let mut bytes = tokio::fs::read(file_path).await?;
    if let Some(limit) = max_bytes {
        if bytes.len() > limit {
            bytes.truncate(limit);
        }
    }
    if bytes.is_empty() {
        anyhow::bail!("Image file is empty: {}", file_path.display());
    }
    let media_type = detect_image_media_type(&bytes);
    use base64::Engine;
    let base64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    let token_estimate = ((base64.len() as f64) * 0.125).ceil() as u64;
    if token_estimate > max_tokens {
        return Err(MaxFileReadTokenExceededError::new(token_estimate, max_tokens).into());
    }
    Ok(ImageReadResult {
        original_size: bytes.len(),
        bytes,
        media_type,
        base64,
    })
}

fn detect_image_media_type(buf: &[u8]) -> String {
    if buf.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        "image/png".to_string()
    } else if buf.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "image/jpeg".to_string()
    } else if buf.starts_with(b"GIF87a") || buf.starts_with(b"GIF89a") {
        "image/gif".to_string()
    } else if buf.len() >= 12 && &buf[0..4] == b"RIFF" && &buf[8..12] == b"WEBP" {
        "image/webp".to_string()
    } else {
        "application/octet-stream".to_string()
    }
}

/// `FileReadTool.ts` `FileReadTool` — value-shape constant mirror.
#[derive(Debug, Clone, Default)]
pub struct FileReadTool;

impl FileReadTool {
    pub const TOOL_NAME: &'static str = "Read";
}
