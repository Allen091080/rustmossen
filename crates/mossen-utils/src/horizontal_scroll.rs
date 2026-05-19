/// Horizontal scroll window result
#[derive(Debug, Clone, PartialEq)]
pub struct HorizontalScrollWindow {
    pub start_index: usize,
    pub end_index: usize,
    pub show_left_arrow: bool,
    pub show_right_arrow: bool,
}

/// Calculate the visible window of items that fit within available width,
/// ensuring the selected item is always visible. Uses edge-based scrolling:
/// the window only scrolls when the selected item would be outside the visible
/// range, and positions the selected item at the edge (not centered).
///
/// * `item_widths` - Array of item widths (each width should include separator if applicable)
/// * `available_width` - Total available width for items
/// * `arrow_width` - Width of scroll indicator arrow (including space)
/// * `selected_idx` - Index of selected item (must stay visible)
/// * `first_item_has_separator` - Whether first item's width includes a separator that should be ignored
pub fn calculate_horizontal_scroll_window(
    item_widths: &[usize],
    available_width: usize,
    arrow_width: usize,
    selected_idx: usize,
    first_item_has_separator: bool,
) -> HorizontalScrollWindow {
    let total_items = item_widths.len();

    if total_items == 0 {
        return HorizontalScrollWindow {
            start_index: 0,
            end_index: 0,
            show_left_arrow: false,
            show_right_arrow: false,
        };
    }

    // Clamp selectedIdx to valid range
    let clamped_selected = selected_idx.min(total_items - 1);

    // If all items fit, show them all
    let total_width: usize = item_widths.iter().sum();
    if total_width <= available_width {
        return HorizontalScrollWindow {
            start_index: 0,
            end_index: total_items,
            show_left_arrow: false,
            show_right_arrow: false,
        };
    }

    // Calculate cumulative widths for efficient range calculations
    let mut cumulative_widths = vec![0usize; total_items + 1];
    for i in 0..total_items {
        cumulative_widths[i + 1] = cumulative_widths[i] + item_widths[i];
    }

    // Helper to get width of range [start, end)
    let range_width = |start: usize, end: usize| -> usize {
        let base_width = cumulative_widths[end] - cumulative_widths[start];
        if first_item_has_separator && start > 0 {
            base_width.saturating_sub(1)
        } else {
            base_width
        }
    };

    // Calculate effective available width based on whether we'll show arrows
    let get_effective_width = |start: usize, end: usize| -> usize {
        let mut width = available_width;
        if start > 0 {
            width = width.saturating_sub(arrow_width);
        }
        if end < total_items {
            width = width.saturating_sub(arrow_width);
        }
        width
    };

    // Edge-based scrolling: Start from the beginning and only scroll when necessary
    let mut start_index = 0usize;
    let mut end_index = 1usize;

    // Expand from start as much as possible
    while end_index < total_items
        && range_width(start_index, end_index + 1)
            <= get_effective_width(start_index, end_index + 1)
    {
        end_index += 1;
    }

    // If selected is within visible range, we're done
    if clamped_selected >= start_index && clamped_selected < end_index {
        return HorizontalScrollWindow {
            start_index,
            end_index,
            show_left_arrow: start_index > 0,
            show_right_arrow: end_index < total_items,
        };
    }

    // Selected is outside visible range - need to scroll
    if clamped_selected >= end_index {
        // Selected is to the right - scroll so selected is at the right edge
        end_index = clamped_selected + 1;
        start_index = clamped_selected;

        // Expand left as much as possible (selected stays at right edge)
        while start_index > 0
            && range_width(start_index - 1, end_index)
                <= get_effective_width(start_index - 1, end_index)
        {
            start_index -= 1;
        }
    } else {
        // Selected is to the left - scroll so selected is at the left edge
        start_index = clamped_selected;
        end_index = clamped_selected + 1;

        // Expand right as much as possible (selected stays at left edge)
        while end_index < total_items
            && range_width(start_index, end_index + 1)
                <= get_effective_width(start_index, end_index + 1)
        {
            end_index += 1;
        }
    }

    HorizontalScrollWindow {
        start_index,
        end_index,
        show_left_arrow: start_index > 0,
        show_right_arrow: end_index < total_items,
    }
}
