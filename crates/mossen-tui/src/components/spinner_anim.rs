//! Advanced spinner and animation components.
//!
//! Translates: Spinner/index.ts, Spinner/FlashingChar.tsx, Spinner/GlimmerMessage.tsx,
//! Spinner/ShimmerChar.tsx, Spinner/SpinnerAnimationRow.tsx, Spinner/SpinnerGlyph.tsx,
//! Spinner/TeammateSpinnerLine.tsx, Spinner/TeammateSpinnerTree.tsx,
//! Spinner/teammateSelectHint.ts, Spinner/useShimmerAnimation.ts,
//! Spinner/useStalledAnimation.ts, Spinner/utils.ts

use std::time::{Duration, Instant};

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget, Wrap},
};

use crate::theme::Theme;

// ===================================================================
// Constants — from teammateSelectHint.ts, utils.ts
// ===================================================================

/// Hint text for teammate selection.
pub const TEAMMATE_SELECT_HINT: &str = "shift + ↑/↓ to select";

/// Default spinner characters (leaf emoji cycle).
pub const DEFAULT_CHARACTERS: &[&str] = &["🍃", "🌿", "☘️", "🍀", "☘️", "🌿"];

/// Full spinner frame sequence (forward + reverse).
pub fn spinner_frames() -> Vec<&'static str> {
    let mut frames: Vec<&str> = DEFAULT_CHARACTERS.to_vec();
    let mut rev: Vec<&str> = DEFAULT_CHARACTERS.to_vec();
    rev.reverse();
    frames.extend(rev);
    frames
}

/// Reduced motion dot for accessibility.
pub const REDUCED_MOTION_DOT: &str = "🍃";

/// Cycle time for reduced motion mode (2s).
pub const REDUCED_MOTION_CYCLE_MS: u64 = 2000;

/// Show token count after this many ms.
pub const SHOW_TOKENS_AFTER_MS: u64 = 30000;

/// Thinking shimmer delay before activation.
pub const THINKING_DELAY_MS: u64 = 3000;

/// Thinking glow period in seconds.
pub const THINKING_GLOW_PERIOD_S: f64 = 2.0;

// ===================================================================
// RGB Color utilities — from utils.ts
// ===================================================================

/// RGB color representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RgbColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl RgbColor {
    pub fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Convert to ratatui Color.
    pub fn to_color(self) -> Color {
        Color::Rgb(self.r, self.g, self.b)
    }
}

/// Error red for stalled animation.
pub const ERROR_RED: RgbColor = RgbColor { r: 171, g: 43, b: 63 };

/// Thinking inactive color.
pub const THINKING_INACTIVE: RgbColor = RgbColor { r: 153, g: 153, b: 153 };

/// Thinking inactive shimmer color.
pub const THINKING_INACTIVE_SHIMMER: RgbColor = RgbColor { r: 185, g: 185, b: 185 };

/// Interpolate between two RGB colors.
pub fn interpolate_color(color1: RgbColor, color2: RgbColor, t: f64) -> RgbColor {
    let t = t.clamp(0.0, 1.0);
    RgbColor {
        r: (color1.r as f64 + (color2.r as f64 - color1.r as f64) * t).round() as u8,
        g: (color1.g as f64 + (color2.g as f64 - color1.g as f64) * t).round() as u8,
        b: (color1.b as f64 + (color2.b as f64 - color1.b as f64) * t).round() as u8,
    }
}

/// Convert HSL hue (0-360) to RGB with s=0.7, l=0.6.
pub fn hue_to_rgb(hue: f64) -> RgbColor {
    let h = ((hue % 360.0) + 360.0) % 360.0;
    let s: f64 = 0.7;
    let l: f64 = 0.6;
    let c = (1.0_f64 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0_f64 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;

    let (r, g, b) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    RgbColor {
        r: ((r + m) * 255.0).round() as u8,
        g: ((g + m) * 255.0).round() as u8,
        b: ((b + m) * 255.0).round() as u8,
    }
}

// ===================================================================
// SpinnerMode — from types.ts
// ===================================================================

/// Mode of the spinner animation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpinnerMode {
    /// Normal spinning animation.
    Streaming,
    /// Tool use in progress.
    ToolUse,
    /// Thinking/processing.
    Thinking,
    /// Stalled (taking too long).
    Stalled,
}

// ===================================================================
// ShimmerAnimation — from useShimmerAnimation.ts
// ===================================================================

/// State for the shimmer animation (glimmer sweeping across text).
#[derive(Debug, Clone)]
pub struct ShimmerAnimationState {
    pub glimmer_index: usize,
    pub message_width: usize,
    pub flash_opacity: f64,
    pub last_tick: Instant,
    pub tick_interval: Duration,
    pub direction_forward: bool,
}

impl ShimmerAnimationState {
    pub fn new(message_width: usize) -> Self {
        Self {
            glimmer_index: 0,
            message_width,
            flash_opacity: 0.0,
            last_tick: Instant::now(),
            tick_interval: Duration::from_millis(80),
            direction_forward: true,
        }
    }

    pub fn tick(&mut self) {
        if self.last_tick.elapsed() < self.tick_interval {
            return;
        }
        self.last_tick = Instant::now();

        if self.message_width == 0 {
            return;
        }

        if self.direction_forward {
            self.glimmer_index += 1;
            if self.glimmer_index >= self.message_width {
                self.direction_forward = false;
            }
        } else {
            if self.glimmer_index == 0 {
                self.direction_forward = true;
            } else {
                self.glimmer_index -= 1;
            }
        }

        // Compute flash opacity based on glimmer position
        let progress = if self.message_width > 1 {
            self.glimmer_index as f64 / (self.message_width - 1) as f64
        } else {
            0.0
        };
        self.flash_opacity = (progress * std::f64::consts::PI).sin();
    }

    pub fn update_width(&mut self, new_width: usize) {
        if new_width != self.message_width {
            self.message_width = new_width;
            self.glimmer_index = self.glimmer_index.min(new_width.saturating_sub(1));
        }
    }
}

// ===================================================================
// StalledAnimation — from useStalledAnimation.ts
// ===================================================================

/// State for stalled animation (pulsing red to indicate timeout).
#[derive(Debug, Clone)]
pub struct StalledAnimationState {
    pub intensity: f64,
    pub stalled_since: Option<Instant>,
    pub stall_threshold: Duration,
    pub last_tick: Instant,
}

impl StalledAnimationState {
    pub fn new(stall_threshold_ms: u64) -> Self {
        Self {
            intensity: 0.0,
            stalled_since: None,
            stall_threshold: Duration::from_millis(stall_threshold_ms),
            last_tick: Instant::now(),
        }
    }

    /// Mark that we started waiting.
    pub fn start_waiting(&mut self) {
        if self.stalled_since.is_none() {
            self.stalled_since = Some(Instant::now());
        }
    }

    /// Reset stall tracking.
    pub fn reset(&mut self) {
        self.stalled_since = None;
        self.intensity = 0.0;
    }

    pub fn tick(&mut self) {
        if let Some(since) = self.stalled_since {
            let elapsed = since.elapsed();
            if elapsed >= self.stall_threshold {
                // Pulse between 0 and 1
                let stalled_elapsed = (elapsed - self.stall_threshold).as_secs_f64();
                self.intensity = ((stalled_elapsed * std::f64::consts::PI).sin() * 0.5 + 0.5).clamp(0.0, 1.0);
            } else {
                self.intensity = 0.0;
            }
        }
        self.last_tick = Instant::now();
    }

    pub fn is_stalled(&self) -> bool {
        self.intensity > 0.0
    }
}

// ===================================================================
// FlashingChar — from FlashingChar.tsx
// ===================================================================

/// Renders a single character with flashing/shimmer effect.
pub struct FlashingCharWidget {
    pub ch: char,
    pub flash_opacity: f64,
    pub base_color: Color,
    pub shimmer_color: Color,
}

impl FlashingCharWidget {
    pub fn new(ch: char, flash_opacity: f64, base_color: Color, shimmer_color: Color) -> Self {
        Self {
            ch,
            flash_opacity,
            base_color,
            shimmer_color,
        }
    }

    pub fn computed_color(&self) -> Color {
        // Try to interpolate if both are Rgb colors
        match (self.base_color, self.shimmer_color) {
            (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => {
                let interp = interpolate_color(
                    RgbColor::new(r1, g1, b1),
                    RgbColor::new(r2, g2, b2),
                    self.flash_opacity,
                );
                interp.to_color()
            }
            _ => {
                if self.flash_opacity > 0.5 {
                    self.shimmer_color
                } else {
                    self.base_color
                }
            }
        }
    }
}

impl Widget for FlashingCharWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let color = self.computed_color();
        buf.set_string(area.x, area.y, &self.ch.to_string(), Style::default().fg(color));
    }
}

// ===================================================================
// ShimmerChar — from ShimmerChar.tsx
// ===================================================================

/// Renders a character with positional shimmer highlighting.
pub struct ShimmerCharWidget {
    pub ch: char,
    pub index: usize,
    pub glimmer_index: usize,
    pub base_color: Color,
    pub shimmer_color: Color,
}

impl ShimmerCharWidget {
    pub fn new(ch: char, index: usize, glimmer_index: usize, base_color: Color, shimmer_color: Color) -> Self {
        Self {
            ch,
            index,
            glimmer_index,
            base_color,
            shimmer_color,
        }
    }

    fn should_shimmer(&self) -> bool {
        let diff = (self.index as isize - self.glimmer_index as isize).unsigned_abs();
        diff <= 1
    }
}

impl Widget for ShimmerCharWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let color = if self.should_shimmer() {
            self.shimmer_color
        } else {
            self.base_color
        };
        buf.set_string(area.x, area.y, &self.ch.to_string(), Style::default().fg(color));
    }
}

// ===================================================================
// SpinnerGlyph — from SpinnerGlyph.tsx
// ===================================================================

/// State for the spinner glyph animation.
#[derive(Debug, Clone)]
pub struct SpinnerGlyphState {
    pub frame: usize,
    pub stalled_intensity: f64,
    pub reduced_motion: bool,
    pub time_ms: u64,
    pub base_color: Color,
}

impl SpinnerGlyphState {
    pub fn new(base_color: Color) -> Self {
        Self {
            frame: 0,
            stalled_intensity: 0.0,
            reduced_motion: false,
            time_ms: 0,
            base_color,
        }
    }

    pub fn current_char(&self) -> &'static str {
        if self.reduced_motion {
            return REDUCED_MOTION_DOT;
        }
        let frames = spinner_frames();
        frames[self.frame % frames.len()]
    }

    pub fn current_color(&self) -> Color {
        if self.stalled_intensity > 0.0 {
            match self.base_color {
                Color::Rgb(r, g, b) => {
                    let base = RgbColor::new(r, g, b);
                    let interp = interpolate_color(base, ERROR_RED, self.stalled_intensity);
                    interp.to_color()
                }
                _ => {
                    if self.stalled_intensity > 0.5 {
                        Color::Red
                    } else {
                        self.base_color
                    }
                }
            }
        } else {
            self.base_color
        }
    }

    pub fn is_dim(&self) -> bool {
        if self.reduced_motion {
            let half_cycle = REDUCED_MOTION_CYCLE_MS / 2;
            (self.time_ms / half_cycle) % 2 == 1
        } else {
            false
        }
    }
}

/// Widget for the spinner glyph.
pub struct SpinnerGlyphWidget<'a> {
    pub state: &'a SpinnerGlyphState,
}

impl<'a> SpinnerGlyphWidget<'a> {
    pub fn new(state: &'a SpinnerGlyphState) -> Self {
        Self { state }
    }
}

impl<'a> Widget for SpinnerGlyphWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 2 || area.height == 0 {
            return;
        }
        let ch = self.state.current_char();
        let color = self.state.current_color();
        let mut style = Style::default().fg(color);
        if self.state.is_dim() {
            style = style.add_modifier(Modifier::DIM);
        }
        buf.set_string(area.x, area.y, ch, style);
    }
}

// ===================================================================
// GlimmerMessage — from GlimmerMessage.tsx
// ===================================================================

/// State for a glimmering message line.
#[derive(Debug, Clone)]
pub struct GlimmerMessageState {
    pub message: String,
    pub mode: SpinnerMode,
    pub shimmer: ShimmerAnimationState,
    pub stalled: StalledAnimationState,
    pub base_color: Color,
    pub shimmer_color: Color,
}

impl GlimmerMessageState {
    pub fn new(message: impl Into<String>, mode: SpinnerMode, base_color: Color, shimmer_color: Color) -> Self {
        let msg = message.into();
        let width = msg.chars().count();
        Self {
            message: msg,
            mode,
            shimmer: ShimmerAnimationState::new(width),
            stalled: StalledAnimationState::new(60000),
            base_color,
            shimmer_color,
        }
    }

    pub fn set_message(&mut self, msg: impl Into<String>) {
        let new_msg = msg.into();
        let width = new_msg.chars().count();
        self.shimmer.update_width(width);
        self.message = new_msg;
    }

    pub fn tick(&mut self) {
        self.shimmer.tick();
        self.stalled.tick();
    }
}

/// Widget for the glimmering message.
pub struct GlimmerMessageWidget<'a> {
    pub state: &'a GlimmerMessageState,
}

impl<'a> GlimmerMessageWidget<'a> {
    pub fn new(state: &'a GlimmerMessageState) -> Self {
        Self { state }
    }
}

impl<'a> Widget for GlimmerMessageWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 || self.state.message.is_empty() {
            return;
        }

        let stalled_intensity = self.state.stalled.intensity;

        if stalled_intensity > 0.0 {
            // Render with stalled color
            let color = match self.state.base_color {
                Color::Rgb(r, g, b) => {
                    let base = RgbColor::new(r, g, b);
                    let interp = interpolate_color(base, ERROR_RED, stalled_intensity);
                    interp.to_color()
                }
                _ => {
                    if stalled_intensity > 0.5 {
                        Color::Red
                    } else {
                        self.state.base_color
                    }
                }
            };
            let avail = area.width as usize;
            let display: String = self.state.message.chars().take(avail).collect();
            buf.set_string(area.x, area.y, &display, Style::default().fg(color));
            return;
        }

        // Normal rendering with shimmer effect
        match self.state.mode {
            SpinnerMode::ToolUse | SpinnerMode::Streaming => {
                // Character-by-character shimmer
                let mut x = area.x;
                for (i, ch) in self.state.message.chars().enumerate() {
                    if x >= area.x + area.width {
                        break;
                    }
                    let color = if i == self.state.shimmer.glimmer_index
                        || (i as isize - self.state.shimmer.glimmer_index as isize).unsigned_abs() <= 1
                    {
                        self.state.shimmer_color
                    } else {
                        self.state.base_color
                    };
                    buf.set_string(x, area.y, &ch.to_string(), Style::default().fg(color));
                    x += unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1) as u16;
                }
            }
            SpinnerMode::Thinking => {
                // Simple dim color for thinking
                let avail = area.width as usize;
                let display: String = self.state.message.chars().take(avail).collect();
                buf.set_string(
                    area.x,
                    area.y,
                    &display,
                    Style::default().fg(Color::DarkGray),
                );
            }
            SpinnerMode::Stalled => {
                let avail = area.width as usize;
                let display: String = self.state.message.chars().take(avail).collect();
                buf.set_string(
                    area.x,
                    area.y,
                    &display,
                    Style::default().fg(Color::Red),
                );
            }
        }
    }
}

// ===================================================================
// SpinnerAnimationRow — from SpinnerAnimationRow.tsx
// ===================================================================

/// Full spinner animation row state.
#[derive(Debug, Clone)]
pub struct SpinnerAnimationRowState {
    pub mode: SpinnerMode,
    pub message: String,
    pub glyph: SpinnerGlyphState,
    pub glimmer: GlimmerMessageState,
    pub elapsed_ms: u64,
    pub tokens: usize,
    pub cost_usd: f64,
    pub reduced_motion: bool,
    pub has_active_tools: bool,
    pub show_thinking_shimmer: bool,
    pub thinking_start: Option<Instant>,
}

impl SpinnerAnimationRowState {
    pub fn new(message: impl Into<String>, mode: SpinnerMode, base_color: Color, shimmer_color: Color) -> Self {
        let msg = message.into();
        Self {
            mode,
            glyph: SpinnerGlyphState::new(base_color),
            glimmer: GlimmerMessageState::new(msg.clone(), mode, base_color, shimmer_color),
            message: msg,
            elapsed_ms: 0,
            tokens: 0,
            cost_usd: 0.0,
            reduced_motion: false,
            has_active_tools: false,
            show_thinking_shimmer: false,
            thinking_start: None,
        }
    }

    pub fn set_message(&mut self, msg: impl Into<String>) {
        let new_msg = msg.into();
        self.glimmer.set_message(new_msg.clone());
        self.message = new_msg;
    }

    pub fn tick(&mut self, frame_delta: u64) {
        self.elapsed_ms += frame_delta;
        self.glyph.frame += 1;
        self.glyph.time_ms = self.elapsed_ms;
        self.glyph.stalled_intensity = self.glimmer.stalled.intensity;
        self.glimmer.tick();

        // Check thinking shimmer activation
        if self.mode == SpinnerMode::Thinking {
            if let Some(start) = self.thinking_start {
                if start.elapsed() >= Duration::from_millis(THINKING_DELAY_MS) {
                    self.show_thinking_shimmer = true;
                }
            }
        }
    }

    pub fn start_thinking(&mut self) {
        self.thinking_start = Some(Instant::now());
    }

    /// Format the stats suffix (elapsed time, tokens, cost).
    pub fn stats_suffix(&self) -> String {
        let mut parts = Vec::new();

        // Elapsed time
        let secs = self.elapsed_ms / 1000;
        if secs > 0 {
            parts.push(format_duration(secs));
        }

        // Token count (only shown after threshold)
        if self.elapsed_ms >= SHOW_TOKENS_AFTER_MS && self.tokens > 0 {
            parts.push(format_number(self.tokens));
        }

        if parts.is_empty() {
            String::new()
        } else {
            format!(" · {}", parts.join(" · "))
        }
    }
}

/// Format duration in human-readable form.
fn format_duration(secs: u64) -> String {
    if secs < 60 {
        format!("{}s", secs)
    } else {
        let mins = secs / 60;
        let remainder = secs % 60;
        if remainder == 0 {
            format!("{}m", mins)
        } else {
            format!("{}m{}s", mins, remainder)
        }
    }
}

/// Format number with separators.
fn format_number(n: usize) -> String {
    if n < 1000 {
        format!("{}", n)
    } else if n < 1_000_000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    }
}

/// Widget for the full spinner animation row.
pub struct SpinnerAnimationRowWidget<'a> {
    pub state: &'a SpinnerAnimationRowState,
    pub theme: &'a Theme,
}

impl<'a> SpinnerAnimationRowWidget<'a> {
    pub fn new(state: &'a SpinnerAnimationRowState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for SpinnerAnimationRowWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 5 || area.height == 0 {
            return;
        }

        // Layout: [glyph 3] [message] [stats]
        let glyph_width = 3u16;
        let stats = self.state.stats_suffix();
        let stats_width = stats.len() as u16;
        let msg_width = area.width.saturating_sub(glyph_width + stats_width + 1);

        // Render glyph
        let glyph_area = Rect::new(area.x, area.y, glyph_width, 1);
        SpinnerGlyphWidget::new(&self.state.glyph).render(glyph_area, buf);

        // Render message
        let msg_area = Rect::new(area.x + glyph_width, area.y, msg_width, 1);
        GlimmerMessageWidget::new(&self.state.glimmer).render(msg_area, buf);

        // Render stats
        if !stats.is_empty() {
            let stats_x = area.x + area.width - stats_width;
            buf.set_string(stats_x, area.y, &stats, Style::default().fg(Color::DarkGray));
        }
    }
}

// ===================================================================
// TeammateSpinnerLine — from TeammateSpinnerLine.tsx
// ===================================================================

/// Status of a teammate in the spinner tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeammateStatus {
    /// Currently streaming/active.
    Active,
    /// Requesting (awaiting response).
    Requesting,
    /// Completed.
    Done,
    /// Paused/waiting.
    Paused,
}

/// State for a single teammate spinner line.
#[derive(Debug, Clone)]
pub struct TeammateSpinnerLineState {
    pub name: String,
    pub status: TeammateStatus,
    pub message: String,
    pub elapsed_ms: u64,
    pub is_selected: bool,
}

impl TeammateSpinnerLineState {
    pub fn new(name: impl Into<String>, status: TeammateStatus) -> Self {
        Self {
            name: name.into(),
            status,
            message: String::new(),
            elapsed_ms: 0,
            is_selected: false,
        }
    }

    pub fn with_message(mut self, msg: impl Into<String>) -> Self {
        self.message = msg.into();
        self
    }

    pub fn status_icon(&self) -> &'static str {
        match self.status {
            TeammateStatus::Active => "●",
            TeammateStatus::Requesting => "↑",
            TeammateStatus::Done => "✓",
            TeammateStatus::Paused => "○",
        }
    }

    pub fn status_color(&self, theme: &Theme) -> Color {
        match self.status {
            TeammateStatus::Active => theme.primary,
            TeammateStatus::Requesting => Color::DarkGray,
            TeammateStatus::Done => theme.success,
            TeammateStatus::Paused => Color::DarkGray,
        }
    }
}

/// Widget for a teammate spinner line.
pub struct TeammateSpinnerLineWidget<'a> {
    pub state: &'a TeammateSpinnerLineState,
    pub theme: &'a Theme,
    pub indent: u16,
}

impl<'a> TeammateSpinnerLineWidget<'a> {
    pub fn new(state: &'a TeammateSpinnerLineState, theme: &'a Theme) -> Self {
        Self {
            state,
            theme,
            indent: 0,
        }
    }

    pub fn with_indent(mut self, indent: u16) -> Self {
        self.indent = indent;
        self
    }
}

impl<'a> Widget for TeammateSpinnerLineWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 10 || area.height == 0 {
            return;
        }

        let mut x = area.x + self.indent;
        let max_x = area.x + area.width;

        // Status icon
        let icon = self.state.status_icon();
        let icon_color = self.state.status_color(self.theme);
        buf.set_string(x, area.y, icon, Style::default().fg(icon_color));
        x += 2;

        // Name
        let name_style = if self.state.is_selected {
            Style::default()
                .fg(self.theme.primary)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(self.theme.text)
        };
        let name_avail = (max_x - x).saturating_sub(2) as usize;
        let name: String = self.state.name.chars().take(name_avail.min(20)).collect();
        buf.set_string(x, area.y, &name, name_style);
        x += name.len() as u16;

        // Message (if any)
        if !self.state.message.is_empty() && x + 3 < max_x {
            buf.set_string(x, area.y, " · ", Style::default().fg(Color::DarkGray));
            x += 3;
            let msg_avail = max_x.saturating_sub(x) as usize;
            let msg: String = self.state.message.chars().take(msg_avail).collect();
            buf.set_string(x, area.y, &msg, Style::default().fg(Color::DarkGray));
        }
    }
}

// ===================================================================
// TeammateSpinnerTree — from TeammateSpinnerTree.tsx
// ===================================================================

/// State for the full teammate spinner tree.
#[derive(Debug, Clone)]
pub struct TeammateSpinnerTreeState {
    pub teammates: Vec<TeammateSpinnerLineState>,
    pub selected_index: Option<usize>,
    pub show_select_hint: bool,
}

impl TeammateSpinnerTreeState {
    pub fn new(teammates: Vec<TeammateSpinnerLineState>) -> Self {
        Self {
            teammates,
            selected_index: None,
            show_select_hint: true,
        }
    }

    pub fn select_next(&mut self) {
        let len = self.teammates.len();
        if len == 0 {
            return;
        }
        match self.selected_index {
            None => {
                self.selected_index = Some(0);
                self.teammates[0].is_selected = true;
            }
            Some(idx) => {
                self.teammates[idx].is_selected = false;
                let new_idx = (idx + 1) % len;
                self.selected_index = Some(new_idx);
                self.teammates[new_idx].is_selected = true;
            }
        }
    }

    pub fn select_prev(&mut self) {
        let len = self.teammates.len();
        if len == 0 {
            return;
        }
        match self.selected_index {
            None => {
                let last = len - 1;
                self.selected_index = Some(last);
                self.teammates[last].is_selected = true;
            }
            Some(idx) => {
                self.teammates[idx].is_selected = false;
                let new_idx = if idx == 0 { len - 1 } else { idx - 1 };
                self.selected_index = Some(new_idx);
                self.teammates[new_idx].is_selected = true;
            }
        }
    }

    pub fn deselect(&mut self) {
        if let Some(idx) = self.selected_index {
            self.teammates[idx].is_selected = false;
        }
        self.selected_index = None;
    }

    pub fn active_count(&self) -> usize {
        self.teammates
            .iter()
            .filter(|t| t.status == TeammateStatus::Active)
            .count()
    }
}

/// Widget for the teammate spinner tree.
pub struct TeammateSpinnerTreeWidget<'a> {
    pub state: &'a TeammateSpinnerTreeState,
    pub theme: &'a Theme,
}

impl<'a> TeammateSpinnerTreeWidget<'a> {
    pub fn new(state: &'a TeammateSpinnerTreeState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for TeammateSpinnerTreeWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width < 10 || self.state.teammates.is_empty() {
            return;
        }

        let max_lines = area.height as usize;
        let visible_count = self.state.teammates.len().min(max_lines.saturating_sub(1));

        for (i, teammate) in self.state.teammates.iter().take(visible_count).enumerate() {
            let y = area.y + i as u16;
            if y >= area.y + area.height {
                break;
            }
            let line_area = Rect::new(area.x, y, area.width, 1);
            TeammateSpinnerLineWidget::new(teammate, self.theme)
                .with_indent(2)
                .render(line_area, buf);
        }

        // Select hint at bottom
        if self.state.show_select_hint && self.state.teammates.len() > 1 {
            let hint_y = area.y + visible_count as u16;
            if hint_y < area.y + area.height {
                buf.set_string(
                    area.x + 2,
                    hint_y,
                    TEAMMATE_SELECT_HINT,
                    Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
                );
            }
        }
    }
}

// ===================================================================
// Spinner extras
// ===================================================================

#[derive(Debug, Clone, Default)]
pub struct TeammateSpinnerTree {
    pub teammates: Vec<String>,
    pub statuses: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct TeammateSpinnerLine {
    pub teammate: String,
    pub spinner_frame: u32,
    pub status: String,
}

#[derive(Debug, Clone, Default)]
pub struct SpinnerAnimationRowProps {
    pub frames: Vec<String>,
    pub interval_ms: u64,
    pub width: u16,
}

#[derive(Debug, Clone, Default)]
pub struct SpinnerAnimationRow {
    pub props: SpinnerAnimationRowProps,
    pub frame_index: usize,
}

/// Default set of spinner frame characters.
pub fn get_default_characters() -> Vec<&'static str> {
    vec!["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
}

/// Convert a (r,g,b) triple in 0..=255 to a 24-bit `Color::Rgb` value.
pub fn to_rgb_color(r: u8, g: u8, b: u8) -> ratatui::style::Color {
    ratatui::style::Color::Rgb(r, g, b)
}

/// Parse a `#RRGGBB` hex colour into a triple.
pub fn parse_rgb(hex: &str) -> Option<(u8, u8, u8)> {
    let s = hex.trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some((r, g, b))
}

#[derive(Debug, Clone, Default)]
pub struct ShimmerChar {
    pub ch: char,
    pub phase: f32,
}

#[derive(Debug, Clone, Default)]
pub struct FlashingChar {
    pub ch: char,
    pub on: bool,
}

#[derive(Debug, Clone, Default)]
pub struct SpinnerGlyph {
    pub frames: Vec<&'static str>,
    pub frame_index: usize,
}

#[derive(Debug, Clone, Default)]
pub struct GlimmerMessage {
    pub text: String,
    pub phase: f32,
}

#[derive(Debug, Clone, Default)]
pub struct StalledAnimationHook {
    pub stalled_for_ms: u64,
    pub frame_index: usize,
}

/// Hook-equivalent useStalledAnimation — advances when stalled > threshold.
pub fn use_stalled_animation(state: &mut StalledAnimationHook, threshold_ms: u64) -> bool {
    if state.stalled_for_ms > threshold_ms {
        state.frame_index = state.frame_index.wrapping_add(1);
        true
    } else {
        false
    }
}

#[derive(Debug, Clone, Default)]
pub struct ShimmerAnimationHook {
    pub phase: f32,
}

/// Hook-equivalent useShimmerAnimation — advance phase, wrap to [0,1).
pub fn use_shimmer_animation(state: &mut ShimmerAnimationHook, dt_ms: u64) -> f32 {
    state.phase += (dt_ms as f32) / 1500.0;
    if state.phase >= 1.0 {
        state.phase -= 1.0;
    }
    state.phase
}
