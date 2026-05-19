# W54 第二档低风险打包施工 · Round 1 归档

## 元信息

| 字段 | 值 |
|---|---|
| 日期 | 2026-05-03 |
| 起点 HEAD | `8538a01` (W53 archive) |
| 输入归档 | `docs/upgrade/W53-second-tier-archive.md` |
| 范围 | D4 bypass-immune 列表扩展 / C4 Stale Session 提示 / C8 `/skills` 搜索框 |
| 性质 | feat + test + docs（无协议 union / 主循环 / ToolUseContext / Workbench / push）|
| 决议 | C5 project purge / C6 plugin prune **deferred to W55**（单独 mutation wave） |

## 0. 一句话总结

W53 §6 推荐的 5 项中的低风险 3 项已落地（D4 + C4 + C8）。两项删除类命令 C5/C6 按 Allen 拍板转入 W55 单独 mutation wave，这一档不动。

## 1. 已完成项

### 1.1 D4 — bypass-immune 危险路径列表扩展

**改动文件**：`utils/permissions/filesystem.ts`

**新增 `DANGEROUS_FILES`**（exact basename）：
- `.npmrc`、`.pypirc`、`.netrc`（包管理器凭证）
- `.env`（单文件 env，suffixed 变体走下面 prefix matcher）
- `authorized_keys`、`id_rsa`、`id_ed25519`（SSH key material 的 defense-in-depth）
- `credentials`（兜底 `~/.aws/credentials` 与非标准位置的同名文件）

**新增 `DANGEROUS_DIRECTORIES`**（path-segment match）：
- `.ssh`、`.aws`、`.kube`、`.docker`（云 / 凭证目录全量保护）

**新增 `DANGEROUS_FILE_PREFIXES`**（**新导出**）：
- `.env.` — 一条规则覆盖 `.env.local` / `.env.production` / `.env.development` / `.env.test` 等所有 suffix 变体
- 已在 `isDangerousFilePathToAutoEdit` 内串接 `.startsWith(prefix)` 匹配（不是孤儿列表）

**没有改的**：
- `checkPathSafetyForAutoEdit` 入口
- `permissions.ts:1146/1254` 的 bypass-immune 通路
- 分类器主流程 / `dangerousPatterns.ts`（那是 bash command 列表，与本项无关）
- Allen 明确说 `config` 仅在 `.ssh/config` 场景保护 → 已通过 `.ssh` 目录覆盖，**未**把通用 `config` 加进 file 列表

### 1.2 C4 — Stale Session 提示（mtime 启发式）

**新建文件**：`utils/staleSession.ts`（pure helper，无 IO，无 React）
- 导出 `STALE_SESSION_THRESHOLD_DAYS = 7`
- 导出 `getStaleSessionAgeDays(modified, now?)`
- 导出 `isSessionStale(modified, now?)`

**改动文件**：`components/LogSelector.tsx`
- 引入两个 helper
- 在 `buildLogMetadata` 末尾追加 `buildStaleSuffix(log.modified)`
- 仅当 mtime ≥ 7 天前才追加；显示文案 `· stale Nd` / `· 过期 N 天`（i18n via `getLocalizedText`）

**契约**：
- ❌ 不写新 schema 字段（不动 `types/logs.ts`，无 `lastActiveAt` / `sessionAge` / `isStale`）
- ❌ 不自动 resume / 不自动 compact / 不自动写 memory / 不删数据
- ✅ 仅 `LogSelector` 行尾 dim 文案追加，纯只读
- ✅ mtime 是文件系统的 modify-time 启发式（touch / 复制可能漂移），文档已说明

**为何选 mtime 而非新 lastActiveAt 字段**：mossen 当前无 `lastActiveAt` 字段；新增 schema 字段属于 Slice 3 改动需要单独 confirm。mtime 已在 `LogOption.modified` 暴露，零迁移成本，启发式精度对"提示用户注意旧会话"的价值场景足够。

### 1.3 C8 — `/skills` 搜索框

**改动文件**：`components/skills/SkillsMenu.tsx`（整文件重写为 clean React，去除历史 React Compiler 编译产物 `_c(35)` 缓存）

**新增能力**：
- 顶部 `🔍 ` 前缀 + `TextInput`（复用现有组件，键盘导航不变）
- `useState<string>('')` 跟踪 `searchQuery`，`useState<number>` 跟踪 `cursorOffset`
- `matchesSearch(skill, normalizedQuery)` 在 name + description + source label + plugin name 上做 case-insensitive 子串匹配
- 搜索为空 → 显示全部 skills（按现有 source 分组）
- 搜索无结果 → 单独 empty-state 卡片：`No skills match "<q>"` / `没有匹配 "<q>" 的技能`
- 顶部计数 i18n：`X of Y skills` / `X / Y 个技能`

**没有改的**：
- ❌ Skill loader（`skills/loadSkillsDir.ts`）
- ❌ Skill 注册 / 加载 / invocation 路径
- ❌ Skill 权限系统
- ❌ Source 分类逻辑（仍然 7 类：policy/user/project/local/flag/plugin/mcp）

**Hook 顺序合规**：所有 `useMemo`/`useState` 都在 `if (allSkills.length === 0) return ...` 之前，避免 `react-hooks/rules-of-hooks` 错误（lint:diff 验证 0 new）。

## 2. Deferred to W55

| 项 | Deferred 原因 |
|---|---|
| **C5 project purge 命令** | 涉及 `~/.mossen/projects/<encoded-cwd>/` 文件系统删除；mutation wave 单独走 |
| **C6 plugin prune 命令** | 涉及 `~/.mossen/plugins/cache/` 删除 + registry 对齐；mutation wave 单独走 |

W55 启动条件：先列出 cache 扫描算法、installed registry 对齐策略、dry-run 行为、`--confirm` 语义、删除边界、是否需要 backup/rollback —— 等 Allen 二次拍板再动手。

## 3. 红线遵守表

| 红线 | 状态 |
|---|---|
| stream-json union 不变 | ✅（仍 29，未触 `controlSchemas.ts` / `coreSchemas.ts`）|
| query loop 不变 | ✅（`query.ts` 0 修改）|
| processUserInput 不变 | ✅（`utils/processUserInput/` 0 修改）|
| ToolUseContext 不变 | ✅（`Tool.ts` 0 修改）|
| 权限分类器主逻辑不变 | ✅（`yoloClassifier.ts` / `permissions.ts` 主路径未触；只扩 `filesystem.ts` 数据 + 新增 prefix matcher）|
| Workbench 不动 | ✅ |
| `commands/insights.ts` WIP 保护 | ✅（未 stage 未改）|
| 不 push / 不 push tags / 不 push GitHub | ✅（最终报告确认）|

## 4. 验证记录

| 验证 | 结果 |
|---|---|
| `git diff --check` | 0 whitespace 错误 |
| 新 W54 smoke | PASS |
| `bash scripts/run_all_smoke.sh` | **ALL PASS**（含 typecheck:diff 0 new / lint:diff 0 new / 12+ wave smoke / layer audit / case 39 fingerprint = `870f99ed494d3d145ed2eb1368132299` 稳定）|
| typecheck baseline drift | 1080 → 1079（-1，未新增） |
| lint baseline drift | 938 → 936（-7 fixed） |

## 5. Commit 结构

| Commit | 内容 |
|---|---|
| feat(permissions/skills/session): low-risk W54 round-1 batch | D4 + C4 + C8 实现 |
| test(cli): cover W54 round-1 low-risk batch | 新 smoke + run_all_smoke.sh 注册 |
| docs(upgrade): record W54 round 1 + C5/C6 deferred | 本文档 |

## 6. 下一步建议

- 若 Allen 满意 W54 Round 1 → 推送到 origin（仅 origin/main，不动 GitHub / tags）等指令
- 若启动 W55 mutation wave → 先产 C5/C6 可行性表（cache 扫描 + registry 对齐 + dry-run 设计），等 Allen 二次拍板再动手
