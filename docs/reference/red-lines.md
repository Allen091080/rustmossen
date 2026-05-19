# 红线清单 (Allen + Wave 0~3 沉淀)

> **目的**: 沉淀 Allen 个人红线 + 工程红线 + 系统红线, 防止主 agent / 子 agent 默认动作触线。
> **来源**: Allen memory + Wave 0/2/3 实战教训 (G3 race / state field deletion / case 39 / G3 race 等)
> **状态**: 活文档 — 任何 wave 完成时若沉淀新红线必须更新本文件。

---

## 1. Allen 个人红线 (永久, 来自 memory)

| 红线 | 触发场景 | memory 引用 |
|------|---------|------------|
| **不 push** (除非用户明说) | 任何 `git push` 操作 | `feedback_no_push.md` |
| **不动 `commands/insights.ts`** (一行 WIP 永久保护) | 不 commit / 不 checkout / 不 stash; 仓库根禁用 `git add .` 和 `git add -A` | `feedback_protect_insights_wip.md` |
| **skill / 子 agent 系统受保护** | skill / 子 agent / gate / fallback / 测试任何处置必须先做 5 维度调研施工包, Allen 拍板后才动手; 不允许默认列"删除/解 gate"动作 | `feedback_skill_subagent_systems_protected.md` |
| **每个 slice 完成前必须彻底测试** | 多 slice 重构必须跑完整 smoke + 启动 + 残留 + LLM 冒烟, 不能只跑 `--help` | `feedback_slice_testing.md` |
| **任务必须 100% 真做完才算完成** | 不能用骨架/SKIP/假阳性测试糊弄; 测试名 + 断言 + 实际行为必须一致 | `feedback_100_percent_or_not_done.md` |

## 2. 工程红线 (Wave 0~3 实战沉淀)

| 红线 | 触发场景 | 沉淀来源 |
|------|---------|---------|
| **删 state schema 字段必须 grep 全仓 read 点** | 否则 TUI 静默 throw 立即退出, `--help`/smoke 测不出; 用 `expect-1` 模拟 TTY 验 | Wave 1.5 + memory `feedback_state_field_deletion.md` |
| **删 interface 字段必须 grep 全仓 implementor** | 同性质于 state field deletion | Wave 3 R7 NEEDS-DESIGN |
| **同 worktree 不允许多子 agent 并发写** | git staging area 是共享 mutable state, 子 agent 间 stash/checkout 必相互覆盖 | Wave 2 G3 race + Wave 3 Slice 3 G3 race |
| **子 agent 严禁所有 git 操作** | 含 `stash` / `checkout` / `reset` / `rebase` / `merge` / `push` / `tag` / `add .` / `add -A` | Wave 3 Slice 3 复盘 |
| **写操作必须串行** | 一次 1 个子 agent 写, 主 agent 验证 + commit 后才能派下一个; 或主 agent 直接 Edit | Wave 3 G3 race 永久不变量 |
| **只读审计可并发** | 多子 agent 并发 Read / Grep / Glob 不冲突, 不持久化任何状态 | Wave 3 第二批实证 |
| **typecheck:diff / lint:diff 0 new 强制** | baseline 按 (file, severity, ruleCode) 或文本匹配, rename 触发 NEW; 必须保留 baseline 平衡 | Wave 3 R4 commit 3 |
| **case 39 fingerprint 全程稳定** | `870f99ed494d3d145ed2eb1368132299` (排除 `status_stdout`/`status_stderr`/`text_output` 运行时态字段) | Wave 2 / Wave 3 |

## 3. 系统红线 (架构层面, 永久)

| 红线 | 范围 | 决策来源 |
|------|------|---------|
| **不动 `scripts/smoke_check.py`** | 任何 wave 都不得修改 (Wave 3 R5 永久不做的传导) | Wave 3 决策 D-R5-1=B + 历史触线复盘 |
| **不动 `utils/i18n/strings.en.ts` / `strings.zh.ts`** | i18n 字典, 仅 UX-Wave 团队改 | Wave 4 阶段 0 体验线边界 |
| **`bunfig.toml` 不轻动** | 影响所有 build, R8 解禁后再评估 | Wave 4 R8 施工包 D-R8-D |
| **不删 `utils/model/antModels.ts` 中 `getAntModelOverrideConfig` alias** | 受 `wave2_a7_growthbook_suffix_null_smoke.py` case 3 保护 | Wave 2 A7 + Wave 3 R4 commit 3 |
| **不动 case 39 涉及文件** | 仅校验 fingerprint, 不修 case 39 自身 | Wave 3 收口报告 |
| **不动 runtime API (`Tool.ts` interface 等)** | 任何 interface 字段改动需 5 维度调研 + Allen 拍板 | Wave 4 R7 NEEDS-DESIGN |
| **CLI 不做 Workbench-like 固定 chrome** | 不再把固定顶栏、侧栏、三列布局挂进 `screens/REPL.tsx` / `FullscreenLayout.tsx` | UX-TUI 暂停决策 + CWB-1 |
| **Workbench/Web/Mobile 不 direct import Core internals** | 禁止直接 import `query.ts` / `screens/REPL.tsx` / `bootstrap/state.ts` / `Tool.ts` / `tools/*`; 必须走 stream-json subprocess | CWB-1/CWB-2/CWB-3 |
| **Extensions 不 direct import Core 私有实现** | 外部扩展必须走 manifest / gateway / permission declaration / event stream | 扩展系统协议 + CWB-1/CWB-2 |
| **stream-json 协议 only-additive** | 可加字段 / 加 union 成员; 不删字段 / 不改字段语义 / 不重命名 union 成员; 任何 SDKMessage / SDKControlRequestInner / StdoutMessage / StdinMessage shape 变化必须同 commit 改 `entrypoints/sdk/{core,control}Schemas.ts` + `scripts/stream-json-schema-whitelist.txt` + `docs/reference/protocol-contract.md`; 破坏性变化必须先写 deprecation 草案 + Allen 拍板 | CWB-3 |
| **CLI binary 不打包 Workbench/Web/Mobile** | `bun run build` 产物只含 Core CLI + stream-json 协议层; Workbench / Web / Mobile 是独立消费者, 通过 `mossen -p --input-format stream-json --output-format stream-json --verbose` 子进程接入, 永不进入 mossen binary | CWB-3 (推 CWB-4 仓库分离) |
| **stream-json 入口锚点不可破坏** | `main.tsx` 双端一致校验 + `cli/print.ts` runHeadless/getStructuredIO/verbose 守卫 + `cli/structuredIO.ts` StructuredIO/control_request/keep_alive + `cli/ndjsonSafeStringify.ts` ndjsonSafeStringify; 由 `scripts/stream_json_contract_smoke.py` Section E 锚点守卫, 缺一即 fail | CWB-3 |

## 4. 触发场景 (主 agent 必须查 red-lines 的时机)

实施前必须读本文件的场景:

| 场景 | 必读章节 |
|------|---------|
| 任何源码 **删除** | §2 (工程红线 — state/interface field) + §3 (系统红线) |
| 任何源码 **重命名** | §2 (typecheck/lint diff) + §3 (相关红线) |
| 任何源码 **移动** | §2 + 全章节 |
| 任何 schema field 增删 | §2 (state/interface field deletion) |
| 任何子 agent 派出 | §2 (写 vs 只读 / G3 race) |
| 任何 git 操作 (尤其 reset / stash / push / tag / merge / rebase) | §1 (Allen 个人) + §2 (子 agent git 禁止) |
| 修改 prompt / slash command / 子 agent system 行为 | §1 (skill 子 agent 受保护) + 5 维度审查 (`audit-checklist.md`) |
| 任何 i18n 字典改动 | §3 (i18n 红线) — 必拒绝, 转 UX-Wave |
| 任何 `smoke_check.py` / `bunfig.toml` 改动 | §3 — 必拒绝或先 Allen 解禁 |
| 任何 Workbench / Web / Mobile / Extension 实现 | §3 (Core/CLI/Workbench 边界) + `docs/waves/core-cli-workbench/layer-boundaries.md` |
| 任何新增 import 边界规则或例外 | `docs/reference/layer-boundary-rules.md` + `scripts/layer_boundary_audit.py` |

## 5. 违反处理 (G3 race 永久不变量)

| 步骤 | 动作 |
|------|------|
| 1 | **立即停下汇报 Allen** — 不擅自回滚 / 不擅自修复 |
| 2 | 报告: 触线红线条目 + 触线动作 + 当前工作树状态 (含 git status / 改动文件清单) |
| 3 | 等 Allen 拍板方案 (回滚 / 继续 / 修复 / 转 wave) |
| 4 | 不可对 git 做任何破坏性动作 (reset / stash / checkout HEAD --) |
| 5 | 子 agent 触线: 主 agent 立即接管恢复 (Wave 3 Slice 3 方案 B 模式) |

## 6. 文件位置 + 维护责任

- **本文件**: `docs/reference/red-lines.md` (Wave 4 阶段 1 落地)
- **memory 镜像**: `~/.claude/projects/-Users-allen-Documents-aiproject-mossensrc/memory/MEMORY.md`
- **更新时机**: 任何 wave 完成时若发现新红线 (G3 race / silent throw / baseline drift / 等) 必须立即更新本文件
- **审查责任**: 主 agent 实施前必读, 子 agent 由主 agent 通过 prompt 注入相关章节

---

*— 红线清单 v1.0 / Wave 4 阶段 1 落地. 后续 Wave 沉淀新红线时更新本文件 + commit message 标 `chore(red-lines): ...`*
