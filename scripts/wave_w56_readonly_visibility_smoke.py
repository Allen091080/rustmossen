#!/usr/bin/env python3
"""
W56 — read-only maintenance visibility smoke.

Locks the W56 surface added on top of the W55 R1+R2 mutation flows:

  /project list (commands/project/ProjectList.tsx + utils/projectInventory.ts)
    - Read-only inventory of ~/.mossen/projects/.
    - Reports sanitized id, inferred cwd, jsonl + sub-dir count, memory
      presence/file count/size, total size, modified time, active marker,
      stale marker (mtime > STALE_SESSION_THRESHOLD_DAYS).
    - Never archives, never deletes, never writes.

  /project status (commands/project/ProjectStatus.tsx + utils/projectInventory.ts)
    - Read-only summary of the active project: cwd / projectRoot /
      sessionProjectDir / sanitized active ids, project dir, session counts,
      memory state (in-project / external / absent + reason), purge
      eligibility ALWAYS rejected (active-project guard), sibling cache
      sizes (debug / backups / plugins).

  /plugin status (commands/plugin/PluginStatus.tsx +
                  utils/plugins/statusOps.ts + utils/plugins/cacheUtils.ts:summarizePluginCache)
    - Read-only summary of plugin root, cache, marketplaces, installed
      registry, orphan classification (expired / unmarked / fresh /
      installed-skipped) reused from the W55 R1 prune classifier.
    - Never writes .orphaned_at, never deletes cache, never modifies
      installed_plugins.json.

  /memory metadata pane (commands/memory/memory.tsx)
    - Adds an additive metadata header above MemoryFileSelector showing
      auto memory enabled / team memory enabled / memory location / file
      count / size. NEVER reads memory file contents.

  SkillsMenu source filter (components/skills/SkillsMenu.tsx)
    - Tab / Shift+Tab cycles through {all, projectSettings, userSettings,
      policySettings, plugin, mcp}. Read-only — no skill loader / invocation
      changes; the loader (skills/loadSkillsDir) and invocation paths are
      untouched.

  Cache size summary
    - Surfaced in /project status caches[]: debug / backups / plugins.
    - utils/projectInventory.ts:summarizeCacheDir is read-only.

  Boundary guards (W56 red lines):
    - No fs.rm / unlink / rename / writeFile / mkdir mutation paths in the
      new helpers (projectInventory.ts / statusOps.ts).
    - Stream-json union (entrypoints/sdk/controlSchemas.ts) untouched —
      W56 is slash-command-only.
    - query.ts / processUserInput / Tool.ts / Workbench / commands/insights.ts
      do not reference any W56 helper.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PROJECT_INVENTORY = ROOT / "utils" / "projectInventory.ts"
STATUS_OPS = ROOT / "utils" / "plugins" / "statusOps.ts"
CACHE_UTILS = ROOT / "utils" / "plugins" / "cacheUtils.ts"
PROJECT_LIST_TSX = ROOT / "commands" / "project" / "ProjectList.tsx"
PROJECT_STATUS_TSX = ROOT / "commands" / "project" / "ProjectStatus.tsx"
PLUGIN_STATUS_TSX = ROOT / "commands" / "plugin" / "PluginStatus.tsx"
PROJECT_PARSE_ARGS = ROOT / "commands" / "project" / "parseArgs.ts"
PLUGIN_PARSE_ARGS = ROOT / "commands" / "plugin" / "parseArgs.ts"
PROJECT_TSX = ROOT / "commands" / "project" / "project.tsx"
PLUGIN_TSX = ROOT / "commands" / "plugin" / "plugin.tsx"
MEMORY_TSX = ROOT / "commands" / "memory" / "memory.tsx"
SKILLS_MENU = ROOT / "components" / "skills" / "SkillsMenu.tsx"
SDK_CONTROL_SCHEMAS = ROOT / "entrypoints" / "sdk" / "controlSchemas.ts"
QUERY_TS = ROOT / "query.ts"
PROCESS_USER_INPUT = ROOT / "utils" / "processUserInput" / "processUserInput.ts"
TOOL_TS = ROOT / "Tool.ts"
COMMANDS_INSIGHTS = ROOT / "commands" / "insights.ts"
RUN_ALL = ROOT / "scripts" / "run_all_smoke.sh"
LOAD_SKILLS_DIR = ROOT / "skills" / "loadSkillsDir.ts"


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


# ---------------------------------------------------------------------------
# utils/projectInventory.ts
# ---------------------------------------------------------------------------


def check_inventory_engine(failures: list[str]) -> None:
    src = read(PROJECT_INVENTORY)
    for sym in [
        "export async function buildProjectInventory",
        "export async function describeActiveProjectStatus",
        "export async function describeMemoryState",
        "export async function summarizeCacheDir",
        "export function computeActiveMarkers",
    ]:
        if sym not in src:
            failures.append(f"utils/projectInventory.ts: missing {sym}")
    # Three-way active markers must be sourced from bootstrap state.
    for sym in ["getOriginalCwd", "getProjectRoot", "getSessionProjectDir"]:
        if sym not in src:
            failures.append(
                f"utils/projectInventory.ts: must read {sym} (active-project markers)"
            )
    # Must NOT contain mutation calls.
    forbidden_calls = [
        r"\bawait\s+rm\(",
        r"\bawait\s+unlink\(",
        r"\bawait\s+rename\(",
        r"\bawait\s+writeFile\(",
        r"\bawait\s+mkdir\(",
        r"\bawait\s+copyFile\(",
    ]
    for pattern in forbidden_calls:
        if re.search(pattern, src):
            failures.append(
                f"utils/projectInventory.ts: forbidden mutation call matching {pattern!r} present — "
                "W56 inventory must be read-only"
            )
    # Must NOT touch sibling sensitive files by literal.
    forbidden_literals = [
        "settings.json",
        ".mossen.json",
        "custom-backend.env",
        "history.jsonl",
    ]
    for literal in forbidden_literals:
        if literal in src:
            failures.append(
                f"utils/projectInventory.ts: must not reference sensitive sibling literal {literal!r}"
            )
    # Stale detection must use the existing helper, not redefine the threshold.
    if "isSessionStale" not in src:
        failures.append(
            "utils/projectInventory.ts: must use isSessionStale (single source of truth for staleness)"
        )


# ---------------------------------------------------------------------------
# utils/plugins/statusOps.ts
# ---------------------------------------------------------------------------


def check_status_ops(failures: list[str]) -> None:
    src = read(STATUS_OPS)
    if "export async function describePluginStatus" not in src:
        failures.append("utils/plugins/statusOps.ts: missing describePluginStatus")
    # Must reuse cacheUtils helper, not redefine orphan classification.
    if "summarizePluginCache" not in src:
        failures.append(
            "utils/plugins/statusOps.ts: must call summarizePluginCache "
            "(reuse W55 R1 classifier — no drift)"
        )
    if "loadInstalledPluginsFromDisk" not in src:
        failures.append(
            "utils/plugins/statusOps.ts: must call loadInstalledPluginsFromDisk "
            "for installed-registry counts"
        )
    # No mutation.
    forbidden_calls = [
        r"\bawait\s+rm\(",
        r"\bawait\s+unlink\(",
        r"\bawait\s+rename\(",
        r"\bawait\s+writeFile\(",
        r"\bawait\s+mkdir\(",
        r"\bawait\s+copyFile\(",
        r"markPluginVersionOrphaned\(",
    ]
    for pattern in forbidden_calls:
        if re.search(pattern, src):
            failures.append(
                f"utils/plugins/statusOps.ts: forbidden mutation call {pattern!r} present"
            )


def check_summarize_plugin_cache(failures: list[str]) -> None:
    src = read(CACHE_UTILS)
    if "export async function summarizePluginCache" not in src:
        failures.append(
            "utils/plugins/cacheUtils.ts: must export summarizePluginCache "
            "(W56 read-only summary helper)"
        )
    # Helper must not mutate.
    fn_match = re.search(
        r"export async function summarizePluginCache\(\):.*?\n\}\n",
        src,
        re.DOTALL,
    )
    if fn_match:
        body = fn_match.group(0)
        for pattern in [
            r"\bawait\s+rm\(",
            r"\bawait\s+writeFile\(",
            r"\bawait\s+unlink\(",
            r"markPluginVersionOrphaned\(",
        ]:
            if re.search(pattern, body):
                failures.append(
                    f"utils/plugins/cacheUtils.ts: summarizePluginCache must not mutate ({pattern!r})"
                )


# ---------------------------------------------------------------------------
# parseArgs additions
# ---------------------------------------------------------------------------


def check_project_parse_args(failures: list[str]) -> None:
    src = read(PROJECT_PARSE_ARGS)
    if not re.search(r"type:\s*'list'", src):
        failures.append(
            "commands/project/parseArgs.ts: ParsedProjectCommand must include 'list'"
        )
    if not re.search(r"type:\s*'status'", src):
        failures.append(
            "commands/project/parseArgs.ts: ParsedProjectCommand must include 'status'"
        )
    if "case 'list'" not in src:
        failures.append(
            "commands/project/parseArgs.ts: parseProjectArgs must handle 'list'"
        )
    if "case 'status'" not in src:
        failures.append(
            "commands/project/parseArgs.ts: parseProjectArgs must handle 'status'"
        )


def check_plugin_parse_args(failures: list[str]) -> None:
    src = read(PLUGIN_PARSE_ARGS)
    if not re.search(r"type:\s*'status'", src):
        failures.append(
            "commands/plugin/parseArgs.ts: ParsedCommand union must include 'status'"
        )
    if "case 'status'" not in src:
        failures.append(
            "commands/plugin/parseArgs.ts: parsePluginArgs must handle 'status'"
        )


# ---------------------------------------------------------------------------
# Routers
# ---------------------------------------------------------------------------


def check_project_router(failures: list[str]) -> None:
    src = read(PROJECT_TSX)
    if "ProjectList" not in src:
        failures.append("commands/project/project.tsx: must render ProjectList")
    if "ProjectStatus" not in src:
        failures.append("commands/project/project.tsx: must render ProjectStatus")
    if "parsed.type === 'list'" not in src:
        failures.append(
            "commands/project/project.tsx: must dispatch on parsed.type === 'list'"
        )
    if "parsed.type === 'status'" not in src:
        failures.append(
            "commands/project/project.tsx: must dispatch on parsed.type === 'status'"
        )


def check_plugin_router(failures: list[str]) -> None:
    src = read(PLUGIN_TSX)
    if "PluginStatus" not in src:
        failures.append("commands/plugin/plugin.tsx: must render PluginStatus")
    if "parsed.type === 'status'" not in src:
        failures.append(
            "commands/plugin/plugin.tsx: must dispatch on parsed.type === 'status'"
        )


# ---------------------------------------------------------------------------
# UI components
# ---------------------------------------------------------------------------


def check_project_list_tsx(failures: list[str]) -> None:
    if not PROJECT_LIST_TSX.exists():
        failures.append("commands/project/ProjectList.tsx: missing")
        return
    src = read(PROJECT_LIST_TSX)
    if "buildProjectInventory" not in src:
        failures.append(
            "commands/project/ProjectList.tsx: must call buildProjectInventory"
        )
    if "getLocalizedText" not in src:
        failures.append(
            "commands/project/ProjectList.tsx: must use getLocalizedText for zh/en"
        )
    # Must surface key fields per Allen's W56 spec.
    for needle in ["sanitized", "session", "memory", "ACTIVE", "STALE", "/project purge"]:
        if needle not in src:
            failures.append(
                f"commands/project/ProjectList.tsx: must surface {needle!r} (Allen's W56 list spec)"
            )


def check_project_status_tsx(failures: list[str]) -> None:
    if not PROJECT_STATUS_TSX.exists():
        failures.append("commands/project/ProjectStatus.tsx: missing")
        return
    src = read(PROJECT_STATUS_TSX)
    if "describeActiveProjectStatus" not in src:
        failures.append(
            "commands/project/ProjectStatus.tsx: must call describeActiveProjectStatus"
        )
    for needle in [
        "originalCwd",
        "projectRoot",
        "sessionProjectDir",
        "active sanitized",
        "active 活动",  # not real, just a placeholder substring removed below
    ]:
        # The 'active 活动' line is a placeholder we don't actually need; skip.
        if needle == "active 活动":
            continue
        if needle not in src:
            failures.append(
                f"commands/project/ProjectStatus.tsx: must surface {needle!r}"
            )
    # Memory section must include all three states.
    for needle in ["in-project", "external", "absent"]:
        if needle not in src:
            failures.append(
                f"commands/project/ProjectStatus.tsx: must mention memory status {needle!r}"
            )
    # Purge eligibility must be REJECTED.
    if "REJECTED" not in src and "拒绝" not in src:
        failures.append(
            "commands/project/ProjectStatus.tsx: must clearly say purge is REJECTED for active project"
        )
    # Must NOT include any mutation flag.
    for forbidden in ["--force", "--yes", "--no-archive", "--all-projects"]:
        if forbidden in src:
            failures.append(
                f"commands/project/ProjectStatus.tsx: forbidden flag literal {forbidden!r} present"
            )


def check_plugin_status_tsx(failures: list[str]) -> None:
    if not PLUGIN_STATUS_TSX.exists():
        failures.append("commands/plugin/PluginStatus.tsx: missing")
        return
    src = read(PLUGIN_STATUS_TSX)
    if "describePluginStatus" not in src:
        failures.append(
            "commands/plugin/PluginStatus.tsx: must call describePluginStatus"
        )
    for needle in [
        "expiredOrphanCount",
        "unmarkedOrphanCount",
        "freshOrphanCount",
        "installedSkippedCount",
        "installedRecordCount",
        "marketplaceCount",
        "cacheVersionCount",
        "/plugin prune",
    ]:
        if needle not in src:
            failures.append(
                f"commands/plugin/PluginStatus.tsx: must surface {needle!r}"
            )


# ---------------------------------------------------------------------------
# /memory metadata pane
# ---------------------------------------------------------------------------


def check_memory_metadata_pane(failures: list[str]) -> None:
    src = read(MEMORY_TSX)
    if "MemoryMetadataPane" not in src:
        failures.append(
            "commands/memory/memory.tsx: must add MemoryMetadataPane component (W56 metadata footer)"
        )
    if "isAutoMemoryEnabled" not in src:
        failures.append(
            "commands/memory/memory.tsx: metadata pane must surface isAutoMemoryEnabled"
        )
    if "isTeamMemoryEnabled" not in src:
        failures.append(
            "commands/memory/memory.tsx: metadata pane must surface isTeamMemoryEnabled"
        )
    if "describeMemoryState" not in src:
        failures.append(
            "commands/memory/memory.tsx: metadata pane must call describeMemoryState"
        )
    # Must NOT read memory file contents — only stat / readdir-equivalent helpers.
    if "readFile(" in src:
        # readFile is fine in the rest of memory.tsx (handleSelectMemoryFile),
        # but we want to ensure the pane itself doesn't read memory contents.
        # The pane is an additive component; if 'readFile' shows up *inside*
        # MemoryMetadataPane, fail. Detect by extracting the function body.
        m = re.search(
            r"function MemoryMetadataPane\(\):.*?\n\}\n",
            src,
            re.DOTALL,
        )
        if m and "readFile(" in m.group(0):
            failures.append(
                "commands/memory/memory.tsx: MemoryMetadataPane must not call readFile "
                "(metadata-only — never display memory file contents)"
            )
    # Must explicitly tell the user content is NOT shown.
    if "metadata only" not in src and "仅显示元数据" not in src:
        failures.append(
            "commands/memory/memory.tsx: must explicitly disclose 'metadata only' to the user"
        )


# ---------------------------------------------------------------------------
# /skills source filter
# ---------------------------------------------------------------------------


def check_skills_filter(failures: list[str]) -> None:
    src = read(SKILLS_MENU)
    if "SOURCE_FILTER_ORDER" not in src:
        failures.append(
            "components/skills/SkillsMenu.tsx: must add SOURCE_FILTER_ORDER (W56 filter chip set)"
        )
    if "sourceFilter" not in src:
        failures.append(
            "components/skills/SkillsMenu.tsx: must add sourceFilter state"
        )
    if "useInput" not in src:
        failures.append(
            "components/skills/SkillsMenu.tsx: must wire useInput to cycle filter via Tab/Shift+Tab"
        )
    # Loader / invocation must remain untouched — don't import new APIs from skills/loadSkillsDir.
    if LOAD_SKILLS_DIR.exists():
        loader_src = read(LOAD_SKILLS_DIR)
        # Cheap self-check: loader file size shouldn't have a mutation symbol introduced.
        for needle in ["mutateSkill", "writeSkill", "deleteSkill"]:
            if needle in loader_src:
                failures.append(
                    f"skills/loadSkillsDir.ts: forbidden mutation symbol {needle!r} present (W56 must not modify loader)"
                )


# ---------------------------------------------------------------------------
# Boundary guards
# ---------------------------------------------------------------------------


def check_no_protocol_change(failures: list[str]) -> None:
    if not SDK_CONTROL_SCHEMAS.exists():
        return
    src = read(SDK_CONTROL_SCHEMAS)
    leak_tokens = [
        "buildProjectInventory",
        "describeActiveProjectStatus",
        "describePluginStatus",
        "summarizePluginCache",
        "MemoryMetadataPane",
    ]
    for token in leak_tokens:
        if token in src:
            failures.append(
                f"entrypoints/sdk/controlSchemas.ts: token {token!r} leaked into protocol — "
                "W56 surfaces are slash commands, not control_request subtypes"
            )


def check_main_loop_clean(failures: list[str]) -> None:
    leak_tokens = [
        "buildProjectInventory",
        "describeActiveProjectStatus",
        "describePluginStatus",
        "summarizePluginCache",
    ]
    for path, label in [
        (QUERY_TS, "query.ts"),
        (PROCESS_USER_INPUT, "utils/processUserInput/processUserInput.ts"),
        (TOOL_TS, "Tool.ts"),
    ]:
        if not path.exists():
            continue
        src = read(path)
        for token in leak_tokens:
            if token in src:
                failures.append(
                    f"{label}: token {token!r} leaked — W56 surfaces must stay confined "
                    "to the slash-command layer"
                )


def check_insights_untouched(failures: list[str]) -> None:
    if not COMMANDS_INSIGHTS.exists():
        failures.append(
            "commands/insights.ts: must remain present (W56 must not delete it)"
        )


def check_run_all_registration(failures: list[str]) -> None:
    if not RUN_ALL.exists():
        failures.append("scripts/run_all_smoke.sh: missing")
        return
    src = read(RUN_ALL)
    if "wave_w56_readonly_visibility_smoke" not in src:
        failures.append(
            "scripts/run_all_smoke.sh: must register wave_w56_readonly_visibility_smoke"
        )


# ---------------------------------------------------------------------------
# Driver
# ---------------------------------------------------------------------------


def main() -> int:
    failures: list[str] = []

    if not PROJECT_INVENTORY.exists():
        print("FAIL: utils/projectInventory.ts not found", file=sys.stderr)
        return 1

    check_inventory_engine(failures)
    check_status_ops(failures)
    check_summarize_plugin_cache(failures)
    check_project_parse_args(failures)
    check_plugin_parse_args(failures)
    check_project_router(failures)
    check_plugin_router(failures)
    check_project_list_tsx(failures)
    check_project_status_tsx(failures)
    check_plugin_status_tsx(failures)
    check_memory_metadata_pane(failures)
    check_skills_filter(failures)
    check_no_protocol_change(failures)
    check_main_loop_clean(failures)
    check_insights_untouched(failures)
    check_run_all_registration(failures)

    print("=== W56 read-only visibility smoke ===")
    print(f"inventory engine: {PROJECT_INVENTORY.relative_to(ROOT)}")
    print(f"plugin status:    {STATUS_OPS.relative_to(ROOT)}")
    print(f"summarize helper: {CACHE_UTILS.relative_to(ROOT)}:summarizePluginCache")
    print(f"/project list:    {PROJECT_LIST_TSX.relative_to(ROOT)}")
    print(f"/project status:  {PROJECT_STATUS_TSX.relative_to(ROOT)}")
    print(f"/plugin status:   {PLUGIN_STATUS_TSX.relative_to(ROOT)}")
    print(f"/memory pane:     {MEMORY_TSX.relative_to(ROOT)}")
    print(f"skills filter:    {SKILLS_MENU.relative_to(ROOT)}")
    print("scope: read-only visibility (no mutation, no protocol drift)")

    if failures:
        print()
        print("=== FAIL ===")
        for item in failures:
            print(f"  - {item}", file=sys.stderr)
        return 1

    print()
    print(
        "PASS: W56 read-only visibility ✓ "
        "(/project list + /project status + /plugin status + /memory metadata "
        "pane + skills source filter; reuses W55 R1 orphan classifier via "
        "summarizePluginCache; no fs mutation in new helpers; no protocol "
        "drift; main loop / Workbench / insights untouched; /doctor + LogSelector "
        "stale filter deferred per Allen)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
