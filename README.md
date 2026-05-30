# Mossen

[English](README.md) | [简体中文](README.zh-CN.md)

[![CI](https://github.com/Allen091080/rustmossen/actions/workflows/ci.yml/badge.svg)](https://github.com/Allen091080/rustmossen/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust 1.80+](https://img.shields.io/badge/rust-1.80%2B-orange.svg)](rust-toolchain.toml)

Mossen is a local-first, Rust-native terminal coding agent for developers who want a Claude Code-style workflow with their own model providers.

It implements conversational coding, file editing, shell execution with permissions, slash commands, context management, MCP integration, sub-agents, and terminal rendering in one Rust workspace.

Mossen is not affiliated with Anthropic or Claude Code. The goal is to provide an open Rust implementation of the same class of developer experience, with user-controlled model providers and credentials.

## Why Mossen

- Rust implementation: the agent runtime, TUI, command system, tool execution, provider bridge, and harnesses live in one Rust workspace.
- Provider flexibility: use OpenAI-compatible Chat Completions, OpenAI Responses-compatible endpoints, or Anthropic-compatible endpoints.
- Local-first configuration: model keys are stored in the user's local Mossen config, not in the repository.
- Terminal-native workflow: interactive TUI, streaming output, permissions, slash commands, history, compaction, and task feedback are first-class.
- Engineering-agent surface: built-in tools for reading, editing, searching, shell execution, planning, MCP, and sub-agent task orchestration.
- Testable release path: harness scripts and smoke tests cover core runtime behavior instead of relying only on manual demos.

## Status

Current public release: **V1.1.0**. Current development target: **V1.2 Reliability & Production UX**.

V1.1 is the external-user-ready release. It focuses on a five-minute source install path, clearer model configuration guidance, provider compatibility checks, sub-agent lifecycle feedback, and focused TUI scroll/copy/input coverage.

See [docs/RELEASE_NOTES_V1.1.0.md](docs/RELEASE_NOTES_V1.1.0.md) for the V1.1 release notes and [docs/LAUNCH.md](docs/LAUNCH.md) for copy-ready launch material.

## Requirements

- Rust 1.80 or newer
- macOS or Linux
- Git
- Recommended: `rg` for fast repository search

## 5-Minute Quick Start

Clone and install the `mossen` binary from source:

```bash
git clone https://github.com/Allen091080/rustmossen.git
cd rustmossen
cargo install --path crates/mossen-cli --bin mossen --locked
```

Add your first model profile. Use your own endpoint, model name, and API key:

```bash
mossen --add-model-profile my-model \
  --provider openai-compatible \
  --baseURL https://api.example.com/v1 \
  --model your-model-name \
  --apiKey "$YOUR_API_KEY"
```

Activate and test it:

```bash
mossen --set-model-profile my-model
mossen --test-model-profile my-model --timeout 30000
```

Start Mossen in your project:

```bash
mossen --cwd /path/to/project
```

Inside the TUI, run `/doctor` if the model is not configured or a provider call fails. `/doctor` reports missing profiles, invalid active profiles, partial custom-backend environment variables, and the next command to run without printing raw API keys or base URLs.

## Build

```bash
cargo build --release -p mossen-cli --bin mossen
```

Run the release binary:

```bash
./target/release/mossen
```

For development, use the repository launcher:

```bash
scripts/start-mossen.sh
```

The launcher rebuilds automatically when Rust sources changed. To skip the build when a debug binary already exists:

```bash
MOSSEN_START_BUILD=never scripts/start-mossen.sh
```

Install locally from the checkout:

```bash
cargo install --path crates/mossen-cli --bin mossen --locked
```

## Quick Use

Interactive mode:

```bash
mossen
```

One-shot mode:

```bash
mossen --oneshot "Explain the current repository structure"
```

Stream JSON output:

```bash
mossen --oneshot "List the test commands for this project" --emit stream-json
```

Use a specific working directory:

```bash
mossen --cwd /path/to/project
```

Inside the TUI, use `/help` to inspect available slash commands. For long-running
work, `/goal <objective>` sets a persistent thread goal; `/goal` shows current
progress, `/goal pause` pauses it, `/goal resume` resumes it, `/goal edit
<objective>` updates it, and `/goal clear` removes it.

## Configure LLM Providers

Mossen stores model profiles in:

```text
~/.mossen/settings.json
```

Do not commit this file. It can contain API keys.

Create a profile:

```bash
mossen --add-model-profile my-model \
  --provider openai-compatible \
  --baseURL https://api.example.com/v1 \
  --model your-model-name \
  --apiKey "$YOUR_API_KEY"
```

Activate it:

```bash
mossen --set-model-profile my-model
```

Check configured profiles:

```bash
mossen --list-model-profiles
```

Test a profile:

```bash
mossen --test-model-profile my-model --timeout 30000
```

Supported provider values:

- `openai-compatible`
- `openai-responses`
- `anthropic`

See [docs/CONFIGURATION.md](docs/CONFIGURATION.md) for the full configuration guide and an example settings file. A Chinese version is available at [docs/CONFIGURATION.zh-CN.md](docs/CONFIGURATION.zh-CN.md).

## Security

- Never commit `~/.mossen/settings.json`, project `.mossen/`, `.env`, or files containing real API keys.
- Public examples must use placeholders such as `<your-api-key>`.
- Before publishing, run the sensitive-data scan in [docs/OPEN_SOURCE_CHECKLIST.md](docs/OPEN_SOURCE_CHECKLIST.md).
- If you use project-scoped config with `--scope project`, make sure the project `.mossen/` directory remains ignored.

## Development Checks

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
git diff --check
```

Targeted harness scripts live under `scripts/harness_*.py`. Run the narrow harness for the subsystem you changed before sending a PR.

## Repository Layout

```text
crates/mossen-cli       CLI entrypoint, TUI launch, stream rendering
crates/mossen-agent     agent runtime, provider bridge, context, hooks
crates/mossen-tools     built-in tools and sub-agent task tools
crates/mossen-commands  slash command implementations
crates/mossen-tui       terminal UI and rendering model
crates/mossen-mcp       MCP integration
crates/mossen-utils     shared config, auth, filesystem and runtime helpers
scripts/                smoke tests, harnesses, release checks
docs/                   public user and maintainer documentation
examples/               non-secret configuration examples
```

## Scope

Mossen is a local CLI project. Hosted service features, team sync, remote attach, and account-managed workflows are not part of the current public release unless they are wired to a real public implementation.

## License

MIT. See [LICENSE](LICENSE).
