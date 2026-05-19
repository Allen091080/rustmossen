# Mossen Stream-JSON Capability Protocol Matrix

> **目的**: mossensrc CLI/Core 通过 stream-json `slash_command` 暴露给 Workbench / Web / Mobile 的**完整能力矩阵**——单一权威源。
> **范围**: 全 102 个 TUI 命令中、`/capabilities` manifest 显式列出的子集。其余命令默认认为不属于 stream-json 协议表面。
> **状态**: manifestVersion = 2 (matrix 扩展，新增 12 个 canonical entries — 3 available + 9 blocked，其中 1 个带 alias)。
> **不在范围**: TUI 命令本体语义、模型/插件/MCP/权限核心实现、`/compact` 真实执行、任何 mutation。
> **计数规则**: manifest **按 canonical command 计数**。alias 不算独立 entry，只在 owning entry 的 `aliases: [...]` 数组里登记并由 `normalizeStreamJsonSlashCommand` 统一解析。当前总计 **23 canonical entries (13 available + 10 blocked) + 4 aliases**（`capability` → `capabilities`，`plugins` → `plugin`，`allowed-tools` → `permissions`，`settings` → `config`）。`/capabilities` 响应里 `capabilities[]` 数组长度等于 canonical count，alias 数量需要 client 自行 sum `aliases.length` 得到。
> **配套**:
> - `services/slashCommandCapabilities.ts` — manifest single source of truth（**23 canonical = 13 available + 10 blocked，4 aliases**）
> - `cli/print.ts` `slash_command` dispatcher (≈L3990–L4555)
> - `entrypoints/sdk/controlSchemas.ts` `SDKControlSlashCommandRequestSchema`
> - smoke: `scripts/wave_w29b_*.py`, `wave_w40_*.py`, `wave_w42_*.py`, `wave_w43_*.py`, `wave_w44_*.py`, `wave_w45_capability_protocol_matrix_smoke.py`
> - `docs/reference/protocol-contract.md` §2.6

---

## 1. 设计原则

1. **manifest = 真源**: Workbench/Web/Mobile 启动后必须先调 `/capabilities`，结果以 `manifestVersion + capabilities[]` 形式回包；UI 用 `payloadKeys` 选 renderer，绝不硬编码命令名→字段路径。
2. **只读**: 所有 `available + readOnly:true` 命令的回包内容必须从已运行进程的现有 in-memory / cache / config 状态直接派生。**不允许**新增 disk write / LLM 调用 / network call / plugin/skill 安装 / MCP 启停 / 配置写入。
3. **不伪造**: 没有真实数据源就 `unavailable` 或 `blocked` + 稳定 `reason`，**不得**编造插件/技能/MCP/cost/权限/钩子/记忆等数据。
4. **不改 TUI 命令**: 命令本体 (`commands/*`) 在本批 0 行变更。`slash_command` 分支只是 CLI/Core 已暴露 helper / getter 的对外封装层。
5. **mutation 必须显式 gate**: 只有 `clear --confirm`（已具 idle guard 与 confirm gate）和 `/model <profile>`（仅 session 级、不写全局）允许写状态。其余 mutation 默认 `blocked`。
6. **错误形态稳定**: 错误响应保持现有 `control_response.error` 字符串形态，不扩成对象；前缀 tag 可被 client 安全 substring-match。

---

## 2. 命令矩阵

| id | command | status | readOnly | sideEffect | resultKind | smoke |
|---|---|:---:|:---:|---|---|---|
| `slash.help` | help | available | ✅ | none | help | w29b |
| `slash.capabilities` | capabilities | available | ✅ | none | capabilities | w43 |
| `slash.status` | status | available | ✅ | none | status | w29b |
| `slash.model` | model | available | ❌ | switches_session_model | model | w29b |
| `slash.clear` | clear | available | ❌ | clears_conversation | clear | w40 |
| `slash.cost` | cost | available | ✅ | none | cost | w44 |
| `slash.skills` | skills | available | ✅ | none | skills | w42 |
| `slash.mcp` | mcp | available | ✅ | none | mcp | w42 |
| `slash.plugin` | plugin (`plugins`) | available | ✅ | none | plugin | w42 |
| `slash.agents` | agents | available | ✅ | none | agents | w42 |
| `slash.permissions` | permissions (`allowed-tools`) | available | ✅ | none | permissions | **w45** |
| `slash.hooks` | hooks | available | ✅ | none | hooks | **w45** |
| `slash.memory` | memory | available | ✅ | none | memory | **w45** |
| `slash.compact` | compact | **blocked** | — | — | error | w29b/w40 |
| `slash.context` | context | **blocked** | — | — | error | w45 |
| `slash.config` | config (`settings`) | **blocked** | — | — | error | w45 |
| `slash.profile` | profile | **blocked** | — | — | error | w45 |
| `slash.doctor` | doctor | **blocked** | — | network | error | w45 |
| `slash.diff` | diff | **blocked** | — | starts_process | error | w45 |
| `slash.ide` | ide | **blocked** | — | starts_process | error | w45 |
| `slash.init` | init | **blocked** | — | writes_files | error | w45 |
| `slash.login` | login | **blocked** | — | auth_state | error | w45 |
| `slash.logout` | logout | **blocked** | — | auth_state | error | w45 |

> 全 23 canonical entries 均带 `id / command / title / kind / protocol / status / readOnly / requiresConfirmation / argsMode / acceptedArgs / sideEffect / resultKind / payloadKeys / errorTags / source / lastVerifiedBySmoke / summary` 字段；blocked 项额外带 `reason`；4 个 alias (`capability` / `plugins` / `allowed-tools` / `settings`) 各自登记在 owning entry 的 `aliases` 数组里，**不作为独立 entry**。

---

## 3. 协议详细 shape

### 3.1 通用 envelope

请求：
```json
{
  "type": "control_request",
  "request_id": "<uuid>",
  "request": { "subtype": "slash_command", "command": "<name>", "args": [] }
}
```

成功：
```json
{
  "type": "control_response",
  "response": {
    "subtype": "success",
    "request_id": "<same>",
    "response": {
      "subtype": "slash_command_result",
      "command": "<name>",
      "status": "completed",
      "summary": "<human-readable>",
      "...": "command-specific keys; see below"
    }
  }
}
```

错误：
```json
{
  "type": "control_response",
  "response": {
    "subtype": "error",
    "request_id": "<same>",
    "error": "<stable_tag>: <command>; <reason>"
  }
}
```

错误 tag 集（client 必须按字面前缀 match，不要解析自由文本）：
- `unsupported_slash_command: <name>; ...` — 不在 manifest
- `unsupported_slash_command_args: <name>; ...` — manifest 在 available，但 args 不被接受
- `confirmation_required: <name>; ...` — 需要 `--confirm`
- `session_not_idle: <name>; ...` — turn 占用
- `model_profile_not_found: <name>; ...` — `/model <profile>` 找不到 profile
- `blocked_slash_command: <name>; ...` — manifest 状态 blocked（compact 仍用 `unsupported_slash_command:` 前缀以保持向后兼容）

### 3.2 `/capabilities`

成功额外字段：
```json
{
  "manifestVersion": 2,
  "capabilities": [
    {
      "id": "slash.cost",
      "command": "cost",
      "title": "Session cost / usage",
      "kind": "slash_command",
      "protocol": "stream_json",
      "aliases": [],
      "status": "available",
      "readOnly": true,
      "requiresConfirmation": false,
      "argsMode": "read_only_no_args",
      "acceptedArgs": [],
      "sideEffect": "none",
      "resultKind": "cost",
      "payloadKeys": ["cost"],
      "errorTags": ["unsupported_slash_command_args: cost"],
      "source": "cli/print.ts:slash_command/cost",
      "lastVerifiedBySmoke": "wave_w44_cost_slash_smoke",
      "summary": "Return current session cost and usage totals."
    }
  ]
}
```

### 3.3 已稳定的 read-only 命令 payload 摘要

> 详细 shape 见各命令的 smoke + dispatcher 源码；以下只列 payload 顶层 key。

- `/help` → `commands[]` + `streamJsonCapabilities[]`
- `/status` → `runtime{cwd, permissionMode, model, sessionId}`
- `/model` (无参) → `model{current, source, available[], profiles[], switched:null}`
- `/model <profile>` → 同上 + `model.switched{profileName, model, source}`
- `/clear --confirm` → `clear{cleared:true, scope:'conversation'}`
- `/cost` → `cost{totalCostUsd, hasUnknownModelCost, totalDurationMs, totalApiDurationMs, totalToolDurationMs, inputTokens, outputTokens, cacheReadInputTokens, cacheCreationInputTokens, webSearchRequests, linesAdded, linesRemoved}`
- `/skills` → `skills[{name, description, source, loadedFrom, userInvocable, modelInvocable, kind, hasWhenToUse}]`
- `/mcp` → `mcp.servers[{name, status, scope, serverInfo, error?, toolCount, tools[], capabilities?}]`
- `/plugin` → `plugins{enabled[{name, enabled, isBuiltin, sourceKind, components[]}], disabled[...], errorCount}`
- `/agents` → `agents[{name, description, source, model, memory, background, hasTools, hasSkills, hasMcpServers, permissionMode, maxTurns}]`

### 3.4 W45 新增 read-only 命令

#### `/permissions`

```json
{
  "command": "permissions",
  "status": "completed",
  "summary": "Permission mode: <mode>",
  "permissions": {
    "mode": "default|acceptEdits|plan|bypassPermissions|...|null",
    "isBypassPermissionsModeAvailable": false,
    "shouldAvoidPermissionPrompts": null,
    "alwaysAllowRuleCounts": { "userSettings": 3, "projectSettings": 1, "...": 0 },
    "alwaysDenyRuleCounts": {},
    "alwaysAskRuleCounts": {},
    "additionalWorkingDirectoryCount": 2
  }
}
```

数据源：`getAppState().toolPermissionContext` (in-memory)。

**禁止**回传：`alwaysAllowRules` / `alwaysDenyRules` / `alwaysAskRules` 的原始 pattern 列表，以及 `additionalWorkingDirectories` 的 path map——pattern 可能含敏感路径/正则。Workbench 如需 pattern 详情请走单独的 mutation-protected 设计，本批不开口。

错误：`unsupported_slash_command_args: permissions; ...`。

#### `/hooks`

```json
{
  "command": "hooks",
  "status": "completed",
  "summary": "<N> hooks configured.",
  "hooks": {
    "total": 7,
    "byEvent": { "PreToolUse": 3, "PostToolUse": 4 },
    "bySource": { "userSettings": 5, "projectSettings": 2 },
    "byType": { "command": 6, "prompt": 1 }
  }
}
```

数据源：`getAllHooks(getAppState())` (`src/utils/hooks/hooksSettings.ts`)，纯 settings 读，无 disk write/network。

**禁止**回传：`config.command` / `config.url` / `config.prompt` 等 hook body——可能含 API key、绝对路径、内部 endpoint。Workbench 如需 hook body 请直接读 settings 文件（settings 协议在 `get_settings` 控制请求里，且按 source 分层）。

错误：`unsupported_slash_command_args: hooks; ...`。

#### `/memory`

```json
{
  "command": "memory",
  "status": "completed",
  "summary": "<N> memory files loaded.",
  "memory": {
    "files": [
      {
        "path": "/abs/path/CLAUDE.md",
        "type": "Project|User|Managed|...",
        "parent": null,
        "globs": [],
        "contentLength": 4321,
        "contentDiffersFromDisk": false
      }
    ]
  }
}
```

数据源：`await getMemoryFiles()` (`src/utils/mossenmd.ts`)，memoize 过的 disk 读，仅在第一次调用时实际 I/O。

**禁止**回传：`content` / `rawContent`——文件内容可能含项目秘密。`path` 是必须的（这就是这个命令存在的意义），但 `contentLength` 已足以让 client 判断是否需要进一步加载。Workbench 如需文件正文请直接读盘（路径已知）或走 `get_context_usage`（按 token 算账）。

错误：`unsupported_slash_command_args: memory; ...`。

### 3.5 已 blocked 命令的稳定 reason

| command | reason | 替代协议 |
|---|---|---|
| `compact` | requires idle ToolUseContext with LLM calls and hooks | （暂无；需独立施工包） |
| `context` | use control_request subtype `get_context_usage` — slash wrapper would duplicate the dedicated builder | `get_context_usage` |
| `config` | use control_request subtype `get_settings` — slash wrapper would risk leaking secret values into summary text | `get_settings` |
| `profile` | duplicate of `/model` (returns profiles[] and current/default markers) | `/model` |
| `doctor` | doctor performs auth/version/network checks and renders a TUI panel; no clean read-only getter | （TUI only） |
| `diff` | use control_request subtype `git_diff_summary` — bounded git status/shortstat (5s timeout, 200-file cap); patch preview available via `includePatch` | `git_diff_summary` |
| `ide` | use mcp_status control_request — IDE is exposed there as an MCP server | `mcp_status` |
| `init` | init writes CLAUDE.md and project metadata — needs a dedicated mutation protocol with confirmation | （未定义） |
| `login` | auth flow requires interactive UI and credential write; use existing CLI guidance | `mossen login` CLI |
| `logout` | auth state mutation; needs a dedicated confirmation protocol before exposing | （未定义） |

所有 blocked 命令在 dispatcher 走 `blocked_slash_command: <command>; <reason>`（compact 为历史兼容仍用 `unsupported_slash_command:`）。Workbench 看到这个 tag 后**不应**重试或回退到 fallback 假数据；正确响应是按 manifest `reason` 引导用户走对应的替代协议或 CLI。

### 3.6 mutation args 拒绝矩阵

| 命令 | mutation 子命令 | 行为 |
|---|---|---|
| `/plugin install/remove/enable/disable` | `args.length > 0` | `unsupported_slash_command_args: plugin; ...` |
| `/skills install/remove/enable/disable` | `args.length > 0` | `unsupported_slash_command_args: skills; ...` |
| `/mcp start/stop/restart/add/remove` | `args.length > 0` | `unsupported_slash_command_args: mcp; ...` |
| `/agents create/delete` | `args.length > 0` | `unsupported_slash_command_args: agents; ...` |
| `/permissions allow/deny/...` | `args.length > 0` | `unsupported_slash_command_args: permissions; ...` |
| `/hooks add/remove` | `args.length > 0` | `unsupported_slash_command_args: hooks; ...` |
| `/memory add/edit/clear` | `args.length > 0` | `unsupported_slash_command_args: memory; ...` |
| `/cost reset/--since` | `args.length > 0` | `unsupported_slash_command_args: cost; ...` |
| `/model <name>` | 1 arg | switch session profile（**唯一**例外） |
| `/model <a> <b>` | ≥ 2 args | `unsupported_slash_command_args: model; ...` |
| `/clear` 无 `--confirm` | 任意 | `confirmation_required: clear; ...` |

Workbench 必须按 manifest `acceptedArgs` 列表构造请求，**不要**自创参数。

---

## 4. 红线（本轮不会发生）

| 红线 | 是否发生 | 备注 |
|---|---|---|
| 改 TUI 命令本体 (`commands/*`) | ❌ 否 | 0 行触动 |
| 改主循环 / ask / query / ToolUseContext 编排 | ❌ 否 | dispatcher 只调 getter |
| 改 `compactConversation` | ❌ 否 | `/compact` 仍 blocked，dispatcher 不 import |
| 安装插件 / 技能 / 启停 MCP | ❌ 否 | `loadAllPluginsCacheOnly` / `getSlashCommandToolSkills` / `buildMcpServerStatuses` 全只读 |
| 写 config / settings / session DB | ❌ 否 | 只读 in-memory |
| 启动 / 停止子进程 | ❌ 否 | 没有 spawn |
| 触发 LLM 调用 | ❌ 否 | dispatcher 路径无 ask/query |
| 改权限模式 | ❌ 否 | `/permissions` 只读 |
| 改 auth 状态 | ❌ 否 | login/logout blocked |
| 写文件（CLAUDE.md / 配置 / cache） | ❌ 否 | init blocked、memory 只读 |

---

## 5. Workbench 消费指引

1. **首请求 `/capabilities`**，缓存 `manifestVersion + capabilities[]`。每次启动主仓 binary 时复检。
2. UI 用 `resultKind` + `payloadKeys` 决定 renderer，不要凭命令名硬编码字段路径。`manifestVersion` bump 时主动重新挑 renderer。
3. **可发现性 ≠ 可执行性**：本批协议是“知道有什么”，不是“做点什么”。任何 mutation (插件安装、skill 启用、MCP 启停、cost 重置、权限/hook/memory 编辑、auth 切换等) 仍要走专用控制请求 (`reload_plugins` / `mcp_set_servers` / `capability_recommendation_response` / `apply_flag_settings` 等)，**不允许**叠加到 `slash_command` 上。
4. 看到 `unsupported_slash_command` / `unsupported_slash_command_args` / `blocked_slash_command` / `confirmation_required` / `session_not_idle` / `model_profile_not_found` 错误时，client 必须按字面 tag 分支处理，不要尝试解析后半段自由文本。
5. cost / plugin / skill / mcp / hooks / memory / permissions 字段为空数组或 0 计数时表示**真实空**；不是 unavailable。如果未来需要"无数据 / 不可用"区分，会在 manifest 加 `availabilityReason`。
6. blocked 命令的 `reason` 字段是稳定字符串（manifest 里固化）；UI 应直接展示给用户，不要尝试规避或 fallback。

---

## 6. 验证清单

```bash
python3 scripts/stream_json_contract_smoke.py
python3 scripts/wave_w29b_slash_command_smoke.py
python3 scripts/wave_w40_slash_bridge_batch_smoke.py
python3 scripts/wave_w42_capability_slash_wrappers_smoke.py
python3 scripts/wave_w43_slash_capability_manifest_smoke.py
python3 scripts/wave_w44_cost_slash_smoke.py
python3 scripts/wave_w45_capability_protocol_matrix_smoke.py
bash scripts/run_all_smoke.sh
```

全部 PASS 才算本批契约不破。`run_all_smoke.sh` 已把以上全部接入。

---

## 7. 后续工作（不在本轮）

- `/cost` 可选维度 (`--since` / `--by-model` / `--by-tool`)：先扩 `cost-tracker` getter，再扩 dispatcher。
- `/plugin install` / `/skills install` / `/mcp start` 等 mutation 协议：必须先做"安全 mutation 审计 + 用户确认 UX"再开口；目前 `capability_recommendation_response` 已覆盖 LSP plugin install 子集。
- `/compact` stream-json 桥：必须先设计 idle-`ToolUseContext` orchestrator + Allen 单独施工包；本文档**不**作 commitment。
- `/init` / `/login` / `/logout`：mutation+credential，需要专用的 confirmation 协议（不要复用 slash_command）。
- `/diff` / `/doctor` / `/ide`：如果未来 Workbench 真需要，应分别拆成 `git_diff_request` / `runtime_doctor_request` / `ide_status_request` 单独控制请求，不通过 slash_command 桥。
- `manifestVersion` ≥ 3 的破坏性变更（删字段 / 改 status 语义）：必须先在 `protocol-contract.md` 写 deprecation 段落，并跑 `wave_w43_*` + `wave_w45_*` 双 smoke 强校。
