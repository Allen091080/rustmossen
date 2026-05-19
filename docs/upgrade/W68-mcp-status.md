# W68 MCP Status

## Scope

W68 adds a read-only MCP status command:

```text
/mcp status
/mcp stat
```

It also localizes the existing `/mcp enable` and `/mcp disable` completion
messages.

## Behavior

`/mcp status` displays:

- total MCP server counts by state
- total tools, prompts/skills, and resources
- per-server state, scope, transport, and exposed capability counts
- failed server error text where available
- pending reconnect attempt counters where available

## Safety

- Read-only AppState view only.
- Reuses existing MCP filter helpers.
- Does not reconnect, enable, disable, authenticate, write config, or mutate
  files.
- No Workbench, stream-json, query loop, ToolUseContext, or
  `commands/insights.ts` changes.

## Validation

`scripts/wave_w68_mcp_status_smoke.py` locks routing, read-only status behavior,
helper reuse, i18n, and red-line boundaries.
