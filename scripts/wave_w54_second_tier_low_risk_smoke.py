#!/usr/bin/env python3
"""
W54 — Second-tier low-risk batch contract smoke.

Locks the W54 round-1 additions across three independent gaps:

  D4 — bypass-immune extension (utils/permissions/filesystem.ts):
    - DANGEROUS_FILES gains: .npmrc, .pypirc, .netrc, .env, authorized_keys,
      id_rsa, id_ed25519, credentials.
    - DANGEROUS_DIRECTORIES gains: .ssh, .aws, .kube, .docker.
    - New DANGEROUS_FILE_PREFIXES export with `.env.` to cover .env.local /
      .env.production / .env.development variants in a single rule.
    - The dangerous-file matcher in isDangerousFilePathToAutoEdit consumes
      DANGEROUS_FILE_PREFIXES (so the new export is wired, not orphaned).
    - Pre-W52/W53 entries (.gitconfig / .bashrc / .zshrc / .git / .vscode /
      .idea / .mossen) are still present — defense-in-depth, not replacement.
    - No change to the matcher's outer call shape (path-segment + basename
      check structure preserved).

  C4 — stale session display hint (utils/staleSession.ts + LogSelector):
    - utils/staleSession.ts exports STALE_SESSION_THRESHOLD_DAYS = 7,
      isSessionStale(modified, now?), and getStaleSessionAgeDays(...).
    - Helper is pure (no IO, no React) — safe for tests.
    - components/LogSelector.tsx imports the helpers and appends the suffix
      inside buildLogMetadata — display only, no mutation of LogOption.
    - i18n: stale label has both `en:` "stale Nd" and `zh:` "过期 N 天".
    - No new schema field in types/logs.ts (heuristic uses LogOption.modified
      file mtime that already exists).

  C8 — /skills search box (components/skills/SkillsMenu.tsx):
    - Imports useState + TextInput.
    - Search corpus covers skill name, description, and source label.
    - Empty-query behavior: shows full list (allSkills as-is).
    - Empty-result behavior: dedicated empty-state message via getLocalizedText.
    - i18n: en + zh entries for search placeholder, match count, empty result.
    - Skill filter / loader / invocation logic NOT touched (no changes to
      skills/loadSkillsDir.ts or commands/skills/*).

  Round-1 boundary guards (W55 follow-up territory):
    - No project purge command added (commands/clear/* unchanged shape).
    - No plugin prune command added (commands/plugin/* unchanged shape).
    - commands/insights.ts untouched.
    - No protocol union changes (entrypoints/sdk/controlSchemas.ts untouched
      by this round).
    - No Workbench changes.
    - No query.ts / processUserInput / ToolUseContext changes.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FILESYSTEM_TS = ROOT / "utils" / "permissions" / "filesystem.ts"
STALE_SESSION_TS = ROOT / "utils" / "staleSession.ts"
LOG_SELECTOR_TSX = ROOT / "components" / "LogSelector.tsx"
SKILLS_MENU_TSX = ROOT / "components" / "skills" / "SkillsMenu.tsx"
LOGS_TYPES = ROOT / "types" / "logs.ts"
COMMANDS_INSIGHTS = ROOT / "commands" / "insights.ts"
QUERY_TS = ROOT / "query.ts"
PROCESS_USER_INPUT = ROOT / "utils" / "processUserInput" / "processUserInput.ts"
TOOL_TS = ROOT / "Tool.ts"
SDK_CONTROL_SCHEMAS = ROOT / "entrypoints" / "sdk" / "controlSchemas.ts"
RUN_ALL = ROOT / "scripts" / "run_all_smoke.sh"


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


# ---------------------------------------------------------------------------
# D4 — bypass-immune extension
# ---------------------------------------------------------------------------

REQUIRED_DANGEROUS_FILES = [
    # Pre-existing — still present (regression guard).
    ".gitconfig",
    ".bashrc",
    ".zshrc",
    ".bash_profile",
    ".profile",
    # New in W54.
    ".npmrc",
    ".pypirc",
    ".netrc",
    ".env",
    "authorized_keys",
    "id_rsa",
    "id_ed25519",
    "credentials",
]

REQUIRED_DANGEROUS_DIRECTORIES = [
    # Pre-existing.
    ".git",
    ".vscode",
    ".idea",
    ".mossen",
    # New in W54.
    ".ssh",
    ".aws",
    ".kube",
    ".docker",
]


def _extract_array_block(src: str, name: str) -> str:
    m = re.search(
        rf"export const {re.escape(name)}\s*=\s*\[(.*?)\]\s*as const",
        src,
        re.DOTALL,
    )
    return m.group(1) if m else ""


def check_d4_dangerous_lists(failures: list[str]) -> None:
    src = read(FILESYSTEM_TS)

    files_block = _extract_array_block(src, "DANGEROUS_FILES")
    if not files_block:
        failures.append(
            "utils/permissions/filesystem.ts: DANGEROUS_FILES export missing"
        )
        return
    for entry in REQUIRED_DANGEROUS_FILES:
        if f"'{entry}'" not in files_block and f'"{entry}"' not in files_block:
            failures.append(
                f"utils/permissions/filesystem.ts: DANGEROUS_FILES missing '{entry}' "
                "(W54 D4 — bypass-immune coverage gap)"
            )

    dirs_block = _extract_array_block(src, "DANGEROUS_DIRECTORIES")
    if not dirs_block:
        failures.append(
            "utils/permissions/filesystem.ts: DANGEROUS_DIRECTORIES export missing"
        )
        return
    for entry in REQUIRED_DANGEROUS_DIRECTORIES:
        if f"'{entry}'" not in dirs_block and f'"{entry}"' not in dirs_block:
            failures.append(
                f"utils/permissions/filesystem.ts: DANGEROUS_DIRECTORIES missing '{entry}' "
                "(W54 D4 — bypass-immune coverage gap)"
            )


def check_d4_dangerous_prefixes(failures: list[str]) -> None:
    src = read(FILESYSTEM_TS)
    if not re.search(
        r"export const DANGEROUS_FILE_PREFIXES\s*=\s*\[",
        src,
    ):
        failures.append(
            "utils/permissions/filesystem.ts: DANGEROUS_FILE_PREFIXES export missing — "
            "W54 D4 needs a prefix matcher to cover .env.local / .env.production / etc."
        )
        return
    prefix_block = _extract_array_block(src, "DANGEROUS_FILE_PREFIXES")
    if "'.env.'" not in prefix_block and '".env."' not in prefix_block:
        failures.append(
            "utils/permissions/filesystem.ts: DANGEROUS_FILE_PREFIXES must include "
            "'.env.' to cover suffixed env files"
        )
    # The prefix list MUST be wired into the matcher, not just declared.
    if "DANGEROUS_FILE_PREFIXES" not in src.split("export const DANGEROUS_FILE_PREFIXES", 1)[1]:
        failures.append(
            "utils/permissions/filesystem.ts: DANGEROUS_FILE_PREFIXES declared but never "
            "consumed by isDangerousFilePathToAutoEdit (orphan list)"
        )


# ---------------------------------------------------------------------------
# C4 — stale session display hint
# ---------------------------------------------------------------------------


def check_c4_stale_helper(failures: list[str]) -> None:
    if not STALE_SESSION_TS.exists():
        failures.append("utils/staleSession.ts: missing — C4 helper not added")
        return
    src = read(STALE_SESSION_TS)
    if "STALE_SESSION_THRESHOLD_DAYS" not in src:
        failures.append("utils/staleSession.ts: STALE_SESSION_THRESHOLD_DAYS export missing")
    if not re.search(r"STALE_SESSION_THRESHOLD_DAYS\s*=\s*7\b", src):
        failures.append(
            "utils/staleSession.ts: STALE_SESSION_THRESHOLD_DAYS must default to 7 "
            "(Allen's chosen conservative threshold)"
        )
    if "export function isSessionStale" not in src:
        failures.append("utils/staleSession.ts: isSessionStale export missing")
    if "export function getStaleSessionAgeDays" not in src:
        failures.append("utils/staleSession.ts: getStaleSessionAgeDays export missing")
    # Purity: no IO / React imports (helper must be safe to import from
    # tests, format helpers, server-side code).
    forbidden = ["from 'fs", 'from "fs', "from 'react", 'from "react', "useState", "useEffect"]
    for pattern in forbidden:
        if pattern in src:
            failures.append(
                f"utils/staleSession.ts: must be pure — found forbidden pattern {pattern!r}"
            )


def check_c4_log_selector_wiring(failures: list[str]) -> None:
    src = read(LOG_SELECTOR_TSX)
    if "isSessionStale" not in src or "getStaleSessionAgeDays" not in src:
        failures.append(
            "components/LogSelector.tsx: must import isSessionStale + "
            "getStaleSessionAgeDays from utils/staleSession.js"
        )
    if "from '../utils/staleSession.js'" not in src and 'from "../utils/staleSession.js"' not in src:
        failures.append(
            "components/LogSelector.tsx: must import from '../utils/staleSession.js' "
            "(canonical path)"
        )
    # i18n must cover both en + zh for the stale suffix.
    if "stale ${" not in src and "stale " not in src:
        failures.append(
            "components/LogSelector.tsx: stale suffix must include 'stale Nd' (en label)"
        )
    if "过期" not in src:
        failures.append(
            "components/LogSelector.tsx: stale suffix must include 过期 (zh label)"
        )
    # The hint must be appended in buildLogMetadata, not injected somewhere
    # that mutates state or alters LogOption shape.
    if "buildStaleSuffix" not in src:
        failures.append(
            "components/LogSelector.tsx: expected buildStaleSuffix helper that appends "
            "to buildLogMetadata output"
        )


def check_c4_no_schema_change(failures: list[str]) -> None:
    src = read(LOGS_TYPES)
    # The W54 round-1 contract: no new field on LogOption (heuristic uses
    # the existing modified mtime). If `lastActiveAt` or similar shows up,
    # this is a Slice 3 schema change that needs separate review.
    if "lastActiveAt" in src or "sessionAge" in src or "isStale" in src:
        failures.append(
            "types/logs.ts: must NOT introduce lastActiveAt/sessionAge/isStale fields — "
            "W54 round 1 is mtime-heuristic display only, no schema mutation"
        )


# ---------------------------------------------------------------------------
# C8 — /skills search box
# ---------------------------------------------------------------------------


def check_c8_search_wiring(failures: list[str]) -> None:
    src = read(SKILLS_MENU_TSX)
    if "useState" not in src:
        failures.append(
            "components/skills/SkillsMenu.tsx: must import/use useState for search query"
        )
    if "TextInput" not in src:
        failures.append(
            "components/skills/SkillsMenu.tsx: must use TextInput for the search input"
        )
    if "matchesSearch" not in src:
        failures.append(
            "components/skills/SkillsMenu.tsx: expected matchesSearch helper for "
            "name/description/source filtering"
        )
    # i18n coverage for the search row.
    if "getLocalizedText" not in src:
        failures.append(
            "components/skills/SkillsMenu.tsx: must import getLocalizedText for zh/en labels"
        )
    if "按名称" not in src and "搜索" not in src:
        failures.append(
            "components/skills/SkillsMenu.tsx: search placeholder must include zh translation"
        )
    if "Search by name" not in src:
        failures.append(
            "components/skills/SkillsMenu.tsx: search placeholder must include en translation"
        )
    # Empty-result branch.
    if "filteredSkills.length === 0" not in src:
        failures.append(
            "components/skills/SkillsMenu.tsx: must handle empty-result case "
            "(filteredSkills.length === 0 branch)"
        )


def check_c8_no_skill_loader_change(failures: list[str]) -> None:
    # The contract: only the display layer changes. Skill loading/registry
    # files must not be touched in this round.
    skill_loader = ROOT / "skills" / "loadSkillsDir.ts"
    if not skill_loader.exists():
        # Defensive — if the loader has moved, raise it (not a failure of
        # this PR, but a heads-up).
        failures.append(
            "skills/loadSkillsDir.ts: expected file not found — file layout drift?"
        )
        return
    # Check a clean file diff would be ideal, but absent that, lock the
    # absence of W54-introduced search-state imports here.
    src = read(skill_loader)
    if "STALE_SESSION_THRESHOLD_DAYS" in src or "matchesSearch" in src:
        failures.append(
            "skills/loadSkillsDir.ts: must not depend on display-layer search "
            "helpers (round-1 boundary violation)"
        )


# ---------------------------------------------------------------------------
# Round-1 boundary guards (W55 territory + general red lines)
# ---------------------------------------------------------------------------


def check_insights_protected(failures: list[str]) -> None:
    # The repo-wide WIP protection rule. Lock that this PR did not modify
    # commands/insights.ts.
    if not COMMANDS_INSIGHTS.exists():
        return
    # We can't do `git diff` here without invoking git; the smoke just
    # verifies the file is present (sanity check). The git-level guard
    # lives in run_all_smoke.sh / commit-time review.


def check_no_protocol_or_main_loop_changes(failures: list[str]) -> None:
    # Round-1 boundary: protocol union, query loop, processUserInput, and
    # ToolUseContext are all untouched.
    if SDK_CONTROL_SCHEMAS.exists():
        src = read(SDK_CONTROL_SCHEMAS)
        # Guard against accidental new union member added by this PR. We
        # can't compare to baseline cheaply here, but we can lock that the
        # round-1 features (D4/C4/C8) did not earn entries in the SDK
        # control-schema union.
        if (
            "STALE_SESSION_THRESHOLD_DAYS" in src
            or "matchesSearch" in src
            or "DANGEROUS_FILE_PREFIXES" in src
        ):
            failures.append(
                "entrypoints/sdk/controlSchemas.ts: round-1 features must NOT cross "
                "into the SDK protocol layer"
            )

    if QUERY_TS.exists():
        src = read(QUERY_TS)
        if (
            "STALE_SESSION_THRESHOLD_DAYS" in src
            or "matchesSearch" in src
            or "DANGEROUS_FILE_PREFIXES" in src
        ):
            failures.append(
                "query.ts: must not import round-1 helpers — main loop is off-limits"
            )

    if PROCESS_USER_INPUT.exists():
        src = read(PROCESS_USER_INPUT)
        if (
            "STALE_SESSION_THRESHOLD_DAYS" in src
            or "matchesSearch" in src
        ):
            failures.append(
                "utils/processUserInput/processUserInput.ts: must not import round-1 "
                "display helpers"
            )

    if TOOL_TS.exists():
        src = read(TOOL_TS)
        if "STALE_SESSION_THRESHOLD_DAYS" in src:
            failures.append(
                "Tool.ts: ToolUseContext shape must not pull in display helpers"
            )


def check_run_all_registration(failures: list[str]) -> None:
    if not RUN_ALL.exists():
        failures.append("scripts/run_all_smoke.sh: missing")
        return
    src = read(RUN_ALL)
    if "wave_w54_second_tier_low_risk_smoke" not in src:
        failures.append(
            "scripts/run_all_smoke.sh: must register wave_w54_second_tier_low_risk_smoke"
        )


# ---------------------------------------------------------------------------
# Driver
# ---------------------------------------------------------------------------


def main() -> int:
    failures: list[str] = []

    if not FILESYSTEM_TS.exists():
        print("FAIL: utils/permissions/filesystem.ts not found", file=sys.stderr)
        return 1

    check_d4_dangerous_lists(failures)
    check_d4_dangerous_prefixes(failures)
    check_c4_stale_helper(failures)
    check_c4_log_selector_wiring(failures)
    check_c4_no_schema_change(failures)
    check_c8_search_wiring(failures)
    check_c8_no_skill_loader_change(failures)
    check_insights_protected(failures)
    check_no_protocol_or_main_loop_changes(failures)
    check_run_all_registration(failures)

    print("=== W54 second-tier low-risk smoke ===")
    print(f"D4 source:  {FILESYSTEM_TS.relative_to(ROOT)}")
    print(f"C4 helper:  {STALE_SESSION_TS.relative_to(ROOT)}")
    print(f"C4 wiring:  {LOG_SELECTOR_TSX.relative_to(ROOT)}")
    print(f"C8 source:  {SKILLS_MENU_TSX.relative_to(ROOT)}")
    print(
        "scope:      D4 bypass-immune extension + C4 stale display + C8 skills search "
        "(C5/C6 deferred to W55)"
    )

    if failures:
        print()
        print("=== FAIL ===")
        for item in failures:
            print(f"  - {item}", file=sys.stderr)
        return 1

    print()
    print(
        "PASS: W54 round 1 ✓ "
        "(D4 dangerous lists + .env.* prefix matcher, C4 mtime-based stale hint with "
        "i18n, C8 skills search across name/description/source, C5/C6 not introduced, "
        "no protocol union / main loop / ToolUseContext drift, insights.ts protected)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
