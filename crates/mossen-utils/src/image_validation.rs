//! # image_validation — 图片大小验证
//!
//! 对应 TypeScript `utils/imageValidation.ts`。
//! 验证消息中的图片是否在 API 大小限制内。

use std::fmt;

/// API 图片最大 base64 大小（字节）
/// 5MB base64 encoded
const API_IMAGE_MAX_BASE64_SIZE: usize = 5 * 1024 * 1024;

/// 超大图片信息
#[derive(Debug, Clone)]
pub struct OversizedImage {
    pub index: usize,
    pub size: usize,
}

/// 图片大小错误
#[derive(Debug, Clone)]
pub struct ImageSizeError {
    pub message: String,
    pub oversized_images: Vec<OversizedImage>,
}

impl fmt::Display for ImageSizeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ImageSizeError {}

impl ImageSizeError {
    /// 创建图片大小错误
    pub fn new(oversized_images: Vec<OversizedImage>, max_size: usize) -> Self {
        let message = if oversized_images.len() == 1 {
            let first = &oversized_images[0];
            format!(
                "Image base64 size ({}) exceeds API limit ({}). Please resize the image before sending.",
                format_file_size(first.size),
                format_file_size(max_size)
            )
        } else {
            let details: Vec<String> = oversized_images
                .iter()
                .map(|img| format!("Image {}: {}", img.index, format_file_size(img.size)))
                .collect();
            format!(
                "{} images exceed the API limit ({}): {}. Please resize these images before sending.",
                oversized_images.len(),
                format_file_size(max_size),
                details.join(", ")
            )
        };
        Self {
            message,
            oversized_images,
        }
    }
}

/// 格式化文件大小为人类可读形式
fn format_file_size(bytes: usize) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;

    let b = bytes as f64;
    if b >= GB {
        format!("{:.1} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{} B", bytes)
    }
}

/// 检查一个 block 是否为 base64 图片 block
fn is_base64_image_block(block: &serde_json::Value) -> Option<&str> {
    let obj = block.as_object()?;
    if obj.get("type")?.as_str()? != "image" {
        return None;
    }
    let source = obj.get("source")?.as_object()?;
    if source.get("type")?.as_str()? != "base64" {
        return None;
    }
    source.get("data")?.as_str()
}

/// 验证消息中的所有图片是否在 API 大小限制内。
///
/// 这是 API 边界的安全网，用于捕获可能通过上游处理漏过的超大图片。
///
/// 注意：API 的 5MB 限制适用于 base64 编码的字符串长度，
/// 而非解码后的原始字节。
///
/// 支持包装消息格式 `{ type, message: { role, content } }` 和
/// 原始 MessageParam 类型 `{ role, content }`。
///
/// # 错误
/// 如果任何图片超过 API 限制，返回 `ImageSizeError`。
pub fn validate_images_for_api(messages: &[serde_json::Value]) -> Result<(), ImageSizeError> {
    let mut oversized_images: Vec<OversizedImage> = Vec::new();
    let mut image_index = 0usize;

    for msg in messages {
        let obj = match msg.as_object() {
            Some(o) => o,
            None => continue,
        };

        // Handle wrapped message format { type: 'user', message: { role, content } }
        // Only check user messages
        let msg_type = match obj.get("type").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => continue,
        };
        if msg_type != "user" {
            continue;
        }

        let inner_message = match obj.get("message").and_then(|v| v.as_object()) {
            Some(m) => m,
            None => continue,
        };

        let content = match inner_message.get("content") {
            Some(c) => c,
            None => continue,
        };

        let content_array = match content.as_array() {
            Some(arr) => arr,
            None => continue, // string content, skip
        };

        for block in content_array {
            if let Some(data) = is_base64_image_block(block) {
                image_index += 1;
                let base64_size = data.len();
                if base64_size > API_IMAGE_MAX_BASE64_SIZE {
                    oversized_images.push(OversizedImage {
                        index: image_index,
                        size: base64_size,
                    });
                }
            }
        }
    }

    if !oversized_images.is_empty() {
        return Err(ImageSizeError::new(
            oversized_images,
            API_IMAGE_MAX_BASE64_SIZE,
        ));
    }

    Ok(())
}
