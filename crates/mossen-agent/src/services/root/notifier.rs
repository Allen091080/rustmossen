//! Notification service — sends terminal/system notifications

use std::process::Stdio;
use tokio::process::Command;
use tracing::{debug, error};

/// Notification options
#[derive(Debug, Clone)]
pub struct NotificationOptions {
    pub message: String,
    pub title: Option<String>,
    pub notification_type: String,
}

/// Notification channel preference
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationChannel {
    Auto,
    ITerm2,
    ITerm2WithBell,
    Kitty,
    Ghostty,
    TerminalBell,
    Disabled,
}

/// Send a notification using the configured channel
pub async fn send_notification(
    opts: &NotificationOptions,
    channel: &NotificationChannel,
    terminal_type: &str,
) -> String {
    let title = opts.title.as_deref().unwrap_or("Mossen");

    match channel {
        NotificationChannel::Auto => send_auto(opts, title, terminal_type).await,
        NotificationChannel::ITerm2 => {
            send_iterm2_notification(title, &opts.message);
            "iterm2".to_string()
        }
        NotificationChannel::ITerm2WithBell => {
            send_iterm2_notification(title, &opts.message);
            send_bell();
            "iterm2_with_bell".to_string()
        }
        NotificationChannel::Kitty => {
            send_kitty_notification(title, &opts.message);
            "kitty".to_string()
        }
        NotificationChannel::Ghostty => {
            send_ghostty_notification(title, &opts.message);
            "ghostty".to_string()
        }
        NotificationChannel::TerminalBell => {
            send_bell();
            "terminal_bell".to_string()
        }
        NotificationChannel::Disabled => "disabled".to_string(),
    }
}

async fn send_auto(opts: &NotificationOptions, title: &str, terminal_type: &str) -> String {
    match terminal_type {
        "iTerm.app" => {
            send_iterm2_notification(title, &opts.message);
            "iterm2".to_string()
        }
        "kitty" => {
            send_kitty_notification(title, &opts.message);
            "kitty".to_string()
        }
        "ghostty" => {
            send_ghostty_notification(title, &opts.message);
            "ghostty".to_string()
        }
        "Apple_Terminal" => {
            send_bell();
            "terminal_bell".to_string()
        }
        _ => "no_method_available".to_string(),
    }
}

fn send_iterm2_notification(title: &str, message: &str) {
    // iTerm2 proprietary escape sequence
    print!("\x1b]9;{}: {}\x07", title, message);
}

fn send_kitty_notification(title: &str, message: &str) {
    let id = rand::random::<u32>() % 10000;
    // Kitty notification escape
    print!("\x1b]99;i={};d=0;{}: {}\x1b\\", id, title, message);
}

fn send_ghostty_notification(title: &str, message: &str) {
    // Ghostty uses OSC 777
    print!("\x1b]777;notify;{};{}\x1b\\", title, message);
}

fn send_bell() {
    print!("\x07");
}

/// TS `getDefaultNotificationTitle` — the default title to show in OS
/// notifications when no per-event title is provided.
pub fn get_default_notification_title() -> String {
    "Mossen".to_string()
}
