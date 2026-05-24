# Phase 3.5 Round 3：Commands / Skill / MCP 三大系统的 UI

> **目标读者**：本地 DeepSeek V4 Flash（通过 ds4-server 运行 Mossen 自身或其他 agent 来执行本文档）。
> **完整 5 阶段索引**：`/Users/allen/Documents/rustmossen/PLAN_PRODUCTION.md`
> **本阶段位置**：Phase 3.5 的第三轮（前置：`phases/03b-experience.md` Round 1 已完成、`phases/03c-experience-round2.md` Round 2 已完成）。
> **是否要先做 Round 2？** 建议 Round 2 的 P0（3c-1/3c-2/3c-3）做完再开 Round 3，因为 Tool result 视觉不到位时其他系统的 UI 也很难判断。但 Round 2 的 P1（3c-4/3c-5/3c-6）和本轮可以并行/穿插。

---

## 1. 这一轮为什么存在（必读）

### 1.1 之前两轮覆盖了什么

| 轮次 | 关注点 |
|------|------|
| **Round 1**（03b） | spinner 叶子动画、permission 弹窗结构化、TaskListV2 接通、Phase 4-1 测试全修 |
| **Round 2**（03c） | tool result 渲染、Edit/Write diff、permission UI 展示参数、TodoWrite 卡死、子 agent 嵌套、Ctrl+C 稳定 |

两轮加起来全是 **transcript 主屏渲染 + 单工具体验**。

### 1.2 三个大系统的 UI 仍然是黑洞

Mossen 是 Mossen Code 翻版，原版有 3 个**独立的子系统**有自己专属的用户交互面：

| 子系统 | 用户面 | Rust 端实现 | UI 接通情况 |
|---|---|---|---|
| **Commands**（slash 命令） | `/help`、`/clear`、`/resume`、`/compact`、`/tasks`、`/model`、`/theme`、`/output-style`、`/memory`、`/status`、`/mcp`、`/cd` 等共 ~30 个 | `mossen-commands/`、`mossen-cli/handlers/` | 🟡 命令分发可能能跑，但**专属 UI 几乎没接**：没 `/` typeahead 提示、没 `/help` 帮助页、没 `/resume` session 选择器 |
| **Skill** | `/<skill-name>` 触发 user-invocable skill；`paths` frontmatter 动态激活 conditional skill；MCP-skill | `mossen-skills/`、`mossen-tools/agent_tool/`（fork agent）、`mossen-cli/src/system_prompt.rs::gather_memory_text` | 🟡 后台加载能跑，**前台用户面没接**：没 skill 列表、没"已发现新 skill"提示、没 skill 调用反馈 |
| **MCP**（Model Context Protocol） | 外部 server 注入的 tool / resource / prompt；channelAllowlist 审批；OAuth / XAA 登录 | `mossen-mcp/` | 🟡 协议层在，**UI 端几乎全黑**：没 MCP server 状态、没 tool 来源标签、没 channelAllowlist 弹窗、没 OAuth 流程 |

audit 烤机时这些系统没单独验过 —— **不是因为它们没问题，是因为 audit 烤机用例不覆盖**。Round 3 把它们补上。

### 1.3 本阶段要解决什么

按用户实际接触频率排：

| 优先级 | Task | 一句话目标 |
|---|---|---|
| 🔴 P0 | 3d-1 | `/` 触发 slash 命令 typeahead 浮动列表（**最高频用户面**） |
| 🔴 P0 | 3d-2 | 核心 slash 命令的专属 UI：`/help` `/clear` `/compact` `/status` |
| 🟡 P1 | 3d-3 | `/resume` session 选择器 + `/tasks` 任务列表 |
| 🟡 P1 | 3d-4 | Skill 调用反馈（`/<skill-name>` 触发时主屏出现 skill 调用块） |
| 🟡 P1 | 3d-5 | Skill 动态发现通知（新 skill 被发现/激活时用户能看到） |
| 🟢 P2 | 3d-6 | MCP server 状态指示器（状态栏 + `/mcp` 详情面板） |
| 🟢 P2 | 3d-7 | MCP tool 来源标签（每个 MCP 工具调用显示 `[server-name]`） |
| 🟢 P2 | 3d-8 | MCP channelAllowlist 首次连接审批弹窗 |

8 个 task。P0/P1 是核心（用户每天都会撞到），P2 是有 MCP server 时才会用到。

### 1.4 本阶段完成判定

- workspace build / test 不退化
- 用户在 TTY 验证：
  - 输 `/` 立即看到命令列表
  - `/help` 显示完整命令清单 + 描述
  - `/clear` 二次确认后清屏
  - `/compact` 显示压缩进度
  - 用户调用 skill 时主屏有可识别的 UI 反馈
  - （如果配了 MCP）状态栏看得到 MCP 连接 + 工具调用带来源标签

### 1.5 本阶段不要做的事

- **不要**改命令系统的内部分发逻辑（在 `mossen-commands/`），本轮只接 UI
- **不要**实现新的 slash 命令（已有的就够了）
- **不要**重写 MCP 协议层
- **不要**碰 `components/` / `terminal-framework/` 子树的删除
- **不要**和 Round 2 的改动冲突：先 grep 确认 round 2 改的文件里没你要再改的同一行
- **不要**追求 100% UI parity 对照 TS 版

---

## 2. 阅读约定

### 2.1 角色与权限

你是 Rust 工程师。**可以**：`Read` / `Edit` / `Write` / `Bash`。
**绝对不能**：`git push` / `git reset --hard` / `rm -rf` / 修改 `/Users/allen/Documents/ds4/`。

### 2.2 执行节奏

一次一个 task，5 段结构：背景 / 位置 / 改动 / 验证 / 完成判定 / 回滚。

**特别提醒**：本轮所有 task 都涉及视觉效果。**所有"看着对不对"必须停下来让用户在 TTY 里看**，agent 在非 TTY 环境只能 build + 简单 oneshot 验证。

### 2.3 卡住时

立即停下报告，不要猜。**特别**：本轮涉及 3 个子系统，每个内部都有复杂状态。**改之前必须先 grep 审计现状**，不要假设某个功能"应该"在哪。

### 2.4 命令前缀

默认 `cd /Users/allen/Documents/rustmossen`。

### 2.5 基线确认

```bash
cd /Users/allen/Documents/rustmossen
cargo check --workspace 2>&1 | tail -3
cargo test --workspace --no-fail-fast 2>&1 | grep "test result:" | tail -10
```

期望：build `Finished`、测试 0 failed。

### 2.6 沟通规则

- 开始：`正在做 3d-X：<标题>`
- 完成：`3d-X 完成。验证通过：<关键输出>`
- 失败：`3d-X 失败 / 卡住。详情：<错误>。已停下。`

---

## 3. 第 0 件事：审计现状

⚠️ **每个 task 开做前先跑对应 grep 看现状**。本轮覆盖的 3 个系统都已经有底层代码，不是从零写，是接 UI。盲改会撞已有逻辑。

```bash
# Commands 现状
ls crates/mossen-commands/src/ 2>/dev/null | head
grep -rln "SlashCommand\|slash_command\|CommandRegistry" crates/mossen-cli/src crates/mossen-commands/src 2>/dev/null | head -10
grep -n "/help\|/clear\|/compact" crates/mossen-cli/src/repl.rs crates/mossen-tui/src/app.rs 2>/dev/null | head -10

# Skill 现状
ls crates/mossen-skills/src/ 2>/dev/null
grep -rln "discover_skill\|conditional_skill\|skillsLoaded\|skill_invocation" crates/ 2>/dev/null | head -10

# MCP 现状
ls crates/mossen-mcp/src/ 2>/dev/null
grep -rln "McpServer\|channelAllowlist\|channel_allowlist\|McpConnection" crates/ 2>/dev/null | head -10

# 之前 audit 文件
cat crates/mossen-cli/PLUGIN_RELOAD_AUDIT.md 2>/dev/null  # Phase 2-4 留下来的
cat crates/mossen-skills/CONDITIONAL_SKILL_AUDIT.md 2>/dev/null  # Phase 2-3
cat crates/mossen-tools/src/agent_tool/SKILL_DISCOVERY_AUDIT.md 2>/dev/null  # Phase 2-2
```

把找到的东西**记在心里**或者写到草稿里，再开始 3d-1。

---

## 4. Task 详情

### 3d-1 Slash 命令 typeahead 浮动列表 —— P0

#### 背景

用户输入 `/` 那一刻就应该浮出**所有可用 slash 命令的列表**，按字母前缀 fuzzy 匹配。这是 Mossen Code / Mossen 最高频的交互入口之一。

Rust 端现状：
- `crates/mossen-tui/src/hooks/typeahead.rs` 和 `unified_suggestions.rs` 已有 hook（Round 1 没接进 prompt input widget）
- `mossen-commands/` 应该有完整的命令注册表（grep 确认）

#### 位置

```bash
# 命令注册表
grep -rn "fn commands\|CommandRegistry::new\|all_commands" crates/mossen-commands/src crates/mossen-cli/src | head -10

# Prompt input widget
cat crates/mossen-tui/src/widgets/prompt_input.rs | head -30

# 已有的 suggestion / typeahead hooks
grep -n "Suggestion\|Typeahead" crates/mossen-tui/src/hooks/typeahead.rs crates/mossen-tui/src/hooks/unified_suggestions.rs | head -10
```

#### 改动

##### Step 1：拿到命令清单

在 `crates/mossen-tui/src/state.rs` 或 `app.rs::new()` 启动时，调用 `mossen_commands::registry::all_commands()` 之类的 API，拿到所有 slash 命令的：

```rust
pub struct SlashCommandInfo {
    pub name: String,        // 不带 / 前缀
    pub description: String,
    pub category: Option<String>,  // help / session / config / debug...
}
```

具体 API 名字以审计为准。如果 mossen-commands 没暴露这种"列全部"的接口，**停下报告**。

存在 AppState：

```rust
pub all_slash_commands: Vec<SlashCommandInfo>,
```

##### Step 2：在 prompt input 检测 `/`

`widgets/prompt_input.rs` 输入处理时，**当当前行起首是 `/` 且光标在第一段连续字符内**时：

- 把 `/` 之后的字符当 query
- fuzzy 匹配 `all_slash_commands`，按相关性排序
- 输出 `Vec<SlashCommandInfo>` 作为当前 suggestions

接到 `state.rs::AppState`：

```rust
pub slash_suggestions: Vec<SlashCommandInfo>,
pub slash_selected_idx: usize,
```

##### Step 3：渲染浮动列表

`app.rs::render` 在 prompt input 区域**上方**（不挡 messages 区）画一个简单浮动框，最多显示前 5 个：

```
┌──────────────────────────────
│ ▸ /help        显示所有命令
│   /clear       清空当前会话
│   /compact     手动压缩对话
│   /resume      恢复历史会话
│   /tasks       查看任务列表
└──────────────────────────────
> /he|
```

`▸` 表示当前选中。

##### Step 4：键盘控制

- ↑/↓ 切换 `slash_selected_idx`
- Tab 或 Enter（如果是只输了 `/` 几个字母没回车提交命令的话）→ 把当前选中的命令名补完到 prompt input
- Esc 关闭浮层

注意 Enter 的语义冲突：用户可能是想"选中这个命令"也可能是想"直接提交当前输入"。建议规则：
- 浮层打开 + 有 selected → Enter 补完命令名 + 关浮层
- 浮层关闭 → Enter 提交输入

#### 验证

```bash
cargo check --workspace 2>&1 | tail -3
```

TTY 视觉验证：在 prompt 输入 `/` 立即看到浮动列表；输入 `/he` 缩短到 `/help`；Tab 补完。

#### 完成判定

- build 过
- 用户验证：`/` 浮列表、↑/↓ 切、Tab 补完、Esc 关闭都正常

#### 回滚

```bash
git checkout -- crates/mossen-tui/src/app.rs \
                crates/mossen-tui/src/state.rs \
                crates/mossen-tui/src/widgets/prompt_input.rs
```

---

### 3d-2 核心 slash 命令的专属 UI（`/help` `/clear` `/compact` `/status`）—— P0

#### 背景

这 4 个是用户启动后**最常用的命令**，必须有视觉反馈：

- `/help` → 弹出/展开一个帮助页，列所有命令 + 描述 + 分类
- `/clear` → 二次确认 modal（防误清），确认后清空 messages
- `/compact` → 显示"正在压缩对话历史…"进度（可能压缩需要几秒到几十秒，必须有反馈）
- `/status` → 显示 model / cost / tokens / context window 使用 / mode / cwd 等会话状态

#### 位置

```bash
# 命令实际执行入口
grep -rn "fn run_command\|fn execute_command\|handle_slash" crates/mossen-cli/src crates/mossen-tui/src | head -10

# 已有的 ActiveModal 变体
grep -A 30 "pub enum ActiveModal" crates/mossen-tui/src/app.rs | head -40
```

#### 改动

##### 3d-2-A `/help`

在 `ActiveModal` 加变体 `HelpDialog`：

```rust
pub enum ActiveModal {
    // ... 原有
    HelpDialog,
}
```

渲染时，从 `all_slash_commands`（3d-1 加的）按 category 分组，画一个全屏 modal：

```
┌─ Mossen 帮助 ────────────────────────────────────────
│
│ 会话管理
│   /clear       清空当前会话
│   /resume      恢复历史会话
│   /compact     手动压缩对话历史
│
│ 信息
│   /help        显示本帮助页
│   /status      显示会话状态
│   /tasks       显示任务列表
│
│ 配置
│   /model       切换模型
│   /theme       切换主题
│   /output-style 切换输出风格
│   /memory      查看 / 编辑记忆
│
│ MCP / 插件
│   /mcp         MCP server 管理
│
│ Esc 退出 ─ ↑↓ 滚动
└─────────────────────────────────────────────────────
```

##### 3d-2-B `/clear`

在 `ActiveModal` 加变体 `ConfirmClear`，弹一个小确认：

```
┌─ 清空会话？─────────────────
│ 当前 N 条消息将被清空。
│ [Enter] 确认  [Esc] 取消
└────────────────────────────
```

确认后调 `app_state.messages.clear()`、`pending_assistant_idx = None`、`assistant_buf.clear()` 等。

##### 3d-2-C `/compact`

`/compact` 触发后端的手动压缩。Compact 不是瞬间的（要调 forked agent 跑摘要）。

在 `state.rs::AppState` 加：

```rust
pub compact_in_progress: bool,
pub compact_progress: Option<String>,  // 当前阶段："compacting" / "writing boundary" / ...
```

`/compact` handler 设 `compact_in_progress = true`，触发后端流程。在主屏顶部画一个 sticky banner：

```
🗜  Compacting conversation (12k → ~? tokens)...
```

后端进度通过 SdkMessage::CompactBoundary 或类似事件回传（应该已有），handle_engine_message 收到时更新 `compact_progress`，最后清空 banner。

##### 3d-2-D `/status`

加变体 `StatusDialog`，弹一个面板：

```
┌─ 状态 ──────────────────────────────────────
│
│ 模型       deepseek-v4-flash
│ Backend    custom (http://localhost:8000)
│ Context    18,432 / 500,000 tokens (3.7%)
│ Cost       $0.0000
│ Turns      8
│ CWD        /Users/allen/Documents/rustmossen
│ Mode       agent (Ctrl+P 切换 plan)
│
│ Esc 关闭
└────────────────────────────────────────────
```

数据从 `app_state` / `app_state.engine_config` 各处取。

#### 验证

```bash
cargo check --workspace 2>&1 | tail -3
```

TTY 验证 4 个命令各自弹出 UI 是否正确。

#### 完成判定

- build 过
- 4 个命令各自的 modal 都能弹出 + 关闭

#### 回滚

```bash
git checkout -- crates/mossen-tui/src/app.rs crates/mossen-tui/src/state.rs
```

---

### 3d-3 `/resume` session 选择器 + `/tasks` 任务列表 —— P1

#### 背景

- `/resume` 必须能列出历史 session 让用户选一个恢复
- `/tasks` 应该显示当前 TaskList（TodoWrite 加的任务）以及所有正在后台跑的 sub-agent

#### 位置

```bash
# session 存储
grep -rn "save_session\|session_id\|SessionStore\|sessions_dir" crates/mossen-cli/src crates/mossen-agent/src | head -10

# 已有的 PickerKind
grep -A 10 "pub enum PickerKind" crates/mossen-tui/src/app.rs
```

#### 改动

##### 3d-3-A `/resume`

复用 Round 1 已加的 `ActiveModal::Picker` + `PickerKind`（添新变体 `Resume`）。

session 列表 source：从 `~/.mossen/projects/<sanitized-cwd>/sessions/` 之类目录扫，每个 session 文件解析出：
- session_id
- 时间戳
- 第一条用户消息（前 80 字符）作为预览

picker UI：

```
┌─ Resume Session ─────────────────────────────
│ ▸ 2026-05-20 19:23  "帮我看一下 Cargo.toml"
│   2026-05-20 14:11  "解释下 dialogue.rs 的 turn 循环"
│   2026-05-20 09:45  "phase3 执行"
│   2026-05-19 22:08  "..."
│ ↑↓ 切换  Enter 恢复  Esc 取消
└─────────────────────────────────────────────
```

⚠️ 如果 mossen-cli 没有现成的 session 列表 API，**停下报告**。session 文件格式 / 恢复逻辑可能很复杂，不要私自发明。

##### 3d-3-B `/tasks`

Round 1 已经把 `TaskListV2Widget` 接到主屏底部了。`/tasks` 命令把它**升级为全屏 modal**，能看到更详细的：
- 每个 todo 的 in_progress / completed / pending 状态
- 每个 todo 的 activeForm / subject / description
- 当前后台跑的 sub-agent（teammate_states 里的）+ 状态

```
┌─ Tasks ──────────────────────────────────────
│
│ TodoWrite tasks (3)
│   ✓ 吃饭
│   ◐ 睡觉           ← in_progress
│   ○ 打豆豆
│
│ Background agents (2)
│   🟢 task_abc12    general-purpose (Running)
│   ⚪ task_def34    plan (Completed)
│
│ Esc 关闭
└─────────────────────────────────────────────
```

#### 验证

```bash
cargo check --workspace 2>&1 | tail -3
```

TTY 验证 `/resume` 和 `/tasks` 各自弹的 modal。

#### 完成判定

- build 过
- `/resume` 能列 session（如果有 session 存在）
- `/tasks` 能弹 TaskList + teammate 列表

#### 回滚

```bash
git checkout -- crates/mossen-tui/src/app.rs crates/mossen-tui/src/state.rs
```

---

### 3d-4 Skill 调用反馈 —— P1

#### 背景

用户输 `/<skill-name>`（不是内置 slash 命令，是某个 skill）时，应该：
- 在 transcript 里出现一个**可识别的 skill 调用块**（不是当作普通 user message 提交）
- 显示 skill 名 + 它被解释为什么 prompt（如果 skill 有 prompt template）

Mossen 的 `crates/mossen-skills/src/` 有 skill 加载逻辑。typeahead（3d-1）应该把 user-invocable skill 也包括在 `/` 候选里。

#### 位置

```bash
# Skill 触发路径
grep -rn "invoke_skill\|skill_invocation\|user_invocable" crates/mossen-skills/src crates/mossen-cli/src 2>/dev/null | head -10

# Skill 数据结构
grep -n "pub struct.*Skill\|pub enum SkillKind" crates/mossen-skills/src/*.rs | head -5
```

#### 改动

##### Step 1：把 user-invocable skill 也加进 `/` typeahead

在 3d-1 的 `all_slash_commands` 来源里，除了 `mossen-commands::all_commands()` 外，再加：

```rust
let skills = mossen_skills::all_user_invocable_skills(...);
for skill in skills {
    all_slash_commands.push(SlashCommandInfo {
        name: skill.name.clone(),
        description: skill.description.clone(),
        category: Some("Skill".to_string()),
    });
}
```

##### Step 2：skill 触发时 transcript 显示专门块

当 prompt input 提交 `/skill-name` 时，命令分发器识别这是 skill 而非内置命令，走 skill 调用路径。

在 transcript 里加一条 `MessageData`：

```rust
MessageData {
    message_type: MessageType::SkillInvocation,
    content: format!("/{}", skill_name),
    skill_template: Some(skill.prompt.clone()),  // 模型实际看到的 prompt
    ...
}
```

需要在 `widgets/message.rs::MessageType` 加变体 `SkillInvocation`，渲染时画一个简短的 banner：

```
🧩  /skill-name  (mossen-skills, fork_agent)
    ↓ resolving template:
    "...skill 模板首 50 字符..."
```

##### Step 3：skill 执行结果回灌

skill 如果是 fork agent 类型，它的输出本质是 sub-agent 输出，复用 Round 1 的 teammate widget。

#### 验证

```bash
cargo check --workspace 2>&1 | tail -3
```

TTY 验证：找一个项目里的 user-invocable skill（看 `~/.mossen/skills/` 或项目 `.mossen/skills/`），输 `/<name>` 看是否：
- `/` typeahead 包含它
- 触发后 transcript 有 SkillInvocation 块
- 执行结果回灌

#### 完成判定

- build 过
- 至少 1 个 skill 能通过 `/` 触发且有 UI 反馈

#### 回滚

```bash
git checkout -- crates/mossen-tui/src/ crates/mossen-cli/src/
```

---

### 3d-5 Skill 动态发现通知 —— P1

#### 背景

Phase 2-2 / 2-3 审计过 conditional skill 和 skill discovery 机制。如果"发现新 skill"或"激活条件 skill"实际触发（用户在某个目录跑 mossen 突然有新 skill 可用），**用户应该被告知**。

#### 位置

```bash
cat crates/mossen-tools/src/agent_tool/SKILL_DISCOVERY_AUDIT.md 2>/dev/null
cat crates/mossen-skills/CONDITIONAL_SKILL_AUDIT.md 2>/dev/null
grep -rn "discover_skill_dirs\|activate_conditional" crates/ 2>/dev/null | head -10
```

#### 改动

##### Step 1：审计当前发现路径

- 是否真的有动态发现？还是只在启动时扫一次？
- 如果有动态发现，触发点在哪？

##### Step 2：加 SkillDiscoveryEvent 到 SdkMessage

如果 Phase 2-2/2-3 审计后发现实际触发点在 dialogue.rs 的 tool result 处理：

```rust
SdkMessage::SkillDiscovered {
    skill_name: String,
    source: String,  // "conditional" / "directory_scan" / ...
    task_id: Option<String>,
},
```

dialogue.rs 在 `discover_skill_dirs_for_paths()` / `activate_conditional_skills_for_paths()` 返回新 skill 时 emit 一条。

##### Step 3：TUI 收到事件画提示

`app.rs::handle_engine_message`：

```rust
SdkMessage::SkillDiscovered { skill_name, source, .. } => {
    self.messages.push(MessageData {
        message_type: MessageType::System,
        content: format!("🧩 新 skill 可用：/{} （来源：{}）", skill_name, source),
        ...
    });
}
```

#### 验证

```bash
cargo check --workspace 2>&1 | tail -3
```

视觉验证：人为放一个 conditional skill 到当前目录，看 mossen 启动时是否提示。

#### 完成判定

- build 过
- 发现新 skill 时 transcript 有可见提示

#### 回滚

```bash
git checkout -- crates/mossen-agent/src/dialogue.rs \
                crates/mossen-tui/src/ \
                crates/mossen-skills/src/
```

---

### 3d-6 MCP server 状态指示器 —— P2

#### 背景

如果用户配了 MCP server（在 `~/.mossen/mcp.json` 或项目 `.mcp.json`），那些 server 的连接状态应该可见：
- 连接成功 / 失败 / 连接中 / needs-auth
- 注册的 tool / resource / prompt 数量

#### 位置

```bash
ls crates/mossen-mcp/src/
grep -n "pub enum.*State\|MCPServerConnection" crates/mossen-mcp/src/*.rs | head -5
grep -rn "mcp_state\|McpCliState" crates/mossen-tui/src/ crates/mossen-cli/src/ | head -10
```

#### 改动

##### Step 1：在 state.rs 加 MCP server 列表

```rust
pub struct McpServerStatus {
    pub name: String,
    pub state: McpConnectionState,  // Connected/Failed/Pending/NeedsAuth/Disabled
    pub tools_count: usize,
    pub last_error: Option<String>,
}

// AppState 加
pub mcp_servers: Vec<McpServerStatus>,
```

##### Step 2：从 engine / mossen-mcp 拉状态

定期（比如每 5 秒 tick）或事件驱动从 mcp 模块拉当前 server 状态。

##### Step 3：状态栏显示

`widgets/status_bar.rs`（如有）加 MCP 部分：

```
[ds4] | [main] | tokens 12.3k/500k | 🟢 2 MCP servers (43 tools) | Idle
```

`🟢` = 全连通；`🟡` = 部分；`🔴` = 全失败；省略 = 没配 MCP。

##### Step 4：`/mcp` 命令打开详情面板

```
┌─ MCP Servers ────────────────────────────
│ 🟢 filesystem  (stdio)        37 tools
│ 🟢 github      (sse)            6 tools
│ 🔴 weather     (http)           — error: connection refused
│
│ Esc 关闭   r 重连   d 禁用
└─────────────────────────────────────────
```

#### 验证

```bash
cargo check --workspace 2>&1 | tail -3
```

TTY 视觉验证（需配 MCP server）。如果用户没配 MCP，本 task 跳过视觉验证只 build 过即可。

#### 完成判定

- build 过
- 如果配了 MCP：状态栏有指示 + `/mcp` 弹详情

#### 回滚

```bash
git checkout -- crates/mossen-tui/src/ crates/mossen-cli/src/
```

---

### 3d-7 MCP tool 来源标签 —— P2

#### 背景

当 MCP server 注入工具（例如 `filesystem__list_files`），用户在主屏看到的工具调用应该标明来源 server，区别于内置工具（Bash / Read / Edit 等）。

#### 位置

```bash
grep -n "serialized_tool\|originalToolName\|normalizedName" crates/mossen-mcp/src/*.rs | head -5
grep -n "render.*tool_name\|tool_name.*render" crates/mossen-tui/src/widgets/message.rs | head -5
```

#### 改动

在 `widgets/message.rs::render_body` 渲染 tool 卡片时：

```rust
let display_name = if tool_name.contains("__") {
    // MCP tool format: "<server>__<tool>"
    let parts: Vec<&str> = tool_name.splitn(2, "__").collect();
    if parts.len() == 2 {
        format!("[{}] {}", parts[0], parts[1])
    } else {
        tool_name.clone()
    }
} else {
    tool_name.clone()
};
```

也可以用专门的 `[mcp]` 标签 + dim 颜色显示 server 名。

#### 验证

```bash
cargo check --workspace 2>&1 | tail -3
```

TTY 验证（需 MCP server 配置）。

#### 完成判定

- build 过
- MCP 工具调用主屏带 `[server-name]` 标签

#### 回滚

```bash
git checkout -- crates/mossen-tui/src/widgets/message.rs
```

---

### 3d-8 MCP channelAllowlist 首次连接审批弹窗 —— P2

#### 背景

项目级 MCP server（`.mcp.json` 在项目里）首次连接需要用户审批（**安全考虑**：项目目录里的 `.mcp.json` 可能被恶意 commit，自动连接危险）。

#### 位置

```bash
grep -rn "channelAllowlist\|channel_allowlist\|requires_approval" crates/mossen-mcp/src crates/mossen-cli/src | head -10
```

#### 改动

##### Step 1：审计当前审批路径

- 是否已有审批逻辑？
- 在哪个位置触发？

##### Step 2：加 ActiveModal::McpChannelApproval

```rust
pub enum ActiveModal {
    // ... 原有
    McpChannelApproval {
        server_name: String,
        config_path: String,
        tools_advertised: Vec<String>,
    },
}
```

弹窗显示：

```
┌─ 信任项目 MCP server？─────────────────────
│
│ 项目 .mcp.json 包含一个 MCP server：
│
│   name:   github-tools
│   path:   /Users/allen/Documents/proj/.mcp.json
│   tools:  fetch_pr, fetch_issue, create_pr ...
│
│ ⚠️ MCP server 可以读你的文件、执行命令、
│    访问网络。仅在你信任此项目时允许。
│
│ [a] 允许（仅此次）
│ [A] 永久允许
│ [d] 禁用
│ [Esc] 取消
└────────────────────────────────────────────
```

##### Step 3：连接前检查 + 弹窗触发

mcp 连接逻辑中，project-scope 的 server 在 connect 前 check 一次 allowlist：

- 已在 allowlist → 直接连
- 不在 → 触发 ActiveModal::McpChannelApproval，等用户决定

#### 验证

```bash
cargo check --workspace 2>&1 | tail -3
```

TTY 验证（需 .mcp.json 测试 fixture）。

#### 完成判定

- build 过
- 项目 MCP 首次连接弹窗 + 选择记录到 allowlist

#### 回滚

```bash
git checkout -- crates/mossen-tui/src/ crates/mossen-mcp/src/ crates/mossen-cli/src/
```

---

## 5. 阶段验收

### 5.1 build / test 不退化

```bash
cd /Users/allen/Documents/rustmossen
cargo build --workspace 2>&1 | tail -3
cargo test --workspace --no-fail-fast 2>&1 | grep "test result:" | tail -10
```

期望：build 0 error、test 0 failed。

### 5.2 TTY 验证（用户手动）

照下面顺序在 TTY 里跑：

```bash
cd /Users/allen/Documents/rustmossen
cargo build --workspace --release
MOSSEN_CODE_USE_CUSTOM_BACKEND=1 \
MOSSEN_CODE_CUSTOM_BASE_URL=http://localhost:8000 \
MOSSEN_CODE_CUSTOM_MODEL=deepseek-v4-flash \
./target/release/mossen
```

- 输 `/` → 出列表
- 输 `/he` → 浮层缩到 `/help`
- Tab 补完 → prompt 变 `/help`
- Enter → 弹 `/help` 详情
- `/clear` → 二次确认
- `/compact` → 看到压缩进度（需要会话有内容才能 compact）
- `/status` → 状态面板
- `/resume` → session 列表（有历史 session 时）
- `/tasks` → 任务面板
- 触发某个 user-invocable skill → transcript 有 skill 块
- 配了 MCP 时：状态栏有指示器、MCP 工具调用带 `[server]` 标签
- 项目级 MCP 首次连接 → 弹审批

每项判定：
- ✅ 看到期望效果
- ❌ 仍然没有
- 🟡 部分

把结果记到 `/tmp/mossen_experience_audit_round3.md`。

### 5.3 完成判定

- 测试 0 failed
- P0（3d-1 / 3d-2）8 项验收 ≥ 6/8 ✅
- 如果配了 MCP：P2 任务也基本能看到效果

### 5.4 报告

向用户报告：

> Phase 3.5 Round 3 完成。
> - 做了 X 个 task：3d-1, 3d-2, ...
> - TTY 验收清单：（贴 audit_round3.md 摘要）
> - 已知未做：（列）
>
> 可以进 Phase 4 全量验收吗？

---

## 附录

### A. 失败回滚

```bash
cd /Users/allen/Documents/rustmossen
git status
git diff <文件>
git checkout -- <文件>
```

### B. 常用命令

| 任务 | 命令 |
|------|------|
| 编译 | `cargo build --workspace` |
| Release（TTY 用） | `cargo build --workspace --release` |
| 测试 | `cargo test --workspace --no-fail-fast` |

### C. 本阶段关键文件 quick ref

| 改/读什么 | 去哪 |
|------|------|
| TUI 主循环 | `crates/mossen-tui/src/app.rs` |
| TUI 状态 | `crates/mossen-tui/src/state.rs` |
| Prompt input（3d-1） | `crates/mossen-tui/src/widgets/prompt_input.rs` |
| Typeahead / suggestion hooks | `crates/mossen-tui/src/hooks/typeahead.rs`、`unified_suggestions.rs` |
| 消息渲染（3d-4 / 3d-7） | `crates/mossen-tui/src/widgets/message.rs` |
| 状态栏（3d-6） | `crates/mossen-tui/src/widgets/status_bar.rs` 或 `components/misc.rs` |
| 命令注册 | `crates/mossen-commands/src/` |
| 命令分发入口 | `crates/mossen-cli/src/handlers/`、`mossen-cli/src/repl.rs` |
| Skill 系统 | `crates/mossen-skills/src/` |
| Skill 发现审计 | `crates/mossen-tools/src/agent_tool/SKILL_DISCOVERY_AUDIT.md`、`crates/mossen-skills/CONDITIONAL_SKILL_AUDIT.md` |
| MCP 协议 | `crates/mossen-mcp/src/` |
| Plugin reload 审计 | `crates/mossen-cli/PLUGIN_RELOAD_AUDIT.md` |

### D. 心法（同前几轮）

ratatui immediate-mode：所有动态状态集中在 `AppState`，render 是纯函数。本轮加的 `all_slash_commands` / `slash_suggestions` / `compact_progress` / `mcp_servers` 全在 AppState，不要在 widget 里搞"组件 state"。

### E. 与 Round 1 / Round 2 的关系

- Round 1（03b）：spinner / permission 数据层 / TaskListV2 接通
- Round 2（03c）：tool result 渲染 / diff / permission UI / TodoWrite 卡死 / sub-agent 嵌套 / Ctrl+C
- **Round 3（本文件）：Commands / Skill / MCP 三大子系统的 UI**

三轮合起来覆盖了从"看得见"（Round 1） → "看得清楚"（Round 2） → "完整的子系统交互"（Round 3）。

---

文档版本：v1.0（Round 3，2026-05-20）
**完整 5 阶段计划见 `PLAN_PRODUCTION.md`**
**Round 1 文件**：`phases/03b-experience.md`
**Round 2 文件**：`phases/03c-experience-round2.md`
**Audit 原始记录**：`/tmp/mossen_experience_audit.md`
