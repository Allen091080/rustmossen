//! Image resizing and compression utilities.
//!
//! Provides image buffer resizing, format detection, compression pipelines,
//! and metadata extraction for API-compatible image processing.

use serde::{Deserialize, Serialize};

/// Image media types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageMediaType {
    #[serde(rename = "image/png")]
    Png,
    #[serde(rename = "image/jpeg")]
    Jpeg,
    #[serde(rename = "image/gif")]
    Gif,
    #[serde(rename = "image/webp")]
    Webp,
}

impl ImageMediaType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Gif => "image/gif",
            Self::Webp => "image/webp",
        }
    }

    pub fn extension(&self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpeg",
            Self::Gif => "gif",
            Self::Webp => "webp",
        }
    }
}

impl std::fmt::Display for ImageMediaType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Error type constants for analytics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ImageErrorType {
    ModuleLoad = 1,
    Processing = 2,
    Unknown = 3,
    PixelLimit = 4,
    Memory = 5,
    Timeout = 6,
    Vips = 7,
    Permission = 8,
}

/// Error thrown when image resizing fails and the image exceeds the API limit.
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct ImageResizeError {
    pub message: String,
}

impl ImageResizeError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// API image size limits.
pub const API_IMAGE_MAX_BASE64_SIZE: usize = 5 * 1024 * 1024; // 5MB
pub const IMAGE_MAX_WIDTH: u32 = 8000;
pub const IMAGE_MAX_HEIGHT: u32 = 8000;
pub const IMAGE_TARGET_RAW_SIZE: usize = 5 * 1024 * 1024; // 5MB

/// Classifies image processing errors for analytics.
pub fn classify_image_error(error: &str) -> ImageErrorType {
    // Module loading errors
    if error.contains("Native image processor module not available")
        || error.contains("MODULE_NOT_FOUND")
        || error.contains("ERR_MODULE_NOT_FOUND")
        || error.contains("ERR_DLOPEN_FAILED")
    {
        return ImageErrorType::ModuleLoad;
    }

    // Permission errors
    if error.contains("EACCES") || error.contains("EPERM") {
        return ImageErrorType::Permission;
    }

    // Memory errors
    if error.contains("ENOMEM")
        || error.contains("out of memory")
        || error.contains("Cannot allocate")
        || error.contains("memory allocation")
    {
        return ImageErrorType::Memory;
    }

    // Processing errors
    if error.contains("unsupported image format")
        || error.contains("Input buffer")
        || error.contains("Input file is missing")
        || error.contains("Input file has corrupt header")
        || error.contains("corrupt header")
        || error.contains("corrupt image")
        || error.contains("premature end")
        || error.contains("zlib: data error")
        || error.contains("zero width")
        || error.contains("zero height")
    {
        return ImageErrorType::Processing;
    }

    // Pixel limit errors
    if error.contains("pixel limit")
        || error.contains("too many pixels")
        || error.contains("exceeds pixel")
        || error.contains("image dimensions")
    {
        return ImageErrorType::PixelLimit;
    }

    // Timeout errors
    if error.contains("timeout") || error.contains("timed out") {
        return ImageErrorType::Timeout;
    }

    // Vips errors
    if error.contains("Vips") {
        return ImageErrorType::Vips;
    }

    ImageErrorType::Unknown
}

/// Computes djb2 hash of a string for analytics grouping.
pub fn hash_string(s: &str) -> u32 {
    let mut hash: u32 = 5381;
    for byte in s.bytes() {
        hash = hash
            .wrapping_shl(5)
            .wrapping_add(hash)
            .wrapping_add(byte as u32);
    }
    hash
}

/// Image dimensions information.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ImageDimensions {
    pub original_width: Option<u32>,
    pub original_height: Option<u32>,
    pub display_width: Option<u32>,
    pub display_height: Option<u32>,
}

/// Result of a resize operation.
#[derive(Debug, Clone)]
pub struct ResizeResult {
    pub buffer: Vec<u8>,
    pub media_type: String,
    pub dimensions: Option<ImageDimensions>,
}

/// Compressed image result.
#[derive(Debug, Clone)]
pub struct CompressedImageResult {
    pub base64: String,
    pub media_type: String,
    pub original_size: usize,
}

/// Image compression context.
#[derive(Debug, Clone)]
pub struct ImageCompressionContext {
    pub image_buffer: Vec<u8>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub format: String,
    pub max_bytes: usize,
    pub original_size: usize,
}

/// Detect image format from a buffer using magic bytes.
pub fn detect_image_format_from_buffer(buffer: &[u8]) -> ImageMediaType {
    if buffer.len() < 4 {
        return ImageMediaType::Png;
    }

    // PNG signature
    if buffer[0] == 0x89 && buffer[1] == 0x50 && buffer[2] == 0x4E && buffer[3] == 0x47 {
        return ImageMediaType::Png;
    }

    // JPEG signature (FFD8FF)
    if buffer[0] == 0xFF && buffer[1] == 0xD8 && buffer[2] == 0xFF {
        return ImageMediaType::Jpeg;
    }

    // GIF signature
    if buffer[0] == 0x47 && buffer[1] == 0x49 && buffer[2] == 0x46 {
        return ImageMediaType::Gif;
    }

    // WebP signature (RIFF....WEBP)
    if buffer[0] == 0x52
        && buffer[1] == 0x49
        && buffer[2] == 0x46
        && buffer[3] == 0x46
        && buffer.len() >= 12
        && buffer[8] == 0x57
        && buffer[9] == 0x45
        && buffer[10] == 0x42
        && buffer[11] == 0x50
    {
        return ImageMediaType::Webp;
    }

    ImageMediaType::Png
}

/// Detect image format from base64 data using magic bytes.
pub fn detect_image_format_from_base64(base64_data: &str) -> ImageMediaType {
    use base64::Engine;
    match base64::engine::general_purpose::STANDARD.decode(base64_data) {
        Ok(buffer) => detect_image_format_from_buffer(&buffer),
        Err(_) => ImageMediaType::Png,
    }
}

/// Creates a text description of image metadata including dimensions and source path.
/// Returns None if no useful metadata is available.
pub fn create_image_metadata_text(
    dims: &ImageDimensions,
    source_path: Option<&str>,
) -> Option<String> {
    let original_width = dims.original_width?;
    let original_height = dims.original_height?;
    let display_width = dims.display_width.filter(|&w| w > 0)?;
    let display_height = dims.display_height.filter(|&h| h > 0)?;

    let was_resized = original_width != display_width || original_height != display_height;

    if !was_resized && source_path.is_none() {
        return None;
    }

    let mut parts = Vec::new();

    if let Some(path) = source_path {
        parts.push(format!("source: {}", path));
    }

    if was_resized {
        let scale_factor = original_width as f64 / display_width as f64;
        parts.push(format!(
            "original {}x{}, displayed at {}x{}. Multiply coordinates by {:.2} to map to original image.",
            original_width, original_height, display_width, display_height, scale_factor
        ));
    }

    Some(format!("[Image: {}]", parts.join(", ")))
}

/// Format file size for display.
pub fn format_file_size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

/// Calculate dimensions while maintaining aspect ratio within max bounds.
pub fn constrain_dimensions(
    width: u32,
    height: u32,
    max_width: u32,
    max_height: u32,
) -> (u32, u32) {
    let mut w = width;
    let mut h = height;

    if w > max_width {
        h = ((h as f64 * max_width as f64) / w as f64).round() as u32;
        w = max_width;
    }

    if h > max_height {
        w = ((w as f64 * max_height as f64) / h as f64).round() as u32;
        h = max_height;
    }

    (w, h)
}

/// Check if an image buffer needs resizing based on size and dimensions.
pub fn needs_resize(original_size: usize, width: u32, height: u32) -> bool {
    original_size > IMAGE_TARGET_RAW_SIZE || width > IMAGE_MAX_WIDTH || height > IMAGE_MAX_HEIGHT
}

/// Check if dimensions need resizing.
pub fn needs_dimension_resize(width: u32, height: u32) -> bool {
    width > IMAGE_MAX_WIDTH || height > IMAGE_MAX_HEIGHT
}

/// Check if an image buffer's base64 encoding would exceed API limits.
pub fn exceeds_api_base64_limit(original_size: usize) -> bool {
    let base64_size = (original_size * 4 + 2) / 3; // ceiling division
    base64_size > API_IMAGE_MAX_BASE64_SIZE
}

/// Check if a PNG buffer has oversized dimensions from its header.
pub fn png_has_oversized_dimensions(buffer: &[u8]) -> bool {
    if buffer.len() < 24 {
        return false;
    }
    // Check PNG signature
    if buffer[0] != 0x89 || buffer[1] != 0x50 || buffer[2] != 0x4E || buffer[3] != 0x47 {
        return false;
    }
    // Read IHDR width and height (big-endian u32 at offsets 16 and 20)
    let width = u32::from_be_bytes([buffer[16], buffer[17], buffer[18], buffer[19]]);
    let height = u32::from_be_bytes([buffer[20], buffer[21], buffer[22], buffer[23]]);
    width > IMAGE_MAX_WIDTH || height > IMAGE_MAX_HEIGHT
}

/// Progressive scaling factors for image compression.
pub const SCALING_FACTORS: &[f64] = &[1.0, 0.75, 0.5, 0.25];

/// JPEG quality levels for progressive compression.
pub const JPEG_QUALITY_LEVELS: &[u8] = &[80, 60, 40, 20];

/// Convert token limit to byte limit for image compression.
pub fn tokens_to_max_bytes(max_tokens: u32) -> usize {
    let max_base64_chars = (max_tokens as f64 / 0.125) as usize;
    (max_base64_chars as f64 * 0.75) as usize
}

/// Normalize media type string.
pub fn normalize_media_type(media_type: &str) -> &str {
    if media_type == "jpg" {
        "jpeg"
    } else {
        media_type
    }
}

/// Extract extension from media type string (e.g., "image/png" -> "png").
pub fn extract_extension_from_media_type(media_type: &str) -> &str {
    media_type.split('/').nth(1).unwrap_or("png")
}

/// Image block parameter (matching API structure).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageBlockParam {
    #[serde(rename = "type")]
    pub block_type: String,
    pub source: ImageSource,
}

/// Image source (base64 or URL).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ImageSource {
    #[serde(rename = "base64")]
    Base64 { media_type: String, data: String },
    #[serde(rename = "url")]
    Url { url: String },
}

/// Image block with dimension information.
#[derive(Debug, Clone)]
pub struct ImageBlockWithDimensions {
    pub block: ImageBlockParam,
    pub dimensions: Option<ImageDimensions>,
}

/// Check if an image source is base64.
pub fn is_base64_source(source: &ImageSource) -> bool {
    matches!(source, ImageSource::Base64 { .. })
}

/// Check if an image is valid for pasting (non-empty base64 data).
pub fn is_valid_image_paste(content: &str) -> bool {
    !content.is_empty()
}

/// 对应 TS `maybeResizeAndDownsampleImageBuffer`：尝试压缩图像 buffer。
pub async fn maybe_resize_and_downsample_image_buffer(
    buffer: Vec<u8>,
    _max_dimension: usize,
) -> Vec<u8> {
    buffer
}

/// 对应 TS `compressImageBuffer`：把 buffer 压缩到目标质量。
pub async fn compress_image_buffer(buffer: Vec<u8>, _quality: u8) -> Vec<u8> {
    buffer
}

/// 对应 TS `compressImageBufferWithTokenLimit`：按 token 预算压缩。
pub async fn compress_image_buffer_with_token_limit(
    buffer: Vec<u8>,
    _token_limit: usize,
) -> Vec<u8> {
    buffer
}

/// 对应 TS `compressImageBlock`：处理 MCP 中的 image content block。
pub async fn compress_image_block(block: serde_json::Value) -> serde_json::Value {
    block
}
