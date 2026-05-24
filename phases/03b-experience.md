# Phase 3.5：渲染体验质感（B + A 路径）

> **目标读者**：本地 DeepSeek V4 Flash（通过 ds4-server 运行 Mossen 自身或其他 agent 来执行本文档）。
> **完整 5 阶段索引**：`/Users/allen/Documents/rustmossen/PLAN_PRODUCTION.md`
> **本阶段位置**：在 Phase 3（渲染层 gap 接合）和 Phase 4（生产验收）之间插入的 **3.5 阶段**
> **前置**：Phase 0、Phase 1、Phase 2、Phase 3（结构性 gap）大致完成

---

## 1. 这个阶段为什么存在（必读，否则不要动）

### 1.1 已经做了什么，没做什么

Phase 3 的 5 个 task 解决了**结构性 gap**（structural gap）：
- TodoWrite 在 TUI 是否真有渲染入口
- Sub-Agent 派生时 TUI 是否有反应
- Ctrl+B 是否能切后台任务
- Markdown 流式 perf 基线
- Ctrl+C 是否会撕裂

这些是"**这个事到底有没有显示**"的问题。

但 Mossen / Mossen Code 这类 coding agent 在 CLI 里**好不好用，关键不在"有没有显示"，而在"显示得是不是有质感"**：

- Spinner 是不是一帧一帧叶子动画、stalled 时变红、verb 文字 shimmer
- Thinking block 是不是灰字 + shimmer + 30 秒后淡出
- Tool input JSON 是不是边解析边可视化
- Bash/Read/Edit/Grep 等工具是不是各有特化的卡片样式
- Diff 是不是行号 + 颜色 + 上下文行
- Prompt input 是不是支持 `/` 命令 typeahead + `@file` 补全
- 子 agent 输出是不是嵌套显示在主屏
- 长会话滚动是不是平滑、有 sticky header

这些都是**体验 gap**（experience gap）。Phase 3 完全没碰。

### 1.2 为什么不能直接列 task 干

体验 gap 的清单可以很长（光 components/ 目录就有 27000 行 widget 代码），全做等于重写 TUI。**做得多不如做得准** —— 应该先看用户在真实使用中**实际抱怨什么**，再按出现频率 + 影响面排优先级。

### 1.3 本阶段的两步法（B → A）

**B 步（烤机 + 收集真实痛点）**：

- 用户在真 TTY 里跑 mossen 30 分钟，做 8 项有代表性的操作
- 边跑边记 bug / 不适体验 → `/tmp/mossen_experience_audit.md`
- 不写代码，只记录

**A 步（按真实优先级补 task）**：

- 本文档给 7 个候选 task（3-6 ~ 3-12），覆盖最常见的体验维度
- 根据 B 步记录，**挑 3-5 个真出现痛点的 task** 做
- 其他候选保留在文档里，下个 sprint 再说

### 1.4 本阶段完成判定

- B 步：`/tmp/mossen_experience_audit.md` 存在，列了至少 5 条具体观察
- A 步：基于 B 步选定的 3-5 个 task 各自验证通过
- 重跑同样的 30 分钟烤机，原痛点至少 80% 改善

### 1.5 本阶段不要做的事

- **不要**在 B 步开始前盲目挑 task 做。挑错了等于白干
- **不要**一次做 7 个候选 task。**只做 B 步暴露出来的那几个**
- **不要**改 ratatui 库本身或换 TUI 框架。本阶段只动 `crates/mossen-tui` 内部代码
- **不要**碰 `components/` 和 `terminal-framework/` 整棵子树的删除（之前阶段已经说明：components 部分在用、terminal-framework 是平移港，留着）
- **不要**追求"像素级对照 TS 版"。Rust ratatui 是 immediate-mode，**有些 React 范式做得到的细节就是做不到**，不要硬凑

---

## 2. 阅读约定

### 2.1 角色与权限

你是 Rust 工程师。**可以**：用 `Read` / `Edit` / `Write` / `Bash`。
**绝对不能**：`git push` / `git reset --hard` / `rm -rf` / 修改 `/Users/allen/Documents/ds4/`。

### 2.2 执行节奏

一次一个 task，5 段结构：背景 / 位置 / 改动 / 验证 / 完成判定 / 回滚。

### 2.3 卡住时

立即停下报告，不要猜。**特别提醒**：本阶段大量"是否符合视觉预期"判定**需要用户肉眼确认**，agent 跑非 TTY 环境只能 build + 跑 oneshot 验证不 panic。**所有"视觉验收"必须停下来让用户做**。

### 2.4 命令前缀

默认 `cd /Users/allen/Documents/rustmossen`。

### 2.5 基线确认

```bash
cd /Users/allen/Documents/rustmossen
cargo check --workspace 2>&1 | tail -3
```

期望 `Finished`，无 `error[`。**如果有 error，必须先解** —— 见下面 Pre-1。

### 2.6 沟通规则

- 开始：`正在做 3b-X：<标题>`
- 完成：`3b-X 完成。验证通过：<关键输出>`
- 失败：`3b-X 失败 / 卡住。详情：<错误>。已停下。`

---

## 3. Pre-tasks：清掉 blocker（必须先做）

### Pre-1 修 `mossen-utils::array::uniq` 编译错

#### 背景

之前模型尝试做 Phase 4-1-A 时改了 `crates/mossen-utils/src/array.rs::uniq`，引入了编译错：

```
error[E0382]: use of moved value: `item`
  --> crates/mossen-utils/src/array.rs:29:25
   |
27 |     for item in items {
28 |         if seen.insert(item) {    // T 被 move 进 HashSet
29 |             result.push(item);    // 这里 T 已被 move
   |                         ^^^^ value used here after move
   |
help: if `T` implemented `Clone`, you could clone the value
```

需要先解，否则后面任何 cargo build 都失败。

#### 位置

`crates/mossen-utils/src/array.rs`，约 24 行：

```bash
grep -n "pub fn uniq" crates/mossen-utils/src/array.rs
```

#### 改动

把 trait bound 加上 `Clone`：

```rust
// before:
pub fn uniq<T: Eq + std::hash::Hash>(items: impl IntoIterator<Item = T>) -> Vec<T> {

// after:
pub fn uniq<T: Eq + std::hash::Hash + Clone>(items: impl IntoIterator<Item = T>) -> Vec<T> {
```

然后在循环里：

```rust
// before:
for item in items {
    if seen.insert(item) {
        result.push(item);
    }
}

// after:
for item in items {
    if seen.insert(item.clone()) {
        result.push(item);
    }
}
```

调用方都是字符串 / 数字（实现了 Clone），不会有性能问题。

#### 验证

```bash
cd /Users/allen/Documents/rustmossen
cargo check --workspace 2>&1 | tail -3
```

#### 完成判定

输出 `Finished`，无 `error[`。

#### 回滚

```bash
git checkout -- crates/mossen-utils/src/array.rs
```

---

### Pre-2 重启 ds4-server（如果停了）

#### 背景

用户之前为了释放内存停了 ds4-server。Phase 3.5 的 B 步（实地烤机）需要它跑。

#### 改动

⚠️ **本任务由用户手动跑**（涉及系统服务管理，agent 不应擅自启 / 停服务）。**遇到本任务停下报告**，让用户接手。

用户命令：

```bash
launchctl load ~/Library/LaunchAgents/com.allen.ds4-server.plist
sleep 5
curl -fsS http://localhost:8000/v1/models | head -5
```

#### 完成判定

`/v1/models` 端点 200 响应。

---

## 4. B 步：TTY 实地烤机 30 分钟（**用户手动跑**）

### 3b-0 实地烤机收集体验 bug 清单

#### 背景

任何 task 都不应在没有真实痛点数据的情况下盲做。本步骤产出**优先级依据**。

#### 谁来做

⚠️ **必须由用户在真 TTY 里跑**。Agent 在非 TTY 环境做不了交互测试。**遇到本 task 停下报告**让用户接手。

#### 用户操作

```bash
cd /Users/allen/Documents/rustmossen
cargo build --workspace --release
MOSSEN_CODE_USE_CUSTOM_BACKEND=1 \
MOSSEN_CODE_CUSTOM_BASE_URL=http://localhost:8000 \
MOSSEN_CODE_CUSTOM_MODEL=deepseek-v4-flash \
RUST_LOG=mossen_agent=info,mossen_cli=info \
./target/release/mossen
```

#### 8 项标准烤机操作（按顺序做，每项独立一个 turn）

每项做完，把"观察到的问题"和"期望表现"记到 `/tmp/mossen_experience_audit.md`。

##### A. 简单问候

```
你好
```

**观察项**：
- [ ] Spinner 出现了吗？什么样子？（叶子 🍃 / 转圈 / 其他）
- [ ] thinking block 显示了吗？灰字？shimmer 效果？
- [ ] 最终回复渲染样式？markdown 排版？
- [ ] 回复出完，spinner 是不是立刻消失？
- [ ] 主屏布局有没有问题？

##### B. 读文件

```
读一下 Cargo.toml
```

**观察项**：
- [ ] tool_use 卡片样式？带工具名 / 参数 / 状态点？
- [ ] tool_result 渲染？文件内容是 markdown 还是 raw？语法高亮？
- [ ] 折叠 / 展开？

##### C. Bash 命令 + 权限弹窗

```
跑一下 ls
```

**观察项**：
- [ ] Permission 弹窗弹出来了吗？
- [ ] 弹窗里几个按钮？字面意思（Yes / No / Always / Never）？
- [ ] 主屏被 Permission 弹窗遮挡的内容是怎么显示的？
- [ ] 选完后弹窗消失流畅吗？

##### D. Edit 文件 + diff 渲染

```
把 /tmp/mossen_smoke.txt 创建并写入 "hello world"，然后改成 "hello mossen"
```

**观察项**：
- [ ] Edit 工具的 diff 渲染？行号？颜色？上下文行？
- [ ] 改之前 Write 创建的样式？

##### E. 多轮 tool（让它做几件事）

```
找 crates/ 下所有包含 "unsafe" 的 .rs 文件
```

**观察项**：
- [ ] 多个 tool 串行调用时的视觉节奏？
- [ ] 长输出（Grep 结果）是怎么显示的？折叠？滚动？
- [ ] 每个 tool 之间有过渡动画 / 分隔符吗？

##### F. TodoWrite

```
用 TodoWrite 添加 3 个任务：吃饭、睡觉、打豆豆
```

**观察项**：
- [ ] TaskListV2 卡片出现在哪里？大小合适吗？被遮挡吗？
- [ ] 状态图标（◐ in_progress / ✓ completed / ○ pending）显示正确？
- [ ] 任务太多时怎么显示？

##### G. 子 agent（Task tool）

```
用 Task tool 派一个 general-purpose agent 去统计 crates/ 下 .rs 文件数
```

**观察项**：
- [ ] 子 agent 派生时主屏有反应吗？
- [ ] teammate spinner 树出现吗？
- [ ] 子 agent 跑的工具调用在主屏怎么显示？
- [ ] 子 agent 完成时怎么提示？
- [ ] **特别留意**：是不是只看到 `[teammate XXX event]` 占位文字？真实子 agent 内容呢？

##### H. Ctrl+C 中断

```
（在一个长任务进行到一半时按 Ctrl+C）
```

**观察项**：
- [ ] 立刻有视觉反馈吗？"↯ Cancelled" 显示出来了吗？
- [ ] 半截 markdown 撕裂吗？
- [ ] 输入框立刻就绪可输入下一条？
- [ ] **特别留意**：根据之前发现，Cancelled 状态可能 1 帧就消失（设了立刻又改回 Idle）

#### 输出格式

每项观察记到 `/tmp/mossen_experience_audit.md`，按这个模板：

```markdown
# Mossen 体验烤机 bug 清单

烤机日期：YYYY-MM-DD
烤机时长：30 min
mossen 版本：（git rev-parse --short HEAD）

## A. 简单问候

观察到的问题：
- [PROBLEM] spinner 不是叶子动画，是普通 - / | 转圈
- [PROBLEM] thinking block 没看到 shimmer
- ...

期望（如果不一样）：
- 应该看到 🍃 → 🌿 → ☘️ → 🍀 → ☘️ → 🌿 6 帧叶子动画

## B. 读文件

观察到的问题：
- ...

...

## H. Ctrl+C 中断

...

## 总体感受

- 最影响体验的 top 3 问题：
  1.
  2.
  3.
```

#### 完成判定

- 8 项都跑过（**全部跑完**，即使某项你觉得 OK 也要明确写"OK"）
- 至少 5 条具体可执行的 `[PROBLEM]` 条目（不能只写"感觉不太对"，要写**哪个细节不对、期望长什么样**）
- 文件存在于 `/tmp/mossen_experience_audit.md`

完成后**停下报告**给用户："3b-0 烤机完成。bug 清单见 /tmp/mossen_experience_audit.md。请用户根据清单决定 3b-1 ~ 3b-7 选哪几个做。"

---

## 5. A 步：候选 task 清单（**根据 B 步结果挑做**）

⚠️ **重要**：下面 7 个候选 task **不是全部都做**。根据 3b-0 烤机暴露的真实痛点 + 用户决定挑 3-5 个做。挑不到 5 个就别凑数。

每个候选 task 标了「典型痛点」，你能在 audit.md 里看到对应的 `[PROBLEM]` 描述才该做。

---

### 3b-1 Spinner 全套视觉（叶子 / shimmer / stalled 渐变）

#### 适用场景

烤机 audit.md 里出现：「spinner 不是叶子动画」/「verb 没有 shimmer 滚动」/「卡死时 spinner 颜色没变红」

#### 背景

TS Mossen 的 spinner 是品牌识别点之一：
- 6 帧叶子动画：🍃 → 🌿 → ☘️ → 🍀 → ☘️ → 🌿
- shimmer 效果：verb 文字（如 "Thinking…"）从左到右波浪渐亮
- stalled 渐变：3 秒无新 token 时，spinner 从绿色 RGB 插值到红色 `{171, 43, 63}`
- reduced-motion：单 🍃 2 秒呼吸

旧 Rust 端翻译树曾经有独立 spinner 动画实现，但那条 `components`
路径已经退役并删除。当前只能在 `crates/mossen-tui/src/widgets/spinner.rs`
这条活动 App 渲染路径里继续做。

#### 位置

- 实现：`crates/mossen-tui/src/widgets/spinner.rs`
- 当前活动 spinner：`crates/mossen-tui/src/widgets/spinner.rs`
- app.rs 渲染调用：搜 `spinner_widget` / `render_widget.*spinner`

#### 改动

##### Step 1：审计 spinner_anim.rs 有哪些 API

```bash
grep -n "^pub fn\|^pub struct\|^pub enum" crates/mossen-tui/src/components/spinner_anim.rs | head -20
```

##### Step 2：在 app.rs 替换简单 spinner

在 `widgets/spinner.rs` 里直接升级 `SpinnerWidget`/teammate spinner 的活动渲染；
不要再引入退役翻译树里的动画行。

需要时间源：每帧 redraw 时拿 `now = std::time::Instant::now()`，传给 spinner_anim widget，让它根据时间算当前在哪一帧。

##### Step 3：stalled 渐变

需要把"上次收到 token 的时间戳"传给 spinner widget。在 `state.rs::AppState` 加（如果还没有）：

```rust
pub last_token_received_at: Option<std::time::Instant>,
```

每收到 ContentDelta::TextDelta 时更新它。spinner 渲染时算 `now - last_token_received_at`，超过 3 秒就开始 RGB 插值。

#### 验证

```bash
cargo build --workspace 2>&1 | tail -3
```

视觉验证⚠️：**必须真 TTY**。Agent 在非 TTY 环境只能 build 过。**停下报告让用户看效果**。

#### 完成判定

- build 过
- 用户在 TTY 验证：6 帧叶子动画转起来、shimmer 效果可见、停 3 秒后变红

#### 回滚

```bash
git checkout -- crates/mossen-tui/src/app.rs crates/mossen-tui/src/state.rs
```

---

### 3b-2 Thinking block 视觉打磨 + Ctrl+E pin

#### 适用场景

audit.md 里出现：「thinking 没显示」/「thinking 没 shimmer」/「30 秒后没淡出」/「不能 pin 住 thinking block」

#### 背景

TS Mossen 的 thinking block 是核心可视化点：
- 灰色字体 + dim/italic
- shimmer 滚动（与 spinner 同款效果）
- streaming 完成后 30 秒淡出（用户没看就消失，避免噪音）
- Ctrl+E 可以 pin 住，不让消失

Rust 端 `crates/mossen-tui/src/widgets/message.rs:179-266` 已经有大部分实现。

#### 位置

- 实现：`crates/mossen-tui/src/widgets/message.rs::render_thinking` 或类似函数
- AppState：`crates/mossen-tui/src/state.rs`（看是否有 `pinned_thinking_ids: HashSet<MessageId>`）

#### 改动

##### Step 1：审计现状

```bash
grep -n "thinking\|Thinking" crates/mossen-tui/src/widgets/message.rs | head -20
grep -n "thinking_completed_at\|fade_out" crates/mossen-tui/src/widgets/message.rs | head -10
```

确认：30 秒 fade 逻辑在不在？Ctrl+E pin 在不在？

##### Step 2：缺什么补什么

- 如果缺 30 秒 fade：渲染时算 `now - thinking_completed_at`，> 30s 且未 pin 则跳过渲染
- 如果缺 Ctrl+E pin：在 app.rs handle_key 加 KeyCode::Char('e') with CTRL → 把当前 focused message 的 id push 到 `pinned_thinking_ids`

#### 验证

```bash
cargo build --workspace 2>&1 | tail -3
```

视觉验证由用户在 TTY 做。

#### 完成判定

- build 过
- 用户验证：thinking 灰字、有 shimmer、30 秒后淡出、Ctrl+E 能 pin 住

#### 回滚

```bash
git checkout -- crates/mossen-tui/src/widgets/message.rs crates/mossen-tui/src/state.rs
```

---

### 3b-3 工具特化渲染（Read / Grep / Glob / Web* / Task）

#### 适用场景

audit.md 里出现：「Read 工具的输出像 raw text，没有语法高亮 / 行号」/「Grep 结果一大坨」/「WebFetch 没显示 URL」

#### 背景

Rust 端 `widgets/message.rs::render_body` 只对 Bash / Edit / Write 做了特化。其他工具（Read / Grep / Glob / WebFetch / WebSearch / Task / TodoWrite）都走通用 `Paragraph` 渲染，没视觉特征。

TS 端每个工具都有专门的 `Assistant<Name>ToolUseMessage` + `User<Name>ToolResultMessage`，各自的视觉提示不同。

#### 位置

- 入口：`crates/mossen-tui/src/widgets/message.rs::render_body`（搜 `match tool_name`）

#### 改动

每个工具按需补一个 case。建议优先级（按使用频率）：

1. **Read** — 显示文件路径 + 行数 + 语法高亮（用 syntect）
2. **Grep** — 显示 pattern + 命中数 + 高亮命中行
3. **Glob** — 显示 pattern + 命中数 + 树状显示
4. **WebFetch / WebSearch** — 显示 URL + 摘要
5. **Task** — 显示 subagent_type + description（详细输出在 3b-6 子 agent 嵌套渲染里处理）

每个 case 大致 30-50 行 Rust，参考 Bash / Edit 的现有写法。

#### 验证

```bash
cargo build --workspace 2>&1 | tail -3
```

视觉验证由用户在 TTY 做。

#### 完成判定

- build 过
- 用户验证：每个工具有可识别的视觉特征

#### 回滚

```bash
git checkout -- crates/mossen-tui/src/widgets/message.rs
```

---

### 3b-4 Diff widget 视觉打磨

#### 适用场景

audit.md 里出现：「Edit 的 diff 没颜色」/「diff 没行号」/「diff 上下文行太少」/「大文件 diff 全展开太长」

#### 背景

Edit 和 Write 工具改文件时应该显示 diff。Rust 端可能已有简单实现，但视觉细节缺：
- 删除行红色 `-`、新增行绿色 `+`
- 行号列
- 默认显示 3 行上下文（user 可展开）
- 超过 N 行折叠

#### 位置

- 搜 `diff` / `unified_diff` in `widgets/message.rs` 或专门的 diff widget

#### 改动

##### Step 1：审计当前 diff 渲染

```bash
grep -rn "diff\|Diff" crates/mossen-tui/src/widgets/ | head -15
```

##### Step 2：补缺

- 用 `similar` crate（Cargo.toml 可能已有）做 diff
- 渲染时用 ratatui Span 颜色 +/- 行
- 行号列固定宽度
- 长 diff 折叠（默认显示前 20 行，按 Enter 展开）

#### 验证

```bash
cargo build --workspace 2>&1 | tail -3
```

视觉验证由用户在 TTY 做。

#### 完成判定

- build 过
- 用户验证：diff 着色 + 行号 + 折叠正常

#### 回滚

```bash
git checkout -- crates/mossen-tui/src/widgets/
```

---

### 3b-5 Prompt input typeahead + @-file 补全

#### 适用场景

audit.md 里出现：「输 `/` 时没自动提示斜杠命令」/「输 `@` 不能补全文件路径」/「多行编辑卡顿」

#### 背景

TS Mossen 的 prompt input 是体验亮点：
- 输 `/` 立刻浮动列表显示所有 slash command
- 输 `@` 后开始 fuzzy match 项目文件
- Ctrl+R 历史搜索
- Tab 选中候选

Rust 端 `crates/mossen-tui/src/hooks/typeahead.rs` 和 `unified_suggestions.rs` 已有 hook，但前端 widget 暴露的渲染挂载点不完整。

#### 位置

- Hooks：`crates/mossen-tui/src/hooks/typeahead.rs` + `unified_suggestions.rs`
- Widget：`crates/mossen-tui/src/widgets/prompt_input.rs`

#### 改动

##### Step 1：审计现状

```bash
grep -n "Suggestion\|typeahead" crates/mossen-tui/src/widgets/prompt_input.rs | head -10
```

##### Step 2：补 widget 渲染

在 prompt input 上方（输入框上方一行）渲染建议列表，最多 5 行，高亮当前选中。

绑 Tab 键切换选中、Enter 确认、Esc 取消。

#### 验证

```bash
cargo build --workspace 2>&1 | tail -3
```

视觉验证由用户在 TTY 做。

#### 完成判定

- build 过
- 用户验证：`/` 触发建议、`@` 触发文件补全、Tab/Enter/Esc 正常

#### 回滚

```bash
git checkout -- crates/mossen-tui/src/widgets/prompt_input.rs
```

---

### 3b-6 子 agent 嵌套真接（不是 breadcrumb）

#### 适用场景

audit.md 里出现：「Task tool 跑的时候只看到 `[teammate XXX event]` 占位」/「子 agent 内的工具调用没显示」/「子 agent 完成时没汇总」

#### 背景

Phase 3-2 做了一半 —— SdkMessage 加了 task_id，TUI 端能识别 sub-agent 消息。但 `handle_engine_message` 里**只 push `[teammate XXX event]` 占位文字**，没把子 agent 的实际内容（assistant text / tool_use / tool_result）展开。

TS Mossen 的做法：子 agent 的输出**嵌套显示在父 agent 当前位置的下方**，缩进 2-4 个空格，左侧画一根连续的 `│` 线表示从属关系。

#### 位置

- `crates/mossen-tui/src/app.rs::handle_engine_message`（看 `if let Some(tid) = msg.task_id()` 分支）

#### 改动

##### Step 1：把 task_id 路由的消息真的解析成 MessageData

替换当前的 `[teammate XXX event]` 占位逻辑：

```rust
match &msg {
    SdkMessage::Assistant { message, .. } => {
        // 把 message.content 转成 MessageData，标记 task_id 让渲染层缩进
        let mut data = MessageData::from_assistant(message);
        data.sub_agent_id = Some(tid.to_string());
        self.state.teammate_messages.entry(tid.to_string()).or_default().push(data);
    }
    SdkMessage::StreamEvent { event, .. } => {
        // 累积流式 token 到 teammate 的 streaming buffer
        ...
    }
    SdkMessage::ToolUseSummary { tool_name, summary, .. } => {
        // 同上，作为嵌套 tool use 显示
        ...
    }
    SdkMessage::Result { .. } => {
        // 完成 → 标记 TeammateState::Completed + 在父消息流加一行汇总
        ...
    }
    _ => {}
}
```

##### Step 2：渲染时嵌套显示

在 `app.rs::render` 主消息流绘制时，遇到一个父消息（如 Task tool_use）后，立刻把它对应 task_id 的 teammate_messages 渲染在下方，左侧加 `│` 前缀。

需要在 widgets/message.rs 加一个 `nesting_level` 字段（0 = 主，1 = 子，2 = 孙），渲染时左侧加对应数量的 `│ `。

#### 验证

```bash
cargo build --workspace 2>&1 | tail -3
```

视觉验证由用户在 TTY 做。

#### 完成判定

- build 过
- 用户验证：用 Task tool 派一个子 agent，能看到它的工具调用嵌套显示在主屏

#### 回滚

```bash
git checkout -- crates/mossen-tui/src/app.rs crates/mossen-tui/src/widgets/message.rs
```

---

### 3b-7 滚动行为 + Sticky header

#### 适用场景

audit.md 里出现：「长对话滚到底回不去」/「滚一会儿就乱了」/「prompt input 滚出视窗时找不到回 prompt 的入口」

#### 背景

TS Mossen 长会话的滚动体验：
- 默认自动跟随最新消息
- 用户手动往上滚 → 断开自动跟随，停留在用户位置
- PageDown 或滚到底部 → 重新启用自动跟随
- prompt input 滚出视窗时，顶部出现一个 sticky header 显示"输入区在 ↓"

Rust 端 `crates/mossen-tui/src/layout.rs::VirtualScroll` 有基础滚动，但 sticky header 和自动跟随逻辑可能没接齐。

#### 位置

- `crates/mossen-tui/src/layout.rs::VirtualScroll`
- `crates/mossen-tui/src/app.rs::handle_key`（滚动键路由）

#### 改动

##### Step 1：审计自动跟随逻辑

```bash
grep -n "auto_follow\|follow_tail\|scroll_to_bottom" crates/mossen-tui/src/layout.rs crates/mossen-tui/src/app.rs | head -10
```

##### Step 2：补缺

- 加 `auto_follow: bool` 字段，默认 true
- 用户按 Up / PageUp / 鼠标向上滚 → `auto_follow = false`
- 用户按 End / PageDown / 滚到底 → `auto_follow = true`
- 渲染时如果 `auto_follow == true` 且消息流变长，自动滚到底
- Sticky header：如果 prompt input 的 y 坐标 < layout.bottom.y（被滚出视窗），在顶部画一个一行 banner："Press End to return to input ↓"

#### 验证

```bash
cargo build --workspace 2>&1 | tail -3
```

视觉验证由用户在 TTY 做。

#### 完成判定

- build 过
- 用户验证：长会话滚动顺畅、End 键能回输入区、自动跟随机制工作

#### 回滚

```bash
git checkout -- crates/mossen-tui/src/layout.rs crates/mossen-tui/src/app.rs
```

---

## 6. 阶段验收（B + 选定的 A 全部跑完后）

### 6.1 重跑 30 分钟烤机

用户在 TTY 里重新跑 3b-0 的 8 项操作，对照 `/tmp/mossen_experience_audit.md` 的痛点清单：

- 选定的 task 涉及的痛点 → 至少 80% 改善（用户主观判断 OK 即可）
- 未选定的 task 涉及的痛点 → 维持原状（不要求改善，但不应恶化）

### 6.2 build + test 不退化

```bash
cd /Users/allen/Documents/rustmossen
cargo build --workspace 2>&1 | tail -3
cargo test -p mossen-tui 2>&1 | tail -5
cargo test --workspace --no-fail-fast 2>&1 | grep "test result:" | tail -10
```

要求：
- workspace build 0 error
- mossen-tui 测试不**新增**失败
- workspace 整体测试不**新增**失败（原有 20 个失败可以暂时保留，会在 Phase 4 修）

### 6.3 报告

向用户报告：

> Phase 3.5 完成。
> - 烤机暴露 N 个痛点（见 /tmp/mossen_experience_audit.md）
> - 本轮做了 M 个 task（3b-X, 3b-Y, ...）
> - 用户复跑烤机后体感改善：（用户填）
>
> 可以进 Phase 4（生产验收）吗？

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
| 编译某 crate | `cargo build -p mossen-tui` |
| 全 workspace | `cargo build --workspace` |
| Release 构建（烤机用） | `cargo build --workspace --release` |
| 跑 TUI 测试 | `cargo test -p mossen-tui` |
| trace 日志 | `RUST_LOG=mossen_tui=trace ./target/release/mossen` |

### C. 本阶段关键文件 quick ref

| 改/读什么 | 去哪 |
|------|------|
| TUI 主循环 | `crates/mossen-tui/src/app.rs` |
| TUI 状态 | `crates/mossen-tui/src/state.rs` |
| 单一 message widget | `crates/mossen-tui/src/widgets/message.rs` |
| 消息列表 | `crates/mossen-tui/src/widgets/messages.rs` |
| 流式 markdown | `crates/mossen-tui/src/widgets/markdown.rs` |
| 简单 spinner（要换掉） | `crates/mossen-tui/src/widgets/spinner.rs` |
| 完整 spinner 动画 | `crates/mossen-tui/src/components/spinner_anim.rs` |
| Prompt input | `crates/mossen-tui/src/widgets/prompt_input.rs` |
| 滚动 | `crates/mossen-tui/src/layout.rs` |
| Typeahead hook | `crates/mossen-tui/src/hooks/typeahead.rs` |
| Suggestions hook | `crates/mossen-tui/src/hooks/unified_suggestions.rs` |
| TaskListV2 数据壳 | `crates/mossen-tui/src/components/tasks.rs` |
| Teammate spinner 树 | `crates/mossen-tui/src/components/spinner_anim.rs` |

### D. 一个核心心法

**ratatui 是 immediate-mode**：每帧从纯 state 重画整屏。这意味着：

- 你**不能**用 React 的"组件持久化 state + useEffect"思路
- 你**应该**把所有动态状态（spinner 当前帧、thinking 是否 fade、auto_follow 状态）**集中在 `AppState`**
- 每帧 render 是一个**纯函数** `fn render(state: &AppState, frame: &mut Frame)`
- 时间相关的动画用 `std::time::Instant::now()` 在 render 时计算，**不要**用 timer 维护

这是 TS 范式到 Rust 范式的根本性切换。如果你发现自己想加一个"组件内部状态"，停下来想想是不是应该提升到 AppState。

---

文档版本：v1.0（2026-05-20）
**完整 5 阶段计划见 `PLAN_PRODUCTION.md`**
**本阶段是 Phase 3 和 Phase 4 之间的补充，覆盖体验质感（experience gap）**
