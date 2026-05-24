# Hook 系统审计 / Phase 1 Harness 融合

## 结论

之前的风险不是“hook 类型不存在”，而是 runtime context 没有贯穿到 dialogue。`settings.json` / plugin hooks 需要通过 `HooksContext` 进入 agent loop；只看 `PostSamplingHookRegistry` 或 compact helper 会误判为已接通。

本轮修复后的主链路：

`mossen-cli session_hooks::build_hooks_context`
-> `mossen-tui EngineConfig.compact_hook_context`
-> `PromptParams.hook_context`
-> `OrchestratorConfig.hook_context`
-> `DialogueSpec.hook_context`
-> dialogue lifecycle call sites

Oneshot 路径同样在 `build_oneshot_prompt_params` 中构建 `hook_context` 并传入 `PromptParams`。

## Hook 状态

| Hook | 当前入口 | settings/plugin 配置是否进入主路径 | 说明 |
|---|---|---:|---|
| `Stop` | `dialogue.rs` stop hook manager | 部分 | 现有 stop hook manager 已在 dialogue 内工作；它不是本轮新增的 `HooksContext` executor 路径。 |
| `PostSampling` | `dialogue.rs` SSE 完成后 | 是 | 新增 `PostSampling` settings/plugin 事件；同时保留进程内 `PostSamplingHookRegistry` watcher。 |
| `PreCompact` | `compact_conversation_with_options` | 是 | manual `/compact` 已经走 TUI hook context；本轮补齐 dialogue pending compact 与 auto compact。 |
| `PostCompact` | `compact_conversation_with_options` | 是 | 同 PreCompact；auto compact 在有 compact hook 时绕过 session-memory 快路径，保证 hook 会触发。 |
| `SessionStart` | `mossen-cli::repl` startup / resume / oneshot startup | 是 | 生命周期属于 CLI bootstrap，不应该在每个 dialogue turn 内重复触发；hook 输出已进入首个 prompt additional blocks。 |

## 已补调用点

| 调用点 | 行为 |
|---|---|
| `dialogue.rs` turn loop pending compact | `execute_pending_compact_request(..., spec.hook_context.as_deref(), &spec.cancel)`，manual trigger 会执行 Pre/PostCompact。 |
| `dialogue.rs` auto compact | `auto_compact_if_needed(..., spec.hook_context.as_deref(), Some(&spec.cancel))`，auto trigger 会执行 Pre/PostCompact。 |
| `dialogue.rs` post sampling | 流式响应完整收齐后先触发 in-memory watcher，再执行 settings/plugin `PostSampling` hook。 |
| `context/mod.rs` auto compact | 有 Pre/PostCompact hook 时使用 `compact_conversation_with_options(trigger="auto")`，避免 session-memory fast path 吞掉 hook。 |
| `mossen-types` / `hooks_dir` / plugin loader / SDK schema | 注册 `PostSampling` 事件，使 settings/plugin/schema 都能识别它。 |

## 仍需后续阶段处理

- `TaskCompleted` helper 存在，但 task 生命周期还没有完整接入。
- `PreToolUse` / `PostToolUse` 等工具 hook 是否贯穿到当前 tool dispatcher，需要另做 Phase 2 审计和 harness。
- Stop hook 当前走 `StopHookManager`，不是 `mossen-utils::hooks_utils::execute_stop_hooks`；如果要完全对齐 settings executor，需要单独迁移，避免双触发。

## 回归覆盖

- `dialogue_executes_settings_post_sampling_hooks`: mock OpenAI-compatible 响应后，`PostSampling` settings hook 写 marker。
- `pending_compact_request_compacts_state_and_emits_boundary`: dialogue pending compact 带 `HooksContext`，PreCompact 输出进入 compact instructions。
- `auto_compact_forwards_hook_context_with_auto_trigger`: auto compact 带 `HooksContext`，`matcher="auto"` 的 PreCompact 输出进入 compact summary。
- `compact_conversation_executes_pre_and_post_compact_hooks`: compact service 层 Pre/PostCompact executor 原有覆盖。
