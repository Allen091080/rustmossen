//! # Figures (figures.ts)
//!
//! Unicode 字符/图标常量。

/// 黑色圆点（macOS 使用 ⏺，其他平台使用 ●）。
#[cfg(target_os = "macos")]
pub const BLACK_CIRCLE: char = '⏺';
#[cfg(not(target_os = "macos"))]
pub const BLACK_CIRCLE: char = '●';

pub const BULLET_OPERATOR: char = '∙';
pub const TEARDROP_ASTERISK: char = '✻';
pub const UP_ARROW: char = '\u{2191}'; // ↑ - used for max 1m merge notice
pub const DOWN_ARROW: char = '\u{2193}'; // ↓ - used for scroll hint
pub const LIGHTNING_BOLT: char = '\u{21af}'; // ↯ - used for fast mode indicator
pub const EFFORT_LOW: char = '○'; // \u25cb - effort level: low
pub const EFFORT_MEDIUM: char = '◐'; // \u25d0 - effort level: medium
pub const EFFORT_HIGH: char = '●'; // \u25cf - effort level: high
pub const EFFORT_MAX: char = '◉'; // \u25c9 - effort level: max (Max 4.6 only)

// Media/trigger status indicators
pub const PLAY_ICON: char = '\u{25b6}'; // ▶
pub const PAUSE_ICON: char = '\u{23f8}'; // ⏸

// MCP subscription indicators
pub const REFRESH_ARROW: char = '\u{21bb}'; // ↻ - used for resource update indicator
pub const CHANNEL_ARROW: char = '\u{2190}'; // ← - inbound channel message indicator
pub const INJECTED_ARROW: char = '\u{2192}'; // → - cross-session injected message indicator
pub const FORK_GLYPH: char = '\u{2442}'; // ⑂ - fork directive indicator

// Review status indicators (ultrareview diamond states)
pub const DIAMOND_OPEN: char = '\u{25c7}'; // ◇ - running
pub const DIAMOND_FILLED: char = '\u{25c6}'; // ◆ - completed/failed
pub const REFERENCE_MARK: char = '\u{203b}'; // ※ - komejirushi, away-summary recap marker

// Issue flag indicator
pub const FLAG_ICON: char = '\u{2691}'; // ⚑ - used for issue flag banner

// Blockquote indicator
pub const BLOCKQUOTE_BAR: char = '\u{258e}'; // ▎ - left one-quarter block, used as blockquote line prefix
pub const HEAVY_HORIZONTAL: char = '\u{2501}'; // ━ - heavy box-drawing horizontal

// Bridge status indicators
pub const BRIDGE_SPINNER_FRAMES: &[&str] = &[
    "\u{00b7}|\u{00b7}",
    "\u{00b7}/\u{00b7}",
    "\u{00b7}\u{2014}\u{00b7}",
    "\u{00b7}\\\u{00b7}",
];
pub const BRIDGE_READY_INDICATOR: &str = "\u{00b7}\u{2714}\u{fe0e}\u{00b7}";
pub const BRIDGE_FAILED_INDICATOR: &str = "\u{00d7}";
