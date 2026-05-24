# Mossen Rust 生产化实施计划（v2.1 · 拆分索引版）

> **目标读者**：本地 DeepSeek V4 Flash（通过 ds4-server 跑 Mossen 自身或其他 agent 执行各 phase 文档）
> **目标**：把 `/Users/allen/Documents/rustmossen` 从 **80%「能编译能跑 oneshot」** 推进到 **生产可用 multi-turn TUI** —— 重点是把已有的零件**接起来**，不是重写
> **预计工期**：约 25-40 个交互轮次 / 2-3 周
> **生效环境**：macOS / Apple Silicon / ds4-server 在 localhost:8000 监听

---

## 📖 怎么用这套文档

本计划被拆成 **5 个独立的 phase 文档**，每个 phase 都是自洽的执行单元（含完整背景、阅读约定、任务详情、验证、回滚）。

**一次只读一个 phase 文件**。读完一份做完一份，再进下一份。

| 阶段 | 文件 | 任务数 | 工期 | 一句话目标 |
|------|------|------|------|----------|
| **Phase 0** 正确性硬伤 | [`phases/00-correctness.md`](phases/00-correctness.md) | 5 | 1-2 天 | 模型身份正确、MOSSEN.md 完整 import、默认值合理 |
| **Phase 1** Harness 融合 | [`phases/01-harness.md`](phases/01-harness.md) | 5 | 3-5 天 | 5 类 hook 全部 wire；子 agent 隔离审计 |
| **Phase 2** 外围 5 系统接合 | [`phases/02-peripherals.md`](phases/02-peripherals.md) | 4 | 3-5 天 | `toolPermissionContext` 单一写入端；skill 动态发现；plugin 联动 |
| **Phase 3** 渲染层 gap 接合 | [`phases/03-rendering.md`](phases/03-rendering.md) | 5 | 3-5 天 | TodoWrite 实时渲染；sub-agent 输出占位；Ctrl+C 不撕裂；perf 基线 |
| **Phase 4** 生产验收 | [`phases/04-acceptance.md`](phases/04-acceptance.md) | 3 | 2-3 天 | 测试全过；30 min TTY 烤机；1h 真实编码 sprint |

**总计 22 task，约 2-3 周**。

---

## 🗺 整体地图（项目当前状态速览）

```
                  ✅ 已实现且在用
                  🟡 已实现但未完全接通（数据壳 / 缺驱动 / 缺调度）
                  ⚪ 部分实现 / 边角缺失

mossen-cli          ✅ main.rs / repl.rs / system_prompt.rs / handlers/
                    ⚪ 默认模型字符串硬编码、MOSSEN.md @-include 没展开
                       → Phase 0

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
├─ api_client.rs    ✅ OpenAI-compat 95% + Provider 80%
└─ stop_hooks.rs    ✅ wire 到 dialogue.rs:451
                       → Phase 1

mossen-tools
├─ agent_tool/      ✅ run_agent / resume_agent / fork_subagent
│   └─ built_in/    ✅ 6 个内置 agent (general/plan/explore/...)
│                   ❓ transcript 隔离 3 个不变量未审计
├─ todo_write_tool/ ✅ 工具实现完整
│                   🟡 TUI 端没监听其 result 来更新 TaskListV2
├─ bash_tool/       ✅ + ⚪ 15 个 bash_security 测试因 regex backref 失败
└─ shared/spawn_multi_agent.rs ✅
                       → Phase 1 / Phase 3 / Phase 4

mossen-tui
├─ app.rs           ✅ 主 event loop / Modal dispatcher / 状态机
├─ widgets/         ✅ 实际在用的 widget 树（messages/markdown/spinner）
├─ components/      ✅ 部分在用（dialogs / permissions / misc / root_large）
│                   🟡 tasks.rs (TaskListV2 / SubAgentProvider) 已写但没人 emit 事件驱动
│                   🟡 spinner_anim.rs (teammate spinner) 已写但没接
├─ terminal-framework/             ⚪ 6934 行平移自 TS terminal framework，未挂入实际渲染路径（不要删，留着备用）
└─ state.rs         🟡 foreground_task_id 在但少配套字段
                       → Phase 3

mossen-skills / mossen-mcp / mossen-utils / mossen-types
                    ✅ 大致到位
                    ⚪ 少数测试失败（semver/json/escape）
                       → Phase 4
```

### 两大融合战场

**Harness 战场**（Phase 1 + Phase 2）：dialogue.rs 是核心，已经能跑一圈。差的是：
- 其他 5 类 hook（pre/post-compact、post-sampling、session-start、task-completed）的调度点未插齐
- 3 层 compact 瀑布之间的协作 + 断路器状态机
- 子 agent transcript 隔离审计（task_local AgentContext）
- 5 大外围系统对 `toolPermissionContext` 写入收口

**渲染战场**（Phase 3）：app.rs 是核心，已经能流式显示。差的是：
- TodoWrite 工具改动 → TaskListV2 widget 渲染（widget 在 components/tasks.rs:376，**没被 app.rs 调用**）
- Task tool / sub-agent 产生的 nested SdkMessage → teammate 渲染
- foreground_task_id 切换 / Ctrl+B 进入后台任务详情
- 流式 markdown 在长答复下的重解析性能（可能有 O(n²) 风险，需测）
- Ctrl+C 中断时的渲染撕裂（无 TurnState 状态机）

---

## 🎯 喂给小模型的标准方法

每次只让模型读 **一个** phase 文件 + 让它执行那一个 phase。**不要一次喂整个 PLAN_PRODUCTION.md**（会触发"反复 Read 拿不到 Phase X 内容"的循环 bug）。

示例：

```bash
# 让 mossen / deepseek-tui 执行 Phase 0：
"/Users/allen/Documents/rustmossen/phases/00-correctness.md 执行 phase0"

# Phase 0 全部完成、用户确认后：
"/Users/allen/Documents/rustmossen/phases/01-harness.md 执行 phase1"

# 以此类推
```

每个 phase 文件都自洽，包含：
- 项目背景（Mossen 是什么、当前状态）
- 本阶段使命
- 本阶段不要做的事（防止越界）
- 阅读约定（角色、节奏、卡住时怎么办、沟通规则）
- 任务列表 + 每个 task 5 段详情
- 阶段验收
- 附录（回滚、命令、文件 quick ref）

---

## 📋 关键文件总览

| 改什么 | 去哪 | 哪个 phase 涉及 |
|------|------|---------------|
| 默认 model 字符串 | `crates/mossen-cli/src/repl.rs` | Phase 0 |
| system prompt 装配 | `crates/mossen-cli/src/system_prompt.rs` | Phase 0 |
| system prompt 文本常量 | `crates/mossen-types/src/constants/prompts.rs` | Phase 0 |
| agent 主循环 | `crates/mossen-agent/src/dialogue.rs` | Phase 0 / Phase 1 |
| hook 管理 | `crates/mossen-agent/src/hooks/` + `stop_hooks.rs` | Phase 1 |
| context compact | `crates/mossen-agent/src/services/compact/` | Phase 1 |
| API 客户端 / provider routing | `crates/mossen-agent/src/api_client.rs` | Phase 0 |
| TUI 渲染主循环 | `crates/mossen-tui/src/app.rs` | Phase 3 |
| TUI 状态 | `crates/mossen-tui/src/state.rs` | Phase 3 |
| Sub-agent (Task tool) | `crates/mossen-tools/src/agent_tool/` | Phase 1 / Phase 3 |
| TodoWrite | `crates/mossen-tools/src/todo_write_tool/` | Phase 3 |
| 子 agent UI 数据壳 | `crates/mossen-tui/src/components/tasks.rs` + `spinner_anim.rs` | Phase 3 |
| 权限 | `crates/mossen-agent/src/...` + `crates/mossen-utils/src/permissions*` | Phase 2 |

---

## 📂 审计文件清单

执行完 Phase 1-2 后会生成 4 份审计文件（不是代码改动，是工程决策依据，不要随意删）：

| 文件 | 来自 |
|------|------|
| `crates/mossen-agent/HOOK_AUDIT.md` | Phase 1-1 |
| `crates/mossen-tools/src/agent_tool/ISOLATION_AUDIT.md` | Phase 1-5 |
| `crates/mossen-agent/PERMISSION_CONTEXT_AUDIT.md` | Phase 2-1 |
| `crates/mossen-cli/PLUGIN_RELOAD_AUDIT.md` | Phase 2-4 |

---

## 🛡 失败回滚

任意 task 失败后：

```bash
cd /Users/allen/Documents/rustmossen
git status                          # 看改了啥
git diff <文件>                     # 看具体改动
git checkout -- <文件>              # 单文件丢弃改动
```

完整回到 TS 删除前基线（紧急情况才用）：

```bash
git log --oneline | head -10        # 找到 381932a baseline
git reset --hard 381932a            # ⚠️ 不可逆，确认前别跑
```

---

## 🚀 第一步从这里开始

```
读 phases/00-correctness.md，按它说的做 Phase 0 的 5 个 task。
做完报告，等用户确认再进 Phase 1。
```

---

文档版本：v2.1（拆分索引版，2026-05-20）
原 v2.0 单文件版（1653 行）已拆成 5 个 phase 文件，每个独立、自洽、可单独喂给小模型执行。
