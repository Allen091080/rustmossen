# Mossen Stream-JSON Compact Queued Protocol (W48)

> **目的**: 将 `compact_conversation` control_request 从永远 blocked 升级为 queued 协议。Workbench 发送 compact 请求后，handler 入队并返回 `status:"queued"`；真实 compact 在 query loop 安全点执行（下一阶段）。
> **范围**: 本轮只实现入队协议 + event schema。不改 query loop、不调用 compactConversation、不构造 ToolUseContext、不构造 CacheSafeParams。
> **状态**: handler 已支持 queued/blocked 返回。compact 仍不执行。query loop safe point 是下一阶段。
> **配套**:
> - `entrypoints/sdk/controlSchemas.ts` — request schema 加 `custom_instructions`；response schema 扩展 `queued`/`blocked`/`completed`/`failed`
> - `services/compact/pendingCompactRequest.ts` — single-slot pending buffer
> - `entrypoints/sdk/coreSchemas.ts` — `SDKCompactCompletedEventSchema`（system event）
> - `cli/print.ts` — compact_conversation 分支 validate + enqueue
> - `scripts/wave_w48_compact_queued_protocol_smoke.py` — 契约 lock
> - `scripts/stream-json-schema-whitelist.txt` — SDKMessage union 27→28

---

## 1. 协议变更

### 1.1 Request schema 扩展

```json
{
  "subtype": "compact_conversation",
  "mode": "manual",
  "dry_run": false,
  "custom_instructions": "Focus on the API design discussion"
}
```

新增字段：
- `custom_instructions` (optional string): 用户自定义 compact 指令，传递到 `compactConversation` 的 `customInstructions` 参数。

### 1.2 Response schema 扩展

status 从 `blocked | completed` 扩展为 `queued | blocked | completed | failed`。

| status | 含义 | 本轮是否返回 |
|---|---|---|
| `queued` | 请求已入队，将在 query loop 安全点执行 | **是** |
| `blocked` | 请求被拒绝（已有 pending / 不支持的 mode / dry_run） | **是** |
| `completed` | compact 执行成功（query loop 发出） | 否（下一阶段） |
| `failed` | compact 执行失败（query loop 发出） | 否（下一阶段） |

### 1.3 compact_completed event schema

新增 SDK system event `compact_completed`，由 query loop safe point 在 compact 执行完毕后发出。

```json
{
  "type": "system",
  "subtype": "compact_completed",
  "request_id": "<original control_request request_id>",
  "compact_result": {
    "status": "completed",
    "preCompactTokenCount": 85000,
    "postCompactTokenCount": 12000,
    "messageCountBefore": 42,
    "messageCountAfter": 8
  },
  "uuid": "...",
  "session_id": "..."
}
```

或失败：
```json
{
  "compact_result": {
    "status": "failed",
    "reason": "Conversation too long..."
  }
}
```

**本轮只锁 schema，不发事件。**

---

## 2. Handler 行为

### 2.1 验证 + 入队流程

```
收到 compact_conversation request
  │
  ├─ mode 不是 "manual" 或 omit → blocked: unsupported mode
  ├─ dry_run === true           → blocked: dry_run not supported yet
  ├─ 已有 pending request        → blocked: another compact request is already pending
  └─ 通过验证                     → enqueue + queued
```

### 2.2 Handler 不做的事

- 不调用 `compactConversation`
- 不 import `compactConversation`
- 不构造 `ToolUseContext`
- 不构造 `CacheSafeParams`
- 不执行任何 compact 逻辑

---

## 3. Pending Buffer

### 3.1 Single-slot 设计

`pendingCompactRequest.ts` 提供单 slot buffer：
- `enqueuePendingCompactRequest(req)` — 成功返回 `{ok: true}`，已有 pending 返回 `{ok: false, reason}`
- `dequeuePendingCompactRequest()` — 取出并清空（query loop 调用）
- `getPendingCompactRequest()` — peek（不取出）
- `hasPendingCompactRequest()` — 布尔检查
- `clearPendingCompactRequest()` — 无条件清空
- `hasCompactRequestTimedOut()` — 超时检查（60s）

### 3.2 数据结构

```typescript
type PendingCompactRequest = {
  requestId: string
  mode: 'manual'
  dryRun: boolean
  customInstructions?: string
  enqueuedAt: number
}
```

---

## 4. 下一阶段（本轮不做）

- **Query loop safe point**: query.ts 中在 `deps.autocompact` 之后加 `dequeuePendingCompactRequest` 检查
- **执行 compact**: 使用 query loop 的真实 ToolUseContext + CacheSafeParams
- **发 compact_completed event**: compact 完成后通过 structuredIO 发 system event
- **超时清理**: pending request 超过 60s 未执行时发 failed event

---

## 5. Workbench 接入指引

### 5.1 本轮可接

Workbench 可以先接 `queued` / `blocked` UI：
- `queued`: 显示 "Compaction queued — will run on next message."
- `blocked`: 显示 reason

### 5.2 下一阶段后可接

- 监听 `compact_completed` event
- `completed`: 显示 "Compacted. 85K → 12K tokens."
- `failed`: 显示 error reason

### 5.3 注意

- compact 不是立即执行的。从 `queued` 到 `compact_completed` 有延迟。
- 本轮 `queued` 后 compact 不会执行（query loop safe point 未接）。
- Workbench 看到 `queued` 后只需等待，不需要额外操作。

---

## 6. 验证清单

```bash
python3 scripts/wave_w48_compact_queued_protocol_smoke.py
python3 scripts/stream_json_contract_smoke.py
python3 scripts/wave_w46_high_value_protocols_smoke.py
python3 scripts/wave_w47_real_capability_operations_smoke.py
bash scripts/run_all_smoke.sh
```

全部 PASS 才算本轮契约不破。

---

## 7. SDKMessage union 增量

| 序号 | schema | 状态 | smoke |
|:-:|---|---|---|
| 28 | `SDKCompactCompletedEventSchema` | schema only, not yet emitted | w48 |

union 总数 27 → **28**；whitelist Section A 同步。
