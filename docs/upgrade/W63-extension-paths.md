# W63 Extension Paths Visibility

## 1. Scope

W63 adds `/plugin paths`, a read-only CLI command that tells users where Mossen expects local extension assets.

It helps users and future docs install standard local skills, commands, agents, and plugin packages without guessing directories.

## 2. Displayed Paths

`/plugin paths` shows:

- user skills / commands / agents
- project skills / commands / agents
- policy-managed skills / commands / agents
- plugin root
- plugin cache
- marketplace cache
- plugin seed dirs

## 3. Non-Goals

This command does not:

- create directories
- install plugins
- install skills
- install MCP servers
- write settings
- fetch or clone remote repositories
- touch Workbench or stream-json

## 4. Validation

Focused smoke:

```bash
python3 scripts/wave_w63_extension_paths_smoke.py
```

Full gate:

```bash
bash scripts/run_all_smoke.sh
```
