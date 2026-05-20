# Phase 1：Harness 融合

> **目标读者**：本地 DeepSeek V4 Flash（通过 ds4-server 运行 Mossen 自身或其他 agent 来执行本文档）。
> **完整 5 阶段索引**：`/Users/allen/Documents/rustmossen/PLAN_PRODUCTION.md`
> **本阶段位置**：5 阶段中的第 2 阶段（Phase 1）
> **前置**：Phase 0 已完成（workspace 编译过、mossen-cli 测试过）。如未完成请先做 `phases/00-correctness.md`。

---

## 1. 项目背景（必须读完再动手）

### 1.1 Mossen 是什么

**Mossen** 是 Rust 写的 coding agent CLI，对标 Claude Code。跑在本机，通过 `ds4-server`（localhost:8000）调用本地 DeepSeek V4 Flash 模型。代码在 `/Users/allen/Documents/rustmossen/`，按 Cargo workspace 组织成 10 个 crate。

### 1.2 什么是 "harness"

在 coding agent 语境里，**harness = agent 主运行循环 + 所有支撑机制**。一轮用户对话的完整流程：

```
用户输入
  ↓
处理用户输入（slash 命令 / 附件 / 图像）
  ↓
组装 system prompt（identity / env / tools / memory / 用户偏好）
  ↓
调用 LLM API（SSE 流式接收）
  ↓
解析响应（thinking / text / tool_use 三流分开）
  ↓
执行工具（Bash / Read / Edit / Write 等，并发或串行）
  ↓
工具结果回灌为下一轮 user message
  ↓
判断终止条件（no tool_use → 退出，max_tokens → 退出，等）
  ↓
[循环]
```

这整个循环 + 上下文管理 + 子 agent 编排 + 钩子调度 = harness。它是整个 agent 的"心脏"。

### 1.3 Mossen 当前 harness 状态（重要 —— 决定本阶段工作性质）

**已实现且在用的零件**：

| 零件 | 位置 | 状态 |
|------|------|------|
| 主 turn 循环（6 段流程） | `crates/mossen-agent/src/dialogue.rs::execute_turn_cycle` | ✅ 跑通 |
| SSE 流式接收 + 解析 | `crates/mossen-agent/src/api_client.rs` + `streaming.rs` | ✅ 跑通 |
| tool 调度 | `crates/mossen-agent/src/services/tools/` | ✅ 跑通 |
| Stop hook 调用 | `dialogue.rs:444-466` | ✅ 已 wire |
| Hook 模块 | `crates/mossen-agent/src/hooks/` （post_sampling / exec_command / exec_agent / mod / executor） | 🟡 模块在，**未被 dialogue 调用** |
| Context 压缩 | `crates/mossen-agent/src/services/compact/` （auto_compact / compact / session_memory_compact） | 🟡 模块在，**未在 compact 流程里调度 hook** |
| Sub-agent 派生（Task tool） | `crates/mossen-tools/src/agent_tool/` （run_agent / fork_subagent / 6 个 built_in agent） | ✅ 派生本身能跑 |
| Sub-agent transcript 隔离 | 同上 | ❓ **未验证** 3 个不变量 |

**问题**：零件齐全，但**没串起来**。除了 stop_hook 之外，其他 4 类 hook（post_sampling / pre_compact / post_compact / session_start）**根本没人调用**，用户在 settings.json 里配的 hook 哑火。这是本阶段要解决的事。

### 1.4 本阶段（Phase 1）要解决什么

**Phase 1 = Harness 融合**。把已实现的 hook 模块、compact 流程、sub-agent 系统**接到主循环上**。5 个 task：

| Task | 一句话目标 |
|------|-----------|
| 1-1 | **审计**：列出现有 hook 模块清单 + dialogue.rs 实际调用覆盖 → 产出 HOOK_AUDIT.md，作为 1-2/1-3/1-4 改动的依据 |
| 1-2 | Wire `PostSamplingHook` 到 dialogue.rs（API 采样完成后、消息回上层前调用） |
| 1-3 | Wire `Pre/PostCompactHook` 到 services/compact/（压缩前后调用） |
| 1-4 | Wire `SessionStartHook` 到 cli main / repl 启动路径 |
| 1-5 | **审计**：sub-agent transcript 隔离 3 个不变量 → 产出 ISOLATION_AUDIT.md，决定是否需要后续 fix |

注意：1-1 和 1-5 都是**审计任务，不写代码**，只产 markdown 报告。1-2/1-3/1-4 的具体改动点要**以 1-1 的审计结论为准**。

### 1.5 本阶段完成判定

5 个 task 都做完后：

- workspace 编译 0 error
- mossen-agent 测试全过
- 跑 `RUST_LOG=mossen_agent=info ./target/debug/mossen --oneshot "..."` 至少看到 1 条 post_sampling 或 session_start hook 调用日志
- 2 份审计文件存在：`HOOK_AUDIT.md` + `ISOLATION_AUDIT.md`

### 1.6 本阶段不要做的事

- **不要**在没有 1-1 审计结论时盲改 hook 调用点（每类 hook 的 ctx 字段、返回值约定都不同）
- **不要**碰 sub-agent transcript 隔离的实际 fix（1-5 只做审计；真要 fix 需要用户先看审计报告决定）
- **不要**改 enum 变体（如 `TerminalReason`、`HookResult`），涉及面太大
- **不要**修测试失败（那是 Phase 4）

---

## 2. 阅读约定（每个 task 都按这套节奏来）

### 2.1 角色与权限

你是 Rust 工程师。**可以**：
- 用 `Read` 读 `/Users/allen/Documents/rustmossen/**` 下任何文件
- 用 `Edit` 修改 Rust 源码、Cargo.toml、Markdown
- 用 `Write` 创建新文件
- 用 `Bash` 跑 `cargo`、`grep`、`ls`、`git diff`

**绝对不能**：
- 运行 `git push`、`git reset --hard`、`rm -rf`
- 删除任何未在本文档明示要删的文件
- 修改 `/Users/allen/Documents/ds4/` 任何东西（模型服务器，独立项目）

### 2.2 执行节奏

**一次只做一个 task**。每个 task 5 段固定结构：背景 / 位置 / 改动 / 验证 / 完成判定 / 回滚。

做完一个 task 必须跑「验证」，输出符合「完成判定」才能继续。**不要批量、不要并行、不要跳验证**。

### 2.3 卡住时

如果位置/改动对不上实际源码（行号漂移、代码已改过、找不到符号），**立即停止本 task**，输出查找过程 + 实际看到的代码 + 推测原因，等用户确认。**不要猜怎么改**。

### 2.4 命令前缀

所有 `Bash` 默认在 `/Users/allen/Documents/rustmossen` 下执行。

### 2.5 开工前基线确认

```bash
cd /Users/allen/Documents/rustmossen
cargo check --workspace 2>&1 | tail -3
```

期望 `Finished`，无 `error[`。

### 2.6 与用户的沟通规则

每个 task 前后**简短**（≤ 3 行）：
- **开始**：`正在做 1-X：<标题>`
- **完成**：`1-X 完成。验证通过：<关键输出>`
- **失败 / 卡住**：`1-X 失败 / 卡住。详情：<错误片段>。已停下，等待指示。`

---

## 3. 本阶段任务清单

| Task | 标题 | 产出 |
|------|------|------|
| 1-1 | 审计：现有 hook 模块 + dialogue.rs 实际调用覆盖 | `crates/mossen-agent/HOOK_AUDIT.md` |
| 1-2 | Wire PostSamplingHook 到 dialogue.rs | 代码改动 |
| 1-3 | Wire Pre/PostCompactHook 到 services/compact/ | 代码改动 |
| 1-4 | Wire SessionStartHook 到 cli / repl 启动路径 | 代码改动 |
| 1-5 | 审计：sub-agent transcript 隔离 3 个不变量 | `crates/mossen-tools/src/agent_tool/ISOLATION_AUDIT.md` |

**1-1 必须最先做**（后面 3 个 wire task 都依赖它的结论）。
**1-5 可以最后做**，但中间任何时刻都可以做。

---

## 4. Task 详情

### 1-1 审计：现有 hook 模块 + dialogue.rs 实际调用覆盖

#### 背景

Rust 端有完整的 hook 模块（`agent/hooks/`、`agent/stop_hooks.rs`、`utils/hooks.rs` 等），但 dialogue.rs 目前**只 wire 了 stop_hook**。其他 5 类 hook（pre/post-compact、post-sampling、session-start、task-completed）的调用点未知 —— 是没接、还是接在别处。

这个 task **不写代码，只产出一份审计报告**。这份报告会成为 1-2/1-3/1-4 wire 改动的精确依据。

#### 位置

需读的文件（不修改）：
- `crates/mossen-agent/src/dialogue.rs` 全文
- `crates/mossen-agent/src/hooks/mod.rs` + 该目录所有 .rs
- `crates/mossen-agent/src/stop_hooks.rs`
- `crates/mossen-agent/src/query/stop_hooks.rs`
- `crates/mossen-utils/src/hooks.rs` + `hooks_utils.rs`
- `crates/mossen-types/src/hooks.rs`
- `crates/mossen-agent/src/services/compact/` 全部
- `crates/mossen-cli/src/handlers/` 全部

可用的 grep 起点：

```bash
grep -rln "Hook\|HookManager\|HookContext\|execute_" crates/mossen-agent/src/ crates/mossen-utils/src/ crates/mossen-types/src/ | head -20
```

#### 改动

产出 `crates/mossen-agent/HOOK_AUDIT.md`，按下面结构填写。**所有「...」处都要填具体内容**，不要留空：

```markdown
# Hook 系统审计 / Phase 1-1

## 已定义的 hook 类型清单

| Hook 类型 | 类型定义位置 file:line | Manager / Executor 位置 file:line | Context struct 字段（核心） |
|---|---|---|---|
| StopHook | ... | ... | ... |
| PostSamplingHook | ... | ... | ... |
| PreCompactHook | ... | ... | ... |
| PostCompactHook | ... | ... | ... |
| SessionStartHook | ... | ... | ... |
| TaskCompletedHook | ... | ... | ... |
| ExecCommandHook | ... | ... | ... |
| ExecAgentHook | ... | ... | ... |

## 在 dialogue.rs / compact / cli main 中的实际调用

| 调用点 file:line | 哪类 hook | 调用前上下文（在 turn 哪个阶段） | 调用后行为（结果如何影响主路径） |
|---|---|---|---|
| dialogue.rs:451 | StopHook | assistant message 拼完、决定是否进 tool_use 阶段之前 | allow → 继续；block → continue turn；prevent → terminate |
| ... | ... | ... | ... |

## 缺口（hook 已定义但未在主路径调度）

- [ ] PostSampling: 定义在 hooks/post_sampling.rs:_ 行，hooks/executor.rs:_ 行有 manager，但 dialogue.rs **无调用**
- [ ] PreCompact: ...
- [ ] PostCompact: ...
- [ ] SessionStart: ...
- [ ] TaskCompleted: ...

## Phase 1 后续 task 的精确 wire 目标

### 1-2 PostSamplingHook
- 期望调用点：dialogue.rs 第 _ 行（具体在 API 流式接收的哪个事件之后，stop_reason 拿到之前/之后）
- 应该传给 hook 的 ctx 字段（基于审计的 PostSamplingHookContext 定义）：...
- 应该如何处理 hook 返回值：...

### 1-3 Pre/PostCompactHook
- pre 期望调用点：services/compact/compact.rs::compact_conversation() 入口
- post 期望调用点：同函数返回前
- ctx 字段、返回值处理：...

### 1-4 SessionStartHook
- 期望调用点：crates/mossen-cli/src/main.rs async main，拿到 cwd / config 之后、构造 EngineConfig 之前
- ctx 字段、返回值处理：...
```

#### 验证

```bash
ls -la crates/mossen-agent/HOOK_AUDIT.md
wc -l crates/mossen-agent/HOOK_AUDIT.md
```

#### 完成判定

- 文件存在
- 行数 ≥ 60
- 没有「...」占位符（所有表格行都填了具体 file:line）

#### 回滚

```bash
rm crates/mossen-agent/HOOK_AUDIT.md
```

---

### 1-2 Wire PostSamplingHook 到 dialogue.rs

#### 背景

PostSampling hook 在 TS 端是 API 采样完成之后、消息回上层之前调用，用于注入 `system-reminder` 等。Rust 端 `crates/mossen-agent/src/hooks/post_sampling.rs` 已实现 manager，但 dialogue.rs 没调用它。

**强依赖 1-1 审计结论**。1-1 没做完不要做本 task。

#### 位置

以 1-1 审计的「1-2 PostSamplingHook 期望调用点」为准。一般来说会在 dialogue.rs 的 SSE 流式接收循环结束后、解析 stop_reason 之前/之后。

#### 改动

参照 stop_hook 的 wire 模式（dialogue.rs 第 444-466 行附近）。大致结构：

```rust
// 1. 在 execute_turn_cycle 函数签名（或它能拿到的 state）里携带 PostSamplingHookManager
//    Arc<PostSamplingHookManager>（无 Arc 的话用引用），从 generate_dialogue 传下来

// 2. 在 1-1 审计指定的位置调用：
let psh_ctx = PostSamplingHookContext {
    // 字段以 1-1 审计的 PostSamplingHookContext 定义为准
};
let psh_result = post_sampling_manager
    .run(&psh_ctx, &assistant_message, cancel)
    .await;
match psh_result {
    Ok(modified_msgs) => {
        // 如果 hook 接口允许修改 messages，按 1-1 审计的约定拼回
        // 如果是 observe-only，直接忽略 Ok 内容
    }
    Err(e) => {
        warn!(error = %e, "post-sampling hook failed; continuing");
        // 策略：hook 失败不能杀整个 turn，记日志后继续
    }
}
```

⚠️ **如果 1-1 审计发现 PostSamplingHook 接口形态与 StopHook 差异大（例如返回类型完全不同），停下报告**。

#### 验证

```bash
cd /Users/allen/Documents/rustmossen
cargo build --workspace 2>&1 | tail -5
cargo test -p mossen-agent dialogue 2>&1 | tail -10
```

如果 `crates/mossen-agent/tests/` 下有现成 hook 集成测试：

```bash
cargo test -p mossen-agent --test '*hook*' 2>&1 | tail -10
```

#### 完成判定

- workspace 编译 `Finished`，无 error
- dialogue 相关测试通过（或不**新增**失败）

#### 回滚

```bash
git checkout -- crates/mossen-agent/src/dialogue.rs
```

---

### 1-3 Wire Pre/PostCompactHook 到 services/compact/compact.rs

#### 背景

`services/compact/compact.rs` 实现了 traditional 压缩，`auto_compact.rs` 实现了断路器 + 阈值检测，`session_memory_compact.rs` 实现了第一层快速压缩。三个流程都是**用户可编程钩子点**（pre_compact 提示要压了、post_compact 拿压缩结果做后处理），但当前未调用任何 hook。

**强依赖 1-1 审计结论**。

#### 位置

以 1-1 审计的「1-3 Pre/PostCompactHook 期望调用点」为准。一般是：

- pre：`services/compact/compact.rs::compact_conversation()` 入口
- post：同函数返回 CompactionResult 之前
- 额外：`auto_compact.rs::should_auto_compact()` 之后、决定真要 compact 之前（也 pre）

#### 改动

参照 1-2 的 wire 模式，在审计指定的位置插入 hook 调用。注意失败策略：

- **pre-compact 失败 → skip compact**（保守，避免错误压缩）
- **post-compact 失败 → 仍返回 CompactionResult**（hook 失败不能让用户失了 boundary marker）

这两条策略**必须在 hook 调用代码的注释里写清楚**，免得未来读代码的人不知道选什么。

如果 hook 接口接受可变 messages 参数（hook 可以修改要压缩的消息），就把 `messages_to_compact` 当 in/out 传过去；如果只是观察型，传不可变引用即可。具体形态以 1-1 审计为准。

#### 验证

```bash
cd /Users/allen/Documents/rustmossen
cargo build --workspace 2>&1 | tail -5
cargo test -p mossen-agent compact 2>&1 | tail -10
```

#### 完成判定

- workspace 编译 `Finished`，无 error
- compact 测试通过（或不新增失败）

#### 回滚

```bash
git checkout -- crates/mossen-agent/src/services/compact/
```

---

### 1-4 Wire SessionStartHook 到 cli / repl 启动路径

#### 背景

SessionStart hook 用于 plugin 初始化、记忆系统预加载、startup banner 等。当前定义在 hooks/，但 main.rs / repl.rs 启动流程里没调用。

**强依赖 1-1 审计结论**。

#### 位置

- hook 定义位置（1-1 审计确认）
- 期望调用点：
  - `crates/mossen-cli/src/main.rs` 的 async main 入口
  - `crates/mossen-cli/src/repl.rs` interactive REPL 启动前

#### 改动

在 main.rs 拿到 cwd / config 之后、构造 EngineConfig 之前调用：

```rust
let session_start_ctx = SessionStartHookContext {
    cwd: cwd.clone(),
    is_interactive: matches!(mode, RunMode::Repl),
    // 其他字段以 1-1 审计为准
};
if let Err(e) = session_start_manager.run(&session_start_ctx, &cancel).await {
    warn!(error = %e, "session-start hook failed; continuing");
}
```

REPL 启动路径同理。

#### 验证

```bash
cd /Users/allen/Documents/rustmossen
cargo build --workspace 2>&1 | tail -5
RUST_LOG=mossen_agent::hooks=debug ./target/debug/mossen --oneshot "hi" 2>&1 | grep -i "session.start\|hook" | head -10
```

#### 完成判定

- build 过
- 日志里出现 session-start hook 相关行（至少 1 条 debug）

#### 回滚

```bash
git checkout -- crates/mossen-cli/src/main.rs crates/mossen-cli/src/repl.rs
```

---

### 1-5 子 agent transcript 隔离硬保证（审计 only）

#### 背景

`crates/mossen-tools/src/agent_tool/run_agent.rs` 等已实现 sub-agent，能从主 agent 派生子任务。但 sub-agent 必须满足 3 个不变量才安全：

1. **transcript 隔离**：子 agent 的对话历史不能进父 agent 的 transcript 文件（否则父 agent `/clear` 后子 agent 输出就丢，并且 transcript 大小爆炸）
2. **AgentContext 传播**：子 agent 内部 spawn 的工具调用必须拿到自己的 `agent_id`（不然权限上下文混乱、metrics 混乱）
3. **结果以 `<task-notification>` XML 块回灌**到父 transcript（标准化的子 agent 输出格式，父 agent 才知道怎么处理）

本 task **只审计，不修代码**。审计发现缺失了就停下报告，等用户决定 fix 时机。

#### 位置

- `crates/mossen-tools/src/agent_tool/run_agent.rs`
- `crates/mossen-tools/src/agent_tool/fork_subagent.rs`
- `crates/mossen-tools/src/shared/spawn_multi_agent.rs`
- 搜 `recordTranscript` / `record_transcript` 等相关写入位置

#### 改动

##### Step 1：审计 3 个不变量

跑 grep：

```bash
# 不变量 1：transcript 文件路径
grep -rn "transcript_path\|transcript_file" crates/mossen-tools/src/agent_tool/ crates/mossen-agent/src/

# 不变量 2：AgentContext 传播机制
grep -rn "task_local\|AgentContext\|agent_id" crates/mossen-tools/src/agent_tool/ crates/mossen-agent/src/

# 不变量 3：task-notification 回灌
grep -rn "task-notification\|task_notification\|TaskNotification" crates/
```

##### Step 2：写审计文件

产出 `crates/mossen-tools/src/agent_tool/ISOLATION_AUDIT.md`：

```markdown
# Sub-Agent Isolation Audit / Phase 1-5

## 不变量 1：transcript 隔离

状态：（满足 / 部分 / 缺失）
证据 file:line：
分析：（子 agent 用的 transcript 路径是否与父不同？怎么生成的？）
建议 fix：（如果缺失，列具体步骤）

## 不变量 2：AgentContext 传播

状态：（满足 / 部分 / 缺失）
证据 file:line：
分析：（子 agent 内部工具调用的 agent_id 怎么传？task_local? 显式参数?）
建议 fix：

## 不变量 3：task-notification 回灌

状态：（满足 / 部分 / 缺失）
证据 file:line：
分析：（子 agent 结果怎么进父 transcript？是否包成 task-notification？）
建议 fix：

## 整体结论

- 全满足 → 无需 fix，本 task 完成
- 有缺失 → **停下报告**，等用户决定后续 fix 时机
```

##### Step 3：根据审计结论决定下一步

- 全满足 → 完成本 task
- 有缺失 → **停下报告**，让用户决定要不要立刻补；如果用户同意补，再走具体 fix。**不要私自补 fix**，因为隔离逻辑改错会污染父 transcript（影响面大）

#### 验证

```bash
ls -la crates/mossen-tools/src/agent_tool/ISOLATION_AUDIT.md
```

#### 完成判定

- 文件存在
- 3 个不变量都有「满足 / 部分 / 缺失」明确判定
- 每个不变量都有 file:line 证据

#### 回滚

```bash
rm crates/mossen-tools/src/agent_tool/ISOLATION_AUDIT.md
```

---

## 5. Phase 1 阶段验收

5 个 task 都做完后：

```bash
cd /Users/allen/Documents/rustmossen
cargo build --workspace 2>&1 | tail -3
cargo test -p mossen-agent 2>&1 | tail -3

# 跑一次完整 oneshot 看 hook 日志
RUST_LOG=mossen_agent=info \
MOSSEN_CODE_USE_CUSTOM_BACKEND=1 \
MOSSEN_CODE_CUSTOM_BASE_URL=http://localhost:8000 \
MOSSEN_CODE_CUSTOM_MODEL=deepseek-v4-flash \
./target/debug/mossen --oneshot "Use the Bash tool to run: echo phase1-done" 2>&1 \
  | grep -iE "hook|compact|sub.agent" | head -20
```

**Phase 1 完成判定**：

- workspace 0 error
- mossen-agent 测试至少不**新增**失败
- 日志能看到至少 1 条 hook 调用（post_sampling 或 session_start）
- 0 条 hook panic
- 2 份审计文件存在：`HOOK_AUDIT.md` + `ISOLATION_AUDIT.md`

通过后向用户报告：

> Phase 1 完成。验证：
> - cargo build --workspace: Finished
> - cargo test -p mossen-agent: ok
> - hook 日志：看到 post_sampling / session_start
>
> 审计文件 HOOK_AUDIT.md / ISOLATION_AUDIT.md 已生成，可 review 后进 Phase 2（外围 5 系统接合）。

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
| 增量编译 | `cargo build -p mossen-agent` |
| 全 workspace 编译 | `cargo build --workspace` |
| 跑测试 | `cargo test -p mossen-agent` |
| 跑单个 hook 测试 | `cargo test -p mossen-agent --test '*hook*' -- --nocapture` |
| trace 日志跑 oneshot | `RUST_LOG=mossen_agent=trace ./target/debug/mossen --oneshot "..."` |

### C. 本阶段关键文件 quick ref

| 改/读什么 | 去哪 |
|------|------|
| agent 主循环 | `crates/mossen-agent/src/dialogue.rs` |
| Hook 模块 | `crates/mossen-agent/src/hooks/` + `stop_hooks.rs` |
| Hook utils | `crates/mossen-utils/src/hooks.rs` + `hooks_utils.rs` |
| Hook 类型 | `crates/mossen-types/src/hooks.rs` |
| Context compact | `crates/mossen-agent/src/services/compact/` |
| CLI 启动 | `crates/mossen-cli/src/main.rs` + `repl.rs` |
| Sub-agent | `crates/mossen-tools/src/agent_tool/` |

---

文档版本：v2.1（拆分版）
**完整 5 阶段计划见 `PLAN_PRODUCTION.md`**
