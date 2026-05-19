#!/usr/bin/env python3
"""
W61 — /mcp add-template contract smoke.

Locks the safe mutation slice for built-in MCP templates:
- /mcp add-template is slash-command-only and routed through the existing /mcp command.
- Dry-run mints a 10-minute one-shot token.
- Confirm writes through services/mcp/config.ts:addMcpConfig, not a duplicate writer.
- Template parameters are explicit and absolute paths are required.
- No auto-connect, Workbench, stream-json, query loop, or insights drift.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PLAN = ROOT / "services" / "mcp" / "builtinTemplatePlan.ts"
TEMPLATES = ROOT / "services" / "mcp" / "builtinTemplates.ts"
ARGS = ROOT / "commands" / "mcp" / "parseTemplateArgs.ts"
UI = ROOT / "commands" / "mcp" / "McpAddTemplate.tsx"
MCP = ROOT / "commands" / "mcp" / "mcp.tsx"
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


def check_plan_engine(failures: list[str]) -> None:
    src = read(PLAN)
    require(
        "export const MCP_TEMPLATE_PLAN_TOKEN_TTL_MS = 10 * 60 * 1000" in src,
        failures,
        "token TTL must be 10 minutes",
    )
    for symbol in [
        "getMcpTemplateInstallPlan",
        "executeMcpTemplateInstallPlan",
        "_resetMcpTemplatePlanStoreForTesting",
    ]:
        require(f"export " in src and symbol in src, failures, f"missing {symbol}")
    require("addMcpConfig(plan.serverName, plan.config, plan.scope)" in src, failures, "confirm must reuse addMcpConfig")
    require("planStore.delete(token)" in src, failures, "token must be deleted on confirm path")
    require("isAbsolute(value)" in src and "path_not_absolute" in src, failures, "absolute path guard missing")
    require("local' | 'user' | 'project" in src, failures, "writable scopes must be local/user/project only")


def check_templates(failures: list[str]) -> None:
    src = read(TEMPLATES)
    require("parameters: ['root']" in src, failures, "root templates must declare root parameter")
    require("parameters: ['db']" in src, failures, "sqlite template must declare db parameter")
    require("instantiateBuiltinMcpTemplate" in src, failures, "missing template instantiation helper")
    for placeholder in [
        "<absolute-project-root>",
        "<absolute-repo-root>",
        "<absolute-docs-root>",
        "<absolute-db-path>",
    ]:
        require(placeholder in src, failures, f"listing placeholder disappeared: {placeholder}")


def check_command_wiring(failures: list[str]) -> None:
    args = read(ARGS)
    ui = read(UI)
    mcp = read(MCP)
    require("parseMcpAddTemplateArgs" in args, failures, "missing add-template parser")
    for flag in ["--name", "--scope", "--root", "--db", "--confirm"]:
        require(flag in args, failures, f"parser missing {flag}")
    require("parts[0] === 'add-template'" in mcp, failures, "/mcp must route add-template")
    require("<McpAddTemplate" in mcp, failures, "McpAddTemplate route missing")
    require("getMcpTemplateInstallPlan" in ui and "executeMcpTemplateInstallPlan" in ui, failures, "UI must call dry-run + confirm helpers")
    require("No files were modified" in ui, failures, "dry-run output must state no files modified")
    require("will not auto-connect" in ui, failures, "dry-run output must state no auto-connect")
    forbidden = ["toggleMcpServer", "MCPReconnect", "useMcpToggleEnabled"]
    for token in forbidden:
        require(token not in ui, failures, f"McpAddTemplate must not auto-connect or toggle MCP: {token}")


def check_boundaries(failures: list[str]) -> None:
    names = diff_names()
    forbidden_exact = {
        "entrypoints/sdk/controlSchemas.ts",
        "entrypoints/sdk/coreSchemas.ts",
        "query.ts",
        "utils/processUserInput/processUserInput.ts",
        "Tool.ts",
        "commands/insights.ts",
    }
    for item in forbidden_exact:
        require(item not in names, failures, f"forbidden touched file in diff: {item}")
    for name in names:
        require(not name.startswith("src/template-shell/"), failures, f"Workbench touched: {name}")
        require(not name.startswith("src-tauri/"), failures, f"Workbench Tauri touched: {name}")


def check_run_all(failures: list[str]) -> None:
    require(
        "wave_w61_mcp_add_template_smoke.py" in read(RUN_ALL),
        failures,
        "run_all_smoke.sh must register W61 smoke",
    )


def main() -> int:
    failures: list[str] = []
    check_plan_engine(failures)
    check_templates(failures)
    check_command_wiring(failures)
    check_boundaries(failures)
    check_run_all(failures)

    if failures:
        print("W61 /mcp add-template smoke: FAIL")
        for failure in failures:
            print(f"  - {failure}")
        return 1
    print("W61 /mcp add-template smoke: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
