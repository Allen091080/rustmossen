#!/usr/bin/env python3
"""
W67 — plugin marketplace add dry-run/confirm smoke.

Locks the safe optional path:
- existing /plugin marketplace add <source> remains routed to PluginSettings
- new /plugin marketplace add --dry-run <source> creates a token plan
- new /plugin marketplace add --confirm <token> reuses existing
  addMarketplaceSource() + saveMarketplaceToSettings()
- no force/yes, no protocol/query loop/Workbench/insights drift
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ENGINE = ROOT / "utils" / "plugins" / "marketplaceAddPlan.ts"
PARSER = ROOT / "commands" / "plugin" / "parseArgs.ts"
ROUTER = ROOT / "commands" / "plugin" / "plugin.tsx"
UI = ROOT / "commands" / "plugin" / "PluginMarketplaceAddPlan.tsx"
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
        "export const PLUGIN_MARKETPLACE_ADD_TOKEN_TTL_MS = 10 * 60 * 1000" in src,
        failures,
        "token TTL must be 10 minutes",
    )
    for symbol in [
        "getPluginMarketplaceAddPlan",
        "executePluginMarketplaceAddPlan",
        "_resetPluginMarketplaceAddPlanStoreForTesting",
    ]:
        require(symbol in src, failures, f"missing {symbol}")
    require("parseMarketplaceInput(trimmed)" in src, failures, "dry-run must reuse parseMarketplaceInput")
    require("addMarketplaceSource(plan.source)" in src, failures, "confirm must reuse addMarketplaceSource")
    require("saveMarketplaceToSettings(name, { source: resolvedSource })" in src, failures, "confirm must save resolved source to settings")
    require("clearAllCaches()" in src, failures, "confirm must clear plugin caches")
    require("planStore.delete(token)" in src, failures, "confirm token must be one-shot")
    for forbidden in [
        "child_process",
        "execSync",
        "spawnSync",
        "git clone",
        "--force",
        "--yes",
        "i-know-what-im-doing",
    ]:
        require(forbidden not in src, failures, f"forbidden bypass/shell token in engine: {forbidden}")


def check_command(failures: list[str]) -> None:
    parser = read(PARSER)
    router = read(ROUTER)
    ui = read(UI)
    settings = read(SETTINGS)
    require("type: 'marketplace-add-plan'" in parser, failures, "parser missing marketplace-add-plan type")
    require("rest[0] === '--dry-run'" in parser, failures, "parser missing --dry-run route")
    require("rest[0] === '--confirm'" in parser, failures, "parser missing --confirm route")
    require("<PluginMarketplaceAddPlan" in router, failures, "plugin router missing plan component")
    require("getPluginMarketplaceAddPlan" in ui and "executePluginMarketplaceAddPlan" in ui, failures, "UI must call dry-run + confirm helpers")
    require("No settings were changed and no marketplace was fetched" in ui, failures, "dry-run text must promise no side effects")
    require("/plugin marketplace add --dry-run <path/url>" in settings, failures, "help must advertise dry-run command")
    require("/plugin marketplace add --confirm <token>" in settings, failures, "help must advertise confirm command")


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
        "wave_w67_plugin_marketplace_add_plan_smoke.py" in read(RUN_ALL),
        failures,
        "run_all_smoke.sh must register W67",
    )
    if failures:
        print("W67 plugin marketplace add-plan smoke: FAIL")
        for failure in failures:
            print(f"  - {failure}")
        return 1
    print("W67 plugin marketplace add-plan smoke: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
