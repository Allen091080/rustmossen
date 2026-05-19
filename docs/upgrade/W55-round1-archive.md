# W55 Round 1 · `/plugin prune` 用户入口归档

## 元信息

| 字段 | 值 |
|---|---|
| 日期 | 2026-05-03 |
| 起点 HEAD | `d956d1a`（W54 round-1 push）|
| 范围 | C6 `/plugin prune` dry-run + `--confirm <token>` |
| 性质 | feat + test + docs（无协议 union / 主循环 / ToolUseContext / Workbench / push）|
| 决议 | C5 `project purge` **仍 deferred 到 Round 2** |

---

## 0. 一句话总结

C6 的真实 gap **不是底层引擎**——`utils/plugins/cacheUtils.ts` 的 `cleanupOrphanedPluginVersionsInBackground` 已是完整 prune pipeline（mark `.orphaned_at` → 7 天后 `fs.rm` → 清空目录）。Round 1 只补**用户主动入口**：`/plugin prune` 默认 dry-run，`--confirm <token>` 才执行；token 一次性、10 分钟 TTL、消费在副作用前；**永不绕过 7 天 grace**。

## 1. 实现的能力

### 1.1 `cacheUtils.ts` 加 2 个 public helper（复用现有 private 引擎）

| 新增导出 | 作用 |
|---|---|
| `PRUNE_PLAN_TOKEN_TTL_MS` | `10 * 60 * 1000`（10 min）|
| `PrunePlanEntry` (type) | `{ versionPath, marketplace, plugin, version, orphanedAtMs, ageDays, sizeBytes }` |
| `PluginPrunePlan` (type) | 4 个分桶 + token + createdAt + zipCacheMode |
| `PluginPruneResult` (type) | `{ marked, deleted, cleanedDirs, errors }` |
| `PluginPruneError` (type) | tagged union: `unknown_token` / `expired_token` / `zip_cache_mode` |
| `getPluginPrunePlan()` | **read-only** 扫描，分类 cache 中所有 version 进 4 桶，铸 token 入 module-level Map |
| `executePluginPrunePlan(token)` | 消费 token → 重新读 installed registry → mark → delete → 清空目录 |
| `_resetPrunePlanStoreForTesting()` | 仅供测试，重置 token store |

**复用的现有 private 引擎**（一行不动）：
- `markPluginVersionOrphaned`（写 `.orphaned_at`）
- `getOrphanedAtPath`（marker 路径）
- `getInstalledVersionPaths`（registry 读取）
- `readSubdirs`（cache 目录枚举）
- `CLEANUP_AGE_MS = 7 * 24 * 60 * 60 * 1000`（7 天 grace 常量）
- `cleanupOrphanedPluginVersionsInBackground`（自动后台 pruner）

### 1.2 dry-run 行为（`/plugin prune`）

1. 扫 `~/.mossen/plugins/cache/<m>/<p>/<v>/` 整树
2. 比对 `installed_plugins.json` 中的 `installPath`
3. 分类入 4 桶：
   - **expiredOrphans**: 有 `.orphaned_at` 且 mtime > 7 天前 → 确认后**删除**
   - **unmarkedOrphans**: 不在 installed registry 且无 marker → 确认后**只标记**（不删！）
   - **freshOrphans**: 有 marker 但 < 7 天 → 显示但**不动**
   - **installedSkipped**: 在 registry 中 → **永不动**
4. 输出每个 version 的路径 / 大小估算 / 标记年龄
5. 输出 token + 10 分钟 TTL + `--confirm <token>` 提示
6. 输出明确的 7 天 grace 说明：「新发现 orphan 只会先标记，不会立刻删除」
7. **零文件系统副作用**：不写 marker、不删任何目录

### 1.3 confirm 行为（`/plugin prune --confirm <token>`）

1. token 查询 → 若不存在 → 报错 `unknown_token`
2. token 查询 → 若 > 10 min → 删除并报错 `expired_token`
3. **token 在任何副作用之前从 store 删除**（一次性，防中途异常重用）
4. 重新读 `getInstalledVersionPaths()`（防止 dry-run 与 confirm 之间用户重装）
5. **Phase 1**：对 unmarkedOrphans 写 `.orphaned_at`（不删！）
6. **Phase 2**：对 expiredOrphans 重新 stat marker：
   - 若 marker 不在了 → skip（用户重装了）
   - 若 marker 年龄突然 < 7 天 → skip（marker 被重置）
   - 否则 → `fs.rm(versionPath, { recursive: true, force: true })`
7. **Phase 3**：清空 plugin / marketplace 父目录（仅当真空时）
8. 输出 `marked / deleted / cleanedDirs / errors` 计数

### 1.4 删除范围（硬白名单）

**只允许 touch**：
- `~/.mossen/plugins/cache/<marketplace>/<plugin>/<version>/`（删 version 目录）
- 同 version 目录内的 `.orphaned_at` 文件（写 marker）
- 完全空的父目录（删空 plugin/marketplace dir）

**永不 touch**：
- ❌ `installed_plugins.json` / `installed_plugins_v2.json`（registry 不修改，仅读）
- ❌ `~/.mossen/plugins/marketplaces/`（market 元数据）
- ❌ plugin settings / pluginOptions
- ❌ user-authored plugin 源
- ❌ symlink 目标（`fs.rm` 默认 recursive 但不 follow symlink；不实测但代码无 follow 选项）
- ❌ install registry 引用的版本（`isStillOrphan` 二次校验）
- ❌ install/update/uninstall/enable/disable 行为

### 1.5 红线：force-prune **未实现**

- ❌ 无 `--force`
- ❌ 无 `--bypass-grace`
- ❌ 无 `--i-know-what-im-doing`
- ❌ 无 `forcePrune` / `bypassGrace` 等任何代码符号
- 对应 smoke `check_cache_utils_no_force_flag` 主动 grep 这些 token，命中即 FAIL

### 1.6 `parseArgs.ts` 新增

```ts
export type ParsedCommand =
  | ...existing 9 cases...
  | { type: 'prune'; confirmToken?: string }

case 'prune': {
  const flagIdx = parts.findIndex(p => p === '--confirm')
  const confirmToken = flagIdx >= 0 ? parts[flagIdx + 1]?.trim() || undefined : undefined
  return { type: 'prune', confirmToken }
}
```

### 1.7 `plugin.tsx` 路由（薄路由层）

```tsx
const parsed = parsePluginArgs(args);
if (parsed.type === 'prune') {
  return <PluginPrune onComplete={onDone} confirmToken={parsed.confirmToken} />;
}
return <PluginSettings onComplete={onDone} args={args} />;
```

PluginSettings 完全未触动。

### 1.8 `PluginPrune.tsx` UI

- `useEffect` 一次性触发：
  - 有 `confirmToken` → `executePluginPrunePlan(token)` → 格式化输出
  - 否则 → `getPluginPrunePlan()` → 格式化 dry-run 预览
- `onComplete(text)` 把结果作为 system message 显示（与 `ValidatePlugin.tsx` 同款 idiom）
- i18n：每条用户文案都有 en + zh
- 7 天 grace 在 dry-run 文案中**双语显式**说明

## 2. 安全性设计要点

### 2.1 token 安全

- 8 hex chars from `crypto.randomBytes(4)` → `5cf94de7` 风格，足够防猜
- 存储在 module-level `Map<string, PluginPrunePlan>`
- 进程重启即失效（不持久化）
- `evictExpiredPlans()` 每次 plan 创建 + 每次 confirm 时扫 store 移除过期项
- **一次性**：`prunePlanStore.delete(token)` 在 `await rm(...)` 之前调用——任何中途异常都不会留下可重用的 live token

### 2.2 防 dry-run/confirm 之间状态漂移

- confirm 时**重新读** `getInstalledVersionPaths()`（不复用 dry-run 时的快照）
- 用户在两步之间重装某 plugin → 该 version 立即被识别为 installed → 跳过
- expiredOrphans 在 delete 前再次 stat `.orphaned_at`：
  - 若 marker 已不在 → skip
  - 若 marker mtime 重置使年龄 < 7 天 → skip
- 用户在 confirm 期间撤销 orphan（重装）也不会被误删

### 2.3 zip-cache 模式

- mossen 支持 `isPluginZipCacheEnabled()` 模式（plugin 以 .zip 形式缓存）
- 该模式下 cleanupOrphanedPluginVersionsInBackground 早 return；新 prune 路径同样早 return
- dry-run 输出明确告知用户 zip-cache 模式不支持 prune
- confirm 路径返回 `{ kind: 'zip_cache_mode' }`，UI 显示对应 i18n 文案

## 3. 红线遵守表

| 红线 | 状态 |
|---|---|
| 不改 stream-json union | ✅（`controlSchemas.ts` 0 触动；smoke 主动 grep 4 个新符号是否泄漏 → 否）|
| 不改 query loop | ✅ |
| 不改 processUserInput | ✅ |
| 不改 ToolUseContext | ✅ |
| 不改 Workbench | ✅ |
| 不改 `commands/insights.ts` | ✅（未 stage 未改）|
| 不改 install registry 行为 | ✅（cacheUtils 不引用 `addPluginInstallation` / `removePluginInstallation` / `updateInstallationPathOnDisk`）|
| 不绕 7 天 grace | ✅（`CLEANUP_AGE_MS` 在 dry-run + confirm 双路径消费；smoke 锁）|
| 不实现 force-prune | ✅（smoke 主动黑名单 9 个 force/bypass token）|
| 不删 in-use plugin | ✅（`isStillOrphan` 二次校验 + 复用 `getInstalledVersionPaths`）|
| 不修改 marketplace metadata | ✅（删除范围硬白名单仅 cache/<m>/<p>/<v>/）|
| 不改自动后台 pruner | ✅（`cleanupOrphanedPluginVersionsInBackground` 0 改动）|
| 不引入新协议 / dispatcher / smoke 跨界 | ✅ |
| 不 push / 不 push tags / 不 push GitHub | ✅ |

## 4. 验证记录

| 验证 | 结果 |
|---|---|
| `git diff --check` | 0 whitespace 错误 |
| `python3 scripts/wave_w55_plugin_prune_smoke.py` | **PASS**（首次即通过 14 项契约）|
| `bun run typecheck:diff` | ✅ 0 new（baseline 1384 → current 1079）|
| `bun run lint:diff` | ✅ 0 new（baseline 943 → current 936）|
| `bash scripts/run_all_smoke.sh` | **ALL PASS**（含 13 wave smoke + layer audit）|
| case 39 fingerprint | **stable** = `870f99ed494d3d145ed2eb1368132299`（未漂移）|

## 5. C5 仍 deferred

C5 `/project purge` 按 W55 Round 0 决议仍**未实现**：

- ❌ 没有 `commands/project/` 目录
- ❌ 没有 `utils/projectPurge.ts`
- ❌ Round 1 不触 `~/.mossen/projects/<encoded-cwd>/`
- ⏸ 等 Round 1 push 后单独启动 Round 2

## 6. Commit 结构

| Commit | 内容 |
|---|---|
| feat(plugin): add safe `/plugin prune` command | cacheUtils 加 2 个 helper + parseArgs + plugin.tsx router + PluginPrune.tsx |
| test(plugin): cover W55 plugin prune contract | scripts/wave_w55_plugin_prune_smoke.py + run_all_smoke.sh 注册 |
| docs(upgrade): record W55 round 1 plugin prune | 本文档 |

## 7. 下一步

- 等 Allen 预览 Round 1 实现 → 单独 push origin/main（不 push tag、不 push GitHub）
- Round 2 启动条件：Round 1 push 完成 + Allen 二次拍板 C5 设计细节（archive 路径 / token 风格 / cwd-encoded 探测细节）
