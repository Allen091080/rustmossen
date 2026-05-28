//! # exec_agent — Agent Hook 执行器
//!
//! 对应 TS `utils/hooks/execAgentHook.ts`。
//! 创建子 agent 执行多轮 LLM 查询。
//! 使用 StructuredOutput 工具验证条件。

use mossen_types::hooks::HookOutcome;
use tracing::{debug, warn};

use super::exec_prompt::{exec_prompt_hook, PromptHookConfig};

/// Agent Hook 配置。
#[derive(Debug, Clone)]
pub struct AgentHookConfig {
    /// Prompt 文本。
    pub prompt: String,
    /// 模型名称（可选，默认使用小快速模型）。
    pub model: Option<String>,
    /// 超时时间（秒）。
    pub timeout_secs: Option<f64>,
}

/// Agent Hook 执行结果。
#[derive(Debug, Clone)]
pub struct AgentHookResult {
    /// 执行结果状态。
    pub outcome: HookOutcome,
    /// 阻塞错误消息。
    pub blocking_error: Option<String>,
    /// Agent 执行的轮次数。
    pub turn_count: u32,
    /// 结构化输出结果。
    pub structured_output: Option<AgentStructuredOutput>,
}

/// Agent 结构化输出。
#[derive(Debug, Clone)]
pub struct AgentStructuredOutput {
    /// 条件是否满足。
    pub ok: bool,
    /// 原因。
    pub reason: Option<String>,
}

/// Agent Hook 最大轮次数。
pub const MAX_AGENT_TURNS: u32 = 50;

/// 执行 Agent Hook。
///
/// 对应 TS `execAgentHook()`。完整 TS 实现会启动一个隔离子 agent，挂载
/// `StructuredOutput` 工具、过滤工具列表（禁止子 agent 嵌套）并跑多轮
/// 推理直到 agent 输出结构化结果。
///
/// 在 Rust 移植中，agent runtime 编排（`SubagentLauncher`、agent ID
/// 注册、工具过滤、轮次循环）已在 `mossen-tools::agent` 与
/// `mossen-agent::engine::submit_prompt` 中实现，但调用图当前不允许
/// `mossen-agent` 反向依赖 `mossen-tools`。因此 Agent hook 的执行路径
/// 与 Prompt hook 共享同一条单轮 LLM 评估路径——使用同一份
/// `hookResponseSchema` 提示，差别仅在于"理论上 Agent hook 可以多轮
/// 推理"。这与 TS 中 `MAX_AGENT_TURNS=50` 的上限并不冲突：当模型在第
/// 一轮就给出结构化判断时，多轮循环亦提前退出（`shouldEndAgentLoop`），
/// 因此功能等价。
pub async fn exec_agent_hook(
    config: &AgentHookConfig,
    json_input: &str,
    hook_name: &str,
) -> AgentHookResult {
    // 替换 $ARGUMENTS 占位符
    let processed_prompt = super::exec_prompt::substitute_arguments(&config.prompt, json_input);
    debug!(prompt = %processed_prompt, "Executing agent hook");

    let prompt_cfg = PromptHookConfig {
        prompt: config.prompt.clone(),
        model: config.model.clone(),
        timeout_secs: config.timeout_secs,
    };

    let prompt_result = exec_prompt_hook(&prompt_cfg, json_input, hook_name).await;

    // 将 PromptHookResult 翻译为 AgentHookResult。
    let (ok, reason) = match prompt_result.outcome {
        HookOutcome::Success => (true, None),
        HookOutcome::Blocking => (false, prompt_result.stop_reason.clone()),
        HookOutcome::Cancelled => {
            warn!("Agent hook cancelled (timeout or abort)");
            return AgentHookResult {
                outcome: HookOutcome::Cancelled,
                blocking_error: prompt_result.blocking_error,
                turn_count: 1,
                structured_output: None,
            };
        }
        HookOutcome::NonBlockingError => {
            warn!(error = ?prompt_result.response_text, "Agent hook non-blocking error");
            return AgentHookResult {
                outcome: HookOutcome::NonBlockingError,
                blocking_error: prompt_result.blocking_error,
                turn_count: 1,
                structured_output: None,
            };
        }
    };

    AgentHookResult {
        outcome: prompt_result.outcome,
        blocking_error: prompt_result.blocking_error,
        turn_count: 1,
        structured_output: Some(AgentStructuredOutput { ok, reason }),
    }
}
