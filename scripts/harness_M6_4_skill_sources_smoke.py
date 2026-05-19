#!/usr/bin/env python3
"""
M6.4 — skill 来源全覆盖 (bundled / user / project) 全部能发现。

按 harness全链路测试.md §C.1 M6.4 契约:
  前置:
    - bundled: 编译时 register, initBundledSkills() 注入 (天然存在 'simplify' / 'loop' 等 ungated)
    - user: $MOSSEN_CONFIG_DIR/skills/m64_user_skill/SKILL.md
    - project: <fixture_cwd>/.mossen/skills/m64_project_skill/SKILL.md
  步骤:
    1. enableConfigs(); initBundledSkills()
    2. bundled = getBundledSkills()  —— 验含 'simplify' (或 'loop') 等 ungated
    3. dirCmds = await getSkillDirCommands(fixture_cwd) (cwd 指向 fixture project)
    4. 验 dirCmds 含 'm64_user_skill' AND 'm64_project_skill'
  观察点:
    - bundled_ungated_present: True
    - user_skill_present: True
    - project_skill_present: True
  反测信号: src/skills/loadSkillsDir.ts:697 改 `userSkills` 分支 always Promise.resolve([])
            → 'm64_user_skill' 不在 dirCmds → fail
  反测信号 2: src/skills/loadSkillsDir.ts:706 改 `projectSettingsEnabled` 分支始终走 Promise.resolve([])
            → 'm64_project_skill' 不在 → fail
  反测信号 3: src/skills/bundled/index.ts initBundledSkills() 删 registerSimplifySkill()
            → 'simplify' 不在 bundled list → fail (mutation 须挑当前 ungated 的)

注: getSkillDirCommands 用 memoize, 所以每个进程独立调用 (本测试只跑一次足够)。
    bun -e 在 run-bun-featured.sh 内置 cd ROOT, snippet 必须 process.chdir 到 fixture cwd。
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log

RUN_BUN = str(ROOT / "run-bun-featured.sh")

USER_SKILL_NAME = "m64_user_skill"
PROJECT_SKILL_NAME = "m64_project_skill"

# 任意一个 ungated bundled skill 都行 —— simplify / loop / batch / keybindings / updateConfig
EXPECTED_BUNDLED_NAMES = ("simplify", "loop", "batch", "keybindings", "updateConfig")


def _write_skill(skill_md_path: Path, name: str, marker: str) -> None:
    skill_md_path.parent.mkdir(parents=True, exist_ok=True)
    content = (
        f"---\n"
        f"name: {name}\n"
        f"description: M6.4 skill {name} marker {marker}\n"
        f"user-invocable: true\n"
        f"---\n"
        f"\n{marker}\n"
    )
    skill_md_path.write_text(content, encoding="utf-8")


def _bun_collect_sources(env: dict, fixture_cwd: str) -> tuple[int, str, str]:
    """跑 bun -e: 切到 fixture_cwd, 列 bundled + dir skills, JSON 输出最后一行。"""
    snippet = (
        "import { enableConfigs } from './utils/config.ts';"
        "enableConfigs();"
        f"process.chdir({json.dumps(fixture_cwd)});"
        "const { initBundledSkills } = await import('./skills/bundled/index.ts');"
        "initBundledSkills();"
        "const { getBundledSkills } = await import('./skills/bundledSkills.ts');"
        "const { getSkillDirCommands } = await import('./skills/loadSkillsDir.ts');"
        "const bundled = getBundledSkills();"
        "const dirCmds = await getSkillDirCommands(process.cwd());"
        "process.stdout.write(JSON.stringify({"
        "  bundled_names: bundled.map(s => s.name),"
        "  dir_names: dirCmds.map(s => s.name),"
        "  cwd: process.cwd()"
        "}) + '\\n');"
    )
    proc = subprocess.run(
        [RUN_BUN, "-e", snippet],
        cwd=str(ROOT),
        text=True,
        capture_output=True,
        timeout=120,
        env=env,
    )
    return proc.returncode, proc.stdout, proc.stderr


def _parse_last_json(stdout: str) -> dict | None:
    for line in reversed((stdout or "").splitlines()):
        line = line.strip()
        if line.startswith("{"):
            try:
                return json.loads(line)
            except json.JSONDecodeError:
                continue
    return None


def case_skill_sources_coverage() -> dict:
    ctx = make_fixture("M6.4")

    # fixture project cwd —— 必须不是 home_dir 本身 (getProjectDirsUpToHome 在到 home 时 break)
    # 放一个独立 dir 在 root_dir 下作为 project root
    fixture_cwd = ctx.root_dir / "project_root"
    fixture_cwd.mkdir(parents=True, exist_ok=True)

    # 写 user skill
    user_skill_md = ctx.mossen_config_home / "skills" / USER_SKILL_NAME / "SKILL.md"
    _write_skill(user_skill_md, USER_SKILL_NAME, "M6_4_USER_BODY")

    # 写 project skill (在 fixture_cwd/.mossen/skills 下)
    project_skill_md = fixture_cwd / ".mossen" / "skills" / PROJECT_SKILL_NAME / "SKILL.md"
    _write_skill(project_skill_md, PROJECT_SKILL_NAME, "M6_4_PROJECT_BODY")

    env = ctx.env.copy()
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)

    rc, out, err = _bun_collect_sources(env, str(fixture_cwd))
    parsed = _parse_last_json(out)

    write_command_log(
        ctx,
        [RUN_BUN, "-e", "<initBundledSkills + getSkillDirCommands(fixture_cwd)>"],
        out,
        err,
        rc,
    )

    if not parsed:
        return {
            "name": "skill_sources_coverage",
            "ok": False,
            "stage": "parse",
            "exit_code": rc,
            "stdout_excerpt": out[:500],
            "stderr_excerpt": err[:500],
            "_ctx": ctx,
        }

    bundled_names = parsed.get("bundled_names") or []
    dir_names = parsed.get("dir_names") or []

    bundled_ungated_present = any(name in bundled_names for name in EXPECTED_BUNDLED_NAMES)
    user_skill_present = USER_SKILL_NAME in dir_names
    project_skill_present = PROJECT_SKILL_NAME in dir_names

    return {
        "name": "skill_sources_coverage",
        "ok": (
            rc == 0
            and bundled_ungated_present
            and user_skill_present
            and project_skill_present
        ),
        "exit_code": rc,
        "bundled_count": len(bundled_names),
        "bundled_ungated_present": bundled_ungated_present,
        "bundled_sample": bundled_names[:10],
        "dir_count": len(dir_names),
        "user_skill_present": user_skill_present,
        "project_skill_present": project_skill_present,
        "dir_names": dir_names,
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def main() -> int:
    res = case_skill_sources_coverage()
    ctx = res.pop("_ctx")
    results = [res]

    write_assertions(
        ctx,
        status="passed" if all(r.get("ok") for r in results) else "failed",
        assertions=[
            {
                "name": r["name"],
                "expected": True,
                "actual": r.get("ok"),
                "passed": r.get("ok"),
                "evidence": (
                    f"exit={r.get('exit_code')} "
                    f"bundled_ungated={r.get('bundled_ungated_present')} "
                    f"user_present={r.get('user_skill_present')} "
                    f"project_present={r.get('project_skill_present')} "
                    f"bundled_n={r.get('bundled_count')} dir_n={r.get('dir_count')}"
                ),
            }
            for r in results
        ],
    )

    summary = {
        "results": results,
        "passed": sum(1 for r in results if r.get("ok")),
        "total": len(results),
        "fixture_root": str(ctx.root_dir),
        "design_note": (
            "M6.4: 3 来源 (bundled / user / project) 全部能发现 —— "
            "bundled 来 initBundledSkills(), user 来 $MOSSEN_CONFIG_DIR/skills, "
            "project 来 <cwd>/.mossen/skills (getProjectDirsUpToHome)。"
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
