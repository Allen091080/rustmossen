# W51 — Memory Runtime Diagnostics

## Overview

Extends two existing read-only protocols to surface comprehensive memory
diagnostics. No new control_request subtypes, no writes, no secrets.

## `/memory` slash_command — Extended Response

The existing `/memory` inventory now includes a `runtime` section:

```
memory: {
  files: [ ... ],          // existing W45 inventory (path, type, contentLength)
  runtime: {
    autoMemoryEnabled,      // isAutoMemoryEnabled()
    extractModeActive,      // isExtractModeActive()
    teamMemory: {
      buildEnabled,         // feature('TEAMMEM')
      enabled,              // isTeamMemoryEnabled()
      rolloutEnabled,       // getFeatureValue('tengu_team_memory')
      path,                 // getTeamMemPath() when enabled, else null
    },
    sessionMemory: {
      enabled,              // getFeatureValue('tengu_session_memory')
      compactEnabled,       // getFeatureValue('tengu_sm_compact')
      initialized,          // isSessionMemoryInitialized()
    },
    compact: {
      autoCompactEnabled,   // isAutoCompactEnabled()
    },
  },
}
```

### Memory layers explained

| Layer | Gate | Default | What it controls |
|-------|------|---------|------------------|
| auto memory | `isAutoMemoryEnabled()` | **on** | Private memory directory, extractMemories writes |
| extract mode | `isExtractModeActive()` | varies | Forked agent memory extraction at end of query |
| team memory | `tengu_team_memory` → `mossen.memory.teamMemoryEnabled` | **off** | Team-shared memory directory (memory/team/) |
| session memory | `tengu_session_memory` → `mossen.compact.sessionMemoryEnabled` | **off** | Session-scoped memory summaries |
| auto compact | `isAutoCompactEnabled()` | varies | Automatic context compaction when token threshold exceeded |

### Security

- No file content returned (only metadata: path, type, contentLength).
- No secrets, API keys, tokens, or credential values.
- No settings values echoed (only enabled/disabled booleans).
- `teamMemory.path` returned only when team memory is enabled.

## `runtime_doctor_summary` — Enhanced Checks

### `memory` check (enhanced)

Previously only reported file count. Now reports:
- Auto memory status (on/off)
- Extract mode status (on/off)
- Team memory status (on/off, when build flag present)
- Session memory initialization (when enabled)
- File count

Severity rules:
- `auto:off` → `warn` + `warning` severity
- `auto:on,team:off` → `ok` + `info` severity (team off is normal)
- `auto:on` → `ok` + `info` severity

### `compact` check (new)

Reports:
- Auto compact status (on/off)
- Manual `/compact` availability via stdin user message
- Slash bridge status (blocked — requires ToolUseContext)

Always `ok` + `info` severity (compact status is informational, not a health issue).

### Prohibited

- No network calls
- No auth probes
- No spawn/execSync
- No file content reading
