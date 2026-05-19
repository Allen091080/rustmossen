//! # API Limits (apiLimits.ts)
//!
//! Mossen API 服务端限制常量。

// =============================================================================
// IMAGE LIMITS
// =============================================================================

/// Maximum base64-encoded image size (API enforced).
pub const API_IMAGE_MAX_BASE64_SIZE: usize = 5 * 1024 * 1024; // 5 MB

/// Target raw image size to stay under base64 limit after encoding.
pub const IMAGE_TARGET_RAW_SIZE: usize = (API_IMAGE_MAX_BASE64_SIZE * 3) / 4; // 3.75 MB

/// Client-side maximum image width.
pub const IMAGE_MAX_WIDTH: u32 = 2000;

/// Client-side maximum image height.
pub const IMAGE_MAX_HEIGHT: u32 = 2000;

// =============================================================================
// PDF LIMITS
// =============================================================================

/// Maximum raw PDF file size that fits within the API request limit after encoding.
pub const PDF_TARGET_RAW_SIZE: usize = 20 * 1024 * 1024; // 20 MB

/// Maximum number of pages in a PDF accepted by the API.
pub const API_PDF_MAX_PAGES: u32 = 100;

/// Size threshold above which PDFs are extracted into page images.
pub const PDF_EXTRACT_SIZE_THRESHOLD: usize = 3 * 1024 * 1024; // 3 MB

/// Maximum PDF file size for the page extraction path.
pub const PDF_MAX_EXTRACT_SIZE: usize = 100 * 1024 * 1024; // 100 MB

/// Max pages the Read tool will extract in a single call with the pages parameter.
pub const PDF_MAX_PAGES_PER_READ: u32 = 20;

/// PDFs with more pages than this get the reference treatment on @ mention.
pub const PDF_AT_MENTION_INLINE_THRESHOLD: u32 = 10;

// =============================================================================
// MEDIA LIMITS
// =============================================================================

/// Maximum number of media items (images + PDFs) allowed per API request.
pub const API_MAX_MEDIA_PER_REQUEST: u32 = 100;
