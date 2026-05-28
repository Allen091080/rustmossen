//! # in_process_runner — 进程内队友运行器
//!
//! 对应 TypeScript `utils/swarm/inProcessRunner.ts`（1552 行）。
//!
//! ## 当前状态
//!
//! Rust 端尚未具备进程内 Agent 运行时所需的依赖：`runAgent()`、
//! AppState 桥、AsyncLocalStorage 等待中。本模块刻意保持为空导出，作为
//! "存在但无功能" 的稳定 API 边界 —— 调用方在拼装阶段（main 二进制）使用
//! 替代实现或拒绝进程内队友功能。后续把以下依赖落地后即可恢复同名 API：
//!
//! - `crate::agent_loop::run_agent`（驱动主推理循环）
//! - `crate::teammate::run_with_teammate_context`（已存在，提供线程局部上下文）
//! - 通用 `AppState` 通道（待与前端层一起设计）
//!
//! ## 暴露的句柄
//!
//! 即便没有实现，下游模块仍需要一个稳定的类型名以便 `use` 语句不破裂。
//! 这里提供 [`InProcessRunnerStatus`] 与轻量的 [`run_in_process_teammate`]，
//! 在被调用时直接返回 [`InProcessRunnerStatus::Unsupported`]，让上层走 fallback
//! （TS 端在 leader 不在或拒绝时也走相同路径）。

use serde::{Deserialize, Serialize};

/// 进程内队友运行结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum InProcessRunnerStatus {
    /// Rust 端尚未启用进程内运行器；调用方应回退到外部 spawn。
    Unsupported,
    /// 成功完成 —— 携带最终的 transcript 路径或会话 ID。
    Completed { session_id: String },
    /// 因明确取消而停止。
    Aborted,
    /// 运行期错误。
    Failed { reason: String },
}

/// 进程内队友运行参数（最小可调用集合；与 TS 端 `RunInProcessTeammateOptions`
/// 字段对齐，可拓展）。
#[derive(Debug, Clone, Default)]
pub struct InProcessTeammateOptions {
    pub agent_id: String,
    pub agent_name: String,
    pub team_name: String,
    pub prompt: String,
    pub plan_mode_required: bool,
    pub parent_session_id: Option<String>,
}

/// 启动进程内队友 —— Rust 端尚未集成 `run_agent`，因此恒返回
/// [`InProcessRunnerStatus::Unsupported`]。调用方对此结果应表现得与
/// TS 端 "leader 拒绝/忙碌" 相同：透明回退到外部 spawn。
pub async fn run_in_process_teammate(_options: InProcessTeammateOptions) -> InProcessRunnerStatus {
    InProcessRunnerStatus::Unsupported
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unsupported_runner_returns_unsupported() {
        let result = run_in_process_teammate(InProcessTeammateOptions::default()).await;
        assert_eq!(result, InProcessRunnerStatus::Unsupported);
    }
}
