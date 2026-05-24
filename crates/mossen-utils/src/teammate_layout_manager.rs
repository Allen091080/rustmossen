//! # teammate_layout_manager — 队友布局管理器
//!
//! 对应 TypeScript `utils/swarm/teammateLayoutManager.ts`。
//! 管理队友窗格的布局和颜色分配。

use std::collections::HashMap;
use std::sync::Mutex;

/// 代理颜色名称
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentColorName {
    Blue,
    Green,
    Yellow,
    Red,
    Magenta,
    Cyan,
    White,
}

impl AgentColorName {
    pub fn as_str(&self) -> &'static str {
        match self {
            AgentColorName::Blue => "blue",
            AgentColorName::Green => "green",
            AgentColorName::Yellow => "yellow",
            AgentColorName::Red => "red",
            AgentColorName::Magenta => "magenta",
            AgentColorName::Cyan => "cyan",
            AgentColorName::White => "white",
        }
    }
}

/// 所有可用的代理颜色
const AGENT_COLORS: &[AgentColorName] = &[
    AgentColorName::Blue,
    AgentColorName::Green,
    AgentColorName::Yellow,
    AgentColorName::Red,
    AgentColorName::Magenta,
    AgentColorName::Cyan,
    AgentColorName::White,
];

/// 追踪队友颜色分配的映射
lazy_static::lazy_static! {
    static ref TEAMMATE_COLOR_ASSIGNMENTS: Mutex<HashMap<String, AgentColorName>> = Mutex::new(HashMap::new());
    static ref COLOR_INDEX: Mutex<usize> = Mutex::new(0);
}

/// 为队友分配唯一的颜色。
/// 颜色以轮询顺序分配。
pub fn assign_teammate_color(teammate_id: &str) -> AgentColorName {
    let mut assignments = TEAMMATE_COLOR_ASSIGNMENTS.lock().unwrap();
    let mut color_index = COLOR_INDEX.lock().unwrap();

    if let Some(color) = assignments.get(teammate_id) {
        return *color;
    }

    let color = AGENT_COLORS[*color_index % AGENT_COLORS.len()];
    assignments.insert(teammate_id.to_string(), color);
    *color_index += 1;

    color
}

/// 获取队友的颜色（如果有）。
pub fn get_teammate_color(teammate_id: &str) -> Option<AgentColorName> {
    let assignments = TEAMMATE_COLOR_ASSIGNMENTS.lock().unwrap();
    assignments.get(teammate_id).copied()
}

/// 清除所有队友颜色分配。
/// 在团队清理期间调用以重置状态。
pub fn clear_teammate_colors() {
    let mut assignments = TEAMMATE_COLOR_ASSIGNMENTS.lock().unwrap();
    let mut color_index = COLOR_INDEX.lock().unwrap();
    assignments.clear();
    *color_index = 0;
}

/// Pane backend 抽象：tmux / iTerm2 适配器都实现这个 trait，由调用方传入。
///
/// TS 端 `teammateLayoutManager.ts` 在运行时通过 `getBackend()` 探测环境
/// 返回 `TmuxBackend` 或 `ITermBackend`。Rust 端把 backend 提到 trait 入参，
/// 让 utils crate 保持对终端模拟器的零依赖；具体实现位于 [`crate::swarm`]
/// 的 backends 子模块（外部生成）。
#[async_trait::async_trait]
pub trait PaneBackend: Send + Sync {
    async fn create_teammate_pane_in_swarm_view(
        &self,
        teammate_name: &str,
        teammate_color: AgentColorName,
    ) -> Result<(String, bool), String>;

    async fn enable_pane_border_status(
        &self,
        window_target: Option<&str>,
        use_swarm_socket: bool,
    ) -> Result<(), String>;

    async fn send_command_to_pane(
        &self,
        pane_id: &str,
        command: &str,
        use_swarm_socket: bool,
    ) -> Result<(), String>;
}

/// 在 swarm 视图中创建新的队友窗格 —— 委托给 [`PaneBackend`]。
pub async fn create_teammate_pane_in_swarm_view(
    backend: &dyn PaneBackend,
    teammate_name: &str,
    teammate_color: AgentColorName,
) -> Result<(String, bool), String> {
    backend
        .create_teammate_pane_in_swarm_view(teammate_name, teammate_color)
        .await
}

/// 启用窗格边框状态（显示窗格标题）—— 委托给 [`PaneBackend`]。
pub async fn enable_pane_border_status(
    backend: &dyn PaneBackend,
    window_target: Option<&str>,
    use_swarm_socket: bool,
) -> Result<(), String> {
    backend
        .enable_pane_border_status(window_target, use_swarm_socket)
        .await
}

/// 向特定窗格发送命令 —— 委托给 [`PaneBackend`]。
pub async fn send_command_to_pane(
    backend: &dyn PaneBackend,
    pane_id: &str,
    command: &str,
    use_swarm_socket: bool,
) -> Result<(), String> {
    backend
        .send_command_to_pane(pane_id, command, use_swarm_socket)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assign_teammate_color() {
        clear_teammate_colors();

        let color1 = assign_teammate_color("teammate1");
        let color2 = assign_teammate_color("teammate2");

        assert_eq!(color1, AgentColorName::Blue);
        assert_eq!(color2, AgentColorName::Green);
    }

    #[test]
    fn test_get_teammate_color() {
        clear_teammate_colors();

        let color = assign_teammate_color("teammate1");
        assert_eq!(get_teammate_color("teammate1"), Some(color));
        assert_eq!(get_teammate_color("unknown"), None);
    }

    #[test]
    fn test_clear_teammate_colors() {
        assign_teammate_color("teammate1");
        clear_teammate_colors();
        assert_eq!(get_teammate_color("teammate1"), None);
    }
}
