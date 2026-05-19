//! Theme definitions for Mossen TUI.
//!
//! Provides color palettes for dark, light, ANSI, and daltonized themes.

use std::fmt;

/// All available theme fields.
#[derive(Debug, Clone)]
pub struct Theme {
    pub auto_accept: String,
    pub bash_border: String,
    pub mossen: String,
    pub mossen_shimmer: String,
    pub mossen_blue_for_system_spinner: String,
    pub mossen_blue_shimmer_for_system_spinner: String,
    pub permission: String,
    pub permission_shimmer: String,
    pub plan_mode: String,
    pub ide: String,
    pub prompt_border: String,
    pub prompt_border_shimmer: String,
    pub text: String,
    pub inverse_text: String,
    pub inactive: String,
    pub inactive_shimmer: String,
    pub subtle: String,
    pub suggestion: String,
    pub remember: String,
    pub background: String,
    pub success: String,
    pub error: String,
    pub warning: String,
    pub merged: String,
    pub warning_shimmer: String,
    pub diff_added: String,
    pub diff_removed: String,
    pub diff_added_dimmed: String,
    pub diff_removed_dimmed: String,
    pub diff_added_word: String,
    pub diff_removed_word: String,
    pub red_for_subagents_only: String,
    pub blue_for_subagents_only: String,
    pub green_for_subagents_only: String,
    pub yellow_for_subagents_only: String,
    pub purple_for_subagents_only: String,
    pub orange_for_subagents_only: String,
    pub pink_for_subagents_only: String,
    pub cyan_for_subagents_only: String,
    pub professional_blue: String,
    pub chrome_yellow: String,
    pub clawd_body: String,
    pub clawd_background: String,
    pub user_message_background: String,
    pub user_message_background_hover: String,
    pub message_actions_background: String,
    pub selection_bg: String,
    pub bash_message_background_color: String,
    pub memory_background_color: String,
    pub rate_limit_fill: String,
    pub rate_limit_empty: String,
    pub fast_mode: String,
    pub fast_mode_shimmer: String,
    pub brief_label_you: String,
    pub brief_label_mossen: String,
    pub rainbow_red: String,
    pub rainbow_orange: String,
    pub rainbow_yellow: String,
    pub rainbow_green: String,
    pub rainbow_blue: String,
    pub rainbow_indigo: String,
    pub rainbow_violet: String,
    pub rainbow_red_shimmer: String,
    pub rainbow_orange_shimmer: String,
    pub rainbow_yellow_shimmer: String,
    pub rainbow_green_shimmer: String,
    pub rainbow_blue_shimmer: String,
    pub rainbow_indigo_shimmer: String,
    pub rainbow_violet_shimmer: String,
}

/// All known theme names.
pub const THEME_NAMES: &[&str] = &[
    "dark",
    "light",
    "light-daltonized",
    "dark-daltonized",
    "light-ansi",
    "dark-ansi",
];

/// A renderable theme name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ThemeName {
    Dark,
    Light,
    LightDaltonized,
    DarkDaltonized,
    LightAnsi,
    DarkAnsi,
}

impl fmt::Display for ThemeName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Dark => write!(f, "dark"),
            Self::Light => write!(f, "light"),
            Self::LightDaltonized => write!(f, "light-daltonized"),
            Self::DarkDaltonized => write!(f, "dark-daltonized"),
            Self::LightAnsi => write!(f, "light-ansi"),
            Self::DarkAnsi => write!(f, "dark-ansi"),
        }
    }
}

impl ThemeName {
    /// Parse from string.
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s {
            "dark" => Some(Self::Dark),
            "light" => Some(Self::Light),
            "light-daltonized" => Some(Self::LightDaltonized),
            "dark-daltonized" => Some(Self::DarkDaltonized),
            "light-ansi" => Some(Self::LightAnsi),
            "dark-ansi" => Some(Self::DarkAnsi),
            _ => None,
        }
    }
}

/// A theme setting as stored in user config. `Auto` follows the system dark/light mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeSetting {
    Auto,
    Named(ThemeName),
}

/// All theme settings including `auto`.
pub const THEME_SETTINGS: &[&str] = &[
    "auto",
    "dark",
    "light",
    "light-daltonized",
    "dark-daltonized",
    "light-ansi",
    "dark-ansi",
];

impl ThemeSetting {
    /// Parse from string.
    pub fn from_str_opt(s: &str) -> Option<Self> {
        if s == "auto" {
            return Some(Self::Auto);
        }
        ThemeName::from_str_opt(s).map(Self::Named)
    }
}

/// Get the theme for the given name.
pub fn get_theme(theme_name: ThemeName) -> Theme {
    match theme_name {
        ThemeName::Light => light_theme(),
        ThemeName::LightAnsi => light_ansi_theme(),
        ThemeName::DarkAnsi => dark_ansi_theme(),
        ThemeName::LightDaltonized => light_daltonized_theme(),
        ThemeName::DarkDaltonized => dark_daltonized_theme(),
        ThemeName::Dark => dark_theme(),
    }
}

/// Converts a theme color string (e.g., `rgb(255,0,0)`) to an ANSI escape sequence prefix.
pub fn theme_color_to_ansi(theme_color: &str) -> String {
    if let Some(rgb) = parse_rgb(theme_color) {
        format!("\x1b[38;2;{};{};{}m", rgb.0, rgb.1, rgb.2)
    } else {
        // Fallback to magenta if parsing fails
        "\x1b[35m".to_string()
    }
}

/// Parse "rgb(r,g,b)" string into (r, g, b) tuple.
fn parse_rgb(s: &str) -> Option<(u8, u8, u8)> {
    let s = s.trim();
    if !s.starts_with("rgb(") || !s.ends_with(')') {
        return None;
    }
    let inner = &s[4..s.len() - 1];
    let parts: Vec<&str> = inner.split(',').collect();
    if parts.len() != 3 {
        return None;
    }
    let r = parts[0].trim().parse::<u8>().ok()?;
    let g = parts[1].trim().parse::<u8>().ok()?;
    let b = parts[2].trim().parse::<u8>().ok()?;
    Some((r, g, b))
}

macro_rules! theme {
    ($($field:ident : $value:expr),* $(,)?) => {
        Theme {
            $($field: $value.to_string()),*
        }
    };
}

fn light_theme() -> Theme {
    theme! {
        auto_accept: "rgb(135,0,255)",
        bash_border: "rgb(255,0,135)",
        mossen: "rgb(103,203,134)",
        mossen_shimmer: "rgb(245,149,117)",
        mossen_blue_for_system_spinner: "rgb(87,105,247)",
        mossen_blue_shimmer_for_system_spinner: "rgb(117,135,255)",
        permission: "rgb(87,105,247)",
        permission_shimmer: "rgb(137,155,255)",
        plan_mode: "rgb(0,102,102)",
        ide: "rgb(71,130,200)",
        prompt_border: "rgb(153,153,153)",
        prompt_border_shimmer: "rgb(183,183,183)",
        text: "rgb(0,0,0)",
        inverse_text: "rgb(255,255,255)",
        inactive: "rgb(102,102,102)",
        inactive_shimmer: "rgb(142,142,142)",
        subtle: "rgb(175,175,175)",
        suggestion: "rgb(87,105,247)",
        remember: "rgb(0,0,255)",
        background: "rgb(0,153,153)",
        success: "rgb(44,122,57)",
        error: "rgb(171,43,63)",
        warning: "rgb(150,108,30)",
        merged: "rgb(135,0,255)",
        warning_shimmer: "rgb(200,158,80)",
        diff_added: "rgb(105,219,124)",
        diff_removed: "rgb(255,168,180)",
        diff_added_dimmed: "rgb(199,225,203)",
        diff_removed_dimmed: "rgb(253,210,216)",
        diff_added_word: "rgb(47,157,68)",
        diff_removed_word: "rgb(209,69,75)",
        red_for_subagents_only: "rgb(220,38,38)",
        blue_for_subagents_only: "rgb(37,99,235)",
        green_for_subagents_only: "rgb(22,163,74)",
        yellow_for_subagents_only: "rgb(202,138,4)",
        purple_for_subagents_only: "rgb(147,51,234)",
        orange_for_subagents_only: "rgb(234,88,12)",
        pink_for_subagents_only: "rgb(219,39,119)",
        cyan_for_subagents_only: "rgb(8,145,178)",
        professional_blue: "rgb(106,155,204)",
        chrome_yellow: "rgb(251,188,4)",
        clawd_body: "rgb(103,203,134)",
        clawd_background: "rgb(34,130,74)",
        user_message_background: "rgb(240,240,240)",
        user_message_background_hover: "rgb(252,252,252)",
        message_actions_background: "rgb(232,236,244)",
        selection_bg: "rgb(180,213,255)",
        bash_message_background_color: "rgb(250,245,250)",
        memory_background_color: "rgb(230,245,250)",
        rate_limit_fill: "rgb(87,105,247)",
        rate_limit_empty: "rgb(39,47,111)",
        fast_mode: "rgb(255,106,0)",
        fast_mode_shimmer: "rgb(255,150,50)",
        brief_label_you: "rgb(37,99,235)",
        brief_label_mossen: "rgb(103,203,134)",
        rainbow_red: "rgb(235,95,87)",
        rainbow_orange: "rgb(245,139,87)",
        rainbow_yellow: "rgb(250,195,95)",
        rainbow_green: "rgb(145,200,130)",
        rainbow_blue: "rgb(130,170,220)",
        rainbow_indigo: "rgb(155,130,200)",
        rainbow_violet: "rgb(200,130,180)",
        rainbow_red_shimmer: "rgb(250,155,147)",
        rainbow_orange_shimmer: "rgb(255,185,137)",
        rainbow_yellow_shimmer: "rgb(255,225,155)",
        rainbow_green_shimmer: "rgb(185,230,180)",
        rainbow_blue_shimmer: "rgb(180,205,240)",
        rainbow_indigo_shimmer: "rgb(195,180,230)",
        rainbow_violet_shimmer: "rgb(230,180,210)",
    }
}

fn dark_theme() -> Theme {
    theme! {
        auto_accept: "rgb(175,135,255)",
        bash_border: "rgb(253,93,177)",
        mossen: "rgb(103,203,134)",
        mossen_shimmer: "rgb(235,159,127)",
        mossen_blue_for_system_spinner: "rgb(147,165,255)",
        mossen_blue_shimmer_for_system_spinner: "rgb(177,195,255)",
        permission: "rgb(177,185,249)",
        permission_shimmer: "rgb(207,215,255)",
        plan_mode: "rgb(72,150,140)",
        ide: "rgb(71,130,200)",
        prompt_border: "rgb(136,136,136)",
        prompt_border_shimmer: "rgb(166,166,166)",
        text: "rgb(255,255,255)",
        inverse_text: "rgb(0,0,0)",
        inactive: "rgb(153,153,153)",
        inactive_shimmer: "rgb(193,193,193)",
        subtle: "rgb(80,80,80)",
        suggestion: "rgb(177,185,249)",
        remember: "rgb(177,185,249)",
        background: "rgb(0,204,204)",
        success: "rgb(78,186,101)",
        error: "rgb(255,107,128)",
        warning: "rgb(255,193,7)",
        merged: "rgb(175,135,255)",
        warning_shimmer: "rgb(255,223,57)",
        diff_added: "rgb(34,92,43)",
        diff_removed: "rgb(122,41,54)",
        diff_added_dimmed: "rgb(71,88,74)",
        diff_removed_dimmed: "rgb(105,72,77)",
        diff_added_word: "rgb(56,166,96)",
        diff_removed_word: "rgb(179,89,107)",
        red_for_subagents_only: "rgb(220,38,38)",
        blue_for_subagents_only: "rgb(37,99,235)",
        green_for_subagents_only: "rgb(22,163,74)",
        yellow_for_subagents_only: "rgb(202,138,4)",
        purple_for_subagents_only: "rgb(147,51,234)",
        orange_for_subagents_only: "rgb(234,88,12)",
        pink_for_subagents_only: "rgb(219,39,119)",
        cyan_for_subagents_only: "rgb(8,145,178)",
        professional_blue: "rgb(106,155,204)",
        chrome_yellow: "rgb(251,188,4)",
        clawd_body: "rgb(103,203,134)",
        clawd_background: "rgb(34,130,74)",
        user_message_background: "rgb(55,55,55)",
        user_message_background_hover: "rgb(70,70,70)",
        message_actions_background: "rgb(44,50,62)",
        selection_bg: "rgb(38,79,120)",
        bash_message_background_color: "rgb(65,60,65)",
        memory_background_color: "rgb(55,65,70)",
        rate_limit_fill: "rgb(177,185,249)",
        rate_limit_empty: "rgb(80,83,112)",
        fast_mode: "rgb(255,120,20)",
        fast_mode_shimmer: "rgb(255,165,70)",
        brief_label_you: "rgb(122,180,232)",
        brief_label_mossen: "rgb(103,203,134)",
        rainbow_red: "rgb(235,95,87)",
        rainbow_orange: "rgb(245,139,87)",
        rainbow_yellow: "rgb(250,195,95)",
        rainbow_green: "rgb(145,200,130)",
        rainbow_blue: "rgb(130,170,220)",
        rainbow_indigo: "rgb(155,130,200)",
        rainbow_violet: "rgb(200,130,180)",
        rainbow_red_shimmer: "rgb(250,155,147)",
        rainbow_orange_shimmer: "rgb(255,185,137)",
        rainbow_yellow_shimmer: "rgb(255,225,155)",
        rainbow_green_shimmer: "rgb(185,230,180)",
        rainbow_blue_shimmer: "rgb(180,205,240)",
        rainbow_indigo_shimmer: "rgb(195,180,230)",
        rainbow_violet_shimmer: "rgb(230,180,210)",
    }
}

fn light_ansi_theme() -> Theme {
    theme! {
        auto_accept: "ansi:magenta",
        bash_border: "ansi:magenta",
        mossen: "ansi:greenBright",
        mossen_shimmer: "ansi:yellowBright",
        mossen_blue_for_system_spinner: "ansi:blue",
        mossen_blue_shimmer_for_system_spinner: "ansi:blueBright",
        permission: "ansi:blue",
        permission_shimmer: "ansi:blueBright",
        plan_mode: "ansi:cyan",
        ide: "ansi:blueBright",
        prompt_border: "ansi:white",
        prompt_border_shimmer: "ansi:whiteBright",
        text: "ansi:black",
        inverse_text: "ansi:white",
        inactive: "ansi:blackBright",
        inactive_shimmer: "ansi:white",
        subtle: "ansi:blackBright",
        suggestion: "ansi:blue",
        remember: "ansi:blue",
        background: "ansi:cyan",
        success: "ansi:green",
        error: "ansi:red",
        warning: "ansi:yellow",
        merged: "ansi:magenta",
        warning_shimmer: "ansi:yellowBright",
        diff_added: "ansi:green",
        diff_removed: "ansi:red",
        diff_added_dimmed: "ansi:green",
        diff_removed_dimmed: "ansi:red",
        diff_added_word: "ansi:greenBright",
        diff_removed_word: "ansi:redBright",
        red_for_subagents_only: "ansi:red",
        blue_for_subagents_only: "ansi:blue",
        green_for_subagents_only: "ansi:green",
        yellow_for_subagents_only: "ansi:yellow",
        purple_for_subagents_only: "ansi:magenta",
        orange_for_subagents_only: "ansi:redBright",
        pink_for_subagents_only: "ansi:magentaBright",
        cyan_for_subagents_only: "ansi:cyan",
        professional_blue: "ansi:blueBright",
        chrome_yellow: "ansi:yellow",
        clawd_body: "ansi:greenBright",
        clawd_background: "ansi:green",
        user_message_background: "ansi:white",
        user_message_background_hover: "ansi:whiteBright",
        message_actions_background: "ansi:white",
        selection_bg: "ansi:cyan",
        bash_message_background_color: "ansi:whiteBright",
        memory_background_color: "ansi:white",
        rate_limit_fill: "ansi:yellow",
        rate_limit_empty: "ansi:black",
        fast_mode: "ansi:red",
        fast_mode_shimmer: "ansi:redBright",
        brief_label_you: "ansi:blue",
        brief_label_mossen: "ansi:greenBright",
        rainbow_red: "ansi:red",
        rainbow_orange: "ansi:redBright",
        rainbow_yellow: "ansi:yellow",
        rainbow_green: "ansi:green",
        rainbow_blue: "ansi:cyan",
        rainbow_indigo: "ansi:blue",
        rainbow_violet: "ansi:magenta",
        rainbow_red_shimmer: "ansi:redBright",
        rainbow_orange_shimmer: "ansi:yellow",
        rainbow_yellow_shimmer: "ansi:yellowBright",
        rainbow_green_shimmer: "ansi:greenBright",
        rainbow_blue_shimmer: "ansi:cyanBright",
        rainbow_indigo_shimmer: "ansi:blueBright",
        rainbow_violet_shimmer: "ansi:magentaBright",
    }
}

fn dark_ansi_theme() -> Theme {
    theme! {
        auto_accept: "ansi:magentaBright",
        bash_border: "ansi:magentaBright",
        mossen: "ansi:greenBright",
        mossen_shimmer: "ansi:yellowBright",
        mossen_blue_for_system_spinner: "ansi:blueBright",
        mossen_blue_shimmer_for_system_spinner: "ansi:blueBright",
        permission: "ansi:blueBright",
        permission_shimmer: "ansi:blueBright",
        plan_mode: "ansi:cyanBright",
        ide: "ansi:blue",
        prompt_border: "ansi:white",
        prompt_border_shimmer: "ansi:whiteBright",
        text: "ansi:whiteBright",
        inverse_text: "ansi:black",
        inactive: "ansi:white",
        inactive_shimmer: "ansi:whiteBright",
        subtle: "ansi:white",
        suggestion: "ansi:blueBright",
        remember: "ansi:blueBright",
        background: "ansi:cyanBright",
        success: "ansi:greenBright",
        error: "ansi:redBright",
        warning: "ansi:yellowBright",
        merged: "ansi:magentaBright",
        warning_shimmer: "ansi:yellowBright",
        diff_added: "ansi:green",
        diff_removed: "ansi:red",
        diff_added_dimmed: "ansi:green",
        diff_removed_dimmed: "ansi:red",
        diff_added_word: "ansi:greenBright",
        diff_removed_word: "ansi:redBright",
        red_for_subagents_only: "ansi:redBright",
        blue_for_subagents_only: "ansi:blueBright",
        green_for_subagents_only: "ansi:greenBright",
        yellow_for_subagents_only: "ansi:yellowBright",
        purple_for_subagents_only: "ansi:magentaBright",
        orange_for_subagents_only: "ansi:redBright",
        pink_for_subagents_only: "ansi:magentaBright",
        cyan_for_subagents_only: "ansi:cyanBright",
        professional_blue: "rgb(106,155,204)",
        chrome_yellow: "ansi:yellowBright",
        clawd_body: "ansi:greenBright",
        clawd_background: "ansi:green",
        user_message_background: "ansi:blackBright",
        user_message_background_hover: "ansi:white",
        message_actions_background: "ansi:blackBright",
        selection_bg: "ansi:blue",
        bash_message_background_color: "ansi:black",
        memory_background_color: "ansi:blackBright",
        rate_limit_fill: "ansi:yellow",
        rate_limit_empty: "ansi:white",
        fast_mode: "ansi:redBright",
        fast_mode_shimmer: "ansi:redBright",
        brief_label_you: "ansi:blueBright",
        brief_label_mossen: "ansi:greenBright",
        rainbow_red: "ansi:red",
        rainbow_orange: "ansi:redBright",
        rainbow_yellow: "ansi:yellow",
        rainbow_green: "ansi:green",
        rainbow_blue: "ansi:cyan",
        rainbow_indigo: "ansi:blue",
        rainbow_violet: "ansi:magenta",
        rainbow_red_shimmer: "ansi:redBright",
        rainbow_orange_shimmer: "ansi:yellow",
        rainbow_yellow_shimmer: "ansi:yellowBright",
        rainbow_green_shimmer: "ansi:greenBright",
        rainbow_blue_shimmer: "ansi:cyanBright",
        rainbow_indigo_shimmer: "ansi:blueBright",
        rainbow_violet_shimmer: "ansi:magentaBright",
    }
}

fn light_daltonized_theme() -> Theme {
    theme! {
        auto_accept: "rgb(135,0,255)",
        bash_border: "rgb(0,102,204)",
        mossen: "rgb(70,170,90)",
        mossen_shimmer: "rgb(255,183,101)",
        mossen_blue_for_system_spinner: "rgb(51,102,255)",
        mossen_blue_shimmer_for_system_spinner: "rgb(101,152,255)",
        permission: "rgb(51,102,255)",
        permission_shimmer: "rgb(101,152,255)",
        plan_mode: "rgb(51,102,102)",
        ide: "rgb(71,130,200)",
        prompt_border: "rgb(153,153,153)",
        prompt_border_shimmer: "rgb(183,183,183)",
        text: "rgb(0,0,0)",
        inverse_text: "rgb(255,255,255)",
        inactive: "rgb(102,102,102)",
        inactive_shimmer: "rgb(142,142,142)",
        subtle: "rgb(175,175,175)",
        suggestion: "rgb(51,102,255)",
        remember: "rgb(51,102,255)",
        background: "rgb(0,153,153)",
        success: "rgb(0,102,153)",
        error: "rgb(204,0,0)",
        warning: "rgb(255,153,0)",
        merged: "rgb(135,0,255)",
        warning_shimmer: "rgb(255,183,50)",
        diff_added: "rgb(153,204,255)",
        diff_removed: "rgb(255,204,204)",
        diff_added_dimmed: "rgb(209,231,253)",
        diff_removed_dimmed: "rgb(255,233,233)",
        diff_added_word: "rgb(51,102,204)",
        diff_removed_word: "rgb(153,51,51)",
        red_for_subagents_only: "rgb(204,0,0)",
        blue_for_subagents_only: "rgb(0,102,204)",
        green_for_subagents_only: "rgb(0,204,0)",
        yellow_for_subagents_only: "rgb(255,204,0)",
        purple_for_subagents_only: "rgb(128,0,128)",
        orange_for_subagents_only: "rgb(255,128,0)",
        pink_for_subagents_only: "rgb(255,102,178)",
        cyan_for_subagents_only: "rgb(0,178,178)",
        professional_blue: "rgb(106,155,204)",
        chrome_yellow: "rgb(251,188,4)",
        clawd_body: "rgb(103,203,134)",
        clawd_background: "rgb(34,130,74)",
        user_message_background: "rgb(220,220,220)",
        user_message_background_hover: "rgb(232,232,232)",
        message_actions_background: "rgb(210,216,226)",
        selection_bg: "rgb(180,213,255)",
        bash_message_background_color: "rgb(250,245,250)",
        memory_background_color: "rgb(230,245,250)",
        rate_limit_fill: "rgb(51,102,255)",
        rate_limit_empty: "rgb(23,46,114)",
        fast_mode: "rgb(255,106,0)",
        fast_mode_shimmer: "rgb(255,150,50)",
        brief_label_you: "rgb(37,99,235)",
        brief_label_mossen: "rgb(70,170,90)",
        rainbow_red: "rgb(235,95,87)",
        rainbow_orange: "rgb(245,139,87)",
        rainbow_yellow: "rgb(250,195,95)",
        rainbow_green: "rgb(145,200,130)",
        rainbow_blue: "rgb(130,170,220)",
        rainbow_indigo: "rgb(155,130,200)",
        rainbow_violet: "rgb(200,130,180)",
        rainbow_red_shimmer: "rgb(250,155,147)",
        rainbow_orange_shimmer: "rgb(255,185,137)",
        rainbow_yellow_shimmer: "rgb(255,225,155)",
        rainbow_green_shimmer: "rgb(185,230,180)",
        rainbow_blue_shimmer: "rgb(180,205,240)",
        rainbow_indigo_shimmer: "rgb(195,180,230)",
        rainbow_violet_shimmer: "rgb(230,180,210)",
    }
}

fn dark_daltonized_theme() -> Theme {
    theme! {
        auto_accept: "rgb(175,135,255)",
        bash_border: "rgb(51,153,255)",
        mossen: "rgb(70,170,90)",
        mossen_shimmer: "rgb(255,183,101)",
        mossen_blue_for_system_spinner: "rgb(153,204,255)",
        mossen_blue_shimmer_for_system_spinner: "rgb(183,224,255)",
        permission: "rgb(153,204,255)",
        permission_shimmer: "rgb(183,224,255)",
        plan_mode: "rgb(102,153,153)",
        ide: "rgb(71,130,200)",
        prompt_border: "rgb(136,136,136)",
        prompt_border_shimmer: "rgb(166,166,166)",
        text: "rgb(255,255,255)",
        inverse_text: "rgb(0,0,0)",
        inactive: "rgb(153,153,153)",
        inactive_shimmer: "rgb(193,193,193)",
        subtle: "rgb(80,80,80)",
        suggestion: "rgb(153,204,255)",
        remember: "rgb(153,204,255)",
        background: "rgb(0,204,204)",
        success: "rgb(51,153,255)",
        error: "rgb(255,102,102)",
        warning: "rgb(255,204,0)",
        merged: "rgb(175,135,255)",
        warning_shimmer: "rgb(255,234,50)",
        diff_added: "rgb(0,68,102)",
        diff_removed: "rgb(102,0,0)",
        diff_added_dimmed: "rgb(62,81,91)",
        diff_removed_dimmed: "rgb(62,44,44)",
        diff_added_word: "rgb(0,119,179)",
        diff_removed_word: "rgb(179,0,0)",
        red_for_subagents_only: "rgb(255,102,102)",
        blue_for_subagents_only: "rgb(102,178,255)",
        green_for_subagents_only: "rgb(102,255,102)",
        yellow_for_subagents_only: "rgb(255,255,102)",
        purple_for_subagents_only: "rgb(178,102,255)",
        orange_for_subagents_only: "rgb(255,178,102)",
        pink_for_subagents_only: "rgb(255,153,204)",
        cyan_for_subagents_only: "rgb(102,204,204)",
        professional_blue: "rgb(106,155,204)",
        chrome_yellow: "rgb(251,188,4)",
        clawd_body: "rgb(103,203,134)",
        clawd_background: "rgb(34,130,74)",
        user_message_background: "rgb(55,55,55)",
        user_message_background_hover: "rgb(70,70,70)",
        message_actions_background: "rgb(44,50,62)",
        selection_bg: "rgb(38,79,120)",
        bash_message_background_color: "rgb(65,60,65)",
        memory_background_color: "rgb(55,65,70)",
        rate_limit_fill: "rgb(153,204,255)",
        rate_limit_empty: "rgb(69,92,115)",
        fast_mode: "rgb(255,120,20)",
        fast_mode_shimmer: "rgb(255,165,70)",
        brief_label_you: "rgb(122,180,232)",
        brief_label_mossen: "rgb(70,170,90)",
        rainbow_red: "rgb(235,95,87)",
        rainbow_orange: "rgb(245,139,87)",
        rainbow_yellow: "rgb(250,195,95)",
        rainbow_green: "rgb(145,200,130)",
        rainbow_blue: "rgb(130,170,220)",
        rainbow_indigo: "rgb(155,130,200)",
        rainbow_violet: "rgb(200,130,180)",
        rainbow_red_shimmer: "rgb(250,155,147)",
        rainbow_orange_shimmer: "rgb(255,185,137)",
        rainbow_yellow_shimmer: "rgb(255,225,155)",
        rainbow_green_shimmer: "rgb(185,230,180)",
        rainbow_blue_shimmer: "rgb(180,205,240)",
        rainbow_indigo_shimmer: "rgb(195,180,230)",
        rainbow_violet_shimmer: "rgb(230,180,210)",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rgb() {
        assert_eq!(parse_rgb("rgb(255,0,128)"), Some((255, 0, 128)));
        assert_eq!(parse_rgb("rgb( 10, 20, 30 )"), Some((10, 20, 30)));
        assert_eq!(parse_rgb("ansi:red"), None);
    }

    #[test]
    fn test_theme_color_to_ansi() {
        let result = theme_color_to_ansi("rgb(255,0,0)");
        assert_eq!(result, "\x1b[38;2;255;0;0m");
    }

    #[test]
    fn test_theme_color_to_ansi_fallback() {
        let result = theme_color_to_ansi("ansi:red");
        assert_eq!(result, "\x1b[35m");
    }

    #[test]
    fn test_get_theme() {
        let theme = get_theme(ThemeName::Dark);
        assert_eq!(theme.text, "rgb(255,255,255)");

        let theme = get_theme(ThemeName::Light);
        assert_eq!(theme.text, "rgb(0,0,0)");
    }

    #[test]
    fn test_theme_name_from_str() {
        assert_eq!(ThemeName::from_str_opt("dark"), Some(ThemeName::Dark));
        assert_eq!(ThemeName::from_str_opt("light"), Some(ThemeName::Light));
        assert_eq!(ThemeName::from_str_opt("invalid"), None);
    }

    #[test]
    fn test_theme_setting_from_str() {
        assert_eq!(ThemeSetting::from_str_opt("auto"), Some(ThemeSetting::Auto));
        assert_eq!(
            ThemeSetting::from_str_opt("dark"),
            Some(ThemeSetting::Named(ThemeName::Dark))
        );
    }
}
