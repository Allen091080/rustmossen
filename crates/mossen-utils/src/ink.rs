//! Ink color conversion utilities.
//!
//! This module provides utilities for converting colors to Ink's TextProps format.

/// Default theme color for agent output
pub const DEFAULT_AGENT_THEME_COLOR: &str = "cyan_FOR_SUBAGENTS_ONLY";

/// Agent color name type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentColorName {
    Blue,
    Green,
    Red,
    Yellow,
    Cyan,
    Magenta,
    White,
    Black,
}

impl AgentColorName {
    /// Parse a color name string into AgentColorName
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "blue" => Some(Self::Blue),
            "green" => Some(Self::Green),
            "red" => Some(Self::Red),
            "yellow" => Some(Self::Yellow),
            "cyan" => Some(Self::Cyan),
            "magenta" => Some(Self::Magenta),
            "white" => Some(Self::White),
            "black" => Some(Self::Black),
            _ => None,
        }
    }
}

/// Mapping from agent colors to theme colors
/// In the TypeScript version, this maps AGENT_COLOR_TO_THEME_COLOR
fn get_theme_color(agent_color: &str) -> Option<&'static str> {
    match agent_color {
        "blue" => Some("blue"),
        "green" => Some("green"),
        "red" => Some("red"),
        "yellow" => Some("yellow"),
        "cyan" => Some("cyan_FOR_SUBAGENTS_ONLY"),
        "magenta" => Some("magenta"),
        _ => None,
    }
}

/// Text color for Ink rendering
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextColor {
    /// Theme color name or ANSI color
    pub value: String,
}

impl TextColor {
    /// Create from a theme color
    pub fn theme(color: &str) -> Self {
        Self {
            value: color.to_string(),
        }
    }

    /// Create from an ANSI color
    pub fn ansi(color: &str) -> Self {
        Self {
            value: format!("ansi:{}", color),
        }
    }
}

impl Default for TextColor {
    fn default() -> Self {
        Self {
            value: DEFAULT_AGENT_THEME_COLOR.to_string(),
        }
    }
}

impl std::fmt::Display for TextColor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value)
    }
}

/// Convert a color string to Ink's TextProps['color'] format.
///
/// Colors are typically AgentColorName values like 'blue', 'green', etc.
/// This converts them to theme keys so they respect the current theme.
/// Falls back to the raw ANSI color if the color is not a known agent color.
///
/// # Arguments
///
/// * `color` - The color string to convert
///
/// # Returns
///
/// The converted color as a TextColor
pub fn to_ink_color(color: Option<&str>) -> TextColor {
    match color {
        None => TextColor::default(),
        Some(c) => {
            // Try to map to a theme color if it's a known agent color
            if let Some(theme_color) = get_theme_color(c) {
                TextColor::theme(theme_color)
            } else {
                // Fall back to raw ANSI color for unknown colors
                TextColor::ansi(c)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_color() {
        let color = to_ink_color(None);
        assert_eq!(color.value, DEFAULT_AGENT_THEME_COLOR);
    }

    #[test]
    fn test_known_agent_color() {
        let color = to_ink_color(Some("blue"));
        assert_eq!(color.value, "blue");
    }

    #[test]
    fn test_unknown_color() {
        let color = to_ink_color(Some("unknown-color"));
        assert_eq!(color.value, "ansi:unknown-color");
    }

    #[test]
    fn test_agent_color_name_parse() {
        assert_eq!(AgentColorName::parse("blue"), Some(AgentColorName::Blue));
        assert_eq!(AgentColorName::parse("GREEN"), Some(AgentColorName::Green));
        assert_eq!(AgentColorName::parse("invalid"), None);
    }
}
