# Phase 4：生产验收

> **目标读者**：本地 DeepSeek V4 Flash（通过 ds4-server 运行 Mossen 自身或其他 agent 来执行本文档）。
> **完整 5 阶段索引**：`/Users/allen/Documents/rustmossen/PLAN_PRODUCTION.md`
> **本阶段位置**：5 阶段中的第 5 阶段（Phase 4，最终）
> **前置**：Phase 0、Phase 1、Phase 2、Phase 3 均已完成。

---

## 1. 项目背景（必须读完再动手）

### 1.1 Mossen 是什么

**Mossen** 是 Rust 写的 coding agent CLI，对标 Claude Code。跑在本机，通过 `ds4-server`（localhost:8000）调用本地 DeepSeek V4 Flash 模型。

### 1.2 本阶段在整个计划中的位置

经过 Phase 0-3：
- 修了 5 个正确性硬伤
- 接通了 5 类 hook 调度
- 审计了 4 个子系统协同点
- 接通了 TUI 端 TodoWrite / sub-agent / Ctrl+C 渲染

现在到**生产验收**：
- 修掉所有测试失败（之前留下来的 20 个）
- 真 TTY 实地烤机 30 分钟
- 用真实项目跑 1 小时编程 sprint

完成这 3 件事 → Mossen Rust v0.1.0 达到 production-grade。

### 1.3 当前测试状态

`cargo test --workspace` 当前有 20 个失败，按 crate 分布：

| Crate | 失败数 | 根因 |
|-------|-------|------|
| mossen-utils | 4 | semver tilde / semver prerelease / json BOM strip / early_input escape sequence |
| mossen-tools | 15 | **全部根因相同**：bash_security.rs 用了 regex backreference `\1`，Rust regex crate 不支持 |
| mossen-skills | 1 | dynamic::glob_matches_basic_and_double_star（`**` 递归通配） |

mossen-tools 那 15 个看着多，但**一个 fix 修全部**（重写 heredoc / destructive 检测，不用 backreference）。

### 1.4 本阶段（Phase 4）要解决什么

3 个 task：

| Task | 一句话目标 | 谁来做 |
|------|----------|-------|
| 4-1 | 修 20 个失败测试 | agent 可以做 |
| 4-2 | 真 TTY 烤机 30 分钟 | **用户手动跑**（agent 跑在非 TTY 环境，没法做） |
| 4-3 | 1 小时真实编码 sprint 验收 | **用户手动跑** |

### 1.5 本阶段完成判定

- `cargo test --workspace` 全过，0 failed
- 真 TTY 30 分钟无 panic / 无静默卡死 / 无渲染撕裂
- 1 小时 sprint 完成 ≥ 4/5 task，mossen 自己崩 ≤ 1 次

### 1.6 本阶段不要做的事

- **不要**跳过验证（这是 production gate，每一条都得满足）
- **不要**因为某个 test 难修就改 test 来迁就实现（要改实现来满足 test 的合理预期）
- **不要**在 4-1 没全过之前进 4-2（测试失败的情况下烤机意义不大）

---

## 2. 阅读约定

### 2.1 角色与权限

你是 Rust 工程师。**可以**：用 `Read` / `Edit` / `Write` / `Bash`。
**绝对不能**：`git push` / `git reset --hard` / `rm -rf` / 修改 `/Users/allen/Documents/ds4/`。

### 2.2 执行节奏

一次一个 task。

### 2.3 卡住时

立即停下报告，不要猜。

### 2.4 命令前缀

默认 `cd /Users/allen/Documents/rustmossen`。

### 2.5 基线确认

```bash
cargo check --workspace 2>&1 | tail -3
```

### 2.6 沟通规则

- 开始：`正在做 4-X：<标题>`
- 完成：`4-X 完成。验证通过：<关键输出>`
- 失败：`4-X 失败 / 卡住。详情：<错误>。已停下。`

---

## 3. 本阶段任务清单

| Task | 标题 | 执行方 |
|------|------|-------|
| 4-1 | 修 20 个失败测试 | agent |
| 4-2 | 真 TTY 烤机 30 分钟 | 用户手动 |
| 4-3 | 1 小时真实编码 sprint 验收 | 用户手动 |

4-1 必须 0 failed 才能进 4-2。4-2 顺利才能进 4-3。

---

## 4. Task 详情

### 4-1 修 20 个失败测试

#### 背景

见 1.3。20 个失败按子任务拆成 3 组：4-1-A（mossen-utils 4 个）/ 4-1-B（mossen-tools 15 个）/ 4-1-C（mossen-skills 1 个）。

#### 改动

##### 4-1-A：mossen-utils 4 个

**a. semver::test_satisfies_tilde**

```bash
cargo test -p mossen-utils semver::test_satisfies_tilde -- --nocapture 2>&1 | tail -20
```

看完整断言列表（所有 (version, range, expected) 元组），确认项目对 `~X.Y.Z` 是 **tilde-minor**（`>= X.Y.Z, < (X+1).0.0`）还是 **tilde-patch**（`>= X.Y.Z, < X.(Y+1).0`）。

⚠️ **改之前和用户确认 tilde 语义**：

```
4-1-A.a：semver 的 ~X.Y.Z 应该是 tilde-minor 还是 tilde-patch？
（npm/yarn 标准是 tilde-patch，但测试断言可能要求 tilde-minor，必须问清楚）
```

确认后改 `crates/mossen-utils/src/semver.rs::satisfies`。

**b. semver::test_prerelease**

```bash
cargo test -p mossen-utils semver::test_prerelease -- --nocapture 2>&1 | tail -10
```

看断言，按预期修。可能和 a 一起修（同一个 satisfies 函数）。

**c. json_read::test_strip_bom**

加 UTF-8 BOM 跳过。在 `crates/mossen-utils/src/json_read.rs`：

```rust
const UTF8_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];

fn strip_bom_inplace(s: &mut String) {
    if s.as_bytes().starts_with(UTF8_BOM) {
        s.replace_range(..UTF8_BOM.len(), "");
    }
}

// 在 read_json / parse_json 入口先调用 strip_bom_inplace(&mut content)
```

**d. early_input::escape_sequence_dropped**

在 `crates/mossen-utils/src/early_input.rs` 处理 raw input 的位置加 ESC 序列识别 + 丢弃。

简单状态机：遇到 `0x1b`（ESC）后进入"丢弃模式"，吃掉后续 `[…<final byte>` 直到 final byte（`0x40-0x7e` 范围）为止。

```rust
let mut out = String::new();
let mut chars = input.chars().peekable();
while let Some(c) = chars.next() {
    if c == '\x1b' {
        // 跳过 [ 之后到 final byte
        if chars.peek() == Some(&'[') {
            chars.next(); // consume '['
            while let Some(&c) = chars.peek() {
                chars.next();
                if (c as u32) >= 0x40 && (c as u32) <= 0x7e { break; }
            }
        }
        continue;
    }
    out.push(c);
}
```

具体形态以测试断言为准。

**验证**：

```bash
cargo test -p mossen-utils 2>&1 | grep "test result:" | tail -3
```

期望：mossen-utils 全过（原 4 个失败都修好）。

##### 4-1-B：mossen-tools 15 个 bash 测试

**根因诊断**：

```bash
cargo test -p mossen-tools bash_tool::bash_security::tests::test_safe_command -- --nocapture 2>&1 | tail -20
```

会看到：

```
regex parse error:
    <<[-~]?['"](\w+)['"].*?\n([\s\S]*?)\n\1
                                         ^^
error: backreferences are not supported
```

`crates/mossen-tools/src/bash_tool/bash_security.rs:154` 附近用了 `\1` backref。Rust 标准 `regex` crate 是 finite-automaton 引擎，不支持 backref（NP-hard）。

**方案**：手写状态机替换 regex。例：heredoc 检测：

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

fn parse_heredoc_delim(rest: &[u8]) -> (Option<String>, usize) {
    // 1. 跳过空白
    let mut i = 0;
    while i < rest.len() && (rest[i] == b' ' || rest[i] == b'\t') { i += 1; }
    if i >= rest.len() { return (None, 0); }
    // 2. 看是否被 ' " 包围
    let (delim, end) = if rest[i] == b'\'' || rest[i] == b'"' {
        let q = rest[i];
        let start = i + 1;
        let mut end = start;
        while end < rest.len() && rest[end] != q { end += 1; }
        if end >= rest.len() { return (None, 0); }
        (String::from_utf8_lossy(&rest[start..end]).to_string(), end + 1)
    } else {
        let start = i;
        let mut end = start;
        while end < rest.len() && (rest[end].is_ascii_alphanumeric() || rest[end] == b'_') { end += 1; }
        (String::from_utf8_lossy(&rest[start..end]).to_string(), end)
    };
    if delim.is_empty() { (None, 0) } else { (Some(delim), end) }
}

fn heredoc_terminator_found(after: &str, delim: &str) -> bool {
    for line in after.lines() {
        if line.trim_start_matches(['\t', ' ']) == delim {
            return true;
        }
    }
    false
}
```

具体函数体根据 15 个测试的输入推断。**先把每个失败测试用 `cargo test ... -- --nocapture` 跑一遍**看清楚每个测试期望什么输入触发什么判定，再实现。

不止 heredoc，还有 destructive_command_warning、sed_validation、should_use_sandbox 等其他用到 regex backref 的位置。每处都改成手写状态机或纯字符串匹配。

**验证**：

```bash
cargo test -p mossen-tools bash_tool:: 2>&1 | tail -10
```

期望：mossen-tools 全过（原 15 个失败都修好）。

##### 4-1-C：mossen-skills 1 个

```bash
cargo test -p mossen-skills glob_matches_basic_and_double_star -- --nocapture 2>&1 | tail -10
```

看完整断言列表。问题在 `crates/mossen-skills/src/dynamic.rs` 的 glob 匹配实现 —— 特别是 `**` 递归通配符的处理。

**方案**：换成 `globset` 标准库：

```rust
use globset::GlobBuilder;

fn matches(pattern: &str, path: &str) -> bool {
    GlobBuilder::new(pattern)
        .literal_separator(false)  // 让 ** 跨 / 匹配
        .build()
        .ok()
        .map(|g| g.compile_matcher().is_match(path))
        .unwrap_or(false)
}
```

`literal_separator(false)` 是关键，让 `**` 真的递归跨目录。

**验证**：

```bash
cargo test -p mossen-skills 2>&1 | tail -5
```

期望：mossen-skills 全过。

#### 总验证

```bash
cd /Users/allen/Documents/rustmossen
cargo test --workspace --no-fail-fast 2>&1 | grep "test result:" | tail -20
```

#### 完成判定

**全部 `test result:` 行都是 `ok`**，**0 failed**。

#### 回滚

逐文件 `git checkout --`。

---

### 4-2 真 TTY 烤机 30 分钟

#### 背景

前面所有 task 都没在真实 TTY 里跑 mossen 的 TUI。生产级要求实际用 30 分钟无 panic / 无静默卡死 / 无渲染撕裂。

#### 谁来做

⚠️ **本 task 用户手动跑，agent 跑在非 TTY 环境做不了**。遇到本 task **停下报告**让用户接手。

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

完成以下 8 项验收，每项独立 turn：

- [ ] 简单问候（"你好"），回复正常
- [ ] Read 文件渲染（让它读 `Cargo.toml`）
- [ ] Bash + permission 弹窗（让它跑 `ls`）
- [ ] Edit + diff 渲染（让它改 `/tmp/mossen_smoke.txt`）
- [ ] 多轮 tool（"找 src 下所有 .rs 文件并统计 unsafe 块数量"）
- [ ] TodoWrite 加 3 个任务（验证 Phase 3-1 接合）
- [ ] 用 Task tool 派生子 agent 跑一个 read-only 任务（验证 Phase 3-2 接合）
- [ ] 中途按 Ctrl+C 一次（验证 Phase 3-5 不撕裂）

每发现一个 bug，在 `/tmp/mossen_smoke_bugs.md` 记一行：

```
- [BUG] 描述 / 触发条件 / 现象
```

30 分钟结束后把 bug 清单交给用户。

#### 完成判定

- 30 分钟无 panic / 无死锁 / 无渲染撕裂
- 8 项验收都 ✓
- bug 清单已交付

---

### 4-3 1 小时真实编码 sprint 验收

#### 背景

最终 production gate：用 mossen + ds4 在一个真实小项目里完成 5 个非平凡任务。

#### 谁来做

⚠️ 同 4-2，**用户手动**。

#### 用户操作

1. 选或创建一个真实小项目（100-300 行 Rust CLI 之类）
2. 用 `mossen` 在 1 小时内完成以下 5 个 task：
   - 加一个 subcommand
   - 写一个集成测试
   - 修一个真实 bug
   - 重构一个函数
   - 写文档
3. 全程录命令、回复、用时
4. 1 小时结束记账：
   - 完成几个 task
   - 平均每个 task 多少 turn
   - mossen 自己崩了几次
   - 模型答错多少次需要纠正

#### 完成判定

- 完成 ≥ 4/5 task
- mossen 自己崩 ≤ 1 次
- 平均每 task ≤ 10 turn

报告："Phase 4 完成。Mossen Rust v0.1.0 已达到 production-grade 验收。"

---

## 5. Phase 4 阶段验收

3 个 task 都完成后：

```bash
cd /Users/allen/Documents/rustmossen
cargo test --workspace 2>&1 | grep "test result:" | tail -20
```

**完成判定**：

- 全部 `test result:` 行都是 `ok`，0 failed
- 4-2 烤机 8 项验收都过
- 4-3 sprint 4-5/5 task 完成

最终向用户报告：

> Phase 4 完成。Mossen Rust v0.1.0 production-grade 验收通过。
>
> - 测试：全过
> - TTY 30min 烤机：8/8 ✓
> - 编码 sprint：X/5 task 完成
> - Mossen 自身崩次数：N

整个 5 阶段计划完成。

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
| 全 workspace 测试 | `cargo test --workspace` |
| 跑单个测试看输出 | `cargo test -p mossen-tools bash_security::tests::test_safe_command -- --nocapture` |
| Release 构建 | `cargo build --workspace --release` |
| 真 TTY 启动 | `./target/release/mossen`（需 env 变量见 4-2） |

### C. 失败测试快速 ref

| Crate | Test | Fix 类别 |
|-------|------|---------|
| mossen-utils | semver::test_satisfies_tilde | 实现 tilde semantics（**先问用户**） |
| mossen-utils | semver::test_prerelease | 同上 |
| mossen-utils | json_read::test_strip_bom | 加 BOM 跳过 |
| mossen-utils | early_input::escape_sequence_dropped | ESC 序列丢弃状态机 |
| mossen-tools | bash_security::* (8 个) | 手写状态机替 regex backref |
| mossen-tools | destructive_command_warning::* (5 个) | 同上 |
| mossen-tools | sed_validation::* | 同上 |
| mossen-tools | should_use_sandbox::* | 同上 |
| mossen-skills | dynamic::glob_matches_basic_and_double_star | 换用 globset `**` 处理 |

---

文档版本：v2.1（拆分版）
**完整 5 阶段计划见 `PLAN_PRODUCTION.md`**
