# W60 CLI Skill / MCP Extension Foundation

## 1. Scope

This wave starts the CLI-only skill/MCP/plugin plan. Workbench is explicitly out of scope.

The implementation reuses existing Mossen systems:

- `skills/bundled/*` for always-available bundled skills.
- `plugins/builtinPlugins.ts` for user-toggleable built-in plugin packs.
- `commands/mcp/*` and `services/mcp/*` for MCP visibility and runtime integration.
- Existing plugin install/cache/status/prune systems for future extension work.

No second skill system, plugin system, or MCP runtime is introduced.

## 2. Delivered Capabilities

### 2.1 Mossen core bundled skills

`skills/bundled/mossenCoreSkills.ts` registers Mossen-specific always-available skills:

- `skill-creator`
- `mcp-builder`
- `doc-coauthoring`
- `mossen-upgrade-planning`
- `mossen-protocol-development`
- `mossen-plugin-development`
- `mossen-permission-safety`
- `mossen-memory-development`
- `mossen-release-maintenance`

These skills are prompt-only. They do not create new execution paths.

### 2.2 Built-in plugin development pack

`plugins/bundled/mossenPluginDev.ts` registers `mossen-plugin-dev@builtin` through the existing built-in plugin registry.

It is default-enabled and user-toggleable in `/plugin`. It provides prompt-only development skills:

- `plugin-structure`
- `skill-development`
- `command-development`
- `hook-development`
- `mcp-integration`
- `plugin-settings`
- `agent-development`

This proves the built-in plugin path is usable for first-party extension packs without inventing a separate distribution path.

### 2.3 MCP template inventory

`services/mcp/builtinTemplates.ts` defines read-only inventory templates:

- `filesystem-readonly`
- `git-readonly`
- `local-docs`
- `playwright-local`
- `sqlite-readonly`

All templates have `defaultEnabled: false`. `/mcp templates` lists them as inventory only. It does not write settings, install servers, start servers, or connect MCP clients.

## 3. Future Extension Model

The intended user extension path is:

1. Local extension directories first.
2. Standard plugin manifest and skill/MCP conventions.
3. Dry-run install preview.
4. Confirm-token for writes.
5. GitHub or other remote install later, with pinning and trust warnings.

Users should eventually be able to install additional skills, MCP servers, and plugins from GitHub-style repositories, but that requires a separate mutation wave. W60 intentionally avoids remote install.

## 4. Red Lines

This wave must not touch:

- stream-json schema union
- query loop
- `processUserInput`
- `ToolUseContext`
- Workbench
- `commands/insights.ts`
- auth/login/logout paths
- remote install or GitHub clone logic

MCP templates must not include credentials, API keys, or provider-specific remote defaults.

## 5. Validation

Focused smoke:

```bash
python3 scripts/wave_w60_skill_mcp_preinstall_smoke.py
```

Full gate:

```bash
bash scripts/run_all_smoke.sh
```

The smoke locks:

- core bundled skill registration
- built-in plugin-dev registration
- MCP templates are disabled inventory
- `/mcp templates` route exists
- forbidden protocol/query/Workbench/insights files are not touched

## 6. Next Waves

Recommended follow-up order:

1. `/mcp add-template` dry-run + confirm, still disabled by default.
2. Local extension pack discovery for user/project/plugin folders.
3. Extension status command that explains installed skills/MCP/plugins.
4. GitHub install with dry-run, pinning, signature/trust notes, and rollback.
5. Optional curated official pack import after the local install path is stable.
