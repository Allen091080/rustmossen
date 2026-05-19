# 架构线 vs 体验线文件边界

> **目的**: 明确 Wave (架构治理线) 与 UX-Wave (体验治理线) 的文件边界, 防止 PR 冲突 + 设计冲突 + 行号偏移导致的施工失败。
> **来源**: Wave 4 阶段 0 体验线边界说明施工包 + Wave 0~3 实战重叠文件复盘。
> **配套**: `docs/reference/red-lines.md` (红线定义) + `docs/reference/audit-checklist.md` (5 维度审查)。

---

## 0. 两条线定位

| 线 | 范围 | 主导文件类型 | 风格 |
|----|------|------------|------|
| **架构线** (Wave N) | 死代码清理 / USER_TYPE 收敛 / 命名中性化 / 基础设施 / 性能 | source (.ts/.tsx) + smoke (Python) | 删除 / 重命名 / 重构 |
| **体验线** (UX-Wave N) | i18n / 用户文案 / cmd description / spinner / statusline | utils/i18n/strings.*.ts + cmd description fields | 新增 / 优化 / 本地化 |

### 0.1 已合 main 的两条线进度 (截至 2026-04-29)

| Wave 类型 | 已完成 | 主仓 HEAD |
|----------|--------|----------|
| 架构线 | Wave 0, Wave 2, Wave 3, Wave 4 | Wave 4 = `7af1c18` |
| 体验线 | UX-Wave1, UX-Wave2 | UX-Wave2 = `6d8d531` (已含在 main `7af1c18` 中) |

---

## 1. 架构线 (Wave N) 不允许触碰的文件

任何 wave 必须拒绝改动以下文件 (转 UX-Wave 或转拒绝):

| 文件 | 理由 | 哪个线管? |
|------|------|----------|
| `utils/i18n/strings.en.ts` | i18n 字典, 用户文案 | **UX-Wave** |
| `utils/i18n/strings.zh.ts` | 同上 | **UX-Wave** |
| `scripts/i18n_runtime_smoke.py` | i18n 验证 smoke | **UX-Wave** |
| `scripts/audit_hardcoded_user_text.py` | 硬编码扫描 | **UX-Wave** |
| (任何 cmd description 字段, 如 `cmd.X.description`) | 用户可见文案 | **UX-Wave** |
| (任何 spinner 字段, 如 SPINNER_VERBS_ZH) | 用户可见文案 | **UX-Wave** |
| (任何 statusline JSON 注入) | 用户可见 | **UX-Wave** |
| `scripts/smoke_check.py` | 系统红线 | **任何线都不动** (Wave 3 R5 永久不做) |
| `commands/insights.ts` | Allen WIP 永久保护 | **任何线都不动** |

## 2. 体验线 (UX-Wave) 不允许触碰的文件

任何 UX-Wave 必须拒绝改动以下文件 (转架构线 / 拒绝):

| 文件 | 理由 | 哪个线管? |
|------|------|----------|
| `utils/permissions/*` (除注释脱敏) | 权限决策, 安全核心 | **架构线** Wave 4 R2 / Wave 5+ |
| `services/analytics/*` | telemetry 内部架构 | **架构线** Wave 4 R3 / Wave 5+ |
| `utils/model/antModels.ts` (类型与函数) | 内部模型类型 (R5 永久不做命名 rename) | **架构线** (限定范围) |
| `Tool.ts` interface | runtime API 边界 (skill/子 agent 受保护) | **架构线** Wave 4 R7 + 5 维度审查 |
| `bootstrap/state.ts` (state schema) | runtime state, 删字段必须 grep 全仓 | **架构线** |
| `scripts/smoke_check.py` | 系统红线 | **任何线都不动** |
| `commands/insights.ts` | Allen WIP | **任何线都不动** |

---

## 3. 灰区文件 (双方都可改, 必须协调)

以下文件**两条线都有改动需求**, 必须严格协调:

| 文件 | 架构线改什么 | 体验线改什么 | 严重度 |
|------|-------------|-------------|:----:|
| `commands.ts` | USER_TYPE === 'ant' → getUserType() (Wave 5 R1.1) | cmd description i18n key (UX-Wave2 已合) | medium |
| `query.ts` | USER_TYPE 替换 (Wave 5 R1.4) | 文案 i18n (UX-Wave2 已合) | medium |
| `constants/prompts.ts` | system prompt USER_TYPE gate (Wave 5 R1.5, 13 处) | system prompt 文案 i18n (UX-Wave2 已合) | **HIGH** (3+ wave 改, 行号必偏) |
| `tools/BashTool/bashPermissions.ts` | USER_TYPE permissions (Wave 5 R2.7) | bash 文案 i18n + 注释脱敏 (UX-Wave2 + Wave 2 + Wave 3 Slice 5) | **HIGH** |
| `tools/PowerShellTool/readOnlyValidation.ts` | USER_TYPE permissions (Wave 5 R2) | PowerShell 文案 (UX-Wave2 + Wave 2 A5) | low-medium |
| `commands/clear/conversation.ts` | discoveredSkillNames 删 (Wave 4 R7) | cmd description i18n (UX-Wave1) | medium |

### 3.1 灰区文件协调规则 (G3 race 永久不变量传导)

| # | 规则 |
|---|------|
| 1 | 启动 Wave N 或 UX-Wave N 前, 必须 `git diff main..<另一 worktree HEAD>` 看是否有未 merge 改动 |
| 2 | 若另一 worktree 触碰同文件, **必须等其合 main 后再启动** |
| 3 | 若必须并行, **由 Allen 决定优先级** + 哪一线让步 (从行号偏移角度) |
| 4 | **同 commit 不允许两线并行** — 一线先合 main 后另一线再启动 |
| 5 | 任何灰区文件改动前, 必须 `git log --follow <文件>` 看历史 wave 改了哪些行, 复核行号偏移 |

---

## 4. 推荐启动顺序 (基于当前 2026-04-29 状态)

```
当前 (2026-04-29):
  main HEAD = 1b3e35a (Wave 3 已合)
  Wave 4 worktree = 1b3e35a (阶段 0 文档完成, 阶段 1 待启动)
  UX-Wave3 worktree = (未启动)

推荐:
  1. Wave 4 阶段 1 (架构治理基建, 0 源码) ← 当前可启动
     - 不影响 UX-Wave3 启动 (0 文件交集)
  2. UX-Wave3 启动 (i18n S6+ 续, 独立 worktree) ← Wave 4 阶段 1 完成后
     - 或并行 Wave 4 阶段 2/3 (R8.2 / R7) — 但只允许 0 灰区文件交集
  3. Wave 5 R1/R2/R3 USER_TYPE 收敛 ← 推 Wave 5
     - 启动前必须确认 UX-Wave3 已合 main (constants/prompts.ts 等 HIGH 重叠文件)
```

### 4.1 并行启动条件 (Wave N 与 UX-Wave M)

允许并行的条件 (全部满足):
- ✅ 0 灰区文件交集 (查 §3 表)
- ✅ 0 架构线/体验线 互禁文件交集 (查 §1 + §2 表)
- ✅ Allen 明确批准并行
- ✅ 主 agent 在两 worktree 启动前都跑过 `git diff main..<both worktrees>` 复核

任一条件不满足 → 强制串行。

---

## 5. 触碰违反处理 (与 `red-lines.md` §5 联动)

| 触线类型 | 处理 |
|---------|------|
| 架构线触 §1 (i18n 字典 / cmd description / spinner) | 立即停下, 转 UX-Wave 处理 |
| 体验线触 §2 (permissions / analytics / model) | 立即停下, 转架构线处理 |
| 任一线触 "任何线都不动" 红线 (smoke_check.py / insights.ts) | 立即停下汇报 Allen, 不擅自处理 |
| 灰区文件协调失败 (并行启动后冲突) | 立即停下, 主 agent 不做 git 操作 (含 stash/reset/checkout), 等 Allen 拍板 |

---

## 6. 新增灰区文件流程

发现新的灰区文件 (两条线都需要改) 时:

| 步骤 | 动作 |
|------|------|
| 1 | 主 agent 实施前发现, 立即更新本文件 §3 表 |
| 2 | commit message: `docs(boundaries): mark <file> as gray-zone` |
| 3 | 在 `red-lines.md` 的"工程红线"章节增补 (若有新模式) |
| 4 | 在 `audit-checklist.md` 触发条件中增补 (若需新维度) |

---

## 7. 维护责任

- **本文件**: `docs/architecture-boundaries.md` (顶层 docs/, 与 6 子目录平级)
- **更新时机**:
  - 任何 wave 完成时若发现新灰区文件 → 更新 §3
  - 任何线红线扩展 → 更新 §1 / §2
  - main HEAD / Wave 进度变化 → 更新 §0.1 + §4

---

*— 架构线 vs 体验线文件边界 v1.0 / Wave 4 阶段 1 落地. 后续 wave 沉淀新边界时更新本文件 + commit message 标 `docs(boundaries): ...`*

---

## 8. Core / CLI / Workbench / Extensions 分层边界 (CWB-1)

> 详细规则见 `docs/waves/core-cli-workbench/layer-boundaries.md` 与 `docs/reference/layer-boundary-rules.md`。

### 8.1 定位

| 层 | 责任 | 不负责 |
| --- | --- | --- |
| Core | agent loop、tools、permissions、memory、context、MCP、skill/plugin loader、stream-json | 面板化 UI、Workbench 本地状态、扩展内容 |
| Classic CLI | binary entrypoint、args、interactive TUI、print、stream-json host | fixed header、sidebar、three-column layout |
| Protocol / SDK | stream-json、runtime snapshot、manifest、future panel-state | React/Ink UI |
| Workbench / Web / Mobile | rich panels、timeline、tasks、diagnostics、visual permissions | direct import Core internals |
| Extensions | skills、plugins、workflows、prompt packs、language packs、panel providers | 静默改变 Core 默认行为 |
| Evolution / Repair | diagnostics、repair report、私有 repair pipeline | 用户端自动 patch Core |

### 8.2 新增强制规则

| 规则 | 处理 |
| --- | --- |
| CLI 不继续做 fixed header / sidebar / three-column layout | 转 Workbench 桌面端实现。 |
| Workbench 第一阶段只通过 CLI binary subprocess + stream-json 接入 | 不允许 in-process import `query.ts` / `screens/REPL.tsx` 等 Core 私有文件。 |
| Extensions 必须通过 manifest / gateway / permission declaration / event stream 接入 | 外部扩展不得 import `tools/*` executor 或 `utils/permissions/*` 私有实现。 |
| Protocol / SDK 必须 UI-agnostic | 不得 import React/Ink components。 |
| Core 不得依赖 Workbench/Web/Mobile implementation | Core 必须可作为独立 CLI binary 运行。 |

### 8.3 审计

`scripts/layer_boundary_audit.py` 已接入 `scripts/run_all_smoke.sh`。新增 Workbench、Web、Mobile、Extension 或 Protocol 目录时，必须同步 `scripts/layer-boundary-rules.json` 并跑完整 runner。
