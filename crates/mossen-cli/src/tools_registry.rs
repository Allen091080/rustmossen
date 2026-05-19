//! 工具注册表 — 连接 mossen-tools 的所有 Instrument。
//!
//! 对应 TS 的 tools.ts，负责构建完整的工具注册表。

use mossen_agent::tool_registry::Tool;
use tracing::info;

/// 全局工具注册表。
pub struct InstrumentRegistry {
    /// 所有已注册的工具实例。
    instruments: Vec<Box<dyn Tool>>,
}

impl InstrumentRegistry {
    /// 构建完整的工具注册表。
    ///
    /// 加载所有内置工具（P0 + P1 + P2 + P3），对应 TS 的 TOOLS() + getTools()。
    pub fn new() -> Self {
        let instruments = mossen_tools::all_tools();
        info!(count = instruments.len(), "instrument registry initialized");
        Self { instruments }
    }

    /// 仅加载 P0 核心工具（轻量模式）。
    pub fn core_only() -> Self {
        let instruments = mossen_tools::all_p0_tools();
        info!(
            count = instruments.len(),
            "instrument registry initialized (core only)"
        );
        Self { instruments }
    }

    /// 按名称查找工具。
    pub fn find(&self, name: &str) -> Option<&dyn Tool> {
        self.instruments
            .iter()
            .find(|t| t.name() == name)
            .map(|t| t.as_ref())
    }

    /// 过滤可用工具（排除禁用列表）。
    pub fn filter_enabled(&self, disabled: &[String]) -> Vec<&dyn Tool> {
        self.instruments
            .iter()
            .filter(|t| !disabled.contains(&t.name().to_string()))
            .map(|t| t.as_ref())
            .collect()
    }

    /// 仅保留指定名称的工具。
    pub fn filter_only(&self, allowed: &[String]) -> Vec<&dyn Tool> {
        if allowed.is_empty() {
            return self.instruments.iter().map(|t| t.as_ref()).collect();
        }
        self.instruments
            .iter()
            .filter(|t| allowed.contains(&t.name().to_string()))
            .map(|t| t.as_ref())
            .collect()
    }

    /// 获取所有已注册工具的引用。
    pub fn all(&self) -> &[Box<dyn Tool>] {
        &self.instruments
    }

    /// 获取工具数量。
    pub fn len(&self) -> usize {
        self.instruments.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.instruments.is_empty()
    }
}

impl Default for InstrumentRegistry {
    fn default() -> Self {
        Self::new()
    }
}
