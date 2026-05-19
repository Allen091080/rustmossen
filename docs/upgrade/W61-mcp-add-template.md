# W61 MCP Add-Template

## 1. Scope

W61 adds a safe slash-command path for installing one of the built-in MCP templates introduced in W60.

The command is:

```text
/mcp add-template <template> [--name <server-name>] [--scope local|user|project] [--root <absolute-path>] [--db <absolute-path>]
/mcp add-template --confirm <token>
```

## 2. Safety Model

- Dry-run is the default.
- Dry-run writes nothing.
- Dry-run mints an 8-hex confirm token with a 10-minute TTL.
- Confirm consumes the token before any side effects.
- Confirm writes through the existing `services/mcp/config.ts:addMcpConfig()` path.
- Confirm does not auto-connect, reconnect, or toggle MCP servers.
- Template path parameters must be absolute paths.

## 3. Template Parameters

| Template | Required parameter |
| --- | --- |
| `filesystem-readonly` | `--root <absolute-path>` |
| `git-readonly` | `--root <absolute-path>` |
| `local-docs` | `--root <absolute-path>` |
| `playwright-local` | none |
| `sqlite-readonly` | `--db <absolute-path>` |

## 4. Non-Goals

W61 does not implement:

- GitHub install.
- Remote marketplace install.
- Credential capture.
- Auto-connect.
- MCP server health checks.
- Workbench UI.
- stream-json protocol changes.

## 5. Validation

Focused smoke:

```bash
python3 scripts/wave_w61_mcp_add_template_smoke.py
```

Full gate:

```bash
bash scripts/run_all_smoke.sh
```

The smoke locks:

- `MCP_TEMPLATE_PLAN_TOKEN_TTL_MS = 10 * 60 * 1000`
- dry-run + confirm helpers exist
- confirm reuses `addMcpConfig`
- path arguments require absolute paths
- `/mcp add-template` route exists
- no auto-connect calls in `McpAddTemplate.tsx`
- no stream-json/query/Workbench/insights drift
