# 逐文件精确翻译执行方案

**总量**: 1,966 个 TS/TSX 文件, 517,364 行代码
**目标**: 100% 逻辑对等翻译，零 stub，零 TS 残留
**策略**: 保留 Cargo workspace 骨架，按依赖层从底向上，逐文件完整翻译
**批次规则**: 每批 ≤20 个文件或 ≤5,000 行 TS 代码（先到为准）

---

## 核心执行原则

1. 每个 TS 文件必须逐函数、逐逻辑块翻译，Rust 实现必须覆盖源文件的全部业务逻辑、异常路径和副作用
2. 禁止空 struct、空 impl、占位函数、todo!() 宏、unimplemented!() 宏
3. 每个 Coding Agent 接到任务时，必须先完整读取对应 TS 源文件，再逐函数翻译
4. 每批完成后 cargo check 验证，确保与已翻译的上游模块正确集成
5. 对于已存在的 stub Rust 文件，完全重写为完整实现

---

## Layer 0: 类型与常量 (39 files, 6,540 lines) --> mossen-types

现有 mossen-types crate 有 17 个 Rust 文件，需要重写为完整实现并补齐缺失。

### Task L0-1: types/ 目录全部文件 (18 files, ~3,800 lines)
- `types/message.ts`, `types/permissions.ts` (441L), `types/textInputTypes.ts` (387L)
- `types/plugin.ts` (363L), `types/logs.ts` (330L), `types/hooks.ts` (290L)
- `types/command.ts`, `types/ids.ts`, `types/connectorText.ts`
- `types/generated/` 子目录所有文件
- **要求**: 对比现有 Rust 文件，逐字段、逐变体补全所有遗漏

### Task L0-2: constants/ 目录全部文件 (21 files, ~2,740 lines)
- `constants/prompts.ts` (1,001L) -- 最大，必须完整翻译所有 prompt 模板
- `constants/github-app.ts` (322L), `constants/spinnerVerbs.ts` (309L)
- `constants/oauth.ts` (224L) 及其余 17 个常量文件
- **要求**: 每个常量值必须完整对应，不遗漏任何条目

---

## Layer 1: 工具函数库 (582 files, 183,561 lines) --> mossen-utils

这是最大的层，占总代码量 35.5%。需按子目录分批。

### Task L1-1 ~ L1-16: utils/ 根目录文件 (312 files, 94,583 lines)
按文件大小从大到小，分 16 批翻译：

**L1-1**: 超大文件第 1 批 (3 files, ~15,600L)
- `utils/messages.ts` (5,470L), `utils/sessionStorage.ts` (5,189L), `utils/hooks.ts` (4,955L)

**L1-2**: 超大文件第 2 批 (3 files, ~10,000L)
- `utils/attachments.ts` (3,948L), `utils/auth.ts` (2,048L), `utils/config.ts` (1,819L)

**L1-3**: 大文件第 3 批 (4 files, ~6,400L)
- `utils/worktree.ts` (1,601L), `utils/Cursor.ts` (1,530L), `utils/mossenmd.ts` (1,520L), `utils/api.ts` (~1,400L)

**L1-4 ~ L1-16**: 剩余 302 个文件按大小分组，每批 20 文件或 5,000 行

### Task L1-17: utils/bash/ (23 files, ~5,000L)
- `utils/bash/bashParser.ts` (4,436L) -- 完整 Shell 命令解析器
- `utils/bash/ast.ts` (2,679L) -- AST 生成
- 及其余 21 个 bash 相关文件

### Task L1-18 ~ L1-19: utils/plugins/ (49 files, ~2,400L)
- `utils/plugins/pluginLoader.ts` (3,302L)
- `utils/plugins/marketplaceManager.ts` (2,649L)
- `utils/plugins/schemas.ts` (1,680L)
- 及其余 46 个插件工具文件

### Task L1-20 ~ L1-21: utils/permissions/ (24 files, ~6,400L)
- `utils/permissions/filesystem.ts` (1,816L)
- `utils/permissions/permissionSetup.ts` (1,547L)
- 及其余 22 个权限工具文件

### Task L1-22: utils/swarm/ (22 files, ~3,500L)
- `utils/swarm/inProcessRunner.ts` (1,552L) 及其余

### Task L1-23: utils/settings/ (19 files, ~3,000L)

### Task L1-24: utils/model/ (18 files, ~2,000L)

### Task L1-25: utils/shell/ (10 files, ~1,500L)
- `utils/shell/readOnlyCommandValidation.ts` (1,893L)

### Task L1-26: utils/hooks/ (17 files, ~1,500L)

### Task L1-27: 剩余 utils/ 子目录 (suggestions/ 等)

---

## Layer 2: 核心服务层 (177 files, 62,008 lines) --> mossen-agent

### ✅ Task L2-1 ~ L2-6: services/api/ (21 files, ~12,000L)
- ✅ mossen_agent/src/api/ 全部 22 个文件完整翻译

### ✅ Task L2-7: services/compact/ (12 files, ~4,500L)
- ✅ compact.rs, auto_compact.rs, micro_compact.rs, prompt.rs 等完整翻译

### ✅ Task L2-8: services/tools/ (4 files, ~2,500L)
- ✅ streaming_tool_executor.rs, tool_execution.rs, tool_hooks.rs, tool_orchestration.rs 完整翻译

### ✅ Task L2-9: services/lsp/ (7 files, ~2,500L)
- ✅ client.rs, diagnostic_registry.rs, server_instance.rs, server_manager.rs 等完整翻译

### ✅ Task L2-10: services/analytics/ (13 files, ~3,000L)
- ✅ datadog.rs, metadata.rs, config.rs, event_queue.rs 等完整翻译

### ✅ Task L2-11: context/ (9 files, ~25,000L)
- ✅ mod.rs, notifications.rs, stats.rs, overlay.rs, modal.rs 等完整翻译

### ⏳ Task L2-12: query/ (4 files, ~27,000L)
- `query/stopHooks.ts` (638L) - 完整翻译 stop_hooks.rs
- `query/tokenBudget.ts` (2,300L) - 待翻译
- `query/config.ts` (1,700L) - 待翻译
- `query/deps.ts` (1,400L) - 待翻译

### ⏳ Task L2-13: coordinator/ (1 file, ~18,000L)
- `coordinator/coordinatorMode.ts` (18,600L) - 待翻译

### ✅ Layer 2 完成验证
- ✅ `cargo check -p mossen-agent` 通过（仅有未使用导入警告，无错误）

---

## Layer 3: 工具定义层 (192 files, ~8,000 lines) --> mossen-tools

### Task L3-1: tools/AgentTool/ (20 files, 2,172L) ✅
- `tools/AgentTool/AgentTool.tsx` (1,322L) -- ✅ 完整翻译 agent.rs
- `tools/AgentTool/constants.ts` (217L) -- ✅ 常量已定义在 agent.rs
- ✅ cargo check 通过（4 warnings, 0 errors）

### Task L3-2: tools/BashTool/ (18 files, 1,441L) ✅
- `tools/BashTool/bash.ts` -- ✅ 完整翻译 bash.rs (263L)
- ✅ cargo check 通过

### Task L3-3: tools/PowerShellTool/ (14 files, 1,194L)

### Task L3-4: tools/plugin/ (23 files, ~1,100L)

### Task L3-5 ~ L3-8: 剩余工具目录 (~117 files, ~2,100L)
- FileEditTool/, FileReadTool/, FileWriteTool/, GlobTool/, GrepTool/
- LSPTool/, MCP 相关工具, REPLTool/, WebFetchTool/, WebSearchTool/ 等

---

## Layer 4: 命令层 (211 files, ~13,000 lines) --> mossen-commands

### Task L4-1 ~ L4-2: commands/plugin/ (23 files, 9,174L) -- 分 2 批
### Task L4-3: commands/install-github-app/ (13 files, 1,510L)
### Task L4-4: commands/mcp/ (12 files, 1,034L)
### Task L4-5: commands/project/ (6 files, 1,108L)
### Task L4-6: commands/ 根目录大文件 -- insights.ts (3,176L) 等
### Task L4-7 ~ L4-12: 剩余命令子目录 (~150 files)

---

## Layer 5: UI 层 (587 files, ~53,000 lines) --> mossen-tui

### Task L5-1 ~ L5-3: components/permissions/ (51 files, 11,318L) -- 分 3 批
### Task L5-4 ~ L5-5: components/messages/ (41 files, 5,948L) -- 分 2 批
### Task L5-6: components/PromptInput/ (21 files, 4,704L)
### Task L5-7: components/agents/ (26 files, 4,352L)
### Task L5-8: components/mcp/ (13 files, 4,227L)
### Task L5-9: components/tasks/ (10 files, 2,735L)
### Task L5-10: components/Settings/ (4 files, 2,633L)
### Task L5-11: components/design-system/ (16 files, 2,199L)
### Task L5-12 ~ L5-15: 剩余 components/ 子目录和根文件
### Task L5-16 ~ L5-18: components/ 根目录大文件 (LogSelector 1,707L, Stats 1,256L, VirtualMessageList 1,081L 等)
### Task L5-19 ~ L5-21: hooks/ (103 files)
### Task L5-22 ~ L5-23: ink/ (97 files)

---

## Layer 6+: 其他目录与根文件

### Task L6-1: screens/ (3 files, 6,032L) -- REPL.tsx 4,865L 等
### Task L6-2: skills/ + plugins/ (21+ files)
### Task L6-3: tasks/ + state/ (17 files)
### Task L6-4: cli/ + remote/ + server/ (26 files)
### Task L6-5: keybindings/ + buddy/ + assistant/ + bootstrap/ (39 files)
### Task L6-6: 根目录 TS 文件第 1 批 -- main.tsx (4,649L), query.ts (1,747L), QueryEngine.ts (1,286L)
### Task L6-7: 根目录 TS 文件第 2 批 -- Tool.ts (791L), commands.ts (716L), setup.ts (478L) 等
### Task L6-8: 剩余目录 (platform/, memdir/, migrations/, native-ts/, schemas/ 等)

---

## 执行节奏

每个 Task 的执行流程：
1. Coding Agent 完整读取该批次所有 TS 源文件
2. 逐文件、逐函数翻译为 Rust（对比现有 Rust 文件，有则重写，无则新建）
3. cargo check 验证编译通过
4. 标记该批次完成，进入下一个 Task

**预估总批次**: ~70 个 Task
**执行顺序**: 严格按 Layer 0 -> 1 -> 2 -> 3 -> 4 -> 5 -> 6+ 依次推进
**Layer 内并行**: 同一 Layer 内不同子目录的 Task 可并行（前提是不修改同一 crate 的同一文件）

---

## 立即开始

确认后，从 **Task L0-1**（types/ 目录 18 个文件的完整翻译）开始执行。
