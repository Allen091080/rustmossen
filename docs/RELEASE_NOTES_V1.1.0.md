# Mossen V1.1.0 Release Notes

[English](#english) | [简体中文](#简体中文)

## English

Mossen V1.1.0 is the external-user-ready release of Mossen, a local-first, Rust-native terminal coding agent.

This release keeps the project source-first, but tightens the path for a new user to clone the repository, install the CLI, configure a model provider, and diagnose common setup problems.

### Highlights

- 5-minute quick start in the English and Chinese README files.
- Clear missing-model guidance, including `/doctor` messaging and safe next-step commands.
- `/doctor` coverage for common configuration failures: missing profiles, malformed settings, missing active profile, missing model profile, incomplete custom backend environment variables, and redacted diagnostics.
- Stable mock coverage for `openai-compatible`, `openai-responses`, and `anthropic` provider protocols.
- Active model profile bridge coverage so configured provider protocol, base URL, and model flow into runtime backend settings.
- Sub-agent lifecycle feedback coverage, including asynchronous Agent startup, visible task ids, child execution, `TaskOutput`, and parent final feedback.
- `/help` and command inventory checks to avoid showing disconnected public capabilities.
- TUI-focused coverage for rendering, scroll behavior, transcript copy/export, and input responsiveness while a background agent is active.
- Default CI remains green, and the full test workflow is manually runnable from GitHub Actions.

### Install From Source

```bash
git clone https://github.com/Allen091080/rustmossen.git
cd rustmossen
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

Supported provider values:

- `openai-compatible`
- `openai-responses`
- `anthropic`

Run `/doctor` inside the TUI when model setup fails or when provider calls do not work as expected.

### Verification

V1.1.0 was released after these gates passed:

- Default CI
- Full Tests workflow
- `scripts/v11_external_user_ready_status.py` reporting `8/8`
- Provider mock protocol matrix
- Sub-agent lifecycle smoke tests
- TUI rendering, copy, and input responsiveness smoke tests

### Current Scope

V1.1.0 is still a source-first local CLI release. Installer packaging, long-running real-provider soak testing, hosted/team/remote workflows, and broader distribution polish can continue after this release.

### Safety Notes

- Do not commit `~/.mossen/settings.json`, project `.mossen/`, `.env`, or real provider credentials.
- Public examples use placeholders such as `<your-api-key>`.
- Model provider configuration lives in the user's local Mossen settings, not in the repository.

## 简体中文

Mossen V1.1.0 是 Mossen 的 external-user-ready 发布版本。Mossen 是一个本地优先、Rust 原生的终端编码 Agent。

这个版本仍然是源码优先发布，但重点收紧了新用户路径：克隆仓库、安装 CLI、配置模型供应商、定位常见配置问题。

### 亮点

- 英文和中文 README 都提供 5 分钟快速启动路径。
- 没有配置模型时提供清晰引导，包括 `/doctor` 提示和安全的下一步命令。
- `/doctor` 覆盖常见配置问题：缺失 profiles、settings 格式错误、active profile 不存在、缺少模型 profile、不完整 custom backend 环境变量，以及诊断输出脱敏。
- `openai-compatible`、`openai-responses`、`anthropic` 三类 provider 协议有稳定 mock 覆盖。
- active model profile bridge 有覆盖，确保配置的 provider protocol、base URL 和 model 能进入运行时 backend 设置。
- 子 Agent 生命周期反馈有覆盖，包括异步 Agent 启动、可见 task id、子任务执行、`TaskOutput` 和父 Agent 最终反馈。
- `/help` 和命令 inventory 会检查公开命令，避免展示没有接通的能力。
- TUI 专项覆盖渲染、滚动、transcript 复制/导出，以及后台 Agent 运行时的输入响应。
- 默认 CI 保持绿色，full test workflow 可以在 GitHub Actions 手动运行。

### 从源码安装

```bash
git clone https://github.com/Allen091080/rustmossen.git
cd rustmossen
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

支持的 `provider` 值：

- `openai-compatible`
- `openai-responses`
- `anthropic`

模型配置失败或 provider 调用异常时，可以在 TUI 内运行 `/doctor`。

### 验证

V1.1.0 发布前通过了这些 gate：

- 默认 CI
- Full Tests workflow
- `scripts/v11_external_user_ready_status.py` 报告 `8/8`
- Provider mock protocol matrix
- 子 Agent lifecycle smoke tests
- TUI rendering、copy、input responsiveness smoke tests

### 当前范围

V1.1.0 仍然是源码优先的本地 CLI 发布。安装包、真实 provider 长稳压测、hosted/team/remote 工作流和更完整的分发体验可以在这个版本之后继续推进。

### 安全说明

- 不要提交 `~/.mossen/settings.json`、项目 `.mossen/`、`.env` 或真实 provider 凭据。
- 公开示例使用 `<your-api-key>` 这类占位符。
- 模型供应商配置保存在用户本机 Mossen settings 中，不写进仓库。
