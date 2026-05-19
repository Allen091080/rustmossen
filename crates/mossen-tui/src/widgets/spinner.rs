//! Spinner animation widget.
//!
//! Translates the Spinner/ directory (12 files) including SpinnerGlyph,
//! SpinnerAnimationRow, FlashingChar, ShimmerChar, and GlimmerMessage.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Widget,
};
use std::time::{Duration, Instant};

/// Spinner animation frames — the rotating glyph characters.
const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Mossen-style spinner frames for the main thinking indicator. The TS port
/// pulses brightness on the `⏺` record glyph, but in a terminal that's
/// usually invisible to the user (the colour shift is subtle and many
/// terminals render bold/dim identically), so we cycle through a real
/// rotating brail-dot sequence — `frame_index` already returns a rolling
/// index, so all that's needed is to feed it distinct glyphs.
const DOT_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Shimmer gradient colors for the glimmer animation.
const SHIMMER_COLORS: &[Color] = &[
    Color::Rgb(60, 60, 90),
    Color::Rgb(80, 80, 120),
    Color::Rgb(100, 110, 160),
    Color::Rgb(130, 140, 200),
    Color::Rgb(160, 170, 230),
    Color::Rgb(130, 140, 200),
    Color::Rgb(100, 110, 160),
    Color::Rgb(80, 80, 120),
];

/// Spinner state — manages animation timing.
pub struct SpinnerState {
    started_at: Instant,
    frame_duration: Duration,
    stalled: bool,
    stalled_at: Option<Instant>,
}

impl SpinnerState {
    pub fn new() -> Self {
        Self {
            started_at: Instant::now(),
            frame_duration: Duration::from_millis(80),
            stalled: false,
            stalled_at: None,
        }
    }

    /// Get current frame index based on elapsed time.
    pub fn frame_index(&self, frame_count: usize) -> usize {
        let elapsed = self.started_at.elapsed();
        let total_ms = elapsed.as_millis() as usize;
        let frame_ms = self.frame_duration.as_millis() as usize;
        if frame_ms == 0 {
            return 0;
        }
        (total_ms / frame_ms) % frame_count
    }

    /// Mark as stalled (e.g., waiting for network).
    pub fn set_stalled(&mut self, stalled: bool) {
        if stalled && !self.stalled {
            self.stalled_at = Some(Instant::now());
        } else if !stalled {
            self.stalled_at = None;
        }
        self.stalled = stalled;
    }

    /// Reset the animation timer.
    pub fn reset(&mut self) {
        self.started_at = Instant::now();
        self.stalled = false;
        self.stalled_at = None;
    }

    /// Elapsed wall time since the spinner was last reset. Used by the
    /// render path to surface a live "Xs" suffix on the status row so the
    /// user knows the request is still in flight rather than guessing at a
    /// hung process.
    pub fn elapsed(&self) -> Duration {
        self.started_at.elapsed()
    }

    /// Get shimmer offset for gradient animation.
    pub fn shimmer_offset(&self) -> usize {
        let elapsed = self.started_at.elapsed().as_millis() as usize;
        (elapsed / 100) % SHIMMER_COLORS.len()
    }
}

impl Default for SpinnerState {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple spinning glyph widget (SpinnerGlyph.tsx equivalent).
pub struct SpinnerGlyphWidget<'a> {
    pub state: &'a SpinnerState,
    pub style: Style,
    pub label: Option<&'a str>,
}

impl<'a> SpinnerGlyphWidget<'a> {
    pub fn new(state: &'a SpinnerState) -> Self {
        Self {
            state,
            style: Style::default().fg(Color::Rgb(130, 170, 255)),
            label: None,
        }
    }

    pub fn label(mut self, label: &'a str) -> Self {
        self.label = Some(label);
        self
    }

    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }
}

impl<'a> Widget for SpinnerGlyphWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let frame_idx = self.state.frame_index(SPINNER_FRAMES.len());
        let glyph = SPINNER_FRAMES[frame_idx];

        let mut x = area.x;
        buf.set_string(x, area.y, glyph, self.style);
        x += 2; // glyph width + space

        if let Some(label) = self.label {
            if x < area.x + area.width {
                let label_style = Style::default().add_modifier(Modifier::DIM);
                buf.set_string(x, area.y, label, label_style);
            }
        }
    }
}

/// Spinner animation row — the main thinking/working indicator.
///
/// Translates SpinnerAnimationRow.tsx (11.3KB).
pub struct SpinnerRowWidget<'a> {
    pub state: &'a SpinnerState,
    pub message: &'a str,
    pub style: Style,
}

impl<'a> SpinnerRowWidget<'a> {
    pub fn new(state: &'a SpinnerState, message: &'a str) -> Self {
        Self {
            state,
            message,
            style: Style::default(),
        }
    }
}

impl<'a> Widget for SpinnerRowWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        // TS uses `✻` (TEARDROP_ASTERISK) as the thinking glyph paired
        // with a rainbow_green_shimmer — a green→cyan→blue→cyan→green
        // band that sweeps along the status line. We reproduce the same
        // optical effect: the glyph pulses on the green→cyan range, and
        // the status text behind it gets a rolling shimmer where the
        // 3-char window centred on `shimmer_offset` is brighter.
        let glyph = "✻";

        // Pulse hue across [120°, 175°] (green → cyan-green) over the
        // animation period. Stalled keeps a warning amber.
        let elapsed_ms = self.state.elapsed().as_millis() as f64;
        let glyph_color = if self.state.stalled {
            Color::Rgb(220, 160, 60)
        } else {
            let phase = ((elapsed_ms / 400.0).sin() * 0.5 + 0.5) as f32;
            hsl_to_rgb(120.0 + phase * 55.0, 0.65, 0.55)
        };
        buf.set_string(area.x, area.y, glyph, Style::default().fg(glyph_color));

        // Render the message text with a moving shimmer window. The
        // base color is dim; a 3-char window centred on `offset`
        // brightens to white. `offset` advances ~10 chars/sec via
        // SpinnerState::shimmer_offset (already wired).
        let msg_x = area.x + 2;
        if msg_x >= area.x + area.width {
            return;
        }
        let available = (area.x + area.width - msg_x) as usize;
        let chars: Vec<char> = self.message.chars().take(available).collect();
        let offset = (elapsed_ms / 60.0) as i64 % (chars.len().max(1) as i64 + 8);
        for (i, ch) in chars.iter().enumerate() {
            let dist = (offset - i as i64).abs();
            let style = if self.state.stalled {
                Style::default().fg(Color::Rgb(220, 160, 60))
            } else if dist <= 2 {
                // brightest centre of the shimmer
                let t = 1.0 - (dist as f32) / 3.0;
                let r = (140.0 + 80.0 * t) as u8;
                let g = (220.0 + 30.0 * t) as u8;
                let b = (180.0 + 50.0 * t) as u8;
                Style::default().fg(Color::Rgb(r, g, b))
            } else {
                Style::default()
                    .fg(Color::Rgb(120, 140, 130))
                    .add_modifier(Modifier::DIM)
            };
            buf.set_string(msg_x + i as u16, area.y, &ch.to_string(), style);
        }
    }
}

/// HSL → sRGB conversion. `h` in degrees (0..360), `s` and `l` in 0..1.
/// Pulled inline so SpinnerRowWidget doesn't need an extra dependency
/// for what is ultimately a tiny piece of math the TS shimmer uses too.
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> Color {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let h_prime = (h.rem_euclid(360.0)) / 60.0;
    let x = c * (1.0 - (h_prime % 2.0 - 1.0).abs());
    let (r1, g1, b1) = match h_prime as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    Color::Rgb(
        ((r1 + m) * 255.0).clamp(0.0, 255.0) as u8,
        ((g1 + m) * 255.0).clamp(0.0, 255.0) as u8,
        ((b1 + m) * 255.0).clamp(0.0, 255.0) as u8,
    )
}

/// Glimmer/shimmer message widget for streaming content.
///
/// Translates GlimmerMessage.tsx (8.2KB) — shows animated gradient text.
pub struct GlimmerWidget<'a> {
    pub state: &'a SpinnerState,
    pub text: &'a str,
}

impl<'a> GlimmerWidget<'a> {
    pub fn new(state: &'a SpinnerState, text: &'a str) -> Self {
        Self { state, text }
    }
}

impl<'a> Widget for GlimmerWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 || self.text.is_empty() {
            return;
        }

        let offset = self.state.shimmer_offset();
        let chars: Vec<char> = self.text.chars().collect();
        let mut x = area.x;

        for (i, ch) in chars.iter().enumerate() {
            if x >= area.x + area.width {
                break;
            }
            let color_idx = (i + offset) % SHIMMER_COLORS.len();
            let style = Style::default().fg(SHIMMER_COLORS[color_idx]);
            buf.set_string(x, area.y, &ch.to_string(), style);
            x += 1;
        }
    }
}
