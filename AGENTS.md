# AGENTS.md

This repository is a Rust workspace. Keep changes scoped and verify them with the narrowest relevant command first.

## Project Rules

- Prefer existing crate boundaries and helper APIs over new abstractions.
- Keep user credentials out of the repository. Do not add real API keys, tokens, local `.mossen/` data, or `.env` files.
- Use placeholders in docs and tests, for example `<your-api-key>` or `sk-test-...`.
- For manual edits, preserve the current Rust style and run `cargo fmt --all -- --check` before publishing.
- When changing user-facing CLI behavior, update README or docs if the public usage changed.

## Useful Commands

```bash
cargo check --workspace
cargo test --workspace
cargo fmt --all -- --check
git diff --check
```

Development launcher:

```bash
scripts/start-mossen.sh
```

Release build:

```bash
cargo build --release -p mossen-cli --bin mossen
```
