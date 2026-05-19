#!/usr/bin/env python3
"""
W69 — plugin install dry-run/confirm smoke.

Locks the optional safe plugin install path:
- legacy /plugin install <plugin> behavior remains routed to PluginSettings
- /plugin install --dry-run <plugin@marketplace|github-url> [--scope ...] creates a plan
- /plugin install --confirm <token> reuses installResolvedPlugin()
- dry-run does not write settings or plugin files
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ENGINE = ROOT / "utils" / "plugins" / "pluginInstallPlan.ts"
PARSER = ROOT / "commands" / "plugin" / "parseArgs.ts"
ROUTER = ROOT / "commands" / "plugin" / "plugin.tsx"
UI = ROOT / "commands" / "plugin" / "PluginInstallPlan.tsx"
SETTINGS = ROOT / "commands" / "plugin" / "PluginSettings.tsx"
RUN_ALL = ROOT / "scripts" / "run_all_smoke.sh"


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def diff_names() -> set[str]:
    committed = subprocess.run(
        ["git", "diff", "--name-only", "origin/main..HEAD"],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=True,
    ).stdout
    dirty = subprocess.run(
        ["git", "diff", "--name-only", "HEAD"],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=True,
    ).stdout
    return {
        line.strip()
        for line in (committed + "\n" + dirty).splitlines()
        if line.strip()
    }


def require(condition: bool, failures: list[str], message: str) -> None:
    if not condition:
        failures.append(message)


def check_engine(failures: list[str]) -> None:
    src = read(ENGINE)
    require(
        "export const PLUGIN_INSTALL_PLAN_TOKEN_TTL_MS = 10 * 60 * 1000" in src,
        failures,
        "token TTL must be 10 minutes",
    )
    for symbol in [
        "getPluginInstallPlan",
        "executePluginInstallPlan",
        "_resetPluginInstallPlanStoreForTesting",
    ]:
        require(symbol in src, failures, f"missing {symbol}")
    require("getPluginById(requestedPlugin)" in src, failures, "dry-run must resolve plugin through existing marketplace lookup")
    require("resolveDependencyClosure" in src, failures, "dry-run must preview dependency closure")
    require("installResolvedPlugin({" in src, failures, "confirm must reuse installResolvedPlugin")
    require("planStore.delete(token)" in src, failures, "confirm token must be one-shot")
    require("marketplace_required" in src, failures, "dry-run must require explicit plugin@marketplace")
    for forbidden in [
        "updateSettingsForSource(",
        "cacheAndRegisterPlugin",
        "addInstalledPlugin",
        "writeFile",
        "rm(",
        "unlink",
        "child_process",
        "--force",
        "--yes",
    ]:
        require(forbidden not in src, failures, f"engine dry-run wrapper must not contain forbidden token: {forbidden}")


def check_command(failures: list[str]) -> None:
    parser = read(PARSER)
    router = read(ROUTER)
    ui = read(UI)
    settings = read(SETTINGS)
    require("type: 'install-plan'" in parser, failures, "parser missing install-plan type")
    require("parts[1] === '--dry-run'" in parser, failures, "parser missing install --dry-run")
    require("parts[1] === '--confirm'" in parser, failures, "parser missing install --confirm")
    require("<PluginInstallPlan" in router, failures, "router missing PluginInstallPlan")
    require("getPluginInstallPlan" in ui and "executePluginInstallPlan" in ui, failures, "UI must call dry-run + confirm helpers")
    require("No settings were changed and no plugin files were written" in ui, failures, "dry-run text must promise no side effects")
    require("/plugin install --dry-run <plugin@market|github-url>" in settings, failures, "help must advertise install dry-run")
    require("/plugin install --confirm <token>" in settings, failures, "help must advertise install confirm")


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
        require(not name.startswith("src-tauri/"), failures, f"Tauri touched: {name}")


def main() -> int:
    failures: list[str] = []
    check_engine(failures)
    check_command(failures)
    check_boundaries(failures)
    require(
        "wave_w69_plugin_install_plan_smoke.py" in read(RUN_ALL),
        failures,
        "run_all_smoke.sh must register W69",
    )
    if failures:
        print("W69 plugin install-plan smoke: FAIL")
        for failure in failures:
            print(f"  - {failure}")
        return 1
    print("W69 plugin install-plan smoke: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
