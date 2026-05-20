# Sub-Agent Isolation Audit

> 审计时间：Phase 1-5
> 依据 PLAN_PRODUCTION.md §1-5 的三项不变量

## 不变量 1：transcript 隔离

**状态：满足**

子 agent 使用独立 transcript 文件路径，不会污染父 transcript。

证据：
- `crates/mossen-tools/src/agent_tool/resume_agent.rs:65`：
  `let transcript_path = session_dir.join("transcript.jsonl")` — 其中 `session_dir` 通过 `get_agent_session_dir(agent_id)` 构造，位于 `~/.mossen/sessions/{parent_session_id}/agents/{agent_id}/`
- `crates/mossen-agent/src/transcript.rs:63-67`：`TranscriptManager` 使用 `storage_dir.join(&session_id).with_extension("json")` 写入独立文件
- `crates/mossen-agent/src/services/session_transcript/mod.rs:20-29`：全局 daily transcript 按日期分桶，子 agent 消息通过独立 `agent_id` 标记可区分

结论：子 agent 的 transcript 路径天然隔离，不会进入父 transcript 文件。

## 不变量 2：AgentContext 传播

**状态：满足**

子 agent 内部的工具调用可以拿到自己的 agentId。

证据：
- `crates/mossen-tools/src/agent_tool/utils.rs:224`：`ProgressTracker` 持有 `agent_id: String`
- `crates/mossen-tools/src/agent_tool/run_agent.rs:34`：`RunAgentOptions.override_agent_id: Option<String>` — 允许父 agent 为子 agent 指定独立 ID
- `crates/mossen-agent/src/tool_registry.rs:389`：工具注册表支持 `agent_id: Option<String>` 上下文
- `crates/mossen-agent/src/api/prompt_cache.rs:89`：`prompt_cache` 跟踪按 `agent_id` 隔离

结论：子 agent 的工具调用上下文能够传播正确的 agentId。

## 不变量 3：task-notification 回灌

**状态：满足**

子 agent 的结果以 `<task-notification>` XML 块回灌到父 transcript。

证据：
- `crates/mossen-cli/src/tasks.rs:1023`：构造 `<task_notification>` 回灌格式
- `crates/mossen-utils/src/sdk_event_queue.rs:140-151`：`emit_task_notification_sdk_event` 发出 SDK 事件
- `crates/mossen-utils/src/session_storage.rs:1406`：`record_transcript` 持久化 transcript
- `crates/mossen-tools/src/agent_tool/fork_subagent.rs:48`：明确注释 `<task-notification>` interaction model
- `crates/mossen-utils/src/collapse_background_bash_notifications.rs:8`：`const TASK_NOTIFICATION_TAG: &str = "task-notification"`
- `crates/mossen-types/src/constants/xml.rs:32`：`pub const TASK_NOTIFICATION_TAG: &str = "task-notification"`

结论：子 agent 完成时通过 `<task-notification>` XML 块回灌结果到父 transcript。

## 修补 task 提议

无需 fix。三项不变量均满足。

- 不变量 1：子 agent transcript 路径已隔离（`agents/{agent_id}/`）
- 不变量 2：AgentContext 通过 `override_agent_id` 传播
- 不变量 3：task-notification 回灌基础设施完整
