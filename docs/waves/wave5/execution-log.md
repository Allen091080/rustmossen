# Wave5 执行日志

> 仓库: `/Users/allen/Documents/aiproject/mossensrc`
> Wave5 worktree: `/Users/allen/Documents/aiproject/mossensrc-wave5`
> Wave5 branch: `worktree/wave5`
> 基线 main HEAD: `945609a1c4c0fd2d36fe5e37cdadbb29e1e1c84a`
> 配套施工包: `/Users/allen/Desktop/mossen升级/07-源码精读与品牌审计/审计结果/wave5-prep/` (5 份 markdown)

---

## Phase 0: 只读审计 (2026-04-29)

**完成**: 4 SA 子 agent 全完成 (USER_TYPE / R7 / BRIDGE+R8.3 / 验证矩阵), Desktop wave5-prep/ 5 份 markdown 产出。
**实测数字**: USER_TYPE 319 hits / 144 files; discoveredSkillNames 13 hits / 6 src files; BRIDGE_MODE 9 hits / 5 src files。
**守纪**: 0 源码 / 0 commit / 0 push / 0 worktree 创建。

---

## Phase 1 — R7 discoveredSkillNames 死代码清理 (2026-04-29)

### 触发条件
- Wave 4 R7 NEEDS-DESIGN 转推 (`docs/waves/wave4/execution-log.md:57`)
- Wave 3 R7 决策 D-S3-N1=B "KEEP discoveredSkillNames" 的转推 (`docs/wave3-execution-log.md:183, 240`)
- Allen 拍板启动 Phase 1 R7 单 slice (P1)

### 受保护边界
本 slice 触 2 处 HIGH 受保护边界 (memory `feedback_skill_subagent_systems_protected.md`):
1. `Tool.ts:225` — runtime API interface 字段 (optional, 删除不 break TS 编译, 但运行期若 code 不做 optional chaining 会 throw)
2. `utils/forkedAgent.ts:386` — sub-agent context fork (silent failure 风险高)

### 真死代码证据 (worktree 内重测)
- 6 目标文件 `discoveredSkillNames` 命中: 13 hits ✓ (与 Phase 0 一致)
- `discoveredSkillNames\.(add|has|size|forEach|map|filter|values|keys|entries)|for .*discoveredSkillNames|\[\.\.\.discoveredSkillNames\]` 命中: **0 hits** ✓
- 仅 useRef placeholder + pass-through + `.clear()`, **0 真用调用**

### 改动清单 (5 source files / -25 LOC)

| 文件 | 行号 | 改动 | 净变化 |
|------|:----:|------|:----:|
| `Tool.ts` | :224-225 | 删 jsdoc 注释 + interface 字段定义 | -2 |
| `QueryEngine.ts` | :192-197 | 删 5 行注释块 + class field `private discoveredSkillNames = new Set<string>()` | -6 |
| `QueryEngine.ts` | :238 | 删 `this.discoveredSkillNames.clear()` | -1 |
| `QueryEngine.ts` | :373 | 删 pass-through `discoveredSkillNames: this.discoveredSkillNames,` | -1 |
| `QueryEngine.ts` | :521 | 删 pass-through `discoveredSkillNames: this.discoveredSkillNames,` | -1 |
| `screens/REPL.tsx` | :1957-1962 | 删 5 行注释 + `const discoveredSkillNamesRef = useRef(new Set<string>());` | -6 |
| `screens/REPL.tsx` | :2430 | 删 pass-through (getToolUseContext) | -1 |
| `screens/REPL.tsx` | :2990 | 删 pass-through (clearConversation 第 1 处调用) | -1 |
| `screens/REPL.tsx` | :4713 | 删 pass-through (clearConversation 第 2 处调用) | -1 |
| `utils/forkedAgent.ts` | :385-386 | 删 sub-agent 字段 + jsdoc 注释 | -2 |
| `commands/clear/conversation.ts` | :52 | 删 destructure param `discoveredSkillNames,` | -1 |
| `commands/clear/conversation.ts` | :60 | 删 type annotation `discoveredSkillNames?: Set<string>` | -1 |
| `commands/clear/conversation.ts` | :131 | 删 `.clear()` 调用 | -1 |
| **总计** | | | **-25** |

### 验证结果 (10 项, 全 PASS)

| # | 验证项 | 命令 | 结果 |
|:-:|------|------|:----:|
| 1 | 6 目标文件 grep `discoveredSkillNames` | `rg -n "discoveredSkillNames" Tool.ts QueryEngine.ts screens/REPL.tsx utils/forkedAgent.ts commands/clear/conversation.ts` | **0 hits** ✓ |
| 2 | 全仓 source grep `discoveredSkillNames` | `rg -n "discoveredSkillNames" --glob '*.{ts,tsx,js,jsx}' --glob '!node_modules/**'` | **0 hits** ✓ |
| 3 | typecheck:diff | `bun run typecheck:diff` | baseline 1384 / current 1357 / fixed 27 / **0 NEW** ✓ |
| 4 | lint:diff | `bun run lint:diff` | baseline 943 / current 938 / fixed 5 / **0 NEW** ✓ |
| 5 | layer_boundary_audit | `python3 scripts/layer_boundary_audit.py` | **PASS** (1940 files / 14192 imports / 4 rules) ✓ |
| 6 | stream_json_contract_smoke | `python3 scripts/stream_json_contract_smoke.py` | **PASS** (24 SDKMessage + 21 control + 8 stdout + 5 stdin + anchors) ✓ — R7 不触 schema, 不触 only-additive 红线 |
| 7 | run_all_smoke.sh --dry-run | 列 20 执行单元 | **PASS** ✓ |
| 8 | run_all_smoke.sh (实跑) | 20 执行单元 | **ALL PASS** ✓ |
| 9 | case 39 fingerprint | step 20 | **`870f99ed494d3d145ed2eb1368132299` 稳定** ✓ |
| 10 | git diff --check / --stat | | 无空白错 / 5 files / -25 lines ✓ |

### 受保护边界处理结论
- ✅ Tool.ts:225 删 interface field — TS 编译 0 NEW typecheck error 验证通过
- ✅ utils/forkedAgent.ts:386 删 sub-agent context field — 0 下游 `.add()`/`.has()`/iteration 调用证据充分; layer boundary audit + stream-json contract + 全 20 smoke PASS 验证 sub-agent 链路无断裂

### Phase 1 守纪声明 (R7 实施)

| 项 | 状态 |
|------|------|
| 0 USER_TYPE 文件改动 | ✅ |
| 0 BRIDGE_MODE / feature flag 文件改动 | ✅ |
| 0 Workbench / CWB 文档脚本改动 | ✅ |
| 0 `scripts/smoke_check.py` 改动 | ✅ |
| 0 `utils/i18n/strings.{en,zh}.ts` 改动 | ✅ |
| 0 `commands/insights.ts` 改动 (Allen WIP) | ✅ |
| 0 `package.json` / `bun.lock` / `bunfig.toml` 改动 | ✅ |
| 0 permission / analytics / model provider 改动 | ✅ |
| 0 git push / merge / tag / rebase / reset / stash / `checkout HEAD --` | ✅ |
| 0 `git add .` / `git add -A` (commit 仅显式 add 6 文件) | ✅ |
| 0 写文件子 agent 派出 (本 slice 主 agent 直接 Edit) | ✅ |

### Worktree 环境备注
- wave5 worktree 创建后 `bun install --frozen-lockfile` 装 dependencies, 但 devDependencies (`@types/*`, `bun-types`) 未装, typecheck 输出异常
- 解决: `node_modules` symlink 到主仓 (`ln -s /Users/allen/Documents/aiproject/mossensrc/node_modules /Users/allen/Documents/aiproject/mossensrc-wave5/node_modules`)
- node_modules 在 .gitignore 内, 不影响 git 状态

---

## Phase 2 — BRIDGE_MODE known debt cleanup + feature-flags env example (2026-04-29)

### 触发条件
- Allen 拍板 D-W5-5 = A 直接删 `bridge=` 诊断输出 + D-W5-6 = A 同 commit 改 KNOWN_DEBT
- Wave 4 R8 转推 (`docs/waves/wave4/execution-log.md:58`) — Bridge 子系统 Wave 1.5 删除尾巴
- R7 已合 main `10baf6f`，Phase 2 在同一 wave5 worktree 续做

### 实施前 baseline
- main HEAD: `10baf6f3da75b6f9f5f6c1155191e5ac1e3128fa`
- wave5 worktree HEAD (Phase 2 起点): `10baf6f` (ff-only synced)
- BRIDGE_MODE 全仓命中: 13 hits (含 9 命中 + 4 docs/log) — 其中 `scripts/smoke_check.py:29178/29237` 系统红线**不动**

### case 39 fingerprint 安全性分析
- case 39 = `custom_backend_auth_runtime_audit` (smoke_check.py:9507-9572)
- case 39 函数体 0 处 bridgeMode 引用 ✓
- case 39 result 字段集 9 字段全与 BRIDGE_MODE 无关
- `scripts/smoke_check.py:29178, 29237` 在 capability matrix 区段（不被 case 39 触发）
- 删除 `runtimeTypes.bridgeMode` 后 `feature_gates.get("bridgeMode")` 返回 None — Python 安全默认
- 结论: **case 39 fingerprint 漂移概率 0**

### Commit 1 改动清单 (BRIDGE_MODE 清理)

| # | 文件 | 行 | 改动 |
|---|------|:----:|------|
| 1 | `platform/featureGatesRuntime.ts` | :39 | 删 `bridgeMode: resolve('BRIDGE_MODE'),` |
| 2 | `platform/runtimeTypes.ts` | :246 | 删 `bridgeMode: boolean` |
| 3 | `cli/handlers/auth.ts` | :427 | 删 `bridge=${...bridgeMode...},` 段（保留前后字符串模板） |
| 4 | `scripts/platform_check.ts` | :168 | 删 `typeof snapshot.featureGates.bridgeMode === 'boolean' &&` |
| 5 | `utils/config.ts` | :550 | 注释 `(requires BRIDGE_MODE)` 删除 |
| 6 | `scripts/feature_audit.py` | :63-67 | 删 `bridge-mode` profile（5 行） |
| 7 | `scripts/feature_audit.py` | :81-85 | `daemon-bridge` 改名 `daemon-only`, features 改 `["DAEMON"]` |
| 8 | `scripts/wave4_r8_feature_flag_smoke.py` | :10/:42/:44 | 注释精简 + `KNOWN_DEBT_RESOLVE_ORPHANS` → `frozenset()` |
| 9 | `启动说明.md` | :246 | env doc 删 `,BRIDGE_MODE` |
| 10 | `docs/design/bun-feature-flag-system.md` | §5.2.1 + §178 | 标 Wave 5 Phase 2 已删，保留历史段 |
| 11 | `docs/reference/smoke-runner.md` | §3 #17 + §94 + §104 | 标 known debt 已清空 |
| 12 | `docs/waves/wave4/execution-log.md` | §58 + §69 | 转推项标 Wave 5 Phase 2 已完成 |
| 13 | `docs/waves/wave5/execution-log.md` | 本段 | 加 Phase 2 段 |

### 系统红线持守
- ✅ 不动 `scripts/smoke_check.py` (含 :29178, :29237 两处 `bridgeMode` 引用) — 系统红线
- ✅ 不动 `bunfig.toml`
- ✅ 不动 `utils/i18n/strings.{en,zh}.ts`
- ✅ 不动 `commands/insights.ts`
- ✅ 不动 R7 已删字段（Tool.ts / QueryEngine.ts / 等）
- ✅ 不动任何 USER_TYPE 文件
- ✅ 不动任何 Workbench / CWB 文件
- ✅ 0 push / 0 tag / 0 rebase / 0 reset / 0 stash / 0 force

### Commit 2 改动清单 (`docs(wave5): add feature flags env example`, 3 files / +131 / -2)

| # | 文件 | 改动 |
|---|------|------|
| 1 | `.mossensrc/feature-flags.env.example` | 新增 129 行: 79 token 分组速查 + Mossen 个人版推荐 export (DIRECT_CONNECT, SSH_REMOTE, TRANSCRIPT_CLASSIFIER, KAIROS, VOICE_MODE, TEAMMEM) + 安全 `${VAR:-default}` 语法变体 + 已废弃段标注 BRIDGE_MODE |
| 2 | `.gitignore` | `.mossensrc/` → `.mossensrc/*` (修复 ignored parent 阻止 negation) + 加白名单例外 `!.mossensrc/feature-flags.env.example`. 真实 `.mossensrc/feature-flags.env` 仍 ignored |
| 3 | `docs/design/bun-feature-flag-system.md` §5.3 | 标 `.example` 模板已落地 |

### Phase 2 收口验证 (8 项, 全 PASS)

| # | 项 | 结果 |
|:-:|------|:----:|
| 1 | rg `BRIDGE_MODE\|bridgeMode` 排除 docs/wave/r8 注释 | **0 source hits** ✓ |
| 2 | `python3 scripts/wave4_r8_feature_flag_smoke.py` | **PASS** (resolve 8 / orphans none / **known debt = (none)**) ✓ |
| 3 | `bun run typecheck:diff` | baseline 1384 / current 1357 / **0 NEW** ✓ |
| 4 | `bun run lint:diff` | baseline 943 / current 938 / **0 NEW** ✓ |
| 5 | `python3 scripts/layer_boundary_audit.py` | **PASS** (1940 files / 14192 imports / 4 rules) ✓ |
| 6 | `python3 scripts/stream_json_contract_smoke.py` | **PASS** (24 SDKMessage + 21 control + 8 stdout + 5 stdin) ✓ |
| 7 | `./scripts/run_all_smoke.sh` | **ALL PASS** (20 执行单元) ✓ |
| 8 | case 39 fingerprint | `870f99ed494d3d145ed2eb1368132299` **稳定** ✓ |

### Phase 2 守纪声明

| 项 | 状态 |
|------|------|
| 0 push / 0 tag / 0 merge / 0 rebase / 0 reset / 0 stash / 0 force | ✅ |
| 0 worktree 删除 / 不用 `git add .` / 显式 add 文件 | ✅ |
| 0 `scripts/smoke_check.py` 改动 | ✅ |
| 0 `bunfig.toml` 改动 | ✅ |
| 0 `utils/i18n/strings.{en,zh}.ts` 改动 | ✅ |
| 0 USER_TYPE / Workbench 文件改动 | ✅ |
| 0 R7 已收束文件改动 (Tool.ts / QueryEngine.ts / REPL.tsx / forkedAgent.ts / commands/clear/conversation.ts) | ✅ |
| 0 `commands/insights.ts` 改动 (Allen WIP) | ✅ |
| 0 `package.json` / `bun.lock` 改动 | ✅ |
| 0 子 agent 写源码 (本 Phase 主 agent 直接 Edit) | ✅ |

### Phase 2 commit 链

| commit | message | 文件 / 净行 |
|--------|---------|------------|
| `6cd78c9` | `refactor(wave5): remove BRIDGE_MODE feature flag debt` | 12 / +74 / -40 |
| `b85911e` | `docs(wave5): add feature flags env example` | 3 / +131 / -2 |

---

*— Wave5 execution log v1.2 / Phase 2 BRIDGE_MODE + env.example 完成 (2 commits)*

---

## Phase 3 — R2.1 yoloClassifier USER_TYPE 收敛 (2026-04-29)

### 触发条件
- Allen 拍板 D-W5-P3-1=A (R1.5 推 Wave6+) + D-W5-P3-2=A (Phase 3 改做 R2.1)
- Allen 拍板 D-W5-P3-R2.1-A=A (改 6 处) + B=α (line 71 保留 inline) + C=B (不新增专项 smoke) + D=A (commit message)
- Phase 2 已合 main `0a4481b`, wave5 worktree ff-only 同步至 `0a4481b`

### 实施前 baseline
- main HEAD: `0a4481bfd3ff7c8f85b3d2cabccd8273a5f1c08f`
- wave5 worktree HEAD: `0a4481b` (ff-only synced)
- yoloClassifier 文件总行数: 1496 行 (改后 1497, +1 import)

### 改动清单 (1 文件 / +7 -6)

| # | 行 (改后) | 改动 | 类别 |
|:-:|:----:|------|------|
| 1 | :37 | 新增 `import { getUserType } from '../sessionStorage.js'` | import |
| 2 | :78 (原 77) | `process.env.USER_TYPE !== 'ant'` → `getUserType() !== 'ant'` (isUsingExternalPermissions) | runtime A |
| 3 | :165 (原 164) | `process.env.USER_TYPE !== 'ant'` → `getUserType() !== 'ant'` (maybeDumpAutoMode) | runtime A |
| 4 | :689 (原 688) | `process.env.USER_TYPE === 'ant' &&` → `getUserType() === 'ant' &&` (getClassifierThinkingConfig) | runtime A |
| 5 | :1337 (原 1336) | `process.env.USER_TYPE === 'ant'` → `getUserType() === 'ant'` (getClassifierModel) | runtime A |
| 6 | :1360 (原 1359) | `process.env.USER_TYPE === 'ant'` → `getUserType() === 'ant'` (resolveTwoStageClassifier) | runtime A |
| 7 | :1382 (原 1381) | `process.env.USER_TYPE === 'ant'` → `getUserType() === 'ant'` (isJsonlTranscriptEnabled) | runtime A |

### Line 71 (原, 改后 :72) 保留原因

`feature('TRANSCRIPT_CLASSIFIER') && process.env.USER_TYPE === 'ant' ? txtRequire(require('./yolo-classifier-prompts/permissions_mossen.txt')) : ''`

- 顶层 const + DCE 链 (51-53 行注释 "Dead code elimination: conditional imports for auto mode classifier prompts. At build time, the bundler inlines .txt files as string literals.")
- 加 `eslint-disable custom-rules/no-process-env-top-level` 显式 DCE 标记
- 与 `constants/prompts.ts:713` / `utils/undercover.ts:18` 同性质红线
- D-W5-P3-R2.1-B=α 拍板: 保留 inline, 不替换为 `getUserType()`
- 推 Wave6+ 单独立 slice 处置 (含 build bundle audit + .txt bundling 验证)

### 验证结果 (8 项, 全 PASS)

| # | 项 | 命令 | 结果 |
|:-:|------|------|:----:|
| 1 | grep yoloClassifier USER_TYPE | `rg -n "process\.env\.USER_TYPE\|getUserType\(\)" utils/permissions/yoloClassifier.ts` | line 72 保留 inline ✓ + 6 处 runtime 替换 ✓ |
| 2 | wave0_perm1 dangerous patterns | `python3 scripts/wave0_perm1_dangerous_patterns_smoke.py` | **PASS** (1/1) ✓ |
| 3 | wave0_perm2 overly broad | `python3 scripts/wave0_perm2_overly_broad_smoke.py` | **PASS** (1/1) ✓ |
| 4 | wave2_a5 cmd allowlist | `python3 scripts/wave2_a5_cmd_allowlist_smoke.py` | **PASS** (6/6) ✓ |
| 5 | harness_R7 no_remote_gb_traffic | `python3 scripts/harness_R7_no_remote_growthbook_traffic_smoke.py` | **PASS** (2/2) ✓ (注: 首跑 default case 失败因 wave5 worktree 缺 per-developer `.mossensrc/custom-backend.env`, symlink 主仓后 PASS, 与 R2.1 无关) |
| 6 | wave4_r8_feature_flag | `python3 scripts/wave4_r8_feature_flag_smoke.py` | **PASS** (resolve 8 / known debt none) ✓ |
| 7 | layer_boundary_audit | `python3 scripts/layer_boundary_audit.py` | **PASS** (1940 files / 14193 imports / 4 rules) — imports +1 vs Phase 2 14192, 即新加 sessionStorage import ✓ |
| 8 | stream_json_contract_smoke | `python3 scripts/stream_json_contract_smoke.py` | **PASS** ✓ |
| 9 | typecheck:diff | `bun run typecheck:diff` | baseline 1384 / current 1357 / **0 NEW** ✓ |
| 10 | lint:diff | `bun run lint:diff` | baseline 943 / current 938 / **0 NEW** ✓ |
| 11 | run_all_smoke (20 单元) | `bash scripts/run_all_smoke.sh` | **ALL PASS** ✓ |
| 12 | case 39 fingerprint | step 20 | `870f99ed494d3d145ed2eb1368132299` **稳定** ✓ |

### 系统红线持守

- ✅ 不动 line 71 顶层 DCE 链 (D-W5-P3-R2.1-B=α 拍板)
- ✅ 不动 `constants/prompts.ts` (R1.5 推 Wave6+)
- ✅ 不动 `utils/undercover.ts` (D-W5-P3-4=B 保留)
- ✅ 不动 `utils/commitAttribution.ts` (DCE 链)
- ✅ 不动 `utils/i18n/strings.{en,zh}.ts`
- ✅ 不动 `commands/insights.ts` (Allen WIP)
- ✅ 不动 `scripts/smoke_check.py` (系统红线)
- ✅ 不动 `bunfig.toml`
- ✅ 不动 R2.2-R2.7 / R2.灰 其它 permissions 文件 (R2.4 bashPermissions / R2.3 permissionSetup / R2.2 dangerousPatterns / R2.5 readOnlyValidation / R2.灰 4 文件)
- ✅ 不动 Workbench / CWB / web / mobile / extensions
- ✅ 不动 R7 / Phase 2 已合 main 文件
- ✅ 0 push / 0 tag / 0 merge main / 0 rebase / 0 reset / 0 stash / 0 force / 0 worktree 删除

### Phase 3 守纪声明

| 项 | 状态 |
|------|------|
| 单 commit 内串行 Edit (G3 race 永久不变量) | ✅ |
| 显式 add 2 文件 (yoloClassifier.ts + execution-log.md), 不用 `git add .` / `git add -A` | ✅ |
| 主 agent 直接 Edit (无子 agent 派出) | ✅ |
| Phase 3 未进入 R2.2-R2.7 / R3 / R1.5 | ✅ |
| line 71 DCE 单点保留 | ✅ |

### Phase 3 commit 链

| commit | message | 文件 / 净行 |
|--------|---------|------------|
| (本 commit) | `refactor(wave5): R2.1 yolo classifier USER_TYPE 收敛 (6 hits / 1 file)` | 2 / +7 / -6 |

---

*— Wave5 execution log v1.3 / Phase 3 R2.1 yoloClassifier 完成*

---

## Phase 4 — R3.2 firstPartyEventExporter USER_TYPE 收敛 (2026-04-29)

### 触发条件
- Allen 拍板 D-W5-P4-R3.2-A=A (启动 R3.2) + B=α (标准 helper 替换 12 处) + C=B (不新增专项 smoke) + D=A (commit message)
- Phase 3 R2.1 已合 main `32721f1`, wave5 worktree ff-only 同步至 `32721f1`

### 实施前 baseline
- main HEAD: `32721f178c2ef86766aa1f5c9891b6f3a6457445`
- wave5 worktree HEAD: `32721f1` (ff-only synced)
- firstPartyEventExporter.ts 文件总行数: 467 行 (改后 468, +1 import)

### 改动清单 (1 文件 / +13 -12)

| # | 行 (改后) | 改动 | 类别 |
|:-:|:----:|------|------|
| 1 | :16 | 新增 `import { getUserType } from '../../utils/sessionStorage.js'` (alphabetical, 在 log.js 后 sleep.js 前) | import |
| 2 | :141 (原 140) | shutdown — `process.env.USER_TYPE === 'ant'` → `getUserType() === 'ant'` | logForDebugging-only |
| 3 | :199 (原 198) | sendEventsInBatches — 同上 | logForDebugging-only |
| 4 | :214 (原 213) | sendEventsInBatches catch error — 同上 | logForDebugging-only |
| 5 | :255 (原 254) | sendBatchWithAuth OAuth expired — 同上 | logForDebugging-only |
| 6 | :285 (原 284) | sendBatchWithAuth 401 retry — 同上 | logForDebugging-only |
| 7 | :306 (原 305) | logSuccess — 同上 (含 2 行 logForDebugging) | logForDebugging-only |
| 8 | :336 (原 335) | scheduleBackoffRetry — 同上 | logForDebugging-only |
| 9 | :355 (原 354) | retryFailedEvents max attempts — 同上 | logForDebugging-only |
| 10 | :379 (原 378) | retryFailedEvents success — 同上 | logForDebugging-only |
| 11 | :426 (原 425) | retryFileInBackground retrying — 同上 | logForDebugging-only |
| 12 | :435 (原 434) | retryFileInBackground previous batch succeeded — 同上 | logForDebugging-only |
| 13 | :440 (原 439) | retryFileInBackground previous batch failed — 同上 | logForDebugging-only |

### 0 DCE / 0 业务行为变化证明
- **0 DCE 红线**: 0 顶层 const / 0 `feature()` macro / 0 `require()` / 0 `eslint-disable custom-rules/no-process-env-top-level` / 0 "DCE" 注释 / 0 `@[MODEL LAUNCH]` 标记
- **0 event payload 变化**: `FirstPartyEventLoggingPayload` 类型定义 (line 41-48) 不变, payload 构造路径 (axios.post line 272/290) 不变
- **0 routing 变化**: endpoint 字段 (line 92-93) 不变, getHostedPlatformUrls() 调用不变
- **0 auth/retry/storage/queue 逻辑变化**: shouldSkipAuth 决策不变, sendBatchWithAuth 鉴权流不变, EventQueue/EventStorage/RetryScheduler 调用不变
- **0 公开 API 签名变化**: enqueue / forceFlush / shutdown / getQueuedEventCount 不变 → 消费者 firstPartyEventLogger.ts 0 contract 破坏
- **行为完全等价**: Mossen 个人版 USER_TYPE undefined → `getUserType()` 返回 'external' → `'external' === 'ant'` 恒 false → 12 处 logForDebugging 全部不触发, 与原 inline `'undefined' === 'ant'` 行为一致

### 验证结果 (12 项, 全 PASS)

| # | 项 | 命令 | 结果 |
|:-:|------|------|:----:|
| 1 | grep firstPartyEventExporter USER_TYPE | `rg -n "process\.env\.USER_TYPE\|getUserType\(\)" services/analytics/firstPartyEventExporter.ts` | 0 处 `process.env.USER_TYPE` ✓ + 12 处 `getUserType() === 'ant'` ✓ |
| 2 | harness_R4 1P events exported | `python3 scripts/harness_R4_1p_events_exported_smoke.py` | **PASS** (1/1, weak-pass mode "1p_disabled") ✓ |
| 3 | harness_R1 no remote telemetry | `python3 scripts/harness_R1_no_remote_telemetry_traffic_smoke.py` | **PASS** (1/1) ✓ |
| 4 | harness_R7 no remote GB traffic | `python3 scripts/harness_R7_no_remote_growthbook_traffic_smoke.py` | **PASS** (2/2) ✓ |
| 5 | wave2_a7 GrowthBook suffix null | `python3 scripts/wave2_a7_growthbook_suffix_null_smoke.py` | **PASS** (3/3) ✓ |
| 6 | wave4_r8 feature flag | `python3 scripts/wave4_r8_feature_flag_smoke.py` | **PASS** (resolve 8 / known debt none) ✓ |
| 7 | layer_boundary_audit | `python3 scripts/layer_boundary_audit.py` | **PASS** (1940 files / 14194 imports / 4 rules) — imports +1 vs Phase 3 14193, 即新加 sessionStorage import ✓ |
| 8 | stream_json_contract_smoke | `python3 scripts/stream_json_contract_smoke.py` | **PASS** ✓ |
| 9 | typecheck:diff | `bun run typecheck:diff` | baseline 1384 / current 1357 / **0 NEW** ✓ |
| 10 | lint:diff | `bun run lint:diff` | baseline 943 / current 938 / **0 NEW** ✓ |
| 11 | run_all_smoke (20 单元) | `bash scripts/run_all_smoke.sh` | **ALL PASS** ✓ |
| 12 | case 39 fingerprint | step 20 | `870f99ed494d3d145ed2eb1368132299` **稳定** ✓ |

### 系统红线持守

- ✅ 不动 `services/analytics/firstPartyEventLogger.ts` (R3.1 别 slice, 7 hits 推 Phase 5+)
- ✅ 不动 `services/analytics/eventQueue.ts` / `eventStorage.ts` / `retryScheduler.ts` (Y-1 helpers)
- ✅ 不动 `services/analytics/datadog.ts` / `index.ts` / `metadata.ts` (R3.灰区, 推 Wave6)
- ✅ 不动 `services/mockRateLimits.ts` (R3.3 别 slice)
- ✅ 不动 `services/api/withRetry.ts` / `mossen.ts` (R3.4a/b 别 slice)
- ✅ 不动 `utils/debug.ts` (debug.ts:111 自身 gate)
- ✅ 不动 `utils/permissions/yoloClassifier.ts` (R2.1 已合 main, line 72 DCE 单点)
- ✅ 不动 `constants/prompts.ts` (R1.5 推 Wave6+)
- ✅ 不动 `utils/undercover.ts` (D-W5-P3-4=B 保留)
- ✅ 不动 `utils/commitAttribution.ts` (DCE 链)
- ✅ 不动 `utils/i18n/strings.{en,zh}.ts`
- ✅ 不动 `commands/insights.ts` (Allen WIP)
- ✅ 不动 `scripts/smoke_check.py` (系统红线)
- ✅ 不动 `bunfig.toml`
- ✅ 不动 Workbench / CWB / web / mobile / extensions
- ✅ 不动 R7 / Phase 1-3 已合 main 文件
- ✅ 0 push / 0 tag / 0 merge main / 0 rebase / 0 reset / 0 stash / 0 force / 0 worktree 删除

### Phase 4 守纪声明

| 项 | 状态 |
|------|------|
| 单 commit 内串行 Edit (G3 race 永久不变量) | ✅ |
| 显式 add 2 文件 (firstPartyEventExporter.ts + execution-log.md), 不用 `git add .` / `git add -A` | ✅ |
| 主 agent 直接 Edit (无子 agent 派出) | ✅ |
| Phase 4 未进入 R3.1 / R3.3 / R3.4 / R2.2-R2.7 / R1.5 | ✅ |

### Phase 4 commit 链

| commit | message | 文件 / 净行 |
|--------|---------|------------|
| (本 commit) | `refactor(wave5): R3.2 firstPartyEventExporter USER_TYPE 收敛 (12 hits / 1 file)` | 2 / +13 / -12 |

---

*— Wave5 execution log v1.4 / Phase 4 R3.2 firstPartyEventExporter 完成*

---

## Phase 5 — R3.1 firstPartyEventLogger USER_TYPE 收敛 (2026-04-29)

### 触发条件
- Allen 拍板 D-W5-P5-R3.1-A=A (启动 R3.1) + B=α (标准 helper 替换 7 处含 line 245 logError) + C=B (不新增专项 smoke) + D=A (commit message)
- Phase 4 R3.2 已合 main `a3e7f75`, wave5 worktree ff-only 同步至 `a3e7f75`

### 实施前 baseline
- main HEAD: `a3e7f75f54deba2a572004419bba8c9d447a23b3`
- wave5 worktree HEAD: `a3e7f75` (ff-only synced)
- firstPartyEventLogger.ts 文件总行数: 457 行 (改后 458, +1 import)

### 改动清单 (1 文件 / +8 -7)

| # | 行 (改后) | 函数 | 改动 | 类别 |
|:-:|:----:|------|------|------|
| 1 | :9 | (import 区) | 新增 `import { getUserType } from '../../utils/sessionStorage.js'` (alphabetical, 在 log.js 后 slowOperations.js 前) | import |
| 2 | :115 (原 114) | shutdown1PEventLogging | `process.env.USER_TYPE === 'ant'` → `getUserType() === 'ant'` | logForDebugging-only (final shutdown trace) |
| 3 | :169 (原 168) | logEventTo1PAsync | 同上 | logForDebugging-only ([MOSSEN-INTERNAL] event trace) |
| 4 | :177 (原 176) | logEventTo1PAsync (core_metadata missing 分支) | 同上 | logForDebugging-only (partial event diag) |
| 5 | :246 (原 245) | logEventTo1PAsync (catch block) | 同上 | **logError** (line 245 specialty: 错误吞噬流程内, ant 时多记一条 logError, Mossen 个人版静默 swallow, 行为与原 inline 等价) |
| 6 | :318 (原 317) | logGrowthBookExperimentTo1P | 同上 | logForDebugging-only (GB experiment trace) |
| 7 | :365 (原 364) | initialize1PEventLogging | 同上 | logForDebugging-only (not enabled diag) |
| 8 | :430 (原 429) | reinitialize1PEventLoggingIfConfigChanged | 同上 | logForDebugging-only (config change reinit trace) |

### Line 246 (原 245) logError 同等处置说明

R3.2 12 hits 100% logForDebugging; R3.1 7 hits 中 1 处 (改后 :246) 是 logError. 性质对比已在启动前复核确认:

- **业务作用**: catch block 内 `// swallow` 模式, ant 时多记一条 logError 帮助 debug, Mossen 个人版 (USER_TYPE undefined) 静默吞噬异常, **不进入 user-facing 错误处理**
- **行为等价性**: 替换前后 Mossen 个人版均不输出 logError (`'undefined' === 'ant'` 与 `'external' === 'ant'` 均为 false)
- **统一处置**: 与其余 6 处 logForDebugging 走同一 helper 替换模板, 单 commit 收口 R3.1 全部命中

### 0 DCE / 0 业务路径影响证明

- **0 DCE 红线**: 0 顶层 const 含 USER_TYPE / 0 `feature()` macro / 0 `require()` 调用 / 0 `eslint-disable custom-rules/no-process-env-top-level` / 0 "DCE" 注释 / 0 `@[MODEL LAUNCH]` 标记
- **enqueue 调用不变**: `exporter.enqueue(...)` (lines 182/212), `_newEventExporter.enqueue(...)` (line 324) — 全部**不在** USER_TYPE if-gate 内
- **payload 构造不变**: `to1PEventFormat()` (line 199), `MossenCodeInternalEvent.toJSON({...})` (line 214-240), `GrowthbookExperimentEvent.toJSON({...})` (line 326-344) — 全部**不在** if-gate 内
- **exporter 构造不变**: `_newEventExporter = new FirstPartyEventExporter({...})` (line 392-401) — 不在 if-gate 内
- **isSinkKilled 决策不变**: `isSinkKilled('firstParty')` (lines 271/309/400) — 不在 if-gate 内
- **sampling 不变**: `shouldSampleEvent` 函数体 (line 50-78) 0 USER_TYPE 命中
- **公开 API 0 contract 破坏**: 6 个消费者 (sink.ts / init.ts / Feedback.tsx / gracefulShutdown.ts / 2 mcpServer.ts) 0 受影响

### 验证结果 (12 项, 全 PASS)

| # | 项 | 命令 | 结果 |
|:-:|------|------|:----:|
| 1 | grep firstPartyEventLogger USER_TYPE | `rg -n "process\.env\.USER_TYPE\|getUserType\(\)" services/analytics/firstPartyEventLogger.ts` | 0 处 `process.env.USER_TYPE` ✓ + 7 处 `getUserType() === 'ant'` ✓ |
| 2 | harness_R4 1P events exported | `python3 scripts/harness_R4_1p_events_exported_smoke.py` | **PASS** (1/1, weak-pass mode "1p_disabled") ✓ |
| 3 | harness_R1 no remote telemetry | `python3 scripts/harness_R1_no_remote_telemetry_traffic_smoke.py` | **PASS** (1/1) ✓ |
| 4 | harness_R7 no remote GB traffic | `python3 scripts/harness_R7_no_remote_growthbook_traffic_smoke.py` | **PASS** (2/2) ✓ |
| 5 | wave2_a7 GrowthBook suffix null | `python3 scripts/wave2_a7_growthbook_suffix_null_smoke.py` | **PASS** (3/3) ✓ |
| 6 | wave4_r8 feature flag | `python3 scripts/wave4_r8_feature_flag_smoke.py` | **PASS** (resolve 8 / known debt none) ✓ |
| 7 | layer_boundary_audit | `python3 scripts/layer_boundary_audit.py` | **PASS** (1940 files / 14195 imports / 4 rules) — imports +1 vs Phase 4 14194, 即新加 sessionStorage import ✓ |
| 8 | stream_json_contract_smoke | `python3 scripts/stream_json_contract_smoke.py` | **PASS** ✓ |
| 9 | typecheck:diff | `bun run typecheck:diff` | baseline 1384 / current 1357 / **0 NEW** ✓ |
| 10 | lint:diff | `bun run lint:diff` | baseline 943 / current 938 / **0 NEW** ✓ |
| 11 | run_all_smoke (20 单元) | `bash scripts/run_all_smoke.sh` | **ALL PASS** ✓ |
| 12 | case 39 fingerprint | step 20 | `870f99ed494d3d145ed2eb1368132299` **稳定** ✓ |

### 系统红线持守

- ✅ 不动 `services/analytics/firstPartyEventExporter.ts` (R3.2 已 Phase 4 收口)
- ✅ 不动 `services/analytics/eventQueue.ts` / `eventStorage.ts` / `retryScheduler.ts` (Y-1 helpers)
- ✅ 不动 `services/analytics/sink.ts` / `sinkKillswitch.ts` / `config.ts` / `growthbook.ts` (logger 消费者 + 配置)
- ✅ 不动 `services/mockRateLimits.ts` (R3.3 别 slice)
- ✅ 不动 `services/api/withRetry.ts` / `mossen.ts` (R3.4a/b 别 slice)
- ✅ 不动 `utils/debug.ts` / `utils/log.ts` (logForDebugging / logError 别文件)
- ✅ 不动 `utils/permissions/yoloClassifier.ts` (R2.1 已合 main, line 72 DCE 单点)
- ✅ 不动 `constants/prompts.ts` (R1.5 推 Wave6+)
- ✅ 不动 `utils/undercover.ts` (D-W5-P3-4=B 保留)
- ✅ 不动 `utils/commitAttribution.ts` (DCE 链)
- ✅ 不动 `utils/i18n/strings.{en,zh}.ts`
- ✅ 不动 `commands/insights.ts` (Allen WIP)
- ✅ 不动 `scripts/smoke_check.py` (系统红线)
- ✅ 不动 `bunfig.toml`
- ✅ 不动 Workbench / CWB / web / mobile / extensions
- ✅ 不动 R7 / Phase 1-4 已合 main 文件
- ✅ 0 push / 0 tag / 0 merge main / 0 rebase / 0 reset / 0 stash / 0 force / 0 worktree 删除

### Phase 5 守纪声明

| 项 | 状态 |
|------|------|
| 单 commit 内串行 Edit (G3 race 永久不变量) | ✅ |
| 显式 add 2 文件 (firstPartyEventLogger.ts + execution-log.md), 不用 `git add .` / `git add -A` | ✅ |
| 主 agent 直接 Edit (无子 agent 派出) | ✅ |
| Phase 5 未进入 R3.3 / R3.4 / R2.2-R2.7 / R1.5 / R3.灰区 | ✅ |

### Phase 5 commit 链

| commit | message | 文件 / 净行 |
|--------|---------|------------|
| (本 commit) | `refactor(wave5): R3.1 firstPartyEventLogger USER_TYPE 收敛 (7 hits / 1 file)` | 2 / +8 / -7 |

---

*— Wave5 execution log v1.5 / Phase 5 R3.1 firstPartyEventLogger 完成*

---

## Phase 6 — R3.4a withRetry USER_TYPE 收敛 (2026-04-29)

### 触发条件
- Allen 拍板 D-W5-P6-R3.4a-A=A (启动) + B=α (标准 helper 替换 2 处, 0 import) + C=B (不新增专项 smoke) + D=A (commit message)
- Phase 5 R3.1 已合 main `85c594f`, wave5 worktree ff-only 同步至 `85c594f`

### 实施前 baseline
- main HEAD: `85c594fff652d0b3cb01bbbae9de2b52181dd39a`
- wave5 worktree HEAD: `85c594f` (ff-only synced)
- withRetry.ts 文件总行数: 829 行 (改后不变, 仅 2 行 modified)

### 关键先验状态 (R3.4a 不需要新加 import)
- `import { getUserType } from '../../utils/sessionStorage.js'` 已在 line 26 (Wave 0 API-001 hotfix 留下)
- `getUserType() === 'external'` 已在 line 361 (Wave 0 API-001 已迁移此文件 1 处)
- R3.4a 是同模式扩展剩余 2 处

### 改动清单 (1 文件 / +2 -2)

| # | 行 | 函数 | 改动 | 类别 |
|:-:|:----:|------|------|------|
| 1 | :203 | retry loop main body (`withRetry`) | `process.env.USER_TYPE === 'ant'` → `getUserType() === 'ant'` (mock rate limit gate) | B (业务 gate, 等价) |
| 2 | :755 | `shouldRetry()` helper | `process.env.USER_TYPE === 'ant'` → `getUserType() === 'ant'` (5xx retry override) | B (业务 gate, 等价) |

### Line 356 注释保留说明

Line 356 是历史注释 (Wave 0 API-001 hotfix 留下):
```ts
// Use getUserType() (fallback 'external') instead of reading the
// env directly: Mossen personal-edition default USER_TYPE is
// undefined, so a raw `process.env.USER_TYPE === 'external'`
// never matches and personal-edition users would loop on 529
// forever (retry storm). getUserType() resolves the same default
// already used elsewhere via sessionStorage.
```

注释字面量含 `process.env.USER_TYPE === 'external'` 但**不是实际 env 检查**, 是历史迁移说明文档. **必须保留**, 删除会破坏 commit log 可追溯性 (Wave 0 API-001 → Wave 5 R3.4a 迁移链历史).

### Line 203 mock rate limit gate 等价说明
```ts
if (getUserType() === 'ant') {
  const mockError = checkMockRateLimitError(retryContext.model, wasFastModeActive)
  if (mockError) throw mockError
}
```
- 用途: ant 内部 `/mock-limits` slash command 注入模拟 rate limit 测试 retry 逻辑
- Mossen 个人版 USER_TYPE undefined → getUserType() = 'external' → false → 跳过 mock (与原 inline 行为一致)
- 上游 ant build USER_TYPE='ant' → true → 注入 mock (与原 inline 一致)
- **0 retry 次数 / backoff / 错误重抛变化**

### Line 755 5xx retry override 等价说明
```ts
if (shouldRetryHeader === 'false') {
  const is5xxError = error.status !== undefined && error.status >= 500
  if (!(getUserType() === 'ant' && is5xxError)) {
    return false  // 尊重 server x-should-retry: false
  }
  // ant 用户对 5xx 忽略 server hint, 继续 retry
}
```
- 用途: 注释 line 751 写明 "Internal users can ignore x-should-retry: false for 5xx server errors only"
- Mossen 个人版 USER_TYPE undefined → false → return false (尊重 header, 不 retry) — 与原 inline 一致
- 上游 ant build USER_TYPE='ant' → 5xx 时忽略 header, 继续 retry — 与原 inline 一致
- **0 retry 决策 / API 路由变化**

### 0 DCE / 0 retry API 业务行为变化证明
- **0 DCE 红线**: 0 顶层 const 含 USER_TYPE / 0 `feature()` macro 嵌套 USER_TYPE / 0 `require()` / 0 `eslint-disable custom-rules/no-process-env-top-level` / 0 "DCE" 注释 / 0 `@[MODEL LAUNCH]` 标记
- **0 retry 次数变化**: `getMaxRetries(options)` (line 180) 不依赖 USER_TYPE; for-loop (line 190) 不在 if-gate 内
- **0 backoff/sleep 变化**: `getRetryDelay(attempt, retryAfter)` / `sleep(...)` 不在 USER_TYPE if-gate 内
- **0 错误重抛变化**: `throw new CannotRetryError(...)` (lines 324/366/378/388) / `throw new FallbackTriggeredError(...)` (line 348) 不在 if-gate 内
- **0 API 调用变化**: `await operation(client, attempt, retryContext)` (line 254) 不在 if-gate 内
- **0 auth/model provider 变化**: `getClient()` / `getHostedOAuthTokens()` / `handleOAuth401Error()` / `clearApiKeyHelperCache()` 不在 if-gate 内
- **0 stream-json contract 变化**: withRetry 是 retry wrapper, 不直接处理 stream-json schema
- **行为完全等价**: `getUserType()` 与 `process.env.USER_TYPE === 'ant'` 在所有输入下产生相同布尔值

### 验证结果 (12 项, 全 PASS)

| # | 项 | 命令 | 结果 |
|:-:|------|------|:----:|
| 1 | grep withRetry USER_TYPE | `rg -n "process\.env\.USER_TYPE\|getUserType\(" services/api/withRetry.ts` | 真代码 0 处 `process.env.USER_TYPE` ✓ + line 356 注释保留 ✓ + 3 处 `getUserType()` (lines 203/361/755) ✓ |
| 2 | wave0_api001 直接 smoke | `python3 scripts/wave0_api001_withretry_smoke.py` | **PASS** (1/1) ✓ |
| 3 | harness_R5 provider priority | `python3 scripts/harness_R5_provider_priority_smoke.py` | **PASS** (5/5) ✓ |
| 4 | harness_R8 default value parity | `python3 scripts/harness_R8_default_value_parity_smoke.py` | **exit 0** ✓ (注: 输出 keys.json missing, 主仓同状态, 是 fixture gap 与 R3.4a 无关) |
| 5 | harness_R7 no remote GB traffic | `python3 scripts/harness_R7_no_remote_growthbook_traffic_smoke.py` | **PASS** (2/2) ✓ |
| 6 | harness_R1 no remote telemetry | `python3 scripts/harness_R1_no_remote_telemetry_traffic_smoke.py` | **PASS** (1/1) ✓ |
| 7 | harness_R4 1P events exported | `python3 scripts/harness_R4_1p_events_exported_smoke.py` | **PASS** (1/1) ✓ |
| 8 | wave2_a7 GrowthBook suffix null | `python3 scripts/wave2_a7_growthbook_suffix_null_smoke.py` | **PASS** (3/3) ✓ |
| 9 | wave4_r8 feature flag | `python3 scripts/wave4_r8_feature_flag_smoke.py` | **PASS** (resolve 8 / known debt none) ✓ |
| 10 | layer_boundary_audit | `python3 scripts/layer_boundary_audit.py` | **PASS** (1940 files / 14195 imports / 4 rules) — imports 不变 (0 新增 import) ✓ |
| 11 | stream_json_contract_smoke | `python3 scripts/stream_json_contract_smoke.py` | **PASS** ✓ |
| 12 | typecheck:diff / lint:diff | `bun run typecheck:diff` / `lint:diff` | typecheck 0 NEW (baseline 1384 / current 1357) / lint 0 NEW (baseline 943 / current 938) ✓ |
| 13 | run_all_smoke (20 单元) | `bash scripts/run_all_smoke.sh` | **ALL PASS** ✓ |
| 14 | case 39 fingerprint | step 20 | `870f99ed494d3d145ed2eb1368132299` **稳定** ✓ |

### 系统红线持守

- ✅ 不动 line 26 import (已存在, 0 改动)
- ✅ 不动 line 356 注释 (Wave 0 API-001 历史说明)
- ✅ 不动 line 361 已迁移 `getUserType() === 'external'` (Wave 0 API-001 已收口)
- ✅ 不动 `services/api/mossen.ts` (R3.4b 别 slice, 推 Phase 7+)
- ✅ 不动 `services/mockRateLimits.ts` (R3.3 别 slice, 推 Phase 7+)
- ✅ 不动 `services/analytics/firstPartyEventLogger.ts` (R3.1 已 Phase 5 收口)
- ✅ 不动 `services/analytics/firstPartyEventExporter.ts` (R3.2 已 Phase 4 收口)
- ✅ 不动 Y-1 helpers / R3.灰区 / R2.* / R1.* / R7
- ✅ 不动 `utils/permissions/yoloClassifier.ts` / `constants/prompts.ts` / `utils/undercover.ts` / `utils/commitAttribution.ts`
- ✅ 不动 `utils/i18n/strings.{en,zh}.ts` / `commands/insights.ts` / `scripts/smoke_check.py` / `bunfig.toml`
- ✅ 不动 Workbench / CWB / web / mobile / extensions
- ✅ 不动 R7 / Phase 1-5 已合 main 文件
- ✅ 0 push / 0 tag / 0 merge main / 0 rebase / 0 reset / 0 stash / 0 force / 0 worktree 删除

### Phase 6 守纪声明

| 项 | 状态 |
|------|------|
| 单 commit 内串行 Edit (G3 race 永久不变量) | ✅ |
| 显式 add 2 文件 (withRetry.ts + execution-log.md), 不用 `git add .` / `git add -A` | ✅ |
| 主 agent 直接 Edit (无子 agent 派出) | ✅ |
| Phase 6 未进入 R3.4b / R3.3 / R2.2-R2.7 / R1.5 | ✅ |

### Phase 6 commit 链

| commit | message | 文件 / 净行 |
|--------|---------|------------|
| (本 commit) | `refactor(wave5): R3.4a withRetry USER_TYPE 收敛 (2 hits / 1 file)` | 2 / +2 / -2 |

---

## Phase 7 — R3.3 mockRateLimits USER_TYPE 收敛 (module-local helper) (2026-04-29)

### 触发条件 + 历史背景
- 首轮 α 方案 (新增 `import { getUserType } from '../utils/sessionStorage.js'`) 触发 boot-time TDZ 循环, 已全量 revert 回 `ec85669`
- Allen 拍板 D-W5-P7-RECOVER-1 = β' module-local helper, D-W5-P7-RECOVER-2 = 加 boot smoke 要求 (不改 harness)
- 不触动 R3.4b `services/api/mossen.ts` / 不扩展 wave0_api001 static_findings

### α 方案失败根因 (供后续 phase 借鉴)

**TDZ 循环路径** (实测):

```
tools/AgentTool/built-in/mossenCodeGuideAgent.ts (顶层 tools 数组求值)
  └─ import 'src/utils/auth.js'
       └─ utils/auth.ts:19-21 import '../services/mockRateLimits.js'
            └─ services/mockRateLimits.ts (α NEW import) → '../utils/sessionStorage.js'
                 └─ utils/sessionStorage.ts → '../commands.js' / '../tools/REPLTool/constants.js' / 等
                      └─ 回到 tools 树 → 再回到 mossenCodeGuideAgent.ts (已部分初始化)
                           └─ 读 FILE_READ_TOOL_NAME → TDZ throw
```

**为何 Phase 4/5/6 没遇到**:
- firstPartyEventExporter (Phase 4) / firstPartyEventLogger (Phase 5) / withRetry (Phase 6 import 已存在) 均不在 `utils/auth.ts` 依赖链上
- mockRateLimits.ts 的特殊地位: **被 `utils/auth.ts` 直接 import** → 几乎所有 tool/command/agent 模块的传递依赖根

**复核盲区暴露**: 启动前复核仅做静态等价证明, 未做模块初始化路径分析。后续 phase 任何"被 utils/auth.ts 直接/间接 import 的服务模块"新增 sessionStorage import 之前, **必须先跑 `bun run help` boot smoke**。

### β' module-local helper 方案

**实施前 baseline**:
- main HEAD: `ec8566933bfe6e32cce7cbaa56ed3cd3d57c00f8`
- wave5 worktree HEAD: `ec85669` (与 main 一致, clean)
- mockRateLimits.ts 文件: 882 行 (改后 891 行, +9 行 = 1 helper 函数 + 注释)

**改动清单 (1 文件 / +12 -11 / 净 +9 行)**:

| # | 行 | 函数 / 位置 | 改动 | 类别 |
|:-:|:----:|------|------|------|
| 0 | :12-18 | (新增 module-local helper) | `+ function getMockRateLimitUserType(): string { return process.env.USER_TYPE \|\| 'external' }` + 4 行注释说明禁 import sessionStorage | helper |
| 1 | :112 | `setMockHeader()` | `process.env.USER_TYPE !== 'ant'` → `getMockRateLimitUserType() !== 'ant'` | B (业务 gate, 等价) |
| 2 | :261 | `addExceededLimit()` | 同上 | B |
| 3 | :289 | `setMockEarlyWarning()` | 同上 | B |
| 4 | :330 | `setMockRateLimitScenario()` | 同上 | B |
| 5 | :611 | `getMockHeaderless429Message()` | 同上 | B |
| 6 | :627 | `getMockHeaders()` (compound) | 同上 | B |
| 7 | :722 | `shouldProcessMockLimits()` | 同上 | B |
| 8 | :817 | `setMockSubscriptionType()` | 同上 | B |
| 9 | :825 | `getMockSubscriptionType()` (compound) | 同上 | B |
| 10 | :837 | `shouldUseMockSubscription()` | `process.env.USER_TYPE === 'ant'` → `getMockRateLimitUserType() === 'ant'` | B (正向 gate, 等价) |
| 11 | :843 | `setMockBillingAccess()` | `process.env.USER_TYPE !== 'ant'` → `getMockRateLimitUserType() !== 'ant'` | B |

**helper 注释原文** (lines 12-15):
```ts
// Module-local USER_TYPE accessor. Kept inline (not imported from
// utils/sessionStorage.js) because this module is imported by utils/auth.ts;
// pulling sessionStorage in here introduces a boot-time cycle through the
// commands/tools tree that triggers a TDZ on tool-name constants.
```

### 0 DCE / 0 mock rate limit 行为变化 / 0 测试逃逸风险升级证明

- **0 DCE 红线**: helper + 11 处全部位于函数体内 / 0 顶层 const 含 USER_TYPE 表达式 / 0 `feature()` macro / 0 `require()` / 0 `eslint-disable custom-rules/no-process-env-top-level` / 0 "DCE" 注释 / 0 `@[MODEL LAUNCH]` 标记
- **等价证明**: `getMockRateLimitUserType()` body = `process.env.USER_TYPE || 'external'`, 与 `getUserType()` (utils/sessionStorage.ts:420-422) **行为完全一致**
  - 个人版 USER_TYPE undefined → `'external'` → 11 重早返触发 (与原 inline 一致)
  - ant 内部 USER_TYPE='ant' → `'ant'` → 进入 mock 路径 (与原 inline 一致)
- **0 mock rate limit 业务行为变化**: `/mock-limits` stub disabled + withRetry.ts:203 Phase 6 外层 gate + mockRateLimits 11 重内层 gate, 三层防御链路完整保留
- **0 测试逃逸风险升级**: helper 与 `getUserType()` 在 boolean 判定上完全等价, 测试逃逸防御不变
- **0 import 路径异常**: 仅复用既有 import (`oauth/types`, `billing`, `hostedLimits`), 0 新增 import → layer_boundary_audit imports 数 14195 不变 (与 Phase 6 一致)

### 验证结果 (14 项, 全 PASS)

| # | 项 | 命令 | 结果 |
|:-:|------|------|:----:|
| 1 | grep mockRateLimits USER_TYPE | `rg -n "process\.env\.USER_TYPE\|getMockRateLimitUserType\(\|getUserType\(" services/mockRateLimits.ts` | 1 处 `process.env.USER_TYPE` (line 17, 仅 helper body) ✓ + 11 处 `getMockRateLimitUserType()` ✓ + 0 处 `getUserType(` ✓ |
| 2 | **boot smoke** `bun run help` | binary 启动 | **PASS** (Usage 头部正常输出, 0 TDZ / 0 cycle) ✓ |
| 3 | wave0_api001 直接 smoke | `python3 scripts/wave0_api001_withretry_smoke.py` | **PASS** (1/1) ✓ |
| 4 | wave2_a5 cmd allowlist | `python3 scripts/wave2_a5_cmd_allowlist_smoke.py` | **PASS** (6/6) ✓ |
| 5 | harness_R5 provider priority | `python3 scripts/harness_R5_provider_priority_smoke.py` | **PASS** (5/5) ✓ |
| 6 | harness_R7 no remote GB traffic | `python3 scripts/harness_R7_no_remote_growthbook_traffic_smoke.py` | **PASS** (2/2) ✓ |
| 7 | wave4_r8 feature flag | `python3 scripts/wave4_r8_feature_flag_smoke.py` | **PASS** (resolve 8 / known debt none) ✓ |
| 8 | layer_boundary_audit | `python3 scripts/layer_boundary_audit.py` | **PASS** (1940 files / **14195 imports 不变** / 4 rules) ✓ |
| 9 | stream_json_contract_smoke | `python3 scripts/stream_json_contract_smoke.py` | **PASS** ✓ |
| 10 | typecheck:diff | `bun run typecheck:diff` | **0 NEW** (baseline 1384 / current 1357 / fixed 27) ✓ |
| 11 | lint:diff | `bun run lint:diff` | **0 NEW** (baseline 943 / current 938 / fixed 5) ✓ |
| 12 | run_all_smoke (20 单元) | `bash scripts/run_all_smoke.sh` | **ALL PASS** ✓ |
| 13 | case 39 fingerprint | step 20 | `870f99ed494d3d145ed2eb1368132299` **稳定** ✓ |
| 14 | git diff --check | `git diff --check` | 0 空白错误 ✓ |

### 系统红线持守

- ✅ 不动 `services/api/withRetry.ts` (Phase 6 R3.4a 已收口)
- ✅ 不动 `services/api/mossen.ts` (R3.4b 别 slice)
- ✅ 不动 `services/rateLimitMocking.ts` (facade)
- ✅ 不动 `utils/auth.ts` (mockRateLimits 上游 import 方, β' 关键守恒目标)
- ✅ 不动 `utils/sessionStorage.ts` (β' 关键守恒目标, 不引入循环)
- ✅ 不动 `commands/mock-limits/index.js` (stub disabled)
- ✅ 不动 `services/analytics/firstPartyEventLogger.ts` / `firstPartyEventExporter.ts` (Phase 4/5 已收口)
- ✅ 不动 `utils/permissions/yoloClassifier.ts` (Phase 3 已收口)
- ✅ 不动 `constants/prompts.ts` / `utils/undercover.ts` / `utils/commitAttribution.ts`
- ✅ 不动 `utils/i18n/strings.{en,zh}.ts` / `commands/insights.ts` / `bunfig.toml`
- ✅ 不动 任何 `scripts/harness_*_smoke.py` / `scripts/wave*_smoke.py` / `scripts/smoke_check.py` (harness 红线)
- ✅ 不动 Workbench / CWB / web / mobile / extensions
- ✅ 0 push / 0 tag / 0 merge main / 0 rebase / 0 reset / 0 stash (Phase 7 期间未新增 stash) / 0 checkout HEAD -- / 0 force / 0 worktree 删除
- ✅ 历史 stash `insights-wip-pre-rollback-20260424` 受保护未动

### Phase 7 守纪声明

| 项 | 状态 |
|------|------|
| 单 commit 内串行 Edit (G3 race 永久不变量) | ✅ |
| 显式 add 2 文件 (mockRateLimits.ts + execution-log.md), 不用 `git add .` / `git add -A` | ✅ |
| 主 agent 直接 Edit (无子 agent 派出) | ✅ |
| boot smoke (`bun run help`) 在 commit 前先跑 | ✅ |
| Phase 7 未进入 R3.4b / R2.2-R2.7 / R1.5 | ✅ |
| 0 触动 harness / smoke 脚本判定逻辑 | ✅ |

### 后续 phase 借鉴 (复核盲区补强)

- **boot smoke 前置**: 任何被 `utils/auth.ts` 直接/间接 import 的服务模块, 添加跨模块 import 之前必须先跑 `bun run help`
- **module-local helper fallback**: 当跨模块 import 触发循环时, 改用 module-local helper (如本 phase β' 方案), 业务等价但不引入 import
- **layer_boundary_audit imports 数对照**: import 数变化 = 引入新跨模块依赖的检测信号, β' 方案 imports 不变是关键证据

### Phase 7 commit 链

| commit | message | 文件 / 净行 |
|--------|---------|------------|
| (本 commit) | `refactor(wave5): R3.3 mockRateLimits USER_TYPE 收敛 (local helper)` | 2 / +12 / -11 (mockRateLimits.ts) + execution-log.md |

---

## Phase 8 — R3.4b mossen API USER_TYPE 收敛 (2026-04-29)

### 触发条件
- Allen 拍板 D-W5-P8-R3.4b-A=α (全量 8 处替换 + 1 import) + B=不新增专项 smoke + C=boot smoke 失败时不自动降级，停下汇报 + D=不同步触动其它 R 子 slice
- Phase 7 R3.3 已合 main `0344b09`, wave5 worktree ff-only 同步至 `0344b09`

### 实施前 baseline
- main HEAD: `0344b0909e32b8d400f044842bd7335fed8b9df9`
- wave5 worktree HEAD: `0344b09` (ff-only synced)
- mossen.ts 文件总行数: 3404 行 (改后 3405 行, +1 行 = 1 import 新增)

### 改动清单 (1 文件 / +9 -8)

| # | 行（旧→新）| 函数 / 上下文 | 改动 | 类别 |
|:-:|:----:|------|------|------|
| 0 | (新增) :92 | imports 区, alphabetically 在 model/model.js 与 systemPromptType.js 之间 | `+ import { getUserType } from '../../utils/sessionStorage.js'` | import |
| 1 | :411→:412 | `isPromptCache1hEligible()` body | `process.env.USER_TYPE === 'ant'` → `getUserType() === 'ant'`（1h prompt cache TTL eligibility latch）| B (业务 gate, 等价) |
| 2 | :459→:460 | `configureEffortParams()` else-if | 同上（numeric effort_override → `mossen_internal`）| B |
| 3 | :1982→:1983 | stream parser `message_start` case | 同上（capture `research` from message_start）| B |
| 4 | :2161→:2162 | stream parser `content_block_delta` case | 同上（capture `research` from content_block_delta）| B |
| 5 | :2200→:2201 | `content_block_stop` 内 AssistantMessage 构造 | 同上（spread `research` into yielded message）| B |
| 6 | :2215→:2216 | stream parser `message_delta` case | 同上（capture `research` + write back to messages）| B |
| 7 | :2587→:2588 | success path AssistantMessage 构造 | 同上（spread `research` into final message）| B |
| 8 | :2684→:2685 | 404 fallback path AssistantMessage 构造 | 同上（spread `research` after 404 → non-streaming fallback）| B |

### Import-cycle 风险与 boot smoke 验证

- **Phase 7 R3.3 失败教训**：mockRateLimits.ts 被 utils/auth.ts 直接 import → 新 sessionStorage import 触发 boot-time TDZ 循环（auth → mockRateLimits → sessionStorage → tools 树 → 回到 mossenCodeGuideAgent 部分初始化态）
- **mossen.ts 拓扑差异**：mossen.ts **下游**于 utils/auth.ts（line 59 `import { getOauthAccountInfo } from '../../utils/auth.js'`），无任何 importer 是 utils/auth.ts → 不闭合 boot-time auth pre-init cycle
- **Witness 先例**：`services/api/withRetry.ts:26` 已存在 `import { getUserType } from '../../utils/sessionStorage.js'`（自 Wave 0 API-001 起），boot smoke 历来 PASS。withRetry 与 mossen 同处 `services/api/` 层级，先例验证 sessionStorage import 在该层级安全
- **强制守卫**：实施提交前 `bun run help` boot smoke
- **boot smoke 实测结果**：**PASS**（Usage 头部正常输出，0 TDZ / 0 boot-time cycle）

### 0 DCE / 0 业务行为变化 / 0 stream-json schema 变化 证明

- **0 DCE 红线**: 8 处全部位于函数体内（行号 ≥ 412） / 0 顶层 const 含 USER_TYPE / 0 `feature() && process.env.USER_TYPE` 嵌套 / 0 `require()` 与 USER_TYPE 共行（line 113 `feature('TRANSCRIPT_CLASSIFIER') + require` 与 USER_TYPE 完全解耦） / 0 `eslint-disable custom-rules/no-process-env-top-level` / 0 `@[MODEL LAUNCH]`
- **等价证明**: `getUserType()` (utils/sessionStorage.ts:420-422) = `process.env.USER_TYPE || 'external'`
  - 个人版 USER_TYPE undefined → `'external'` → `=== 'ant'` false → 与原 inline 一致
  - ant 内部 USER_TYPE='ant' → `'ant'` → `=== 'ant'` true → 与原 inline 一致
- **0 stream-json schema 变化**: `research` 字段对个人版（`getUserType() !== 'ant'`）永不附加；6 处 research-related gate（hits 3-8）不破 stream-json contract（`stream_json_contract_smoke.py` PASS）
- **0 prompt cache 1h TTL 决策变化**（hit 1）: bootstrap state latch 等价，不击穿 server-side cache
- **0 numeric effort_override 行为变化**（hit 2）: `extraBodyParams.mossen_internal.effort_override` 注入路径等价
- **0 provider 兼容影响**: mossen.ts 仅 Anthropic SDK 路径；OpenAI / custom backend (MiniMax/GLM) 走 `services/api/openai.ts` 等其它文件，0 cross-provider 影响
- **0 tool-call 路由变化**: tool-use 块（line 1991+ `'content_block_start'` switch case `'tool_use'`）不在 USER_TYPE gate 内
- **0 response payload 形状变化**: 个人版 `research === undefined`，3 处 spread（hits 5/7/8）短路 → AssistantMessage 形状与替换前完全一致

### 验证结果 (12 项, 全 PASS)

| # | 项 | 命令 | 结果 |
|:-:|------|------|:----:|
| 1 | grep mossen.ts USER_TYPE | `rg -n "process\.env\.USER_TYPE\|getUserType\(" services/api/mossen.ts` | 真代码 **0 处 `process.env.USER_TYPE`** ✓ + 8 处 `getUserType() === 'ant'` (lines 412/460/1983/2162/2201/2216/2588/2685) ✓ |
| 2 | **boot smoke** `bun run help` | binary 启动 | **PASS** (Usage 头部正常输出, 0 TDZ / 0 cycle) ✓ |
| 3 | wave0_api001 直接 smoke | `python3 scripts/wave0_api001_withretry_smoke.py` | **PASS** (1/1) ✓ |
| 4 | harness_R5 provider priority | `python3 scripts/harness_R5_provider_priority_smoke.py` | **PASS** (5/5) ✓ |
| 5 | harness_R7 no remote GB traffic | `python3 scripts/harness_R7_no_remote_growthbook_traffic_smoke.py` | **PASS** (2/2) ✓ |
| 6 | wave2_a5 cmd allowlist | `python3 scripts/wave2_a5_cmd_allowlist_smoke.py` | **PASS** (6/6) ✓ |
| 7 | stream_json_contract_smoke | `python3 scripts/stream_json_contract_smoke.py` | **PASS** (24 SDKMessage + 21 control + 8 stdout + 5 stdin + anchors) ✓ |
| 8 | typecheck:diff | `bun run typecheck:diff` | **0 NEW** (baseline 1384 / current 1357 / fixed 27) ✓ |
| 9 | lint:diff | `bun run lint:diff` | **0 NEW** (baseline 943 / current 938 / fixed 5) ✓ |
| 10 | layer_boundary_audit | `python3 scripts/layer_boundary_audit.py` | **PASS** (1940 files / **14196 imports** / 4 rules) — imports +1 (sessionStorage 新增 import, 与预测一致) ✓ |
| 11 | run_all_smoke (20 单元) | `bash scripts/run_all_smoke.sh` | **ALL PASS** ✓ |
| 12 | case 39 fingerprint | step 20 | `870f99ed494d3d145ed2eb1368132299` **稳定** ✓ |

### 系统红线持守

- ✅ 不动 `services/mockRateLimits.ts` (Phase 7 R3.3 已收口)
- ✅ 不动 `services/api/withRetry.ts` (Phase 6 R3.4a 已收口)
- ✅ 不动 `utils/auth.ts` / `utils/sessionStorage.ts`（仅消费 import，未修改）
- ✅ 不动 `services/rateLimitMocking.ts` / `commands/mock-limits/index.js`
- ✅ 不动 `services/analytics/firstPartyEventLogger.ts` / `firstPartyEventExporter.ts` (Phase 4/5 已收口)
- ✅ 不动 `utils/permissions/yoloClassifier.ts` (Phase 3 已收口)
- ✅ 不动 `constants/prompts.ts` / `utils/undercover.ts` / `utils/commitAttribution.ts`
- ✅ 不动 `utils/i18n/strings.{en,zh}.ts` / `commands/insights.ts` / `bunfig.toml`
- ✅ 不动 任何 `scripts/harness_*_smoke.py` / `scripts/wave*_smoke.py` / `scripts/smoke_check.py` (harness 红线)
- ✅ 不动 Workbench / CWB / web / mobile / extensions
- ✅ 不动 Y-1 helpers / R3.灰区 / R2.2-R2.7 / R1.* / R7
- ✅ 0 push / 0 tag / 0 merge main / 0 rebase / 0 reset / 0 stash / 0 checkout HEAD -- / 0 force / 0 worktree 删除

### Phase 8 守纪声明

| 项 | 状态 |
|------|------|
| 单 commit 内串行 Edit (G3 race 永久不变量) | ✅ |
| 显式 add 2 文件 (mossen.ts + execution-log.md), 不用 `git add .` / `git add -A` | ✅ |
| 主 agent 直接 Edit (无子 agent 派出) | ✅ |
| boot smoke (`bun run help`) 在 commit 前先跑 | ✅ |
| Phase 8 未进入 R2.2-R2.7 / R1.5 / Wave6 | ✅ |
| 0 触动 harness / smoke 脚本判定逻辑 | ✅ |
| α boot smoke 失败时未自动降级 β''（实测 PASS, 未触发降级路径）| ✅ |

### Phase 8 commit 链

| commit | message | 文件 / 净行 |
|--------|---------|------------|
| (本 commit) | `refactor(wave5): R3.4b mossen API USER_TYPE 收敛 (8 hits / 1 file)` | 2 / +9 / -8 (mossen.ts) + execution-log.md |

---

*— Wave5 execution log v1.8 / Phase 8 R3.4b mossen API 完成*
