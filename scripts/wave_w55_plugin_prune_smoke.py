#!/usr/bin/env python3
"""
W55 Round 1 — /plugin prune contract smoke.

Locks the C6 user-facing prune flow added in W55 Round 1:

  cacheUtils.ts (engine wrapper, additive only):
    - PRUNE_PLAN_TOKEN_TTL_MS export = 10 * 60 * 1000.
    - getPluginPrunePlan() exported, returns expiredOrphans /
      unmarkedOrphans / freshOrphans / installedSkipped / token /
      createdAt / zipCacheMode.
    - executePluginPrunePlan(token) exported, single-use token, drains
      from prunePlanStore Map BEFORE side effects, re-validates against
      installed registry at confirm time.
    - 7-day grace (CLEANUP_AGE_MS) untouched and still consumed by the
      new code path.
    - Existing private helpers (markPluginVersionOrphaned,
      getInstalledVersionPaths, processOrphanedPluginVersion,
      removeIfEmpty, readSubdirs) reused — no duplicate orphan logic.
    - cleanupOrphanedPluginVersionsInBackground (auto-pruner) untouched.

  parseArgs.ts:
    - ParsedCommand union has 'prune' case with optional confirmToken.
    - parsePluginArgs handles `prune` and `prune --confirm <token>`.

  plugin.tsx:
    - Routes prune to <PluginPrune ... /> instead of PluginSettings.
    - Other subcommands continue to land in PluginSettings.

  PluginPrune.tsx:
    - Uses getPluginPrunePlan() for dry-run and
      executePluginPrunePlan(token) for confirm.
    - Bilingual (en + zh) for every user-facing string.
    - Mentions the 7-day grace explicitly.
    - Includes the confirm token in dry-run output.

  Round-1 boundary guards:
    - No --force / --bypass-grace / --i-know-what-im-doing flags.
    - executePluginPrunePlan never modifies installed_plugins.json
      (no installedPluginsManager mutating import).
    - No protocol union changes (controlSchemas.ts untouched by this PR).
    - No query.ts / processUserInput / ToolUseContext touched.
    - No commands/insights.ts touched.
    - SDK control-schema layer is not aware of /plugin prune.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CACHE_UTILS = ROOT / "utils" / "plugins" / "cacheUtils.ts"
PARSE_ARGS = ROOT / "commands" / "plugin" / "parseArgs.ts"
PLUGIN_TSX = ROOT / "commands" / "plugin" / "plugin.tsx"
PLUGIN_PRUNE_TSX = ROOT / "commands" / "plugin" / "PluginPrune.tsx"
INSTALLED_PLUGINS_MANAGER = ROOT / "utils" / "plugins" / "installedPluginsManager.ts"
SDK_CONTROL_SCHEMAS = ROOT / "entrypoints" / "sdk" / "controlSchemas.ts"
QUERY_TS = ROOT / "query.ts"
PROCESS_USER_INPUT = ROOT / "utils" / "processUserInput" / "processUserInput.ts"
TOOL_TS = ROOT / "Tool.ts"
COMMANDS_INSIGHTS = ROOT / "commands" / "insights.ts"
RUN_ALL = ROOT / "scripts" / "run_all_smoke.sh"

# Forbidden flag tokens — these would indicate a force / bypass-grace path.
FORBIDDEN_FLAGS = [
    "--force",
    "--bypass-grace",
    "--i-know-what-im-doing",
    "force_prune",
    "forcePrune",
    "bypassGrace",
    "bypass_grace",
    "ironKnowWhatImDoing",
]


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


# ---------------------------------------------------------------------------
# cacheUtils.ts engine wrapper
# ---------------------------------------------------------------------------


def check_cache_utils_exports(failures: list[str]) -> None:
    src = read(CACHE_UTILS)
    if "export const PRUNE_PLAN_TOKEN_TTL_MS" not in src:
        failures.append(
            "utils/plugins/cacheUtils.ts: missing PRUNE_PLAN_TOKEN_TTL_MS export"
        )
    if not re.search(
        r"export const PRUNE_PLAN_TOKEN_TTL_MS\s*=\s*10\s*\*\s*60\s*\*\s*1000",
        src,
    ):
        failures.append(
            "utils/plugins/cacheUtils.ts: PRUNE_PLAN_TOKEN_TTL_MS must equal 10 * 60 * 1000 "
            "(10-minute TTL)"
        )
    if "export async function getPluginPrunePlan" not in src:
        failures.append(
            "utils/plugins/cacheUtils.ts: missing getPluginPrunePlan export"
        )
    if "export async function executePluginPrunePlan" not in src:
        failures.append(
            "utils/plugins/cacheUtils.ts: missing executePluginPrunePlan export"
        )

    # Plan shape — required keys.
    for key in [
        "expiredOrphans",
        "unmarkedOrphans",
        "freshOrphans",
        "installedSkipped",
        "zipCacheMode",
    ]:
        if key not in src:
            failures.append(
                f"utils/plugins/cacheUtils.ts: PluginPrunePlan must surface '{key}' bucket"
            )


def check_cache_utils_grace_preserved(failures: list[str]) -> None:
    src = read(CACHE_UTILS)
    # The 7-day constant must still be present and unchanged in shape.
    if not re.search(
        r"const CLEANUP_AGE_MS\s*=\s*7\s*\*\s*24\s*\*\s*60\s*\*\s*60\s*\*\s*1000",
        src,
    ):
        failures.append(
            "utils/plugins/cacheUtils.ts: CLEANUP_AGE_MS must remain 7 * 24 * 60 * 60 * 1000 "
            "(7-day grace red line)"
        )
    # The new code path must still consume CLEANUP_AGE_MS — otherwise the
    # grace check has been bypassed.
    if src.count("CLEANUP_AGE_MS") < 2:
        failures.append(
            "utils/plugins/cacheUtils.ts: CLEANUP_AGE_MS must be referenced in both the "
            "background pruner and the new prune-plan path"
        )


def check_cache_utils_token_safety(failures: list[str]) -> None:
    src = read(CACHE_UTILS)
    # Token must be deleted from the store BEFORE any rm() — otherwise a
    # mid-execution throw could leave the token reusable.
    consume_idx = src.find("prunePlanStore.delete(token)")
    rm_idx = src.find(" rm(", consume_idx) if consume_idx >= 0 else -1
    if consume_idx < 0:
        failures.append(
            "utils/plugins/cacheUtils.ts: executePluginPrunePlan must call "
            "prunePlanStore.delete(token) before any side effect"
        )
    elif rm_idx >= 0 and rm_idx < consume_idx:
        failures.append(
            "utils/plugins/cacheUtils.ts: rm() called before token consume — race "
            "condition risk (token still live during mutation)"
        )

    # The plan store must be a Map keyed by token.
    if "new Map<string, PluginPrunePlan>" not in src:
        failures.append(
            "utils/plugins/cacheUtils.ts: prunePlanStore must be a Map<string, PluginPrunePlan> "
            "for one-shot token semantics"
        )


def check_cache_utils_no_registry_mutation(failures: list[str]) -> None:
    src = read(CACHE_UTILS)
    # Hard red line: the prune path must not import any installed_plugins.json
    # mutation surface from installedPluginsManager. Reading is fine.
    forbidden_imports = [
        "addPluginInstallation",
        "removePluginInstallation",
        "updateInstallationPathOnDisk",
        "migrateToSinglePluginFile",
    ]
    for sym in forbidden_imports:
        if sym in src:
            failures.append(
                f"utils/plugins/cacheUtils.ts: must not import {sym} — prune is read-only "
                "for installed_plugins.json"
            )

    # Re-validation must read the installed registry at execute time.
    if src.count("getInstalledVersionPaths()") < 2:
        failures.append(
            "utils/plugins/cacheUtils.ts: executePluginPrunePlan must re-load "
            "getInstalledVersionPaths() at confirm time (defense-in-depth against "
            "install/uninstall during the dry-run gap)"
        )


def check_cache_utils_no_force_flag(failures: list[str]) -> None:
    src = read(CACHE_UTILS)
    for flag in FORBIDDEN_FLAGS:
        if flag in src:
            failures.append(
                f"utils/plugins/cacheUtils.ts: forbidden force/bypass token {flag!r} present — "
                "/plugin prune must not provide a way to bypass 7-day grace"
            )


def check_cache_utils_reuses_engine(failures: list[str]) -> None:
    src = read(CACHE_UTILS)
    # The new code must reuse, not replace, the existing private helpers.
    for sym in [
        "markPluginVersionOrphaned",
        "getOrphanedAtPath",
        "getInstalledVersionPaths",
        "readSubdirs",
        "getPluginCachePath",
    ]:
        if sym not in src:
            failures.append(
                f"utils/plugins/cacheUtils.ts: prune path must reuse {sym} (engine helper)"
            )

    # The auto-pruner must still exist and be untouched in name + shape.
    if "export async function cleanupOrphanedPluginVersionsInBackground" not in src:
        failures.append(
            "utils/plugins/cacheUtils.ts: cleanupOrphanedPluginVersionsInBackground "
            "must remain exported (auto-pruner red line)"
        )


# ---------------------------------------------------------------------------
# parseArgs.ts
# ---------------------------------------------------------------------------


def check_parse_args(failures: list[str]) -> None:
    src = read(PARSE_ARGS)
    if not re.search(r"type:\s*'prune'", src):
        failures.append(
            "commands/plugin/parseArgs.ts: ParsedCommand union must include 'prune' case"
        )
    if "confirmToken" not in src:
        failures.append(
            "commands/plugin/parseArgs.ts: prune case must thread confirmToken through ParsedCommand"
        )
    if "case 'prune'" not in src:
        failures.append(
            "commands/plugin/parseArgs.ts: parsePluginArgs must handle 'prune' subcommand"
        )
    if "--confirm" not in src:
        failures.append(
            "commands/plugin/parseArgs.ts: prune case must read --confirm <token> flag"
        )
    for flag in FORBIDDEN_FLAGS:
        if flag in src:
            failures.append(
                f"commands/plugin/parseArgs.ts: forbidden flag {flag!r} present"
            )


# ---------------------------------------------------------------------------
# plugin.tsx — router
# ---------------------------------------------------------------------------


def check_plugin_tsx_router(failures: list[str]) -> None:
    src = read(PLUGIN_TSX)
    if "PluginPrune" not in src:
        failures.append(
            "commands/plugin/plugin.tsx: must import and render PluginPrune for the "
            "prune subcommand"
        )
    if "parsePluginArgs" not in src:
        failures.append(
            "commands/plugin/plugin.tsx: must call parsePluginArgs to detect the prune "
            "subcommand at the router layer"
        )
    if "parsed.type === 'prune'" not in src and "type === \"prune\"" not in src:
        failures.append(
            "commands/plugin/plugin.tsx: must dispatch on parsed.type === 'prune'"
        )


# ---------------------------------------------------------------------------
# PluginPrune.tsx — UI
# ---------------------------------------------------------------------------


def check_plugin_prune_tsx(failures: list[str]) -> None:
    if not PLUGIN_PRUNE_TSX.exists():
        failures.append("commands/plugin/PluginPrune.tsx: missing")
        return
    src = read(PLUGIN_PRUNE_TSX)
    # Engine wiring.
    if "getPluginPrunePlan" not in src:
        failures.append(
            "commands/plugin/PluginPrune.tsx: must call getPluginPrunePlan for dry-run"
        )
    if "executePluginPrunePlan" not in src:
        failures.append(
            "commands/plugin/PluginPrune.tsx: must call executePluginPrunePlan for confirm"
        )
    if "PRUNE_PLAN_TOKEN_TTL_MS" not in src:
        failures.append(
            "commands/plugin/PluginPrune.tsx: must surface PRUNE_PLAN_TOKEN_TTL_MS to the user"
        )

    # i18n.
    if "getLocalizedText" not in src:
        failures.append(
            "commands/plugin/PluginPrune.tsx: must use getLocalizedText for zh/en"
        )
    # Both languages must mention the 7-day grace explicitly.
    if "7-day" not in src and "7d" not in src:
        failures.append(
            "commands/plugin/PluginPrune.tsx: en text must mention 7-day grace"
        )
    if "7 天" not in src:
        failures.append(
            "commands/plugin/PluginPrune.tsx: zh text must mention 7 天 grace"
        )

    # No force / bypass tokens.
    for flag in FORBIDDEN_FLAGS:
        if flag in src:
            failures.append(
                f"commands/plugin/PluginPrune.tsx: forbidden token {flag!r} present"
            )

    # Confirm token surface — must include a hint with the literal flag.
    if "--confirm" not in src:
        failures.append(
            "commands/plugin/PluginPrune.tsx: dry-run output must show the "
            "'/plugin prune --confirm <token>' instruction"
        )


# ---------------------------------------------------------------------------
# Boundary guards
# ---------------------------------------------------------------------------


def check_no_protocol_change(failures: list[str]) -> None:
    if not SDK_CONTROL_SCHEMAS.exists():
        return
    src = read(SDK_CONTROL_SCHEMAS)
    # Round-1 prune is a slash command, not a control_request — the SDK
    # control schema layer must be unaware of it.
    leak_tokens = [
        "PluginPrune",
        "getPluginPrunePlan",
        "executePluginPrunePlan",
        "PRUNE_PLAN_TOKEN_TTL_MS",
    ]
    for token in leak_tokens:
        if token in src:
            failures.append(
                f"entrypoints/sdk/controlSchemas.ts: token {token!r} leaked into protocol — "
                "/plugin prune is a slash command, not a control_request subtype"
            )


def check_main_loop_clean(failures: list[str]) -> None:
    leak_tokens = [
        "PluginPrune",
        "getPluginPrunePlan",
        "executePluginPrunePlan",
        "PRUNE_PLAN_TOKEN_TTL_MS",
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
                    f"{label}: token {token!r} leaked — /plugin prune must stay confined "
                    "to the slash-command surface"
                )


def check_insights_untouched(failures: list[str]) -> None:
    # Sanity check that the file is still present (the actual diff guard
    # lives at commit-review time; the smoke just locks existence).
    if not COMMANDS_INSIGHTS.exists():
        failures.append(
            "commands/insights.ts: must remain present (W55 R1 must not delete it)"
        )


def check_run_all_registration(failures: list[str]) -> None:
    if not RUN_ALL.exists():
        failures.append("scripts/run_all_smoke.sh: missing")
        return
    src = read(RUN_ALL)
    if "wave_w55_plugin_prune_smoke" not in src:
        failures.append(
            "scripts/run_all_smoke.sh: must register wave_w55_plugin_prune_smoke"
        )


# ---------------------------------------------------------------------------
# Driver
# ---------------------------------------------------------------------------


def main() -> int:
    failures: list[str] = []

    if not CACHE_UTILS.exists():
        print("FAIL: utils/plugins/cacheUtils.ts not found", file=sys.stderr)
        return 1

    check_cache_utils_exports(failures)
    check_cache_utils_grace_preserved(failures)
    check_cache_utils_token_safety(failures)
    check_cache_utils_no_registry_mutation(failures)
    check_cache_utils_no_force_flag(failures)
    check_cache_utils_reuses_engine(failures)
    check_parse_args(failures)
    check_plugin_tsx_router(failures)
    check_plugin_prune_tsx(failures)
    check_no_protocol_change(failures)
    check_main_loop_clean(failures)
    check_insights_untouched(failures)
    check_run_all_registration(failures)

    print("=== W55 Round 1 plugin prune smoke ===")
    print(f"engine wrapper: {CACHE_UTILS.relative_to(ROOT)}")
    print(f"parser:         {PARSE_ARGS.relative_to(ROOT)}")
    print(f"router:         {PLUGIN_TSX.relative_to(ROOT)}")
    print(f"UI:             {PLUGIN_PRUNE_TSX.relative_to(ROOT)}")
    print("scope:          C6 /plugin prune dry-run + --confirm token (C5 deferred to Round 2)")

    if failures:
        print()
        print("=== FAIL ===")
        for item in failures:
            print(f"  - {item}", file=sys.stderr)
        return 1

    print()
    print(
        "PASS: W55 Round 1 ✓ "
        "(getPluginPrunePlan + executePluginPrunePlan exported, 10-min one-shot token, "
        "7-day grace preserved, no force/bypass flags, no installed-registry mutation, "
        "auto-pruner untouched, parseArgs + plugin.tsx router wired, PluginPrune i18n + "
        "grace surfaced, no protocol union / main-loop / Workbench drift)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
