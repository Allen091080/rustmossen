//! Theme and color system for the TUI layer.
//!
//! Resolves user theme preferences into static ratatui styles.

use crate::render_profile::RenderColorMode;
use ratatui::style::{Color, Modifier, Style};
use std::fmt;

/// Theme name — resolved (never "auto" at render time).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ThemeName {
    #[default]
    Dark,
    Light,
    DarkHighContrast,
    LightHighContrast,
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
    pub color_mode: RenderColorMode,

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
        Self::for_name_with_color_mode(name, RenderColorMode::Color)
    }

    pub fn from_env_for_name(name: ThemeName) -> Self {
        Self::for_name_with_color_mode(name, RenderColorMode::from_env())
    }

    pub fn for_name_with_color_mode(name: ThemeName, color_mode: RenderColorMode) -> Self {
        Self::colored(name).with_color_mode(color_mode)
    }

    fn colored(name: ThemeName) -> Self {
        match name {
            ThemeName::Dark | ThemeName::DarkHighContrast => Self::dark(name),
            ThemeName::Light | ThemeName::LightHighContrast => Self::light(name),
        }
    }

    pub fn with_color_mode(mut self, color_mode: RenderColorMode) -> Self {
        self.color_mode = color_mode;
        if color_mode.uses_color() {
            return self;
        }

        self.primary = Color::Reset;
        self.secondary = Color::Reset;
        self.error = Color::Reset;
        self.warning = Color::Reset;
        self.success = Color::Reset;
        self.info = Color::Reset;
        self.text = Color::Reset;
        self.text_dim = Color::Reset;
        self.text_subtle = Color::Reset;
        self.background = Color::Reset;
        self.surface = Color::Reset;
        self.border = Color::Reset;
        self.border_focused = Color::Reset;
        self.selection = Color::Reset;
        self.user_message_bg = Color::Reset;
        self.assistant_message_bg = Color::Reset;
        self.system_message_fg = Color::Reset;
        self.permission = Color::Reset;
        self.permission_denied = Color::Reset;
        self.spinner_primary = Color::Reset;
        self.spinner_secondary = Color::Reset;
        self
    }

    pub fn uses_color(&self) -> bool {
        self.color_mode.uses_color()
    }

    pub fn terminal_color(&self, color: Color) -> Color {
        if self.uses_color() {
            color
        } else {
            Color::Reset
        }
    }

    fn dark(name: ThemeName) -> Self {
        let high_contrast = matches!(name, ThemeName::DarkHighContrast);
        Self {
            name,
            color_mode: RenderColorMode::Color,
            primary: if high_contrast {
                Color::Cyan
            } else {
                Color::Rgb(130, 170, 255)
            },
            secondary: if high_contrast {
                Color::Magenta
            } else {
                Color::Rgb(180, 140, 255)
            },
            error: if high_contrast {
                Color::Red
            } else {
                Color::Rgb(255, 100, 100)
            },
            warning: if high_contrast {
                Color::Yellow
            } else {
                Color::Rgb(255, 200, 80)
            },
            success: if high_contrast {
                Color::Green
            } else {
                Color::Rgb(100, 220, 100)
            },
            info: if high_contrast {
                Color::Cyan
            } else {
                Color::Rgb(100, 180, 255)
            },
            text: Color::White,
            text_dim: Color::Gray,
            text_subtle: if high_contrast {
                Color::Gray
            } else {
                Color::DarkGray
            },
            background: Color::Reset,
            surface: if high_contrast {
                Color::Reset
            } else {
                Color::Rgb(30, 30, 40)
            },
            border: if high_contrast {
                Color::White
            } else {
                Color::DarkGray
            },
            border_focused: if high_contrast {
                Color::Cyan
            } else {
                Color::Rgb(130, 170, 255)
            },
            selection: if high_contrast {
                Color::Blue
            } else {
                Color::Rgb(60, 60, 80)
            },
            user_message_bg: if high_contrast {
                Color::Reset
            } else {
                Color::Rgb(40, 40, 55)
            },
            assistant_message_bg: Color::Reset,
            system_message_fg: if high_contrast {
                Color::Gray
            } else {
                Color::DarkGray
            },
            permission: if high_contrast {
                Color::Yellow
            } else {
                Color::Rgb(255, 200, 80)
            },
            permission_denied: if high_contrast {
                Color::Red
            } else {
                Color::Rgb(255, 100, 100)
            },
            spinner_primary: if high_contrast {
                Color::Cyan
            } else {
                Color::Rgb(130, 170, 255)
            },
            spinner_secondary: if high_contrast {
                Color::White
            } else {
                Color::Rgb(80, 80, 120)
            },
        }
    }

    fn light(name: ThemeName) -> Self {
        let high_contrast = matches!(name, ThemeName::LightHighContrast);
        Self {
            name,
            color_mode: RenderColorMode::Color,
            primary: if high_contrast {
                Color::Blue
            } else {
                Color::Rgb(0, 90, 200)
            },
            secondary: if high_contrast {
                Color::Magenta
            } else {
                Color::Rgb(120, 50, 200)
            },
            error: if high_contrast {
                Color::Red
            } else {
                Color::Rgb(200, 40, 40)
            },
            warning: if high_contrast {
                Color::Yellow
            } else {
                Color::Rgb(180, 120, 0)
            },
            success: if high_contrast {
                Color::Green
            } else {
                Color::Rgb(30, 150, 30)
            },
            info: if high_contrast {
                Color::Blue
            } else {
                Color::Rgb(0, 100, 200)
            },
            text: Color::Black,
            text_dim: if high_contrast {
                Color::Black
            } else {
                Color::DarkGray
            },
            text_subtle: if high_contrast {
                Color::Black
            } else {
                Color::Gray
            },
            background: Color::Reset,
            surface: if high_contrast {
                Color::Reset
            } else {
                Color::Rgb(245, 245, 250)
            },
            border: if high_contrast {
                Color::Black
            } else {
                Color::Gray
            },
            border_focused: if high_contrast {
                Color::Blue
            } else {
                Color::Rgb(0, 90, 200)
            },
            selection: if high_contrast {
                Color::Cyan
            } else {
                Color::Rgb(200, 220, 255)
            },
            user_message_bg: if high_contrast {
                Color::Reset
            } else {
                Color::Rgb(235, 235, 245)
            },
            assistant_message_bg: Color::Reset,
            system_message_fg: if high_contrast {
                Color::Black
            } else {
                Color::Gray
            },
            permission: if high_contrast {
                Color::Yellow
            } else {
                Color::Rgb(180, 120, 0)
            },
            permission_denied: if high_contrast {
                Color::Red
            } else {
                Color::Rgb(200, 40, 40)
            },
            spinner_primary: if high_contrast {
                Color::Blue
            } else {
                Color::Rgb(0, 90, 200)
            },
            spinner_secondary: if high_contrast {
                Color::Black
            } else {
                Color::Rgb(180, 180, 200)
            },
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_color_mode_removes_semantic_terminal_colors() {
        let theme = Theme::for_name_with_color_mode(ThemeName::Dark, RenderColorMode::Plain);

        assert_eq!(theme.color_mode, RenderColorMode::Plain);
        assert_eq!(theme.primary, Color::Reset);
        assert_eq!(theme.error, Color::Reset);
        assert_eq!(theme.selection, Color::Reset);
    }

    #[test]
    fn high_contrast_theme_uses_distinct_palette() {
        let regular = Theme::for_name(ThemeName::Dark);
        let high = Theme::for_name(ThemeName::DarkHighContrast);

        assert_ne!(regular.border_focused, high.border_focused);
        assert_eq!(high.border, Color::White);
    }
}
