//! Spinner animation widget.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Widget,
};
use std::time::{Duration, Instant};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::render_glyphs::{RenderGlyphMode, RenderGlyphs};
use crate::render_profile::RenderColorMode;
use crate::theme::Theme;

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
    last_activity_at: Instant,
    frame_duration: Duration,
    stalled: bool,
    stalled_at: Option<Instant>,
}

impl SpinnerState {
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            started_at: now,
            last_activity_at: now,
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

    /// Mark a fresh backend/render event without resetting total elapsed time.
    pub fn mark_activity(&mut self) {
        self.last_activity_at = Instant::now();
        if self.stalled {
            self.stalled = false;
            self.stalled_at = None;
        }
    }

    /// Reset the animation timer.
    pub fn reset(&mut self) {
        let now = Instant::now();
        self.started_at = now;
        self.last_activity_at = now;
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

    /// Time since the last engine/render activity. Stalled color should be
    /// driven by this, not by total turn duration.
    pub fn idle_for(&self) -> Duration {
        self.last_activity_at.elapsed()
    }

    pub fn is_stalled(&self) -> bool {
        self.stalled
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

/// Simple spinning glyph widget.
pub struct SpinnerGlyphWidget<'a> {
    pub state: &'a SpinnerState,
    pub style: Style,
    pub label: Option<&'a str>,
    pub glyphs: RenderGlyphs,
}

impl<'a> SpinnerGlyphWidget<'a> {
    pub fn new(state: &'a SpinnerState) -> Self {
        Self {
            state,
            style: Style::default(),
            label: None,
            glyphs: RenderGlyphs::default(),
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

    pub fn glyphs(mut self, glyphs: RenderGlyphs) -> Self {
        self.glyphs = glyphs;
        self
    }
}

impl<'a> Widget for SpinnerGlyphWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let frames = self.glyphs.spinner_frames();
        let glyph = frames[self.state.frame_index(frames.len())];

        let mut x = area.x;
        buf.set_string(x, area.y, glyph, self.style);
        x = x.saturating_add(display_width_u16(glyph).saturating_add(1));

        if let Some(label) = self.label {
            if x < area.x + area.width {
                let label_style = Style::default().add_modifier(Modifier::DIM);
                render_styled_text(buf, x, area.y, area.x + area.width, label, |_| label_style);
            }
        }
    }
}

/// Spinner animation row — the main thinking/working indicator.
///
/// Animated spinner row for streaming/thinking states.
pub struct SpinnerRowWidget<'a> {
    pub state: &'a SpinnerState,
    pub message: &'a str,
    pub style: Style,
    pub glyphs: RenderGlyphs,
    pub color_mode: RenderColorMode,
}

impl<'a> SpinnerRowWidget<'a> {
    pub fn new(state: &'a SpinnerState, message: &'a str) -> Self {
        Self {
            state,
            message,
            style: Style::default(),
            glyphs: RenderGlyphs::default(),
            color_mode: RenderColorMode::default(),
        }
    }

    pub fn glyphs(mut self, glyphs: RenderGlyphs) -> Self {
        self.glyphs = glyphs;
        self
    }

    pub fn color_mode(mut self, color_mode: RenderColorMode) -> Self {
        self.color_mode = color_mode;
        self
    }
}

impl<'a> Widget for SpinnerRowWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let frames = self.glyphs.working_frames();
        let glyph = frames[self.state.frame_index(frames.len())];

        // Pulse hue across [120°, 175°] (green → cyan-green) over the
        // animation period. After 3 seconds without any backend activity,
        // ease to red so slow / stuck backends are visually obvious.
        let elapsed_ms = self.state.elapsed().as_millis() as f64;
        let stalled = self.state.stalled || self.state.idle_for().as_millis() >= 3_000;
        let uses_color = self.color_mode.uses_color();
        let glyph_color = if stalled {
            Color::Rgb(171, 43, 63)
        } else {
            let phase = ((elapsed_ms / 400.0).sin() * 0.5 + 0.5) as f32;
            hsl_to_rgb(120.0 + phase * 55.0, 0.65, 0.55)
        };
        let glyph_style = if uses_color {
            Style::default().fg(glyph_color)
        } else {
            self.style
        };
        buf.set_string(area.x, area.y, glyph, glyph_style);

        // Render the message text with a moving shimmer window. The
        // base color is dim; a 3-char window centred on `offset`
        // brightens to white. `offset` advances ~10 chars/sec via
        // SpinnerState::shimmer_offset.
        let msg_x = area.x.saturating_add(display_width_u16(glyph));
        if msg_x >= area.x + area.width {
            return;
        }
        let chars: Vec<char> = self.message.chars().collect();
        let offset = (elapsed_ms / 60.0) as i64 % (chars.len().max(1) as i64 + 8);
        let mut x = msg_x;
        for (i, ch) in chars.iter().enumerate() {
            let dist = (offset - i as i64).abs();
            let style = if !uses_color {
                Style::default().add_modifier(Modifier::DIM)
            } else if stalled {
                Style::default().fg(Color::Rgb(171, 43, 63))
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
            let width = UnicodeWidthChar::width(*ch).unwrap_or(0) as u16;
            if width == 0 {
                continue;
            }
            if x + width > area.x + area.width {
                break;
            }
            buf.set_string(x, area.y, &ch.to_string(), style);
            x = x.saturating_add(width);
        }
    }
}

/// HSL → sRGB conversion. `h` in degrees (0..360), `s` and `l` in 0..1.
/// Pulled inline so SpinnerRowWidget doesn't need an extra dependency
/// for what is ultimately a tiny piece of math.
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
/// Shows animated gradient text.
pub struct GlimmerWidget<'a> {
    pub state: &'a SpinnerState,
    pub text: &'a str,
    pub color_mode: RenderColorMode,
}

impl<'a> GlimmerWidget<'a> {
    pub fn new(state: &'a SpinnerState, text: &'a str) -> Self {
        Self {
            state,
            text,
            color_mode: RenderColorMode::default(),
        }
    }

    pub fn color_mode(mut self, color_mode: RenderColorMode) -> Self {
        self.color_mode = color_mode;
        self
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
            let width = UnicodeWidthChar::width(*ch).unwrap_or(0) as u16;
            if width == 0 {
                continue;
            }
            if x + width > area.x + area.width {
                break;
            }
            let color_idx = (i + offset) % SHIMMER_COLORS.len();
            let style = if self.color_mode.uses_color() {
                Style::default().fg(SHIMMER_COLORS[color_idx])
            } else {
                Style::default()
            };
            buf.set_string(x, area.y, &ch.to_string(), style);
            x = x.saturating_add(width);
        }
    }
}

fn render_styled_text<F>(buf: &mut Buffer, mut x: u16, y: u16, max_x: u16, text: &str, style_for: F)
where
    F: Fn(usize) -> Style,
{
    for (i, ch) in text.chars().enumerate() {
        let width = UnicodeWidthChar::width(ch).unwrap_or(0) as u16;
        if width == 0 {
            continue;
        }
        if x + width > max_x {
            break;
        }
        buf.set_string(x, y, &ch.to_string(), style_for(i));
        x = x.saturating_add(width);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeammateStatus {
    Active,
    Requesting,
    Done,
    Paused,
}

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

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }

    fn status_icon(&self, glyphs: RenderGlyphs) -> &'static str {
        match self.status {
            TeammateStatus::Active => match glyphs.mode {
                RenderGlyphMode::Unicode => "●",
                RenderGlyphMode::Ascii => "*",
            },
            TeammateStatus::Requesting => match glyphs.mode {
                RenderGlyphMode::Unicode => "↑",
                RenderGlyphMode::Ascii => "^",
            },
            TeammateStatus::Done => match glyphs.mode {
                RenderGlyphMode::Unicode => "✓",
                RenderGlyphMode::Ascii => "+",
            },
            TeammateStatus::Paused => match glyphs.mode {
                RenderGlyphMode::Unicode => "○",
                RenderGlyphMode::Ascii => "o",
            },
        }
    }

    fn status_color(&self, theme: &Theme) -> Color {
        match self.status {
            TeammateStatus::Active => theme.primary,
            TeammateStatus::Requesting => theme.text_dim,
            TeammateStatus::Done => theme.success,
            TeammateStatus::Paused => theme.text_dim,
        }
    }
}

pub struct TeammateSpinnerLineWidget<'a> {
    state: &'a TeammateSpinnerLineState,
    theme: &'a Theme,
    indent: u16,
    glyphs: RenderGlyphs,
}

impl<'a> TeammateSpinnerLineWidget<'a> {
    pub fn new(state: &'a TeammateSpinnerLineState, theme: &'a Theme) -> Self {
        Self {
            state,
            theme,
            indent: 0,
            glyphs: RenderGlyphs::default(),
        }
    }

    pub fn with_indent(mut self, indent: u16) -> Self {
        self.indent = indent;
        self
    }

    pub fn glyphs(mut self, glyphs: RenderGlyphs) -> Self {
        self.glyphs = glyphs;
        self
    }
}

impl<'a> Widget for TeammateSpinnerLineWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 8 || area.height == 0 {
            return;
        }

        let mut x = area.x.saturating_add(self.indent.min(area.width));
        let max_x = area.x.saturating_add(area.width);
        if x >= max_x {
            return;
        }

        buf.set_string(
            x,
            area.y,
            self.state.status_icon(self.glyphs),
            Style::default().fg(self.state.status_color(self.theme)),
        );
        x = x.saturating_add(
            display_width_u16(self.state.status_icon(self.glyphs)).saturating_add(1),
        );

        if x >= max_x {
            return;
        }
        let name_style = if self.state.is_selected {
            Style::default()
                .fg(self.theme.primary)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(self.theme.text)
        };
        let name = truncate_cells(
            &self.state.name,
            max_x.saturating_sub(x).saturating_sub(2).min(20) as usize,
        );
        buf.set_string(x, area.y, &name, name_style);
        x = x.saturating_add(UnicodeWidthStr::width(name.as_str()).min(u16::MAX as usize) as u16);

        if !self.state.message.is_empty() && x + 3 < max_x {
            let sep = self.glyphs.separator();
            buf.set_string(x, area.y, sep, Style::default().fg(self.theme.text_dim));
            x = x.saturating_add(UnicodeWidthStr::width(sep).min(u16::MAX as usize) as u16);
            let message = truncate_cells(&self.state.message, max_x.saturating_sub(x) as usize);
            buf.set_string(
                x,
                area.y,
                &message,
                Style::default().fg(self.theme.text_dim),
            );
        }
    }
}

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
}

pub struct TeammateSpinnerTreeWidget<'a> {
    state: &'a TeammateSpinnerTreeState,
    theme: &'a Theme,
    glyphs: RenderGlyphs,
}

impl<'a> TeammateSpinnerTreeWidget<'a> {
    pub fn new(state: &'a TeammateSpinnerTreeState, theme: &'a Theme) -> Self {
        Self {
            state,
            theme,
            glyphs: RenderGlyphs::default(),
        }
    }

    pub fn glyphs(mut self, glyphs: RenderGlyphs) -> Self {
        self.glyphs = glyphs;
        self
    }
}

impl<'a> Widget for TeammateSpinnerTreeWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width < 8 || self.state.teammates.is_empty() {
            return;
        }

        let visible_count = self
            .state
            .teammates
            .len()
            .min((area.height as usize).saturating_sub(1));
        for (i, teammate) in self.state.teammates.iter().take(visible_count).enumerate() {
            let line_area = Rect::new(area.x, area.y + i as u16, area.width, 1);
            TeammateSpinnerLineWidget::new(teammate, self.theme)
                .with_indent(2)
                .glyphs(self.glyphs)
                .render(line_area, buf);
        }

        if self.state.show_select_hint && self.state.teammates.len() > 1 {
            let y = area.y + visible_count as u16;
            if y < area.y + area.height {
                let hint = truncate_cells("shift + up/down to select", area.width as usize);
                buf.set_string(
                    area.x + 2.min(area.width.saturating_sub(1)),
                    y,
                    hint,
                    Style::default()
                        .fg(self.theme.text_dim)
                        .add_modifier(Modifier::ITALIC),
                );
            }
        }
    }
}

fn truncate_cells(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut used = 0usize;
    for ch in text.chars() {
        let width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used.saturating_add(width) > max_width {
            break;
        }
        used = used.saturating_add(width);
        out.push(ch);
    }
    out
}

fn display_width_u16(text: &str) -> u16 {
    UnicodeWidthStr::width(text).min(u16::MAX as usize) as u16
}

#[cfg(test)]
mod tests {
    use super::{
        GlimmerWidget, SpinnerRowWidget, SpinnerState, TeammateSpinnerLineState,
        TeammateSpinnerTreeState, TeammateSpinnerTreeWidget, TeammateStatus,
    };
    use crate::render_glyphs::RenderGlyphs;
    use crate::render_profile::RenderColorMode;
    use crate::theme::Theme;
    use ratatui::{buffer::Buffer, layout::Rect, style::Color, widgets::Widget};
    use std::time::{Duration, Instant};

    fn render_row(message: &str, width: u16) -> String {
        render_row_with_glyphs(message, width, RenderGlyphs::unicode())
    }

    fn render_row_with_glyphs(message: &str, width: u16, glyphs: RenderGlyphs) -> String {
        let state = SpinnerState::new();
        let mut buf = Buffer::empty(Rect::new(0, 0, width, 1));
        SpinnerRowWidget::new(&state, message)
            .glyphs(glyphs)
            .render(Rect::new(0, 0, width, 1), &mut buf);
        let mut out = String::new();
        for x in 0..width {
            out.push_str(buf[(x, 0)].symbol());
        }
        out
    }

    fn assert_plain_buffer(buf: &Buffer) {
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                let cell = &buf[(x, y)];
                assert_eq!(
                    cell.fg,
                    Color::Reset,
                    "plain spinner leaked foreground at ({x}, {y})"
                );
                assert_eq!(
                    cell.bg,
                    Color::Reset,
                    "plain spinner leaked background at ({x}, {y})"
                );
            }
        }
    }

    #[test]
    fn spinner_row_clips_multibyte_message_by_display_width() {
        let rendered = render_row("逐行阅读代码", 8);

        assert!(
            rendered.contains('逐') && rendered.contains('行') && rendered.contains('阅'),
            "spinner should render visible wide chars:\n{rendered:?}"
        );
        assert!(
            !rendered.contains('代') && !rendered.contains('码'),
            "spinner should not write past available cell width:\n{rendered:?}"
        );
    }

    #[test]
    fn spinner_row_can_render_ascii_frames() {
        let rendered = render_row_with_glyphs("Thinking...", 16, RenderGlyphs::ascii());

        assert!(
            rendered.starts_with('|')
                || rendered.starts_with('/')
                || rendered.starts_with('-')
                || rendered.starts_with('\\'),
            "ascii spinner should use plain frames:\n{rendered:?}"
        );
        for forbidden in ["🍃", "🌿", "☘", "🍀"] {
            assert!(!rendered.contains(forbidden), "{rendered:?}");
        }
    }

    #[test]
    fn plain_color_mode_suppresses_spinner_row_colors() {
        let state = SpinnerState::new();
        let mut buf = Buffer::empty(Rect::new(0, 0, 24, 1));

        SpinnerRowWidget::new(&state, "Thinking")
            .color_mode(RenderColorMode::Plain)
            .render(Rect::new(0, 0, 24, 1), &mut buf);

        assert_plain_buffer(&buf);
    }

    #[test]
    fn plain_color_mode_suppresses_glimmer_colors() {
        let state = SpinnerState::new();
        let mut buf = Buffer::empty(Rect::new(0, 0, 24, 1));

        GlimmerWidget::new(&state, "streaming")
            .color_mode(RenderColorMode::Plain)
            .render(Rect::new(0, 0, 24, 1), &mut buf);

        assert_plain_buffer(&buf);
    }

    #[test]
    fn spinner_activity_keeps_long_active_turn_from_becoming_stalled() {
        let mut state = SpinnerState::new();
        state.started_at = Instant::now() - Duration::from_secs(30);
        state.last_activity_at = Instant::now() - Duration::from_secs(4);
        assert!(state.idle_for() >= Duration::from_secs(3));

        state.mark_activity();

        assert!(state.elapsed() >= Duration::from_secs(30));
        assert!(state.idle_for() < Duration::from_secs(1));
        assert!(!state.stalled);
    }

    #[test]
    fn teammate_tree_clips_multibyte_rows_by_display_width() {
        let theme = Theme::default();
        let state = TeammateSpinnerTreeState::new(vec![TeammateSpinnerLineState::new(
            "逐行阅读子 agent",
            TeammateStatus::Active,
        )
        .with_message("正在检查完整渲染链路")]);
        let mut buf = Buffer::empty(Rect::new(0, 0, 24, 3));

        TeammateSpinnerTreeWidget::new(&state, &theme)
            .glyphs(RenderGlyphs::unicode())
            .render(buf.area, &mut buf);

        let mut rendered = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                rendered.push_str(buf[(x, y)].symbol());
            }
        }
        assert!(rendered.contains("agent") || rendered.contains('逐'));
    }
}
