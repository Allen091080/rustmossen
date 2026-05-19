# W64 `/skills` Bundled Visibility Fix

## Summary

W60 added Mossen core bundled skills and the builtin `mossen-plugin-dev` skill pack. Both register commands with `source: 'bundled'` and `loadedFrom: 'bundled'`.

The `/skills` menu still filtered only file, plugin, and MCP skill sources, so the preinstalled skills were registered but hidden from the menu. In a fresh CLI this looked like:

```text
Skills
No skills found
Create skills in .mossen/skills/ or ~/.mossen/skills/
```

## Fix

- Treat `bundled` as a first-class `/skills` source.
- Include bundled skills in the command filter.
- Render a bundled source group before plugin/MCP groups.
- Add a bundled source filter chip.
- Avoid calling filesystem skill path helpers for bundled skills.
- Localize bundled skill descriptions at render time so slash suggestions follow the current language after `/lang` switches.

## Boundaries

- No skill loader or invocation behavior changed.
- No bundled skill definitions changed.
- No plugin registry behavior changed.
- No stream-json, query loop, Workbench, or `commands/insights.ts` changes.

## Validation

`scripts/wave_w64_skills_bundled_visibility_smoke.py` locks the regression: `/skills` must accept `loadedFrom: 'bundled'`, render/filter the bundled group, and keep W60 bundled registrations intact.

It also locks the follow-up i18n fix: W60 core bundled skills and builtin plugin-dev skill definitions keep canonical English descriptions, while `getLocalizedCommandDescription()` provides Chinese descriptions dynamically. This avoids freezing the language at startup or registration time.
