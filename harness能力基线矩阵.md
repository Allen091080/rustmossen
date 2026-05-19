# Mossen 能力基线矩阵

> **文档目的**：列出 Mossen 个人版当前应对齐的所有能力面，作为 `harness全链路测试.md` 所有测试场景的事实基线。
>
> **配套文档**：`harness全链路测试.md`（M0.1 验收产物 = 本文档）。
>
> **维护规则**：本文档由 M0.1 产出，后续测试发现的新能力或误解纠正必须回写本文档。每次更新必须附变更日志。
>
> **当前 HEAD**：`86e5e92` (2026-04-25)
>
> **状态**：M0.1 第一稿。基于源码静态调研 + CLI --help。**未经 runtime 真验证**——这是基线，不是测试通过。

---

## 0. 文档结构

| 章节 | 内容 |
|---|---|
| §1 | 12 个能力面逐项基线（每项含：官方能力 / 个人版状态 / 验证方式 / 风险 / 用户确认） |
| §2 | 关键发现 + 我之前的误解纠正（影响 `harness全链路测试.md` §3 已写场景） |
| §3 | 必须回写 `harness全链路测试.md` 的衍生待办 |
| §4 | 数据来源 + 调研方法 |
| §5 | 变更日志 |
| 附录 A | 100 个 slash command 全清单（按目录） |
| 附录 B | 50+ CLI 选项全清单（来自 --help） |

---

## 1. 12 个能力面基线

### 1.1 CLI 启动模式

**官方能力**（参考上游 Claude Code）：
- 交互模式（默认 TUI）
- `--print / -p` 非交互单 shot
- `--continue / -c` 续上次会话
- `--resume / -r [id]` 按 session id resume，或交互 picker
- `--session-id <uuid>` 指定 session id
- `--fork-session` resume 时 fork 新 session
- `--from-pr [value]` 从 PR 恢复
- `--worktree / -w` 创建 git worktree
- `--bare` 最小模式（跳过 hooks/LSP/plugin sync 等）
- `--ide` 自动连 IDE
- `--tmux` 创建 tmux session

**个人版当前状态**：
- ✅ 全部 50+ CLI 选项**保留**（见附录 B）
- ⚠️ `--from-pr` 需要 GitHub 集成，个人版可能依赖
- ⚠️ `--ide` 需要 IDE extension，未验证
- ✅ `-p` / `-c` / `-r` / `--worktree` 都是本地操作，应可用
- ✅ `--bare` 模式明确不依赖 hosted credentials（"hosted credentials are never read"）

**验证方式**：
- M8 系列 slash command 全量测试中覆盖
- 部分需要单独 e2e（如 `--from-pr` / `--ide` / `--tmux`）

**风险**：
- `--from-pr` 可能调用 GitHub API（hosted）—— 应在测试中验证或确认隐藏
- `--ide` 协议可能依赖官方 hosted bridge ——需查

**用户确认**：（pending — 用户 review 后填）

---

### 1.2 Slash commands

**官方能力**：未确切清单（已 fork），但 mossen 仓库当前 `commands/` 下有 **89 个子目录 + 12 个顶层 .ts = 101 个文件入口**（已 3 次 snapshot 验稳定）。

**M0.3 关键发现（runtime 真注册 vs 文件入口）**：通过 bun import `commands.ts` 调 `getCommands(cwd)` 拿真注册数组——**45 个真注册命令**（已过滤掉 hosted/console only 的 56 个）。落在 `harness_slash_command_matrix.json`，按 5 类分类：
- no_side_effect: 13
- writes_config: 17
- high_risk_tool: 6
- external_service: 1
- uncategorized: 8（17.7% 待人工 review，仍在 §1.1.2 阈值内）

**个人版当前状态**：
- 100 个入口存在（见附录 A 全清单）
- 部分明显是 **hosted-only 残留**，应隐藏或删除：
  - `install-github-app`, `install-slack-app` — 官方 marketplace
  - `oauth-refresh` — OAuth 流程
  - `share`, `mobile`, `desktop` — 跨设备/分享
  - `feedback`, `bughunter`, `perf-issue`, `release-notes`, `stickers` — 上游运营功能
  - `teleport`, `remote-env`, `remote-setup` — 远程能力
  - `chrome` — Mossen in Chrome 集成
  - `passes` — 似上游账户系统
- 部分是 **Mossen 个人版核心**，必须保留 + 可用：
  - `help`, `clear`, `compact`, `context`, `cost`, `model`, `permissions`, `mcp`, `memory`, `skills`, `plugin`, `lang`, `theme`, `vim`, `resume`, `rewind`, `session`, `status`, `init`, `init-verifiers`, `commit`, `review`, `security-review`, `doctor`, `agents`, `output-style`, `proactive`, `fast`, `effort`, `add-dir`, `worktree`-related
- **不确定**（需用户决定隐藏 / 保留 / 改造）：
  - `auto-mode`, `advisor`, `brief`, `assistant`, `tasks`, `thinkback`, `voice`, `ide`, `keybindings`, `hooks`, `pr_comments`, `branch`, `tag`, `diff`, `files`, `copy`, `export`, `summary`, `usage`, `stats`, `cost`, `extra-usage`, `session`, `rate-limit-options`, `reset-limits`, `mock-limits`, `privacy-settings`, `setup-token`, `setup`, `terminalSetup`, `login`, `logout`, `auth`, `sandbox-toggle`, `ant-trace`, `break-cache`, `btw`, `good-mossen`, `heapdump`, `profile`, `debug-tool-call`, `ctx_viz`, `env`, `exit`, `onboarding`, `rename`, `commit-push-pr`, `autofix-pr`, `issue`, `theme`, `thinkback-play`, `reload-plugins`, `release-notes`, `update`, `upgrade`, `version`, `passes`

**验证方式**（按 §C.3 要求）：
- 必须先**实际启动 mossen 跑一遍 `/help`** 拿到当前可见命令清单（不能只 grep 源码）
- 所有可见命令落 `harness_slash_command_matrix.json`，按 §C.3 表分类（无副作用 / 写配置 / 外部服务 / 高风险 / 暂不支持）
- 安全命令（无副作用）必须真跑（M8.2）
- 写配置命令必须 fixture HOME 内真跑（M8.3）
- 外部服务命令默认隐藏或 mock，不许真访问官方服务（M8.3）
- 高风险命令必须经过权限测试（M8.3）
- 暂不支持命令必须隐藏，可见即 fail（M8.4）

**风险**：
- 100 个命令逐个分类工作量大（≈ 5-10 hr）
- 部分命令 hosted 残留可能在用户体验上漏掉，导致用户敲了不该出现的命令报错

**用户确认**：（pending — 用户 review 后明确 hosted-only 命令是否要这次清理还是只标隐藏）

---

### 1.3 Agent loop

**官方能力**：用户消息 → 模型 → tool_use → tool_result → 模型 → final response，含流式输出、错误恢复、多轮上下文、中断恢复。

**个人版当前状态**：
- ✅ 主循环在 `query.ts`（已确认）
- ✅ 流式：`StreamEvent` / `RequestStartEvent` 类型存在
- ✅ 错误恢复：`createAssistantAPIErrorMessage` 存在
- ✅ Microcompact + autoCompact + reactiveCompact + contextCollapse 都有
- ⚠️ 中断/cancel：未确认入口
- ⚠️ Plan mode 切换：`ExitPlanModeV2Tool.ts` 存在，permission mode 含 `'plan'`

**验证方式**：
- M1.1-M1.4（第一批）+ M1.5-M1.7（追加 Codex 门禁：流式 + 工具失败恢复 / 中断 / 计划模式不误切换）
- 必须用确定性 marker（不能靠 LLM 字面）
- 必须收集 session log JSONL 验 tool_use / tool_result event

**风险**：
- M1.5（流式 + 工具失败）需要构造工具内部失败的 fixture
- M1.6（中断）需要发送 SIGINT 给子进程，stdin 操作复杂
- M1.7（plan 不误切换）需要造一个会让模型想切 plan 的 prompt，验证不被切

**用户确认**：（pending）

---

### 1.4 上下文管理

**官方能力**：token 统计、`/context` 视图、auto-compact、manual `/compact`、compact 后语义保留、resume 后上下文边界、statusline ctx 显示。

**个人版当前状态**：
- ✅ `analyzeContext.ts` 存在，含完整 4 类 token 分类
- ✅ `microCompact.ts` + `autoCompact.ts` + `reactiveCompact.ts` 都在
- ✅ `compact` command 存在
- ✅ statusline 含 ctx 显示（之前 chat_tui smoke 输出可见 `ctx: 0%`）
- ✅ 已彻底清 anthropic-hosted countTokens API（commit 8ddf54e）—— 现在只走 `getSmallFastModel()` 的 `.create`

**验证方式**：
- M4.1 auto-compact 触发 + 语义保留（第一批）
- M4.2 `/context` 显示真 token 占比（第一批）
- M4.3 manual `/compact`（追加）
- M4.4 statusline ctx 准确（追加）
- M4.5 resume 后上下文边界（追加）

**风险**：
- M4.1 触发 auto-compact 需要构造长对话 + ctx 阈值小，setup 复杂
- M4.5 resume 与新窗口同目录的边界——这是用户实际 pain point（见 §C.5），必须区分清楚

**用户确认**：（pending）

---

### 1.5 记忆系统

**官方能力**：用户级 / 项目级 / 本地级 memory 文件加载，跨 session 持久化，重开窗口同目录自动加载，resume 上下文 vs 项目记忆区分。

**个人版当前状态（关键纠正）**：
- ⚠️ **"4 类 memory" 在代码里是按 _type_ 分（user / feedback / project / reference），不是按 _scope_ 分（Project / Local / User / ProjectRules）**
- 实际枚举：`memdir/memoryTypes.ts:14` `MEMORY_TYPES = ['user', 'feedback', 'project', 'reference']`
- 这是 frontmatter `type:` 字段的 4 种取值，不是 4 个加载位置
- **加载 scope** 是另一个维度，由 `mossenmd.ts` 决定（含 InstructionsMemoryType / 项目 / 本地 / 用户级目录）—— 需进一步调研
- ✅ `mossenmd.ts` 中有 `getMemoryFiles()` loader
- ✅ INDIVIDUAL 模式 vs COMBINED 模式（私人 vs 团队）—— 个人版应是 INDIVIDUAL

**验证方式**：
- M5.1（写事实 → 重启 → 取出，第一批） — **契约写法对**
- M5.2（4 类 memory 真各自加载，第一批） — **契约写错**：marker 应该按 type 分（user/feedback/project/reference frontmatter），但写场景时假设是按 scope 分（Project/Local/User/ProjectRules 文件位置）。**需修正**。
- M5.3（跨 worktree 共享，第一批） — 契约对
- M5.4（新窗口同目录项目记忆自动加载，追加 P0）
- M5.5（resume 上下文 vs 项目记忆，追加 P0）
- M5.6（memory 文件变更 reload，追加 P1）

**风险**：
- **M5.2 必须修契约**——这是 M0.1 第一个发现的硬错误
- M5.4 / M5.5 区分需要先理解 mossen 的 "session log" vs "memory file" 物理路径

**用户确认**：（pending — 是否同意 M5.2 契约修正）

---

### 1.6 Skill 系统

**官方能力**：bundled / user / project / local skill 4 来源，发现、加载、热重载、调用、token 注入、错误展示。

**个人版当前状态（关键发现）**：
- ⚠️ **bundled skills 当前只有 1 个**：`skills/bundled/verify/SKILL.md`（其它 bundled 在哪？或者删了？）
- ✅ `loadSkillsDir.ts` + `bundledSkills.ts` + `mcpSkillBuilders.ts` 全部存在
- ✅ `commands/skills/` 目录存在（应该是 `/skills` slash command）
- ✅ MCP skills 通过 `mcpSkills.ts` 注册（动态发现）
- ⚠️ 4 来源全部支持但需要在 fixture 下验证

**验证方式**：
- M6.1（/skill 列表非空，第一批）
- M6.2（bundled skill 调用 e2e，第一批）—— **唯一 bundled 是 `verify`，必须用它**
- M6.3（skill 改文件 → 重启 → 反映，第一批）
- M6.4（4 来源 bundled/user/project/local，追加 P0）
- M6.5（skill 指令真注入 agent loop，追加 P0）
- M6.6（坏 skill / 错 frontmatter，追加 P1）

**风险**：
- 只有 1 个 bundled skill 验"列表"够，但验"4 来源"必须 fixture 造另外 3 类（user / project / local）
- M6.5 需要验"skill 内容真影响后续 tool/回复行为"——比 M6.2 更强

**用户确认**：（pending）

---

### 1.7 MCP

**官方能力**：stdio / sse / http transport，配置 scope（user/project/local），list、tool call、失败 server、超长输出截断、禁用策略。

**个人版当前状态**：
- ✅ Transport 4 种：`stdio` / `sse` / `http` / **`hosted-proxy`**（最后一个是上游 hosted 残留）
- ⚠️ **`hosted-proxy` 应禁用**（个人版不应有 hosted MCP proxy）—— 需 audit
- ✅ `services/mcp/client.ts` 完整 + `mcp_truncation_failsafe` smoke 已加（commit 39b65d7）

**验证方式**：
- M3.1 mock server 注册 + /mcp list（第一批）
- M3.2 MCP tool 调用真执行（第一批）
- M3.3 超长输出真截断 ✅（已通过）
- M3.4 配置 scope 和失败 server（追加 P0）
- M3.5 tool schema + 参数校验（追加 P1）

**风险**：
- M3.1 / M3.2 需要写一个最简 mock MCP server（python stdio JSON-RPC）
- `hosted-proxy` transport 是否真能在个人版触发——需要单独 audit（不是这次 scope）

**用户确认**：（pending — `hosted-proxy` transport 是否本次审计 + 隐藏？还是后续单独清）

---

### 1.8 Plugin

**官方能力**：本地 plugin、marketplace、list、command、reload、disable、scope。

**个人版当前状态**：
- ✅ `utils/plugins/loadPluginCommands.ts` + `loadPluginHooks.ts` + `loadPluginAgents.ts` 都在
- ✅ `commands/plugin/` + `commands/reload-plugins/` 存在
- ⚠️ Marketplace 路径未确认——可能 hosted 残留
- ⚠️ Plugin 当前是否在仓库中真有装载——未确认

**验证方式**：
- M7.1 mock plugin 装 + /plugin list（第一批）
- M7.2 plugin command 真触发（第一批）
- M7.3 plugin reload/disable/scope（追加 P1）
- M7.4 plugin 失败隔离（追加 P1）

**风险**：
- 需要造一个 mock plugin 目录（最简结构）
- Marketplace 隐藏策略未明

**用户确认**：（pending）

---

### 1.9 权限

**官方能力**：6 种 permission mode + 显式 allow/deny + 配置规则 + 危险命令拦截 + 模式切换不应被模型擅自改变。

**个人版当前状态（CLI --help 确认）**：
- 6 种 mode：`acceptEdits` / `bypassPermissions` / `default` / `dontAsk` / `plan` / `auto`
- ✅ `--allowedTools` / `--disallowedTools` / `--allow-dangerously-skip-permissions` / `--dangerously-skip-permissions` 都在
- ✅ `commands/permissions/` 存在
- ✅ `tools/ExitPlanModeTool/ExitPlanModeV2Tool.ts` 存在（plan 模式专属）

**验证方式**：
- M2.1 危险工具 deny（第一批）
- M2.2 allow 后真执行（第一批）
- M2.3 /permissions 配置真生效（第一批）
- M2.4 4 种 mode 全覆盖（追加 P0）—— **但实际是 6 种，需修正**
- M2.5 危险 Edit/Write 权限（追加 P0）
- M2.6 权限配置 scope（追加 P0）

**风险**：
- **M2.4 契约写"4 种"，实际是 6 种**——需修正（追加 `dontAsk` 和 `auto`）
- M2.1 / M2.2 需要 stdin 模拟用户敲 deny/allow，可能复杂

**用户确认**：（pending — 是否同意 M2.4 改为 6 种 mode）

---

### 1.10 Backend（custom backend / OAuth）

**官方能力**：OpenAI-compatible / custom backend、模型 override、API 错误、auth 缺失提示。

**个人版当前状态（关键确认）**：
- ✅ Custom backend 是个人版**唯一**支持的 backend
- env: `MOSSEN_CODE_USE_CUSTOM_BACKEND` + `MOSSEN_CODE_CUSTOM_BASE_URL` + `MOSSEN_CODE_CUSTOM_API_KEY` / `MOSSEN_CODE_CUSTOM_AUTH_TOKEN`
- ✅ **Built-in OAuth 已禁用**：`cli/handlers/auth.ts:138` 明确报错 "Built-in account flow is disabled in Mossen. Configure MOSSEN_CODE_CUSTOM_BASE_URL..."
- ⚠️ 还有 47 处 `MOSSEN_CODE_USE_BEDROCK/VERTEX/FOUNDRY` env 残留 —— 上游 hosted 路径，本次不动（前面已记录）
- ⚠️ `MOSSEN_CODE_ENABLE_HOSTED_AUTH_ADAPTER` 是逃生口，正常个人版不应启用

**验证方式**：
- M9.1 custom backend agent loop（追加 P0）
- M9.2 auth 缺失提示（追加 P0）
- M9.3 model override / fallback（追加 P1）

**风险**：
- M9.1 需要真配 custom backend env（用户的真 API key）—— **会 burn API 额度**
- 必须确认错误消息明确指向 custom backend，不引导官方登录

**用户确认**：（pending — 是否提供测试用 custom backend 配置 + 限额）

---

### 1.11 语言

**官方能力**：zh / en / auto / toggle，footer / tip / slash 描述 / 错误 / 权限卡片 / statusline 都应一致语言。

**个人版当前状态**：
- ✅ `commands/lang/` 存在，参数 `[zh|en|auto]`
- ⚠️ Language 设置存哪未完全确认——`commands/lang/index.ts` 只暴露 entry，实际 setter 在 `lang.js`（懒加载）
- ✅ `hooks/useVoice.ts:119` 提到 `settings.language`——存在 settings 中

**验证方式**：
- M11.1 zh/en/auto/toggle 全链路（追加 P0）
- M11.2 英文模式遇中文输入（追加 P1）

**风险**：
- 必须验 footer / tip / slash 描述 / 权限卡 / statusline / 错误 全部跟随语言切换
- chat_tui smoke 之前 transient 失败的根因之一（LLM 字面 + 中文）—— M11.1 应该有更稳定的 fixture

**用户确认**：（pending）

---

### 1.12 长任务

**官方能力**：30 分钟+ 任务有 heartbeat / 进度 / 失败总结，timeout 显式归因，刷新/重启后历史可见。

**个人版当前状态**：
- ⚠️ 长任务专门组件未确认——可能在 `tasks/` 或 `services/`
- ⚠️ Heartbeat 实现未明
- ⚠️ Resume 后历史 event 是否完整可见——需查 session log 持久化

**验证方式**：
- M10.1 30 分钟任务不中断（追加 P0，**fake slow tool 即可**）
- M10.2 timeout 可见且归因（追加 P0）
- M10.3 嵌套子任务恢复（追加 P1）

**风险**：
- M10.1 fake slow tool 写法：构造一个 sleep 30 min 的 Bash 工具调用，验中间有可见 heartbeat
- M10.2 必须区分"timeout 失败" vs "静默 idle"

**用户确认**：（pending）

---

## 2. 关键发现 + 误解纠正

### 2.1 我之前在 `harness全链路测试.md` 写错的契约

| 测试 ID | 错误 | 正确 | 影响 |
|---|---|---|---|
| M5.2 | 假设 4 类 memory = Project/Local/User/ProjectRules（按 scope）| 实际 4 类 = user/feedback/project/reference（按 type，frontmatter 字段） | 必须修 §3.5 M5.2 的"前置"和"步骤" |
| M2.4 | 假设 permission 4 种 mode | 实际 6 种：default / plan / acceptEdits / bypassPermissions / dontAsk / auto | 必须修 §C.1 M2.4 关键验收 |
| 测试范围 | 假设 slash command 45+ | 实际 101 个入口（89 dirs + 12 ts，3 次 snapshot 一致） | M8.1 工作量翻倍，必须落 JSON 矩阵 |
| Bundled skills | 假设有 N 个 | 实际只有 1 个 (`verify`) | M6.2 必须用 `verify`；M6.4 验 4 来源时其他 3 个用 fixture |

### 2.2 hosted 残留（不本次清，但记下来）

- `MOSSEN_CODE_USE_BEDROCK/VERTEX/FOUNDRY` 47 处引用
- `MCP transport 'hosted-proxy'`
- `MOSSEN_CODE_ENABLE_HOSTED_AUTH_ADAPTER` 逃生口
- `commands/install-github-app/` `commands/install-slack-app/` `commands/passes/` 等明显 hosted 命令
- `commands/share/` `commands/mobile/` `commands/desktop/` `commands/teleport/` `commands/remote-env/` `commands/remote-setup/` `commands/chrome/`
- 这些不是本次"测 e2e"目标，但 M8.4（不应开放命令隐藏）会触及

### 2.3 测试基础设施先决条件

按 §1.1.2 / §1.1.3，必须先建：
- `/tmp/mossen-harness/<test-id>/` fixture root 结构
- 独立 HOME / MOSSEN_CONFIG_HOME / XDG_CONFIG_HOME 模板
- `artifacts/` 目录约定（command/env/stdout/stderr/exit_code/session_log/assertions.json/mutation.diff）
- harness runner 契约（让 smoke_check.py 能聚合 assertions.json）

**这些是 M0.2 / M0.3 / M0.4 的工作**，必须在 M1.x 开测前完成。

---

## 3. 衍生待办（必须回写 `harness全链路测试.md`）

| # | 待办 | 影响位置 |
|---|---|---|
| W1 | 修 M5.2 契约：marker 按 type 分（4 个 frontmatter `type:` 各种），不按 scope 分 | §3.5 M5.2 |
| W2 | 修 M2.4 契约：6 种 permission mode（不是 4 种） | §C.1 M2.4 |
| W3 | M6.2 明确用 `verify` skill | §3.6 M6.2 |
| W4 | M8.1 工作量更新：100 个 slash command 全量验证（不是 45+） | §C.3 |
| W5 | 标注 §2.2 表里 M3.4 应包含 "audit hosted-proxy transport 隐藏策略" | §C.1 M3.4 |
| W6 | M0.2/M0.3/M0.4 必须在任何 M1.x 之前完成 | §C.1 + §4 SOP |

---

## 4. 数据来源 + 调研方法

| 数据 | 来源 |
|---|---|
| CLI 选项 | `./run-mossen.sh --help` 全文（附录 B） |
| Slash command 入口 | Python `os.path.isdir` 统计（snapshot 3 次：89 dirs + 12 ts，结果稳定，存于 `/tmp/mossen-harness/M0.1/artifacts/snapshots.txt`） |
| Bundled skills | `find skills/bundled -name "SKILL.md"` |
| MCP transports | `grep transportType services/mcp/client.ts` |
| Permission modes | CLI --help `--permission-mode` choices |
| Memory 4 类 | `Read memdir/memoryTypes.ts` |
| Custom backend env | `grep MOSSEN_CODE_USE_CUSTOM_BACKEND utils/customBackend.ts` |
| Hosted auth 禁用证据 | `grep "Built-in account flow is disabled" cli/handlers/auth.ts` |
| Plugin loader 入口 | `grep loadPluginCommands utils/plugins/` |

**未做**（留给 M0.2-M0.4 / 后续）：
- 真启动 mossen 跑 `/help` 拿到**实际可见**的 slash command 列表（区别于源码中所有入口）
- Session log 物理路径确认（`~/.mossen/projects/...`?）
- Resume 与新窗口同目录的真实加载流程
- Hosted-proxy transport 在个人版能否触发

---

## 5. 变更日志

| 日期 | 变更 | by |
|---|---|---|
| 2026-04-25 | 初稿（M0.1 第一稿，源码静态调研基线） | Claude |

---

## 附录 A: 100 个 Slash Command 全清单（按目录）

### A.1 子目录入口（88 个）

```
add-dir, agents, ant-trace, assistant, autofix-pr, backfill-sessions, branch,
break-cache, btw, bughunter, chrome, clear, color, compact, config, context,
copy, cost, ctx_viz, debug-tool-call, desktop, diff, doctor, effort, env,
exit, export, extra-usage, fast, feedback, files, good-mossen, heapdump, help,
hooks, ide, install-github-app, install-slack-app, issue, keybindings, lang,
login, logout, mcp, memory, mobile, mock-limits, model, oauth-refresh,
onboarding, output-style, passes, perf-issue, permissions, plan, plugin,
pr_comments, privacy-settings, proactive, profile, rate-limit-options,
release-notes, reload-plugins, remote-env, remote-setup, rename, reset-limits,
resume, rewind, sandbox-toggle, session, share, skills, stats, status,
stickers, summary, tag, tasks, teleport, terminalSetup, theme, thinkback-play,
thinkback, upgrade, usage, vim, voice
```

### A.2 顶层独立 .ts 文件（12 个）

```
advisor, brief, commit-push-pr, commit, createMovedToPluginCommand,
init-verifiers, init, insights, proactive, review, security-review, version
```

### A.3 分类初判（待 M0.3 真跑 /help 后修正）

| 分类 | 命令（举例） | 处理 |
|---|---|---|
| 核心保留 | help, clear, compact, context, model, permissions, mcp, memory, skills, plugin, lang, theme, vim, resume, init, commit, review, doctor, agents, output-style, status, cost, fast, effort | 必须可用，M8.2/M8.3 真跑 |
| Hosted 残留疑似 | install-github-app, install-slack-app, share, mobile, desktop, chrome, teleport, remote-env, remote-setup, oauth-refresh, passes, feedback, bughunter, perf-issue, release-notes, stickers, autofix-pr, pr_comments | M8.4 验隐藏 |
| 不确定 | auto-mode, advisor, brief, assistant, tasks, thinkback, voice, ide, keybindings, hooks, branch, tag, diff, files, copy, export, summary, usage, stats, extra-usage, session, rate-limit-options, reset-limits, mock-limits, privacy-settings, setup-token, terminalSetup, login, logout, sandbox-toggle, ant-trace, break-cache, btw, good-mossen, heapdump, profile, debug-tool-call, ctx_viz, env, exit, onboarding, rename, commit-push-pr, issue, theme, thinkback-play, reload-plugins, update, upgrade, version, advisor, brief, init-verifiers, insights, security-review | 等用户拍板 |

---

## 附录 B: 50+ CLI 选项（来自 --help）

```
--add-dir <dirs...>                      允许工具访问的额外目录
--agent <agent>                          会话使用的 agent
--agents <json>                          自定义 agents JSON
--allow-dangerously-skip-permissions     允许 bypass 但不默认启用
--allowedTools, --allowed-tools <tools>  允许的工具
--append-system-prompt <prompt>          追加 system prompt
--bare                                   最小模式（hosted credentials 永远不读）
--betas <betas...>                       Beta headers（API key 用户）
--brief                                  启用 SendUserMessage agent-to-user 通信
--chrome                                 Mossen in Chrome 集成
-c, --continue                           续上次会话
--dangerously-skip-permissions           Bypass 所有权限
-d, --debug [filter]                     debug 模式
--debug-file <path>                      debug 日志文件
--disable-slash-commands                 禁用所有 skills
--disallowedTools                        禁用的工具
--effort <level>                         努力级别 low/medium/high/max
--fallback-model <model>                 默认模型 overload 时的 fallback
--file <specs...>                        启动时下载 file_id:relative_path
--fork-session                           resume 时 fork 新 session
--from-pr [value]                        从 PR 恢复会话
-h, --help                               帮助
--ide                                    自动连 IDE
--include-hook-events                    包含 hook 事件
--include-partial-messages               包含部分消息
--input-format <format>                  text 或 stream-json
--json-schema <schema>                   结构化输出 JSON Schema
--max-budget-usd <amount>                最大花费上限
--mcp-config <configs...>                MCP servers JSON 文件
--mcp-debug                              [deprecated] MCP debug
--model <model>                          模型
-n, --name <name>                        session 名
--no-chrome                              禁用 Chrome 集成
--no-session-persistence                 不保存 session
--output-format <format>                 text/json/stream-json
--permission-mode <mode>                 6 种 mode
--plugin-dir <path>                      加载 plugin 目录（可重复）
-p, --print                              非交互单 shot
--proactive                              主动自治模式
--replay-user-messages                   stream-json 下回显用户消息
-r, --resume [value]                     恢复会话
--session-id <uuid>                      指定 session id
--setting-sources <sources>              user/project/local
--settings <file-or-json>                额外 settings
--strict-mcp-config                      只用 --mcp-config 的 server
--system-prompt <prompt>                 自定义 system prompt
--tmux                                   创建 tmux session（需要 --worktree）
--tools <tools...>                       可用工具列表
--verbose                                覆盖 verbose 配置
-v, --version                            版本号
-w, --worktree [name]                    创建 git worktree
```

子命令（10 个）：

```
agents [options]      列出 agents
auth                  管理认证
auto-mode             自动模式分类器配置
doctor                自动更新器健康检查
install [target]      安装 native build
mcp                   配置 MCP servers
plugin|plugins        管理 plugins
setup-token           配置可复用 backend credentials
update|upgrade        检查更新
```

---

**结束**
