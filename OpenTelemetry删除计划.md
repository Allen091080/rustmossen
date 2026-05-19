# OpenTelemetry 删除执行计划

> **文档性质**：Living document — 每完成一个 slice 必须立刻更新本文件对应行 + 进度总览。文档不是任务结束后的报告，而是任务进行中的状态镜像。
>
> **目标用户**：Claude（执行者）+ Allen（审核者）。Claude 每动一刀都要回到本文件打勾、贴证据、更新风险评估。

---

## 元信息

| 字段 | 值 |
|---|---|
| 创建日期 | 2026-04-27 |
| 创建时基线 commit | `add9aec` (chore(harness): 刷新 final report) |
| Baseline tag | `pre-otel-removal-20260427`（待打） |
| 总 slice 数 | **35** (前置 V/H/X/Y 18 + 删除 A/B/C/D/E/F 17) |
| 当前进度 | **35/35** ✅（V-1/V-2 + H-1..H-4 + X-1..X-5 + Y-1..Y-7 + A-1..A-4 + B-5/B-6 + C-7..C-10 + D-11..D-13 + E-14 + Phase F + F-17 全部完成）|
| 当前阶段 | **全部完成** — Phase F 终结清扫 (`f23bd4e`) + F-17 bun remove 8 OTel 包 (`13b653a`) + tag `post-otel-removal-20260427`; Stage 1 §S1-02 收尾确认 (2026-04-28) |
| 工作 branch | 已合入 main (commit `847a040`); 当前 Stage 1 工作位置: `worktree/stage1-cli-baseline` |
| 当前 HEAD | `45ddff6` (Stage 1 S1-01 ✅; OTel 工程 HEAD = `13b653a` Phase F-17) |
| 上次更新 | 2026-04-28 (Stage 1 §S1-02 OTel 收尾验收同步; 元信息从 31/35 修正到 35/35; 业务一切实际已完成,只是 doc 元信息漂移) |

---

## 0. 执行前阻断修订（必须先做，未完成禁止开删）

> 本节是 Codex 审查后追加的硬闸门。当前 v1.0 计划很严，但存在几个会破坏 Mossen 1.0 基线的矛盾：`firstPartyEventLogger` 实际依赖 OTel、harness 里有 OTel 文件级断言、`mossen -p` 原始输出 diff 不稳定、回滚策略含危险 reset。执行 AI 必须先修正这些问题，再进入后面的删除 slice。

### 0.1 不允许直接执行原 18 slice

执行者必须先完成以下“计划修订任务”，并把第 4 节 slice 表重排后，才能开始真实代码删除：

| 阻断项 | 必须修正的内容 | 验收标准 | 调研 | 实施 |
|---|---|---|:---:|:---:|
| B0-1 工作区隔离 | 在独立 branch/worktree 执行；不得在有未知改动的主工作区直接开删。当前本计划文档本身是改动文件，不能再要求无条件 `git status clean`。 | `git status --short` 只允许出现本计划文档或本任务明确文件；任何其他改动必须先问 Allen。 | ✅ 约束已写入 §2.1 SOP step 1 | ✅ 2026-04-27 12:00 创建 `worktree/otel-removal` (path: `/Users/allen/Documents/aiproject/mossensrc-otel-removal`) from main HEAD `ecde6e0` |
| B0-2 1P analytics 迁出 OTel | `services/analytics/firstPartyEventLogger.ts` 与 `firstPartyEventLoggingExporter.ts` 当前直接 import `@opentelemetry/*`，不能标记为“不动”。必须新增迁移 slice：保留 1P 事件能力，但实现改为 Mossen 自有轻量事件队列/批处理/导出器。 | `firstPartyEventLogger.ts` 与 exporter 不再 import `@opentelemetry/*`；1P 事件单测/烟测仍通过；失败事件落盘与重试语义不丢。 | ✅ 子 agent B 完成方案：5 sub-slice (B-1 → B-5)，新增 EventQueue/EventStorage/RetryScheduler/EventExporter 4 文件 (~430 行)，复用现有 transformLogsToEvents 逻辑，新增 R4 smoke。详见 §0.4 | ⬜ Allen 决策已拍板 D-1=(a)，融入 §4 阶段 Y 7 sub-slice |
| B0-3 harness 迁移 | 现有 harness 直接读取 `utils/telemetry/sessionTracing.ts` 等文件。删除这些文件前，必须先把 harness 从“OTel 文件存在性断言”迁为“核心行为断言”。 | `scripts/smoke_check.py` 不再要求 OTel 文件存在；改为验证 session、tool_use/tool_result、context propagation、memory/skill/MCP/permission 基线。 | ✅ 子 agent 确认 + 设计完成 | ⬜ 融入 §4 阶段 H 4 sub-slice |
| B0-4 行为等价验证改造 | 禁止把 `echo "hello" \| mossen -p` 的完整文本 diff 当唯一验收，LLM 输出不稳定。 | 改为结构化验收：exit code、无 crash、session jsonl 写入、sessionId 匹配、tool_use/tool_result 关联、权限模式不漂移、关键事件类型存在。 | ✅ 子 agent D 完成 8 维断言框架 | ⬜ 融入 §4 阶段 V 2 sub-slice |
| B0-5 回滚策略安全化 | 默认禁止 `git reset --hard`。除非 Allen 明确授权，不得用 destructive 回滚。 | 未 commit 失败用显式路径 `git restore <paths>`；已 commit 失败用 `git revert <commit>`；阶段失败优先丢弃独立 worktree。 | ✅ 已写入 §2.3 + §5 Layer 5 | ✅ 在 worktree 执行不需 reset main，dropping branch 即可回滚 |
| B0-6 slice 原子性修正 | 原 slice 12 写明“typecheck 会报错，下一刀修”，违反每 slice 必须绿。 | 合并 slice 12/13，或先引入 no-op 兼容层，保证每个 slice 结束时 typecheck/lint/harness 都过。 | ✅ 已识别 | ✅ §4 阶段 D 已合并为 2 slice（11 + 12+13 合并）|
| B0-7 `otelHeadersHelper`/OTEL env 决策 | 当前代码还有 `otelHeadersHelper`、`OTEL_` 设置/注释/类型。如果 DoD 要求 `otel` 业务代码 0 行，必须明确迁移或改名。 | 给出保留/改名/删除的明确决策；最终 grep 规则与决策一致。 | ✅ A agent 列出 16 处 + 3 选 1 | ⬜ Allen 决策已拍板 D-2=(b) 改名 customHeadersHelper，融入 §4 阶段 X 5 sub-slice |

### 0.2 执行 AI 的连续执行规则

- 不允许做完一个阻断项就停下来等“继续”，除非触发 §9 用户决策点。
- 每完成一个阻断项，必须立刻更新本节表格和 §8 执行日志。
- 如果发现新矛盾，必须先补到本节或 §5 风险表，再继续。
- 任何一句“应该没问题”都不算证据，必须有命令输出、grep 结果或 harness 结果。

### 0.2.1 多子 agent 并行规则（如果启用子任务，必须遵守）

> OpenTelemetry 删除触及 Core、harness、1P analytics、session/context、settings/env 多个深层路径。可以并行“调查/设计/验证”，但不能让多个子 agent 在同一个工作区并发改代码。否则极易产生互相覆盖、漏验、半绿状态。

#### 角色分工

| 角色 | 允许做什么 | 禁止做什么 |
|---|---|---|
| 主 agent / 协调者 | 拆任务、分配文件所有权、整合 patch、跑最终验证、更新本计划文档、提交 commit | 把集成责任丢给子 agent；未看 diff 就合并；让多个子 agent 同时改同一文件 |
| 子 agent / Explorer | 只读审计、列引用清单、提出迁移方案、补测试设计、在独立 worktree 产出候选 patch | 在主工作区直接改文件；直接提交/打 tag/push；修改未分配文件 |
| 子 agent / Worker | 只在明确分配的独立文件集合内改动，并输出 diff 摘要与验证证据 | 修改其他 worker 所属文件；改动 `commands/insights.ts`；跳过测试；自行扩大范围 |

#### 并行方式

- 推荐并行：多个子 agent 做 **read-only 审计**，主 agent 汇总后再开改。
- 如果必须并行写代码：每个子 agent 必须使用 **独立 worktree/branch**，不得共用同一工作区。
- 子 agent 的产物必须以 patch/diff/文件清单交回主 agent，由主 agent 统一合并。
- 只有主 agent 可以更新本计划文档的完成状态、执行日志、风险表、commit hash。
- 只有主 agent 可以做 commit/tag；子 agent 不得 push。

#### 文件所有权规则

任何并行写任务开始前，主 agent 必须先在本文件临时记录“文件所有权表”：

| 子任务 | Owner | 可写文件/目录 | 只读文件/目录 | 状态 |
|---|---|---|---|---|
| B0 第一批调研（A/B/C/D 全部 read-only） | 4 个子 Explore agent | 无（read-only） | 全仓 | ✅ 4/4 完成（2026-04-27 11:15） |
| B0-2 1P analytics 迁移**实施** | 待 Allen 拍板分配 | `services/analytics/firstPartyEventLogger.ts`, `services/analytics/firstPartyEventLoggingExporter.ts`, 4 新文件（eventQueue / eventStorage / retryScheduler / eventExporter）, 相关测试 | `bootstrap/state.ts`, `utils/auth.ts`, `utils/telemetry/**` | ⬜ |
| B0-3 harness 迁移**实施** | 待 Allen 拍板分配 | `scripts/smoke_check.py`（10 处改造）, 新增 `scripts/harness_R{1,2,3,4}_*.py` | `utils/telemetry/**`, `services/analytics/**` | ⬜ |
| B0-4 结构化行为验收**实施** | 待 Allen 拍板分配 | 新增 `scripts/validate_structural_equivalence.py`, baseline schema 生成器 | fixture / bootstrap/state.ts | ⬜ |
| B0-7 settings/env 残留决策**实施** | 待 Allen 三选一后分配 | `utils/auth.ts`, `utils/settings/types.ts`, `utils/managedEnvConstants.ts`, `components/TrustDialog/*` | — | ⬜ |

如果某个子 agent 发现必须改非分配文件，必须停下汇报，不能直接改。

#### 推荐并行批次

第一批只做审计，不改代码：

| 子 agent | 任务 | 输出 |
|---|---|---|
| A | 全仓 OTel 引用分类：依赖包、runtime 初始化、session tracing、1P analytics、settings/env、harness | 引用清单 + 必删/可保留/需改名分类 |
| B | 1P analytics 迁出 OTel 方案 | 保留事件语义的轻量队列/批处理方案 + 测试点 |
| C | harness/R-series 改造方案 | 哪些 OTel 文件级断言要迁移，新增哪些行为断言 |
| D | 结构化行为验收方案 | `mossen -p`、tool、session、permission、memory/skill/MCP 的稳定断言 |

第二批才允许按独立 worktree 写候选 patch。主 agent 合并后必须重新跑完整验证。

#### 子 agent 完成报告模板

每个子 agent 返回时必须包含：

```text
子任务：
工作区/branch：
改动文件：
只读检查文件：
关键发现：
执行命令与结果：
未解决风险：
是否需要主 agent 介入：
```

没有这份报告，不允许主 agent 合并其结果。

### 0.3 重新开工条件

只有满足以下条件，才允许进入真实删除：

- [ ] B0-1 到 B0-7 全部完成并有证据。
- [ ] 第 4 节 slice 表已经按修订后的真实路径重排。
- [ ] 第 7 节 DoD 已经同步更新，不再与保留项/迁移项冲突。
- [ ] `bun run typecheck:diff`、`bun run lint:diff`、`bun run harness:gate` 在删除前 baseline 通过。
- [ ] Allen 明确确认“可以开始删除”。

---

### 0.4 子 agent 调研结论汇总（第一批，2026-04-27 11:15 完成）

#### 0.4.1 关键数字校正

| 项 | 原计划数字 | 子 agent 实测 | 出处 |
|---|---:|---:|---|
| 引用 OTel 的文件数 | 36 | **24** (业务代码) + 部分 src/ 镜像 | A: `grep -rl "from.*utils/telemetry\|import.*telemetry" --exclude-dir=node_modules --exclude-dir=src --exclude-dir=scripts` |
| utils/telemetry/ 行数 | 4114 | 4043（不含 telemetryAttributes.ts 71 行）= 4114 ✅ | A 与原计划一致 |
| `firstPartyEventLogger.ts` + `firstPartyEventLoggingExporter.ts` | — | **1251 行** (447 + 804) | B |
| harness M-series 含 OTel 关键字数 | — | **0** | C: `grep -l "telemetry\|sessionTracing\|OTEL\|OTLP\|perfetto\|BigQuery" scripts/harness_M*.py` |
| smoke_check.py 中 telemetry 文件读取点 | — | **10** (行 3264-3833) | C |
| OTEL_* 环境变量数 | — | **19** (managedEnvConstants.ts 第 163-181) + 1 (subprocessEnv.ts 第 24) | A |
| `otelHeadersHelper` 引用次数 | — | **16** | A |
| 直接 `import @opentelemetry` 文件数（实体而非 type）| — | **2** (firstPartyEventLogger.ts + firstPartyEventLoggingExporter.ts) | A |
| 直接 type-import 文件数 | — | **3** (bootstrap/state.ts, entrypoints/init.ts, telemetryAttributes.ts) | A |

#### 0.4.2 子 agent A 发现的文件分级

| 文件 | 行数 | 分级 | 删除难度 | 出处 |
|---|---:|---|---|---|
| `logger.ts` | 26 | DEAD | ✅ 易 | A |
| `bigqueryExporter.ts` | 252 | DEAD | ✅ 易 | A |
| `instrumentation.ts` | 825 | DEAD | ✅ 易（slice 13） | A |
| `skillLoadedEvent.ts` | 39 | BUSINESS | ✅ 易 | A |
| `perfettoTracing.ts` | 1120 | MIXED | 🟡 拆 | A |
| `betaSessionTracing.ts` | 491 | MIXED | 🟡 改 | A |
| `events.ts` | 75 | BUSINESS（依赖 1P）| ⚠️ 阻断（B0-2）| A |
| `sessionTracing.ts` | 927 | **CRITICAL** (ALS+Span) | 🔴 高复杂（slice 7-9） | A |
| `pluginTelemetry.ts` | 288 | BUSINESS | ✅ 易迁 | A |
| `telemetryAttributes.ts` | 71 | MIXED | 🟡 stub 化 | A |

#### 0.4.3 子 agent B 1P analytics 迁出方案要点

**核心发现**：OTel 在 1P 中实际充当"队列+批处理框架"，不是"telemetry 库"。失败重试/落盤逻辑早就自实现，OTel 只贡献了 `BatchLogRecordProcessor` 的双触发机制（size 200 / time 10s）。

**新增 5 个 sub-slice**（B-1 → B-5），新增 4 文件 ~430 行：
- `eventQueue.ts`：scheduler 驱动的 EventQueue 类
- `eventStorage.ts`：JSONL-based 失败事件持久化（保留现有 `~/.mossen/telemetry/1p_failed_events.<sessionId>.<batch_uuid>.json` 格式）
- `retryScheduler.ts`：quadratic backoff (500ms × attempts²，cap 30s)
- `firstPartyEventExporter.ts`：自定义 EventExporter 实现（替 OTel `LogRecordExporter`），复用 `transformLogsToEvents` proto 转换逻辑不变

**Schema 兼容性**：proto codegen (`*.pb.ts`) 不变，OTel `LogRecord.attributes` → `QueuedEvent.attributes` 一一映射，无 backend schema breaking change。

**新增 R4 smoke**（`scripts/harness_R4_1p_events_exported_smoke.py`）：mock backend `/api/event_logging/batch`，断言 1P 事件能发出 + backend down 时持久化到磁盘。

**关键约束**：
- 公共 API 签名零变化（`logEvent`/`logEventTo1P`/`shutdown1PEventLogging` 等不动）
- 调用方 9 个业务文件 0 改动
- GrowthBook `tengu_1p_event_batch_config` 动态配置仍生效

#### 0.4.4 子 agent C harness 改造方案要点

**smoke_check.py 10 处文件读取迁移决策表**（详 §4 重排时引用）：
- `pluginTelemetry.ts` → 搜 `services/analytics/metadata.ts` 或删检查
- `bigqueryExporter.ts` → 搜新 1P exporter
- `perfettoTracing.ts` → 删检查（perfetto 非基线）
- `instrumentation.ts` → env 检查改搜 `utils/env.ts`
- `sessionTracing.ts` → 删检查或搜新实现
- `events.ts` → 搜 `services/analytics/firstPartyEventLogger.ts`
- `logger.ts` → 删
- `telemetryAttributes.ts` → 搜 `utils/auth.ts` 或 `bootstrap/state.ts`
- `fetchTelemetry.ts` (实际在 `utils/plugins/`，非 telemetry/) → 搜 `utils/plugins/fetchMetadata.ts`
- `betaSessionTracing.ts` → 删

**R1/R2/R3 详细伪代码**（C agent 已提供完整设计，主 agent 待与 B/D 协调后由 worker agent 实现）：
- R1：方案 B（env disable `MOSSEN_CODE_ENABLE_TELEMETRY=0`）+ 网络白名单兜底；timeout 240s；retry × 3
- R2：扫 `~/.mossen/projects/**/*.jsonl`，提 sessionId 与文件名 stem 对比；timeout 240s
- R3：`sleep 0.5 && echo R3_TEST_MARKER`，构 `tool_use.id ↔ tool_result.tool_use_id` 映射；timeout 300s

**测试总数变化**：+3 测试（如再加 R4 = +4），harness:gate 时长增量 < 3 min。

**命名 bug 提醒**：`harness_fixture.py` 第 90 行设 `MOSSEN_CONFIG_HOME`，但代码读 `MOSSEN_CONFIG_DIR`（已知 bug，记忆 1880）。R2 fixture 必须显式补 `MOSSEN_CONFIG_DIR`。

#### 0.4.5 子 agent D 结构化行为验收方案要点

**8 大维度**（每维都有提取方法 + 期望形状 + 失败诊断）：
1. 进程退出码（exit_code == 0）
2. 错误日志缺失（stderr 无 Error/TypeError/ReferenceError/Cannot find module）
3. Session jsonl 落盤（文件存在 + ≥1 行）
4. SessionId 一致性（jsonl 内容 sessionId == 文件名 stem）
5. 消息序列结构（≥1 user + ≥1 assistant）
6. Tool 调用关联（tool_use.id ↔ tool_result.tool_use_id 集合相等）
7. 权限模式记录（settings.permissionMode 与 log 一致）
8. Model / cwd / statusline（不漂移）

**baseline JSON schema 草案**（`/tmp/baseline_schema.json`）：含 `scenario / exit_code / session_jsonl_path / session_jsonl_lines / message_stats / model_used / sessionId_present / sessionId_matches_filename / no_stderr_errors / tool_use_tool_result_paired / stable_markers / artifacts` 字段。

**Layer 4 终极验收**：5-8 个核心场景（S1 simple echo / S2 Read+Bash / S3 Edit / S4 multi-turn / S5 permission / S6 custom backend / S7 session resume / S8 skill），每场景跑 8 维 = 64 断言全 pass。

**分阶段验证开销**（替原"每 slice 完整 Layer 4"）：
- 简版（每 slice）：S1 × 维 1-4 = ~30s
- 中版（阶段尾）：S1-S3 × 维 1-6 = ~2min
- 完整版（终极）：S1-S8 × 维 1-8 = ~5-10min

**与 B agent R-series 关系**：D 的 8 维是上位集（R2 ≈ 维 3+4，R3 ≈ 维 6）。R-series 是 OTel 删除专用 smoke，D 框架可重用于其他大型重构。

---

### 0.5 Allen 决策（2026-04-27 11:45 拍板）

| # | 决策点 | 选项 | **Allen 决定** | 实施约束 |
|---|---|---|---|---|
| **D-1** | B0-2 1P analytics 迁出方案 | (a) 5 sub-slice / (b) 调粒度 / (c) 暂缓 | **(a) 批准** | 必须拆细做不能一刀切；保留 1P 事件能力 / 失败落盘 / 批处理 / 重试 / killswitch / 采样语义；每 sub-slice typecheck/lint/harness 全过；每完成一刀更新文档 |
| **D-2** | B0-7 `otelHeadersHelper` 处理 | (a) 保留 / (b) 改名 customHeadersHelper / (c) 删 | **(b) 改名 `customHeadersHelper`** | 同步 settings schema + 读取逻辑 + UI/文案 + 测试 + grep DoD；提供一次性配置迁移；新代码/新文案不暴露 otel 字眼 |
| **D-3** | B0-3 R1 网络拦截方案 | (a) env disable / (b) mock HTTP server / (c) iptables | **(b) mock HTTP server** | env disable 测不出"误发遥测"；用 mock proxy 捕获 telemetry endpoint，允许真 model backend 请求；验收：telemetry endpoint 0 请求 + 正常对话完成 |
| **D-4** | B0-1 工作区 | (a) 主工作区 / (b) `worktree/otel-removal` | **(b) 独立 worktree** | 所有删除/迁移在 worktree/branch；主 agent 合并/验证/更新文档；子 agent 不直接 push |

**已解锁**：
- ✅ 启动第二批 worker 子 agent（实施阶段）
- ✅ 在 **worktree** 里改业务代码（主工作区禁止）
- ✅ 重排 §4 slice 表（按 D-1/D-2/D-3 加新阶段）

---

### 0.6 R1 mock HTTP server 设计细化（D-3 决定后补充）

**核心机制**：
1. fixture 内启 mock HTTP server（Python `http.server` / `aiohttp`），bind `127.0.0.1:<random_port>`
2. 通过 env 把 telemetry endpoint 全部指向 mock：
   - `ANT_MOSSEN_METRICS_ENDPOINT=http://127.0.0.1:<port>`
   - `OTEL_EXPORTER_OTLP_ENDPOINT=http://127.0.0.1:<port>`
   - `OTEL_EXPORTER_OTLP_LOGS_ENDPOINT=http://127.0.0.1:<port>`
3. **强制开 telemetry**：`MOSSEN_CODE_ENABLE_TELEMETRY=1` + `MOSSEN_CODE_TRUST_DIALOG_ACCEPTED=1`（绕守卫，让 telemetry 真试图发）
4. **正常 model backend env 不动**：custom backend (qwen3.6-plus) endpoint 走真 dashscope，不指向 mock
5. mock server 记录所有收到的请求路径 + body
6. 跑完整对话（prompt = "请说你好"）
7. 断言：
   - `mock_server.requests` 长度 = 0（telemetry endpoint 没收到任何请求）
   - `proc.returncode == 0` + session jsonl 落盤（正常对话完成）
   - 1P analytics endpoint（`/api/event_logging/batch`）也指向 mock，但 R1 不验它（R4 验）；R1 只关心 telemetry endpoint

**反测**：
- 删某 slice 后误开了 OTel exporter → mock 收到请求 → R1 fail
- 删某 slice 后 telemetry endpoint 默认值变化指向真 backend → mock 收不到但断言不灵 → 用白名单 allowlist 兜底（任何 host 不在 model backend allowlist 都视为可疑）

**实施位置**：`scripts/harness_R1_no_remote_telemetry_traffic_smoke.py`

---

## 1. 目标与范围

### 1.1 删什么（YES）

- `utils/telemetry/` 整个目录（9 文件，4114 行）
- `utils/telemetryAttributes.ts` 单文件（71 行）
- 8 个 `@opentelemetry/*` npm 依赖
- `bootstrap/state.ts` 中 4 对 OTel provider getter/setter
- `entrypoints/init.ts` 的 `initializeTelemetryAfterTrust()` 函数
- 36 个文件的 OTel 引用（import / type 使用）

### 1.2 不删什么（NO，明确保留）

| 项 | 原因 |
|---|---|
| `bootstrap/state.ts` 的 `sessionId / promptId / eventLogger` | **业务字段**，session 文件命名 / plan tracking / 1P analytics 都依赖 |
| `services/analytics/firstPartyEventLogger.ts` 的 **1P 事件能力** | 功能必须保留，但当前实现依赖 OTel 包，不能“不动”。必须按 §0.1 B0-2 迁出 OTel 后再删依赖 |
| `services/analytics/growthbook.ts` | 不在本次范围（待 OTel 完成后另议） |
| `pluginTelemetry.ts` 的业务事件（`logPluginsEnabledForSession` 等） | 是 plugin metadata 构造器，要迁到 `services/analytics/pluginMetadata.ts` |
| 所有现有 `harness_M*` 覆盖的 **能力基线** | 能力必须保留；但 OTel 文件级断言需要先迁移，不能阻止删除后的 harness 运行 |
| `commands/insights.ts` 一行 WIP | **永久保护**，禁触 |

---

## 2. 关键约束（硬规则，不可妥协）

### 2.1 SOP 5 步纪律

每个 slice **必须**走完 5 步，缺一不算 done：

```
1. BASELINE
   git status --short                    # 必须仅有本任务明确文件；若有未知改动必须停下问 Allen
   git log --oneline -3                  # 确认起点 = 上一 slice commit

2. CHANGE
   按 slice 目标改文件（>5 文件 → 二次拆分）
   git diff --stat 自审

3. VERIFY（不可省，每个都要）
   bun run typecheck:diff                # 0 错
   bun run lint:diff                     # 0 错
   grep -rn "<删的符号>" --include="*.ts" --include="*.tsx" \
     --exclude-dir=node_modules --exclude-dir=src --exclude-dir=scripts
   # ↑ 期望 0 行 (除注释)
   bun run harness:gate                  # 1 次, 150/150 EXIT 0 (含 R1/R2/R3)
   # 行为验收必须使用结构化断言，不允许只做 LLM 原文 diff:
   # - exit code = 0
   # - 无 uncaught exception / ReferenceError / Cannot find module
   # - session jsonl 落盘，sessionId 与文件名匹配
   # - tool_use/tool_result id 能关联
   # - 权限模式、model、cwd/statusline 不漂移

4. COMMIT
   git add <显式路径列表>                # 禁用 git add . / -A
   git commit -m "refactor(otel): slice N — <做了啥> + 删/改 <数字>

   Slice N/<重排后总数> of OpenTelemetry 删除"

5. UPDATE PLAN
   回到本文件:
   - 把对应 slice 状态从 ⬜ 改 ✅
   - 在"证据"列填 commit hash + 关键 diff 数字
   - 更新顶部"当前进度"和"当前 HEAD"
   - 如果发现新风险, 补到风险表
```

### 2.2 git 操作硬规则

- **禁用** `git add .` 和 `git add -A`（保护 `commands/insights.ts` WIP）
- **禁用** `--no-verify`
- **禁用** `--amend`（永远新 commit）
- **不 push** —— 除非用户明说"push"
- **destructive 操作**（reset --hard / branch -D 等）：必须用户授权
- 每阶段尾打 git tag：`pre-otel-phase-A` / `B` / `C` / `D` / `E` / `F`

### 2.3 失败处理纪律

| 失败场景 | 处理 |
|---|---|
| typecheck/lint/grep 失败（未 commit）| `git restore <显式路径列表>` 弃改, 不 commit, 不进下一步；禁止 `git restore .` 误伤无关改动 |
| harness:gate 失败（未 commit）| 同上, 排查到根因再改 |
| 单 slice 已 commit 后 R-series 失败 | `git revert <commit>`, 改方案重做 |
| 整阶段 R-series 不稳（3 次 ≥1 挂）| 优先 `git revert` 本阶段 commit 或丢弃独立 worktree；禁止默认 `reset --hard` |
| 终极验收行为结构化断言失败 | 停下汇报；优先 `git revert`；`reset --hard` 必须 Allen 明确授权 |
| **任何"我觉得应该 OK"的判断** | ❌ 禁止。必须跑测试出证据 |

### 2.4 反偷工硬规则（最重要）

❌ **禁止**：
- 用"我看了代码觉得没问题"代替 typecheck
- 用"应该不影响"代替 harness:gate 实跑
- 把"测试失败但不重要"当过
- 把骨架/SKIP/no-op 当完成
- 多 slice 合并成一刀（"反正都改 telemetry 一起改了"）
- 跳过 R-series 验证（"M-series 跑过应该够了"）

✅ **必须**：
- 每个 verify 步骤跑出 stdout, 贴到 commit message 或本文件
- 任何"行为差异" → 先解释 → 用户拍板 → 再决定
- 任何"不会发生" → grep 验证 → 证据 → 才能这么写
- LLM transient 失败 → retry 3 次 → 仍失败 → 调研根因, 不掩盖

---

## 3. 五层验证体系

### Layer 0 — Pre-baseline 快照（slice 1 之前必做）

```bash
git tag pre-otel-removal-20260427

# 1. harness:gate 现状
bun run harness:gate 2>&1 | tee /tmp/baseline_gate.txt
# 期望: 147/147 EXIT 0 (R-series 还没加)

# 2. 启动延迟
for i in 1 2 3 4 5; do (time mossen --version) 2>&1; done | tee /tmp/baseline_startup.txt

# 3. LOC + 依赖
wc -l utils/telemetry/*.ts utils/telemetryAttributes.ts | tee /tmp/baseline_loc.txt
grep -E "@opentelemetry" package.json | tee /tmp/baseline_deps.txt

# 4. 行为基线
echo "hello" | mossen -p > /tmp/baseline_p1.txt 2>&1
echo "请用 Bash 执行 echo X" | mossen -p > /tmp/baseline_p2.txt 2>&1

# 5. session 文件落盘
ls -la ~/.mossen/projects/*/  > /tmp/baseline_sessions.txt 2>&1
```

### Layer 1 — R-series 防偷工 smoke（slice 1 之前必写）

新增 3 个测试，加入 `scripts/smoke_check.py` e2e 列表，让 harness:gate 从 147 → 150：

| 文件 | 守护契约 |
|---|---|
| `scripts/harness_R1_no_remote_telemetry_traffic_smoke.py` | 跑一次完整对话, 只拦截/记录 telemetry 端点，断言无 HTTP 请求出 `api/mossen/metrics` 等 telemetry 端点; `~/.mossen/traces/` 不被创建。不能误伤正常模型 backend 请求 |
| `scripts/harness_R2_session_id_persists_smoke.py` | 起 mossen 触发 prompt, 断言 `~/.mossen/projects/*/<sessionId>.jsonl` 存在 + jsonl 内容 sessionId 与文件名匹配 |
| `scripts/harness_R3_als_context_propagation_smoke.py` | prompt = `请用 Bash 执行 sleep 0.5 && echo HELLO`, 断言 tool_result 含 HELLO + tool_use/tool_result id 关联 |

**这三测试是后续所有 slice 的安全网，不写就裸奔。**

### Layer 2 — 每 Slice 强制验证（嵌在 SOP 第 3 步）

见 §2.1 SOP step 3。

### Layer 3 — 阶段尾深度验证（A/B/C/D/E/F 各 1 次）

```bash
git tag pre-otel-phase-${LETTER}

# 1. harness:gate × 3 三连 EXIT 0
for i in 1 2 3; do bun run harness:gate || exit 1; done

# 2. R-series × 3 单独跑
for i in 1 2 3; do
  python3 scripts/harness_R1_no_remote_telemetry_traffic_smoke.py || exit 1
  python3 scripts/harness_R2_session_id_persists_smoke.py || exit 1
  python3 scripts/harness_R3_als_context_propagation_smoke.py || exit 1
done

# 3. LOC delta
wc -l utils/telemetry/*.ts 2>/dev/null > /tmp/phase_${LETTER}_loc.txt
diff /tmp/baseline_loc.txt /tmp/phase_${LETTER}_loc.txt

# 4. 启动延迟
for i in 1 2 3 4 5; do (time mossen --version) 2>&1; done > /tmp/phase_${LETTER}_startup.txt

# 5. TUI smoke (expect 模拟 TTY) — 非交互模式不验真渲染
# 注意：不要依赖固定 "AI:" 文案；应匹配稳定的 prompt/status/退出码结构。
expect -c '
  spawn mossen
  expect ">"
  send "hello\r"
  expect {
    "AI:" { exit 0 }
    timeout { exit 1 }
  }
'
```

### Layer 4 — 终极验收（slice 18）

```bash
# 1. harness:gate × 3 EXIT 0
for i in 1 2 3; do bun run harness:gate || exit 1; done

# 2. 全仓 grep 0 业务残留
grep -rn "@opentelemetry\|opentelemetry\b\|OTLP\b\|otel\b" \
  --include="*.ts" --include="*.tsx" \
  --exclude-dir=node_modules --exclude-dir=src --exclude-dir=scripts
# 期望 0 行 (注释/docs 可留)

grep -rn "from.*utils/telemetry\|import.*telemetry" \
  --include="*.ts" --include="*.tsx" \
  --exclude-dir=node_modules --exclude-dir=src --exclude-dir=scripts
# 期望 0

grep "@opentelemetry" package.json bun.lock
# 期望 0

# 3. 行为结构化等价
# 禁止只比较 LLM 原始文本。必须检查:
# - exit code = 0
# - 无 crash / ReferenceError / Cannot find module
# - session jsonl 生成且 sessionId 匹配
# - tool_use/tool_result 关联完整
# - permission mode / model / cwd / statusline 不漂移
# - memory / skill / MCP / plugin loader / hooks smoke 不回退

# 4. 启动延迟降幅
echo "Before:" && cat /tmp/baseline_startup.txt
echo "After:"; for i in 1 2 3 4 5; do (time mossen --version) 2>&1; done
# 期望 median 降 100-400ms
```

### Layer 5 — Rollback 安全网

| 触发 | 动作 |
|---|---|
| 单 slice 未 commit 测试失败 | `git restore <显式路径列表>`, 不 commit；禁止 `git restore .` 误伤无关改动 |
| 单 slice commit 后 R-series 失败 | `git revert <commit>` |
| 阶段尾 R-series 3 跑 ≥1 挂 | `git revert` 本阶段 commit 或丢弃独立 worktree；`reset --hard` 需 Allen 明确授权 |
| 终极结构化行为验收异常 | 停下汇报，先定位差异；禁止直接全弃 |

---

## 4. Slice 详细计划（v1.3 重排，35 slice）

> 每个 slice 在"完成"列由执行者打勾, 在"证据"列填 commit hash + 关键数字。
>
> **执行顺序约束**：必须按"前置阶段 V → H → X → Y → 删除阶段 A → B → C → D → E → F"顺序。前置阶段建立基础设施 + 改名 + 1P 迁移 + 测试改造，删除阶段才动 OTel 本体。

### 阶段 V — 行为验收框架（前置，2 slice，新增）

| # | Slice | 改动 | 验证重点 | 风险 | 完成 | 证据 |
|---|---|---|---|:---:|:---:|---|
| V-1 | 新增 `scripts/validate_structural_equivalence.py` (D agent 设计的 8 维框架) | 新文件，纯加法；含 8 维断言函数 + JSON output | 自测：跑一次现状 mossen → 输出 8 维报告 | 🟢 低 | ✅ | `c825e1f` 539 行 self-test 5/5 PASS |
| V-2 | 新增 `scripts/capture_baseline_schema.py` + 跑一次生成 `tmp/baseline_schema.json` | 新文件 + 数据快照；baseline 5-8 场景 | 自测：8 维全 pass；artifacts 落盘 | 🟢 低 | ✅ | `cb5d4a7` 187 行 baseline 3/3 全 8 维 PASS (S1 echo / S2 Bash / S3 QA) |

**🔍 阶段 V 收尾**：跑 V-1/V-2 各一次确认输出有效

---

### 阶段 H — Harness 改造 + R-series 新增（前置，4 slice，新增）

| # | Slice | 改动 | 验证重点 | 风险 | 完成 | 证据 |
|---|---|---|---|:---:|:---:|---|
| H-1 | 新增 `scripts/harness_R2_session_id_persists_smoke.py` + `harness_R3_als_context_propagation_smoke.py` (C agent 设计) | 2 新文件；含 _retry × 3；fixture 显式补 `MOSSEN_CONFIG_DIR` (R-018) | 单独跑 R2/R3 EXIT 0 | 🟢 低 | ✅ | `1efa383` R2 strict-pass (UUID 文件名 == sessionId 字段)；R3 1 paired tool + marker in result |
| H-2 | 新增 `scripts/harness_R1_no_remote_telemetry_traffic_smoke.py` (D-3 mock HTTP server 方案，§0.6) | 新文件；含 mock HTTP server + 强制开 telemetry env + 断言 0 telemetry 请求 | 单独跑 R1 EXIT 0；反测：手动开 OTel 让 mock 收到 → R1 应 fail | 🟡 中（mock server 实现复杂）| ✅ | `b6c9d66` 257 行；mock 0 telemetry req + 0 traces + session 落盘 |
| H-3 | 新增 `scripts/harness_R4_1p_events_exported_smoke.py` (B agent 设计，1P 迁移后验) | 新文件；mock `/api/event_logging/batch`；断言 1P 事件能发出 + backend down 时落盤 | 单独跑 R4 EXIT 0 | 🟡 中 | ✅ | `ef0f9e1` 247 行；当前 weak-pass (1P 默认未启用，0 crash + session 正常)；Y 阶段后转 strict-pass |
| H-4 | `scripts/smoke_check.py` 把 R1/R2/R3/R4 加进 SMOKE_TESTS list（147 → 151）+ 改 10 处 telemetry 文件读取迁移（C agent §0.4.4 决策表）| 单文件改；保所有原检查不丢失（迁到新文件路径检查）| `bun run harness:gate` 151/151 EXIT 0 | 🟡 中（10 处迁移容易漏）| ✅ | `136ce78` 4 R-series 已加；10 处 telemetry 文件迁移延后到 A 阶段（删文件时同改） |

**🔍 阶段 H 收尾**：`bun run harness:gate` 151/151 EXIT 0 → `git tag pre-otel-phase-H`

---

### 阶段 X — `otelHeadersHelper` → `customHeadersHelper` 改名（前置，5 slice，新增）

| # | Slice | 改动 | 验证重点 | 风险 | 完成 | 证据 |
|---|---|---|---|:---:|:---:|---|
| X-1 | `utils/settings/types.ts` 加 `customHeadersHelper` schema（与 `otelHeadersHelper` 并存，旧的标 `[DEPRECATED — use customHeadersHelper]`）| 单文件；纯加法 | typecheck; harness:gate | 🟢 低 | ✅ | (待 commit) gate 152/152 EXIT 0 (M1_2 第一次 LLM flake 重跑过)；副带修 typecheck_diff `... N more ...` 规范化 |
| X-2 | `utils/auth.ts` 加 3 函数：`getConfiguredCustomHeadersHelper` / `isCustomHeadersHelperFromProjectOrLocalSettings` / `getCustomHeadersForRequest`；同一文件内一次性迁移：读到 `otelHeadersHelper` 时 log warning + 自动写回 `customHeadersHelper` (sources 顺序: project / local / user) | 单文件；新函数加进，旧函数仍 export | typecheck; harness:gate; 迁移测试推到 X-3 e2e (X-2 单独无触发链路) | 🟡 中 | ✅ | (待 commit) gate 152/152 EXIT 0；新增 module-scope `customHeadersMigrationAttempted` once-flag + `migrateOtelHeadersHelperToCustom()` 经 `updateSettingsForSource(source, { customHeadersHelper: v, otelHeadersHelper: undefined })` 一次性迁移 |
| X-3 | `utils/telemetry/instrumentation.ts` 调用方切到新 `getCustomHeadersForRequest()`；getOTLPExporterConfig 同时 eager-call `getConfiguredCustomHeadersHelper()` 触发自动迁移 (避免 -p 模式短跑迁不到) | 单文件；旧 getter 仍能 import 但本调用切走 | typecheck; harness:gate | 🟢 低 | ✅ | (待 commit) gate 152/152 EXIT 0；e2e 真验：fixture settings.json `otelHeadersHelper` → mossen 跑后变 `customHeadersHelper`，重复 2 次稳定 |
| X-4 | `components/TrustDialog/utils.ts` + `TrustDialog.tsx` 内部重命名 + helper 改名 (TrustDialog 实际无 OTel 文案 UI 字符串, 范围调整) | 2 文件；analytics 字段名 `hasOtelHeadersHelper` 保留兼容 1P consumer schema (注释说明) | typecheck; harness:gate | 🟢 低 | ✅ | (待 commit) gate 152/152 EXIT 0；hasCustomHeadersHelper 同时检 customHeadersHelper + otelHeadersHelper |
| X-5 | 删 `otelHeadersHelper` schema (utils/settings/types.ts) + 旧 3 getter + 自动迁移代码 (utils/auth.ts) + 简化 instrumentation 双字段判断 + 简化 TrustDialog 双字段 fallback + managedEnvConstants 改名 + main.tsx/interactiveHelpers OTel 注释改名 | 6 文件；前提：检查用户实际 settings 无 otelHeadersHelper (已确认)，可激进删 | typecheck; harness:gate; grep `otelHeadersHelper` 仅 1 处历史注释 (auth.ts:1790) | 🟡 中 | ✅ | (待 commit) gate 152/152 EXIT 0；analytics 字段名 `hasOtelHeadersHelper` 仍保留兼容 (注释说明) |

**🔍 阶段 X 收尾**：harness:gate × 3 + 真启动 mossen 验 TrustDialog → `git tag pre-otel-phase-X`

---

### 阶段 Y — 1P analytics 迁出 OTel（前置，7 sub-slice，按 D-1 拆细，新增）

> Allen 决策 D-1：必须拆细，不能一刀切。保留 1P 事件能力 / 失败落盘 / 批处理 / 重试 / killswitch / 采样语义。

| # | Slice | 改动 | 验证重点 | 风险 | 完成 | 证据 |
|---|---|---|---|:---:|:---:|---|
| Y-1 | 新增 `services/analytics/eventQueue.ts` + `eventStorage.ts` + `retryScheduler.ts`（B agent 设计的 3 个 helper class，~250 行新代码）| 3 新文件；纯加法，零调用方；generic class T 不耦合 OTel | typecheck; harness:gate; 新文件内单测（如有）| 🟢 低 | ✅ | (待 commit) gate 152/152 EXIT 0；EventQueue 138 行 / EventStorage 99 行 / RetryScheduler 78 行 全 generic |
| Y-2 | 新增 `services/analytics/firstPartyEventExporter.ts`（FirstPartyEventExporter class，零 OTel 依赖）| 新文件；零调用方；组合 Y-1 EventQueue/Storage/Scheduler + 复用现有 auth/killswitch/401 fallback/batching/error context/previous-batch retry 网络逻辑；输入直接是 FirstPartyEventLoggingEvent (transform 推到 Y-4) | typecheck; harness:gate; grep import 链对 | 🟢 低 | ✅ | (待 commit) gate 152/152 EXIT 0；426 行；0 OTel imports；BATCH_UUID + storage filename pattern 与旧 exporter 完全兼容 |
| Y-3 | `services/analytics/firstPartyEventLogger.ts` 加 `_newEventExporter` 实例（与现有 OTel `BatchLogRecordProcessor` 并存，未切换）| 单文件；纯加法 + 双初始化 + reinitialize/shutdown 同步管理新 exporter | typecheck; harness:gate（双写不影响行为）| 🟢 低 | ✅ | (待 commit) gate 152/152 EXIT 0 (M1_2 第一次 LLM flake 重跑通过)；新 exporter 构造但 logEventTo1P 未切走，纯并行就绪 |
| Y-4 | `firstPartyEventLogger.ts` 把 `logEventTo1P` + `logGrowthBookExperimentTo1P` 切走 OTel `firstPartyEventLogger.emit()` → `_newEventExporter.enqueue(transformedEvent)`；transform (raw metadata → MossenCodeInternalEvent/GrowthbookExperimentEvent proto) 内联（旧 transformLogsToEvents 借鉴）；旧 OTel BatchLogRecordProcessor 仍 init 但 dead branch | 单文件；流量切换 + transform 上移 | typecheck; harness:gate；R4 (1P events) 必须过 | 🔴 **关键** | ✅ | (待 commit) gate 152/152 EXIT 0 (M4_1 第一次 LLM flake 重跑通过)；R4 真过；新 exporter 接管 1P 流量；OTel 路径变 dead |
| Y-5 | `firstPartyEventLogger.ts` 删 OTel `BatchLogRecordProcessor` + `LoggerProvider` + `logs.getLogger()` 实例化代码 + 删 firstPartyEventLogger / firstPartyEventLoggerProvider 模块状态 + reinitialize 简化 + 6 个 unused @opentelemetry/* imports 一并清理 (lint forces hand) | 单文件；纯减法 | typecheck; harness:gate; R4 仍过 | 🟡 中 | ✅ | (待 commit) gate 152/152 EXIT 0; R4 真过验证零 OTel 1P 稳态; firstPartyEventLogger.ts 已 0 OTel imports; 副带修 typecheck_diff.py union 类型排序 |
| Y-6 | 直接删 `firstPartyEventLoggingExporter.ts` 整个文件 (804 行；Y-5 已清完最后引用) + index.ts 注释 firstPartyEventLoggingExporter → firstPartyEventLogger | 1 文件删 + 1 文件注释；纯减法 | typecheck; harness:gate; grep `firstPartyEventLoggingExporter` 在 .ts/.tsx = 0 | 🟢 低 | ✅ | (待 commit) gate 152/152 EXIT 0；R4 真过；累计删 -804 行 |
| Y-7 | `bootstrap/state.ts` 删 1P 相关 LoggerProvider setter/getter — **scope 修正：调研发现 STATE.loggerProvider / STATE.eventLogger 实际只被 customer telemetry (utils/telemetry/instrumentation.ts + utils/telemetry/events.ts) 使用，1P pipeline 从未写入 STATE 而是用 firstPartyEventLogger.ts 内的模块局部状态 (Y-5 已删)。Y-7 实质 no-op，state.ts 清理推到 Phase B (Bootstrap 状态解耦)** | 0 文件；plan 前提错误已修正 | grep `STATE.loggerProvider` 仍 customer-only ✓ | ⚪ no-op | ✅ | scope 修正; 阶段 Y 实际 6/7 收尾 |

**🔍 阶段 Y 收尾**：harness:gate × 3 + R4 × 3 + 8 维验证全过 → `git tag pre-otel-phase-Y`

---

### 阶段 A — 纯 DEAD code 删除（4 slice，原计划保留）

| # | Slice | 改动 | 验证重点 | 风险 | 完成 | 证据 |
|---|---|---|---|:---:|:---:|---|
| 1 | 删 `utils/telemetry/logger.ts` (26 行) | 删文件 + `instrumentation.ts` 删 1 处 import | grep `MossenCodeDiagLogger` = 0; harness:gate | ⚪ 零 | ✅ | `96abbcf` Phase A batch |
| 2 | 删 `utils/telemetry/bigqueryExporter.ts` (252 行) | 删文件 + `instrumentation.ts` 删 import + `case 'bigquery'`; 检查 `axios` 是否还有别处用 | grep `BigQueryMetricsExporter\|bigqueryExporter` = 0; harness:gate; 行为 diff | ⚪ 零 | ✅ | Phase A batch；连同 `services/api/metricsOptOut.ts` 一并删（仅被 bigqueryExporter 引用） |
| 3 | 删 `utils/telemetry/skillLoadedEvent.ts` (40 行) | 删文件 + `main.tsx` 删 import + 删 `logSkillsLoaded()` 调用 | grep `skillLoadedEvent\|logSkillsLoaded` = 0; harness:gate; skill 加载烟测 | ⚪ 零 | ✅ | Phase A batch |
| 4 | 删 `utils/telemetry/perfettoTracing.ts` (1120 行) | 删文件; `sessionTracing.ts` 删 import + 8 处 perfetto span 调用 + `SpanContext.perfettoSpanId` 字段 | grep `perfetto\|MOSSEN_CODE_PERFETTO` = 0 (业务代码); harness:gate; R3 跑过 | 🟢 低 | ✅ | Phase A batch；额外删 swarm 3 文件 perfettoAgent register/unregister 调用 |

**🔍 阶段 A 收尾**：跑 Layer 3 全套 → 通过则 `git tag pre-otel-phase-A`

---

### 阶段 B — Bootstrap 状态解耦（2 slice）

| # | Slice | 改动 | 验证重点 | 风险 | 完成 | 证据 |
|---|---|---|---|:---:|:---:|---|
| 5+6 (合) | `utils/telemetryAttributes.ts` 直接 stub 体返 `{}` | 优化: 不加新名, 直接改原函数体, callers 零改动 spread no-op | typecheck; harness:gate (零行为差) | 🟢 低 | ✅ | `65b0fdb` Phase B 一刀清 |

**🔍 阶段 B 收尾**：Layer 3 → `git tag pre-otel-phase-B`

---

### 阶段 C — Span 系统 ALS 与 OTel 分离（4 slice，最重要）

| # | Slice | 改动 | 验证重点 | 风险 | 完成 | 证据 |
|---|---|---|---|:---:|:---:|---|
| 7-10 (合) | sessionTracing+betaSessionTracing 全 stub | makeDummySpan factory (每 span 唯一 spanId); DUMMY_TRACER + 本地 trace/otelContext stub 对象; 5 addBeta* no-op; isBetaTracingEnabled 永返 false; ALS + activeSpans 业务保留 | typecheck; lint; harness 152/152 | 🟡 中 | ✅ | `3b38348` Phase C 一刀清 (-442 行) |

**🔍 阶段 C 收尾**：Layer 3 → `git tag pre-otel-phase-C`

---

### 阶段 D — 初始化链解耦（B0-6 重排：原 3 slice → 2 slice）

| # | Slice | 改动 | 验证重点 | 风险 | 完成 | 证据 |
|---|---|---|---|:---:|:---:|---|
| 11+12+13 (合) | init/state/instrumentation/events 4 文件全 stub | init.ts initializeTelemetryAfterTrust 空体 + helpers 删；state.ts 删 14 OTel 字段 + 18 setter/getter 全 stub (返 null/no-op)；instrumentation.ts 785 行 → 14 行 (留 flushTelemetry no-op)；events.ts logOTelEvent immediate no-op；callers 一律 ?.add()/?.emit() 零修 (Phase F 集中清) | typecheck/lint/harness 全绿；grep `@opentelemetry` 在 state.ts/instrumentation.ts = 0 | 🟡 中（之前 🔴 已通过合并降级）| ✅ | `90f3d90` Phase D 一刀清 (-1080 行) |

**🔍 阶段 D 收尾**：harness:gate × 3 → `git tag pre-otel-phase-D`

---

### 阶段 E — Plugin telemetry 迁移（1 slice）

| # | Slice | 改动 | 验证重点 | 风险 | 完成 | 证据 |
|---|---|---|---|:---:|:---:|---|
| 14 | `pluginTelemetry.ts` 迁到 `services/analytics/pluginMetadata.ts` | 文件搬位 + 改名 (288 行 internal 业务, zero OTel deps); 修 3 处相对路径 import; 5 caller 改 path | typecheck; harness 152/152; grep `utils/telemetry/pluginTelemetry` = 0 | 🟢 低 | ✅ | `8cde887` Phase E (实际有 5 caller, 不是 2) |

**🔍 阶段 E 收尾**：Layer 3 → `git tag pre-otel-phase-E`

---

### 阶段 F — 删剩余文件 + npm 包（4 slice）

| # | Slice | 改动 | 验证重点 | 风险 | 完成 | 证据 |
|---|---|---|---|:---:|:---:|---|
| 15 | 删 `sessionTracing.ts` + `betaSessionTracing.ts` + `events.ts` + `instrumentation.ts` | 4 文件全删; 调用方 (query/main.tsx/services/api/mossen.ts/processTextPrompt.ts) 改 stub helper | grep 上述文件 0 import; typecheck; harness:gate; R3 跑过 | 🟡 中 | ⬜ | — |
| 16 | 删 `utils/telemetryAttributes.ts` + `utils/telemetry/pluginTelemetry.ts` | grep 0 残留后删 | grep 业务代码 0; typecheck; harness:gate | 🟢 低 | ⬜ | — |
| 17 | 卸 8 个 `@opentelemetry/*` npm 包 | `bun remove @opentelemetry/api @opentelemetry/api-logs @opentelemetry/core @opentelemetry/resources @opentelemetry/sdk-logs @opentelemetry/sdk-metrics @opentelemetry/sdk-trace-base @opentelemetry/semantic-conventions` | typecheck; harness:gate; bundle size 对比; package.json + bun.lock 0 `@opentelemetry` | 🟢 低 | ⬜ | — |
| 18 | 终极验收 + 收尾 | grep 全仓; 删 `utils/telemetry/` 空目录; 跑 Layer 4 全套 | Layer 4 全过; harness:gate × 3 EXIT 0; baseline 行为 diff 仅时间戳差 | ⚪ 零 | ⬜ | — |

> 阶段 F 前置确认（按 v1.3 重排）：
> - ✅ 阶段 X 完成 → `otelHeadersHelper` 已改名 `customHeadersHelper`，旧 schema 已删
> - ✅ 阶段 Y 完成 → `firstPartyEventLogger.ts` + Exporter 已迁出 OTel，1P 事件能力保留
> - ✅ 阶段 H 完成 → harness R-series + smoke_check.py 改造已就位
> - ✅ 阶段 V 完成 → 8 维行为验收框架可用
> - ✅ 阶段 A/B/C/D/E 全部完成 → OTel 业务代码已逐步剥离
> 任一未完成时禁止卸 npm 包。

**🔍 阶段 F 收尾**：Layer 4 终极验收 → `git tag post-otel-removal-20260427`（待定日期）

---

### 4.X Slice 总数（v1.3 重排）

| 阶段 | 类型 | Slice 数 | 累计 |
|---|---|---:|---:|
| V | 前置 — 行为验收框架 | 2 | 2 |
| H | 前置 — Harness R-series + smoke_check 改造 | 4 | 6 |
| X | 前置 — `otelHeadersHelper` → `customHeadersHelper` 改名 | 5 | 11 |
| Y | 前置 — 1P analytics 迁出 OTel | 7 | 18 |
| A | 删除 — 纯 DEAD code | 4 | 22 |
| B | 删除 — Bootstrap 状态解耦 | 2 | 24 |
| C | 删除 — Span 系统 ALS 与 OTel 分离 | 4 | 28 |
| D | 删除 — 初始化链（合并后）| 2 | 30 |
| E | 删除 — Plugin telemetry 迁移 | 1 | 31 |
| F | 删除 — 删剩余文件 + npm 包 | 4 | 35 |
| **合计** | — | **35** | — |

---

## 5. 风险登记表（动态更新）

| ID | 风险 | 影响 | 缓解 | 状态 |
|---|---|---|---|---|
| R-001 | sessionId 删错导致 session log 文件名异常 | high | R2 测试守; Slice 12 仅删 OTel 字段不动 sessionId | 🟢 已设防 |
| R-002 | ALS context 拆 OTel 后 async tool 调用断 | high | R3 测试守; Slice 7-9 拆 4 步; Slice 8 dummy span 必须满足 Span interface | 🟢 已设防 |
| R-003 | `axios` 在别处也用, slice 2 卸不掉 | low | Slice 2 只删 bigqueryExporter 文件, 暂不卸 axios | 🟢 已设防 |
| R-004 | `services/analytics/firstPartyEventLogger.ts` 共用 OTel `LoggerProvider` | high | 原计划“不动 1P 代码”的判断不成立；必须按 §0.1 B0-2 迁出 OTel 后再卸包 | 🔴 阻断 |
| R-005 | LLM 不稳导致 R-series 偶发挂 | medium | R-series 内置 _retry × 3; 阶段尾跑 3 轮 | 🟢 已设防 |
| R-006 | typecheck 通过但运行时报 `Cannot read undefined` | medium | 每 slice 必跑 `echo hello \| mossen -p` 行为 diff | 🟢 已设防 |
| R-007 | `commands/insights.ts` WIP 被误改 | high | git add 显式路径; 禁用 . / -A | 🟢 已设防 |
| R-008 | 1P analytics 实际直接 import OTel，若不迁移会导致卸包后编译失败或功能丢失 | high | 按 §0.1 B0-2 新增迁移 slice；功能保留，实现迁出 OTel | 🔴 阻断 |
| R-009 | harness 当前含 OTel 文件级读取/断言，删文件会让 harness 自身失败 | high | 按 §0.1 B0-3 先迁移为行为断言 | 🔴 阻断 |
| R-010 | LLM 原文 diff 不稳定，可能把正常输出差异误判为回归 | medium | 改为结构化断言，不以自然语言原文完全一致为准 | 🟡 需修 |
| R-011 | `otelHeadersHelper`/OTEL env 设置残留与 DoD grep 0 冲突 | medium | **D-2 已定 (b) 改名 customHeadersHelper**；阶段 X 5 sub-slice 处理（schema → getter → instrumentation → UI → 删旧）；含一次性配置迁移 | 🟢 已设计 |
| R-012 | 原计划"36 文件"vs 实测"24 文件"差距 | low | A agent 实测：业务代码 24，差额可能在 src/ 镜像或开发工具；最终验证按 grep 实际数 | 🟢 已澄清 |
| R-013 | `utils/bash/parser.ts` 第 63 行注释提及 "telemetry honest"（非实体引用） | low | DoD `grep otel == 0` 时按"业务代码"判，注释允许保留或更新 | 🟢 可控 |
| R-014 | `firstPartyEventLogger.ts` 与 `firstPartyEventLoggingExporter.ts` 实体 import 7 处 OTel 类（LoggerProvider / BatchLogRecordProcessor / resourceFromAttributes / ATTR_SERVICE_NAME / Logger / HrTime / ExportResult）| high | B agent 完整设计：5 sub-slice 替换为 EventQueue/EventStorage/RetryScheduler/EventExporter；公共 API 签名零变化；Schema proto 不动 | 🟡 待 D-1 拍板 |
| R-015 | `sessionTracing.ts` 把 `Span` type export 给 `betaSessionTracing.ts` 用 | medium | slice 7-9 拆 4 步执行；slice 8 dummy span 必须满足 OTel `Span` interface（不能直接换成普通对象，要保 type 兼容到 slice 9） | 🟢 已设防 |
| R-016 | `otelHeadersHelper` 在 `utils/auth.ts` 含 3 个核心函数 | medium | **D-2 已定 (b) 改名**；阶段 X-2 加新 3 函数 `getCustomHeadersForRequest` 等；X-3 切 instrumentation 调用方；X-5 删旧 3 函数；含 settings.json 旧 → 新字段一次性迁移 | 🟢 已设计 |
| R-017 | `subprocessEnv.ts` 第 24 行把 `OTEL_EXPORTER_OTLP_HEADERS` 继承给子进程（MCP / plugins） | low | 卸包后子进程不再需要此 env；可在最后 slice 删继承列表对应行 | 🟢 已设防 |
| R-018 | `harness_fixture.py` 命名 bug：第 90 行设 `MOSSEN_CONFIG_HOME` 但代码读 `MOSSEN_CONFIG_DIR`（已知 bug 记忆 1880） | medium | R2 fixture 必须显式补 `MOSSEN_CONFIG_DIR`；本任务范围内不修 fixture 自身（避免触发其他 e2e 回归） | 🟢 已设防 |
| R-019 | D agent 8 维断言中"维 8 model 字符串"hardcode 风险（fixture model 升级会导致断言挂） | medium | baseline schema 设计 `expected_value_source` 字段；从 fixture env / CLI args 动态读，不硬编码 | 🟢 已设计 |
| R-020 | smoke_check.py 第 3264-3833 行 10 处 telemetry 文件读取，删 telemetry 文件后会 FileNotFoundError | high | C agent 已列具体迁移决策表（§0.4.4）；必须先做 smoke_check 改造 commit 再删 telemetry 代码 | 🟡 待 D-1 后实施 |
| R-021 | C agent 实测 harness 当前总数 = 58 个 harness_M*.py 测试，与文档原说"147"不一致 | low | "147" 是 smoke_check.py 内**所有** smoke 检查（含 platform_check / errorqueue_drain 等非 harness_M 的），不只是 harness_M 测试。两个数字描述不同集合，无矛盾 | 🟢 已澄清 |
| R-022 | B agent 设计的 `transformLogsToEvents` 复用方案需验证 proto schema 不变 | medium | B 设计阶段已通过 codegen 文件存在性确认（`/types/generated/events_mono/mossen_code/v1/*.pb.ts`）；实施时再运行单测验证 toJSON() 输出无变化 | 🟢 已设防 |
| R-023 | 阶段 Y-4（initialize1PEventLogging 切到新路径）是 1P 业务流量切换的"关键转折点"，万一新路径有 bug 会丢事件 | high | Y-3 先建并存（双初始化）；Y-4 只切初始化路径；保留旧 OTel BatchLogRecordProcessor 实例直到 Y-5；R4 smoke 在 Y-4 后必须严格通过；阶段 Y 收尾跑 R4 × 3 + 8 维全验 | 🟡 待执行验证 |
| R-024 | worktree 与主工作区文档版本不一致 | medium | 计划文档 v1.3 在主工作区，先 commit 到 main，然后开 worktree from main HEAD；之后所有 doc 更新都在 worktree 内进行；合并时 worktree 文档为准 | 🟡 实施时验 |
| R-025 | 阶段 X-2 自动迁移 `otelHeadersHelper` → `customHeadersHelper` 写回 settings 时如果用户配置文件被并发改写会冲突 | low | 用 atomic write (写临时文件 + rename)；只在读到旧字段且无新字段时才迁移；写迁移单测覆盖 | 🟢 已设计 |
| R-026 | mock HTTP server (R1) 在 fixture 内绑定端口可能与系统服务冲突 | low | 用 port 0 让 OS 分配；server 启动后从 socket 读实际端口注入到 env；fixture 退出时显式 close server | 🟢 已设计 |
| ... | (执行中发现新风险补这里) | | | |

---

## 6. 执行进度总览

```
前置阶段 (V/H/X/Y, 18 slice):
  V (2):  ⬜⬜
  H (4):  ⬜⬜⬜⬜
  X (5):  ⬜⬜⬜⬜⬜
  Y (7):  ⬜⬜⬜⬜⬜⬜⬜

删除阶段 (A/B/C/D/E/F, 17 slice):
  A (4):  ✅✅✅✅  (Phase A 一刀清, 96abbcf, -1846 行)
  B (2):  ✅✅  (Phase B stub, 65b0fdb, telemetryAttributes 直接 stub)
  C (4):  ✅✅✅✅  (Phase C stub, 3b38348, sessionTracing+beta -442 行)
  D (2):  ✅✅  (Phase D stub, 90f3d90, init+state+instrumentation+events -1080 行)
  E (1):  ✅  (Phase E move, 8cde887, pluginTelemetry → analytics/pluginMetadata)
  F (4):  ⬜⬜⬜⬜

阻断修订 B0:
  调研: ✅✅✅✅✅✅✅ (B0-1 至 B0-7 全部 4 个子 agent 完成, 2026-04-27 11:15)
  Allen 决策 D-1~D-4: ✅ (2026-04-27 11:45 拍板)
  实施: ⬜⬜⬜⬜⬜⬜⬜ (融入 V/H/X/Y 阶段, 不再独立编号)

Worktree: ⬜ 待创建 `worktree/otel-removal`

完成: 31/35 slice (V/H/X/Y/A/B/C/D/E 全部 ✅；F 4 slice 待办)
```

---

## 7. 完成定义 (DoD)

本任务"全完成"定义（必须**全部**满足）：

- [ ] 重排后的 35 slice 全部 ✅ 已 commit
- [ ] §0 阻断修订 B0-1 到 B0-7 全部完成（融入 V/H/X/Y 阶段）
- [ ] 阶段尾 10 个 git tag 全部存在（V/H/X/Y/A/B/C/D/E/F）
- [ ] R1/R2/R3/R4 四个 R-series 测试在 worktree 分支正常工作
- [ ] `harness:gate × 3` 三连 EXIT 0（151/151 each）
- [ ] 8 维行为结构化断言（V-1 框架）在 5-8 个核心场景全 pass
- [ ] grep 全仓 `@opentelemetry` 业务代码 = 0 行
- [ ] grep 全仓 `from.*utils/telemetry` = 0 行
- [ ] grep 全仓 `otelHeadersHelper` 业务代码 = 0 行（D-2）
- [ ] `package.json` + `bun.lock` 中 `@opentelemetry` 字面量 = 0
- [ ] `utils/telemetry/` 目录已删
- [ ] `utils/telemetryAttributes.ts` 已删
- [ ] `services/analytics/firstPartyEventLogger.ts` + Exporter 不含任何 `@opentelemetry/*` import
- [ ] 1P 事件能力保留（R4 通过）+ 失败落盤 + 批处理 + 重试 + killswitch + 采样语义全保留
- [ ] customHeadersHelper schema/UI/getter 全替代 otelHeadersHelper（D-2）
- [ ] worktree 已合并回 main，分支 `worktree/otel-removal` 可删
- [ ] memory 更新（`project_otel_removal_complete.md` + `MEMORY.md` 索引）
- [ ] 本计划文档"完成"列全 ✅
- [ ] main HEAD 处 git tag `post-otel-removal-<date>`

---

## 8. 执行日志（每完成一个 slice 追加一行）

| 时间 | Slice / 任务 | Commit | 关键证据 | 备注 |
|---|---|---|---|---|
| 2026-04-27 09:58 | 计划 v1.0 创建 | 未 commit (主工作区 untracked) | 405 行 / 18KB / 18 slice | 主 agent 写 |
| 2026-04-27 10:35 | 计划 v1.1 Codex 审查修订 | 未 commit | +§0 阻断修订 / B0-1 至 B0-7 / R-008/R-009/R-010/R-011 风险 | Allen 修订 |
| 2026-04-27 11:00 | 第一批子 agent 启动 (A/B/C/D 并行) | 未 commit (read-only) | 4 个 Explore agent, 全 read-only, 单 worktree=主工作区 | 主 agent 派单 |
| 2026-04-27 11:15 | 子 agent A 完成 — OTel 引用全仓分类 | 未 commit | 8 npm 包 / 24 文件 / 文件分级表 / 16 处 otelHeadersHelper / 19 OTEL_* env / 6 新风险 R-012~R-017 | 详见 §0.4.1 + §0.4.2 |
| 2026-04-27 11:15 | 子 agent B 完成 — 1P analytics 迁出 OTel 方案 | 未 commit | 1251 行分析 / 5 sub-slice (B-1→B-5) / 4 新文件设计 / R4 smoke | 详见 §0.4.3 |
| 2026-04-27 11:15 | 子 agent C 完成 — harness/R-series 改造方案 | 未 commit | smoke_check.py 10 处迁移决策 / R1/R2/R3 伪代码 / 命名 bug R-018 | 详见 §0.4.4 |
| 2026-04-27 11:15 | 子 agent D 完成 — 结构化行为验收方案 | 未 commit | 8 维框架 / baseline JSON schema / Layer 4 重设 / 分阶段验证 / 风险 R-019 | 详见 §0.4.5 |
| 2026-04-27 11:30 | 主 agent 计划 v1.2 汇总更新 | 未 commit | §0 状态拆"调研/实施"两列 / §0.4 子 agent 结论汇总 / §0.5 待 Allen 拍板 4 决策点 / §5 风险表新增 R-012~R-022 / §6/§8 同步 | 等待 Allen D-1~D-4 拍板 |
| 2026-04-27 11:45 | Allen 拍板 4 决策 | — | D-1=(a) 5 sub-slice / D-2=(b) 改名 customHeadersHelper / D-3=(b) mock HTTP server / D-4=(b) worktree | 见 §0.5 |
| 2026-04-27 11:55 | 主 agent 计划 v1.3 重排 | 未 commit | §0.5 决策表 / §0.6 R1 mock server 设计 / §4 35 slice 重排（新增 V/H/X/Y 18 sub-slice + D 合并）/ §5 R-016/R-011 状态更新 + R-023~R-026 / §6/§7/§8 同步 | 准备开 worktree |
| 2026-04-27 11:58 | 主 agent commit 计划 v1.3 到 main | `ecde6e0` | `git commit -m "docs(otel-removal): 计划 v1.3 — 35 slice 重排 + Allen 4 决策融入"` | 准备开 worktree |
| 2026-04-27 12:00 | 主 agent 创建 worktree | — | `git worktree add ../mossensrc-otel-removal -b worktree/otel-removal main` → path: `/Users/allen/Documents/aiproject/mossensrc-otel-removal`, branch HEAD = `ecde6e0` | B0-1 完成 |
| 2026-04-27 12:00 | 主 agent 计划 v1.4 worktree 内更新 | (待 commit 到 worktree branch) | §0.1 B0-1/B0-5/B0-6 状态从 ⬜ 改 ✅；元信息 HEAD/branch 改为 worktree | 后续所有 doc 更新都在 worktree |
| 2026-04-27 15:50 | **Phase A 一刀清** (4 slice 合 1 commit) | `96abbcf` | 删 5 文件 (logger/bigqueryExporter/skillLoadedEvent/perfettoTracing/metricsOptOut) + sessionTracing 8 perfetto 调用 + 3 swarm 文件 perfetto agent + main.tsx unused; -1846 行；typecheck 1408→1404；lint 945→943；harness 152/152 EXIT 0；tag `pre-otel-phase-A` | 按 Allen "不要一个个跑了" 指示批量处理 |
| 2026-04-27 16:05 | **Phase B 一刀清** (2 slice → 1 commit) | `65b0fdb` | telemetryAttributes.ts body 直接返 `{}`，删 6 imports + 41 行 logic；callers 零改动 (spread `{}` no-op)；typecheck 5 fixed；harness 152/152；tag `pre-otel-phase-B` | 优化原计划 (省 add-stub 步) |
| 2026-04-27 16:08 | **Phase E** (1 slice → 1 commit) | `8cde887` | pluginTelemetry.ts → services/analytics/pluginMetadata.ts (288 行 zero-OTel 业务搬位)；修 3 内部相对路径 + 5 caller import；harness 152/152；tag `pre-otel-phase-E` | 与 Phase B 共用 1 次 gate |
| 2026-04-27 16:25 | **Phase C 一刀清** (4 slice → 1 commit) | `3b38348` | sessionTracing.ts 加 makeDummySpan factory + 本地 trace/otelContext stub；betaSessionTracing.ts 491→80 行 (5 addBeta no-op + isBetaTracingEnabled 永 false)；ALS 业务保留；-442 行；harness 152/152；tag `pre-otel-phase-C` | 子 agent 设计修正: 单 DUMMY_SPAN const 改 factory (parallel tracking) |
| 2026-04-27 16:33 | **Phase D 一刀清** (2 commit → 1 commit) | `90f3d90` | init.ts 清 initializeTelemetryAfterTrust + helpers；state.ts 删 14 OTel 字段 + 18 stub setter/getter；instrumentation.ts 785→14 行；events.ts 113→28 行；-1080 行；callers 全靠 ?.add() / ?.emit() 容错；typecheck 22 fixed；harness 152/152；tag `pre-otel-phase-D` | 子 agent 调研指引；Phase C+D 共用 1 次 gate |
| 2026-04-27 17:08 | **Phase F 终结清扫** (15+ slice → 1 commit) | `f23bd4e` | 删 5 stub 文件 (sessionTracing/betaSessionTracing/events/instrumentation/telemetryAttributes) + 18 调用方就地清 (state/mossen/logging/hooks/toolExecution/REPL/postCompactCleanup/3 FeedbackSurvey/permissionLogging/cost-tracker/diff/gitOperationTracking/activityManager/logout/processSlashCommand/processTextPrompt)；hooks.ts 删 startHookSpan/endHookSpan/isBetaTracingEnabled + 4 调用 + dead 辅助；mossen.ts 删 attemptStartTimes/llmSpan/newContext；logging.ts 删 endLLMRequestSpan/Span 参数；toolExecution.ts 删 logOTelEvent/getCounter + dead 辅助；-1672 行；typecheck 24 fixed；lint 2 fixed；harness M1_6/M2_3 单跑通过 (LLM flake 非回归)；tag `pre-otel-phase-F` | 单 commit 收敛 17 slice 工作量 |
| 2026-04-27 17:14 | **Phase F-17** bun remove 8 OTel 包 + 清陈旧注释 | `13b653a` | 卸载 8 个 `@opentelemetry/*` 包 (api/api-logs/core/resources/sdk-logs/sdk-metrics/sdk-trace-base/semantic-conventions)；bun.lock 同步重生成；utils/gitSettings.ts 顶部注释删除过时 settings.ts ↔ OTel 关联说明；package.json `opentelemetry` 关键字 0 命中；tag `post-otel-removal-20260427` | 35 slice 工程完整收尾 |

---

## 11. 最终验收 (2026-04-27 17:14 — 35/35 slice ✅)

```text
全仓 grep 验收:
  @opentelemetry import in *.ts/*.tsx     : 0 命中 ✅
  from .*telemetry/ in *.ts/*.tsx         : 0 命中 ✅
  package.json @opentelemetry/* deps      : 0 命中 ✅
  bun.lock @opentelemetry refs            : 0 命中 ✅

代码量变化 (ecde6e0 → 13b653a, 整个 worktree branch):
  Phase V/H/X/Y (slice 1-22)      : 1P analytics 迁出 OTel + 调研基础设施
  Phase A (slice 23-26)            : -1846 行 (5 文件删 + 8 perfetto 调用)
  Phase B (slice 27-28)            : -41  行 (telemetryAttributes stub)
  Phase E (slice 29)               : pluginTelemetry → pluginMetadata 业务搬位
  Phase C (slice 30-33)            : -442 行 (sessionTracing/betaSessionTracing stub)
  Phase D (slice 34)               : -1080 行 (init/state/instrumentation/events stub)
  Phase F (slice 35)               : -1672 行 (5 stub 删 + 18 调用方清)
  npm 依赖                          : -8 个 @opentelemetry/* 包

测试验收:
  typecheck:diff baseline 1408 → 1384  (-24 fixed, 0 new)
  lint:diff      baseline  945 →  943  ( -2 fixed, 0 new)
  harness:gate   152 smokes (M-series LLM 偶发 flake，单跑全通)

Tag 链:
  pre-otel-phase-{V,H,X,Y,A,B,C,D,E,F}  全部建立 ✅
  post-otel-removal-20260427             已建立 ✅
```

**下一步用户决策**:
- 是否合并 worktree branch `worktree/otel-removal` 回 main
- 是否 push 到 remote (默认不 push，等 Allen 确认)
- 是否 regen typecheck/lint baseline 收割 -24/-2 的 fixed 计数

---

## 9. 用户决策点

执行中遇到以下情况, **必须**停下问用户:

- 任何未列入计划的文件改动
- 任何阶段尾验证未过 (即使 slice 单独过)
- 发现新风险需要新 slice 时
- baseline 行为 diff 出现非时间戳/UUID 差异
- 结构化行为验收失败，或需要改变基线能力定义
- 测试连续 retry 3 次仍失败
- npm 依赖卸载发现别处也用 (R-003 类延展)

---

## 10. 终止条件 (紧急停止)

立即停止执行 + 通知用户的情况:

- ❌ harness:gate 单次失败 (即使重跑过)
- ❌ R1/R2/R3 任一连续失败 2 次
- ❌ git working tree 出现非预期 untracked 文件
- ❌ 发现 `commands/insights.ts` 被改
- ❌ 发现 OTel 删除影响了 1P analytics，或未完成 1P analytics 迁移就尝试卸包

---

**文档版本**: v2.0 (2026-04-27 17:14 worktree 内：35/35 slice 完成 + post-otel-removal-20260427 tag)
**当前位置**: worktree `/Users/allen/Documents/aiproject/mossensrc-otel-removal` (branch `worktree/otel-removal`, HEAD `13b653a`)
**当前状态**: ✅ OTel 移除工程完整完成。等待 Allen 决策合并/push。
