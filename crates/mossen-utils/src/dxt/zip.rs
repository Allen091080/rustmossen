//! DXT zip handling — translated from utils/dxt/zip.ts.
//!
//! Provides validated zip extraction with zip-bomb / path-traversal defenses
//! plus a low-level central-directory parser that recovers Unix file modes
//! (which `zip::ZipArchive` does not always preserve cleanly).

use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::path::{Component, Path};

use anyhow::{anyhow, bail, Result};
use tracing::debug;

/// Safety limits applied during extraction. Mirrors the `LIMITS` object in
/// `utils/dxt/zip.ts` byte-for-byte so the TS and Rust pipelines reject the
/// same archives.
pub const MAX_FILE_SIZE: u64 = 512 * 1024 * 1024; // 512MB per file
pub const MAX_TOTAL_SIZE: u64 = 1024 * 1024 * 1024; // 1024MB total uncompressed
pub const MAX_FILE_COUNT: usize = 100_000; // Maximum number of files
pub const MAX_COMPRESSION_RATIO: f64 = 50.0; // >50:1 is suspicious
pub const MIN_COMPRESSION_RATIO: f64 = 0.5; // <0.5:1 may indicate pre-compressed malicious content

/// State tracker for zip file validation during extraction.
#[derive(Debug, Default)]
pub struct ZipValidationState {
    pub file_count: usize,
    pub total_uncompressed_size: u64,
    pub compressed_size: u64,
    pub errors: Vec<String>,
}

/// File metadata required to validate a single entry. Mirrors the
/// `ZipFileMetadata` type fed to the fflate `filter` callback.
#[derive(Debug, Clone)]
pub struct ZipFileMetadata<'a> {
    pub name: &'a str,
    pub original_size: Option<u64>,
}

/// Result of validating a single entry. Mirrors `FileValidationResult` from
/// the TS source — we keep the explicit struct (rather than collapsing into
/// `Result<()>`) so the surface matches the TS API.
#[derive(Debug, Clone)]
pub struct FileValidationResult {
    pub is_valid: bool,
    pub error: Option<String>,
}

impl FileValidationResult {
    fn ok() -> Self {
        Self { is_valid: true, error: None }
    }

    fn fail(msg: impl Into<String>) -> Self {
        Self { is_valid: false, error: Some(msg.into()) }
    }
}

/// Detect `..` traversal segments anywhere in `path`. This matches the
/// `containsPathTraversal` helper from `utils/path.ts` that the TS source
/// imports — duplicated locally to keep this module self-contained during
/// the port.
fn contains_path_traversal(path: &str) -> bool {
    if path.is_empty() {
        return false;
    }
    // Reject literal '..' segments on both forward and backward slashes.
    let normalized = path.replace('\\', "/");
    normalized
        .split('/')
        .any(|segment| segment == "..")
}

/// Validates a file path to prevent path traversal attacks. Mirrors
/// `isPathSafe` from the TS source.
pub fn is_path_safe(file_path: &str) -> bool {
    if contains_path_traversal(file_path) {
        return false;
    }

    let path = Path::new(file_path);

    // Walk components: any ParentDir survives traversal-check above for
    // edge cases (e.g. embedded ".." after normalize), and any RootDir /
    // Prefix indicates an absolute path.
    for component in path.components() {
        match component {
            Component::ParentDir => return false,
            Component::RootDir | Component::Prefix(_) => return false,
            _ => {}
        }
    }

    if path.is_absolute() {
        return false;
    }

    true
}

/// Validates a single file during zip extraction. Mirrors `validateZipFile`
/// from the TS source — it mutates `state` in-place and returns the
/// validation result. The TS version overwrites `error` on each check so the
/// LAST failing check wins; we preserve that semantics.
pub fn validate_zip_file(
    file: &ZipFileMetadata<'_>,
    state: &mut ZipValidationState,
) -> FileValidationResult {
    state.file_count += 1;

    let mut error: Option<String> = None;

    // Check file count.
    if state.file_count > MAX_FILE_COUNT {
        error = Some(format!(
            "Archive contains too many files: {} (max: {})",
            state.file_count, MAX_FILE_COUNT
        ));
    }

    // Validate path safety.
    if !is_path_safe(file.name) {
        error = Some(format!(
            "Unsafe file path detected: \"{}\". Path traversal or absolute paths are not allowed.",
            file.name
        ));
    }

    // Check individual file size.
    let file_size = file.original_size.unwrap_or(0);
    if file_size > MAX_FILE_SIZE {
        error = Some(format!(
            "File \"{}\" is too large: {}MB (max: {}MB)",
            file.name,
            file_size / 1024 / 1024,
            MAX_FILE_SIZE / 1024 / 1024
        ));
    }

    // Track total uncompressed size.
    state.total_uncompressed_size += file_size;

    // Check total size.
    if state.total_uncompressed_size > MAX_TOTAL_SIZE {
        error = Some(format!(
            "Archive total size is too large: {}MB (max: {}MB)",
            state.total_uncompressed_size / 1024 / 1024,
            MAX_TOTAL_SIZE / 1024 / 1024
        ));
    }

    // Check compression ratio for zip bomb detection.
    if state.compressed_size > 0 {
        let current_ratio =
            state.total_uncompressed_size as f64 / state.compressed_size as f64;
        if current_ratio > MAX_COMPRESSION_RATIO {
            error = Some(format!(
                "Suspicious compression ratio detected: {:.1}:1 (max: {}:1). This may be a zip bomb.",
                current_ratio, MAX_COMPRESSION_RATIO as u64
            ));
        }
    }

    match error {
        Some(msg) => {
            state.errors.push(msg.clone());
            FileValidationResult::fail(msg)
        }
        None => FileValidationResult::ok(),
    }
}

/// Unzips raw zip bytes and returns its contents as a map of file paths to
/// byte vectors. Mirrors `unzipFile` from the TS source.
///
/// The TS version is async because it lazy-imports `fflate`; the Rust version
/// is sync because `zip::ZipArchive` is sync. Callers that need async can
/// pair this with `tokio::task::spawn_blocking`.
pub fn unzip_file(zip_data: &[u8]) -> Result<HashMap<String, Vec<u8>>> {
    let compressed_size = zip_data.len() as u64;

    let mut state = ZipValidationState {
        file_count: 0,
        total_uncompressed_size: 0,
        compressed_size,
        errors: Vec::new(),
    };

    let reader = Cursor::new(zip_data);
    let mut archive = zip::ZipArchive::new(reader)
        .map_err(|e| anyhow!("Failed to read zip archive: {}", e))?;

    let mut result: HashMap<String, Vec<u8>> = HashMap::new();

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| anyhow!("Failed to read zip entry: {}", e))?;

        // Use the canonical entry name. We avoid `enclosed_name()` so the
        // raw archive path reaches `validate_zip_file` for traversal checks.
        let name = entry.name().to_string();

        // Skip directory entries — they have no content and the TS path also
        // produces no files for them.
        if entry.is_dir() || name.ends_with('/') {
            continue;
        }

        let meta = ZipFileMetadata {
            name: &name,
            original_size: Some(entry.size()),
        };
        let validation = validate_zip_file(&meta, &mut state);
        if !validation.is_valid {
            // Mirror the TS `throw new Error(...)` thrown from the filter.
            return Err(anyhow!(validation.error.unwrap_or_else(|| {
                "Zip validation failed".to_string()
            })));
        }

        let mut contents = Vec::with_capacity(entry.size() as usize);
        entry
            .read_to_end(&mut contents)
            .map_err(|e| anyhow!("Failed to read file \"{}\": {}", name, e))?;

        result.insert(name, contents);
    }

    debug!(
        "Zip extraction completed: {} files, {}KB uncompressed",
        state.file_count,
        state.total_uncompressed_size / 1024
    );

    Ok(result)
}

/// Parse Unix file modes from a zip's central directory. Mirrors the
/// `parseZipModes` helper from the TS source, including the same scan-back
/// EOCD strategy and the ZIP64 fallback (returns `{}` on archives that
/// require it).
pub fn parse_zip_modes(data: &[u8]) -> HashMap<String, u32> {
    let mut modes: HashMap<String, u32> = HashMap::new();

    if data.len() < 22 {
        return modes;
    }

    // 1. Find the End of Central Directory record (sig 0x06054b50). It lives
    //    in the trailing 22 + 65535 bytes. Scan backwards.
    let max_back: usize = 22 + 0xffff;
    let min_eocd: usize = data.len().saturating_sub(max_back);
    let mut eocd: Option<usize> = None;
    let mut i = data.len() - 22;
    loop {
        if read_u32_le(data, i) == Some(0x0605_4b50) {
            eocd = Some(i);
            break;
        }
        if i == min_eocd {
            break;
        }
        i -= 1;
    }
    let eocd = match eocd {
        Some(e) => e,
        None => return modes, // malformed — let zip's error surface elsewhere
    };

    let entry_count = match read_u16_le(data, eocd + 10) {
        Some(v) => v as usize,
        None => return modes,
    };
    let mut off = match read_u32_le(data, eocd + 16) {
        Some(v) => v as usize,
        None => return modes,
    };

    // 2. Walk central directory entries (sig 0x02014b50). Each entry has a
    //    46-byte fixed header followed by variable-length name/extra/comment.
    for _ in 0..entry_count {
        if off + 46 > data.len() {
            break;
        }
        if read_u32_le(data, off) != Some(0x0201_4b50) {
            break;
        }
        let version_made_by = match read_u16_le(data, off + 4) {
            Some(v) => v,
            None => break,
        };
        let name_len = match read_u16_le(data, off + 28) {
            Some(v) => v as usize,
            None => break,
        };
        let extra_len = match read_u16_le(data, off + 30) {
            Some(v) => v as usize,
            None => break,
        };
        let comment_len = match read_u16_le(data, off + 32) {
            Some(v) => v as usize,
            None => break,
        };
        let external_attr = match read_u32_le(data, off + 38) {
            Some(v) => v,
            None => break,
        };

        if off + 46 + name_len > data.len() {
            break;
        }
        let name = String::from_utf8_lossy(&data[off + 46..off + 46 + name_len]).to_string();

        // versionMadeBy high byte = host OS. 3 = Unix. For Unix zips, the high
        // 16 bits of externalAttr hold st_mode (file type + permission bits).
        if (version_made_by >> 8) == 3 {
            let mode = (external_attr >> 16) & 0xffff;
            if mode != 0 {
                modes.insert(name, mode);
            }
        }

        off += 46 + name_len + extra_len + comment_len;
    }

    modes
}

fn read_u16_le(buf: &[u8], off: usize) -> Option<u16> {
    if off + 2 > buf.len() {
        return None;
    }
    Some(u16::from_le_bytes([buf[off], buf[off + 1]]))
}

fn read_u32_le(buf: &[u8], off: usize) -> Option<u32> {
    if off + 4 > buf.len() {
        return None;
    }
    Some(u32::from_le_bytes([
        buf[off],
        buf[off + 1],
        buf[off + 2],
        buf[off + 3],
    ]))
}

/// Tag for distinguishing ENOENT vs other I/O errors when wrapping read
/// failures. Mirrors the `isENOENT` import from `utils/errors.ts`.
fn is_enoent(err: &std::io::Error) -> bool {
    err.kind() == std::io::ErrorKind::NotFound
}

/// Reads a zip file from disk asynchronously and unzips it. Mirrors
/// `readAndUnzipFile` from the TS source — propagates ENOENT verbatim, wraps
/// every other failure in `"Failed to read or unzip file: {msg}"`.
pub async fn read_and_unzip_file(file_path: &Path) -> Result<HashMap<String, Vec<u8>>> {
    let zip_data = match tokio::fs::read(file_path).await {
        Ok(bytes) => bytes,
        Err(err) => {
            if is_enoent(&err) {
                return Err(anyhow::Error::from(err));
            }
            bail!("Failed to read or unzip file: {}", err);
        }
    };

    match unzip_file(&zip_data) {
        Ok(map) => Ok(map),
        Err(err) => bail!("Failed to read or unzip file: {}", err),
    }
}
