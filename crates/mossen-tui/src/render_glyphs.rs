//! Glyph profile for terminal rendering.
//!
//! Colors are not enough for accessibility: some terminals, fonts, and logs
//! cannot render box drawing or emoji reliably. This profile keeps semantic
//! labels readable when the renderer has to fall back to ASCII-only output.

use ratatui::symbols::border;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RenderGlyphMode {
    Unicode,
    Ascii,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RenderGlyphs {
    pub mode: RenderGlyphMode,
    pub border: border::Set,
    pub disclosure_expanded: &'static str,
    pub disclosure_collapsed: &'static str,
    pub prompt: &'static str,
    pub user: &'static str,
    pub assistant: &'static str,
    pub system: &'static str,
    pub error: &'static str,
    pub file_change: &'static str,
    pub command_output: &'static str,
    pub progress: &'static str,
    pub attachment: &'static str,
    pub tool: &'static str,
    pub approval_decision: &'static str,
    pub final_summary: &'static str,
    pub skill: &'static str,
    pub thinking: &'static str,
    pub project: &'static str,
}

impl RenderGlyphs {
    pub const fn unicode() -> Self {
        Self {
            mode: RenderGlyphMode::Unicode,
            border: border::ROUNDED,
            disclosure_expanded: "▼",
            disclosure_collapsed: "▶",
            prompt: "❯",
            user: "❯",
            assistant: "✻",
            system: "ℹ",
            error: "!",
            file_change: "Δ",
            command_output: "/",
            progress: "⋯",
            attachment: "📎",
            tool: "⚡",
            approval_decision: "↳",
            final_summary: "✓",
            skill: "◆",
            thinking: "💭",
            project: "📁",
        }
    }

    pub const fn ascii() -> Self {
        Self {
            mode: RenderGlyphMode::Ascii,
            border: ASCII_BORDER,
            disclosure_expanded: "v",
            disclosure_collapsed: ">",
            prompt: ">",
            user: ">",
            assistant: "*",
            system: "i",
            error: "!",
            file_change: "D",
            command_output: "/",
            progress: ".",
            attachment: "@",
            tool: "$",
            approval_decision: ">",
            final_summary: "+",
            skill: "*",
            thinking: "?",
            project: "dir",
        }
    }

    pub fn from_env() -> Self {
        if ascii_env_enabled() || locale_is_ascii_only() {
            Self::ascii()
        } else {
            Self::unicode()
        }
    }

    pub fn spinner_frames(self) -> &'static [&'static str] {
        match self.mode {
            RenderGlyphMode::Unicode => &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"],
            RenderGlyphMode::Ascii => &["|", "/", "-", "\\"],
        }
    }

    pub fn working_frames(self) -> &'static [&'static str] {
        match self.mode {
            RenderGlyphMode::Unicode => &["🍃", "🌿", "☘️", "🍀", "☘️", "🌿"],
            RenderGlyphMode::Ascii => &["|", "/", "-", "\\"],
        }
    }

    pub fn selected_indicator(self) -> &'static str {
        match self.mode {
            RenderGlyphMode::Unicode => "▸",
            RenderGlyphMode::Ascii => ">",
        }
    }

    pub fn separator(self) -> &'static str {
        match self.mode {
            RenderGlyphMode::Unicode => " · ",
            RenderGlyphMode::Ascii => " - ",
        }
    }

    pub fn ellipsis(self) -> &'static str {
        match self.mode {
            RenderGlyphMode::Unicode => "…",
            RenderGlyphMode::Ascii => "...",
        }
    }
}

impl Default for RenderGlyphs {
    fn default() -> Self {
        Self::from_env()
    }
}

pub const ASCII_BORDER: border::Set = border::Set {
    top_left: "+",
    top_right: "+",
    bottom_left: "+",
    bottom_right: "+",
    vertical_left: "|",
    vertical_right: "|",
    horizontal_top: "-",
    horizontal_bottom: "-",
};

fn ascii_env_enabled() -> bool {
    matches!(
        std::env::var("MOSSEN_TUI_GLYPHS"),
        Ok(value) if value.eq_ignore_ascii_case("ascii")
            || value.eq_ignore_ascii_case("plain")
            || value == "1"
    ) || matches!(
        std::env::var("MOSSEN_TUI_ASCII"),
        Ok(value) if value != "0" && !value.eq_ignore_ascii_case("false")
    )
}

fn locale_is_ascii_only() -> bool {
    let locale = std::env::var("LC_ALL")
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| {
            std::env::var("LC_CTYPE")
                .ok()
                .filter(|value| !value.is_empty())
        })
        .or_else(|| std::env::var("LANG").ok().filter(|value| !value.is_empty()));

    matches!(locale.as_deref(), Some("C") | Some("POSIX"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_profile_uses_plain_terminal_glyphs() {
        let glyphs = RenderGlyphs::ascii();

        assert_eq!(glyphs.border.top_left, "+");
        assert_eq!(glyphs.border.horizontal_top, "-");
        assert_eq!(glyphs.border.vertical_left, "|");
        assert_eq!(glyphs.disclosure_expanded, "v");
        assert_eq!(glyphs.tool, "$");
        assert_eq!(glyphs.thinking, "?");
        assert_eq!(glyphs.spinner_frames(), &["|", "/", "-", "\\"]);
        assert_eq!(glyphs.working_frames(), &["|", "/", "-", "\\"]);
        assert_eq!(glyphs.selected_indicator(), ">");
        assert_eq!(glyphs.separator(), " - ");
        assert_eq!(glyphs.ellipsis(), "...");
    }
}
