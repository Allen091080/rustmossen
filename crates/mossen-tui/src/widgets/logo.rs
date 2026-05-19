//! Logo widget — ASCII art branding for the TUI.
//!
//! Translates LogoV2/ directory (16 files) into a simple logo renderer.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::Widget,
};

/// Logo variant to display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogoVariant {
    /// Full multi-line ASCII art
    Full,
    /// Single-line compact version
    Compact,
    /// Minimal dot indicator
    Dot,
}

/// The Mossen logo widget.
pub struct LogoWidget {
    pub variant: LogoVariant,
    pub style: Style,
}

impl LogoWidget {
    pub fn new(variant: LogoVariant) -> Self {
        Self {
            variant,
            style: Style::default().fg(Color::Rgb(130, 170, 255)),
        }
    }

    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    fn full_lines() -> &'static [&'static str] {
        &[
            r"  __  __                          ",
            r" |  \/  | ___  ___ ___  ___ _ __  ",
            r" | |\/| |/ _ \/ __/ __|/ _ \ '_ \ ",
            r" | |  | | (_) \__ \__ \  __/ | | |",
            r" |_|  |_|\___/|___/___/\___|_| |_|",
        ]
    }

    fn compact_text() -> &'static str {
        "◆ Mossen"
    }

    fn dot_text() -> &'static str {
        "◆"
    }
}

impl Widget for LogoWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        match self.variant {
            LogoVariant::Full => {
                let lines = Self::full_lines();
                for (i, line) in lines.iter().enumerate() {
                    if i as u16 >= area.height {
                        break;
                    }
                    let truncated: String = line.chars().take(area.width as usize).collect();
                    buf.set_string(area.x, area.y + i as u16, &truncated, self.style);
                }
            }
            LogoVariant::Compact => {
                let text = Self::compact_text();
                buf.set_string(area.x, area.y, text, self.style);
            }
            LogoVariant::Dot => {
                let text = Self::dot_text();
                buf.set_string(area.x, area.y, text, self.style);
            }
        }
    }
}
