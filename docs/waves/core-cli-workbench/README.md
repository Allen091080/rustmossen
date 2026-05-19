# Core / CLI / Workbench 分层边界

> 状态: CWB-1/CWB-2 第一批入仓。
> 目标: 在开始 Workbench、Web、Mobile 或扩展系统实现前，先把 Core 稳定边界写成仓库内可执行约束。

## 1. 一句话原则

Mossen Core 负责可靠执行，CLI 负责最小稳定交互，Workbench/Web/Mobile 负责丰富展示，Extensions 负责能力增长。

Workbench 和扩展系统不能直接改 Core 内部状态，也不能把 CLI 渲染树当作桌面端 UI 基座。

## 2. 当前决策

| 决策 | 结论 |
| --- | --- |
| CLI 定位 | 保持最基础、最稳定、最可信的核心通道，可作为二进制供其它应用调用。 |
| UX-TUI 面板化 | 暂停，不在 CLI 中继续做固定顶栏、侧栏、三列布局。 |
| Workbench 第一阶段 | 独立 Surface，通过 CLI binary subprocess + stream-json 接入。 |
| Web / Mobile 第一阶段 | 同 Workbench，不 in-process import Core。 |
| Extensions | 通过 manifest / gateway / permission / event stream 接入，不 direct import Core 私有实现。 |
| in-process SDK | 推迟到协议、类型和审计稳定后再开放。 |

## 3. 本批次入仓内容

| 文件 | 作用 |
| --- | --- |
| `docs/waves/core-cli-workbench/layer-boundaries.md` | 详细分层边界和允许/禁止依赖方向。 |
| `docs/reference/layer-boundary-rules.md` | 静态审计规则说明和维护流程。 |
| `scripts/layer-boundary-rules.json` | 审计规则的机器可读版本。 |
| `scripts/layer_boundary_audit.py` | TS/TSX/JS/JSX import 方向审计。 |
| `scripts/run_all_smoke.sh` | 接入 layer boundary audit。 |

## 4. 后续开发入口

启动 Workbench、扩展系统、自进化或 Core migration 前，执行 AI 必须先读:

1. `docs/reference/red-lines.md`
2. `docs/architecture-boundaries.md`
3. `docs/waves/core-cli-workbench/layer-boundaries.md`
4. `docs/reference/layer-boundary-rules.md`
5. `docs/reference/smoke-runner.md`

如果实现需要突破本目录定义的禁止方向，不能直接改代码，必须先写新的施工包并等 Allen 拍板。

