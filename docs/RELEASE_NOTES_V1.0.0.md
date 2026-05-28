# Mossen V1.0.0 Release Notes

[English](#english) | [简体中文](#简体中文)

## English

Mossen V1.0.0 is the first public source release of Mossen, a local-first, Rust-native terminal coding agent.

Mossen implements the core Claude Code-style developer workflow in a Rust workspace: conversational coding, file editing, shell execution with permissions, slash commands, context management, MCP integration, sub-agents, and terminal rendering.

Mossen is not affiliated with Anthropic or Claude Code. It is an open implementation of the same class of terminal coding-agent experience, designed for users who want to control their own model providers and credentials.

### Highlights

- Rust-native agent runtime, CLI, TUI, tool execution, command system, and harnesses.
- Interactive TUI and one-shot execution modes.
- Built-in tools for reading, writing, editing, searching, shell execution, planning, MCP, and sub-agent task orchestration.
- Slash command system for common coding-agent workflows.
- Model profiles for `openai-compatible`, `openai-responses`, and `anthropic` providers.
- Local credential storage under `~/.mossen/settings.json`.
- Public configuration guide and non-secret example settings.
- Release harnesses and smoke scripts for core workflow validation.

### Install From Source

```bash
git clone https://github.com/Allen091080/rustmossen.git
cd rustmossen
cargo build --release -p mossen-cli --bin mossen
./target/release/mossen
```

Local install:

```bash
cargo install --path crates/mossen-cli --bin mossen --locked
```

### Configure A Model Provider

```bash
mossen --add-model-profile my-model \
  --provider openai-compatible \
  --baseURL https://api.example.com/v1 \
  --model your-model-name \
  --apiKey "$YOUR_API_KEY"

mossen --set-model-profile my-model
mossen --test-model-profile my-model --timeout 30000
```

See [CONFIGURATION.md](CONFIGURATION.md) for the full configuration guide.

### Current Scope

V1.0.0 is a source release focused on core functionality. Installer packaging and long-running real-provider soak coverage can continue after the source release. The current goal is to make the core local coding-agent workflow buildable, usable, and configurable from source.

### Safety Notes

- Do not commit `~/.mossen/settings.json`, project `.mossen/`, `.env`, or real provider credentials.
- Public examples use placeholders such as `<your-api-key>`.
- Run [OPEN_SOURCE_CHECKLIST.md](OPEN_SOURCE_CHECKLIST.md) before publishing forks or release builds.

## 简体中文

Mossen V1.0.0 是 Mossen 的第一个公开源码版本。Mossen 是一个本地优先、Rust 原生的终端编码代理。

Mossen 在 Rust workspace 中实现 Claude Code 风格的核心开发工作流：对话式编码、文件编辑、带权限控制的 Shell 执行、斜杠命令、上下文管理、MCP 集成、子 Agent 和终端渲染。

Mossen 与 Anthropic 或 Claude Code 没有关联。它是同类终端编码 Agent 体验的开放实现，面向希望自己控制模型供应商和凭据的用户。

### 亮点

- Rust 原生 Agent 运行时、CLI、TUI、工具执行、命令系统和 harness。
- 支持交互式 TUI 和一次性任务模式。
- 内置读取、写入、编辑、搜索、Shell 执行、计划、MCP 和子 Agent 任务编排工具。
- 用于常见编码 Agent 工作流的斜杠命令系统。
- 支持 `openai-compatible`、`openai-responses` 和 `anthropic` provider 的模型 profile。
- 凭据本地保存在 `~/.mossen/settings.json`。
- 公开配置指南和不含密钥的示例配置。
- 用 release harness 和 smoke scripts 覆盖核心工作流验证。

### 从源码安装

```bash
git clone https://github.com/Allen091080/rustmossen.git
cd rustmossen
cargo build --release -p mossen-cli --bin mossen
./target/release/mossen
```

本地安装：

```bash
cargo install --path crates/mossen-cli --bin mossen --locked
```

### 配置模型供应商

```bash
mossen --add-model-profile my-model \
  --provider openai-compatible \
  --baseURL https://api.example.com/v1 \
  --model your-model-name \
  --apiKey "$YOUR_API_KEY"

mossen --set-model-profile my-model
mossen --test-model-profile my-model --timeout 30000
```

完整配置说明见 [CONFIGURATION.zh-CN.md](CONFIGURATION.zh-CN.md)。

### 当前范围

V1.0.0 是聚焦核心功能的源码发布。安装包、分发体验和真实 provider 长稳压测可以在源码发布之后继续推进。当前目标是让核心本地编码 Agent 工作流能够从源码构建、使用和配置。

### 安全说明

- 不要提交 `~/.mossen/settings.json`、项目 `.mossen/`、`.env` 或真实 provider 凭据。
- 公开示例使用 `<your-api-key>` 这类占位符。
- 发布 fork 或 release build 前，请运行 [OPEN_SOURCE_CHECKLIST.zh-CN.md](OPEN_SOURCE_CHECKLIST.zh-CN.md)。
