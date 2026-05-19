//! Theme and color system for the TUI layer.
//!
//! Translates the React Ink ThemeProvider + color.ts pattern into a static
//! theme struct with resolved ratatui styles.

use ratatui::style::{Color, Modifier, Style};
use std::fmt;

/// Theme name — resolved (never "auto" at render time).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ThemeName {
    Dark,
    Light,
    DarkHighContrast,
    LightHighContrast,
}

impl Default for ThemeName {
    fn default() -> Self {
        Self::Dark
    }
}

impl fmt::Display for ThemeName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Dark => write!(f, "dark"),
            Self::Light => write!(f, "light"),
            Self::DarkHighContrast => write!(f, "dark-high-contrast"),
            Self::LightHighContrast => write!(f, "light-high-contrast"),
        }
    }
}

/// Theme setting — includes "auto" as a user preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeSetting {
    Auto,
    Explicit(ThemeName),
}

impl Default for ThemeSetting {
    fn default() -> Self {
        Self::Explicit(ThemeName::Dark)
    }
}

/// Resolved theme colors for the TUI.
#[derive(Debug, Clone)]
pub struct Theme {
    pub name: ThemeName,

    // Primary semantic colors
    pub primary: Color,
    pub secondary: Color,
    pub error: Color,
    pub warning: Color,
    pub success: Color,
    pub info: Color,

    // Text colors
    pub text: Color,
    pub text_dim: Color,
    pub text_subtle: Color,

    // Background colors
    pub background: Color,
    pub surface: Color,

    // UI element colors
    pub border: Color,
    pub border_focused: Color,
    pub selection: Color,

    // Message-specific colors
    pub user_message_bg: Color,
    pub assistant_message_bg: Color,
    pub system_message_fg: Color,

    // Permission colors
    pub permission: Color,
    pub permission_denied: Color,

    // Spinner/animation colors
    pub spinner_primary: Color,
    pub spinner_secondary: Color,
}

impl Theme {
    /// Create theme for the given name.
    pub fn for_name(name: ThemeName) -> Self {
        match name {
            ThemeName::Dark | ThemeName::DarkHighContrast => Self::dark(name),
            ThemeName::Light | ThemeName::LightHighContrast => Self::light(name),
        }
    }

    fn dark(name: ThemeName) -> Self {
        Self {
            name,
            primary: Color::Rgb(130, 170, 255),
            secondary: Color::Rgb(180, 140, 255),
            error: Color::Rgb(255, 100, 100),
            warning: Color::Rgb(255, 200, 80),
            success: Color::Rgb(100, 220, 100),
            info: Color::Rgb(100, 180, 255),
            text: Color::White,
            text_dim: Color::Gray,
            text_subtle: Color::DarkGray,
            background: Color::Reset,
            surface: Color::Rgb(30, 30, 40),
            border: Color::DarkGray,
            border_focused: Color::Rgb(130, 170, 255),
            selection: Color::Rgb(60, 60, 80),
            user_message_bg: Color::Rgb(40, 40, 55),
            assistant_message_bg: Color::Reset,
            system_message_fg: Color::DarkGray,
            permission: Color::Rgb(255, 200, 80),
            permission_denied: Color::Rgb(255, 100, 100),
            spinner_primary: Color::Rgb(130, 170, 255),
            spinner_secondary: Color::Rgb(80, 80, 120),
        }
    }

    fn light(name: ThemeName) -> Self {
        Self {
            name,
            primary: Color::Rgb(0, 90, 200),
            secondary: Color::Rgb(120, 50, 200),
            error: Color::Rgb(200, 40, 40),
            warning: Color::Rgb(180, 120, 0),
            success: Color::Rgb(30, 150, 30),
            info: Color::Rgb(0, 100, 200),
            text: Color::Black,
            text_dim: Color::DarkGray,
            text_subtle: Color::Gray,
            background: Color::Reset,
            surface: Color::Rgb(245, 245, 250),
            border: Color::Gray,
            border_focused: Color::Rgb(0, 90, 200),
            selection: Color::Rgb(200, 220, 255),
            user_message_bg: Color::Rgb(235, 235, 245),
            assistant_message_bg: Color::Reset,
            system_message_fg: Color::Gray,
            permission: Color::Rgb(180, 120, 0),
            permission_denied: Color::Rgb(200, 40, 40),
            spinner_primary: Color::Rgb(0, 90, 200),
            spinner_secondary: Color::Rgb(180, 180, 200),
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::for_name(ThemeName::default())
    }
}

// --- Style helpers ---

impl Theme {
    pub fn style_error(&self) -> Style {
        Style::default().fg(self.error)
    }

    pub fn style_warning(&self) -> Style {
        Style::default().fg(self.warning)
    }

    pub fn style_success(&self) -> Style {
        Style::default().fg(self.success)
    }

    pub fn style_info(&self) -> Style {
        Style::default().fg(self.info)
    }

    pub fn style_dim(&self) -> Style {
        Style::default().fg(self.text_dim)
    }

    pub fn style_subtle(&self) -> Style {
        Style::default().fg(self.text_subtle)
    }

    pub fn style_bold(&self) -> Style {
        Style::default().add_modifier(Modifier::BOLD)
    }

    pub fn style_primary(&self) -> Style {
        Style::default().fg(self.primary)
    }

    pub fn style_border(&self) -> Style {
        Style::default().fg(self.border)
    }

    pub fn style_border_focused(&self) -> Style {
        Style::default().fg(self.border_focused)
    }
}
