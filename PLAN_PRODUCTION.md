# Mossen Rust 生产化实施计划

> **目标读者**：本地 DeepSeek V4 Flash（通过 ds4-server / Mossen 自身执行）
> **目标**：把 `/Users/allen/Documents/rustmossen` 的 Rust 重写版 Mossen 从 "能编译跑通 oneshot" 推进到 **生产可用**
> **预计工期**：约 10-15 个交互轮次（一人一项一项做）
> **生效环境**：macOS / Apple Silicon / ds4-server 已在 localhost:8000 监听

---

## 0. 阅读约定（重要，必须先读）

### 0.1 角色与权限

你是一名 Rust 工程师。你**可以**：
- 用 `Read` 读 `/Users/allen/Documents/rustmossen/**` 下任何文件
- 用 `Edit` 修改 Rust 源码、Cargo.toml、Markdown 文档
- 用 `Write` 创建新文件
- 用 `Bash` 跑 `cargo`、`grep`、`ls`、`git diff` 等工具

你**绝对不能**：
- 运行 `git push`、`git reset --hard`、`rm -rf` 等不可逆操作
- 删除任何未在本文档中明示要删的文件
- 修改 `/Users/allen/Documents/ds4/` 下任何东西（那是模型服务器，不在本任务范围）
- 退出 `crates/` 目录工作 —— 本项目只剩 Rust，没有其他语言源码

### 0.2 执行节奏

**一次只做一个 task**。每个 task 的结构：

```
[Task ID] 简短标题
背景：为什么要做
位置：要改的文件路径 + 行号
改动：具体改什么（含 before/after 代码块）
验证：跑哪条命令证明成功
完成判定：命令必须输出什么才算 done
回滚：失败时怎么恢复
```

做完一个 task 必须跑"验证"命令，输出符合"完成判定"才能继续下一个。**不要批量做、不要并行做、不要跳过验证**。

### 0.3 卡住时

如果某个 task 的"改动"指令在源码中**找不到匹配的位置**（行号变化、代码已被改过、等），**立即停止本 task**，输出一段说明：
- 你试了哪些查找
- 看到的实际代码是什么
- 你觉得文档过时还是源码已被人改过

然后等用户确认。**不要自己猜怎么改**。

### 0.4 命令前缀

所有 `Bash` 命令默认在 `/Users/allen/Documents/rustmossen` 下执行。本文档不会重复 `cd`，但如果你的 shell 重置过 cwd，请先 `cd /Users/allen/Documents/rustmossen`。

### 0.5 编译/测试基线（开工前必跑）

在开始 M1-1 之前先跑一次基线确认环境健康：

```bash
cd /Users/allen/Documents/rustmossen
cargo check --workspace 2>&1 | tail -3
```

**期望**：最后一行包含 `Finished` 字样，没有 `error[`。如果有任何 `error[`，停下来报告。

---

## 阶段总览

| 阶段 | 任务数 | 工期 | 目标 |
|------|------|------|------|
| **M1** 正确性硬伤 | 5 | 1-2 天 | 模型身份正确 / 用户指令被读到 / 默认值合理 |
| **M2** 测试修复 + 实地烤机 | 6 | 3-5 天 | 全部 1057 测试通过，TTY 真机跑过 30 min |
| **M3** 生产化打磨 | 4 | 3-5 天 | 默认 profile 接 ds4，错误信息可读，doctor 增强 |

---

# 🅼 M1 阶段：正确性硬伤

## M1-1 修正硬编码的模型名 `"claude-opus-4-5"`

### 背景

`crates/mossen-cli/src/repl.rs` 在两处把默认模型名硬编码为 `"claude-opus-4-5"`。当用户走 custom backend（指向本地 ds4-server）时，系统 prompt 的 env_info 段会出现 `You are powered by the model claude-opus-4-5.` —— 这会误导模型对自己能力的判断（它会以为自己是 Anthropic 的 Opus）。

正确做法：当 `MOSSEN_CODE_USE_CUSTOM_BACKEND=1` 时，优先读 `MOSSEN_CODE_CUSTOM_MODEL` 环境变量，没设再 fallback 到一个 generic 字符串（`"custom-backend-model"`）。

### 位置

文件：`crates/mossen-cli/src/repl.rs`
两处硬编码：
- 第 275 行附近（oneshot 路径的默认值）
- 第 463 行附近（exec/print 路径的默认值）

先用以下命令确认行号未漂移：

```bash
grep -n 'claude-opus-4-5' crates/mossen-cli/src/repl.rs
```

**期望输出**：恰好 2 行命中，行号大致在 270-280 和 460-470 之间。如果出现 0 行或 > 2 行，停下来报告。

### 改动

新建一个 helper 函数放在文件顶部 use 块下面（或合适位置），然后两处调用它。

#### Step 1：添加 helper

在 `crates/mossen-cli/src/repl.rs` 中找到第一个 `pub fn` 或 `pub async fn` 定义之前的位置，插入：

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

#### Step 2：替换两处硬编码

把两处 `.unwrap_or_else(|| "claude-opus-4-5".to_string())` 都改成 `.unwrap_or_else(default_model_for_unset_cli)`。

注意：闭包 `|| default_model_for_unset_cli()` 也可以，但直接传函数引用更简洁。如果函数签名要求闭包，用 `|| default_model_for_unset_cli()`。

### 验证

```bash
cd /Users/allen/Documents/rustmossen
cargo build -p mossen-cli 2>&1 | tail -5
```

**完成判定**：输出包含 `Finished` 且不包含 `error[`。warning 可以忽略。

进一步语义验证（可选但建议）：

```bash
MOSSEN_CODE_USE_CUSTOM_BACKEND=1 \
MOSSEN_CODE_CUSTOM_MODEL=deepseek-v4-flash \
MOSSEN_CODE_CUSTOM_BASE_URL=http://localhost:8000 \
./target/debug/mossen --oneshot "What model are you?" 2>&1 | tail -10
```

回复中**不应**出现 `claude-opus-4-5` 字样。

### 回滚

```bash
cd /Users/allen/Documents/rustmossen
git diff crates/mossen-cli/src/repl.rs   # 看下改了啥
git checkout -- crates/mossen-cli/src/repl.rs  # 仅在确认要回滚时
```

---

## M1-2 修正 TUI 默认模型 `"MiniMax-M2"`

### 背景

`crates/mossen-cli/src/repl.rs:81` 把交互式 REPL（TUI）的默认模型硬编码成 `"MiniMax-M2"`。这是早期开发期占位，已经不再合适。同 M1-1 一样，应该读 `MOSSEN_CODE_CUSTOM_MODEL` 环境变量。

### 位置

文件：`crates/mossen-cli/src/repl.rs`
约第 81 行：

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

**期望输出**：恰好 1 行命中（其他 MiniMax-M2 出现在 system_prompt.rs / app.rs / custom_backend.rs，那些是**测试用**的常量，本任务**不要动**）。

### 改动

把这一行的 `.unwrap_or_else(|| "MiniMax-M2".to_string())` 改成 `.unwrap_or_else(default_model_for_unset_cli)`（复用 M1-1 加的 helper）。

### 验证

```bash
cd /Users/allen/Documents/rustmossen
cargo build -p mossen-cli 2>&1 | tail -5
grep -n '"MiniMax-M2"' crates/mossen-cli/src/repl.rs
```

**完成判定**：
- `cargo build` 输出 `Finished`，无 error
- 第二条 grep **返回空**（repl.rs 里不再有 MiniMax-M2 字面量）

### 回滚

`git checkout -- crates/mossen-cli/src/repl.rs`

---

## M1-3 在 CLAUDE.md / MOSSEN.md 中支持 `@<file>` 递归 include

### 背景

用户可能在 `~/.claude/CLAUDE.md` 或 `MOSSEN.md` 里写一行 `@OTHER.md` 来 import 其他文件（这是常见 agent 配置约定）。Mossen 的 `gather_memory_text`（`crates/mossen-cli/src/system_prompt.rs:192-249`）目前只是**把整个文件当字面量塞进 prompt**，不展开 @-include。

这意味着当用户的 `~/.claude/CLAUDE.md` 只有一行 `@RTK.md` 时，传给模型的只是这 6 个字符，真正的 RTK.md 内容（约 1 KB）丢失。

### 位置

文件：`crates/mossen-cli/src/system_prompt.rs`
函数：`pub async fn gather_memory_text(cwd: &std::path::Path) -> String`（第 192 行起）

### 改动

新增一个递归展开函数 `expand_at_includes`，在 `gather_memory_text` 内部，每读到一个文件就先展开。

#### Step 1：在 `system_prompt.rs` 加 expand 函数（放在 `gather_memory_text` 之前）

```rust
/// 递归展开 `@<file>` import。语义约定如下：
/// - 一行（前后允许空白）只包含 `@<path>` 就替换为该文件内容
/// - path 是相对路径时，相对 `parent`（被 include 的源文件目录）解析
/// - 防止循环：用 `visited` 跟踪已展开的 canonical 路径，已访问的直接保留原行
/// - 最大递归深度 5；超出后保留原文（防爆栈 + 防止恶意构造）
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
            // 必须不含空格；允许相对/绝对路径；至少 1 字符
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
            // canonicalize 用于循环检测；失败就用原 candidate
            let canon = std::fs::canonicalize(&candidate).unwrap_or_else(|_| candidate.clone());
            if visited.contains(&canon) {
                // 已展开过，保留原行作为提示
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
                    // 递归展开 child
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
                    // 读不到就保留原文，避免静默丢失
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

#### Step 2：在 `gather_memory_text` 中调用展开

找到 `gather_memory_text` 中每处 `tokio::fs::read_to_string(&path).await` 后立即调用 `expand_at_includes`。这个函数中有 3 处读文件位置（user-global、project-root、nested）。每处改成：

**Before**:
```rust
if let Ok(text) = tokio::fs::read_to_string(&path).await {
    let trimmed = text.trim();
    if !trimmed.is_empty() {
        sections.push(format!(
            "Contents of {} (...):\n\n{}",
            path.display(),
            trimmed
        ));
    }
}
```

**After**:
```rust
if let Ok(text) = tokio::fs::read_to_string(&path).await {
    let parent = path.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| std::path::PathBuf::from("."));
    let mut visited = std::collections::HashSet::new();
    // 把当前文件标为 visited 防自引用
    if let Ok(canon) = std::fs::canonicalize(&path) {
        visited.insert(canon);
    }
    let expanded = expand_at_includes(&text, &parent, &mut visited, 0).await;
    let trimmed = expanded.trim();
    if !trimmed.is_empty() {
        sections.push(format!(
            "Contents of {} (...):\n\n{}",
            path.display(),
            trimmed
        ));
    }
}
```

三处都按同样模板替换（注意每处的格式字符串中 `(...)` 部分要保留原文，不要乱改）。

### 验证

```bash
cd /Users/allen/Documents/rustmossen
cargo build -p mossen-cli 2>&1 | tail -5
```

**完成判定 1**：`Finished` 输出，无 error。

```bash
# 准备一个测试 fixture
mkdir -p /tmp/mossen_include_test
echo '@included.md' > /tmp/mossen_include_test/MOSSEN.md
echo 'This is the included content.' > /tmp/mossen_include_test/included.md

# 用一个简易 Rust 单测来验证：手写到 tests/include_test.rs
cat > crates/mossen-cli/tests/at_include_test.rs <<'TEST'
//! Integration test: verify @-include expansion in gather_memory_text.

use mossen_cli::system_prompt::gather_memory_text;
use std::path::Path;

#[tokio::test]
async fn at_include_expands_one_level() {
    let cwd = Path::new("/tmp/mossen_include_test");
    let text = gather_memory_text(cwd).await;
    assert!(
        text.contains("This is the included content."),
        "@-include should have been expanded, got:\n{}",
        text
    );
    assert!(
        !text.contains("@included.md"),
        "Raw @-include marker should have been replaced, got:\n{}",
        text
    );
}
TEST
cargo test -p mossen-cli --test at_include_test 2>&1 | tail -10
```

**完成判定 2**：测试输出 `test result: ok. 1 passed`。

注意 `mossen_cli::system_prompt::gather_memory_text` 需要在 lib root 可访问。如果 build 提示找不到符号，先检查 `crates/mossen-cli/src/lib.rs` 是否 `pub mod system_prompt;`。如果没有 lib.rs（这是个 bin crate），需要先告诉用户 —— 集成测试需要 lib 暴露。**这种情况下停下来报告，不要自己加 lib.rs**。

### 回滚

```bash
git checkout -- crates/mossen-cli/src/system_prompt.rs
rm -f crates/mossen-cli/tests/at_include_test.rs
rm -rf /tmp/mossen_include_test
```

---

## M1-4 修正 fallback model 的占位实现

### 背景

`crates/mossen-agent/src/dialogue.rs` 第 283 行附近，当 stream 返回 `RetryError::FallbackTriggered` 时，目前的实现是 `warn!` + 立刻 `return Ok(TerminalReason::ModelError)` —— 等于直接把这个 turn 失败掉。

对于 single-model 场景（本地 ds4-server 只有一个模型），fallback 本来就不该被触发；但代码里依然存在触发路径（context 超限、token 限制等）。**期望行为**：FallbackTriggered 在没配置 fallback handler 时，应当被当作"用当前模型重试"。

### 位置

文件：`crates/mossen-agent/src/dialogue.rs`
约第 283-296 行：

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

### 改动

把上述整个匹配臂改成：

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

⚠️ **前置确认**：`TerminalReason` 枚举里**必须**有 `Retry` 变体。先跑：

```bash
grep -n "enum TerminalReason\|^    Retry" crates/mossen-agent/src/dialogue.rs | head -5
```

如果没有 `Retry` 变体，**停下来报告**。这种情况下需要先在 enum 加 `Retry,`，再在调用方处理。本任务不要私自动 enum。

### 验证

```bash
cd /Users/allen/Documents/rustmossen
cargo build -p mossen-agent 2>&1 | tail -5
```

**完成判定**：`Finished`，无 error。

### 回滚

`git checkout -- crates/mossen-agent/src/dialogue.rs`

---

## M1-5 在自定义后端路径加可观测日志

### 背景

`crates/mossen-agent/src/api_client.rs:148-150` 在 `is_custom_backend_enabled()` 为 true 时把请求 route 到 OpenAI-compat 路径。但目前**没有 info! 日志**告诉用户实际打的是哪个 base URL —— 排查问题时只能猜。

### 位置

文件：`crates/mossen-agent/src/api_client.rs`
找到这段：

```rust
if mossen_utils::custom_backend::is_custom_backend_enabled() {
    return call_streaming_openai_compat(params, cancel).await;
}
```

确认行号：

```bash
grep -n "call_streaming_openai_compat" crates/mossen-agent/src/api_client.rs | head -3
```

### 改动

在 `return` 之前加一行 info！日志：

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

⚠️ 文件顶部如果还没 `use tracing::info;`，本改动里用 `tracing::info!` 完整路径就不需要加 use。如果文件已经用 `use tracing::{debug, info, ...}`，直接 `info!(...)`。两种写法选一种保持文件一致。先 `head -20 crates/mossen-agent/src/api_client.rs` 看一下当前 import 风格。

### 验证

```bash
cd /Users/allen/Documents/rustmossen
cargo build -p mossen-agent 2>&1 | tail -5
```

**完成判定**：`Finished`，无 error。

实际语义验证（可选）：

```bash
RUST_LOG=mossen_agent=info \
MOSSEN_CODE_USE_CUSTOM_BACKEND=1 \
MOSSEN_CODE_CUSTOM_BASE_URL=http://localhost:8000 \
MOSSEN_CODE_CUSTOM_MODEL=deepseek-v4-flash \
./target/debug/mossen --oneshot "hi" 2>&1 | grep "custom backend routing"
```

应当看到一条 `custom backend routing: ...` 日志。

### 回滚

`git checkout -- crates/mossen-agent/src/api_client.rs`

---

## M1 阶段验收

5 个 task 都做完后跑：

```bash
cd /Users/allen/Documents/rustmossen
cargo build --workspace 2>&1 | tail -3
cargo test -p mossen-cli 2>&1 | tail -3
```

**M1 完成判定**：
- workspace 编译 0 error
- mossen-cli 测试全过（包括新加的 at_include_test）

然后向用户报告："M1 完成。可以进 M2 吗？"等用户确认。

---

# 🅼 M2 阶段：测试修复 + 实地烤机

## M2-1 修 `mossen-utils::semver::test_satisfies_tilde`

### 背景

测试 `crates/mossen-utils/src/semver.rs:172` 期望 `satisfies("1.2.3", "~1.0.0") == true`，但当前实现返回 false。

标准 SemVer 里 `~1.0.0` 意为 `>= 1.0.0, < 1.1.0`，`1.2.3` **不应**满足。但 Mossen 的测试期望表明项目里 `~` 的语义是 **tilde-major**（`>= X.Y.Z, < (X+1).0.0`）。需要先决定哪个是 ground truth。

### 步骤

1. 读 `crates/mossen-utils/src/semver.rs` 全文，重点看 `satisfies` 和 `~` 的处理路径。
2. 看 test_satisfies_tilde 完整断言列表，列出所有 (version, range, expected) 元组。
3. 推断项目想要的 tilde 语义，**与用户确认**后再改实现。
4. 改 `satisfies` 实现使所有断言通过。

**⚠️ 这个 task 必须等用户先确认 tilde 语义再动手**。在 verify "项目想要 tilde-major" 之前不要乱改实现。

### 验证

```bash
cd /Users/allen/Documents/rustmossen
cargo test -p mossen-utils semver:: 2>&1 | tail -10
```

**完成判定**：`semver::tests::test_satisfies_tilde` 和 `semver::tests::test_prerelease` 都 ok。

---

## M2-2 修 `mossen-utils::json_read::test_strip_bom`

### 背景

测试 `crates/mossen-utils/src/json_read.rs` 期望 strip 掉文件开头的 UTF-8 BOM (`\xef\xbb\xbf`) 后能正常解析 JSON。当前实现遗漏了 BOM 检测。

### 步骤

1. 读 `crates/mossen-utils/src/json_read.rs` 看 `strip_bom` / `read_json` 的实现。
2. 在 JSON 读取前先检查并跳过 BOM：

```rust
const UTF8_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];

fn strip_bom_inplace(s: &mut String) {
    if s.as_bytes().starts_with(UTF8_BOM) {
        s.replace_range(..UTF8_BOM.len(), "");
    }
}
```

调用方在 `serde_json::from_str` 之前先 `strip_bom_inplace(&mut content)`。

### 验证

```bash
cargo test -p mossen-utils json_read::tests::test_strip_bom 2>&1 | tail -5
```

**完成判定**：`test result: ok. 1 passed`。

---

## M2-3 修 `mossen-utils::early_input::escape_sequence_dropped`

### 背景

测试 `crates/mossen-utils/src/early_input.rs` 期望终端转义序列（如 `\x1b[A`）在 `early_input` 处理时被丢弃。当前实现漏处理。

### 步骤

1. 读 `crates/mossen-utils/src/early_input.rs` 找处理 raw input 的位置。
2. 看测试用例 `escape_sequence_dropped` 的输入和期望输出。
3. 在处理逻辑里加 ESC 序列识别 + 丢弃。

通常用一个简单状态机：遇到 `0x1b`（ESC）后进入"丢弃模式"，吃掉后续 `[…<final byte>` 直到 final byte（`0x40-0x7e` 范围）为止。

### 验证

```bash
cargo test -p mossen-utils early_input 2>&1 | tail -5
```

**完成判定**：`escape_sequence_dropped` test ok。

---

## M2-4 修 `mossen-tools::bash_security` 等 15 个 bash 测试（regex backreference 问题）

### 背景

**所有 15 个 bash_security 相关测试都因为同一个根因失败**：`crates/mossen-tools/src/bash_tool/bash_security.rs:154` 的正则用了 backreference `\1`，但 Rust 标准 `regex` crate **不支持**它（regex 引擎是 finite-automaton 的，backref 是 NP-hard）。

### 选项

A. 改用 `fancy-regex` crate（支持 backref，但慢一些）
B. 用普通正则 + 后处理（自己写状态机匹配 heredoc）

**推荐 B**：避免引入新依赖，且 heredoc 的语法本来就够小，状态机几十行 Rust。

### 步骤

1. 读 `crates/mossen-tools/src/bash_tool/bash_security.rs` 154 行附近的 regex 定义和调用方。
2. 列出所有用到 `\1` 的位置。
3. 把每个 `regex::Regex` 加 backref 的地方改成手写的解析函数。

例：heredoc 的 `<<EOF ... EOF` 匹配可以这样写：

```rust
/// 检测一段 bash 是否包含 heredoc。
/// 不用 regex backref，手写状态机：
///   1. 找 `<<[-~]?` 标记
///   2. 读后面的 quote 包围或 unquoted 字符串作为 delimiter
///   3. 在后续行里找单独一行 `delimiter`
fn contains_heredoc(src: &str) -> bool {
    let bytes = src.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'<' && bytes[i + 1] == b'<' {
            let mut j = i + 2;
            if j < bytes.len() && (bytes[j] == b'-' || bytes[j] == b'~') {
                j += 1;
            }
            // 提取 delimiter
            let (delim, k) = parse_heredoc_delim(&bytes[j..]);
            if let Some(d) = delim {
                // 在 k.. 后面找 \n<d>\n 或 \n<d>$ 
                let after = &src[j + k..];
                if heredoc_terminator_found(after, &d) {
                    return true;
                }
            }
        }
        i += 1;
    }
    false
}

fn parse_heredoc_delim(rest: &[u8]) -> (Option<String>, usize) { /* ... */ }
fn heredoc_terminator_found(after: &str, delim: &str) -> bool { /* ... */ }
```

具体函数体根据 15 个测试的输入推断。**先把每个失败测试用 `cargo test -- --nocapture` 跑一遍**看清楚每个测试期望什么输入触发什么判定，再实现。

### 验证

```bash
cd /Users/allen/Documents/rustmossen
cargo test -p mossen-tools bash_tool:: 2>&1 | tail -10
```

**完成判定**：`mossen-tools` 全部测试通过（59 + 15 = 74 个全 ok）。

---

## M2-5 修 `mossen-skills::dynamic::glob_matches_basic_and_double_star`

### 背景

`crates/mossen-skills` 里 glob 匹配的 `**` （递归通配符）处理有 bug。

### 步骤

1. `cargo test -p mossen-skills glob_matches -- --nocapture` 看完整断言列表。
2. 找 `crates/mossen-skills/src/dynamic.rs` 里的 glob 匹配实现。
3. 比对 `globset` 标准库行为修正。一般是用 `globset::GlobBuilder::new(pat).literal_separator(false).build()` 即可获得标准 `**` 语义。

### 验证

```bash
cargo test -p mossen-skills 2>&1 | tail -5
```

**完成判定**：`mossen-skills` 全部测试通过。

---

## M2-6 真 TTY 烤机 30 分钟

### 背景

前面所有 task 都没在真实 TTY 里跑 mossen 的 TUI。生产级要求实际用 30 分钟无 panic / 无静默卡死 / 渲染正确。

### 步骤

1. 用户开一个 terminal，跑：

```bash
cd /Users/allen/Documents/rustmossen
cargo build --workspace --release  # release 模式更接近生产
MOSSEN_CODE_USE_CUSTOM_BACKEND=1 \
MOSSEN_CODE_CUSTOM_BASE_URL=http://localhost:8000 \
MOSSEN_CODE_CUSTOM_MODEL=deepseek-v4-flash \
RUST_LOG=mossen_agent=info,mossen_cli=info \
./target/release/mossen
```

2. 完成以下 6 项验收（每项独立 turn）：
   - [ ] 问个简单问题（"你好"），看回复正常
   - [ ] 让它读一个文件（`Read /Users/allen/Documents/rustmossen/Cargo.toml`），看 tool use 渲染
   - [ ] 让它跑一条 Bash（`Bash ls`），看 permission 弹窗 + 输出渲染
   - [ ] 让它做 Edit（创建并修改 `/tmp/mossen_smoke.txt`），看 diff 渲染
   - [ ] 给它一个多轮任务（"找 src 下所有 .rs 文件并统计 unsafe 块数量"），看连续 tool 调用 + spinner
   - [ ] 中途按 Ctrl+C 一次，看是否能优雅停止并恢复 prompt

3. 每发现一个 bug，在 `/tmp/mossen_smoke_bugs.md` 里记一行：`- [BUG] 描述 / 触发条件 / 现象`

4. 30 分钟结束后把 `/tmp/mossen_smoke_bugs.md` 交给用户。

### 完成判定

- 30 分钟内**没有 panic、没有死锁、没有渲染撕裂**
- 6 项验收都 ✓
- bug 清单已交付

⚠️ 这个 task 需要**真实 TTY 交互**，本计划的执行 agent 一般跑在非交互环境下没法直接做。**遇到这个 task 时停下来，让用户手动跑**。

---

## M2 阶段验收

```bash
cd /Users/allen/Documents/rustmossen
cargo test --workspace 2>&1 | grep "test result:" | tail -20
```

**M2 完成判定**：所有 `test result:` 行都是 `ok`，0 failed。

---

# 🅼 M3 阶段：生产化打磨

## M3-1 内置 `ds4-local` 默认 profile

### 目标

把 `--profile ds4-local` 设为默认，用户不用记一堆环境变量，进来就是接 ds4-server。

### 步骤

1. 在 `crates/mossen-cli/src/repl.rs` 或 config 加载位置找 profile 解析点。
2. 如果有内置 profile 机制，新增条目；如果没有，加一个 const PROFILE_DS4_LOCAL: &str = ...，用于 fallback。
3. 检测 `MOSSEN_CODE_USE_CUSTOM_BACKEND` 未设时，自动检查 `localhost:8000/v1/models` 通不通，通就启用 ds4-local。
4. 文档化：在 README.md 加一段 "First run" 说明。

### 验证

清空所有 MOSSEN 环境变量后直接跑 `./target/debug/mossen --oneshot "hi"`，应该自动连上 ds4。

---

## M3-2 增强 `mossen doctor`

### 目标

`mossen doctor` 应当检查：
- ds4-server 联通性（`GET /v1/models`）
- 模型 `deepseek-v4-flash` 是否在返回列表中
- 工具 schema 是否能编译（迭代每个工具，调用 `tool.schema()` 看是否 panic）
- KV cache 路径是否可写
- 系统资源（内存余量、磁盘空间）

每项输出 `✓ / ✗` + 一句话原因。

### 验证

```bash
./target/debug/mossen doctor 2>&1 | grep -E "^\s*[✓✗]"
```

应该看到 5+ 行 check 项。

---

## M3-3 错误信息可读化

### 目标

把当前所有 `anyhow::Error` 在用户可见出口（CLI 退出、TUI status bar、HTTP error response）做一次"翻译"，每个错误带：
- 一句简短描述（中文）
- 一行"接下来怎么办"建议

### 步骤

1. 列出所有 `eprintln!("Error: {}", e)` / `bail!()` / 类似出口。
2. 给每个常见错误写映射表（例如 "connection refused" → "ds4-server 没启动，跑 `launchctl load …`"）。
3. 输出格式统一：
   ```
   ✗ <一句话错误> 
     原因: <技术原因>
     建议: <下一步动作>
   ```

### 验证

人工跑 5 种典型错误（端口关、模型名错、磁盘满、context 超限、tool schema 不合法）确认输出符合格式。

---

## M3-4 生产验收：1 小时真实编码 sprint

### 步骤

1. 在一个真实小项目里（比如做个 100 行的 Rust CLI），用 mossen + ds4 完成至少 5 个非平凡 task：
   - 加一个 subcommand
   - 写一个集成测试
   - 修一个 bug
   - 重构一个函数
   - 写文档
2. 全程录下命令、回复、用时。
3. 1 小时结束记账：
   - 完成几个 task
   - 平均每个 task 多少 turn
   - mossen 自己崩了几次
   - 模型答错多少次需要纠正
4. 把账本交给用户。

### 完成判定

- 完成 ≥ 4/5 task
- mossen 自己崩 ≤ 1 次
- 平均每个 task ≤ 10 turn

---

# 附录

## A. 失败回滚通用步骤

任意 task 失败后：

```bash
cd /Users/allen/Documents/rustmossen
git status                          # 看改了啥
git diff <文件>                     # 看具体改动
git checkout -- <文件>              # 仅仅丢弃单个文件的改动
```

⚠️ **永远不要** `git reset --hard`。

## B. 常用命令速查

| 任务 | 命令 |
|------|------|
| 增量编译某 crate | `cargo build -p mossen-cli` |
| 编译全部 | `cargo build --workspace` |
| 跑某 crate 测试 | `cargo test -p mossen-utils` |
| 跑某测试 | `cargo test -p mossen-tools bash_security::tests::test_safe_command -- --nocapture` |
| 看运行时日志 | `RUST_LOG=mossen_agent=trace,mossen_cli=trace ./target/debug/mossen ...` |
| ds4-server 健康 | `curl -sf http://localhost:8000/v1/models \| head -5` |
| 抓 mossen 实际发的 prompt | 见 `/tmp/mossen_capture.py`（已存在） |

## C. 项目结构速查

```
/Users/allen/Documents/rustmossen/
├── crates/
│   ├── mossen-agent/         # agent loop (dialogue.rs)、API client、provider
│   ├── mossen-cli/           # 入口 main.rs、repl.rs、system_prompt.rs
│   ├── mossen-commands/      # slash 命令实现
│   ├── mossen-mcp/           # MCP 协议
│   ├── mossen-remote/        # 远程 mossen 连接
│   ├── mossen-skills/        # /skill 命令的 skill 加载
│   ├── mossen-tools/         # Bash/Read/Edit/Write 等工具实现
│   ├── mossen-tui/           # ratatui 渲染
│   ├── mossen-types/         # 类型 + 常量 prompts.rs
│   └── mossen-utils/         # 通用 helper
├── Cargo.toml                # workspace
└── PLAN_PRODUCTION.md        # 本文档
```

## D. 关键文件 quick ref

| 你要改什么 | 去哪 |
|----------|------|
| 默认 model 名 | `crates/mossen-cli/src/repl.rs` |
| system prompt 装配 | `crates/mossen-cli/src/system_prompt.rs` |
| system prompt 文本常量 | `crates/mossen-types/src/constants/prompts.rs` |
| agent 主循环 | `crates/mossen-agent/src/dialogue.rs` |
| API 客户端 / provider routing | `crates/mossen-agent/src/api_client.rs` |
| TUI 渲染主循环 | `crates/mossen-tui/src/app.rs` |
| Bash 工具实现 | `crates/mossen-tools/src/bash.rs` 和 `bash_tool/` |
| 工具注册表 | `crates/mossen-agent/src/tools_index.rs` |
| 配置 / 后端解析 | `crates/mossen-utils/src/custom_backend.rs` |

## E. 与用户的沟通规则

执行任何 task 前后，向用户报告**简短**（≤ 3 行）：

- **开始时**：`正在做 M1-X：<标题>`
- **完成时**：`M1-X 完成。验证通过：<完成判定的关键输出>`
- **失败时**：`M1-X 失败。错误：<错误片段>。已停下，等待指示。`

**不要长篇大论**。完成报告要附上 `cargo build` / `cargo test` 的最后几行作为证据。

---

文档版本：1.0（2026-05-19）
作者：本计划由 ds4-server 的姐妹 agent 生成
执行环境：本机 ds4-server（DeepSeek V4 Flash）+ Mossen Rust 重写版
