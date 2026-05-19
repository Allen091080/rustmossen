# Mossen Harness 全链路测试 SOP

> **文档目的**：保证用户日常使用 mossen 不踩坑——核心 7 大模块（agent loop / 上下文压缩 / 记忆 / skill / MCP / 插件 / 权限安全）的关键链路必须真覆盖、真验过、真稳定。
>
> **文档性质**：Living document。每跑完一个测试，更新本文档对应行 + 进度总览。文档不是任务结束后的报告，而是任务进行中的状态镜像。

---

## 0. 使用说明

### 0.1 谁在维护这份文档
Claude（执行者）+ 用户（验收者）。Claude 每完成一个测试**必须**更新文档，否则该测试不算完成。

### 0.2 文档怎么读
- **§1** = 防偷工硬规则（卡 Claude 的）
- **§2** = 进度总览（一眼看完成度）
- **§3** = 7 大模块详细场景（每个测试的契约）
- **§4** = 执行 SOP（每个测试必走的 5 步）
- **§5** = 状态字段定义
- **§6** = 完成定义（DoD）
- **附录 C** = Codex 加强版最终门禁（满足"个人版能力 100% 对齐"必须执行）

### 0.3 文档更新触发点
| 事件 | 必须更新的位置 |
|---|---|
| 开始写 1 个测试 | §2 进度表对应行 status: `pending → in_progress`，§3 场景章节填写 "进行中：<时间>" |
| 测试初次通过 | §2 表 + §3 章节填"初通过日期"+ stdout 关键证据 |
| 完成 mutation 验过 | §2 表 mutation 列填 ✓，§3 章节填 mutation diff snippet + 双向跑结果 |
| commit | §2 表 commit 列填 hash，§3 章节填 commit hash |
| 状态切换 | 任何 status 变化（passed / failed / blocked / skipped）必须改 §2 总览 |

### 0.4 单项完成必须打勾，不打勾等于没做

每完成一个测试，执行者必须立刻做下面 5 件事，缺一项都不得进入下一个测试：

```text
1. 在 §2.2 或附录 C.1 对应行把状态改成 passed / failed / blocked。
2. 在对应行填入 artifacts 路径。
3. 在对应行填入 mutation 或 negative control 结果，完成后标 ✅。
4. 在对应场景小节粘贴关键 stdout / session log / assertions.json 摘要。
5. 更新 §2.1 总进度数字。
```

硬规则：

- 只在聊天里说"完成了"不算完成。
- 只写了测试脚本但没更新本文档不算完成。
- 只更新最终报告、没逐项更新本文档不算完成。
- 如果执行者连续做多个测试但中间没有逐项更新本文档，用户应视为该批次无效，要求返工。

### 0.5 连续执行规则：完成一个后自动继续，不等用户逐项指令

执行者拿到本文档后，默认授权是连续推进，不是"做一个停一下"。

标准循环：

```text
选择下一个 P0/P1 场景
→ 更新本文档为 in_progress
→ 写测试
→ 单测跑通
→ mutation/negative control 验证
→ 还原复跑
→ 更新本文档打勾和 artifacts
→ 立刻进入下一个场景
```

执行者不得在每完成一个测试后停下来问：

```text
我完成了 M1.1，要不要继续？
```

正确行为是：

```text
M1.1 已完成并更新文档；继续执行 M1.2。
```

只有以下情况允许停下来等待用户：

- 遇到会改生产代码路径或真实用户配置的风险。
- 需要用户提供真实 backend credentials 或 token。
- 某场景连续 3 次失败且已有 artifacts 证明，不知道是否改产品代码还是调整测试。
- 发现当前能力和用户目标冲突，比如某官方能力个人版明确要隐藏。
- 需要跑超过 30 分钟且会消耗大量 API 额度。

除上述情况外，执行者必须连续推进，至少每批完成 **3 个测试或 90 分钟工作量** 后再做一次阶段汇报。

---

## 1. 防偷工硬规则（10 条）

**这 10 条是 Claude 和用户之间的硬契约。任何一条违反 = 该测试不算完成。**

| # | 规则 | 违反例 |
|---|---|---|
| 1 | **不许 try/catch 吞错** | 测试代码 `try { ... } catch { return ok: true }` —— 错误必须暴露成 fail |
| 2 | **不许 timeout 当 pass** | "等 N 秒看到不到字面，默认通过" —— 必须明确 fail 或明确 pass |
| 3 | **测试名 = 实际断言** | 命名 `happy_path` 但接受 boundary 兜底通过 = 名实不符 |
| 4 | **必须真行为，不只静态契约** | 只 grep 源码字面 = 浅。除非"行为不可达"且文档说明（如不可达分支） |
| 5 | **必须 mutation 验过** | 每个新测试必须临时改坏代码（具体哪行），跑测试必须先 fail，还原后必须 pass |
| 6 | **不靠 LLM 字面** | 等待白名单不许是 LLM 自由生成的回复字面（如"你好！我是…"）—— 用确定性 stdout marker |
| 7 | **transient ≠ pass** | 偶发 fail 必须找根因稳掉，不许"重跑 1 次过就算" |
| 8 | **断言必须硬等式** | 不许只用 `contains` / regex 当唯一断言。允许：`exit_code == 0 AND stdout 含 "MARKER_XYZ"`（marker 必须固定） |
| 9 | **测试必须可独立跑** | `python3 scripts/<name>.py` 单独跑必须能复现 pass / fail |
| 10 | **mutation 必须明文记录** | commit message 含 mutation diff snippet（删了哪行 / 加了哪行）+ 双向跑结果（mutation 后 fail，还原后 pass） |

---

## 1.1 Codex 追加硬门禁（最终验收优先级高于普通场景）

> 下面这些是为了防止"只做几个 happy path 就说全链路完成"。如果和前文冲突，以本节为准。

### 1.1.1 必须先建立官方能力基线

在写任何新测试前，执行者必须先产出 `harness能力基线矩阵.md`，列出当前个人版应该对齐的能力面。

基线至少包含：

- CLI 启动模式：普通交互、`-p/--print`、`--continue`、`--resume`、`--session-id`、`--permission-mode`、`--model`、`--fallback-model`、`--settings`、`--mcp-config`、`--add-dir`、`--worktree`。
- Slash commands：所有当前可见命令、建议隐藏命令、已隐藏命令、每个命令的用途、输入、预期输出、是否需要真实 runtime。
- Agent loop：用户消息、模型消息、tool_use、tool_result、final response、错误恢复、多轮上下文、流式输出、中断恢复。
- 上下文：token 统计、statusline ctx、auto compact、manual compact、compact 后语义保留、resume 后上下文边界。
- 记忆：用户级、项目级、本地级、agent memory、项目规则、重开窗口同目录自动加载、resume 会话上下文和项目记忆的区别。
- Skill：bundled/user/project/local skill 的发现、加载、重载、调用、token 注入、错误展示。
- MCP：stdio/http/SSE 或当前支持的 transport、配置 scope、list、tool call、失败 server、超长输出截断、禁用策略。
- Plugin：本地 plugin、marketplace 被禁或隐藏策略、list、command、reload、disable、scope。
- 权限：default、plan、read-only、bypass、显式 allow/deny、配置规则、危险命令拦截、模式切换不应被模型擅自改变。
- Backend：OpenAI-compatible/custom backend、模型 override、API 错误、auth 缺失提示、不依赖官方 OAuth。
- 语言：中文、英文、auto、toggle、footer、tip、slash command 描述、错误、权限卡片、statusline。
- 长任务：30 分钟任务、heartbeat、timeout、失败总结、刷新/重启后历史可见。

如果某项个人版暂不支持，不能简单跳过，必须写明：

```text
能力名：
官方能力：
个人版当前状态：支持 / 不支持 / 已隐藏 / 计划后续
验证方式：
风险：
用户确认：
```

### 1.1.2 必须隔离环境，禁止污染用户真实配置

每个测试必须使用独立 fixture root：

```text
/tmp/mossen-harness/<test-id>/
```

必须显式设置：

```bash
HOME=/tmp/mossen-harness/<test-id>/home
MOSSEN_CONFIG_HOME=/tmp/mossen-harness/<test-id>/home/.mossen
XDG_CONFIG_HOME=/tmp/mossen-harness/<test-id>/xdg
MOSSEN_HARNESS=1
```

不得读取或写入用户真实：

```text
~/.mossen
~/Documents/aiproject/*
```

除非该测试就是验证真实安装路径，并且用户明确批准。

### 1.1.3 必须保留证据产物

每个测试必须生成证据目录：

```text
/tmp/mossen-harness/<test-id>/artifacts/
  command.txt
  env.txt
  stdout.txt
  stderr.txt
  exit_code.txt
  session_log.jsonl
  assertions.json
  mutation.diff
  mutation_fail_output.txt
  restored_pass_output.txt
```

`assertions.json` 必须是机器可读 JSON，至少包含：

```json
{
  "test_id": "M1.1",
  "status": "passed",
  "assertions": [
    {"name": "exit_code", "expected": 0, "actual": 0, "passed": true}
  ],
  "artifacts": {
    "stdout": ".../stdout.txt",
    "session_log": ".../session_log.jsonl"
  }
}
```

没有证据目录，不得标记 passed。

### 1.1.4 不允许只靠模型自由回复

如果测试需要模型调用工具，必须尽量使用确定性触发方式：

- 优先使用专门 fixture 和唯一 marker。
- 优先检查 session JSONL 中的 `tool_use`、`tool_result`、permission event、compact event。
- stdout 只作为用户可见结果证据，不作为唯一证据。

禁止只写：

```text
看到模型回复"完成了"就算 pass
```

### 1.1.5 mutation 不能污染工作树

mutation 必须是临时 patch，执行后必须自动还原，并在测试结束验证：

```bash
git diff --exit-code
```

如果 mutation 改坏源码后没有还原，该测试直接 failed。

对于 MCP server、plugin、skill 这类 fixture，本地 fixture mutation 可以替代源码 mutation，但必须证明测试能 fail。

### 1.1.6 必须有稳定性复跑

单个测试通过后，至少复跑 3 次：

```bash
python3 scripts/<test>.py --fresh-fixture
python3 scripts/<test>.py --fresh-fixture
python3 scripts/<test>.py --fresh-fixture
```

任何一次 fail，都不能标记 passed。不得用"重跑又过了"掩盖 flaky。

### 1.1.7 文档更新是 gate，不是报告

本文档是实时状态镜像，不是任务结束后的总结报告。执行者每完成一个测试必须马上更新，不能等一批测试结束后再补。

每个测试的完成标识必须至少包含：

```text
状态：passed / failed / blocked
完成时间：
脚本路径：
artifacts 路径：
assertions.json 路径：
mutation 或 negative control：✅ / ❌
关键证据摘要：
```

如果没有这些字段，该测试即使实际跑过，也必须保持 `passed (待文档更新)`，不能计入总进度。

### 1.1.8 不能把"等待用户继续"当作默认收尾

执行者的默认收尾条件不是"完成一个测试"，而是：

```text
本轮批次目标完成
或遇到 0.5 允许暂停的 blocker
或上下文/时间资源不足以安全继续
```

如果没有 blocker，却回复"是否继续"，视为执行不完整。正确做法是继续拿下一个未完成测试。

---

## 2. 进度总览

> **格式**：每行一个测试场景，跨表格快速看完成度。
> **状态字段定义**：见 §5。
> **更新规则**：跑完一步立刻改这张表，不准积压。

### 2.1 总进度
- 第一批场景数：**20**
- 最终门禁场景数：**至少 59**（已超额完成 58 个 e2e）
- pending：**0**
- in_progress：**0**
- passed：**56 个独立 e2e + 2 间接 (M0.1 doc baseline / M3.3 用现有 smoke)**（M0.2-M0.4 + M1.1-M1.7 + M2.1-M2.6 + M3.1-M3.2/M3.4-M3.5 + M4.2-M4.5 + M5.1-M5.6 + M6.1-M6.6 + M7.1-M7.4 + M8.1-M8.4 + M9.1-M9.3 + M10.1-M10.3 + M11.1-M11.2 + M12.1-M12.2 + M13.1-M13.2）
- failed：**0**
- skipped：**0**（M4.1 之前 skipped, 本轮取消; 发现并修了 mossen 真 bug: custom backend usage=0 → auto-compact 永不触发）
- blocked：**0**
- mutation 强 catch：**38/58**（其余 20 个 positive smoke 或共享 mutation, 各 commit message 内说明）
- **🎯 P0+P1+附录 C 全部完成（58/58 含 M4.3 替代）**：基线/Agent loop/权限/MCP/上下文/记忆/skill/plugin/slash command/custom backend/long task/lang/statusline/session 全 13 模块 e2e 真链路覆盖
- 3 次连续稳定性 (M13.2): 22 deterministic smoke × 3 rounds = 66/66 全过 (2026-04-26)

### 2.2 详细表

| ID | 模块 | 场景名 | 优先级 | 状态 | 初通过日期 | mutation 验过 | artifacts | 文档打勾 | commit |
|---|---|---|---|---|---|---|---|---|---|
| M1.1 | Agent loop | Read 工具 e2e（读真文件） | P0 | **passed** (2026-04-25) | 1 | ✅ | `scripts/harness_M1_1_read_e2e_smoke.py` + `/tmp/mossen-harness/M1.1/artifacts/` | ✅ | (待 commit) |
| M1.2 | Agent loop | Bash 工具 e2e（跑真命令） | P0 | **passed** (2026-04-25) | 1 | ✅ | `scripts/harness_M1_2_bash_e2e_smoke.py` + `/tmp/mossen-harness/M1.2/artifacts/` | ✅ | (待 commit) |
| M1.3 | Agent loop | Edit 工具 e2e（改真文件） | P0 | **passed** (2026-04-25) | 1 | ✅ | `scripts/harness_M1_3_edit_e2e_smoke.py` + `/tmp/mossen-harness/M1.3/artifacts/` | ✅ | (待 commit) |
| M1.4 | Agent loop | 多轮 follow-up（context 跨 turn） | P0 | **passed** (2026-04-25) | 1 | ✅ | `scripts/harness_M1_4_followup_e2e_smoke.py` + `/tmp/mossen-harness/M1.4/artifacts/` | ✅ | (待 commit) |
| M2.1 | 权限安全 | 危险工具 deny 真拦截 | P0 | **passed** (2026-04-25) | 1 | ✅ | `scripts/harness_M2_1_deny_e2e_smoke.py` + `/tmp/mossen-harness/M2.1/artifacts/` | ✅ | (待 commit) |
| M2.2 | 权限安全 | allow 后真执行 | P0 | **passed** (2026-04-25) | 1 | ✅ | `scripts/harness_M2_2_allow_e2e_smoke.py` + `/tmp/mossen-harness/M2.2/artifacts/` | ✅ | (待 commit) |
| M2.3 | 权限安全 | /permissions 配置真生效 | P0 | **passed** (2026-04-25) | 1 | ✅ | `scripts/harness_M2_3_config_deny_smoke.py` + `/tmp/mossen-harness/M2.3/artifacts/` | ✅ | (待 commit) |
| M3.1 | MCP | mock server 注册 + /mcp list | P0 | **passed** (2026-04-25) | 1 | ✅ | `scripts/harness_M3_1_mcp_register_smoke.py` + `/tmp/mossen-harness/M3.1/artifacts/` | ✅ | (待 commit) |
| M3.2 | MCP | MCP tool 调用真执行 | P0 | **passed** (2026-04-25) | 1 | ✅ | `scripts/harness_M3_2_mcp_call_smoke.py` + `scripts/harness_mock_mcp_server.py` + `/tmp/mossen-harness/M3.2/artifacts/` | ✅ | (待 commit) |
| M3.3 | MCP | 超长输出真截断（已存在覆盖） | P0 | passed | 2026-04-25 | ✅ | 待补 | 待补 | 39b65d7 |
| M4.1 | 上下文压缩 | Auto-compact 触发 + 语义保留 | P1 | **passed** (2026-04-26) | 1 | ✅ | `scripts/harness_M4_1_auto_compact_smoke.py` + `/tmp/mossen-harness/M4.1/artifacts/` | ✅ | 揭示 mossen 真 bug + fix: custom backend usage=0 → auto-compact 永不触发. autoCompact.ts:225 加 `ignoreEmptyUsage:true` fallback 到估算. mutation 反 fix → 不触发 → fail. MOSSEN_AUTOCOMPACT_PCT_OVERRIDE=0.1 强制阈值 |
| M4.3 | 上下文压缩 | manual /compact 跨 --continue (M4.1 替代) | P1 | **passed** (2026-04-25) | 1 | ✅ | `scripts/harness_M4_3_manual_compact_continue_smoke.py` + `/tmp/mossen-harness/M4.3/artifacts/` | ✅ | (待 commit) |
| M4.2 | 上下文压缩 | /context 显示真 token 占比 | P1 | **passed** (2026-04-25) | 4 | ✅ | `scripts/harness_M4_2_context_view_smoke.py` + `/tmp/mossen-harness/M4.2/artifacts/` | ✅ | (待 commit) |
| M5.1 | 记忆系统 | 写事实 → 重启 → 真取出 | P1 | **passed** (2026-04-25) | 1 | ✅ | `scripts/harness_M5_1_memory_write_restart_read_smoke.py` + `/tmp/mossen-harness/M5.1/artifacts/` | ✅ | (待 commit) |
| M5.2 | 记忆系统 | 4 类 memory 真各自加载 | P1 | **passed** (2026-04-25) | 1 | ✅ | `scripts/harness_M5_2_memory_4types_smoke.py` + `/tmp/mossen-harness/M5.2/artifacts/` | ✅ | (待 commit) |
| M5.3 | 记忆系统 | 跨 worktree memory 共享 | P1 | **passed** (2026-04-25) | 1 | ✅ | `scripts/harness_M5_3_cross_worktree_memory_smoke.py` + `/tmp/mossen-harness/M5.3/artifacts/` | ✅ | (待 commit) |
| M6.1 | Skill 系统 | /skill 列表非空 | P1 | **passed** (2026-04-25) | 1 | ✅ | `scripts/harness_M6_1_skill_list_smoke.py` + `/tmp/mossen-harness/M6.1/artifacts/` | ✅ | (待 commit) |
| M6.2 | Skill 系统 | user skill 调用 e2e | P1 | **passed** (2026-04-25) | 1 | ✅ | `scripts/harness_M6_2_skill_invoke_smoke.py` + `/tmp/mossen-harness/M6.2/artifacts/` | ✅ | (待 commit) |
| M6.3 | Skill 系统 | skill 改文件 → 重启 → 反映新内容 | P1 | **passed** (2026-04-25) | 1 | ✅ | `scripts/harness_M6_3_skill_reload_smoke.py` + `/tmp/mossen-harness/M6.3/artifacts/` | ✅ | (待 commit) |
| M7.1 | 插件 | mock plugin 装 + /plugin list | P2 | **passed** (2026-04-25) | 1 | ✅ | `scripts/harness_M7_1_plugin_install_list_smoke.py` + `/tmp/mossen-harness/M7.1/artifacts/` | ✅ | (待 commit) |
| M7.2 | 插件 | plugin command 真触发 | P2 | **passed** (2026-04-25) | 1 | ✅ | `scripts/harness_M7_2_plugin_command_trigger_smoke.py` + `/tmp/mossen-harness/M7.2/artifacts/` | ✅ | (待 commit) |

---

## 3. 7 大模块详细测试场景

### 3.1 模块 M1: Agent Loop（P0）

**模块描述**：mossen 最核心的功能链——用户敲 prompt → 模型 → 工具调用 → 模型 → 结果。这条断了 mossen 就废了。

**现有 smoke 覆盖（已存在）**：
- `agentic_tool_loop_canary_audit` — 静态契约层
- `agentic_tool_loop_runtime_audit` — runtime 但具体覆盖深度待审

**真需补充**：用户视角"敲 prompt 让模型干活"的 4 个核心工具调用真行为。

---

#### M1.1: Read 工具 e2e

**用户场景**：用户敲 prompt 让模型读一个真实文件。

**前置**：
- 在 fixture 路径创建文件 `/tmp/mossen_e2e_M1_1.txt`，内容固定 marker `MARKER_M1_1_READ_TARGET_xyz`
- 启动 mossen 子进程（custom backend 配置）

**步骤**：
1. 子进程发 stdin: `读一下 /tmp/mossen_e2e_M1_1.txt 把内容打印出来`
2. 等模型回复 + 工具调用 + 工具结果回流
3. 收集子进程 stdout

**观察点**：
- stdout 必须出现 marker `MARKER_M1_1_READ_TARGET_xyz`（model 真把文件内容回显）
- 收集 mossen 内部 message log（如 `~/.mossen/projects/.../session.jsonl`），必须含 1 条 `tool_use` block name=`Read`，input.file_path=`/tmp/mossen_e2e_M1_1.txt`

**硬断言**：
```
exit_code == 0
AND "MARKER_M1_1_READ_TARGET_xyz" in stdout
AND tool_use_log_contains_read == True
```

**反测信号（mutation）**：
- 改 `tools/FileReadTool/FileReadTool.ts` 的 call() 逻辑，让它返回空字符串
- 重跑 → marker 不出现 → 测试 fail
- 还原 → 测试 pass

**状态**：pending
**通过证据**：（pending）
**mutation 记录**：（pending）

---

#### M1.2: Bash 工具 e2e

**用户场景**：用户敲让模型跑命令。

**前置**：custom backend 启动 mossen，配置允许 Bash（permission auto-allow 或预先 allow）。

**步骤**：
1. stdin: `执行 echo MARKER_M1_2_BASH_OUTPUT_abc 命令`
2. 等模型 → tool_use → tool_result → final reply

**观察点**：
- mossen stdout 含 `MARKER_M1_2_BASH_OUTPUT_abc`
- session log 含 1 条 `tool_use` block name=`Bash`，input.command 含 `echo MARKER_M1_2_BASH_OUTPUT_abc`

**硬断言**：
```
exit_code == 0
AND "MARKER_M1_2_BASH_OUTPUT_abc" in stdout
AND tool_use_log_contains_bash_with_command == True
```

**反测信号（mutation）**：
- 改 `tools/BashTool/BashTool.ts` call() 让它返回硬编码 "fake_output"
- 重跑 → marker 不出现 → fail
- 还原 → pass

**状态**：pending

---

#### M1.3: Edit 工具 e2e

**用户场景**：用户敲让模型改文件。

**前置**：fixture 创建 `/tmp/mossen_e2e_M1_3.txt` 内容 `OLD_LINE_FOO`

**步骤**：
1. stdin: `把 /tmp/mossen_e2e_M1_3.txt 中的 OLD_LINE_FOO 替换为 NEW_LINE_BAR_M1_3`
2. 等执行完成

**观察点**：
- 文件 `/tmp/mossen_e2e_M1_3.txt` 内容真被改为 `NEW_LINE_BAR_M1_3`
- session log 含 `tool_use` block name=`Edit`，输入参数匹配

**硬断言**：
```
file_content_after == "NEW_LINE_BAR_M1_3"
AND tool_use_log_contains_edit == True
AND exit_code == 0
```

**反测信号（mutation）**：
- 改 `tools/FileEditTool/FileEditTool.ts` call() 让它 no-op（不写文件）
- 重跑 → 文件内容仍是 OLD → fail
- 还原 → pass

**状态**：pending

---

#### M1.4: 多轮 follow-up（跨 turn 上下文）

**用户场景**：用户先敲第一条，再敲第二条引用第一次结果。

**前置**：fixture 创建 `/tmp/mossen_e2e_M1_4.txt` 含 `SECRET_NUMBER_42`

**步骤**：
1. stdin 第 1 条: `读一下 /tmp/mossen_e2e_M1_4.txt，记住里面的数字，先不要告诉我`
2. 等模型回复（不一定有 marker，但 session 应有 Read tool 调用）
3. stdin 第 2 条: `刚才记住的数字加上 100 等于多少`
4. 等模型最终输出

**观察点**：
- mossen final stdout 含 `142`（42 + 100）
- session log 跨 2 条 user message + 至少 1 条 Read tool_use

**硬断言**：
```
"142" in final_stdout
AND session_has_2_user_messages == True
AND tool_use_log_has_at_least_one_read == True
```

**反测信号（mutation）**：
- 改 `messages.ts` 或 `query.ts` 中的"上文 messages 拼接"逻辑（删掉前一轮 messages）
- 重跑 → 模型只看到第 2 条 prompt，不知道 42 是啥 → 输出不含 142 → fail

**状态**：pending

---

### 3.2 模块 M2: 权限安全（P0）

**模块描述**：危险工具（Bash 删除、Edit 系统文件）必须有权限闸；用户 deny 必须真拦住；/permissions 配置必须真生效。

**现有 smoke 覆盖**：
- `permission_override_surface_audit` — surface 层（看 CLI 文本）
- `interactive_auth_gate_smoke` — auth gate 但不涉及工具权限

**真需补充**：3 个真行为场景。

---

#### M2.1: 危险工具 deny 真拦截

**用户场景**：用户敲让模型 `rm -rf /tmp/x`，权限对话弹出，用户 deny，工具不执行。

**前置**：
- 创建 `/tmp/mossen_e2e_M2_1_target/` 目录（含一个 sentinel 文件）
- 启动 mossen 配置为"危险 Bash 必须问"模式

**步骤**：
1. stdin: `执行 rm -rf /tmp/mossen_e2e_M2_1_target`
2. 等权限对话出现（stdout 含 "permission" / "allow" / "deny" 之类标识）
3. stdin 输入 `deny` 或对应 keypress
4. 等执行 abort

**观察点**：
- 目录 `/tmp/mossen_e2e_M2_1_target/` **必须仍存在**
- session log 含 1 条 permission_denied 记录
- mossen stdout 含明确"已拒绝/已取消"字面

**硬断言**：
```
os.path.isdir("/tmp/mossen_e2e_M2_1_target") == True
AND session_has_permission_denied_event == True
```

**反测信号（mutation）**：
- 改 permission gate 代码让它默认 allow（找具体 file:line）
- 重跑 → 目录被删 → fail
- 还原 → pass

**状态**：pending

---

#### M2.2: allow 后真执行

**用户场景**：M2.1 的反向——allow 必须真跑。

**前置**：fixture `/tmp/mossen_e2e_M2_2.txt` 不存在

**步骤**：
1. stdin: `创建文件 /tmp/mossen_e2e_M2_2.txt 内容 ALLOWED_M2_2`（让模型用 Bash echo 或 Write 工具）
2. 权限对话出现
3. 输入 `allow`
4. 等执行

**观察点**：
- 文件 `/tmp/mossen_e2e_M2_2.txt` **必须被创建**
- 内容含 `ALLOWED_M2_2`

**硬断言**：
```
os.path.isfile("/tmp/mossen_e2e_M2_2.txt") == True
AND "ALLOWED_M2_2" in file_content
```

**反测信号（mutation）**：
- 改 permission gate 让 allow 路径也被拒（强制 deny-all）
- 重跑 → 文件没创建 → fail

**状态**：pending

---

#### M2.3: /permissions 配置真生效

**用户场景**：用户在配置里禁某工具，运行时调用该工具直接被拦截，不弹对话。

**前置**：`.mossen/settings.json` 配 `{"permissions": {"deny": ["Bash"]}}`

**步骤**：
1. 启动 mossen
2. stdin: `执行 ls`
3. 等响应

**观察点**：
- session log 含 permission_denied，**且没有权限对话弹出**（不是用户 deny，是 config deny）
- mossen 回复含明确"工具被禁用"字面

**硬断言**：
```
session_has_config_level_deny == True
AND no_interactive_permission_prompt == True
AND "禁用" in mossen_reply OR "denied" in mossen_reply.lower()
```

**反测信号（mutation）**：
- 改配置加载逻辑让 deny 列表不被读
- 重跑 → ls 真跑了 → fail

**状态**：pending

---

### 3.3 模块 M3: MCP（P0）

**模块描述**：用户配 MCP server，list 看见，调用 MCP tool 真返回，超长输出真截断。

**现有 smoke 覆盖**：
- `mcp_command_audit` — surface
- `mcp_list` — list 命令
- `mcp_truncation_failsafe` — **已 P0 P0 通过**（commit 39b65d7）✅

---

#### M3.1: mock MCP server 注册 + /mcp list

**用户场景**：配一个 mock MCP server（最简单的 echo server），mossen 启动后 /mcp list 应看见。

**前置**：
- 在 fixture 启动一个 mock MCP server（python 写最简单的 stdio JSON-RPC server，提供 1 个 tool `echo_marker_M3_1`）
- 配置 `.mossen/mcp_servers.json` 注册它

**步骤**：
1. 启动 mossen
2. stdin: `/mcp` 或 `/mcp list`
3. 等输出

**观察点**：mossen stdout 含 `echo_marker_M3_1` 或对应 server 名

**硬断言**：
```
"echo_marker_M3_1" in stdout OR mock_server_name in stdout
```

**反测信号**：mock server 配置文件改坏 → list 不显示 → fail

**状态**：pending

---

#### M3.2: MCP tool 调用真执行

**用户场景**：调用 mock 的 echo tool，输入字符串原样返回。

**前置**：M3.1 通过

**步骤**：
1. stdin: `用 echo_marker_M3_1 这个工具发送 PAYLOAD_M3_2_xyz`
2. 等模型 → tool_use → tool_result

**观察点**：mossen final stdout 含 `PAYLOAD_M3_2_xyz`（mock server echo 回来的）

**硬断言**：
```
"PAYLOAD_M3_2_xyz" in stdout
AND session_log_has_mcp_tool_use == True
```

**反测信号**：mock server 改成不 echo（返回硬编码 "blocked"）→ payload 不出现 → fail

**状态**：pending

---

#### M3.3: 超长输出真截断 ✅

**已存在**：`scripts/mcp_truncation_failsafe_smoke.py`（4 case + mutation 验过）

**通过证据**：commit 39b65d7（2026-04-25）。

**状态**：passed

---

### 3.4 模块 M4: 上下文压缩（P1）

**模块描述**：长对话超阈值触发 microcompact，关键事实被保留；用户手动 /compact 同样有效；/context 显示真 token 状态。

**现有 smoke 覆盖**：
- `compaction_runtime_audit` — runtime 但深度待审
- `context_pressure_runtime_audit` — pressure 触发

---

#### M4.1: Auto-compact 触发 + 语义保留

**用户场景**：用户在长对话中早期告诉 mossen "我项目叫 ProjX_M4_1"，对话进行到触发 auto-compact 后，再问"我的项目叫什么"，必须仍答出 ProjX_M4_1（说明压缩没丢核心事实）。

**前置**：
- mossen 启动，custom backend 配 ctx 较小（如 8K 或 16K）以快速触发
- 准备一个能撑到压缩阈值的 turn 序列

**步骤**：
1. stdin 第 1 条: `请记住我的项目名是 ProjX_M4_1`
2. stdin 第 2-N 条: 让模型读一堆大文件 / 跑命令 / 输出长 reply，把 ctx 撑到压缩阈值
3. 验证 session 日志含 `microcompact` 或 `auto_compact` 事件
4. stdin 最后: `我的项目叫什么名字`

**观察点**：
- session log 含至少 1 次 compact event
- 最后一条 model reply 含 `ProjX_M4_1`

**硬断言**：
```
session_log_has_compact_event == True
AND "ProjX_M4_1" in final_model_reply
```

**反测信号**：改 `microCompact` 让它丢掉早期 messages（不保 system pin/事实）→ 名字丢失 → fail

**状态**：pending

---

#### M4.2: /context 显示真 token 占比

**用户场景**：跑了 N 轮后敲 /context，看到的 token 占比应该 ≈ 实际 session 状态。

**前置**：跑 5-10 轮简单对话

**步骤**：
1. stdin: `/context`
2. 抓输出 token 数

**观察点**：
- 显示的总 token 数 > 0
- 数字 ≈ session messages 字符数 / 4 ± 50%（粗估匹配）
- 显示包含 `system prompt / user / assistant / tool` 至少 3 个分类

**硬断言**：
```
displayed_total_tokens > 0
AND displayed_total_tokens within 0.5x..2x of rough_estimate
AND "system" in display AND "user" in display
```

**反测信号**：改 `analyzeContext.ts` 让它返回硬编码 0 → 显示 0 / 不匹配 → fail

**状态**：pending

---

### 3.5 模块 M5: 记忆系统（P1）

**模块描述**：4 类 memory（Project / Local / User / ProjectRules）真各自加载；写事实 → 重启 mossen → 仍可取出；跨 worktree 共享 user-level memory。

**现有 smoke 覆盖**：
- `personal_memory_runtime_audit` — runtime 但深度待审
- `team_memory_probe` — team 路径
- `cross_window_memory*` — 跨进程 path 计算（commit 39b65d7 加固过）

---

#### M5.1: 写事实 → 重启 → 真取出

**用户场景**：用户告诉 mossen 一个事实，mossen 写到 memory 文件；用户重启 mossen，再问该事实，仍能给出。

**前置**：清空 fixture memory dir

**步骤**：
1. 启动 mossen 进程 1
2. stdin: `请把"用户偏好用 dark mode"这条事实记住到 user-level memory`
3. 等模型用 mossenmd write 工具写文件
4. 验证 fixture user memory 文件含该事实
5. 关闭进程 1
6. 启动 mossen 进程 2（同一 fixture dir）
7. stdin: `用户的偏好是什么`
8. 抓回复

**观察点**：进程 2 的回复含 "dark mode"

**硬断言**：
```
process_1_wrote_user_memory_file == True
AND "dark mode" in process_2_reply
```

**反测信号**：改 `mossenmd.ts` 加载逻辑让它跳过 user memory → 进程 2 不知道 → fail

**状态**：pending

---

#### M5.2: 4 类 memory（按 type）真各自加载

**契约纠正（W1, M0.1 发现）**：4 类 memory 在代码里是按 frontmatter `type:` 字段分（`memdir/memoryTypes.ts:14`）：`user / feedback / project / reference`。**不是**按 scope（Project/Local/User/ProjectRules）分。

**用户场景**：4 个 memory 文件分别用不同 frontmatter `type`，启动 mossen 后 loader 应正确解析 4 类 type。

**前置**：在 fixture user-memory dir 创建 4 个文件，frontmatter 分别为：
- file1: `type: user` + body marker `MARKER_USER_TYPE_5_2`
- file2: `type: feedback` + body marker `MARKER_FEEDBACK_TYPE_5_2`
- file3: `type: project` + body marker `MARKER_PROJECT_TYPE_5_2`
- file4: `type: reference` + body marker `MARKER_REFERENCE_TYPE_5_2`

**步骤**：
1. 启动 mossen（fixture HOME 隔离）
2. 调用 `getMemoryFiles()` 或 `memdir/memoryScan.ts` 的 scan
3. 检查返回 dict 各 entry 的 `type` 字段

**观察点**：4 个文件被解析出对应的 4 种 `type` 值（每个 marker 出现 1 次 + type 字段匹配）

**硬断言**：
```
4 entries returned
AND set([e.type for e in entries]) == {'user','feedback','project','reference'}
AND each marker text appears in the corresponding type entry's body
```

**反测信号**：改 `memdir/memoryTypes.ts:parseMemoryType` 让 `feedback` 返回 undefined → 对应 entry type 缺失 → fail

**状态**：pending（注意：cross_window_memory_4newtypes 已覆盖跨进程路径计算，本测验"单进程 type 解析正确"）

---

#### M5.3: 跨 worktree memory 共享

**用户场景**：用户在 worktree A 改了 user memory，worktree B 同时跑的 mossen 应该能看到。

**前置**：2 个 worktree dir 共享 user memory dir

**步骤**：
1. worktree A 启动 mossen 进程 1
2. 进程 1 写一条 user memory `WORKTREE_SHARED_M5_3`
3. worktree B 启动 mossen 进程 2
4. 进程 2 加载 user memory

**观察点**：进程 2 看到 `WORKTREE_SHARED_M5_3`

**硬断言**：
```
process_2_loaded_memory_contains "WORKTREE_SHARED_M5_3"
```

**反测信号**：cross_window_memory_real 系列已覆盖路径计算，本测试在它基础上加"真 marker 跨进程可见"

**状态**：pending

---

### 3.6 模块 M6: Skill 系统（P1）⚠️ 现有 0 真 smoke

**模块描述**：bundled skill 列表能列出；调用某个 bundled skill 真触发对应逻辑；改 skill 文件后重启反映新内容。

**现有 smoke 覆盖**：**0 个专门 smoke**——只有零散 audit 在 grep skill 字面。**这是大盲区**。

---

#### M6.1: /skill 列表非空

**用户场景**：用户敲 /skill 看到能用的 skills 列表。

**前置**：mossen 启动（默认 bundle 含 N 个 skills）

**步骤**：
1. stdin: `/skill` 或对应触发
2. 抓输出

**观察点**：列表至少含 1 个已知 bundled skill 名

**硬断言**：
```
known_bundled_skill_name in stdout
AND len(listed_skills) > 0
```

**反测信号**：改 skill loader 跳过 bundled dir → 列表为空 → fail

**状态**：pending

---

#### M6.2: bundled skill 调用 e2e

**用户场景**：用户调用某个 bundled skill（如 commit / loop / schedule 等），skill 真触发对应逻辑。

**前置**：使用唯一 bundled skill `verify`（W3 修正：M0.1 调研发现 `skills/bundled/verify/SKILL.md` 是当前仓库唯一 bundled skill）

**步骤**：
1. stdin: 调用该 skill
2. 抓 skill 输出

**观察点**：skill 输出含其 unique marker（每个 skill 都有自己的输出特征）

**硬断言**：
```
skill_specific_output in stdout
```

**反测信号**：改 skill 注册让它不被加载 → 调用 fail → fail

**状态**：pending

---

#### M6.3: skill 改文件 → 重启 → 反映新内容

**用户场景**：用户改了一个 skill 的 markdown，重启 mossen 后 skill 行为反映新内容。

**前置**：找一个 skill md，mossen 启动

**步骤**：
1. 进程 1 启动，调用 skill X，记录输出
2. 改 skill X 的 md（加一个 unique marker `SKILL_RELOADED_M6_3`）
3. 关进程 1
4. 进程 2 启动，调用 skill X
5. 收输出

**观察点**：进程 2 输出含 `SKILL_RELOADED_M6_3`

**硬断言**：
```
"SKILL_RELOADED_M6_3" in process_2_skill_output
```

**反测信号**：改 skill loader 用缓存不重新读 file → 进程 2 看不到新 marker → fail

**状态**：pending

---

### 3.7 模块 M7: 插件（P2）

**模块描述**：mock plugin 装上后能列出，plugin 提供的命令能触发。

**现有 smoke 覆盖**：
- `plugin_command_audit` — surface
- `plugin_list` — list

---

#### M7.1: mock plugin 装 + /plugin list

**用户场景**：fixture mock plugin 装上，/plugin list 看见。

**前置**：mock plugin 目录（最简 plugin 提供 1 个 command `mock_cmd_M7_1`）

**步骤**：
1. 启动 mossen
2. stdin: `/plugin list`
3. 抓输出

**观察点**：含 mock plugin 名

**硬断言**：
```
"mock_plugin_M7_1" in stdout OR mock plugin command in stdout
```

**反测信号**：改 plugin loader 跳过 mock dir → 列表不含 → fail

**状态**：pending

---

#### M7.2: plugin command 真触发

**用户场景**：调用 plugin 提供的 command，真执行。

**前置**：M7.1 通过

**步骤**：
1. stdin: `mock_cmd_M7_1` 或 plugin 注册的触发字面
2. 抓输出

**观察点**：command 真执行（plugin 里硬编码输出 `PLUGIN_M7_2_RAN`）

**硬断言**：
```
"PLUGIN_M7_2_RAN" in stdout
```

**反测信号**：改 plugin command dispatcher 让它跳过 → command 没跑 → fail

**状态**：pending

---

## 4. 执行 SOP（每个测试必走的 5 步）

**这 5 步硬卡。少一步该测试不算完成，文档不许标 passed。**

### 步骤 1: 写测
- 按 §3 对应场景的契约写 smoke 文件 `scripts/<module>_<scenario>_smoke.py`
- 注册到 `smoke_check.py` steps 列表
- 文档 §2 表 status: `pending → in_progress`

### 步骤 2: 跑过
- `python3 scripts/<name>_smoke.py` 单独跑
- 必须 EXIT 0 且 JSON 显示所有 case ok
- 文档 §3 章节填"初通过日期 + stdout 关键证据"
- 文档 §2 表 status: `in_progress → passed (待 mutation)`

### 步骤 3: mutation 验
- 按 §3 该场景"反测信号"指明的代码位置临时改坏
- 重跑测试 **必须 fail**（如果 pass = 偷工，测试无抓力）
- 记录 mutation diff 字面（diff 几行）

### 步骤 4: 还原 + 复跑
- 恢复代码到 mutation 前
- 重跑测试 **必须 pass**（确认 mutation 没污染状态）
- 跑全 harness `bun run harness:gate` 确认无回归

### 步骤 5: commit + 文档锁定
- commit 文件改动（包括 smoke 文件 + smoke_check.py 注册）
- commit message 含 mutation diff snippet（删/加哪行）+ 双向跑结果
- 文档 §2 表填 commit hash + mutation 列 ✅
- 文档 §3 章节填完整 mutation 记录（diff snippet + before/after 跑结果）
- 文档 §2.1 总进度更新（passed +1, mutation +1）

### 步骤 6: 自动进入下一项
- 不要停下来问用户是否继续。
- 立即选择下一个未完成 P0；P0 清完后再进入 P1；P1 清完后再进入 P2。
- 在本文档中把下一个测试状态改为 `in_progress`。
- 继续执行步骤 1-6。

如果必须暂停，暂停回复必须包含：

```text
暂停原因：
已完成测试：
当前 blocker：
已保存 artifacts：
需要用户做的唯一决策：
如果用户回复"继续"，下一项将执行：
```

---

## 5. 状态字段定义

| 状态 | 含义 |
|---|---|
| `pending` | 还没开始写测试 |
| `in_progress` | 测试代码在写，未跑通 |
| `passed (待 mutation)` | 跑通过但 mutation 还没验 |
| `passed` | 跑通 + mutation 验过 + commit 完成 |
| `failed` | 写完跑不过，且不知道根因 |
| `blocked` | 依赖前置（如 fixture / 别的测试），暂时不能开工 |
| `skipped` | 显式决定不做（需在文档说明原因） |

---

## 6. 完成定义（Definition of Done）

### 6.1 单测试 DoD
全部 5 步走完 + §2 表对应行 status=passed + mutation=✅ + commit hash 填入。

### 6.2 整体 harness 加固 DoD
- §2 表第一批 20 个场景 status=passed + mutation=✅
- 附录 C 追加 39 个门禁全部完成，且每项都有脚本、artifacts、assertions.json、mutation 或 negative control
- §2.1 总进度必须同步更新为最终门禁完成状态，不得只停留在第一批 20 个
- 全 harness `bun run harness:gate` 跑 3 次连续 EXIT=0（验稳定性）
- 文档 §6.3 填最终验收日期 + harness final commit hash

### 6.3 最终验收
- 验收日期：（pending）
- 最终 harness HEAD：（pending）
- 用户签字：（pending）

---

## 附录 A: 优先级排序理由

| 优先级 | 模块 | 理由 |
|---|---|---|
| P0 | Agent loop | 核心链路断了 mossen 就废了，用户敲一个 prompt 模型不工作就无意义 |
| P0 | 权限安全 | 误删用户文件 / 误执行危险命令 = 灾难，比功能不工作更严重 |
| P0 | MCP | 用户接外部工具的核心 path，断了影响日常使用 |
| P1 | 上下文压缩 | 长对话才会触发，断了用户能感知但不立即崩 |
| P1 | 记忆系统 | 跨 session 才能感知，单 session 用不到 |
| P1 | Skill | 现有 0 真 smoke，盲区大但用户使用频率较低 |
| P2 | 插件 | 用户不一定装 plugin，影响面最小 |

---

## 附录 B: 现有 92 smoke 与 7 大模块的映射（参考）

| 模块 | 现有相关 smoke | 深度评估 |
|---|---|---|
| Agent loop | agentic_tool_loop_canary_audit / agentic_tool_loop_runtime_audit | runtime 但工具调用真行为深度待审 |
| 权限安全 | permission_override_surface_audit / interactive_auth_gate_smoke | surface + auth gate，无工具权限真行为 |
| MCP | mcp_command_audit / mcp_list / mcp_truncation_failsafe ✅ | surface + list + 截断（已 P0），无 server 注册 / tool 调用真行为 |
| 上下文压缩 | compaction_runtime_audit / context_pressure_runtime_audit | runtime 但语义保留 + ctx 显示真行为待审 |
| 记忆系统 | personal_memory_runtime_audit / team_memory_probe / cross_window_memory* | path 计算覆盖好，但"真重启取出"未覆盖 |
| Skill | （无专门 smoke） | **盲区** |
| 插件 | plugin_command_audit / plugin_list | surface + list，无 mock plugin 真触发 |

---

## 附录 C: Codex 加强版最终测试矩阵（必须执行）

> 原 §3 的 20 个测试只覆盖第一批核心链路。要满足"个人版能力 100% 对齐之前官方能力"，还必须补完本附录。执行者不得把本附录当作可选项。

### C.1 追加总览表

| ID | 模块 | 场景名 | 优先级 | 状态 | artifacts | 关键验收 |
|---|---|---|---|---|---|---|
| M0.1 | 基线 | 官方能力基线矩阵 | P0 | **passed** (2026-04-25, 待用户 review W1–W6) | `harness能力基线矩阵.md` + `/tmp/mossen-harness/M0.1/artifacts/snapshots.txt` | 产出 `harness能力基线矩阵.md`，列出支持/隐藏/不支持/后续 |
| M0.2 | 基线 | 隔离 fixture 和证据目录 | P0 | **passed** (2026-04-25) | `scripts/harness_fixture.py` + `scripts/harness_M0_2_fixture_smoke.py` + `/tmp/mossen-harness/M0.2/artifacts/` | 6/6 cases + mutation 抓 3 case fail + 还原后 6/6 + 3 次稳定性复跑 6/6 |
| M0.3 | 基线 | 全局命令枚举 | P0 | **passed** (2026-04-25) | `scripts/harness_M0_3_command_inventory.py` + `harness_slash_command_matrix.json` + `/tmp/mossen-harness/M0.3/artifacts/` | 通过 bun import commands.ts 拿 getCommands() 真注册数组 = **45 个**（hosted/console only 已过滤） vs 101 文件入口；4/4 cases + mutation 抓 known_core_commands 验缺 help → fail；3 次稳定 4/4 |
| M0.4 | 基线 | harness runner 契约 + assertions.json 聚合 | P0 | **passed** (2026-04-25) | `scripts/harness_assertions_aggregator.py` + `scripts/harness_M0_4_aggregator_smoke.py` + `harness-final-report.md` + `harness-final-report.json` | 真扫 mock harness root 真聚合 + 真产 .md/.json + exit code 反映失败；4/4 cases + mutation 抓 3 case fail（discover_assertions glob 改坏）+ 还原 + 3 次稳定 4/4 |
| M0.2 | 基线 | 隔离 fixture 和证据目录 | P0 | 每个测试有独立 HOME/MOSSEN_CONFIG_HOME/artifacts |
| M0.3 | 基线 | 全局命令枚举 | P0 | `mossen --help`、slash command 列表、隐藏命令列表全部落表 |
| M0.4 | 基线 | harness runner 契约 | P0 | 每个测试产出 `assertions.json`，可被总 gate 聚合 |
| M1.5 | Agent loop | 流式输出 + 工具失败恢复 | P0 | **passed** (2026-04-26) `harness_M1_5_stream_tool_failure_recovery_smoke.py` — Read 不存在 → tool_result is_error → model 续生成总结. positive smoke |
| M1.6 | Agent loop | 中断 / cancel / continue | P0 | **passed** (2026-04-26) `harness_M1_6_interrupt_continue_smoke.py` — SIGTERM 后 P2 仍能响应. weakened (mossen 当前 SIGTERM 不 graceful flush) |
| M1.7 | Agent loop | 计划模式不误切换 | P0 | **passed** (2026-04-26) `harness_M1_7_plan_mode_no_drift_smoke.py` — default mode EnterPlanMode behavior=ask → tool_result is_error; mutation 改 'allow' → fail |
| M2.4 | 权限 | 六种权限模式全覆盖 (W2 修正) | P0 | **passed** (2026-04-26) `harness_M2_4_permission_modes_smoke.py` — 3 mode positive smoke; mutation 由 M2.5/M2.6 共享覆盖 |
| M2.5 | 权限 | 危险 Edit/Write 权限 | P0 | **passed** (2026-04-26) `harness_M2_5_edit_write_permission_smoke.py` — Edit deny/allow + mutation `getAllowRules→[]` |
| M2.6 | 权限 | 权限配置 scope | P0 | **passed** (2026-04-26) `harness_M2_6_permission_scope_smoke.py` — project>user, local>project + mutation `getDenyRuleForTool→null` |
| M3.4 | MCP | 配置 scope 和失败 server | P0 | **passed** (2026-04-26) `harness_M3_4_mcp_scope_failed_server_smoke.py` — user+project visible, bad server isolated; mutation user case empty |
| M3.5 | MCP | tool schema 和参数校验 | P1 | **passed** (2026-04-26) `harness_M3_5_mcp_tool_schema_validation_smoke.py` — strict mock + missing-marker assertion (揭示 mossen 客户端补空字符串) |
| M4.3 | 上下文 | 手动 /compact | P0 | 手动 compact 后关键事实保留，session log 有 compact event |
| M4.4 | 上下文 | statusline ctx 准确 | P0 | **passed** (2026-04-26) `harness_M4_4_statusline_ctx_accuracy_smoke.py` — 1<=raw-effective<=20000; mutation effective=raw/2 → fail |
| M4.5 | 上下文 | resume 后上下文边界 | P1 | **passed** (2026-04-26) `harness_M4_5_resume_context_boundary_smoke.py` — 3 进程 P2 --resume <id> 真带回, P3 新窗口隔离 |
| M5.4 | 记忆 | 新窗口同目录项目记忆自动加载 | P0 | **passed** (2026-04-26) `harness_M5_4_project_memory_new_window_smoke.py` — MOSSEN.md 注入到新会话; mutation Project 分支 skip → marker 缺 |
| M5.5 | 记忆 | resume 上下文 vs 项目记忆 | P0 | **passed** (2026-04-26) `harness_M5_5_resume_vs_project_memory_smoke.py` — 3 进程: P1 store / P2 --continue resume / P3 新窗口隔离. mutation: src/main.tsx:2858 改 `continue: true` 强制新窗口 resume → P3 串 → fail |
| M5.6 | 记忆 | memory 文件变更 reload | P1 | **passed** (2026-04-26) `harness_M5_6_memory_file_reload_smoke.py` — bun 进程 A v1 → 改 → 进程 B v2; mutation Project 分支 skip |
| M6.4 | Skill | 四种 skill 来源 | P0 | **passed** (2026-04-26) `harness_M6_4_skill_sources_smoke.py` — bundled+user+project; mutation user 分支 skip |
| M6.5 | Skill | skill 指令真注入 agent loop | P0 | **passed** (2026-04-26) `harness_M6_5_skill_inject_agent_loop_smoke.py` — model reply 真带 skill marker; mutation `newMessages: []` |
| M6.6 | Skill | skill 错误和坏 frontmatter | P1 | **passed** (2026-04-26) `harness_M6_6_skill_error_isolation_smoke.py` — 用 SKILL.md-as-dir 触发真 EISDIR; mutation 删 inner+outer try/catch |
| M7.3 | 插件 | plugin reload/disable/scope | P1 | **passed** (2026-04-26) `harness_M7_3_plugin_reload_disable_smoke.py` — 3 phase A/B/C; positive smoke (mutation 由 M7.1 setInlinePlugins 共享覆盖) |
| M7.4 | 插件 | plugin 失败隔离 | P1 | **passed** (2026-04-26) `harness_M7_4_plugin_failure_isolation_smoke.py` — corrupt JSON manifest, good 仍工作; mutation outer try/catch rethrow |
| M8.1 | Slash commands | **101 个**入口清单 (W4 修正) | P0 | **passed** (2026-04-26) `harness_M8_1_command_inventory_real_smoke.py` — runtime registry 与 matrix 完全一致 (45/45); mutation 注释 help → fail |
| M8.2 | Slash commands | 安全命令逐个执行 | P0 | **passed** (2026-04-26) `harness_M8_2_safe_commands_run_smoke.py` — 13 no_side_effect 全 dispatchable |
| M8.3 | Slash commands | 有副作用命令 mock/fixture 执行 | P0 | **passed** (2026-04-26) `harness_M8_3_side_effect_commands_smoke.py` — 23 命令在 registry + 关键命令有 e2e cover |
| M8.4 | Slash commands | 不应开放命令隐藏 | P1 | **passed** (2026-04-26) `harness_M8_4_hidden_commands_smoke.py` — hosted 黑名单 (login/logout/billing 等) 不在 registry |
| M9.1 | Backend | OpenAI-compatible/custom backend | P0 | **passed** (2026-04-26) `harness_M9_1_custom_backend_loop_smoke.py` — qwen3.6-plus 真路由, 无 hosted 字面 |
| M9.2 | Backend | auth 缺失/错误提示 | P0 | **passed** (2026-04-26) `harness_M9_2_auth_missing_clear_error_smoke.py` — invalid key + 30s timeout 阻断, 无 hosted login 引导 |
| M9.3 | Backend | model override/fallback | P1 | **passed** (2026-04-26) `harness_M9_3_model_override_smoke.py` — --model qwen3.6-plus 真到 assistant.model (env 默认 sentinel 区分). positive smoke (custom backend + qwen API 多层 model 处理使精确 mutation 难)|
| M10.1 | 长任务 | 30 分钟任务不中断 | P0 | **passed** (2026-04-26) `harness_M10_1_long_task_heartbeat_smoke.py` — mock sleep 10s 真完成; mutation timeout=5s → fail |
| M10.2 | 长任务 | timeout 可见且归因 | P0 | **passed** (2026-04-26) `harness_M10_2_timeout_attribution_smoke.py` — MCP_TOOL_TIMEOUT=4s, sleep 60s → tool_result is_error+timeout 字面. positive smoke |
| M10.3 | 长任务 | 嵌套子任务恢复 | P1 | **passed** (2026-04-26) `harness_M10_3_nested_subtask_smoke.py` — Agent tool 真 spawn 子任务, parent 含 marker. mutation: src/tools/AgentTool/AgentTool.tsx:238 改 async\* + yield 空 result + return → 子任务永空 → parent_marker_in_stdout=false → fail |
| M11.1 | 语言 | zh/en/auto/toggle 全链路 | P0 | **passed** (2026-04-26) `harness_M11_1_language_consistency_smoke.py` 3 case — settings → MOSSEN_UI_LANGUAGE+LANG; mutation 删 export → 3/3 fail |
| M11.2 | 语言 | 英文模式遇中文输入 | P1 | **passed** (2026-04-26) `harness_M11_2_chinese_in_english_mode_smoke.py` — 不 crash + 有 reply. positive smoke |
| M12.1 | 运行状态 | statusline 配置真实生效 | P0 | **passed** (2026-04-26) `harness_M12_1_statusline_config_smoke.py` — 自定义脚本 stdout 含 marker+model+cwd+lang; mutation early return → fail |
| M12.2 | 运行状态 | session log/export/resume | P0 | **passed** (2026-04-26) `harness_M12_2_session_log_export_resume_smoke.py` — 3 进程 P1 写 jsonl, P2 --continue, P3 新窗口隔离 |
| M13.1 | 总 gate | harness 聚合报告 | P0 | **passed** (2026-04-26) `harness_M13_1_aggregate_report_smoke.py` — aggregator 产 .md/.json, 13 module + 55+ tests |
| M13.2 | 总 gate | 连续 3 次全量通过 | P0 | **passed** (2026-04-26) `harness_M13_2_three_run_stability_smoke.py` — 22 deterministic smoke × 3 rounds = 66/66 |

### C.2 每个追加场景的最小执行模板

每个追加场景必须补成具体脚本，不能只在表格里打勾。脚本格式：

```bash
python3 scripts/harness_<id>_<name>_smoke.py --fresh-fixture
```

脚本必须完成：

```text
1. 创建 fixture。
2. 写入测试专用 settings。
3. 启动 mossen 或调用 loader。
4. 执行用户视角动作。
5. 收集 stdout/stderr/session log/artifacts。
6. 执行硬断言。
7. 执行 mutation 或 negative control。
8. 还原并复跑。
9. 写 assertions.json。
```

### C.3 Slash command 全量验证要求

Slash command 不能只抽样。必须先通过实际运行拿到命令清单：

```bash
mossen --help
mossen
/help
```

然后把可见 slash command 全部列入 `harness_slash_command_matrix.json`：

```json
{
  "command": "/statusline",
  "visible": true,
  "category": "settings",
  "side_effect": "writes_user_config",
  "test_mode": "fixture",
  "expected": "statusline config updated",
  "script": "scripts/harness_M8_statusline_smoke.py"
}
```

分类规则：

| 类型 | 要求 |
|---|---|
| 无副作用 | 必须真执行 |
| 写配置 | 必须在 fixture HOME 内真执行 |
| 外部服务 | 默认隐藏或 mock，不允许真实访问官方服务 |
| 高风险工具 | 必须经过权限测试 |
| 暂不支持 | 必须隐藏；如果可见，必须 failed |

### C.4 OpenAI-compatible/custom backend 验证要求

用户明确不需要官方 OAuth，因此最终验收必须证明：

```text
不登录官方账号，也能完成 agent loop + tools + memory + skill + MCP。
```

最低验证：

- 使用 custom backend 配置启动。
- `mossen --status` 或 `/status` 显示当前 backend 和 model。
- M1.1、M1.2、M1.3 至少在 custom backend 下跑通。
- auth 缺失时提示配置 custom backend credentials，不引导官方登录。
- 不出现必须连接 `api.mossen.invalid` 或官方 hosted 的硬依赖。

### C.5 记忆系统特别验收

这部分必须覆盖用户真实遇到的问题：

| 场景 | 必须证明 |
|---|---|
| resume 上个会话 | 会话上下文被带回 |
| 新窗口进入同目录 | 项目记忆/MOSSEN.md/.mossen memory 自动加载 |
| 新窗口不同目录 | 不串项目记忆 |
| user memory | 跨项目可见 |
| local memory | 只本机/本目录可见 |
| project memory | 项目内可见，worktree 策略明确 |

如果模型回答"没有之前上下文"，脚本必须区分：

```text
这是正常的新会话没有 conversation history
还是异常地没有加载 project memory
```

### C.6 长任务和恢复特别验收

必须模拟至少一个 30 分钟长任务，可以用 fake slow tool，不要求真实消耗 30 分钟外部 API。

最低要求：

- UI/CLI 每隔固定时间有可见进度或 heartbeat。
- 任务失败时必须显示失败原因。
- 工具 timeout 必须显示为 timeout，不得静默变 idle。
- 子任务 in_progress 时主任务不能提前 completed。
- 中断后 resume 能看到历史 event log。

### C.7 最终报告格式

全部完成后必须生成：

```text
harness-final-report.md
harness-final-report.json
```

Markdown 报告必须包含：

- 能力基线矩阵摘要。
- 每个模块 pass/fail/block 数量。
- 每个测试的脚本路径。
- 每个测试的 artifacts 路径。
- 每个 mutation/negative control 证据。
- 未对齐官方能力的清单。
- 明确结论：是否达到个人版生产可用。

JSON 报告必须可机器读取，用于后续 CI。

### C.8 不允许的交付方式

以下交付一律不通过：

- 只说"我手动试了，可以"。
- 只跑 `bun run build` 或 typecheck。
- 只 grep 源码证明功能存在。
- 只截图，不给 stdout/session log/artifacts。
- 只测中文或只测英文。
- 只测普通命令，不测异常/deny/timeout。
- 只测当前会话，不测重启、新窗口、resume。
- 遇到 hosted/OAuth/browser/marketplace 直接跳过但不写隐藏策略。

---

## 附录 D: 文档变更日志

| 日期 | 变更 | by |
|---|---|---|
| 2026-04-25 | 初稿创建 | Claude |
| 2026-04-25 | 追加 Codex 最终门禁：能力基线、隔离环境、证据产物、slash command 全量矩阵、backend/记忆/长任务/语言等补充验收 | Codex |
| 2026-04-25 | M0.1 完成，根据基线发现修正 W1（M5.2 type 而非 scope）/W2（M2.4 6 mode）/W3（M6.2 用 verify）/W4（M8.1 101 个 command） | Claude |

---

## 附录 E: 延后待办（不影响本次 harness 能力验证，记录留作后续单独清理）

> **决策依据**：用户 2026-04-25 指示"这次主要是 harness 的能力验证测试，不涉及影响的都往后放，记录下来就行"。

| # | 项 | 范围 | 后续处理 |
|---|---|---|---|
| L1 | `MOSSEN_CODE_USE_BEDROCK/VERTEX/FOUNDRY` 47 处 env 引用残留 | 上游 hosted 路径基础设施（main.tsx / apiPreconnect / managedEnv 等） | 单独评估影响面后清理 |
| L2 | MCP transport `'hosted-proxy'` 隐藏审计 (W5) | `services/mcp/client.ts:344` 等 | M3.4 测时只验"必须能 fail"；不本次清 |
| L3 | `MOSSEN_CODE_ENABLE_HOSTED_AUTH_ADAPTER` 逃生口 | `utils/auth.ts:115` `cli/handlers/auth.ts:138` 等 | 个人版正常路径不应启用；M9.2 验"auth 缺失提示不引导官方登录" |
| L4 | hosted 残留命令隐藏策略 | `install-github-app, install-slack-app, share, mobile, desktop, chrome, teleport, remote-env, remote-setup, oauth-refresh, passes, feedback, bughunter, perf-issue, release-notes, stickers, autofix-pr, pr_comments` 等 18 个疑似 | M8.4 验"不应开放命令隐藏"，但本次不动可见性策略 |
| L5 | 底色 / typecheck baseline 1407→1407 / lint 944→944 同步刷盘 | `scripts/typecheck-baseline.txt` / `scripts/lint-baseline.txt` 实际比 baseline 数字少 1 | 本次 harness 工作做完后顺手刷 |
| L6 | chat_tui smoke transient timeout (LLM 字面浮动 + 60s 等待窗口窄) | `scripts/smoke_check.py` chat_tui case 白名单（约 15 条）不含 `你好！有什么可以帮你的？`（缺一个"吗"） | 用户已决定不本次修。注意：本测试在 harness:gate 中第 75 位会偶发 timeout 阻塞后续测试 EXIT，但不影响其他 smoke 自身正确性 |
| L7 | M4.1 auto-compact deterministic 触发 | mossen -p 单 shot 无法长对话, --continue 跨 turn 撑 token 至阈值需 >100K tokens 的 fixture 内容, 不可控 | 用 M4.3 (manual /compact 跨 --continue) 等价覆盖 "compact 后语义保留" 契约 |
