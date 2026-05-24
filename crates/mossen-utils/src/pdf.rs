//! PDF file reading and page extraction utilities.
//!
//! Provides functions to read PDFs as base64, get page counts via pdfinfo,
//! and extract pages as JPEG images via pdftoppm.

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::process::Command;
use uuid::Uuid;

/// Maximum raw PDF size for base64 encoding (~20MB).
pub const PDF_TARGET_RAW_SIZE: u64 = 20 * 1024 * 1024;

/// Maximum raw PDF size for page extraction.
pub const PDF_MAX_EXTRACT_SIZE: u64 = 200 * 1024 * 1024;

/// PDF error reasons.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PdfErrorReason {
    Empty,
    TooLarge,
    PasswordProtected,
    Corrupted,
    Unknown,
    Unavailable,
}

/// A structured PDF error.
#[derive(Debug, Clone)]
pub struct PdfError {
    pub reason: PdfErrorReason,
    pub message: String,
}

/// Result type for PDF operations.
pub type PdfResult<T> = std::result::Result<T, PdfError>;

/// PDF file data with base64 encoding.
#[derive(Debug, Clone)]
pub struct PdfFileData {
    pub file_path: PathBuf,
    pub base64: String,
    pub original_size: u64,
}

/// Result of reading a PDF.
#[derive(Debug, Clone)]
pub struct PdfReadResult {
    pub data_type: String,
    pub file: PdfFileData,
}

/// Read a PDF file and return it as base64-encoded data.
pub async fn read_pdf(file_path: &Path) -> PdfResult<PdfReadResult> {
    let metadata = fs::metadata(file_path).await.map_err(|e| PdfError {
        reason: PdfErrorReason::Unknown,
        message: e.to_string(),
    })?;

    let original_size = metadata.len();

    if original_size == 0 {
        return Err(PdfError {
            reason: PdfErrorReason::Empty,
            message: format!("PDF file is empty: {}", file_path.display()),
        });
    }

    if original_size > PDF_TARGET_RAW_SIZE {
        return Err(PdfError {
            reason: PdfErrorReason::TooLarge,
            message: format!(
                "PDF file exceeds maximum allowed size of {}.",
                format_file_size(PDF_TARGET_RAW_SIZE)
            ),
        });
    }

    let file_buffer = fs::read(file_path).await.map_err(|e| PdfError {
        reason: PdfErrorReason::Unknown,
        message: e.to_string(),
    })?;

    // Validate PDF magic bytes
    if file_buffer.len() < 5 || &file_buffer[..5] != b"%PDF-" {
        return Err(PdfError {
            reason: PdfErrorReason::Corrupted,
            message: format!(
                "File is not a valid PDF (missing %PDF- header): {}",
                file_path.display()
            ),
        });
    }

    let base64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &file_buffer);

    Ok(PdfReadResult {
        data_type: "pdf".to_string(),
        file: PdfFileData {
            file_path: file_path.to_path_buf(),
            base64,
            original_size,
        },
    })
}

/// Get the number of pages in a PDF file using pdfinfo.
pub async fn get_pdf_page_count(file_path: &Path) -> Option<u32> {
    let output = Command::new("pdfinfo").arg(file_path).output().await.ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let re = regex::Regex::new(r"(?m)^Pages:\s+(\d+)").ok()?;
    let caps = re.captures(&stdout)?;
    caps.get(1)?.as_str().parse::<u32>().ok()
}

/// Result of extracting PDF pages.
#[derive(Debug, Clone)]
pub struct PdfExtractPagesResult {
    pub data_type: String,
    pub file_path: PathBuf,
    pub original_size: u64,
    pub count: usize,
    pub output_dir: PathBuf,
}

/// Cache for pdftoppm availability.
static PDFTOPPM_AVAILABLE: Lazy<Mutex<Option<bool>>> = Lazy::new(|| Mutex::new(None));

/// Reset the pdftoppm availability cache.
pub fn reset_pdftoppm_cache() {
    *PDFTOPPM_AVAILABLE.lock() = None;
}

/// Check whether pdftoppm is available.
pub async fn is_pdftoppm_available() -> bool {
    {
        let cached = PDFTOPPM_AVAILABLE.lock();
        if let Some(val) = *cached {
            return val;
        }
    }

    let result = Command::new("pdftoppm").arg("-v").output().await;

    let available = match result {
        Ok(output) => output.status.success() || !output.stderr.is_empty(),
        Err(_) => false,
    };

    *PDFTOPPM_AVAILABLE.lock() = Some(available);
    available
}

/// Options for page extraction.
#[derive(Debug, Clone, Default)]
pub struct ExtractPagesOptions {
    pub first_page: Option<u32>,
    pub last_page: Option<u32>,
}

/// Extract PDF pages as JPEG images using pdftoppm.
pub async fn extract_pdf_pages(
    file_path: &Path,
    tool_results_dir: &Path,
    options: Option<ExtractPagesOptions>,
) -> PdfResult<PdfExtractPagesResult> {
    let metadata = fs::metadata(file_path).await.map_err(|e| PdfError {
        reason: PdfErrorReason::Unknown,
        message: e.to_string(),
    })?;

    let original_size = metadata.len();

    if original_size == 0 {
        return Err(PdfError {
            reason: PdfErrorReason::Empty,
            message: format!("PDF file is empty: {}", file_path.display()),
        });
    }

    if original_size > PDF_MAX_EXTRACT_SIZE {
        return Err(PdfError {
            reason: PdfErrorReason::TooLarge,
            message: format!(
                "PDF file exceeds maximum allowed size for text extraction ({}).",
                format_file_size(PDF_MAX_EXTRACT_SIZE)
            ),
        });
    }

    if !is_pdftoppm_available().await {
        return Err(PdfError {
            reason: PdfErrorReason::Unavailable,
            message: "pdftoppm is not installed. Install poppler-utils (e.g. `brew install poppler` or `apt-get install poppler-utils`) to enable PDF page rendering.".to_string(),
        });
    }

    let uuid = Uuid::new_v4();
    let output_dir = tool_results_dir.join(format!("pdf-{}", uuid));
    fs::create_dir_all(&output_dir)
        .await
        .map_err(|e| PdfError {
            reason: PdfErrorReason::Unknown,
            message: e.to_string(),
        })?;

    let prefix = output_dir.join("page");
    let mut args = vec!["-jpeg".to_string(), "-r".to_string(), "100".to_string()];

    if let Some(ref opts) = options {
        if let Some(first) = opts.first_page {
            args.push("-f".to_string());
            args.push(first.to_string());
        }
        if let Some(last) = opts.last_page {
            if last != u32::MAX {
                args.push("-l".to_string());
                args.push(last.to_string());
            }
        }
    }

    args.push(file_path.to_string_lossy().to_string());
    args.push(prefix.to_string_lossy().to_string());

    let output = Command::new("pdftoppm")
        .args(&args)
        .output()
        .await
        .map_err(|e| PdfError {
            reason: PdfErrorReason::Unknown,
            message: format!("Failed to run pdftoppm: {}", e),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if regex::Regex::new(r"(?i)password")
            .unwrap()
            .is_match(&stderr)
        {
            return Err(PdfError {
                reason: PdfErrorReason::PasswordProtected,
                message: "PDF is password-protected. Please provide an unprotected version."
                    .to_string(),
            });
        }
        if regex::Regex::new(r"(?i)damaged|corrupt|invalid")
            .unwrap()
            .is_match(&stderr)
        {
            return Err(PdfError {
                reason: PdfErrorReason::Corrupted,
                message: "PDF file is corrupted or invalid.".to_string(),
            });
        }
        return Err(PdfError {
            reason: PdfErrorReason::Unknown,
            message: format!("pdftoppm failed: {}", stderr),
        });
    }

    // Read generated image files
    let mut entries = fs::read_dir(&output_dir).await.map_err(|e| PdfError {
        reason: PdfErrorReason::Unknown,
        message: e.to_string(),
    })?;

    let mut image_files = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(".jpg") {
            image_files.push(name);
        }
    }
    image_files.sort();

    if image_files.is_empty() {
        return Err(PdfError {
            reason: PdfErrorReason::Corrupted,
            message: "pdftoppm produced no output pages. The PDF may be invalid.".to_string(),
        });
    }

    Ok(PdfExtractPagesResult {
        data_type: "parts".to_string(),
        file_path: file_path.to_path_buf(),
        original_size,
        count: image_files.len(),
        output_dir,
    })
}

/// Format file size for display.
fn format_file_size(bytes: u64) -> String {
    if bytes >= 1_073_741_824 {
        format!("{:.1} GB", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}
