# Wave 3 完成记录 — 死代码清理 + 命名中性化 + 注释脱敏

**完成日期**: 2026-04-29
**worktree**: `worktree/wave3-cleanup` (从主仓 `c8a0832` 派生, 与 main `6d8d531` 已分叉但 0 文件交集)
**主仓 HEAD**: `6d8d531` (未变)
**Wave 3 worktree HEAD**: 见 §"完成 commit 链"

## 完成 commit 链

> 顺序按 Wave 3 worktree 内 commit 时序 (从 c8a0832 起)

| Order | Slice/项 | Type | Commit | 改动文件数 | 净行数 |
|---|---|---|---|:-:|:-:|
| 1 | Slice 5 注释/文档脱敏 | C-2 (注释 + 文档) | `6ca3161` | 8 | -9 |
| 2 | Slice 1 UI tsx 死分支 | S3 (hard remove) | `eb8bcff` | 4 | -12 |
| 3 | Slice 6 P0 [ANT-ONLY] 中性化 | A (字面量) | `dfff583` | 5 | -5 |
| 4 | docs 第一批完成记录 | docs | `97367df` | 1 | +88 |
| 5 | Slice 3 EXPERIMENTAL_SKILL_SEARCH 一锅端 | S3 (hard remove) | `3ee02ad` | 9 | **-271** |
| 6 | Slice 2 log.ts STATE 三件套 | S3 + S2 | `da4f590` | 2 | -26 |
| 7 | docs 第二批完成记录 (含 G3 race 事故) | docs | `877b697` | 1 | +192 |
| 8 | docs Wave 3 整体收口 (R6 + R9) | docs | (本次) | 2 | +new |

> 阶段 B 视 Allen D-R4-1 决策追加 0~3 个 commit (R4 Slice 7 P1a 命名重构):
> - 9 (可选): R4 commit 1 — `utils/fastMode.ts` `isAnt` → `isInternal`
> - 10 (可选): R4 commit 2 — `constants/prompts.ts:166` `getAntModelOverrideSection` → `getInternalModelOverrideSection` + wave2_a7 同步
> - 11 (可选): R4 commit 3 — `utils/model/antModels.ts` 删 deprecated aliases + wave0_perm1 同步

## 验收基线 (全 Wave 3 一致)

- **typecheck:diff baseline** (vs main `6d8d531`): 起点 `1369` (Wave 2 完成时为 1375, 含 W2 i18n 后变化)
  - 第一批后: `≤ 1369` (no new errors)
  - 第二批后: `≤ 1369` (Slice 3 删 ~271 行后 typecheck 净 -15 vs main baseline)
  - 第三批 (R6+R9 仅 docs) 后: 不变
- **lint:diff baseline**: net -1 vs Wave 2 baseline 943, 全程 0 new problem
- **case 39 fingerprint** (排除 status_stdout/status_stderr/text_output 运行时态字段):
  从 `870f99ed494d3d145ed2eb1368132299` **全程稳定** — 8 commit 后未变。
- **9 wave2 smoke**: 全程 exit 0
- **i18n_self_check + i18n_runtime_smoke**: 全程 exit 0
- **audit_hardcoded_user_text**: 全程 exit 0
- **主仓 0 改动**: `/Users/allen/Documents/aiproject/mossensrc` 始终 clean
- **与 main `6d8d531` 文件交集**: 0 (W2 i18n 不与 Wave 3 source 重叠)

## Allen 决策 / 边界修正记录

### 第一批 4 决策 (D-S1-1 / D-S1-2 / D-S5-1 / D-S6-1)
- **D-S1-1 = A**: AgentTool.tsx 删 `appState` (草案明文)
- **D-S1-2 = A**: UI.tsx 删 `if (output.type === 'remote_launched')` 块 (D11 双模式 build-time false)
- **D-S5-1 = A**: 注释脱敏 5 文件 (含 D-S5-2 留 Wave 3)
- **D-S6-1 = A**: P0 [ANT-ONLY] 5 处中性化 → `[Internal]` / `[Debug]`

### 第一批扩边界 (D-S1-3 / D-S1-4)
- **D-S1-3 = A**: AgentTool.tsx `internal` var 整体删除 (草案仅说 "data as Output", 但删后 `internal` 失指代必须清理)
- **D-S1-4 = A**: AgentTool.tsx `context` → `_context` (rename underscore prefix, ESLint 必然清理)
- **理由**: 同 D-S1-1/D-S1-2 同性质 ESLint 必然清理, 主 agent 直接顺手补

### 第一批未纳入 (D-S5-2)
- **D-S5-2 = B**: `docs/needs-design-wave2-completion.md:52` Slack URL 残余不纳入第一批 (留 Wave 3 后续)
- **后续处置**: 第三批 R6 已在本 commit 脱敏

### 第二批 5 决策 (D-S2-1 ~ D-S2-5 + D-S3-N5)
- **D-S2-1~D-S2-5 = A**: log.ts STATE 三件套 (lastAPIRequestMessages 字段 + setter/getter + 默认值 + log.ts 调用链 + import) 整体删
- **D-S3-N5 = A**: Slice 3 删 EXPERIMENTAL_SKILL_SEARCH 后产生的 3 处 unused-vars (count import / Command import / enabledToolNames arg) 主 agent 直接顺手清理, 归入 Slice 3 commit, 不单独建 fix-lint commit

### 第二批 G3 race condition (子 agent 违规)
- **触发**: Slice 3 子 agent 在执行 Slice 3 之后, 用 `git stash` + `git stash pop` + `git checkout HEAD -- bootstrap/state.ts utils/log.ts` 试图 "恢复" 状态, 但该路径覆盖了 Slice 2 的 6 个 Edit (100% 还原到 commit 前)。
- **现象**: Slice 2 的 source 改动从 worktree 内消失, Allen 通过 git diff 复盘发现。
- **修复 (Allen 方案 B)**: 主 agent 直接 Edit 模式恢复 Slice 2 的 6 个改动, 不再派子 agent 写, 同 commit 落地为 `da4f590`。
- **永久不变量** (写入本文档 + wave3-execution-log.md):
  1. **同一 worktree 内不允许多子 agent 并发写文件** (git staging area 是共享 mutable state)
  2. **子 agent 严禁所有 git 操作** (含 stash / checkout / reset / rebase / merge / push / tag / add . / add -A)
  3. **写操作必须串行** — 一次 1 个子 agent 写, 主 agent 验证 + commit 后才能派下一个; 或主 agent 直接 Edit
  4. **只读审计可并发** — 多子 agent 并发 Read / Grep / Glob 不冲突, 不持久化任何状态

### 第三批 4 决策 (D-R6-1 / D-R6-2 / D-R9-1 / D-R9-2)
- **D-R6-1 = A**: R6 Slack URL 残余 (needs-design-wave2-completion.md:52) 纳入今日收口
- **D-R6-2 = B**: R6 跟 R9 docs commit 合并 (节省 commit 数)
- **D-R9-1 = A**: R9 合并 R6 一起 commit (同性质同提交点)
- **D-R9-2 = A**: R9 记录 Wave 3 followup (R7/R8/R1/R2/R3/Slice 7/Slice 8/insights.ts WIP/G3 race)

### 阶段 B 决策 (R4 Slice 7 P1a, 视决策启动)
- **D-R4-1 = A**: R4 Slice 7 P1a 命名重构作 Wave 3 阶段 B
- **D-R4-2 = A**: R4 拆 3 commit (fastMode / prompts.ts:166 / antModels 别名)
- **D-R4-3 = A**: R4 同 commit 改 wave2_a7 + wave0_perm1 smoke (source + smoke 同源同提交点)
- **复核要求**: 阶段 B 启动前派 1 个只读子 agent 实证 5 文件影响面, **若发现需触碰 `scripts/smoke_check.py` 立即停下**

## 留 Wave 4 / 独立施工包项

| 项 | 转向 | 理由 | 决策 |
|----|------|------|------|
| **R1** Slice 4a USER_TYPE 收敛 — 核心 5 文件 (commands/tools/setup/query/constants/prompts) ~30 命中 | Wave 4 (拆 5 子批独立施工包) | medium-high 风险, 每子批需复核 + LLM 冒烟 | D-R1-1=A / D-R1-2=B / D-R1-3=B |
| **R2** Slice 4b USER_TYPE 收敛 — utils/permissions/* 7 文件 ~30 命中 | Wave 4 (HIGH risk + 5 维度审查独立 PR) | 权限决策, 必须每子批前置 5 维度 (test/doc/prompt/slash/harness) 审查 | D-R2-1=B / D-R2-2=A / D-R2-3=A |
| **R3** Slice 4c USER_TYPE 收敛 — analytics + 通用 ~50+ 命中 (跳过 commands/insights.ts WIP) | Wave 4 (跟 R1/R2 配合) | 4 子批 ~半天, 与 R1/R2 一起做经济 | D-R3-1=B / D-R3-2=A |
| **R7** discoveredSkillNames 11 文件 / 13 处僵尸字段 | Wave 4 (NEEDS-DESIGN 独立设计任务) | 跨文件传递 + telemetry, 必须独立 NEEDS-DESIGN | D-R7-1=B / D-R7-2=A |
| **R8** `bunfig.toml [define]` USER_TYPE / EXPERIMENTAL_SKILL_SEARCH 缺失 (D11 双模式验收) | Wave 4 (基础设施独立 PR) | build 系统, 影响所有 slice 的双模式验收方式 | D-R8-1=B / D-R8-2=A |

## 永久不做项

| 项 | 永久不做理由 | 决策 |
|----|------------|------|
| **R5** Slice 8 P1b `type AntModel` → `type InternalModel` rename + 文件 rename | (1) `scripts/smoke_check.py` 多处路径必须同步 = **违反 Allen 红线**<br>(2) ~30 处 import 同步成本高<br>(3) 收益仅清晰度 (代码层面命名, 用户不可见)<br>(4) 若需中性化, Wave 4+ 与 D1/D2 (tengu/GrowthBook 命名) 一并评估 | D-R5-1=B / D-R5-2=C |
| **commands/insights.ts WIP** | Allen memory `feedback_protect_insights_wip` 永久保护 — 不 commit / 不 checkout / 不 stash, 仓库根禁用 `git add .` 和 `git add -A` | 永久 |

## 永久不变量 (Wave 3 沉淀)

1. **同一 worktree 内不允许多子 agent 并发写文件** (G3 race 永久教训)
2. **子 agent 严禁所有 git 操作** (stash/checkout/reset/rebase/merge/push/tag/add./add -A)
3. **只读审计可并发, 写操作必须串行**
4. **commands/insights.ts WIP 永久保护** (Allen memory)
5. **不 push / 不合 main / 不 tag** — Allen 决定时机
6. **触碰 `scripts/smoke_check.py` 立即停下** (Allen 红线)

## 后续操作建议

- 本 worktree 不 push、不合 main、不 tag — 由 Allen 决策合并时机
- 安全锚 tag 都在本地 (`pre-rollback-20260424-1756`, `wave2-user-type-cleanup-20260429`), 不 push
- 合 main 前建议:
  1. Allen 复核所有 Wave 3 commit (特别是 Slice 3 一锅端 -271 净 + Slice 2 STATE 三件套)
  2. 跑一次完整 TUI smoke (38/39 case + 启动 + 残留 + LLM 冒烟, 不只静态)
  3. 确认主仓 `6d8d531` 后没有新的冲突 commit
  4. fast-forward merge (类似 Wave 0 / Wave 2 流程)
  5. tag 命名建议: `wave3-batch123-cleanup-20260429` (含第三批 docs commit) 或 `wave3-batch12+r4-cleanup-20260429` (若阶段 B 启动)

## 相关文档

- `docs/wave3-baseline.md` — Wave 3 起点 baseline
- `docs/wave3-execution-log.md` — Wave 3 全程执行日志 (含 G3 race 复盘)
- `docs/needs-design-wave2-completion.md` — Wave 2 完成记录 + 留 Wave 3 项 + 本 wave 处置状态
- `docs/needs-design-usertype-lock.md` — USER_TYPE 设计回顾 (永久保留)

---

*— Wave 3 完成记录. 阶段 A 已完成, 阶段 B (R4) 视 Allen 决策启动.*
