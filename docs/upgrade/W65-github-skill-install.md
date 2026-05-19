# W65 GitHub Skill Install

## Scope

W65 adds the CLI-only GitHub skill install path:

```text
/skills install <github-url>              # dry-run
/skills install --confirm <token>         # install
```

This wave intentionally does **not** implement GitHub plugin install or MCP install. Those are separate mutation surfaces.

## Behavior

- Accepts public GitHub targets:
  - `https://github.com/owner/repo`
  - `https://github.com/owner/repo/tree/ref/path/to/skill`
  - `https://github.com/owner/repo/blob/ref/path/to/SKILL.md`
  - `owner/repo`
- Uses the GitHub contents API.
- Requires a `SKILL.md` at the target.
- Reuses Mossen's existing frontmatter parser and skill frontmatter validation.
- Dry-run is side-effect free and prints:
  - source repo/ref/path
  - derived skill name
  - install path
  - file list and size
  - warnings
  - confirm token
- Confirm token is 8 hex chars, 10 minute TTL, one-shot.
- Confirm installs into `~/.mossen/skills/<skill-name>/`.
- Existing skills are not overwritten.
- After install, command and skill caches are cleared and the running session is notified.

## Safety

- No shelling out to `git`.
- No `--force` / `--yes` bypass.
- File count and byte size are bounded.
- Only safe relative file paths are written.
- Temporary install directory is removed on failure.
- No Workbench, stream-json, query loop, ToolUseContext, or `commands/insights.ts` changes.

## Validation

`scripts/wave_w65_github_skill_install_smoke.py` locks the command parser, engine boundaries, dry-run/confirm contract, install path, cache refresh, and no forbidden file drift.
