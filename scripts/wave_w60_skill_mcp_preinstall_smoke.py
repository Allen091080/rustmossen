#!/usr/bin/env python3
"""
W60 — CLI skill / MCP preinstall contract smoke.

Locks the first safe slice of the Mossen CLI-only skill/MCP plan:
- Core Mossen bundled skills are registered through existing bundled skill APIs.
- The plugin development pack is a built-in plugin, not a second plugin system.
- MCP templates are read-only inventory, default disabled, and routed via /mcp templates.
- No stream-json, query loop, Workbench, or insights changes leak into this wave.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def diff_names() -> set[str]:
    result = subprocess.run(
        ["git", "diff", "--name-only", "HEAD"],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=True,
    )
    return {line.strip() for line in result.stdout.splitlines() if line.strip()}


def require(condition: bool, failures: list[str], message: str) -> None:
    if not condition:
        failures.append(message)


def check_core_skills(failures: list[str]) -> None:
    path = ROOT / "skills" / "bundled" / "mossenCoreSkills.ts"
    index = ROOT / "skills" / "bundled" / "index.ts"
    src = read(path)
    idx = read(index)
    expected = [
        "skill-creator",
        "mcp-builder",
        "doc-coauthoring",
        "mossen-upgrade-planning",
        "mossen-protocol-development",
        "mossen-plugin-development",
        "mossen-permission-safety",
        "mossen-memory-development",
        "mossen-release-maintenance",
    ]
    for name in expected:
        require(name in src, failures, f"missing core bundled skill {name}")
    require(
        "registerBundledSkill" in src,
        failures,
        "core skills must reuse registerBundledSkill",
    )
    require(
        "registerMossenCoreSkills" in idx and "registerMossenCoreSkills()" in idx,
        failures,
        "skills/bundled/index.ts must register Mossen core skills",
    )


def check_plugin_dev_pack(failures: list[str]) -> None:
    path = ROOT / "plugins" / "bundled" / "mossenPluginDev.ts"
    index = ROOT / "plugins" / "bundled" / "index.ts"
    src = read(path)
    idx = read(index)
    expected = [
        "plugin-structure",
        "skill-development",
        "command-development",
        "hook-development",
        "mcp-integration",
        "plugin-settings",
        "agent-development",
    ]
    for name in expected:
        require(name in src, failures, f"missing plugin-dev skill {name}")
    require(
        "registerBuiltinPlugin" in src and "defaultEnabled: true" in src,
        failures,
        "plugin dev pack must reuse registerBuiltinPlugin and be enabled by default",
    )
    require(
        "registerMossenPluginDevPlugin" in idx
        and "registerMossenPluginDevPlugin()" in idx,
        failures,
        "plugins/bundled/index.ts must register plugin-dev builtin plugin",
    )


def check_mcp_templates(failures: list[str]) -> None:
    templates = ROOT / "services" / "mcp" / "builtinTemplates.ts"
    command = ROOT / "commands" / "mcp" / "mcp.tsx"
    index = ROOT / "commands" / "mcp" / "index.ts"
    ui = ROOT / "commands" / "mcp" / "McpTemplates.tsx"
    src = read(templates)
    cmd = read(command)
    idx = read(index)
    ui_src = read(ui)
    expected = [
        "filesystem-readonly",
        "git-readonly",
        "local-docs",
        "playwright-local",
        "sqlite-readonly",
    ]
    for name in expected:
        require(name in src, failures, f"missing MCP template {name}")
    require(
        len(re.findall(r"defaultEnabled:\s*false", src)) >= len(expected),
        failures,
        "all MCP templates must be defaultEnabled: false",
    )
    require(
        "getBuiltinMcpTemplates" in src and "McpServerConfig" in src,
        failures,
        "MCP templates must expose typed getBuiltinMcpTemplates inventory",
    )
    require(
        "parts[0] === 'templates'" in cmd and "<McpTemplates" in cmd,
        failures,
        "/mcp must route templates to McpTemplates",
    )
    require(
        "templates" in idx and "enable|disable" in idx,
        failures,
        "/mcp argumentHint must mention templates",
    )
    require(
        "read-only inventory" in ui_src and "will not automatically enabled" not in ui_src,
        failures,
        "McpTemplates must present inventory semantics without auto-enable wording bugs",
    )
    forbidden = ["github.com/", "slack", "gmail", "calendar", "OPENAI_API_KEY"]
    for token in forbidden:
        require(
            token not in src,
            failures,
            f"MCP templates must not bake remote/provider credential token {token}",
        )


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
        require(
            not name.startswith("src/template-shell/"),
            failures,
            f"Workbench/template-shell must not be touched: {name}",
        )
        require(
            not name.startswith("src-tauri/"),
            failures,
            f"Workbench Tauri must not be touched: {name}",
        )


def check_run_all(failures: list[str]) -> None:
    run_all = read(ROOT / "scripts" / "run_all_smoke.sh")
    require(
        "wave_w60_skill_mcp_preinstall_smoke.py" in run_all,
        failures,
        "run_all_smoke.sh must register W60 smoke",
    )


def main() -> int:
    failures: list[str] = []
    check_core_skills(failures)
    check_plugin_dev_pack(failures)
    check_mcp_templates(failures)
    check_boundaries(failures)
    check_run_all(failures)

    if failures:
        print("W60 skill/MCP preinstall smoke: FAIL")
        for failure in failures:
            print(f"  - {failure}")
        return 1

    print("W60 skill/MCP preinstall smoke: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
