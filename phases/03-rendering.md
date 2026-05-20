# Phase 3：渲染层 gap 接合

> **目标读者**：本地 DeepSeek V4 Flash（通过 ds4-server 运行 Mossen 自身或其他 agent 来执行本文档）。
> **完整 5 阶段索引**：`/Users/allen/Documents/rustmossen/PLAN_PRODUCTION.md`
> **本阶段位置**：5 阶段中的第 4 阶段（Phase 3）
> **前置**：Phase 0、Phase 1、Phase 2 已完成。

---

## 1. 项目背景（必须读完再动手）

### 1.1 Mossen 是什么

**Mossen** 是 Rust 写的 coding agent CLI，对标 Claude Code。跑在本机，通过 `ds4-server`（localhost:8000）调用本地 DeepSeek V4 Flash 模型。代码在 `/Users/allen/Documents/rustmossen/`。

### 1.2 什么是 "渲染层"

渲染层 = 用户在终端里**实际看到的**那一面。包含：

- 主屏（消息流、流式 markdown）
- 输入框（多行编辑、suggestion）
- Spinner / loading 动画
- Modal / 弹窗（Permission 确认、Resume 选择、Model 切换）
- 状态栏
- 子 agent / 后台任务的占位渲染
- TodoWrite 任务列表实时显示

代码在 `crates/mossen-tui/src/`。

### 1.3 TS 原版 vs Rust 端的渲染范式差异（重要）

| 范式 | TS / Ink（React for terminal） | Rust / ratatui |
|------|-------------------------------|---------------|
| 渲染模式 | reactive（state 变 → React 局部重渲） | immediate-mode（每帧整屏重画） |
| 状态形态 | 9 层 Context + zustand store | 单一可变 `App` 结构体 |
| 流式增量 | token 进 React state → Ink diff → ANSI 微更新 | 整 buffer 累积 → 整帧重画 markdown |
| Modal 层叠 | 3 层 Context 可叠 | `ActiveModal` 单态枚举（**这是有意设计**，与 TS REPL "exactly one modal interactive" 语义一致） |

注意：Rust 端的 ActiveModal 单态、InputJsonDelta `=> {}` 丢弃**都是有意设计**，不是 bug。审计代码注释里有说明，不要"修"它们。

### 1.4 Mossen 当前渲染层状态

| 区域 | 状态 | 说明 |
|------|------|------|
| 主 event loop (`app.rs::run`) | ✅ | tokio::select! input + engine_rx |
| 流式 markdown (`widgets/markdown.rs`) | ✅ | 但每 token delta 重解析整个 buffer，长答复 perf 待测 |
| MessageRow 分派 | ✅ | 按 MessageType enum |
| Thinking block | ✅ | 灰字 shimmer + 30s fade + Ctrl+E pin |
| ToolUse / ToolResult | ✅ | Bash/Edit/Write 有特化，其他工具走通用 |
| Permission 弹窗 | ✅ | `components::permissions::PermissionRequest` |
| Picker（Theme / OutputStyle） | ✅ | `components::root_large::Picker` |
| Spinner | ✅ | 单状态 |
| **TaskListV2 widget**（components/tasks.rs） | 🟡 | **数据壳已写、app.rs 没驱动** |
| **SubAgent / Teammate spinner**（spinner_anim.rs） | 🟡 | **代码已写、engine 没 emit** |
| `foreground_task_id` 状态字段 | 🟡 | **存在但没人切换** |
| Ctrl+B 后台任务详情 | 🟡 | **键路由未接** |
| Ctrl+C 中断协议（TurnState） | 🟡 | **半截渲染可能撕裂** |

### 1.5 本阶段（Phase 3）要解决什么

把 TUI 端**已写好但没被驱动**的渲染零件接上。5 个 task：

| Task | 一句话目标 |
|------|-----------|
| 3-1 | TodoWrite 工具改动 → TaskListV2 widget 实时渲染 |
| 3-2 | Sub-Agent (Task tool) 派生时的输出 → teammate widget 接通（含 SdkMessage 加 task_id） |
| 3-3 | `foreground_task_id` 切换 + Ctrl+B 后台任务详情 |
| 3-4 | 流式 markdown 重解析性能基线测量（benchmark） |
| 3-5 | Ctrl+C 中断协议（TurnState 状态机），防止半截渲染撕裂 |

### 1.6 本阶段完成判定

- workspace 编译 0 error
- mossen-tui 测试至少不**新增**失败
- benchmark 文件 `benches/markdown_streaming.rs` 存在能跑
- `MOSSEN_CODE_USE_CUSTOM_BACKEND=1 ... ./target/debug/mossen --oneshot "用 TodoWrite 添加 3 个任务"` 不 panic

完整 TUI 渲染验证留到 Phase 4 真机烤机。

### 1.7 本阶段不要做的事

⚠️ **不要删 `components/` 和 `ink/`**：
- `components/` 大量被 app.rs 引用（dialogs / permissions / misc / root_large / root_medium 等），删了会导致编译错
- `ink/` 是平移港没碍事，留着以后可能复用

⚠️ **不要"修" `ActiveModal` 单态、`InputJsonDelta => {}` 丢弃**：这两处是有意设计，匹配 TS 端语义。代码注释里说清楚了。

⚠️ **不要重写 markdown widget**：Phase 3-4 只是测基线，超阈值才报告，不在本阶段实施优化。

⚠️ **不要碰子 agent transcript 隔离的实际 fix**（那是 Phase 1-5 审计 + 后续 sprint 的事）。

---

## 2. 阅读约定

### 2.1 角色与权限

你是 Rust 工程师。**可以**：用 `Read` / `Edit` / `Write` / `Bash`。
**绝对不能**：`git push` / `git reset --hard` / `rm -rf` / 修改 `/Users/allen/Documents/ds4/`。

### 2.2 执行节奏

一次一个 task，每个 5 段结构：背景 / 位置 / 改动 / 验证 / 完成判定 / 回滚。验证通过才能下一个。

### 2.3 卡住时

立即停下报告，不要猜。

### 2.4 命令前缀

默认 `cd /Users/allen/Documents/rustmossen`。

### 2.5 基线确认

```bash
cargo check --workspace 2>&1 | tail -3
```

### 2.6 沟通规则

- 开始：`正在做 3-X：<标题>`
- 完成：`3-X 完成。验证通过：<关键输出>`
- 失败：`3-X 失败 / 卡住。详情：<错误>。已停下。`

---

## 3. 本阶段任务清单

| Task | 标题 | 改动复杂度 |
|------|------|----------|
| 3-1 | TodoWrite 工具事件 → TaskListV2 widget 渲染流水 | 中（涉及 state + handler + render） |
| 3-2 | Sub-Agent (Task tool) 输出 → teammate widget 接通 | 高（要改 SdkMessage 加 task_id + 双向通道） |
| 3-3 | foreground_task_id 切换 + Ctrl+B 后台任务详情 | 中 |
| 3-4 | 流式 markdown 重解析性能基线测量 | 低（写 benchmark） |
| 3-5 | Ctrl+C 中断协议：TurnState 状态机 | 中（涉及 enum + 多处状态切换 + 渲染） |

3-4 可以最先做（独立 benchmark，不依赖其他）。其他按顺序。

---

## 4. Task 详情

### 3-1 TodoWrite 工具事件 → TaskListV2 widget 渲染流水

#### 背景

`crates/mossen-tools/src/todo_write_tool/` 实现了 TodoWrite 工具（模型用它管理任务列表），`crates/mossen-tui/src/components/tasks.rs:376` 等定义了 `TaskListV2 / TaskAssignmentMsg / SubAgentProvider` 数据结构和 widget。

**但 app.rs 当前没消费这些** —— 整个 TodoWrite 在 TUI 里走通用的 ToolResult 路径，渲染成一段普通文本，没有独立的"任务清单"视图。

期望效果：模型用 TodoWrite 加任务时，主屏底部（或侧边栏 / sticky header）出现一个**实时更新**的 TaskList 卡片，显示每个任务的状态。

#### 位置

- 工具实现：`crates/mossen-tools/src/todo_write_tool/`
- 渲染壳（数据结构）：`crates/mossen-tui/src/components/tasks.rs:376`（`TaskListV2`）
- TUI 主 loop：`crates/mossen-tui/src/app.rs::handle_engine_message`
- TUI 状态：`crates/mossen-tui/src/state.rs`

#### 改动

##### Step 1：审计 + 设计接合点

```bash
grep -n "TodoWrite\|todo_write\|TaskListV2" crates/mossen-tui/src/app.rs
grep -n "TodoWrite\|todo_write" crates/mossen-tui/src/state.rs
```

预期都空命中 —— 这就是要补的 gap。

##### Step 2：在 state.rs 加 TaskListState

在 `crates/mossen-tui/src/state.rs` 加：

```rust
#[derive(Debug, Clone, Default)]
pub struct TaskListState {
    pub tasks: Vec<TodoItem>,         // 复用 mossen-tools::todo_write_tool 的 TodoItem
    pub last_update: Option<std::time::Instant>,
}
```

然后在 `AppState` 结构体里加字段：

```rust
pub task_list: TaskListState,
```

`TodoItem` 类型从 `mossen_tools::todo_write_tool` 导入。如果它没 `pub`，**停下报告**，不要私自改可见性。

##### Step 3：在 handle_engine_message 里监听 TodoWrite tool_result

`app.rs::handle_engine_message`，当 `MessageType::ToolResult` 且 `tool_name == "TodoWrite"` 时：

```rust
if tool_name == "TodoWrite" {
    // 从 tool input（不是 result！）解析 todos 数组
    // tool_input 应该在 ToolUse 消息里，需要找到对应的 ToolUse 并拿 input
    if let Ok(parsed) = serde_json::from_value::<TodoWriteArgs>(tool_input.clone()) {
        self.app_state.task_list.tasks = parsed.todos;
        self.app_state.task_list.last_update = Some(std::time::Instant::now());
    }
}
```

具体怎么从 tool_use 拿 input，看现有 ToolResult 处理逻辑（grep `handle_tool_result` / `tool_input`）。

##### Step 4：在主屏渲染 TaskListV2

`app.rs::render` 函数里，如果 `app_state.task_list.tasks.is_empty() == false`，在消息列表底部 / 侧边栏 / sticky header 渲染：

```rust
if !self.app_state.task_list.tasks.is_empty() {
    let widget = crate::components::tasks::TaskListV2Widget::new(&self.app_state.task_list.tasks);
    f.render_widget(widget, area);
}
```

如果 `components::tasks::TaskListV2Widget` 不存在（只有 `TaskListV2` 数据结构）：
- 在 `components/tasks.rs` 加一个 `impl Widget for TaskListV2`，或者
- 新建一个 `TaskListV2Widget<'a>` 持有 `&'a [TodoItem]`

具体形态以现有 components/tasks.rs 风格为准（看周边 widget 怎么写的）。

#### 验证

```bash
cd /Users/allen/Documents/rustmossen
cargo build --workspace 2>&1 | tail -5
```

oneshot 不 panic 验证：

```bash
MOSSEN_CODE_USE_CUSTOM_BACKEND=1 \
MOSSEN_CODE_CUSTOM_BASE_URL=http://localhost:8000 \
MOSSEN_CODE_CUSTOM_MODEL=deepseek-v4-flash \
./target/debug/mossen --oneshot "用 TodoWrite 添加三个任务：吃饭、睡觉、打豆豆" 2>&1 | tail -10
```

#### 完成判定

- build 过
- oneshot 不 panic
- 完整渲染验证留到 Phase 4 真 TTY

#### 回滚

```bash
git checkout -- crates/mossen-tui/src/app.rs \
                crates/mossen-tui/src/state.rs \
                crates/mossen-tui/src/components/tasks.rs
```

---

### 3-2 Sub-Agent (Task tool) 输出 → teammate widget 接通

#### 背景

`crates/mossen-tools/src/agent_tool/` 实现了完整的 sub-agent 派生（含 6 个内置 agent：general/plan/explore/verification/statusline_setup/mossen_code_guide），用户用 Task tool 可以让主 agent 派生一个子任务。

`crates/mossen-tui/src/components/tasks.rs:381` 和 `components/spinner_anim.rs` 已实现 `TeammateAssignment / SubAgentProvider / describe_teammate_activity` 等 widget 和 helper。

**但当 Task tool 触发子 agent 跑起来时，TUI 完全感受不到** —— 不显示 spinner 树、不显示子 agent 当前在调啥工具、不显示子 agent 输出。子 agent 完成才能看到一行总结。

#### 位置

- agent_tool 入口：`crates/mossen-tools/src/agent_tool/run_agent.rs`
- engine → TUI 通道：搜 `engine_tx` / `engine_rx` / `SdkMessage`
- TUI teammate widget：`crates/mossen-tui/src/components/spinner_anim.rs:740+`
- 数据壳：`crates/mossen-tui/src/state.rs::foreground_task_id`

#### 改动

##### Step 1：审计 engine ↔ TUI 通道

```bash
grep -rn "engine_tx\|engine_rx\|SdkMessage" crates/mossen-tui/src/app.rs crates/mossen-tools/src/agent_tool 2>/dev/null | head -15
```

理解 SdkMessage 怎么从子 agent 冒泡到主 TUI 的，或者**它根本没冒泡**（这是最常见的情况）。

##### Step 2：给 SdkMessage 加 task_id

如果 SdkMessage 没有 task_id 字段，加上：

```rust
pub struct SdkMessage {
    pub task_id: Option<String>,  // None = 主 agent, Some(_) = sub agent
    // ... 其他原字段保留
}
```

⚠️ 改 SdkMessage 影响面大，所有发送 / 接收 SdkMessage 的地方都要兼容。先 grep 看影响范围：

```bash
grep -rn "SdkMessage" crates/ | wc -l
```

如果命中 ≥ 30 个文件，**停下报告**，让用户决定要不要本 sprint 改这个。

子 agent 派生时填上 task_id；主 agent 不填（None）。

##### Step 3：在 app.rs::handle_engine_message 按 task_id 路由

```rust
match msg.task_id.as_deref() {
    None => {
        // 主 agent 消息，按原逻辑入主消息流
    }
    Some(tid) => {
        // 子 agent 消息：进 state.teammate_messages.entry(tid).or_default().push(...)
        // 同时更新 state.teammate_states.entry(tid).or_insert(TeammateState::Running)
    }
}
```

state.rs 加配套字段：

```rust
pub teammate_messages: HashMap<String, Vec<MessageData>>,
pub teammate_states: HashMap<String, TeammateState>,
```

`TeammateState` 定义如果还没有，在 state.rs 加：

```rust
#[derive(Debug, Clone)]
pub enum TeammateState {
    Running,
    Completed(String),  // 完成时的 summary
    Failed(String),
}
```

##### Step 4：渲染 teammate spinner 树

在 render 路径里，如果 `app_state.teammate_states.len() > 0`，画 `components::spinner_anim::TeammateSpinnerTree` widget（应该已存在）。

#### 验证

```bash
cd /Users/allen/Documents/rustmossen
cargo build --workspace 2>&1 | tail -5
```

#### 完成判定

- build 过，sub-agent oneshot 测试不 panic
- 完整渲染验证留到 Phase 4

#### 回滚

```bash
git checkout -- crates/mossen-tui/src/ crates/mossen-tools/src/agent_tool/
```

---

### 3-3 foreground_task_id 切换 + Ctrl+B 后台任务详情

#### 背景

`state.rs:81 foreground_task_id` 字段已存在但**没人切换它**。期望行为：用户按 Ctrl+B 时进入"后台任务列表"modal，选一个 task 后切换 foreground，主屏专门显示这个 task 的消息流。

#### 位置

- `crates/mossen-tui/src/state.rs::AppState::foreground_task_id`
- `crates/mossen-tui/src/event.rs` 或 `app.rs::handle_key`：Ctrl+B 路由
- 后台任务列表 widget：搜 `BackgroundTask\|background_task`

#### 改动

##### Step 1：审计

```bash
grep -rn "foreground_task_id\|background_task\|Ctrl.B\|KeyCode.Char.'b'" crates/mossen-tui/src 2>/dev/null | head -10
```

##### Step 2：实现 Ctrl+B 切换 modal

如果当前 `app.rs::handle_key` 没处理 Ctrl+B：

```rust
KeyCode::Char('b') if modifiers.contains(KeyModifiers::CONTROL) => {
    if matches!(self.active_modal, ActiveModal::None) {
        let tasks = self.collect_background_tasks();
        if !tasks.is_empty() {
            self.active_modal = ActiveModal::Picker {
                kind: PickerKind::BackgroundTasks,
                title: "Background Tasks".into(),
                items: tasks.iter().map(|t| t.label.clone()).collect(),
                selected: 0,
            };
        }
    }
}
```

如果 `PickerKind::BackgroundTasks` 没有，加这个变体（参考 `Theme / OutputStyle` 的写法）。

`collect_background_tasks` 是新加的 helper，从 `app_state.teammate_states`（3-2 加的）里收集。

##### Step 3：选择后切换 foreground_task_id

picker 选定后（在 modal 关闭处理路径里）：

```rust
self.app_state.foreground_task_id = Some(selected_task_id);
// 触发 redraw
```

##### Step 4：主屏按 foreground_task_id 过滤显示

`render` 里，如果 `foreground_task_id` 是 `Some(tid)`，只显示 `teammate_messages[tid]` 而不是主消息流。再加一个返回主屏的快捷键（如再按 Ctrl+B 或 Esc）。

#### 验证

```bash
cd /Users/allen/Documents/rustmossen
cargo build --workspace 2>&1 | tail -5
```

#### 完成判定

- build 过
- 键路由验证留到 Phase 4 真 TTY

#### 回滚

```bash
git checkout -- crates/mossen-tui/src/
```

---

### 3-4 流式 markdown 重解析性能基线测量 + 决策

#### 背景

`crates/mossen-tui/src/widgets/markdown.rs` 用 pulldown-cmark + syntect 解析 markdown。Rust ratatui 是 immediate-mode，每个 token delta 到达后**重解析整个累积 buffer**。

在长答复（10k+ token）+ 高 chunk 频率（30 t/s）下，可能成为 CPU 热点（O(n²) 趋势）。但**实际是不是问题，必须测**。

#### 位置

- markdown widget：`crates/mossen-tui/src/widgets/markdown.rs`
- 调用方：`crates/mossen-tui/src/widgets/message.rs::render_streaming` 之类（grep `parse_to_lines`）

#### 改动

##### Step 1：写 benchmark

新建 `crates/mossen-tui/benches/markdown_streaming.rs`（如果还没有 benches 目录，cargo 会自动创建）：

```rust
use criterion::{criterion_group, criterion_main, Criterion, black_box};
use mossen_tui::widgets::markdown::MarkdownWidget;

fn bench_streaming_reparse(c: &mut Criterion) {
    // 模拟 200 个 token delta，每个 ~50 字节，最终 10 KB
    let chunks: Vec<String> = (0..200).map(|i| format!(" word{} more", i)).collect();

    c.bench_function("markdown_reparse_per_chunk_10kb_final", |b| {
        b.iter(|| {
            let mut acc = String::new();
            for chunk in &chunks {
                acc.push_str(chunk);
                let _lines = MarkdownWidget::parse_to_lines(black_box(&acc));
            }
        });
    });
}

criterion_group!(benches, bench_streaming_reparse);
criterion_main!(benches);
```

在 `crates/mossen-tui/Cargo.toml` 加：

```toml
[dev-dependencies]
criterion = "0.5"

[[bench]]
name = "markdown_streaming"
harness = false
```

如果 `MarkdownWidget::parse_to_lines` 不是 pub 函数，**停下报告**（要么开 visibility，要么改 bench 调用方式，需用户决定）。

##### Step 2：跑基线

```bash
cd /Users/allen/Documents/rustmossen
cargo bench -p mossen-tui --bench markdown_streaming 2>&1 | tail -20
```

记录单次完整流式过程的耗时。

##### Step 3：决策

- 耗时 < 500 ms（200 chunks 累积 10 KB）→ 不需要优化，本 task 结束
- 耗时 > 1 s → **真有 perf 问题，停下报告**，让用户决定优化策略（incremental parser / 末行 only reparse / line cache）

#### 验证

```bash
ls -la crates/mossen-tui/benches/markdown_streaming.rs
cargo bench -p mossen-tui --bench markdown_streaming 2>&1 | tail -5
```

#### 完成判定

- bench 文件存在
- bench 跑出数据（不一定要快，知道数据就行）

#### 回滚

```bash
rm crates/mossen-tui/benches/markdown_streaming.rs
git checkout -- crates/mossen-tui/Cargo.toml
```

---

### 3-5 Ctrl+C 中断协议：TurnState 状态机

#### 背景

当前 `event::InputAction::Interrupt` 和 `engine_rx` 之间没有原子终止协议。如果在 `assistant_buf` 半截 + ToolUse 已 push 但 ToolResult 未到时按 Ctrl+C，下一个 `terminal.draw` 会渲染半截 markdown / 空 ToolResult 占位 —— **撕裂**（用户看到半句话 + 空白）。

引入 TurnState 状态机：每个 turn 有 Idle/Streaming/Cancelling/Cancelled 4 个状态，渲染层按状态画终止符（如 "↯ 已取消"）。

#### 位置

- `crates/mossen-tui/src/app.rs::handle_engine_message`（`assistant_buf` 累积位置）
- `crates/mossen-tui/src/event.rs::InputAction::Interrupt`
- `crates/mossen-tui/src/state.rs::AppState`

#### 改动

##### Step 1：加 TurnState 枚举到 state.rs

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnState {
    Idle,        // 没在 turn 里，可以接受新 prompt
    Streaming,   // assistant 流式中
    Cancelling,  // 收到 Ctrl+C 但还没清干净
    Cancelled,   // 已 finalized，进入下个 prompt
}

// AppState 加字段：
pub turn_state: TurnState,
```

`Default` 值是 `Idle`。

##### Step 2：在关键路径切换状态

- 收到第一个 StreamEvent → `turn_state = Streaming`
- 流式正常完成 → `turn_state = Idle`
- Ctrl+C 触发 → `turn_state = Cancelling`
- finalize 完成（pending_assistant_idx 收尾、buffer flush） → `turn_state = Cancelled` → `Idle`

每个切换点都加注释说明触发条件。

##### Step 3：渲染时按状态画终止符

`app.rs::render` 里：

```rust
if self.app_state.turn_state == TurnState::Cancelling {
    // 在 pending_assistant_idx 对应消息末尾画终止符
    if let Some(idx) = self.pending_assistant_idx {
        if let Some(msg) = self.messages.get_mut(idx) {
            // 给 content 末尾追加一个 styled 标记（不要污染 raw text，仅渲染时插）
            // 或者在 widgets/message.rs 的 render 里加一个 if turn_state==Cancelling 的分支
        }
    }
}
```

实现方式以现有 widgets/message.rs 风格为准。

#### 验证

```bash
cd /Users/allen/Documents/rustmossen
cargo build --workspace 2>&1 | tail -5
```

#### 完成判定

- build 过
- 撕裂消除需要真 TTY 实测，留到 Phase 4

#### 回滚

```bash
git checkout -- crates/mossen-tui/src/
```

---

## 5. Phase 3 阶段验收

```bash
cd /Users/allen/Documents/rustmossen
cargo build --workspace 2>&1 | tail -3
cargo test -p mossen-tui 2>&1 | tail -5
ls crates/mossen-tui/benches/markdown_streaming.rs
```

**Phase 3 完成判定**：

- workspace 0 error
- mossen-tui 测试至少不**新增**失败
- bench 文件存在

向用户报告：

> Phase 3 完成。TodoWrite / sub-agent / Ctrl+C 的渲染管线已接通，性能基线已测。准备进 Phase 4 真机验收。

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
| 编译 mossen-tui | `cargo build -p mossen-tui` |
| 全 workspace | `cargo build --workspace` |
| 跑 TUI 测试 | `cargo test -p mossen-tui` |
| 跑 benchmark | `cargo bench -p mossen-tui --bench markdown_streaming` |

### C. 本阶段关键文件 quick ref

| 改/读什么 | 去哪 |
|------|------|
| TUI 主循环 | `crates/mossen-tui/src/app.rs` |
| TUI 状态 | `crates/mossen-tui/src/state.rs` |
| Widget 树（在用） | `crates/mossen-tui/src/widgets/` |
| Components（数据壳 + Modal widget） | `crates/mossen-tui/src/components/` |
| TaskListV2 数据壳 | `crates/mossen-tui/src/components/tasks.rs` |
| Teammate spinner | `crates/mossen-tui/src/components/spinner_anim.rs` |
| Markdown 解析 | `crates/mossen-tui/src/widgets/markdown.rs` |
| Sub-agent 派生 | `crates/mossen-tools/src/agent_tool/` |
| TodoWrite 工具 | `crates/mossen-tools/src/todo_write_tool/` |

---

文档版本：v2.1（拆分版）
**完整 5 阶段计划见 `PLAN_PRODUCTION.md`**
