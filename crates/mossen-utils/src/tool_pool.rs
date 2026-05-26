//! # tool_pool — 工具池合并与过滤
//!
//! 对应 TypeScript `utils/toolPool.ts`。
//! 提供 coordinator 模式下的工具过滤和合并逻辑。

use std::collections::HashSet;

/// 工具特征
pub trait Tool: Clone {
    fn name(&self) -> &str;
    fn is_mcp_tool(&self) -> bool;
}

/// 简化的工具实现
#[derive(Debug, Clone)]
pub struct ToolItem {
    pub name: String,
    pub is_mcp: bool,
}

impl Tool for ToolItem {
    fn name(&self) -> &str {
        &self.name
    }
    fn is_mcp_tool(&self) -> bool {
        self.is_mcp
    }
}

/// 工具权限上下文模式
pub type ToolPermissionMode = String;

/// PR 活动订阅工具后缀
const PR_ACTIVITY_TOOL_SUFFIXES: &[&str] = &["subscribe_pr_activity", "unsubscribe_pr_activity"];

/// 检查是否是 PR 活动订阅工具
pub fn is_pr_activity_subscription_tool(name: &str) -> bool {
    PR_ACTIVITY_TOOL_SUFFIXES
        .iter()
        .any(|suffix| name.ends_with(suffix))
}

/// Coordinator 模式允许的工具集（需从 constants 获取）
fn get_coordinator_mode_allowed_tools() -> HashSet<&'static str> {
    // 实际实现中从常量模块加载
    HashSet::new()
}

/// 检查是否处于 coordinator 模式
fn is_coordinator_mode() -> bool {
    false
}

/// 应用 coordinator 工具过滤
///
/// PR 活动订阅工具始终被允许，因为订阅管理属于编排。
pub fn apply_coordinator_tool_filter<T: Tool>(tools: Vec<T>) -> Vec<T> {
    let allowed = get_coordinator_mode_allowed_tools();
    tools
        .into_iter()
        .filter(|t| allowed.contains(t.name()) || is_pr_activity_subscription_tool(t.name()))
        .collect()
}

/// 合并工具池并应用 coordinator 模式过滤。
///
/// 纯函数，合并 initialTools 和 assembled，去重后按名称排序。
/// 内置工具排在 MCP 工具之前以保证 prompt-cache 稳定性。
///
/// # 参数
/// - `initial_tools`: 额外包含的工具（内置 + 启动时 MCP）
/// - `assembled`: assembleToolPool 返回的工具（内置 + MCP，已去重）
/// - `_mode`: 权限上下文模式
pub fn merge_and_filter_tools<T: Tool>(
    initial_tools: Vec<T>,
    assembled: Vec<T>,
    _mode: &ToolPermissionMode,
) -> Vec<T> {
    // 合并并按名称去重（initial_tools 优先）
    let mut seen = HashSet::new();
    let mut all_tools = Vec::new();

    for tool in initial_tools.into_iter().chain(assembled) {
        if seen.insert(tool.name().to_string()) {
            all_tools.push(tool);
        }
    }

    // 分区：内置和 MCP
    let (mut mcp, mut built_in): (Vec<T>, Vec<T>) =
        all_tools.into_iter().partition(|t| t.is_mcp_tool());

    // 按名称排序以保持稳定性
    built_in.sort_by(|a, b| a.name().cmp(b.name()));
    mcp.sort_by(|a, b| a.name().cmp(b.name()));

    let mut tools: Vec<T> = built_in;
    tools.append(&mut mcp);

    // 如果处于 coordinator 模式，应用过滤
    if is_coordinator_mode() {
        return apply_coordinator_tool_filter(tools);
    }

    tools
}
