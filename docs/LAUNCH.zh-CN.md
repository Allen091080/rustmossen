# Mossen 推广文案

[English](LAUNCH.md) | [简体中文](LAUNCH.zh-CN.md)

这个文件用于 Mossen V1.1.0 发布时直接复制使用。

## 定位

Mossen 是一个本地优先、Rust 原生的终端编码代理，面向希望用自有模型供应商获得 Claude Code 风格工作流的开发者。

## 简短介绍

Mossen 是一个用 Rust 编写的开源终端编码代理。它支持对话式编码、文件编辑、带权限控制的 Shell 执行、斜杠命令、上下文管理、MCP、子 Agent，以及可配置模型供应商。

## 一句话介绍

Mossen 是一个 Rust 原生、本地优先的终端编码 Agent，提供 Claude Code 风格工作流，并让用户自己控制模型供应商。

## V2EX / 即刻 / 中文社区草稿

标题：

```text
开源 Mossen：一个 Rust 原生的终端编码 Agent
```

正文：

```text
我发布了 Mossen V1.1.0，一个用 Rust 写的终端编码 Agent。

它的目标是用本地优先、模型供应商可控的方式，实现 Claude Code 风格的开发工作流：

- 对话式编码
- 文件读取和编辑
- 带权限控制的 Shell 执行
- 斜杠命令
- 上下文管理
- MCP 集成
- 子 Agent 任务
- 终端 TUI 和渲染

模型配置支持：

- OpenAI-compatible Chat Completions
- OpenAI Responses-compatible endpoint
- Anthropic-compatible endpoint

API key 存在本机 ~/.mossen/settings.json，不放进仓库。

GitHub: https://github.com/Allen091080/rustmossen
中文 README: https://github.com/Allen091080/rustmossen/blob/main/README.zh-CN.md
配置说明: https://github.com/Allen091080/rustmossen/blob/main/docs/CONFIGURATION.zh-CN.md

V1.1.0 是面向外部用户可试用的源码发布。快速启动、provider 配置、`/doctor` 诊断、子 Agent 反馈，以及 TUI 滚动/复制/输入响应都做了收口。现在最希望收到的反馈是：构建是否顺利、模型供应商兼容性、终端交互体验、子 Agent 行为，以及 Rust 架构上的问题。
```

## 知乎 / 掘金文章开头

```text
过去一段时间，我一直在做 Mossen：一个 Rust 原生的终端编码 Agent。

它不是对某个现有工具的简单包装，而是从 Agent 运行时、TUI、命令系统、工具执行、模型桥接、MCP、子 Agent、harness 验证等部分开始，尝试用 Rust 完整实现一套 Claude Code 风格的终端编码工作流。

这次发布的是 V1.1.0 源码版本。它优先解决外部用户能不能从 README 快速跑起来、模型配置失败时能不能自助定位、子 Agent 执行是否有完整反馈、以及 TUI 基础交互是否进入可验证修复路径的问题；安装包、分发体验和更长时间的真实 provider 稳定性验证会继续推进。
```

## X / 推特中文草稿

```text
我发布了 Mossen V1.1.0：一个 Rust 原生的终端编码 Agent。

它提供 Claude Code 风格的本地终端工作流：
- 文件编辑
- 带权限的 Shell 执行
- 斜杠命令
- 上下文管理
- MCP
- 子 Agent
- 可配置模型供应商

https://github.com/Allen091080/rustmossen
```

## B 站 / 视频脚本

建议做 3-5 分钟短视频：

1. 说明 Mossen 是什么：Rust 原生、本地优先、终端编码 Agent。
2. 展示从源码构建。
3. 展示添加模型 profile，使用占位符或打码 key。
4. 进入 TUI，让它分析一个小项目。
5. 让它修改文件并运行测试。
6. 展示 `/help` 或 `/model`。
7. 最后展示 GitHub README 和中文配置文档。

不要展示真实 API key、私有仓库路径、私有对话记录或 provider 私有输出。

## 推荐 GitHub Topics

```text
rust
cli
terminal
tui
coding-agent
developer-tools
llm
mcp
openai-compatible
local-first
```

## 推广检查清单

- README 顶部定位清楚。
- CI badge 和 license badge 正常显示。
- GitHub release 已创建。
- 仓库 description 和 topics 已设置。
- 准备一段 60-90 秒 demo GIF 或视频。
- 发布后一周重点回复 issue、修构建问题、补配置文档。
