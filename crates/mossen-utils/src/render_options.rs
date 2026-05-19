//! # render_options — Ink 渲染选项
//!
//! 对应 TypeScript `utils/renderOptions.ts`。
//! 处理 stdin 被管道时的 TTY 输入重定向。

use std::io::IsTerminal;
use std::sync::OnceLock;

/// 渲染选项
#[derive(Debug, Clone)]
pub struct RenderOptions {
    pub exit_on_ctrl_c: bool,
    pub stdin_override: bool,
}

/// 缓存的 stdin 覆盖状态
static STDIN_OVERRIDE_COMPUTED: OnceLock<bool> = OnceLock::new();

/// 获取 stdin 覆盖状态。
///
/// 当 stdin 是管道时，尝试打开 /dev/tty 作为替代输入源。
/// 结果会在进程生命周期内缓存。
fn get_stdin_override() -> bool {
    *STDIN_OVERRIDE_COMPUTED.get_or_init(|| {
        // 检查 stdin 是否已经是 TTY
        if std::io::stdin().is_terminal() {
            return false;
        }

        // CI 环境跳过
        if std::env::var("CI")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
        {
            return false;
        }

        // MCP 模式下跳过（输入劫持会破坏 MCP）
        if std::env::args().any(|arg| arg == "mcp") {
            return false;
        }

        // Windows 没有 /dev/tty
        if cfg!(windows) {
            return false;
        }

        // 尝试打开 /dev/tty
        match std::fs::OpenOptions::new().read(true).open("/dev/tty") {
            Ok(_) => true,
            Err(_) => false,
        }
    })
}

/// 返回基础渲染选项，包含需要时的 stdin 覆盖。
///
/// # 参数
/// - `exit_on_ctrl_c`: 是否在 Ctrl+C 时退出（对话框通常为 false）
pub fn get_base_render_options(exit_on_ctrl_c: bool) -> RenderOptions {
    let stdin_override = get_stdin_override();
    RenderOptions {
        exit_on_ctrl_c,
        stdin_override,
    }
}
