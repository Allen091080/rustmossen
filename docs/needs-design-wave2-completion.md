# Wave 2 完成记录 — 潜伏激活面治理

**完成日期**: 2026-04-28
**worktree**: `worktree/wave2-user-type` (从主仓 `4e15dde` 派生)
**主仓 HEAD**: `4e15dde` (未变)
**工程产出**: 9 commits, 32 files changed, +1447 / -831 (净 +616, 含 9 新 smoke ~1335 行)

## 完成 slice 列表 (按 commit 顺序)

| Order | Slice | Type | Commit | 改动文件 | 净行数 |
|---|---|---|---|---|---|
| 1 | A1 MOS-BASH-BQ | S2 (函数体置空) | `3a167f0` | tools/BashTool/bashPermissions.ts | -26 |
| 2 | A2 MOS-ENV-STRIP | C-2 (集合 30→6) | `e084441` | tools/BashTool/bashPermissions.ts | -16 |
| 3 | A5 MOS-CMD-ALLOWLIST | C-2 + S3 | `4bc6d22` | tools/BashTool/readOnlyValidation.ts + tools/PowerShellTool/readOnlyValidation.ts | -65 |
| 4 | A3 MOS-CCR | S3 (hard remove) | `ac38d54` | AgentTool 4 文件 + UI.tsx + docs + 新执行日志 | -19 |
| 5 | A4 MOS-CANONICAL | S3 (hard remove) | `5e50bdf` | tools/SkillTool/SkillTool.ts | -253 |
| 6 | A7 ANT-GROWTHBOOK | A (return null) | `e7fab78` | constants/prompts.ts | +5 |
| 7 | A6 ANT-STUCK | S3 + smoke 4 字符串 | `9a97d51` | skills/bundled/{stuck.ts,index.ts} + smoke + docs | -84 |
| 8 | B-C2 ANT-DUMPPROMPTS | S3 + 9 引用清理 | `7ecf0e8` | services/api/dumpPrompts.ts + 8 caller + 新 smoke | -262 |
| 9 | C1 TYPE-MIGRATE-PROMPTSUGGEST | A (getUserType) | `62231b7` | services/PromptSuggestion/promptSuggestion.ts | +4 |

## 验收基线 (全 9 slice 一致)

- **typecheck:diff baseline 1384**: 完成时 `1375` (fixed 9, no new errors)
- **lint:diff baseline 943**: 完成时 `939` (fixed 4, no new problems)
- **case 39 fingerprint** (排除 status_stdout/status_stderr/text_output 运行时态字段):
  从 `870f99ed494d3d145ed2eb1368132299` 全程稳定 — 所有 slice 后未变。
- **focused smoke 总命中**: 9 个 wave2 smoke / 51 个 case (4+1+6+5+12+3+7+4+4=46... 实际 51 含子项)
- **主仓 0 改动**: `/Users/allen/Documents/aiproject/mossensrc` 始终 clean。

## Allen 决策 / 边界修正记录

### A3 边界扩张 — UI.tsx type-level 依赖补漏 (方案 1)
- **触发**: v3 §2.4 内部矛盾 — 步骤 5 要求删 `RemoteLaunchedOutput` type, 但禁止改动条款写"UI.tsx 留 Wave 3"。
- **事实**: UI.tsx 是 type-level import (`import type`), 删 type 必破 typecheck。
- **Allen 决策**: 扩 A3 边界至 UI.tsx 仅删 type-level (import + cast 改 inline anonymous shape), 不动 'remote_launched' 字符串字面量 (Wave 3 物理删 .tsx 时清)。
- **记录位置**: `docs/wave2-execution-log.md` (本 worktree)

### G3 race condition (主 agent + 子 agent 共享 worktree git index)
- **触发**: A6 主 agent 与 A7 子 agent 并发, 两个 agent 共享同一 worktree 的 git index。
- **现象**: A6 staging 后 A7 子 agent 抢先 git commit, 导致 commit `f21bee2` 内容 = A7 / message = A6 错位。
- **修复**: 主 agent soft reset, 取 A7 staged content commit 为 `e7fab78` (正确 A7 message), 然后 stage A6 文件 commit 为 `9a97d51` (正确 A6 message)。子 agent 检测到 race 后立即停下未做修复。
- **教训**: 单 worktree 内多 agent 并发时, git staging area 是共享 mutable state, 需要互斥 (per-slice lockfile 或 pre-commit hook 校验 staging 一致性)。Wave 2 之后的并发施工建议考虑此点。

### Wave 2 不建 harness/usertype-gate/
- **来源**: Allen memory `feedback_skill_subagent_systems_protected.md` + Wave 2 v3 整体策略。
- **影响**: C1 v3 §2.9 spec 中的 `harness/smoke/promptSuggestion-rateLimit-suppress.test.ts` (TypeScript 测试)替换为 `scripts/wave2_c1_*.py` (Python static smoke), 与其余 8 个 wave2 smoke 保持模式一致。

## 留 Wave 3 (本 wave 不处置)

> **Wave 3 处置状态** (2026-04-29 更新, 详见 `docs/needs-design-wave3-completion.md`):
>
> | 项 | Wave 3 处置 | commit |
> |----|------------|--------|
> | UI.tsx remote_launched 字面量 | ✅ Slice 1 已清 | `eb8bcff` |
> | bashPermissions.ts:211 Slack 注释 | ✅ Slice 5 已脱敏 | `6ca3161` |
> | log.ts:349-351 stale comment + ant 分支 | ✅ Slice 2 已清 | `da4f590` |
> | EXPERIMENTAL_SKILL_SEARCH gate die requires | ✅ Slice 3 已清 (一锅端 9 文件) | `3ee02ad` |
> | tools/SkillTool/prompt.ts 自然解耦 | ✅ Slice 3 已清 | `3ee02ad` |
> | 全仓 USER_TYPE === 'X' 比较收敛 | ⏳ 推 Wave 4 (R1/R2/R3, 286 处分 16 子批) | - |
> | 全仓 *.tsx "external" === 'mossen' 字面量 | ✅ Slice 1 已清 | `eb8bcff` |
> | needs-design-usertype-lock.md:17 历史 | ✅ 永久保留 (设计回顾) | - |
> | services/skillSearch/* 死 require | ✅ Slice 3 已清 | `3ee02ad` |
> | tools/DiscoverSkillsTool/ 死 require | ✅ Slice 3 已清 | `3ee02ad` |

- `tools/AgentTool/UI.tsx` 中 'remote_launched' 字符串字面量 2-3 处 (留 .tsx 物理删时一并清)
- `tools/BashTool/bashPermissions.ts:211` 注释中内部沟通频道引用 (pre-existing comment, Wave 3 Slice 5 已脱敏)
- `utils/log.ts:349` 关于 dumpPrompts.ts 的 stale comment + `:351` `process.env.USER_TYPE === 'ant'` 分支
- `commands.ts:92`, `constants/prompts.ts:98`, `query.ts:61`, `commands.ts:90` 中 `EXPERIMENTAL_SKILL_SEARCH` gate 的 die 路径 require (4 处死 require, A4 仅清 SkillTool.ts 主面)
- `tools/SkillTool/prompt.ts` 在 A4 hard remove 后 prompt 不提 canonical, 已自然解耦
- 全仓其他 `process.env.USER_TYPE === 'X'` 比较位置 (Wave 2C 仅收敛 promptSuggestion.ts 单点, Wave 3+ 全仓推广)
- 全仓 `*.tsx` 中 dead-code-elimination 占位 `"external" === 'mossen'` 字面量 (Wave 3 死代码清理)
- `docs/needs-design-usertype-lock.md:17` 设计回顾文档 (保留历史轨迹)
- `services/skillSearch/*` 4 子模块 (本来就不存在;`commands.ts:92` `constants/prompts.ts:98` 还有 require 的死路径,留 Wave 3 一并清)
- `tools/DiscoverSkillsTool/` (本来就不存在;`constants/prompts.ts:98` 死 require)

## 后续操作建议

- 本 worktree 不 push、不合 main、不 tag — 由 Allen 决策合并时机。
- 9 个安全锚 tag 都在本地 (`pre-s4-wave2-*`, `pre-wave2-A1..A7-*`, `pre-wave2-BC2-*`, `pre-wave2-C1-*`), 不 push。
- 合 main 前建议:
  1. Allen 复核所有 9 个 commit (特别是 A3/A4/B-C2 这种结构性删改)
  2. 跑一次完整 TUI smoke (38/39 case, 不只是静态 smoke)
  3. 确认主仓 4e15dde 后没有新的冲突 commit
  4. fast-forward merge (类似 Wave 0 流程)
