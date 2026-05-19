//! # modifiers — 修饰键检测
//!
//! 对应 TypeScript `utils/modifiers.ts`。
//! macOS 原生修饰键检测（仅 macOS 有效）。

/// 修饰键类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModifierKey {
    Shift,
    Command,
    Control,
    Option,
}

/// 预热原生修饰键模块（macOS only，Rust 版本中为 no-op）。
pub fn prewarm_modifiers() {
    // Rust 原生二进制不需要预热 napi 模块
}

/// 检查指定修饰键是否当前被按下。
///
/// 非 macOS 平台始终返回 false。
pub fn is_modifier_pressed(_modifier: ModifierKey) -> bool {
    // 在 Rust 中需要通过 CoreGraphics 或 IOKit 实现
    // 此处保持与 TS 相同的平台检查逻辑
    if cfg!(not(target_os = "macos")) {
        return false;
    }
    // macOS: 需要调用 CGEventSourceFlagsState 等 API
    // 当前简化实现，总是返回 false
    false
}
