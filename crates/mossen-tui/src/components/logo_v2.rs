//! Logo and branding components (V2).
//!
//! Translates: LogoV2/AnimatedAsterisk.tsx, LogoV2/AnimatedClawd.tsx,
//! LogoV2/ChannelsNotice.tsx, LogoV2/Clawd.tsx, LogoV2/CondensedLogo.tsx,
//! LogoV2/EmergencyTip.tsx, LogoV2/Feed.tsx, LogoV2/FeedColumn.tsx,
//! LogoV2/feedConfigs.tsx, LogoV2/GuestPassesUpsell.tsx, LogoV2/LogoV2.tsx,
//! LogoV2/MossenAgentBanner.tsx, LogoV2/Opus1mMergeNotice.tsx,
//! LogoV2/OverageCreditUpsell.tsx, LogoV2/VoiceModeNotice.tsx, LogoV2/WelcomeV2.tsx

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
// Constants
// ===================================================================

/// Width of the Mossen agent banner.
pub const MOSSEN_AGENT_BANNER_WIDTH: u16 = 62;

/// Text mark for Mossen branding.
pub const MOSSEN_TEXT_MARK: &str = "✻";

/// Width for the welcome V2 layout.
pub const WELCOME_V2_WIDTH: u16 = 58;

/// Maximum width for the left panel in logo display.
pub const LEFT_PANEL_MAX_WIDTH: usize = 80;

// ===================================================================
// Feed types — from Feed.tsx, FeedColumn.tsx, feedConfigs.tsx
// ===================================================================

/// A single line in a feed display.
#[derive(Debug, Clone)]
pub struct FeedLine {
    pub text: String,
    pub timestamp: Option<String>,
}

impl FeedLine {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            timestamp: None,
        }
    }

    pub fn with_timestamp(mut self, ts: impl Into<String>) -> Self {
        self.timestamp = Some(ts.into());
        self
    }
}

/// Configuration for a feed panel.
#[derive(Debug, Clone)]
pub struct FeedConfig {
    pub title: String,
    pub lines: Vec<FeedLine>,
    pub footer: Option<String>,
    pub empty_message: Option<String>,
    pub custom_content_width: Option<usize>,
}

impl FeedConfig {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            lines: Vec::new(),
            footer: None,
            empty_message: None,
            custom_content_width: None,
        }
    }

    pub fn with_lines(mut self, lines: Vec<FeedLine>) -> Self {
        self.lines = lines;
        self
    }

    pub fn with_footer(mut self, footer: impl Into<String>) -> Self {
        self.footer = Some(footer.into());
        self
    }

    pub fn with_empty_message(mut self, msg: impl Into<String>) -> Self {
        self.empty_message = Some(msg.into());
        self
    }

    /// Calculate the minimum width needed for this feed.
    pub fn calculate_width(&self) -> usize {
        let mut max_width = unicode_width_str(&self.title);

        if let Some(cw) = self.custom_content_width {
            max_width = max_width.max(cw);
        } else if self.lines.is_empty() {
            if let Some(ref msg) = self.empty_message {
                max_width = max_width.max(unicode_width_str(msg));
            }
        } else {
            let max_ts_width = self
                .lines
                .iter()
                .filter_map(|l| l.timestamp.as_ref())
                .map(|ts| unicode_width_str(ts))
                .max()
                .unwrap_or(0);

            for line in &self.lines {
                let ts_part = if max_ts_width > 0 { max_ts_width + 2 } else { 0 };
                let line_width = unicode_width_str(&line.text) + ts_part;
                max_width = max_width.max(line_width);
            }
        }

        if let Some(ref footer) = self.footer {
            max_width = max_width.max(unicode_width_str(footer));
        }

        max_width
    }
}

/// Helper to measure string width.
fn unicode_width_str(s: &str) -> usize {
    unicode_width::UnicodeWidthStr::width(s)
}

/// Feed widget — renders a single feed panel.
pub struct FeedWidget<'a> {
    pub config: &'a FeedConfig,
    pub actual_width: u16,
    pub theme: &'a Theme,
}

impl<'a> FeedWidget<'a> {
    pub fn new(config: &'a FeedConfig, actual_width: u16, theme: &'a Theme) -> Self {
        Self {
            config,
            actual_width,
            theme,
        }
    }
}

impl<'a> Widget for FeedWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width < 5 {
            return;
        }

        let mut y = area.y;

        // Title
        let title_style = Style::default()
            .fg(self.theme.primary)
            .add_modifier(Modifier::BOLD);
        let title = truncate_str(&self.config.title, area.width as usize);
        buf.set_string(area.x, y, &title, title_style);
        y += 1;

        if y >= area.y + area.height {
            return;
        }

        // Lines or empty message
        if self.config.lines.is_empty() {
            if let Some(ref msg) = self.config.empty_message {
                let display = truncate_str(msg, area.width as usize);
                buf.set_string(area.x, y, &display, Style::default().fg(Color::DarkGray));
            }
        } else {
            let max_ts_width = self
                .config
                .lines
                .iter()
                .filter_map(|l| l.timestamp.as_ref())
                .map(|ts| unicode_width_str(ts))
                .max()
                .unwrap_or(0);

            for line in &self.config.lines {
                if y >= area.y + area.height {
                    break;
                }

                let text_avail = if max_ts_width > 0 {
                    (area.width as usize).saturating_sub(max_ts_width + 2)
                } else {
                    area.width as usize
                };
                let text = truncate_str(&line.text, text_avail);
                buf.set_string(area.x, y, &text, Style::default().fg(self.theme.text));

                if let Some(ref ts) = line.timestamp {
                    let ts_x = area.x + area.width - max_ts_width as u16;
                    buf.set_string(ts_x, y, ts, Style::default().fg(Color::DarkGray));
                }
                y += 1;
            }
        }

        // Footer
        if let Some(ref footer) = self.config.footer {
            if y < area.y + area.height {
                let display = truncate_str(footer, area.width as usize);
                buf.set_string(area.x, y, &display, Style::default().fg(Color::DarkGray));
            }
        }
    }
}

/// FeedColumn widget — renders multiple feeds side-by-side.
pub struct FeedColumnWidget<'a> {
    pub feeds: &'a [FeedConfig],
    pub gap: u16,
    pub theme: &'a Theme,
}

impl<'a> FeedColumnWidget<'a> {
    pub fn new(feeds: &'a [FeedConfig], theme: &'a Theme) -> Self {
        Self {
            feeds,
            gap: 2,
            theme,
        }
    }
}

impl<'a> Widget for FeedColumnWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if self.feeds.is_empty() || area.width < 10 || area.height < 2 {
            return;
        }

        let widths: Vec<usize> = self.feeds.iter().map(|f| f.calculate_width()).collect();
        let total_gap = (self.feeds.len().saturating_sub(1) as u16) * self.gap;
        let total_content: u16 = widths.iter().map(|&w| w as u16).sum();

        if total_content + total_gap > area.width {
            // Fall back: just render first feed
            FeedWidget::new(&self.feeds[0], area.width, self.theme).render(area, buf);
            return;
        }

        let mut x = area.x;
        for (i, feed) in self.feeds.iter().enumerate() {
            let w = widths[i] as u16;
            let feed_area = Rect::new(x, area.y, w, area.height);
            FeedWidget::new(feed, w, self.theme).render(feed_area, buf);
            x += w + self.gap;
        }
    }
}

// ===================================================================
// AnimatedAsterisk — from AnimatedAsterisk.tsx
// ===================================================================

/// Animated asterisk characters for branding.
const ASTERISK_FRAMES: &[&str] = &["✱", "✲", "✳", "✴", "✵", "✶", "✷", "✸", "✹", "✺", "✻"];

/// State for animated asterisk.
#[derive(Debug, Clone)]
pub struct AnimatedAsteriskState {
    pub frame_index: usize,
    pub last_tick: Instant,
    pub interval: Duration,
}

impl AnimatedAsteriskState {
    pub fn new() -> Self {
        Self {
            frame_index: 0,
            last_tick: Instant::now(),
            interval: Duration::from_millis(150),
        }
    }

    pub fn tick(&mut self) {
        if self.last_tick.elapsed() >= self.interval {
            self.frame_index = (self.frame_index + 1) % ASTERISK_FRAMES.len();
            self.last_tick = Instant::now();
        }
    }

    pub fn current_frame(&self) -> &'static str {
        ASTERISK_FRAMES[self.frame_index]
    }
}

impl Default for AnimatedAsteriskState {
    fn default() -> Self {
        Self::new()
    }
}

/// Widget for animated asterisk.
pub struct AnimatedAsteriskWidget<'a> {
    pub state: &'a AnimatedAsteriskState,
    pub theme: &'a Theme,
}

impl<'a> AnimatedAsteriskWidget<'a> {
    pub fn new(state: &'a AnimatedAsteriskState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for AnimatedAsteriskWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        buf.set_string(
            area.x,
            area.y,
            self.state.current_frame(),
            Style::default().fg(self.theme.primary),
        );
    }
}

// ===================================================================
// Clawd / AnimatedClawd — from Clawd.tsx, AnimatedClawd.tsx
// ===================================================================

/// Static ASCII art for the Clawd mascot.
const CLAWD_ART: &[&str] = &[
    "  /\\_/\\  ",
    " ( o.o ) ",
    "  > ^ <  ",
];

/// Widget for the static Clawd mascot.
pub struct ClawdWidget<'a> {
    pub theme: &'a Theme,
}

impl<'a> ClawdWidget<'a> {
    pub fn new(theme: &'a Theme) -> Self {
        Self { theme }
    }
}

impl<'a> Widget for ClawdWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        for (i, line) in CLAWD_ART.iter().enumerate() {
            let y = area.y + i as u16;
            if y >= area.y + area.height {
                break;
            }
            buf.set_string(
                area.x,
                y,
                line,
                Style::default().fg(self.theme.primary),
            );
        }
    }
}

/// State for animated Clawd (blinking eyes, etc.).
#[derive(Debug, Clone)]
pub struct AnimatedClawdState {
    pub blink_open: bool,
    pub last_blink: Instant,
    pub blink_interval: Duration,
    pub blink_duration: Duration,
}

impl AnimatedClawdState {
    pub fn new() -> Self {
        Self {
            blink_open: true,
            last_blink: Instant::now(),
            blink_interval: Duration::from_secs(3),
            blink_duration: Duration::from_millis(150),
        }
    }

    pub fn tick(&mut self) {
        let elapsed = self.last_blink.elapsed();
        if self.blink_open && elapsed >= self.blink_interval {
            self.blink_open = false;
            self.last_blink = Instant::now();
        } else if !self.blink_open && elapsed >= self.blink_duration {
            self.blink_open = true;
            self.last_blink = Instant::now();
        }
    }

    pub fn eye_char(&self) -> &'static str {
        if self.blink_open { "o" } else { "-" }
    }
}

impl Default for AnimatedClawdState {
    fn default() -> Self {
        Self::new()
    }
}

/// Widget for animated Clawd.
pub struct AnimatedClawdWidget<'a> {
    pub state: &'a AnimatedClawdState,
    pub theme: &'a Theme,
}

impl<'a> AnimatedClawdWidget<'a> {
    pub fn new(state: &'a AnimatedClawdState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for AnimatedClawdWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 3 || area.width < 9 {
            return;
        }
        let eye = self.state.eye_char();
        let lines = [
            "  /\\_/\\  ".to_string(),
            format!(" ( {}.{} ) ", eye, eye),
            "  > ^ <  ".to_string(),
        ];
        for (i, line) in lines.iter().enumerate() {
            let y = area.y + i as u16;
            if y >= area.y + area.height {
                break;
            }
            buf.set_string(area.x, y, line, Style::default().fg(self.theme.primary));
        }
    }
}

// ===================================================================
// ChannelsNotice — from ChannelsNotice.tsx
// ===================================================================

/// Channel notice type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelNoticeType {
    NewFeature,
    Upgrade,
    Downgrade,
}

/// State for the channels notice.
#[derive(Debug, Clone)]
pub struct ChannelsNoticeState {
    pub notice_type: ChannelNoticeType,
    pub channel_name: String,
    pub message: String,
    pub visible: bool,
    pub seen_count: u32,
    pub max_shows: u32,
}

impl ChannelsNoticeState {
    pub fn new(notice_type: ChannelNoticeType, channel: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            notice_type,
            channel_name: channel.into(),
            message: message.into(),
            visible: true,
            seen_count: 0,
            max_shows: 3,
        }
    }

    pub fn should_show(&self) -> bool {
        self.visible && self.seen_count < self.max_shows
    }

    pub fn mark_seen(&mut self) {
        self.seen_count += 1;
        if self.seen_count >= self.max_shows {
            self.visible = false;
        }
    }

    pub fn dismiss(&mut self) {
        self.visible = false;
    }
}

/// Widget for channels notice.
pub struct ChannelsNoticeWidget<'a> {
    pub state: &'a ChannelsNoticeState,
    pub theme: &'a Theme,
}

impl<'a> ChannelsNoticeWidget<'a> {
    pub fn new(state: &'a ChannelsNoticeState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for ChannelsNoticeWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.state.should_show() || area.height == 0 || area.width < 10 {
            return;
        }

        let icon = match self.state.notice_type {
            ChannelNoticeType::NewFeature => "✨",
            ChannelNoticeType::Upgrade => "⬆",
            ChannelNoticeType::Downgrade => "⬇",
        };

        let line = Line::from(vec![
            Span::raw(format!("{} ", icon)),
            Span::styled(
                &self.state.message,
                Style::default().fg(Color::DarkGray),
            ),
        ]);
        Paragraph::new(line).render(area, buf);
    }
}

// ===================================================================
// EmergencyTip — from EmergencyTip.tsx
// ===================================================================

/// State for emergency tips.
#[derive(Debug, Clone)]
pub struct EmergencyTipState {
    pub tip: String,
    pub visible: bool,
}

impl EmergencyTipState {
    pub fn new(tip: impl Into<String>) -> Self {
        Self {
            tip: tip.into(),
            visible: true,
        }
    }

    pub fn dismiss(&mut self) {
        self.visible = false;
    }
}

/// Widget for emergency tip display.
pub struct EmergencyTipWidget<'a> {
    pub state: &'a EmergencyTipState,
    pub theme: &'a Theme,
}

impl<'a> EmergencyTipWidget<'a> {
    pub fn new(state: &'a EmergencyTipState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for EmergencyTipWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.state.visible || area.height == 0 || area.width < 10 {
            return;
        }
        let line = Line::from(vec![
            Span::styled("⚠ ", Style::default().fg(Color::Yellow)),
            Span::styled(
                &self.state.tip,
                Style::default().fg(Color::Yellow),
            ),
        ]);
        Paragraph::new(line).render(area, buf);
    }
}

// ===================================================================
// GuestPassesUpsell — from GuestPassesUpsell.tsx
// ===================================================================

/// State for guest passes upsell notice.
#[derive(Debug, Clone)]
pub struct GuestPassesUpsellState {
    pub passes_remaining: u32,
    pub visible: bool,
    pub seen_count: u32,
    pub max_shows: u32,
}

impl GuestPassesUpsellState {
    pub fn new(passes_remaining: u32) -> Self {
        Self {
            passes_remaining,
            visible: passes_remaining > 0,
            seen_count: 0,
            max_shows: 5,
        }
    }

    pub fn should_show(&self) -> bool {
        self.visible && self.passes_remaining > 0 && self.seen_count < self.max_shows
    }

    pub fn mark_seen(&mut self) {
        self.seen_count += 1;
    }
}

/// Widget for guest passes upsell.
pub struct GuestPassesUpsellWidget<'a> {
    pub state: &'a GuestPassesUpsellState,
    pub theme: &'a Theme,
}

impl<'a> GuestPassesUpsellWidget<'a> {
    pub fn new(state: &'a GuestPassesUpsellState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for GuestPassesUpsellWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.state.should_show() || area.height == 0 {
            return;
        }
        let msg = format!(
            "🎫 You have {} guest pass{} remaining",
            self.state.passes_remaining,
            if self.state.passes_remaining == 1 { "" } else { "es" }
        );
        Paragraph::new(msg)
            .style(Style::default().fg(self.theme.primary))
            .render(area, buf);
    }
}

// ===================================================================
// OverageCreditUpsell — from OverageCreditUpsell.tsx
// ===================================================================

/// State for overage credit upsell.
#[derive(Debug, Clone)]
pub struct OverageCreditUpsellState {
    pub amount: Option<String>,
    pub visible: bool,
    pub two_line: bool,
}

impl OverageCreditUpsellState {
    pub fn new(amount: Option<String>) -> Self {
        Self {
            visible: amount.is_some(),
            amount,
            two_line: false,
        }
    }

    pub fn with_two_line(mut self) -> Self {
        self.two_line = true;
        self
    }

    fn title(&self) -> String {
        match &self.amount {
            Some(amt) => format!("{} extra credit", amt),
            None => "extra usage credit".to_string(),
        }
    }

    fn subtitle(&self) -> &'static str {
        "Available for overages beyond plan limits"
    }
}

/// Widget for overage credit upsell.
pub struct OverageCreditUpsellWidget<'a> {
    pub state: &'a OverageCreditUpsellState,
    pub max_width: u16,
    pub theme: &'a Theme,
}

impl<'a> OverageCreditUpsellWidget<'a> {
    pub fn new(state: &'a OverageCreditUpsellState, max_width: u16, theme: &'a Theme) -> Self {
        Self {
            state,
            max_width,
            theme,
        }
    }
}

impl<'a> Widget for OverageCreditUpsellWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.state.visible || area.height == 0 {
            return;
        }

        let title = self.state.title();
        let subtitle = self.state.subtitle();

        if self.state.two_line && area.height >= 2 {
            let title_display = truncate_str(&title, area.width as usize);
            buf.set_string(area.x, area.y, &title_display, Style::default().fg(self.theme.primary));
            let sub_display = truncate_str(subtitle, area.width as usize);
            buf.set_string(area.x, area.y + 1, &sub_display, Style::default().fg(Color::DarkGray));
        } else {
            let combined = format!("{} · {}", title, subtitle);
            let display = truncate_str(&combined, area.width as usize);
            // Highlight the title portion
            let highlight_len = title.len().min(display.len());
            let highlighted = &display[..highlight_len];
            let rest = &display[highlight_len..];
            buf.set_string(area.x, area.y, highlighted, Style::default().fg(self.theme.primary));
            if !rest.is_empty() {
                buf.set_string(
                    area.x + highlight_len as u16,
                    area.y,
                    rest,
                    Style::default().fg(Color::DarkGray),
                );
            }
        }
    }
}

// ===================================================================
// MossenAgentBanner — from MossenAgentBanner.tsx
// ===================================================================

/// State for the Mossen agent banner display.
#[derive(Debug, Clone)]
pub struct MossenAgentBannerState {
    pub show_meta: bool,
    pub agent_name: Option<String>,
    pub version: String,
    pub model_name: String,
    pub cwd: String,
    pub billing_type: String,
}

impl MossenAgentBannerState {
    pub fn new(version: impl Into<String>, model: impl Into<String>, cwd: impl Into<String>) -> Self {
        Self {
            show_meta: true,
            agent_name: None,
            version: version.into(),
            model_name: model.into(),
            cwd: cwd.into(),
            billing_type: String::new(),
        }
    }

    pub fn with_agent(mut self, name: impl Into<String>) -> Self {
        self.agent_name = Some(name.into());
        self
    }

    pub fn with_billing(mut self, billing: impl Into<String>) -> Self {
        self.billing_type = billing.into();
        self
    }

    pub fn hide_meta(mut self) -> Self {
        self.show_meta = false;
        self
    }
}

/// Widget for the Mossen agent banner.
pub struct MossenAgentBannerWidget<'a> {
    pub state: &'a MossenAgentBannerState,
    pub theme: &'a Theme,
}

impl<'a> MossenAgentBannerWidget<'a> {
    pub fn new(state: &'a MossenAgentBannerState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for MossenAgentBannerWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < MOSSEN_AGENT_BANNER_WIDTH || area.height < 5 {
            return;
        }

        let mut y = area.y;

        // Clawd art
        for line in CLAWD_ART {
            if y >= area.y + area.height {
                break;
            }
            buf.set_string(area.x, y, line, Style::default().fg(self.theme.primary));
            y += 1;
        }

        if !self.state.show_meta {
            return;
        }

        y += 1; // spacing

        // Product name + version
        if y < area.y + area.height {
            let version_line = Line::from(vec![
                Span::styled(
                    "Mossen",
                    Style::default()
                        .fg(self.theme.primary)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(
                    format!("v{}", self.state.version),
                    Style::default().fg(Color::DarkGray),
                ),
            ]);
            buf.set_line(area.x, y, &version_line, area.width);
            y += 1;
        }

        // Model + billing
        if y < area.y + area.height {
            let model_info = if self.state.billing_type.is_empty() {
                self.state.model_name.clone()
            } else {
                format!("{} · {}", self.state.model_name, self.state.billing_type)
            };
            let display = truncate_str(&model_info, area.width as usize);
            buf.set_string(area.x, y, &display, Style::default().fg(Color::DarkGray));
            y += 1;
        }

        // CWD + agent
        if y < area.y + area.height {
            let cwd_info = if let Some(ref agent) = self.state.agent_name {
                format!("@{} · {}", agent, self.state.cwd)
            } else {
                self.state.cwd.clone()
            };
            let display = truncate_str(&cwd_info, area.width as usize);
            buf.set_string(area.x, y, &display, Style::default().fg(Color::DarkGray));
        }
    }
}

// ===================================================================
// VoiceModeNotice — from VoiceModeNotice.tsx
// ===================================================================

/// State for the voice mode notice.
#[derive(Debug, Clone)]
pub struct VoiceModeNoticeState {
    pub visible: bool,
    pub seen_count: u32,
    pub max_shows: u32,
    pub asterisk: AnimatedAsteriskState,
}

impl VoiceModeNoticeState {
    pub fn new(voice_enabled: bool) -> Self {
        Self {
            visible: !voice_enabled,
            seen_count: 0,
            max_shows: 3,
            asterisk: AnimatedAsteriskState::new(),
        }
    }

    pub fn should_show(&self) -> bool {
        self.visible && self.seen_count < self.max_shows
    }

    pub fn mark_seen(&mut self) {
        self.seen_count += 1;
    }

    pub fn tick(&mut self) {
        self.asterisk.tick();
    }
}

/// Widget for voice mode notice.
pub struct VoiceModeNoticeWidget<'a> {
    pub state: &'a VoiceModeNoticeState,
    pub theme: &'a Theme,
}

impl<'a> VoiceModeNoticeWidget<'a> {
    pub fn new(state: &'a VoiceModeNoticeState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for VoiceModeNoticeWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.state.should_show() || area.height == 0 || area.width < 20 {
            return;
        }
        let frame = self.state.asterisk.current_frame();
        let line = Line::from(vec![
            Span::styled(frame, Style::default().fg(self.theme.primary)),
            Span::styled(
                " Voice mode is now available · /voice to enable",
                Style::default().fg(Color::DarkGray),
            ),
        ]);
        buf.set_line(area.x, area.y, &line, area.width);
    }
}

// ===================================================================
// Opus1mMergeNotice — from Opus1mMergeNotice.tsx
// ===================================================================

/// State for the Opus 1M merge notice.
#[derive(Debug, Clone)]
pub struct Opus1mMergeNoticeState {
    pub visible: bool,
    pub message: String,
}

impl Opus1mMergeNoticeState {
    pub fn new(should_show: bool) -> Self {
        Self {
            visible: should_show,
            message: "opus-4-1m is now the default for Opus. Your model setting has been updated.".to_string(),
        }
    }

    pub fn dismiss(&mut self) {
        self.visible = false;
    }
}

/// Widget for Opus 1M merge notice.
pub struct Opus1mMergeNoticeWidget<'a> {
    pub state: &'a Opus1mMergeNoticeState,
    pub theme: &'a Theme,
}

impl<'a> Opus1mMergeNoticeWidget<'a> {
    pub fn new(state: &'a Opus1mMergeNoticeState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for Opus1mMergeNoticeWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.state.visible || area.height == 0 {
            return;
        }
        let line = Line::from(vec![
            Span::styled("ℹ ", Style::default().fg(Color::Blue)),
            Span::styled(
                &self.state.message,
                Style::default().fg(Color::DarkGray),
            ),
        ]);
        Paragraph::new(line)
            .wrap(Wrap { trim: false })
            .render(area, buf);
    }
}

// ===================================================================
// CondensedLogo — from CondensedLogo.tsx
// ===================================================================

/// State for condensed logo display.
#[derive(Debug, Clone)]
pub struct CondensedLogoState {
    pub version: String,
    pub model_name: String,
    pub effort_suffix: String,
    pub billing_type: String,
    pub cwd: String,
    pub agent_name: Option<String>,
    pub columns: u16,
    pub guest_passes_upsell: Option<GuestPassesUpsellState>,
    pub overage_credit_upsell: Option<OverageCreditUpsellState>,
    pub clawd: AnimatedClawdState,
}

impl CondensedLogoState {
    pub fn new(version: impl Into<String>, model: impl Into<String>, cwd: impl Into<String>) -> Self {
        Self {
            version: version.into(),
            model_name: model.into(),
            effort_suffix: String::new(),
            billing_type: String::new(),
            cwd: cwd.into(),
            agent_name: None,
            columns: 80,
            guest_passes_upsell: None,
            overage_credit_upsell: None,
            clawd: AnimatedClawdState::new(),
        }
    }

    pub fn tick(&mut self) {
        self.clawd.tick();
    }
}

/// Widget for condensed logo.
pub struct CondensedLogoWidget<'a> {
    pub state: &'a CondensedLogoState,
    pub theme: &'a Theme,
}

impl<'a> CondensedLogoWidget<'a> {
    pub fn new(state: &'a CondensedLogoState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for CondensedLogoWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 3 || area.width < 20 {
            return;
        }

        let text_width = (area.width as usize).max(20).saturating_sub(15);

        // Layout: Clawd on left, info on right
        let clawd_area = Rect::new(area.x, area.y, 10, 3.min(area.height));
        AnimatedClawdWidget::new(&self.state.clawd, self.theme).render(clawd_area, buf);

        let info_x = area.x + 12;
        let info_width = area.width.saturating_sub(12);
        let mut y = area.y;

        // Line 1: Product name + version
        if y < area.y + area.height && info_width > 5 {
            let product_title = "Mossen";
            let version_display = truncate_str(&self.state.version, text_width.saturating_sub(13).max(6));
            let line = Line::from(vec![
                Span::styled(product_title, Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" "),
                Span::styled(
                    format!("v{}", version_display),
                    Style::default().fg(Color::DarkGray),
                ),
            ]);
            buf.set_line(info_x, y, &line, info_width);
            y += 1;
        }

        // Line 2: Model + billing
        if y < area.y + area.height && info_width > 5 {
            let model_display = format!("{}{}", self.state.model_name, self.state.effort_suffix);
            let model_billing = if self.state.billing_type.is_empty() {
                model_display
            } else {
                format!("{} · {}", model_display, self.state.billing_type)
            };
            let display = truncate_str(&model_billing, text_width);
            buf.set_string(info_x, y, &display, Style::default().fg(Color::DarkGray));
            y += 1;
        }

        // Line 3: CWD + agent
        if y < area.y + area.height && info_width > 5 {
            let cwd_info = if let Some(ref agent) = self.state.agent_name {
                format!("@{} · {}", agent, self.state.cwd)
            } else {
                self.state.cwd.clone()
            };
            let display = truncate_str(&cwd_info, text_width);
            buf.set_string(info_x, y, &display, Style::default().fg(Color::DarkGray));
            y += 1;
        }

        // Upsells
        if let Some(ref upsell) = self.state.guest_passes_upsell {
            if upsell.should_show() && y < area.y + area.height {
                let upsell_area = Rect::new(info_x, y, info_width, 1);
                GuestPassesUpsellWidget::new(upsell, self.theme).render(upsell_area, buf);
                y += 1;
            }
        }
        if let Some(ref upsell) = self.state.overage_credit_upsell {
            if upsell.visible && y < area.y + area.height {
                let upsell_area = Rect::new(info_x, y, info_width, 2.min(area.y + area.height - y));
                OverageCreditUpsellWidget::new(upsell, info_width, self.theme).render(upsell_area, buf);
            }
        }
    }
}

// ===================================================================
// LogoV2 — from LogoV2.tsx (main logo widget)
// ===================================================================

/// Full logo display state.
#[derive(Debug, Clone)]
pub struct LogoV2State {
    pub condensed: CondensedLogoState,
    pub is_condensed_mode: bool,
    pub show_onboarding: bool,
    pub feeds: Vec<FeedConfig>,
    pub voice_notice: VoiceModeNoticeState,
    pub channels_notice: Option<ChannelsNoticeState>,
    pub opus_notice: Opus1mMergeNoticeState,
    pub emergency_tip: Option<EmergencyTipState>,
}

impl LogoV2State {
    pub fn new(version: impl Into<String>, model: impl Into<String>, cwd: impl Into<String>) -> Self {
        Self {
            condensed: CondensedLogoState::new(version, model, cwd),
            is_condensed_mode: false,
            show_onboarding: false,
            feeds: Vec::new(),
            voice_notice: VoiceModeNoticeState::new(false),
            channels_notice: None,
            opus_notice: Opus1mMergeNoticeState::new(false),
            emergency_tip: None,
        }
    }

    pub fn tick(&mut self) {
        self.condensed.tick();
        self.voice_notice.tick();
    }
}

/// Widget for the full logo V2 display.
pub struct LogoV2Widget<'a> {
    pub state: &'a LogoV2State,
    pub theme: &'a Theme,
}

impl<'a> LogoV2Widget<'a> {
    pub fn new(state: &'a LogoV2State, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for LogoV2Widget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 3 || area.width < 20 {
            return;
        }

        // Condensed mode
        if self.state.is_condensed_mode {
            CondensedLogoWidget::new(&self.state.condensed, self.theme).render(area, buf);
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(6),  // Banner
                Constraint::Length(1),  // Notices
                Constraint::Min(0),    // Feeds
            ])
            .split(area);

        // Banner (full agent banner)
        let banner_state = MossenAgentBannerState::new(
            &self.state.condensed.version,
            &self.state.condensed.model_name,
            &self.state.condensed.cwd,
        );
        MossenAgentBannerWidget::new(&banner_state, self.theme).render(chunks[0], buf);

        // Notices area
        let mut notice_y = chunks[1].y;

        // Voice mode notice
        if self.state.voice_notice.should_show() && notice_y < chunks[1].y + chunks[1].height {
            let notice_area = Rect::new(chunks[1].x, notice_y, chunks[1].width, 1);
            VoiceModeNoticeWidget::new(&self.state.voice_notice, self.theme).render(notice_area, buf);
            notice_y += 1;
        }

        // Channels notice
        if let Some(ref cn) = self.state.channels_notice {
            if cn.should_show() && notice_y < chunks[1].y + chunks[1].height {
                let notice_area = Rect::new(chunks[1].x, notice_y, chunks[1].width, 1);
                ChannelsNoticeWidget::new(cn, self.theme).render(notice_area, buf);
                notice_y += 1;
            }
        }

        // Opus notice
        if self.state.opus_notice.visible && notice_y < chunks[1].y + chunks[1].height {
            let notice_area = Rect::new(chunks[1].x, notice_y, chunks[1].width, 1);
            Opus1mMergeNoticeWidget::new(&self.state.opus_notice, self.theme).render(notice_area, buf);
            let _ = notice_y;
        }

        // Emergency tip
        if let Some(ref tip) = self.state.emergency_tip {
            if tip.visible && chunks[2].height > 0 {
                let tip_area = Rect::new(chunks[2].x, chunks[2].y, chunks[2].width, 1);
                EmergencyTipWidget::new(tip, self.theme).render(tip_area, buf);
            }
        }

        // Feeds
        if !self.state.feeds.is_empty() && chunks[2].height > 1 {
            let feed_y = chunks[2].y + 1;
            let feed_h = chunks[2].height.saturating_sub(1);
            if feed_h > 0 {
                let feed_area = Rect::new(chunks[2].x, feed_y, chunks[2].width, feed_h);
                FeedColumnWidget::new(&self.state.feeds, self.theme).render(feed_area, buf);
            }
        }
    }
}

// ===================================================================
// WelcomeV2 — from WelcomeV2.tsx
// ===================================================================

/// Widget for the welcome screen V2.
pub struct WelcomeV2Widget<'a> {
    pub version: &'a str,
    pub columns: u16,
    pub is_custom_backend: bool,
    pub theme: &'a Theme,
}

impl<'a> WelcomeV2Widget<'a> {
    pub fn new(version: &'a str, columns: u16, theme: &'a Theme) -> Self {
        Self {
            version,
            columns,
            is_custom_backend: false,
            theme,
        }
    }

    pub fn custom_backend(mut self) -> Self {
        self.is_custom_backend = true;
        self
    }
}

impl<'a> Widget for WelcomeV2Widget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 3 || area.width < 20 {
            return;
        }

        if self.columns >= MOSSEN_AGENT_BANNER_WIDTH + 4 {
            // Full banner
            let banner = MossenAgentBannerState::new(self.version, "", "");
            MossenAgentBannerWidget::new(&banner, self.theme).render(area, buf);
            return;
        }

        // Compact welcome
        let width = WELCOME_V2_WIDTH.min(area.width);
        let mut y = area.y;

        // Title line
        if y < area.y + area.height {
            let title_line = if self.is_custom_backend {
                Line::from(vec![
                    Span::styled(MOSSEN_TEXT_MARK, Style::default().fg(self.theme.primary)),
                    Span::raw(" "),
                    Span::styled("Welcome to Mossen", Style::default().fg(self.theme.primary)),
                    Span::raw(" "),
                    Span::styled(
                        format!("v{}", self.version),
                        Style::default().fg(Color::DarkGray),
                    ),
                ])
            } else {
                Line::from(vec![
                    Span::styled("Welcome to Mossen", Style::default().fg(self.theme.primary)),
                    Span::raw(" "),
                    Span::styled(
                        format!("v{}", self.version),
                        Style::default().fg(Color::DarkGray),
                    ),
                ])
            };
            buf.set_line(area.x, y, &title_line, width);
            y += 1;
        }

        // Clawd
        if y + 3 <= area.y + area.height {
            y += 1; // margin
            let clawd_area = Rect::new(area.x, y, 10, 3);
            ClawdWidget::new(self.theme).render(clawd_area, buf);
            y += 3;
        }

        // Product name
        if y < area.y + area.height {
            y += 1;
            buf.set_string(
                area.x,
                y,
                "Mossen",
                Style::default()
                    .fg(self.theme.text)
                    .add_modifier(Modifier::BOLD),
            );
        }
    }
}

// ===================================================================
// Utility
// ===================================================================

/// Truncate a string to fit within `max_width` characters.
fn truncate_str(s: &str, max_width: usize) -> String {
    if unicode_width_str(s) <= max_width {
        s.to_string()
    } else {
        let mut result = String::new();
        let mut width = 0;
        for c in s.chars() {
            let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
            if width + cw + 1 > max_width {
                result.push('…');
                break;
            }
            width += cw;
            result.push(c);
        }
        result
    }
}

// ===================================================================
// Feed width and column logic
// ===================================================================

/// Width of the feed column based on terminal width.
pub fn calculate_feed_width(terminal_width: u16) -> u16 {
    if terminal_width >= 120 {
        40
    } else if terminal_width >= 80 {
        terminal_width / 3
    } else {
        terminal_width.saturating_sub(4)
    }
}

#[derive(Debug, Clone, Default)]
pub struct FeedColumn {
    pub items: Vec<String>,
    pub width: u16,
    pub scroll_offset: usize,
}

// ===================================================================
// OverageCreditUpsell
// ===================================================================

#[derive(Debug, Clone, Default)]
pub struct OverageCreditFeed {
    pub headline: String,
    pub current_credits: u64,
    pub bonus_credits: u64,
    pub valid_until_ms: u64,
}

/// Whether the user is eligible for an overage-credit grant.
pub fn is_eligible_for_overage_credit_grant(
    paid_tier: bool,
    overage_count: u32,
    last_grant_at_ms: u64,
    now_ms: u64,
) -> bool {
    if !paid_tier || overage_count < 2 {
        return false;
    }
    now_ms.saturating_sub(last_grant_at_ms) > 30 * 24 * 3600 * 1000
}

/// Whether to actually show the upsell now (combines eligibility + seen count).
pub fn should_show_overage_credit_upsell(eligible: bool, seen_count: u32) -> bool {
    eligible && seen_count < 3
}

/// Maybe refresh the cached overage-credit state.
pub fn maybe_refresh_overage_credit_cache(last_refresh_ms: u64, now_ms: u64) -> bool {
    now_ms.saturating_sub(last_refresh_ms) > 3600 * 1000
}

/// Hook-equivalent useShowOverageCreditUpsell — returns the should-show flag.
pub fn use_show_overage_credit_upsell(
    paid_tier: bool,
    overage_count: u32,
    last_grant_ms: u64,
    seen_count: u32,
    now_ms: u64,
) -> bool {
    should_show_overage_credit_upsell(
        is_eligible_for_overage_credit_grant(paid_tier, overage_count, last_grant_ms, now_ms),
        seen_count,
    )
}

/// Increment the "seen" counter for the overage credit upsell.
pub fn increment_overage_credit_upsell_seen_count(seen: &mut u32) {
    *seen = seen.saturating_add(1);
}

#[derive(Debug, Clone, Default)]
pub struct OverageCreditUpsell {
    pub feed: OverageCreditFeed,
    pub seen_count: u32,
}

/// Build the OverageCredit feed config.
pub fn create_overage_credit_feed(bonus_credits: u64) -> OverageCreditFeed {
    OverageCreditFeed {
        headline: format!("You earned {} bonus credits", bonus_credits),
        current_credits: 0,
        bonus_credits,
        valid_until_ms: 0,
    }
}

#[derive(Debug, Clone, Default)]
pub struct AnimatedAsterisk {
    pub frame: u32,
}

#[derive(Debug, Clone, Default)]
pub struct AnimatedClawd {
    pub pose: String,
    pub frame: u32,
}

#[derive(Debug, Clone, Default)]
pub struct ChannelsNotice {
    pub channel: String,
}

#[derive(Debug, Clone, Default)]
pub struct VoiceModeNotice {
    pub message: String,
}

// ===================================================================
// feedConfigs
// ===================================================================

#[derive(Debug, Clone, Default)]
pub struct LogoFeedConfig {
    pub id: String,
    pub headline: String,
    pub items: Vec<String>,
}

pub fn create_recent_activity_feed(items: Vec<String>) -> LogoFeedConfig {
    LogoFeedConfig {
        id: "recent".into(),
        headline: "Recent activity".into(),
        items,
    }
}

pub fn create_whats_new_feed(items: Vec<String>) -> LogoFeedConfig {
    LogoFeedConfig {
        id: "whats-new".into(),
        headline: "What's new".into(),
        items,
    }
}

pub fn create_project_onboarding_feed(steps: Vec<String>) -> LogoFeedConfig {
    LogoFeedConfig {
        id: "onboarding".into(),
        headline: "Get started".into(),
        items: steps,
    }
}

pub fn create_guest_passes_feed(remaining: u64) -> LogoFeedConfig {
    LogoFeedConfig {
        id: "guest-passes".into(),
        headline: format!("{} guest passes left", remaining),
        items: vec![],
    }
}

// ===================================================================
// Opus1m merge notice
// ===================================================================

/// Whether to show the Opus 1M merge notice.
pub fn should_show_opus1m_merge_notice(seen_before: bool, model: &str) -> bool {
    !seen_before && (model.contains("opus") || model.contains("Opus"))
}

#[derive(Debug, Clone, Default)]
pub struct Opus1mMergeNotice {
    pub visible: bool,
}

#[derive(Debug, Clone, Default)]
pub struct MossenAgentBanner {
    pub agent_name: String,
}

/// Pose for the Clawd mascot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClawdPose {
    Standing,
    Waving,
    Sleeping,
    Thinking,
    Celebrating,
}

#[derive(Debug, Clone, Default)]
pub struct MossenDotLogo {
    pub size: u16,
}

#[derive(Debug, Clone, Default)]
pub struct WelcomeV2 {
    pub user_name: String,
    pub session_count: u64,
}

#[derive(Debug, Clone, Default)]
pub struct LogoV2 {
    pub welcome: WelcomeV2,
    pub feeds: Vec<LogoFeedConfig>,
}

// ===================================================================
// Guest passes upsell
// ===================================================================

/// Hook-equivalent: whether to show the guest passes upsell.
pub fn use_show_guest_passes_upsell(remaining: u64, seen_count: u32) -> bool {
    remaining > 0 && seen_count < 5
}

/// Increment the guest passes seen count.
pub fn increment_guest_passes_seen_count(seen: &mut u32) {
    *seen = seen.saturating_add(1);
}

#[derive(Debug, Clone, Default)]
pub struct GuestPassesUpsell {
    pub remaining: u64,
    pub seen_count: u32,
}

#[derive(Debug, Clone, Default)]
pub struct EmergencyTip {
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct CondensedLogo {
    pub label: String,
}
