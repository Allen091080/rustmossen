//! Responsive terminal rendering profile.
//!
//! The profile is Layer 3 state: it decides how much detail fits in a
//! viewport, while `render_model` continues to own the semantic transcript.

use crate::render_glyphs::RenderGlyphs;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RendererProfile {
    Small,
    Medium,
    Large,
}

impl RendererProfile {
    pub fn from_width(width: u16) -> Self {
        match width {
            0..=79 => Self::Small,
            80..=119 => Self::Medium,
            _ => Self::Large,
        }
    }

    pub fn tool_preview_lines(self) -> usize {
        match self {
            Self::Small => 8,
            Self::Medium => 16,
            Self::Large => 24,
        }
    }

    pub fn tool_expanded_lines(self) -> usize {
        match self {
            Self::Small => 80,
            Self::Medium => 160,
            Self::Large => 240,
        }
    }

    pub fn tool_section_line_chars(self) -> usize {
        match self {
            Self::Small => 120,
            Self::Medium => 200,
            Self::Large => 240,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RenderColorMode {
    Color,
    Plain,
}

impl RenderColorMode {
    pub fn from_env() -> Self {
        Self::from_env_values(
            std::env::var("NO_COLOR").ok().as_deref(),
            std::env::var("FORCE_COLOR").ok().as_deref(),
            std::env::var("TERM").ok().as_deref(),
            std::env::var("MOSSEN_TUI_COLOR").ok().as_deref(),
        )
    }

    pub fn from_env_values(
        no_color: Option<&str>,
        force_color: Option<&str>,
        term: Option<&str>,
        mossen_tui_color: Option<&str>,
    ) -> Self {
        if matches_setting(mossen_tui_color, &["never", "none", "plain", "0", "false"]) {
            return Self::Plain;
        }
        if matches_setting(mossen_tui_color, &["always", "color", "1", "true"]) {
            return Self::Color;
        }
        if no_color.is_some() {
            return Self::Plain;
        }
        if force_color.is_some() {
            return Self::Color;
        }
        if matches!(term, Some("dumb")) {
            return Self::Plain;
        }
        Self::Color
    }

    pub fn uses_color(self) -> bool {
        matches!(self, Self::Color)
    }
}

impl Default for RenderColorMode {
    fn default() -> Self {
        Self::from_env()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TerminalRenderProfile {
    pub width: RendererProfile,
    pub glyphs: RenderGlyphs,
    pub color_mode: RenderColorMode,
}

impl TerminalRenderProfile {
    pub fn from_env(width: u16) -> Self {
        Self {
            width: RendererProfile::from_width(width),
            glyphs: RenderGlyphs::from_env(),
            color_mode: RenderColorMode::from_env(),
        }
    }

    pub fn plain(width: u16) -> Self {
        Self {
            width: RendererProfile::from_width(width),
            glyphs: RenderGlyphs::ascii(),
            color_mode: RenderColorMode::Plain,
        }
    }
}

fn matches_setting(value: Option<&str>, choices: &[&str]) -> bool {
    let Some(value) = value else {
        return false;
    };
    choices
        .iter()
        .any(|choice| value.eq_ignore_ascii_case(choice))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renderer_profile_is_selected_from_terminal_width() {
        assert_eq!(RendererProfile::from_width(60), RendererProfile::Small);
        assert_eq!(RendererProfile::from_width(79), RendererProfile::Small);
        assert_eq!(RendererProfile::from_width(80), RendererProfile::Medium);
        assert_eq!(RendererProfile::from_width(119), RendererProfile::Medium);
        assert_eq!(RendererProfile::from_width(120), RendererProfile::Large);
    }

    #[test]
    fn color_mode_respects_terminal_env_precedence() {
        assert_eq!(
            RenderColorMode::from_env_values(Some("1"), None, Some("xterm-256color"), None),
            RenderColorMode::Plain
        );
        assert_eq!(
            RenderColorMode::from_env_values(None, Some("1"), Some("dumb"), None),
            RenderColorMode::Color
        );
        assert_eq!(
            RenderColorMode::from_env_values(None, None, Some("dumb"), None),
            RenderColorMode::Plain
        );
        assert_eq!(
            RenderColorMode::from_env_values(
                Some("1"),
                None,
                Some("xterm-256color"),
                Some("always")
            ),
            RenderColorMode::Color
        );
        assert_eq!(
            RenderColorMode::from_env_values(
                None,
                Some("1"),
                Some("xterm-256color"),
                Some("plain")
            ),
            RenderColorMode::Plain
        );
    }

    #[test]
    fn plain_terminal_profile_combines_width_glyphs_and_color() {
        let profile = TerminalRenderProfile::plain(60);

        assert_eq!(profile.width, RendererProfile::Small);
        assert_eq!(profile.glyphs, RenderGlyphs::ascii());
        assert_eq!(profile.color_mode, RenderColorMode::Plain);
    }
}
