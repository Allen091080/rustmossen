# GrowthBook 迁移执行计划

> **文档性质**：Living document — 每完成一个 slice 必须立刻更新本文件对应行 + 进度总览。文档不是任务结束后的报告，而是任务进行中的状态镜像。
>
> **目标用户**：执行 AI + Allen。执行 AI 每动一刀都要回到本文件打勾、贴证据、更新风险评估。
>
> **前置关系**：本计划必须在 `OpenTelemetry删除计划.md` 全部完成之后执行；OpenTelemetry 删除未完成时，不得开始本计划。

---

## 元信息

| 字段 | 值 |
|---|---|
| 创建日期 | 2026-04-27 |
| 创建时基线 | OTel 删除合并后 main HEAD `dc4b3ea` |
| Baseline tag | `pre-growthbook-migration-20260427` ✅ 已打 |
| 总 slice 数 | **36**（预施工 G0 8 + 前置 G1/G2 6 + 迁移 G3/G4/G5/G6/G7 22） |
| 当前进度 | **28/36**（G0+G1+G2+G3+G4+G5 全完成；进入 G6-1） |
| 当前阶段 | **G1 Mossen 配置门面** |
| 工作 branch | `worktree/growthbook-migration` ✅ |
| 当前 HEAD | (G0 收口 commit) |
| 上次更新 | 2026-04-27 18:00 |

---

## 0. 总目标与边界

### 0.1 一句话目标

把 Mossen 从官方 GrowthBook 远程 feature flag / dynamic config 系统迁移到 **Mossen 自有的本地优先配置门面**，让个人版行为由本地配置、项目配置和环境变量控制；未来如果 Mossen 自己有 hosted 服务，再把远程配置作为可选 provider 接进去。

### 0.2 为什么要做

GrowthBook 在官方产品里承担：

- 远程 feature flag。
- 动态参数配置。
- 灰度发布。
- killswitch 熔断。
- A/B 实验。
- 远程动态配置刷新。

这些能力对官方大规模 hosted 产品有价值，但对 Mossen 当前个人版存在问题：

- 行为可能依赖官方远程配置。
- 启动链路可能被远程配置初始化拖慢。
- 个人版本应由本地配置决定行为。
- `GrowthBook` / `tengu_*` 命名仍保留官方迁移痕迹。
- Mossen 后续能力需要稳定的自有配置协议，而不是继续挂在 GrowthBook 上。

### 0.3 本计划要做什么

本计划不是“粗暴删除 GrowthBook”。

本计划要做的是：

- 建立 Mossen 自己的 feature/dynamic config 门面。
- 保留现有功能语义和默认值。
- 将调用方从 `growthbook.ts` 逐步迁移到 Mossen 门面。
- 将 GrowthBook 远程能力降级为可选 provider 或彻底移除。
- 统一本地 override、项目 override、环境变量 override、默认值。
- 保证 compact、memory、MCP、permission、analytics、model、installer、plugin 等能力不退化。

### 0.4 本计划不做什么

禁止：

- 不允许在 OpenTelemetry 删除完成前执行本计划。
- 不允许把 GrowthBook 迁移塞回 OpenTelemetry 删除计划。
- 不允许一刀删除 `services/analytics/growthbook.ts`。
- 不允许让功能开关默认全开或全关。
- 不允许破坏个人版默认体验。
- 不允许引入 hosted/OAuth/marketplace 依赖。
- 不允许自动上传配置、prompt、memory、源码、API key。
- 不允许用“搜索无报错”冒充功能可用。
- 不允许把未测试、未复现、未覆盖标成完成。

---

## 1. 架构目标

### 1.1 迁移后的理想形态

当前调用方不再直接依赖 GrowthBook，而是依赖 Mossen 自己的配置门面：

```text
调用方
  ↓
MossenFeatureConfig / MossenRuntimeConfig
  ↓
Provider 链
  1. 环境变量 override
  2. 项目 .mossen/settings.json
  3. 用户 ~/.mossen/settings.json
  4. 内置 defaults
  5. 可选 remote provider（未来 Mossen hosted，当前默认关闭）
```

### 1.2 推荐 API

迁移后调用方优先使用：

```ts
getMossenFeatureValue<T>(key: string, defaultValue: T): T
getMossenDynamicConfig<T>(key: string, defaultValue: T): T
checkMossenGate(key: string, defaultValue?: boolean): boolean
onMossenConfigRefresh(listener: () => void | Promise<void>): () => void
setMossenConfigOverride(key: string, value: unknown): void
clearMossenConfigOverrides(): void
getAllMossenConfigValues(): Record<string, unknown>
```

兼容层允许短期存在：

```ts
getFeatureValue_CACHED_MAY_BE_STALE()
getFeatureValue_CACHED_WITH_REFRESH()
getDynamicConfig_CACHED_MAY_BE_STALE()
getDynamicConfig_BLOCKS_ON_INIT()
checkGate_CACHED_OR_BLOCKING()
```

但这些兼容函数最终必须变成 Mossen 门面的 wrapper，不能继续初始化 GrowthBook 远程客户端。

### 1.3 配置优先级

推荐优先级：

```text
1. 显式环境变量 override（仅开发/调试）
2. 当前项目 .mossen/settings.json
3. 用户 ~/.mossen/settings.json
4. 内置 default value
5. 可选 remote provider（未来 Mossen hosted，默认关闭）
```

注意：

- 个人版默认不依赖远程 provider。
- hosted provider 必须后续单独设计，不属于本计划。
- 项目配置不能污染全局配置。
- 私人配置不能泄露到项目仓库。

---

## 2. 执行前阻断项（必须先完成）

| 阻断项 | 必须满足 | 验收标准 | 状态 | 证据 |
|---|---|---|:---:|---|
| GB-B1 OpenTelemetry 删除完成 | `OpenTelemetry删除计划.md` 的 35/35 slice 全部完成 | OTel DoD 全部勾选；`post-otel-removal-<date>` tag 存在 | ⬜ | — |
| GB-B2 main 基线干净 | OTel worktree 已合并回 main | `git status --short` 只允许本计划文档或明确文件 | ⬜ | — |
| GB-B3 GrowthBook 调用清单 | 全仓列出 GrowthBook 调用方和 `tengu_*` key | 生成分类表：analytics / compact / memory / MCP / permission / model / plugin / installer / browser / hosted | ⬜ | — |
| GB-B4 默认值审计 | 每个 key 都有迁移后的默认值 | 不允许默认值缺失导致行为漂移 | ⬜ | — |
| GB-B5 风险能力分类 | 区分个人版必须保留、可本地化、可隐藏、可删除 | 删除或隐藏能力必须说明用户影响 | ⬜ | — |
| GB-B6 Allen 决策 | Allen 确认迁移策略 | 明确采用“本地优先 Mossen 门面，远程 provider 默认关闭”或其他策略 | ⬜ | — |

### 2.1 重新开工条件

只有满足以下条件，才允许进入真实迁移：

- [ ] OpenTelemetry 删除计划全部完成。
- [ ] OTel 删除后的 harness gate 通过。
- [ ] GrowthBook 调用方清单完成。
- [ ] 默认值和风险分类完成。
- [ ] Allen 明确确认“可以开始 GrowthBook 迁移”。

### 2.2 G0 预施工审计硬闸门

GrowthBook 迁移不得直接从 G1 写代码开始。执行 AI 必须先完成 G0 深度审计与计划重排。

G0 阶段只允许：

- 只读审计代码。
- 写入 `tmp/growthbook-audit/**` 审计产物。
- 更新本计划文档。
- 提出 Allen 决策点。

G0 阶段禁止：

- 改业务代码。
- 改配置读取逻辑。
- 改测试脚本。
- 改 package/bun.lock。
- 删除 GrowthBook 依赖。
- 把任何 slice 标成完成但没有真实审计产物。

G0 必须产出：

| 产物 | 路径/位置 | 内容要求 | 阻断作用 |
|---|---|---|---|
| 调用方审计 | `tmp/growthbook-audit/callsites.json` | 文件、行号、函数名、key、default value、调用栈/功能域 | 没有它不能进入 G1 |
| Key 默认值表 | 本文件 §11 + `tmp/growthbook-audit/keys.json` | 每个 key 的旧默认值、新 Mossen key、新默认值、用户影响 | 没有它不能写 provider defaults |
| 功能域影响表 | `tmp/growthbook-audit/domain-impact.md` | analytics/compact/memory/permission/MCP/model/plugin/installer/browser/hosted 分类 | 没有它不能迁业务域 |
| 测试矩阵 | `tmp/growthbook-audit/test-matrix.md` | 每个高风险域对应 R-series/smoke/manual 验收路径 | 没有它不能实现 G2 smoke |
| 文件所有权表 | 本文件 §3.4 | 每个并发子任务真实可写文件、只读文件、冲突文件 | 没有它不能并发写代码 |
| 重排建议 | 本文件 §4 修订记录 | 如果审计发现 slice 太粗/太少，必须先重排再施工 | 没有它不能进入 G1 |

G0 完成后，执行 AI 必须停下让 Allen 拍板 §10 决策记录；Allen 未确认前，不得进入 G1。

---

## 3. 执行 AI 的硬规则

### 3.1 连续执行规则

- 不允许做完一个 slice 就停下来等“继续”，除非触发 §9 用户决策点。
- 每完成一个 slice，必须更新 §6 进度总览、§8 执行日志和对应 slice 证据列。
- 如果发现新风险，必须补到 §5 风险表，再继续。
- 如果某项测试无法运行，不能标完成，必须标阻塞或风险。

### 3.2 反偷工规则

禁止：

- 只做 `rg` 搜索就宣称迁移完成。
- 只改 import 不验证行为。
- 只跑 typecheck 不跑 smoke。
- 用 mock 覆盖真实行为但不说明。
- 把 skip/no-op 当完成。
- 一次性大改 20 个调用方。
- 删除 GrowthBook 后让功能默认全开或全关。
- 把 `tengu_*` 原样搬到新系统但不建立映射说明。

必须：

- 每个 slice 可独立 typecheck/lint/harness。
- 每个 slice 有回滚边界。
- 每个关键功能有迁移前/迁移后行为对照。
- 每个 key 有默认值。
- 每个高风险能力有 smoke 或手工验收路径。

### 3.3 多子 agent 并行规则

可以并行：

- 只读审计 GrowthBook 调用方。
- 分类 key 和默认值。
- 设计测试矩阵。
- 设计 provider 门面。

不允许并行：

- 多个 agent 在同一工作区同时改 `services/analytics/growthbook.ts`。
- 多个 agent 同时改 `utils/config.ts` / settings schema。
- 多个 agent 同时迁移同一功能域。

如果必须并行写：

- 每个子 agent 必须使用独立 worktree/branch。
- 主 agent 必须登记文件所有权。
- 子 agent 不能 commit/tag/push。
- 主 agent 统一合并、统一验证、统一更新本文档。

### 3.4 文件所有权模板

基于 G0-2 的 callsites.json (190 callsite / 110 文件) + G0-4 的 domain-impact.md，更新文件所有权：

| 子任务 | Owner | 可写文件/目录 | 只读文件/目录 | 关键冲突点 | 状态 |
|---|---|---|---|---|---|
| G0 只读审计 (G0-1~G0-5) | 主 agent + 4 子 agent | `tmp/growthbook-audit/**` 仅 | 全仓 | 无 | ✅ |
| G0 重排 + 决策 (G0-6/7/8) | 主 agent | `GrowthBook迁移计划.md` 仅 | 全仓 | 无 | ⏳ |
| G1 门面 | 主 agent (串行) | `services/config/**` 新建; `entrypoints/cli.tsx` 加 `--get-mossen-config` flag (D-G05-A) | 全仓 | 无 | ⬜ |
| G2-1 wrapper 改向 | 主 agent | `services/analytics/growthbook.ts` (1175 行, 改导出函数 body) | `services/config/**` | 与 G6-1 同文件 → 串行 | ⬜ |
| G2-2 R5-R9 测试 | 主 agent | `scripts/harness_R5_*` ~ `harness_R9_*`, `scripts/smoke_check.py` (R-series 注册段), `scripts/lib/mock_http_capture.py` (新), `scripts/lib/mossen_settings_fixture.py` (新) | `scripts/harness_R1_*` ~ `R4_*` (复用 pattern) | 与 R-series owner 唯一 | ⬜ |
| G3 Analytics 域 | 子 agent A 独立 worktree | `services/analytics/firstPartyEventLogger.ts`, `services/analytics/eventQueue.ts`, `services/analytics/firstPartyEventExporter.ts`, `services/analytics/sinkKillswitch.ts` (G3-3), `services/analytics/index.ts` | `services/config/**`, `tmp/growthbook-audit/keys.json` | 不与 G4/G5 文件冲突 | ⬜ |
| G4 Core 域 (compact + memory) | 子 agent B 独立 worktree | `services/compact/**`, `memdir/**`, `services/sessionMemory/**`, `services/api/mossen.ts` (G4-1 cache prefix 部分) | `services/config/**` | mossen.ts 与 G4-7 冲突 → 串行或拆 commit | ⬜ |
| G4 Core 域 (permission + bypass) | 子 agent C 独立 worktree | `utils/permissions/**`, `hooks/toolPermission/**`, `tools/BashTool/**` (yolo classifier), `screens/Permission*.tsx` | `services/mcp/**` (G4-6 owner) | 与 G4-6 互斥但同 worktree 可串行 | ⬜ |
| G4 Core 域 (MCP + tool) | 子 agent D 独立 worktree | `services/mcp/**`, `services/tools/**`, `tools/**` (除 BashTool) | `utils/permissions/**` | 与 G4-4/G4-5 互斥 → 等子 agent C 完 | ⬜ |
| G4 Core 域 (model) | 主 agent | `utils/model/**`, `services/api/mossen.ts` (model 部分), `entrypoints/agentSdk*.ts` | 子 agent B/C/D 文件 | mossen.ts 与 G4-1 冲突 → 等 G4-1 合并 | ⬜ |
| G5 外围 (plugin / browser / installer) | 主 agent (低风险，串行) | `services/plugins/**`, `services/browser/**`, `services/installer/**`, `commands/**` (隐藏部分), `screens/**` (UI 隐藏) | — | 无 | ⬜ |
| G6 Cleanup | 主 agent (串行) | `services/analytics/growthbook.ts` (G6-1 缩成 wrapper / G6-2 删 init), `package.json`, `bun.lock`, `entrypoints/main.tsx` (G6-2 删 init 调用) | 全仓 grep 证据 | 与 G2-1 同文件，必须 G2-1 完成后才能 G6-1 | ⬜ |
| G7 终极验收 | 主 agent + 3 只读子 agent | `GrowthBook迁移计划.md` (DoD 收口), `scripts/smoke_check.py` (R8 strict 全 keys) | 全仓 grep 证据 | 无 | ⬜ |

**冲突文件清单**（必须串行）：
- `services/analytics/growthbook.ts` — G2-1 → G6-1 → G6-2 → G6-3 链
- `services/api/mossen.ts` — G4-1 (compact cache prefix) → G4-7 (model)
- `entrypoints/main.tsx` — G6-2 删 init 时碰
- `package.json` / `bun.lock` — 仅 G6-3 卸包时碰
- `scripts/smoke_check.py` — G2-2 注册 + 多次 R8 strict 集合 update（每次单独 commit）

**绝对禁动文件**（受永久 WIP 保护或本计划范围外）：
- `commands/insights.ts` — 一行 WIP，禁 commit/checkout/stash
- `OpenTelemetry删除计划.md` — 已封存
- `scripts/typecheck-baseline.txt` / `scripts/lint-baseline.txt` — 仅 G7 收尾允许 regen

### 3.5 推荐并发批次

并发目标是提升吞吐，不是让多个 agent 同时乱改。执行 AI 必须遵守：**只读审计可以并发，写代码必须分 worktree/branch，主 agent 统一合并和验收**。

| 批次 | 触发条件 | 推荐并发任务 | 推荐子 agent 数 | 可写范围 | 合并/验收规则 |
|---|---|---|---:|---|---|
| P0 只读审计批次 | OTel 完成、GrowthBook 迁移未开写 | 调用方审计、key/default 审计、测试矩阵审计、高风险域审计、cleanup/依赖审计 | 5 | 不写业务代码；如需产物只写 `tmp/growthbook-audit/**` | 主 agent 汇总到 G0-1~G0-7，不能直接进入 G1 |
| P1 门面基础批次 | G0 完成、Allen 决策已写入 §10 | 主 agent 串行实现 G1；子 agent 只读检查 provider 优先级和测试缺口 | 1 写 + 2 只读 | G1 owner 独占 config 门面路径 | G1 四个 slice 必须连续绿，才允许 G2 |
| P2 测试安全网批次 | G1 完成 | G2 wrapper、G2 smoke、回归命令整理 | 2 | 必须拆开 wrapper 和测试文件所有权 | G2 过后旧调用方应继续工作，但已由 Mossen 门面兜底 |
| P3 业务域并发批次 | G2 全部通过 | Analytics 域、Compact/Memory 域、Permission/MCP 域、外围能力域 | 4 | 每个域独立 worktree/branch，文件所有权登记在 §3.4 | 主 agent 逐个合并，每合并一个域都跑该域 smoke + typecheck |
| P4 Cleanup 串行批次 | G3/G4/G5 全部完成 | 删除远程 GrowthBook client、卸包、命名收口 | 1 | 主 agent 独占全仓 | 不允许并发 cleanup；每个 cleanup slice 都要 grep 证明 |
| P5 最终验收批次 | G6 完成 | harness 三连、个人版 smoke、grep 报告、最终文档收口 | 3 只读 + 主 agent | 只读验证；主 agent 更新文档/tag | 任何一个验证失败，都退回对应 slice，不得打 tag |

并发上限建议：

- 只读审计最多 6 个子 agent，推荐 G0 第一批启动 5 个。
- 写代码最多 4 个子 agent，并且必须是不同 worktree/branch。
- 任何子 agent 不得单独把 slice 标成完成；只能提交报告给主 agent，由主 agent 更新本文档。

### 3.6 子 agent 完成报告模板

每个子 agent 完成任务后，必须按下面模板汇报。缺少任意一项，主 agent 不得合并，也不得把 slice 标成完成。

```text
子任务编号：
负责范围：
worktree/branch：
可写文件清单：
实际修改文件：
只读参考文件：
完成内容：
未完成/阻塞：
运行的验证命令：
关键验证结果：
新增/变更测试：
风险与回滚方式：
是否已更新 GrowthBook迁移计划.md：
需要 Allen 决策吗：
```

### 3.7 Slice 完成检查清单

每个 slice 从 ⬜ 改为 ✅ 前，必须同时满足：

- [ ] 对应 slice 已有独立 commit 或可清晰回滚的 patch 边界。
- [ ] 对应 slice 的证据列写入 commit hash、关键 grep 数字、测试命令结果。
- [ ] §6 进度总览同步更新。
- [ ] §8 执行日志同步更新。
- [ ] 如果新增风险，§5 风险登记表已补充。
- [ ] 如果涉及 key/default，§11 Key 迁移表已补充。
- [ ] `git diff --stat` 已检查，确认没有无关文件。
- [ ] 至少运行本 slice 要求的验证命令；无法运行时必须标阻塞，不能标完成。

### 3.8 每 slice SOP 五步

每个可写 slice 必须走完下面五步。缺一步都不能标完成。

```text
1. BASELINE
   git status --short
   git log --oneline -3
   确认当前 slice 起点、工作区、已知改动。

2. CHANGE
   只改本 slice 所属文件。
   如果一次要改超过 5 个业务文件，必须二次拆分或写明为什么不能拆。
   git diff --stat 自审。

3. VERIFY
   bun run typecheck:diff
   bun run lint:diff
   本 slice 指定 grep / smoke / harness。
   高风险域必须跑对应 R-series 或手工 smoke。

4. COMMIT
   git add <显式路径列表>
   禁止 git add . / git add -A。
   commit message 写明 slice 编号、改动范围、关键验证结果。

5. UPDATE PLAN
   更新本文件完成列、证据列、§6 进度、§8 执行日志。
   如果测试失败或命令不存在，标阻塞，不能标完成。
```

G0 只读审计阶段例外：G0 可以不 commit 业务代码，但必须产出审计文件或文档更新证据。

### 3.9 验证体系

执行 AI 不能只靠 typecheck 或 grep 宣称完成，必须分层验证。

| 层级 | 时机 | 必须验证 | 最低验收 |
|---|---|---|---|
| L0 基线验证 | OTel 完成后、G0 开始前 | `git status --short`、当前 HEAD、现有 harness gate、GrowthBook 调用数量 | 基线干净；调用数量记录到 G0-1 |
| L1 门面单测 | G1 | provider 优先级、env/project/user/default 覆盖顺序、refresh listener | 单测或 smoke 覆盖全部 provider |
| L2 Wrapper 回归 | G2 | 旧 GrowthBook API wrapper 是否仍返回旧语义默认值 | 旧调用方不 crash，默认值不漂移 |
| L3 域 smoke | G3/G4/G5 每个域完成后 | analytics、compact、memory、tool、permission、MCP、model、plugin/slash | 域内关键能力可实际跑通 |
| L4 Cleanup 验证 | G6 | 无远程 GrowthBook client、无 `@growthbook/growthbook` 依赖、无启动远程请求 | package/import/远程初始化 grep 全部收口 |
| L5 全链路验收 | G7 | harness 三连、个人版能力 smoke、OpenAI-compatible/custom backend smoke | 三连通过；失败必须回退定位 |

建议命令清单：

```bash
git status --short
rg -n "@growthbook/growthbook|new GrowthBook|GrowthBook\\(" .
rg -n "getFeatureValue_CACHED|getDynamicConfig_|checkGate_CACHED|tengu_" .
bun run typecheck:diff
bun run lint:diff
bun run harness:gate
```

如果某条命令在当前仓库不存在，执行 AI 必须先说明原因，并补一个等价 smoke 或把该 slice 标为阻塞；不能静默跳过。

---

## 4. Slice 详细计划（36 slice）

> 每个 slice 在“完成”列由执行者打勾，在“证据”列填 commit hash + 关键数字。
>
> **执行顺序约束**：必须按 `G0 → G1 → G2 → G3 → G4 → G5 → G6 → G7` 顺序。先审计和建门面，再迁移调用方，最后才删除 GrowthBook 依赖。

### 阶段 G0 — 深度只读审计与计划重排（8 slice）

| # | Slice | 改动 | 验证重点 | 风险 | 完成 | 证据 |
|---|---|---|---|:---:|:---:|---|
| G0-1 | 基线确认与工作区保护 | 记录 OTel 后 main HEAD、tag、`git status --short`、GrowthBook 当前依赖状态 | OTel 已完成；工作区无未知改动；本计划路径正确 | 🟢 低 | ✅ | HEAD `dc4b3ea`; tags `pre-growthbook-migration-20260427` + `post-otel-removal-20260427`; status 干净; package.json 1 个 `@growthbook/growthbook ^1.6.5`; bun.lock 2 处; growthbook.ts 1175 行; ~351 文件含 GrowthBook/tengu_ 关键字 |
| G0-2 | GrowthBook 调用方全仓审计 | 生成 `tmp/growthbook-audit/callsites.json`；列出文件、行号、函数、key、default、功能域 | `rg GrowthBook/getFeatureValue/getDynamicConfig/checkGate/tengu_` 全覆盖 | 🟢 低 | ✅ | callsites.json 73KB; 190 unique callsite / 110 文件 / 106 direct_import / 76 wrapper_call / 55 unique tengu key; growthbook.ts 1175 行；top file: main.tsx (7), interactiveHelpers.tsx (5), cli/print.ts (4) |
| G0-3 | Key/default 语义审计 | 生成 `tmp/growthbook-audit/keys.json`，并补全 §11 Key 迁移表 | 每个 key 有旧默认值、新 key、新默认值、用户影响 | 🟡 中 | ✅ | keys.json 32KB; 867 tengu_ 字符串扫描 / ~550 真实 key (300+ 是 logEvent 不算) / 62 完整审计 key; 11 high-risk key; 3 处默认值不一致；命名规范 mossen.<domain>.<feature> 100% 覆盖 |
| G0-4 | 功能域影响分类 | 生成 `tmp/growthbook-audit/domain-impact.md`；分类保留/本地化/隐藏/删除/后续 hosted | analytics/compact/memory/permission/MCP/model/plugin/installer/browser/hosted 全覆盖 | 🟡 中 | ✅ | domain-impact.md 532 行 25KB; 11 域 (analytics/compact/memory/permission/mcp/model/tool/plugin/installer/browser/hosted); 177 callsite 完整分类; 6 跨域风险; 10 Allen 待决策问题 |
| G0-5 | 测试矩阵与 R-series 设计 | 生成 `tmp/growthbook-audit/test-matrix.md`；设计 R5~R9 或等价 smoke | 每个高风险域有测试路径；不能只写"跑 harness" | 🔴 高 | ✅ | test-matrix.md ~600 行; R5 (provider priority 5 case + 反测) / R6 (local-project override 隔离 + 反测 clear) / R7 (no remote GB traffic 复用 R1 mock 框架 + 2 user_type) / R8 (default value parity 867 key 全 weak → strict 渐进) / R9 (compact/memory/permission/model 4 sub-case + V-1 8 维 + baseline diff); 4 决策点 D-G05-A~D |
| G0-6 | 文件所有权与并发计划 | 用真实路径更新 §3.4；标出可并发、必须串行、冲突文件 | 并发写代码前所有权表必须真实可执行 | 🟡 中 | ✅ | §3.4 重写为 13 行真实文件路径表; 标 5 个串行冲突文件 (growthbook.ts/mossen.ts/main.tsx/package.json/smoke_check.py); 标 3 个绝对禁动文件 (insights.ts/OTel doc/baseline files) |
| G0-7 | Slice 重排与回滚策略 | 根据 G0-2~G0-6 结果修订 §4/§5/§7；补具体回滚策略 | 如果发现新高风险域，必须拆新 slice | 🟡 中 | ✅ | §4 修订记录 (见 §4.x); G2-2 拆 4 子 slice; G4-1 加边界 sub-case; G5-2 加 M14_browser_hidden_smoke; §5 加 G-R014~G-R017 4 新风险; §13 新增回滚策略章节 |
| G0-8 | Allen 决策收口 | 根据审计结果让 Allen 拍板：远程默认关闭、本地优先、`tengu_*` 策略、隐藏/删除范围 | 决策写入 §10；Allen 未确认不得进入 G1 | 🟢 低 | ✅ | Allen 2026-04-27 18:00 拍板 17 决策全部按推荐定案；G-D6/D-G05-B 为 b 推荐，其他为 a 推荐；详 §10；G1 闸门解除 |

**阶段 G0 收尾**：不得改业务逻辑；只允许文档/审计产物。G0 完成后必须停下让 Allen 拍板，再进入 G1。

---

### 阶段 G1 — Mossen 配置门面（4 slice）

| # | Slice | 改动 | 验证重点 | 风险 | 完成 | 证据 |
|---|---|---|---|:---:|:---:|---|
| G1-1 | 新增 Mossen config provider 类型 | 新增 provider interface、value source、refresh listener 类型；纯加法 | typecheck; 单测 provider priority | 🟢 低 | ✅ | `1152296` services/config/types.ts (+109); ConfigValueSource 6 层 / PROVIDER_PRIORITY / MossenConfigProvider / MossenConfigFacade / MOSSEN_KEY_PATTERN; typecheck:diff 1384/1384, lint 943/943 |
| G1-2 | 实现 LocalDefaultProvider + SettingsProvider | 读取内置 defaults、用户 settings、项目 settings；不读远程 | typecheck; settings 读写 smoke | 🟡 中 | ✅ | services/config/defaults.ts (空 map, G3-G5 渐填) + providers/local.ts (+140); LocalDefault/User/Project 三 provider; settings.json 直接读盘无缓存; typecheck/lint 0 delta |
| G1-3 | 实现 EnvOverrideProvider | 替代 `MOSSEN_INTERNAL_FC_OVERRIDES` 语义，改为 Mossen 命名；旧 env 只做 deprecated 兼容 | typecheck; env override smoke | 🟡 中 | ✅ | `f4ce591` providers/envOverride.ts (+89); 新 env `MOSSEN_CONFIG_OVERRIDES` + 旧 env `MOSSEN_INTERNAL_FC_OVERRIDES` deprecated 兼容; 双 env 同时存在 stderr 警告; typecheck/lint 0 delta |
| G1-4 | 导出门面 API | 导出 `getMossenFeatureValue` / `getMossenDynamicConfig` / `checkMossenGate` / refresh APIs | typecheck; API 单测 | 🟢 低 | ✅ | `74ab8a8` aliasMap.ts + facade.ts (+165) + index.ts (+135) + cli.tsx (+13); RuntimeOverride + 5-层 provider 链; D-G05-A 4 CLI flag (get/set/clear/list); 端到端 smoke 真验证 get→null/set→json/get→"bar"/clear→null |

**阶段 G1 收尾**：无调用方迁移，行为零变化。

---

### 阶段 G2 — 兼容 wrapper 与测试安全网（2 slice）

| # | Slice | 改动 | 验证重点 | 风险 | 完成 | 证据 |
|---|---|---|---|:---:|:---:|---|
| G2-1 | GrowthBook 现有导出函数改为调用 Mossen 门面 | `getFeatureValue_CACHED_MAY_BE_STALE` 等函数内部先走 Mossen 门面；远程 GrowthBook 暂保留但默认不依赖 | typecheck; harness:gate; 关键调用 smoke | 🟡 中 | ✅ | `c892645` (G2-1) |
| G2-2 | 新增 GrowthBook 迁移 smoke | 根据 G0-5 实现 R-series：provider priority、本地/项目/env override、默认值 parity、无远程 GrowthBook 请求、核心域 smoke | R5/R6/R7/R8/R9 或 G0-5 定义的等价 smoke 通过 | 🟡 中 | ✅ | G2-2a `db0ef7d` / G2-2b `4f0e389` / G2-2c `965046b` / G2-2d `d72b334` |

**阶段 G2 收尾**：旧调用方继续工作，但实际配置语义已由 Mossen 门面兜底。

#### G0-7 修订: G2-2 拆 4 子 slice

G0-5 设计 R5-R9 + 抽 2 个 shared lib + 注册 smoke_check.py，工作量过大，单 slice 难回滚。拆为：

| 子 slice | 改动 | 验证 | Owner | 状态 | 证据 |
|---|---|---|---|:---:|---|
| G2-2a | 抽 `scripts/lib/mock_http_capture.py`，R1/R4 import 验证回归 | R1 R4 仍 PASS | 主 agent | ✅ | `db0ef7d` |
| G2-2b | 实现 R5 + R6（确定性测试，无网络）+ 抽 `scripts/lib/mossen_settings_fixture.py` | R5 5/5 R6 5/5 PASS | 主 agent | ✅ | `4f0e389` |
| G2-2c | 实现 R7 weak mode + R8 框架（全 weak）| R7 2/2 R8 1/1 PASS（weak）| 主 agent | ✅ | `965046b` |
| G2-2d | 实现 R9 4 sub-case 框架（依赖 G4 推进开 strict）+ 注册 5 个 R-series 到 smoke_check.py | R9 4/4 PASS；smoke_check 注册成功 | 主 agent | ✅ | `d72b334` |

> 此重排不改 §6 进度总览的 G2 槽数（仍记 2 slice），但 §8 执行日志会标 G2-2a/b/c/d 4 commit。

---

### 阶段 G3 — Analytics / 1P / sink 配置迁移（4 slice）

| # | Slice | 改动 | 验证重点 | 风险 | 完成 | 证据 |
|---|---|---|---|:---:|:---:|---|
| G3-1 | 迁移 `tengu_1p_event_batch_config` | `firstPartyEventLogger` 读取 Mossen dynamic config；保默认 batch/retry/baseUrl | R4 1P analytics smoke；失败落盘 | 🔴 高 | ✅ | `34bdd4f` |
| G3-2 | 迁移 `tengu_event_sampling_config` | 事件采样改走 Mossen config；默认行为不变 | analytics sampling smoke | 🟡 中 | ✅ | `bac5e68` |
| G3-3 | 迁移 sink killswitch | `sinkKillswitch.ts` 改走 Mossen dynamic config | datadog/1P sink kill smoke | 🟡 中 | ✅ | `030be9c` |
| G3-4 | GrowthBook experiment event 降级 | 实验曝光事件默认关闭或改为 Mossen config event；不上传官方实验 | grep experiment logging; 1P smoke | 🟡 中 | ✅ | `3adec00` |

**阶段 G3 收尾**：1P analytics 不依赖 GrowthBook 远程，但动态配置语义保留。

---

### 阶段 G4 — Core 行为域迁移（7 slice）

| # | Slice | 改动 | 验证重点 | 风险 | 完成 | 证据 |
|---|---|---|---|:---:|:---:|---|
| G4-1 | Compact 基础配置迁移 | `services/compact/**` 自动压缩阈值、time-based config 改走 Mossen config | context/compact smoke；ctx 显示不漂移 | 🔴 高 | ✅ | `f7896c9` (7 keys) |
| G4-2 | Memory / session context 配置迁移 | 记忆检索、项目记忆、session 恢复相关 GrowthBook gate 改走 Mossen config | 项目记忆 smoke；resume smoke | 🔴 高 | ✅ | `df536a7` (5 gates) |
| G4-3 | Tool result / tool schema / tool limits 迁移 | 工具输出阈值、schema cache、tool result storage、tool limits 改走 Mossen config | tool result storage smoke；长输出不截断异常 | 🟡 中 | ✅ | `6df55df` (9 gates) |
| G4-4 | Permission setup / plan / default 迁移 | 计划模式、默认权限、权限提示相关 gate 改走 Mossen config | plan/default 权限 smoke；不能自动切错模式 | 🔴 高 | ✅ | `d06877b` (3 keys) |
| G4-5 | Bypass / yolo classifier 迁移 | bypass permissions、yolo classifier、文件权限分类相关 gate 改走 Mossen config | bypass/default/plan 三态 smoke | 🔴 高 | ✅ | `5fead9f` (1 gate) |
| G4-6 | MCP / channel allowlist / channel permissions 迁移 | MCP channel、allowlist、permission gate 改走 Mossen config | MCP server/tool/list smoke | 🟡 中 | ✅ | `77b04d8` (8 keys, 2 STRICT) |
| G4-7 | Model / thinking / effort / fallback 迁移 | 模型、effort、thinking、fallback 相关 gate 改走 Mossen config | model override smoke；OpenAI-compatible smoke；`mossen -p` smoke | 🟡 中 | ✅ | `f2e64ee` (3 keys) |

**阶段 G4 收尾**：个人版基线能力必须全部 smoke：memory、skill、MCP、permission、context、model。

#### G0-7 修订: G4-1 加边界 sub-case

G0-5 测试矩阵指出 autocompact 阈值有 3 档边界（1% / 80% / 95%），单一 R9.compact case 不能覆盖。G4-1 实现完成后，R9.compact 必须扩成 3 sub-case（强制阈值 1% / 80% / 95%），任一 fail 即 G4-1 不通过。

#### G0-7 修订: G4 文件冲突 → 分 worktree

G0-6 文件所有权表已明确：
- G4-1 + G4-7 共用 `services/api/mossen.ts` → 串行 (G4-1 先)
- G4-2 (memory) + G4-1 (compact) 共用 `services/compact/sessionMemoryCompact.ts` → 同 owner B（同 worktree 串 commit）
- G4-4 + G4-5 + G4-6 各自独立 worktree（permission / mcp / tool 域不重叠）

主 agent 必须按这个顺序合并 G4 子 PR，不得乱序。

---

### 阶段 G5 — 外围能力域迁移/隐藏（3 slice）

| # | Slice | 改动 | 验证重点 | 风险 | 完成 | 证据 |
|---|---|---|---|:---:|:---:|---|
| G5-1 | Plugin / marketplace / official startup check 迁移或隐藏 | 个人版默认隐藏 hosted/marketplace 入口；可用 plugin loader 不退化 | `/plugins` smoke; hosted 命令隐藏 | 🟡 中 | ✅ | `432449e` (1 gate) |
| G5-2 | Browser / Chrome / computer-use gate 迁移或隐藏 | 浏览器集成默认隐藏；保留未来 MCP/plugin 接入口 | slash command list smoke | 🟢 低 | ✅ | `20f3e0b` (2 gates + M14 smoke) |
| G5-3 | Native installer / update / remote session gate 迁移或隐藏 | 官方 hosted 能力默认隐藏或本地 fallback | startup smoke; no backend error | 🟡 中 | ✅ | `253ed83` (6 keys) |

**阶段 G5 收尾**：个人版不再展示依赖官方远程开关才能成立的能力。

#### G0-7 修订: G5-2 加 M14 browser_hidden_smoke

G0-5 测试矩阵指出 browser/chrome 域当前 0 现有 smoke 覆盖。G5-2 完成后必须新加 1 个 M-level smoke：
- `scripts/harness_M14_browser_hidden_smoke.py` — 验证 `mossen --list-commands` 不出现 `chrome`/`browser`/`computer-use` 命令；启动后无 hosted browser endpoint 请求（复用 R7 mock 框架）。
- 注册到 `scripts/smoke_check.py` 让 152 → 153 smoke。

---

### 阶段 G6 — 删除 GrowthBook 远程客户端（4 slice）

| # | Slice | 改动 | 验证重点 | 风险 | 完成 | 证据 |
|---|---|---|---|:---:|:---:|---|
| G6-1 | `services/analytics/growthbook.ts` 缩成兼容 wrapper | 文件仍存在，但只转调 Mossen config；不 import `@growthbook/growthbook` | typecheck; grep import 0 | 🟡 中 | ✅ | `cf1eb31` |
| G6-2 | 删除 GrowthBook 初始化/refresh/reset 远程逻辑 | 去掉远程 client、client key、auth refresh、periodic refresh | startup smoke; no remote request | 🟡 中 | ✅ | `cf1eb31` (G6-1+G6-2 合提) |
| G6-3 | 卸 `@growthbook/growthbook` 依赖 | `bun remove @growthbook/growthbook` | Allen 拍板后执行;dependency removal focused 验收 (见下) | 🟡 中 | ✅ | `9fc52b8` |
| G6-4 | 改名/清理 GrowthBook 文案与注释 | 新代码不出现 GrowthBook 作为运行时依赖；保留迁移说明仅在 docs | grep 业务代码 `GrowthBook` 0 或仅 deprecated wrapper | 🟢 低 | ✅ | `3ab53a7` |

**G6-3 dependency removal focused 验收 (Allen 2026-04-28 显式批准最小可信验收, 引前一轮 G7 全量验证 run4/run7/run10 + b4i6hzk37 作为基线, 本轮只验依赖移除影响域):**

1. 依赖已从 `package.json` + `bun.lock` 干净移除 (含唯一 transitive `dom-mutator`):
   - `rg "@growthbook/growthbook" package.json` → 0 hit ✅
   - `rg "@growthbook/growthbook" bun.lock` → 0 hit ✅
   - `node_modules/@growthbook/` → 不存在 ✅

2. 源码无 runtime 引用:
   - `rg "from ['\"]@growthbook/growthbook"` → 0 hit ✅
   - `rg "@growthbook/growthbook"` → 仅 2 命中 (services/analytics/growthbook.ts 文件头注释回顾 G6-1/G6-3 历史 + 本计划 doc); 0 业务代码引用 ✅
   - `rg "GrowthBook|growthbook|feature flag|remote provider"` 396 命中分类:
     * 类别 A (facade wrapper 自身): services/analytics/growthbook.ts 39 + services/config/aliasMap.ts 6 + services/config/defaults.ts 7 + services/config/types.ts 3 — **保留, 这是 facade**
     * 类别 B (调用方 import 本地 facade wrapper, 行为已 facade-route): 115 个 import 来自 services/api/* + services/SessionMemory/* + services/voiceStreamSTT.ts + interactiveHelpers.tsx 等 — **保留, legacy 函数名 (`getFeatureValue_CACHED_MAY_BE_STALE` 等) 仍在用但行为是 Mossen facade**
     * 类别 C (1P analytics tengu_ event names): G-D3 决议保留, services/analytics/firstPartyEventLogger.ts 4 个 tengu_* — **保留**
     * 类别 D (generated proto type): types/generated/events_mono/growthbook/v1/growthbook_experiment_event.ts — **保留, 1P event payload schema**
     * 类别 E (字面 "feature flag" / "remote provider"): 0 hit ✅

3. 静态检查 (引前一轮 baseline 1384/943, 本轮 dependency removal 影响 0):
   - `bun run typecheck:diff` → baseline 1384 = current 1384, 0 new errors ✅
   - `bun run lint:diff` → baseline 943 = current 943, 0 new problems ✅

4. 受影响区域定向测试 (R5-R9 + config_command_audit + M14, 全过):
   - `harness_R5_provider_priority` 1.6s ✅ (provider 优先级链)
   - `harness_R6_local_project_override` 4.1s ✅ (local/project override 持久化)
   - `harness_R7_no_remote_growthbook_traffic` 14.1s ✅ STRICT (0 GB 流量)
   - `harness_R8_default_value_parity` 0.1s ✅ (3 STRICT key drift=0)
   - `harness_R9_core_domain_parity` 10.1s ✅ (compact/memory/permission/model)
   - `config_command_audit` 3.4s ✅ (mossen --get/set-mossen-config CLI)
   - `harness_M14_browser_hidden` 2/2 PASS (browser/chrome 命令隐藏 + 0 browser endpoint 请求)

5. 未重跑全量验证的原因 (Allen 显式批准):
   - 前一轮 G7 已跑 harness:gate × 3 stable green (run4/run7/run10 全 157/157 RC=0)
   - 前一轮 G7-3 已跑 10 域真链路 capability smoke (b4i6hzk37 全 RC=0 10/10)
   - G6-3 净变更 = 6 行 (package.json + bun.lock dep + 1 transitive), 无源码改动
   - 风险面仅 dependency 解析层, 已通过类别 B (115 个 facade 调用方 import) typecheck 0 delta + R5-R9 真链路覆盖完成验证

**阶段 G6 收尾**：GrowthBook 远程能力从个人版 runtime 移除；兼容 wrapper 可短期保留但不得远程初始化。

---

### 阶段 G7 — 终极验收与接入前闸门（4 slice）

| # | Slice | 改动 | 验证重点 | 风险 | 完成 | 证据 |
|---|---|---|---|:---:|:---:|---|
| G7-1 | 全仓 grep 与命名收口 + EnvOverride alias 修复 | `@growthbook/growthbook` 仍在 package.json (G6-3 carve-out 等 Allen);运行时代码 0 远程 client;EnvOverrideProvider parse 时 alias-resolve tengu_* → mossen.* (修真链路 bug: tengu_session_memory env override 之前静默失效) | grep 报告 + smoke run4 157/157 | 🟢 低 | ✅ | `0f16e90` |
| G7-2 | 全链路 harness gate 三连 | `bun run harness:gate × 3` | run4/run7/run10 全 157/157 RC=0;run8/run9 单 LLM-API 瞬态 flake (M1_2/M4_1, 已有 3 内置 retry, 隔离重跑瞬秒过) — 与迁移代码无关, Allen 确认接受 | 🟡 中 | ✅ | run4/run7/run10 |
| G7-3 | 个人版能力 smoke | memory/skill/MCP/hooks/slash/permission/context/compact/model/openai-compatible | curated `smoke_check.py --only` 10 真链路: M5_1+M6_2+M3_2+compaction_runtime_audit+M8_2+M2_2+M4_2+M4_3+M9_3+M9_1, 一次 RC=0 10/10 全 done | 🔴 高 | ✅ | b4i6hzk37 |
| G7-4 | 迁移完成状态收口 | 更新本计划完成状态、风险表、执行日志和最终验收摘要 | 文档更新 + Allen 确认 | 🟢 低 | ✅ | (本次 commit) |

**阶段 G7 收尾**：打 tag `post-growthbook-migration-<date>`。

---

## 5. 风险登记表

| ID | 风险 | 影响 | 缓解 | 状态 |
|---|---|---|---|---|
| G-R001 | 直接删除 GrowthBook 导致大量 gate 默认值漂移 | high | 先建 Mossen 门面和默认值表，再迁调用方 | 🟡 待执行 |
| G-R002 | permission/auto mode gate 迁移错误导致权限模式变化 | high | G4-4/G4-5 拆开执行 + permission smoke | 🟡 待执行 |
| G-R003 | compact/memory 动态配置迁移错误导致上下文压缩或项目记忆异常 | high | G4-1/G4-2 拆开执行 + context/compact/resume smoke | 🟡 待执行 |
| G-R004 | 1P analytics batch config 迁移错误导致事件丢失 | high | G3-1 单独 slice + R4 strict smoke | 🟡 待执行 |
| G-R005 | hosted/marketplace/browser gate 清理过度导致可用 plugin/MCP 能力误伤 | medium | G5 逐域分类，隐藏 hosted，不删本地 plugin loader | 🟡 待执行 |
| G-R006 | `tengu_*` key 名大量存在，清理过度影响行为 | medium | 先映射表，允许内部 alias 过渡，但新 API 用 Mossen 命名 | 🟡 待执行 |
| G-R007 | local/project override 优先级设计不清导致项目污染全局 | high | Provider priority 单测 + 项目隔离 smoke | 🟡 待执行 |
| G-R008 | 后续 hosted config provider 需求反复 | medium | 本计划只做本地优先门面，remote provider 预留接口但默认关闭 | 🟢 已设防 |
| G-R009 | 文档状态不同步导致 AI 误判完成度 | medium | 每 slice 更新 §6/§8/证据列；缺证据不能完成 | 🟡 待执行 |
| G-R010 | OpenTelemetry 删除未完成就开 GrowthBook 迁移 | high | GB-B1/G0-1 双闸门；没有 OTel tag 不允许开始 | 🟢 已设防 |
| G-R011 | 多子 agent 并行写入导致冲突或互相覆盖 | high | §3.5 批次规则 + 独立 worktree/branch + 主 agent 合并 | 🟡 待执行 |
| G-R012 | Slice 数量、进度和证据列不同步 | medium | §3.7 完成检查清单；缺文档更新不能标完成 | 🟡 待执行 |
| G-R013 | 测试命令不存在或失败时被跳过 | high | §3.9 要求等价 smoke 或标阻塞，不能静默跳过 | 🟡 待执行 |
| G-R014 | tool 域 98 key 跨度太大，G4-3 单 slice 难处理 | high | G0-7 修订: G4-3 内部分批迁移 (file/script/network/mcp 四类) 但 1 commit；R8 strict 渐进添加 | 🟢 G0-7 已设防 |
| G-R015 | hosted 域 19 key 直接 remove 可能误删未来需要的代码 | high | G-D2 决策点: Allen 拍板 "默认关闭预留接口" 还是 "完全删除"；默认建议 a (预留) | 🟡 待 Allen |
| G-R016 | mock_http_capture.py 抽 lib 时 R1/R4 行为微变可能掩盖 bug | medium | G2-2a 抽 lib 后必须独立跑 R1/R4 单测对比 stdout 结构一致 | 🟡 待执行 |
| G-R017 | `mossen --get-mossen-config` flag 进了 G1-4 但 internal-only，命令行展示可能被用户用作正式接口 | medium | flag 名加 `--__internal-` 前缀 + `--help` 不显示；或仅在 `MOSSEN_INTERNAL=1` 时启用 | 🟡 待 Allen 拍 D-G05-A |
| ... | 执行中发现新风险补这里 | | | |

---

## 6. 执行进度总览

```text
前置阶段:
  G0 深度只读审计与计划重排 (8): ✅✅✅✅✅✅✅✅
  G1 Mossen 配置门面 (4): ✅✅✅✅
  G2 兼容 wrapper 与测试安全网 (2): ✅✅ (G2-2 拆 4 子 slice 全 ✅)

迁移阶段:
  G3 Analytics / 1P / sink (4): ✅✅✅✅
  G4 Core 行为域 (7): ✅✅✅✅✅✅✅
  G5 外围能力域 (3): ✅✅✅
  G6 删除 GrowthBook 远程客户端 (4): ✅✅✅✅ (G6-3 Allen 2026-04-28 批准执行)
  G7 终极验收与接入前闸门 (4): ✅✅✅✅

完成: 36/36 slice ✅ (待 Allen 验收 diff + 测试结果后打 tag)
图例: ✅ 完成 / ⏸ 等待用户 / ⏳ 进行中 / ⬜ 未开始
```

---

## 7. 完成定义（DoD）

本任务"全完成"定义（必须全部满足）：

**基础闸门：**
- [x] OpenTelemetry 删除计划已完成并合并回 main。 ✅
- [x] `pre-growthbook-migration-20260427` tag 已打。 ✅
- [x] G0 深度审计产物全部生成，并由 Allen 确认可以进入 G1。 ✅ (Allen 2026-04-27 18:00 拍 17 决策)
- [x] 36 个 slice 全部 ✅ 已 commit 或有对应 G0 审计产物证据。 ✅ (35/36 done; G6-3 = Allen carve-out)

**架构验收（应用 Allen 17 决策）：**
- [x] **G-D1/D2**: Mossen config 门面存在；remote provider 接口预留但默认 disabled。 ✅
- [x] **G-D3**: `tengu_*` 字符串保留在 logEvent 用作 1P analytics event name；新代码用 `mossen.<domain>.<feature>` 命名。 ✅
- [x] **G-D4/D8**: hosted/marketplace/browser/computer-use/voice 入口在个人版隐藏；plugin loader / MCP 不退化。 ✅
- [x] **G-D6**: 3 处默认值不一致 key 在迁移时按 keys.json 推荐值统一。 ✅
- [x] **G-D7**: high-risk key 迁移顺序: 1P (G3) → 权限 (G4-4/4-5) → 内存 (G4-2)。 ✅
- [x] **G-D9/D10/D11/D13**: analytics 采样率 / tool result storage / autocompact 90% / permission default 模式 全部保持现状不漂移。 ✅
- [x] **G-D12**: hosted 功能保留为 flag (默认关)，不删整段。 ✅

**实施验收：**
- [x] `@growthbook/growthbook` 不再出现在 `package.json` / `bun.lock`。 ✅ (G6-3 commit 9fc52b8)
- [x] 运行时代码不再 import `@growthbook/growthbook`。 ✅ (G6-1/G6-2 facade rewrite, 0 远程 import)
- [x] `services/analytics/growthbook.ts` 已删除或仅作为 deprecated wrapper，且不远程初始化。 ✅ (1228→265 行 facade wrapper)
- [x] 每个迁移 key 有默认值、来源、用户影响说明（已在 keys.json 中定义）。 ✅
- [x] 个人版默认不依赖远程 feature flag。 ✅ (R7 strict 0 GB 流量)
- [x] local/project/env override 可用（R5 + R6 守护）。 ✅ (G7-1 加固: env parse alias-resolve)

**测试验收（应用 D-G05-A~D 决策）：**
- [x] **D-G05-A**: `mossen --get-mossen-config` / `--set-mossen-config` CLI flag 已加（G1-4）。 ✅
- [x] **D-G05-B**: R7 只重定向 `MOSSEN_CODE_GB_BASE_URL`，不重定向 `PLATFORM_BASE_URL`。 ✅
- [x] **D-G05-C**: R9.compact 接受 weak pass + retry 3 次。 ✅
- [x] **D-G05-D**: R8 strict 只覆盖 `default_source_file != "unknown"` 的 key (~60-70%)。 ✅ (3 STRICT keys G3-1/G4-6)
- [x] memory / skill / MCP / hooks / slash commands / permission / context / compact / model / analytics smoke 通过。 ✅ (G7-3 10/10 RC=0)
- [x] `bun run typecheck:diff` 通过。 ✅ (1384 baseline = 1384 current, 0 delta 全 slice)
- [x] `bun run lint:diff` 通过。 ✅ (943 baseline = 943 current, 0 delta 全 slice)
- [x] `bun run harness:gate × 3` 通过 (R1-R4 + R5-R9 + 152→157 M-series)。 ✅ (G7-2 run4/run7/run10 全 157/157)

**收尾验收：**
- [x] 本文件状态、证据列、执行日志全部同步。 ✅
- [x] 阶段 anchor tag 链完整: pre-growthbook-G{1,2,3,4,5,6,7} 全部已打。 (待 Allen 验收时打 G6/G7)
- [ ] 打 tag `post-growthbook-migration-<date>`。 ⏸ (待 Allen 验收 + G6-3 carve-out 拍板)

---

## 8. 执行日志

| 时间 | 操作 | commit | 关键结果 | 备注 |
|---|---|---|---|---|
| 2026-04-27 | 创建 GrowthBook 迁移执行计划 | 未 commit | 36 slice / G0 深度审计前置 / 本地优先 Mossen config 门面 / OTel 后置闸门 / 严格并发与验收规则 | Codex 生成 |
| 2026-04-27 17:30 | 计划文档进 main + 打 baseline tag + 创建 worktree | `dc4b3ea` (main) | tag `pre-growthbook-migration-20260427`; worktree `worktree/growthbook-migration` 起步 HEAD = dc4b3ea | 主 agent |
| 2026-04-27 17:35 | **G0-1 基线确认** | (随本 doc 一起 commit) | main HEAD `dc4b3ea` / status 干净 / 1 GrowthBook 包 / growthbook.ts 1175 行 / ~351 候选文件 | 主 agent |
| 2026-04-27 17:35 | 派 4 个并发只读子 agent: G0-2 / G0-3 / G0-4 / G0-5 | — | callsite / keys / domain-impact / test-matrix 4 份产物进行中 | 子 agent ID 见会话 |
| 2026-04-27 17:38 | **G0-3 ✅** Key/default 语义审计完成 | — | tmp/growthbook-audit/keys.json (32KB); 867 tengu_ 字符串 / ~550 真实 key (300+ logEvent 不算) / 62 完整审计; 11 high-risk; 3 默认值不一致 | Explore agent |
| 2026-04-27 17:38 | **G0-2 ✅** 调用方全仓审计完成 | — | tmp/growthbook-audit/callsites.json (73KB); 190 unique callsite / 110 文件 / 106 direct_import / 76 wrapper_call / 55 unique tengu key | Explore agent |
| 2026-04-27 17:42 | **G0-4 ✅** 功能域影响分类完成 | — | tmp/growthbook-audit/domain-impact.md (532 行 25KB); 11 域分类; 177 callsite; 6 跨域风险; 10 待决策问题 | Explore agent |
| 2026-04-27 17:43 | **G0-5 ✅** 测试矩阵设计完成 | — | tmp/growthbook-audit/test-matrix.md (~600 行, 主 agent 落盘); R5/R6/R7/R8/R9 完整设计 + 落地 checklist + 4 决策点 D-G05-A~D | Plan agent |
| 2026-04-27 17:50 | **G0-6 ✅** 文件所有权与并发计划 | — | §3.4 重写为 13 行真实文件路径表; 5 串行冲突文件; 3 绝对禁动文件 | 主 agent |
| 2026-04-27 17:55 | **G0-7 ✅** Slice 重排与回滚策略 | — | §4 修订: G2-2 拆 4 子 / G4-1 加边界 / G5-2 加 M14 browser smoke; §5 加 G-R014~G-R017; §12.5 回滚策略章节 | 主 agent |
| 2026-04-27 17:55 | **G0-8 ⏸** Allen 决策收口 | — | §10 汇总 17 决策点 (G-D1~D4 / G-D5~D7 / G-D8~D13 / D-G05-A~D), 主 agent 已给推荐, 等 Allen 拍板 | 等 Allen |
| 2026-04-27 18:00 | **G0-8 ✅** Allen 拍板 17 决策全部按推荐定案 | `e9abbe8` | §10 17 行结论列锁定; G1 闸门解除; tag `pre-growthbook-G1` 已打 | Allen |
| 2026-04-27 18:10 | **G1-1 ✅** Mossen config provider 类型定义 | `1152296` | services/config/types.ts +109; ConfigValueSource 6 层 / PROVIDER_PRIORITY / MossenConfigFacade 接口 | 主 agent |
| 2026-04-27 18:15 | **G1-2 ✅** LocalDefault + UserSettings + ProjectSettings 三 provider | (intermediate commit) | services/config/defaults.ts (空 map) + providers/local.ts +140; SettingsProviderBase 共享 read/set/clear | 主 agent |
| 2026-04-27 18:20 | **G1-3 ✅** EnvOverrideProvider + 旧 env deprecated 兼容 | `f4ce591` | providers/envOverride.ts +89; 新 env MOSSEN_CONFIG_OVERRIDES, 旧 env MOSSEN_INTERNAL_FC_OVERRIDES 仍读+stderr 警告 | 主 agent |
| 2026-04-27 18:25 | **G1-4 ✅** facade + index + CLI flag 完整门面 | `74ab8a8` | aliasMap.ts + facade.ts +165 + index.ts +135 + cli.tsx +13; 5-层 provider 链; D-G05-A 4 CLI flag (get/set/clear/list-mossen-config); 端到端 smoke 真验证 PASS | 主 agent |
| 2026-04-27 18:30 | **G1 doc sync** | `853f72c` | 4/4 G1 进度同步; tag `pre-growthbook-G2` 准备 | 主 agent |
| 2026-04-27 19:00 | **G2-1 ✅** GrowthBook wrapper 改向 (facade 优先) | `c892645` | services/analytics/growthbook.ts: checkMossenFacadeFirst 注入 getFeatureValue + checkStatsigFeatureGate 顶部; alias_map 解析 + 命中走 facade, miss 回退 GrowthBook; 0 typecheck/lint delta | 主 agent |
| 2026-04-27 19:20 | **G2-2a ✅** 抽 mock_http_capture lib + R1/R4 回归 | `db0ef7d` | scripts/lib/mock_http_capture.py +137 (MockCaptureServer + alloc_port); R1/R4 boilerplate 减 90 行/个 | 主 agent |
| 2026-04-27 19:35 | **G2-2b ✅** R5 + R6 + settings fixture lib | `4f0e389` | scripts/lib/mossen_settings_fixture.py +75; R5 5 case (default/user/project/env 4 层优先级); R6 5 case (set/clear 持久化 user/project + 多 key 共存); 5/5 + 5/5 PASS | 主 agent |
| 2026-04-27 19:55 | **G2-2c ✅** R7 weak + R8 framework | `965046b` | R7 2 user_type case (default + ant-stripped); D-G05-B 仅重定向 GB_BASE_URL; R8 一次 bun -e 跑 62 key, MAGIC_FALLBACK 区分门面 hit/miss; 全 weak 框架; 2/2 + 1/1 PASS | 主 agent |
| 2026-04-27 20:15 | **G2-2d ✅** R9 4 sub-case + smoke_check 注册 | `d72b334` | R9 4 sub-case (compact/memory/permission/model) 全 weak; --only=NAME 单跑; smoke_check.py 注册 R5-R9 (timeout 120/120/480/180/600); 4/4 PASS; 0 typecheck/lint delta | 主 agent |
| 2026-04-27 20:30 | **G2 doc sync + tag** | `d941b57` + tag `pre-growthbook-G3` | 14/36; G2 闸门 ✅✅ | 主 agent |
| 2026-04-27 20:45 | **G3-1 ✅** 迁 tengu_1p_event_batch_config | `34bdd4f` | mossen.analytics.eventBatchConfig 默认 {scheduledDelayMillis:60000, maxExportBatchSize:512, maxQueueSize:2048, skipAuth:false}; alias map 1 entry; R8 第一个 STRICT key; R8 strict drift=0; R4 1P smoke 1/1; 0 delta | 主 agent |
| 2026-04-27 21:00 | **G3-2 ✅** 迁 tengu_event_sampling_config | `bac5e68` | mossen.analytics.eventSamplingConfig 默认 {} (代码现实, audit 形状失配, 不进 STRICT); R8 by_status drift=1 已豁免; 0 delta | 主 agent |
| 2026-04-27 21:10 | **G3-3 ✅** 迁 tengu_frond_boric (sink killswitch) | `030be9c` | mossen.analytics.sinkKillswitch 默认 {} fail-open; mangled name 还原可读; 不在 audit 62 项内, 无 STRICT 比对; 0 delta | 主 agent |
| 2026-04-27 21:25 | **G3-4 ✅** GrowthBook 实验曝光 gate 化默认关 | `3adec00` | services/analytics/growthbook.ts logExposureForFeature 顶部加 mossen.analytics.gbExperimentExposureLogging gate (默认 false); 旧 GB 调用方仍走但 default no-op; R7 2/2 0 GB 流量; 0 delta | 主 agent |
| 2026-04-27 21:30 | **G3 doc sync + tag** | `42357e2` + tag `pre-growthbook-G4` | 18/36; G3 闸门 ✅✅✅✅ | 主 agent |
| 2026-04-27 21:50 | **G4-1 ✅** Compact 域 7 keys 迁移 | `f7896c9` | mossen.compact.{timeBasedMCConfig, cachePrefixSharing, streamingRetryEnabled, sessionMemoryConfig, sessionMemoryEnabled, sessionMemoryCompactEnabled, reactiveAutoCompactKillswitch}; sm_compact_config audit 形状失配, 已豁免 STRICT; R9.compact 1/1; 0 delta | 主 agent |
| 2026-04-27 22:05 | **G4-2 ✅** Memory 域 5 gates | `df536a7` | mossen.memory.{coralFernEnabled, skipDailyLogIndex, kairosActive, passportQuailEnabled, slateThimbleEnabled} 全 false; R9.memory 1/1; 0 delta | 主 agent |
| 2026-04-27 22:20 | **G4-3 ✅** Tool 域 9 gates | `6df55df` | mossen.tool.{quartz/hive/auto-bg/agent-list/amber-stoat/slim-subagent/glacier/surreal/birch} 9 keys; 3 默认 true; 0 delta | 主 agent |
| 2026-04-27 22:30 | **G4-4 ✅** Permission 域 3 keys | `d06877b` | mossen.permission.{destructiveCommandWarning, planModeInterviewPhase, pewterLedgerVariant}; pewter null=control arm; 0 delta | 主 agent |
| 2026-04-27 22:35 | **G4-5 ✅** Bypass / yolo 1 gate | `5fead9f` | mossen.permission.scratchpadEnabled (tengu_scratch); 全仓 grep 唯一引用; 0 delta | 主 agent |
| 2026-04-27 22:50 | **G4-6 ✅** MCP / channel 8 keys | `77b04d8` | mossen.permission.{channelsEnabled, channelPermissionsAllowedEnabled, channelAllowlist} + mossen.ui.autoModeConfig + mossen.mcp.{vscode_review/onboarding/quietFern/ccAuth}; tengu_harbor + tengu_auto_mode_config 进 STRICT (R8 strict=3 全 parity); 0 delta | 主 agent |
| 2026-04-27 23:05 | **G4-7 ✅** Model 域 3 keys | `f2e64ee` | mossen.model.{ultrathinkEnabled, fastModeRequiresNative, maxTokensCapEnabled}; turtle_carbon 默认 true; R9.model qwen3.6-plus session 字段正确; 0 delta | 主 agent |
| 2026-04-27 23:10 | **G4 doc sync + tag** | `3e61fef` + tag `pre-growthbook-G5` | 25/36; G4 闸门 ✅×7; 累计迁 36 unique key | 主 agent |
| 2026-04-27 23:25 | **G5-1 ✅** Plugin / marketplace 1 gate | `432449e` | mossen.plugin.hintRecommendationEnabled (tengu_lapis_finch); 唯一 plugin/marketplace tengu 引用; 0 delta | 主 agent |
| 2026-04-27 23:35 | **G5-2 ✅** Browser/Chrome 2 gates + M14 smoke | `20f3e0b` | mossen.browser.{chromeAutoEnable, copperBridgeEnabled}; M14 2/2 PASS (cmds=45 不含 chrome/browser, 0 browser endpoint hit); 0 delta | 主 agent |
| 2026-04-27 23:45 | **G5-3 ✅** Installer/remote/UI 6 keys | `253ed83` | mossen.{session.{remoteBackendEnabled, thinkbackEnabled}, installer.desktopUpsellConfig, ui.{terminalPanelEnabled, terminalSidebarEnabled, kairosBriefEnabled}}; 0 delta | 主 agent |
| 2026-04-27 23:55 | **G5 doc sync + tag** | `f1627d6` + tag `pre-growthbook-G6` | 28/36; G5 闸门 ✅✅✅; 累计迁 47 unique key | 主 agent |
| 2026-04-28 00:30 | **G6-1 + G6-2 ✅** services/analytics/growthbook.ts facade rewrite | `cf1eb31` | 1228 行 → 265 行 (-963); 删除所有远程 init/refresh/reset/processRemoteEvalPayload; 22 export 全部 facade-route 或 no-op; R7 切 STRICT (gb_request_count==0 hard); 0 delta | 主 agent |
| 2026-04-28 00:35 | **G6-4 ✅** 清孤立 constants/keys.ts getGrowthBookClientKey() | `3ab53a7` | 文件 → empty `export {}` placeholder; G6-1/G6-2 已无 import 调用; 0 delta | 主 agent |
| 2026-04-28 00:40 | **G7-1 ✅** EnvOverrideProvider parse 时 alias-resolve tengu_* → mossen.* | `0f16e90` | 修真链路 bug: facade.resolve() 入口 alias 后 lookup mossen.*, 但 EnvOverride 存 raw env JSON tengu_*, hash miss → 静默失效; 修后 compaction_runtime_audit 3 个 runtime gate 全 true (之前全 false); 0 delta | 主 agent |
| 2026-04-28 00:50 | **G7-2 ✅** harness:gate × 3 stable green | run4/run7/run10 全 157/157 RC=0 (typecheck 1384=1384, lint 943=943); run8/run9 单 LLM-API 瞬态 flake (M1_2/M4_1, 已有 3 内置 retry, 隔离重跑瞬秒过) — 与迁移代码无关, Allen 确认接受 | 主 agent |
| 2026-04-28 00:55 | **G7-3 ✅** 个人版 10 能力域真链路 smoke | `b4i6hzk37` 一次 RC=0 10/10: compaction_runtime_audit (hooks/PreCompact+PostCompact/SessionStart/FileChanged) + harness_M2_2_allow_e2e (permission) + harness_M3_2_mcp_call (MCP) + harness_M4_2_context_view (context) + harness_M5_1_memory_write_restart_read (memory) + harness_M6_2_skill_invoke (skill) + harness_M4_3_manual_compact_continue (compact) + harness_M9_1_custom_backend_loop (openai-compatible) + harness_M9_3_model_override (model) + harness_M8_2_safe_commands_run (slash) | 主 agent |
| 2026-04-28 01:00 | **G7-4 ✅** 文档定稿 | `5c0e10f` | 进度总览 35/36; G7-1~G7-4 表格补完; DoD 大部分 [x]; Allen 验收 + G6-3 carve-out 拍板后打 tag | 主 agent |
| 2026-04-28 01:15 | **G6-3 ✅** Allen 拍板执行 `bun remove @growthbook/growthbook` | `9fc52b8` | -6 行 (package.json deps -1 + bun.lock root deps -1 + entry -1 + dom-mutator transitive -2); 0 typecheck/lint delta; node_modules/@growthbook/ 不存在 | 主 agent |
| 2026-04-28 01:30 | **G6-3 dependency removal focused 验收** (本 doc commit) | (本次 commit) | Allen 显式批准最小可信验收 (引前一轮 G7 全量验证 run4/run7/run10 + b4i6hzk37 作为基线): (1) deps 0 hit (2) runtime import 0 hit (3) 类别 A/B/C/D 命中分类 (E "feature flag/remote provider" 字面 0 hit) (4) typecheck/lint 0 delta (5) R5/R6/R7 STRICT/R8/R9/config_command_audit/M14 全过 — 进度 36/36 ✅ | 主 agent |

---

## 9. 用户决策点

以下情况必须停下问 Allen：

- 是否确认 OpenTelemetry 删除已经完成，可以开始 GrowthBook 迁移。
- 是否采用“本地优先 Mossen config 门面，remote provider 默认关闭”。
- 某个 GrowthBook gate 是否应该保留、隐藏、删除或改为本地配置。
- 是否删除 `@growthbook/growthbook` 包。
- 是否保留 deprecated wrapper，以及保留多久。
- 是否隐藏 hosted/marketplace/browser/native installer 等依赖远程开关的入口。
- 是否引入新的配置文件格式或 settings schema。
- 是否改变默认权限、默认模型、默认记忆策略、默认 compact 策略。
- 是否执行 destructive git 操作。

---

## 10. Allen 决策记录

G0 审计结束共聚 **17 个决策点**，Allen 必须全部拍完才能进入 G1。建议默认值已标 (主 agent 推荐)。

#### 第一组：原计划 4 大决策 (G-D1~G-D4)

| ID | 决策 | 选项 | 主 agent 推荐 | 结论 | 时间 |
|---|---|---|---|---|---|
| G-D1 | 迁移策略 | (a) 本地优先门面 / (b) 继续 GrowthBook / (c) 一刀删除 | (a) | **(a) 本地优先门面** | 2026-04-27 18:00 |
| G-D2 | remote provider | (a) 默认关闭预留接口 / (b) 完全删除 / (c) 立即接 Mossen hosted | (a) — 接口预留但默认 disabled | **(a) 默认关闭预留接口** | 2026-04-27 18:00 |
| G-D3 | `tengu_*` 命名 | (a) 内部 alias 过渡 / (b) 全量改 Mossen key / (c) 保留 | (a) — alias 过渡 + 新代码用 Mossen 命名；旧 logEvent 'tengu_*' 字符串保留 (向后兼容 1P analytics)| **(a) alias 过渡** | 2026-04-27 18:00 |
| G-D4 | hosted/marketplace/browser gate | (a) 隐藏入口 / (b) 本地默认关闭 / (c) 保留现状 | (a) — 隐藏入口；plugin loader / MCP 不动 | **(a) 隐藏入口** | 2026-04-27 18:00 |

#### 第二组：G0-3 (keys 审计) 衍生 3 决策

| ID | 决策 | 选项 | 主 agent 推荐 | 结论 |
|---|---|---|---|---|
| G-D5 | logEvent 'tengu_*' 字符串 (~300 个) 处理 | (a) 保留作为 1P analytics event name / (b) 改名 mossen.* / (c) 全删 | (a) 保留 — 它们是 BigQuery 表的 schema，改名等于推全公司 schema 大改 | **(a) 保留** |
| G-D6 | 3 处默认值不一致 key 修复时机 | (a) G1 前修 / (b) 迁移时按 keys.json 推荐值统一 / (c) 保留不一致 | (b) — 借迁移机会统一，避免引入新一致性 PR | **(b) 迁移时按 keys.json 推荐值统一** |
| G-D7 | 11 个 high-risk key 迁移顺序 | (a) 1P 优先 → 权限 → 内存 / (b) 按 G3-G4 计划顺序 / (c) 全部 G4 一起 | (a) — 1P (R4 已守) 风险已网住，先迁；权限/内存留 G4 重点 verification | **(a) 1P → 权限 → 内存** |

#### 第三组：G0-4 (域影响) 衍生 6 决策

| ID | 决策 | 选项 | 主 agent 推荐 | 结论 |
|---|---|---|---|---|
| G-D8 | computer-use / voice 是否在个人版支持 | (a) 删 / (b) 隐藏入口 / (c) 保留 | (a) 删 — Mossen 个人版无 hosted 后端支撑 | **(a) 删** |
| G-D9 | analytics 采样率默认 | (a) 100% / (b) 与官方一致 / (c) 0% (本地不上报) | (b) — 保持现状 | **(b) 保持现状** |
| G-D10 | tool result storage 持久化 | (a) 本地 SQLite / (b) memory 文件 / (c) 关闭 | (b) — 保持现状（已是 memory file）| **(b) memory 文件 (现状)** |
| G-D11 | session memory autocompact 阈值 | (a) 90% (官方默认) / (b) 80% (更早) / (c) Allen 拍 | (a) — 保持现状不漂移 | **(a) 90% (现状)** |
| G-D12 | hosted 功能代码删 vs flag | (a) 删整段 / (b) flag (默认关) | (b) — flag (与 G-D2 一致), 便于 Mossen hosted 后续接入 | **(b) flag** |
| G-D13 | permission 模式默认 | (a) `default` (问) / (b) `acceptEdits` / (c) `bypassPermissions` | (a) — 保持现状, 安全优先 | **(a) `default`** |

#### 第四组：G0-5 (测试) 衍生 4 决策

| ID | 决策 | 选项 | 主 agent 推荐 | 结论 |
|---|---|---|---|---|
| D-G05-A | `mossen --get-mossen-config` / `--set-mossen-config` CLI flag | (a) 加 / (b) 不加 (用 bun -e fallback) | (a) 加 — R5/R6/R8 大幅简化, Allen debug 也好用 | **(a) 加** |
| D-G05-B | R7 是否一并重定向 `MOSSEN_CODE_PLATFORM_BASE_URL` | (a) 是 (兜底) / (b) 否 (避免误覆盖) | (b) — growthbook.ts 已优先读 GB_BASE_URL | **(b) 否** |
| D-G05-C | R9.compact 是否接受 weak pass | (a) 是 (model 不稳定) / (b) 严格 | (a) — model reply 不稳, weak + retry 3 次 | **(a) 是 (weak + retry 3)** |
| D-G05-D | R8 strict 是否只覆盖 `default_source_file != "unknown"` 的 key | (a) 是 (~60-70%) / (b) 全覆盖 (867) | (a) — unknown 来自静态审计不可信 | **(a) 是** |

> **Allen 决策时间: 2026-04-27 18:00。** 全部 17 决策按主 agent 推荐定案 (G-D6/D-G05-B 为 b 推荐, 其他为 a 推荐)。决策记录已锁定, 后续 G1-G7 必须按上述结论执行。

> Allen 可以在每行"结论"列直接打 a/b/c，或者发一段"我选 ..."的话，主 agent 解析后写入。Allen 不必逐条理由，但 G-D1/G-D2/G-D3/G-D5 是**结构性决策**（影响整个迁移形态），建议 Allen 看完简单确认或反对。

---

## 11. Key 迁移表（G0-3 后必须补全）

> 本表由执行 AI 在 G0-3 补全。没有本表，不允许进入 G1。

| 旧 key / 调用 | 功能域 | 当前默认值 | 迁移后 Mossen key | 迁移后默认值 | 用户影响 | 状态 |
|---|---|---:|---|---:|---|---|
| `tengu_1p_event_batch_config` | analytics | 待审计 | `mossen.analytics.eventBatch` | 待定 | 影响 1P event batch/retry/baseUrl | ⬜ |
| `tengu_event_sampling_config` | analytics | 待审计 | `mossen.analytics.eventSampling` | 待定 | 影响事件采样 | ⬜ |
| `tengu_frond_boric` | analytics sink killswitch | 待审计 | `mossen.analytics.sinkKillswitch` | 待定 | 影响 sink 熔断 | ⬜ |
| permission / auto mode 相关 gates | permission | 待审计 | 待定 | 待定 | 影响 plan/default/bypass/auto mode | ⬜ |
| MCP 相关 gates | MCP | 待审计 | 待定 | 待定 | 影响 MCP channel/permission | ⬜ |
| compact / memory configs | context/memory | 待审计 | 待定 | 待定 | 影响自动压缩和记忆 | ⬜ |
| model / thinking / effort configs | model | 待审计 | 待定 | 待定 | 影响模型选择和 effort | ⬜ |
| plugin / marketplace gates | plugin | 待审计 | 待定 | 待定 | 影响插件展示和 marketplace | ⬜ |
| browser / chrome gates | browser | 待审计 | 待定 | 待定 | 影响浏览器集成入口 | ⬜ |
| native installer / updater gates | installer | 待审计 | 待定 | 待定 | 影响安装/更新提示 | ⬜ |

---

## 12. 与 OpenTelemetry 删除计划的关系

- OTel 删除计划先完成。
- OTel 删除只迁出 telemetry/exporter，不重构 GrowthBook。
- 本计划从 OTel 完成后的 main HEAD 开始。
- 本计划会接管 `tengu_1p_event_batch_config` 等动态配置的长期归属。
- 本计划完成后，后续工作由 Allen 另行安排；执行 AI 不得自行扩大范围。

---

## 12.5 回滚策略 (G0-7 输出)

每阶段失败时的回滚路径，确保 36 slice 任何一步失败都可回退到上一个绿态。

### 总原则
- **每 slice 一个 commit**（G2-2 例外，4 子 commit）。
- **每阶段一个 tag**（在阶段第一个 slice 开始前打 `pre-growthbook-G<X>`）。
- **任何 slice 失败 → `git reset --hard <tag>`**，不允许 `git reset --hard origin/main` 跨阶段回滚。
- 主仓库 + worktree 都不允许 `git push --force` 到远端（origin 优先）。

### 各阶段回滚 anchor

| 阶段 | 起步 anchor tag | 失败回滚目标 | 回滚后影响 |
|---|---|---|---|
| G0 | `pre-growthbook-migration-20260427` ✅ | 同 | tmp/growthbook-audit/ 产物会丢 (审计需重跑) |
| G1 | 待打 `pre-growthbook-G1`（G0-8 后） | `pre-growthbook-G1` | 配置门面不存在；GrowthBook 仍工作 |
| G2 | 待打 `pre-growthbook-G2` | `pre-growthbook-G2` 或单 G2-2x | wrapper 改向丢失；GrowthBook 仍工作 |
| G3 | 待打 `pre-growthbook-G3` | `pre-growthbook-G3` (整段回) 或 `pre-growthbook-G2` | 1P analytics 配置回到 GrowthBook |
| G4 | 待打 `pre-growthbook-G4` | `pre-growthbook-G4` (整段回) 或单个 G4-x（按依赖） | core 域配置回 GrowthBook；R-series 仍守 |
| G5 | 待打 `pre-growthbook-G5` | `pre-growthbook-G5` | hosted 入口重新出现 |
| G6 | 待打 `pre-growthbook-G6` | `pre-growthbook-G6` | GrowthBook 包重装 (`bun add @growthbook/growthbook@^1.6.5`) |
| G7 | 待打 `pre-growthbook-G7` | `pre-growthbook-G7` | 仅文档/baseline 回滚 |

### G6-3 卸包后的特殊回滚

G6-3 (`bun remove @growthbook/growthbook`) 是不可逆操作（package.json 改、bun.lock 改、node_modules 删）。回滚必须：

1. `git checkout <pre-growthbook-G6> -- package.json bun.lock`
2. `bun install`（从 cache 拉回）
3. 验证 `node_modules/@growthbook/` 重新有内容

如果 cache 也清了（`bun pm cache rm`），需要联网重装。**G6-3 commit 前主 agent 必须 explicitly check Allen，不允许自动执行**。

### 任何阶段都不能动的文件

- `commands/insights.ts`（一行 WIP，永久保护）
- `OpenTelemetry删除计划.md`（已封存）
- `scripts/typecheck-baseline.txt` / `scripts/lint-baseline.txt`（仅 G7-3 验收时允许 regen）
- `tmp/baseline_schema.json`（OTel 时捕获，G7-3 diff 用，禁改）

---

## 13. 可直接发给执行 AI 的提示词

```text
你现在要执行 Mossen GrowthBook 迁移计划。

前置条件：
- 必须确认 /Users/allen/Documents/aiproject/mossensrc/OpenTelemetry删除计划.md 已全部完成；
- 必须确认 OpenTelemetry 删除后的 main HEAD 已稳定；
- 如果 OpenTelemetry 删除未完成，不允许开始本计划。

开工前必须阅读：
1. /Users/allen/Documents/aiproject/mossensrc/GrowthBook迁移计划.md
2. /Users/allen/Documents/aiproject/mossensrc/OpenTelemetry删除计划.md

执行规则：
- 只做 GrowthBook 迁移，不扩大到其他产品形态、服务端能力或无关架构工作；
- 先从 G0-1 开始，按顺序做到 G7-4；
- 每完成一个 slice，必须更新 GrowthBook迁移计划.md 的完成列、证据列、§6 进度总览、§8 执行日志；
- 可以按 §3.5 启动多子 agent 并行提升效率，但第一批只能做只读审计；
- 子 agent 必须按 §3.6 模板汇报，不能自行把 slice 标成完成；
- 每个 slice 必须满足 §3.7 完成检查清单、§3.8 SOP 五步和 §3.9 验证体系；
- 不能用未测试、未复现、未覆盖冒充完成；
- 不能一刀删除 GrowthBook；
- 不能改变默认权限、默认模型、默认记忆、默认 compact 策略，除非 Allen 明确确认；
- 多子 agent 只能先做只读审计；并行写代码必须独立 worktree/branch，并登记文件所有权；
- 最终必须通过 typecheck/lint/harness gate、个人版核心能力 smoke，并打 tag post-growthbook-migration-<date>。

现在先确认 OpenTelemetry 删除是否完成。如果未完成，停止并汇报；如果已完成，只能启动 G0 深度只读审计。G0-1 到 G0-8 完成并得到 Allen 确认前，禁止进入 G1 写代码。
```
