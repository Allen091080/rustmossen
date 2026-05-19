# W55 Round 2 · `/project purge` 用户入口归档

## 元信息

| 字段 | 值 |
|---|---|
| 日期 | 2026-05-03 |
| 起点 HEAD | `65a76fd`（W55 round-1 push）|
| 范围 | C5 `/project purge` dry-run + `--confirm <token>` (archive-only) |
| 性质 | feat + test + docs（无协议 union / 主循环 / ToolUseContext / Workbench / push）|
| 决议 | C5 落地（Round 2 single-shot），后续待 Round 3 拍板 |

---

## 0. 一句话总结

Round 2 落地 `/project purge` 的**白名单 archive-only 路径**：默认 dry-run，`--confirm <token>` 才执行；token 一次性、10 分钟 TTL、消费在副作用前；**永不删除 active project**（三方 guard：originalCwd + projectRoot + sessionProjectDir）；**默认保留 memory**（`memory/` entry 在 archive 名单外）；`--include-memory` 在 memory override 配置下 reject；archive 始终通过 `~/.mossen/backups/purge-<ts>-<hex>/<sanitized>/` 持久化，不提供 `--no-archive`；Phase A 拷贝失败即 STOP，Phase B 删除失败仅记录。

## 1. 实现的能力

### 1.1 `utils/projectPurge.ts`（新增 engine）

| 新增导出 | 作用 |
|---|---|
| `PROJECT_PURGE_TOKEN_TTL_MS` | `10 * 60 * 1000`（10 min）|
| `ProjectPurgeEntry` (type) | 单个顶层 entry 描述（`name`, `absPath`, `kind`, `sizeBytes`, `isMemory`） |
| `ProjectPurgePlan` (type) | dry-run 方案（target/sanitized/projectDir/memoryStatus/toArchive/toSkip/archiveDir + token + createdAt + totalArchiveBytes） |
| `ProjectPurgeResult` (type) | confirm 结果（archived/skipped/errors/manifestPath/projectDirRemoved/phaseAHalted） |
| `ProjectPurgeError` (type) | 7 种 tagged error: `unknown_token` / `expired_token` / `active_project` / `invalid_target` / `external_memory_include_rejected` / `token_target_mismatch` / `project_dir_missing` |
| `getProjectPurgePlan({ targetCwd?, includeMemory? })` | dry-run，read-only；铸造一次性 token 入 module-level Map |
| `executeProjectPurgePlan({ token, targetCwd? })` | 消费 token → 重新 active guard → 重新 enumerate → archive (Phase A) → delete (Phase B) → manifest (Phase C) → empty-dir cleanup (Phase D) |
| `_resetProjectPurgePlanStoreForTesting()` | 仅供测试，重置 token store |

**复用的现有 helper**：
- `getOriginalCwd` / `getProjectRoot` / `getSessionProjectDir`（三方 active guard）
- `sanitizePath` / `getProjectsDir` / `findProjectDir`（路径解析，含 Bun.hash vs djb2Hash 容错）
- `getMossenConfigHomeDir`（backups 根）
- `getSettingsForSource`（settings override 探测，policy/flag/local/user 4 层）

**0 修改**：
- ❌ `bootstrap/state.ts`（仅 import）
- ❌ `utils/sessionStorage*.ts`（仅 import）
- ❌ `memdir/paths.ts`（不引用，仅检查 env vars 与 settings 字段）
- ❌ `utils/plugins/cacheUtils.ts`（W55 R1 闭环）
- ❌ `query.ts` / `processUserInput` / `Tool.ts` / `Workbench/`
- ❌ `commands/insights.ts`
- ❌ `entrypoints/sdk/controlSchemas.ts`（stream-json union 0 触动）

### 1.2 dry-run 行为（`/project purge`）

1. 解析 `--target <cwd>`、`--include-memory`、`--confirm <token>`
2. 主动 reject 6 个禁用 flag → `unsupported_flag`
3. realpath + NFC 解析 target；失败 → `invalid_target`
4. 三方 active guard：`sanitizePath(getOriginalCwd())` / `sanitizePath(getProjectRoot())` / `basename(getSessionProjectDir())` 任一匹配 → `active_project`
5. memory override 检测（env `MOSSEN_COWORK_MEMORY_PATH_OVERRIDE` / `MOSSEN_CODE_REMOTE_MEMORY_DIR` + 4 层 settings `autoMemoryDirectory`）→ `memoryStatus = 'external'`
6. 若 `--include-memory` + `external` → `external_memory_include_rejected`
7. `findProjectDir(canonical)` 容错查找；若 ENOENT → `project_dir_missing`
8. 顶层 `readdir({ withFileTypes: true })`，分桶：
   - `memory` → 默认进 `toSkip`；`--include-memory` 且 in-project 才进 `toArchive`
   - 其他 → 进 `toArchive`
9. 计算每个 entry 的 `sizeBytes`（递归累加）
10. 拼出 archive destination：`{configHome}/backups/purge-{iso-with-dashes}-{8hex}/{sanitized}/`
11. 铸 token，存入 `prunePlanStore`，返回 plan
12. **零文件系统副作用**：不 mkdir、不 copy、不 delete、不 writeFile

dry-run 输出（双语）：
- target cwd / sanitized id / project dir
- memory section（external/in-project/absent + 警告语）
- summary line：`Will archive N entries (total XMB). Skipped: M.`
- 详细 entry 列表（kind + name + size）
- archive destination 路径
- 若有 archive 内容：grace 提示 + `--confirm <token>` 命令示例（含 target/include-memory flag 回显）

### 1.3 confirm 行为（`/project purge --confirm <token>`）

1. token 查询 → 不存在 → `unknown_token`
2. token TTL > 10 min → `expired_token`（同时 evict）
3. **token 在任何副作用之前从 store 删除**（一次性，防中途异常重用）
4. 若传入 `--target` → realpath → 与 plan.targetCwd 比对，不一致 → `token_target_mismatch`
5. **重新执行三方 active guard**（防 dry-run 与 confirm 之间 session 切换）
6. **重新执行 memory override 检测**；若 plan bound includeMemory 但当前 memory 不 in-project → `external_memory_include_rejected`
7. **重新 enumerate** project dir 顶层（不复用 plan.toArchive 快照）
8. **Phase A**: `mkdir(archiveDir, { recursive: true })`，逐 entry `copyRecursiveNoSymlink`：
   - 用 `lstat` 检测 symlink 并 skip（永不 follow）
   - 任一 entry copy 失败 → 清理半成品 → `phaseAHalted = true` → 跳出，**不进 Phase B**
9. **Phase B**（仅当 Phase A 全成功）: 逐 entry `rm({ recursive: true, force: true })`；per-entry 失败记入 errors
10. **Phase C**: 写 `purge-manifest.json`（schemaVersion 1，含 archived/skipped/errors/memoryLocation/memoryExternalPath 等所有 D2 字段）
11. **Phase D**（仅当 Phase A 全成功）: 若 `readdir(originalProjectDir)` 长度 0 → `rm` project dir → `projectDirRemoved = true`

### 1.4 删除范围（硬白名单）

**只允许 touch**：
- `~/.mossen/projects/<sanitizePath(targetCwd)>/` 下被计划命中的顶层 entries（rm）
- `~/.mossen/backups/purge-<ts>-<hex>/<sanitized>/` 及其子文件（mkdir + copyFile + writeFile）
- 上述 archive dir 下的 `purge-manifest.json`（writeFile）
- 仅当 project dir 完全为空时，project dir 本身（rm）

**永不 touch**：
- ❌ `~/.mossen/settings.json` / `~/.mossen/.mossen.json` / `~/.mossen/custom-backend.env`
- ❌ `~/.mossen/history.jsonl`
- ❌ `~/.mossen/plugins/` / `plans/` / `debug/` / `file-history/` / `paste-cache/`
- ❌ `~/.mossen/session-env/` / `session-transcripts/` / `sessions/` / `shell-snapshots/`
- ❌ `~/.mossen/tasks/` / `~/.mossen/telemetry/`
- ❌ memory override 指向的外部路径（`--include-memory` 在 override 下 reject）
- ❌ symlink target（`copyRecursiveNoSymlink` 用 `lstat` skip；`rm` 默认不 follow）
- ❌ 任何 active project（三方 guard，dry-run + confirm 双拒）
- ❌ project root 的「空目录前 rm」（`remaining.length === 0` 后才允许删 project dir）
- ❌ 工作目录内任何文件
- ❌ install/update/uninstall/enable/disable 行为
- ❌ stream-json union / query loop / processUserInput / ToolUseContext / Workbench / commands/insights.ts

### 1.5 红线：force / bypass-active **未实现**

- ❌ 无 `--force`
- ❌ 无 `--yes`
- ❌ 无 `--i-know-what-im-doing`
- ❌ 无 `--all-projects`
- ❌ 无 `--orphan-only`
- ❌ 无 `--no-archive`
- ❌ 无 `forcePurge` / `bypassActive` / `skipArchive` / `purgeNoArchive` 等任何代码符号
- 解析层主动 reject 6 个 flag → `unsupported_flag` tagged error
- smoke 主动 grep 12 个 force/bypass token，命中即 FAIL

### 1.6 `commands/project/parseArgs.ts`（新增）

```ts
export type ParsedProjectCommand =
  | { type: 'menu' }
  | { type: 'help' }
  | { type: 'purge'; target?: string; includeMemory: boolean; confirmToken?: string }
  | { type: 'unsupported_flag'; flag: string }
```

forbidden flag 主动 reject（先 check forbidden，后 parse 允许的 flag）。

### 1.7 `commands/project/project.tsx`（新增 thin router）

```tsx
const parsed = parseProjectArgs(args);
if (parsed.type === 'purge') return <ProjectPurge ... />;
if (parsed.type === 'unsupported_flag') return <ProjectPurge ... unsupportedFlag={parsed.flag} />;
// help / menu → onDone(usage hint)
```

### 1.8 `commands/project/index.tsx`（新增 Command registration）

```ts
const project = {
  type: 'local-jsx',
  name: 'project',
  aliases: [],
  description: 'Manage project storage (purge sessions, preserve memory)',
  immediate: true,
  load: () => import('./project.js'),
} satisfies Command
```

### 1.9 `commands/project/ProjectPurge.tsx`（新增 UI）

- `useEffect` 一次性触发：
  - `unsupportedFlag` → 立即输出 reject 文案
  - 有 `confirmToken` → `executeProjectPurgePlan(...)` → 格式化输出
  - 否则 → `getProjectPurgePlan(...)` → 格式化 dry-run 预览
- `onComplete(text)` 把结果作为 system message 显示（与 `PluginPrune.tsx` 同款 idiom）
- i18n：每条用户文案都有 en + zh
- memory preservation 在 dry-run 文案中**双语显式**说明
- archive-only / 三方 guard / `--include-memory` consequence 在 footer 中双语提示
- 处理全部 7 种 tagged error，UI 都用 `getLocalizedText` 双语回显

### 1.10 `commands.ts`（注册）

```ts
import project from './commands/project/index.js'  // line 102
// ...
COMMANDS: [..., plugin, project, pr_comments, ...]  // line 263
```

### 1.11 `utils/commandDescription.ts`（i18n 中文映射）

新增 `case 'project':` → `'管理项目存储（清理会话、保留 memory）'`。

## 2. 安全性设计要点

### 2.1 token 安全

- 8 hex chars from `crypto.randomBytes(4)` → 同 W55 R1
- 存储在 module-level `Map<string, ProjectPurgePlan>`
- 进程重启即失效
- `evictExpiredPlans()` 每次 plan 创建 + 每次 confirm 时扫 store 移除过期项
- **一次性**：`projectPurgePlanStore.delete(opts.token)` 在 `await mkdir/copyFile/rm/writeFile` 之前 — 任何中途异常都不会留下可重用的 live token
- token 绑定 plan.targetCwd；可选 `--target` 必须 realpath 后与 plan 一致

### 2.2 三方 active guard

- `getOriginalCwd()` — 当前启动 cwd（中途 EnterWorktreeTool 会更新）
- `getProjectRoot()` — 稳定项目根（`--worktree` 时算入；中途不动）
- `getSessionProjectDir()` — 会话关联的 project dir（switchSession 触发）
- **三处任一匹配 → reject**
- dry-run 与 confirm **双重执行**（防 dry-run 后用户切换会话）
- 不依赖 cwd 字符串相等：用 `sanitizePath(canonical)` 与 `basename(sessionDir)` 比对，规避大小写/编码差异

### 2.3 memory override 防护

四种 override 检测顺序：
1. `process.env.MOSSEN_COWORK_MEMORY_PATH_OVERRIDE`
2. `process.env.MOSSEN_CODE_REMOTE_MEMORY_DIR`
3. `getSettingsForSource('policySettings').autoMemoryDirectory`
4. `getSettingsForSource('flagSettings').autoMemoryDirectory`
5. `getSettingsForSource('localSettings').autoMemoryDirectory`
6. `getSettingsForSource('userSettings').autoMemoryDirectory`

任一命中 → `memoryStatus = 'external'`，`memoryExternalReason` 记录 source，`memoryExternalHint` 记录路径。

`--include-memory` 在 external 状态下 → `external_memory_include_rejected`（dry-run + confirm 双拒）。

### 2.4 symlink 防护

`copyRecursiveNoSymlink`：
- 第一行 `lstat(src)` 而非 `stat`
- `st.isSymbolicLink()` → 直接 return（不 follow，不 copy）
- 仅 `isDirectory` / `isFile` 才递归/拷贝
- 其他特殊文件（socket / device）也 skip

`rm({ recursive: true, force: true })` — Node 默认不 follow symlink；删除的是 symlink 节点本身，不会顺着 symlink 删外部目标。

### 2.5 path traversal 防护

- `realpath(rawTarget)` 解析所有 `..` 和 symlink
- `sanitizePath` 把所有非字母数字替换成 `-`，结果一定是单一目录组件
- `path.join(getProjectsDir(), sanitized)` 拼接 → 物理上不可能逃逸到 `~/.mossen/projects/` 之外

### 2.6 Phase A halt 安全

- Phase A 第一个 entry 拷贝失败 → 立即 STOP
- 已成功拷贝的 entry **不会** 在 Phase B 被删除（因为我们直接 break，不进入 Phase B）
- 失败 entry 的半成品在 archiveDir 下被 best-effort 清理
- 用户 project dir 保持完整（除了已成功 archive 但未删除的 entry）

### 2.7 manifest 完整性

每次 confirm 都会写 `purge-manifest.json`（schemaVersion 1）：

```json
{
  "schemaVersion": 1,
  "purgedAt": "2026-05-03T...Z",
  "targetCwd": "/abs/path",
  "sanitizedTarget": "-abs-path",
  "originalProjectDir": "...",
  "includeMemory": false,
  "memoryPreserved": true,
  "memoryLocation": "in-project|external|absent",
  "memoryExternalPath": "...",     // 仅 external 时
  "memoryExternalReason": "...",   // env.* 或 settings.<source>.autoMemoryDirectory
  "archivedEntries": [{ name, kind, bytes }],
  "skippedEntries": [{ name, kind, reason }],
  "errors": [{ phase, name, message }],
  "totalArchivedBytes": 12345,
  "phaseAHalted": false
}
```

manifest 写失败也会记入 errors，不阻塞 Phase D。

## 3. 红线遵守表

| 红线 | 状态 |
|---|---|
| 不改 stream-json union | ✅（`controlSchemas.ts` 0 触动；smoke `check_no_protocol_change` 主动 grep 4 个新符号是否泄漏 → 否）|
| 不改 query loop | ✅（smoke `check_main_loop_clean` 锁 `query.ts` / `processUserInput` / `Tool.ts`）|
| 不改 processUserInput | ✅ |
| 不改 ToolUseContext | ✅ |
| 不改 Workbench | ✅ |
| 不改 `commands/insights.ts` | ✅（未 stage 未改）|
| 不删 active project | ✅（三方 guard，dry-run + confirm 双拒；smoke `check_engine_active_guard_double` 锁 ≥3 处 detectActiveProject 调用）|
| 不直接 fs.rm project root | ✅（仅在 `remaining.length === 0` 后；smoke `check_engine_no_project_root_rm` 锁）|
| 不删 memory（默认）| ✅（白名单：name === 'memory' 自动进 toSkip；i18n 双语提示）|
| 不越界外部 memory 路径 | ✅（`--include-memory` + external → reject；6 处 settings 探测覆盖；smoke `check_engine_memory_override` 锁三个关键字符串）|
| 不实现 force / no-archive / yes / i-know-what / all-projects / orphan-only | ✅（parseArgs 主动 reject + smoke `check_engine_no_force_flag` + `check_parse_args` 双锁）|
| 不 follow symlink 删除外部目标 | ✅（`lstat + isSymbolicLink` skip；smoke `check_engine_symlink_safety` 锁）|
| 不修改非 projects/+backups/ 目录 | ✅（smoke `check_engine_no_sibling_literals` 主动 grep 15 条 sibling literal）|
| 不引入新协议 / dispatcher / smoke 跨界 | ✅ |
| 不 push / 不 push tags / 不 push GitHub | ✅ |

## 4. 验证记录

| 验证 | 结果 |
|---|---|
| `git diff --check` | 0 whitespace 错误 |
| `python3 scripts/wave_w55_project_purge_smoke.py` | **PASS**（19 项契约首次即通过，含 token-safety 函数体内扫描）|
| `python3 scripts/wave_w55_plugin_prune_smoke.py` | **PASS**（W55 R1 已移除 obsolete `check_no_project_purge`）|
| `python3 scripts/wave_w54_second_tier_low_risk_smoke.py` | **PASS**（W54 已移除 obsolete `check_no_purge_or_prune_commands`）|
| `bun run typecheck:diff` | ✅ 0 new（baseline 1384 → current 1079）|
| `bun run lint:diff` | ✅ 0 new（baseline 943 → current 936）|
| `python3 scripts/audit_hardcoded_user_text.py` | ✅ allowlist updated（+1：`commands/project/index.tsx:7`）|
| `bash scripts/run_all_smoke.sh` | **ALL PASS** |
| case 39 fingerprint | **stable** = `870f99ed494d3d145ed2eb1368132299`（未漂移）|

## 5. 与 Round 1 的对比

| 维度 | W55 R1 (`/plugin prune`) | W55 R2 (`/project purge`) |
|---|---|---|
| 引擎已存在 | ✅（`cleanupOrphanedPluginVersionsInBackground`）| ❌ 全新 |
| 默认 dry-run | ✅ | ✅ |
| token TTL | 10 min | 10 min |
| 一次性 token | ✅（`prunePlanStore.delete` 在 rm 前）| ✅（`projectPurgePlanStore.delete` 在 mkdir/copy 前）|
| 7 天 grace | ✅（CLEANUP_AGE_MS）| N/A（archive 模式取代 grace）|
| archive | ❌（直接 rm 过期 orphan）| ✅（强制 archive 到 backups）|
| 双重 active guard | N/A（plugin cache 不分项目）| ✅（三方 + dry-run/confirm 双拒）|
| memory 保护 | N/A | ✅（默认保留；`--include-memory` 在 override 下 reject）|
| symlink 防护 | N/A（cache 一定是 owned dirs）| ✅（lstat + skip）|
| Phase A halt 安全 | N/A（按 entry 独立处理）| ✅（首次 copy 失败即 STOP）|
| manifest | N/A | ✅（schemaVersion 1）|

## 6. Commit 结构

| Commit | 内容 |
|---|---|
| feat(project): add safe `/project purge` command | utils/projectPurge.ts + commands/project/{parseArgs.ts,project.tsx,ProjectPurge.tsx,index.tsx} + commands.ts 注册 + commandDescription.ts i18n |
| test(project): cover W55 project purge contract | scripts/wave_w55_project_purge_smoke.py + run_all_smoke.sh 注册 + W54/W55-R1 obsolete deferral guard 移除 + audit allowlist +1 |
| docs(upgrade): record W55 round 2 project purge | 本文档 |

## 7. 下一步

- 等 Allen 预览 Round 2 实现 → 单独 push origin/main（不 push tag、不 push GitHub）
- Round 2 全部 D1–D10 决策落实
- W55 sprint 收尾；后续档次留给 Allen 决策（Round 3 还未启动）
