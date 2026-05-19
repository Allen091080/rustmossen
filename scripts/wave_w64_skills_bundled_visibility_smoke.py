#!/usr/bin/env python3
"""
W64 — /skills bundled visibility regression smoke.

W60 registered Mossen core skills and builtin plugin-dev skills as
`loadedFrom/source = 'bundled'`. This smoke locks the UI path so /skills
does not filter them out and regress back to "No skills found".
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


def check_skills_menu(failures: list[str]) -> None:
    src = read(ROOT / "components" / "skills" / "SkillsMenu.tsx")
    require(
        "type SkillSource = SettingSource | 'bundled' | 'plugin' | 'mcp'" in src,
        failures,
        "SkillSource must include bundled",
    )
    require(
        "cmd.loadedFrom === 'bundled'" in src,
        failures,
        "isSkillCommand must accept bundled commands",
    )
    require(
        re.search(r"SOURCE_RENDER_ORDER[\s\S]*'bundled'[\s\S]*'plugin'", src) is not None,
        failures,
        "SOURCE_RENDER_ORDER must render bundled skills before plugin/mcp groups",
    )
    require(
        re.search(r"SOURCE_FILTER_ORDER[\s\S]*'bundled'[\s\S]*'plugin'", src) is not None,
        failures,
        "SOURCE_FILTER_ORDER must expose bundled filter chip",
    )
    require(
        "bundled: []" in src,
        failures,
        "skillsBySource groups must include bundled bucket",
    )
    require(
        "Built into Mossen" in src and "Mossen 内置" in src,
        failures,
        "bundled group subtitle must be explicit and localized",
    )
    require(
        "if (source === 'bundled')" in src
        and src.index("if (source === 'bundled')") < src.index("getSkillsPath(source, 'skills')"),
        failures,
        "bundled subtitle must not call getSkillsPath() with a non-file source",
    )


def check_bundled_sources(failures: list[str]) -> None:
    bundled = read(ROOT / "skills" / "bundledSkills.ts")
    core = read(ROOT / "skills" / "bundled" / "mossenCoreSkills.ts")
    builtin_plugins = read(ROOT / "plugins" / "builtinPlugins.ts")
    plugin_dev = read(ROOT / "plugins" / "bundled" / "mossenPluginDev.ts")
    descriptions = read(ROOT / "utils" / "commandDescription.ts")
    require(
        "source: 'bundled'" in bundled and "loadedFrom: 'bundled'" in bundled,
        failures,
        "programmatic bundled skills must still register as bundled",
    )
    require(
        "source: 'bundled'" in builtin_plugins and "loadedFrom: 'bundled'" in builtin_plugins,
        failures,
        "builtin plugin skills must still register as bundled",
    )
    require(
        "getLocalizedText" not in core
        and "Create, refine, and evaluate Mossen skills" in core
        and "创建、优化和评估 Mossen skill" not in core,
        failures,
        "core bundled skill definitions must keep canonical English descriptions",
    )
    require(
        "getLocalizedText" not in plugin_dev
        and "Create or refine a Mossen skill" in plugin_dev
        and "创建或优化 Mossen skill" not in plugin_dev,
        failures,
        "builtin plugin-dev skill definitions must keep canonical English descriptions",
    )
    require(
        "case 'skill-creator':" in descriptions
        and "创建、优化和评估 Mossen skill" in descriptions
        and "case 'mossen-plugin-development':" in descriptions
        and "设计 Mossen 插件、内置 skills" in descriptions
        and "case 'mcp-builder':" in descriptions,
        failures,
        "core bundled skill descriptions must be localized at render time",
    )
    require(
        "case 'skill-development':" in descriptions
        and "创建或优化 Mossen skill" in descriptions
        and "case 'plugin-structure':" in descriptions
        and "case 'agent-development':" in descriptions,
        failures,
        "builtin plugin-dev skill descriptions must be localized at render time",
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
        "wave_w64_skills_bundled_visibility_smoke.py" in run_all,
        failures,
        "run_all_smoke.sh must register W64 smoke",
    )


def main() -> int:
    failures: list[str] = []
    check_skills_menu(failures)
    check_bundled_sources(failures)
    check_boundaries(failures)
    check_run_all(failures)

    if failures:
        print("W64 /skills bundled visibility smoke: FAIL")
        for failure in failures:
            print(f"  - {failure}")
        return 1

    print("W64 /skills bundled visibility smoke: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
