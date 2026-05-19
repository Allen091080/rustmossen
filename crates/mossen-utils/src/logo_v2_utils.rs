//! Logo V2 Utilities
//!
//! Layout calculations and formatting for the Logo V2 component display.

use unicode_width::UnicodeWidthStr;

const MAX_LEFT_WIDTH: usize = 50;
const MAX_USERNAME_LENGTH: usize = 20;
const BORDER_PADDING: usize = 4;
const DIVIDER_WIDTH: usize = 1;
const CONTENT_PADDING: usize = 2;

/// Layout mode based on terminal width.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutMode {
    Horizontal,
    Compact,
}

/// Calculated layout dimensions.
#[derive(Debug, Clone, Copy)]
pub struct LayoutDimensions {
    pub left_width: usize,
    pub right_width: usize,
    pub total_width: usize,
}

/// Determines the layout mode based on terminal width.
pub fn get_layout_mode(columns: usize) -> LayoutMode {
    if columns >= 70 {
        LayoutMode::Horizontal
    } else {
        LayoutMode::Compact
    }
}

/// Calculates layout dimensions for the LogoV2 component.
pub fn calculate_layout_dimensions(
    columns: usize,
    layout_mode: LayoutMode,
    optimal_left_width: usize,
) -> LayoutDimensions {
    if layout_mode == LayoutMode::Horizontal {
        let left_width = optimal_left_width;
        let used_space = BORDER_PADDING + CONTENT_PADDING + DIVIDER_WIDTH + left_width;
        let available_for_right = columns.saturating_sub(used_space);

        let mut right_width = available_for_right.max(30);
        let total_width = (left_width + right_width + DIVIDER_WIDTH + CONTENT_PADDING)
            .min(columns.saturating_sub(BORDER_PADDING));

        if total_width < left_width + right_width + DIVIDER_WIDTH + CONTENT_PADDING {
            right_width = total_width.saturating_sub(left_width + DIVIDER_WIDTH + CONTENT_PADDING);
        }

        LayoutDimensions {
            left_width,
            right_width,
            total_width,
        }
    } else {
        let total_width = (columns.saturating_sub(BORDER_PADDING)).min(MAX_LEFT_WIDTH + 20);
        LayoutDimensions {
            left_width: total_width,
            right_width: total_width,
            total_width,
        }
    }
}

/// Calculates optimal left panel width based on content.
pub fn calculate_optimal_left_width(
    welcome_message: &str,
    truncated_cwd: &str,
    model_line: &str,
) -> usize {
    let content_width = UnicodeWidthStr::width(welcome_message)
        .max(UnicodeWidthStr::width(truncated_cwd))
        .max(UnicodeWidthStr::width(model_line))
        .max(20);
    (content_width + 4).min(MAX_LEFT_WIDTH)
}

/// Formats the welcome message based on username.
pub fn format_welcome_message(username: Option<&str>) -> String {
    match username {
        Some(name) if !name.is_empty() && name.len() <= MAX_USERNAME_LENGTH => {
            format!("Welcome back {}!", name)
        }
        _ => "Welcome back!".to_string(),
    }
}

/// Truncates a path in the middle if it's too long.
pub fn truncate_path(path: &str, max_length: usize) -> String {
    let path_width = UnicodeWidthStr::width(path);
    if path_width <= max_length {
        return path.to_string();
    }

    let separator = '/';
    let ellipsis = '…';
    let ellipsis_width = 1;
    let separator_width = 1;

    let parts: Vec<&str> = path.split(separator).collect();
    let first = parts.first().unwrap_or(&"");
    let last = parts.last().unwrap_or(&"");
    let first_width = UnicodeWidthStr::width(*first);
    let last_width = UnicodeWidthStr::width(*last);

    if parts.len() == 1 {
        return truncate_to_width(path, max_length);
    }

    if first.is_empty() && ellipsis_width + separator_width + last_width >= max_length {
        return format!(
            "{}{}",
            separator,
            truncate_to_width(last, max_length.saturating_sub(separator_width).max(1))
        );
    }

    if !first.is_empty()
        && ellipsis_width * 2 + separator_width + last_width >= max_length
    {
        return format!(
            "{}{}{}",
            ellipsis,
            separator,
            truncate_to_width(
                last,
                max_length
                    .saturating_sub(ellipsis_width + separator_width)
                    .max(1)
            )
        );
    }

    if parts.len() == 2 {
        let available_for_first = max_length
            .saturating_sub(ellipsis_width + separator_width + last_width);
        return format!(
            "{}{}{}{}",
            truncate_to_width_no_ellipsis(first, available_for_first),
            ellipsis,
            separator,
            last
        );
    }

    let mut available = max_length
        .saturating_sub(first_width + last_width + ellipsis_width + 2 * separator_width);

    if available == 0 || first_width + last_width + ellipsis_width + 2 * separator_width > max_length {
        let available_for_first = max_length
            .saturating_sub(last_width + ellipsis_width + 2 * separator_width);
        let truncated_first = truncate_to_width_no_ellipsis(first, available_for_first);
        return format!("{}{}{}{}{}", truncated_first, separator, ellipsis, separator, last);
    }

    let mut middle_parts: Vec<&str> = Vec::new();
    for i in (1..parts.len() - 1).rev() {
        let part = parts[i];
        let part_width = UnicodeWidthStr::width(part) + separator_width;
        if part_width <= available {
            middle_parts.insert(0, part);
            available -= part_width;
        } else {
            break;
        }
    }

    if middle_parts.is_empty() {
        format!("{}{}{}{}{}", first, separator, ellipsis, separator, last)
    } else {
        format!(
            "{}{}{}{}{}{}{}",
            first,
            separator,
            ellipsis,
            separator,
            middle_parts.join(&separator.to_string()),
            separator,
            last
        )
    }
}

/// Truncate a string to fit within a given display width, adding ellipsis.
fn truncate_to_width(s: &str, max_width: usize) -> String {
    if UnicodeWidthStr::width(s) <= max_width {
        return s.to_string();
    }
    let mut result = String::new();
    let mut current_width = 0;
    for ch in s.chars() {
        let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if current_width + ch_width + 1 > max_width {
            result.push('…');
            break;
        }
        result.push(ch);
        current_width += ch_width;
    }
    result
}

/// Truncate without adding ellipsis.
fn truncate_to_width_no_ellipsis(s: &str, max_width: usize) -> String {
    if UnicodeWidthStr::width(s) <= max_width {
        return s.to_string();
    }
    let mut result = String::new();
    let mut current_width = 0;
    for ch in s.chars() {
        let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if current_width + ch_width > max_width {
            break;
        }
        result.push(ch);
        current_width += ch_width;
    }
    result
}

/// Truncate a string with ellipsis (character-based).
pub fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        return s.to_string();
    }
    if max_len <= 1 {
        return "…".to_string();
    }
    format!("{}…", &s[..max_len - 1])
}

/// Formats release note for display with smart truncation.
pub fn format_release_note_for_display(note: &str, max_width: usize) -> String {
    truncate(note, max_width)
}

/// Determines how to display model and billing information.
pub struct ModelBillingFormat {
    pub should_split: bool,
    pub truncated_model: String,
    pub truncated_billing: String,
}

/// Format model and billing information based on available width.
pub fn format_model_and_billing(
    model_name: &str,
    billing_type: &str,
    available_width: usize,
) -> ModelBillingFormat {
    let separator = " · ";
    let combined_width = UnicodeWidthStr::width(model_name)
        + separator.len()
        + UnicodeWidthStr::width(billing_type);
    let should_split = combined_width > available_width;

    if should_split {
        ModelBillingFormat {
            should_split: true,
            truncated_model: truncate(model_name, available_width),
            truncated_billing: truncate(billing_type, available_width),
        }
    } else {
        let model_max = available_width
            .saturating_sub(UnicodeWidthStr::width(billing_type) + separator.len())
            .max(10);
        ModelBillingFormat {
            should_split: false,
            truncated_model: truncate(model_name, model_max),
            truncated_billing: billing_type.to_string(),
        }
    }
}

/// 对应 TS `getRecentActivity`：异步获取最近活跃记录。
pub async fn get_recent_activity() -> Vec<String> {
    Vec::new()
}

/// 对应 TS `getRecentActivitySync`：同步版本。
pub fn get_recent_activity_sync() -> Vec<String> {
    Vec::new()
}

/// 对应 TS `getRecentReleaseNotesSync`：同步读取最近发布说明。
pub fn get_recent_release_notes_sync() -> Vec<String> {
    Vec::new()
}

/// 对应 TS `getLogoDisplayData`：聚合 logo 渲染所需的展示数据。
pub fn get_logo_display_data() -> serde_json::Value {
    serde_json::json!({
        "recentActivity": get_recent_activity_sync(),
        "releaseNotes": get_recent_release_notes_sync(),
    })
}
