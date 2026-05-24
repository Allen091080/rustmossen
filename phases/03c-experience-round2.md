# Phase 3.5 Round 2：渲染体验质感（按 audit 真实痛点补完）

> **目标读者**：本地 DeepSeek V4 Flash（通过 ds4-server 运行 Mossen 自身或其他 agent 来执行本文档）。
> **完整 5 阶段索引**：`/Users/allen/Documents/rustmossen/PLAN_PRODUCTION.md`
> **本阶段位置**：Phase 3.5 的第二轮（Round 1 是 `phases/03b-experience.md`）。
> **前置**：03b（Round 1）已完成 —— spinner 叶子动画、permission 弹窗结构化、TaskListV2 接通、Phase 4-1 测试全修。

---

## 1. 这一轮为什么还要做（必读）

### 1.1 Round 1 实际做完了什么

03b 那一轮，模型完成：

| Round 1 改动 | 文件 | 状态 |
|------|------|------|
| Spinner 叶子动画 6 帧 + stalled 渐变红 | `widgets/spinner.rs` | ✅ 真改 |
| Permission 弹窗结构化（`Shell{command}` / `FileEdit{path}` / `FileWrite{path}` / `FileRead{path}` / `WebFetch{url}` 变体） | `components/permissions.rs` | ✅ 数据层接通，**但 UI 显示待验证** |
| TaskListV2 widget 真接到 app.rs | `app.rs` + `components/tasks.rs` | ✅ 真接 |
| Teammate spinner tree widget 真接 | `app.rs` | ✅ 接了 |
| Transcript / message rendering 大改 | `widgets/message.rs` +604 行 | ⚠️ 改了大量代码，**TTY 视觉效果待用户验证** |
| Phase 4-1 测试全修 | `bash_tool/*`, `utils/{semver,json_read,early_input,array}.rs`, `skills/dynamic.rs` | ✅ workspace 0 failed |

### 1.2 烤机 audit 暴露的 Top 5 痛点 vs Round 1 覆盖

烤机 audit（`/tmp/mossen_experience_audit.md`）总结了 5 个最影响体验的问题：

| audit Top 5 | Round 1 是否解决 |
|---|---|
| 1. **Transcript / tool result 不显示**（模型做了啥用户看不见） | ⚠️ message.rs 大改了，但**视觉效果未在 TTY 验证**。审计当时根本看不到 assistant 回复块、Read 文件内容、Bash output、Grep 匹配列表 |
| 2. **Permission 弹窗缺参数**（用户 Allow 前看不到风险） | ⚠️ 数据层加了 `Shell{command}` / `FileRead{path}` 等变体，但 **UI 渲染端是否真把参数画出来还没看到** |
| 3. **TodoWrite / Task 卡死后续工具流**（180 秒不结束、Enter Ctrl+C 都无效，要外部 kill） | ❌ 没碰 |
| 4. **Bash/Read/Grep/Edit 缺专用卡片和结果渲染** | ❌ 没碰（permission 只是 popup，工具卡片是 transcript 里的渲染） |
| 5. **Ctrl+C 取消语义不稳定**（简单长任务会退出整个 TUI；卡死时又无效） | ❌ 没碰（Round 1 加了 `TurnState` enum，但 Cancelled 状态 1 帧消失的 bug 仍在） |

**Round 1 解了 1 个、改善了 1 个、留了 3 个完全没动**。本轮专门收尾 audit Top 5。

### 1.3 本阶段（Round 2）要解决什么

按 audit 真实优先级排：

| 优先级 | Task | 一句话目标 |
|---|---|---|
| 🔴 P0 | 3c-1 | 让 Tool result 在 transcript 里**真的看得见**（Bash output / Read 内容 / Grep 匹配） |
| 🔴 P0 | 3c-2 | Edit/Write 加真正的 diff widget（着色 + 行号 + 上下文行） |
| 🔴 P0 | 3c-3 | Permission 弹窗 UI 端**把已结构化的参数画出来**（path、command、diff） |
| 🟡 P1 | 3c-4 | 修 TodoWrite / Task 卡死后续工具流的 harness bug |
| 🟡 P1 | 3c-5 | Sub-agent 内部 Bash permission 真嵌套到主屏（不是 breadcrumb） |
| 🟡 P1 | 3c-6 | Ctrl+C 语义稳定：第一次取消、不退 TUI；卡死时也要能取消 |
| 🟢 P2 | 3c-7（可选） | Thinking block 完整体验（30 秒 fade + Ctrl+E pin） |

7 个 task。**P0 三个必做，P1 三个必做，P2 看时间挑**。

### 1.4 本阶段完成判定

- workspace build / test 不退化
- 用户在 TTY 复跑 audit 的 8 项操作（A-H）：
  - A 简单问候 → 看得到 assistant 回复块
  - B 读文件 → 看得到 Read 卡片有路径 + 内容预览 + 行号
  - C Bash → 看得到命令 + stdout/stderr 渲染
  - D Edit/Write → 看得到 diff 着色
  - E Grep → 看得到 pattern + 匹配文件列表
  - F TodoWrite → 不再卡死，连续两次能正常
  - G Task → 子 agent 内部工具调用在主屏可见可操作
  - H Ctrl+C → 长任务取消 → 不退 TUI；卡死能取消
- audit 里 Top 5 痛点至少 4 个明显改善

### 1.5 本阶段**不要做**的事

- **不要**重新改 Round 1 已做的 spinner / permission 数据层 / TaskListV2 接线
- **不要**重写 ratatui / 换 TUI 框架
- **不要**追求"像素级对照 TS 版"
- **不要**碰 `components/` 和 `terminal-framework/` 子树的删除
- **不要**在 P0 没做完时跳 P1 / P2
- **不要**在没看过 audit.md（`/tmp/mossen_experience_audit.md`）的情况下开做。**第一件事就是读它**

---

## 2. 阅读约定

### 2.1 角色与权限

你是 Rust 工程师。**可以**：`Read` / `Edit` / `Write` / `Bash`。
**绝对不能**：`git push` / `git reset --hard` / `rm -rf` / 修改 `/Users/allen/Documents/ds4/`。

### 2.2 执行节奏

一次一个 task，5 段结构：背景 / 位置 / 改动 / 验证 / 完成判定 / 回滚。

**特别提醒**：本轮大量 task 涉及视觉效果，agent 在非 TTY 环境只能 build + 简单 oneshot 验证不 panic。**真正"看着对不对"必须停下来让用户在 TTY 里看**。

### 2.3 卡住时

立即停下报告，不要猜。

### 2.4 命令前缀

默认 `cd /Users/allen/Documents/rustmossen`。

### 2.5 基线确认

```bash
cd /Users/allen/Documents/rustmossen
cargo check --workspace 2>&1 | tail -3
cargo test --workspace --no-fail-fast 2>&1 | grep "test result:" | tail -10
```

期望：build `Finished`、测试 0 failed（Round 1 全修了，本轮不能退化）。

### 2.6 沟通规则

- 开始：`正在做 3c-X：<标题>`
- 完成：`3c-X 完成。验证通过：<关键输出>`
- 失败：`3c-X 失败 / 卡住。详情：<错误>。已停下。`

---

## 3. 第 0 件事：读 audit.md

```bash
cat /tmp/mossen_experience_audit.md
```

**必须读完整份**，特别关注：

- A-H 各操作的 `[PROBLEM]` 条目（你具体要解决什么）
- "最影响体验的 Top 5"段
- "建议进入 A 路径的候选 task" 段（之前选了哪 5 个、之前漏了哪些 —— 本轮要补上）

读完之后再开始 3c-1。**不读直接开做，必然抓错重点**。

---

## 4. Task 详情

### 3c-1 Tool result 渲染（Bash / Read / Grep）—— P0

#### 背景

audit 反复出现的最大痛点：

- **A**：assistant 回复块看不到
- **B**：Read 工具执行后看不到文件内容、摘要、路径、行号
- **C**：Bash 允许后看不到 shell output
- **E**：Grep 没渲染可读的匹配文件列表

Round 1 在 `widgets/message.rs` 加了 604 行，可能解了部分（assistant text block），但**Bash / Read / Grep 的 tool result 仍然没专门渲染**。

#### 位置

`crates/mossen-tui/src/widgets/message.rs`，找 `render_body` 或 `match tool_name` 分支。

```bash
grep -n "tool_name\|render_body\|match.*tool" crates/mossen-tui/src/widgets/message.rs | head -20
```

#### 改动

在 ToolResult 渲染路径里**按 tool_name 加分支**，为下面 3 个工具各加专门渲染：

##### 3c-1-A Bash

```rust
// 当 tool_name == "Bash"：
// 1. 显示原始命令（从对应的 ToolUse 块拿，需要在 state 里关联 tool_use_id → input）
// 2. exit_code（如果在 ToolResult 里）
// 3. stdout / stderr 分两块显示，stderr 用 red 着色
// 4. 长输出（> 30 行）折叠为 "[X lines, expand with Enter]" 占位
```

参考布局（用 ratatui Block + Paragraph）：

```
┌─ Bash ─────────────────────────────
│ $ ls
│ exit: 0
│ ────────────────────
│ Cargo.lock
│ Cargo.toml
│ README.md
│ ...
│ [12 more lines]
└────────────────────────────────────
```

##### 3c-1-B Read

```rust
// 当 tool_name == "Read"：
// 1. 显示文件路径
// 2. 显示读取范围（offset + limit 或 "full file"）
// 3. 文件内容用 syntect 高亮（按扩展名识别语言）
// 4. 长文件折叠，默认显示前 20 行
// 5. 行号列固定宽度
```

参考布局：

```
┌─ Read: /Users/allen/Documents/rustmossen/Cargo.toml ─
│   1 │ [workspace]
│   2 │ resolver = "2"
│   3 │ members = [
│   4 │     "crates/mossen-cli",
│ ...
│  [33 more lines]
└──────────────────────────────────────────────
```

##### 3c-1-C Grep

```rust
// 当 tool_name == "Grep"：
// 1. 显示 pattern + path + mode（files_with_matches / content / count）
// 2. 匹配数量
// 3. 匹配文件列表（如果 mode = files_with_matches）
// 4. 匹配行（如果 mode = content）—— 高亮命中的 pattern
```

参考布局：

```
┌─ Grep: pattern="unsafe" path="crates/" mode=files_with_matches ─
│ 27 matches in 14 files
│   crates/mossen-utils/src/lib.rs
│   crates/mossen-utils/src/raw_buf.rs
│   crates/mossen-tui/src/widgets/markdown.rs
│   ...
└────────────────────────────────────────────────────
```

#### 关于"从 ToolUse 拿到 input"

ToolResult 消息本身不带 input（input 在对应的 ToolUse 块里）。需要：

1. 在 `state.rs` 加一个 `pending_tool_inputs: HashMap<String, serde_json::Value>`，key 是 `tool_use_id`
2. handle_engine_message 看到 ToolUse 块时把 input 缓存
3. 看到 ToolResult 时按 `tool_use_id` 取出 input，传给 render

或者更简单：把 ToolUse 和 ToolResult **合并成一个 MessageData**（用 `tool_use_id` 配对），存在 state 的 messages vec 里。看 Round 1 message.rs 的现有数据结构选哪种最自然。

#### 验证

```bash
cargo check --workspace 2>&1 | tail -3
cargo build --workspace 2>&1 | tail -3
```

#### 完成判定

- build 0 error
- **TTY 视觉验证**（必须停下让用户跑）：
  - Bash 命令执行后看得到命令 + output
  - Read 看得到文件路径 + 内容
  - Grep 看得到 pattern + 匹配文件

#### 回滚

```bash
git checkout -- crates/mossen-tui/src/widgets/message.rs crates/mossen-tui/src/state.rs
```

---

### 3c-2 Edit/Write diff widget —— P0

#### 背景

audit D：

> Write/Edit permission 弹窗只显示工具名，没有显示目标路径和写入内容预览 / old/new 文本或 diff。**实际文件最终内容是 `hello mossen`，说明工具执行成功，但 TUI 没有 diff widget、行号、颜色或上下文行。**

Round 1 没碰 diff。本轮要做一个真正的 diff widget。

#### 位置

新建文件 `crates/mossen-tui/src/widgets/diff.rs` 或加在 `widgets/message.rs` 里。

需要 diff 库：`similar` crate（看 Cargo.toml 是否已有；没有就加 dev-dep）：

```bash
grep -n "similar" crates/mossen-tui/Cargo.toml
grep -rn "similar::" crates/ | head -3
```

#### 改动

##### 3c-2-A 新增 `DiffWidget`

```rust
pub struct DiffWidget<'a> {
    pub path: &'a str,
    pub old_content: &'a str,
    pub new_content: &'a str,
    pub theme: &'a crate::theme::Theme,
    pub context_lines: usize, // 默认 3
    pub max_lines: usize,     // 折叠阈值，默认 30
}

impl<'a> Widget for DiffWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // 1. 用 similar::TextDiff::from_lines(old, new) 算 diff
        // 2. 遍历 changes, 按 ChangeTag 着色：
        //    - Insert: green，prefix `+`
        //    - Delete: red，prefix `-`
        //    - Equal: 默认色（上下文行）
        // 3. 行号列：old_line_no / new_line_no 各占固定宽度
        // 4. 总行数 > max_lines 时显示 "...N hidden lines..."
    }
}
```

参考布局：

```
┌─ Edit: /tmp/mossen_smoke.txt ──────
│  1  1 │ hello world          ← unchanged
│  -  - │ -hello mossen        ← removed (red)
│  -  2 │ +hello mossen        ← added (green)
└────────────────────────────────────
```

##### 3c-2-B 接入 ToolResult 渲染

在 `widgets/message.rs::render_body` 的 `tool_name == "Edit"` 和 `"Write"` 分支里，调用 `DiffWidget`：

```rust
match tool_name.as_str() {
    "Edit" => {
        // 从 ToolUse input 拿 file_path / old_string / new_string
        // 渲染 DiffWidget
    }
    "Write" => {
        // Write 是新文件，没 old，把 old_content 设为 "" 让所有行都是 + green
    }
    "Bash" => { /* 3c-1-A */ }
    "Read" => { /* 3c-1-B */ }
    "Grep" => { /* 3c-1-C */ }
    _ => { /* 通用渲染 */ }
}
```

#### 验证

```bash
cargo check --workspace 2>&1 | tail -3
```

视觉验证由用户在 TTY 做：

```
让 mossen: 创建 /tmp/diff_test.txt 写 "hello\nworld\n"，然后改 "world" 为 "moo"
```

#### 完成判定

- build 0 error
- TTY 视觉：能看到 diff 行号 + 颜色 + 上下文行

#### 回滚

```bash
git checkout -- crates/mossen-tui/src/widgets/
```

---

### 3c-3 Permission 弹窗 UI 把已结构化的参数画出来 —— P0

#### 背景

Round 1 给 `components/permissions.rs` 加了 `Shell{command}` / `FileEdit{path}` 等变体，但**只是数据结构**。audit 报告 permission 弹窗仍然"只显示 `Tool: Read` / `Read`，没有显示路径 `Cargo.toml`"。

也就是说：**变体加了，渲染端没读变体里的参数画出来**。本 task 修这个。

#### 位置

```bash
grep -rn "PermissionRequest\|permission_dialog\|render.*permission" crates/mossen-tui/src/components/permissions.rs | head -10
grep -rn "Tool: " crates/mossen-tui/src/ | head -5
```

#### 改动

找 permission 弹窗的 render 函数（应该在 `components/permissions.rs::AccessGateWidget::render` 或类似名字）。当前可能是这样：

```rust
// 当前（约略）
let title = format!("Tool: {}", tool_name);
// ... render title only
```

改成：

```rust
let (title, details) = match permission_kind {
    PermissionKind::Shell { command } => {
        ("Bash".to_string(), Some(format!("$ {}", command)))
    }
    PermissionKind::FileRead { path } => {
        ("Read".to_string(), Some(format!("path: {}", path)))
    }
    PermissionKind::FileEdit { path } => {
        ("Edit".to_string(), Some(format!("path: {}", path)))
        // 如果能拿到 old/new diff，调用 3c-2 的 DiffWidget 在 details 里渲染
    }
    PermissionKind::FileWrite { path } => {
        ("Write".to_string(), Some(format!("path: {}", path)))
    }
    PermissionKind::WebFetch { url } => {
        ("WebFetch".to_string(), Some(format!("url: {}", url)))
    }
    // ... 其他变体
    _ => (tool_name.clone(), None),
};

// render title + details（details 用 dim 风格画在按钮上方）
```

⚠️ **如果发现 `Shell{command}` 等变体在 ds4-server / engine 端**根本没填**（即 PermissionRequest 还是用通用 `Tool{name}` 变体送过来），那就**停下报告** —— Round 1 数据层的事可能没接通到产生端。

#### 验证

```bash
cargo check --workspace 2>&1 | tail -3
```

视觉验证：用户在 TTY 跑 `ls` 等需要 permission 的命令，看弹窗是不是显示完整命令。

#### 完成判定

- build 过
- TTY 视觉：Bash permission 显示具体命令；Read permission 显示路径；其他工具类似

#### 回滚

```bash
git checkout -- crates/mossen-tui/src/components/permissions.rs
```

---

### 3c-4 TodoWrite / Task 卡死后续工具流 —— P1

#### 背景

audit F + G 描述的最严重交互 bug：

> TodoWrite 完成后模型进入重复/异常工具流：日志里出现 TodoWrite complete、后续 model call、TaskList、延迟重复 TodoWrite、又一次 TaskList permission。**TUI 长时间停在 `Thinking...`，TaskList 卡片被覆盖/挤压，超过 180 秒没有回到可输入状态。**Enter 和 Ctrl+C 都不能可靠恢复，最后需要外部 kill 进程。

这**不是渲染 bug，是 harness 层的 deadlock 或 permission 协议错位**。但它 100% 影响体验。

#### 位置

需要先**审计**才能改。日志在 audit.md 顶部列出：

```
/Users/allen/Library/Caches/mossen/logs/mossen-87823.log
/Users/allen/Library/Caches/mossen/logs/mossen-88551.log
/Users/allen/Library/Caches/mossen/logs/mossen-92795.log
```

#### 改动

##### Step 1：先审计日志找根因

读其中一个 log，找：
- TodoWrite tool_use → tool_result 之间有没有正常完成？
- 之后又发起了什么 tool_use？是不是同样的 TodoWrite？
- 是否在等用户 permission？哪一边在等？

输出审计报告 `crates/mossen-agent/TODOWRITE_DEADLOCK_AUDIT.md`：

```markdown
# TodoWrite / Task 卡死分析 / Phase 3c-4

## 现象重述

- 触发：用户让 mossen 用 TodoWrite 加 3 个任务
- 第一次 TodoWrite tool_use → permission accept → tool_result OK
- 然后：（写从日志看到的实际事件序列）

## 日志关键时间线

| 时间 | 事件 | 来源 |
|---|---|---|
| ... | TodoWrite tool_use | mossen-87823.log:LXX |
| ... | TodoWrite permission accept | ... |
| ... | TodoWrite tool_result | ... |
| ... | ??? | ... |

## 推测根因

（基于日志推测：是 model 进入循环？还是 TUI 状态机错误等 permission？还是 KV cache 命中失败导致 prefill 重跑？）

## 修复建议

（具体哪个 file:line 应该改什么）
```

##### Step 2：根据审计结论决定

- 如果是 **model 端循环**（model 看 TaskList 结果不对自己又触发 TodoWrite）：调整 system prompt 或 TodoWrite 工具的 result 格式
- 如果是 **TUI 状态机错位**（TUI 以为还在等 permission 但 engine 已经 OK）：修 app.rs 状态转换
- 如果是 **deadlock**（两端互等）：找出在等什么，加 timeout 或主动 release

**根因不明时停下报告**，让用户决定怎么修。

#### 验证

修完后 TTY 复跑 audit F：

```
让 mossen: 用 TodoWrite 添加 3 个任务：吃饭、睡觉、打豆豆。
（连续 2 次）
```

应当：
- 第一次和第二次都能正常完成
- 不卡死，不需要外部 kill
- TaskList 卡片稳定显示

#### 完成判定

- 审计文件存在
- 如果做了 fix：build 过 + TTY 验证不卡死
- 如果根因不明：审计文件清楚说明等谁来定 fix

#### 回滚

```bash
git diff
git checkout -- <受影响文件>
```

---

### 3c-5 Sub-agent 内部 Bash permission 真嵌套到主屏 —— P1

#### 背景

audit G：

> 允许 Task 后，子 agent 内部继续发起 Bash tool_use；日志能看到 `tool_name=Bash`，但**主屏没有显示可操作的 Bash permission 弹窗**。Task 流程因此卡在全局 `Thinking...`。

Round 1 的 3-2 加了 SdkMessage::task_id 字段 + teammate spinner tree，但 sub-agent 内部的 permission 请求**没有冒泡到主屏 ActiveModal**。

#### 位置

```bash
grep -rn "PermissionRequest\|ActiveModal::Permission" crates/mossen-tools/src/agent_tool/ crates/mossen-agent/src/dialogue.rs | head -10
grep -rn "subagent\|sub_agent" crates/mossen-agent/src/services/tools/ | head -10
```

#### 改动

##### Step 1：审计 sub-agent 内部 permission 路径

子 agent（agent_tool/run_agent.rs）派生时拿到的 `canUseTool` callback 是什么？

- 如果是 **null / no-op**：子 agent 内部 Bash 直接 deny / unconditionally allow，那就**完全不弹**
- 如果是 **同主 agent 的 canUseTool**：理论上能弹，但可能 UI 没接到子 agent 的 task_id 路由

##### Step 2：让子 agent permission 经主 ActiveModal

最直接：子 agent 的 permission request 通过 engine_tx 走 `SdkMessage::PermissionRequest { task_id: Some("..."), ... }` 类型（如果还没这个变体，加上）。

app.rs::handle_engine_message 看到 PermissionRequest：

- 若 task_id == None → 正常弹（已有逻辑）
- 若 task_id == Some(tid) → 也弹，但 title 里加 `[teammate XXX]` 前缀，让用户知道是子 agent 的请求

弹窗按钮选择回灌也要走 task_id 路由：accept → 给子 agent 的 callback 而不是主 agent。

##### Step 3：如果设计上更复杂

如果发现子 agent 是完全独立的 query() loop，跟主 agent 共享 engine_tx 但不共享 ActiveModal，**停下报告**，让用户决定要不要重新设计 permission 路由。

#### 验证

```bash
cargo check --workspace 2>&1 | tail -3
```

TTY 复跑 audit G：

```
用 Task 工具派一个 general-purpose agent 去统计 crates/ 下 .rs 文件数，并只返回数字。
```

应当：
- Task permission 弹窗显示 agent 类型 + prompt
- 子 agent 内部 Bash 调用时主屏出现 permission（标记 `[teammate XXX]`）
- 用户能 accept 子 agent 的 Bash
- 子 agent 完成时给出数字结果

#### 完成判定

- build 过
- TTY 视觉：子 agent permission 在主屏可见可操作

#### 回滚

```bash
git checkout -- crates/mossen-tools/src/agent_tool/ crates/mossen-agent/src/ crates/mossen-tui/src/app.rs
```

---

### 3c-6 Ctrl+C 语义稳定 —— P1

#### 背景

audit H：

> 在早期简单长请求中，**Ctrl+C 直接退出整个 TUI**，没有显示 `Cancelled` 或返回输入框。
> 在 TodoWrite/Task 卡住状态中，Ctrl+C 没有立即取消，也没有明显视觉反馈。

Round 1 加了 `TurnState::Cancelling/Cancelled` enum，但实际行为没修。

#### 位置

```bash
grep -n "Ctrl.C\|InputAction::Interrupt\|TurnState::Cancel" crates/mossen-tui/src/app.rs crates/mossen-tui/src/event.rs | head -10
```

#### 改动

##### Step 1：Ctrl+C 退 TUI 的根因

理论上 Ctrl+C 在 raw mode 下不应该退 TUI（crossterm 应该 capture）。如果实际退了，可能是：
- raw mode 没启用 / 被 disable 了
- panic（cargo build --release vs debug 不一样）
- SIGINT 直接被 process 收到（没 capture）

查 `crates/mossen-tui/src/event.rs` 的 crossterm 初始化逻辑。

##### Step 2：Cancelling → 可见 → Idle 的 transition

Round 1 加的 `TurnState::Cancelling` 1 帧后立刻转 `Idle`，用户根本看不到。修复：

```rust
// app.rs 的 handle_key Ctrl+C 处理
self.state.turn_state = TurnState::Cancelling;
self.engine_rx = None;  // 断 engine
// 关键：插一个 "↯ 已取消" 消息进 messages vec，不要立刻 Idle
self.messages.push(MessageData {
    message_type: MessageType::System,
    content: "↯ Cancelled".to_string(),
    ...
});
self.state.turn_state = TurnState::Idle;  // OK 现在可以 Idle，但 "↯ Cancelled" 消息留下来了
```

##### Step 3：卡死时的强制取消

audit F 的卡死状态下 Ctrl+C 无效，可能是因为：
- engine_rx 没有真在等数据（已经断了，等的是别的 channel）
- 或者 ActiveModal::PermissionRequest 状态下 Ctrl+C 没接

加：

```rust
// 卡死场景双保险：Ctrl+C 总是 reset 状态
KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
    // 1. 把所有 in-flight 的 modal close 掉
    self.active_modal = ActiveModal::None;
    // 2. 断 engine_rx
    self.engine_rx = None;
    // 3. 重置 turn state
    self.state.turn_state = TurnState::Idle;
    self.state.is_streaming = false;
    self.state.is_waiting_for_response = false;
    // 4. 留一行 "↯ Cancelled"
    self.messages.push(MessageData { ... });
    return;
}
```

确认这一段在所有 key 处理路径里**最先**被检查（不要被 modal handler 抢走）。

#### 验证

```bash
cargo check --workspace 2>&1 | tail -3
```

TTY 视觉验证：

1. 跑一个长任务（比如让 mossen 找 src 下所有 unsafe 块）→ 中途 Ctrl+C → 应该看到 "↯ Cancelled" 留在屏幕上 + 回到输入框，**TUI 不退**
2. 故意触发 audit F 的卡死场景 → Ctrl+C → 应该立刻清掉 modal + 显示 cancelled + 可以输入下一条

#### 完成判定

- build 过
- TTY 验证：Ctrl+C 不退 TUI、cancelled 状态可见、卡死也能恢复

#### 回滚

```bash
git checkout -- crates/mossen-tui/src/app.rs crates/mossen-tui/src/event.rs crates/mossen-tui/src/state.rs
```

---

### 3c-7（可选）Thinking block 完整体验

#### 适用场景

P0 / P1 全做完，还有余力 + audit.md 里 A 段 thinking 体验还是不达预期。

#### 背景

TS Mossen 的 thinking block 有 3 个细节：

- 灰字 + dim/italic + shimmer
- streaming 完成后 30 秒淡出
- Ctrl+E 可以 pin 住不淡出

Round 1 spinner.rs 加了 shimmer 颜色变化，但**thinking block 本身的 30s fade 和 Ctrl+E pin 没确认**。

#### 位置

```bash
grep -n "thinking\|Thinking" crates/mossen-tui/src/widgets/message.rs | head -15
grep -n "thinking_completed_at\|pinned_thinking" crates/mossen-tui/src/ -r | head -5
```

#### 改动

##### Step 1：审计现状

- `thinking_completed_at: Option<Instant>` 字段在 MessageData 里有吗？
- render 时是否检查 `now - thinking_completed_at > 30s`？
- `pinned_thinking_ids` 在 AppState 里有吗？
- Ctrl+E 有 key handler 吗？

##### Step 2：补缺

```rust
// state.rs::AppState 加
pub pinned_thinking_ids: std::collections::HashSet<usize>,  // message idx

// app.rs::handle_key 加 Ctrl+E
KeyCode::Char('e') if modifiers.contains(KeyModifiers::CONTROL) => {
    if let Some(idx) = self.focused_message_idx {
        if !self.state.pinned_thinking_ids.insert(idx) {
            self.state.pinned_thinking_ids.remove(&idx); // toggle
        }
    }
    return;
}

// widgets/message.rs::render_thinking 渲染时检查
let should_render = self.data.thinking.is_some() && (
    self.app_state.pinned_thinking_ids.contains(&self.idx)
    || self.data.thinking_completed_at.map_or(true, |t| t.elapsed().as_secs() < 30)
);
if !should_render { return; }
```

#### 验证

```bash
cargo check --workspace 2>&1 | tail -3
```

TTY 验证：

- 让 mossen 思考一段话 → thinking 灰字出现
- 等 30 秒 → thinking 应该淡出消失
- 重新触发一次思考 → Ctrl+E → 30 秒后不淡出

#### 完成判定

- build 过
- TTY 验证：30s fade + Ctrl+E pin 都生效

#### 回滚

```bash
git checkout -- crates/mossen-tui/src/
```

---

## 5. 阶段验收

### 5.1 build / test 不退化

```bash
cd /Users/allen/Documents/rustmossen
cargo build --workspace 2>&1 | tail -3
cargo test --workspace --no-fail-fast 2>&1 | grep "test result:" | tail -10
```

期望：build 0 error、test 0 failed（Round 1 已经全过，本轮不能退化）。

### 5.2 重跑 audit 的 8 项操作（必须用户手动）

⚠️ **用户在真 TTY 里跑**：

```bash
cd /Users/allen/Documents/rustmossen
cargo build --workspace --release
MOSSEN_CODE_USE_CUSTOM_BACKEND=1 \
MOSSEN_CODE_CUSTOM_BASE_URL=http://localhost:8000 \
MOSSEN_CODE_CUSTOM_MODEL=deepseek-v4-flash \
./target/release/mossen
```

照 `/tmp/mossen_experience_audit.md` 的 A-H 8 项每项独立 turn 跑一遍。每项判定：

- 看到 audit 里期望的视觉效果 → ✅
- 仍然是原 PROBLEM → ❌
- 部分改善 → 🟡

把结果写到 `/tmp/mossen_experience_audit_round2.md`，对照 Round 1 的版本。

### 5.3 完成判定

- A-H 至少 6/8 标记 ✅（包括 audit 标识的 Top 5 痛点至少改善 4 个）
- workspace build / test 不退化

### 5.4 报告

向用户报告：

> Phase 3.5 Round 2 完成。
> - 做了 X 个 task：3c-1, 3c-2, ...
> - audit A-H 8 项验收结果（写出来）
> - 原 Top 5 痛点改善情况
>
> 可以进 Phase 4-2 / 4-3（30 min 烤机 + 1h sprint）吗？

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
| Release（烤机用） | `cargo build --workspace --release` |
| 测试 | `cargo test --workspace --no-fail-fast` |
| TTY 跑（用户手动） | 见 5.2 |
| 看 audit | `cat /tmp/mossen_experience_audit.md` |

### C. 本阶段关键文件 quick ref

| 改/读什么 | 去哪 |
|------|------|
| Audit 原始记录 | `/tmp/mossen_experience_audit.md` |
| TUI 主循环 | `crates/mossen-tui/src/app.rs` |
| TUI 状态 | `crates/mossen-tui/src/state.rs` |
| 消息渲染（Round 1 改过） | `crates/mossen-tui/src/widgets/message.rs` |
| 工具 tool result 分支（本轮 3c-1） | 同上 |
| Permission 弹窗（Round 1 加结构、本轮 3c-3 渲染） | `crates/mossen-tui/src/components/permissions.rs` |
| Diff widget（本轮 3c-2 新增） | `crates/mossen-tui/src/widgets/diff.rs`（新建）或 `widgets/message.rs` |
| Sub-agent 派生（本轮 3c-5） | `crates/mossen-tools/src/agent_tool/run_agent.rs` |
| Sub-agent UI 路由（本轮 3c-5） | `crates/mossen-tui/src/app.rs::handle_engine_message` |
| Ctrl+C 处理（本轮 3c-6） | `crates/mossen-tui/src/app.rs::handle_key` |
| Event 模块 | `crates/mossen-tui/src/event.rs` |
| Mossen 日志位置 | `/Users/allen/Library/Caches/mossen/logs/mossen-*.log` |

### D. 一个核心心法（同 Round 1）

**ratatui 是 immediate-mode**：每帧从纯 state 重画整屏。

- 不要 React 的"组件持久 state"
- 所有动态状态（spinner 帧、thinking fade 计时、turn state、pinned ids）**集中在 AppState**
- render 是纯函数 `fn render(state: &AppState, frame: &mut Frame)`
- 时间相关动画用 `Instant::now()` 在 render 时算

如果你想加"组件内部状态"，停下问自己：是不是应该提升到 AppState？

### E. 本轮和 Round 1 的关系

- Round 1（`phases/03b-experience.md`）：做了 spinner 叶子动画、permission 数据层、TaskListV2 接通、message.rs 大改、Phase 4-1 测试全修
- Round 2（**本文件**）：补 audit 里 Round 1 没解的 Top 5 痛点 —— tool result 不显示、permission 弹窗 UI、TodoWrite 卡死、子 agent 嵌套、Ctrl+C 稳定

两轮加起来覆盖 audit 的全部 PROBLEM 条目。

---

文档版本：v1.0（Round 2，2026-05-20）
**完整 5 阶段计划见 `PLAN_PRODUCTION.md`**
**Round 1 文件**：`phases/03b-experience.md`
**Audit 原始记录**：`/tmp/mossen_experience_audit.md`
