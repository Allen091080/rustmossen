# Mossen Rust 生产化实施计划（v2 · 融合版）

> **目标读者**：本地 DeepSeek V4 Flash（通过 ds4-server 跑 Mossen 自身或其他 agent 执行本文档）
> **目标**：把 `/Users/allen/Documents/rustmossen` 从 **80%「能编译能跑 oneshot」** 推进到 **生产可用 multi-turn TUI** —— 重点是把已有的零件**接起来**，不是重写
> **预计工期**：约 25-40 个交互轮次 / 2-3 周
> **生效环境**：macOS / Apple Silicon / ds4-server 在 localhost:8000 监听
> **核心战场**：两大融合方向 —— ① Agent runtime / harness ② TUI 渲染

---

## 0. 阅读约定（必读）

### 0.1 角色与权限

你是 Rust 工程师。**可以**：
- 用 `Read` 读 `/Users/allen/Documents/rustmossen/**` 下任何文件
- 用 `Edit` 修改 Rust 源码、Cargo.toml、Markdown
- 用 `Write` 创建新文件
- 用 `Bash` 跑 `cargo`、`grep`、`ls`、`git diff`

**绝对不能**：
- 运行 `git push`、`git reset --hard`、`rm -rf`
- 删除任何未在本文档明示要删的文件
- 修改 `/Users/allen/Documents/ds4/` 任何东西（模型服务器，独立项目）
- 退出 `crates/` 工作（项目只剩 Rust，没有其他语言源码）

### 0.2 执行节奏

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

做完一个 task 必须跑「验证」，输出符合「完成判定」才能继续。**不要批量、不要并行、不要跳验证**。

### 0.3 卡住时

如果某个 task 的「位置」/「改动」对不上实际源码（行号漂移、代码已改过、找不到符号），**立即停止**并报告：
- 你试了哪些查找
- 实际看到什么
- 推测文档过时还是源码已改

然后等用户确认。**不要猜怎么改**。

### 0.4 命令前缀

所有 `Bash` 默认在 `/Users/allen/Documents/rustmossen` 下执行。

### 0.5 开工前基线确认

```bash
cd /Users/allen/Documents/rustmossen
cargo check --workspace 2>&1 | tail -3
```

**期望**：最后一行 `Finished`，无 `error[`。有 error 就停下报告。

---

## 1. 整体地图

Mossen Rust 的现实情况：

```
                  ✅ 已实现且在用
                  🟡 已实现但未完全接通（数据壳 / 缺驱动 / 缺调度）
                  ⚪ 部分实现 / 边角缺失
                  🔴 未实现

mossen-cli          ✅ main.rs / repl.rs / system_prompt.rs / handlers/
                    ⚪ 默认模型字符串硬编码、CLAUDE.md @-include 没展开

mossen-agent
├─ dialogue.rs      ✅ 6 段 turn 循环、流式接 SSE、tool 调度
│                   🟡 仅 wire 了 stop_hook，其他 5 类 hook 未调度
├─ hooks/           ✅ executor.rs / post_sampling.rs / exec_command.rs
│                   🟡 模块在，但 dialogue 未调用其他 hook 类型
├─ services/
│   ├─ compact/     ✅ auto_compact / compact / session_memory_compact 都在
│   │               🟡 pre/post-compact 钩子未在 compact 流程里调度
│   ├─ tools/       ✅ streaming_tool_executor.rs
│   └─ agent_summary
├─ api_client.rs    ✅ OpenAI-compat 95% + Anthropic 80%
└─ stop_hooks.rs    ✅ wire 到 dialogue.rs:451

mossen-tools
├─ agent_tool/      ✅ run_agent / resume_agent / fork_subagent
│   └─ built_in/    ✅ 6 个内置 agent (general/plan/explore/...)
├─ todo_write_tool/ ✅ 工具实现完整
├─ bash_tool/       ✅ + ⚪ 15 个 bash_security 测试因 regex backref 失败
└─ shared/spawn_multi_agent.rs ✅

mossen-tui
├─ app.rs           ✅ 主 event loop / Modal dispatcher / 状态机
├─ widgets/         ✅ 实际在用的 widget 树（messages/markdown/spinner）
├─ components/      ✅ 部分在用（dialogs / permissions / misc / root_large）
│                   🟡 tasks.rs (TaskListV2 / SubAgentProvider) 已写但没人 emit 事件驱动
│                   🟡 spinner_anim.rs (teammate spinner) 已写但没接
├─ ink/             ⚪ 6934 行平移自 TS Ink，未挂入实际渲染路径
└─ state.rs         🟡 foreground_task_id 在但少配套字段

mossen-skills / mossen-mcp / mossen-utils / mossen-types
                    ✅ 大致到位，少数测试失败（semver/json/escape）
```

### 两大融合战场

**Harness 战场**：dialogue.rs 是核心，已经能跑一圈。差的是：
- 其他 5 类 hook（pre/post-compact、post-sampling、session-start、task-completed）的调度点未插齐
- 3 层 compact 瀑布之间的协作 + 断路器状态机
- 子 agent transcript 隔离（task_local AgentContext）
- 工具 streaming 累积 + 并发分区的实际触发路径

**渲染战场**：app.rs 是核心，已经能流式显示。差的是：
- TodoWrite 工具改动 → TaskListV2 widget 渲染（widget 在 components/tasks.rs:376，**没被 app.rs 调用**）
- Task tool / sub-agent 产生的 nested SdkMessage → teammate 渲染
- foreground_task_id 切换 / Ctrl+B 进入后台任务详情
- 流式 markdown 在长答复下的重解析性能（可能有 O(n²) 风险，需测）

---

## 2. 阶段总览

| 阶段 | 任务数 | 工期 | 目标 |
|------|------|------|------|
| **Phase 0** 正确性硬伤 | 5 | 1-2 天 | 模型身份正确、CLAUDE.md 完整 import、默认值合理 |
| **Phase 1** Harness 融合 | 5 | 3-5 天 | 5 类 hook 全部 wire；3 层 compact 协作通；子 agent 隔离硬保证 |
| **Phase 2** 外围 5 系统接合 | 4 | 3-5 天 | `toolPermissionContext` 单一写入端；skill / MCP / plugin 改动事件总线 |
| **Phase 3** 渲染层 gap 接合 | 5 | 3-5 天 | TodoWrite 实时渲染；子 agent 输出占位；性能基线 |
| **Phase 4** 生产验收 | 3 | 2-3 天 | 测试全过；30 min TTY 烤机；1 h 真实编码 sprint |

**总计 22 task，约 2-3 周**。

---

# 🅿 Phase 0：正确性硬伤

## 0-1 修正硬编码模型名 `"claude-opus-4-5"`

### 背景

`crates/mossen-cli/src/repl.rs` 两处把默认模型名硬编码为 `"claude-opus-4-5"`。custom backend 模式下，env_info section 输出 `You are powered by the model claude-opus-4-5.` —— 误导模型自我能力评估。

### 位置

```bash
grep -n 'claude-opus-4-5' crates/mossen-cli/src/repl.rs
```

**期望**：恰好 2 行命中，行号在 270-280 和 460-470。0 或 >2 行就停下报告。

### 改动

#### Step 1：加 helper（放在文件首个 `pub fn` 之前）

```rust
/// 返回 oneshot / exec 路径的默认 model id。
/// 优先级：MOSSEN_CODE_CUSTOM_MODEL → "custom-backend-model"
fn default_model_for_unset_cli() -> String {
    std::env::var("MOSSEN_CODE_CUSTOM_MODEL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "custom-backend-model".to_string())
}
```

#### Step 2：替换两处

把 `.unwrap_or_else(|| "claude-opus-4-5".to_string())` 都改成 `.unwrap_or_else(default_model_for_unset_cli)`。

### 验证

```bash
cargo build -p mossen-cli 2>&1 | tail -5
```

**完成判定**：`Finished` 输出，无 `error[`。warning 不计。

### 回滚

`git checkout -- crates/mossen-cli/src/repl.rs`

---

## 0-2 修正 TUI 默认模型 `"MiniMax-M2"`

### 背景

`crates/mossen-cli/src/repl.rs:81` 默认 TUI 模型 `"MiniMax-M2"`。同 0-1，应读 `MOSSEN_CODE_CUSTOM_MODEL`。

### 位置

```bash
grep -n '"MiniMax-M2"' crates/mossen-cli/src/repl.rs
```

**期望**：恰好 1 行命中。其他 `MiniMax-M2` 在 system_prompt.rs / app.rs / custom_backend.rs 都是测试常量，**不动**。

### 改动

把这行的 `.unwrap_or_else(|| "MiniMax-M2".to_string())` 改成 `.unwrap_or_else(default_model_for_unset_cli)`（复用 0-1 的 helper）。

### 验证

```bash
cargo build -p mossen-cli 2>&1 | tail -5
grep -n '"MiniMax-M2"' crates/mossen-cli/src/repl.rs
```

**完成判定**：build `Finished`；第二条 grep 返回空。

### 回滚

`git checkout -- crates/mossen-cli/src/repl.rs`

---

## 0-3 在 CLAUDE.md / MOSSEN.md 中支持 `@<file>` 递归 include

### 背景

用户可能在 `~/.claude/CLAUDE.md` 或 `MOSSEN.md` 里写 `@OTHER.md` 来 import 其他文件（agent 配置约定）。Rust 端 `gather_memory_text`（`crates/mossen-cli/src/system_prompt.rs:192-249`）**把整个文件当字面量塞进 prompt**，不展开 @-include。

实际后果：当用户 `~/.claude/CLAUDE.md` 只有一行 `@RTK.md` 时，传给模型的只是这 6 个字符，真正 RTK.md 内容（约 1 KB）丢失。

### 位置

文件：`crates/mossen-cli/src/system_prompt.rs`
函数：`pub async fn gather_memory_text(cwd: &std::path::Path) -> String`（约 192 行起）

### 改动

#### Step 1：加 expand 函数（放在 `gather_memory_text` 之前）

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

#### Step 2：在 `gather_memory_text` 的 3 处 `tokio::fs::read_to_string` 后加调用

把 user-global / project-root / nested 三处都改成：

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

三处格式字符串里的 `(...)` 部分按原文保留，不要改。

### 验证

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

**完成判定**：`test result: ok. 1 passed`。

⚠️ 如果 `cargo test` 报「找不到 `mossen_cli::system_prompt::gather_memory_text` 符号」，说明 mossen-cli 是 bin crate 无 lib.rs。**这种情况停下报告**，不要私自加 lib.rs。

### 回滚

```bash
git checkout -- crates/mossen-cli/src/system_prompt.rs
rm -f crates/mossen-cli/tests/at_include_test.rs
rm -rf /tmp/mossen_include_test
```

---

## 0-4 修正 fallback model 占位实现

### 背景

`crates/mossen-agent/src/dialogue.rs` 第 283 行附近，`RetryError::FallbackTriggered` 当前直接 `warn!` + 返回 `TerminalReason::ModelError`，把这个 turn 失败掉。single-model 场景应当当作"用当前 model 重试"，而不是失败。

### 位置

```bash
grep -n "FallbackTriggered" crates/mossen-agent/src/dialogue.rs
```

预期 1 处命中。

### 改动

把整个匹配臂改成：

```rust
Err(RetryError::FallbackTriggered { original, fallback }) => {
    // 当前没有真正的 fallback handler。策略：
    //   - 记 warn 告诉用户"想 fallback 但没装"
    //   - 不失败，返回 TerminalReason::Retry，让上层用当前 model 再试
    warn!(
        %original, %fallback,
        "Fallback requested but no handler wired; will retry with the original model"
    );
    return Ok((TerminalReason::Retry, cost_state));
}
```

⚠️ **前置确认**：

```bash
grep -nE "enum TerminalReason|^    Retry" crates/mossen-agent/src/dialogue.rs | head -5
```

如果 `TerminalReason` enum 没 `Retry` 变体，**停下报告**。本任务不要私自动 enum。

### 验证

```bash
cargo build -p mossen-agent 2>&1 | tail -5
```

**完成判定**：`Finished`。

### 回滚

`git checkout -- crates/mossen-agent/src/dialogue.rs`

---

## 0-5 在 custom backend 路径加可观测日志

### 背景

`crates/mossen-agent/src/api_client.rs:148-150` 在 `is_custom_backend_enabled()` 为 true 时 route 到 OpenAI-compat 路径，但没有 info! 日志告诉用户实际打哪个 base URL —— 排查问题时只能猜。

### 位置

```bash
grep -n "call_streaming_openai_compat" crates/mossen-agent/src/api_client.rs | head -3
```

### 改动

在 `return call_streaming_openai_compat(params, cancel).await;` 之前插入：

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

文件如果已 `use tracing::info;`，可以省 `tracing::` 前缀。先 `head -20` 看 import 风格。

### 验证

```bash
cargo build -p mossen-agent 2>&1 | tail -5
```

可选语义验证：

```bash
RUST_LOG=mossen_agent=info \
MOSSEN_CODE_USE_CUSTOM_BACKEND=1 \
MOSSEN_CODE_CUSTOM_BASE_URL=http://localhost:8000 \
MOSSEN_CODE_CUSTOM_MODEL=deepseek-v4-flash \
./target/debug/mossen --oneshot "hi" 2>&1 | grep "custom backend routing"
```

应看到一条 `custom backend routing: ...` 日志。

### 回滚

`git checkout -- crates/mossen-agent/src/api_client.rs`

---

## Phase 0 验收

```bash
cargo build --workspace 2>&1 | tail -3
cargo test -p mossen-cli 2>&1 | tail -3
```

**完成判定**：workspace 编译 0 error；mossen-cli 测试全过（包括新加的 at_include_test）。

向用户报告："Phase 0 完成。可以进 Phase 1 吗？"等用户确认。

---

# 🅿 Phase 1：Harness 融合

> 战场：dialogue.rs / hooks/ / services/compact/ / agent_tool/
> 目标：把已实现的 hook 模块、compact 瀑布、sub-agent 系统**接到主循环**

## 1-1 审计：列出现有 hook 模块 + dialogue.rs 实际调用覆盖

### 背景

Rust 端有完整的 hook 模块（agent/hooks/、agent/stop_hooks.rs、utils/hooks.rs 等），但 dialogue.rs 目前**只 wire 了 stop_hook**。其他 5 类 hook（pre/post-compact、post-sampling、session-start、task-completed）的调用点未知 —— 是没接、还是接在别处。这个 task **不写代码，只产出一份审计报告**。

### 位置

需读：
- `crates/mossen-agent/src/dialogue.rs` 全文
- `crates/mossen-agent/src/hooks/mod.rs` + 该目录所有 .rs
- `crates/mossen-agent/src/stop_hooks.rs`
- `crates/mossen-agent/src/query/stop_hooks.rs`
- `crates/mossen-utils/src/hooks.rs` + `hooks_utils.rs`
- `crates/mossen-types/src/hooks.rs`
- `crates/mossen-agent/src/services/compact/` 全部
- `crates/mossen-cli/src/handlers/` 全部

### 改动

产出一份 markdown 文件 `crates/mossen-agent/HOOK_AUDIT.md`，包含：

```markdown
# Hook 系统审计 / Phase 1-1

## 已定义的 hook 类型清单
| Hook 类型 | 定义位置 file:line | manager / executor 位置 |
|---|---|---|
| StopHook | ... | ... |
| PostSamplingHook | ... | ... |
| PreCompactHook | ... | ... |
| PostCompactHook | ... | ... |
| SessionStartHook | ... | ... |
| TaskCompletedHook | ... | ... |
| ExecCommandHook | ... | ... |
| ExecAgentHook | ... | ... |

## 在 dialogue.rs / compact / cli main 中的实际调用
| 调用点 | file:line | 调用哪类 hook | 上下文 |
|---|---|---|---|
| ... | ... | ... | ... |

## 缺口（hook 已定义但未在主路径调度）
- [ ] PostSampling: 定义在 hooks/post_sampling.rs:_ ，但 dialogue.rs 未调用
- [ ] PreCompact: ...

## Phase 1 后续 task 的 wire 目标
- 1-2: wire PostSamplingHook 到 dialogue.rs 第 _ 行（API 响应到 → 工具调度前）
- 1-3: wire PreCompactHook / PostCompactHook 到 services/compact/compact.rs:_
- 1-4: wire SessionStartHook 到 main.rs / repl.rs:_
```

### 验证

```bash
ls -la crates/mossen-agent/HOOK_AUDIT.md
wc -l crates/mossen-agent/HOOK_AUDIT.md
```

**完成判定**：文件存在、≥ 50 行、表格完整无空行。

### 回滚

`rm crates/mossen-agent/HOOK_AUDIT.md`

---

## 1-2 Wire PostSamplingHook 到 dialogue.rs

### 背景

PostSampling hook 在 TS 端是 API 采样完成之后、消息回上层之前调用，用于注入 `system-reminder` 等。Rust 端 `crates/mossen-agent/src/hooks/post_sampling.rs` 已实现，但 dialogue.rs 没调用它。

### 位置

基于 Phase 1-1 审计：
- hook 定义：`crates/mossen-agent/src/hooks/post_sampling.rs`
- 期望调用点：dialogue.rs 中 API 响应解析完毕、进入 tool 调度前

### 改动

参照 stop_hook 的 wire 模式（dialogue.rs 第 444-466 行附近）：

```rust
// 1. 在 execute_turn_cycle 函数签名 / state 中携带 PostSamplingHookManager
// 2. API 流式接收结束后（拿到 stop_reason 之前 / 之后取决于审计结论）调用：
let psh_result = post_sampling_manager.run(&psh_ctx, &assistant_message, cancel).await;
match psh_result {
    Ok(modified_msgs) => {
        // 把 modified_msgs 拼回 state.messages
    }
    Err(e) => {
        warn!(error = %e, "post-sampling hook failed; continuing");
    }
}
```

具体 ctx 字段、错误处理形态以 Phase 1-1 审计结论为准。⚠️ **如果审计发现 PostSamplingHook 接口与 StopHook 形态差异大，停下报告**。

### 验证

```bash
cargo build --workspace 2>&1 | tail -5
cargo test -p mossen-agent dialogue 2>&1 | tail -10
```

**完成判定**：`Finished`；dialogue 相关测试通过。

如果有现成 hook fixture 测试（看 `crates/mossen-agent/tests/`），跑它：

```bash
cargo test -p mossen-agent --test '*hook*' 2>&1 | tail -10
```

### 回滚

`git checkout -- crates/mossen-agent/src/dialogue.rs`

---

## 1-3 Wire Pre/PostCompactHook 到 services/compact/compact.rs

### 背景

`services/compact/compact.rs` 实现了 traditional 压缩，`auto_compact.rs` 实现了断路器 + 阈值检测，`session_memory_compact.rs` 实现了第一层快速压缩。三个流程都是**用户可编程钩子点**（pre_compact 提示要压了、post_compact 拿压缩结果做后处理），但当前未调用任何 hook。

### 位置

基于 Phase 1-1 审计：
- 期望调用点 1：`services/compact/compact.rs::compact_conversation()` 入口（pre-compact）
- 期望调用点 2：同函数返回 CompactionResult 之前（post-compact）
- 期望调用点 3：`auto_compact.rs::should_auto_compact()` 之后、决定真要 compact 之前（pre-compact）

### 改动

参照 1-2 的 wire 模式，在 3 个调用点分别插入 hook 调用。注意：

- **pre-compact 失败 → skip compact**（保守，避免错误压缩）
- **post-compact 失败 → 仍返回 CompactionResult**（hook 失败不能让用户失了 boundary marker）
- 这两条策略要在 hook 调用代码的注释里写清楚

如果 hook 接口接受可变 messages 参数（hook 可以修改要压缩的消息），就把 `messages_to_compact` 当 in/out 参数传过去；如果只是观察型，传不可变引用即可。

### 验证

```bash
cargo build --workspace 2>&1 | tail -5
cargo test -p mossen-agent compact 2>&1 | tail -10
```

**完成判定**：`Finished`；compact 测试通过。

### 回滚

```bash
git checkout -- crates/mossen-agent/src/services/compact/
```

---

## 1-4 Wire SessionStartHook 到 cli / repl 启动路径

### 背景

SessionStart hook 用于 plugin 初始化、记忆系统预加载、startup banner 等。当前定义在 hooks/，但 main.rs / repl.rs 启动流程里没调用。

### 位置

基于 Phase 1-1 审计：
- hook 定义位置（待审计确认）
- 期望调用点：`crates/mossen-cli/src/main.rs` async main 入口 + `crates/mossen-cli/src/repl.rs` interactive REPL 启动前

### 改动

在 main.rs 拿到 cwd / config 之后、构造 EngineConfig 之前，调用：

```rust
let session_start_ctx = SessionStartHookContext {
    cwd: cwd.clone(),
    is_interactive: matches!(mode, RunMode::Repl),
    // ... 其他字段以审计为准
};
if let Err(e) = session_start_manager.run(&session_start_ctx, &cancel).await {
    warn!(error = %e, "session-start hook failed; continuing");
}
```

### 验证

```bash
cargo build --workspace 2>&1 | tail -5
RUST_LOG=mossen_agent::hooks=debug ./target/debug/mossen --oneshot "hi" 2>&1 | grep -i "session.start\|hook"
```

**完成判定**：build 过 + 日志里出现 session-start hook 相关行（至少一行 debug）。

### 回滚

`git checkout -- crates/mossen-cli/src/main.rs crates/mossen-cli/src/repl.rs`

---

## 1-5 子 agent transcript 隔离硬保证

### 背景

`crates/mossen-tools/src/agent_tool/run_agent.rs` 等已实现 sub-agent，但需要验证 3 个不变量：

1. **transcript 隔离**：子 agent 的 transcript 不能进父 transcript 文件
2. **AgentContext 传播**：子 agent 内部 spawn 的工具调用拿到自己的 agentId
3. **结果以 `<task-notification>` XML 块回灌**到父 transcript

### 位置

- 审计：`crates/mossen-tools/src/agent_tool/run_agent.rs` / `fork_subagent.rs`
- 配套：`crates/mossen-tools/src/shared/spawn_multi_agent.rs`
- transcript 写入：搜 `recordTranscript` / `record_transcript` 相关位置

### 改动

#### Step 1：审计每个不变量当前状态

跑这些 grep 并把结果记下来：

```bash
# 不变量 1：transcript 文件路径是否隔离
grep -rn "transcript_path\|transcript_file" crates/mossen-tools/src/agent_tool/ crates/mossen-agent/src/

# 不变量 2：AgentContext 传播机制
grep -rn "task_local\|AgentContext\|agent_id" crates/mossen-tools/src/agent_tool/ crates/mossen-agent/src/

# 不变量 3：task-notification 回灌
grep -rn "task-notification\|task_notification\|TaskNotification" crates/
```

#### Step 2：写一份小报告

`crates/mossen-tools/src/agent_tool/ISOLATION_AUDIT.md`：

```markdown
# Sub-Agent Isolation Audit

## 不变量 1：transcript 隔离
状态：（满足 / 部分 / 缺失）
证据 file:line：
建议 fix：

## 不变量 2：AgentContext 传播
状态：...

## 不变量 3：task-notification 回灌
状态：...

## 修补 task 提议
（如有缺失，列出具体 fix 步骤；如全满足，写"无需 fix"）
```

#### Step 3：根据审计结论决定下一步

- 全满足 → 完成本 task
- 有缺失 → **停下报告**，让用户决定要不要立刻补；如果用户同意补，再走具体 fix。**不要私自补 fix**，因为隔离逻辑改错会污染父 transcript

### 验证

```bash
ls -la crates/mossen-tools/src/agent_tool/ISOLATION_AUDIT.md
```

**完成判定**：审计文件存在；3 个不变量都有"满足 / 部分 / 缺失"判定 + 证据 file:line。

### 回滚

`rm crates/mossen-tools/src/agent_tool/ISOLATION_AUDIT.md`

---

## Phase 1 验收

```bash
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
- mossen-agent 测试通过
- 日志中能看到至少 1 条 post-sampling hook 调用 + 0 条 hook panic

向用户报告："Phase 1 完成。审计文件 HOOK_AUDIT.md / ISOLATION_AUDIT.md 已生成，可 review 后进 Phase 2。"

---

# 🅿 Phase 2：外围 5 系统接合

> 战场：tools/ / skills/ / mcp/ / plugins/ / permissions/
> 目标：5 个系统对 `toolPermissionContext` 写入收口；skill 改动触发缓存失效广播

## 2-1 toolPermissionContext 单一写入端审计

### 背景

权限上下文（`toolPermissionContext`）的语义是「allow/deny/ask 规则的运行态汇总」。在 TS 端，5 个系统（skill `allowed-tools`、MCP channelAllowlist、plugin policy、settings.json 规则、用户实时决策）**全部写入同一份**`alwaysAllowRules / denyRules / askRules / disallowedTools`。Rust 端如果各子系统持有自己的 context 副本，规则就会四散。

### 位置

```bash
grep -rln "toolPermissionContext\|tool_permission_context\|ToolPermissionContext\|alwaysAllowRules\|always_allow_rules" crates/ 2>/dev/null
```

预期至少 5-10 文件命中。

### 改动

产出 `crates/mossen-agent/PERMISSION_CONTEXT_AUDIT.md`：

```markdown
# ToolPermissionContext 写入审计

## 结构定义
位置 file:line：
字段（alwaysAllowRules / denyRules / askRules / disallowedTools / mode）：

## 所有写入位置
| 写入者 | file:line | 写哪个字段 | 触发场景 |
|---|---|---|---|
| skill loader | ... | alwaysAllowRules.command | skill `allowed-tools` 注入 |
| MCP channel | ... | ... | ... |
| plugin install | ... | ... | ... |
| permission dialog (Always allow) | ... | ... | 用户在弹窗选「Always allow」 |
| settings.json loader | ... | ... | 启动时加载 |

## 是否有"单一可变源"
判定：（是 / 否 / 多个副本互相不同步）

## 若有多副本：建议合并方案
（例：把 X / Y / Z 都改成 query `&AppState.permission_context` 而非各自持有；或引入 Arc<RwLock<PermissionContext>> 共享）
```

### 验证

```bash
ls -la crates/mossen-agent/PERMISSION_CONTEXT_AUDIT.md
wc -l crates/mossen-agent/PERMISSION_CONTEXT_AUDIT.md
```

**完成判定**：文件存在，≥ 40 行，包含「是否单一可变源」明确判定。

### 回滚

`rm crates/mossen-agent/PERMISSION_CONTEXT_AUDIT.md`

---

## 2-2 Skill discovery hook 接合

### 背景

TS 端 `discoverSkillDirsForPaths()`（每个工具调用产生新文件路径时）会向上扫描 `.mossen/skills/`，`activateConditionalSkillsForPaths()` 按 `paths` frontmatter 激活条件 skill。Rust 端 `mossen-skills` 已加载初始 skill 集，但**动态发现**这一钩子可能没接。

### 位置

```bash
grep -rln "discover_skill\|discoverSkillDirs\|activate_conditional\|conditional_skills" crates/mossen-skills/src crates/mossen-agent/src crates/mossen-tools/src 2>/dev/null
```

### 改动

#### Step 1：审计当前状态

跑上面的 grep，列出找到的 / 没找到的功能。

#### Step 2：决定接合策略

如果存在 `discover_skill_dirs_for_paths` 函数但未调用：
- 在 dialogue.rs 的 tool result 处理后（每个工具产生新文件路径之后）调用一次
- 调用结果回灌到 AppState.skills

如果**完全不存在**：
- 停下报告。这个功能比较复杂，需要用户决定要不要本 sprint 内做

### 验证

```bash
cargo build --workspace 2>&1 | tail -3
```

**完成判定**：build 过 + 至少在 dialogue 或 tool dispatcher 里能 grep 到 `discover_skill` 字样的调用。

### 回滚

`git diff` 看改了哪些，对每个文件 `git checkout --`。

---

## 2-3 Conditional skill activation 接合

### 背景

某些 skill 在 frontmatter 里写 `paths: [pattern1, pattern2]`，仅当当前工作目录或最近修改文件匹配时才激活。Rust 端 `mossen-skills/src/dynamic.rs` 应该有这套（注意：该文件还有 1 个 glob 测试失败，见 Phase 4）。

### 位置

```bash
grep -n "conditional\|Conditional\|paths_match\|activate" crates/mossen-skills/src/dynamic.rs 2>/dev/null | head -10
```

### 改动

#### Step 1：审计

- 列出 `dynamic.rs` 里 conditional-related 函数
- 看是否在 tool dispatcher / skill loader 流程里被调用

#### Step 2：接合

参照 Phase 2-2 同款逻辑。如果有 `activate_conditional_skills` 函数但未调用，找合适触发点（cwd 变化 / file 修改 / 启动时）调用。

### 验证

```bash
cargo build -p mossen-skills 2>&1 | tail -3
cargo test -p mossen-skills 2>&1 | tail -5
```

**完成判定**：build 过；mossen-skills 测试至少不**新增**失败（原本 glob 测试失败留到 Phase 4 修）。

### 回滚

`git checkout -- crates/mossen-skills/`

---

## 2-4 Plugin install / reload → skill / MCP 联动

### 背景

TS 端 `/reload-plugins` 触发：清 skill 缓存 → 重扫 skill → emit `skillsLoaded` → MCP 重连。Rust 端 `mossen-skills` + `mossen-mcp` + plugin 模块之间需要事件总线协同。

### 位置

```bash
grep -rln "reload_plugin\|reloadPlugin\|skillsLoaded\|skills_loaded\|plugin_reload" crates/ 2>/dev/null | head -10
```

### 改动

#### Step 1：审计

产出 `crates/mossen-cli/PLUGIN_RELOAD_AUDIT.md`：

```markdown
# Plugin Reload 联动审计

## reload-plugins 命令实现位置
file:line：

## reload 后触发的下游事件
- [ ] 清 skill 缓存：（是 / 否，证据）
- [ ] 重扫 skill 目录：（是 / 否）
- [ ] 通知 TUI 刷新工具列表：（是 / 否）
- [ ] MCP 重连：（是 / 否）

## 是否有 EventBus / 信号机制
判定：

## 缺失项 fix 提议
```

#### Step 2：根据审计决定

- 全联动 → 完成
- 缺联动 → 停下报告，让用户决定补哪些

### 验证

```bash
ls -la crates/mossen-cli/PLUGIN_RELOAD_AUDIT.md
```

**完成判定**：审计文件存在，4 个联动点都有明确判定。

### 回滚

`rm crates/mossen-cli/PLUGIN_RELOAD_AUDIT.md`

---

## Phase 2 验收

```bash
cargo build --workspace 2>&1 | tail -3
cargo test --workspace --no-fail-fast 2>&1 | grep "test result:" | tail -10
ls crates/mossen-agent/HOOK_AUDIT.md \
   crates/mossen-tools/src/agent_tool/ISOLATION_AUDIT.md \
   crates/mossen-agent/PERMISSION_CONTEXT_AUDIT.md \
   crates/mossen-cli/PLUGIN_RELOAD_AUDIT.md 2>&1
```

**Phase 2 完成判定**：
- build 0 error
- 测试结果与 Phase 0 后相同（不新增失败）
- 4 份审计文件都存在

向用户报告："Phase 2 完成。4 份审计文件已生成，包含发现的缺口与建议。可 review 后进 Phase 3（渲染层）。"

---

# 🅿 Phase 3：渲染层 gap 接合

> 战场：mossen-tui/src/app.rs / widgets/ / components/
> 目标：TodoWrite 实时显示、Sub-Agent 输出占位、性能基线
> ⚠️ **不要删 components/ 和 ink/**。components 部分在用（dialogs/permissions/misc/root_large 等），ink 是平移港但未碍事。删除前先 grep 确认每个子模块的使用情况

## 3-1 TodoWrite 工具事件 → TaskListV2 widget 渲染流水

### 背景

`crates/mossen-tools/src/todo_write_tool/` 实现了 TodoWrite 工具，`crates/mossen-tui/src/components/tasks.rs:376` 等定义了 `TaskListV2 / TaskAssignmentMsg / SubAgentProvider` 数据结构和 widget，但 **app.rs 当前没消费这些** —— 整个 TodoWrite 在 TUI 里走通用的 ToolResult 路径，无独立渲染。

### 位置

- 工具实现：`crates/mossen-tools/src/todo_write_tool/`
- 渲染壳：`crates/mossen-tui/src/components/tasks.rs:376` (`TaskListV2`)
- TUI 主 loop：`crates/mossen-tui/src/app.rs::handle_engine_message`

### 改动

#### Step 1：审计 + 设计接合点

跑：

```bash
grep -n "TodoWrite\|todo_write\|TaskListV2" crates/mossen-tui/src/app.rs
grep -n "TodoWrite\|todo_write" crates/mossen-tui/src/state.rs
```

预期都空命中。

#### Step 2：在 app.rs 里加 TaskList 状态

`state.rs` 加：

```rust
pub struct TaskListState {
    pub tasks: Vec<TodoItem>,         // 复用 mossen-tools::todo_write_tool 的 TodoItem
    pub last_update: Option<std::time::Instant>,
}

// 在 AppState 内：
pub task_list: TaskListState,
```

#### Step 3：在 handle_engine_message 里监听 TodoWrite tool_result

`app.rs::handle_engine_message`，当 `MessageType::ToolResult` 且 `tool_name == "TodoWrite"` 时：
- 解析 tool input（含 todos 数组）
- 更新 `self.app_state.task_list.tasks`
- 触发一次 redraw

#### Step 4：在主屏上画 TaskListV2

`app.rs::render` 里，如果 `app_state.task_list.tasks.is_empty() == false`，在合适位置（消息列表底部 / 侧边栏 / sticky header）渲染：

```rust
let widget = crate::components::tasks::TaskListV2Widget::new(&self.app_state.task_list.tasks);
f.render_widget(widget, area);
```

如果 `components::tasks::TaskListV2Widget` 不存在但 `TaskListV2`（数据结构）存在 → 需要在 components/tasks.rs 里加一个 `impl Widget for TaskListV2` 或新建一个 `TaskListV2Widget`。

### 验证

```bash
cargo build --workspace 2>&1 | tail -5
```

实际语义验证（要真 TTY 才完整）：

```bash
# oneshot 验证至少不 panic
MOSSEN_CODE_USE_CUSTOM_BACKEND=1 \
MOSSEN_CODE_CUSTOM_BASE_URL=http://localhost:8000 \
MOSSEN_CODE_CUSTOM_MODEL=deepseek-v4-flash \
./target/debug/mossen --oneshot "用 TodoWrite 添加三个任务：吃饭、睡觉、打豆豆" 2>&1 | tail -10
```

**完成判定**：build 过，oneshot 不 panic。完整渲染验证留到 Phase 4 TTY 烤机。

### 回滚

`git checkout -- crates/mossen-tui/src/app.rs crates/mossen-tui/src/state.rs crates/mossen-tui/src/components/tasks.rs`

---

## 3-2 Sub-Agent (Task tool) 输出 → teammate widget 接通

### 背景

`crates/mossen-tools/src/agent_tool/` 实现完整的 sub-agent 派生（含 6 个内置 agent），`crates/mossen-tui/src/components/tasks.rs:381` 和 `components/spinner_anim.rs` 实现了 `TeammateAssignment / SubAgentProvider / describe_teammate_activity`。**但当 Task tool 触发子 agent 跑起来时，TUI 完全感受不到 —— 不显示 spinner 树、不显示子 agent 当前在调啥工具、不显示子 agent 输出**。

### 位置

- agent_tool 进入点：`crates/mossen-tools/src/agent_tool/run_agent.rs`
- engine → TUI 通道：搜 `engine_tx` / `engine_rx` / `SdkMessage`
- TUI 端 teammate widget：`crates/mossen-tui/src/components/spinner_anim.rs:740+`
- 数据壳：`crates/mossen-tui/src/state.rs::foreground_task_id`

### 改动

#### Step 1：审计 engine ↔ TUI 通道

跑：

```bash
grep -rn "engine_tx\|engine_rx\|SdkMessage" crates/mossen-tui/src/app.rs crates/mossen-tools/src/agent_tool 2>/dev/null | head -15
```

理解 SdkMessage 是怎么从子 agent 冒泡到主 TUI 的，或者 **它根本没冒泡**（这是常见问题）。

#### Step 2：补 SdkMessage 携带 task_id

如果 SdkMessage 没有 task_id 字段，加上：

```rust
pub struct SdkMessage {
    pub task_id: Option<String>,  // None = 主 agent, Some(_) = sub agent
    // ... 其他字段
}
```

子 agent 派生时填上 task_id；主 agent 不填。

#### Step 3：在 app.rs::handle_engine_message 按 task_id 路由

```rust
match msg.task_id.as_deref() {
    None => {
        // 主 agent 消息，按原逻辑入主消息流
    }
    Some(tid) => {
        // 子 agent 消息：进入 state.teammate_messages.entry(tid).or_default().push(...)
        // 同时更新 state.teammate_states.entry(tid).or_insert(TeammateState::Running)
    }
}
```

#### Step 4：渲染 teammate spinner 树

在 render 路径里，如果 `app_state.teammate_states.len() > 0`，画 `components::spinner_anim::TeammateSpinnerTree` widget。

### 验证

```bash
cargo build --workspace 2>&1 | tail -5
```

**完成判定**：build 过，sub-agent oneshot 测试不 panic。完整渲染验证留到 Phase 4。

### 回滚

```bash
git checkout -- crates/mossen-tui/src/ crates/mossen-tools/src/agent_tool/
```

---

## 3-3 foreground_task_id 切换 + Ctrl+B 后台任务详情

### 背景

`state.rs:81 foreground_task_id` 存在但**没人切换它**。Ctrl+B 期望进入后台任务详情视图，但当前没接键路由。

### 位置

- `crates/mossen-tui/src/state.rs::AppState::foreground_task_id`
- `crates/mossen-tui/src/event.rs` 或 `app.rs::handle_key` 里 Ctrl+B 路由
- TUI 后台任务列表 widget：搜 `BackgroundTask\|background_task`

### 改动

#### Step 1：审计

```bash
grep -rn "foreground_task_id\|background_task\|Ctrl.B\|KeyCode.Char.'b'" crates/mossen-tui/src 2>/dev/null | head -10
```

#### Step 2：实现 Ctrl+B 切换 modal

如果当前 `app.rs::handle_key` 没处理 Ctrl+B：

```rust
KeyCode::Char('b') if modifiers.contains(KeyModifiers::CONTROL) => {
    if self.active_modal.is_none() {
        // 进入后台任务列表 modal
        let tasks = self.collect_background_tasks();
        self.active_modal = ActiveModal::Picker {
            kind: PickerKind::BackgroundTasks,
            title: "Background Tasks".into(),
            items: tasks.iter().map(|t| t.label.clone()).collect(),
            selected: 0,
        };
    }
}
```

如果 `PickerKind::BackgroundTasks` 没有，加这个变体（参考 `Theme / OutputStyle` 的写法）。

#### Step 3：选择后切换 foreground_task_id

picker 选定后：

```rust
self.app_state.foreground_task_id = Some(selected_task_id);
// 触发 redraw，让主屏专门展示这个 task 的消息流
```

### 验证

```bash
cargo build --workspace 2>&1 | tail -5
```

**完成判定**：build 过。键路由验证留到 Phase 4 TTY。

### 回滚

`git checkout -- crates/mossen-tui/src/`

---

## 3-4 流式 markdown 重解析性能基线测量 + 决策

### 背景

`crates/mossen-tui/src/widgets/markdown.rs:47` 用 pulldown-cmark + syntect 解析 markdown。Rust ratatui 是 immediate-mode，每个 token delta 到达后**重解析整个累积 buffer**。在长答复（10k token）+ 高 chunk 频率（30 t/s）下，可能成为 CPU 热点。

### 位置

- markdown widget：`crates/mossen-tui/src/widgets/markdown.rs`
- 调用方：`crates/mossen-tui/src/widgets/message.rs::render_streaming` 之类

### 改动

#### Step 1：写基线测量

`crates/mossen-tui/benches/markdown_streaming.rs`（如果还没有 benches 目录，先建）：

```rust
use criterion::{criterion_group, criterion_main, Criterion, black_box};
use mossen_tui::widgets::markdown::MarkdownWidget;

fn bench_streaming_reparse(c: &mut Criterion) {
    // 模拟 200 个 token delta，每个加上 ~50 字节，最终 10KB
    let chunks: Vec<String> = (0..200).map(|i| format!(" word{}", i)).collect();

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

Cargo.toml 加：

```toml
[dev-dependencies]
criterion = "0.5"

[[bench]]
name = "markdown_streaming"
harness = false
```

#### Step 2：跑基线

```bash
cargo bench -p mossen-tui --bench markdown_streaming 2>&1 | tail -20
```

记录单次完整流式过程的总耗时。

#### Step 3：决策

- 耗时 < 500ms（200 chunks）→ 不需要优化，本 task 结束
- 耗时 > 1s → 真的有 perf 问题，**停下报告**，让用户决定优化策略（incremental parser / 末行 only reparse / 缓存）

### 验证

```bash
ls -la crates/mossen-tui/benches/markdown_streaming.rs
cargo bench -p mossen-tui --bench markdown_streaming 2>&1 | tail -5
```

**完成判定**：bench 文件存在 + bench 跑出数据（不一定要快）。

### 回滚

```bash
rm crates/mossen-tui/benches/markdown_streaming.rs
git checkout -- crates/mossen-tui/Cargo.toml
```

---

## 3-5 Ctrl+C 中断协议：TurnState 状态机

### 背景

当前 `event::InputAction::Interrupt` 和 `engine_rx` 之间没有原子终止协议。如果在 `assistant_buf` 半截 + ToolUse 已 push 但 ToolResult 未到时按 Ctrl+C，下一个 `terminal.draw` 会渲染半截 markdown / 空 ToolResult 占位 —— **撕裂**。

### 位置

- `crates/mossen-tui/src/app.rs::handle_engine_message`（`assistant_buf` 累积）
- `crates/mossen-tui/src/event.rs::InputAction::Interrupt`

### 改动

#### Step 1：加 TurnState 枚举到 state.rs

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnState {
    Idle,        // 没在 turn 里
    Streaming,   // assistant 流式中
    Cancelling,  // 收到 Ctrl+C 但还没清干净
    Cancelled,   // 已 finalized，进入下个 prompt
}

// AppState 加字段：
pub turn_state: TurnState,
```

#### Step 2：在关键路径切换状态

- Stream 开始：`turn_state = Streaming`
- Stream 完成：`turn_state = Idle`
- Ctrl+C：`turn_state = Cancelling`
- finalize 完成：`turn_state = Cancelled` → `Idle`

#### Step 3：渲染时按状态画终止符

`render` 里：

```rust
if self.app_state.turn_state == TurnState::Cancelling {
    // 在 pending_assistant_idx 对应消息末尾画 "↯ 已取消" 终止符
    if let Some(idx) = self.pending_assistant_idx {
        // ... 在消息底部加一行 styled 标记
    }
}
```

### 验证

```bash
cargo build --workspace 2>&1 | tail -5
```

**完成判定**：build 过。撕裂消除需要 TTY 实测，留到 Phase 4。

### 回滚

`git checkout -- crates/mossen-tui/src/`

---

## Phase 3 验收

```bash
cargo build --workspace 2>&1 | tail -3
cargo test -p mossen-tui 2>&1 | tail -5
```

**Phase 3 完成判定**：
- build 0 error
- mossen-tui 测试至少不**新增**失败
- benches/markdown_streaming.rs 存在且能跑

向用户报告："Phase 3 完成。TodoWrite / sub-agent / Ctrl+C 的渲染管线已接通，性能基线已测。准备进 Phase 4 真机验收。"

---

# 🅿 Phase 4：生产验收

> 战场：测试套 + 真 TTY + 真实编码 sprint
> 目标：1057 测试全过；30 分钟无 panic；1 小时编码 sprint 至少完成 4/5 task

## 4-1 修 20 个失败测试

### 背景

`cargo test --workspace` 当前有 20 个失败：
- mossen-utils: 4 个（semver tilde / semver prerelease / json BOM / early_input escape）
- mossen-tools: 15 个（bash_security / destructive_warning / sed_validation / should_use_sandbox —— 全部根因是 regex backreference 不支持）
- mossen-skills: 1 个（dynamic::glob_matches_basic_and_double_star）

### 改动

#### 4-1-A mossen-utils 4 个

a. **semver::test_satisfies_tilde**：先 `cargo test -p mossen-utils semver -- --nocapture` 看完整断言，确认项目对 `~X.Y.Z` 是 tilde-minor（`>= X.Y.Z, < (X+1).0.0`）还是 tilde-patch（`>= X.Y.Z, < X.(Y+1).0`）。**和用户确认**后再改 `crates/mossen-utils/src/semver.rs::satisfies`。

b. **semver::test_prerelease**：和 a 一起修。

c. **json_read::test_strip_bom**：加 UTF-8 BOM 跳过。

```rust
const UTF8_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];
fn strip_bom_inplace(s: &mut String) {
    if s.as_bytes().starts_with(UTF8_BOM) {
        s.replace_range(..UTF8_BOM.len(), "");
    }
}
// 在 read_json / parse_json 入口先调用
```

d. **early_input::escape_sequence_dropped**：在 raw input 处理处加 ESC 序列丢弃状态机。

#### 4-1-B mossen-tools 15 个 bash 测试（regex backref）

所有 15 个测试根因是 `crates/mossen-tools/src/bash_tool/bash_security.rs:154` 用了 `\1` backreference。Rust `regex` crate 不支持。

**方案**：手写状态机替换 regex。例如 heredoc 检测：

```rust
fn contains_heredoc(src: &str) -> bool {
    let bytes = src.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'<' && bytes[i + 1] == b'<' {
            let mut j = i + 2;
            if j < bytes.len() && (bytes[j] == b'-' || bytes[j] == b'~') {
                j += 1;
            }
            let (delim, k) = parse_heredoc_delim(&bytes[j..]);
            if let Some(d) = delim {
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
```

逐个测试 `cargo test -p mossen-tools <test_name> -- --nocapture` 看输入 / 期望，让 contains_heredoc / classify_destructive 等手写函数满足。

#### 4-1-C mossen-skills 1 个

`crates/mossen-skills/src/dynamic.rs::glob_matches_basic_and_double_star` —— 看完整断言，用 `globset::GlobBuilder::new(p).literal_separator(false).build()` 改实现。

### 验证

```bash
cargo test --workspace --no-fail-fast 2>&1 | grep "test result:" | tail -20
```

**完成判定**：全部 `test result:` 行都是 `ok`，**0 failed**。

### 回滚

逐文件 `git checkout --`。

---

## 4-2 真 TTY 烤机 30 分钟

### 背景

前面 task 都没在真实 TTY 里跑 mossen TUI。生产级要求 30 分钟实际使用无 panic / 无静默卡死 / 渲染不撕裂。

### 步骤

⚠️ **本 task 需要用户手动跑**，自动执行的 agent 一般跑在非 TTY 环境。遇到本 task **停下报告**让用户接手。

用户操作：

```bash
cd /Users/allen/Documents/rustmossen
cargo build --workspace --release
MOSSEN_CODE_USE_CUSTOM_BACKEND=1 \
MOSSEN_CODE_CUSTOM_BASE_URL=http://localhost:8000 \
MOSSEN_CODE_CUSTOM_MODEL=deepseek-v4-flash \
RUST_LOG=mossen_agent=info,mossen_cli=info \
./target/release/mossen
```

完成以下 8 项验收，每项独立 turn：

- [ ] 简单问候（"你好"），回复正常
- [ ] Read 文件渲染（读 Cargo.toml）
- [ ] Bash + permission 弹窗（`ls`）
- [ ] Edit + diff 渲染（改 /tmp/mossen_smoke.txt）
- [ ] 多轮 tool（"找 src 下所有 .rs 文件并统计 unsafe 块数量"）
- [ ] TodoWrite 加 3 个任务（验证 3-1 接合）
- [ ] 用 Task tool 派生子 agent 跑一个 read-only 任务（验证 3-2 接合）
- [ ] 中途按 Ctrl+C 一次（验证 3-5 不撕裂）

每发现一个 bug 在 `/tmp/mossen_smoke_bugs.md` 记一行：`- [BUG] 描述 / 触发条件 / 现象`。

30 分钟结束后把 bug 清单交给用户。

### 完成判定

- 30 分钟无 panic / 无死锁 / 无渲染撕裂
- 8 项验收都 ✓
- bug 清单交付

---

## 4-3 1 小时真实编码 sprint 验收

### 背景

最终 production gate：用 mossen + ds4 在一个真实小项目里完成 5 个非平凡任务。

### 步骤

⚠️ 同 4-2，**用户手动**：

1. 选 / 创建一个真实小项目（100-300 行 Rust CLI 之类）
2. 用 `mossen` 在 1 小时内完成以下 5 个 task：
   - 加一个 subcommand
   - 写一个集成测试
   - 修一个真实 bug
   - 重构一个函数
   - 写文档
3. 全程录命令、回复、用时
4. 1 小时结束记账：完成几个 task / 平均每 task 多少 turn / mossen 自己崩了几次 / 模型答错多少次

### 完成判定

- 完成 ≥ 4/5 task
- mossen 自己崩 ≤ 1 次
- 平均每 task ≤ 10 turn

报告："Phase 4 完成。Mossen Rust v0.1.0 已达到 production-grade 验收。"

---

# 附录

## A. 失败回滚通用步骤

```bash
cd /Users/allen/Documents/rustmossen
git status                          # 看改了啥
git diff <文件>                     # 看具体改动
git checkout -- <文件>              # 单文件丢弃
```

完整回到 TS 删除前基线：

```bash
git log --oneline | head -10        # 找到 381932a baseline
git reset --hard 381932a            # ⚠️ 只在彻底重来时用
```

## B. 常用命令速查

| 任务 | 命令 |
|------|------|
| 增量编译某 crate | `cargo build -p mossen-cli` |
| 编译全部 | `cargo build --workspace` |
| 跑某 crate 测试 | `cargo test -p mossen-utils` |
| 跑某测试单条 | `cargo test -p mossen-tools bash_security::tests::test_safe_command -- --nocapture` |
| 看运行时日志 | `RUST_LOG=mossen_agent=trace ./target/debug/mossen ...` |
| ds4-server 健康检查 | `curl -sf http://localhost:8000/v1/models \| head -5` |
| 抓 mossen 发的实际请求 | `/tmp/mossen_capture.py`（已存在的代理工具） |
| benchmarks | `cargo bench -p mossen-tui` |

## C. 项目结构速查

```
/Users/allen/Documents/rustmossen/
├── crates/
│   ├── mossen-agent/         # agent loop / API client / provider / hooks / compact
│   ├── mossen-cli/           # 入口 main.rs / repl.rs / system_prompt.rs
│   ├── mossen-commands/      # slash 命令
│   ├── mossen-mcp/           # MCP 协议
│   ├── mossen-remote/        # 远程 mossen 连接
│   ├── mossen-skills/        # /skill 命令的 skill 加载
│   ├── mossen-tools/         # Bash / Read / Edit / Write / Agent / TodoWrite
│   ├── mossen-tui/           # ratatui 渲染
│   ├── mossen-types/         # 类型 + prompts.rs
│   └── mossen-utils/         # 通用 helper
├── scripts/                   # Python smoke / acceptance 测试
├── Cargo.toml                 # workspace
├── PLAN_PRODUCTION.md         # 本文档
└── README.md
```

## D. 关键文件 quick ref

| 改什么 | 去哪 |
|------|------|
| 默认 model 字符串 | `crates/mossen-cli/src/repl.rs` |
| system prompt 装配 | `crates/mossen-cli/src/system_prompt.rs` |
| system prompt 文本常量 | `crates/mossen-types/src/constants/prompts.rs` |
| agent 主循环 | `crates/mossen-agent/src/dialogue.rs` |
| hook 管理 | `crates/mossen-agent/src/hooks/` + `stop_hooks.rs` |
| context compact | `crates/mossen-agent/src/services/compact/` |
| API 客户端 / provider routing | `crates/mossen-agent/src/api_client.rs` |
| TUI 渲染主循环 | `crates/mossen-tui/src/app.rs` |
| TUI 状态 | `crates/mossen-tui/src/state.rs` |
| Sub-agent (Task tool) | `crates/mossen-tools/src/agent_tool/` |
| TodoWrite | `crates/mossen-tools/src/todo_write_tool/` |
| 子 agent UI 数据壳 | `crates/mossen-tui/src/components/tasks.rs` + `spinner_anim.rs` |
| 权限 | `crates/mossen-agent/src/...` + `crates/mossen-utils/src/permissions*` |

## E. 与用户的沟通规则

每个 task 前后**简短**（≤ 3 行）：

- **开始**：`正在做 X-Y：<标题>`
- **完成**：`X-Y 完成。验证通过：<关键输出>`
- **失败 / 卡住**：`X-Y 失败 / 卡住。详情：<错误片段>。已停下，等待指示。`

**不要长篇**。完成报告附上 `cargo build` / `cargo test` 最后几行作为证据。

## F. 审计文件清单

执行完 Phase 1-2 后，仓库会多出 4 份审计文件（不要随意删，是工程决策的依据）：

- `crates/mossen-agent/HOOK_AUDIT.md`（Phase 1-1）
- `crates/mossen-tools/src/agent_tool/ISOLATION_AUDIT.md`（Phase 1-5）
- `crates/mossen-agent/PERMISSION_CONTEXT_AUDIT.md`（Phase 2-1）
- `crates/mossen-cli/PLUGIN_RELOAD_AUDIT.md`（Phase 2-4）

每份审计在自己 task 的"完成判定"里有最小行数要求。

---

文档版本：v2.0（2026-05-19）
本计划由 Phase 0-4 22 个 task 组成，估计 2-3 周完成。
执行环境：本机 ds4-server (DeepSeek V4 Flash) + Mossen Rust 重写版
