//! # file_read — 同步文件读取路径
//!
//! 对应 TypeScript `utils/fileRead.ts`。
//! 提供同步文件读取功能，包括编码检测和行尾风格检测。
//!
//! 从 file.ts 提取的叶子模块，仅依赖 fsOperations 和 debug，
//! 避免拉入完整的设置 SCC 依赖链。

use std::path::Path;
use tracing::debug;

/// 行尾类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEndingType {
    Crlf,
    Lf,
}

impl LineEndingType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Crlf => "CRLF",
            Self::Lf => "LF",
        }
    }
}

/// 文件编码类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileEncoding {
    Utf8,
    Utf16Le,
}

/// 通过读取文件头部字节检测编码。
///
/// 空文件默认使用 UTF-8。
/// 检测 UTF-16 LE BOM (FF FE) 和 UTF-8 BOM (EF BB BF)。
pub fn detect_encoding_for_resolved_path(resolved_path: &Path) -> FileEncoding {
    let data = match std::fs::read(resolved_path) {
        Ok(d) => d,
        Err(_) => return FileEncoding::Utf8,
    };

    let bytes_read = data.len().min(4096);

    // Empty files should default to utf8
    if bytes_read == 0 {
        return FileEncoding::Utf8;
    }

    if bytes_read >= 2 && data[0] == 0xFF && data[1] == 0xFE {
        return FileEncoding::Utf16Le;
    }

    if bytes_read >= 3 && data[0] == 0xEF && data[1] == 0xBB && data[2] == 0xBF {
        return FileEncoding::Utf8;
    }

    // For non-empty files, default to utf8
    FileEncoding::Utf8
}

/// 检测字符串中的行尾风格。
///
/// 统计 CRLF 和 LF 的出现次数，返回较多的那种。
pub fn detect_line_endings_for_string(content: &str) -> LineEndingType {
    let mut crlf_count = 0usize;
    let mut lf_count = 0usize;

    let bytes = content.as_bytes();
    for i in 0..bytes.len() {
        if bytes[i] == b'\n' {
            if i > 0 && bytes[i - 1] == b'\r' {
                crlf_count += 1;
            } else {
                lf_count += 1;
            }
        }
    }

    if crlf_count > lf_count {
        LineEndingType::Crlf
    } else {
        LineEndingType::Lf
    }
}

/// 带元数据的文件读取结果
pub struct FileReadMetadata {
    pub content: String,
    pub encoding: FileEncoding,
    pub line_endings: LineEndingType,
}

/// 读取文件并返回内容、检测到的编码和原始行尾风格。
///
/// 调用者在写回文件时（例如 FileEditTool）可以复用这些信息，
/// 而不必分别调用 detect_file_encoding / detect_line_endings，
/// 后者会分别重做 safeResolvePath + readSync(4KB)。
pub fn read_file_sync_with_metadata(file_path: &Path) -> anyhow::Result<FileReadMetadata> {
    let resolved_path = file_path.canonicalize().unwrap_or_else(|_| file_path.to_path_buf());

    let is_symlink = file_path.is_symlink();
    if is_symlink {
        debug!(
            "Reading through symlink: {:?} -> {:?}",
            file_path, resolved_path
        );
    }

    let encoding = detect_encoding_for_resolved_path(&resolved_path);

    let raw = match encoding {
        FileEncoding::Utf8 => std::fs::read_to_string(&resolved_path)?,
        FileEncoding::Utf16Le => {
            let bytes = std::fs::read(&resolved_path)?;
            // Skip BOM if present
            let start = if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
                2
            } else {
                0
            };
            let u16s: Vec<u16> = bytes[start..]
                .chunks_exact(2)
                .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
                .collect();
            String::from_utf16_lossy(&u16s)
        }
    };

    // Detect line endings from the raw head before CRLF normalization
    let sample_len = raw.len().min(4096);
    let line_endings = detect_line_endings_for_string(&raw[..sample_len]);

    let content = raw.replace("\r\n", "\n");

    Ok(FileReadMetadata {
        content,
        encoding,
        line_endings,
    })
}

/// 同步读取文件内容（CRLF 已规范化为 LF）
pub fn read_file_sync(file_path: &Path) -> anyhow::Result<String> {
    Ok(read_file_sync_with_metadata(file_path)?.content)
}
