# Mossen 整体链路 Harness 设计 v1.0

> 制定日期：2026-04-25
> 起点：bridge 删除 11 条 smoke 全过 TUI 还崩了两次（参见 commits `14a709f` `622767a`）
> 目标：覆盖 Mossen 个人版"必须不能断"链路，9 层分布

## 0. 起源

`scripts/smoke_check.py` 现有 79 条 smoke / audit，但 bridge 删除暴露：
- TUI 真渲染没有 case（只有 `chat_tui` 用 PTY，但启动 trust 一弱就滑过）
- lazy-loaded 组件 dangling import 没人抓（hotfix #2 起因）
- 删 state 字段后悬空 read 没人抓（hotfix #1 起因）

P0-05 / P0-06 的 typecheck:diff + lint:diff 部分填补了上面 2 / 3，但**真渲染 + lifecycle + 异常注入**还有黑区。

## 1. 9 层覆盖矩阵 v1.0

| 层 | 内容 | 当前覆盖 | 当前缺口 | 何时加固 |
|---|---|---|---|---|
| **L0 启动链** | `--help` / `--version` / TUI 真渲染 / 多 cwd 启动 | 部分（`chat_tui` 21s 真 LLM + `statusline_tui` PTY 渲染）| 多 cwd 启动验证 / dangling import 早期抓取 | typecheck:diff 已挡 |
| **L1 命令注册** | 95+18 命令存在 / 名称冲突 / hidden gate 一致 | 强（`command_inventory` 79/79 + 多个 `*_command_audit` smoke）| 每条命令"点进去"动行为 | P1-5-B 用户本机 |
| **L2 工具调用** | Bash/Edit/Read/Grep/Glob/Write/Task 等真跑 | 中（`agentic_tool_loop_runtime_audit` 部分覆盖）| 每个 tool 单独 e2e + 失败路径 | 后续 |
| **L3 状态机 / lifecycle** | 长任务、子任务、超时、恢复 | 弱（`long_task_scenarios` 骨架 7 场景，全 skip 等用户本机）| s1-s7 真跑 | P1-1 用户本机 |
| **L4 配置 / 环境** | settings 优先级 / env / profile 切换 | 部分（`config_command_audit` + `auth_command_audit`）| 多 profile 真切换 | 后续 |
| **L5 记忆 / 会话** | 跨窗口 / resume / MOSSEN.md 自动加载 | 中（`cross_window_memory` 4/4 不变量）| 真起 2 个 mossen 进程跑 /memory add | 后续 |
| **L6 权限模式** | 切换 / 持久化 / UI 一致性 | 弱（permission audit 只静态）| Shift+Tab 真切换 + UI 验 | 后续 |
| **L7 真实 LLM 闭环** | 多轮工具 / 失败重试 / 超时 / 中断 | 中（`chat_tui` 21s + `agentic_tool_loop_runtime_audit`）| 多轮 / 失败注入 | 部分 P1-1 |
| **L8 异常注入** | API 超时 / token 耗尽 / 工具崩 / 文件丢 | **零** | 全部 | 本设计 + dev env override |

## 2. 现有 smoke 归档（按层）

按 `python3 scripts/smoke_check.py --list-checks` 提取 79 条，初步归类（有重叠时取主层）：

### L0 启动链
- `chat_tui` — PTY 真起 TUI + 中文对话 + Ctrl+C
- `statusline_tui` — PTY 渲染状态栏验证
- `chrome_tui` — Chrome 集成入口
- `doctor_tty` — `/doctor` PTY 验证

### L1 命令注册
- `command_inventory` — 95 用户可见 / 18 stub / 114 total
- `command_gap_audit` / `dormant_command_audit` / `maintenance_command_audit`
- `auth_command_audit` / `plugin_command_audit` / `mcp_command_audit` / `config_command_audit`
- `fast_path_audit`（hotfix #4 精修后只剩 tmux）
- 各种 `*_option_audit` / `*_surface_audit`（约 20 条）

### L2 工具调用
- `agentic_tool_loop_runtime_audit` — 真 LLM 闭环（部分 tool）

### L3 lifecycle
- `long_task_scenarios` — 7 场景骨架（默认 skip）

### L4 配置 / 环境
- `external_bridge_boundary_audit` — bridge 已删后只验"无外部依赖"
- `dependency_audit` / `platform_check` / `feature_audit`

### L5 记忆 / 会话
- `cross_window_memory` — 4 不变量（路径计算 + disable gate）

### L6 权限模式
- 部分 `*_permission_audit`（静态字符串匹配）

### L7 真实 LLM
- `chat_tui` — 真发"你好"对话
- `agentic_tool_loop_runtime_audit` — 多轮工具

### L8 异常注入
- **暂无**

## 3. 何时算"通过"

每层独立判定：

| 层 | 通过判定 |
|---|---|
| L0 | TUI 在 mossensrc / 临时 dir / 非 git dir 各能起来 + Ctrl+C 干净退出 |
| L1 | inventory 数量稳定 ± 5 / 0 命令冲突 / hidden gate 不矛盾 |
| L2 | 7 个核心 tool 各 1 次成功调用（Bash/Edit/Read/Grep/Glob/Write/Task） |
| L3 | s1+s2+s4+s5+s6 通过（5/7） |
| L4 | settings 4 source 优先级正确 / 多 env 切换无残留 |
| L5 | 跨窗口 4 不变量 + 真双进程 /memory add+list |
| L6 | 6 mode × Shift+Tab × UI 一致 + plan 进入提示 |
| L7 | 多轮工具 ≥3 / 失败重试 ≥1 / 超时恢复 ≥1 |
| L8 | 5 类异常注入（API 超时 / token 耗尽 / 工具崩 / 文件丢 / 网络丢） — 全部优雅恢复 |

## 4. `bun run harness` 包装

承接现有 smoke + 未来 layer-specific 扩展：

```bash
bun run harness                # = python3 scripts/smoke_check.py
bun run harness:layer L0       # 仅 L0 子集（暂未实现）
bun run harness:layer L8       # L8 异常注入（暂未实现）
```

## 5. 不变量：bridge 两次崩溃必须能被抓到

凡是删 bridge 那种"删字段 / 删模块"导致的 dangling refs，**必须**在 commit 前被 P0-05 的 `typecheck:diff` 抓出。已经验证：

- 删 state 字段 → `typecheck:diff` 立即报 TS2339 (Property does not exist)
- 删 module 导致 dangling import → `typecheck:diff` 立即报 TS2307 (Cannot find module)
- React render-time throw → `MossenErrorBoundary` 包的组件 fallback + logError，不再静默 unmount

**这三层（typecheck:diff + lint:diff + MossenErrorBoundary）**联合起来 = bridge 两次崩溃在 P0-05 之后**不可能再发生**。

## 6. 演进路径

按 P1 沉淀依次填充：

1. **P1-1 真跑后** → L3 / L7 标准 case 入库
2. **P1-4 真跑后** → L5 双进程 case 入库
3. **P1-5-B 完成后** → L1 命令试用结论入库
4. **P1-6 完善后** → L6 6 mode UI 一致 case 入库
5. **专门做 L8** → dev env override + 5 类注入

## 7. 当前总数

- 79 smoke / audit（包括 5 个 P1 新增：cross_window_memory / command_inventory / long_task_scenarios + 已有的 chat_tui / statusline_tui）
- 1488 typecheck baseline 错误（gate 防新增）
- 945 lint baseline 问题（gate 防新增）
- 4 层（L0/L1/L5/L7）覆盖 ≥3 case，3 层（L2/L4/L6）部分，L3 骨架，L8 空缺

`bun run harness` 当前等价于 `python3 scripts/smoke_check.py`。layer 子集模式留作 P0-08 后续 slice。
