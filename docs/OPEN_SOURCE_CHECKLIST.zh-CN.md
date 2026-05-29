# 开源发布检查清单

[English](OPEN_SOURCE_CHECKLIST.md) | [简体中文](OPEN_SOURCE_CHECKLIST.zh-CN.md)

发布仓库前请使用这份清单。

## 必须检查

- 确认 `Cargo.toml` 中公开版本为 `1.1.0`。
- 确认 `LICENSE` 存在，并且和 `Cargo.toml` 声明的 license 一致。
- 移除内部计划、审计文档、对话导出、本地渲染/session 数据，以及生成的 harness report 文件。
- 只保留公开文档、源码、示例和可复现测试资产。
- 确认仓库中没有模型供应商凭据。
- 确认 `.mossen/`、`.env`、secret 文件、本地报告和生成的 harness 产物都被忽略。

## 敏感数据扫描

运行：

```bash
rg -n --hidden \
  --glob '!target/**' \
  --glob '!**/.git/**' \
  --glob '!Cargo.lock' \
  'sk-[A-Za-z0-9_-]{12,}|api[_-]?key|auth[_-]?token|MOSSEN_CODE_CUSTOM_(API_KEY|AUTH_TOKEN)|ANTHROPIC_API_KEY|OPENAI_API_KEY' .
```

逐条检查所有命中。`sk-test-...` 这类测试占位符可以接受，真实 key 不能接受。

同时检查 ignored 文件：

```bash
git status --ignored --short
```

## 构建和测试

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
git diff --check
```

针对改动子系统运行对应 harness。发布准备至少包含：

```bash
python3 scripts/harness_R11_1_release_readiness_contract_smoke.py
python3 scripts/harness_M10_4_async_agent_taskoutput_smoke.py
python3 scripts/harness_M17_1_tui_rendering_interaction_smoke.py
```

真实 provider 长稳压测适合在二进制发布前执行，但不要提交 key 或 provider 私有输出。

## GitHub 准备

- 添加清晰的仓库描述。
- 添加 topics，例如 `rust`、`cli`、`terminal`、`coding-agent`、`mcp`、`llm`。
- 仓库公开后启用 branch protection。
- 添加格式化、check 和测试 CI。
- 如果托管平台支持，使用私有 security advisory 处理安全报告。

## 发布说明

V1.1.0 应该清楚说明：

- Mossen 是 Rust 原生的终端编码代理。
- 它实现 Claude Code 风格的本地编码工作流。
- 它与 Anthropic 没有关联。
- 用户需要配置自己的模型供应商。
- API key 保留在本地，不应该提交到仓库。
