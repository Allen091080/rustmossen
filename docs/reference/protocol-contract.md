# Protocol Contract — Mossen Stream-JSON / Runtime Snapshot / Extension Manifest

> **目的**: 把 Mossen 对外暴露的协议边界写成权威源, 让 Workbench / Web / Mobile / Extension 在动手前必须先读这里, 任何破坏 stream-json 协议形状的改动会被 `scripts/stream_json_contract_smoke.py` 阻断。
> **来源**: CWB-1 (分层边界) + CWB-2 (import 静态审计) + CWB-3 实测 (本文件 + whitelist + smoke 落地).
> **状态**: v1.0 — stream-json 已稳定; runtime snapshot / extension manifest / panel-state 仍是草案; in-process SDK 推 CWB-D9.
> **配套**:
> - `docs/waves/core-cli-workbench/layer-boundaries.md` (分层边界)
> - `docs/reference/layer-boundary-rules.md` (静态 import 审计)
> - `docs/reference/red-lines.md` (协议红线)
> - `scripts/stream-json-schema-whitelist.txt` (single source of truth)
> - `scripts/stream_json_contract_smoke.py` (静态校验)

---

## 0. 一句话定位

Workbench / Web / Mobile **第一阶段只能**通过 CLI binary subprocess + stream-json 接入 Mossen Core, **禁止** in-process import `query.ts` / `screens/REPL.tsx` / `bootstrap/state.ts` / `Tool.ts` / `tools/*` / `state/AppStateStore.ts`。

CLI 是唯一稳定二进制入口; 所有 Surface 的 fallback。CLI 二进制内**不**包含 Workbench / Web / Mobile / 自进化 GUI 代码。

任何破坏性协议变化 (删字段 / 改语义 / 重命名 union 成员) 必须先写 deprecation 草案 + Allen 显式拍板, 不能在普通 commit 中悄悄落入。

---

## 1. 协议优先级

| 优先级 | 协议 | 状态 | 入口 / 启动方式 |
|:-:|---|:---:|---|
| **P0** | **stream-json** | ✅ 稳定 | `mossen -p --input-format stream-json --output-format stream-json --verbose` |
| P1 | runtime snapshot | ⏳ CWB-4 草案 (本文 §4) | `mossen runtime snapshot --format json` (未实现) |
| P2 | panel-state | ⏳ Wave 6+ 草案 | (未定义) |
| P3 | extension manifest | ⏳ CWB-D8 草案 (本文 §5) | `mossen extension validate <path>` (未实现) |
| P4 | in-process SDK | ⏳ **CWB-D9 推迟** | 推迟到 SDK typecheck 0 错 + `sdkUtilityTypes.js` 补齐 + StdoutMessage / control 类型族补齐 + codegen 跑通 |

第一阶段 Workbench 只允许使用 P0。其他协议要么是草案 (P1/P2/P3), 要么是已知阻塞 (P4)。

---

## 2. stream-json — Event Shape

### 2.1 三层结构

```
P0 stream-json 协议
├─ stdout (mossen → Workbench): StdoutMessage union (8 成员)
│   └─ 含 SDKMessage union (27 成员)
└─ stdin  (Workbench → mossen): StdinMessage union (6 成员)
    └─ 含 SDKControlRequestInner union (29 成员)
```

### 2.2 SDKMessage union (28 成员)

实测来源: `entrypoints/sdk/coreSchemas.ts`。完整清单维护在 `scripts/stream-json-schema-whitelist.txt` Section A。任何增删必须同 commit 改 whitelist + 本文件 + smoke 校验。W48 加入 `SDKCompactCompletedEventSchema` (compact 完成 system event)。

```
SDKAssistantMessage / SDKUserMessage / SDKUserMessageReplay / SDKResultMessage /
SDKSystemMessage / SDKPartialAssistantMessage / SDKCompactBoundaryMessage /
SDKCompactCompletedEvent / SDKStatusMessage / SDKAPIRetryMessage /
SDKLocalCommandOutputMessage / SDKHookStartedMessage / SDKHookProgressMessage /
SDKHookResponseMessage / SDKToolProgressMessage / SDKAuthStatusMessage /
SDKTaskNotificationMessage / SDKTaskStartedMessage / SDKTaskProgressMessage /
SDKSessionStateChangedMessage / SDKFilesPersistedEvent / SDKToolUseSummaryMessage /
SDKRateLimitEvent / SDKElicitationCompleteMessage / SDKPromptSuggestionMessage /
SDKToolResultMessage / SDKCapabilityRecommendationMessage /
SDKCapabilityRecommendationResultMessage
```

### 2.3 SDKControlRequestInner union (29 成员)

实测来源: `entrypoints/sdk/controlSchemas.ts`。完整清单见 whitelist Section B。W46 加入 4 个 high-value dedicated 协议 (`compact_conversation` / `get_config_summary` / `runtime_doctor_summary` / `git_diff_summary`)，详见 `docs/W_MAIN_HIGH_VALUE_CONTROL_PROTOCOLS.md`。W47 加入 3 个 real capability operations (`apply_config_change` / `get_capability_operations` / `project_memory_operation`)，详见 `docs/W_MAIN_REAL_CAPABILITY_OPERATIONS.md`。

```
SDKControlInterruptRequest / SDKControlPermissionRequest / SDKControlInitializeRequest /
SDKControlSetPermissionModeRequest / SDKControlSetModelRequest /
SDKControlSetMaxThinkingTokensRequest / SDKControlMcpStatusRequest /
SDKControlGetContextUsageRequest / SDKHookCallbackRequest /
SDKControlMcpMessageRequest / SDKControlRewindFilesRequest /
SDKControlCancelAsyncMessageRequest / SDKControlSeedReadStateRequest /
SDKControlMcpSetServersRequest / SDKControlReloadPluginsRequest /
SDKControlMcpReconnectRequest / SDKControlMcpToggleRequest /
SDKControlStopTaskRequest / SDKControlApplyFlagSettingsRequest /
SDKControlGetSettingsRequest / SDKControlElicitationRequest /
SDKControlSlashCommandRequest /
SDKControlCompactConversationRequest / SDKControlGetConfigSummaryRequest /
SDKControlRuntimeDoctorSummaryRequest / SDKControlGitDiffSummaryRequest /
SDKControlApplyConfigChangeRequest / SDKControlGetCapabilityOperationsRequest /
SDKControlProjectMemoryOperationRequest
```

### 2.4 StdoutMessage union (8 成员) — Workbench 消费

实测来源: `entrypoints/sdk/controlSchemas.ts:642-651`。

```
SDKMessageSchema (含 27 union)
SDKStreamlinedTextMessageSchema
SDKStreamlinedToolUseSummaryMessageSchema
SDKPostTurnSummaryMessageSchema
SDKControlResponseSchema
SDKControlRequestSchema
SDKControlCancelRequestSchema
SDKKeepAliveMessageSchema
```

### 2.5 StdinMessage union (6 成员) — Workbench 发送

实测来源: `entrypoints/sdk/controlSchemas.ts:655-661`。

```
SDKUserMessageSchema
SDKControlRequestSchema
SDKControlResponseSchema
SDKKeepAliveMessageSchema
SDKUpdateEnvironmentVariablesMessageSchema
SDKCapabilityRecommendationResponseSchema
```

`capability_recommendation_response` is the stdin response to a
`capability_recommendation` stdout event. The host must echo the event's
`recommendation_id` and one of the event's `choice_id` values. CLI/Core handles
the side effects: explicit `install` uses the shared plugin install core, while
`not_now`, `never_for_capability`, and `disable_all_recommendations` update the
recommendation preference state. Clients must not install plugins locally or
invent an installed state.

After processing the response, CLI/Core emits a `capability_recommendation_result`
SDK message with `{recommendation_id, choice_id, action, status, summary}`.
This is an acknowledgement/completion signal only; it intentionally does not
include local paths, install logs, or raw plugin manifests.

### 2.6 `slash_command` — Workbench/SDK 能力封装入口

`SDKControlSlashCommandRequestSchema` 是 stream-json 客户端触发 CLI/Core 能力的统一边界协议。它不是 TUI `/model`、`/clear` 等命令本体，也不允许 Workbench 直接 import 或调用命令内部实现。

**Request**

```json
{
  "type": "control_request",
  "request_id": "<client-generated-id>",
  "request": {
    "subtype": "slash_command",
    "command": "model",
    "args": []
  }
}
```

**Success response**

```json
{
  "type": "control_response",
  "response": {
    "subtype": "success",
    "request_id": "<same-id>",
    "response": {
      "subtype": "slash_command_result",
      "command": "<command>",
      "status": "completed",
      "summary": "<human-readable summary>",
      "...": "command-specific structured payload"
    }
  }
}
```

**Error response**

```json
{
  "type": "control_response",
  "response": {
    "subtype": "error",
    "request_id": "<same-id>",
    "error": "unsupported_slash_command: compact; stream-json compact bridge requires idle ToolUseContext"
  }
}
```

#### 2.6.1 当前支持矩阵

权威源: `services/slashCommandCapabilities.ts`。Workbench 启动后应先请求 `/capabilities`，以该 manifest 作为 runtime source of truth；静态 UI registry 只作为旧 binary fallback。

Manifest entries include `resultKind` and `payloadKeys`. UI clients should use those fields to select renderers instead of guessing from command names. A successful response always keeps `response.subtype === 'slash_command_result'`; command-specific payloads live under the listed keys.

The `/capabilities` response also includes `manifestVersion` (currently `2`). Each entry includes `acceptedArgs`; clients must not invent extra arguments beyond this list. Side-effect commands such as `clear` must declare their confirmation gate through both `requiresConfirmation` and `acceptedArgs`.

Current command-specific payload shapes are intentionally narrow:
`runtime{cwd,permissionMode,model,sessionId}`,
`model{current,source,available[],profiles[],switched?}`,
`clear{cleared,scope}`,
`cost{totalCostUsd,durations,tokens,lines}`,
`skills[]`,
`mcp{servers[]}`,
`plugins{enabled[],disabled[],errorCount}`,
and `agents[]`.

| command | status | side effect | argsMode | acceptedArgs | resultKind | payloadKeys | 说明 |
|---|---|---|---|---|---|---|---|
| `help` | available | none | none | none | `help` | `commands`, `streamJsonCapabilities` | 列出可见 slash 命令，并标出 stream-json 是否支持。 |
| `capabilities` | available | none | none | none | `capabilities` | `capabilities` | 机器可读能力 manifest。Workbench/Web/Mobile 必须优先消费它。 |
| `status` | available | none | none | none | `status` | `runtime` | 只读 runtime 状态。 |
| `model` | available | switches_session_model | profile_name | profile name | `model` | `model` | 无参返回模型状态和 profile 清单；一个参数时按 CLI `/model <profileName>` 语义切换当前会话 profile，不写全局配置。 |
| `clear` | available | clears_conversation | confirm_only | `--confirm` | `clear` | `clear` | 有副作用，必须 confirm，且当前 turn idle。 |
| `cost` | available | none | read_only_no_args | none | `cost` | `cost` | 只读当前 session 成本、耗时、tokens 和代码行变更统计；不写配置。 |
| `skills` | available | none | read_only_no_args | none | `skills` | `skills` | 只读技能清单；不回传技能正文和本地路径。 |
| `mcp` | available | none | read_only_no_args | none | `mcp` | `mcp` | 只读 MCP 状态；不回传 raw server config/headers/URL。 |
| `plugin` / `plugins` | available | none | read_only_no_args | none | `plugin` | `plugins` | 只读插件清单；不安装、不 reload、不改配置、不回传本地路径。 |
| `agents` | available | none | read_only_no_args | none | `agents` | `agents` | 只读 agent 清单；不回传 prompt 正文和本地路径，不启动 agent。 |
| `compact` | blocked | none | none | none | `error` | none | 需要 idle `ToolUseContext`、LLM 调用、hooks 和 compaction runtime；不得在 control_request handler 内伪实现。 |

#### 2.6.2 slash bridge 红线

- `slash_command` 只封装 **CLI/Core 对外能力**，不改 TUI 命令本体语义。
- 新增 command 前必须先更新 `services/slashCommandCapabilities.ts`，再实现 `cli/print.ts` 分支，再加 focused smoke。
- 不允许在 Workbench 侧把 preview/mock 能力标成 available；主仓 manifest 是真源头。
- 有副作用命令必须在 manifest 中标出 `readOnly:false`、`requiresConfirmation`、`sideEffect` 和 args gate。
- `/compact` 不得绕过 Core 直接调用 `compactConversation()`；除非先设计安全的 idle `ToolUseContext` orchestrator。

### 2.7 权威源

| 维度 | 文件 |
|---|---|
| Zod schema 定义 | `entrypoints/sdk/coreSchemas.ts`, `entrypoints/sdk/controlSchemas.ts` |
| host 端 parser/emitter | `cli/structuredIO.ts` (859 行, `class StructuredIO`) |
| stream-json 模式分发 | `main.tsx` + `cli/print.ts` (见 §3) |
| slash capability manifest | `services/slashCommandCapabilities.ts` |
| 序列化 | `cli/ndjsonSafeStringify.ts` (NDJSON 安全, 防 U+2028/U+2029 切断) |

---

## 3. stream-json — 启动 SOP

### 3.1 唯一推荐入口

```bash
mossen -p \
  --input-format stream-json \
  --output-format stream-json \
  --verbose
```

### 3.2 实现链 (file:line, main HEAD e586296)

| 步 | 文件 | 行号 | 作用 |
|:-:|---|:---:|---|
| 1 | `main.tsx` | 1005 | `--output-format` choices `['text', 'json', 'stream-json']` |
| 2 | `main.tsx` | 1009 | `--input-format` choices `['text', 'stream-json']` |
| 3 | `main.tsx` | 1862-1864 | input/output 一致校验: `inputFormat === 'stream-json' && outputFormat !== 'stream-json'` → exit |
| 4 | `cli/print.ts` | 451 | `export async function runHeadless(...)` |
| 5 | `cli/print.ts` | 583 | `getStructuredIO(inputPrompt, options)` 调用 |
| 6 | `cli/print.ts` | 783-785 | `--output-format=stream-json requires --verbose` exit 守卫 |
| 7 | `cli/print.ts` | 5020 | `function getStructuredIO(...)` 定义 |
| 8 | `cli/structuredIO.ts` | 135 | `export class StructuredIO` |
| 9 | `cli/structuredIO.ts` | 344 | `keep_alive` 处理 |
| 10 | `cli/structuredIO.ts` | 433/442 | `control_request` 处理 + 必填校验 |

### 3.3 失败模式 (命令拒绝启动 stream-json)

| 失败 | 触发 | 退出方式 |
|---|---|---|
| 缺 `--verbose` | `--output-format=stream-json` 但无 `--verbose` | exit, 输出 "Error: When using --print, --output-format=stream-json requires --verbose" (`cli/print.ts:783-785`) |
| 双端不一致 | `--input-format=stream-json` 但 `--output-format !== 'stream-json'` | exit, 输出 "Error: --input-format=stream-json requires output-format=stream-json." (`main.tsx:1862-1864`) |
| --sdk-url 不全 | `--sdk-url` 但 input/output 不双 stream-json | exit, 输出 sdk-url 必须双端 stream-json (`main.tsx:1872`) |
| --replay-user-messages 不全 | 同上 | exit (`main.tsx:1881`) |
| --include-partial-messages 不全 | 缺 `--print` 或 output-format ≠ stream-json | exit (`main.tsx:1889`) |

---

## 4. Runtime Snapshot — 命令草案 (CWB-4 推)

### 4.1 候选命令

```bash
mossen runtime snapshot --format json     # 推 CWB-4 实施
```

### 4.2 候选 schema

来源: `platform/runtimeTypes.ts` (`PlatformRuntimeSnapshot`, 282 行 23 子 snapshot 域):

```
PlatformRuntimeSnapshot {
  provider, localGit, directConnect, sshRemote, systemPrompt,
  memory, compression, skills, security, plugins, mcp, remote,
  assistant, chrome, voice, teamMemory, agents, sessions, swarm,
  featureGates, manifest
}
```

### 4.3 约束

| 约束 | 必须 |
|---|---|
| 只读 | ✅ 不写 state |
| 0 network | ✅ 不调 backend |
| 0 LLM | ✅ 不 spawn LLM |
| 0 mossen 启动 | ❌ snapshot 命令本身需启动 mossen 进程, 但应 < 1s 退出 |
| 输出格式 | JSON, 与 `runtimeTypes.ts` 类型一致 |

### 4.4 用途

- 已实现: Workbench 启动后通过 `slash_command` / `capabilities` 获取 slash 能力 manifest。
- 未实现: 非 slash runtime snapshot 仍推 CWB-4。
- Web / Mobile 远程查 capability 状态
- 诊断包生成 (Allen 拍板的 Evolution / Repair 路径)

### 4.5 阻塞

- 命令未实现 (改 `main.tsx` + 新增 `commands/runtime/*`, 推 CWB-4)
- 当前 `platform/runtime.ts` 只是 import 桥 (1 行 `import { getSystemPrompt } from '../constants/prompts.js'`); 真正 snapshot 在 `platform/{provider,localGit,directConnect,sshRemote,memory,...}Runtime.ts` 拼装 (CWB-4 时统一接口)

---

## 5. Extension Manifest — 命令草案 (CWB-D8 推)

### 5.1 候选命令

```bash
mossen extension validate <path>          # 推 CWB-D8 实施
```

### 5.2 候选 schema

来源: `platform/manifest.ts` (1 行: `import type { PlatformCapabilityManifestEntry } from './runtimeTypes.js'`) + `runtimeTypes.ts` `PlatformCapabilityManifestEntry`:

```
PlatformCapabilityManifestEntry {
  id: PlatformCapabilityDomain   // 19 个 domain
  title: string
  status: 'wired' | 'degraded' | 'disabled' | 'snapshot-missing'
  modules: string[]
  validation: string[]
}
```

### 5.3 约束

- 只读 schema 校验 (不安装 / 不启用)
- 0 network
- 失败模式与 §3.3 相同模式 (exit + 错误码)

---

## 6. 协议演进规则 (only-additive)

### 6.1 允许 (additive)

| 操作 | 适用 |
|---|---|
| 加字段 (optional) | SDKMessage / SDKControlRequestInner / Stdout/StdinMessage 任意成员 |
| 加 union 成员 | SDKMessage / SDKControlRequestInner / Stdout/StdinMessage 顶层 union |
| 加 control_request subtype | SDKControlRequestInner |
| 加 hook event | HOOK_EVENTS (`coreTypes.ts:25-53`, 27 个) |

加项必须同 commit 同步:
1. `entrypoints/sdk/{core,control}Schemas.ts` Zod schema
2. `scripts/stream-json-schema-whitelist.txt`
3. `docs/reference/protocol-contract.md` §2.x
4. 必要时 `cli/structuredIO.ts` 处理路径

### 6.2 禁止 (breaking)

| 操作 | 处理 |
|---|---|
| 删字段 | **禁止**, 触 red-lines.md §3 stream-json only-additive 红线 |
| 改字段语义 (类型不变但含义变) | **禁止**, 必须新加字段 + 旧字段 deprecated |
| 删 union 成员 | **禁止** (Workbench 已实装版本会崩) |
| 重命名 union 成员 | **禁止** (= 删 + 加, 属于 breaking) |
| 让 TUI 文案混入协议 | **禁止**, 协议层 UI-agnostic (`layer-boundary-rules.json` `protocol-no-react-ink-ui` 已 enforce) |
| 让 Workbench 依赖非协议 stdout (free-text) | **禁止**, 必须走 schema 化 event |

### 6.3 破坏性变化 SOP

任何 breaking 改动:
1. 写 deprecation 草案 (`docs/waves/core-cli-workbench/protocol-deprecation-<id>.md`)
2. Allen 显式拍板
3. 新加 deprecated 字段 / event 共存 ≥ 1 个 release cycle
4. 同 commit 改 whitelist + smoke + 本文件 §2 + red-lines.md
5. 通知所有 Workbench / Web / Mobile / Extension 维护方

---

## 7. Workbench 消费 SOP

### 7.1 推荐架构

```text
Workbench process
  ↓ spawn mossen binary subprocess
  ↓ stdin/stdout pipe (NDJSON)
  ↓ Workbench-side NDJSON parser (自己实现, 不 import cli/ndjsonSafeStringify.ts)
  ↓ Workbench-side schema validator (从 entrypoints/sdk/*Schemas 反序列化, 但当前只能拷贝 schema 文本; 等 CWB-D9 后可 in-process import)
  ↓ Workbench store (React/Vue/Svelte/Electron/Tauri UI 层)
```

### 7.2 错误恢复

| 场景 | 处理 |
|---|---|
| subprocess 崩溃 | Workbench 重启 mossen + `--resume <session-id>` (CWB-4 候选 `mossen runtime snapshot` 后实现 session 索引) |
| schema 验证失败 (Workbench 不认识的 event) | log + ignore (协议 only-additive 保证向前兼容) |
| `--verbose` / 双端一致校验失败 | Workbench 启动失败, 检查命令行参数 |
| stdin 写入超大 message (> Bun line buffer) | 拆成多 message, 每个 ≤ 64KB 安全 |

### 7.3 严禁 (Workbench 不允许做)

- in-process import 任何 `mossensrc/*` 文件 (违反 `surface-no-core-internals`, `layer_boundary_audit` 阻断)
- 直接读 `bootstrap/state.ts` / `state/AppStateStore.ts`
- 直接调 `Tool.ts` 任何 implementor
- 直接渲染 `screens/REPL.tsx` 内任何 React component
- Vendoring `mossensrc/` 源码 (CWB-Workbench-V0 推单独 repo)
- 在 mossen binary 包内塞 Workbench GUI 代码 (CLI 二进制不包含 Workbench)

### 7.4 模板瘦身原则 (Workbench 实施前不约束本仓, 但作为后续启动时的 SOP)

未来 Workbench repo 启动时:
- mock / template / placeholder 内容**不得**进入默认运行路径
- 先瘦身模板, 只保主框架 (subprocess adapter + NDJSON parser + schema validator + 1 个真实 panel)
- 一个真实功能一个真实功能接 (review artifact / task panel / permission dialog 等), 每个独立 commit + 独立验证
- 不允许"全功能模板" + 后续逐步替换 (容易留死代码 / 假阳性测试)

---

## 8. 维护责任与验证命令

### 8.1 文件位置

- **本文件**: `docs/reference/protocol-contract.md`
- **whitelist**: `scripts/stream-json-schema-whitelist.txt` (single source of truth)
- **smoke**: `scripts/stream_json_contract_smoke.py` (静态校验, 接入 `run_all_smoke.sh` step 19)
- **CLI binary 协议层 (永久不动)**: `cli/structuredIO.ts`, `cli/print.ts`, `cli/ndjsonSafeStringify.ts`, `entrypoints/sdk/{core,control}Schemas.ts`

### 8.2 维护时机

| 触发 | 动作 |
|---|---|
| `coreSchemas.ts` 加 / 改 SDKMessage union 成员 | 同 commit 改 whitelist Section A + 本文件 §2.2 |
| `controlSchemas.ts` 加 / 改 SDKControlRequestInner / Stdout/Stdin | 同 commit 改 whitelist Section B/C/D + 本文件 §2.3-2.5 |
| `main.tsx` 改 stream-json 入口 (新 flag / 新 format) | 同 commit 改 whitelist Section E anchors + 本文件 §3 行号 + smoke 锚点 |
| `cli/print.ts` 改 stream-json 守卫 (新 exit 路径) | 同 commit 改 whitelist Section E anchors + 本文件 §3.3 + smoke 锚点 |
| SDK in-process 路径修复 (e.g., 补 `sdkUtilityTypes.js`, 跑 codegen) | 同 commit 删 whitelist Section F known_blocker + 更新 CWB-D9 状态 + 本文件 §1 P4 |
| 任何 breaking 改动 | 不允许. 走 §6.3 SOP |

### 8.3 验证命令

```bash
# CWB-3 阶段必跑
python3 scripts/stream_json_contract_smoke.py    # < 0.1s
bash scripts/run_all_smoke.sh --dry-run          # 列 stream_json_contract_smoke step
bash scripts/run_all_smoke.sh                    # ALL PASS, case 39 fingerprint 稳定

# 任何 schema / 入口 / 守卫改动后必跑
bun run typecheck:diff                           # 0 NEW
bun run lint:diff                                # 0 NEW
python3 scripts/layer_boundary_audit.py          # PASS
python3 scripts/stream_json_contract_smoke.py    # PASS (含 anchor + known_blocker 校验)

# Workbench / Web / Mobile / Extension 实启前必跑
python3 scripts/layer_boundary_audit.py          # 防直接 import Core internals
python3 scripts/stream_json_contract_smoke.py    # 防协议漂移
```

---

## 9. CWB 系列定位

| 阶段 | 状态 | 范围 |
|---|:---:|---|
| CWB-1 | ✅ 已合 main `e586296` | 分层边界文档 (`layer-boundaries.md` + `architecture-boundaries.md` §8 + `red-lines.md` §3) |
| CWB-2 | ✅ 已合 main `e586296` | 静态 import 审计 (`layer_boundary_audit.py` + `layer-boundary-rules.json` + `layer-boundary-rules.md`) |
| **CWB-3** (本文件) | ✅ 本 commit | 协议契约 + stream-json contract smoke + runtime snapshot 草案 |
| CWB-4 | ⏳ 推 | `mossen runtime snapshot` 命令实现 + Workbench adapter 启动前的 layer_boundary_audit 规则扩展 |
| CWB-5 | ⏳ 推 | CLI binary packaging hardening (`utils/bundledMode.ts` / `nativeInstaller/*` 审计) |
| CWB-D8 | ⏳ 推 | extension manifest 协议落地 (`mossen extension validate <path>`) |
| CWB-D9 | ⏳ 推 | SDK in-process typecheck 修复 (补 `sdkUtilityTypes.js` + codegen + StdoutMessage / control 类型族补齐); 本文 §1 P4 阻塞依据维护在 whitelist Section F |
| CWB-Workbench-V0 | ⏳ 推 | 独立 `mossen-workbench` repo 启动 (subprocess + stream-json adapter, **不在 mossensrc 主仓**) |

---

*— Protocol Contract v1.1 / CWB-3+W44 / stream-json slash capability manifest 已纳入权威契约。*
