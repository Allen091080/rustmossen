#!/usr/bin/env python3
"""
W65 — GitHub skill install contract smoke.

Locks CLI-only `/skills install <github-url>`:
- dry-run + confirm-token
- GitHub contents API + SKILL.md validation
- installs only into ~/.mossen/skills/<name>
- refreshes skill cache/session after install
- no Workbench/protocol/query-loop/insights drift
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


def check_engine(failures: list[str]) -> None:
    src = read(ROOT / "utils" / "skills" / "githubSkillInstall.ts")
    require(
        "GITHUB_SKILL_INSTALL_TOKEN_TTL_MS = 10 * 60 * 1000" in src,
        failures,
        "confirm token TTL must be 10 minutes",
    )
    for name in [
        "getGitHubSkillInstallPlan",
        "executeGitHubSkillInstallPlan",
        "_resetGitHubSkillInstallPlanStoreForTesting",
    ]:
        require(f"export async function {name}" in src or f"export function {name}" in src, failures, f"missing export {name}")
    require("https://api.github.com/repos/" in src, failures, "must use GitHub contents API")
    require("SKILL.md" in src and "parseFrontmatter" in src, failures, "must validate SKILL.md frontmatter")
    require("parseSkillFrontmatterFields" in src, failures, "must reuse skill frontmatter parser")
    require("join(getMossenConfigHomeDir(), 'skills', skillName)" in src, failures, "install target must be ~/.mossen/skills/<name>")
    require("MAX_FILES = 100" in src and "MAX_TOTAL_BYTES = 2 * 1024 * 1024" in src, failures, "file/byte limits must be bounded")
    require("installPlans.delete(token)" in src, failures, "confirm tokens must be one-shot")
    require("clearCommandsCache()" in src and "skillChangeDetector.notifyChange" in src, failures, "install must refresh skill/command caches")
    forbidden = ["child_process", "execSync", "spawnSync", "git clone", "--force", "--yes"]
    for token in forbidden:
        require(token not in src, failures, f"forbidden install shortcut present: {token}")


def check_command(failures: list[str]) -> None:
    parser = read(ROOT / "commands" / "skills" / "parseArgs.ts")
    router = read(ROOT / "commands" / "skills" / "skills.tsx")
    ui = read(ROOT / "commands" / "skills" / "GitHubSkillInstall.tsx")
    index = read(ROOT / "commands" / "skills" / "index.ts")
    require("type: 'install'" in parser and "--confirm" in parser, failures, "skills parser must support install + --confirm")
    require("<GitHubSkillInstall" in router and "parseSkillsArgs" in router, failures, "skills router must route install subcommand")
    require("argumentHint: '[install <github-url>]'" in index, failures, "skills index must advertise install hint")
    require("dry-run" in ui and "/skills install --confirm" in ui, failures, "UI must show dry-run + confirm command")
    require("getLocalizedText" in ui and "GitHub skill 安装" in ui, failures, "UI must be localized")


def check_hot_reload(failures: list[str]) -> None:
    detector = read(ROOT / "utils" / "skills" / "skillChangeDetector.ts")
    require("function notifyChange(path: string)" in detector, failures, "skillChangeDetector must expose manual notifyChange")
    require("notifyChange," in detector, failures, "skillChangeDetector object must export notifyChange")


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
        require(not name.startswith("src/template-shell/"), failures, f"Workbench/template-shell touched: {name}")
        require(not name.startswith("src-tauri/"), failures, f"Workbench Tauri touched: {name}")


def check_run_all(failures: list[str]) -> None:
    run_all = read(ROOT / "scripts" / "run_all_smoke.sh")
    require("wave_w65_github_skill_install_smoke.py" in run_all, failures, "run_all_smoke must register W65")


def main() -> int:
    failures: list[str] = []
    check_engine(failures)
    check_command(failures)
    check_hot_reload(failures)
    check_boundaries(failures)
    check_run_all(failures)
    if failures:
        print("W65 GitHub skill install smoke: FAIL")
        for failure in failures:
            print(f"  - {failure}")
        return 1
    print("W65 GitHub skill install smoke: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
