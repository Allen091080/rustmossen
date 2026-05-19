# W67 Plugin Marketplace Add Plan

## Scope

W67 adds an optional safe CLI path for adding plugin marketplaces:

```text
/plugin marketplace add --dry-run <source>
/plugin marketplace add --confirm <token>
```

The existing `/plugin marketplace add <source>` behavior is preserved for
compatibility. W67 does not replace the legacy direct-add path.

## Behavior

- Dry-run parses the source with the existing `parseMarketplaceInput()` helper.
- Dry-run does not fetch, clone, write settings, or mutate caches.
- Dry-run returns a 10 minute one-shot token.
- Confirm reuses the existing `addMarketplaceSource()` path.
- Confirm saves the resolved marketplace source via `saveMarketplaceToSettings()`.
- Confirm clears plugin caches after successful add.

## Safety

- No shelling out or custom git clone path.
- No `--force` / `--yes` bypass.
- No plugin install is performed; this only adds a marketplace source.
- No Workbench, stream-json, query loop, ToolUseContext, or
  `commands/insights.ts` changes.

## Validation

`scripts/wave_w67_plugin_marketplace_add_plan_smoke.py` locks the parser,
router, dry-run/confirm engine, one-shot token, existing helper reuse, command
help, and red-line boundaries.
