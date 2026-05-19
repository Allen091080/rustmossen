#!/usr/bin/env python3
"""
W62 — /plugin sources read-only visibility smoke.

Locks a visibility slice over the existing plugin marketplace system. This
must reuse marketplaceManager / officialMarketplace / pluginDirectories and
must not create a new GitHub installer or mutate plugin state.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOURCE_STATUS = ROOT / "utils" / "plugins" / "sourceStatus.ts"
PLUGIN_SOURCES = ROOT / "commands" / "plugin" / "PluginSources.tsx"
PARSE_ARGS = ROOT / "commands" / "plugin" / "parseArgs.ts"
PLUGIN_ROUTER = ROOT / "commands" / "plugin" / "plugin.tsx"
RUN_ALL = ROOT / "scripts" / "run_all_smoke.sh"


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def diff_names() -> set[str]:
    result = subprocess.run(
        ["git", "diff", "--name-only", "origin/main..HEAD"],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=True,
    )
    committed = {line.strip() for line in result.stdout.splitlines() if line.strip()}
    result2 = subprocess.run(
        ["git", "diff", "--name-only", "HEAD"],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=True,
    )
    dirty = {line.strip() for line in result2.stdout.splitlines() if line.strip()}
    return committed | dirty


def require(condition: bool, failures: list[str], message: str) -> None:
    if not condition:
        failures.append(message)


def check_source_status(failures: list[str]) -> None:
    src = read(SOURCE_STATUS)
    for token in [
        "loadKnownMarketplacesConfigSafe",
        "getDeclaredMarketplaces",
        "OFFICIAL_MARKETPLACE_NAME",
        "OFFICIAL_MARKETPLACE_SOURCE",
        "getPluginSeedDirs",
        "getPluginsDirectory",
        "getMarketplacesCacheDir",
        "getMarketplaceSourceDisplay",
    ]:
        require(token in src, failures, f"sourceStatus must reuse {token}")
    for forbidden in [
        "saveMarketplaceToSettings",
        "saveKnownMarketplacesConfig",
        "installPlugin",
        "execFile",
        "gitExe",
        "axios",
        "writeFile",
        "rm(",
        "unlink",
    ]:
        require(forbidden not in src, failures, f"sourceStatus must be read-only; found {forbidden}")


def check_command(failures: list[str]) -> None:
    parser = read(PARSE_ARGS)
    router = read(PLUGIN_ROUTER)
    ui = read(PLUGIN_SOURCES)
    require("type: 'sources'" in parser, failures, "parseArgs missing sources command")
    require("case 'sources'" in parser and "case 'source'" in parser, failures, "parseArgs missing sources aliases")
    require("<PluginSources" in router, failures, "plugin router missing PluginSources")
    require("describePluginSources" in ui, failures, "PluginSources must call describePluginSources")
    require("does not install, update, remove, clone, or fetch" in ui, failures, "PluginSources must state no mutation/fetch")
    for forbidden in ["installPlugin", "saveMarketplace", "loadMarketplacesWithGracefulDegradation", "clearAllCaches"]:
        require(forbidden not in ui, failures, f"PluginSources must not mutate/fetch: {forbidden}")


def check_boundaries(failures: list[str]) -> None:
    names = diff_names()
    for item in {
        "entrypoints/sdk/controlSchemas.ts",
        "entrypoints/sdk/coreSchemas.ts",
        "query.ts",
        "utils/processUserInput/processUserInput.ts",
        "Tool.ts",
        "commands/insights.ts",
    }:
        require(item not in names, failures, f"forbidden touched file: {item}")
    for name in names:
        require(not name.startswith("src/template-shell/"), failures, f"Workbench touched: {name}")
        require(not name.startswith("src-tauri/"), failures, f"Workbench Tauri touched: {name}")


def check_run_all(failures: list[str]) -> None:
    require(
        "wave_w62_plugin_sources_smoke.py" in read(RUN_ALL),
        failures,
        "run_all_smoke.sh must register W62 smoke",
    )


def main() -> int:
    failures: list[str] = []
    check_source_status(failures)
    check_command(failures)
    check_boundaries(failures)
    check_run_all(failures)
    if failures:
        print("W62 /plugin sources smoke: FAIL")
        for failure in failures:
            print(f"  - {failure}")
        return 1
    print("W62 /plugin sources smoke: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
