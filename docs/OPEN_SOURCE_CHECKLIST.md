# Open Source Release Checklist

[English](OPEN_SOURCE_CHECKLIST.md) | [简体中文](OPEN_SOURCE_CHECKLIST.zh-CN.md)

Use this checklist before publishing the repository.

## Required

- Confirm the public version is set to `1.1.0` in `Cargo.toml`.
- Confirm `LICENSE` exists and matches the license declared in `Cargo.toml`.
- Remove internal planning, audit, conversation export, local render/session data, and generated harness report files.
- Keep only public docs, source code, examples, and reproducible test assets.
- Ensure model provider credentials are not in the repository.
- Ensure `.mossen/`, `.env`, secret files, local reports, and generated harness artifacts are ignored.

## Secret Scan

Run:

```bash
rg -n --hidden \
  --glob '!target/**' \
  --glob '!**/.git/**' \
  --glob '!Cargo.lock' \
  'sk-[A-Za-z0-9_-]{12,}|api[_-]?key|auth[_-]?token|MOSSEN_CODE_CUSTOM_(API_KEY|AUTH_TOKEN)|ANTHROPIC_API_KEY|OPENAI_API_KEY' .
```

Review every match. Test placeholders such as `sk-test-...` are acceptable; real keys are not.

Also check ignored files:

```bash
git status --ignored --short
```

## Build And Test

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
git diff --check
```

Run targeted harness scripts for changed subsystems. For release readiness, include at least:

```bash
python3 scripts/harness_R11_1_release_readiness_contract_smoke.py
python3 scripts/harness_M10_4_async_agent_taskoutput_smoke.py
python3 scripts/harness_M17_1_tui_rendering_interaction_smoke.py
```

Real-provider long soaks are useful before binary releases, but do not commit keys or provider-specific private output.

## GitHub Preparation

- Add a clear repository description.
- Add topics such as `rust`, `cli`, `terminal`, `coding-agent`, `mcp`, `llm`.
- Enable branch protection once the repository is public.
- Add CI for formatting, check, and tests.
- Use GitHub private security advisories if available.

## Release Notes

V1.1.0 should say clearly:

- Mossen is a Rust-native terminal coding agent.
- It implements the Claude Code-style local coding workflow.
- It is not affiliated with Anthropic.
- Users must configure their own model provider.
- API keys stay local and should never be committed.
