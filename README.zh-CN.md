# Mossen

[English](README.md) | [简体中文](README.zh-CN.md)

[![CI](https://github.com/Allen091080/rustmossen/actions/workflows/ci.yml/badge.svg)](https://github.com/Allen091080/rustmossen/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust 1.80+](https://img.shields.io/badge/rust-1.80%2B-orange.svg)](rust-toolchain.toml)

Mossen 是一个本地优先、Rust 原生的终端编码代理，面向希望用自有模型供应商获得 Claude Code 风格工作流的开发者。

它在同一个 Rust workspace 中实现对话式编码、文件编辑、带权限控制的 Shell 执行、斜杠命令、上下文管理、MCP 集成、子 Agent 和终端渲染。

Mossen 与 Anthropic 或 Claude Code 没有关联。本项目的目标是提供同类开发体验的开放 Rust 实现，让用户自己控制模型供应商、Base URL 和凭据。

## 为什么是 Mossen

- Rust 实现：Agent 运行时、TUI、命令系统、工具执行、模型桥接和 harness 都在同一个 Rust workspace 中。
- 模型供应商灵活：支持 OpenAI-compatible Chat Completions、OpenAI Responses-compatible endpoint，以及 Anthropic-compatible endpoint。
- 本地优先配置：模型 key 存在用户本机 Mossen 配置中，不放进仓库。
- 终端原生工作流：交互式 TUI、流式输出、权限、斜杠命令、历史、压缩和任务反馈是一等能力。
- 工程 Agent 能力面：内置读取、编辑、搜索、Shell 执行、计划、MCP 和子 Agent 任务编排工具。
- 可测试发布路径：核心运行时行为由 harness 脚本和 smoke tests 覆盖，而不是只依赖人工演示。

## 状态

当前公开版本：**V1.1.0**。当前开发目标：**V1.2 Reliability & Production UX**。

V1.1 是面向外部用户可试用的发布版本。重点是 5 分钟源码安装路径、清晰的模型配置引导、provider 兼容性检查、子 Agent 生命周期反馈，以及 TUI 滚动、复制、输入延迟的专项覆盖。

V1.1 发布说明见 [docs/RELEASE_NOTES_V1.1.0.md](docs/RELEASE_NOTES_V1.1.0.md)，推广文案见 [docs/LAUNCH.zh-CN.md](docs/LAUNCH.zh-CN.md)。

## 环境要求

- Rust 1.80 或更新版本
- macOS 或 Linux
- Git
- 推荐安装：`rg`，用于快速搜索仓库内容

## 5 分钟快速启动

从源码安装 `mossen` 命令：

```bash
git clone https://github.com/Allen091080/rustmossen.git
cd rustmossen
cargo install --path crates/mossen-cli --bin mossen --locked
```

添加第一个模型 profile。把下面的 endpoint、模型名和 API key 换成你自己的：

```bash
mossen --add-model-profile my-model \
  --provider openai-compatible \
  --baseURL https://api.example.com/v1 \
  --model your-model-name \
  --apiKey "$YOUR_API_KEY"
```

启用并测试它：

```bash
mossen --set-model-profile my-model
mossen --test-model-profile my-model --timeout 30000
```

在你的项目中启动 Mossen：

```bash
mossen --cwd /path/to/project
```

如果还没有配置模型，或 provider 调用失败，进入 TUI 后运行 `/doctor`。`/doctor` 会提示缺失 profile、无效 active profile、不完整的 custom-backend 环境变量，以及下一条应该执行的命令，同时不会打印原始 API key 或 Base URL。

## 构建

```bash
cargo build --release -p mossen-cli --bin mossen
```

运行 release binary：

```bash
./target/release/mossen
```

开发时可以使用仓库启动脚本：

```bash
scripts/start-mossen.sh
```

该脚本会在 Rust 源码变化后自动重新构建。如果已有 debug binary，并希望跳过构建：

```bash
MOSSEN_START_BUILD=never scripts/start-mossen.sh
```

从当前 checkout 安装到本机：

```bash
cargo install --path crates/mossen-cli --bin mossen --locked
```

## 快速使用

交互模式：

```bash
mossen
```

一次性任务模式：

```bash
mossen --oneshot "Explain the current repository structure"
```

输出 stream JSON：

```bash
mossen --oneshot "List the test commands for this project" --emit stream-json
```

指定工作目录：

```bash
mossen --cwd /path/to/project
```

进入 TUI 后，可以用 `/help` 查看可用斜杠命令。对于长任务，可以用
`/goal <objective>` 设置持久线程目标；`/goal` 查看当前进度，`/goal pause`
暂停，`/goal resume` 恢复，`/goal edit <objective>` 更新目标，`/goal clear`
清除目标。

## 配置 LLM 供应商

Mossen 的模型 profile 存在：

```text
~/.mossen/settings.json
```

不要提交这个文件。它可能包含 API key。

创建 profile：

```bash
mossen --add-model-profile my-model \
  --provider openai-compatible \
  --baseURL https://api.example.com/v1 \
  --model your-model-name \
  --apiKey "$YOUR_API_KEY"
```

启用 profile：

```bash
mossen --set-model-profile my-model
```

查看已配置 profile：

```bash
mossen --list-model-profiles
```

测试 profile：

```bash
mossen --test-model-profile my-model --timeout 30000
```

支持的 `provider` 值：

- `openai-compatible`
- `openai-responses`
- `anthropic`

完整配置说明见 [docs/CONFIGURATION.zh-CN.md](docs/CONFIGURATION.zh-CN.md)，英文版见 [docs/CONFIGURATION.md](docs/CONFIGURATION.md)。示例配置文件见 [examples/settings.example.json](examples/settings.example.json)。

## 安全

- 不要提交 `~/.mossen/settings.json`、项目 `.mossen/`、`.env`，或任何包含真实 API key 的文件。
- 公开示例必须使用 `<your-api-key>` 这类占位符。
- 发布前请运行 [docs/OPEN_SOURCE_CHECKLIST.zh-CN.md](docs/OPEN_SOURCE_CHECKLIST.zh-CN.md) 中的敏感数据扫描。
- 如果用 `--scope project` 写入项目级配置，请确认项目 `.mossen/` 目录仍然被忽略。

## 开发检查

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
git diff --check
```

针对具体子系统的 harness 脚本位于 `scripts/harness_*.py`。提交 PR 前，请运行和改动范围最相关的窄范围 harness。

## 仓库结构

```text
crates/mossen-cli       CLI 入口、TUI 启动、stream 渲染
crates/mossen-agent     Agent 运行时、模型桥接、上下文、hooks
crates/mossen-tools     内置工具和子 Agent 任务工具
crates/mossen-commands  斜杠命令实现
crates/mossen-tui       终端 UI 和渲染模型
crates/mossen-mcp       MCP 集成
crates/mossen-utils     共享配置、鉴权、文件系统和运行时工具
scripts/                smoke tests、harness、发布检查
docs/                   公开用户和维护者文档
examples/               不含密钥的配置示例
```

## 范围

Mossen 是一个本地 CLI 项目。Hosted service、team sync、remote attach 和账号托管工作流不属于当前公开版本范围，除非它们已经接入真实的公开实现。

## License

MIT. See [LICENSE](LICENSE).
