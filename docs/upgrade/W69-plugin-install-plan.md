# W69 Plugin Install Plan

## Scope

W69 adds an optional safe plugin install path:

```text
/plugin install --dry-run <plugin@marketplace> [--scope user|project|local]
/plugin install --confirm <token>
```

The existing `/plugin install <plugin>` and `/plugin install <plugin@market>`
paths are preserved for compatibility.

## Behavior

- Dry-run requires explicit `plugin@marketplace`.
- Dry-run resolves the plugin through the existing marketplace lookup.
- Dry-run previews dependency closure and policy failures.
- Dry-run does not write settings or plugin files.
- Confirm uses a 10 minute one-shot token.
- Confirm reuses the existing `installResolvedPlugin()` implementation.

## Safety

- No duplicate plugin installer.
- No `--force` / `--yes` bypass.
- No Workbench, stream-json, query loop, ToolUseContext, or
  `commands/insights.ts` changes.

## Validation

`scripts/wave_w69_plugin_install_plan_smoke.py` locks parser routing, dry-run
behavior, confirm behavior, helper reuse, command help, and red-line boundaries.
