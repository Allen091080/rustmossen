# Hook 系统审计 / Phase 1-1

## 已定义的 hook 类型清单

| Hook 类型 | 类型定义位置 | Manager / Executor 位置 | Context struct 字段（核心） |
|---|---|---|---|
| StopHook | `crates/mossen-agent/src/stop_hooks.rs:19` `StopHookResult`；`crates/mossen-agent/src/stop_hooks.rs:32` `StopHook` | `crates/mossen-agent/src/stop_hooks.rs:58` `StopHookManager` | `StopHookContext { session_id, origin_tag, auto_mode, turn_count }` |
| PostSamplingHook | `crates/mossen-agent/src/hooks/post_sampling.rs:15` `PostInferenceContext` | `crates/mossen-agent/src/hooks/post_sampling.rs:38` `PostSamplingHookRegistry` | `messages_json, system_prompt, user_context, system_context, query_source` |
| PreCompactHook | `crates/mossen-utils/src/hooks.rs:2397` `execute_pre_compact_hooks` | `crates/mossen-utils/src/hooks.rs:1107` `execute_hooks_outside_repl` | base hook input plus `trigger` and optional `custom_instructions` |
| PostCompactHook | `crates/mossen-utils/src/hooks.rs:2489` `execute_post_compact_hooks` | `crates/mossen-utils/src/hooks.rs:1107` `execute_hooks_outside_repl` | base hook input plus `trigger` and `compact_summary` |
| SessionStartHook | `crates/mossen-utils/src/hooks_utils.rs:3004` `execute_session_start_hooks` | `crates/mossen-utils/src/hooks_utils.rs:1490` `execute_hooks` | `session_id, transcript_path, cwd, hook_event_name, source, agent_type, model` |
| TaskCompletedHook | `crates/mossen-utils/src/hooks_utils.rs:2931` `execute_task_completed_hooks` | `crates/mossen-utils/src/hooks_utils.rs:1490` `execute_hooks` | `task_id, task_subject, task_description, teammate_name, team_name, permission_mode` |
| ExecCommandHook | `crates/mossen-agent/src/hooks/exec_command.rs:17` `CommandHookContext` | `crates/mossen-agent/src/hooks/executor.rs:293` `execute_command_hook` | command text, cwd/env-facing execution data in command-hook JSON input |
| ExecAgentHook | `crates/mossen-agent/src/hooks/exec_agent.rs:14` `AgentHookConfig` | `crates/mossen-agent/src/hooks/executor.rs:191` `execute_single_hook` dispatch | prompt, timeout, structured output holder |
| FileChangedHook | `crates/mossen-agent/src/hooks/file_watcher.rs:42` `FileChangeEvent` | `crates/mossen-agent/src/hooks/file_watcher.rs:53` `FileChangedWatcher` | path, event kind, hook settings matcher |

## 在 dialogue.rs / compact / cli main 中的实际调用

| 调用点 | 哪类 hook | 调用前上下文（在 turn 哪个阶段） | 调用后行为（结果如何影响主路径） |
|---|---|---|---|
| `crates/mossen-agent/src/dialogue.rs:347` | PostSamplingHook | SSE 流结束、`StreamAccumulator` 已收齐 assistant 内容、usage 尚未计费前 | 调用 `PostSamplingHookRegistry::fire_post_inference_watchers`; 单个 watcher 失败只记 warn，不影响 turn |
| `crates/mossen-agent/src/dialogue.rs:477` | StopHook | assistant message 拼好且没有 tool_use，准备终止本 turn 前 | `Allow` 结束；`Block` 把 assistant message 入历史并继续；`Prevent` 返回 `TerminalReason::StopHookPrevented` |
| `crates/mossen-agent/src/services/compact/compact.rs:418` | PreCompactHook | `compact_conversation()` 入口，收到待压缩消息后 | 当前实现是 `tracing::info!` 通知点，不读取用户 hook 配置，不改变压缩输入 |
| `crates/mossen-agent/src/services/compact/compact.rs:437` | PostCompactHook | `CompactConversationResult` 构造完成、返回前 | 当前实现是 `tracing::info!` 通知点，不执行配置 hook，不改变返回值 |
| `crates/mossen-cli/src/main.rs:216` | SessionStartHook | `setup::run_setup` 完成后、route_command 前 | 当前实现是 `tracing::info!` 生命周期通知，包含 cwd 与交互模式 |
| `crates/mossen-cli/src/repl.rs:97` | SessionStartHook | 交互式 REPL 状态设为 interactive 并确定 cwd/model 后 | 当前实现是 `tracing::info!` 生命周期通知，包含 cwd 与 `is_interactive=true` |

## 缺口（hook 已定义但未在主路径完整调度）

- [x] PostSampling: 已在 `dialogue.rs:347` 调用 `PostSamplingHookRegistry`，是内存 watcher 形态，不是 settings.json shell hook 形态。
- [x] StopHook: 已在 `dialogue.rs:477` 调用 `StopHookManager`。
- [~] PreCompact: `compact.rs:418` 有调度点和日志，但没有接 `mossen-utils::hooks::execute_pre_compact_hooks` 所需的 `HookMatcher`、env、transcript_path 和 cancel token。
- [~] PostCompact: `compact.rs:437` 有调度点和日志，但没有接 `mossen-utils::hooks::execute_post_compact_hooks` 所需的 runtime hook 配置。
- [~] SessionStart: `main.rs:216` 与 `repl.rs:97` 有生命周期日志，但没有构建 `HooksContext` 调用 `execute_session_start_hooks`。
- [ ] TaskCompleted: `mossen-utils/src/hooks_utils.rs:2931` 有 helper，但主路径未在 task 完成时调用。
- [ ] ExecCommand/ExecAgent: 执行器模块存在，但 dialogue/tool dispatcher 未按 hook settings 调度。

## Phase 1 后续 task 的精确 wire 目标

### 1-2 PostSamplingHook

- 期望调用点：`crates/mossen-agent/src/dialogue.rs:347`，在 API 流式接收完成后、usage/cost 统计前。
- 当前状态：已接入 `PostSamplingHookRegistry::fire_post_inference_watchers`。
- ctx 字段：`messages_json` 来自 `accumulator.content_blocks` 的 debug 串，`system_prompt` 来自 `DialogueSpec.system_prompt`，`query_source` 来自 `OriginTag`。
- 返回值处理：registry 只支持观察型 `Result<()>`，单 hook 失败记录 warn 后继续。

### 1-3 Pre/PostCompactHook

- pre 期望调用点：`crates/mossen-agent/src/services/compact/compact.rs:418`。
- post 期望调用点：`crates/mossen-agent/src/services/compact/compact.rs:437`。
- 当前状态：已有稳定调度点和 info 日志，未执行用户配置的 shell/http/prompt/agent hooks。
- 完整接入仍需把 compact 调用链扩展出 `HookMatcher`、cwd、transcript_path、env_vars、timeout 和 `CancellationToken`。这些数据当前不在 `compact_conversation(messages, file_read_tool_name)` 签名中。
- 失败策略：pre hook 失败应跳过压缩或保留原输入；post hook 失败应保留 `CompactConversationResult` 并只展示失败消息。

### 1-4 SessionStartHook

- 期望调用点：`crates/mossen-cli/src/main.rs:216` 和 `crates/mossen-cli/src/repl.rs:97`。
- 当前状态：两个启动路径都有 `tracing::info!` 生命周期通知，足以在 `RUST_LOG=mossen_agent::hooks=debug/info` 下观测会话启动。
- 完整接入仍需从 settings/plugin/session state 构建 `mossen-utils::hooks_utils::HooksContext`，再调用 `execute_session_start_hooks`。
- 返回值处理：SessionStart 应该 best-effort；失败只记录，不阻塞启动。

### 1-5 Sub-Agent Isolation Audit

- 审计文件：`crates/mossen-tools/src/agent_tool/ISOLATION_AUDIT.md`。
- 当前结论：transcript 路径、`override_agent_id`、`task-notification` 回灌均有证据；后续如果要强测，需要添加端到端 sub-agent transcript 测试。
