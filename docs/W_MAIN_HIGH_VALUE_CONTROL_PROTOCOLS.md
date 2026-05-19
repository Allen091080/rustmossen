# Mossen Stream-JSON High-Value Dedicated Control Protocols (W46)

> **目的**: 给 5 个被 slash_command 拒绝的"高价值能力" — `compact` / `config` / `doctor` / `diff` / `ide` — 设计并落地**专用 control_request 协议**，让 Workbench 能用稳定 schema 消费，而不是绕开 slash 黑名单或 fork CLI/TUI。
> **范围**: 仅 stream-json `control_request` 层；不改 TUI 命令本体、不改主循环、不改插件/MCP/auth/config 写入路径。
> **状态**: 4 个新 subtype 已落地（含 1 个 blocked-with-reason 占位）；IDE 复用现有 `mcp_status`。Workbench 可立刻按本文档对齐 UI、parser、状态机。
> **配套**:
> - `entrypoints/sdk/controlSchemas.ts` — 4 个新 schema 定义
> - `cli/print.ts` — 对应 dispatcher 分支
> - `scripts/stream-json-schema-whitelist.txt` Section B — 29 成员
> - `scripts/wave_w46_high_value_protocols_smoke.py` — 契约 lock
> - `services/slashCommandCapabilities.ts` — slash entry reason 已指向新协议
> - `docs/reference/protocol-contract.md` §2.3 — union 实测 26 成员
> - `docs/W_MAIN_READONLY_CAPABILITY_PROTOCOLS.md` — slash matrix 不变（still 23 canonical / 4 alias）

---

## 1. 设计原则

1. **dedicated channel**: 高价值能力**不**通过 `slash_command` 绕道，而是各拿独立 `control_request.subtype`。slash 仍 blocked，`reason` 字段指向对应的 dedicated subtype，让 Workbench 可发现。
2. **read-only 优先**: 4 个 subtype 中 3 个是纯只读快照 (`get_config_summary` / `runtime_doctor_summary` / `git_diff_summary`)；1 个是 mutation 占位 (`compact_conversation` 永远返回 blocked) 等待后续安全 orchestrator 设计。
3. **真实数据源**: 所有数据都来自现有 in-memory getter 或 bounded subprocess；**不**伪造、**不**编造、**不**走 LLM/network/auth。
4. **secret-safe**: `get_config_summary` 只回 key 名 + 数量、不回 value；`git_diff_summary` 不回 patch；`runtime_doctor_summary` 不回长 error dump。
5. **bounded subprocess**: `git_diff_summary` 是唯一会 spawn 子进程的协议——固定 5s timeout、200 文件 cap、读专用 git 子命令、不走任何写命令。
6. **error shape 不变**: 复用现有 `control_response.error` 字符串。所有错误前缀稳定（`blocked_control_request`/`unsupported_control_request_args`/`...`），client 按字面 substring 处理。

---

## 2. 4 个新 control_request subtype

### 2.1 `compact_conversation` — blocked-with-reason 占位

**Request**:
```json
{
  "type": "control_request",
  "request_id": "<uuid>",
  "request": {
    "subtype": "compact_conversation",
    "mode": "manual",       // optional, only "manual" accepted today
    "dry_run": false        // optional
  }
}
```

**Response (always blocked in this wave)**:
```json
{
  "type": "control_response",
  "response": {
    "subtype": "success",
    "request_id": "<same>",
    "response": {
      "status": "blocked",
      "reason": "compact_conversation: requires idle ToolUseContext with LLM and hook orchestration — implementation deferred to a follow-up wave"
    }
  }
}
```

**Future shape (when implemented)**: `status: "completed"` will gain `summary` + `truncatedMessageCount`.

**Why blocked now**: real compaction needs (a) an idle main loop, (b) the same LLM/hook/permission context that ask/query holds, (c) atomic snapshot of message buffer. The control_request handler runs concurrently with main loop, so synthesising this is unsafe. Workbench must lock UI ("Compact unavailable in this build") rather than fall back to a fake.

**Slash relationship**: `slash.compact` manifest entry's `reason` now points at this subtype. UI can call this, get `status:"blocked"`, and render the reason verbatim.

### 2.2 `get_config_summary` — redacted settings shape

**Request**:
```json
{
  "type": "control_request",
  "request_id": "<uuid>",
  "request": { "subtype": "get_config_summary" }
}
```

**Response**:
```json
{
  "type": "control_response",
  "response": {
    "subtype": "success",
    "request_id": "<same>",
    "response": {
      "summary": "<N> settings sources contributed; <M> effective top-level keys.",
      "sources": [
        {
          "source": "userSettings",
          "present": true,
          "keyCount": 12,
          "topLevelKeys": ["model", "permissions", "..."]
        }
      ],
      "effectiveKeyCount": 18,
      "effectiveTopLevelKeys": ["model", "..."]
    }
  }
}
```

**数据源**: `getSettingsWithSources()` (`utils/settings/settings.ts:836`)。

**Redaction guarantee**: 永远只回 **key names + counts**，**绝不**回 value。值可能含 API key、URL、token、user-supplied path。Workbench 如需 raw 数据要走现有 `get_settings` 协议（responsibility 在 client 端做 redaction）。w46 smoke 锁住了禁用锚点 `JSON.stringify` / `.values(` / `settings: entry.settings` / `effective: withSources.effective`，避免回归泄漏。

**Slash relationship**: `slash.config` (`settings` alias) 仍 blocked，`reason` 指 `get_config_summary` + `get_settings` 二选一。

### 2.3 `runtime_doctor_summary` — structured in-process checks

**Request**:
```json
{
  "type": "control_request",
  "request_id": "<uuid>",
  "request": { "subtype": "runtime_doctor_summary" }
}
```

**Response**:
```json
{
  "type": "control_response",
  "response": {
    "subtype": "success",
    "request_id": "<same>",
    "response": {
      "summary": "7 checks; 0 failed, 1 warning.",
      "checks": [
        {
          "id": "cwd",
          "title": "Working directory",
          "status": "ok",
          "severity": "info",
          "summary": "/Users/allen/Documents/aiproject/mossensrc"
        },
        {
          "id": "session",
          "title": "Session id",
          "status": "ok",
          "severity": "info",
          "summary": "<uuid>"
        },
        {
          "id": "model",
          "title": "Active model",
          "status": "ok",
          "severity": "info",
          "summary": "claude-opus-4-7"
        },
        {
          "id": "permission_mode",
          "title": "Permission mode",
          "status": "ok",
          "severity": "info",
          "summary": "default"
        },
        {
          "id": "mcp",
          "title": "MCP servers",
          "status": "ok",
          "severity": "info",
          "summary": "3 configured; 0 failed/disconnected"
        },
        {
          "id": "memory",
          "title": "Memory files",
          "status": "ok",
          "severity": "info",
          "summary": "5 loaded"
        },
        {
          "id": "hooks",
          "title": "Configured hooks",
          "status": "ok",
          "severity": "info",
          "summary": "12 hooks across user/project/local/session"
        }
      ]
    }
  }
}
```

**check id 集合（稳定）**: `cwd` / `session` / `model` / `permission_mode` / `mcp` / `memory` / `hooks`。`status` ∈ `ok | warn | fail | unavailable`；`severity` ∈ `info | warning | error`。

**Safety**: w46 smoke 锁住的禁用锚点：`fetch(`、`https://`、`http://`、`spawn(`、`execSync(`。任何网络/进程操作都会让 smoke FAIL。诊断永远不读 `~/.mossen/auth.json` / token / API key。

**Slash relationship**: `slash.doctor` 仍 blocked，`reason` 指 `runtime_doctor_summary`。

### 2.4 `git_diff_summary` — bounded git status snapshot

**Request**:
```json
{
  "type": "control_request",
  "request_id": "<uuid>",
  "request": {
    "subtype": "git_diff_summary",
    "includeUntracked": false   // optional; default omits untracked
  }
}
```

**Response (在 git repo 里)**:
```json
{
  "type": "control_response",
  "response": {
    "subtype": "success",
    "request_id": "<same>",
    "response": {
      "status": "ok",
      "summary": "5 entries; 3 files changed, 42 insertions(+), 8 deletions(-)",
      "cwd": "/Users/allen/Documents/aiproject/mossensrc",
      "files": [
        { "path": "src/...", "status": " M", "staged": false }
      ],
      "stats": {
        "changedFiles": 3,
        "insertions": 42,
        "deletions": 8
      },
      "truncated": false
    }
  }
}
```

**Response (非 git repo)**:
```json
{
  "status": "not_git_repo",
  "summary": "Working directory is not inside a git work tree.",
  "cwd": "...",
  "reason": "<git rev-parse stderr or undefined>"
}
```

**Response (git failed / timeout)**:
```json
{
  "status": "unavailable",
  "summary": "git status failed.",
  "cwd": "...",
  "reason": "git status timed out after 5000ms"
}
```

**Constraints (locked by w46 smoke)**:
- `TIMEOUT_MS = 5000` — 硬 timeout，超时 SIGKILL
- `FILE_CAP = 200` — 多余文件丢弃，置 `truncated: true`
- `GIT_OPTIONAL_LOCKS=0` — 不抢 git index lock
- `stdio: ['ignore', 'pipe', 'pipe']` — 不继承 stdin
- 子命令白名单：仅 `rev-parse --is-inside-work-tree` / `status --porcelain` / `diff --shortstat`
- 禁用锚点：`git push` / `git commit` / `git checkout` / `git reset` / patch echo

**Slash relationship**: `slash.diff` 仍 blocked，`reason` 指 `git_diff_summary`。

---

## 3. IDE — 复用 `mcp_status`

IDE 集成在 mossen 内部本来就是以"特殊 MCP server"形式接入的（`src/utils/ide.ts:getIdeClientName` 通过 server.config 标记识别）。再设计独立 IDE 协议会重复 `mcp_status` 已经返回的 `mcpServers[].config / status / capabilities`。

**因此**：
- 不新增 `ide_status` subtype。
- `slash.ide` 仍 blocked，manifest reason 指 `mcp_status`。
- Workbench 用 `mcp_status` 拿全 server 列表，按既有惯例（server name / config 标记）筛 IDE entry，渲染 IDE 状态。

---

## 4. SDKControlRequestInner union 增量

| 序号 | subtype | request schema | response shape | 状态 |
|:-:|---|---|---|---|
| 23 | `compact_conversation` | `SDKControlCompactConversationRequestSchema` | `SDKControlCompactConversationResponseSchema` | always blocked |
| 24 | `get_config_summary` | `SDKControlGetConfigSummaryRequestSchema` | `SDKControlGetConfigSummaryResponseSchema` | available |
| 25 | `runtime_doctor_summary` | `SDKControlRuntimeDoctorSummaryRequestSchema` | `SDKControlRuntimeDoctorSummaryResponseSchema` | available |
| 26 | `git_diff_summary` | `SDKControlGitDiffSummaryRequestSchema` | `SDKControlGitDiffSummaryResponseSchema` | available（非 git repo 返回 `not_git_repo`，不算 fail） |

union 总数 22 → **26**（W46）→ **29**（W47）；whitelist Section B 同步、stream_json_contract_smoke `29 control` 标签同步。

---

## 5. Slash manifest 联动（无回归）

`/capabilities` 的 manifestVersion 仍是 **2**；slash entries 数量仍是 **23 canonical + 4 alias**；本轮没有新增 slash entry。修改的只是 5 个已有 blocked entry 的 `reason` 字段，让它们各自指向 dedicated control_request subtype（IDE 指 `mcp_status`，其他 4 个指本轮新增 subtype）。

| slash entry | manifest reason 指向 |
|---|---|
| `slash.compact` | `compact_conversation` |
| `slash.config` (alias `settings`) | `get_config_summary` 或 `get_settings` |
| `slash.doctor` | `runtime_doctor_summary` |
| `slash.diff` | `git_diff_summary` |
| `slash.ide` | `mcp_status` |

W43/W45 smoke 仍 PASS（reason 字段格式没变）。

---

## 6. Workbench 消费指引

1. **见 manifest reason 即用 dedicated subtype**：看到 `slash.X` 是 blocked 时，从 `reason` 字段中提取 `control_request subtype "Y"` 模式，然后改用对应 `control_request`。
2. **secret-safe 协议永不读 value**：`get_config_summary` 不回 value；如需 raw 一定走 `get_settings` 并自行 redact。
3. **subprocess-bound 协议预期 timeout**：`git_diff_summary` 5s timeout 是契约，client 必须 tolerate 5s+ 等待与 `status:"unavailable"` 兜底。
4. **doctor checks 是稳定 id 集合**：UI 按 `id` 渲染图标/排序，不要 hard-code `summary` 文本（文本是人话，可能本地化）。
5. **compact 永远 blocked，不可绕开**：`compact_conversation` 当前 100% 返回 `status:"blocked"`，UI 应禁用按钮、显示 reason；待后续 wave 拿到 `status:"completed"` 才开口。

---

## 7. 不在本轮（明确未来工作）

- `compact_conversation` 真正实现：需要 idle main-loop orchestrator + ToolUseContext snapshot 协议；要单独施工包。
- `get_config_summary` 加 `--source` 过滤、`--keys-only` 模式：先扩 schema、再扩 dispatcher。
- `runtime_doctor_summary` 加 `network` / `version` / `auth` 检查项：必须先做"安全 network probe + auth-check 沙箱"设计，本轮拒绝。
- `git_diff_summary` 加 patch 输出：必须先设计 patch size cap + 二进制文件处理 + 安全转义；目前明确不返回 patch。
- `init` / `login` / `logout`：mutation+credential，需要专用的 confirmation 协议（不要复用 `slash_command`，也不要塞进本批 4 个 subtype）。

---

## 8. 验证清单

```bash
python3 scripts/stream_json_contract_smoke.py
python3 scripts/wave_w29b_slash_command_smoke.py
python3 scripts/wave_w43_slash_capability_manifest_smoke.py
python3 scripts/wave_w45_capability_protocol_matrix_smoke.py
python3 scripts/wave_w46_high_value_protocols_smoke.py
bash scripts/run_all_smoke.sh
```

全部 PASS 才算契约不破。`run_all_smoke.sh` 已接入 W46。
