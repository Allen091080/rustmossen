//! # terminal — 终端文本截断与换行渲染
//!
//! 对应 TypeScript `utils/terminal.ts`。
//! 提供终端内容的行级截断渲染功能。

use unicode_width::UnicodeWidthStr;

use crate::string_utils::prefix_chars;

/// 最多显示的行数
const MAX_LINES_TO_SHOW: usize = 3;

/// 防止溢出的填充字符数
const PADDING_TO_PREVENT_OVERFLOW: usize = 10;

/// 行换行结果
struct WrapResult {
    above_the_fold: String,
    remaining_lines: usize,
}

/// 将文本按可见宽度换行，返回折叠区域和剩余行数。
///
/// 使用 unicode-width 计算可见字符宽度（类似 TS 中的 stringWidth）。
fn wrap_text(text: &str, wrap_width: usize) -> WrapResult {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut wrapped_lines: Vec<String> = Vec::new();

    for line in &lines {
        let visible_width = UnicodeWidthStr::width(*line);
        if visible_width <= wrap_width {
            wrapped_lines.push(line.trim_end().to_string());
        } else {
            // Break long lines into chunks of wrap_width visible characters.
            // We iterate by character, accumulating visible width.
            let mut position = 0;
            let chars: Vec<char> = line.chars().collect();
            while position < chars.len() {
                let mut chunk = String::new();
                let mut chunk_width = 0;
                let mut i = position;
                while i < chars.len() && chunk_width < wrap_width {
                    let ch = chars[i];
                    let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
                    if chunk_width + ch_width > wrap_width && chunk_width > 0 {
                        break;
                    }
                    chunk.push(ch);
                    chunk_width += ch_width;
                    i += 1;
                }
                if i == position {
                    // Prevent infinite loop for zero-width chars
                    i += 1;
                }
                wrapped_lines.push(chunk.trim_end().to_string());
                position = i;
            }
        }
    }

    let total = wrapped_lines.len();
    let remaining_lines = if total > MAX_LINES_TO_SHOW {
        total - MAX_LINES_TO_SHOW
    } else {
        0
    };

    // If there's only 1 line after the fold, show it directly
    if remaining_lines == 1 {
        let fold = wrapped_lines[..MAX_LINES_TO_SHOW + 1].join("\n");
        return WrapResult {
            above_the_fold: fold.trim_end().to_string(),
            remaining_lines: 0,
        };
    }

    let fold = wrapped_lines[..std::cmp::min(MAX_LINES_TO_SHOW, total)].join("\n");
    WrapResult {
        above_the_fold: fold.trim_end().to_string(),
        remaining_lines,
    }
}

/// 渲染截断内容以在终端显示。
///
/// 如果内容超过最大行数，截断内容并附加一条消息指示剩余行数。
///
/// # 参数
/// - `content`: 要渲染的内容
/// - `terminal_width`: 终端宽度（用于换行）
/// - `suppress_expand_hint`: 是否隐藏展开提示
///
/// # 返回
/// 截断后的渲染内容字符串
pub fn render_truncated_content(
    content: &str,
    terminal_width: usize,
    suppress_expand_hint: bool,
) -> String {
    let trimmed_content = content.trim_end();
    if trimmed_content.is_empty() {
        return String::new();
    }

    let wrap_width = if terminal_width > PADDING_TO_PREVENT_OVERFLOW {
        terminal_width - PADDING_TO_PREVENT_OVERFLOW
    } else {
        10
    };

    // Only process enough content for the visible lines. Avoids O(n) wrapping
    // on huge outputs (e.g. 64MB binary dumps that cause 382K-row screens).
    let max_chars = MAX_LINES_TO_SHOW * wrap_width * 4;
    let pre_truncated = trimmed_content.chars().count() > max_chars;
    let content_for_wrapping = if pre_truncated {
        prefix_chars(trimmed_content, max_chars)
    } else {
        trimmed_content.to_string()
    };

    let WrapResult {
        above_the_fold,
        remaining_lines,
    } = wrap_text(&content_for_wrapping, wrap_width);

    let estimated_remaining = if pre_truncated {
        let estimated = trimmed_content.len() / wrap_width.max(1);
        let past_fold = if estimated > MAX_LINES_TO_SHOW {
            estimated - MAX_LINES_TO_SHOW
        } else {
            0
        };
        remaining_lines.max(past_fold)
    } else {
        remaining_lines
    };

    if estimated_remaining > 0 {
        let hint = if suppress_expand_hint {
            String::new()
        } else {
            " (ctrl+o to expand)".to_string()
        };
        format!(
            "{}\n\u{2026} +{} lines{}",
            above_the_fold, estimated_remaining, hint
        )
    } else {
        above_the_fold
    }
}

/// 快速检查: OutputLine 是否会截断这个内容?
///
/// 仅计算原始换行符（忽略终端宽度换行），因此对于一行超长文本
/// 可能返回 false——这是可接受的，因为常见情况是多行输出。
pub fn is_output_line_truncated(content: &str) -> bool {
    let mut pos = 0;
    // Need more than MAX_LINES_TO_SHOW newlines (content fills > 3 lines).
    // The +1 accounts for wrapText showing an extra line when remainingLines==1.
    for _ in 0..=MAX_LINES_TO_SHOW {
        match content[pos..].find('\n') {
            Some(idx) => pos += idx + 1,
            None => return false,
        }
    }
    // A trailing newline is a terminator, not a new line — match
    // renderTruncatedContent's trimEnd() behavior.
    pos < content.len()
}
