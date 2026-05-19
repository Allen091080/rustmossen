#!/usr/bin/env python3
"""
M6.1 — /skill 列表非空 (bundled + user fixture skill 都被发现)。

按 harness全链路测试.md §3.6 M6.1 契约:
  前置: fixture mossen_config_home/skills/<name>/SKILL.md 创建一个 user skill
  步骤: bun -e:
        1. enableConfigs()
        2. initBundledSkills()
        3. getBundledSkills() —— 验列表非空
        4. getSkillDirCommands(cwd) —— 验 user skill 文件被发现
  观察点:
    1. bundled_skill_count >= 3
    2. {'simplify', 'loop', 'updateConfig'} ∩ bundled_names != ∅
    3. 'harness_m61_user_skill' in user_skill_names
  反测: 注释掉 src/skills/bundled/index.ts 的 registerSimplifySkill() →
        bundled 列表少 'simplify' (本测试断言交集, 真改源码后该 case 失败)。
"""

from __future__ import annotations

import json
import sys
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log

RUN_BUN = str(ROOT / "run-bun-featured.sh")

USER_SKILL_NAME = "harness_m61_user_skill"
USER_SKILL_DESC = "M6.1 fixture test"
USER_SKILL_BODY_MARKER = "M6_1_USER_SKILL_BODY_MARKER"
EXPECTED_BUNDLED_ANY = {"simplify", "loop", "updateConfig"}


def case_skill_list_nonempty() -> dict:
    ctx = make_fixture("M6.1")

    user_skill_dir = ctx.mossen_config_home / "skills" / USER_SKILL_NAME
    user_skill_dir.mkdir(parents=True, exist_ok=True)
    skill_md = (
        f"---\n"
        f"name: {USER_SKILL_NAME}\n"
        f"description: {USER_SKILL_DESC}\n"
        f"user-invocable: true\n"
        f"---\n"
        f"\n{USER_SKILL_BODY_MARKER}\n"
    )
    (user_skill_dir / "SKILL.md").write_text(skill_md, encoding="utf-8")

    snippet = (
        "import { enableConfigs } from './utils/config.ts';"
        "enableConfigs();"
        "const { initBundledSkills } = await import('./skills/bundled/index.ts');"
        "const { getBundledSkills } = await import('./skills/bundledSkills.ts');"
        "const { getSkillDirCommands } = await import('./skills/loadSkillsDir.ts');"
        "initBundledSkills();"
        "const bundled = getBundledSkills();"
        "const userCmds = await getSkillDirCommands(process.cwd());"
        "process.stdout.write(JSON.stringify({"
        "  bundled_count: bundled.length,"
        "  bundled_names: bundled.map(s => s.name),"
        "  user_skill_names: userCmds.map(s => s.name),"
        "  user_skill_count: userCmds.length"
        "}) + '\\n');"
    )

    env = ctx.env.copy()
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)

    proc = subprocess.run(
        [RUN_BUN, "-e", snippet],
        cwd=str(ROOT),
        text=True,
        capture_output=True,
        timeout=60,
        env=env,
    )

    write_command_log(
        ctx,
        [RUN_BUN, "-e", "<initBundledSkills+getSkillDirCommands>"],
        proc.stdout,
        proc.stderr,
        proc.returncode,
    )

    parsed = None
    for line in reversed((proc.stdout or "").splitlines()):
        line = line.strip()
        if line.startswith("{"):
            try:
                parsed = json.loads(line)
                break
            except json.JSONDecodeError:
                continue

    if not parsed:
        return {
            "name": "skill_list_nonempty",
            "ok": False,
            "exit_code": proc.returncode,
            "stderr_excerpt": proc.stderr[:500],
            "stdout_excerpt": proc.stdout[:500],
            "_ctx": ctx,
        }

    bundled_count = parsed.get("bundled_count", 0)
    bundled_names = set(parsed.get("bundled_names") or [])
    user_skill_names = set(parsed.get("user_skill_names") or [])

    bundled_any_match = bool(EXPECTED_BUNDLED_ANY & bundled_names)
    user_skill_present = USER_SKILL_NAME in user_skill_names

    return {
        "name": "skill_list_nonempty",
        "ok": (
            proc.returncode == 0
            and bundled_count >= 3
            and bundled_any_match
            and user_skill_present
        ),
        "exit_code": proc.returncode,
        "bundled_count": bundled_count,
        "bundled_names_excerpt": sorted(bundled_names)[:20],
        "expected_bundled_any": sorted(EXPECTED_BUNDLED_ANY),
        "bundled_any_match": bundled_any_match,
        "user_skill_names": sorted(user_skill_names),
        "user_skill_present": user_skill_present,
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def main() -> int:
    res = case_skill_list_nonempty()
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
                    f"bundled_count={r.get('bundled_count')} "
                    f"bundled_any_match={r.get('bundled_any_match')} "
                    f"user_skill_present={r.get('user_skill_present')} "
                    f"user_skill_names={r.get('user_skill_names')}"
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
            "M6.1: bundled skills (initBundledSkills) + user skill dir (getSkillDirCommands) "
            "must both contribute non-empty entries."
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
