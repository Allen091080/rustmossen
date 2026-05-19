#!/usr/bin/env python3
"""
W70 — remote extension install smoke.

Locks CLI-only, on-demand remote install surfaces:
- /plugin install --dry-run <github-url> reuses PluginInstallPlan and existing
  installResolvedPlugin() confirm path.
- /mcp install --dry-run <url> previews remote MCP JSON and confirm reuses
  addMcpConfig().
- No protocol/query/Workbench/insights drift.
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


def check_plugin_github(failures: list[str]) -> None:
    engine = read("utils/plugins/pluginInstallPlan.ts")
    ui = read("commands/plugin/PluginInstallPlan.tsx")
    parser = read("commands/plugin/parseArgs.ts")
    settings = read("commands/plugin/PluginSettings.tsx")

    require("parseGitHubPluginTarget" in engine, failures, "plugin plan must parse GitHub targets")
    require("loadGitHubPluginManifest" in engine, failures, "plugin plan must fetch GitHub plugin manifest during dry-run")
    require("PluginMarketplaceEntrySchema().safeParse" in engine, failures, "GitHub plugin manifest must be schema-validated")
    require("GITHUB_DIRECT_MARKETPLACE" in engine and "github-direct" in engine, failures, "direct GitHub plugin marketplace sentinel missing")
    require("source: 'git-subdir'" in engine and "source: 'url'" in engine, failures, "GitHub plugin source must support root and subdir")
    require("installResolvedPlugin({" in engine, failures, "confirm must reuse installResolvedPlugin")
    require("invalid_github_target" in engine and "invalid_github_target" in ui, failures, "invalid GitHub target error missing")
    require("<plugin@market|github-url>" in parser and "<plugin@market|github-url>" in settings, failures, "help/parser comment must advertise GitHub URL")
    require("writeFile(" not in engine and "updateSettingsForSource(" not in engine, failures, "dry-run engine must not write settings/files directly")


def check_mcp_remote(failures: list[str]) -> None:
    engine = read("services/mcp/remoteInstallPlan.ts")
    ui = read("commands/mcp/McpInstall.tsx")
    parser = read("commands/mcp/parseInstallArgs.ts")
    router = read("commands/mcp/mcp.tsx")

    require("MCP_REMOTE_PLAN_TOKEN_TTL_MS = 10 * 60 * 1000" in engine, failures, "MCP remote token TTL must be 10 minutes")
    require("getMcpRemoteInstallPlan" in engine and "executeMcpRemoteInstallPlan" in engine, failures, "MCP remote plan exports missing")
    require("McpJsonConfigSchema().safeParse" in engine, failures, "MCP remote must parse mcpServers JSON")
    require("McpServerConfigSchema().safeParse" in engine, failures, "MCP remote must parse single-server config JSON")
    require("addMcpConfig(plan.serverName, plan.config, plan.scope)" in engine, failures, "MCP confirm must reuse addMcpConfig")
    require("planStore.delete(token)" in engine, failures, "MCP confirm token must be one-shot")
    require("raw.githubusercontent.com" in engine, failures, "GitHub blob URL must convert to raw URL")
    require("McpInstall" in router and "parseMcpInstallArgs" in router, failures, "/mcp install router missing")
    require("--dry-run" in parser and "--confirm" in parser and "--name" in parser and "--scope" in parser, failures, "MCP install parser missing flags")
    require("No files were modified" in ui and "addMcpConfig()" in ui, failures, "MCP dry-run UI must surface no-write/addMcpConfig contract")
    require("writeFile(" not in engine and "updateSettingsForSource(" not in engine, failures, "MCP dry-run engine must not write settings/files directly")


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
    require("wave_w70_remote_extension_install_smoke.py" in run_all, failures, "run_all must register W70 smoke")


def main() -> int:
    failures: list[str] = []
    check_plugin_github(failures)
    check_mcp_remote(failures)
    check_boundaries(failures)
    check_run_all(failures)
    if failures:
        print("W70 remote extension install smoke: FAIL")
        for failure in failures:
            print(f"  - {failure}")
        return 1
    print("W70 remote extension install smoke: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
