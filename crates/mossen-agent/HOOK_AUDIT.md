# Hook 系统审计 / Phase 1-1

## 已定义的 hook 类型清单
| Hook 类型 | 定义位置 | manager / executor 位置 |
|---|---|---|
| StopHook | `mossen-agent/src/stop_hooks.rs:32` (trait) | `StopHookManager` (同文件), `dialogue.rs:435` |
| PostSamplingHook | `mossen-agent/src/hooks/post_sampling.rs:38` (PostSamplingHookRegistry) | 同文件，但 `dialogue.rs` 未调用 |
| PreCompactHook | `mossen-types/src/hooks.rs:16` (HookEvent::PreCompact) | 无 manager 实现，`services/compact/` 未调用 |
| PostCompactHook | `mossen-types/src/hooks.rs:21` (HookEvent::PostCompact) | 无 manager 实现，`services/compact/` 未调用 |
| SessionStartHook | `mossen-types/src/hooks.rs:24` (HookEvent::SessionStart) | 无 manager 实现，`main.rs`/`repl.rs` 未调用 |
| ExecCommandHook | `mossen-agent/src/hooks/exec_command.rs` | HookExecutor 存在，但 `dialogue.rs` 未调度 |
| ExecAgentHook | `mossen-agent/src/hooks/exec_agent.rs` | HookExecutor 存在，但 `dialogue.rs` 未调度 |
| FileChangedHook | `mossen-agent/src/hooks/file_watcher.rs` | 模块存在，但未接入主循环 |
| CwdChangedHook | `mossen-types/src/hooks.rs:47` (HookEvent::CwdChanged) | 仅定义，未接入 |

## 在 dialogue.rs / compact / cli main 中的实际调用
| 调用点 | 调用哪类 hook | 上下文 |
|---|---|---|
| `dialogue.rs:435-460` | StopHook | turn 循环结束前，evaluate_halt_signals |
| `dialogue.rs:451` | StopHook::Block | 阻塞时继续循环 |
| `dialogue.rs:458` | StopHook::Prevent | 阻止时终止会话 |
| `compact.rs` (全部) | 无 hook 调用 | compact 流程未接 pre/post compact hook |
| `main.rs` (全部) | 无 hook 调用 | 启动路径未接 SessionStartHook |
| `repl.rs` (全部) | 无 hook 调用 | REPL 启动未接 SessionStartHook |

## 缺口（hook 已定义但未在主路径调度）
- [ ] PostSampling: 定义在 `hooks/post_sampling.rs:38`，PostSamplingHookRegistry 存在，但 `dialogue.rs` 未在 API 响应后调用
- [ ] PreCompact: HookEvent 定义在 `mossen-types/src/hooks.rs:16`，但 `services/compact/` 未在 compact 前调用
- [ ] PostCompact: HookEvent 定义在 `mossen-types/src/hooks.rs:21`，但 `services/compact/` 未在 compact 后调用
- [ ] SessionStart: HookEvent 定义在 `mossen-types/src/hooks.rs:24`，但 `main.rs`/`repl.rs` 启动路径未调用
- [ ] ExecCommand/ExecAgent: 执行器模块存在 (`hooks/exec_command.rs`, `hooks/exec_agent.rs`)，但 `dialogue.rs` 未调度
- [ ] FileChanged/CwdChanged: watcher 模块存在但未接入主渲染循环

## Phase 1 后续 task 的 wire 目标
- 1-2: wire PostSamplingHook 到 dialogue.rs — API 流式接收结束后、tool 调度前
- 1-3: wire PreCompactHook / PostCompactHook 到 services/compact/compact.rs — compact 入口和返回前
- 1-4: wire SessionStartHook 到 main.rs / repl.rs — async main 入口和 REPL 启动前
- 1-5: 子 agent transcript 隔离硬保证（另见 ISOLATION_AUDIT.md）
