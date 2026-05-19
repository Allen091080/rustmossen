# W20-A — Stream-JSON Protocol Extension 施工包

**Mode**: 审计 / read-only construction package — 本文件仅是计划，未写代码、未改 schema、未动 emit 路径、未碰 harness。
**Date**: 2026-05-01
**Scope**: 设计如何扩展 mossen CLI 的 stream-json STDOUT 协议，让 mossen-workbench 能渲染真实 tool_result / permission / subagent / TodoWrite / compact / token streaming 信号，而不是当前的占位／推断。
**Inputs**: 4 个并行 SA (SA-1 schema · SA-2 event sources · SA-3 CLI/TUI compat · SA-4 Workbench needs) + W19 prior gap doc。
**Output sibling**: `mossen-workbench/docs/W19_PROTOCOL_GAP.md` (已存在，本文承接其 §5 priority list)。

---

## 0. TL;DR — 一页核心结论

1. **协议 schema 已经包含大部分所需事件类型**（SA-1 + SA-2 复核）。`SDKMessage` union 24 成员中已含 `SDKToolProgressMessage` / `SDKTaskStartedMessage` / `SDKTaskProgressMessage` / `SDKCompactBoundaryMessage` / `SDKPartialAssistantMessage` / `SDKLocalCommandOutputMessage` / `SDKHookStartedMessage`；permission 走 `SDKControlRequestSchema`（已在 StdoutMessage union 内）。
2. **真正缺失的工作分三类**：
   - **A. Workbench 消费缺失**（70% 的 gap）：协议已定义、CLI 已 emit，但 `mossen-workbench/src/App.tsx:884-932` 只 route `assistant` + `result`，其它类型只进 `runtimeRows`。→ 不需要动 mossen 协议，改 Workbench 即可。
   - **B. CLI emit 路径缺失**（20% 的 gap）：协议 schema 存在，但 `cli/structuredIO.ts` 没有产出。典型：`SDKToolProgressMessage` 定义在 `coreSchemas.ts:1659` 但 streaming tool executor 没有调用 `structuredIO.write()` 把进度写出去。→ 需要主仓改动。
   - **C. 真正的协议新增**（10% 的 gap）：tool_use ↔ tool_result 关联（`tool_use_id` 字段），permission decision 回写通道，结构化 `todos_updated` 事件。→ 需要主仓改动 + only-additive schema 扩展。
3. **TUI／CLI 不会被砸**（SA-3 复核）：TUI 用 React 的 in-process state，不消费 stdout；REPL 的 `switch(event.type)` 没有 `default: throw`，未识别 type 静默忽略；print mode 是纯 producer。→ 新增 type 字段、加 union member 都安全。
4. **推荐第一个实施 slice 是 P0-1: tool_use_id 关联 + tool_result mirror**。Workbench 体验最大、CLI 改动最小、只动 Workbench 路由 + `cli/structuredIO.ts` 一个写入点。

---

## 1. 当前 stream-json STDOUT event inventory

权威源：`scripts/stream-json-schema-whitelist.txt` + `entrypoints/sdk/{coreSchemas,controlSchemas}.ts` + `docs/reference/protocol-contract.md`。

### 1.1 Stdout union 8 成员（`StdoutMessageSchema`）

| # | Schema | Kind | Workbench 当前消费？ | 备注 |
|---|--------|------|----------------------|------|
| 1 | `SDKMessageSchema` (24-member 内 union) | 主流 | ✓ assistant / result；× 其余 21 类 | App.tsx:884-932 只 route 这 2 个 |
| 2 | `SDKStreamlinedTextMessageSchema` | streamlined text | × | runtimeRows only |
| 3 | `SDKStreamlinedToolUseSummaryMessageSchema` | streamlined tool summary | × | runtimeRows only |
| 4 | `SDKPostTurnSummaryMessageSchema` | post-turn summary | × | runtimeRows only |
| 5 | `SDKControlResponseSchema` | control resp | × (control loop owns) | |
| 6 | `SDKControlRequestSchema` | control req（permission 等） | × | **关键：permission 已经在 stdout，Workbench 没消费** |
| 7 | `SDKControlCancelRequestSchema` | cancel | × | |
| 8 | `SDKKeepAliveMessageSchema` | keep_alive | × (心跳) | |

### 1.2 SDKMessage 内 24 成员现状（与 Workbench 需求映射）

| Schema (file:line) | Workbench 需求 | 当前 emit | Workbench 当前接 |
|--------------------|----------------|-----------|------------------|
| `SDKAssistantMessage` | text/tool_use 块 | ✓ | ✓ extractAssistantBlocks |
| `SDKUserMessage` | echo/replay | ✓ | × (本地已有) |
| `SDKResultMessage` | result block | ✓ | ✓ buildResultBlock |
| `SDKSystemMessage` (init) | session 元数据 | ✓ | runtimeRows only |
| `SDKPartialAssistantMessage` (coreSchemas:1507) | token streaming | ✓ (raw stream_event) | × |
| `SDKCompactBoundaryMessage` (coreSchemas:1517) | compact summary | ✓ (boundary only) | × |
| `SDKStatusMessage` | "compacting/working" | 部分 | × |
| `SDKAPIRetryMessage` | retry 提示 | ✓ | × |
| `SDKLocalCommandOutputMessage` (coreSchemas:1601) | tool output mirror | 待复核 | × |
| `SDKHookStartedMessage` (coreSchemas:1615) | hook 生命周期 | ✓ | × |
| `SDKHookProgressMessage` | hook 进度 | ✓ | × |
| `SDKHookResponseMessage` | hook 结束 | ✓ | × |
| `SDKToolProgressMessage` (coreSchemas:1659) | tool 长任务进度 | **× schema-only**（SA-2 GREEN→YELLOW） | × |
| `SDKAuthStatusMessage` (coreSchemas:1672) | 鉴权 | ✓ | × |
| `SDKTaskNotificationMessage` | subagent 通知 | ✓ | × |
| `SDKTaskStartedMessage` (coreSchemas:1726) | subagent 起 | ✓ | × |
| `SDKTaskProgressMessage` (coreSchemas:1761) | subagent 进 | ✓ | × |
| `SDKSessionStateChangedMessage` | session 状态 | ✓ | × |
| `SDKFilesPersistedEvent` | 持久化 | ✓ | × |
| `SDKToolUseSummaryMessage` (coreSchemas:1780) | tool 总结 | ✓ | × |
| `SDKRateLimitEvent` | 限流 | ✓ | × |
| `SDKElicitationCompleteMessage` (coreSchemas:1790) | 输入收集完 | ✓ | × |
| `SDKPromptSuggestionMessage` | 建议 | ✓ | × |
| `SDKUserMessageReplay` | 历史回放 | ✓ | × |

> **结论**：Workbench 当前只消费 24 类中的 3 类（assistant/result/system-init 部分）。21 类协议已经发出但被丢进 runtimeRows，是体验大头。

---

## 2. 缺失事件清单（按 Workbench 需求拆分）

每条注明：(a) 协议是否已支持 (b) emit 是否就绪 (c) Workbench 是否已消费 (d) 缺口归类。

### 2.1 tool_use ↔ tool_result 关联（**P0**）
- 协议：tool_use 已通过 `SDKAssistantMessage` 出，tool_result 走 `mutableMessages` 内存，**没有作为 stdout 顶层事件**。
- emit：缺失。`services/tools/StreamingToolExecutor.ts:108-150` 把结果写进 `TrackedTool.results[]`，再走下一轮 user message，但没有以 `tool_result` 为 type 的 stdout 行。
- Workbench：当前 `ToolBlock.status` 永远停在 `'running'`（`mossen-workbench/src/template-shell/conversation/eventBlocks.ts` parser 没法看到完成）。
- 缺口归类：**C. 真正新增**（需要协议层加 `tool_result` 事件 + `tool_use_id` 关联字段）。

### 2.2 permission_request mirror（**P0**）
- 协议：已在 `SDKControlRequestSchema` 内（StdoutMessage union 第 6 个），permission 是 `SDKControlPermissionRequest` 子类型。
- emit：✓ 已在 stdout（control 协议同时双向走 stdin/stdout）。
- Workbench：× 完全没接 `control_request` 这条 union 分支。
- 缺口归类：**A. Workbench 消费缺失** + 一点 **C**（decision 回写通道需要 design）。

### 2.3 permission_decision 回写（**P0**）
- 协议：×。当前 decision 由 control_response 走 stdin 回 CLI；Workbench 自己点 Approve/Deny 后还没有标准的回写通道。
- emit：N/A。
- Workbench：UI 占位有（`ApprovalBlockView`），按钮无 wiring。
- 缺口归类：**C. 真正新增**（需要 design：是 stdin control_response 还是新增 tool？）。

### 2.4 subagent 生命周期（**P1**）
- 协议：✓ `SDKTaskStartedMessage` / `SDKTaskProgressMessage` / `SDKTaskNotificationMessage` 已有。
- emit：✓ 已 emit（`tools/AgentTool/runAgent.ts` → SDK message stream）。
- Workbench：× App.tsx 没 route。
- 缺口归类：**A. Workbench 消费缺失**（零 CLI 改动）。

### 2.5 TodoWrite 结构化事件（**P1**）
- 协议：×。当前 Workbench 通过 inline `tool_use { name: 'TodoWrite' }` 推断（`eventBlocks.ts:looksLikeTodoTool`）。
- emit：内部状态 `context.appState.todos[agentId]`（`TodoWriteTool.ts:88-94`）是更干净的源，但没有 stdout 事件。
- Workbench：✓ 推断版本可用，结构化事件会更稳。
- 缺口归类：**C. 真正新增**（可选；优化项不是 blocker）。

### 2.6 compact 摘要（**P1**）
- 协议：✓ `SDKCompactBoundaryMessage`（coreSchemas:1517）。
- emit：✓ 边界 emit 已有；缺 "compaction completed + delta tokens" 事件（SA-2 注：`utils/messages.js:createCompactBoundaryMessage`）。
- Workbench：× App.tsx 没 route。
- 缺口归类：**A 主要 + 一点 B**（如果想要 delta tokens 需要 emit 增强）。

### 2.7 tool_progress（**P2**）
- 协议：✓ `SDKToolProgressMessage`（coreSchemas:1659）。
- emit：**× schema-only**。SA-2 标 YELLOW：streaming tool 已经在内部 call `renderToolUseProgressMessage()` 但没有走 `structuredIO.write()`。
- Workbench：× 没消费。
- 缺口归类：**B. CLI emit 路径缺失**（hot-path RED：可能 10-100 events/tool；要加节流）。

### 2.8 token_delta / 流式 token（**P2**）
- 协议：✓ `SDKPartialAssistantMessage`（coreSchemas:1507，包 raw `RawMessageStreamEvent`）。
- emit：✓ 已 emit（`QueryEngine.ts:288-300` 处理 `message_delta`）。
- Workbench：× 没消费（没必要 — `ResultBlock` 已经有总数；流式 token 是 4K output → 400-4000 events/turn，UI 抖动）。
- 缺口归类：**A**（消费）+ 性能护栏。

### 2.9 error / structured stderr（**P2**）
- 协议：×。错误现在通过 stderr 文本 + `SDKStatusMessage(status='error')`（其实 status union 里没 'error'）。
- emit：缺。
- Workbench：靠 `adapter.onError` 抓 stderr，但没有结构化的 type/code/scrubbed message。
- 缺口归类：**C. 真正新增**（可选；当前已有 stderr fallback）。

---

## 3. 每个事件的建议 schema（only-additive）

### 红线
> **不删字段、不改字段语义、不重命名**。所有新字段必须 `.optional()`；所有新 union member 必须只追加。修改 `scripts/stream-json-schema-whitelist.txt` 时只追加，不删。

### 3.1 P0-1 `tool_result` event

```ts
// 新 SDKMessage union member（追加到 coreSchemas.ts SDKMessage union 末尾）
const SDKToolResultMessageSchema = z.object({
  type: z.literal('tool_result'),
  tool_use_id: z.string(),                   // 关联 tool_use.id
  status: z.enum(['complete', 'failed', 'cancelled']),
  summary: z.string().max(512).optional(),   // 已截断的人读摘要
  error_kind: z.string().optional(),         // 仅 status='failed' 时
  uuid: z.string().optional(),
  session_id: z.string().optional(),
  parent_tool_use_id: z.string().nullable().optional(),
})
```
**字段策略**：`summary` 服务端截断 ≤512（避免重放整段 tool 输出）；`status='failed'` 时附 `error_kind`；不携带 raw output（Workbench 当前不渲染，详细数据走 inspector raw 通道）。

### 3.2 P0-2 `tool_use_id` 字段补齐

`tool_use` 已有 `id`，但当前流出的 `SDKAssistantMessage` 内嵌 anthropic content block 已有 `id`。Workbench 需要确保 `eventBlocks.ts` 抓到 `id` 写入 `ToolBlock.toolUseId`（已有字段）。无 schema 改动。

### 3.3 P0-3 permission stdout 路由 + decision 通道

- **request**: 已在 stdout（`SDKControlRequestSchema` + `SDKControlPermissionRequest`），无 schema 改动；Workbench 路由 `control_request.subtype === 'permission'`。
- **decision**: 沿用现有 control 通道，Workbench 通过 stdin 写 `control_response { request_id, response: {behavior: 'allow'|'deny'} }`。
  - 协议层无需新增类型（已在 `SDKControlResponseSchema`）。
  - 需要 design：Workbench 是否信任本地 user click → 直接写 stdin，还是先经过 confirmation modal？建议本 slice 只 wire request 路由，decision channel 单独 design。

### 3.4 P1 `compact_summary` 路由

无新 schema；Workbench 路由 `SDKCompactBoundaryMessage` 渲染为 `StatusBlock(level='info', text=summary)`。可选增强：emit 端补充 `pre_tokens` / `post_tokens` / `kept_segment_count`（all `.optional()`）。

### 3.5 P1 subagent lifecycle

无新 schema；Workbench 路由 `SDKTaskStartedMessage` → 创建 `AgentBlock(status='running')`，`SDKTaskProgressMessage` → 更新 `summary`，`SDKTaskNotificationMessage` 含 `status: 'complete'|'failed'` 转化为 `AgentBlock.status`。

如果 `SDKTaskNotificationMessage` 现 schema 没有 final-status 字段，需要补：

```ts
// 仅 only-additive 增量
SDKTaskNotificationMessageSchema.extend({
  result_status: z.enum(['complete', 'failed']).optional(),
  result_summary: z.string().max(512).optional(),
})
```

### 3.6 P1 `todos_updated`（可选）

```ts
const SDKTodosUpdatedMessageSchema = z.object({
  type: z.literal('todos_updated'),
  agent_id: z.string(),
  items: z.array(z.object({
    id: z.string(),
    content: z.string(),
    status: z.enum(['pending', 'in_progress', 'completed']),
    activeForm: z.string().optional(),
  })),
  uuid: z.string().optional(),
  session_id: z.string().optional(),
})
```
仅在 `TodoWriteTool` resolve 后 emit；emit 点 `tools/TodoWriteTool/TodoWriteTool.ts:88-94`。

### 3.7 P2 `tool_progress` emit wiring

无 schema 改（已存在 `SDKToolProgressMessage`）。需要：
- `services/tools/StreamingToolExecutor.ts` 在 `pendingProgress` push 时 mirror 到 `structuredIO.write({type: 'tool_progress', tool_use_id, message, progress?})`；
- 节流：每 tool ≤2 events/sec，`message` 截断 ≤256 字符；
- 默认 OFF，靠 env flag `MOSSEN_STREAM_TOOL_PROGRESS=1` 或 `--stream-tool-progress` 解锁。

### 3.8 P2 `token_delta` 暂不新增

`SDKPartialAssistantMessage` 已经包含原始 `message_delta.usage`。Workbench 如果要消费，建议：
- 客户端 buffer 100ms / 100 tokens；
- 不渲染单 token，渲染累计 input/output tokens 的快照；
- 不需要协议改动。

---

## 4. 向后兼容策略

1. **only-additive 严格执行**：whitelist 文件只追加；现有字段不动；现有 union member 不动。
2. **新 type 字符串静默吸收**：
   - SA-3 已确认：mossen REPL `screens/REPL.tsx:2438-2452` 的 `switch(event.type)` 没有 `default: throw`，未知 type 静默忽略。print mode 是纯 producer 不消费。
   - 旧版 Workbench / 第三方 SDK consumer：建议在文档明示 "未识别 type 应忽略，不应 throw"；Zod 这边 `SDKMessage union.parse()` 默认对未识别 type 会 fail，需要 union 加新 member 后 schema 同步。
3. **schema whitelist gate**：`scripts/stream_json_contract_smoke.py` 校验 whitelist；每加新事件先改 whitelist + smoke，再加 emit / parser。
4. **feature flag**：高 hot-path 事件（tool_progress, token_delta）默认 OFF，env 解锁。
5. **anchor 不动**：whitelist Section E 列出的协议锚点字面量保持不变。

---

## 5. 安全策略（敏感内容 / 注入）

| 风险 | 来源 | 对策 |
|------|------|------|
| Tool input 含 API key / env / PII | `permission_request.input`（SA-2 RED）、`tool_progress.message`（SA-2 YELLOW） | 服务端截断 + 字段白名单；`summary` 字段值由 emit 端去敏感处理（重用 `cli/structuredIO.ts:93-117 buildRequiresActionDetails` 的现有 redaction） |
| Tool 输出含原始 API/文件内容 | `tool_result.summary`（≤512 char）、`SDKLocalCommandOutputMessage` | summary 短摘要；raw 走 inspector 通道不入 stdout |
| Streaming 错误 leak stack trace / endpoint | `SDKAPIRetryMessage`、未来 error event | error_kind 走枚举；message 截断；URL 去 query params |
| stderr 串入 stdout | 已有 guard `utils/streamJsonStdoutGuard.ts:75-76` | 保持，新事件不绕开 guard |
| 注入 / 伪事件（Workbench 端） | parse 完全信任 stdin | Zod 严格 parse；未通过 schema 的行 → adapter.onError("parse-failed") 而不是 swallow |
| Permission decision 注入 | Workbench 写 stdin 可能伪造 request_id 同意非自己提示的请求 | CLI 端校验 request_id 必须在 pending set；超时清理 |

---

## 6. 性能策略

| Event | 预估频率 | 风险 | 护栏 |
|-------|----------|------|------|
| tool_result | 1 / tool 调用 | 低 | summary ≤512 |
| permission_request | 1 / 用户交互 | 低 | 无特殊 |
| compact_summary | 1 / N 轮 | 低 | summary ≤1KB |
| subagent lifecycle | 2-5 / subagent | 低 | 无特殊 |
| todos_updated | 1 / TodoWrite call | 低 | 整 list 替换；items[] 长度 cap 100 |
| tool_progress | **10-100 / long tool**（SA-2 RED） | 高 | feature flag OFF；emit 端 ≤2 events/sec/tool；message ≤256；默认不 emit |
| token_delta | **400-4000 / turn**（SA-2 RED） | 高 | 不新增协议事件；Workbench 如果消费要 buffer 100ms / 100 tokens |

`structuredIO.write` 是同步 stdout flush，volume 大时会阻塞 agent loop。tool_progress / token_delta 任何方案都必须 benchmark：5 分钟 long tool + 1k assistant 输出 < 10MB stdout。

---

## 7. Slice 拆分（P0 / P1 / P2 + 第一个推荐）

### P0（≤2 周内完成，体验回报最大）

#### **P0-1: tool_use_id 关联 + tool_result 事件**（推荐第一个 slice）
- 主仓改动：
  1. whitelist 加 `SDKToolResultMessage`；
  2. `coreSchemas.ts` 加 schema + 进 SDKMessage union；
  3. `cli/structuredIO.ts` 在 tool 完成后 emit（hook 点：`services/tools/StreamingToolExecutor.ts` `getRemainingResults()` 内或其调用方）；
  4. `scripts/stream_json_contract_smoke.py` 同步白名单 + 增加正例 fixture。
- Workbench 改动：
  1. `App.tsx` adapter.onEvent 增 `ev.type === 'tool_result'` 分支；
  2. 通过 `tool_use_id` 找到匹配 `ToolBlock`，patch `status` + `summary`；
  3. `eventBlocks.ts` 不变（已有 `toolUseId` 字段）。
- 工作量：主仓 ~150 行 + Workbench ~50 行。

#### **P0-2: permission_request 路由（不含 decision channel）**
- 主仓：无改动（已在 stdout）。
- Workbench：`App.tsx` 加 `ev.type === 'control_request'` + `subtype === 'permission'` 分支；构造 `ApprovalBlock(decision='pending', toolName, summary)`；UI 按钮暂时只是 disabled 占位。
- 工作量：Workbench ~80 行。

#### **P0-3: compact_summary 路由**
- 主仓：无改动。
- Workbench：`App.tsx` 加 `ev.type === 'compact_boundary'` 分支；构造 `StatusBlock(level='info', text=summary)` 追加到当前消息。
- 工作量：Workbench ~30 行。

### P1（1 个月内）

- **P1-A: subagent lifecycle 路由**：Workbench-only，0 主仓改动；route `task_started` / `task_progress` / `task_notification` → AgentBlock。
- **P1-B: TodoWrite 结构化 emit**（可选）：主仓加 `SDKTodosUpdatedMessage`；Workbench parser 优先吃结构化事件，fallback 旧推断；保留 `looksLikeTodoTool` 不删。
- **P1-C: permission decision channel 设计 + wiring**：design doc 单独写；落地后 Workbench 按钮可用。

### P2（看必要性）

- **P2-A: tool_progress emit + 节流**：feature flag 后开启；先内部用 dogfooding。
- **P2-B: token_delta 客户端 buffer**：Workbench-only；不动协议。
- **P2-C: 结构化 error event**：先确定 error 字段集合再说；当前 stderr fallback 够用。

---

## 8. 每个 slice 的验证矩阵

通用要求（每个 slice 必须满足才算完）：

| 维度 | 要求 |
|------|------|
| schema | whitelist 同步，`stream_json_contract_smoke.py` 通过 |
| Zod | `coreSchemas.test.ts` 加正例 + 反例 |
| emit | 真链路 e2e（harness）：发触发请求 → 抓 stdout → 验事件存在且字段正确 |
| Workbench | 渲染 smoke：mock 事件 → 渲染 → 断言 DOM 含期望 testid + 文案 |
| TUI 不回归 | mossen REPL 跑 LLM 真链路 smoke，确认无 throw / 无新报错 |
| 性能 | tool_progress / token_delta 必须 5 分钟 stress benchmark < 10MB 输出 |
| 安全 | redaction 单测：放 fake API key 进 input → 验 stdout 不含 |

### Slice-specific

#### P0-1
- **schema**: whitelist diff 只 +1 行 `SDKToolResultMessageSchema`；smoke pass。
- **emit**: harness 跑 "Bash echo hi" → 抓 stdout 必含 `{type:'tool_result', tool_use_id, status:'complete', summary:'hi'}`。
- **Workbench**: 单测注入 tool_use 然后 tool_result → ToolBlock.status 由 `running` 转 `complete`，summary 显示。
- **失败回滚**: revert whitelist + schema + emit，Workbench 分支保持但 dormant；不破坏 main。

#### P0-2 / P0-3
- **Workbench-only smoke**: mock SDK event → Workbench DOM 出现 ApprovalBlock / StatusBlock 对应 testid。
- **CLI smoke**: 跑 permission 触发场景 → stdout 必含 control_request；当前 mossen 行为不变。

#### P1-B
- **回退兼容**: Workbench 老 build 收到 `todos_updated` event，Zod 不识别 → 降级为 unknown，不 throw。

---

## 9. STOP 条件（任何一条触发即暂停 / 转 design 复盘）

1. **协议红线被破坏**：发现需要删字段 / 改字段语义 / rename type。任何 only-additive 都做不到的需求 → 停，回到 design。
2. **TUI 出现 hard-fail**：加新事件后 mossen REPL 真链路 smoke throw 或退出。SA-3 说 forgiving，但需要实测验证；如果错了，停。
3. **Schema whitelist smoke fail 且无法修**：whitelist 同步逻辑错位，3 次 commit 内修不好就停。
4. **stream-json 性能 regress > 10%**：5 分钟 long tool benchmark stdout 大小或 turn latency 涨 > 10% → 停，加节流前不上 main。
5. **redaction 漏 leak**：fake API key 测试通过任一 leak 路径 → 停。
6. **decision channel design 不收敛**：P0-2 落地后 Allen 不批 P1-C 的 channel 方案 → permission button 永久 disabled，Workbench 文案要 "需 mossen CLI 端审批"，不强行上线半成品。
7. **subagent lifecycle 字段在 mossen 实际 emit 中和 schema 不一致**：实测 stdout 字段缺失或多余，先回去查 SDK 定义对齐。
8. **harness 真链路 smoke 跑 > 10 分钟**：性能分数下滑明显 → 停，性能审计。
9. **同时改主仓 + Workbench 两边的 commit 比例失调**：单 slice 主仓 > 500 行 → 拆 slice，不允许大爆炸。
10. **Allen 复审任何 slice 喊停** — 默认无条件停。

---

## 10. 推荐路线（执行顺序）

1. **Slice 0（本 W20-A 之后立刻做）**: 把本文 + W19_PROTOCOL_GAP.md 给 Allen 评审。决定要不要走 only-additive 协议路线 vs. 用 fixture-only 让 Workbench 自演。
2. **Slice P0-1: tool_result + tool_use_id** —— 体验回报最大，主仓 + Workbench 都改但每边都小。先做这个验证整套节奏（schema → smoke → emit → Workbench route → e2e）。
3. **Slice P0-3: compact_summary 路由** —— 0 主仓改动；Workbench 端 30 行；快速验证 Workbench-only slice 节奏。
4. **Slice P1-A: subagent lifecycle 路由** —— 0 主仓改动；和 P0-3 同节奏，Workbench 三个事件分支。
5. **Slice P0-2: permission_request 路由（占位）** —— Workbench-only；UI 占位先看效果。
6. **Slice P1-C: permission decision channel design** —— 单独 design doc 落 Allen 拍板。
7. **Slice P1-B: TodoWrite 结构化 emit**（可选）—— 看 P0-1 后字段抽象是否值得复用。
8. **Slice P2-***（看 dogfooding 反馈再排）。

---

## 附录 A — 4 个 SA 核心结论摘要

- **SA-1（schema 现状清点）**：whitelist 24-member SDKMessage union + 8-member StdoutMessage union 已经覆盖大多数所需 type；新增需走 only-additive；anchor 不动。（agent 中途被 35-tool budget 截断；后续靠主仓 grep 补上 schema 行号定位。）
- **SA-2（agent loop event sources）**：8 信号逐项 source map 完整；GREEN（subagent / tool_result / TodoWrite / compact 路由 / token_delta 已 emit）；YELLOW（permission mirror / compact 完成 / tool_progress 路径缺 wiring）；RED（permission input 含敏感字段；tool_progress / token_delta 是 hot path）。
- **SA-3（CLI/TUI 兼容）**：TUI 用 in-process React state 不消费 stdout；REPL switch 静默吸收未识别 type；print mode 是纯 producer。**新增 type 不会砸 TUI**。
- **SA-4（Workbench needs）**：每信号最小字段集已枚举；`ConversationBlock` discriminated union 现有 6 类（ToolBlock / TodoBlock / ApprovalBlock / AgentBlock / ResultBlock / StatusBlock）**全部够用，不需要新增 block kind**；缺的是 App.tsx route 和少量 schema 字段（tool_use_id / status / summary）。

---

## 附录 B — 关键文件 / 行号速查

- whitelist：`scripts/stream-json-schema-whitelist.txt`
- 协议契约权威 doc：`docs/reference/protocol-contract.md`
- Zod schemas：`entrypoints/sdk/coreSchemas.ts:1507`（PartialAssistant）/ `:1517`（CompactBoundary）/ `:1601`（LocalCommandOutput）/ `:1659`（ToolProgress）/ `:1726`（TaskStarted）/ `:1761`（TaskProgress）/ `:1780`（ToolUseSummary）/ `:1865-1888`（SDKMessage union）
- Control schemas：`entrypoints/sdk/controlSchemas.ts`（permission request / response）
- Stdout emitter：`cli/structuredIO.ts:135 class StructuredIO`
- Tool exec：`services/tools/StreamingToolExecutor.ts:108-150`
- TodoWrite：`tools/TodoWriteTool/TodoWriteTool.ts:31-115`
- Subagent：`tools/AgentTool/runAgent.ts`
- Compact：`services/compact/compact.ts:389`
- Print mode：`cli/print.ts`
- Stream guard：`utils/streamJsonStdoutGuard.ts:75-76`
- TUI render（确认不消费 stdout）：`screens/REPL.tsx:1174-1190`，`state/AppState.tsx:36-50`
- Workbench adapter event dispatch：`mossen-workbench/src/App.tsx:884-932`
- Workbench block parser：`mossen-workbench/src/template-shell/conversation/eventBlocks.ts`
- Workbench block model：`mossen-workbench/src/template-shell/runtimeIdentity.ts`（ConversationBlock union）
- Prior gap audit：`mossen-workbench/docs/W19_PROTOCOL_GAP.md`

---

**END W20-A — 不写代码，等 Allen 复审拍板第一个 slice。**
