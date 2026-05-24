# Mossen

Mossen 是基于这套源码持续打磨出来的**个人单机版软件工程 AI CLI**。
这套仓库当前目标不是重写官方产品，而是尽量在**个人可用、可控后端、可持续维护**的前提下，把用户可见体验和核心行为语义逐步对齐到官方水平。

## 当前状态

- 品牌名：`Mossen`
- CLI 命令：`mossen`
- 当前默认自定义后端：`Qwen 3.6 Plus`
- 运行方式：可在源码目录外直接全局调用
- 主要交互语言：支持**按用户对话语言动态切换**中英运行态提示

## 快速启动

```bash
npm install
npm link
mossen
```

常用验证命令：

```bash
python3 scripts/smoke_check.py
python3 scripts/personal_acceptance_check.py
python3 scripts/personal_acceptance_check.py --with-extended-real-tasks
```

## 这轮主要改了什么

### 1. 品牌升级

- 将高频用户可见品牌统一为 `Mossen / 🍃`
- 欢迎页、Logo、Header、状态说明、命令帮助、OAuth/MCP/IDE/插件/权限等大量用户面文案已收口
- 旧品牌、通用 coding assistant 文案和 Max marketing 已大幅清理
- spinner 已改成绿色品牌风格，并使用叶子动画帧序列：
  - `🍃 → 🌿 → ☘️ → 🍀 → ☘️ → 🌿`

### 2. 个人版能力收口

- 构建了个人单机版 acceptance/smoke 主闸门
- 核心能力已稳定验证：
  - slash 命令链
  - tool use
  - 多轮任务/真实 coding task
  - permissions 模式
  - resume / rewind / branch / model / compact / context
  - local/cloud model tier
  - profile / effort / execution / reasoning

### 3. 与官方行为语义对齐

- 不再只做“协议兼容层”，而是继续推进“官方语义等价层”
- 已补齐或增强的关键语义：
  - thinking / text 分层
  - tool use / tool result roundtrip
  - continuation / stop / fallback
  - provider parity
  - context window / 1M gating 真值驱动

### 4. 语言体验

- 运行态提示不再只靠系统语言，而是尽量跟随用户当前问话语言
- 中文环境下，大量原本固定英文的提示已切到动态中英
- 已覆盖大量高频和低频用户面，包括：
  - `/help`
  - `/doctor`
  - `/status`
  - `/context`
  - `/ide`
  - `/hooks`
  - `/memory`
  - `/resume`
  - `/session`
  - `/usage`
  - `/mcp`
  - permissions 相关弹层

### 5. 打包和启动方式

- 已支持在**非源码目录**下直接用 `mossen` 启动
- 当前采用的是全局 launcher/link 方案，不是独立二进制发布
- 优点是：源码更新后，`mossen` 立即生效

## 这轮解决过的关键问题

### 1. `-p` 模式挂住

问题：
- 普通 `-p` 会卡住，但 `--output-format stream-json` 能正常完成

定位：
- 挂点在 `executeStopHooks()` 这条链

处理：
- 对 `sdk + custom-backend` 会话绕过不安全的 stop hook 路径

### 2. spinner 假转圈 / 任务已完成但还在转

问题：
- 主任务已经结束，但 spinner 仍继续旋转

定位：
- 主 spinner 之前看的是整条命令队列，子代理内部队列也会误计入

处理：
- 改成只看主线程可处理队列
- 顺手切换到绿色叶子品牌 spinner

### 3. checklist 残留

问题：
- 上一轮任务完成后，旧 checklist 仍挂在下一轮任务中

处理：
- 收紧 checklist 完成态和清理逻辑
- 强化 task update prompt，要求边做边更新而不是最后一次性结算

### 4. LSP 报错

问题：
- `LSP for typescript-lsp failed`

定位：
- 缺少 `typescript-language-server` / `typescript`

处理：
- 补全依赖
- LSP manager 增加二进制缺失保护：缺依赖时跳过 server，而不是拖垮整条初始化链

### 5. 1M 上下文假开

问题：
- 前端如果写死 `[1m]`，会给用户错误预期

处理：
- 改为**以后端能力真值为准**
- 只有显式声明 `MOSSEN_CODE_CUSTOM_MAX_INPUT_TOKENS>=1000000` 才开放 1M
- `auth status --text` 和 `/status` 现已显示真实 `Context window`

### 6. 运行态中英文不一致

问题：
- fresh session、spinner、帮助页、低频弹层中仍残留大量英文

处理：
- 大规模把固定英文用户面切到动态中英
- 保证中文对话下，除代码/命令/模型名等特殊词外，用户面尽量中文

## 当前验证结果

最近一轮明确跑过并通过的主验证包括：

- `python3 scripts/smoke_check.py`
- `python3 scripts/personal_acceptance_check.py`
- `python3 scripts/personal_acceptance_check.py --with-extended-real-tasks`
- 多项 targeted audits：
  - provider parity
  - mcp command audit
  - LSP binary guard
  - language/runtime audits

## 当前已知边界

下面这些目前**不算个人单机版 blocker**，但要明确知道：

- `direct-connect`
- `ssh remote`
- hosted bridge / remote attach
- team memory sync / hosted workflow

原因主要有两类：

1. 当前快照缺源码模块
2. 这些能力依赖官方 hosted 服务，不是单机版能完整替代的

## 还剩哪些问题

### 1. 第二层品牌残留仍可能有零散漏网

虽然高频用户面已经基本收口，但低频路径里仍可能还有少量旧品牌 / 老命令名残留。
这类问题一般不再是主链阻塞，而是继续打磨项。

### 2. 语言体验仍需要继续真实使用中观察

虽然动态中英切换已经铺开，但最终标准仍然是：

- 用户用中文对话：运行态尽量中文
- 用户用英文对话：运行态尽量英文

这个需要继续靠真实使用把边角漏口收完。

### 3. 底层 legacy 命名迁移已切到专门分支推进

这条线现在已经不再和 `main` 主线混做。
高风险命名迁移被隔离到专门分支上单独推进，用来集中处理底层路径、说明文件和环境变量真值的整体替换。

## 后续建议

如果以后继续推进，我建议按这个顺序：

1. 继续清理第二层品牌残留
2. 继续用真实项目压语言切换和 spinner/checklist 稳定性
3. 继续在专门迁移分支上验证底层 legacy 命名到 Mossen 真值的整体替换
4. 如需分发给其他机器，再做真正的独立发布包，而不是仅用 `npm link`

## 备注

- 本仓库当前是**个人单机版 Mossen 主线**
- 目标是“尽量对齐官方的核心行为语义和用户可见体验”
- 但这**不等于**不同底模（例如 Qwen/GPT/DeepSeek）的智力水平会天然等于任何特定闭源模型

如果你后面再回来维护这套仓库，建议先看：

1. 本 README
2. `PLAN_PRODUCTION.md`（生产化路线图）
3. `scripts/smoke_check.py`
4. `scripts/personal_acceptance_check.py`
5. `AGENTS.md`
