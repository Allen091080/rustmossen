# 5 维度审查 Checklist

> **目的**: 任何高风险源码改动 (删除 / 重命名 / 移动 / schema 改动) 启动前, 主 agent 必须填写本 checklist, 防止盲区导致 TUI 静默 throw / 测试假阳性 / smoke 漏验证。
> **来源**: Wave 0~3 实战 — 5 维度 (test / doc / prompt / slash / harness) 是 Allen 多次拍板时使用的统一审查框架。
> **强制性**: 不通过即停下不实施, 派只读子 agent 复核盲区, Allen 拍板补盲区方案 (如 R7-prep / R2-prep 模式)。

---

## 0. 触发条件

任何以下操作启动前必须填写本 checklist:

| 触发 | 范围 |
|------|------|
| **删** 任何 source file (.ts/.tsx/.py) | 全删除 / 移动到 .archive/ |
| **重命名** 任何 source file 或 export | 含文件 rename + symbol rename |
| **移动** 任何 source file | mv 跨目录 |
| **删 / 重命名 schema field** (interface / type / state) | 含 React state / Zustand store / TS interface field |
| **修改 prompt / slash command / 子 agent system 行为** | constants/prompts.ts / commands.ts / Tool.ts interface |
| **删 / 重命名 deprecated alias** | export const X = Y 形式 |

不触发本 checklist 的场景 (无需填写):
- 单纯 typo / 注释修复
- docs 文件改动
- 单一 bug 修 (不动接口 / schema)
- shell script / config 文件改动 (含 bunfig.toml — 但仍需查 red-lines.md §3)

---

## 1. 维度 1 — 测试覆盖

| 检查项 | 状态 | 备注 |
|--------|:---:|------|
| 该文件是否有 `.test.ts` / `.spec.ts` (单元测试)? | ☐ Y / ☐ N / ☐ N/A | 0 测试 = 风险 high, 必须前置补测试 (R-prep 模式) |
| 该文件是否在 `harness/` 中被引用 (e2e)? | ☐ Y / ☐ N / ☐ N/A | grep `import.*<filename>` harness/ |
| 改动后哪些测试需要更新? | (列清单) | 同 commit 必须更新 |
| 是否有 audit smoke 覆盖 (`scripts/wave[N]_*_smoke.py` / `scripts/audit_*.py`)? | ☐ Y / ☐ N / ☐ N/A | 列出覆盖的 smoke 文件 |
| 改动后 audit smoke 是否需要同步? | ☐ Y / ☐ N | 同 commit 必须改 (Wave 3 R4 commit 2/3 模式) |

## 2. 维度 2 — 文档引用

| 检查项 | 状态 | 备注 |
|--------|:---:|------|
| `docs/` 是否引用该文件? | ☐ Y / ☐ N | grep `<filename>` docs/ |
| `needs-design-*.md` 是否提及? | ☐ Y / ☐ N | docs/needs-design/ + docs/needs-design-*.md (顶层兼容) |
| `architecture-boundaries.md` 是否标记为受保护? | ☐ Y / ☐ N | 若是, 必须 Allen 拍板才能动 |
| 改动后哪些 docs 需要更新? | (列清单) | 同 commit 必须更新 (含 wave[N]/completion.md) |

## 3. 维度 3 — Prompt 引用

| 检查项 | 状态 | 备注 |
|--------|:---:|------|
| `constants/prompts.ts` 是否引用该文件 (含函数名 / type / 字符串)? | ☐ Y / ☐ N | grep `<symbol>` constants/prompts.ts |
| 改动后是否影响 system prompt 或 LLM 行为? | ☐ Y / ☐ N | 若 Y → 必须真 LLM 冒烟 (不只静态 smoke) |
| `wave2_a7_growthbook_suffix_null_smoke.py` 等 prompt-related smoke 是否需要同步? | ☐ Y / ☐ N | 同 commit 改 |

## 4. 维度 4 — Slash command 引用

| 检查项 | 状态 | 备注 |
|--------|:---:|------|
| `commands.ts` 是否引用该文件 (含 import / 字符串)? | ☐ Y / ☐ N | grep `<symbol>` commands.ts |
| 任何 `/command` 是否依赖 (e.g., `/clear` 依赖 conversation.ts)? | ☐ Y / ☐ N | 列出依赖的 slash command |
| 改动后是否影响 `command-inventory.md` (114 cmd 自动清单)? | ☐ Y / ☐ N | 若 Y → 重新生成 |

## 5. 维度 5 — Harness 引用

| 检查项 | 状态 | 备注 |
|--------|:---:|------|
| `harness/` 是否有 e2e 测试? | ☐ Y / ☐ N | grep `<symbol>` harness/ |
| `smoke_check.py` 158 case 是否覆盖? | ☐ Y / ☐ N | 列出覆盖的 case 名 |
| **case 39 fingerprint 是否受影响?** | ☐ Y / ☐ N | 若 Y → 改动后必须重计算 fingerprint, 与 `870f99ed...` 比对 |
| `harness_M[N]_*` / `harness_R[N]_*` 是否需要同步? | ☐ Y / ☐ N | 同 commit 改 |

## 6. 红线复盘 (与 `red-lines.md` 联动)

| 红线 | 状态 |
|------|------|
| 不触 §1 Allen 个人红线 (不动 insights.ts / skill 系统 / 不 push) | ☐ ✅ |
| 不触 §2 工程红线 (state/interface field grep / G3 race / typecheck baseline) | ☐ ✅ |
| 不触 §3 系统红线 (smoke_check.py / i18n / bunfig.toml / case 39) | ☐ ✅ |
| 已读 `architecture-boundaries.md` 确认文件不在 Wave/UX 灰区 | ☐ ✅ |

## 7. 不通过处理

任何 §1-§6 出现 ☐ N (未检查) 或 ☐ ❌ (触线):

| 步骤 | 动作 |
|------|------|
| 1 | **停下不实施** |
| 2 | 派只读子 agent 复核盲区 (e.g., 0 测试 → 复核是否有 indirect e2e; 0 文档 → 复核是否在另一文档命名空间) |
| 3 | 子 agent 报告盲区实证 |
| 4 | Allen 拍板方案: 补盲区前置 (R-prep 模式) / 推后做 / 其他 |
| 5 | 补盲区完成后再重新跑本 checklist |

## 8. 通过后实施前 SOP

| 步骤 | 动作 |
|------|------|
| 1 | 打安全锚 tag (本地, 不 push): `pre-<change>-YYYYMMDD-HHMM` |
| 2 | 在 `docs/waves/wave[N]/execution-log.md` 记录本 checklist 填写结果 |
| 3 | 主 agent 串行 Edit (G3 race 永久不变量 — 不派子 agent 写) |
| 4 | 每 commit 后跑 focused smoke (按维度 1+5 列出的 smoke) |
| 5 | 全部完成后跑总验证 (`bash scripts/run_all_smoke.sh`) |

## 9. 模板使用

复制本 checklist 到 `docs/waves/wave[N]/audit-checklist-<change>.md`, 逐项填写。完成后:
- 通过: 进入实施
- 不通过: 停下汇报 Allen + 记录在 execution-log

---

*— 5 维度审查 Checklist v1.0 / Wave 4 阶段 1 落地. 后续 wave 实战发现新维度时增补.*
