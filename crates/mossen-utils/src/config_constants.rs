//! # config_constants — 配置常量
//!
//! 对应 TypeScript `utils/configConstants.ts`。
//! 这些常量放在单独的文件中以避免循环依赖问题。
//! 必须保持零依赖。

/// 通知渠道常量。
pub const NOTIFICATION_CHANNELS: &[&str] = &[
    "auto",
    "iterm2",
    "iterm2_with_bell",
    "terminal_bell",
    "kitty",
    "ghostty",
    "notifications_disabled",
];

/// 有效的编辑器模式（不包括已废弃的 'emacs'，它会自动迁移到 'normal'）。
pub const EDITOR_MODES: &[&str] = &["normal", "vim"];

/// 有效的队友模式。
/// 'tmux' = 传统基于 tmux 的队友
/// 'in-process' = 在同一进程中运行的就地队友
/// 'auto' = 基于上下文自动选择（默认）
pub const TEAMMATE_MODES: &[&str] = &["auto", "tmux", "in-process"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notification_channels() {
        assert!(NOTIFICATION_CHANNELS.contains(&"auto"));
        assert!(NOTIFICATION_CHANNELS.contains(&"iterm2"));
    }

    #[test]
    fn test_editor_modes() {
        assert!(EDITOR_MODES.contains(&"normal"));
        assert!(EDITOR_MODES.contains(&"vim"));
    }

    #[test]
    fn test_teammate_modes() {
        assert!(TEAMMATE_MODES.contains(&"auto"));
        assert!(TEAMMATE_MODES.contains(&"tmux"));
        assert!(TEAMMATE_MODES.contains(&"in-process"));
    }
}
