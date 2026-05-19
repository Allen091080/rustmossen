# Core / CLI / Workbench 分层边界

> 来源: Desktop 边界文档、扩展系统协议、自进化治理模型、UX-TUI 暂停决策，以及 Wave 0-4 的红线沉淀。
> 范围: Mossen Core、classic CLI、Workbench/Web/Mobile Surface、Protocol/SDK、Extensions、Evolution/Repair。

## 1. 分层模型

```text
Stable Core
  agent loop / tools / permissions / memory / context / MCP / skill/plugin loader / stream-json

Classic CLI
  binary entrypoint / args / interactive TUI / print mode / stream-json host

Protocol / SDK
  stream-json / runtime snapshot / manifest / future panel-state / future typed SDK

Surfaces
  Workbench desktop / Web / Mobile / future IDE companion

Extensions
  skills / plugins / workflows / prompt packs / language packs / panel providers / hooks definitions

Evolution / Repair
  diagnostics / repair reports / private source repair pipeline / signed updates
```

Core 是可靠执行层。CLI 是默认可用通道。Workbench/Web/Mobile 是展示层。Extensions 增加能力，但不静默改变 Core 默认行为。

## 2. 允许依赖方向

| 方向 | 说明 |
| --- | --- |
| CLI -> Core | CLI 可以调用 Core 来完成启动、输入、agent loop、工具和权限。 |
| CLI -> Protocol | CLI 可以承载 print / stream-json 等协议输出。 |
| Workbench/Web/Mobile -> CLI binary subprocess | 第一阶段唯一推荐接入方式。 |
| Workbench/Web/Mobile -> Protocol output | 解析 stream-json、runtime snapshot、manifest、diagnostics。 |
| Extensions -> Gateway / manifest / permission declaration | 扩展通过稳定入口声明能力。 |
| Core -> Protocol types | Core 可以产出稳定协议事件。 |

## 3. 禁止依赖方向

| 禁止方向 | 原因 |
| --- | --- |
| Workbench/Web/Mobile -> `query.ts` / `QueryEngine.ts` | 会把 Surface 绑死在 agent loop 私有实现上。 |
| Workbench/Web/Mobile -> `screens/REPL.tsx` | 会把桌面端当 CLI 渲染树复用，重演 UX-TUI 问题。 |
| Workbench/Web/Mobile -> `bootstrap/state.ts` / `state/AppStateStore.ts` | 会绕过 Core 状态边界。 |
| Workbench/Web/Mobile -> `Tool.ts` / `tools/*` | 会绕过工具执行契约和权限。 |
| Extensions -> `utils/permissions/*` / `hooks/toolPermission/*` | 会绕过权限策略。 |
| Extensions -> `tools/*` executor | 会把扩展变成 Core 私有实现插件。 |
| Protocol / SDK -> React/Ink components | 协议层必须 UI-agnostic。 |
| Core -> Workbench/Web/Mobile implementation | Core 不能依赖某个 Surface 才能运行。 |

## 4. Core 保护域

以下默认按 Core 或 Core-adjacent 处理:

```text
main.tsx
query.ts
QueryEngine.ts
Tool.ts
Task.ts
commands.ts
cli/*
bootstrap/state.ts
state/AppStateStore.ts
screens/REPL.tsx
components/PromptInput/*
components/Messages.tsx
components/VirtualMessageList.tsx
components/FullscreenLayout.tsx
tools/*
utils/permissions/*
hooks/useCanUseTool.tsx
hooks/toolPermission/*
services/api/*
services/modelRuntime/*
services/compact/*
utils/memory/*
services/mcp/*
utils/plugins/*
utils/skills/*
cli/structuredIO.ts
cli/print.ts
```

Core 可以改，但必须是 Core bugfix、Core migration 或协议稳定化任务。不能为了 Workbench 视觉需求直接改 Core。

## 5. CLI 边界

CLI 继续保留:

- interactive CLI
- print mode
- stream-json mode
- slash command
- statusline
- permissions dialog
- message rendering
- Vim/input capability

CLI 不继续做:

- fixed header
- sidebar
- three-column layout
- forced fullscreen as default UX
- Workbench-like render tree

原因是传统 terminal scrollback 模式无法稳定 pin 顶栏/侧栏。面板化诉求转入 Workbench 桌面端。

## 6. Workbench 第一阶段边界

第一阶段 Workbench 必须独立于 mossensrc Core 源码运行:

```text
Workbench process
  -> spawn mossen binary
  -> consume stream-json events
  -> read runtime/manifest/diagnostic JSON when available
  -> maintain Workbench-local UI state
```

禁止第一阶段:

- vendoring `mossensrc`
- import `mossensrc/query.ts`
- import `mossensrc/screens/REPL.tsx`
- import `mossensrc/bootstrap/state.ts`
- import `mossensrc/Tool.ts`
- import `mossensrc/tools/*`
- patch Core files from Workbench

## 7. Extensions 边界

扩展负责增加能力，不负责静默改变核心行为。

扩展可以:

- 声明 skill / plugin / workflow / prompt pack / language pack
- 声明权限需求
- 通过 gateway 请求执行
- 输出 event / panel data / diagnostics

扩展不可以:

- direct import Core 私有实现
- 绕过权限
- 静默改变默认模型、权限模式、记忆写入、上下文压缩、agent loop
- 在用户端自动 patch Core

## 8. 自进化边界

自进化可以生成建议、诊断包、repair report、workflow 草稿和扩展草稿。

自进化不能在用户机器上直接修改 Core 源码。Core 修复必须走私有 repair pipeline、完整验证和签名更新。

## 9. 静态审计

本边界由 `scripts/layer_boundary_audit.py` 和 `scripts/layer-boundary-rules.json` 执行第一版 import 方向审计。

第一版重点抓四类高风险:

1. Workbench/Surface direct import Core internals。
2. 外部 Extensions direct import permission/tool internals。
3. Protocol/SDK import React/Ink UI。
4. Core import Workbench/Surface implementation。

审计接入 `scripts/run_all_smoke.sh`，任何新增违规都必须在 commit 前修复或停下汇报 Allen。

