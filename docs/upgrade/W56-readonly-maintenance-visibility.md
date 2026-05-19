# W56 · Read-only maintenance visibility

## 元信息

| 字段 | 值 |
|---|---|
| 日期 | 2026-05-03 |
| 起点 HEAD | `96e9870`（W55 R2 push）|
| 范围 | 6 of 8 子项落地（2 个 deferred）|
| 性质 | feat + test + docs（read-only 只展示，无 mutation / 无协议 / 无主循环 / 无 Workbench / 无 push）|

## 0. 一句话总结

W56 是 W55 mutation 流的可视化对手戏 —— 引入 `/project list` / `/project status` / `/plugin status` 三个**全只读** slash 命令让用户在执行 `/project purge` / `/plugin prune` 之前能先 audit；并扩展 `/memory`（metadata pane）+ `SkillsMenu`（source filter）+ 在 `/project status` 内嵌 `~/.mossen/{debug,backups,plugins}/` 大小概览。两个高风险子项（`/doctor` 增强 + LogSelector stale filter）因目标文件是 React-compiler 编译输出（739 / 1707 行）被主动 deferred。

## 1. 完成的 6 个子项

### 1.1 `/project list` ✅

**新增**：`commands/project/ProjectList.tsx`（203 行）+ `utils/projectInventory.ts`（含 `buildProjectInventory()`）。

**输出字段**（每个 project entry）：
- sanitized id
- inferred cwd（高/低置信度，低置信度时显式标 `(opaque id)`）
- project dir 绝对路径
- session jsonl count + sub-session dir count
- memory presence + file count + size
- total size（递归字节）
- last modified（相对时间："today" / "Nd ago"）
- ACTIVE 标记（命中 originalCwd / projectRoot / sessionProjectDir 任一）
- STALE 标记（mtime > 7 天，复用 `utils/staleSession.ts:isSessionStale`）

**aggregate 信息**：项目总数、聚合大小、active 数量、stale 数量。

**结尾提示**：建议 `/project purge --target <cwd>` 清理 + active 项目受保护。

### 1.2 `/project status` ✅

**新增**：`commands/project/ProjectStatus.tsx`（260 行）+ `utils/projectInventory.ts:describeActiveProjectStatus()`。

**输出字段**：
- 三方 active 标记：`originalCwd` / `projectRoot` / `sessionProjectDir` / `activeSanitized` set
- 当前活动项目 dir + session 数 + 总大小 + modified 时间
- memory 状态（`in-project` / `external` / `absent`）+ path + reason + file count + size
- purge 资格：**REJECTED**（active-project guard）+ 引导 `/project list` → `/project purge --target <cwd>`
- 兄弟 cache 概览：`~/.mossen/debug/` / `~/.mossen/backups/` / `~/.mossen/plugins/`（每条 path + size + entry count）

### 1.3 `/plugin status` ✅

**新增**：`commands/plugin/PluginStatus.tsx`（206 行）+ `utils/plugins/statusOps.ts`（96 行）+ `utils/plugins/cacheUtils.ts:summarizePluginCache()` 新增导出。

**输出字段**：
- plugin root path（存在性）
- cache path
- marketplaces dir（存在性）
- installed registry path（可加载性）
- registry counts: installed plugin count + installed version count
- cache counts: marketplace / unique plugin / cache version / total bytes
- orphan classification: expired (>7d) / unmarked / fresh (<=7d) / installed-skipped
- prune eligibility（YES / no orphans / zip-cache 模式）
- 建议命令：`/plugin prune` 或 zip-cache 提示

**关键设计**：`summarizePluginCache()` 复用 W55 R1 orphan classifier 的相同逻辑（marketplace 遍历 / version stat / `.orphaned_at` mtime / `CLEANUP_AGE_MS = 7d`），**不复制不漂移**。`statusOps.ts` 调用 `summarizePluginCache()` + `loadInstalledPluginsFromDisk()` 拼成总览，全程零 mutation：smoke 主动 grep `await rm/unlink/rename/writeFile/mkdir` + `markPluginVersionOrphaned`，命中即 FAIL。

### 1.5 `/memory` metadata pane ✅

**修改**：`commands/memory/memory.tsx` — 加 `<MemoryMetadataPane />` 组件，渲染于 `<MemoryFileSelector />` 之上。

**展示字段**：
- auto memory enabled（`isAutoMemoryEnabled()`）
- team memory enabled（`isTeamMemoryEnabled()`）
- memory location（`in-project` / `external` / `absent`）+ 外部 reason
- file count + total size

**显式安全提示**："metadata only — file contents are never displayed here" / "（仅显示元数据 —— 此处不会展示 memory 文件内容）"。

**Smoke 主动验证**：grep `MemoryMetadataPane` 函数体内不含 `readFile(`（防 contents 泄漏）。

### 1.6 `/skills` source filter ✅

**修改**：`components/skills/SkillsMenu.tsx` —— 在 W54 C8 search box 上面追加 source 过滤 chip 条。

**行为**：
- chip 顺序：`all / projectSettings / userSettings / policySettings / plugin / mcp`
- 当前选中的 chip 用 `inverse` 高亮
- `Tab` 向前 cycle / `Shift+Tab` 向后 cycle
- 过滤 + 搜索 query 双重应用（先按 source 过滤，再按 search query 过滤）

**保护**：
- 不动 skill loader（`skills/loadSkillsDir.ts`）
- 不动 skill invocation 路径
- smoke 主动 grep loader 内不出现 `mutateSkill / writeSkill / deleteSkill` 等符号

### 1.8 debug/cache size summary ✅（嵌入 /project status）

**复用** `utils/projectInventory.ts:summarizeCacheDir()` —— 一个纯只读的 `readdir + walk` 走子。`/project status` 输出包含 3 条：`debug/` / `backups/` / `plugins/`，每条 path + size + entry count。

## 2. 主动 Deferred 的 2 个子项

### 2.1 ⏸ `/doctor` 增强（DEFERRED）

`screens/Doctor.tsx` 是 **739 行** React-compiler 编译输出（`_c(N)` cache hooks），手编辑添加额外 sections 风险高。Allen 的 W56 spec 允许"如果某个子项需要... 那个子项立即 deferred"。

**等价补偿**：project / plugin / memory 的 readiness 信息已经通过：
- `/project list`（项目 inventory）
- `/project status`（active project + cache 概览 + memory 状态）
- `/plugin status`（cache + orphan + registry）

…全部以 slash 命令独立访问到。doctor 的"我能不能 purge / prune"提示效果由 `/project status` 的 "Purge eligibility: REJECTED + 建议命令" 段落和 `/plugin status` 的 "Suggested: /plugin prune" 段落分担。

未来若需 doctor 真合并这些 readiness check，留给 Round 2（高风险，需要单独的施工包）。

### 2.2 ⏸ LogSelector stale filter（DEFERRED）

`components/LogSelector.tsx` 是 **1707 行** React-compiler 编译输出，hook 序列、filter pipeline 都被编译器扁平化。手编辑 stale filter 会要在编译输出顶层精确插入 `useState` + `useEffect` + filter 链上一节，风险高。

**等价补偿**：W54 C4 已经在每条 session entry 的 metadata 行加了 stale 后缀（"3d stale"），用户能眼看出来。后续若要 LogSelector 真支持过滤，需要单独的 Round（最好同时把 LogSelector "去编译输出化" 改回纯 React-compiler 不影响的源）。

## 3. 红线遵守表

| 红线 | 状态 |
|---|---|
| 不改 stream-json union | ✅（`controlSchemas.ts` 0 触动；smoke 主动 grep 5 个新符号是否泄漏 → 否）|
| 不改 query loop | ✅ |
| 不改 processUserInput | ✅ |
| 不改 ToolUseContext | ✅ |
| 不改 Workbench | ✅ |
| 不改 `commands/insights.ts` | ✅ |
| 不写 memory | ✅（metadata pane 只 stat / readdir，main `editFileInEditor` 路径未触动）|
| 不删除任何文件 | ✅（smoke 主动 grep `rm/unlink/rename/writeFile/mkdir` 在新 helper 中 → 0 命中）|
| 不修改 installed_plugins.json | ✅（statusOps 仅 `loadInstalledPluginsFromDisk`）|
| 不写 `.orphaned_at` | ✅（summarizePluginCache 仅 stat marker，从不 writeFile）|
| 不修改 skill loader / invocation | ✅（SkillsMenu 改的是 UI 层；smoke 主动 grep loader 文件无 mutation 符号）|
| 不新增 stream-json subtype | ✅ |
| 不新增 mutation flag | ✅ |
| 不 push / 不 push tags / 不 push GitHub | ✅ |

## 4. 验证记录

| 验证 | 结果 |
|---|---|
| `git diff --check` | 0 whitespace 错误 |
| `python3 scripts/wave_w56_readonly_visibility_smoke.py` | **PASS**（首次即通过）|
| `bash scripts/run_all_smoke.sh` | **ALL PASS**（含 W55 R1 + R2 + W54 + 16 个先前 wave smoke）|
| `bun run typecheck:diff` | ✅ 0 new（baseline 1384 → current 1079）|
| `bun run lint:diff` | ✅ 0 new（baseline 943 → current 936）|
| `python3 scripts/audit_hardcoded_user_text.py` | ✅ 191 hits all in baseline（0 new）|
| case 39 fingerprint | **stable** = `870f99ed494d3d145ed2eb1368132299` |

## 5. Commit 结构

| Commit | 内容 |
|---|---|
| feat(cli): add read-only maintenance visibility commands | utils/projectInventory.ts + utils/plugins/statusOps.ts + utils/plugins/cacheUtils.ts:summarizePluginCache + commands/project/{ProjectList.tsx,ProjectStatus.tsx,parseArgs.ts,project.tsx} + commands/plugin/{PluginStatus.tsx,parseArgs.ts,plugin.tsx} + commands/memory/memory.tsx metadata pane + components/skills/SkillsMenu.tsx source filter |
| test(cli): cover W56 read-only visibility contracts | scripts/wave_w56_readonly_visibility_smoke.py + run_all_smoke.sh 注册 |
| docs(upgrade): record W56 read-only maintenance visibility | 本文档 |

## 6. 复用 vs 新增

| 行为 | 来源 |
|---|---|
| 三方 active markers | 复用 W55 R2 `bootstrap/state.ts` 的 `getOriginalCwd` / `getProjectRoot` / `getSessionProjectDir` |
| stale 阈值（7 天）| 复用 W54 C4 `utils/staleSession.ts:isSessionStale` |
| sanitized id / project dir 解析 | 复用 `utils/sessionStoragePortable.ts:sanitizePath / getProjectsDir` |
| memory override 检测（4 层）| **新增** `utils/projectInventory.ts:detectMemoryOverride`（与 `utils/projectPurge.ts` 同 idiom，独立实现避免循环依赖）|
| 插件 orphan 分类 | **新增导出** `utils/plugins/cacheUtils.ts:summarizePluginCache`（与 `getPluginPrunePlan` 同样的 4 桶逻辑，但不发 token、不进 plan store）|
| installed plugin registry 读取 | 复用 `utils/plugins/installedPluginsManager.ts:loadInstalledPluginsFromDisk` |
| memory 启用 gate | 复用 `memdir/paths.ts:isAutoMemoryEnabled` + `memdir/teamMemPaths.ts:isTeamMemoryEnabled` |

## 7. Smoke 设计要点

`scripts/wave_w56_readonly_visibility_smoke.py` 含 16 项契约 check：

| # | 检查 | 锁住的 |
|---|---|---|
| 1 | inventory engine 导出 5 个核心函数 | API 完整 |
| 2 | inventory 用三方 active markers | 一致性 |
| 3 | inventory 无任何 `await rm/unlink/rename/writeFile/mkdir/copyFile` | 只读红线 |
| 4 | inventory 不引用 settings.json/.mossen.json/custom-backend.env/history.jsonl 字面量 | 边界 |
| 5 | inventory 用 `isSessionStale` | 单一阈值源 |
| 6 | statusOps 调用 `summarizePluginCache + loadInstalledPluginsFromDisk` | 复用 vs drift |
| 7 | statusOps 无 mutation 调用 + 无 `markPluginVersionOrphaned` | 只读红线 |
| 8 | `summarizePluginCache` 函数体无 mutation | 锁源 |
| 9 | parseArgs 含 `'list'` `'status'` case | 路由完整 |
| 10 | router 在 plugin/project 都有 `parsed.type === 'status'` 分支 | 路由完整 |
| 11 | `ProjectList.tsx` 含 ACTIVE / STALE / `/project purge` / sanitized / session / memory 关键词 | 用户面 |
| 12 | `ProjectStatus.tsx` 含三方 active marker / 三态 memory / REJECTED 守卫 + 不含 `--force/--yes/--no-archive/--all-projects` | 用户面 + 红线 |
| 13 | `PluginStatus.tsx` 含 8 个关键 count 字段 + `/plugin prune` 引导 | 用户面 |
| 14 | `memory.tsx` 含 MemoryMetadataPane + isAutoMemoryEnabled + isTeamMemoryEnabled + describeMemoryState + 显式 "metadata only" 双语 + 函数体内无 `readFile(` | 隐私红线 |
| 15 | SkillsMenu 含 `SOURCE_FILTER_ORDER` + `sourceFilter` state + `useInput` + loader 文件无 mutation 符号 | 行为锁 |
| 16 | controlSchemas / query.ts / processUserInput / Tool.ts 不含 W56 helper 符号 | 边界 |
| ‒ | run_all_smoke.sh 注册 | gating |
| ‒ | commands/insights.ts 仍存在 | WIP 保护 |

## 8. 下一步

- 等 Allen 预览 → 单独 push origin/main（不 push tag、不 push GitHub）
- 后续 Round 候选：
  - Doctor 真合并 readiness（需要先把 Doctor.tsx 去编译输出化）
  - LogSelector stale filter（需要先把 LogSelector.tsx 去编译输出化）
  - `/plugin status --json` 输出（如需 SDK 消费）—— 需要 Allen 拍板 protocol 扩展
