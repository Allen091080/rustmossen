#!/usr/bin/env python3
"""
M1.2 — Agent loop e2e: 用户 prompt 让模型用 Bash 工具跑命令。

按 harness全链路测试.md §3.1 M1.2 契约:
  前置: fixture HOME 隔离
  步骤: stdin 发 "请用 Bash 工具执行 echo MARKER_M1_2_BASH_OUTPUT_unique_xyz"
  观察点:
    1. exit_code == 0
    2. marker 出现在 stdout
    3. session log 含 tool_use Bash + input.command 含 echo + marker
  反测: 改 BashTool 让它 no-op → marker 不出现 → fail
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log

MARKER = "MARKER_M1_2_BASH_OUTPUT_unique_xyz"


def _find_session_logs(fixture_home: Path) -> list[Path]:
    found = []
    for pattern in ("**/projects/**/*.jsonl", "**/sessions/**/*.jsonl",
                    "**/.mossen/**/*.jsonl"):
        for p in fixture_home.glob(pattern):
            if p not in found:
                found.append(p)
    return found


def case_bash_e2e_full() -> dict:
    ctx = make_fixture("M1.2")

    prompt = f"请用 Bash 工具执行 echo {MARKER}"

    proc = subprocess.run(
        [str(ROOT / "run-mossen.sh"), "-p", "--allowedTools", "Bash"],
        input=prompt,
        env=ctx.env,
        capture_output=True,
        text=True,
        timeout=180,
        cwd=str(ROOT),
    )

    write_command_log(ctx, [str(ROOT / "run-mossen.sh"), "-p", "--allowedTools", "Bash"],
                      proc.stdout, proc.stderr, proc.returncode)

    marker_in_stdout = MARKER in proc.stdout

    session_logs = _find_session_logs(ctx.home_dir)
    tool_use_bash_found = False
    bash_command_includes_marker = False
    tool_result_has_marker = False  # 关键: 验工具真返回 marker (反 model 文字绕过)
    for log_file in session_logs:
        try:
            for line in log_file.read_text().splitlines():
                if not line.strip():
                    continue
                obj = json.loads(line)
                msg = obj.get("message", obj)
                content = msg.get("content")
                if isinstance(content, list):
                    for block in content:
                        if not isinstance(block, dict):
                            continue
                        if block.get("type") == "tool_use" and block.get("name") == "Bash":
                            tool_use_bash_found = True
                            input_data = block.get("input", {})
                            cmd = str(input_data.get("command", ""))
                            if MARKER in cmd:
                                bash_command_includes_marker = True
                        elif block.get("type") == "tool_result":
                            # 检查 tool_result 内容是否含 marker (即工具真执行)
                            result_content = block.get("content")
                            if MARKER in str(result_content):
                                tool_result_has_marker = True
        except (json.JSONDecodeError, OSError):
            continue

    return {
        "name": "bash_e2e_full",
        "ok": (
            proc.returncode == 0
            and marker_in_stdout
            and tool_use_bash_found
            and bash_command_includes_marker
            and tool_result_has_marker  # 强契约: marker 必须在工具结果里, 不只在 model 文字回复
        ),
        "exit_code": proc.returncode,
        "marker_in_stdout": marker_in_stdout,
        "tool_use_bash_found": tool_use_bash_found,
        "bash_command_includes_marker": bash_command_includes_marker,
        "tool_result_has_marker": tool_result_has_marker,
        "session_log_count": len(session_logs),
        "stdout_excerpt": proc.stdout[:300],
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def main() -> int:
    # Transient retry: model 偶发不调 Bash → 测试 fail. Retry 至多 3 次.
    res1 = None
    ctx = None
    for attempt in range(3):
        res1 = case_bash_e2e_full()
        ctx = res1.pop("_ctx")
        if res1.get("ok"):
            res1["_attempt"] = attempt + 1
            break
        res1["_attempt"] = attempt + 1
    results = [res1]

    write_assertions(ctx,
                     status="passed" if all(r.get("ok") for r in results) else "failed",
                     assertions=[
                         {"name": r["name"], "expected": True,
                          "actual": r.get("ok"), "passed": r.get("ok"),
                          "evidence": f"marker={r.get('marker_in_stdout')} tool_use_bash={r.get('tool_use_bash_found')} cmd_has_marker={r.get('bash_command_includes_marker')}"}
                         for r in results
                     ])

    summary = {
        "results": results,
        "passed": sum(1 for r in results if r.get("ok")),
        "total": len(results),
        "fixture_root": str(ctx.root_dir),
        "design_note": "M1.2 Bash 工具 e2e",
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
