//! # Files (files.ts)
//!
//! 二进制文件扩展名与检测常量。

use std::collections::HashSet;

use once_cell::sync::Lazy;

/// Binary file extensions to skip for text-based operations.
/// These files can't be meaningfully compared as text and are often large.
pub static BINARY_EXTENSIONS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    [
        // Images
        ".png",
        ".jpg",
        ".jpeg",
        ".gif",
        ".bmp",
        ".ico",
        ".webp",
        ".tiff",
        ".tif",
        // Videos
        ".mp4",
        ".mov",
        ".avi",
        ".mkv",
        ".webm",
        ".wmv",
        ".flv",
        ".m4v",
        ".mpeg",
        ".mpg",
        // Audio
        ".mp3",
        ".wav",
        ".ogg",
        ".flac",
        ".aac",
        ".m4a",
        ".wma",
        ".aiff",
        concat!(".op", "us"),
        // Archives
        ".zip",
        ".tar",
        ".gz",
        ".bz2",
        ".7z",
        ".rar",
        ".xz",
        ".z",
        ".tgz",
        ".iso",
        // Executables/binaries
        ".exe",
        ".dll",
        ".so",
        ".dylib",
        ".bin",
        ".o",
        ".a",
        ".obj",
        ".lib",
        ".app",
        ".msi",
        ".deb",
        ".rpm", // Documents (PDF is here; FileReadTool excludes it at the call site)
        ".pdf",
        ".doc",
        ".docx",
        ".xls",
        ".xlsx",
        ".ppt",
        ".pptx",
        ".odt",
        ".ods",
        ".odp",
        // Fonts
        ".ttf",
        ".otf",
        ".woff",
        ".woff2",
        ".eot", // Bytecode / VM artifacts
        ".pyc",
        ".pyo",
        ".class",
        ".jar",
        ".war",
        ".ear",
        ".node",
        ".wasm",
        ".rlib",
        // Database files
        ".sqlite",
        ".sqlite3",
        ".db",
        ".mdb",
        ".idx", // Design / 3D
        ".psd",
        ".ai",
        ".eps",
        ".sketch",
        ".fig",
        ".xd",
        ".blend",
        ".3ds",
        ".max",
        // Flash
        ".swf",
        ".fla", // Lock/profiling data
        ".lockb",
        ".dat",
        ".data",
    ]
    .into_iter()
    .collect()
});

/// Check if a file path has a binary extension.
pub fn has_binary_extension(file_path: &str) -> bool {
    if let Some(dot_pos) = file_path.rfind('.') {
        let ext = &file_path[dot_pos..];
        BINARY_EXTENSIONS.contains(ext.to_lowercase().as_str())
    } else {
        false
    }
}

/// Number of bytes to read for binary content detection.
pub const BINARY_CHECK_SIZE: usize = 8192;

/// Check if a buffer contains binary content by looking for null bytes
/// or a high proportion of non-printable characters.
pub fn is_binary_content(buffer: &[u8]) -> bool {
    // Check first BINARY_CHECK_SIZE bytes (or full buffer if smaller)
    let check_size = buffer.len().min(BINARY_CHECK_SIZE);

    let mut non_printable = 0usize;
    for &byte in &buffer[..check_size] {
        // Null byte is a strong indicator of binary
        if byte == 0 {
            return true;
        }
        // Count non-printable, non-whitespace bytes
        // Printable ASCII is 32-126, plus common whitespace (9, 10, 13)
        if byte < 32 && byte != 9 && byte != 10 && byte != 13 {
            non_printable += 1;
        }
    }

    if check_size == 0 {
        return false;
    }
    // If more than 10% non-printable, likely binary
    (non_printable as f64) / (check_size as f64) > 0.1
}
