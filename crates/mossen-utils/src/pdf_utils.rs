//! PDF 工具函数。
//!
//! 翻译自 `utils/pdfUtils.ts`。

use std::collections::HashSet;
use std::sync::LazyLock;

use crate::model_cost::get_canonical_name;

/// 特殊处理的文档扩展名集合。
pub static DOCUMENT_EXTENSIONS: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| HashSet::from(["pdf"]));

/// PDF 页面范围。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PdfPageRange {
    pub first_page: u32,
    pub last_page: Option<u32>,
}

/// 解析页面范围字符串为 first_page/last_page 数值。
///
/// 支持的格式：
/// - "5" → { first_page: 5, last_page: Some(5) }
/// - "1-10" → { first_page: 1, last_page: Some(10) }
/// - "3-" → { first_page: 3, last_page: None } (表示无上界)
///
/// 输入无效时返回 None（非数字、零、反转范围）。
/// 页码从 1 开始。
pub fn parse_pdf_page_range(pages: &str) -> Option<PdfPageRange> {
    let trimmed = pages.trim();
    if trimmed.is_empty() {
        return None;
    }

    // "N-" 开放式范围
    if trimmed.ends_with('-') {
        let first: u32 = trimmed[..trimmed.len() - 1].parse().ok()?;
        if first < 1 {
            return None;
        }
        return Some(PdfPageRange {
            first_page: first,
            last_page: None,
        });
    }

    let dash_index = trimmed.find('-');
    if dash_index.is_none() {
        // 单页: "5"
        let page: u32 = trimmed.parse().ok()?;
        if page < 1 {
            return None;
        }
        return Some(PdfPageRange {
            first_page: page,
            last_page: Some(page),
        });
    }

    // 范围: "1-10"
    let dash_index = dash_index.unwrap();
    let first: u32 = trimmed[..dash_index].parse().ok()?;
    let last: u32 = trimmed[dash_index + 1..].parse().ok()?;
    if first < 1 || last < 1 || last < first {
        return None;
    }
    Some(PdfPageRange {
        first_page: first,
        last_page: Some(last),
    })
}

/// 检查当前模型是否支持 PDF 阅读。
///
/// PDF 文档块适用于所有提供商（1P, Vertex, Bedrock, Foundry）。
/// Fast 3 是唯一不支持 PDF 的旧模型；使用该模型时回退到
/// 页面提取路径（poppler-utils）。子串匹配覆盖所有提供商 ID 格式。
pub fn is_pdf_supported(model: &str) -> bool {
    !get_canonical_name(model).contains("mossen-3-fast")
}

/// 检查文件扩展名是否为 PDF 文档。
///
/// `ext` - 文件扩展名（带或不带前导点号）。
pub fn is_pdf_extension(ext: &str) -> bool {
    let normalized = if let Some(stripped) = ext.strip_prefix('.') {
        stripped
    } else {
        ext
    };
    DOCUMENT_EXTENSIONS.contains(normalized.to_lowercase().as_str())
}
