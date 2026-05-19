#!/usr/bin/env python3
"""
W66 — MCP template render-time i18n smoke.

Locks the small CLI-only display fix:
- builtin MCP template definitions remain canonical English
- Chinese text is mapped at render time
- /mcp templates no longer points users at a future add-template wave
- no protocol, query loop, Workbench, or insights drift
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TEMPLATES = ROOT / "services" / "mcp" / "builtinTemplates.ts"
MCP_TEMPLATES = ROOT / "commands" / "mcp" / "McpTemplates.tsx"
MCP_ADD_TEMPLATE = ROOT / "commands" / "mcp" / "McpAddTemplate.tsx"
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


def check_template_source(failures: list[str]) -> None:
    src = read(TEMPLATES)
    require(
        "title: 'Filesystem readonly'" in src
        and "description:\n      'Template for a local filesystem MCP server" in src,
        failures,
        "builtin template canonical English definitions must remain intact",
    )
    require(
        "export function getLocalizedBuiltinMcpTemplateText" in src,
        failures,
        "missing render-time localization helper",
    )
    for token in [
        "文件系统只读",
        "Git 只读",
        "本地文档",
        "本地 Playwright 浏览器",
        "SQLite 只读",
    ]:
        require(token in src, failures, f"missing Chinese template text: {token}")


def check_render_paths(failures: list[str]) -> None:
    templates = read(MCP_TEMPLATES)
    add_template = read(MCP_ADD_TEMPLATE)
    for src_name, src in [
        ("McpTemplates.tsx", templates),
        ("McpAddTemplate.tsx", add_template),
    ]:
        require(
            "getLocalizedBuiltinMcpTemplateText" in src,
            failures,
            f"{src_name} must consume render-time template localization",
        )
        require(
            "getInteractiveLanguageTag" in src,
            failures,
            f"{src_name} must choose localized notes by current language",
        )
    require(
        "future waves may add explicit /mcp add-template" not in templates,
        failures,
        "templates view must not mention obsolete future add-template copy",
    )
    require(
        "install one with /mcp add-template <template>" in templates,
        failures,
        "templates view must point to existing add-template command",
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
        require(item not in names, failures, f"forbidden touched file: {item}")
    for name in names:
        require(not name.startswith("src/template-shell/"), failures, f"Workbench touched: {name}")
        require(not name.startswith("src-tauri/"), failures, f"Tauri touched: {name}")


def main() -> int:
    failures: list[str] = []
    check_template_source(failures)
    check_render_paths(failures)
    check_boundaries(failures)
    require(
        "wave_w66_mcp_template_i18n_smoke.py" in read(RUN_ALL),
        failures,
        "run_all_smoke.sh must register W66",
    )

    if failures:
        print("W66 MCP template i18n smoke: FAIL")
        for failure in failures:
            print(f"  - {failure}")
        return 1
    print("W66 MCP template i18n smoke: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
