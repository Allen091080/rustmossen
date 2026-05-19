# NEEDS-DESIGN-USERTYPE-LOCK

> **Status**: Wave 0 设计记录 + **Wave 7 Door Lock 已实施 [`def26fd`]** (2026-04-30).
> **Wave 0 决策**: 不直接在 `run-mossen.sh` `export USER_TYPE=external`.
> 仅做 NEEDS-DESIGN-API-001 (`services/api/withRetry.ts:354`) 作为方案 B
> 第一个示范点; 全仓推广留 Wave 2 评估.
> **Wave 7 决策**: 采纳 **shell CLI-only 锁 + JS 兜底** 的混合方案，详见 §8.
> **来源**: USER_TYPE-runtime-gate-research/00-summary.md §0.5 / §7.3.6 +
> S3-action-Wave0-施工包.md §4 + Wave7 实施日志.

## 1. 背景

`USER_TYPE-runtime-gate-research/entry-point.md` (第 10 类调研) 确认:
mossen 个人版默认 **`USER_TYPE = undefined`** (启动链 0 处设置).

潜伏激活面:
- 用户复制粘贴 anthropic 上游教程 `export USER_TYPE=ant` → 立即激活
  ~5 条 ant gate (skill stuck Slack post / prompt GrowthBook suffix /
  dumpPrompts 落盘 / Shift+Tab silent escalation / errorLogSink 落盘)
- 用户残留 `export USER_TYPE=mossen` → 立即激活 ~14 条 mossen gate
  (canonical skill / Bash 命令 BQ 上传 / MOSSEN env strip / gh+aki 等;
  原 CCR isolation:'remote' 路径已 Wave 2A-A3 hard remove)

> **Wave 2 (2026-04-28) 补**: 上述 5 条 ant gate 中 stuck/dumpPrompts/GrowthBook suffix 已 hard remove (A6/B-C2/A7); 仅 errorLogSink 落盘和 Shift+Tab escalation 仍在 USER_TYPE=ant gate 下.

自然反应: 在 `run-mossen.sh` 显式 `export USER_TYPE=external`,
锁定语义, 杜绝潜伏激活.

## 2. 利

1. **杜绝潜伏激活面**: 用户即使在 shell rc 中 export USER_TYPE=ant/mossen,
   `run-mossen.sh` 启动后会被覆盖.
2. **getUserType() 一致性**: 4 处 `getUserType()` 调用 (sessionStorage.ts
   局部) 与 ~330 处直接读 env 行为统一 (都拿 `'external'`).
3. **NEEDS-DESIGN-API-001 简化**: 不需要改 withRetry.ts:354 谓词, 因为
   `process.env.USER_TYPE === 'external'` 直接成立.
4. **明确单元身份**: "mossen 个人版用户 = external 用户"成为显式契约,
   而不是 fallback 隐含.

## 3. 弊

1. **改变全局语义**: 一旦 export USER_TYPE=external, 全仓 ~330 处
   `process.env.USER_TYPE === 'external'` 比较都激活, 影响面非常大.
2. **analytics 反向风险**: datadog `env=external` + metadata
   `userType=external` 会让 mossen 用户事件被标成"external", 与上游
   anthropic external (Pro/PAYG 客户) 用户混入同一 dashboard 切面
   (analytics.md P1-1/2 — 之前是 `undefined` → 不上报或上报空字段, 改后
   变成"伪装上游 external").
3. **sub-agent 类 isolation gate 行为变化**: `loadAgentsDir.ts` 等
   `=== 'external'` 比较若有, 行为变化. (注: 原 USER_TYPE=mossen
   isolation:'remote' 分支已 Wave 2A-A3 hard remove, 不再是潜伏激活面.)
4. **permissions 类 P0-PERM-5 行为变化**: `getNextPermissionMode.ts:41`
   Shift+Tab 模式跳转矩阵中 external 走 acceptEdits → plan → bypass → auto,
   mossen 用户的键盘操作语义变化.
5. **withRetry.ts:354 立即触发** (这是想要的, 但应单独验证 — 已通过
   NEEDS-DESIGN-API-001 单点 slice 完成).
6. **测试基础设施可能依赖 USER_TYPE 不被 force**: smoke 测试 / harness 中
   的 env_override 可能假设 USER_TYPE 来自调用者, 强制 export 可能让某些
   测试 fixture 失效.
7. **upstream merge 冲突**: 任何 anthropic 上游对 USER_TYPE 设置逻辑的
   改动, 都会与 `run-mossen.sh` export 冲突.

## 4. 候选方案

### 方案 A — 立即在 run-mossen.sh 加 export USER_TYPE=external (不推荐)

- **做什么**: 在 `run-mossen.sh` 头部加 1 行 `export USER_TYPE=external`
- **优点**: 1 行改动
- **缺点**: 上述 7 项弊端全部一次性触发, 风险面巨大, 无法 slice 验证

### 方案 B — 逐步用 getUserType() 替代直接读 env (本设计推荐)

- **思路**: 不改 entry-point, 改读取方式
- **步骤**:
  1. **本 Wave 0 内**: NEEDS-DESIGN-API-001 (`withRetry.ts:354`) 是第一个
     示范 — `process.env.USER_TYPE` → `getUserType()` (已完成).
  2. **Wave 2 同步**: 把 17 P0 中所有 `process.env.USER_TYPE === 'external'` /
     `!== 'external'` 比较 (entry-point.md §I.1 已识别但未列具体位置) 全部
     用 `getUserType()` 替代, 让 mossen 默认 fallback 到 'external' 的语义
     在所有"应该"的位置激活.
  3. **Wave 3 选做**: `process.env.USER_TYPE === 'ant'` / `=== 'mossen'`
     的比较保留直接读 env (因为这两类是用户主动 opt-in 的"内部模式"语义,
     不应 fallback).
- **优点**:
  - 不改全局 env, 不破坏 process.env 读取语义
  - 每处迁移都可单独验证
  - 与上游 anthropic 行为差异最小
- **缺点**:
  - ~330 处直接读 env 的 gate, 全部识别 + 评估"该用 getUserType() 还是
    该保留" 工作量大
  - **不解决潜伏激活面** (用户主动 export USER_TYPE=ant 仍激活 ant gate)

### 方案 C — 在 cli.tsx 启动 banner 加显式提示 (备选)

- **做什么**: 启动时检查 `process.env.USER_TYPE` 若为 `'ant'` 或 `'mossen'`,
  显示警告 "USER_TYPE=X detected; this enables internal-only behaviors.
  Unset env if not intended."
- **优点**: 不改 gate 行为, 仅可见性
- **缺点**: 用户可能忽略警告; 仍无法防误激活

## 5. Wave 0 决策 (Allen 拍板)

**仅推进方案 B 的第一个示范点 (NEEDS-DESIGN-API-001)**, 不动 entry-point.
全仓 getUserType() 替代留 Wave 2 评估.

理由:
- 方案 A 风险过大, 一次性触发 7 项弊端
- 方案 B 可 slice 验证, 已通过 NEEDS-DESIGN-API-001 单点完成第一个示范
- 方案 C 不解决根本问题, 后续可作为 Wave 3 UX 改进

## 6. Wave 2 评估必含

如 Allen 后续推进方案 B 全仓迁移, Wave 2 设计单必须包含:

1. **全仓 grep**: `process.env.USER_TYPE === 'external'` /
   `process.env.USER_TYPE !== 'external'` 全部位置清单
2. **每处独立评估**: 该比较是否应改为 `getUserType()` (即 mossen 默认应走
   'external' 路径), 或保留直接读 env (即此比较的语义本就要求"显式 ant 才
   触发某行为, 默认 undefined 不应当成 external")
3. **行为差异表**: 每处迁移前/后, mossen 默认用户感知差异 (含 analytics /
   permission 模式 / fast mode / fallback 逻辑等)
4. **回滚策略**: 全仓改造的 slice 拆分 + 每 slice 独立锚点

## 7. 不在本设计范围

- 用户主动 `export USER_TYPE=ant` 的潜伏激活面
  → 留 Wave 2 单独施工包 (按 00-summary §5.2.2 5 条 ant 路径分别评估)
- 用户主动 `export USER_TYPE=mossen` 的潜伏激活面
  → 留 Wave 2 单独施工包 (按 00-summary §5.2.1 15 条 mossen 路径分别评估)
- .tsx 9 处 inline 死代码物理删除
  → 留 Wave 3 (与品牌中性化合并)

## 8. Wave 7 Door Lock 实施 [`def26fd`] (2026-04-30)

Wave 0 方案 A 弊端 1+2+6+7 通过"shell 层条件化 + JS 兜底"组合得到大幅缓解；
Wave 0 方案 B 渐进迁移仍持续推进 (Wave 6/7 已收口 7+ 处 ant runtime gate)。

### 8.1 为什么需要 Door Lock

1. `USER_TYPE=ant` / `USER_TYPE=mossen` 是**内部 / 预留**运行模式开关，
   不是 mossen 公开版用户应当能触达的语义。
2. 公开版不能信任用户 shell 残留的 `export USER_TYPE=ant` /
   `export USER_TYPE=mossen` (来自 anthropic 上游教程、其它工具、
   shell rc 复制粘贴等)。
3. **仅在 `entrypoints/cli.tsx` 加 JS sanitizer 不够**，因为
   ESM import hoisting 规范：所有 import 在 statement 之前评估完毕。
   `tools.ts:16-19` 是 ant-only **top-level conditional require**：
   ```ts
   const REPLTool =
     process.env.USER_TYPE === 'ant'
       ? require('./tools/REPLTool/REPLTool.js').REPLTool
       : null
   ```
   这种 require 在 `cli.tsx` 第 4 行 `applyUserTypeRuntimeLock()` statement
   执行**之前**就触发。`commands.ts:49-52` 的 `agentsPlatform` 是同型 pattern。
4. **实测旧失败**：在 Wave6 + Wave7 早期 commit (`8fd3bb6`) 状态下，
   ```
   USER_TYPE=ant bun run help
   → Cannot find module './tools/REPLTool/REPLTool.js' from tools.ts
   ```
   原因正是 ESM hoisting 时 sanitizer 未运行 + REPLTool.js 不存在于 mossen 个人版。
5. 修复方式：在 **Bun 启动前** 由 shell 完成 USER_TYPE 归一化，
   以保证 tools.ts top-level require 看到 `external`。

### 8.2 Door Lock 当前方式（三层协作）

| 层级 | 文件 | 角色 |
|---|---|---|
| Shell 层 | `run-bun-featured.sh:119-144` | 仅当真实 CLI 入口 (`entrypoints/cli.tsx`) 时 export USER_TYPE，覆盖 Bun module loading 之前的窗口 |
| JS 入口兜底 | `entrypoints/cli.tsx:3-4` | side-effect import + `applyUserTypeRuntimeLock()` 二次归一化 |
| 兜底叶子 | `utils/userTypeRuntimeLock.ts` | zero-dep `normalizeUserType` / `isInternalUserTypeUnlocked` / `applyUserTypeRuntimeLock` |
| `getUserType()` 路径 | `utils/userType.ts` | `getUserType()` 走 `normalizeUserType` —— SDK / test / mcp 等绕开 cli.tsx 的调用方也被规范化 |

#### 8.2.1 Shell 层 CLI-only 触发条件

```bash
if [[ ${#exec_args[@]} -gt 0 ]] && {
  [[ "${exec_args[0]}" == "entrypoints/cli.tsx" ]] ||
  [[ "${exec_args[0]}" == "$ROOT_DIR/entrypoints/cli.tsx" ]]
}; then
  # ...export 锁定 USER_TYPE...
fi
```

**不锁** `bun -e` / `--eval` / 其它 entry，避免破坏
`scripts/error_boundary_usertype_gate_smoke.py` 这类需要测试 raw `USER_TYPE=ant`
真行为的 smoke。这是有意保留的测试兼容例外。

#### 8.2.2 规则（与 `utils/userTypeRuntimeLock.ts` 等价）

| 输入 USER_TYPE | unlock | 输出 |
|---|---|---|
| unset / empty | * | external |
| `external` | * | external |
| `ant` | (unset / 非 `1`) | external |
| `ant` | `1` | ant |
| `mossen` | (unset / 非 `1`) | external |
| `mossen` | `1` | mossen |
| 其它未知值 | * | external |

unlock 严格匹配 `MOSSEN_CODE_ALLOW_INTERNAL_USER_TYPE === '1'`，
避免 `'0'` / `'false'` / `'true'` 误判。

#### 8.2.3 unlock path 行为

`USER_TYPE=ant MOSSEN_CODE_ALLOW_INTERNAL_USER_TYPE=1 bun run help`
仍可能因为缺失 `REPLTool.js` 等内部模块而失败 ——
**这是预期行为：unlock path reaches internal tooling as expected**，
不算 public lock 失败。Mossen 公开版不发布 REPLTool 等内部 tool。

### 8.3 后续清理 TODO

1. **继续清理剩余直接 `process.env.USER_TYPE` inline**，特别是：
   - top-level / boot-time 路径（boot 链上的 conditional require / array spread）
   - DCE require 相关点（构建期决策）
   不能一次性大改，按 slice 推进。
2. **module-local helper 是临时折中**（W6-6 / W6-7 / W7-C 在
   `commands/context/context-noninteractive.ts` / `commands/cost/index.ts` /
   `commands/feedback/index.ts` / `commands/commit.ts` /
   `commands/commit-push-pr.ts` 末尾追加 `is*InternalUser()`），
   原因是 i18n allowlist baseline 行号不能漂移。
   如果未来 allowlist 机制升级（按 hash 而非 line），统一回 `utils/userType.ts`。
3. **`USER_TYPE === 'mossen'` 19 处属于预留模式体系**，
   分布在 14 文件（BashTool / SkillTool / AgentTool / TaskStopTool /
   FileEditTool / ToolSearchTool / REPLTool / WebFetchTool /
   EnterPlanModeTool / ConfigTool / bashPermissions / shouldUseSandbox 等），
   需 Wave 8+ 单独审计施工包，**不要零散修改**（参见 W7-H 调研报告
   `Wave7-W7H-BashTool-prompt-mossen-来源核实.md`）。
4. **`bun -e` 不被 shell 锁是测试兼容例外**。
   未来如有更好的 test unlock 机制（如 smoke 显式
   `MOSSEN_CODE_ALLOW_INTERNAL_USER_TYPE=1` + 标准 fixture），
   可考虑收紧并废除该例外。
5. **不要为了让测试变绿而修改 harness/smoke 判定逻辑**。
   测试断言应反映 Door Lock 的实际语义，而不是被 lock 绕过。

### 8.4 明确禁止

- ❌ 不要因为 Door Lock 改 harness/smoke/scripts/allowlist。
- ❌ 不要删除 `MOSSEN_CODE_ALLOW_INTERNAL_USER_TYPE=1` 解锁口
  （内部 / 企业 / 受控测试场景仍需要它）。
- ❌ 不要一次性大改所有 USER_TYPE inline（按 slice + 验证矩阵推进）。
- ❌ 不要改 `tools.ts` 的 top-level DCE require / `commands.ts:50`
  agentsPlatform conditional require / INTERNAL_ONLY_COMMANDS，
  除非有单独设计包 + 完整 5 维度调研。
- ❌ 不要回滚 Door Lock public 部分以让 unlock path（含
  `USER_TYPE=ant MOSSEN_CODE_ALLOW_INTERNAL_USER_TYPE=1`）能完整启动 ——
  unlock path 的 internal tooling 缺失是预期。

### 8.5 Wave7 实施 commit 链

| commit | 内容 |
|---|---|
| `f97c0a9` | W7-B internal command spread USER_TYPE gate |
| `10b11a9` | W7-C commit command USER_TYPE gates (module-local helper) |
| `4f4582d` | W7-D attribution USER_TYPE gates |
| `5742ac6` | W7-E remove unused query internal gate |
| `e5c30ce` | W7-A issue flag banner USER_TYPE gate |
| `8fd3bb6` | Door Lock public USER_TYPE runtime mode (JS layer) |
| **`def26fd`** | **Door Lock shell entrypoint timing fix（本节实施 commit）** |

### 8.6 Wave7 后核心验证矩阵

- `USER_TYPE=ant bun run help` → PASS（无 `MOSSEN INTERNAL`）
- `USER_TYPE=mossen bun run help` → PASS
- `USER_TYPE=weird bun run help` → PASS
- `USER_TYPE=ant ./run-bun-featured.sh -e 'console.log(process.env.USER_TYPE)'`
  → 输出 `ant`（bun -e 不锁）
- `python3 scripts/error_boundary_usertype_gate_smoke.py` → 3/3 PASS
- `bash scripts/run_all_smoke.sh` → ALL PASS + case 39 fingerprint
  `870f99ed494d3d145ed2eb1368132299` 稳定
