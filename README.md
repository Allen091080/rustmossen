# Mossen

Mossen is a Rust-native terminal coding agent. It implements the core Claude Code-style workflow in a local-first, provider-flexible CLI: conversational coding, file editing, shell execution with permissions, slash commands, context management, MCP integration, sub-agents, and terminal rendering.

Mossen is not affiliated with Anthropic or Claude Code. The goal is to provide an open Rust implementation of the same class of developer experience, with user-controlled model providers and credentials.

## Why Mossen

- Rust implementation: the agent runtime, TUI, command system, tool execution, provider bridge, and harnesses live in one Rust workspace.
- Provider flexibility: use OpenAI-compatible Chat Completions, OpenAI Responses-compatible endpoints, or Anthropic-compatible endpoints.
- Local-first configuration: model keys are stored in the user's local Mossen config, not in the repository.
- Terminal-native workflow: interactive TUI, streaming output, permissions, slash commands, history, compaction, and task feedback are first-class.
- Engineering-agent surface: built-in tools for reading, editing, searching, shell execution, planning, MCP, and sub-agent task orchestration.
- Testable release path: harness scripts and smoke tests cover core runtime behavior instead of relying only on manual demos.

## Status

Current release target: **V1.0.0**.

This version is intended to be the first public source release. Packaging and installer polish can evolve after the source release; the priority for V1.0 is that the core coding-agent workflow is usable and configurable from source.

## Requirements

- Rust 1.80 or newer
- macOS or Linux
- Git
- Recommended: `rg` for fast repository search

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

Inside the TUI, use `/help` to inspect available slash commands.

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

See [docs/CONFIGURATION.md](docs/CONFIGURATION.md) for the full configuration guide and an example settings file.

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

Mossen is a local CLI project. Hosted service features, team sync, remote attach, and account-managed workflows are not required for the V1.0 source release unless they are wired to a real public implementation.

## License

MIT. See [LICENSE](LICENSE).
