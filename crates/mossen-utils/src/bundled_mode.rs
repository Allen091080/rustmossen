//! # bundled_mode — 打包模式检测
//!
//! 对应 TypeScript `utils/bundledMode.ts`。
//! 检测运行时是否使用 Bun 或打包模式。

/// 检测当前运行时是否是 Bun。
pub fn is_running_with_bun() -> bool {
    // Rust 没有 Bun.runtime，但可以在编译时通过 feature flag 检测
    cfg!(feature = "bundled")
}

/// 检测是否作为 Bun 编译的独立可执行文件运行。
/// 检测嵌入文件（Bun 编译的可执行文件会包含 Bun.embeddedFiles）。
pub fn is_in_bundled_mode() -> bool {
    // Rust 运行时无法直接访问 Bun.embeddedFiles
    // 此功能需要编译时元数据或通过某种 IPC 机制与 Bun 运行时通信
    // 当前实现返回 false，因为 Rust 进程本身不是 Bun 编译的
    // 如果需要检测父进程是否为 Bun 编译的，可以使用 /proc/self/exe 或其他机制
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_running_with_bun() {
        // 在 Rust 中这个函数会返回编译时决定的值
        let _ = is_running_with_bun();
    }

    #[test]
    fn test_is_in_bundled_mode() {
        assert!(!is_in_bundled_mode());
    }
}
