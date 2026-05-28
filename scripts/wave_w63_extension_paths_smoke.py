#!/usr/bin/env python3
"""
W63 — /plugin paths read-only extension path visibility smoke.

This locks the CLI-only local extension path inventory. It must not create
directories, install plugins, or mutate settings.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
EXT = ROOT / "utils" / "plugins" / "extensionPaths.ts"
UI = ROOT / "commands" / "plugin" / "PluginPaths.tsx"
PARSE_ARGS = ROOT / "commands" / "plugin" / "parseArgs.ts"
ROUTER = ROOT / "commands" / "plugin" / "plugin.tsx"
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


def check_paths_helper(failures: list[str]) -> None:
    src = read(EXT)
    for token in [
        "getSkillsPath",
        "getMossenConfigHomeDir",
        "getCanonicalConfigDirName",
        "getPrimaryScopedConfigDir",
        "getManagedFilePath",
        "getPluginsDirectory",
        "getMarketplacesCacheDir",
        "getPluginSeedDirs",
        "describeExtensionPaths",
    ]:
        require(token in src, failures, f"extensionPaths must reuse {token}")
    for label in ["User extensions", "Project extensions", "Policy extensions", "Plugin extension system"]:
        require(label in src, failures, f"missing group label {label}")
    for forbidden in ["mkdir", "writeFile", "save", "installPlugin", "rm(", "unlink", "execFile"]:
        require(forbidden not in src, failures, f"extensionPaths must be read-only; found {forbidden}")


def check_command(failures: list[str]) -> None:
    parser = read(PARSE_ARGS)
    router = read(ROUTER)
    ui = read(UI)
    require("type: 'paths'" in parser, failures, "parseArgs missing paths type")
    require("case 'paths'" in parser and "case 'path'" in parser, failures, "parseArgs missing paths aliases")
    require("<PluginPaths" in router, failures, "plugin router missing PluginPaths")
    require("describeExtensionPaths" in ui, failures, "PluginPaths must call describeExtensionPaths")
    require("does not install or create anything" in ui, failures, "PluginPaths must state read-only behavior")


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
        "wave_w63_extension_paths_smoke.py" in read(RUN_ALL),
        failures,
        "run_all_smoke.sh must register W63 smoke",
    )


def main() -> int:
    failures: list[str] = []
    check_paths_helper(failures)
    check_command(failures)
    check_boundaries(failures)
    check_run_all(failures)
    if failures:
        print("W63 /plugin paths smoke: FAIL")
        for failure in failures:
            print(f"  - {failure}")
        return 1
    print("W63 /plugin paths smoke: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
