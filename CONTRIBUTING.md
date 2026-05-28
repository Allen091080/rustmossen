# Contributing

Thanks for helping improve Mossen.

## Development

```bash
cargo check --workspace
cargo test --workspace
cargo fmt --all -- --check
git diff --check
```

Use targeted harness scripts from `scripts/` for the subsystem you changed.

## Pull Requests

- Keep changes focused.
- Include tests or harness evidence for behavior changes.
- Update public docs when commands, configuration, or provider behavior changes.
- Do not include real API keys, local `.mossen/` contents, transcripts, or private benchmark output.

## Provider Tests

Provider tests should use placeholders or local mock providers unless the test is explicitly documented as a maintainer-only real-provider soak. Never commit real credentials.
