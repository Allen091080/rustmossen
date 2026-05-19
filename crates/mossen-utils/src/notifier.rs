//! # notifier — 多通道通知服务
//!
//! 对应 TS `services/notifier.ts`。支持 iterm2、kitty、ghostty、
//! terminal_bell 等终端通知通道。

// ---------------------------------------------------------------------------
// 类型定义
// ---------------------------------------------------------------------------

/// 通知选项。
///
/// 对应 TS `NotificationOptions`。
#[derive(Debug, Clone)]
pub struct NotificationOptions {
    /// 通知消息。
    pub message: String,
    /// 通知标题。
    pub title: Option<String>,
    /// 通知类型。
    pub notification_type: String,
}

/// 通知通道。
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

impl NotificationChannel {
    /// 从字符串解析通道。
    pub fn from_str(s: &str) -> Self {
        match s {
            "auto" => Self::Auto,
            "iterm2" => Self::ITerm2,
            "iterm2_with_bell" => Self::ITerm2WithBell,
            "kitty" => Self::Kitty,
            "ghostty" => Self::Ghostty,
            "terminal_bell" => Self::TerminalBell,
            "notifications_disabled" => Self::Disabled,
            _ => Self::Auto,
        }
    }
}

/// 通知发送 trait。
///
/// 终端实现此 trait 来处理不同通道的通知。
pub trait TerminalNotification: Send + Sync {
    /// iTerm2 通知（OSC 9）。
    fn notify_iterm2(&self, opts: &NotificationOptions);
    /// iTerm2 + bell 通知。
    fn notify_bell(&self);
    /// Kitty 通知（OSC 99）。
    fn notify_kitty(&self, opts: &NotificationOptions, title: &str, id: u32);
    /// Ghostty 通知（OSC 777）。
    fn notify_ghostty(&self, opts: &NotificationOptions, title: &str);
}

// ---------------------------------------------------------------------------
// 通知发送
// ---------------------------------------------------------------------------

/// 获取默认通知标题。
pub fn get_default_notification_title(product_name: &str) -> String {
    product_name.to_string()
}

/// 发送通知到指定通道。
///
/// 对应 TS `sendNotification()`。返回实际使用的方法名称。
pub fn send_notification(
    opts: &NotificationOptions,
    channel: &NotificationChannel,
    terminal: &dyn TerminalNotification,
    terminal_type: Option<&str>,
    default_title: &str,
) -> String {
    let title = opts.title.as_deref().unwrap_or(default_title);

    match channel {
        NotificationChannel::Auto => send_auto(opts, terminal, terminal_type, title),
        NotificationChannel::ITerm2 => {
            terminal.notify_iterm2(opts);
            "iterm2".to_string()
        }
        NotificationChannel::ITerm2WithBell => {
            terminal.notify_iterm2(opts);
            terminal.notify_bell();
            "iterm2_with_bell".to_string()
        }
        NotificationChannel::Kitty => {
            let id = generate_kitty_id();
            terminal.notify_kitty(opts, title, id);
            "kitty".to_string()
        }
        NotificationChannel::Ghostty => {
            terminal.notify_ghostty(opts, title);
            "ghostty".to_string()
        }
        NotificationChannel::TerminalBell => {
            terminal.notify_bell();
            "terminal_bell".to_string()
        }
        NotificationChannel::Disabled => "disabled".to_string(),
    }
}

/// 自动检测终端类型并发送通知。
fn send_auto(
    opts: &NotificationOptions,
    terminal: &dyn TerminalNotification,
    terminal_type: Option<&str>,
    title: &str,
) -> String {
    match terminal_type {
        Some("Apple_Terminal") => {
            terminal.notify_bell();
            "terminal_bell".to_string()
        }
        Some("iTerm.app") => {
            terminal.notify_iterm2(opts);
            "iterm2".to_string()
        }
        Some("kitty") => {
            let id = generate_kitty_id();
            terminal.notify_kitty(opts, title, id);
            "kitty".to_string()
        }
        Some("ghostty") => {
            terminal.notify_ghostty(opts, title);
            "ghostty".to_string()
        }
        _ => "no_method_available".to_string(),
    }
}

/// 生成 Kitty 通知 ID。
fn generate_kitty_id() -> u32 {
    rand::random::<u32>() % 10000
}
