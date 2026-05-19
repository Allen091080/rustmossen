# W66 MCP Template i18n

## Scope

W66 fixes the CLI display layer for built-in MCP templates:

```text
/mcp templates
/mcp add-template <template>
```

The template definitions remain canonical English. Chinese copy is mapped at
render time, so switching CLI language does not persist the wrong language into
template metadata.

## Behavior

- `/mcp templates` now localizes template titles, descriptions, and notes.
- `/mcp add-template` dry-run now localizes template title and notes.
- `/mcp templates` no longer says add-template belongs to a future wave; it
  points users at the existing `/mcp add-template <template>` flow.

## Safety

- No MCP config write-path changes.
- No new templates.
- No auto-connect behavior.
- No Workbench, stream-json, query loop, ToolUseContext, or
  `commands/insights.ts` changes.

## Validation

`scripts/wave_w66_mcp_template_i18n_smoke.py` locks canonical English source
metadata, render-time Chinese mappings, obsolete copy removal, command guidance,
and red-line boundaries.
