# W53 第二档中期升级 Wave 0 归档报告

## 元信息

| 字段 | 值 |
|---|---|
| 日期 | 2026-05-03 |
| HEAD hash | `2535ff4879bfafd06e48b550812bfec5721ddbc6` |
| origin/main | `2535ff4`（本地与远端同步） |
| 输入计划 | `/Users/allen/Desktop/mossen官方升级点/升级点概览.md` + `第二档_中期做_升级计划.md` |
| 上游归档 | `docs/upgrade/W52-first-tier-archive.md`（第一档已结案） |
| 本轮性质 | 只读调研归档（Wave 0） |
| 本轮改动 | docs-only（仅追加本文档） |
| 范围 | A1–A5 / B1–B4 / C1–C9 / D1–D7 共 25 项 |

---

## 0. 一句话总结

第二档 25 项实测分布：**N/A 10 / REAL 13 / DEFER 2 / STOP 0**。

文档基线（约 v2.1.92–109）落后 mossen 当前能力 3–4 月，**A 组 5 项中 3 项已基本完成**（流式 IO、虚拟滚动、FPS 监控），**B 组 4 项中 3 项已完成**（MCP Elicitation、Hook 系统 27 事件、记忆索引），D 组的 iron_gate / 沙箱已就位。剩余真实 gap 集中在 **C 组小命令 + D 组权限增强** 两块。

**推荐 W54 优先做**：C5 project purge、C6 plugin prune、C8 /skills 搜索框、C4 Stale Session 提示、D4 bypass-immune 列表扩展（5 项小动作可合一个 Wave 收口）。

---

## 1. 总览表

| 编号 | 名称 | 分组 | 判定 | 原因摘要 | 下一步 |
|---|---|---|---|---|---|
| A1 | Prompt 渲染优化 | A 性能 | REAL | `components/PromptInput/PromptInput.tsx` 实存；尚未做 memo/Profiler 基线测量 | 先测基线再判断收益 |
| A2 | Token 效率 `/resume` ~600 节省 | A 性能 | REAL | `utils/sessionStorage.ts:1818 SerializedMessage` 实存；未测当前 token 数 | 先 tokenize 标准 session 测基线 |
| A3 | 文件读写流式化 | A 性能 | **N/A** | `utils/readFileInRange.ts` 已实现 fast/streaming 双路径，>10MB 自动走 createReadStream | 不施工 |
| A4 | 虚拟滚动 1k → 8k+ | A 性能 | **N/A** | `components/VirtualMessageList.tsx` + `hooks/useVirtualScroll.ts` 已是完整虚拟化（PESSIMISTIC_HEIGHT/SLIDE_STEP/MAX_MOUNTED_ITEMS=300）；无硬上限 | 不施工 |
| A5 | 渲染 FPS 监控埋点 | A 性能 | **N/A** | `context/fpsMetrics.tsx` + `utils/fpsTracker.ts` + `cost-tracker.ts:141 saveCurrentSessionCosts(fpsMetrics?)` 已实现 | 不施工 |
| B1 | MCP Elicitation Hooks | B 协议 | **N/A** | `services/mcp/elicitationHandler.ts` + `controlSchemas.ts:522 SDKControlElicitationRequest/ResponseSchema` + `coreSchemas.ts:638 ElicitationHookInputSchema` + Elicitation/ElicitationResult 两个 hook event 已落地；@modelcontextprotocol/sdk ^1.29 | 不施工 |
| B2 | Agent SDK Hook 系统对齐 5 钩子点 | B 协议 | **N/A** | `entrypoints/sdk/coreTypes.ts HOOK_EVENTS` 已 27 个事件（含 PreToolUse/PostToolUse/PostToolUseFailure/UserPromptSubmit/Stop/StopFailure/PermissionRequest/PermissionDenied 等），远超官方 5 钩子规模 | 不施工 |
| B3 | 压缩 quality 评估 | B 协议 | DEFER | `services/compact/{compact,microCompact,apiMicrocompact}.ts` 实存；缺 eval 集；构造 10-session eval + 召回评分本身是独立工程，价值未触发 | 等用户报告召回退化时再启 |
| B4 | 记忆索引 + 本地同步接口 | B 协议 | **N/A** | `memdir/findRelevantMemories.ts`（LLM 选择）+ `memdir/memoryScan.ts`（200 文件上限 + frontmatter 索引）+ `services/teamMemorySync/` 已存在 | 不施工 |
| C1 | 交互式 `/effort` 滑块 UI | C 体验 | REAL | `commands/effort/effort.tsx` 实存（low/medium/high + auto + env override）；缺无参滑块 UI | 小 slice，可做 |
| C2 | OS 级通知 | C 体验 | **N/A** | `services/notifier.ts` 已用 osascript / preferredNotifChannel + `executeNotificationHooks` 实现；不需要 node-notifier | 不施工（如要加 Linux notify-send 走 channel 扩展，属于增量补丁） |
| C3 | Session Recap "While you were away" | C 体验 | **N/A** | `services/awaySummary.ts` + `hooks/useAwaySummary.ts` 实存 | 不施工 |
| C4 | Stale Session 提示 | C 体验 | REAL | grep `stale` 仅命中 git/cache 语义；无 lastActiveAt/sessionAge 字段；真实 gap | W54 候选 |
| C5 | `project purge` 命令 | C 体验 | REAL | `commands/clear/{clear,caches,conversation}.ts` 仅清当前 session；无跨 project purge；真实 gap | W54 候选（高保护要求：禁动 cwd 内文件 + 保 commands/insights.ts WIP） |
| C6 | `plugin prune` 命令 | C 体验 | REAL | grep 无 `plugin prune` / `orphan plugin` 命令；真实 gap | W54 候选 |
| C7 | Vim 视觉模式完整化 | C 体验 | REAL | `hooks/useVimInput.ts` 仅 INSERT/NORMAL 两态；无 VISUAL / yank / paste | 单 slice 中等改动，需独 confirm |
| C8 | `/skills` 搜索框 | C 体验 | REAL | `components/skills/SkillsMenu.tsx` 按 source 分组列表；无搜索/过滤输入框 | W54 候选 |
| C9 | LSP 诊断摘要展开式渲染 | C 体验 | DEFER | `components/DiagnosticsDisplay.tsx` 已有 verbose toggle + `CtrlOToExpand` 提示；当前折叠/展开能力够用，再优化收益小 | 不施工 |
| D1 | `/less-permission-prompts` skill | D 权限 | REAL | `utils/permissions/denialTracking.ts` 是基础但未对外做 skill；无 `less-permission*` 文件 | 必须 5 维度调研后做 |
| D2 | 权限模板系统 3 预设 | D 权限 | REAL | grep `permissionTemplate/Preset` 0 命中（yoloClassifier 的 `<permissions_template>` 是 LLM prompt 占位，非用户预设）；真实 gap | 必须 5 维度调研 |
| D3 | Auto Mode 权限分类器精度 | D 权限 | REAL | `utils/permissions/yoloClassifier.ts` 已 2-stage 分类器；继续提升属真实改进项 | 必须 5 维度调研 + eval 集 |
| D4 | bypass-immune 扩展 | D 权限 | REAL | `permissions.ts:527/1146/1254` 已有 bypass-immune 框架；扩展 `.zshrc/.idea/.vscode` 是小列表加项 | W54 候选（小） |
| D5 | 推测式分类器 + iron_gate | D 权限 | **N/A** | `permissions.ts:849 tengu_iron_gate_closed` + 2-stage classifier (`stage1Usage/stage2Usage`) 已就位 | 不施工 |
| D6 | 本地安全审计日志 | D 权限 | REAL | 当前权限决策走 `logEvent('tengu_tool_use_can_use_tool_rejected/allowed')` 进 analytics；无本地用户可见 jsonl | 必须 5 维度调研（脱敏/滚动/路径） |
| D7 | 沙箱逃逸检测加强 | D 权限 | **N/A** | `tools/BashTool/shouldUseSandbox.ts` + `utils/sandbox/sandbox-adapter.ts` 已有完整沙箱（settings.json 写入拦截 / 路径校验 / git fsmonitor escape 处理） | 不施工 |

---

## 2. A 组逐项判断（5 项 / 性能与稳定性）

### A1. Prompt 渲染优化

- **代码证据**：`components/PromptInput/PromptInput.tsx`（2200+ 行主组件）；调用方 `screens/REPL.tsx`、`utils/processUserInput/`。
- **现状**：组件已大量使用 `useMemo/useCallback`（rg `React\.memo|useDeferredValue|useMemo` 命中 65+ 处），但官方所谓 "重渲染减 74%" 必须先做 React Profiler 基线才能判断是否有改善空间。
- **判定**：**REAL**（待测）。
- **建议 slice**：Slice 0 测量基线（输入 100 字符的渲染次数）→ 若基线已经低，直接降级为 N/A；否则进 Slice 1/2/3。
- **注意**：mossen 已用 React Compiler（`react/compiler-runtime` 全面铺开），自动 memo 程度高，纯手工 memo 收益可能边际。

### A2. Token 效率 `/resume` ~600 节省

- **代码证据**：`utils/sessionStorage.ts:53/1052/1818` 有 `SerializedMessage` 类型与序列化路径；`commands/clear/conversation.ts` 等清理路径。
- **现状**：未做 tokenizer 基线测量；mossen 已有 microcompact / apiMicrocompact 等能力，可能已天然瘦身。
- **判定**：**REAL**（待测）。
- **建议 slice**：Slice 0 跑 tokenizer 测一组 standard session 当前体积 → 若已 < 600 节省阈值，直接 N/A。
- **依赖**：与 B3（压缩 quality 评估）共用 eval 集。

### A3. 文件读写流式化

- **代码证据**：`utils/readFileInRange.ts`：注释明确两条路径——
  - **Fast path（< 10 MB）**：`fs.readFile()` 全读；
  - **Streaming path（大文件 / 管道 / 设备）**：`fs.createReadStream` + 手写 `indexOf('\n')` 扫描；模块级 named handler、`.once('open'/'end'/'error')`、StreamState 闭包零创建、`stream.destroy(err)` on byte-cap。
- `tools/FileReadTool/FileReadTool.ts:1016` 调 `readFileInRange`；`tools/FileWriteTool/FileWriteTool.ts:253` 也已封装。
- **判定**：**N/A**。已实现，且实现质量高于文档基线。
- **不做项**。

### A4. 虚拟滚动 1k → 8k+

- **代码证据**：`components/VirtualMessageList.tsx`（800+ 行）+ `hooks/useVirtualScroll.ts`（316 行）：
  - `MAX_MOUNTED_ITEMS = 300`（绑定 fiber 上限）
  - `SLIDE_STEP = 25`（avoid 290ms sync block）
  - `PESSIMISTIC_HEIGHT = 1`（保证 mount span 触底）
  - `OVERSCAN_ROWS` + `getItemTop` 走 Yoga `computedTop`（drift-free）
  - `ScrollBox` 已做 Ink-output-level viewport culling（`render-node-to-output.ts:617`）
- 注释明确："1000-message session costs ~250 MB grow-only memory" — 已设计为 8k+ 工作。
- **判定**：**N/A**。文档基线（"上限 1k"）与 mossen 现状不符。
- **不做项**。

### A5. 渲染 FPS 监控埋点

- **代码证据**：
  - `context/fpsMetrics.tsx`（FpsMetricsProvider + useFpsMetrics context）
  - `utils/fpsTracker.ts`（`record(durationMs)`、averageFps、low1PctFps）
  - `interactiveHelpers.tsx:326` 调 `fpsTracker.record(event.durationMs)`
  - `cost-tracker.ts:141 saveCurrentSessionCosts(fpsMetrics?)` 持久化 FPS 进 session 成本记录
  - `components/App.tsx:3` 全局 Provider 挂载
- **判定**：**N/A**。本地 jsonl + dev hud 的产品诉求已被 cost-tracker + analytics 覆盖。
- **不做项**。

---

## 3. B 组逐项判断（4 项 / 协议与 Hook 对齐）

### B1. MCP Elicitation Hooks

- **代码证据**：
  - `services/mcp/elicitationHandler.ts`（200+ 行，完整实现）：`registerElicitationHandler` 注册 `ElicitRequestSchema` handler；form/url 两种 mode；`runElicitationHooks` 让 hook 可程序化响应；`tengu_mcp_elicitation_shown/response` analytics
  - `entrypoints/sdk/controlSchemas.ts:522/538` 已有 `SDKControlElicitationRequestSchema` / `SDKControlElicitationResponseSchema`，并已注册进 union（line 848）
  - `entrypoints/sdk/coreSchemas.ts:638/656` `ElicitationHookInputSchema` / `ElicitationResultHookInputSchema`
  - `package.json: @modelcontextprotocol/sdk ^1.29.0`（已是 elicitation 支持版本）
  - `coreTypes.ts HOOK_EVENTS` 含 'Elicitation' / 'ElicitationResult'
  - `executeElicitationHooks` / `executeElicitationResultHooks` 在 `utils/hooks.ts` 实存
- **协议风险**：N/A — 已纳入 stream-json union 的 5 处同改框架。
- **5 处同改是否需要**：N/A — 已完成。
- **判定**：**N/A**。比文档基线更完整。
- **不做项**。

### B2. Agent SDK Hook 系统对齐 5 钩子点

- **代码证据**：`entrypoints/sdk/coreTypes.ts HOOK_EVENTS` 共 **27 个事件**：

  ```
  PreToolUse, PostToolUse, PostToolUseFailure, Notification,
  UserPromptSubmit, SessionStart, SessionEnd, Stop, StopFailure,
  SubagentStart, SubagentStop, PreCompact, PostCompact,
  PermissionRequest, PermissionDenied, Setup,
  TeammateIdle, TaskCreated, TaskCompleted,
  Elicitation, ElicitationResult, ConfigChange,
  WorktreeCreate, WorktreeRemove, InstructionsLoaded,
  CwdChanged, FileChanged
  ```

- 文档基线要求的 preToolUse / postToolUse / preMessage / postMessage / onError 中：preToolUse/postToolUse 已有；UserPromptSubmit/Stop/StopFailure 覆盖 message 边界；PermissionRequest/PermissionDenied/PostToolUseFailure 覆盖 onError 语义。
- `utils/hooks/AsyncHookRegistry.ts` + `utils/hooks/hookEvents.ts` + `utils/hooks/hooksConfigManager.ts` 完整。
- **协议风险**：N/A — 当前 27 hook event 已超前。
- **5 处同改是否需要**：N/A。
- **判定**：**N/A**。
- **不做项**。

### B3. 压缩 quality 评估

- **代码证据**：`services/compact/{compact,microCompact,apiMicrocompact,autoCompact,sessionMemoryCompact}.ts` 完整 4 层压缩；`tengu_cached_microcompact` / `tengu_time_based_microcompact` analytics 已埋。
- **缺**：eval 集（10 标准 session）+ 召回评分脚本 + 本地 quality jsonl。这是一个独立工程，且 mossen 当前压缩稳定运行多月，无用户报告召回退化。
- **协议风险**：低（不动协议，只加观测）。
- **5 处同改是否需要**：否。
- **判定**：**DEFER**。等出现召回质量信号或用户复现的退化案例再启。
- **不做项**。

### B4. 记忆索引 + 本地同步接口

- **代码证据**：
  - `memdir/findRelevantMemories.ts`：LLM-based 选择 top-5 memory（Sonnet `selectRelevantMemories`）
  - `memdir/memoryScan.ts`：`scanMemoryFiles` 200 文件上限 + frontmatter 解析 + mtime 排序
  - `services/teamMemorySync/index.ts:825` 团队记忆同步存在
  - `commands/memory/memory.tsx:111 getMemoryFiles()`
  - `memdir/{memoryAge,memoryTypes,paths,teamMemPaths,teamMemPrompts}.ts` 完整子系统
- **协议风险**：N/A。
- **5 处同改是否需要**：否。
- **判定**：**N/A**。文档基线"建 memory_index.json"是 mossen 已用 LLM 选择 + frontmatter 索引取代。
- **不做项**。

---

## 4. C 组逐项判断（9 项 / 体验小升级）

### C1. 交互式 `/effort` 滑块 UI

- **代码证据**：`commands/effort/effort.tsx` + `commands/effort/index.ts`；`utils/effort.ts`；EffortValue 含 low/medium/high + auto；`MOSSEN_CODE_EFFORT_LEVEL` env override；`getEffortValueDescription` 已可生成实时描述。
- **用户触达性**：高（每会话首次调用 `/effort`）。
- **判定**：**REAL**。需新增无参时的 Ink 选择器组件（与 ConsoleOAuthFlow / OutputStylePicker 同款）。
- **依赖**：第一档 #8 xhigh 参数（W52 中 #8 判定为"做 capability mapping，backend 不支持降级"——若 backend 已就绪可一起做）。
- **下一步**：W54 候选。

### C2. OS 级通知

- **代码证据**：`services/notifier.ts`：
  - `sendNotification` + `getDefaultNotificationTitle` + `executeNotificationHooks` + `tengu_notification_method_used` analytics
  - 用 `osascript` 直发（macOS），`preferredNotifChannel` 配置项支持多通道
- **缺**：node-notifier 跨平台桥（mossen 已用原生 osascript 实现 macOS；Linux notify-send 通道未明确添加）
- **用户触达性**：中。
- **判定**：**N/A**（核心能力已存在）。如 Allen 后续要补 Linux notify-send，是 channel 扩展小补丁，不算独立 wave 项。
- **不做项**（W54 不开）。

### C3. Session Recap "While you were away"

- **代码证据**：`services/awaySummary.ts` + `hooks/useAwaySummary.ts` + `query.ts` 调用 + `constants/prompts.ts` 提示模板。
- **用户触达性**：中（resume 后看到）。
- **判定**：**N/A**。已实现。"增加可折叠/展开 + 关键事件标记"是 polish 优化，价值边际低。
- **不做项**。

### C4. Stale Session 提示

- **代码证据**：grep `stale` 仅命中 git fsmonitor / analytics retry / cache invalidation，**无 session age / lastActiveAt / sessionIdle 字段**。
- **用户触达性**：中（24h+ resume 时）。
- **判定**：**REAL**。
- **下一步**：W54 候选。需补 `lastActiveAt` 持久化 + 启动时阈值判定 + UI 卡片。

### C5. `project purge` 命令

- **代码证据**：`commands/clear/{clear,caches,conversation}.ts` 仅清当前 session 缓存；无跨 project purge。
- **用户触达性**：低频但高价值（用户切了多个 project 后 home 目录会膨胀）。
- **判定**：**REAL**。
- **下一步**：W54 候选。**强保护**：必须禁动 cwd 内文件、必须保 `commands/insights.ts` WIP（用户 memory 中明确）。

### C6. `plugin prune` 命令

- **代码证据**：grep 无任何 `plugin prune` / `orphan plugin` 路径；`commands/plugin/{ManagePlugins,BrowseMarketplace,...}.tsx` 是管理 UI，非 cleanup。
- **用户触达性**：低频但高价值（plugin manifest 删除后 cache 残留）。
- **判定**：**REAL**。
- **下一步**：W54 候选。

### C7. Vim 视觉模式完整化

- **代码证据**：`hooks/useVimInput.ts:36`：`useState<VimMode>('INSERT')`；rg `VISUAL` 在 `VimTextInput.tsx` / `useVimInput.ts` 0 命中；`switchToInsertMode` / `switchToNormalMode` 仅两态切换。
- **用户触达性**：中（编辑器模式 = vim 的用户少但坚定）。
- **判定**：**REAL**，但属中等改动（state machine 扩 v/V 模式 + yank buffer + p 操作 + 跨终端 clipboard）。
- **下一步**：单 slice 候选，需 Allen 拍板优先级。

### C8. `/skills` 搜索框

- **代码证据**：`commands/skills/skills.tsx` + `components/skills/SkillsMenu.tsx`：按 source 分组列表（policySettings / userSettings / projectSettings / localSettings / flagSettings / plugin / mcp），无 search input 与 fuzzy filter。
- **用户触达性**：中（skill 数 > 10 时排查变慢）。
- **判定**：**REAL**。
- **下一步**：W54 候选。fuzzy 用 `fuzzysort` 或自写小函数皆可，无大依赖。

### C9. LSP 诊断摘要展开式渲染

- **代码证据**：`components/DiagnosticsDisplay.tsx`：已有 `verbose` toggle 与 `CtrlOToExpand` 提示控件（`components/CtrlOToExpand.tsx` 完整实现）；`fileCount/totalIssues` 已计算。
- **用户触达性**：中（LSP 配置后才有）。
- **判定**：**DEFER**。当前折叠 = 默认显示 count + severity，展开走 Ctrl+O / verbose 模式，已满足"折叠卡片"需求。再做"点击展开"在 TUI 下交互价值小（无鼠标 click）。
- **不做项**。

---

## 5. D 组逐项判断（7 项 / 权限与安全）

> D 组所有 REAL 项必须先做 5 维度调研（skill/子 agent/gate/fallback/测试），不允许直接动权限主路径——见 `feedback_skill_subagent_systems_protected.md`。

### D1. `/less-permission-prompts` skill

- **代码证据**：`utils/permissions/denialTracking.ts` 已记录 `consecutiveDenials/totalDenials`，但仅服务于自动模式 fallback；`maxConsecutive=3 / maxTotal=20` 是参数。无对外 skill / 推荐算法。
- **权限风险**：中。建议 + 用户手动接受不直接绕过；但分析逻辑要看历史批准 jsonl。
- **5 维度调研**：必须做。
- **判定**：**REAL**，但放后置。
- **下一步**：先 5 维度调研施工包，Allen 拍板。

### D2. 权限模板系统 3 预设（默认/严格/宽松）

- **代码证据**：grep `permissionTemplate/Preset` 0 命中；`utils/permissions/yoloClassifier.ts` 中的 `<permissions_template>` 是 LLM prompt 占位符（`auto_mode_system_prompt.txt`），与用户预设无关；mossen 当前权限规则仅 `permissionsLoader.ts` 走 settings json。
- **权限风险**：中。"切换后立即生效"会动 toolPermissionContext 主路径。
- **5 维度调研**：必须做（特别是 fallback：用户切错预设后是否能撤回）。
- **判定**：**REAL**。
- **下一步**：5 维度调研后再排。

### D3. Auto Mode 权限分类器精度提升

- **代码证据**：`utils/permissions/yoloClassifier.ts`（1300+ 行）已是 mossen 现役主分类器；2-stage classifier 已就位（`stage1Usage/stage2Usage`、`tengu_iron_gate_closed` gate）；`utils/permissions/yolo-classifier-prompts/auto_mode_system_prompt.txt` 是规则模板。
- **权限风险**：高（主路径精度直接决定误拦/误放率）。
- **5 维度调研**：必须做 + eval 集（100 工具调用样本）。
- **判定**：**REAL**，但属重活，需独立 confirm。
- **下一步**：先做规则审计 + eval 集，Allen 拍板后再改规则。

### D4. bypass-immune 扩展

- **代码证据**：`permissions.ts:527 / 1146 / 1254` 已有 `bypass-immune` 框架（"must prompt even in bypassPermissions mode"）；`dangerousPatterns.ts` 列举 CROSS_PLATFORM_CODE_EXEC 等模式；扩展 `.zshrc/.bashrc/.idea/.vscode` 是在已有数据集加路径条目。
- **权限风险**：低（只加，不改主流程）。
- **5 维度调研**：轻量（只 fallback + 测试）。
- **判定**：**REAL**（小）。
- **下一步**：W54 候选（与 C 组小命令打包做一个 wave）。

### D5. 推测式分类器 + iron_gate

- **代码证据**：
  - `utils/permissions/permissions.ts:849` `tengu_iron_gate_closed` GrowthBook gate 已在用
  - `permissions.ts:788/807` 引用 `classifierResult.stage1Usage / stage2Usage` — **2-stage classifier 已实装**
  - `yoloClassifier.ts:1112` "Dispatch to 2-stage XML classifier if enabled via GrowthBook"
- iron_gate 默认 `true`（fail closed）；分类器 unavailable 时 deny + retry guidance。
- **判定**：**N/A**。文档基线"加第 3 阶段"在 mossen 当前 2-stage + iron_gate 体系下不直接可比，且当前体系已在跑。
- **不做项**。

### D6. 本地安全审计日志

- **代码证据**：当前权限决策走 `services/tools/toolExecution.ts:849/953/1124/1388` 等多处 `logEvent('tengu_tool_use_can_use_tool_rejected/allowed/success/error')`，落 internal analytics（GrowthBook / mossen analytics）。**无用户可见的本地 jsonl**。`utils/hooks.ts:4155` 注释提及 "audit log" 但仅指 enterprise hook 用途。
- **权限风险**：中（涉及脱敏：禁止记录 prompt 正文 / file content）。
- **5 维度调研**：必须做（脱敏规则、滚动归档、文件路径权限）。
- **判定**：**REAL**。
- **下一步**：5 维度调研后再排；与 C5 project purge 共用 `~/.mossen/` 路径管理。

### D7. 沙箱逃逸检测加强

- **代码证据**：
  - `tools/BashTool/shouldUseSandbox.ts`：完整 sandbox 入口判断（disabledCommands / userExcludedCommands / SandboxManager）
  - `utils/sandbox/{sandbox-adapter,sandbox-ui-utils,sandboxRuntimeAdapter}.ts`：核心实现
  - `sandbox-adapter.ts:230` "Always deny writes to settings.json files to prevent sandbox escape"
  - `sandbox-adapter.ts:259` git fsmonitor escape 已显式处理
  - `tools/BashTool/readOnlyValidation.ts`：`bashCommandIsSafe_DEPRECATED` + DOCKER/GH/GIT/PYRIGHT/RIPGREP 各种 read-only 命令白名单
- **判定**：**N/A**（核心已就位）。文档基线"检测 /etc/passwd 访问"在 mossen 沙箱模型下属于已被 SandboxManager 拦截。
- **不做项**。

---

## 6. 推荐下一轮（W54）施工队列

### W54 候选 1（推荐打包，5–7 人天）

> 全部低风险、UI/CLI 表面、无主路径触动，可一个 wave 收口。

| 项 | 估 | 理由 |
|---|---|---|
| **C5 project purge 命令** | 1 天 | 高保护小命令；不动 cwd 内文件；二次确认 prompt |
| **C6 plugin prune 命令** | 0.5 天 | 检测 orphan plugin + 删除 cache，逻辑独立 |
| **C8 /skills 搜索框** | 1 天 | 纯 UI 增项，SkillsMenu 加 search input + fuzzy 过滤 |
| **C4 Stale Session 提示** | 1 天 | lastActiveAt 字段 + 启动判定 + 单 UI 卡片 |
| **D4 bypass-immune 列表扩展** | 0.5 天 | 在已有 immune 框架加 `.zshrc/.bashrc/.idea/.vscode` |

**为什么推荐**：5 项都是新增小能力，零协议改动、零权限主路径触动、无 5 处同改要求；可共享一组 smoke 验证。预估 commit 总量 < 10 个、文件总量 < 20 个。

### W54 候选 2（次推荐，单选）

| 项 | 估 | 触发条件 |
|---|---|---|
| **C7 Vim 视觉模式完整化** | 3–5 天 | 仅当 Allen 自己用 vim 模式遇到痛点时再做 |
| **A1 / A2 性能基线测量** | 2 天 | 想先看是否真有优化空间时做（slice 0 only） |
| **C1 /effort 滑块 UI** | 1 天 | 需第一档 #8 xhigh backend 完成后做 |

### 需要单独 Allen confirm 的高风险项

| 项 | 风险 | 调研要求 |
|---|---|---|
| **D1 /less-permission-prompts skill** | 中 | 5 维度调研 |
| **D2 权限模板 3 预设** | 中 | 5 维度调研 + fallback 设计 |
| **D3 Auto Mode 分类器精度** | 高 | 5 维度调研 + eval 集（100 样本）|
| **D6 本地安全审计日志** | 中 | 5 维度调研 + 脱敏规则 |

### 暂缓项

| 项 | 缓的理由 |
|---|---|
| **B3 压缩 quality 评估** | 无用户报告退化信号；构造 eval 集成本高 |
| **C9 LSP 诊断展开** | 当前 CtrlOToExpand + verbose toggle 已够用 |

---

## 7. 不做项说明（10 项 N/A 一览）

下列 10 项当前 mossen 已有同等或更强能力，**禁止为完成第二档计划硬做**：

| 项 | 不做的根据 |
|---|---|
| **A3 文件读写流式化** | `utils/readFileInRange.ts` 已 fast/streaming 双路径 + StreamState 闭包零创建 |
| **A4 虚拟滚动 1k → 8k+** | `VirtualMessageList` + `useVirtualScroll` 已 PESSIMISTIC_HEIGHT/SLIDE_STEP/MAX_MOUNTED_ITEMS 完整虚拟化 |
| **A5 渲染 FPS 监控埋点** | `context/fpsMetrics.tsx` + `utils/fpsTracker.ts` + `cost-tracker.saveCurrentSessionCosts` |
| **B1 MCP Elicitation Hooks** | `services/mcp/elicitationHandler.ts` + control schemas + Elicitation/ElicitationResult hook events 已落地 |
| **B2 Hook 系统对齐 5 钩子点** | `HOOK_EVENTS` 27 个事件远超 5 钩子规模 |
| **B4 记忆索引 + 同步接口** | `memdir/findRelevantMemories` + `memoryScan` + `teamMemorySync` |
| **C2 OS 级通知** | `services/notifier.ts` + osascript + preferredNotifChannel + executeNotificationHooks |
| **C3 Session Recap** | `services/awaySummary.ts` + `hooks/useAwaySummary.ts` |
| **D5 推测式分类器 + iron_gate** | `tengu_iron_gate_closed` + 2-stage classifier 已实装 |
| **D7 沙箱逃逸检测加强** | `tools/BashTool/shouldUseSandbox.ts` + `utils/sandbox/` + 显式 settings.json 写入拦截 |

如未来发现某项 mossen 实现实际有缺口，应按 **真实 gap → 单项立项**，不要回头硬补整档。

---

## 8. 验证

| 项 | 结果 |
|---|---|
| 本轮性质 | docs-only |
| `git diff --check` | 见最终报告 |
| `git status --short` | 见最终报告 |
| 是否跑 smoke | **否** |
| 不跑 smoke 的原因 | 仅追加一个 docs 文件，不触任何代码 / schema / dispatcher / smoke / Workbench；typecheck / lint / harness / case 39 fingerprint 都不可能漂移；跑 smoke 浪费 5+ 分钟无信号 |

---

## 9. 提交

如完成本轮归档，提交一个 docs commit：

```
docs(upgrade): archive W53 second-tier candidates
```

不 push、不 push tags、不 push GitHub。等待 Allen 决定是否启 W54。
