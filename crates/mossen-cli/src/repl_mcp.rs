//! REPL MCP 全局句柄。
//!
//! 跨模块共享当前 REPL 进程内的 `McpServerManager`，
//! 使得 `setup` 预取阶段、`repl` 运行阶段和 `exit` 清理阶段
//! 能够指向同一个 manager 实例。

use std::sync::{Arc, RwLock};

use once_cell::sync::OnceCell;

use mossen_mcp::discovery::OfficialRegistry;
use mossen_mcp::server::McpServerManager;

/// 全局 McpServerManager 句柄。
static MANAGER: OnceCell<RwLock<Option<Arc<McpServerManager>>>> = OnceCell::new();

/// 全局官方注册表预取句柄（用于 `setup.rs::prefetchOfficialMcpUrls`）。
static OFFICIAL_REGISTRY: OnceCell<Arc<OfficialRegistry>> = OnceCell::new();

fn slot() -> &'static RwLock<Option<Arc<McpServerManager>>> {
    MANAGER.get_or_init(|| RwLock::new(None))
}

/// 设置（或替换）全局 manager。
pub fn set_manager(manager: Arc<McpServerManager>) {
    if let Ok(mut guard) = slot().write() {
        *guard = Some(manager);
    }
}

/// 获取全局 manager 的克隆引用（如果已设置）。
pub fn get_manager() -> Option<Arc<McpServerManager>> {
    slot().read().ok().and_then(|g| g.clone())
}

/// 清空全局 manager（在退出清理后调用）。
pub fn clear_manager() {
    if let Ok(mut guard) = slot().write() {
        *guard = None;
    }
}

/// 获取或初始化官方注册表客户端。
///
/// 使用 `MOSSEN_REMOTE_BASE_URL` 或默认 `https://api.mossen.ai`。
pub fn get_or_init_official_registry() -> Arc<OfficialRegistry> {
    OFFICIAL_REGISTRY
        .get_or_init(|| {
            let base = std::env::var("MOSSEN_REMOTE_BASE_URL")
                .unwrap_or_else(|_| "https://api.mossen.ai".to_string());
            Arc::new(OfficialRegistry::new(&base))
        })
        .clone()
}
