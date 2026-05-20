# Phase 0：正确性硬伤

> **目标读者**：本地 DeepSeek V4 Flash（通过 ds4-server 运行 Mossen 自身或其他 agent 来执行本文档）。
> **完整 5 阶段索引**：`/Users/allen/Documents/rustmossen/PLAN_PRODUCTION.md`
> **本阶段位置**：5 阶段中的第 1 阶段（Phase 0）

---

## 1. 项目背景（必须读完再动手）

### 1.1 Mossen 是什么

**Mossen** 是一个 Rust 写的 coding agent CLI（终端编程助手），对标 Claude Code。它跑在本机，通过 `ds4-server`（在 localhost:8000 监听）调用本地部署的 DeepSeek V4 Flash 模型。

代码在 `/Users/allen/Documents/rustmossen/`，按 Cargo workspace 组织成 10 个 crate：

```
crates/
├── mossen-agent/     ← agent 主循环（dialogue.rs）、API client、hooks、context compact
├── mossen-cli/       ← 入口 main.rs / 交互 REPL / system prompt 装配
├── mossen-commands/  ← slash 命令（/help、/clear 等）
├── mossen-mcp/       ← MCP（Model Context Protocol）服务
├── mossen-remote/    ← 远程 mossen 连接
├── mossen-skills/    ← /skill 命令的 skill 加载
├── mossen-tools/     ← Bash / Read / Edit / Write / Agent / TodoWrite 等工具
├── mossen-tui/       ← ratatui 渲染（TUI 主屏）
├── mossen-types/     ← 类型 + system prompt 文本常量
└── mossen-utils/     ← 通用 helper
```

### 1.2 项目当前状态（重要 —— 决定本阶段的工作性质）

**已经做到的**：
- 全 workspace 编译通过（0 error，仅 warning）
- 主 binary `target/debug/mossen` 能跑
- `mossen --oneshot "..."` 单轮请求端到端打通：连 ds4-server → SSE 流式接收 → tool_use 解析 → tool 执行 → 回灌模型 → 最终回复
- 实测能跑 Bash 工具调用一整圈

**还没接通的**（不属于本阶段范畴，留给后续阶段）：
- 5 类 hook 中只有 stop_hook 接到 dialogue.rs，其他 4 类没调度
- Context 三层 compact 各自实现了但相互协作未串通
- TUI 端 TodoWrite / sub-agent 输出渲染没接

### 1.3 本阶段（Phase 0）要解决什么

**Phase 0 = 正确性硬伤**。具体说是 5 个**单点 bug**，每个独立、改动小（每个 10-60 行代码）、互不依赖。这些 bug 会**误导模型自我认知**或**让用户配置失效**，但不阻塞 oneshot 跑通：

| Task | 一句话问题描述 |
|------|---------------|
| 0-1 | `repl.rs` 把默认模型名硬编码成 `"claude-opus-4-5"`，custom backend 时模型以为自己是 Anthropic Opus |
| 0-2 | TUI 默认模型 `"MiniMax-M2"`，同样误导 |
| 0-3 | 用户 `~/.claude/CLAUDE.md` 的 `@OTHER.md` import 语法不展开，用户写的项目指令被静默丢弃 |
| 0-4 | `fallback model` 触发时直接报错失败整个 turn，应当用当前 model 重试 |
| 0-5 | custom backend 路径没 info 日志，排查问题只能猜 base URL |

### 1.4 本阶段完成判定

跑完 5 个 task 后：

```bash
cd /Users/allen/Documents/rustmossen
cargo build --workspace 2>&1 | tail -3
cargo test -p mossen-cli 2>&1 | tail -3
```

应该看到：
- `cargo build` 输出 `Finished`，无 `error[`
- `cargo test -p mossen-cli` 全过（包括 0-3 新增的 `at_include_test`）

然后向用户报告："Phase 0 完成。可以进 Phase 1 吗？" 等用户确认。

### 1.5 本阶段不要做的事

- 不要修其他阶段的 task（hook 调度、TUI 渲染、测试修复都不是 Phase 0）
- 不要"顺手"清理 warning（226 个 camelCase warning 是别的 sprint 的事）
- 不要重命名/重构（即便看起来很想）

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
- 修改 `/Users/allen/Documents/ds4/` 任何东西（那是模型服务器，独立项目，不在本任务范围）

### 2.2 执行节奏

**一次只做一个 task**。每个 task 5 段固定结构：

```
[Task ID] 简短标题
背景：为什么要做
位置：要改 / 要审计的文件 + 行号
改动：具体做什么（含 before/after 代码块或审计命令）
验证：跑哪条命令证明成功
完成判定：命令必须输出什么才算 done
回滚：失败时怎么恢复
```

做完一个 task 必须跑「验证」，输出符合「完成判定」才能继续下一个。**不要批量、不要并行、不要跳验证**。

### 2.3 卡住时

如果某个 task 的「位置」/「改动」对不上实际源码（行号漂移、代码已改过、找不到符号），**立即停止本 task**，输出：
- 你试了哪些查找
- 实际看到什么
- 推测文档过时还是源码已改

然后等用户确认。**不要猜怎么改**。

### 2.4 命令前缀

所有 `Bash` 默认在 `/Users/allen/Documents/rustmossen` 下执行。本文档不会重复 `cd`。如果你的 shell 重置过 cwd，先 `cd /Users/allen/Documents/rustmossen`。

### 2.5 开工前基线确认

```bash
cd /Users/allen/Documents/rustmossen
cargo check --workspace 2>&1 | tail -3
```

**期望**：最后一行包含 `Finished`，无 `error[`。如果有任何 `error[`，停下报告。

### 2.6 与用户的沟通规则

每个 task 前后**简短**（≤ 3 行）：

- **开始**：`正在做 0-X：<标题>`
- **完成**：`0-X 完成。验证通过：<关键输出>`
- **失败 / 卡住**：`0-X 失败 / 卡住。详情：<错误片段>。已停下，等待指示。`

**不要长篇**。完成报告附上 `cargo build` / `cargo test` 最后几行作为证据。

---

## 3. 本阶段任务清单

| Task | 标题 | 改动量 |
|------|------|--------|
| 0-1 | 修正硬编码模型名 `"claude-opus-4-5"` | ~15 行 |
| 0-2 | 修正 TUI 默认模型 `"MiniMax-M2"` | 1 行 |
| 0-3 | 在 CLAUDE.md / MOSSEN.md 中支持 `@<file>` 递归 include | ~80 行 |
| 0-4 | 修正 fallback model 占位实现 | 5 行 |
| 0-5 | 在 custom backend 路径加可观测日志 | 10 行 |

按编号顺序做。0-2 复用 0-1 加的 helper 函数，必须在 0-1 之后。其他无强依赖但仍按顺序。

---

## 4. Task 详情

### 0-1 修正硬编码模型名 `"claude-opus-4-5"`

#### 背景

`crates/mossen-cli/src/repl.rs` 两处把默认模型名硬编码为 `"claude-opus-4-5"`。当用户走 custom backend（指向本地 ds4-server）时，system prompt 的 env_info section 会输出 `You are powered by the model claude-opus-4-5.` —— 这会误导模型对自己能力的判断（它可能自信地说"我是 Claude，我能做 X"）。

正确做法：当 `MOSSEN_CODE_USE_CUSTOM_BACKEND=1` 时，优先读 `MOSSEN_CODE_CUSTOM_MODEL` 环境变量，没设再 fallback 到一个 generic 字符串。

#### 位置

文件：`crates/mossen-cli/src/repl.rs`
两处硬编码：约第 275 行（oneshot 路径）和约第 463 行（exec/print 路径）。

确认行号未漂移：

```bash
grep -n 'claude-opus-4-5' crates/mossen-cli/src/repl.rs
```

**期望**：恰好 2 行命中，行号在 270-280 和 460-470 之间。0 或 > 2 行就停下报告。

#### 改动

##### Step 1：加 helper（放在文件首个 `pub fn` 之前）

```rust
/// 返回 oneshot / exec 路径的默认 model id。
///
/// 优先级：
/// 1. `MOSSEN_CODE_CUSTOM_MODEL` 环境变量（custom backend 场景下用户显式设置的模型名）
/// 2. `"custom-backend-model"`（generic 占位，避免误称 claude-opus-4-5）
fn default_model_for_unset_cli() -> String {
    std::env::var("MOSSEN_CODE_CUSTOM_MODEL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "custom-backend-model".to_string())
}
```

##### Step 2：替换两处硬编码

把两处 `.unwrap_or_else(|| "claude-opus-4-5".to_string())` 都改成 `.unwrap_or_else(default_model_for_unset_cli)`。

如果闭包形式编译不过，用 `|| default_model_for_unset_cli()`。

#### 验证

```bash
cd /Users/allen/Documents/rustmossen
cargo build -p mossen-cli 2>&1 | tail -5
```

#### 完成判定

输出包含 `Finished` 且不包含 `error[`。warning 可以忽略。

可选语义验证：

```bash
MOSSEN_CODE_USE_CUSTOM_BACKEND=1 \
MOSSEN_CODE_CUSTOM_MODEL=deepseek-v4-flash \
MOSSEN_CODE_CUSTOM_BASE_URL=http://localhost:8000 \
./target/debug/mossen --oneshot "What model are you?" 2>&1 | tail -10
```

回复中**不应**出现 `claude-opus-4-5` 字样。

#### 回滚

```bash
git checkout -- crates/mossen-cli/src/repl.rs
```

---

### 0-2 修正 TUI 默认模型 `"MiniMax-M2"`

#### 背景

`crates/mossen-cli/src/repl.rs:81` 把交互式 REPL（TUI 模式）的默认模型硬编码成 `"MiniMax-M2"`。这是早期开发期占位。同 0-1 一样，应该读 `MOSSEN_CODE_CUSTOM_MODEL` 环境变量。

#### 位置

文件：`crates/mossen-cli/src/repl.rs` 约第 81 行：

```rust
let model = s
    .model_override
    .clone()
    .or_else(|| config.model_override.clone())
    .unwrap_or_else(|| "MiniMax-M2".to_string());
```

确认行号：

```bash
grep -n '"MiniMax-M2"' crates/mossen-cli/src/repl.rs
```

**期望**：恰好 1 行命中。其他 `MiniMax-M2` 在 `system_prompt.rs` / `app.rs` / `custom_backend.rs` 都是**测试常量**，本任务**不要动**。

#### 改动

把 repl.rs 这一行的 `.unwrap_or_else(|| "MiniMax-M2".to_string())` 改成 `.unwrap_or_else(default_model_for_unset_cli)`（复用 0-1 加的 helper）。

#### 验证

```bash
cd /Users/allen/Documents/rustmossen
cargo build -p mossen-cli 2>&1 | tail -5
grep -n '"MiniMax-M2"' crates/mossen-cli/src/repl.rs
```

#### 完成判定

- `cargo build` 输出 `Finished`，无 error
- 第二条 grep **返回空**（repl.rs 里不再有 MiniMax-M2 字面量）

#### 回滚

```bash
git checkout -- crates/mossen-cli/src/repl.rs
```

---

### 0-3 在 CLAUDE.md / MOSSEN.md 中支持 `@<file>` 递归 include

#### 背景

用户的 `~/.claude/CLAUDE.md` 或项目 `MOSSEN.md` 里常常写一行 `@OTHER.md` 来 import 其他文件（这是 coding agent 圈的常见约定，Claude Code 原版就支持）。

但 Mossen 的 Rust 端 `gather_memory_text`（在 `crates/mossen-cli/src/system_prompt.rs:192-249`）当前只是**把整个文件原文塞进 system prompt**，不展开 @-include。

实际后果：当用户的 `~/.claude/CLAUDE.md` 只有一行 `@RTK.md` 时，传给模型的只是这 6 个字符（`@RTK.md`），真正的 RTK.md 内容（约 1 KB 的具体指令）完全丢失。模型收不到用户的偏好/工具说明。

#### 位置

文件：`crates/mossen-cli/src/system_prompt.rs`
函数：`pub async fn gather_memory_text(cwd: &std::path::Path) -> String`（约第 192 行起）

#### 改动

##### Step 1：在 `gather_memory_text` 之前加一个新函数

```rust
/// 递归展开 `@<file>` import。语义：
/// - 一行（前后允许空白）只包含 `@<path>` 就替换为该文件内容
/// - path 是相对路径时，相对 `parent`（被 include 源文件的目录）解析
/// - 防循环：`visited` 跟踪已展开的 canonical 路径，已访问的直接保留原行
/// - 最大递归深度 5；超出后保留原文（防爆栈 + 防恶意构造）
/// - 文件读不到也保留原文（保守，避免静默丢内容）
async fn expand_at_includes(
    raw: &str,
    parent: &std::path::Path,
    visited: &mut std::collections::HashSet<std::path::PathBuf>,
    depth: u8,
) -> String {
    if depth >= 5 {
        return raw.to_string();
    }
    let mut out = String::with_capacity(raw.len());
    for line in raw.lines() {
        let trimmed = line.trim();
        let include_target = trimmed.strip_prefix('@').and_then(|rest| {
            if !rest.is_empty() && !rest.contains(char::is_whitespace) {
                Some(rest)
            } else {
                None
            }
        });

        if let Some(target) = include_target {
            let candidate = if std::path::Path::new(target).is_absolute() {
                std::path::PathBuf::from(target)
            } else {
                parent.join(target)
            };
            let canon = std::fs::canonicalize(&candidate).unwrap_or_else(|_| candidate.clone());
            if visited.contains(&canon) {
                out.push_str(line);
                out.push('\n');
                continue;
            }
            visited.insert(canon.clone());

            match tokio::fs::read_to_string(&candidate).await {
                Ok(child_raw) => {
                    let child_parent = candidate
                        .parent()
                        .map(|p| p.to_path_buf())
                        .unwrap_or_else(|| parent.to_path_buf());
                    let expanded = Box::pin(expand_at_includes(
                        &child_raw,
                        &child_parent,
                        visited,
                        depth + 1,
                    ))
                    .await;
                    out.push_str(&expanded);
                    if !expanded.ends_with('\n') {
                        out.push('\n');
                    }
                }
                Err(_) => {
                    out.push_str(line);
                    out.push('\n');
                }
            }
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}
```

##### Step 2：在 `gather_memory_text` 的 3 处读文件后加调用

`gather_memory_text` 有 3 处 `tokio::fs::read_to_string(&path).await`：
1. 读 user-global（如 `~/.claude/CLAUDE.md`）
2. 读 project-root（`./MOSSEN.md` 等 4 个候选名）
3. 读 nested（`./.mossen/MOSSEN.md`）

每处把这段：

```rust
if let Ok(text) = tokio::fs::read_to_string(&path).await {
    let trimmed = text.trim();
    if !trimmed.is_empty() {
        sections.push(format!("Contents of {} (...):\n\n{}", path.display(), trimmed));
    }
}
```

改成：

```rust
if let Ok(text) = tokio::fs::read_to_string(&path).await {
    let parent = path.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| std::path::PathBuf::from("."));
    let mut visited = std::collections::HashSet::new();
    if let Ok(canon) = std::fs::canonicalize(&path) {
        visited.insert(canon);
    }
    let expanded = expand_at_includes(&text, &parent, &mut visited, 0).await;
    let trimmed = expanded.trim();
    if !trimmed.is_empty() {
        sections.push(format!("Contents of {} (...):\n\n{}", path.display(), trimmed));
    }
}
```

三处格式字符串里的 `(...)` 部分**按原文保留**，不要乱改。

#### 验证

```bash
cd /Users/allen/Documents/rustmossen
cargo build -p mossen-cli 2>&1 | tail -5
```

build 过之后，加一个集成测试验证 @-include 真的展开：

```bash
mkdir -p /tmp/mossen_include_test
echo '@included.md' > /tmp/mossen_include_test/MOSSEN.md
echo 'This is the included content.' > /tmp/mossen_include_test/included.md

cat > crates/mossen-cli/tests/at_include_test.rs <<'TEST'
use mossen_cli::system_prompt::gather_memory_text;
use std::path::Path;

#[tokio::test]
async fn at_include_expands_one_level() {
    let cwd = Path::new("/tmp/mossen_include_test");
    let text = gather_memory_text(cwd).await;
    assert!(text.contains("This is the included content."), "got:\n{}", text);
    assert!(!text.contains("@included.md"), "got:\n{}", text);
}
TEST

cargo test -p mossen-cli --test at_include_test 2>&1 | tail -10
```

#### 完成判定

- `cargo build -p mossen-cli` 输出 `Finished`，无 error
- 测试输出 `test result: ok. 1 passed`

#### 边界情况：bin crate 无 lib.rs

如果 `cargo test` 报「找不到 `mossen_cli::system_prompt::gather_memory_text` 符号」，说明 mossen-cli 只是个 bin crate，没 lib.rs，外部测试访问不到内部模块。**这种情况下停下报告，不要私自加 lib.rs**（涉及 crate 类型变更，影响面大）。

#### 回滚

```bash
git checkout -- crates/mossen-cli/src/system_prompt.rs
rm -f crates/mossen-cli/tests/at_include_test.rs
rm -rf /tmp/mossen_include_test
```

---

### 0-4 修正 fallback model 占位实现

#### 背景

`crates/mossen-agent/src/dialogue.rs` 第 283 行附近，当 stream 返回 `RetryError::FallbackTriggered`（API 提示用 fallback model 重试）时，当前实现直接 `warn!` + 返回 `TerminalReason::ModelError`，把整个 turn 失败掉。

对于 single-model 场景（本地 ds4-server 只跑一个模型），fallback 本来就不该被触发；但代码里依然存在触发路径（context 超限、token 限制等）。**期望行为**：FallbackTriggered 在没配置 fallback handler 时，应当被当作"用当前模型重试"，不是失败。

#### 位置

文件：`crates/mossen-agent/src/dialogue.rs`，约第 283-296 行：

```rust
Err(RetryError::FallbackTriggered { original, fallback }) => {
    warn!(%original, %fallback, "Fallback model would be used (not yet wired)");
    return Ok((
        TerminalReason::ModelError {
            error: anyhow::anyhow!(
                "Fallback triggered from {} to {} but no fallback handler installed",
                original,
                fallback
            ),
        },
        cost_state,
    ));
}
```

确认行号：

```bash
grep -n "FallbackTriggered" crates/mossen-agent/src/dialogue.rs
```

#### 改动

把整个匹配臂改成：

```rust
Err(RetryError::FallbackTriggered { original, fallback }) => {
    // 当前实现没有真正的 fallback handler。我们的策略是：
    //   - 记日志告诉用户"想 fallback 但没装"
    //   - 不把这个 turn 失败掉，而是返回 TerminalReason::Retry，
    //     让上层循环用当前 model 再试一次
    // single-model 场景（本机 ds4 单模型）下，这条路径几乎不会被走到；
    // 真要走到，重试是最不伤体验的选择。
    warn!(
        %original, %fallback,
        "Fallback requested but no handler wired; will retry with the original model"
    );
    return Ok((TerminalReason::Retry, cost_state));
}
```

#### 前置确认

```bash
grep -nE "enum TerminalReason|^    Retry" crates/mossen-agent/src/dialogue.rs | head -5
```

如果 `TerminalReason` 没有 `Retry` 变体，**停下报告**。本任务不要私自动 enum（涉及 enum 变更，调用方都得改）。

#### 验证

```bash
cd /Users/allen/Documents/rustmossen
cargo build -p mossen-agent 2>&1 | tail -5
```

#### 完成判定

输出 `Finished`，无 error。

#### 回滚

```bash
git checkout -- crates/mossen-agent/src/dialogue.rs
```

---

### 0-5 在自定义后端路径加可观测日志

#### 背景

`crates/mossen-agent/src/api_client.rs:148-150` 在 `is_custom_backend_enabled()` 为 true 时把请求 route 到 OpenAI-compat 路径（`/v1/chat/completions`），但当前**没有 info! 日志**告诉用户实际打的是哪个 base URL —— 排查问题时只能盲猜。

加上这条日志后，`RUST_LOG=mossen_agent=info` 就能看到每次请求实际指向哪。

#### 位置

文件：`crates/mossen-agent/src/api_client.rs`，找到：

```rust
if mossen_utils::custom_backend::is_custom_backend_enabled() {
    return call_streaming_openai_compat(params, cancel).await;
}
```

确认行号：

```bash
grep -n "call_streaming_openai_compat" crates/mossen-agent/src/api_client.rs | head -3
```

#### 改动

在 `return` 之前加一条 info 日志：

```rust
if mossen_utils::custom_backend::is_custom_backend_enabled() {
    let base_url = std::env::var("MOSSEN_CODE_CUSTOM_BASE_URL")
        .unwrap_or_else(|_| "<unset>".to_string());
    tracing::info!(
        target: "mossen_agent::api_client",
        base_url = %base_url,
        model = %params.model,
        "custom backend routing: OpenAI-compat /chat/completions"
    );
    return call_streaming_openai_compat(params, cancel).await;
}
```

如果文件顶部已 `use tracing::{debug, info, ...}`，直接 `info!(...)` 就行。先 `head -20 crates/mossen-agent/src/api_client.rs` 看 import 风格选一种保持一致。

#### 验证

```bash
cd /Users/allen/Documents/rustmossen
cargo build -p mossen-agent 2>&1 | tail -5
```

#### 完成判定

输出 `Finished`，无 error。

可选语义验证：

```bash
RUST_LOG=mossen_agent=info \
MOSSEN_CODE_USE_CUSTOM_BACKEND=1 \
MOSSEN_CODE_CUSTOM_BASE_URL=http://localhost:8000 \
MOSSEN_CODE_CUSTOM_MODEL=deepseek-v4-flash \
./target/debug/mossen --oneshot "hi" 2>&1 | grep "custom backend routing"
```

应看到一条 `custom backend routing: ...` 日志。

#### 回滚

```bash
git checkout -- crates/mossen-agent/src/api_client.rs
```

---

## 5. Phase 0 阶段验收

5 个 task 都做完后跑：

```bash
cd /Users/allen/Documents/rustmossen
cargo build --workspace 2>&1 | tail -3
cargo test -p mossen-cli 2>&1 | tail -3
```

**Phase 0 完成判定**：

- workspace 编译 0 error
- mossen-cli 测试全过（包括 0-3 新加的 at_include_test）

通过后向用户报告：

> Phase 0 完成。验证：
> - cargo build --workspace: Finished
> - cargo test -p mossen-cli: test result: ok. ...
>
> 可以进 Phase 1（Harness 融合）吗？

等用户确认。

---

## 附录

### A. 失败回滚通用步骤

任意 task 失败后：

```bash
cd /Users/allen/Documents/rustmossen
git status                          # 看改了啥
git diff <文件>                     # 看具体改动
git checkout -- <文件>              # 仅丢弃单个文件改动
```

⚠️ **永远不要** `git reset --hard`。

### B. 常用命令速查

| 任务 | 命令 |
|------|------|
| 增量编译某 crate | `cargo build -p mossen-cli` |
| 编译全部 | `cargo build --workspace` |
| 跑某 crate 全部测试 | `cargo test -p mossen-utils` |
| 跑单个测试 | `cargo test -p mossen-cli at_include_test -- --nocapture` |
| ds4-server 健康检查 | `curl -sf http://localhost:8000/v1/models \| head -5` |

### C. 本阶段关键文件 quick ref

| 改什么 | 去哪 |
|------|------|
| 默认 model 字符串（0-1、0-2） | `crates/mossen-cli/src/repl.rs` |
| @-include 展开（0-3） | `crates/mossen-cli/src/system_prompt.rs` |
| fallback model 重试（0-4） | `crates/mossen-agent/src/dialogue.rs` |
| custom backend 日志（0-5） | `crates/mossen-agent/src/api_client.rs` |

---

文档版本：v2.1（拆分版）
**完整 5 阶段计划见 `PLAN_PRODUCTION.md`**
