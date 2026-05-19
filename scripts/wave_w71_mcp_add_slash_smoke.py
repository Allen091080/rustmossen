#!/usr/bin/env python3
"""
W71 — slash /mcp add smoke.

Locks the CLI-only MCP add flow:
- /mcp add previews stdio/http/sse configs with dry-run semantics.
- /mcp add --confirm <token> reuses addMcpConfig().
- No query loop, protocol, Workbench, or insights drift.
"""

from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def read(path: str) -> str:
    return (ROOT / path).read_text()


def require(condition: bool, failures: list[str], message: str) -> None:
    if not condition:
        failures.append(message)


def changed_files() -> set[str]:
    output = subprocess.check_output(
        ["git", "diff", "--name-only", "origin/main..HEAD"],
        cwd=ROOT,
        text=True,
    )
    current = subprocess.check_output(
        ["git", "diff", "--name-only"],
        cwd=ROOT,
        text=True,
    )
    return {line.strip() for line in (output + current).splitlines() if line.strip()}


def check_engine(failures: list[str]) -> None:
    engine = read("services/mcp/slashAddPlan.ts")
    require(
        "MCP_SLASH_ADD_PLAN_TOKEN_TTL_MS = 10 * 60 * 1000" in engine,
        failures,
        "slash add token TTL must be 10 minutes",
    )
    require(
        "getMcpSlashAddPlan" in engine and "executeMcpSlashAddPlan" in engine,
        failures,
        "slash add plan exports missing",
    )
    require(
        "McpServerConfigSchema().safeParse" in engine,
        failures,
        "slash add must schema-validate generated MCP config",
    )
    require(
        "addMcpConfig(plan.serverName, plan.config, plan.scope)" in engine,
        failures,
        "confirm must reuse addMcpConfig",
    )
    require(
        "planStore.delete(token)" in engine,
        failures,
        "confirm token must be one-shot",
    )
    require(
        "parseEnvVars(opts.env)" in engine and "parseHeaders(opts.headers)" in engine,
        failures,
        "slash add must reuse existing env/header parsers",
    )
    require(
        "writeFile(" not in engine and "updateSettingsForSource(" not in engine,
        failures,
        "dry-run engine must not write settings/files directly",
    )


def check_parser_and_ui(failures: list[str]) -> None:
    parser = read("commands/mcp/parseAddArgs.ts")
    ui = read("commands/mcp/McpAdd.tsx")
    router = read("commands/mcp/mcp.tsx")

    require("parseMcpAddArgs" in parser, failures, "parseMcpAddArgs export missing")
    require("'--'" in parser and "commandParts" in parser, failures, "parser must support -- command delimiter")
    for token in ["--scope", "-s", "--transport", "-t", "--env", "-e", "--header", "-H", "--confirm", "--dry-run"]:
        require(token in parser, failures, f"parser missing {token}")
    require("Unsupported flag for /mcp add" in ui, failures, "UI must handle unsupported flags")
    require("No files were modified" in ui and "addMcpConfig()" in ui, failures, "dry-run UI must surface no-write/addMcpConfig contract")
    require("/mcp add --confirm" in ui, failures, "dry-run UI must show confirm command")
    require("playwright --scope local -- npx -y @playwright/mcp@latest" in ui, failures, "UI must include Playwright stdio example")
    require("parts[0] === 'add'" in router and "<McpAdd" in router and "parseMcpAddArgs" in router, failures, "mcp router must wire /mcp add")


def check_boundaries(failures: list[str]) -> None:
    changed = changed_files()
    forbidden = {
        "commands/insights.ts",
        "query.ts",
        "utils/processUserInput/processUserInput.ts",
        "entrypoints/sdk/controlSchemas.ts",
        "entrypoints/sdk/coreSchemas.ts",
    }
    for path in forbidden:
        require(path not in changed, failures, f"forbidden file changed: {path}")
    require(not any(path.startswith("src/") for path in changed), failures, "Workbench/src files must not change")


def check_run_all(failures: list[str]) -> None:
    run_all = read("scripts/run_all_smoke.sh")
    require("wave_w71_mcp_add_slash_smoke.py" in run_all, failures, "run_all must register W71 smoke")


def main() -> int:
    failures: list[str] = []
    check_engine(failures)
    check_parser_and_ui(failures)
    check_boundaries(failures)
    check_run_all(failures)
    if failures:
        print("W71 slash /mcp add smoke: FAIL")
        for failure in failures:
            print(f"  - {failure}")
        return 1
    print("W71 slash /mcp add smoke: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
