# Mossen Stream-JSON Real Capability Operations (W47)

> **目的**: 推进 Workbench 端可消费的"真能力操作"协议层。本轮 (a) 真实落地 2 项扩展能力，(b) 用 schema-locked blocked-with-reason 占住 2 个高风险 mutation 协议表面，(c) 明确 STOP 不能安全实现的项并写下原因。
> **范围**: stream-json `control_request` 层。不改 TUI 命令本体、不改主循环、不写配置/插件/MCP/auth。
> **状态**: 3 个新 control_request subtype + 2 个现有 subtype 字段扩展；union 26 → 29。
> **配套**:
> - `entrypoints/sdk/controlSchemas.ts` — 3 新 schema + 2 扩
> - `cli/print.ts` — 3 新 dispatcher 分支 + 2 扩
> - `scripts/stream-json-schema-whitelist.txt` Section B — 29 成员
> - `scripts/wave_w47_real_capability_operations_smoke.py` — 契约 lock
> - `services/slashCommandCapabilities.ts` — slash blocked entries 不变
> - 上一波文档：`docs/W_MAIN_HIGH_VALUE_CONTROL_PROTOCOLS.md`（W46，4 个 dedicated subtype）

---

## 1. 决定矩阵

| 能力 | 本轮决定 | 实施方式 |
|---|---|---|
| **A. compact 真执行** | ❌ **STOP** | 缺 `ToolUseContext` + `cacheSafeParams` + 主循环编排；伪造任一项都违反"100% real or not done"。`compact_conversation` 继续无条件返回 `status:"blocked"`。 |
| **B. config 真写** | ⚠️ **SCHEMA + BLOCKED** | 新增 `apply_config_change` subtype；dispatcher 永远返回 `status:"blocked"` + 稳定 reason；不调用 `updateSettingsForSource`。 |
| **C. plugin/skill/MCP mutation 路由发现** | ✅ **REAL** | 新增 `get_capability_operations` discovery 端点，列出每个 (capabilityId, operation) → existing safe executor 子类型。本身**不**执行任何 mutation。 |
| **D. project memory / init 写入** | ⚠️ **SCHEMA + BLOCKED** | 新增 `project_memory_operation` subtype；dispatcher 永远返回 `status:"blocked"` + 稳定 reason；不写 CLAUDE.md / memory 文件。 |
| **E. diff patch preview** | ✅ **REAL** | 扩 `git_diff_summary` 加 `includePatch` 选项；100KB 字节 cap、20 文件 cap、`git diff --numstat` 二进制检测、5s timeout 与 no-patch path 共用。 |
| **F. doctor optional probes** | ⚠️ **SCHEMA + DEFERRED** | 扩 `runtime_doctor_summary` 加 `includeNetworkProbes` 选项；启用时返回 `probe_disabled_in_this_build` 占位 check，**永不**做真实 network/version probe。 |
| **G. auth/login/logout** | ❌ **OUT OF SCOPE** | 不新增任何 auth mutation 协议。slash.login / slash.logout 仍 blocked。w47 smoke `check_no_auth_path` 锁住任何 `subtype === 'login'/'logout'/'auth_*'/'apply_credential'/'set_credential'` 的引入。 |

---

## 2. 协议详细 shape

### 2.1 `apply_config_change` — schema-locked blocked

**Request**:
```json
{
  "type": "control_request",
  "request_id": "<uuid>",
  "request": {
    "subtype": "apply_config_change",
    "source": "userSettings|projectSettings|localSettings",
    "changes": { "<key>": "<value>" },
    "dryRun": true,
    "confirm": false
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
      "reason": "apply_config_change: requires settings key allowlist + secret blacklist + per-source validation + backup/rollback subsystem (deferred)",
      "summary": "apply_config_change is blocked in this build"
    }
  }
}
```

**Why blocked now**: real writer `updateSettingsForSource()` accepts arbitrary `SettingsJson`. Safe exposure needs (a) per-key allowlist enforced server-side, (b) secret/key blacklist (e.g., `apiKeyHelper`, `baseURL`, `bearer`, `Authorization`), (c) per-source validation (e.g., `policySettings` write must be rejected), (d) backup/rollback. Until that subsystem ships, the schema serves UI lock-in only.

**Future shape (when implemented)**: `status` may become `preview` or `applied` and `changedKeys` will be populated. Current dispatcher never returns those.

**Slash relationship**: `slash.config` (`settings` alias) stays blocked; manifest reason already mentions both `get_config_summary` (redacted read) and `get_settings` (full read).

### 2.2 `get_capability_operations` — discovery routing map

**Request**:
```json
{
  "type": "control_request",
  "request_id": "<uuid>",
  "request": { "subtype": "get_capability_operations" }
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
      "summary": "<N> capability operations: <A> available, <B> blocked.",
      "operations": [
        {
          "capabilityId": "plugins",
          "operation": "reload",
          "status": "available",
          "executor": "reload_plugins",
          "requiresConfirmation": false,
          "dryRunSupported": false,
          "summary": "Refresh plugin/agent/skill caches and re-register hooks."
        }
      ]
    }
  }
}
```

**全部 routing entries (14 total)**:

| capabilityId | operation | status | executor | confirm |
|---|---|---|---|---|
| plugins | reload | available | `reload_plugins` | no |
| plugins | install | available | `capability_recommendation_response` | yes |
| plugins | enable_disable | blocked | none | yes |
| mcp | set_servers | available | `mcp_set_servers` | no |
| mcp | reconnect | available | `mcp_reconnect` | no |
| mcp | toggle | available | `mcp_toggle` | no |
| skills | reload | available | `reload_plugins` | no |
| skills | install | blocked | none | yes |
| config | apply | blocked | `apply_config_change` | yes |
| config | apply_flag | available | `apply_flag_settings` | no |
| project_memory | init_or_update | blocked | `project_memory_operation` | yes |
| compact | manual | blocked | `compact_conversation` | no |
| auth | login | blocked | none | yes |
| auth | logout | blocked | none | yes |

**Critical contract**: this dispatcher branch **never executes anything**. It only returns the routing map. Workbench is expected to call `executor` subtype directly to actually run the operation. w47 smoke locks the no-execution invariant: `installPlugin(`/`spawn(`/`execSync(`/`writeFileSyncAndFlush`/`updateSettingsForSource(` all forbidden inside this branch.

### 2.3 `project_memory_operation` — schema-locked blocked

**Request**:
```json
{
  "type": "control_request",
  "request_id": "<uuid>",
  "request": {
    "subtype": "project_memory_operation",
    "operation": "preview_init|apply_init|preview_memory_update|apply_memory_update",
    "path": "<relative path>",
    "content": "<new content>",
    "dryRun": true,
    "confirm": false
  }
}
```

**Response (always blocked)**:
```json
{
  "status": "blocked",
  "reason": "project_memory_operation:<op> requires path-traversal guard + project-root sandbox + backup/previousHash + diff preview + confirm-token UX (deferred)",
  "summary": "project_memory_operation operation=\"<op>\" is blocked in this build"
}
```

**Why blocked now**: writes to `CLAUDE.md` and `.mossen/rules/*.md` need (a) path-traversal guard (`..` rejection, absolute path rejection unless inside project root), (b) project-root sandbox (must be inside resolved cwd), (c) backup or `previousHash` so client can detect concurrent edit, (d) diff preview (already a subset of `git_diff_summary`), (e) confirm-token UX (one-shot token from preview to apply). Implementing any of these in isolation is unsafe.

**Slash relationship**: `slash.init` and the (not-yet-existent) `slash.memory_write` would point here.

### 2.4 `git_diff_summary` 扩展：`includePatch`

**Request** (扩):
```json
{
  "subtype": "git_diff_summary",
  "includeUntracked": false,
  "includePatch": true     // W47 new
}
```

**Response** (扩):
```json
{
  "status": "ok",
  "summary": "...",
  "cwd": "...",
  "files": [...],
  "stats": {...},
  "truncated": false,
  "patch": {
    "included": true,
    "truncated": false,
    "totalBytes": 4231,
    "fileCount": 7,
    "binaryFiles": ["assets/logo.png"],
    "skippedFiles": [],
    "text": "diff --git a/... b/...\n@@ -1,1 +1,2 @@\n..."
  }
}
```

**Constraints (locked by w47 smoke)**:
- `PATCH_BYTE_CAP = 100 * 1024` — total `text` field hard cap; truncation is by codepoint slice (no multibyte split).
- `PATCH_FILE_CAP = 20` — files beyond this end up in `skippedFiles`.
- `git diff --numstat` runs first to detect binary (`-`/`-` markers); binary files end up in `binaryFiles`, never in patch text.
- 5s subprocess timeout (shared with no-patch path).
- `truncated` flag set if either byte cap hit OR `skippedFiles.length > 0`.
- 子命令仍受 W46 已有白名单约束：`rev-parse` / `status` / `diff` 三个读命令；W47 扩展不引入新写命令。

**When `includePatch` is omitted/false**: response shape is identical to W46 — no `patch` field. W47 smoke locks: `git diff` patch text never appears unless `includePatch: true`.

### 2.5 `runtime_doctor_summary` 扩展：`includeNetworkProbes`

**Request** (扩):
```json
{
  "subtype": "runtime_doctor_summary",
  "includeNetworkProbes": true   // W47 new
}
```

**Response when `includeNetworkProbes: true`** — same checks as W46 plus:
```json
{
  "checks": [
    /* W46 checks: cwd, session, model, permission_mode, mcp, memory, hooks */
    {
      "id": "network_probe",
      "title": "Backend reachability",
      "status": "unavailable",
      "severity": "info",
      "summary": "probe_disabled_in_this_build — real network probes require dedicated sandbox design (deferred)"
    },
    {
      "id": "version_probe",
      "title": "Latest CLI version",
      "status": "unavailable",
      "severity": "info",
      "summary": "probe_disabled_in_this_build — version probe deferred"
    }
  ]
}
```

**Why deferred not real**: real network probe needs (a) timeout per probe, (b) DNS leak guard for offline users, (c) endpoint allowlist, (d) PII-free error capture. The `version_probe` would compare local CLI version against a release feed — that's a network call too. Implementing safely needs a probe sandbox; out of W47 scope.

w47 smoke locks: forbidden anchors `fetch(` / `https://` / `http://` / `spawn(` / `execSync(` inside this branch.

---

## 3. STOP 项的诚实文档

### 3.1 compact_conversation — STOP

**Why**: `compactConversation()` (services/compact/compact.ts:364) requires:
- `messages: Message[]` — accessible from dispatcher (existing `mutableMessages` reference works)
- **`context: ToolUseContext`** — has `setSDKStatus`/`setStreamMode`/`setResponseLength`/`onCompactProgress` callbacks that wire into the live `query()` stream renderer; abort controller is the active turn's controller; getAppState/setAppState are the live state; `executePreCompactHooks` runs against the active hook scheduler
- **`cacheSafeParams: CacheSafeParams`** — captures the most-recent prompt cache parameters from `ask()`; only valid mid-turn
- side effects: API call (LLM request), pre/post-compact hooks, possible `setStreamMode('requesting')` → renders to terminal
- **Concurrency**: control_request handler runs on stdin event loop while `query()` may be active; mutating `mutableMessages` mid-turn would race with main loop

**To safely implement**: requires either
- (a) a dedicated idle-only orchestrator that waits for `query()` to complete, snapshots state, runs compaction, swaps result back — needs a new state machine
- (b) refactoring `compactConversation` to accept a "stream-json mode" without TUI callbacks — needs auditing every callback site

Neither is a stream-json wrapper concern; they require main-loop changes. Out of scope per Allen's red lines.

### 3.2 plugin/skill/MCP mutation unified executor — STOP

**Why**: existing `reload_plugins` / `mcp_set_servers` / `mcp_reconnect` / `mcp_toggle` / `capability_recommendation_response` already cover the safe operations. Adding a unified executor would (a) duplicate dispatch logic, (b) introduce another error path. The discovery endpoint `get_capability_operations` is the right level of abstraction here.

### 3.3 init / memory write — STOP

**Why**: `commands/init.ts` is a TUI flow with a multi-step React panel. There is no headless writer. memory file writes go through the TUI editor command. Implementing safely needs the full UX redesign (preview + diff + confirm token).

### 3.4 auth / login / logout — STOP (Allen explicit)

Out of scope. No auth mutation protocol is added in this wave.

---

## 4. SDKControlRequestInner union 增量

| 序号 | subtype | 状态 | smoke |
|:-:|---|---|---|
| 27 | `apply_config_change` | always blocked | w47 |
| 28 | `get_capability_operations` | available | w47 |
| 29 | `project_memory_operation` | always blocked | w47 |

union 总数 26 → **29**；whitelist Section B、stream_json_contract_smoke `29 control` 同步。

---

## 5. Slash manifest 联动（无回归）

W47 不改 slash manifest entries。所有 5 个 blocked entries (`slash.compact` / `slash.config` / `slash.doctor` / `slash.diff` / `slash.ide`) 的 `reason` 字段在 W46 已经指向了 dedicated subtype。`slash.login` / `slash.logout` 仍 blocked，未触动。w47 smoke 显式断言这 7 个 slash entries 仍 blocked。

---

## 6. Workbench 消费指引

1. **Mutation 入口规则**：永远先调 `get_capability_operations`，按 `executor` 字段选 subtype。**禁止**把任何 mutation 直接塞 `slash_command`。
2. **blocked subtype 渲染**：`apply_config_change` / `project_memory_operation` / `compact_conversation` 都返回 `status:"blocked"` 而**不**是 control_response error。这是约定的"future shape lock"——UI 应展示 `reason` 字符串而不报错。
3. **patch preview 默认 off**：`git_diff_summary` 不带 `includePatch` 时 `patch` 字段不出现。带 `includePatch:true` 后 client 必须 tolerate `truncated`、`binaryFiles`、`skippedFiles`、`totalBytes` 信号。
4. **doctor probe 默认 off**：`runtime_doctor_summary` 不带 `includeNetworkProbes` 时 7 个 W46 checks；带了则追加 `network_probe` + `version_probe` placeholder（status `unavailable`）。本轮不应根据 placeholder 做任何业务决策。
5. **discovery 是真协议**：`get_capability_operations.operations[]` 是 server-side truth；UI 渲染按 `status` (`available`/`blocked`/`unavailable`)、按 `requiresConfirmation` 决定 confirm flow，按 `dryRunSupported` 决定 preview UX。

---

## 7. 验证清单

```bash
python3 scripts/stream_json_contract_smoke.py
python3 scripts/wave_w29b_slash_command_smoke.py
python3 scripts/wave_w43_slash_capability_manifest_smoke.py
python3 scripts/wave_w45_capability_protocol_matrix_smoke.py
python3 scripts/wave_w46_high_value_protocols_smoke.py
python3 scripts/wave_w47_real_capability_operations_smoke.py
bash scripts/run_all_smoke.sh
```

W47 全 9 个 slash/control smoke 全部 PASS 才算契约不破。`run_all_smoke.sh` 已接入。

---

## 8. 后续工作（不在本轮）

- `compact_conversation` 真实现：需要 idle-`ToolUseContext` orchestrator + cacheSafeParams snapshot 协议；独立施工包。
- `apply_config_change` 真实现：需要 settings key allowlist (服务端硬编码)、secret blacklist、per-source validation、backup/rollback；独立施工包。
- `project_memory_operation` 真实现：需要 path-traversal guard + project-root sandbox + diff preview + confirm-token UX；独立施工包。
- `runtime_doctor_summary` 真 network/version probe：需要 probe sandbox（timeout/DNS guard/endpoint allowlist/PII-free errors）；独立施工包。
- auth / login / logout：必须设计独立 auth mutation 协议，本轮明确不做。
