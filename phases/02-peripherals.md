# Phase 2：外围 5 系统接合

> **目标读者**：本地 DeepSeek V4 Flash（通过 ds4-server 运行 Mossen 自身或其他 agent 来执行本文档）。
> **完整 5 阶段索引**：`/Users/allen/Documents/rustmossen/PLAN_PRODUCTION.md`
> **本阶段位置**：5 阶段中的第 3 阶段（Phase 2）
> **前置**：Phase 0 和 Phase 1 已完成（特别是 Phase 1-1 产出的 HOOK_AUDIT.md 是本阶段一些 task 的基础）。

---

## 1. 项目背景（必须读完再动手）

### 1.1 Mossen 是什么

**Mossen** 是 Rust 写的 coding agent CLI，对标 Mossen Code。跑在本机，通过 `ds4-server`（localhost:8000）调用本地 DeepSeek V4 Flash 模型。

### 1.2 什么是 "外围 5 系统"

除了 harness 主循环之外，coding agent 还有 5 大支撑系统，**它们之间需要协同**：

| 系统 | 作用 | Rust 端 crate |
|------|------|--------------|
| **Memory（记忆）** | 跨会话持久化用户偏好、项目状态、纠错反馈 | `mossen-utils/src/...memory*` |
| **Skill（技能）** | 把 Markdown SKILL.md 加载为模型可调用的命令 | `mossen-skills` |
| **MCP（Model Context Protocol）** | 通过协议连接外部服务，整合远程工具 | `mossen-mcp` |
| **Plugin（插件）** | 第三方扩展（打包 skill + command + hook + MCP） | `mossen-cli` 内 |
| **Permission（权限）** | 决定每次工具调用是 allow/deny/ask | `mossen-agent` + `mossen-utils` |

### 1.3 这 5 个系统为什么需要 "接合"

在 TS 原版里，这 5 个系统通过**共同的运行态数据结构** `toolPermissionContext` 协同：

```
skill loader      ─┐
MCP channelAllow  ─┤
plugin policy     ─┼──→ 写入同一份 toolPermissionContext
settings.json     ─┤        { alwaysAllowRules, denyRules,
permission dialog ─┘          askRules, disallowedTools }
```

所有权限规则**单一可变源**。任何系统改了规则，整个 agent 立刻看到一致状态。

**Rust 端的风险**：如果各子系统持有自己的 context 副本，规则就会四散，用户在 skill 里 allow 一个工具，下次 MCP 还是会去问。这是本阶段要审计 + 修补的事。

另外还有一些**事件总线协同**问题：

- 装/卸 plugin 应当触发：清 skill 缓存 → 重扫 skill → 通知 TUI → MCP 重连
- 工具产生新文件路径应当触发：动态发现 skill 目录 → 激活条件 skill
- 这些事件链如果没串起来，用户看到的工具列表会"陈旧"

### 1.4 Mossen 当前外围系统状态

| 系统 | Rust 端代码完整度 | 接合状态 |
|------|------------------|---------|
| Memory | 完整（utils 模块） | 写入端是否单源未知 |
| Skill | 完整（mossen-skills + dynamic.rs） | 动态发现钩子可能未接 |
| MCP | 完整（mossen-mcp） | reload 联动可能缺 |
| Plugin | 完整 | reload 事件总线可能缺 |
| Permission | 完整（7 步决策树） | toolPermissionContext 单源未审计 |

5 个系统各自 ≥ 95% 完成，但**协同点**没人审计过。本阶段就是审计 + 补关键协同。

### 1.5 本阶段（Phase 2）要解决什么

**4 个 task，其中 2 个是审计任务**（产出 markdown），2 个是接合 task（基于审计结论决定改还是不改）：

| Task | 一句话目标 |
|------|-----------|
| 2-1 | **审计**：`toolPermissionContext` 单一写入端 → 产出 PERMISSION_CONTEXT_AUDIT.md |
| 2-2 | Skill discovery 钩子接合（工具调用产生新路径时动态扫 .mossen/skills/） |
| 2-3 | Conditional skill activation（按 paths frontmatter 激活条件 skill） |
| 2-4 | **审计**：Plugin reload 联动（清 skill / 重扫 / MCP 重连）→ 产出 PLUGIN_RELOAD_AUDIT.md |

### 1.6 本阶段完成判定

跑完 4 个 task 后：
- workspace 编译 0 error
- workspace 测试不**新增**失败（原有的 20 个测试失败留到 Phase 4 修）
- 4 份审计文件齐：HOOK_AUDIT.md / ISOLATION_AUDIT.md（Phase 1）+ PERMISSION_CONTEXT_AUDIT.md / PLUGIN_RELOAD_AUDIT.md（本阶段）

### 1.7 本阶段不要做的事

- **不要**在没有审计结论时盲改子系统间的规则同步逻辑（错改会污染权限模型）
- **不要**重构 5 个系统的内部实现（本阶段只关心**协同点**）
- **不要**重写 EventBus（如果没有，先审计完看用户决定）
- **不要**修测试失败（那是 Phase 4）

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

- 开始：`正在做 2-X：<标题>`
- 完成：`2-X 完成。验证通过：<关键输出>`
- 失败：`2-X 失败 / 卡住。详情：<错误>。已停下。`

---

## 3. 本阶段任务清单

| Task | 标题 | 产出 |
|------|------|------|
| 2-1 | toolPermissionContext 单一写入端审计 | `crates/mossen-agent/PERMISSION_CONTEXT_AUDIT.md` |
| 2-2 | Skill discovery hook 接合 | 代码改动（如果功能存在但未接） |
| 2-3 | Conditional skill activation 接合 | 代码改动 |
| 2-4 | Plugin install / reload → skill / MCP 联动审计 | `crates/mossen-cli/PLUGIN_RELOAD_AUDIT.md` |

2-1 先做。2-2 / 2-3 可并行（但仍按顺序，不要并发）。2-4 最后。

---

## 4. Task 详情

### 2-1 toolPermissionContext 单一写入端审计

#### 背景

权限上下文 `toolPermissionContext` 是 agent 的**运行态权限规则汇总**，包含：
- `alwaysAllowRules`：允许的工具/命令
- `denyRules`：拒绝的工具/命令
- `askRules`：每次询问的工具/命令
- `disallowedTools`：完全禁用的工具
- `mode`：默认权限模式（default / acceptEdits / bypassPermissions / plan / 等）

在 TS 端，**5 个系统全部写入同一份**：skill `allowed-tools` 注入 → MCP channelAllowlist → plugin policy → settings.json 启动加载 → 用户在弹窗选「Always allow」。

Rust 端如果每个系统各自持有 context 副本（比如 skill loader 写自己的、MCP 写自己的），规则就会四散，用户看到的权限行为不一致。

本 task **审计**这件事。**不改代码**。

#### 位置

grep 起点：

```bash
grep -rln "toolPermissionContext\|tool_permission_context\|ToolPermissionContext\|alwaysAllowRules\|always_allow_rules" crates/ 2>/dev/null
```

预期至少 5-10 文件命中。

#### 改动

产出 `crates/mossen-agent/PERMISSION_CONTEXT_AUDIT.md`：

```markdown
# ToolPermissionContext 写入审计 / Phase 2-1

## 结构定义

- 类型定义 file:line：
- 字段清单：（alwaysAllowRules / denyRules / askRules / disallowedTools / mode / 其他）

## 所有写入位置

| 写入者 | file:line | 写哪个字段 | 触发场景 |
|---|---|---|---|
| skill loader | ... | alwaysAllowRules.command | skill `allowed-tools` 注入 |
| MCP channelAllowlist | ... | ... | 项目级 MCP 首次连接审批 |
| plugin install | ... | ... | 装 plugin 时 |
| permission dialog (Always allow) | ... | ... | 用户在弹窗选「Always allow」 |
| settings.json loader | ... | ... | 启动时加载 |
| ... | ... | ... | ... |

## 是否有"单一可变源"

判定：（是 / 否 / 多个副本互相不同步）
证据：
分析：

## 若有多副本：建议合并方案

（例 1：把 X / Y / Z 都改成读 `&AppState.permission_context`，不要各自持有副本）
（例 2：引入 `Arc<RwLock<PermissionContext>>` 由所有系统共享）
（例 3：用 channel 同步副本 —— 最差方案，仅作 fallback）
```

#### 验证

```bash
ls -la crates/mossen-agent/PERMISSION_CONTEXT_AUDIT.md
wc -l crates/mossen-agent/PERMISSION_CONTEXT_AUDIT.md
```

#### 完成判定

- 文件存在
- 行数 ≥ 40
- 包含「是否单一可变源」明确判定（不能模棱两可）

#### 回滚

```bash
rm crates/mossen-agent/PERMISSION_CONTEXT_AUDIT.md
```

---

### 2-2 Skill discovery hook 接合

#### 背景

TS 端 `discoverSkillDirsForPaths()` 在每个工具调用产生新文件路径时被触发：向上扫描 `.mossen/skills/` 目录，发现新 skill 就动态加载。这让"在某个项目里有特定 skill"成为可能。

Rust 端 `mossen-skills` 已加载初始 skill 集（启动时扫一次），但**这个动态发现钩子**可能没接。如果没接，用户在 agent 运行中往项目里加 skill 文件，agent 不会发现。

#### 位置

grep 起点：

```bash
grep -rln "discover_skill\|discoverSkillDirs\|activate_conditional\|conditional_skills" crates/mossen-skills/src crates/mossen-agent/src crates/mossen-tools/src 2>/dev/null
```

#### 改动

##### Step 1：审计当前状态

跑上面的 grep，列出：
- 找到的函数（discover_skill_dirs_for_paths 或类似名字）→ 它在哪个文件
- 被哪些位置调用 → 列调用方
- 没找到 → 标记为「功能不存在」

##### Step 2：决定接合策略

**情况 A：函数存在但未在 dialogue 调用**

在 dialogue.rs 的 tool result 处理后（每个工具调用结束、产生了新文件路径之后）调用：

```rust
// 在 tool result 拼好、回灌进下一轮 messages 之前
let new_paths = collect_paths_from_tool_results(&tool_results);
if !new_paths.is_empty() {
    if let Some(discovered) = mossen_skills::discover_skill_dirs_for_paths(&new_paths).await {
        // 把新发现的 skill 注册到 AppState.skills
    }
}
```

具体 API 形态以审计找到的真实函数为准。

**情况 B：函数完全不存在**

**停下报告**。这个功能比较复杂（涉及 skill 加载、缓存失效、TUI 通知刷新），需要用户决定要不要本 sprint 内做。

#### 验证

```bash
cd /Users/allen/Documents/rustmossen
cargo build --workspace 2>&1 | tail -3
```

#### 完成判定

- build 过
- 至少在 dialogue 或 tool dispatcher 里能 grep 到 `discover_skill` 字样的调用（如果情况 A）
- 或者审计报告说「函数不存在，已停下等用户」（如果情况 B）

#### 回滚

```bash
git diff | head -20   # 看改了啥
git checkout -- <文件路径>
```

---

### 2-3 Conditional skill activation 接合

#### 背景

某些 skill 的 SKILL.md frontmatter 里写：

```yaml
---
name: ts-react-helper
paths: ['**/*.tsx', '**/*.jsx']
---
```

意思是：**仅当当前工作目录或最近修改文件匹配这些 pattern 时，该 skill 才激活**。

Rust 端 `mossen-skills/src/dynamic.rs` 应该有这套（注意：该文件还有 1 个 glob 测试失败，留到 Phase 4 修）。本 task 检查 conditional activation 是否在合适触发点被调用。

#### 位置

```bash
grep -n "conditional\|Conditional\|paths_match\|activate" crates/mossen-skills/src/dynamic.rs 2>/dev/null | head -10
```

#### 改动

##### Step 1：审计

- 列出 `dynamic.rs` 里 conditional-related 函数（应该有 `activate_conditional_skills_for_paths` 或类似名字）
- 看是否在 tool dispatcher / skill loader 流程里被调用

##### Step 2：接合

参照 2-2 的逻辑：
- 函数存在但未调用 → 在合适触发点（cwd 变化 / file 修改 / 启动时）调用
- 完全不存在 → 停下报告

合适触发点（举例）：

- 启动时（mossen-cli main 里）扫一次
- 用户切换 cwd（`/cd` 命令）时
- 工具调用产生新文件路径时（与 2-2 同款触发）

#### 验证

```bash
cd /Users/allen/Documents/rustmossen
cargo build -p mossen-skills 2>&1 | tail -3
cargo test -p mossen-skills 2>&1 | tail -5
```

#### 完成判定

- build 过
- mossen-skills 测试至少不**新增**失败（原本 glob 测试失败留到 Phase 4 修）

#### 回滚

```bash
git checkout -- crates/mossen-skills/
```

---

### 2-4 Plugin install / reload → skill / MCP 联动审计

#### 背景

TS 端 `/reload-plugins` 命令触发的下游事件链：

```
清 skill 缓存
  ↓
重扫 skill 目录（含 plugin 提供的）
  ↓
emit skillsLoaded 信号
  ↓
TUI 刷新工具列表
  ↓
MCP 服务器重连（plugin 可能注入了 MCP 配置）
```

如果 Rust 端这个事件链没串起来，用户装/卸 plugin 后看到的状态会**陈旧**（旧 skill 还在、TUI 没更新）。

本 task **只审计**，不动代码。

#### 位置

```bash
grep -rln "reload_plugin\|reloadPlugin\|skillsLoaded\|skills_loaded\|plugin_reload" crates/ 2>/dev/null | head -10
```

#### 改动

产出 `crates/mossen-cli/PLUGIN_RELOAD_AUDIT.md`：

```markdown
# Plugin Reload 联动审计 / Phase 2-4

## /reload-plugins 命令实现位置

file:line：
触发哪个函数：

## reload 后触发的下游事件清单

- [ ] 清 skill 缓存：（是 / 否，证据 file:line）
- [ ] 重扫 skill 目录（含 plugin skill）：（是 / 否，证据）
- [ ] emit skillsLoaded 信号 / 通知 TUI 刷新工具列表：（是 / 否，证据）
- [ ] MCP 重连：（是 / 否，证据）

## 是否有 EventBus / 信号机制

判定：
位置：
分析：（如果有，是什么形态？channel? watch? broadcast?；如果没有，建议用什么）

## 缺失项 fix 提议

（针对每个未联动的下游事件，给具体 fix 方案：在哪个文件加 emit、谁监听）

## 整体结论

- 全联动 → 完成
- 缺联动 → **停下报告**，让用户决定补哪些
```

#### 验证

```bash
ls -la crates/mossen-cli/PLUGIN_RELOAD_AUDIT.md
```

#### 完成判定

- 文件存在
- 4 个下游事件都有明确判定（不能模糊）

#### 回滚

```bash
rm crates/mossen-cli/PLUGIN_RELOAD_AUDIT.md
```

---

## 5. Phase 2 阶段验收

```bash
cd /Users/allen/Documents/rustmossen
cargo build --workspace 2>&1 | tail -3
cargo test --workspace --no-fail-fast 2>&1 | grep "test result:" | tail -10
ls crates/mossen-agent/HOOK_AUDIT.md \
   crates/mossen-tools/src/agent_tool/ISOLATION_AUDIT.md \
   crates/mossen-agent/PERMISSION_CONTEXT_AUDIT.md \
   crates/mossen-cli/PLUGIN_RELOAD_AUDIT.md 2>&1
```

**Phase 2 完成判定**：

- workspace 0 error
- 测试结果与 Phase 0 结束时基本一致（不新增失败）
- 4 份审计文件都存在（Phase 1 的 2 份 + Phase 2 的 2 份）

向用户报告：

> Phase 2 完成。4 份审计文件已生成：HOOK_AUDIT / ISOLATION_AUDIT / PERMISSION_CONTEXT_AUDIT / PLUGIN_RELOAD_AUDIT。可 review 后进 Phase 3（渲染层 gap 接合）。

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
| 编译某 crate | `cargo build -p mossen-skills` |
| 全 workspace | `cargo build --workspace` |
| 跑测试 | `cargo test -p mossen-skills` |
| 全测试不停止 | `cargo test --workspace --no-fail-fast` |

### C. 本阶段关键文件 quick ref

| 系统 | 主要文件 |
|------|---------|
| Permission Context | 搜 `crates/mossen-agent/src/...permission*` |
| Skill 加载 | `crates/mossen-skills/src/` |
| Skill 动态发现 | `crates/mossen-skills/src/dynamic.rs` |
| Plugin 管理 | `crates/mossen-cli/src/...plugin*` |
| MCP | `crates/mossen-mcp/src/` |

---

文档版本：v2.1（拆分版）
**完整 5 阶段计划见 `PLAN_PRODUCTION.md`**
