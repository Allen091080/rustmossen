# W52 第一档调研归档报告

> 生成日期：2026-05-03
> HEAD：`0f2c660`（与 sprint 起点 tag `pre-upgrade-sprint-20260503-1925` 一致，未动）
> 输入计划：`/Users/allen/Desktop/mossen官方升级点/第一档_立即做_升级计划.md` 13 项
> 本轮改动：**0 文件 / 0 commit / 0 smoke / 0 tag**（纯只读 grep / cat / wc）
> 性质：**Wave 0 调研归档**——按"动代码前先产可行性表"规则，本轮只判定不施工

---

## 0. 一句话总结

第一档 13 项里 **9 项 N/A 归档** + **2 项 STOP 等触发** + **1 项品牌收口（非阻塞，另行执行）** + **1 项需要施工**（#10 Named Plan Files）。#10 属于核心代码改动（影响 plan 持久化命名 + worktree 命名 + `/resume` 检索），按规则需 Allen 先 confirm 方案再写代码。本文档第二节就是该方案。

---

## 1. 13 项判定与归档证据

### 1.1 N/A 直接归档（6 项 / 占 46%）

| # | 项 | 判定理由 + grep 证据 |
|---|---|---|
| 3 | Stream idle timeout 休眠唤醒修复 | **N/A**：`utils/idleTimeout.ts:38` 已用 `Date.now() - lastIdleTime` wallclock-based 增量比较，非 setTimeout 累加，休眠不会误触发。无需任何修复 |
| 5 | CJK 字符渲染宽度修复 | **N/A**：`ink/stringWidth.ts` 222 行已用 `emoji-regex` + `get-east-asian-width` + `getGraphemeSegmenter` + Bun.stringWidth fast path，与官方 v2.1.126 修复同源方案 |
| 6 | Trackpad 滚动跳跃修复 | **N/A**：`components/ScrollKeybindingHandler.tsx` 含 WHEEL_ACCEL 窗口、bounce 防抖、wheel-mode 指数衰减、设备切换识别（WHEEL_MODE_IDLE_DISENGAGE_MS=1500）、device discrete 检测注释，**比官方 v2.1.116 修复深得多** |
| 9 | Auto 主题（终端深浅色检测）| **N/A**：`utils/systemTheme.ts` 119 行已实现 `COLORFGBG`（同步初猜）+ OSC 11 探测（异步精确）；`components/ThemePicker.tsx:119` 已有 `value: "auto"` 选项 |
| 11 | MCP `alwaysLoad` 配置项 | **N/A**：mossen 已实现且更精细——通过 `_meta['mossen/alwaysLoad']` 命名空间。`entrypoints/agentSdkTypes.ts:84` declare、`services/mcp/client.ts:1783` 读取、`Tool.ts:445` 接入、`tools/ToolSearchTool/prompt.ts:62` 跳过 deferred 判定，全链路就位 |
| 12 | MCP 结果大小可配置 | **N/A**：mossen 已用 `Tool.ts:465 maxResultSizeChars` per-tool 字段（BashTool 30k / 多数 tool 100k / FileReadTool Infinity），**比官方 v2.1.114 全局 500KB 单值方案更精细** |

### 1.2 N/A 倾向 / 需轻量复核（2 项 / 占 15%）

| # | 项 | 判定理由 + 触发条件 |
|---|---|---|
| 2 | Bash 工具 fd 耗尽修复 | **N/A 倾向**：`utils/ShellCommand.ts` 已有 `cleanup()` / `kill()` / `#cleanupListeners()` / `treeKill` + 用 `'exit'` 而非 `'close'`（注释明确："Use 'exit' not 'close': 'close' waits for stdio to close"）等 idiomatic fd 管理。**触发复核条件**：用户实际反馈 fd 耗尽症状（"too many open files"）后再做 stress test 验证 |
| 7 | 长 URL 点击区域修复 | **N/A 倾向**：`ink/supports-hyperlinks.ts` 检测 + `ink/output.ts` per-grapheme `hyperlink` 字段存储（行 42 / 595 / 600）+ `ink/selection.ts isUrlChar` 选择，已实现 OSC 8 跨行不切断。**触发复核条件**：用户反馈"点击长 URL 跳错地方"再人眼验证 |

### 1.3 STOP / 等触发后再决策（2 项 / 占 15%）

| # | 项 | STOP 理由 + 触发条件 |
|---|---|---|
| 1 | Clipboard 安全漏洞修复评估 | **STOP**：mossen 用 OSC 52（ANSI 转义直送终端）+ pbcopy（macOS 标准管道），都是不落盘的终端原生通道，与"内容意外被 EDR/SIEM 抓"的常见诱因（落盘临时文件、剪贴板历史扩展）路径不同。无 CVE 编号或具体 patch hash，盲查盲改风险大于收益。证据：5 处 `setClipboard` 写入（`ink/termio/osc.ts:138` + `screens/REPL.tsx:3693` + `ink/ink.tsx:1022` + `screens/ResumeConversation.tsx:185` + `components/ConsoleOAuthFlow.tsx:159`）。**触发条件**：拿到具体 CVE/patch 信息或安全反馈后再激活 |
| 4 | 图片处理内存泄漏修复 | **STOP**：mossen 图片处理用 base64 字符串持久化（`utils/imageStore.ts` 167 行），`Buffer.from(...base64)` 调用都是局部短生命周期转换，没有 setInterval/全局缓存/事件监听未清的典型泄漏模式。代码量：`imagePaste.ts` 416 行 / `imageStore.ts` 167 / `imageResizer.ts` 880 / `screenshotClipboard.ts` 121。Heap snapshot 是真功夫工程（构造长会话 + DevTools profiling + 多轮对比），价值小。**触发条件**：用户实际反馈 OOM 或长会话内存监控异常后再做 |

### 1.4 半 N/A / 命名差异归档（1 项 / 占 8%）

| # | 项 | 归档理由 |
|---|---|---|
| 8 | xhigh effort 参数支持 | **N/A**：mossen `utils/effort.ts` EffortLevel 已是 `low/medium/high/max` + `auto`，**`max` 在语义上等同上游 `xhigh`**（最高推理深度）。注释明确："`max` is Opus 4.6 only for public models"。加 `xhigh` alias 会引入两个名字指同一个东西的模糊性，需在 i18n / help / 错误 / 补全 / settings 持久化多处维护两套术语，违反"100% real or not done"。**结论：命名差异不构成功能 gap，永久 N/A** |

### 1.5 品牌收口（1 项 / 占 8%，非阻塞另行执行）

| # | 项 | 归档理由 |
|---|---|---|
| 13 | 品牌 P0 22 处必改 | **品牌核心迁移已完成**：运行时 loader (`utils/naming.ts:4-6` 常量 `MOSSEN.md` / `.mossen` / `MOSSEN_CONFIG_DIR`)、memory 加载链 (`utils/mossenmd.ts` 头注释完全围绕 `MOSSEN.md / ~/.mossen / .mossen/MOSSEN.md / .mossen/rules/*.md` 工作)、`/init` 命令写入目标 (`commands/init.ts` 中 `OLD_INIT_PROMPT` 与 `NEW_INIT_PROMPT` 都是 "create a MOSSEN.md file") 已迁。用户配置主路径 `~/.mossen/` 与 `MOSSEN.md` 已是 canonical。<br><br>**但仍有非运行时残留待收口**：（a）blocked protocol/manifest/schema 的描述文案——`cli/print.ts:4549` "Initialize CLAUDE.md"、`services/slashCommandCapabilities.ts:512,528` `title: 'CLAUDE.md initializer'` + reason "init writes CLAUDE.md..."、`entrypoints/sdk/controlSchemas.ts:803` schema description 含 "(CLAUDE.md, .mossen/rules/*)"——这些应改为 `MOSSEN.md / project memory files`；（b）docs 示例：`docs/W_MAIN_*.md` 多处含 `CLAUDE.md` 路径示例与 `claude-opus-4-7` 模型名样本、`docs/reference/red-lines.md:80` 含 `~/.claude/projects/...` 路径；（c）harness 测试假名：`scripts/harness_M9_12_*.py:151 "claude-fake"` 与 `scripts/harness_M12_1_*.py:42 "claude-sonnet-4-5-mossen-test"`（test placeholder 是否需要改名待评估）；（d）历史 sprint 文档：`20260425升级.md` 含历史 `~/.claude/` 引用。<br><br>**结论**：**非阻塞品牌收口项，另行执行**；不阻塞 W52 #10 Named Plan Files 推进。本轮不施工，不在 #10 方案范围。后续单独立项时按"manifest/schema/dispatcher 文案 → docs 示例 → harness 假名"分批，仍受协议层 5 处同改约束（涉及 `services/slashCommandCapabilities.ts` 与 `entrypoints/sdk/controlSchemas.ts` 必须同 commit 跑 protocol smoke）。**`CLAUDE.md` 不再描述为"有意保留"——manifest/schema/dispatcher 文案残留是工程债，docs 示例残留是历史债。** |

### 1.6 真实 gap / 需施工（1 项 / 占 8%）

| # | 项 | 简述 |
|---|---|---|
| 10 | Named Plan Files | **REAL gap**：`utils/plans.ts:40` 用 `generateWordSlug()` 随机词命名（如 `purple-tiger-7.plan.md`），不是 prompt-derived 语义命名（如 `refactor-auth.plan.md`）。**详细方案见第 2 节** |

---

## 2. #10 Named Plan Files 方案

### 2.1 现状摘要

mossen 当前 plan 文件命名链路：

1. 首次写 plan 时调用 `getPlanSlug()`（`utils/plans.ts:32`）
2. 无 cache 时调用 `generateWordSlug()`（`utils/words.ts:785`）→ 形如 `purple-tiger-7`
3. 重名时 `MAX_SLUG_RETRIES = 10` 重新生成完全不同的 word-slug
4. slug 写入 `bootstrap/state.ts:152 planSlugCache: Map<sessionId, slug>`
5. slug 在 `utils/sessionStorage.ts:1023-1065` 持久化到 `SerializedMessage.slug`（`types/logs.ts:16`）
6. resume 时 `getSlugFromLog(log)` 从 message.slug 字段恢复
7. `utils/plans.ts:120 getPlanFilePath()` 拼出 `${slug}.md`（main）或 `${slug}-agent-${agentId}.md`（subagent）

### 2.2 读写点完整清单（10 处）

| # | 文件 | 角色 | 影响 |
|---|---|---|---|
| 1 | `utils/plans.ts:32 getPlanSlug()` | **slug 生成器**（核心入口）| **唯一需要改的源头** |
| 2 | `utils/plans.ts:54 setPlanSlug()` | cache write | 不动 |
| 3 | `utils/plans.ts:64 clearPlanSlug()` / `:71 clearAllPlanSlugs()` | cache invalidate | 不动 |
| 4 | `utils/plans.ts:79 getPlansDirectory()` | 目录 | 不动 |
| 5 | `utils/plans.ts:120 getPlanFilePath()` | 拼路径 `${slug}.md` | 不动（拼接前已是新 slug）|
| 6 | `utils/plans.ts:149 getSlugFromLog()` + `:164 copyPlanForResume()` + `:239 copyPlanForFork()` | resume 兼容 | **需考虑：旧 word-slug session resume 必须正常**|
| 7 | `bootstrap/state.ts:152 planSlugCache` + `:1330 getPlanSlugCache()` | session cache | 不动 |
| 8 | `utils/sessionStorage.ts:1025 / :1065` | 持久化到 SerializedMessage.slug | 不动（字段语义不变）|
| 9 | `tools/EnterWorktreeTool/EnterWorktreeTool.ts:90 const slug = input.name ?? getPlanSlug()` | **worktree 名字依赖 plan slug**！| **需考虑：slug 必须是 git branch 名 + 文件系统路径双 safe** |
| 10 | `setup.ts:194 worktreeName ?? getPlanSlug()` | worktree 名字回退 | 同上 |

**关键发现**：plan slug 同时被用作 **git worktree 名 + git branch 名 + plan 文件名**，命名规则约束面比想象大。

### 2.3 已有可复用基础设施

mossen 已有 `commands/rename/generateSessionName.ts:42 generateSessionName()`，通过 Haiku LLM 从 conversation 生成 kebab-case 命名（system prompt: `'Generate a short kebab-case name (2-4 words)... Examples: "fix-login-bug", "add-auth-feature"...'`）。**这正是官方 Named Plan Files 同类方案**。

但 `generateSessionName` 需要 conversation 文本作输入，**不能在首次 prompt 时直接用**——首次 plan slug 生成时还没"对话"可摘要。

### 2.4 方案对比

| 方案 | 描述 | 优点 | 缺点 |
|---|---|---|---|
| A | **同步 LLM 派生**：首 prompt 时同步调用 Haiku 生成 slug | 命名质量最高 | 增加首次 plan 写入延迟（~500ms-2s）；网络失败需 fallback |
| B | **异步 LLM 派生 + 后台 rename**：先用 word-slug，Haiku 返回后 rename 文件 | 不阻塞首次 | rename 涉及 race（worktree 已建、文件已写、log 中 slug 已持久化），高复杂度 |
| C | **纯本地 slugify（保留中文）**：从首 prompt 截断 + lowercase + kebab-case + 保留 CJK | 零延迟、零外部依赖 | **中文 slug 用作 git branch / git worktree 路径在某些 IDE / 第三方 git 工具下不友好；CJK 路径在跨平台/CI 环境潜在风险** |
| C' | **ASCII-safe prompt slug v0**：纯本地 slugify，强制只允许 `a-z 0-9 -`，CJK / 非 ASCII 字符不进 slug，失败 fallback 到 `generateWordSlug()` | 零延迟、零外部依赖、**worktree/branch/file 三处统一 ASCII safe**；纯加法、零数据迁移 | 中文 prompt 在 v0 里直接走 word-slug fallback（中文用户暂时拿不到 prompt-derived 命名，但保留 worktree/branch 安全）|
| D | **混合**：英文 prompt 走本地 slugify；中文 prompt fallback 到 word-slug；可选异步 Haiku 派生作为 v2 | 平衡复杂度与价值 | 仍需做异步 rename（如启用 v2）|

### 2.5 推荐方案：**C' — ASCII-safe prompt slug v0 + word-slug fallback**

**为什么改成 C' 而不是 C**：
- plan slug 同时被用作 **plan 文件名 + git worktree 路径 + git branch 名**（见 §2.2 #9 / #10），三处共用同一个 slug
- CJK 字符做 git branch 名 / git worktree 路径在 macOS/Linux FS 层面 OK，但**对部分 git 工具、IDE 集成、跨平台同步、CI runner 仍是兼容陷阱**
- v0 必须保证 worktree/branch/file 三处统一 ASCII safe，宁愿中文 prompt 暂时 fallback 到 word-slug，也不冒险把 CJK 放进 git ref
- v0 不是 LLM semantic naming，是**"比随机词更可读，同时保证 worktree/branch/file safe"**的简单升级
- 中文语义命名留作未来可选的 v2（display title 与 slug 解耦），本轮不做

**为什么不选 A / B / D**：
- B 异步 rename 必须改 `setPlanSlug` + 持久化 + worktree 重建，是大改造，不是"小修复"
- D 引入"中英文不同行为"的双逻辑分支但 ASCII 安全没收紧，仍有 CJK 进 git ref 风险
- A 增加首次延迟（mossen 用户对延迟敏感，且 Haiku 失败处理麻烦）

**C' 方案细节**：

```
新函数 generatePromptSlug(prompt: string): string | null
  1. 取 prompt 前 80 字符
  2. 转小写
  3. 去除 ANSI 转义序列
  4. 去除 Markdown 修饰符（# * _ ` ~ > 等）
  5. 去除 emoji（用 emoji-regex）
  6. 去除路径危险字符（/ \ : ? * < > | " 等）
  7. 非 ASCII 字符（CJK / 全角标点等）替换为单个 `-`，不保留
  8. 空白和剩余标点归一成单个 `-`
  9. 连续 `-` 压缩为单个 `-`
  10. 去掉首尾 `-`
  11. 截断至 48 字符（保留 -2/-3 后缀空间）
  12. 若结果为空 / 长度 < 2 / 全是连字符 / 非 ASCII 内容占比过高，返回 null
  13. 必须通过 validateWorktreeSlug() 校验，失败返回 null

修改 getPlanSlug(sessionId?, options?: { firstUserPrompt?: string }):
  cache miss 时：
    if (options?.firstUserPrompt) {
      const promptSlug = generatePromptSlug(options.firstUserPrompt)
      if (promptSlug) {
        // 重名时加 -2 / -3 / ... 直至 MAX_SLUG_RETRIES
        // 重名 NOT 重新派生，只加数字后缀（避免命名漂移）
        return findUniqueSlugWithSuffix(promptSlug)
      }
    }
    // fallback：保持当前行为
    return findUniqueWordSlug()  // 当前 generateWordSlug + retry，行为 100% 不变
```

**约束清单（必须全部满足）**：
1. slug 字符集严格限定 `a-z 0-9 -`（ASCII-safe）
2. 必须通过 `validateWorktreeSlug()`（`utils/worktree.ts:67`），未通过即 fallback
3. 重名只加 `-2 / -3` 数字后缀，**不重新生成完全不同的随机词**
4. 旧 session slug 不迁移（保留 `purple-tiger-7.plan.md` 文件原状）
5. 不做异步 rename
6. 不改 worktree 逻辑（`tools/EnterWorktreeTool/EnterWorktreeTool.ts` 不动）
7. 不碰 query loop / REPL 主流程（除非 Slice 3 单独再 confirm）
8. 中文 prompt 在 v0 里允许 fallback 到 word-slug
9. `copyPlanForResume` / `copyPlanForFork` 不调 `generatePromptSlug`（resume/fork 必须沿用 log 中已有 slug，避免破坏既有 worktree）
10. 未来若需要中文语义，另加 display title 字段（与 slug 解耦），不把中文放进 slug / branch / worktree 名

### 2.6 兼容性保证

旧 session resume 必须正常：
- `getSlugFromLog(log)` 从 `message.slug` 读出原 slug（可能是旧 word-slug 也可能是新 prompt-slug）
- `copyPlanForResume` 直接用从 log 读出的 slug，**不重新生成**
- 老的 `purple-tiger-7.plan.md` 文件继续被旧 session 读写
- **零数据迁移、零破坏性变更**

### 2.7 Slice 划分

| Slice | 内容 | 改动文件 | 性质 | 测试 |
|---|---|---|---|---|
| 1 | **只新增纯函数与测试，不接入运行时**：`generatePromptSlug` + `findUniqueSlugWithSuffix` 工具函数 + 单测（英文 / 中文 fallback / emoji / Markdown / 路径危险字符 / 超长 / 空 / 全标点 / `validateWorktreeSlug` 失败回退 等 case）| `utils/plans.ts` 加新函数 + `utils/__tests__/plans.test.ts`（新建）| **纯加法、无运行时调用** | 新单测全 PASS |
| 2 | **只改 `getPlanSlug` 支持可选 prompt 参数；fallback 保持原行为**：签名加 `options?.firstUserPrompt`；options 缺省时行为与当前 100% 一致；options 提供时走 §2.5 流程，失败仍回退到 `findUniqueWordSlug` | `utils/plans.ts:32` | **加可选参数、不改默认路径** | 新单测覆盖：缺省调用 = 当前行为；提供 prompt = 新路径；prompt 派生失败 = fallback 到 word-slug |
| 3 | **先定位真实 plan 创建调用点，输出小方案，Allen 二次 confirm 后再接入**：本 slice **第一步只调研 grep 出所有 `getPlanSlug()` 调用点**（`screens/REPL.tsx` / `commands/plan/*` / `EnterWorktreeTool` / `setup.ts`），输出小方案：哪一处是"首次 plan 创建"边界、首 user prompt 怎么传入、不影响 query loop / REPL 主流程；**等 Allen 拍板这份小方案后再写代码** | 调研阶段 0 改动；接入阶段视方案而定 | **核心代码、必须单独二次 confirm** | 接入后跑 harness M1.7 plan mode 不退化 |
| 4 | **新增 smoke 锁住契约**：`scripts/wave_w52_named_plan_files_smoke.py` | + 1 文件 | 新加 smoke | smoke PASS |

### 2.8 Smoke 设计（Slice 4）

`scripts/wave_w52_named_plan_files_smoke.py` 静态规则验证（grep + AST）：

1. `utils/plans.ts` 中 `generatePromptSlug` 函数存在且 export
2. `generatePromptSlug` 输出强制 ASCII-safe（grep 函数体内含字符集白名单 `[a-z0-9-]` 或等价正则）
3. `generatePromptSlug` 末尾调用 `validateWorktreeSlug`（防 git branch / 文件名失败）
4. `getPlanSlug` 签名包含可选 `firstUserPrompt`
5. fallback 到 `generateWordSlug` 路径仍存在（旧逻辑未删）
6. `copyPlanForResume` / `copyPlanForFork` 不调 `generatePromptSlug`（resume/fork 不重新生成 slug，沿用 log 中已有 slug）
7. 新单测文件存在：`utils/__tests__/plans.test.ts`
8. 旧 session slug 兼容路径未变（grep `getSlugFromLog` 实现保持原样）

### 2.9 风险与回滚

**风险**：
- Slice 3 调用点穿透涉及找首 user message → 主循环上下文，**核心代码**，必须先单独征求 Allen 二次 confirm（已经在 Slice 划分里写明）
- v0 不保留 CJK，中文 prompt 在 v0 直接 fallback 到 word-slug —— 文档预期管理：**这是有意为之的兼容性收紧**，未来 v2 通过 display title / slug 解耦再补
- 重名 `-2 -3` 在并发首次写入时可能 race → 已有 MAX_SLUG_RETRIES 框架可复用，但仍需原子化检测

**回滚**：
- 单 commit 反转。Slice 1+2 是纯加法（旧 fallback 路径未删），删除 commit 即回到当前行为
- 不写 migration、不动持久化、不改 schema → **零数据风险**
- Slice 3 接入后若发现问题，单独反转 Slice 3 commit，Slice 1+2 留下也不会触发新行为（因为没人调）

### 2.10 等 Allen confirm 的具体决策点

请你回复以下 3 件事：

1. **方案选择**：是否同意推荐方案 **C' — ASCII-safe prompt slug v0 + word-slug fallback**？
2. **Slice 3 节奏**：你想我做完 Slice 1+2 后**再单独给你看 Slice 3 调用点小方案**（推荐），还是现在就一起方案？
3. **Slice 4 新单测**：是否同意新建 `utils/__tests__/plans.test.ts`（mossen 当前无此单测，纯加法无破坏）？

---

### 2.11 Slice 3 调研结论：暂不接入运行时（2026-05-03 决议）

Slice 3 调研报告（详见对话归档）已交付，本节落档最终决策。

#### 1. 最终决策

- **Slice 3 暂停**，不接入运行时。
- **W52 收口为 Slice 1/2**：纯函数 `generatePromptPlanSlug` + `getPlanSlug(..., { firstUserPrompt })` 可选参数底座。
- 当前默认行为不变；现有 5 个 `getPlanSlug()` 调用点全部继续走 word-slug fallback。

#### 2. 原因

- 真正的"plan slug 首次缓存写入"边界出现在 `utils/attachments.ts:1145 getPlanModeAttachments`，由其内部 `getPlanFilePath(toolUseContext.agentId)` 第一次触发 `getPlanSlug(getSessionId())` 完成 cache miss → cache.set。
- 该函数的调用链来自 `query.ts:1611` 与 `utils/processUserInput/processUserInput.ts:476` 的 `getAttachmentMessages`，再上游就是 `screens/REPL.tsx` / `QueryEngine`。这些路径属于核心主流程，本阶段被 §2.7 Slice 3 boundary 约束明确禁动。
- `tools/ExitPlanModeTool/ExitPlanModeV2Tool.ts:246` 的 `call()` 阶段太晚——进入 plan_mode 第一帧时 `getPlanModeAttachments` 已先行执行，slug 已被 word-slug 写入 cache。在 exit plan 阶段再用 `setPlanSlug` 覆盖会造成：
  - **双 plan 路径**：旧 word-slug 文件遗留磁盘 + 新 prompt-slug 文件并存；
  - **孤儿文件**：attachments re-entry 分支可能仍持旧路径引用；
  - **UI/磁盘路径不一致**：worktree 路径 / branch 名 / plan 文件名三方失配，破坏 §2.5 C′ 的核心保证。
- `commands/plan/plan.tsx:82` 的 `/plan` 命令分支只覆盖**用户显式敲 `/plan`** 的小子集（估计 < 20%）。绝大多数 plan_mode 由 Tab toggle / permissionMode 切换触发，不走该路径。收益低且语义不完整。

#### 3. 不接入清单（W52 范围内不动）

- ❌ 不改 `screens/REPL.tsx`
- ❌ 不改 `query.ts` 主循环
- ❌ 不改 `utils/processUserInput/**`
- ❌ 不改 `utils/attachments.ts`（`getPlanModeAttachments` 不动）
- ❌ 不改 `tools/EnterWorktreeTool/EnterWorktreeTool.ts`
- ❌ 不改 `tools/ExitPlanModeTool/ExitPlanModeV2Tool.ts`
- ❌ 不改 `setup.ts`
- ❌ 不改 `utils/plans.ts copyPlanForResume / copyPlanForFork`（resume / fork 不重新生成 slug）
- ❌ 不改 `utils/permissions/filesystem.ts isSessionPlanFile`

#### 4. 后续条件（启动新 Wave 才能解锁）

如果未来要继续做 Named Plan Files runtime 接入，必须**单独开新 Wave**，且新 Wave 必须先满足：

1. 重新设计"plan 首次创建边界"——明确在哪一行、由谁、用什么数据 prime `getPlanSlugCache`，不能晚于 `getPlanModeAttachments` 第一帧。
2. **三方一致性强保证**：`plans/{slug}.md` 文件名、git worktree 路径、git branch 名必须由同一来源生成、同时落地，slug 一旦定就不能更换。
3. **resume / fork 不重新生成 slug**：旧 session 一律读源 log 中的 slug（与 Slice 1/2 已锁定的 smoke 一致）。
4. **不引入异步 rename**：禁止 `fs.rename` / `renameSync` 修复历史 slug，符合 W52 §2.5 C' "no-rename design" 红线。
5. **解禁 REPL/query/processUserInput 至少一处**：必须在新 Wave 范围内显式列出动哪个核心入口，并经 Allen 二次拍板。

#### 5. 当前 W52 状态

- ✅ Slice 1（纯函数 `generatePromptPlanSlug`）已完成：ASCII-safe / 三方安全 / CJK 回退 word-slug。
- ✅ Slice 2（`getPlanSlug` 可选 `firstUserPrompt`）已完成：默认调用 shape 100% 不变，smoke 已锁。
- ✅ Slice 4（smoke 与 runtime check）已完成：`scripts/wave_w52_named_plan_files_smoke.py` + `scripts/wave_w52_runtime_check.ts` 注册进 `run_all_smoke.sh`。
- 🟡 Slice 3 deferred：`generatePromptPlanSlug` 与 `getPlanSlug(..., { firstUserPrompt })` 作为底座存在，但**当前无任何运行时调用方**传入 `firstUserPrompt`。
- 🟢 用户行为零变化：所有 plan 文件继续按 `purple-tiger-7.plan.md` 风格生成。

→ **W52 收口完成，可结案。** 后续 Named Plan Files runtime 接入由新 Wave 承接。

---

## 3. 总结与下一步

### 3.1 第一档 sprint 真实状态

- 第一档 13 项原计划 10–18 人天 → 经调研后真实施工面 **1 项 #10 + 1 份调研归档（本文档）+ 1 项品牌收口（#13，非阻塞，另行执行不在 W52 范围）**
- W52 实际工作量：**~ 1–2 天**（仅 #10 的 4 个 slice）
- 文档基线（v2.1.92–109）严重落后于 mossen 实际状态（W44–W51），mossen 已悄悄做完 6 项官方修复（#3 #5 #6 #9 #11 #12），且部分实现更精细
- 品牌核心迁移（loader/init/配置主路径）已完成，但文案/docs/test 假名残留待非阻塞收口

### 3.2 建议的下一步走法

1. **Allen 确认本归档报告**（特别是 #10 方案的 4 个决策点）
2. 若 #10 方案得到 confirm → 启动 Slice 1+2（纯加法、纯函数、无核心改动），完成后报总结
3. **同时启动第二档 Wave 0 调研**（25 项），按相同流程逐项判定。第二档很可能 N/A 率同样高
4. Slice 3（核心代码改动）必须在 Slice 1+2 完成后单独再征求一次

### 3.3 本轮归档物料

- 本文档：`docs/upgrade/W52-first-tier-archive.md`
- 调研使用的 grep 命令完整记录在本对话日志中（jsonl 文件可追溯）
- 0 commit、0 tag、0 smoke 改动

---

## 附录 A：调研用 grep 命令清单（可追溯）

为后续验证或第二档复用，以下是本轮关键 grep（去重后）：

```bash
# #1 Clipboard
rg -n "clipboard|pbcopy|pbpaste" --type ts -l
rg -n "clipboard.write|setClipboard|copyToClipboard|writeText" --type ts

# #2 Bash fd
rg -n "spawn|execSync|child_process" tools/BashTool/
rg -n "destroy|kill|cleanup|stdio" utils/ShellCommand.ts

# #3 idle timeout
cat utils/idleTimeout.ts

# #4 image
rg -n "image/png|image/jpeg|isImage|imageBuffer|pasteImage|encodeImage" --type ts -l
wc -l utils/imagePaste.ts utils/imageStore.ts utils/imageResizer.ts

# #5 CJK
grep -E '(string-width|wcwidth)' package.json
head -30 ink/stringWidth.ts

# #6 Trackpad
sed -n '40,90p' components/ScrollKeybindingHandler.tsx

# #7 URL
rg -n "OSC.*8|terminal-link|hyperlink" --type ts -l
head -25 ink/supports-hyperlinks.ts

# #8 effort
head -40 commands/effort/index.ts
grep "EffortLevel\|low\|medium\|high\|max" utils/effort.ts | head

# #9 theme
rg -n "COLORFGBG|themeFromOscColor|getSystemThemeName" --type ts

# #10 plans
rg -n "getPlanSlug|planSlug|getPlanFilePath" --type ts
sed -n '110,135p' utils/plans.ts
sed -n '149,235p' utils/plans.ts
head -80 commands/rename/generateSessionName.ts

# #11 alwaysLoad
rg -n "alwaysLoad" --type ts

# #12 maxResultSizeChars
rg -n "maxResultSizeChars" --type ts

# #13 brand（本轮修正——用全 glob 而非 --type ts）
rg -n "CLAUDE\.md|~/.claude|\.claude/|claude-sonnet|claude-opus|claude-fake" \
  --glob '!node_modules/**' --glob '!.git/**' --glob '!bun.lock'

# #13 真实 loader / init 验证
cat utils/naming.ts          # CANONICAL_PROJECT_INSTRUCTIONS_FILENAME = 'MOSSEN.md'
head -40 utils/mossenmd.ts   # 头注释完全围绕 MOSSEN.md / ~/.mossen 工作
head -40 commands/init.ts    # OLD_INIT_PROMPT / NEW_INIT_PROMPT 都写 MOSSEN.md
```

---

## 附录 B：Slice 1 + Slice 2 执行证据（2026-05-03）

### 范围
- Slice 1：`generatePromptPlanSlug` 纯函数 + `findUniqueSlugWithSuffix` 私有 helper
- Slice 2：`getPlanSlug` 加可选 `options?: { firstUserPrompt?: string }` 参数
- **未做**：Slice 3（运行时调用点穿透）、Slice 4 之外的任何接入

### Commits
- `b245676` — `feat(plan): add prompt-derived safe plan slug helper`（utils/plans.ts +163 / -9）
- `ac7603e` — `test(plan): cover named plan slug generation`（runtime check + smoke + run_all_smoke.sh 注册）
- 本 commit — `docs(upgrade): record W52 named plan slice 1 and 2`（追加本附录）

### 改动文件清单
- `utils/plans.ts` — 加 `generatePromptPlanSlug` export + 改 `getPlanSlug` 签名（缺省路径行为不变）
- `scripts/wave_w52_runtime_check.ts` — 新增，15 个 runtime case
- `scripts/wave_w52_named_plan_files_smoke.py` — 新增，静态契约锁定 + 调用 runtime check
- `scripts/run_all_smoke.sh` — 注册 `wave_w52_named_plan_files_smoke` 一行

### 未触动的核心代码（Slice 3 边界）
- `screens/REPL.tsx` — 0 改动
- `query.ts` — 0 改动
- `tools/EnterWorktreeTool/EnterWorktreeTool.ts` — 0 改动（仍 `getPlanSlug()` 无参调用，行为完全不变）
- `setup.ts` — 0 改动（同上）
- `utils/sessionStorage.ts` — 0 改动（slug 持久化字段语义不变）
- `utils/worktree.ts` — 0 改动（`validateWorktreeSlug` 仅被 import，未修改）
- `bootstrap/state.ts` — 0 改动（`planSlugCache` 不变）
- `commands/plan/plan.tsx` — 0 改动

### 验证结果
```
python3 scripts/wave_w52_named_plan_files_smoke.py
  → PASS: W52 named plan files Slice 1+2 ✓
    - 静态契约 8 项全过
    - runtime case 15 项全过

bash scripts/run_all_smoke.sh
  → ALL PASS（含 lint:diff / typecheck:diff / 12 个 wave smoke / layer audit / case 39 fingerprint）

python3 scripts/typecheck_diff.py
  → ✅ no new typecheck errors（baseline 1384, current 1080）

python3 scripts/lint_diff.py
  → ✅ no new lint problems（baseline 943, current 938）

git diff --check
  → 干净
```

### Slice 3 决策点（待 Allen 二次 confirm）

按 §2.7 Slice 3 设计，下一步必须先单独输出"调用点接入小方案"：
1. 真实 plan 创建边界在哪个文件 / 哪一行（grep 出所有 `getPlanSlug()` 无参调用点的入口位置）
2. 首 user prompt 怎么传入（不能动 query loop / REPL 主流程的方案）
3. 影响面：`EnterWorktreeTool` 与 `setup.ts` 调用点是否需要同步改、resume/fork 不调用是否能保持

完成上述小方案、Allen 第二次拍板后才能写 Slice 3 接入代码。

### 兼容性确认
- 旧 session resume：`copyPlanForResume` 不调 `generatePromptPlanSlug`，从 log 读 slug 后直接 set 进 cache → 旧 word-slug session 行为 100% 不变
- 旧 fork：`copyPlanForFork` 内调 `getPlanSlug(targetSessionId)` 无 options 参数 → 走 word-slug fallback 分支 → 行为 100% 不变
- 现有 `getPlanSlug()` / `getPlanSlug(id)` 调用点（`REPL.tsx` / `setup.ts` / `EnterWorktreeTool` / `getPlanFilePath`）全部走 word-slug fallback → 当前用户感知零变化
- 既有 `purple-tiger-7.plan.md` 文件保留原状，不迁移、不 rename
- 持久化 schema 不变（`SerializedMessage.slug` 字段未动）

### 中文 prompt 行为（v0 设计明示）
- 中文为主的 prompt（ASCII alnum < 50% 比例）→ `generatePromptPlanSlug` 返回 null
- 调用方走原 `generateWordSlug()` fallback
- 中文用户在 v0 暂时拿不到 prompt-derived 命名，但 worktree/branch/file 三处 ASCII 安全
- 未来 v2 可加 display title 字段与 slug 解耦（不在 W52 范围）
