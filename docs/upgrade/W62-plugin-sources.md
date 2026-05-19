# W62 Plugin Sources Visibility

## 1. Scope

W62 adds `/plugin sources`, a read-only CLI visibility command for the existing plugin marketplace system.

It reuses:

- `utils/plugins/marketplaceManager.ts`
- `utils/plugins/officialMarketplace.ts`
- `utils/plugins/marketplaceHelpers.ts`
- `utils/plugins/pluginDirectories.ts`

It does not create a new plugin installer.

## 2. Behavior

`/plugin sources` displays:

- plugin root path
- marketplace cache path
- configured seed directories
- official marketplace name/source
- known and declared marketplace entries
- install/cache locations where available
- suggested existing commands such as `/plugin install <plugin>@<marketplace>`

## 3. Non-Goals

This command does not:

- install plugins
- update marketplaces
- remove marketplaces
- clone or fetch from GitHub
- write settings
- mutate installed plugin registry
- touch Workbench
- touch stream-json or query loop

## 4. Validation

Focused smoke:

```bash
python3 scripts/wave_w62_plugin_sources_smoke.py
```

Full gate:

```bash
bash scripts/run_all_smoke.sh
```
