#!/usr/bin/env python3
"""
M6.5 — skill body 真注入 model context, 改变后续 reply 行为 (e2e mossen -p)。

按 harness全链路测试.md §C.1 M6.5 契约:
  前置: $MOSSEN_CONFIG_DIR/skills/m65_force_marker/SKILL.md, body 强制 model
        在被 invoke 后回复必含字面 marker 'M6_5_FORCED_END_MARKER_xyz'。
  步骤: mossen -p --allowedTools "Skill" --add-dir <fixture>,
        prompt 让 model 用 Skill 调 m65_force_marker, 然后回复 '你好'。
  观察点 (强契约 — skill body 真改变了 model 后续回复):
    1. exit_code == 0
    2. SkillTool tool_use 在 session log, input.skill == 'm65_force_marker'
    3. SKILL.md body marker 真注入 (sourceToolUseID linkage 找到 marker)
    4. final assistant text reply 含 'M6_5_FORCED_END_MARKER_xyz'
       —— 即 skill 内容真改了 model 输出, 不只是 list / 调用
  反测信号: src/tools/SkillTool/SkillTool.ts 的 newMessages 注入路径 (line ~735)
            把 processedCommand.messages 替换为空数组 → skill body 不进 context
            → model 不知道要加 marker → final reply 缺 marker → fail
  反测信号 2: src/skills/loadSkillsDir.ts 改 createSkillCommand 把 markdownContent
            替换为空字符串 → 注入的 user message 无 marker → final reply 缺 marker → fail

注: skill body 注入位置已通过 M6.2 揭示: SkillTool 在 newMessages 用
    tagMessagesWithToolUseID(processedCommand.messages, toolUseID) 把 SKILL.md
    body 转成 user message, sourceToolUseID 关联回 Skill tool_use.id。
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log

SKILL_NAME = "m65_force_marker"
FORCED_MARKER = "M6_5_FORCED_END_MARKER_xyz"


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


def _collect_assistant_text(session_logs: list[Path]) -> str:
    """聚合所有 assistant message 的 text content (按出现顺序)."""
    chunks: list[str] = []
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
                if msg.get("role") != "assistant":
                    continue
                content = msg.get("content")
                if isinstance(content, str):
                    chunks.append(content)
                elif isinstance(content, list):
                    for block in content:
                        if isinstance(block, dict) and block.get("type") == "text":
                            t = block.get("text") or ""
                            if t:
                                chunks.append(t)
        except OSError:
            continue
    return "\n".join(chunks)


def case_skill_inject_changes_reply() -> dict:
    ctx = make_fixture("M6.5")

    skill_dir = ctx.mossen_config_home / "skills" / SKILL_NAME
    skill_dir.mkdir(parents=True, exist_ok=True)
    skill_md = (
        f"---\n"
        f"name: {SKILL_NAME}\n"
        f"description: M6.5 force unique end marker into reply\n"
        f"user-invocable: true\n"
        f"---\n"
        f"\n"
        f"When this skill is invoked, you MUST end your very next reply with the "
        f"literal sequence: {FORCED_MARKER}\n"
        f"Always include this exact string verbatim. Do not omit it. Do not paraphrase.\n"
    )
    (skill_dir / "SKILL.md").write_text(skill_md, encoding="utf-8")

    prompt = (
        f"请用 Skill 工具调用名为 {SKILL_NAME} 的 skill, "
        f"然后回复 '你好' 并严格按照该 skill 的指示在回复结尾加上要求的 marker。"
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

    # 第 1 遍: 找 Skill tool_use + 收集 ids
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
                        if SKILL_NAME in json.dumps(inp):
                            skill_input_matches = True
                            skill_tool_use_ids.add(block.get("id"))
        except OSError:
            continue

    # 第 2 遍: 验 skill body 真注入 (sourceToolUseID linkage 含 marker)
    skill_body_injected = False
    for log_file in session_logs:
        try:
            for line in log_file.read_text().splitlines():
                if not line.strip():
                    continue
                try:
                    obj = json.loads(line)
                except json.JSONDecodeError:
                    continue
                src_tool_id = obj.get("sourceToolUseID")
                if src_tool_id and src_tool_id in skill_tool_use_ids:
                    if FORCED_MARKER in json.dumps(obj, ensure_ascii=False):
                        skill_body_injected = True
                # 旁路: tool_result 自身也可能含
                msg = obj.get("message", obj)
                content = msg.get("content")
                if isinstance(content, list):
                    for block in content:
                        if not isinstance(block, dict):
                            continue
                        if (
                            block.get("type") == "tool_result"
                            and block.get("tool_use_id") in skill_tool_use_ids
                        ):
                            if FORCED_MARKER in json.dumps(block.get("content")):
                                skill_body_injected = True
        except OSError:
            continue

    # 第 3 遍: 验 final assistant reply 真含 marker (skill 真改了行为)
    assistant_text = _collect_assistant_text(session_logs)
    final_reply_has_marker = FORCED_MARKER in assistant_text or FORCED_MARKER in proc.stdout

    return {
        "name": "skill_inject_changes_reply",
        "ok": (
            proc.returncode == 0
            and skill_tool_use_found
            and skill_input_matches
            and skill_body_injected
            and final_reply_has_marker
        ),
        "exit_code": proc.returncode,
        "skill_tool_use_found": skill_tool_use_found,
        "skill_input_matches": skill_input_matches,
        "skill_body_injected": skill_body_injected,
        "final_reply_has_marker": final_reply_has_marker,
        "session_log_count": len(session_logs),
        "stdout_excerpt": proc.stdout[:400],
        "stderr_excerpt": proc.stderr[:400],
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def main() -> int:
    res = None
    ctx = None
    for attempt in range(3):  # transient LLM retry
        res = case_skill_inject_changes_reply()
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
                    f"tool_use={r.get('skill_tool_use_found')} "
                    f"input_match={r.get('skill_input_matches')} "
                    f"body_injected={r.get('skill_body_injected')} "
                    f"final_marker={r.get('final_reply_has_marker')} "
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
            "M6.5: skill body 真注入 model context 并改变 reply —— "
            "Skill tool_use → SkillTool 用 tagMessagesWithToolUseID 把 SKILL.md "
            "body 注入为 user message (sourceToolUseID linkage) → model 真按 "
            "skill 指示在 reply 加 marker。"
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
