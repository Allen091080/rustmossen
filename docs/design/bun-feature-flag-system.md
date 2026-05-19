# Bun Feature Flag 体系

> **目的**: 文档化 mossen 项目的三级切换体系 (bunfig MACRO / bun feature / process.env), 防止误判 (如 Wave 3 prep "bunfig.toml [define] 缺 USER_TYPE/EXPERIMENTAL_SKILL_SEARCH" 的实测推翻)。
> **来源**: Wave 4 R8 施工包实证 + `run-bun-featured.sh` 源码 + 全仓 `feature(...)` 调用统计。
> **状态**: v1.1 设计文档 (Wave 4 阶段 2 R8.2 已落地). R8.3 命名混淆解决推 Wave 5+。

---

## 1. 三级切换体系总览

| 体系 | 机制 | 触发点 | 范围 | 文件位置 |
|------|------|--------|------|---------|
| **bunfig.toml [define]** | 编译时常量替换 (build-time) | `bun run` / `bun build` 加载时 | 7 个 `MACRO.*` 字符串 (版本/构建元数据) | `bunfig.toml` |
| **bun --feature=X** | 编译时 + runtime feature gate | `run-bun-featured.sh` 注入 `--feature=X` args | **79 feature token** (KAIROS / VOICE_MODE / BUDDY 等, 见 §2.2) | `.mossensrc/feature-flags.env` (per-developer, 不入仓) |
| **process.env.X** | runtime env var | shell export / 进程启动 | USER_TYPE / MOSSEN_CODE_USE_BEDROCK / 等 | shell |

**关键**: 三级**完全独立**, 用法不可混用。

---

## 2. 各级使用场景

### 2.1 何时用 MACRO (bunfig.toml [define])

**用途**: 版本元数据 + 构建期不变常量。

**当前 7 个 MACRO** (`bunfig.toml`):
```toml
[define]
"MACRO.VERSION" = "\"1.0.0\""
"MACRO.BUILD_TIME" = "\"2026-04-18T00:00:00.000Z\""
"MACRO.PACKAGE_URL" = "\"@mossen/mossen-code\""
"MACRO.NATIVE_PACKAGE_URL" = "\"@mossen/mossen-code\""
"MACRO.FEEDBACK_CHANNEL" = "\"https://github.com/mossen/mossen-code/issues\""
"MACRO.ISSUES_EXPLAINER" = "\"open an issue at https://github.com/mossen/mossen-code/issues\""
"MACRO.VERSION_CHANGELOG" = "\"\""
```

**特点**:
- 编译时直接替换源码中的 `MACRO.X` 引用为字符串字面量
- 所有 build (dev / prod) **都生效**
- 不能在 runtime 动态切换
- 适合: 版本号 / 构建时间戳 / 包名 / 反馈渠道 URL 等"出厂即定"的常量

**不适合**:
- ❌ 功能开关 (用 feature gate)
- ❌ runtime 用户身份 (用 process.env)
- ❌ 任何需要在 dev / prod 不一样的字符串

### 2.2 何时用 feature (bun --feature=X)

**用途**: 功能模块的编译时 + runtime 切换 (build-time DCE + runtime gate)。

**当前 79 feature token** (实测从全仓 `feature(...)` 调用统计, Wave 4 阶段 2 R8.2 落地):

权威清单维护在 **`scripts/feature-flag-token-whitelist.txt`** (字典序, single source of truth)。`scripts/wave4_r8_feature_flag_smoke.py` 自动校验全仓 `feature()` 调用集与 whitelist 完全相等 — 任一方漂移即 fail。

代表性 token (按子系统分组):
- Kairos 系: `KAIROS`, `KAIROS_BRIEF`, `KAIROS_CHANNELS`, `KAIROS_DREAM`, `KAIROS_GITHUB_WEBHOOKS`, `KAIROS_PUSH_NOTIFICATION`
- 子 agent / 工具: `FORK_SUBAGENT`, `BUILTIN_EXPLORE_PLAN_AGENTS`, `MONITOR_TOOL`, `OVERFLOW_TEST_TOOL`, `WEB_BROWSER_TOOL`, `WORKFLOW_SCRIPTS`
- Voice / Buddy / Teammem: `VOICE_MODE`, `BUDDY`, `TEAMMEM`
- Classifier: `BASH_CLASSIFIER`, `TRANSCRIPT_CLASSIFIER`
- Compact: `REACTIVE_COMPACT`, `CACHED_MICROCOMPACT`, `CONTEXT_COLLAPSE`, `PROMPT_CACHE_BREAK_DETECTION`
- 其他: `AUTO_THEME`, `CONNECTOR_TEXT`, `HISTORY_SNIP`, `HARD_FAIL`, `ULTRATHINK`, `ULTRAPLAN`, `AGENT_MEMORY_SNAPSHOT`, `AGENT_TRIGGERS`, `AGENT_TRIGGERS_REMOTE`, `TERMINAL_PANEL`, `UDS_INBOX`, ... (+ 其他, 完整 79 见 whitelist 文件)

**新增 / 移除 SOP**: 见 §4.2 / §4.4。同步漏改 whitelist 会被 R8.2 smoke 在 `bash scripts/run_all_smoke.sh` 阻断。

**机制**:
1. 开发者在 `.mossensrc/feature-flags.env` 定义 `MOSSENSRC_BUN_FEATURES="KAIROS,BUDDY,..."`
2. `run-bun-featured.sh` 启动时读取 env, 转换为 `bun --feature=KAIROS --feature=BUDDY ...`
3. Bun 编译时根据 `--feature=X` 决定哪些代码块进入 bundle (DCE)
4. 代码层 `feature("X")` 在 bundle 中变为 const true/false

**特点**:
- dev: 通过 `feature-flags.env` 控制 (per-developer)
- prod / CI: 通过环境变量 `MOSSENSRC_BUN_FEATURES` 控制 (CI 配置位置待确认)
- 同一行代码 `if (feature("KAIROS")) { ... }` 在 dev 和 prod 可能行为不同 (取决于 feature 是否启用)
- 适合: 大型功能模块 (Voice / Kairos / Bridge etc) 的开关

### 2.3 何时用 process.env.X (runtime env)

**用途**: runtime 用户身份 / backend 选择 / 紧急开关。

**典型 token**:
- `process.env.USER_TYPE` (ant / external / mossen) — 受 Wave 4 R1/R2/R3 收敛, 推 Wave 5
- `process.env.MOSSEN_CODE_USE_BEDROCK` / `..._VERTEX` / `..._FOUNDRY` — cloud provider 选择
- `process.env.DISABLE_ERROR_REPORTING` — 紧急关 telemetry
- `process.env.MOSSENSRC_BUN_FEATURES` — feature flag 体系入口
- `process.env.MOSSEN_CODE_DEBUG` — 调试模式

**特点**:
- 完全 runtime, 编译时不参与 DCE
- 修改即生效 (重启进程)
- 适合: 用户身份 / 配置覆盖 / 紧急开关

**不适合**:
- ❌ 功能模块 DCE (用 feature)
- ❌ 出厂常量 (用 MACRO)

---

## 3. dev / prod 行为差异

### 3.1 dev (`bun run` / `run-bun-featured.sh`)

```
1. shell 调用 ./run-bun-featured.sh <args>
2. 脚本读 ~/.mossensrc/custom-backend.env (可选)
3. 脚本读 ~/.mossensrc/feature-flags.env (可选)
4. 从 MOSSENSRC_BUN_FEATURES 提取 feature 列表
5. 拼装 bun --feature=X --feature=Y ... <args>
6. exec bun ...
```

- bunfig.toml [define] **生效** (MACRO 替换)
- feature() **生效** (按 .mossensrc/feature-flags.env)
- process.env.X **生效** (按 shell)

### 3.2 prod (CI build, 配置位置待确认)

- bunfig.toml [define] **生效** (同 dev)
- feature() 行为取决于 CI 是否设置 `MOSSENSRC_BUN_FEATURES`
- process.env.X 取决于 CI 配置

**当前 CI 行为不文档化** — 建议 R8.2 audit smoke (Wave 5) 校验。

### 3.3 验收覆盖矩阵 (双模式必跑场景)

任何涉及 feature() 或 process.env 的源码改动, 应在以下两种模式下都验收:

| 模式 | 启动方式 | 验证 |
|------|---------|------|
| dev (典型 ant 配置) | `MOSSENSRC_BUN_FEATURES="KAIROS,VOICE_MODE,..." ./run-bun-featured.sh ...` | 启动 + skill 调用 + LLM 冒烟 |
| dev (mossen 默认) | `./run-bun-featured.sh ...` (无 feature env, 或最小集) | 启动 + 残留 + smoke |
| prod (推断) | bun build + 同样 feature 集 | 输出 bundle 大小 + DCE 证据 |

R8.1 阶段不强制双模式 — Wave 5 R8.2 audit smoke 落地后可自动验证。

---

## 4. SOP — 新增 / 移除 feature 步骤

### 4.1 新增 MACRO (bunfig.toml [define])

| 步骤 | 动作 |
|------|------|
| 1 | 在 `bunfig.toml [define]` 加 `"MACRO.NEW_KEY" = "\"value\""` |
| 2 | 源码中通过 `MACRO.NEW_KEY` 引用 (TS 编译器看到字符串字面量) |
| 3 | 跑 `bun run typecheck:diff` 验证无 NEW error |
| 4 | commit message: `chore(bunfig): add MACRO.NEW_KEY for ...` |
| 5 | **不跨 wave 加 MACRO** — bunfig.toml 是系统红线, 须 Allen 解禁 |

### 4.2 新增 feature gate (bun --feature=X)

| 步骤 | 动作 |
|------|------|
| 1 | 源码中加 `if (feature("NEW_FEATURE")) { ... }` |
| 2 | 在 `.mossensrc/feature-flags.env` 模板 (待 Wave 5 R8.2 落地) 加注释说明 |
| 3 | 全仓 grep `feature("NEW_FEATURE"` 验证只有 1 个 token |
| 4 | dev 跑 `MOSSENSRC_BUN_FEATURES="NEW_FEATURE" ./run-bun-featured.sh ...` 验证启用 |
| 5 | dev 跑无该 feature 验证 disabled 路径 |
| 6 | commit message: `feat(<module>): add NEW_FEATURE feature gate` |

### 4.3 新增 process.env gate

| 步骤 | 动作 |
|------|------|
| 1 | **优先用 `getUserType()` / 类似 fallback 函数**, 不直接读 `process.env.X` (Wave 0 NEEDS-DESIGN-API-001 教训) |
| 2 | 若必须读 env, 加注释说明 fallback 行为 |
| 3 | 跑 `wave0_perm1` / `wave0_perm2` 等 USER_TYPE 三态 smoke 验证 |
| 4 | commit message: `feat(<module>): add NEW_ENV_GATE runtime gate` |
| 5 | **R1/R2/R3 USER_TYPE 286+ 处收敛进行中** (推 Wave 5), 不要新增 USER_TYPE 直读 |

### 4.4 移除 feature / MACRO

| 步骤 | 动作 |
|------|------|
| 1 | 全仓 grep 该 token, 确认 0 引用后才能删 (Wave 1.5 + Wave 5 Phase 2 BRIDGE_MODE 删除模式) |
| 2 | 同 commit 删 bunfig.toml 对应行 / `.mossensrc/feature-flags.env` 注释 |
| 3 | typecheck:diff + lint:diff 必须 0 NEW |
| 4 | commit message: `refactor(<module>): drop dead FEATURE feature` |

---

## 5. 已知问题 / 红线

### 5.1 bunfig.toml [define] 不含 build-time gate (这是设计, 不是 bug)

- bunfig.toml [define] **故意只用作 MACRO 字符串常量**, 不作 feature gate
- 所有 build-time DCE 走 `feature()` API + Bun feature flag 体系
- USER_TYPE 走 runtime, **不走 build-time** (Wave 0 NEEDS-DESIGN-USERTYPE-LOCK 决策)

### 5.2 R8.2 audit smoke 已落地 (Wave 4 阶段 2)

`scripts/wave4_r8_feature_flag_smoke.py` 提供 4 项静态校验:
- A: `bunfig.toml [define]` 段所有 key 必须 `MACRO.*` 前缀
- B: 全仓 `feature('TOKEN')` 唯一集 == `scripts/feature-flag-token-whitelist.txt`
- C: `platform/featureGatesRuntime.ts` 内 `resolve('TOKEN')` 必须在 whitelist 或 `KNOWN_DEBT_RESOLVE_ORPHANS`
- D: 输出诊断 (feature 总数 / whitelist 总数 / orphans / known debt 清单)

100% 静态 (0 network / 0 LLM / 0 mossen 启动), 实测 < 0.2s, 已接入 `scripts/run_all_smoke.sh` 第 17 step。

**仍待补**:
- dev / prod build bundle 一致性 (DCE 证据) — Wave 5+
- 未引用的 MACRO 检测 (当前 7 个全部在源码中被引用, 暂无 dead MACRO 风险) — Wave 5+

### 5.2.1 已处置 dead-code 残留

| 项 | 文件 | 处置 |
|----|------|------|
| ~~`BRIDGE_MODE` resolve orphan~~ | ~~`platform/featureGatesRuntime.ts:39`~~ | **Wave 5 Phase 2 已删** (`refactor(wave5): remove BRIDGE_MODE feature flag debt`). resolve callsite + runtimeTypes 字段 + auth.ts 诊断输出 + platform_check.ts 校验 + utils/config.ts 注释 + 启动说明.md env doc 全清. `KNOWN_DEBT_RESOLVE_ORPHANS` 已改 `frozenset()`. |
| ~~`BRIDGE_MODE` profile in `feature_audit.py`~~ | ~~`scripts/feature_audit.py:64-67, 82-86`~~ | **Wave 5 Phase 2 已删**: `bridge-mode` profile 整删, `daemon-bridge` profile 改名为 `daemon-only` 且 features 仅 `["DAEMON"]`. |

### 5.3 `.mossensrc/feature-flags.env` 是 per-developer (不入仓)

- 每个开发者的 dev feature 集合可能不同, 易造成"我能跑你不能跑"
- **Wave 5 Phase 2 已落地**: `.mossensrc/feature-flags.env.example` 模板 (79 token 分组速查 + Mossen 个人版推荐 export + 已废弃 token 标注), `.gitignore` 加白名单例外仅追踪 `.example` 自身; 真实 `.mossensrc/feature-flags.env` 仍 per-developer 不入仓

### 5.4 红线引用 (与 `red-lines.md` §3)

- **`bunfig.toml` 不轻动** — 改动需 Allen 解禁
- **`scripts/smoke_check.py` 不动** — Wave 3 R5 永久不做的传导
- **不动 `utils/i18n/strings.*.ts`** — 不属本体系范围, 但常被混淆

---

## 6. 维护责任

- **本文件**: `docs/design/bun-feature-flag-system.md`
- **更新时机**:
  - 新增 / 移除 MACRO → 同 commit 更新 §2.1 列表
  - 新增 / 移除 feature token → **同 commit 改 `scripts/feature-flag-token-whitelist.txt`** (R8.2 smoke 强制校验, 漏改会 fail)
  - dev / prod 行为差异 (CI 配置确认) → 必须更新 §3.2
  - `KNOWN_DEBT_RESOLVE_ORPHANS` 变化 (`scripts/wave4_r8_feature_flag_smoke.py`) → 必须 Allen 拍板 + 同步 §5.2.1

---

*— Bun Feature Flag 体系 v1.1 / Wave 4 阶段 2 R8.2 落地 (静态审计 smoke + whitelist + known debt). R8.3 命名混淆推 Wave 5+.*
