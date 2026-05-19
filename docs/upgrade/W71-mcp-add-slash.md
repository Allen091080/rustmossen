# W71 slash `/mcp add`

## Scope

W71 adds a CLI-only slash command path for adding MCP servers from inside Mossen:

```text
/mcp add playwright --scope local -- npx -y @playwright/mcp@latest
/mcp add --confirm <token>
```

The existing external CLI command remains unchanged:

```text
mossen mcp add playwright --scope local -- npx -y @playwright/mcp@latest
```

## Behavior

`/mcp add` is safe by default. Without `--confirm`, it only builds a dry-run plan and prints a one-time confirm token.

- Default scope is `local`.
- Supported scopes are `local`, `user`, and `project`.
- Default transport is `stdio`.
- Supported transports are `stdio`, `http`, and `sse`.
- Stdio command arguments are passed after `--`.
- Stdio env values can be provided with `--env KEY=value` or `-e KEY=value`.
- HTTP/SSE headers can be provided with `--header "Name: value"` or `-H "Name: value"`.

Confirm uses the stored token:

```text
/mcp add --confirm <token>
```

Tokens are 8 hex chars, valid for 10 minutes, stored in memory, and consumed before any config write.

## Safety

The dry-run path has no filesystem side effects. It only schema-validates the generated MCP config with `McpServerConfigSchema`.

The confirm path reuses the existing canonical writer:

```ts
addMcpConfig(plan.serverName, plan.config, plan.scope)
```

This means duplicate-name checks, enterprise policy checks, config scope behavior, and server-name validation stay in the existing MCP config layer.

The command does not auto-connect the MCP server. Users can reconnect/restart MCP after config is written.

## Non-goals

- No query loop changes.
- No stream-json protocol changes.
- No Workbench changes.
- No automatic marketplace sync or automatic MCP discovery.
- No GitHub/remote JSON fetch; that remains `/mcp install`.
- No OAuth/XAA support in slash add. Those remain external CLI surfaces for now.

## Example: Playwright MCP

Inside Mossen:

```text
/mcp add playwright --scope local -- npx -y @playwright/mcp@latest
```

Then confirm with the printed token:

```text
/mcp add --confirm <token>
```

Check config visibility:

```text
/mcp status
```

After reconnect/restart, ask Mossen to use Playwright.
