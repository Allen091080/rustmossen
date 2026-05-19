#!/usr/bin/env python3
"""
W68 — /mcp status read-only smoke.

Locks a CLI-only MCP visibility slice:
- /mcp status and /mcp stat route to a read-only component
- status output reads AppState MCP clients/tools/commands/resources
- status output uses existing MCP filter helpers
- enable/disable completion text is localized
- no reconnect/toggle/config mutation in the status component
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
STATUS = ROOT / "commands" / "mcp" / "McpStatus.tsx"
MCP = ROOT / "commands" / "mcp" / "mcp.tsx"
INDEX = ROOT / "commands" / "mcp" / "index.ts"
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


def check_status_component(failures: list[str]) -> None:
    src = read(STATUS)
    require("useAppState(state => state.mcp)" in src, failures, "status must read mcp app state")
    for helper in [
        "filterToolsByServer",
        "filterMcpPromptsByServer",
        "filterResourcesByServer",
    ]:
        require(helper in src, failures, f"status must reuse {helper}")
    require("MCP status (read-only)" in src, failures, "status title must say read-only")
    require("本命令不会 reconnect、启用、禁用、认证或修改 MCP 配置" in src, failures, "zh no-mutation disclaimer missing")
    for forbidden in [
        "useMcpReconnect",
        "useMcpToggleEnabled",
        "reconnectMcpServer",
        "toggleMcpServer",
        "addMcpConfig",
        "setMcpServerEnabled",
        "writeFile",
        "rm(",
        "unlink",
    ]:
        require(forbidden not in src, failures, f"McpStatus must be read-only; found {forbidden}")


def check_route_and_i18n(failures: list[str]) -> None:
    mcp = read(MCP)
    index = read(INDEX)
    require("<McpStatus" in mcp, failures, "mcp router must route McpStatus")
    require("parts[0] === 'status' || parts[0] === 'stat'" in mcp, failures, "mcp router missing status/stat")
    require("[status|templates|add-template|enable|disable [server-name]]" in index, failures, "argumentHint missing status")
    require("getLocalizedText" in mcp and "所有 MCP server" in mcp, failures, "toggle completion text must be localized")


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
    check_status_component(failures)
    check_route_and_i18n(failures)
    check_boundaries(failures)
    require(
        "wave_w68_mcp_status_smoke.py" in read(RUN_ALL),
        failures,
        "run_all_smoke.sh must register W68",
    )
    if failures:
        print("W68 MCP status smoke: FAIL")
        for failure in failures:
            print(f"  - {failure}")
        return 1
    print("W68 MCP status smoke: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
