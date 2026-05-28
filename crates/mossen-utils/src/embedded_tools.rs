//! # embedded_tools — 内嵌搜索工具检测
//!
//! 对应 TypeScript `utils/embeddedTools.ts`。

/// 当前构建是否内嵌了 bfs/ugrep 搜索工具。
///
/// 为 true 时:
/// - find 和 grep 在 Mossen shell 中被 shell 函数覆盖
/// - 专用 Glob/Grep 工具从工具注册表中移除
/// - 引导 Mossen 避免使用 find/grep 的提示被省略
pub fn has_embedded_search_tools() -> bool {
    let is_truthy = std::env::var("EMBEDDED_SEARCH_TOOLS")
        .map(|v| matches!(v.to_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false);
    if !is_truthy {
        return false;
    }
    let entrypoint = std::env::var("MOSSEN_CODE_ENTRYPOINT").unwrap_or_default();
    entrypoint != "sdk-ts"
        && entrypoint != "sdk-py"
        && entrypoint != "sdk-cli"
        && entrypoint != "local-agent"
}

/// 包含内嵌搜索工具的二进制路径。
///
/// 仅在 has_embedded_search_tools() 为 true 时有意义。
pub fn embedded_search_tools_binary_path() -> String {
    std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default()
}
