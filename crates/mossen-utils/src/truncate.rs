//! 字符串截断工具
//!
//! 对应 TS `truncate.ts`。

use unicode_segmentation::UnicodeSegmentation;

/// 在字符串中间截断路径，保留目录上下文和文件名。
///
/// 宽度感知：使用 stringWidth() 进行正确的 CJK/emoji 测量。
///
/// # 示例
/// "src/components/deeply/nested/folder/MyComponent.tsx" 
/// 当 max_length 为 30 时变为 "src/components/…/MyComponent.tsx"
pub fn truncate_path_middle(path: &str, max_length: usize) -> String {
    use std::path::Path;

    // 无需截断
    if path.width() <= max_length {
        return path.to_string();
    }

    // 处理边缘情况
    if max_length == 0 {
        return "…".to_string();
    }

    if max_length < 5 {
        return truncate_to_width(path, max_length);
    }

    // 查找文件名
    let path_obj = Path::new(path);
    let filename = path_obj.file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string());
    
    let directory = path_obj.parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    
    let filename_width = filename.width();

    // 如果文件名本身就太长，从开头截断
    if filename_width >= max_length.saturating_sub(1) {
        return truncate_start_to_width(path, max_length);
    }

    // 计算目录前缀可用空间
    // 结果格式: directory + "…" + filename
    let available_for_dir = max_length.saturating_sub(1).saturating_sub(filename_width);

    if available_for_dir == 0 {
        return truncate_start_to_width(&filename, max_length);
    }

    // 截断目录并组合
    let truncated_dir = truncate_to_width_no_ellipsis(&directory, available_for_dir);
    format!("{}…{}", truncated_dir, filename)
}

/// 截断字符串以适应最大显示宽度。
///
/// 在字素边界分割以避免破坏 emoji 或代理对。
/// 发生截断时附加 '…'。
pub fn truncate_to_width(text: &str, max_width: usize) -> String {
    if text.width() <= max_width {
        return text.to_string();
    }
    if max_width <= 1 {
        return "…".to_string();
    }
    
    let mut result = String::new();
    let mut width = 0;
    
    for segment in text.graphemes(true) {
        let seg_width = segment.width();
        if width + seg_width > max_width.saturating_sub(1) {
            break;
        }
        result.push_str(segment);
        width += seg_width;
    }
    
    format!("{}…", result)
}

/// 从字符串开头截断，保留尾部。
///
/// 发生截断时前置 '…'。
/// 宽度感知和字素安全。
pub fn truncate_start_to_width(text: &str, max_width: usize) -> String {
    if text.width() <= max_width {
        return text.to_string();
    }
    if max_width <= 1 {
        return "…".to_string();
    }
    
    let segments: Vec<&str> = text.graphemes(true).collect();
    let mut width = 0;
    let mut start_idx = segments.len();
    
    for i in (0..segments.len()).rev() {
        let seg_width = segments[i].width();
        if width + seg_width > max_width.saturating_sub(1) {
            break;
        }
        width += seg_width;
        start_idx = i;
    }
    
    format!("{}{}", "…", segments[start_idx..].concat())
}

/// 截断字符串以适应最大显示宽度，不附加省略号。
///
/// 当调用者添加自己的分隔符时很有用（例如使用 '…' 在中间截断）。
pub fn truncate_to_width_no_ellipsis(text: &str, max_width: usize) -> String {
    if text.width() <= max_width {
        return text.to_string();
    }
    if max_width == 0 {
        return String::new();
    }
    
    let mut result = String::new();
    let mut width = 0;
    
    for segment in text.graphemes(true) {
        let seg_width = segment.width();
        if width + seg_width > max_width {
            break;
        }
        result.push_str(segment);
        width += seg_width;
    }
    
    result
}

/// 截断字符串以适应最大显示宽度。
///
/// 发生截断时附加 '…'。
pub fn truncate(text: &str, max_width: usize, single_line: bool) -> String {
    let mut result = text.to_string();
    
    if single_line {
        if let Some(pos) = result.find('\n') {
            result = result[..pos].to_string();
            if result.width() + 1 > max_width {
                return truncate_to_width(&result, max_width);
            }
            return format!("{}…", result);
        }
    }
    
    if result.width() <= max_width {
        return result;
    }
    truncate_to_width(&result, max_width)
}

/// 文本换行。
pub fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current_line = String::new();
    let mut current_width = 0;
    
    for segment in text.graphemes(true) {
        let seg_width = segment.width();
        if current_width + seg_width <= width {
            current_line.push_str(segment);
            current_width += seg_width;
        } else {
            if !current_line.is_empty() {
                lines.push(current_line.clone());
            }
            current_line = segment.to_string();
            current_width = seg_width;
        }
    }
    
    if !current_line.is_empty() {
        lines.push(current_line);
    }
    
    lines
}

// 字符串宽度计算辅助
trait StrWidth {
    fn width(&self) -> usize;
}

impl StrWidth for str {
    fn width(&self) -> usize {
        // 简化的宽度计算：ASCII 字符宽度为 1，CJK 字符宽度为 2
        self.chars().map(|c| {
            if c.is_ascii() { 1 } else { 2 }
        }).sum()
    }
}
