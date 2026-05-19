#!/usr/bin/env python3
"""
M6.2 — bundled / user skill 真触发 e2e (mossen -p + Skill 工具)。

按 harness全链路测试.md §3.6 M6.2 契约:
  前置: fixture mossen_config_home/skills/m62_unique_skill/SKILL.md 含
        marker 'M6_2_SKILL_BODY_MARKER_xyz' (frontmatter user-invocable: true)
  步骤: 启动 mossen -p --allowedTools "Skill",
        prompt 让 model 用 Skill 工具调用 'm62_unique_skill', 把 body 原样输出
  观察点:
    1. exit_code == 0
    2. session log 含 type=tool_use, name=Skill, input.skill 含 'm62_unique_skill'
    3. session log 含 对应 tool_use_id 的 tool_result, content 含 marker
  反测: 改 src/skills/loadSkillsDir.ts 的 getSkillDirCommands 让它 return [] →
        mossen 系统提示无该 skill → tool_use 不会发出 → fail
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log

SKILL_NAME = "m62_unique_skill"
SKILL_BODY_MARKER = "M6_2_SKILL_BODY_MARKER_xyz"


def _find_session_logs(fixture_home: Path) -> list[Path]:
    found = []
    for pattern in (
        "**/projects/**/*.jsonl",
        "**/sessions/**/*.jsonl",
        "**/.mossen/**/*.jsonl",
    ):
        for p in fixture_home.glob(pattern):
            if p not in found:
                found.append(p)
    return found


def case_skill_invoke_real() -> dict:
    ctx = make_fixture("M6.2")

    skill_dir = ctx.mossen_config_home / "skills" / SKILL_NAME
    skill_dir.mkdir(parents=True, exist_ok=True)
    skill_md = (
        f"---\n"
        f"name: {SKILL_NAME}\n"
        f"description: M6.2 harness skill — when invoked, output marker {SKILL_BODY_MARKER}\n"
        f"user-invocable: true\n"
        f"---\n"
        f"\n{SKILL_BODY_MARKER}\n\n"
        f"When invoked, simply respond with: {SKILL_BODY_MARKER}\n"
    )
    (skill_dir / "SKILL.md").write_text(skill_md, encoding="utf-8")

    prompt = (
        f"请用 Skill 工具调用名为 {SKILL_NAME} 的 skill, "
        f"然后把它的内容原样输出 (含 marker {SKILL_BODY_MARKER})"
    )

    env = ctx.env.copy()
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)

    proc = subprocess.run(
        [
            str(ROOT / "run-mossen.sh"), "-p",
            "--allowedTools", "Skill",
            "--add-dir", str(ctx.root_dir),
        ],
        input=prompt,
        env=env,
        capture_output=True,
        text=True,
        timeout=240,
        cwd=str(ctx.root_dir),
    )

    write_command_log(
        ctx,
        ["mossen", "-p", "--allowedTools", "Skill", "--add-dir", str(ctx.root_dir)],
        proc.stdout,
        proc.stderr,
        proc.returncode,
    )

    session_logs = _find_session_logs(ctx.home_dir)
    skill_tool_use_found = False
    skill_input_matches = False
    skill_tool_use_ids: set = set()
    skill_result_has_marker = False

    for log_file in session_logs:
        try:
            for line in log_file.read_text().splitlines():
                if not line.strip():
                    continue
                try:
                    obj = json.loads(line)
                except json.JSONDecodeError:
                    continue
                msg = obj.get("message", obj)
                content = msg.get("content")
                if not isinstance(content, list):
                    continue
                for block in content:
                    if not isinstance(block, dict):
                        continue
                    if block.get("type") == "tool_use" and block.get("name") == "Skill":
                        skill_tool_use_found = True
                        inp = block.get("input") or {}
                        inp_str = json.dumps(inp)
                        if SKILL_NAME in inp_str:
                            skill_input_matches = True
                            skill_tool_use_ids.add(block.get("id"))
        except OSError:
            continue

    # SKILL body 注入方式: 新 user message 含 sourceToolUseID 指回 Skill tool_use,
    # text content 含 SKILL.md body (附 "Base directory for this skill" 前缀)
    for log_file in session_logs:
        try:
            for line in log_file.read_text().splitlines():
                if not line.strip():
                    continue
                try:
                    obj = json.loads(line)
                except json.JSONDecodeError:
                    continue
                # Path 1: tool_result content
                msg = obj.get("message", obj)
                content = msg.get("content")
                if isinstance(content, list):
                    for block in content:
                        if not isinstance(block, dict):
                            continue
                        if block.get("type") == "tool_result":
                            tu_id = block.get("tool_use_id")
                            if tu_id in skill_tool_use_ids:
                                result_str = json.dumps(block.get("content"))
                                if SKILL_BODY_MARKER in result_str:
                                    skill_result_has_marker = True
                # Path 2: meta user message linked via sourceToolUseID
                src_tool_id = obj.get("sourceToolUseID")
                if src_tool_id in skill_tool_use_ids:
                    obj_str = json.dumps(obj, ensure_ascii=False)
                    if SKILL_BODY_MARKER in obj_str:
                        skill_result_has_marker = True
        except OSError:
            continue

    return {
        "name": "skill_invoke_real",
        "ok": (
            proc.returncode == 0
            and skill_tool_use_found
            and skill_input_matches
            and skill_result_has_marker
        ),
        "exit_code": proc.returncode,
        "skill_tool_use_found": skill_tool_use_found,
        "skill_input_matches": skill_input_matches,
        "skill_result_has_marker": skill_result_has_marker,
        "session_log_count": len(session_logs),
        "stdout_excerpt": proc.stdout[:300],
        "stderr_excerpt": proc.stderr[:300],
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def main() -> int:
    res = None
    ctx = None
    for attempt in range(3):  # transient LLM retry
        res = case_skill_invoke_real()
        ctx = res.pop("_ctx")
        if res.get("ok"):
            res["_attempt"] = attempt + 1
            break
        res["_attempt"] = attempt + 1
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
                    f"skill_tool_use={r.get('skill_tool_use_found')} "
                    f"input_match={r.get('skill_input_matches')} "
                    f"result_marker={r.get('skill_result_has_marker')} "
                    f"sessions={r.get('session_log_count')}"
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
            "M6.2: user skill on disk → mossen -p discovers via getSkillDirCommands → "
            "model uses Skill tool → tool_result真含 SKILL.md body marker."
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
