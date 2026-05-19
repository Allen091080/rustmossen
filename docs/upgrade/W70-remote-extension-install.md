# W70 Remote Extension Install

## Scope

W70 keeps the extension model CLI-only and on-demand. It does not add automatic marketplace sync, preinstall policy, Workbench protocol, or background update behavior.

Implemented surfaces:

- `/plugin install --dry-run <github-url> [--scope user|project|local]`
- `/plugin install --confirm <token>`
- `/mcp install --dry-run <url> [--name server] [--scope local|user|project]`
- `/mcp install --confirm <token>`

## Plugin GitHub Install

The plugin path extends the W69 safe install planner. `plugin@marketplace` continues to work exactly as before. If the dry-run target is a GitHub URL or `owner/repo`, Mossen:

1. Reads `.mossen-plugin/plugin.json` or `plugin.json` through the GitHub contents API.
2. Validates the manifest with `PluginMarketplaceEntrySchema`.
3. Builds a temporary direct install entry under the `github-direct` sentinel marketplace.
4. Resolves dependencies using the existing dependency resolver.
5. Stores a 10-minute one-shot token.

Confirm reuses the existing `installResolvedPlugin()` path. Root repos are installed as HTTPS git sources; tree/blob subdirectories are installed as `git-subdir` sources. No settings or plugin files are written during dry-run.

## MCP Remote Config Install

The MCP path accepts JSON from an HTTP(S) URL or a GitHub blob URL. It supports:

- Standard MCP JSON: `{ "mcpServers": { "name": { ... } } }`
- Single-server config JSON, when `--name <server>` is provided

If a remote MCP JSON contains multiple servers, dry-run requires `--name` so the user chooses exactly one server. Confirm reuses `addMcpConfig()` and only writes config; it does not auto-connect the server.

## Safety

- 10-minute one-shot confirm tokens.
- No `--force` or `--yes` path.
- No stream-json schema changes.
- No query loop changes.
- No Workbench changes.
- No `commands/insights.ts` changes.

## Validation

W70 adds `scripts/wave_w70_remote_extension_install_smoke.py` and registers it in `scripts/run_all_smoke.sh`.
